//! Streaming state machine: receives Chat Completions SSE chunks, accumulates
//! deltas, and emits Responses API SSE events matching the event flow in the
//! OpenAI Responses Streaming Events reference.
//!
//! Text flow: `response.created` → `response.in_progress` →
//! `response.output_item.added` → `response.content_part.added` →
//! `response.output_text.delta` ×N → `response.output_text.done` →
//! `response.content_part.done` → `response.output_item.done` →
//! `response.completed`
//!
//! Reasoning flow: `response.created` → `response.in_progress` →
//! `response.output_item.added` (reasoning) → `response.content_part.added` →
//! `response.reasoning_text.delta` ×N → `response.reasoning_text.done` →
//! `response.content_part.done` → `response.output_item.done` →
//! `response.output_item.added` (message) → … → `response.completed`
//!
//! Function call flow: `response.created` → `response.in_progress` →
//! `response.output_item.added` → `response.function_call_arguments.delta` ×N →
//! `response.function_call_arguments.done` → `response.output_item.done` →
//! `response.completed`

use super::chat;
use super::event;
pub use super::event::StreamEvent;
use super::item::{self, OutputContentBlock, OutputItem, ReasoningTextPart};
use super::responses::{Response, ResponseStatus};
use std::collections::{HashMap, HashSet};

// ── Streaming accumulator ────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct StreamState {
    pub response_id: String,
    pub msg_id: String,
    pub model: String,
    pub accumulated_text: String,
    pub reasoning_content: String,
    pub tool_calls: Vec<ToolCallAccumulator>,
    pub has_started: bool,
    pub created: i64,
    pub msg_output_index: i64,
    pub reasoning_id: String,
    pub reasoning_output_index: i64,
    pub has_refusal: bool,
    pub usage: Option<chat::Usage>,
    /// Tracked finish_reason from the most recent chunk choice.
    pub finish_reason: Option<String>,
    /// Accumulated logprobs for the text content.
    pub text_logprobs: Vec<super::item::TextLogprob>,
    /// Output items that have been closed mid-stream (e.g. reasoning→text transition).
    pub completed_items: Vec<OutputItem>,
    pub next_output_index: i64,
    /// Monotonic event sequence number, incremented on each event emission.
    pub next_sequence_number: i64,
    /// If set, reasoning content is encrypted into `encrypted_content`
    /// instead of being stored as plain `content`.
    pub compact_key: Option<[u8; 32]>,
    /// Tool names Codex declared as `custom` (code-mode). Upstream returns them
    /// as `function` calls; for these names the client-facing events are emitted
    /// as `custom_tool_call` instead (see [`emit_tool_call_deltas`]).
    pub custom_tool_names: HashSet<String>,
    /// Tool name → namespace name for tools declared inside a Codex `namespace`
    /// bundle (e.g. `spawn_agent` → `"collaboration"`). Chat Completions carries
    /// no namespace field, so this is threaded separately and re-attached to
    /// the `function_call`/`custom_tool_call` events emitted for these names
    /// (see [`emit_tool_call_deltas`]) — otherwise Codex's `(name, namespace)`-
    /// keyed registry rejects the call as `"unsupported call: <name>"`. Empty
    /// whenever the turn declared no `namespace` tools, e.g. when Codex's own
    /// multi-agent feature flags are off.
    pub tool_namespaces: HashMap<String, String>,
    /// Truncation char-scale `(full_chars, sent_chars)` applied to the reported
    /// `usage` before it reaches Codex. Codex gates auto-compaction on the
    /// reported `total_tokens`, so when history is truncated we scale the count
    /// back up to the true pre-truncation size. `None` leaves usage untouched.
    /// See `handlers::apply_input_char_scale`.
    pub(crate) truncation_scale: Option<(u64, u64)>,
    /// Set when a chunk carries an upstream `error` (e.g. a provider outage
    /// reported mid-stream). Once set, [`build_completion_events`] emits
    /// `response.failed` instead of treating the turn as completed.
    pub error: Option<chat::ChatError>,
}

#[derive(Debug, Default, Clone)]
pub struct ToolCallAccumulator {
    pub id: String,
    pub name: String,
    pub arguments: String,
    pub index: i64,
    pub fc_id: String,
    pub output_index: i64,
    /// True when this tool name is in [`StreamState::custom_tool_names`].
    pub is_custom: bool,
    /// Namespace name if this tool name is in [`StreamState::tool_namespaces`].
    pub namespace: Option<String>,
}

impl StreamState {
    pub fn new(response_id: String, msg_id: String, model: String) -> Self {
        Self {
            reasoning_id: format!("rs_{}", uuid::Uuid::new_v4().to_string().replace('-', "")),
            response_id,
            msg_id,
            model,
            ..Default::default()
        }
    }

