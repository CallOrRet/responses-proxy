use crate::types::{MessageRole, chat, item::*, responses};

// ── Main conversion: Responses API → Chat Completions API ────────────────

/// Convert a Responses API request into a Chat Completions API request.
///
/// Fetches previous conversation history from the store (via
/// `req.previous_response_id`) and handles prepending history + merging
/// `req.instructions` into the leading system message.
///
/// Returns an error with the list of unsupported Responses-only features.
pub async fn responses_to_chat(
    #[allow(unused_variables)] mut req: responses::Request,
    state: &crate::app::State,
) -> Result<chat::Request, Vec<String>> {
    let mut unsupported_features = Vec::new();
    if req.prompt.is_some() {
        unsupported_features.push("prompt".to_string());
    }
    if req.conversation.is_some() {
        unsupported_features.push("conversation".to_string());
    }
    if !unsupported_features.is_empty() {
        return Err(unsupported_features);
    }

    let mut messages: Vec<chat::MessageRequest> = Vec::new();
    let mut pending_reasoning: Option<String> = None;

    // Map reasoning effort — Responses API values match Chat API directly.
    // reasoning.summary has no Chat Completions equivalent and is left for rewrite profiles.
    let reasoning_str: Option<String> = {
        let r = req.reasoning.as_ref();
        let effort = r.and_then(|r| r.effort.as_ref());
        effort.map(|e| {
            match e {
                crate::types::ReasoningEffort::None => "none",
                crate::types::ReasoningEffort::Minimal => "minimal",
                crate::types::ReasoningEffort::Low => "low",
                crate::types::ReasoningEffort::Medium => "medium",
                crate::types::ReasoningEffort::High => "high",
                crate::types::ReasoningEffort::Xhigh => "xhigh",
            }
            .into()
        })
    };

    // Structured output → response_format
    let response_format = req
        .text
        .as_ref()
        .and_then(|t| t.format.as_ref())
        .map(|f| match f {
            responses::TextFormat::JsonSchema {
                name,
                schema,
                strict,
                description,
            } => chat::ResponseFormat::JsonSchema(chat::JsonSchemaFormat {
                format_type: "json_schema".into(),
                json_schema: chat::JsonSchema {
                    name: name.clone(),
                    description: description.clone(),
                    schema: Some(schema.clone()),
                    strict: *strict,
                },
            }),
            responses::TextFormat::JsonObject => {
                chat::ResponseFormat::JsonObject(chat::JsonObjectFormat {
                    format_type: "json_object".into(),
                })
            }
            responses::TextFormat::Text => chat::ResponseFormat::Text(chat::TextFormat {
                format_type: "text".into(),
            }),
        });

    // ── History + instructions ────────────────────────────────────────────
    let instructions = std::mem::take(&mut req.instructions).filter(|i| !i.is_empty());

    let prev_messages: Vec<chat::MessageRequest> = match req.previous_response_id {
        Some(ref prev_id) => state.store().get(prev_id).await.unwrap_or_default(),
        None => vec![],
    };

    if !prev_messages.is_empty() {
        messages = prev_messages;

        // The first message in stored history is always a system message
        // holding the instructions from the previous turn.
        if let Some(first) = messages.first_mut()
            && matches!(first, chat::MessageRequest::System(_))
        {
            if let Some(new_instructions) = instructions {
                *first = chat::MessageRequest::System(chat::SystemMessage {
                    content: chat::MessageContent::Text(new_instructions),
                    name: None,
                });
            }
        } else if let Some(new_instructions) = instructions {
            messages.insert(
                0,
                chat::MessageRequest::System(chat::SystemMessage {
                    content: chat::MessageContent::Text(new_instructions),
                    name: None,
                }),
            );
        }
    } else if let Some(ref new_instructions) = instructions {
        messages.push(chat::MessageRequest::System(chat::SystemMessage {
            content: chat::MessageContent::Text(new_instructions.clone()),
            name: None,
        }));
    }

    // Walk input items
    let items: Vec<InputItem> = req.input;
    if !items.is_empty() {
        let mut deferred: Vec<chat::MessageRequest> = Vec::new();
        let mut pending_tool_calls: Vec<chat::ToolCallRequest> = Vec::new();

        let flush_tools = |msgs: &mut Vec<chat::MessageRequest>,
                           p: &mut Vec<chat::ToolCallRequest>,
                           r: &mut Option<String>| {
            if !p.is_empty() {
                msgs.push(chat::MessageRequest::Assistant(chat::AssistantMessage {
                    content: None,
                    name: None,
                    refusal: None,
                    audio: None,
                    tool_calls: Some(std::mem::take(p)),
                    function_call: None,
                    reasoning_content: r.take(),
                }));
            }
        };

        for item in items.into_iter() {
            match item {
                InputItem::FunctionCallOutput(fco) => {
                    let cs = match &fco.output {
                        FunctionOutputValue::String(s) => s.clone(),
                        FunctionOutputValue::Array(blocks) => {
                            extract_text_from_output_blocks(blocks)
                        }
                    };
                    deferred.push(chat::MessageRequest::Tool(chat::ToolMessage {
                        content: chat::MessageContent::Text(cs),
                        tool_call_id: fco.call_id.clone(),
                    }));
                }
                InputItem::Reasoning(r) => {
                    flush_tools(
                        &mut messages,
                        &mut pending_tool_calls,
                        &mut pending_reasoning,
                    );
                    messages.append(&mut deferred);
                    if let Some(t) = extract_reasoning(&r, state.compact_key()) {
                        pending_reasoning = Some(match pending_reasoning.take() {
                            Some(e) => format!("{}\n{}", e, t),
                            None => t,
                        });
                    }
                }
                InputItem::Compaction(c) => {
                    flush_tools(
                        &mut messages,
                        &mut pending_tool_calls,
                        &mut pending_reasoning,
                    );
                    messages.append(&mut deferred);
                    // Decrypt encrypted_content into a system message
                    if let Some(ref encrypted) = c.encrypted_content
                        && let Some(key) = state.compact_key()
                        && let Some(decrypted) = crate::crypto::decrypt(key, encrypted)
                        && !decrypted.is_empty()
                    {
                        messages.push(chat::MessageRequest::System(chat::SystemMessage {
                            content: chat::MessageContent::Text(decrypted),
                            name: None,
                        }));
                    }
                }
                InputItem::FunctionCall(fc) => {
                    pending_tool_calls.push(chat::ToolCallRequest::Function {
                        id: fc.call_id.clone(),
                        function: chat::ToolCallFunction {
                            name: fc.name.clone(),
                            arguments: fc.arguments.clone(),
                        },
                    });
                }
                InputItem::Message(msg) => {
                    flush_tools(
                        &mut messages,
                        &mut pending_tool_calls,
                        &mut pending_reasoning,
                    );
                    messages.append(&mut deferred);
                    let reasoning = match msg.role {
                        MessageRole::Assistant => pending_reasoning.clone(),
                        _ => {
                            pending_reasoning.take();
                            None
                        }
                    };
                    if let Some(chat_msg) = convert_input_message(msg, reasoning) {
                        messages.push(chat_msg);
                    }
                }
                InputItem::CustomToolCall(ctc) => {
                    pending_tool_calls.push(chat::ToolCallRequest::Function {
                        id: ctc.call_id.clone(),
                        function: chat::ToolCallFunction {
                            name: ctc.name.clone(),
                            arguments: ctc.input.clone(),
                        },
                    });
                }
                InputItem::CustomToolCallOutput(ctco) => {
                    let cs = match &ctco.output {
                        CustomToolOutputValue::String(s) => s.clone(),
                        CustomToolOutputValue::Array(blocks) => {
                            extract_text_from_output_blocks(blocks)
                        }
                    };
                    deferred.push(chat::MessageRequest::Tool(chat::ToolMessage {
                        content: chat::MessageContent::Text(cs),
                        tool_call_id: ctco.call_id.clone(),
                    }));
                }
                other => {
                    tracing::warn!(
                        item = ?other,
                        "InputItem variant not convertible to Chat API — skipping"
                    );
                }
            }
        }
        flush_tools(
            &mut messages,
            &mut pending_tool_calls,
            &mut pending_reasoning,
        );
        messages.append(&mut deferred);
    }

    // top_logprobs is a Chat Completions concept; Responses API uses include: ["message.output_text.logprobs"]
    let logprobs: Option<bool> = None;
    let top_logprobs_val: Option<i64> = None;

    let reasoning_effort = reasoning_str.as_ref().map(|s| match s.as_str() {
        "none" => crate::types::ReasoningEffort::None,
        "minimal" => crate::types::ReasoningEffort::Minimal,
        "low" => crate::types::ReasoningEffort::Low,
        "medium" => crate::types::ReasoningEffort::Medium,
        "high" => crate::types::ReasoningEffort::High,
        "max" | "xhigh" => crate::types::ReasoningEffort::Xhigh,
        _ => crate::types::ReasoningEffort::None,
    });

    // Responses streaming completion events include final usage. Chat upstreams only
    // provide that reliably when include_usage is enabled.
    let stream_options = if req.stream {
        Some(chat::StreamOptions {
            include_usage: Some(true),
            include_obfuscation: Some(
                req.stream_options
                    .as_ref()
                    .is_none_or(|so| so.include_obfuscation),
            ),
        })
    } else {
        req.stream_options.as_ref().map(|so| chat::StreamOptions {
            include_usage: None,
            include_obfuscation: Some(so.include_obfuscation),
        })
    };

    // Merge verbosity: TextConfig.verbosity takes precedence over top-level verbosity
    let verbosity = req
        .text
        .as_ref()
        .and_then(|t| t.verbosity.as_ref())
        .or(req.verbosity.as_ref())
        .cloned();

    // Reason: summary / generate_summary — log if set (profile decides downstream field)
    if let Some(ref r) = req.reasoning {
        if r.summary.is_some() {
            tracing::debug!(?r.summary, "reasoning.summary set");
        }
        if r.generate_summary.is_some() {
            tracing::debug!(?r.generate_summary, "reasoning.generate_summary set (deprecated)");
        }
    }

    Ok(chat::Request {
        model: req.model,
        messages,
        temperature: Some(req.temperature),
        top_p: Some(req.top_p),
        max_completion_tokens: req.max_output_tokens,
        stream: Some(req.stream),
        stream_options,
        // Pass through request metadata
        prompt_cache_key: req.prompt_cache_key.clone(),
        prompt_cache_retention: req.prompt_cache_retention.clone(),
        safety_identifier: req.safety_identifier.clone(),
        service_tier: req.service_tier.clone(),
        verbosity,
        parallel_tool_calls: Some(req.parallel_tool_calls),
        store: Some(req.store),
        tools: req.tools.and_then(|tools| {
            let allow = &state.config().allowed_tool_types;
            let converted = tools
                .iter()
                .filter_map(|t| match t {
                    crate::types::tool::ToolRequest::Function(f) => {
                        if allow.contains(&"function".to_string()) {
                            Some(chat::ToolRequest::Function {
                                function: chat::FunctionTool {
                                    name: f.name.clone().unwrap_or_default(),
                                    description: f.description.clone(),
                                    parameters: f.parameters.clone(),
                                    strict: f.strict,
                                },
                            })
                        } else {
                            None
                        }
                    }
                    crate::types::tool::ToolRequest::Custom(c) => {
                        if allow.contains(&"custom".to_string()) {
                            Some(chat::ToolRequest::Custom {
                                custom: chat::CustomTool {
                                    name: c.name.clone().unwrap_or_default(),
                                    description: c.description.clone(),
                                    format: c.format.as_ref().map(|f| match f {
                                        crate::types::tool::CustomToolFormat::Text(_) => {
                                            chat::CustomToolFormat::Text
                                        }
                                        crate::types::tool::CustomToolFormat::Grammar(g) => {
                                            chat::CustomToolFormat::Grammar {
                                                grammar: chat::Grammar {
                                                    definition: g.definition.clone(),
                                                    syntax: g.syntax.clone(),
                                                },
                                            }
                                        }
                                    }),
                                },
                            })
                        } else {
                            None
                        }
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            if converted.is_empty() {
                None
            } else {
                Some(converted)
            }
        }),
        tool_choice: req.tool_choice.and_then(convert_tool_choice),
        response_format,
        stop: req.stop,
        logprobs,
        top_logprobs: top_logprobs_val,
        reasoning_effort,
        ..Default::default()
    })
}

fn convert_tool_choice(tc: crate::types::tool::ToolChoice) -> Option<chat::ToolChoice> {
    match tc {
        crate::types::tool::ToolChoice::String(s) => Some(chat::ToolChoice::Mode(s)),
        crate::types::tool::ToolChoice::Specific(s) if s.tool_type == "custom" => {
            Some(chat::ToolChoice::Custom(chat::ToolChoiceCustom {
                choice_type: "custom".into(),
                custom: chat::ToolChoiceCustomName { name: s.name },
            }))
        }
        crate::types::tool::ToolChoice::Specific(s) if s.tool_type == "function" => {
            Some(chat::ToolChoice::Function(chat::ToolChoiceFunction {
                choice_type: "function".into(),
                function: chat::ToolChoiceFunctionName { name: s.name },
            }))
        }
        crate::types::tool::ToolChoice::Mode(m) => Some(chat::ToolChoice::AllowedTools(
            chat::ToolChoiceAllowedTools {
                choice_type: "allowed_tools".into(),
                mode: m.mode,
                tools: m.tools,
            },
        )),
        other => {
            tracing::warn!(
                ?other,
                "tool_choice variant not convertible to Chat API — skipping"
            );
            None
        }
    }
}

// ── Bulk conversion: Vec<InputItem> → Vec<MessageRequest> ─

pub fn items_to_chat_messages(
    items: &[InputItem],
    state: &crate::app::State,
) -> Vec<chat::MessageRequest> {
    let mut messages: Vec<chat::MessageRequest> = Vec::new();
    let mut pending_reasoning: Option<String> = None;
    let mut deferred: Vec<chat::MessageRequest> = Vec::new();
    let mut pending_tool_calls: Vec<chat::ToolCallRequest> = Vec::new();

    let flush = |msgs: &mut Vec<chat::MessageRequest>,
                 p: &mut Vec<chat::ToolCallRequest>,
                 r: &mut Option<String>| {
        if !p.is_empty() {
            msgs.push(chat::MessageRequest::Assistant(chat::AssistantMessage {
                content: None,
                name: None,
                refusal: None,
                audio: None,
                tool_calls: Some(std::mem::take(p)),
                function_call: None,
                reasoning_content: r.take(),
            }));
        }
    };

    for item in items {
        match item {
            InputItem::FunctionCallOutput(fco) => {
                let cs = match &fco.output {
                    FunctionOutputValue::String(s) => s.clone(),
                    FunctionOutputValue::Array(blocks) => extract_text_from_output_blocks(blocks),
                };
                deferred.push(chat::MessageRequest::Tool(chat::ToolMessage {
                    content: chat::MessageContent::Text(cs),
                    tool_call_id: fco.call_id.clone(),
                }));
            }
            InputItem::Reasoning(r) => {
                flush(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                );
                messages.append(&mut deferred);
                if let Some(t) = extract_reasoning(r, state.compact_key()) {
                    pending_reasoning = Some(match pending_reasoning.take() {
                        Some(e) => format!("{}\n{}", e, t),
                        None => t,
                    });
                }
            }
            InputItem::Compaction(c) => {
                flush(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                );
                messages.append(&mut deferred);
                // Decrypt encrypted_content into a system message
                if let Some(ref encrypted) = c.encrypted_content
                    && let Some(key) = state.compact_key()
                    && let Some(text) = crate::crypto::decrypt(key, encrypted)
                    && !text.is_empty()
                {
                    messages.push(chat::MessageRequest::System(chat::SystemMessage {
                        content: chat::MessageContent::Text(text),
                        name: None,
                    }));
                }
            }
            InputItem::FunctionCall(fc) => {
                pending_tool_calls.push(chat::ToolCallRequest::Function {
                    id: fc.call_id.clone(),
                    function: chat::ToolCallFunction {
                        name: fc.name.clone(),
                        arguments: fc.arguments.clone(),
                    },
                });
            }
            InputItem::Message(msg) => {
                flush(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                );
                messages.append(&mut deferred);
                let r = match msg.role {
                    MessageRole::Assistant => pending_reasoning.clone(),
                    _ => {
                        pending_reasoning.take();
                        None
                    }
                };
                if let Some(chat_msg) = convert_input_message(msg.clone(), r) {
                    messages.push(chat_msg);
                }
            }
            InputItem::CustomToolCall(ctc) => {
                pending_tool_calls.push(chat::ToolCallRequest::Function {
                    id: ctc.call_id.clone(),
                    function: chat::ToolCallFunction {
                        name: ctc.name.clone(),
                        arguments: ctc.input.clone(),
                    },
                });
            }
            InputItem::CustomToolCallOutput(ctco) => {
                let cs = match &ctco.output {
                    CustomToolOutputValue::String(s) => s.clone(),
                    CustomToolOutputValue::Array(blocks) => extract_text_from_output_blocks(blocks),
                };
                deferred.push(chat::MessageRequest::Tool(chat::ToolMessage {
                    content: chat::MessageContent::Text(cs),
                    tool_call_id: ctco.call_id.clone(),
                }));
            }
            other => {
                tracing::warn!(
                    item = ?other,
                    "InputItem variant not convertible to Chat API in items_to_chat_messages — skipping"
                );
            }
        }
    }
    flush(
        &mut messages,
        &mut pending_tool_calls,
        &mut pending_reasoning,
    );
    messages.append(&mut deferred);
    messages
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn convert_input_message(
    msg: InputMessage,
    reasoning: Option<String>,
) -> Option<chat::MessageRequest> {
    if msg.content.is_empty() && msg.role != MessageRole::Assistant {
        return None;
    }
    let mapped_role = match msg.role {
        MessageRole::User => "user",
        MessageRole::System => "system",
        MessageRole::Developer => "developer",
        MessageRole::Assistant => "assistant",
        _ => return None,
    };

    match mapped_role {
        "system" => {
            let text = extract_text(&msg.content);
            Some(chat::MessageRequest::System(chat::SystemMessage {
                content: chat::MessageContent::Text(text),
                name: None,
            }))
        }
        "developer" => {
            let text = extract_text(&msg.content);
            Some(chat::MessageRequest::Developer(chat::DeveloperMessage {
                content: chat::MessageContent::Text(text),
                name: None,
            }))
        }
        "user" => {
            let parts = convert_content_to_user_parts(&msg.content);
            if parts.is_empty() {
                None
            } else if parts
                .iter()
                .all(|p| matches!(p, chat::ContentPart::Text { .. }))
            {
                // All-text content → join into plain string format for compatibility
                let text: String = parts
                    .iter()
                    .filter_map(|p| match p {
                        chat::ContentPart::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Some(chat::MessageRequest::User(chat::UserMessage {
                    content: chat::UserContent::Text(text),
                    name: None,
                }))
            } else {
                // Multimodal content → Parts format
                Some(chat::MessageRequest::User(chat::UserMessage {
                    content: chat::UserContent::Parts(parts),
                    name: None,
                }))
            }
        }
        "assistant" => {
            let text = extract_text(&msg.content);
            Some(chat::MessageRequest::Assistant(chat::AssistantMessage {
                content: if text.is_empty() {
                    None
                } else {
                    Some(chat::AssistantContent::Text(text))
                },
                name: None,
                refusal: None,
                audio: None,
                tool_calls: None,
                function_call: None,
                reasoning_content: reasoning,
            }))
        }
        _ => None,
    }
}

/// Convert Responses input content blocks to Chat user content parts.
fn convert_content_to_user_parts(blocks: &[InputContentBlock]) -> Vec<chat::ContentPart> {
    blocks
        .iter()
        .filter_map(|b| match b {
            InputContentBlock::Text { text } => {
                Some(chat::ContentPart::Text { text: text.clone() })
            }
            InputContentBlock::Image {
                image_url, detail, ..
            } => {
                let url = image_url.clone().unwrap_or_default();
                if url.is_empty() {
                    None
                } else {
                    Some(chat::ContentPart::Image {
                        image_url: chat::ImageUrl {
                            url,
                            detail: detail.clone(),
                        },
                    })
                }
            }
            InputContentBlock::File {
                file_id,
                file_url,
                file_data,
                filename,
            } => {
                if file_id.is_none() && file_url.is_none() && file_data.is_none() {
                    None
                } else {
                    Some(chat::ContentPart::File {
                        file: chat::FileData {
                            file_data: file_data.clone(),
                            file_id: file_id.clone(),
                            filename: filename.clone(),
                        },
                    })
                }
            }
            InputContentBlock::Audio { data, format } => Some(chat::ContentPart::Audio {
                input_audio: chat::InputAudio {
                    data: data.clone(),
                    format: format.clone(),
                },
            }),
        })
        .collect()
}

fn extract_text(blocks: &[InputContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            InputContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_text_from_output_blocks(blocks: &[InputContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            InputContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_reasoning(r: &Reasoning, decrypt_key: Option<&[u8; 32]>) -> Option<String> {
    // Try encrypted_content first (with decryption), then plain content, then summary.
    if let Some(ref encrypted) = r.encrypted_content
        && let Some(key) = decrypt_key
        && let Some(decrypted) = crate::crypto::decrypt(key, encrypted)
        && !decrypted.is_empty()
    {
        return Some(decrypted);
    }
    let mut parts = Vec::new();
    for v in &r.summary {
        parts.push(v.text.clone());
    }
    if let Some(ref content) = r.content {
        for v in content {
            parts.push(v.text.clone());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}
