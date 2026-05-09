use crate::models::*;
use uuid::Uuid;

/// Convert a Chat Completions API response back into a Responses API response.
pub fn chat_to_responses(
    chat_resp: ChatCompletionResponse,
    original_model: String,
) -> ResponsesResponse {
    let msg_id = format!("msg_{}", Uuid::new_v4().to_string().replace('-', ""));
    let mut output_items: Vec<OutputItem> = Vec::new();
    let mut incomplete_details: Option<serde_json::Value> = None;

    // Extract the top choice (Responses API typically returns a single output).
    if let Some(choice) = chat_resp.choices.first() {
        // Build the assistant message output item.
        let mut content_blocks: Vec<OutputContentBlock> = Vec::new();

        // Text content
        if let Some(ref text) = choice.message.content {
            if !text.is_empty() {
                content_blocks.push(OutputContentBlock::Text {
                    text: text.clone(),
                    annotations: vec![],
                });
            }
        } else if choice.message.content.is_none()
            && choice.message.tool_calls.is_none()
        {
            // Empty content — treat as refusal if content_filter, else empty text
            if choice.finish_reason.as_deref() == Some("content_filter") {
                content_blocks.push(OutputContentBlock::Refusal {
                    refusal: "content_filter".into(),
                });
            } else {
                content_blocks.push(OutputContentBlock::Text {
                    text: String::new(),
                    annotations: vec![],
                });
            }
        }

        // Determine status from finish_reason.
        let status = match choice.finish_reason.as_deref() {
            Some("stop") => "completed",
            Some("tool_calls") => "completed",
            Some("length") => "completed",
            Some("content_filter") => "incomplete",
            _ => "completed",
        };

        incomplete_details = match choice.finish_reason.as_deref() {
            Some("content_filter") => Some(serde_json::json!({
                "reason": "content_filter"
            })),
            Some("length") => Some(serde_json::json!({
                "reason": "max_output_tokens"
            })),
            _ => None,
        };

        // Add the message output item
        if !content_blocks.is_empty()
            || choice.message.content.is_some()
            || choice.message.tool_calls.is_none()
        {
            output_items.push(OutputItem::Message(OutputMessage {
                id: msg_id.clone(),
                role: "assistant",
                status,
                content: content_blocks,
            }));
        }

        // Convert tool_calls to function_call output items
        if let Some(ref tool_calls) = choice.message.tool_calls {
            for tc in tool_calls {
                output_items.push(OutputItem::FunctionCall(OutputFunctionCall {
                    id: format!("fc_{}", Uuid::new_v4().to_string().replace('-', "")),
                    call_id: tc.id.clone(),
                    name: tc.function.name.clone(),
                    arguments: tc.function.arguments.clone(),
                    status: "completed",
                }));
            }
        }
    }

    // Handle error responses
    let error = chat_resp.error.map(|e| {
        serde_json::json!({
            "code": e.code.unwrap_or_default(),
            "message": e.message,
        })
    });

    let usage = chat_resp.usage.map(|u| {
        let reasoning_tokens = u
            .completion_tokens_details
            .as_ref()
            .and_then(|d| d.get("reasoning_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        let cached_tokens = u
            .prompt_tokens_details
            .as_ref()
            .map(|d| {
                let hit = d
                    .get("prompt_cache_hit_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let miss = d
                    .get("prompt_cache_miss_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                (hit + miss) as u32
            })
            .unwrap_or(0);

        ResponseUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
            input_tokens_details: InputTokensDetails { cached_tokens },
            output_tokens_details: OutputTokensDetails { reasoning_tokens },
        }
    });

    ResponsesResponse {
        id: if chat_resp.id.starts_with("resp_") {
            chat_resp.id
        } else {
            format!("resp_{}", Uuid::new_v4().to_string().replace('-', ""))
        },
        object: "response",
        created_at: chat_resp.created as f64,
        status: if error.is_some() {
            "failed".into()
        } else {
            "completed".into()
        },
        model: original_model,
        output: output_items,
        usage,
        incomplete_details,
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_text_response() {
        let chat = ChatCompletionResponse {
            id: "chatcmpl-123".into(),
            object: "chat.completion".into(),
            created: 1715550000,
            model: "deepseek-chat".into(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".into()),
                    content: Some("Hello! How can I help you?".into()),
                    tool_calls: None,
                },
                finish_reason: Some("stop".into()),
            }],
            usage: Some(ChatUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
                completion_tokens_details: None,
                prompt_tokens_details: None,
            }),
            error: None,
        };

        let resp = chat_to_responses(chat, "gpt-4o".into());

        assert_eq!(resp.object, "response");
        assert_eq!(resp.model, "gpt-4o");
        assert_eq!(resp.status, "completed");
        assert_eq!(resp.created_at, 1715550000.0);
        assert_eq!(resp.output.len(), 1);

        match &resp.output[0] {
            OutputItem::Message(msg) => {
                assert_eq!(msg.role, "assistant");
                assert_eq!(msg.status, "completed");
                assert_eq!(msg.content.len(), 1);
                match &msg.content[0] {
                    OutputContentBlock::Text { text, annotations } => {
                        assert_eq!(text, "Hello! How can I help you?");
                        assert!(annotations.is_empty());
                    }
                    _ => panic!("Expected text content block"),
                }
            }
            _ => panic!("Expected message output item"),
        }

        let usage = resp.usage.unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 5);
        assert_eq!(usage.total_tokens, 15);
    }

    #[test]
    fn test_response_with_tool_calls() {
        let chat = ChatCompletionResponse {
            id: "chatcmpl-456".into(),
            object: "chat.completion".into(),
            created: 1715550000,
            model: "deepseek-chat".into(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".into()),
                    content: Some("Let me check the weather.".into()),
                    tool_calls: Some(vec![ChatToolCall {
                        id: "call_abc".into(),
                        call_type: "function".into(),
                        function: ChatFunctionCall {
                            name: "get_weather".into(),
                            arguments: r#"{"city":"NYC"}"#.into(),
                        },
                    }]),
                },
                finish_reason: Some("tool_calls".into()),
            }],
            usage: Some(ChatUsage {
                prompt_tokens: 20,
                completion_tokens: 10,
                total_tokens: 30,
                completion_tokens_details: None,
                prompt_tokens_details: None,
            }),
            error: None,
        };

        let resp = chat_to_responses(chat, "gpt-4o".into());

        assert_eq!(resp.output.len(), 2);

        // First item: message with assistant text
        match &resp.output[0] {
            OutputItem::Message(msg) => {
                match &msg.content[0] {
                    OutputContentBlock::Text { text, .. } => {
                        assert_eq!(text, "Let me check the weather.");
                    }
                    _ => panic!("Expected text"),
                }
            }
            _ => panic!("Expected message"),
        }

        // Second item: function call
        match &resp.output[1] {
            OutputItem::FunctionCall(fc) => {
                assert_eq!(fc.call_id, "call_abc");
                assert_eq!(fc.name, "get_weather");
                assert_eq!(fc.arguments, r#"{"city":"NYC"}"#);
            }
            _ => panic!("Expected function_call"),
        }
    }

    #[test]
    fn test_usage_mapping() {
        let chat = ChatCompletionResponse {
            id: "chatcmpl-789".into(),
            object: "chat.completion".into(),
            created: 1715550000,
            model: "deepseek-chat".into(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".into()),
                    content: Some("Hi".into()),
                    tool_calls: None,
                },
                finish_reason: Some("stop".into()),
            }],
            usage: Some(ChatUsage {
                prompt_tokens: 100,
                completion_tokens: 50,
                total_tokens: 150,
                completion_tokens_details: Some(
                    serde_json::json!({"reasoning_tokens": 20}),
                ),
                prompt_tokens_details: None,
            }),
            error: None,
        };

        let resp = chat_to_responses(chat, "gpt-4o".into());
        let usage = resp.usage.unwrap();

        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
        assert_eq!(usage.output_tokens_details.reasoning_tokens, 20);
        assert_eq!(usage.input_tokens_details.cached_tokens, 0);
    }

    #[test]
    fn test_multiple_choices() {
        let chat = ChatCompletionResponse {
            id: "chatcmpl-multi".into(),
            object: "chat.completion".into(),
            created: 1715550000,
            model: "deepseek-chat".into(),
            choices: vec![
                ChatChoice {
                    index: 0,
                    message: ChatResponseMessage {
                        role: Some("assistant".into()),
                        content: Some("First choice".into()),
                        tool_calls: None,
                    },
                    finish_reason: Some("stop".into()),
                },
                ChatChoice {
                    index: 1,
                    message: ChatResponseMessage {
                        role: Some("assistant".into()),
                        content: Some("Second choice".into()),
                        tool_calls: None,
                    },
                    finish_reason: Some("stop".into()),
                },
            ],
            usage: None,
            error: None,
        };

        let resp = chat_to_responses(chat, "gpt-4o".into());
        // Only the first choice is used in Responses API output
        assert_eq!(resp.output.len(), 1);
        match &resp.output[0] {
            OutputItem::Message(msg) => match &msg.content[0] {
                OutputContentBlock::Text { text, .. } => {
                    assert_eq!(text, "First choice");
                }
                _ => panic!("Expected text"),
            },
            _ => panic!("Expected message"),
        }
    }

    #[test]
    fn test_length_finish_reason() {
        let chat = ChatCompletionResponse {
            id: "chatcmpl-len".into(),
            object: "chat.completion".into(),
            created: 1715550000,
            model: "deepseek-chat".into(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".into()),
                    content: Some("Truncated...".into()),
                    tool_calls: None,
                },
                finish_reason: Some("length".into()),
            }],
            usage: None,
            error: None,
        };

        let resp = chat_to_responses(chat, "gpt-4o".into());
        assert_eq!(resp.status, "completed");
    }

    #[test]
    fn test_content_filter_finish_reason() {
        let chat = ChatCompletionResponse {
            id: "chatcmpl-cf".into(),
            object: "chat.completion".into(),
            created: 1715550000,
            model: "deepseek-chat".into(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".into()),
                    content: None,
                    tool_calls: None,
                },
                finish_reason: Some("content_filter".into()),
            }],
            usage: None,
            error: None,
        };

        let resp = chat_to_responses(chat, "gpt-4o".into());
        match &resp.output[0] {
            OutputItem::Message(msg) => {
                assert_eq!(msg.status, "incomplete");
            }
            _ => panic!("Expected message"),
        }
    }

    #[test]
    fn test_error_response() {
        let chat = ChatCompletionResponse {
            id: "".into(),
            object: "error".into(),
            created: 0,
            model: "".into(),
            choices: vec![],
            usage: None,
            error: Some(ChatError {
                message: "Invalid API key".into(),
                code: Some("invalid_api_key".into()),
            }),
        };

        let resp = chat_to_responses(chat, "gpt-4o".into());
        assert_eq!(resp.status, "failed");
        assert!(resp.error.is_some());
        assert_eq!(resp.output.len(), 0);
    }

    #[test]
    fn test_response_with_reasoning_tokens() {
        let chat = ChatCompletionResponse {
            id: "chatcmpl-reason".into(),
            object: "chat.completion".into(),
            created: 1715550000,
            model: "deepseek-reasoner".into(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".into()),
                    content: Some("Answer".into()),
                    tool_calls: None,
                },
                finish_reason: Some("stop".into()),
            }],
            usage: Some(ChatUsage {
                prompt_tokens: 50,
                completion_tokens: 100,
                total_tokens: 150,
                completion_tokens_details: Some(
                    serde_json::json!({"reasoning_tokens": 80}),
                ),
                prompt_tokens_details: None,
            }),
            error: None,
        };

        let resp = chat_to_responses(chat, "gpt-4o".into());
        let usage = resp.usage.unwrap();
        assert_eq!(usage.output_tokens_details.reasoning_tokens, 80);
        assert_eq!(usage.output_tokens, 100);
    }

    #[test]
    fn test_response_id_is_prefixed() {
        let chat = ChatCompletionResponse {
            id: "some-chat-id-12345".into(),
            object: "chat.completion".into(),
            created: 1715550000,
            model: "deepseek-chat".into(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".into()),
                    content: Some("Hi".into()),
                    tool_calls: None,
                },
                finish_reason: Some("stop".into()),
            }],
            usage: None,
            error: None,
        };

        let resp = chat_to_responses(chat, "gpt-4o".into());
        assert!(resp.id.starts_with("resp_"));
    }

    #[test]
    fn test_empty_choices_produces_empty_output() {
        let chat = ChatCompletionResponse {
            id: "chatcmpl-empty".into(),
            object: "chat.completion".into(),
            created: 1715550000,
            model: "deepseek-chat".into(),
            choices: vec![],
            usage: None,
            error: None,
        };

        let resp = chat_to_responses(chat, "gpt-4o".into());
        assert_eq!(resp.status, "completed");
        assert_eq!(resp.output.len(), 0);
    }

    #[test]
    fn test_null_content_with_tool_calls() {
        // Pure tool call response — content is null, tool_calls are present
        let chat = ChatCompletionResponse {
            id: "chatcmpl-tools".into(),
            object: "chat.completion".into(),
            created: 1715550000,
            model: "deepseek-chat".into(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".into()),
                    content: None,
                    tool_calls: Some(vec![ChatToolCall {
                        id: "call_xyz".into(),
                        call_type: "function".into(),
                        function: ChatFunctionCall {
                            name: "search".into(),
                            arguments: r#"{"q":"test"}"#.into(),
                        },
                    }]),
                },
                finish_reason: Some("tool_calls".into()),
            }],
            usage: None,
            error: None,
        };

        let resp = chat_to_responses(chat, "gpt-4o".into());
        assert_eq!(resp.output.len(), 1); // just the function_call, no message
        match &resp.output[0] {
            OutputItem::FunctionCall(fc) => {
                assert_eq!(fc.name, "search");
            }
            other => panic!("Expected FunctionCall, got {:?}", other),
        }
    }

    #[test]
    fn test_content_filter_produces_refusal() {
        let chat = ChatCompletionResponse {
            id: "chatcmpl-cf2".into(),
            object: "chat.completion".into(),
            created: 1715550000,
            model: "deepseek-chat".into(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".into()),
                    content: None,
                    tool_calls: None,
                },
                finish_reason: Some("content_filter".into()),
            }],
            usage: None,
            error: None,
        };

        let resp = chat_to_responses(chat, "gpt-4o".into());
        match &resp.output[0] {
            OutputItem::Message(msg) => {
                assert_eq!(msg.status, "incomplete");
                match &msg.content[0] {
                    OutputContentBlock::Refusal { refusal } => {
                        assert_eq!(refusal, "content_filter");
                    }
                    other => panic!("Expected Refusal, got {:?}", other),
                }
            }
            other => panic!("Expected Message, got {:?}", other),
        }
    }

    #[test]
    fn test_incomplete_details_for_length() {
        let chat = ChatCompletionResponse {
            id: "chatcmpl-len2".into(),
            object: "chat.completion".into(),
            created: 1715550000,
            model: "deepseek-chat".into(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".into()),
                    content: Some("truncated...".into()),
                    tool_calls: None,
                },
                finish_reason: Some("length".into()),
            }],
            usage: None,
            error: None,
        };

        let resp = chat_to_responses(chat, "gpt-4o".into());
        let details = resp.incomplete_details.unwrap();
        assert_eq!(details["reason"], "max_output_tokens");
    }

    #[test]
    fn test_incomplete_details_for_content_filter() {
        let chat = ChatCompletionResponse {
            id: "chatcmpl-cf3".into(),
            object: "chat.completion".into(),
            created: 1715550000,
            model: "deepseek-chat".into(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".into()),
                    content: Some("filtered...".into()),
                    tool_calls: None,
                },
                finish_reason: Some("content_filter".into()),
            }],
            usage: None,
            error: None,
        };

        let resp = chat_to_responses(chat, "gpt-4o".into());
        let details = resp.incomplete_details.unwrap();
        assert_eq!(details["reason"], "content_filter");
    }

    #[test]
    fn test_no_incomplete_details_for_stop() {
        let chat = ChatCompletionResponse {
            id: "chatcmpl-stop".into(),
            object: "chat.completion".into(),
            created: 1715550000,
            model: "deepseek-chat".into(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".into()),
                    content: Some("done".into()),
                    tool_calls: None,
                },
                finish_reason: Some("stop".into()),
            }],
            usage: None,
            error: None,
        };

        let resp = chat_to_responses(chat, "gpt-4o".into());
        assert!(resp.incomplete_details.is_none());
    }

    #[test]
    fn test_prompt_cache_tokens_mapped() {
        let chat = ChatCompletionResponse {
            id: "chatcmpl-cache".into(),
            object: "chat.completion".into(),
            created: 1715550000,
            model: "deepseek-chat".into(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".into()),
                    content: Some("cached response".into()),
                    tool_calls: None,
                },
                finish_reason: Some("stop".into()),
            }],
            usage: Some(ChatUsage {
                prompt_tokens: 100,
                completion_tokens: 30,
                total_tokens: 130,
                completion_tokens_details: None,
                prompt_tokens_details: Some(serde_json::json!({
                    "prompt_cache_hit_tokens": 80,
                    "prompt_cache_miss_tokens": 20
                })),
            }),
            error: None,
        };

        let resp = chat_to_responses(chat, "gpt-4o".into());
        let usage = resp.usage.unwrap();
        assert_eq!(usage.input_tokens_details.cached_tokens, 100); // 80 + 20
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 30);
    }
}