    fn next_output_index(&mut self) -> i64 {
        let idx = self.next_output_index;
        self.next_output_index += 1;
        idx
    }

    /// Convert accumulated streaming state into a chat `ResponseMessage`.
    /// When items were closed mid-stream (reasoning→text, tool_calls→text, etc.)
    /// their content is recovered from `completed_items`.
    fn extract_reasoning_from_completed(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for item in &self.completed_items {
            if let OutputItem::Reasoning(r) = item {
                // Try encrypted_content first
                if let Some(ref encrypted) = r.encrypted_content
                    && let Some(ref key) = self.compact_key
                    && let Some(decrypted) = crate::crypto::decrypt(key, encrypted)
                    && !decrypted.is_empty()
                {
                    parts.push(decrypted);
                    continue;
                }
                // Fall back to summary + content
                for p in &r.summary {
                    parts.push(p.text.clone());
                }
                if let Some(ref content) = r.content {
                    for p in content {
                        parts.push(p.text.clone());
                    }
                }
            }
        }
        parts.join("\n")
    }

    /// Recover assistant message text/refusal closed mid-stream. A text→tool_calls
    /// transition pushes the message into `completed_items` and clears
    /// `accumulated_text`; mirror the reasoning/tool_call recovery so persisted
    /// history keeps the assistant's preamble instead of dropping it.
    fn extract_message_from_completed(&self) -> (String, String) {
        let (mut text, mut refusal) = (String::new(), String::new());
        for item in &self.completed_items {
            if let OutputItem::Message(m) = item {
                for block in &m.content {
                    match block {
                        OutputContentBlock::Text { text: t, .. } => text.push_str(t),
                        OutputContentBlock::Refusal { refusal: r } => refusal.push_str(r),
                    }
                }
            }
        }
        (text, refusal)
    }

    fn extract_tool_calls_from_completed(completed: &[OutputItem]) -> Vec<chat::ToolCallResponse> {
        completed
            .iter()
            .filter_map(|item| match item {
                OutputItem::FunctionCall(fc) => Some(chat::ToolCallResponse::Function {
                    id: fc.call_id.clone(),
                    function: chat::ToolCallFunction {
                        name: fc.name.clone(),
                        arguments: fc.arguments.clone(),
                    },
                }),
                OutputItem::CustomToolCall(ctc) => Some(chat::ToolCallResponse::Custom {
                    id: ctc.call_id.clone(),
                    custom: chat::ToolCallCustom {
                        name: ctc.name.clone(),
                        input: ctc.input.clone(),
                    },
                }),
                _ => None,
            })
            .collect()
    }

    pub fn to_response_message(&self) -> chat::ResponseMessage {
        let (mut text, mut refusal) = self.extract_message_from_completed();
        if self.has_refusal {
            refusal.push_str(&self.accumulated_text);
        } else {
            text.push_str(&self.accumulated_text);
        }
        let content = (!text.is_empty()).then_some(text);
        let refusal = (!refusal.is_empty()).then_some(refusal);
        // Gather tool calls: open ones + any closed mid-stream
        let mut tool_calls: Vec<chat::ToolCallResponse> = Vec::new();
        for tc in &self.tool_calls {
            if !tc.id.is_empty() {
                tool_calls.push(chat::ToolCallResponse::Function {
                    id: tc.id.clone(),
                    function: chat::ToolCallFunction {
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    },
                });
            }
        }
        if tool_calls.is_empty() {
            tool_calls = Self::extract_tool_calls_from_completed(&self.completed_items);
        }
        let reasoning = {
            if !self.reasoning_content.is_empty() {
                Some(self.reasoning_content.clone())
            } else {
                let from_completed = self.extract_reasoning_from_completed();
                if from_completed.is_empty() {
                    None
                } else {
                    Some(from_completed)
                }
            }
        };
        tracing::debug!(
            reasoning_open = %self.reasoning_content,
            completed_items_count = self.completed_items.len(),
            has_reasoning = reasoning.is_some(),
            reasoning_len = reasoning.as_ref().map(|r| r.len()).unwrap_or(0),
            "to_response_message: reasoning_content"
        );
        chat::ResponseMessage {
            content,
            refusal,
            role: "assistant".into(),
            annotations: None,
            audio: None,
            function_call: None,
            reasoning_content: reasoning,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
        }
    }
}

// ── Reasoning item builder ──────────────────────────────────────────────

