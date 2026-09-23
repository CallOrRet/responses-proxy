use crate::types::{
    MessageRole, chat,
    item::*,
    responses::{self, IncompleteReason, ResponseStatus},
};

/// Convert a Chat Completions response back to Responses API format.
pub fn chat_to_responses(
    chat_resp: chat::Completion,
    original_model: String,
    compact_key: Option<&[u8; 32]>,
) -> responses::Response {
    // Handle error responses from upstream
    if let Some(ref error) = chat_resp.error {
        return responses::Response {
            id: format!("resp_{}", uuid::Uuid::new_v4().to_string().replace('-', "")),
            status: ResponseStatus::Failed,
            model: original_model,
            error: Some(responses::Error {
                code: error.code.clone(),
                message: error.message.clone(),
                r#type: None,
                param: error.param.clone(),
            }),
            ..Default::default()
        };
    }

    let mut output_items: Vec<OutputItem> = Vec::new();
    let mut incomplete_details: Option<responses::IncompleteDetails> = None;

    for choice in &chat_resp.choices {
        let mut content_blocks: Vec<OutputContentBlock> = Vec::new();

        // Reasoning content → reasoning output item
        let reasoning_content = choice.message.reasoning_content.as_deref();
        if let Some(reasoning) = reasoning_content
            && !reasoning.is_empty()
        {
            output_items.push(OutputItem::Reasoning(
                crate::types::streaming::build_reasoning_item(
                    format!("rs_{}", uuid::Uuid::new_v4().to_string().replace('-', "")),
                    reasoning,
                    compact_key,
                ),
            ));
        }

        // Map Chat API logprobs to Responses API format
        let response_logprobs: Option<Vec<crate::types::item::TextLogprob>> = choice
            .logprobs
            .as_ref()
            .and_then(|lp| lp.content.as_ref())
            .map(|tokens| {
                tokens
                    .iter()
                    .map(|t| crate::types::item::TextLogprob {
                        token: t.token.clone(),
                        bytes: t.bytes.clone().unwrap_or_default(),
                        logprob: t.logprob,
                        top_logprobs: t
                            .top_logprobs
                            .iter()
                            .map(|tl| crate::types::item::TopLogprob {
                                token: tl.token.clone(),
                                bytes: tl.bytes.clone().unwrap_or_default(),
                                logprob: tl.logprob,
                            })
                            .collect(),
                    })
                    .collect()
            });

        // Map Chat API annotations to Responses format
        let chat_annotations: Vec<crate::types::item::OutputAnnotation> = choice
            .message
            .annotations
            .as_ref()
            .map(|anns| {
                anns.iter()
                    .map(|a| crate::types::item::OutputAnnotation::UrlCitation {
                        url: a.url_citation.url.clone(),
                        title: a.url_citation.title.clone(),
                        start_index: a.url_citation.start_index,
                        end_index: a.url_citation.end_index,
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Text content or refusal
        if let Some(ref content) = choice.message.content {
            if !content.is_empty() {
                content_blocks.push(OutputContentBlock::Text {
                    text: content.clone(),
                    annotations: chat_annotations,
                    logprobs: response_logprobs,
                });
            }
        } else if choice.finish_reason.as_deref() == Some("content_filter")
            && choice.message.tool_calls.is_none()
        {
            content_blocks.push(OutputContentBlock::Refusal {
                refusal: "content_filter".into(),
            });
        }

        // Status and incomplete details
        let normalized_reason = choice.finish_reason.clone();
        let item_status = match normalized_reason.as_deref() {
            Some("stop") | Some("tool_calls") | Some("length") => "completed",
            Some("content_filter") | Some("server_error") => "incomplete",
            _ => "completed",
        };

        incomplete_details = match choice.finish_reason.as_deref() {
            Some("content_filter") => Some(responses::IncompleteDetails {
                reason: IncompleteReason::ContentFilter,
            }),
            Some("length") => Some(responses::IncompleteDetails {
                reason: IncompleteReason::MaxOutputTokens,
            }),
            _ => None,
        };

        // Tool calls → function_call output items
        if let Some(ref tool_calls) = choice.message.tool_calls {
            for tc in tool_calls {
                match tc {
                    chat::ToolCallResponse::Function { id, function } => {
                        output_items.push(OutputItem::FunctionCall(FunctionCall {
                            call_id: id.clone(),
                            name: function.name.clone(),
                            arguments: function.arguments.clone(),
                            id: Some(id.clone()),
                            namespace: None,
                            status: Some("completed".into()),
                        }));
                    }
                    chat::ToolCallResponse::Custom { id, custom } => {
                        output_items.push(OutputItem::CustomToolCall(CustomToolCall {
                            call_id: id.clone(),
                            name: custom.name.clone(),
                            input: custom.input.clone(),
                            id: Some(id.clone()),
                            namespace: None,
                        }));
                    }
                }
            }
        }

        // Message output item
        if !content_blocks.is_empty() {
            output_items.push(OutputItem::Message(OutputMessage {
                id: format!("msg_{}", uuid::Uuid::new_v4().to_string().replace('-', "")),
                role: "assistant".into(),
                status: if item_status == "completed" {
                    "completed".into()
                } else {
                    "incomplete".into()
                },
                phase: None,
                content: content_blocks,
            }));
        }
    }

    let (final_status, incomplete_details) = if let Some(details) = incomplete_details {
        (ResponseStatus::Incomplete, Some(details))
    } else {
        (ResponseStatus::Completed, None)
    };

    responses::Response {
        id: format!("resp_{}", uuid::Uuid::new_v4().to_string().replace('-', "")),
        created_at: chat_resp.created,
        status: final_status,
        model: original_model,
        output: output_items,
        incomplete_details,
        usage: chat_resp.usage.map(responses::Usage::from),
        ..Default::default()
    }
}

/// Post-process a converted (non-streaming) response: tag `function_call`/
/// `custom_tool_call` output items whose name belongs to a Codex `namespace`
/// tool bundle (e.g. `spawn_agent` in `"collaboration"`) with that bundle's
/// name. Chat Completions carries no namespace field, so [`chat_to_responses`]
/// always emits `namespace: None`; Codex's own tool-call registry is keyed by
/// `(name, namespace)` and rejects the call as `"unsupported call: <name>"`
/// without this tag restored.
///
/// Must run before [`remap_custom_tool_calls`], which copies `fc.namespace`
/// onto the `CustomToolCall` it builds.
///
/// No-op when `namespaces` is empty — i.e. whenever the current turn (and its
/// cached fallback) carried no `namespace` tool, which includes every Codex
/// session with its own multi-agent feature flags turned off. Zero regression
/// in that case: every affected item keeps the `namespace: None` it already had.
pub fn apply_tool_namespaces(
    resp: &mut responses::Response,
    namespaces: &std::collections::HashMap<String, String>,
) {
    if namespaces.is_empty() {
        return;
    }
    for item in &mut resp.output {
        match item {
            OutputItem::FunctionCall(fc) => {
                if let Some(ns) = namespaces.get(&fc.name) {
                    fc.namespace = Some(ns.clone());
                }
            }
            OutputItem::CustomToolCall(ct) => {
                if let Some(ns) = namespaces.get(&ct.name) {
                    ct.namespace = Some(ns.clone());
                }
            }
            _ => {}
        }
    }
}

/// Post-process a converted (non-streaming) response: for tool calls whose name
/// Codex declared as `custom` (code-mode), rewrite the emitted `function_call`
/// output item into the `custom_tool_call` shape Codex expects, unwrapping the
/// `{ input }` arguments we wrapped on the request side.
///
/// No-op when `custom_names` is empty — i.e. for every model below gpt-5.6,
/// which never sends `additional_tools`, so there are no regressions.
pub fn remap_custom_tool_calls(
    resp: &mut responses::Response,
    custom_names: &std::collections::HashSet<String>,
) {
    if custom_names.is_empty() {
        return;
    }
    for item in &mut resp.output {
        if let OutputItem::FunctionCall(fc) = item
            && custom_names.contains(&fc.name)
        {
            *item = OutputItem::CustomToolCall(CustomToolCall {
                call_id: fc.call_id.clone(),
                input: crate::types::streaming::unwrap_custom_input(&fc.arguments),
                name: fc.name.clone(),
                id: fc.id.clone(),
                namespace: fc.namespace.clone(),
            });
        }
    }
}

impl From<chat::Usage> for responses::Usage {
    fn from(u: chat::Usage) -> Self {
        let cached_tokens = u
            .prompt_tokens_details
            .as_ref()
            .map(|d| d.cached_tokens)
            .unwrap_or(0);
        let reasoning_tokens = u
            .completion_tokens_details
            .as_ref()
            .map(|d| d.reasoning_tokens)
            .unwrap_or(0);
        Self {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
            input_tokens_details: responses::InputTokensDetails { cached_tokens },
            output_tokens_details: responses::OutputTokensDetails { reasoning_tokens },
        }
    }
}

/// Convert Responses output items into request input items for continuation.
///
/// This is necessarily limited to the item shapes that Chat Completions can
/// replay: assistant messages, reasoning text, and function/custom tool calls.
pub fn output_to_input_items(output: &[OutputItem]) -> Vec<InputItem> {
    let mut items = Vec::new();

    for item in output {
        match item {
            OutputItem::Message(msg) => {
                let content = msg
                    .content
                    .iter()
                    .map(|block| match block {
                        OutputContentBlock::Text { text, .. } => {
                            InputContentBlock::Text { text: text.clone() }
                        }
                        OutputContentBlock::Refusal { refusal } => InputContentBlock::Text {
                            text: refusal.clone(),
                        },
                    })
                    .collect();

                items.push(InputItem::Message(InputMessage {
                    role: MessageRole::Assistant,
                    content,
                    status: Some(msg.status.clone()),
                }));
            }
            OutputItem::Reasoning(reasoning) => {
                items.push(InputItem::Reasoning(reasoning.clone()));
            }
            OutputItem::FunctionCall(call) => {
                items.push(InputItem::FunctionCall(call.clone()));
            }
            OutputItem::CustomToolCall(call) => {
                items.push(InputItem::CustomToolCall(call.clone()));
            }
            _ => {}
        }
    }

    items
}
