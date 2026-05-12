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
    pub message_item_added: bool,
    pub msg_output_index: i64,
    pub reasoning_item_added: bool,
    pub reasoning_id: String,
    pub reasoning_output_index: i64,
    pub has_refusal: bool,
    next_output_index: i64,
    pub usage: Option<chat::Usage>,
}

#[derive(Debug, Default, Clone)]
pub struct ToolCallAccumulator {
    pub id: String,
    pub name: String,
    pub arguments: String,
    pub index: i64,
    pub item_added: bool,
    pub fc_id: String,
    pub output_index: i64,
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

    fn alloc_output_index(&mut self) -> i64 {
        let idx = self.next_output_index;
        self.next_output_index += 1;
        idx
    }
}

// ── Chunk processing ────────────────────────────────────────────────────

/// Parse a single SSE data line from the Chat API and emit Responses API events.
///
/// Returns `None` if the chunk produced no events (e.g. empty usage-only chunk).
pub fn process_chunk(state: &mut StreamState, data: &str) -> Option<Vec<StreamEvent>> {
    if data == "[DONE]" {
        return Some(build_completion_events(state));
    }

    // Parse the Chat Completions chunk using the typed struct
    let chunk: chat::Chunk = serde_json::from_str(data).ok()?;

    // Capture creation timestamp
    state.created = chunk.created;

    // Usage-only chunk (choices empty, usage present) — store usage, no events
    if chunk.choices.is_empty() {
        if let Some(ref usage) = chunk.usage {
            state.usage = Some(usage.clone());
        }
        return None;
    }

    let mut events = Vec::new();
    let mut has_content = false;

    for choice in &chunk.choices {
        let delta = &choice.delta;

        // Reasoning content delta
        if let Some(ref reasoning) = delta.reasoning_content
            && !reasoning.is_empty()
        {
            has_content = true;
            emit_reasoning_delta(state, reasoning, &mut events);
        }

        // Text content delta
        if let Some(ref content) = delta.content
            && !content.is_empty()
        {
            has_content = true;
            emit_text_delta(state, content, &mut events);
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

fn emit_lifecycle_start(state: &StreamState) -> Vec<StreamEvent> {
    let resp = build_partial_response(state, ResponseStatus::InProgress);
    vec![
        StreamEvent::Created(event::Created {
            response: resp.clone(),
            sequence_number: 0,
        }),
        StreamEvent::InProgress(event::InProgress {
            response: resp,
            sequence_number: 0,
        }),
    ]
}

/// Build completion events when the upstream Chat API signals `[DONE]`.
///
/// Emits per the doc event flow:
/// - reasoning: `reasoning_text.done` → `content_part.done` → `output_item.done`
/// - each tool call: `function_call_arguments.done` → `output_item.done`
/// - message (text): `output_text.done` → `content_part.done` → `output_item.done`
/// - message (refusal): `refusal.done` → `content_part.done` → `output_item.done`
/// - `response.completed`
fn build_completion_events(state: &mut StreamState) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    let mut output_items: Vec<OutputItem> = Vec::new();

    // ── Finish reasoning item ────────────────────────────────────────────
    if state.reasoning_item_added {
        let ri = state.reasoning_output_index;

        // reasoning_text.done — §15.2
        events.push(StreamEvent::ReasoningTextDone(event::ReasoningTextDone {
            content_index: 0,
            item_id: state.reasoning_id.clone(),
            output_index: ri,
            text: state.reasoning_content.clone(),
            sequence_number: 0,
        }));

        // content_part.done
        events.push(StreamEvent::ContentPartDone(event::ContentPartDone {
            content_index: 0,
            item_id: state.reasoning_id.clone(),
            output_index: ri,
            part: event::ContentPart::ReasoningText {
                text: state.reasoning_content.clone(),
            },
            sequence_number: 0,
        }));

        // output_item.done
        let r_content: Vec<ReasoningTextPart> = if state.reasoning_content.is_empty() {
            vec![]
        } else {
            vec![ReasoningTextPart {
                type_: "reasoning_text".into(),
                text: state.reasoning_content.clone(),
            }]
        };
        let ri_item = OutputItem::Reasoning(item::Reasoning {
            id: state.reasoning_id.clone(),
            summary: vec![],
            encrypted_content: None,
            content: if r_content.is_empty() {
                None
            } else {
                Some(r_content.clone())
            },
            status: Some("completed".into()),
        });
        events.push(StreamEvent::OutputItemDone(event::OutputItemDone {
            output_index: ri,
            item: ri_item.clone(),
            sequence_number: 0,
        }));
        output_items.push(ri_item);
    }

    // ── Finish tool call items ────────────────────────────────────────────
    for tc in &state.tool_calls {
        if tc.id.is_empty() {
            continue;
        }
        let fc_id = if tc.fc_id.is_empty() {
            format!("fc_{}", uuid::Uuid::new_v4().to_string().replace('-', ""))
        } else {
            tc.fc_id.clone()
        };

        // function_call_arguments.done — §9.2
        events.push(StreamEvent::FunctionCallArgumentsDone(
            event::FunctionCallArgumentsDone {
                arguments: tc.arguments.clone(),
                item_id: fc_id.clone(),
                output_index: tc.output_index,
                sequence_number: 0,
            },
        ));

        // output_item.done
        let fc_item = OutputItem::FunctionCall(item::FunctionCall {
            call_id: tc.id.clone(),
            name: tc.name.clone(),
            arguments: tc.arguments.clone(),
            id: Some(fc_id),
            namespace: None,
            status: Some("completed".into()),
        });
        events.push(StreamEvent::OutputItemDone(event::OutputItemDone {
            output_index: tc.output_index,
            item: fc_item.clone(),
            sequence_number: 0,
        }));
        output_items.push(fc_item);
    }

    // ── Finish message item ───────────────────────────────────────────────
    if state.message_item_added {
        let mi = state.msg_output_index;

        if state.has_refusal {
            // refusal.done — §8.2
            events.push(StreamEvent::RefusalDone(event::RefusalDone {
                content_index: 0,
                item_id: state.msg_id.clone(),
                output_index: mi,
                refusal: state.accumulated_text.clone(),
                sequence_number: 0,
            }));

            // content_part.done
            events.push(StreamEvent::ContentPartDone(event::ContentPartDone {
                content_index: 0,
                item_id: state.msg_id.clone(),
                output_index: mi,
                part: event::ContentPart::Refusal {
                    refusal: state.accumulated_text.clone(),
                },
                sequence_number: 0,
            }));
        } else {
            // output_text.done — §7.2
            events.push(StreamEvent::TextDone(event::TextDone {
                content_index: 0,
                item_id: state.msg_id.clone(),
                output_index: mi,
                text: state.accumulated_text.clone(),
                logprobs: None,
                sequence_number: 0,
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
                sequence_number: 0,
            }));
        }

        // output_item.done
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
                logprobs: None,
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
            sequence_number: 0,
        }));
        output_items.push(msg_item);
    }

    // ── Build final Response ──────────────────────────────────────────────
    let mut response = build_partial_response(state, ResponseStatus::Completed);
    response.output = output_items;

    if let Some(ref usage) = state.usage {
        response.usage = Some(super::responses::Usage {
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            input_tokens_details: super::responses::InputTokensDetails {
                cached_tokens: usage
                    .prompt_tokens_details
                    .as_ref()
                    .map_or(0, |d| d.cached_tokens),
            },
            output_tokens_details: super::responses::OutputTokensDetails {
                reasoning_tokens: usage
                    .completion_tokens_details
                    .as_ref()
                    .map_or(0, |d| d.reasoning_tokens),
            },
        });
    }

    events.push(StreamEvent::Completed(event::Completed {
        response,
        sequence_number: 0,
    }));

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