/// Build a `Reasoning` output item.  If `compact_key` is set the text is
/// encrypted into `encrypted_content` (leaving `content` `None`); otherwise
/// it goes into `content` as plain text.
pub(crate) fn build_reasoning_item(
    id: String,
    text: &str,
    compact_key: Option<&[u8; 32]>,
) -> item::Reasoning {
    if let Some(key) = compact_key {
        item::Reasoning {
            id: Some(id),
            summary: vec![],
            encrypted_content: crate::crypto::encrypt(key, text),
            content: None,
            status: Some("completed".into()),
        }
    } else {
        item::Reasoning {
            id: Some(id),
            summary: vec![],
            encrypted_content: None,
            content: Some(vec![ReasoningTextPart {
                type_: "reasoning_text".into(),
                text: text.to_string(),
            }]),
            status: Some("completed".into()),
        }
    }
}

// ── Chunk processing ────────────────────────────────────────────────────

/// Parse one already-decoded Chat API SSE data object and emit Responses API events.
///
/// This is used by rewrite-aware streaming paths so they can parse JSON once,
/// mutate it as `serde_json::Value`, and then deserialize directly into the typed
/// Chat chunk without a stringify/re-parse round trip.
pub fn process_chunk_value(
    state: &mut StreamState,
    value: serde_json::Value,
) -> Option<Vec<StreamEvent>> {
    let chunk: chat::Chunk = serde_json::from_value(value).ok()?;

    // Capture creation timestamp unless the transport already emitted lifecycle
    // events with a proxy-side timestamp.
    if state.created == 0 {
        state.created = chunk.created;
    }

    if chunk.error.is_some() {
        state.error = chunk.error;
    }

    // Capture usage whenever the provider includes it. The OpenAI reference
    // sends it on a separate choices-empty chunk, but some providers attach it
    // to the final chunk that still carries a finish_reason choice — reading it
    // only on the choices-empty path would silently drop it there.
    if let Some(ref usage) = chunk.usage {
        state.usage = Some(usage.clone());
    }

    // Usage-only chunk (no choices) — nothing further to emit.
    if chunk.choices.is_empty() {
        return None;
    }

    let mut events = Vec::new();
    let mut has_content = false;

    for choice in &chunk.choices {
        let delta = &choice.delta;

        // Track finish_reason from the last choice that has one
        if choice.finish_reason.is_some() {
            state.finish_reason = choice.finish_reason.clone();
        }

        // Detect which item types are present in this chunk's delta
        let has_reasoning = delta
            .reasoning_content
            .as_ref()
            .is_some_and(|r| !r.is_empty());
        let has_text = delta.content.as_ref().is_some_and(|c| !c.is_empty())
            || delta.refusal.as_ref().is_some_and(|r| !r.is_empty());
        let has_tool_calls = delta.tool_calls.is_some();

        // ── Transitions: close items whose type is no longer present ──────
        // When the stream switches from one item type to another, emit the
        // "done" events for the previous type immediately, clear accumulated
        // content, and save the completed output item.

        // Reasoning → text / tool_calls transition
        if !state.reasoning_content.is_empty() && !has_reasoning {
            events.extend(close_reasoning_item(state));
            state.reasoning_content.clear();
        }

        // Text → tool_calls transition
        if !state.accumulated_text.is_empty() && !has_text && has_tool_calls {
            events.extend(close_message_item(state));
            state.accumulated_text.clear();
            state.has_refusal = false;
        }

        // Tool calls → text transition (close all active tool calls)
        if has_text {
            for tc_idx in 0..state.tool_calls.len() {
                if !state.tool_calls[tc_idx].id.is_empty() {
                    events.extend(close_tool_call_item(state, tc_idx));
                    state.tool_calls[tc_idx].id.clear();
                    state.tool_calls[tc_idx].name.clear();
                    state.tool_calls[tc_idx].arguments.clear();
                }
            }
        }

        // Extract token logprobs from this chunk if present
        // Two formats: event::TextLogprob for streaming events, item::TextLogprob for output items
        let stream_logprobs: Option<Vec<super::event::TextLogprob>> = choice
            .logprobs
            .as_ref()
            .and_then(|lp| lp.content.as_ref())
            .map(|tokens| {
                tokens
                    .iter()
                    .map(|t| super::event::TextLogprob {
                        token: t.token.clone(),
                        logprob: t.logprob,
                        top_logprobs: t
                            .top_logprobs
                            .iter()
                            .map(|tl| super::event::TextTopLogprob {
                                token: tl.token.clone(),
                                logprob: tl.logprob,
                            })
                            .collect(),
                    })
                    .collect()
            });

        // Also accumulate item-format logprobs for the final output item
        if let Some(lps) = choice.logprobs.as_ref().and_then(|lp| lp.content.as_ref()) {
            for t in lps {
                state.text_logprobs.push(super::item::TextLogprob {
                    token: t.token.clone(),
                    bytes: t.bytes.clone().unwrap_or_default(),
                    logprob: t.logprob,
                    top_logprobs: t
                        .top_logprobs
                        .iter()
                        .map(|tl| super::item::TopLogprob {
                            token: tl.token.clone(),
                            bytes: tl.bytes.clone().unwrap_or_default(),
                            logprob: tl.logprob,
                        })
                        .collect(),
                });
            }
        }

        // text_logprobs are accumulated separately below in item format

        // Reasoning content delta
        if has_reasoning {
            has_content = true;
            emit_reasoning_delta(
                state,
                delta.reasoning_content.as_ref().unwrap(),
                &mut events,
            );
        }

        // Text content delta
        if let Some(ref content) = delta.content
            && !content.is_empty()
        {
            has_content = true;
            emit_text_delta(state, content, &mut events, stream_logprobs.clone());
        }

        // Refusal delta
        if let Some(ref refusal) = delta.refusal
            && !refusal.is_empty()
        {
            has_content = true;
            emit_refusal_delta(state, refusal, &mut events);
        }

        // Tool call deltas
        if let Some(ref tool_calls) = delta.tool_calls {
            has_content = true;
            emit_tool_call_deltas(state, tool_calls, &mut events);
        }
    }

    // On first content, emit lifecycle start events
    if has_content && !state.has_started {
        events = emit_lifecycle_start(state)
            .into_iter()
            .chain(events)
            .collect();
        state.has_started = true;
    }

    if events.is_empty() {
        None
    } else {
        Some(events)
    }
}

