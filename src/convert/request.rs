use crate::types::{MessageRole, chat, item::*, responses};

// ── Main conversion: Responses API → Chat Completions API ────────────────

/// Convert a Responses API request into a Chat Completions API request.
pub fn responses_to_chat(
    #[allow(unused_variables)] req: responses::Request,
    state: &crate::app::State,
) -> chat::Request {
    let mut messages: Vec<chat::MessageRequest> = Vec::new();
    let mut pending_reasoning: Option<String> = None;

    // Map reasoning effort — Responses API effort → Chat API string
    let effort = req.reasoning.as_ref().and_then(|r| r.effort.as_ref());
    let reasoning_str: Option<String> = match effort {
        None => None,
        Some(e) => match e {
            crate::types::ReasoningEffort::None => None,
            crate::types::ReasoningEffort::Xhigh => Some("max".into()),
            _ => Some("high".into()),
        },
    };

    // Structured output → response_format
    let response_format = req
        .text
        .as_ref()
        .and_then(|t| t.format.as_ref())
        .map(|f| match f {
            responses::TextFormat::JsonSchema { .. } | responses::TextFormat::JsonObject => {
                chat::ResponseFormat::JsonObject(chat::JsonObjectFormat {
                    format_type: "json_object".into(),
                })
            }
            responses::TextFormat::Text => chat::ResponseFormat::Text(chat::TextFormat {
                format_type: "text".into(),
            }),
        });

    // Instructions become a system message
    let mut system_parts: Vec<String> = Vec::new();
    if let Some(ref i) = req.instructions
        && !i.is_empty()
    {
        system_parts.push(i.clone());
    }

    // Walk input items
    let items: Vec<InputItem> = req.input;
    if items.is_empty() {
        if !system_parts.is_empty() {
            messages.push(chat::MessageRequest::System(chat::SystemMessage {
                content: chat::MessageContent::Text(system_parts.join("\n\n")),
                name: None,
            }));
        }
    } else {
        let user_start = items
            .iter()
            .position(|item| matches!(item, InputItem::Message(m) if m.role == MessageRole::User))
            .unwrap_or(0);

        let mut skip: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for (idx, item) in items.iter().enumerate().take(user_start) {
            match item {
                InputItem::Message(m)
                    if m.role == MessageRole::System || m.role == MessageRole::Developer =>
                {
                    let t = extract_text(&m.content);
                    if !t.is_empty() {
                        system_parts.push(t);
                    }
                    skip.insert(idx);
                }
                InputItem::Compaction(c) => {
                    if let Some(ref ec) = c.created_by
                        && let Some(key) = state.compact_key()
                    {
                        let decrypted =
                            crate::crypto::decrypt(key, ec).unwrap_or_else(|| ec.clone());
                        if !decrypted.is_empty() {
                            system_parts.push(decrypted);
                        }
                    }
                    skip.insert(idx);
                }
                _ => {}
            }
        }

        if !system_parts.is_empty() {
            messages.push(chat::MessageRequest::System(chat::SystemMessage {
                content: chat::MessageContent::Text(system_parts.join("\n\n")),
                name: None,
            }));
        }

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

        for (i, item) in items.into_iter().enumerate() {
            if skip.contains(&i) {
                continue;
            }
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
                    if let Some(t) = extract_reasoning(&r) {
                        pending_reasoning = Some(match pending_reasoning.take() {
                            Some(e) => format!("{}\n{}", e, t),
                            None => t,
                        });
                    }
                }
                InputItem::Compaction(_c) => {
                    flush_tools(
                        &mut messages,
                        &mut pending_tool_calls,
                        &mut pending_reasoning,
                    );
                    messages.append(&mut deferred);
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
                _ => {}
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

    chat::Request {
        model: req.model,
        messages,
        temperature: Some(req.temperature),
        top_p: Some(req.top_p),
        max_tokens: req.max_output_tokens,
        stream: Some(req.stream),
        tools: req.tools.map(|tools| {
            tools
                .iter()
                .filter_map(|t| match t {
                    crate::types::tool::ToolRequest::Function(f) => {
                        if state
                            .config()
                            .tool_type_allowlist
                            .contains(&"function".to_string())
                        {
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
                    _ => None,
                })
                .collect()
        }),
        tool_choice: req.tool_choice.map(|tc| match tc {
            crate::types::tool::ToolChoice::String(s) => chat::ToolChoice::Mode(s),
            _ => chat::ToolChoice::Mode("auto".into()),
        }),
        response_format,
        stop: None,
        logprobs,
        top_logprobs: top_logprobs_val,
        reasoning_effort,
        ..Default::default()
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
                if let Some(t) = extract_reasoning(r) {
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
                // Decrypt compaction content into a system message
                if let Some(ref ec) = c.created_by {
                    let text = match state.compact_key() {
                        Some(key) => crate::crypto::decrypt(key, ec).unwrap_or_else(|| ec.clone()),
                        None => ec.clone(),
                    };
                    if !text.is_empty() {
                        messages.push(chat::MessageRequest::System(chat::SystemMessage {
                            content: chat::MessageContent::Text(text),
                            name: None,
                        }));
                    }
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
            _ => {}
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
    let text = extract_text(&msg.content);
    let role_str = match msg.role {
        MessageRole::User => "user",
        MessageRole::System => "system",
        MessageRole::Developer => "system",
        MessageRole::Assistant => "assistant",
        _ => return None,
    };
    match role_str {
        "system" => Some(chat::MessageRequest::System(chat::SystemMessage {
            content: chat::MessageContent::Text(text),
            name: None,
        })),
        "user" => Some(chat::MessageRequest::User(chat::UserMessage {
            content: chat::UserContent::Text(text),
            name: None,
        })),
        "assistant" => Some(chat::MessageRequest::Assistant(chat::AssistantMessage {
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
        })),
        _ => None,
    }
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

fn extract_reasoning(r: &Reasoning) -> Option<String> {
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