// ── Delta emit helpers ──────────────────────────────────────────────────

fn emit_reasoning_delta(state: &mut StreamState, reasoning: &str, events: &mut Vec<StreamEvent>) {
    if !state.reasoning_item_added {
        let idx = state.alloc_output_index();
        state.reasoning_output_index = idx;
        events.push(StreamEvent::OutputItemAdded(event::OutputItemAdded {
            output_index: idx,
            item: OutputItem::Reasoning(item::Reasoning {
                id: state.reasoning_id.clone(),
                summary: vec![],
                encrypted_content: None,
                content: Some(vec![]),
                status: Some("in_progress".into()),
            }),
            sequence_number: 0,
        }));
        events.push(StreamEvent::ContentPartAdded(event::ContentPartAdded {
            content_index: 0,
            item_id: state.reasoning_id.clone(),
            output_index: idx,
            part: event::ContentPart::ReasoningText {
                text: String::new(),
            },
            sequence_number: 0,
        }));
        state.reasoning_item_added = true;
    }
    state.reasoning_content.push_str(reasoning);
    events.push(StreamEvent::ReasoningTextDelta(event::ReasoningTextDelta {
        delta: reasoning.to_string(),
        item_id: state.reasoning_id.clone(),
        output_index: state.reasoning_output_index,
        content_index: 0,
        sequence_number: 0,
    }));
}