// ── Lifecycle events ────────────────────────────────────────────────────

fn emit_lifecycle_start(state: &mut StreamState) -> Vec<StreamEvent> {
    let resp = build_partial_response(state, ResponseStatus::InProgress);
    vec![
        StreamEvent::Created(event::Created {
            response: resp.clone(),
            sequence_number: next_seq(state),
        }),
        StreamEvent::InProgress(event::InProgress {
            response: resp,
            sequence_number: next_seq(state),
        }),
    ]
}

// ── Item close helpers ───────────────────────────────────────────────────
//
// These are called both at stream-end (by build_completion_events) and
// mid-stream when the active item type transitions (e.g. reasoning→text,
// tool_calls→text) so that item lifecycle events are emitted at the right time.

fn close_reasoning_item(state: &mut StreamState) -> Vec<StreamEvent> {
    let ri = state.reasoning_output_index;
    let mut events = Vec::new();

    // reasoning_text.done
    events.push(StreamEvent::ReasoningTextDone(event::ReasoningTextDone {
        content_index: 0,
        item_id: state.reasoning_id.clone(),
        output_index: ri,
        text: state.reasoning_content.clone(),
        sequence_number: next_seq(state),
    }));

    // content_part.done
    events.push(StreamEvent::ContentPartDone(event::ContentPartDone {
        content_index: 0,
        item_id: state.reasoning_id.clone(),
        output_index: ri,
        part: event::ContentPart::ReasoningText {
            text: state.reasoning_content.clone(),
        },
        sequence_number: next_seq(state),
    }));

    // output_item.done
    let ri_item = OutputItem::Reasoning(build_reasoning_item(
        state.reasoning_id.clone(),
        &state.reasoning_content,
        state.compact_key.as_ref(),
    ));
    events.push(StreamEvent::OutputItemDone(event::OutputItemDone {
        output_index: ri,
        item: ri_item.clone(),
        sequence_number: next_seq(state),
    }));
    state.completed_items.push(ri_item);

    events
}

/// Extract the freeform `input` from the `{ "input": "..." }` wrapper we present
/// custom (code-mode) tools with. Falls back to the raw string if it is not the
/// expected JSON shape.
pub fn unwrap_custom_input(arguments: &str) -> String {
    serde_json::from_str::<serde_json::Value>(arguments)
        .ok()
        .and_then(|v| v.get("input")?.as_str().map(str::to_string))
        .unwrap_or_else(|| arguments.to_string())
}

