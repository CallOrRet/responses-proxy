use crate::models::*;

/// Convert a Responses API request into a Chat Completions API request.
pub fn responses_to_chat(req: ResponsesRequest) -> ChatCompletionRequest {
    let mut messages = Vec::new();

    // 1. Handle `instructions` → prepend as system message.
    if let Some(ref instructions) = req.instructions {
        if !instructions.is_empty() {
            messages.push(ChatMessage {
                role: "system".into(),
                content: ChatMessageContent::String(instructions.clone()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            });
        }
    }

    // 2. Convert `input` items.
    match req.input {
        Input::String(s) => {
            messages.push(ChatMessage {
                role: "user".into(),
                content: ChatMessageContent::String(s),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            });
        }
        Input::Array(items) => {
            for item in items {
                match item {
                    InputItem::Message(msg) => {
                        if let Some(chat_msg) = convert_input_message(&msg) {
                            // Merge system/developer messages with existing system message
                            // if instructions already created one.
                            if (msg.role == MessageRole::System
                                || msg.role == MessageRole::Developer)
                                && messages.first().map_or(false, |m| m.role == "system")
                            {
                                let text = extract_text_from_content(&msg.content);
                                if let ChatMessageContent::String(ref mut existing) =
                                    messages[0].content
                                {
                                    existing.push_str("\n\n");
                                    existing.push_str(&text);
                                }
                            } else {
                                messages.push(chat_msg);
                            }
                        }
                    }
                    InputItem::FunctionCall(fc) => {
                        messages.push(ChatMessage {
                            role: "assistant".into(),
                            content: ChatMessageContent::Null,
                            name: None,
                            tool_calls: Some(vec![ChatToolCall {
                                id: fc.call_id.clone(),
                                call_type: "function".into(),
                                function: ChatFunctionCall {
                                    name: fc.name,
                                    arguments: fc.arguments,
                                },
                            }]),
                            tool_call_id: None,
                        });
                    }
                    InputItem::FunctionCallOutput(fco) => {
                        let content_str = match fco.output {
                            FunctionCallOutputValue::String(s) => s,
                            FunctionCallOutputValue::Array(ref blocks) => {
                                extract_text_from_output_blocks(blocks)
                            }
                        };
                        messages.push(ChatMessage {
                            role: "tool".into(),
                            content: ChatMessageContent::String(content_str),
                            name: None,
                            tool_calls: None,
                            tool_call_id: Some(fco.call_id),
                        });
                    }
                    InputItem::Unknown(_) => {
                        // Skip unknown item types for maximum compatibility.
                    }
                }
            }
        }
    }

    // 3. Normalize tools: Responses API flat format → Chat API nested format.
    let chat_tools = req.tools.map(|tools| {
        tools
            .into_iter()
            .filter_map(|t| {
                if t.tool_type == "function" {
                    Some(ChatTool {
                        tool_type: "function".into(),
                        function: ChatFunctionDef {
                            name: t.name.unwrap_or_default(),
                            description: t.description,
                            parameters: t.parameters,
                            strict: t.strict,
                        },
                    })
                } else {
                    // For mcp, web_search, etc., we skip them since Chat API
                    // doesn't support them natively.
                    None
                }
            })
            .collect()
    });

    // 4. Map `reasoning` → DeepSeek `thinking`.
    let thinking = req.reasoning.as_ref().map(|_| ThinkingConfig {
        thinking_type: "enabled".into(),
    });

    ChatCompletionRequest {
        model: req.model,
        messages,
        temperature: req.temperature,
        top_p: req.top_p,
        max_tokens: req.max_output_tokens,
        tools: chat_tools,
        tool_choice: req.tool_choice,
        stream: req.stream,
        stop: req.stop,
        top_logprobs: req.top_logprobs,
        thinking,
    }
}

/// Convert an input message item to a ChatMessage.
/// Returns None if the message has no text content.
fn convert_input_message(msg: &InputMessage) -> Option<ChatMessage> {
    let role = match msg.role {
        MessageRole::User => "user",
        MessageRole::System => "system",
        MessageRole::Developer => "system", // Chat API doesn't have "developer" role
        MessageRole::Assistant => "assistant",
    };

    let text = extract_text_from_content(&msg.content);
    if text.is_empty() && msg.role != MessageRole::Assistant {
        return None;
    }

    // Assistant messages may have no content (just tool_calls), but in the
    // Responses API input, assistant messages don't carry tool_calls directly —
    // tool calls are separate function_call items in the input array.
    let content = if text.is_empty() && role == "assistant" {
        ChatMessageContent::Null
    } else {
        ChatMessageContent::String(text)
    };

    Some(ChatMessage {
        role: role.into(),
        content,
        name: None,
        tool_calls: None,
        tool_call_id: None,
    })
}

/// Extract text from input content blocks (join all `input_text` blocks).
fn extract_text_from_content(blocks: &[InputContentBlock]) -> String {
    let parts: Vec<&str> = blocks
        .iter()
        .filter_map(|b| match b {
            InputContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    parts.join("\n")
}

/// Extract text from function_call_output blocks array.
fn extract_text_from_output_blocks(blocks: &[serde_json::Value]) -> String {
    let parts: Vec<&str> = blocks
        .iter()
        .filter_map(|v| {
            v.get("type")
                .and_then(|t| t.as_str())
                .filter(|t| *t == "input_text")
                .and_then(|_| v.get("text"))
                .and_then(|t| t.as_str())
        })
        .collect();
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_text_content(text: &str) -> Vec<InputContentBlock> {
        vec![InputContentBlock::Text {
            text: text.into(),
        }]
    }

    #[test]
    fn test_string_input_becomes_user_message() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::String("Hello world".into()),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.messages.len(), 1);
        assert_eq!(chat.messages[0].role, "user");
        assert_eq!(chat.messages[0].content.as_str().unwrap(), "Hello world");
        assert_eq!(chat.model, "gpt-4o");
    }

    #[test]
    fn test_instructions_becomes_system_message() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::String("What is Rust?".into()),
            instructions: Some("You are a helpful assistant.".into()),
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.messages.len(), 2);
        assert_eq!(chat.messages[0].role, "system");
        assert_eq!(
            chat.messages[0].content.as_str().unwrap(),
            "You are a helpful assistant."
        );
        assert_eq!(chat.messages[1].role, "user");
    }

    #[test]
    fn test_array_input_with_user_message() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::Array(vec![InputItem::Message(InputMessage {
                role: MessageRole::User,
                content: make_text_content("Hello"),
                status: None,
            })]),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.messages.len(), 1);
        assert_eq!(chat.messages[0].role, "user");
        assert_eq!(chat.messages[0].content.as_str().unwrap(), "Hello");
    }

    #[test]
    fn test_array_input_with_system_and_user() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::Array(vec![
                InputItem::Message(InputMessage {
                    role: MessageRole::System,
                    content: make_text_content("You are a bot."),
                    status: None,
                }),
                InputItem::Message(InputMessage {
                    role: MessageRole::User,
                    content: make_text_content("Hi"),
                    status: None,
                }),
            ]),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.messages.len(), 2);
        assert_eq!(chat.messages[0].role, "system");
        assert_eq!(chat.messages[1].role, "user");
    }

    #[test]
    fn test_instructions_merged_with_system_message() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::Array(vec![
                InputItem::Message(InputMessage {
                    role: MessageRole::System,
                    content: make_text_content("System instruction from input."),
                    status: None,
                }),
                InputItem::Message(InputMessage {
                    role: MessageRole::User,
                    content: make_text_content("Hi"),
                    status: None,
                }),
            ]),
            instructions: Some("Top-level instructions.".into()),
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.messages.len(), 2);
        assert_eq!(chat.messages[0].role, "system");
        let content = chat.messages[0].content.as_str().unwrap();
        assert!(content.contains("Top-level instructions."));
        assert!(content.contains("System instruction from input."));
        // instructions come first, then the input system message
        assert!(content.starts_with("Top-level instructions."));
    }

    #[test]
    fn test_function_call_item_to_assistant_message() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::Array(vec![InputItem::FunctionCall(FunctionCallItem {
                call_id: "call_abc".into(),
                name: "get_weather".into(),
                arguments: r#"{"city":"NYC"}"#.into(),
                id: None,
                status: None,
            })]),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.messages.len(), 1);
        assert_eq!(chat.messages[0].role, "assistant");
        let tool_calls = chat.messages[0].tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_abc");
        assert_eq!(tool_calls[0].function.name, "get_weather");
        assert_eq!(tool_calls[0].function.arguments, r#"{"city":"NYC"}"#);
    }

    #[test]
    fn test_function_call_output_to_tool_message() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::Array(vec![InputItem::FunctionCallOutput(
                FunctionCallOutputItem {
                    call_id: "call_abc".into(),
                    output: FunctionCallOutputValue::String("72°F and sunny".into()),
                    id: None,
                    status: None,
                },
            )]),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.messages.len(), 1);
        assert_eq!(chat.messages[0].role, "tool");
        assert_eq!(
            chat.messages[0].content.as_str().unwrap(),
            "72°F and sunny"
        );
        assert_eq!(
            chat.messages[0].tool_call_id.as_deref().unwrap(),
            "call_abc"
        );
    }

    #[test]
    fn test_full_tool_conversation_roundtrip() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::Array(vec![
                InputItem::Message(InputMessage {
                    role: MessageRole::User,
                    content: make_text_content("What's the weather in NYC?"),
                    status: None,
                }),
                InputItem::FunctionCall(FunctionCallItem {
                    call_id: "call_1".into(),
                    name: "get_weather".into(),
                    arguments: r#"{"city":"New York"}"#.into(),
                    id: None,
                    status: None,
                }),
                InputItem::FunctionCallOutput(FunctionCallOutputItem {
                    call_id: "call_1".into(),
                    output: FunctionCallOutputValue::String("Sunny, 72°F".into()),
                    id: None,
                    status: None,
                }),
                InputItem::Message(InputMessage {
                    role: MessageRole::Assistant,
                    content: make_text_content("The weather in NYC is sunny and 72°F."),
                    status: None,
                }),
            ]),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.messages.len(), 4);
        assert_eq!(chat.messages[0].role, "user");
        assert_eq!(chat.messages[1].role, "assistant");
        assert!(chat.messages[1].tool_calls.is_some());
        assert_eq!(chat.messages[2].role, "tool");
        assert_eq!(chat.messages[3].role, "assistant");
        assert_eq!(
            chat.messages[3].content.as_str().unwrap(),
            "The weather in NYC is sunny and 72°F."
        );
    }

    #[test]
    fn test_tools_normalization_flat_to_nested() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::String("Hi".into()),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: Some(vec![ToolParam {
                tool_type: "function".into(),
                name: Some("get_weather".into()),
                description: Some("Get the weather".into()),
                parameters: Some(serde_json::json!({"type": "object", "properties": {}})),
                strict: Some(true),
            }]),
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        let tools = chat.tools.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].tool_type, "function");
        assert_eq!(tools[0].function.name, "get_weather");
        assert_eq!(tools[0].function.description.as_deref().unwrap(), "Get the weather");
        assert_eq!(tools[0].function.strict.unwrap(), true);
    }

    #[test]
    fn test_non_function_tools_are_filtered() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::String("Hi".into()),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: Some(vec![
                ToolParam {
                    tool_type: "function".into(),
                    name: Some("func1".into()),
                    description: None,
                    parameters: None,
                    strict: None,
                },
                ToolParam {
                    tool_type: "web_search_preview".into(),
                    name: None,
                    description: None,
                    parameters: None,
                    strict: None,
                },
            ]),
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        let tools = chat.tools.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].function.name, "func1");
    }

    #[test]
    fn test_temperature_and_top_p_passthrough() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::String("Hi".into()),
            instructions: None,
            temperature: Some(0.7),
            top_p: Some(0.9),
            max_output_tokens: Some(2048),
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.temperature, Some(0.7));
        assert_eq!(chat.top_p, Some(0.9));
        assert_eq!(chat.max_tokens, Some(2048));
    }

    #[test]
    fn test_stop_passthrough() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::String("Hi".into()),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: Some(Stop::Multiple(vec!["END".into(), "STOP".into()])),
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert!(chat.stop.is_some());
    }

    #[test]
    fn test_stream_flag_passthrough() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::String("Hi".into()),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: Some(true),
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.stream, Some(true));
    }

    #[test]
    fn test_developer_role_maps_to_system() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::Array(vec![
                InputItem::Message(InputMessage {
                    role: MessageRole::Developer,
                    content: make_text_content("Dev instructions"),
                    status: None,
                }),
                InputItem::Message(InputMessage {
                    role: MessageRole::User,
                    content: make_text_content("Hi"),
                    status: None,
                }),
            ]),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.messages[0].role, "system");
        assert_eq!(chat.messages[0].content.as_str().unwrap(), "Dev instructions");
    }

    #[test]
    fn test_multiple_content_blocks_joined() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::Array(vec![InputItem::Message(InputMessage {
                role: MessageRole::User,
                content: vec![
                    InputContentBlock::Text {
                        text: "Hello".into(),
                    },
                    InputContentBlock::Text {
                        text: "World".into(),
                    },
                ],
                status: None,
            })]),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.messages[0].content.as_str().unwrap(), "Hello\nWorld");
    }

    #[test]
    fn test_empty_input_array() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::Array(vec![]),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.messages.len(), 0);
    }

    #[test]
    fn test_tool_choice_passthrough() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::String("Hi".into()),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: Some(serde_json::json!("required")),
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.tool_choice.unwrap(), serde_json::json!("required"));
    }

    #[test]
    fn test_function_call_output_array_extracts_text() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::Array(vec![InputItem::FunctionCallOutput(
                FunctionCallOutputItem {
                    call_id: "call_1".into(),
                    output: FunctionCallOutputValue::Array(vec![serde_json::json!({
                        "type": "input_text",
                        "text": "Result text here"
                    })]),
                    id: None,
                    status: None,
                },
            )]),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.messages[0].role, "tool");
        assert_eq!(chat.messages[0].content.as_str().unwrap(), "Result text here");
    }

    #[test]
    fn test_image_and_file_blocks_filtered_out() {
        // image and file blocks should be silently ignored, only text extracted
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::Array(vec![InputItem::Message(InputMessage {
                role: MessageRole::User,
                content: vec![
                    InputContentBlock::Text {
                        text: "Describe this image:".into(),
                    },
                    InputContentBlock::Image(serde_json::json!({
                        "type": "input_image",
                        "image_url": "https://example.com/img.png"
                    })),
                    InputContentBlock::File(serde_json::json!({
                        "type": "input_file",
                        "file_id": "file-abc"
                    })),
                ],
                status: None,
            })]),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        // Should only have the text content, image/file silently dropped
        assert_eq!(
            chat.messages[0].content.as_str().unwrap(),
            "Describe this image:"
        );
    }

    #[test]
    fn test_unknown_input_item_ignored() {
        // Unknown item types should be silently skipped
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::Array(vec![
                InputItem::Unknown(serde_json::json!({
                    "type": "item_reference",
                    "id": "item_abc"
                })),
                InputItem::Message(InputMessage {
                    role: MessageRole::User,
                    content: make_text_content("Hello"),
                    status: None,
                }),
                InputItem::Unknown(serde_json::json!({
                    "type": "reasoning_item",
                    "content": "let me think..."
                })),
            ]),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        // Only the user message should survive; unknowns skipped
        assert_eq!(chat.messages.len(), 1);
        assert_eq!(chat.messages[0].role, "user");
        assert_eq!(chat.messages[0].content.as_str().unwrap(), "Hello");
    }

    #[test]
    fn test_unknown_content_block_ignored() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::Array(vec![InputItem::Message(InputMessage {
                role: MessageRole::User,
                content: vec![
                    InputContentBlock::Text {
                        text: "Hello".into(),
                    },
                    InputContentBlock::Unknown(serde_json::json!({
                        "type": "some_future_block_type",
                        "data": "ignored"
                    })),
                    InputContentBlock::Text {
                        text: "World".into(),
                    },
                ],
                status: None,
            })]),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.messages[0].content.as_str().unwrap(), "Hello\nWorld");
    }

    #[test]
    fn test_stop_single_string() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::String("Hi".into()),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: Some(Stop::Single("END".into())),
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert!(chat.stop.is_some());
    }

    #[test]
    fn test_top_logprobs_passthrough() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::String("Hi".into()),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: Some(5),
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.top_logprobs, Some(5));
    }

    #[test]
    fn test_empty_instructions_ignored() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::String("Hi".into()),
            instructions: Some("".into()),
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        // Empty instructions should not produce a system message
        assert_eq!(chat.messages.len(), 1);
        assert_eq!(chat.messages[0].role, "user");
    }

    #[test]
    fn test_reasoning_maps_to_thinking() {
        let req = ResponsesRequest {
            model: "deepseek-reasoner".into(),
            input: Input::String("Solve complex math".into()),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: Some(serde_json::json!({"effort": "high"})),
            text: None,
        };

        let chat = responses_to_chat(req);
        assert!(chat.thinking.is_some());
        let thinking = chat.thinking.unwrap();
        assert_eq!(thinking.thinking_type, "enabled");
    }

    #[test]
    fn test_no_reasoning_no_thinking() {
        let req = ResponsesRequest {
            model: "deepseek-chat".into(),
            input: Input::String("Hi".into()),
            instructions: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert!(chat.thinking.is_none());
    }

    #[test]
    fn test_developer_role_merges_with_instructions() {
        // instructions + developer role in input should merge (same as system merge)
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: Input::Array(vec![
                InputItem::Message(InputMessage {
                    role: MessageRole::Developer,
                    content: make_text_content("Developer instruction."),
                    status: None,
                }),
                InputItem::Message(InputMessage {
                    role: MessageRole::User,
                    content: make_text_content("Hi"),
                    status: None,
                }),
            ]),
            instructions: Some("Top-level instructions.".into()),
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
            stop: None,
            top_logprobs: None,
            previous_response_id: None,
            store: None,
            metadata: None,
            reasoning: None,
            text: None,
        };

        let chat = responses_to_chat(req);
        assert_eq!(chat.messages.len(), 2);
        assert_eq!(chat.messages[0].role, "system");
        let content = chat.messages[0].content.as_str().unwrap();
        assert!(content.contains("Top-level instructions."));
        assert!(content.contains("Developer instruction."));
    }
}
