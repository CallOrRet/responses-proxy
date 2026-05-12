use crate::types::{
    chat,
    item::*,
    responses::{self, IncompleteReason, ResponseStatus},
};

/// Convert a Chat Completions response back to Responses API format.
pub fn chat_to_responses(
    chat_resp: chat::Completion,
    original_model: String,
) -> responses::Response {
    // Handle error responses from upstream
    if let Some(ref error) = chat_resp.error {
        return responses::Response {
            id: format!("resp_{}", uuid::Uuid::new_v4().to_string().replace('-', "")),
            status: ResponseStatus::Failed,
            model: original_model,
            error: Some(responses::Error {
                code: error.code.clone().unwrap_or_else(|| "server_error".into()),
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
        if let Some(ref reasoning) = choice.message.reasoning_content
            && !reasoning.is_empty()
        {
            output_items.push(OutputItem::Reasoning(Reasoning {
                id: format!("rs_{}", uuid::Uuid::new_v4().to_string().replace('-', "")),
                summary: vec![],
                encrypted_content: None,
                content: Some(vec![ReasoningTextPart {
                    type_: "reasoning_text".into(),
                    text: reasoning.clone(),
                }]),
                status: Some("completed".into()),
            }));
        }

        // Text content or refusal
        if let Some(ref content) = choice.message.content {
            if !content.is_empty() {
                content_blocks.push(OutputContentBlock::Text {
                    text: content.clone(),
                    annotations: vec![],
                    logprobs: None,
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
        let item_status = match choice.finish_reason.as_deref() {
            Some("stop") | Some("tool_calls") | Some("length") => "completed",
            Some("content_filter") | Some("insufficient_system_resource") => "incomplete",
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

    // Build usage — with DeepSeek-style fallback for cached tokens
    let usage = chat_resp.usage.map(|u| {
        let cached_tokens = u
            .prompt_tokens_details
            .as_ref()
            .map(|d| d.cached_tokens)
            .unwrap_or_else(|| {
                u.prompt_cache_hit_tokens.unwrap_or(0) + u.prompt_cache_miss_tokens.unwrap_or(0)
            });
        responses::Usage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
            input_tokens_details: responses::InputTokensDetails { cached_tokens },
            output_tokens_details: responses::OutputTokensDetails {
                reasoning_tokens: u
                    .completion_tokens_details
                    .as_ref()
                    .map(|d| d.reasoning_tokens)
                    .unwrap_or(0),
            },
        }
    });

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
        usage,
        ..Default::default()
    }
}