fn close_tool_call_item(state: &mut StreamState, idx: usize) -> Vec<StreamEvent> {
    // Copy the fields we need in a scoped borrow so the mutable `next_seq` /
    // `completed_items` calls below don't conflict with the tool_calls borrow.
    let (id, name, arguments, output_index, fc_id_raw, is_custom, namespace) = {
        let tc = &state.tool_calls[idx];
        (
            tc.id.clone(),
            tc.name.clone(),
            tc.arguments.clone(),
            tc.output_index,
            tc.fc_id.clone(),
            tc.is_custom,
            tc.namespace.clone(),
        )
    };
    let mut events = Vec::new();
    let item_id = if fc_id_raw.is_empty() {
        let prefix = if is_custom { "ctc" } else { "fc" };
        format!(
            "{}_{}",
            prefix,
            uuid::Uuid::new_v4().to_string().replace('-', "")
        )
    } else {
        fc_id_raw
    };

    if is_custom {
        // Codex expects a custom_tool_call for these. Per-chunk deltas were
        // suppressed (they carried the JSON `{ input }` wrapper), so emit the
        // unwrapped freeform input once as delta + done.
        let input = unwrap_custom_input(&arguments);
        events.push(StreamEvent::CustomToolCallInputDelta(
            event::CustomToolCallInputDelta {
                delta: input.clone(),
                item_id: item_id.clone(),
                output_index,
                sequence_number: next_seq(state),
            },
        ));
        events.push(StreamEvent::CustomToolCallInputDone(
            event::CustomToolCallInputDone {
                input: input.clone(),
                item_id: item_id.clone(),
                output_index,
                sequence_number: next_seq(state),
            },
        ));
        events.push(StreamEvent::OutputItemDone(event::OutputItemDone {
            output_index,
            item: OutputItem::CustomToolCall(item::CustomToolCall {
                call_id: id.clone(),
                input,
                name: name.clone(),
                id: Some(item_id),
                namespace: namespace.clone(),
            }),
            sequence_number: next_seq(state),
        }));
        // Keep the function-shaped call in completed_items so the stored/replayed
        // assistant message stays function-typed (the upstream only accepts
        // function tools).
        state
            .completed_items
            .push(OutputItem::FunctionCall(item::FunctionCall {
                call_id: id,
                name,
                arguments,
                id: None,
                namespace,
                status: Some("completed".into()),
            }));
        return events;
    }

    // function_call_arguments.done
    events.push(StreamEvent::FunctionCallArgumentsDone(
        event::FunctionCallArgumentsDone {
            arguments: arguments.clone(),
            item_id: item_id.clone(),
            output_index,
            sequence_number: next_seq(state),
        },
    ));

    // output_item.done
    let fc_item = OutputItem::FunctionCall(item::FunctionCall {
        call_id: id,
        name,
        arguments,
        id: Some(item_id),
        namespace,
        status: Some("completed".into()),
    });
    events.push(StreamEvent::OutputItemDone(event::OutputItemDone {
        output_index,
        item: fc_item.clone(),
        sequence_number: next_seq(state),
    }));
    state.completed_items.push(fc_item);

    events
}

fn close_message_item(state: &mut StreamState) -> Vec<StreamEvent> {
    let mi = state.msg_output_index;
    let mut events = Vec::new();

    if state.has_refusal {
        // refusal.done
        events.push(StreamEvent::RefusalDone(event::RefusalDone {
            content_index: 0,
            item_id: state.msg_id.clone(),
            output_index: mi,
            refusal: state.accumulated_text.clone(),
            sequence_number: next_seq(state),
        }));

        // content_part.done
        events.push(StreamEvent::ContentPartDone(event::ContentPartDone {
            content_index: 0,
            item_id: state.msg_id.clone(),
            output_index: mi,
            part: event::ContentPart::Refusal {
                refusal: state.accumulated_text.clone(),
            },
            sequence_number: next_seq(state),
        }));
    } else {
        // output_text.done
        let stream_lps: Vec<event::TextLogprob> = state
            .text_logprobs
            .iter()
            .map(|lp| event::TextLogprob {
                token: lp.token.clone(),
                logprob: lp.logprob,
                top_logprobs: lp
                    .top_logprobs
                    .iter()
                    .map(|tl| event::TextTopLogprob {
                        token: tl.token.clone(),
                        logprob: tl.logprob,
                    })
                    .collect(),
            })
            .collect();
        let final_stream_logprobs = if stream_lps.is_empty() {
            None
        } else {
            Some(stream_lps)
        };

        events.push(StreamEvent::TextDone(event::TextDone {
            content_index: 0,
            item_id: state.msg_id.clone(),
            output_index: mi,
            text: state.accumulated_text.clone(),
            logprobs: final_stream_logprobs,
            sequence_number: next_seq(state),
        }));

        // content_part.done
        events.push(StreamEvent::ContentPartDone(event::ContentPartDone {
            content_index: 0,
            item_id: state.msg_id.clone(),
            output_index: mi,
            part: event::ContentPart::Text {
                text: state.accumulated_text.clone(),
                annotations: vec![],
            },
            sequence_number: next_seq(state),
        }));
    }

    // output_item.done
    let item_logprobs = if state.text_logprobs.is_empty() {
        None
    } else {
        Some(state.text_logprobs.clone())
    };
    let msg_content = if state.accumulated_text.is_empty() {
        vec![]
    } else if state.has_refusal {
        vec![OutputContentBlock::Refusal {
            refusal: state.accumulated_text.clone(),
        }]
    } else {
        vec![OutputContentBlock::Text {
            text: state.accumulated_text.clone(),
            annotations: vec![],
            logprobs: item_logprobs,
        }]
    };
    let msg_item = OutputItem::Message(item::OutputMessage {
        id: state.msg_id.clone(),
        role: "assistant".into(),
        status: "completed".into(),
        content: msg_content,
        phase: None,
    });
    events.push(StreamEvent::OutputItemDone(event::OutputItemDone {
        output_index: mi,
        item: msg_item.clone(),
        sequence_number: next_seq(state),
    }));
    state.completed_items.push(msg_item);

    events
}