fn emit_text_delta(state: &mut StreamState, content: &str, events: &mut Vec<StreamEvent>) {
    if !state.message_item_added {
        let idx = state.alloc_output_index();
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
            sequence_number: 0,
        }));
        events.push(StreamEvent::ContentPartAdded(event::ContentPartAdded {
            content_index: 0,
            item_id: state.msg_id.clone(),
            output_index: idx,
            part: event::ContentPart::Text {
                text: String::new(),
                annotations: vec![],
            },
            sequence_number: 0,
        }));
        state.message_item_added = true;
    }
    state.accumulated_text.push_str(content);
    events.push(StreamEvent::TextDelta(event::TextDelta {
        delta: content.to_string(),
        item_id: state.msg_id.clone(),
        output_index: state.msg_output_index,
        content_index: 0,
        sequence_number: 0,
        logprobs: None,
    }));
}

fn emit_refusal_delta(state: &mut StreamState, refusal: &str, events: &mut Vec<StreamEvent>) {
    if !state.message_item_added {
        let idx = state.alloc_output_index();
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
            sequence_number: 0,
        }));
        events.push(StreamEvent::ContentPartAdded(event::ContentPartAdded {
            content_index: 0,
            item_id: state.msg_id.clone(),
            output_index: idx,
            part: event::ContentPart::Refusal {
                refusal: String::new(),
            },
            sequence_number: 0,
        }));
        state.message_item_added = true;
    }
    state.has_refusal = true;
    state.accumulated_text.push_str(refusal);
    events.push(StreamEvent::RefusalDelta(event::RefusalDelta {
        delta: refusal.to_string(),
        item_id: state.msg_id.clone(),
        output_index: state.msg_output_index,
        content_index: 0,
        sequence_number: 0,
    }));
}

fn emit_tool_call_deltas(
    state: &mut StreamState,
    tool_calls: &[chat::DeltaToolCall],
    events: &mut Vec<StreamEvent>,
) {
    for tc in tool_calls {
        let idx = tc.index as usize;

        // Ensure accumulator slot exists
        while state.tool_calls.len() <= idx {
            state.tool_calls.push(ToolCallAccumulator::default());
        }

        // Pre-compute output_index if this is a new tool call
        let first_time = !state.tool_calls[idx].item_added;
        let oi = if first_time {
            state.alloc_output_index()
        } else {
            state.tool_calls[idx].output_index
        };

        // Now borrow the slot
        let slot = &mut state.tool_calls[idx];

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
            let fc_id = format!("fc_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
            slot.fc_id.clone_from(&fc_id);
            slot.item_added = true;

            events.push(StreamEvent::OutputItemAdded(event::OutputItemAdded {
                output_index: oi,
                item: OutputItem::FunctionCall(item::FunctionCall {
                    call_id: slot.id.clone(),
                    name: slot.name.clone(),
                    arguments: String::new(),
                    id: Some(fc_id),
                    namespace: None,
                    status: Some("in_progress".into()),
                }),
                sequence_number: 0,
            }));
        }

        // Emit arguments delta
        if let Some(ref func) = tc.function
            && let Some(ref args) = func.arguments
        {
            events.push(StreamEvent::FunctionCallArgumentsDelta(
                event::FunctionCallArgumentsDelta {
                    delta: args.clone(),
                    item_id: slot.fc_id.clone(),
                    output_index: slot.output_index,
                    sequence_number: 0,
                },
            ));
        }
    }
}