/// Build completion events when the upstream Chat API signals `[DONE]`.
///
/// Closes any items that are still open (those that didn't transition mid-stream),
/// then emits the final `response.completed` or `response.incomplete` event.
pub fn build_completion_events(state: &mut StreamState) -> Vec<StreamEvent> {
    let mut events = Vec::new();

    // Close any items still open at stream end (mid-stream transitions already
    // closed items whose delta type stopped appearing).
    if !state.reasoning_content.is_empty() {
        events.extend(close_reasoning_item(state));
    }
    for tc_idx in 0..state.tool_calls.len() {
        if !state.tool_calls[tc_idx].id.is_empty() {
            events.extend(close_tool_call_item(state, tc_idx));
        }
    }
    if !state.accumulated_text.is_empty() {
        events.extend(close_message_item(state));
    }

    // `close_*_item` above already pushed every still-open item into
    // completed_items (in the right order), so it's the full output. Note:
    // don't re-derive output items from `state`'s accumulators here even
    // though they're still populated (intentionally not cleared — callers
    // like the compaction path read `accumulated_text`/`reasoning_content`
    // straight off the drained `StreamState` after this returns) — re-adding
    // from them duplicates every item `close_*_item` already recorded.
    let output_items: Vec<OutputItem> = state.completed_items.clone();

    // ── Build final Response ──────────────────────────────────────────────
    let (final_status, incomplete_details) = if state.error.is_some() {
        (ResponseStatus::Failed, None)
    } else {
        match state.finish_reason.as_deref() {
            Some("length") => (
                ResponseStatus::Incomplete,
                Some(super::responses::IncompleteDetails {
                    reason: super::responses::IncompleteReason::MaxOutputTokens,
                }),
            ),
            Some("content_filter") => (
                ResponseStatus::Incomplete,
                Some(super::responses::IncompleteDetails {
                    reason: super::responses::IncompleteReason::ContentFilter,
                }),
            ),
            _ => (ResponseStatus::Completed, None),
        }
    };

    let mut response = build_partial_response(state, final_status);
    response.output = output_items;
    response.incomplete_details = incomplete_details;
    if let Some(ref error) = state.error {
        response.error = Some(super::responses::Error {
            code: error.code.clone(),
            message: error.message.clone(),
            r#type: None,
            param: error.param.clone(),
        });
    }

    if let Some(ref usage) = state.usage {
        let mut u = super::responses::Usage::from(usage.clone());
        // Scale input_tokens up for truncation before Codex sees it: Codex keys
        // its client-side auto-compaction off the server-reported total_tokens.
        crate::handlers::apply_input_char_scale(Some(&mut u), state.truncation_scale);
        tracing::debug!(
            input_tokens = u.input_tokens,
            total_tokens = u.total_tokens,
            scale = ?state.truncation_scale,
            "reported usage"
        );
        response.usage = Some(u);
    } else {
        tracing::debug!("reported usage: none from upstream");
    }

    match final_status {
        ResponseStatus::Incomplete => {
            events.push(StreamEvent::Incomplete(event::Incomplete {
                response,
                sequence_number: next_seq(state),
            }));
        }
        ResponseStatus::Failed => {
            events.push(StreamEvent::Failed(event::Failed {
                response,
                sequence_number: next_seq(state),
            }));
        }
        _ => {
            events.push(StreamEvent::Completed(event::Completed {
                response,
                sequence_number: next_seq(state),
            }));
        }
    }

    events
}

fn build_partial_response(state: &StreamState, status: ResponseStatus) -> Response {
    Response {
        id: state.response_id.clone(),
        model: state.model.clone(),
        status,
        created_at: state.created,
        output: vec![],
        ..Default::default()
    }
}

fn next_seq(state: &mut StreamState) -> i64 {
    let s = state.next_sequence_number;
    state.next_sequence_number += 1;
    s
}

// ── Delta emit helpers ──────────────────────────────────────────────────

fn emit_reasoning_delta(state: &mut StreamState, reasoning: &str, events: &mut Vec<StreamEvent>) {
    if state.reasoning_content.is_empty() {
        let idx = state.next_output_index();
        state.reasoning_output_index = idx;
        events.push(StreamEvent::OutputItemAdded(event::OutputItemAdded {
            output_index: idx,
            item: OutputItem::Reasoning(item::Reasoning {
                id: Some(state.reasoning_id.clone()),
                summary: vec![],
                encrypted_content: None,
                content: Some(vec![]),
                status: Some("in_progress".into()),
            }),
            sequence_number: next_seq(state),
        }));
        events.push(StreamEvent::ContentPartAdded(event::ContentPartAdded {
            content_index: 0,
            item_id: state.reasoning_id.clone(),
            output_index: idx,
            part: event::ContentPart::ReasoningText {
                text: String::new(),
            },
            sequence_number: next_seq(state),
        }));
    }
    state.reasoning_content.push_str(reasoning);
    events.push(StreamEvent::ReasoningTextDelta(event::ReasoningTextDelta {
        delta: reasoning.to_string(),
        item_id: state.reasoning_id.clone(),
        output_index: state.reasoning_output_index,
        content_index: 0,
        sequence_number: next_seq(state),
    }));
}

fn emit_text_delta(
    state: &mut StreamState,
    content: &str,
    events: &mut Vec<StreamEvent>,
    logprobs: Option<Vec<super::event::TextLogprob>>,
) {
    if state.accumulated_text.is_empty() {
        let idx = state.next_output_index();
        state.msg_output_index = idx;
        events.push(StreamEvent::OutputItemAdded(event::OutputItemAdded {
            output_index: idx,
            item: OutputItem::Message(item::OutputMessage {
                id: state.msg_id.clone(),
                role: "assistant".into(),
                status: "in_progress".into(),
                content: vec![],
                phase: None,
            }),
            sequence_number: next_seq(state),
        }));
        events.push(StreamEvent::ContentPartAdded(event::ContentPartAdded {
            content_index: 0,
            item_id: state.msg_id.clone(),
            output_index: idx,
            part: event::ContentPart::Text {
                text: String::new(),
                annotations: vec![],
            },
            sequence_number: next_seq(state),
        }));
    }
    state.accumulated_text.push_str(content);
    events.push(StreamEvent::TextDelta(event::TextDelta {
        delta: content.to_string(),
        item_id: state.msg_id.clone(),
        output_index: state.msg_output_index,
        content_index: 0,
        sequence_number: next_seq(state),
        logprobs,
    }));
}

fn emit_refusal_delta(state: &mut StreamState, refusal: &str, events: &mut Vec<StreamEvent>) {
    if state.accumulated_text.is_empty() {
        let idx = state.next_output_index();
        state.msg_output_index = idx;
        events.push(StreamEvent::OutputItemAdded(event::OutputItemAdded {
            output_index: idx,
            item: OutputItem::Message(item::OutputMessage {
                id: state.msg_id.clone(),
                role: "assistant".into(),
                status: "in_progress".into(),
                content: vec![],
                phase: None,
            }),
            sequence_number: next_seq(state),
        }));
        events.push(StreamEvent::ContentPartAdded(event::ContentPartAdded {
            content_index: 0,
            item_id: state.msg_id.clone(),
            output_index: idx,
            part: event::ContentPart::Refusal {
                refusal: String::new(),
            },
            sequence_number: next_seq(state),
        }));
    }
    state.has_refusal = true;
    state.accumulated_text.push_str(refusal);
    events.push(StreamEvent::RefusalDelta(event::RefusalDelta {
        delta: refusal.to_string(),
        item_id: state.msg_id.clone(),
        output_index: state.msg_output_index,
        content_index: 0,
        sequence_number: next_seq(state),
    }));
}

fn emit_tool_call_deltas(
    state: &mut StreamState,
    tool_calls: &[chat::DeltaToolCall],
    events: &mut Vec<StreamEvent>,
) {
    // Use a local seq counter to avoid borrow conflicts with state.tool_calls
    let mut local_seq = state.next_sequence_number;

    for tc in tool_calls {
        let idx = tc.index as usize;

        // Ensure accumulator slot exists
        while state.tool_calls.len() <= idx {
            state.tool_calls.push(ToolCallAccumulator::default());
        }

        // Is this the first appearance of this tool call?
        let first_time = state.tool_calls[idx].id.is_empty();
        let oi = if first_time {
            state.next_output_index()
        } else {
            state.tool_calls[idx].output_index
        };

        // Determine custom-ness from the (possibly newly arriving) name before
        // taking the mutable slot borrow. Codex declared these as custom tools;
        // upstream returns them as function calls, so the client-facing item is
        // emitted as `custom_tool_call`.
        let is_custom = state.tool_calls[idx].is_custom
            || tc
                .function
                .as_ref()
                .and_then(|f| f.name.as_deref())
                .map(|n| state.custom_tool_names.contains(n))
                .unwrap_or(false);

        // Same reasoning for the namespace tag: resolve from the (possibly
        // newly arriving) name, falling back to what the slot already has.
        let namespace = tc
            .function
            .as_ref()
            .and_then(|f| f.name.as_deref())
            .and_then(|n| state.tool_namespaces.get(n).cloned())
            .or_else(|| state.tool_calls[idx].namespace.clone());

        // Now borrow the slot
        let slot = &mut state.tool_calls[idx];
        slot.is_custom = is_custom;
        slot.namespace = namespace.clone();

        // Capture id / name on first appearance
        if let Some(ref id) = tc.id {
            slot.id.clone_from(id);
        }
        if let Some(ref func) = tc.function {
            if let Some(ref name) = func.name {
                slot.name.clone_from(name);
            }
            if let Some(ref args) = func.arguments {
                slot.arguments.push_str(args);
            }
        }

        // Emit output_item.added on first appearance
        if first_time {
            slot.output_index = oi;
            let item_id = format!(
                "{}_{}",
                if is_custom { "ctc" } else { "fc" },
                uuid::Uuid::new_v4().to_string().replace('-', "")
            );
            slot.fc_id.clone_from(&item_id);
            let item = if is_custom {
                OutputItem::CustomToolCall(item::CustomToolCall {
                    call_id: slot.id.clone(),
                    input: String::new(),
                    name: slot.name.clone(),
                    id: Some(item_id),
                    namespace: namespace.clone(),
                })
            } else {
                OutputItem::FunctionCall(item::FunctionCall {
                    call_id: slot.id.clone(),
                    name: slot.name.clone(),
                    arguments: String::new(),
                    id: Some(item_id),
                    namespace: namespace.clone(),
                    status: Some("in_progress".into()),
                })
            };
            events.push(StreamEvent::OutputItemAdded(event::OutputItemAdded {
                output_index: oi,
                item,
                sequence_number: {
                    let s = local_seq;
                    local_seq += 1;
                    s
                },
            }));
        }

        // Emit arguments delta. For custom tools we suppress per-chunk deltas
        // (the arguments are the JSON `{ input }` wrapper, not the freeform
        // input); the unwrapped input is emitted once at close.
        if !is_custom
            && let Some(ref func) = tc.function
            && let Some(ref args) = func.arguments
        {
            events.push(StreamEvent::FunctionCallArgumentsDelta(
                event::FunctionCallArgumentsDelta {
                    delta: args.clone(),
                    item_id: slot.fc_id.clone(),
                    output_index: slot.output_index,
                    sequence_number: {
                        let s = local_seq;
                        local_seq += 1;
                        s
                    },
                },
            ));
        }
    }

    state.next_sequence_number = local_seq;
}

// ── Stream event preparation ────────────────────────────────────────────

pub(crate) struct PreparedStreamEvent {
    pub event: StreamEvent,
    pub body: serde_json::Value,
    pub event_type: String,
}

pub(crate) fn process_upstream_stream_data(
    state: &mut StreamState,
    data: &str,
    chat_in: &crate::config::RewriteConfig,
    responses_out: &crate::config::RewriteConfig,
) -> Result<Vec<PreparedStreamEvent>, String> {
    if data == "[DONE]" {
        return build_completion_events(state)
            .into_iter()
            .map(|event| prepare_stream_event(event, responses_out))
            .collect();
    } else {
        let mut value: serde_json::Value = serde_json::from_str(data).map_err(|e| e.to_string())?;

        if !chat_in.is_empty() {
            crate::rewrite::apply_rewrite(&mut value, chat_in)?;
        }

        if let Some(events) = process_chunk_value(state, value) {
            return events
                .into_iter()
                .map(|event| prepare_stream_event(event, responses_out))
                .collect();
        }
    }

    Ok(vec![])
}

pub(crate) fn prepare_stream_event(
    event: StreamEvent,
    responses_out: &crate::config::RewriteConfig,
) -> Result<PreparedStreamEvent, String> {
    let mut body = serde_json::to_value(&event).map_err(|e| e.to_string())?;
    if !responses_out.is_empty() {
        crate::rewrite::apply_rewrite(&mut body, responses_out)?;
    }
    let event_type = body["type"].as_str().unwrap_or("message").to_string();

    Ok(PreparedStreamEvent {
        event,
        body,
        event_type,
    })
}
