/// Verification tests: real code with realistic payloads.
/// Run with: cargo test verification
use crate::convert_request::responses_to_chat;
use crate::convert_response::chat_to_responses;
use crate::models::*;
use crate::streaming;
use serde_json::json;

// ── Scenario 1: Simple text, no streaming ────────────────────────────

#[test]
fn s1_simple_text_request() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "What is 2+2? Reply with just the number."
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["model"], "gpt-5.5");
    assert_eq!(j["messages"].as_array().unwrap().len(), 1);
    assert_eq!(j["messages"][0]["role"], "user");
    assert_eq!(
        j["messages"][0]["content"],
        "What is 2+2? Reply with just the number."
    );
    assert!(j.get("thinking").is_none());
    assert!(j.get("reasoning_effort").is_none());
}

#[test]
fn s1_simple_text_response() {
    let chat: ChatCompletionResponse = serde_json::from_value(json!({
        "id": "chatcmpl-abc123",
        "object": "chat.completion",
        "created": 1715550000u64,
        "model": "deepseek-v4-pro",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "4"},
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 12,
            "completion_tokens": 1,
            "total_tokens": 13,
            "completion_tokens_details": {"reasoning_tokens": 0}
        }
    }))
    .unwrap();

    let resp = chat_to_responses(chat, "gpt-5.5".into());
    let j = serde_json::to_value(&resp).unwrap();

    assert_eq!(j["object"], "response");
    assert_eq!(j["status"], "completed");
    assert_eq!(j["model"], "gpt-5.5");
    assert!(j["id"].as_str().unwrap().starts_with("resp_"));

    let output = j["output"].as_array().unwrap();
    assert_eq!(output.len(), 1);
    assert_eq!(output[0]["type"], "message");
    assert_eq!(output[0]["role"], "assistant");
    assert_eq!(output[0]["status"], "completed");
    assert_eq!(output[0]["content"][0]["text"], "4");

    assert_eq!(j["usage"]["input_tokens"], 12);
    assert_eq!(j["usage"]["output_tokens"], 1);
    assert_eq!(j["usage"]["total_tokens"], 13);
    assert_eq!(j["usage"]["input_tokens_details"]["cached_tokens"], 0);
    assert_eq!(j["usage"]["output_tokens_details"]["reasoning_tokens"], 0);
}

// ── Scenario 2: Instructions + Reasoning xhigh ───────────────────────

#[test]
fn s2_instructions_and_reasoning_request() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Solve the complex equation.",
        "instructions": "You are a math tutor. Always show your work.",
        "reasoning": {"effort": "xhigh"}
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    let msgs = j["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0]["role"], "system");
    assert_eq!(
        msgs[0]["content"],
        "You are a math tutor. Always show your work."
    );
    assert_eq!(msgs[1]["role"], "user");

    assert_eq!(j["thinking"]["type"], "enabled");
    assert_eq!(j["reasoning_effort"], "max");
}

#[test]
fn s2_reasoning_content_response() {
    let chat: ChatCompletionResponse = serde_json::from_value(json!({
        "id": "chatcmpl-def456",
        "object": "chat.completion",
        "created": 1715550000u64,
        "model": "deepseek-v4-pro",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "x = 5",
                "reasoning_content": "First, we isolate x by..."
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 40,
            "completion_tokens": 50,
            "total_tokens": 90,
            "completion_tokens_details": {"reasoning_tokens": 30}
        }
    }))
    .unwrap();

    let resp = chat_to_responses(chat, "gpt-5.5".into());
    let j = serde_json::to_value(&resp).unwrap();

    let output = j["output"].as_array().unwrap();
    assert_eq!(output.len(), 2);
    // reasoning item first
    assert_eq!(output[0]["type"], "reasoning");
    assert_eq!(output[0]["summary"][0]["type"], "summary_text");
    assert_eq!(output[0]["summary"][0]["text"], "First, we isolate x by...");
    // message second
    assert_eq!(output[1]["type"], "message");
    assert_eq!(output[1]["content"][0]["text"], "x = 5");
    // usage
    assert_eq!(j["usage"]["output_tokens_details"]["reasoning_tokens"], 30);
}

// ── Scenario 3: All reasoning effort levels ──────────────────────────

#[test]
fn s3_reasoning_effort_all_levels() {
    let cases = &[
        ("none", false, None),
        ("minimal", true, Some("high")),
        ("low", true, Some("high")),
        ("medium", true, Some("high")),
        ("high", true, Some("high")),
        ("xhigh", true, Some("max")),
    ];

    for (effort, expect_think, expect_re) in cases {
        let req: ResponsesRequest = serde_json::from_value(json!({
            "model": "gpt-5.5",
            "input": "Hi",
            "reasoning": {"effort": effort}
        }))
        .unwrap();

        let chat = responses_to_chat(req, &["function".into()]);
        let j = serde_json::to_value(&chat).unwrap();

        if *expect_think {
            assert_eq!(j["thinking"]["type"], "enabled", "effort={effort}");
            assert_eq!(j["reasoning_effort"], expect_re.unwrap(), "effort={effort}");
        } else {
            assert!(
                j.get("thinking").is_none(),
                "effort={effort} should have no thinking"
            );
            assert!(
                j.get("reasoning_effort").is_none(),
                "effort={effort} should have no reasoning_effort"
            );
        }
    }
}

// ── Scenario 4: Thinking mode strips logprobs ────────────────────────

#[test]
fn s4_thinking_disables_logprobs() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Hi",
        "top_logprobs": 5,
        "reasoning": {"effort": "high"}
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    assert!(j.get("logprobs").is_none());
    assert!(j.get("top_logprobs").is_none());
}

// ── Scenario 5: Full tool conversation roundtrip ─────────────────────

#[test]
fn s5_tool_conversation_request() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": [
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Weather in NYC?"}]},
            {"type": "function_call", "call_id": "call_1", "name": "get_weather", "arguments": "{\"city\":\"New York\"}"},
            {"type": "function_call_output", "call_id": "call_1", "output": "Sunny, 72F"},
            {"type": "message", "role": "assistant", "content": [{"type": "input_text", "text": "NYC is sunny, 72F."}]}
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    let msgs = j["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 4);
    assert_eq!(msgs[0]["role"], "user");
    assert_eq!(msgs[0]["content"], "Weather in NYC?");
    assert_eq!(msgs[1]["role"], "assistant");
    assert_eq!(msgs[1]["tool_calls"][0]["id"], "call_1");
    assert_eq!(msgs[1]["tool_calls"][0]["function"]["name"], "get_weather");
    assert_eq!(msgs[2]["role"], "tool");
    assert_eq!(msgs[2]["tool_call_id"], "call_1");
    assert_eq!(msgs[2]["content"], "Sunny, 72F");
    assert_eq!(msgs[3]["role"], "assistant");
    assert_eq!(msgs[3]["content"], "NYC is sunny, 72F.");
}

#[test]
fn s5_tool_call_response() {
    let chat: ChatCompletionResponse = serde_json::from_value(json!({
        "id": "chatcmpl-tools",
        "object": "chat.completion",
        "created": 1715550000u64,
        "model": "deepseek-v4-pro",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_xyz",
                    "type": "function",
                    "function": {"name": "search", "arguments": "{\"q\":\"test\"}"}
                }]
            },
            "finish_reason": "tool_calls"
        }]
    }))
    .unwrap();

    let resp = chat_to_responses(chat, "gpt-5.5".into());
    let j = serde_json::to_value(&resp).unwrap();

    let output = j["output"].as_array().unwrap();
    // Only function_call, no message (content=null + tool_calls present)
    assert_eq!(output.len(), 1);
    assert_eq!(output[0]["type"], "function_call");
    assert_eq!(output[0]["call_id"], "call_xyz");
    assert_eq!(output[0]["name"], "search");
}

// ── Scenario 6: Content filter ───────────────────────────────────────

#[test]
fn s6_content_filter_response() {
    let chat: ChatCompletionResponse = serde_json::from_value(json!({
        "id": "chatcmpl-cf",
        "object": "chat.completion",
        "created": 1715550000u64,
        "model": "deepseek-v4-pro",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": null},
            "finish_reason": "content_filter"
        }]
    }))
    .unwrap();

    let resp = chat_to_responses(chat, "gpt-5.5".into());
    let j = serde_json::to_value(&resp).unwrap();

    assert_eq!(j["status"], "completed");
    assert_eq!(j["output"][0]["status"], "incomplete");
    assert_eq!(j["output"][0]["content"][0]["type"], "refusal");
    assert_eq!(j["output"][0]["content"][0]["refusal"], "content_filter");
    assert_eq!(j["incomplete_details"]["reason"], "content_filter");
}

// ── Scenario 7: Error response ───────────────────────────────────────

#[test]
fn s7_error_response() {
    let chat: ChatCompletionResponse = serde_json::from_value(json!({
        "id": "",
        "object": "error",
        "created": 0,
        "model": "",
        "choices": [],
        "error": {"message": "Invalid API key", "code": "invalid_api_key"}
    }))
    .unwrap();

    let resp = chat_to_responses(chat, "gpt-5.5".into());
    let j = serde_json::to_value(&resp).unwrap();

    assert_eq!(j["status"], "failed");
    assert!(j["output"].as_array().unwrap().is_empty());
    assert_eq!(j["error"]["code"], "invalid_api_key");
    assert_eq!(j["error"]["message"], "Invalid API key");
}

// ── Scenario 8: All finish reasons ───────────────────────────────────

#[test]
fn s8_finish_reason_all() {
    let cases = &[
        ("stop", "completed", &None),
        ("tool_calls", "completed", &None),
        ("length", "completed", &Some("max_output_tokens")),
        ("content_filter", "incomplete", &Some("content_filter")),
        ("insufficient_system_resource", "incomplete", &None),
    ];

    for (reason, exp_status, exp_details) in cases {
        let chat: ChatCompletionResponse = serde_json::from_value(json!({
            "id": "chatcmpl-test",
            "object": "chat.completion",
            "created": 1715550000u64,
            "model": "deepseek-v4-pro",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "test"},
                "finish_reason": reason
            }]
        }))
        .unwrap();

        let resp = chat_to_responses(chat, "gpt-5.5".into());
        let j = serde_json::to_value(&resp).unwrap();

        assert_eq!(
            j["output"][0]["status"].as_str().unwrap(),
            *exp_status,
            "finish_reason={reason}"
        );
        match exp_details {
            Some(expected) => {
                assert_eq!(
                    j["incomplete_details"]["reason"].as_str().unwrap(),
                    *expected,
                    "finish_reason={reason}"
                );
            }
            None => {
                assert!(
                    j.get("incomplete_details").map_or(true, |v| v.is_null()),
                    "finish_reason={reason} expected no incomplete_details, got {:?}",
                    j.get("incomplete_details")
                );
            }
        }
    }
}

// ── Scenario 9: Tool normalization (flat→nested, allowlist filter) ──

#[test]
fn s9_tool_normalization() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Hi",
        "tools": [
            {"type": "function", "name": "get_weather", "description": "Weather", "parameters": {"type": "object"}, "strict": true},
            {"type": "web_search_preview"}
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    let tools = j["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1); // web_search_preview filtered
    assert_eq!(tools[0]["type"], "function");
    assert_eq!(tools[0]["function"]["name"], "get_weather");
    assert_eq!(tools[0]["function"]["description"], "Weather");
    assert_eq!(tools[0]["function"]["strict"], true);
}

// ── Scenario 10: Response format from text.format ────────────────────

#[test]
fn s10_text_format_to_response_format() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Output JSON",
        "text": {"format": {"type": "json_object"}}
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["response_format"]["type"], "json_object");
}

#[test]
fn s10_no_text_no_response_format() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Hi"
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    assert!(j.get("response_format").is_none());
}

// ── Scenario 11: Feedback passthrough (temperature, top_p, stop) ────

#[test]
fn s11_passthrough_fields() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Hi",
        "temperature": 0.7,
        "top_p": 0.9,
        "max_output_tokens": 2048,
        "stop": ["END", "STOP"],
        "top_logprobs": 3,
        "tool_choice": "required"
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["temperature"], 0.7);
    assert_eq!(j["top_p"], 0.9);
    assert_eq!(j["max_tokens"], 2048);
    assert_eq!(j["stop"][0], "END");
    assert_eq!(j["stop"][1], "STOP");
    assert_eq!(j["top_logprobs"], 3);
    assert_eq!(j["logprobs"], true); // top_logprobs>0 → logprobs=true
    assert_eq!(j["tool_choice"], "required");
}

// ── Scenario 12: Streaming usage chunk ───────────────────────────────

#[test]
fn s12_streaming_usage_captured() {
    let mut state = streaming::StreamState::new(
        "resp_test".into(),
        "msg_test".into(),
        "deepseek-v4-pro".into(),
    );
    state.accumulated_text = "Answer".into();

    // Usage-only chunk
    let events = streaming::process_chunk(
        &mut state,
        r#"{"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15,"completion_tokens_details":{"reasoning_tokens":3}}}"#,
    );
    assert!(events.is_none());
    assert!(state.usage.is_some());
    assert_eq!(state.usage.as_ref().unwrap()["prompt_tokens"], 10);
    assert_eq!(
        state.usage.as_ref().unwrap()["completion_tokens_details"]["reasoning_tokens"],
        3
    );

    // [DONE] includes usage
    let events = streaming::process_chunk(&mut state, "[DONE]").unwrap();
    let completed = events
        .iter()
        .find(|e| matches!(e, streaming::StreamEvent::Completed(_)))
        .unwrap();
    let j = match completed {
        streaming::StreamEvent::Completed(v) => v,
        _ => panic!(),
    };
    assert_eq!(j["response"]["usage"]["prompt_tokens"], 10);
    assert_eq!(j["response"]["usage"]["total_tokens"], 15);
}

// ── Scenario 13: Streaming output_index no duplicates ────────────────

#[test]
fn s13_streaming_output_index_unique() {
    let mut state = streaming::StreamState::new(
        "resp_test".into(),
        "msg_test".into(),
        "deepseek-v4-pro".into(),
    );

    let events = streaming::process_chunk(
        &mut state,
        r#"{"choices":[{"delta":{"content":"Let me check.","tool_calls":[{"index":0,"id":"call_x","type":"function","function":{"name":"search","arguments":"{}"}}]}}]}"#,
    )
    .unwrap();

    let indices: Vec<u64> = events
        .iter()
        .filter_map(|e| match e {
            streaming::StreamEvent::OutputItemAdded(v) => Some(v["output_index"].as_u64().unwrap()),
            _ => None,
        })
        .collect();
    assert_eq!(indices.len(), 2);
    assert_ne!(indices[0], indices[1]);
    assert_eq!(indices[1], indices[0] + 1);
}

#[test]
fn s13_streaming_output_index_with_reasoning() {
    let mut state = streaming::StreamState::new(
        "resp_test".into(),
        "msg_test".into(),
        "deepseek-v4-pro".into(),
    );

    streaming::process_chunk(
        &mut state,
        r#"{"choices":[{"delta":{"reasoning_content":"Let me think"}}]}"#,
    );
    streaming::process_chunk(
        &mut state,
        r#"{"choices":[{"delta":{"content":"Answer","tool_calls":[{"index":0,"id":"call_x","type":"function","function":{"name":"search","arguments":"{}"}}]}}]}"#,
    );

    // reasoning=0, message=1, tool_call=2
    assert!(state.reasoning_item_added);
    assert!(state.message_item_added);
    assert_eq!(state.msg_output_index, 1);
    assert_eq!(state.tool_calls[0].output_index, 2);
}

// ── Scenario 14: Streaming in_progress after created ─────────────────

#[test]
fn s14_streaming_in_progress_emitted() {
    let mut state = streaming::StreamState::new(
        "resp_test".into(),
        "msg_test".into(),
        "deepseek-v4-pro".into(),
    );

    let events =
        streaming::process_chunk(&mut state, r#"{"choices":[{"delta":{"content":"Hello"}}]}"#)
            .unwrap();

    let types: Vec<&str> = events.iter().map(|e| e.event_type()).collect();
    let created = types.iter().position(|t| *t == "response.created").unwrap();
    let in_progress = types
        .iter()
        .position(|t| *t == "response.in_progress")
        .unwrap();
    assert!(created < in_progress);
}

#[test]
fn s14_streaming_in_progress_only_once() {
    let mut state = streaming::StreamState::new(
        "resp_test".into(),
        "msg_test".into(),
        "deepseek-v4-pro".into(),
    );

    streaming::process_chunk(&mut state, r#"{"choices":[{"delta":{"content":"A"}}]}"#);
    // second chunk — no duplicate in_progress
    let events =
        streaming::process_chunk(&mut state, r#"{"choices":[{"delta":{"content":"B"}}]}"#).unwrap();

    let has_in_progress = events
        .iter()
        .any(|e| matches!(e, streaming::StreamEvent::InProgress(_)));
    assert!(!has_in_progress);
}

// ── Scenario 15: Cached tokens ───────────────────────────────────────

#[test]
fn s15_cached_tokens_openai_style() {
    let chat: ChatCompletionResponse = serde_json::from_value(json!({
        "id": "chatcmpl-cache",
        "object": "chat.completion",
        "created": 1715550000u64,
        "model": "deepseek-v4-pro",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "response"},
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 30,
            "total_tokens": 130,
            "prompt_tokens_details": {"cached_tokens": 80}
        }
    }))
    .unwrap();

    let resp = chat_to_responses(chat, "gpt-5.5".into());
    let j = serde_json::to_value(&resp).unwrap();

    assert_eq!(j["usage"]["input_tokens"], 100);
    assert_eq!(j["usage"]["input_tokens_details"]["cached_tokens"], 80);
}

#[test]
fn s15_cached_tokens_deepseek_style() {
    let chat: ChatCompletionResponse = serde_json::from_value(json!({
        "id": "chatcmpl-ds",
        "object": "chat.completion",
        "created": 1715550000u64,
        "model": "deepseek-v4-pro",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "response"},
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 30,
            "total_tokens": 130,
            "prompt_cache_hit_tokens": 60,
            "prompt_cache_miss_tokens": 40
        }
    }))
    .unwrap();

    let resp = chat_to_responses(chat, "gpt-5.5".into());
    let j = serde_json::to_value(&resp).unwrap();

    // Falls back to hit+miss when prompt_tokens_details is absent
    assert_eq!(j["usage"]["input_tokens_details"]["cached_tokens"], 100);
}

// ── Scenario 16: Instructions merged with input system message ───────

#[test]
fn s16_instructions_merge_with_system() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": [
            {"type": "message", "role": "system", "content": [{"type": "input_text", "text": "You are helpful."}]},
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Hi"}]}
        ],
        "instructions": "Top-level instructions."
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    let msgs = j["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0]["role"], "system");
    let sys = msgs[0]["content"].as_str().unwrap();
    assert!(sys.starts_with("Top-level instructions."));
    assert!(sys.contains("You are helpful."));
    assert_eq!(msgs[1]["role"], "user");
}

// ── Scenario 17: Developer role maps to system ───────────────────────

#[test]
fn s17_developer_to_system() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": [
            {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "Dev rules"}]},
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Hi"}]}
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["messages"][0]["role"], "system");
    assert_eq!(j["messages"][0]["content"], "Dev rules");
}

// ── Scenario 18: Reasoning in input history ──────────────────────────

#[test]
fn s18_reasoning_item_in_input() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": [
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "What's weather?"}]},
            {"type": "reasoning", "id": "rs_1", "summary": [{"type": "summary_text", "text": "Let me check the API."}]},
            {"type": "function_call", "call_id": "call_1", "name": "get_weather", "arguments": "{\"city\":\"NYC\"}"}
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    let msgs = j["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 2); // user + assistant(tool_calls)
    assert_eq!(msgs[0]["role"], "user");
    assert_eq!(msgs[1]["role"], "assistant");
    assert_eq!(msgs[1]["tool_calls"][0]["id"], "call_1");
    // reasoning content attached to the assistant(tool_calls) message
    assert_eq!(msgs[1]["reasoning_content"], "Let me check the API.");
}

// ── Scenario 19: Multiple content blocks joined ──────────────────────

#[test]
fn s19_multiple_content_blocks_joined() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": [
            {"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": "Hello"},
                {"type": "input_text", "text": "World"}
            ]}
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["messages"][0]["content"], "Hello\nWorld");
}

// ── Scenario 20: Empty instructions ignored ──────────────────────────

#[test]
fn s20_empty_instructions_ignored() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Hi",
        "instructions": ""
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["messages"].as_array().unwrap().len(), 1);
    assert_eq!(j["messages"][0]["role"], "user");
}

// ── Scenario 21: Image/file blocks silently dropped ──────────────────

#[test]
fn s21_image_file_blocks_dropped() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": [{"type": "message", "role": "user", "content": [
            {"type": "input_text", "text": "Describe:"},
            {"type": "input_image", "image_url": "https://example.com/img.png"},
            {"type": "input_file", "file_id": "file-abc"}
        ]}]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["messages"][0]["content"], "Describe:");
}

// ── Scenario 22: Unknown item/content silently skipped ───────────────

#[test]
fn s22_unknown_items_skipped() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": [
            {"type": "item_reference", "id": "item_abc"},
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Hello"}]},
            {"type": "future_item", "data": "ignored"}
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["messages"].as_array().unwrap().len(), 1);
    assert_eq!(j["messages"][0]["content"], "Hello");
}

// ── Scenario 23: String input with instructions ──────────────────────

#[test]
fn s23_string_input_with_instructions() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "What is Rust?",
        "instructions": "You are a helpful assistant."
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    let msgs = j["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0]["role"], "system");
    assert_eq!(msgs[0]["content"], "You are a helpful assistant.");
    assert_eq!(msgs[1]["role"], "user");
    assert_eq!(msgs[1]["content"], "What is Rust?");
}

// ── Scenario 24: Array input with username ──── (tests position) ─────

#[test]
fn s24_function_call_output_array() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": [{"type": "function_call_output", "call_id": "call_1", "output": [
            {"type": "input_text", "text": "Result text here"}
        ]}]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["messages"][0]["role"], "tool");
    assert_eq!(j["messages"][0]["content"], "Result text here");
    assert_eq!(j["messages"][0]["tool_call_id"], "call_1");
}

// ── Scenario 25: Stream options passthrough ──────────────────────────

#[test]
fn s25_stream_true() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Hi",
        "stream": true
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["stream"], true);
}

// ── Scenario 26: Consecutive function calls merge ────────────────────

#[test]
fn s26_consecutive_function_calls_merge() {
    let req: ResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": [
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Get weather and time"}]},
            {"type": "function_call", "call_id": "call_1", "name": "get_weather", "arguments": "{\"city\":\"NYC\"}"},
            {"type": "function_call", "call_id": "call_2", "name": "get_time", "arguments": "{\"tz\":\"EST\"}"},
            {"type": "function_call_output", "call_id": "call_1", "output": "Sunny"},
            {"type": "function_call_output", "call_id": "call_2", "output": "3pm"}
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &["function".into()]);
    let j = serde_json::to_value(&chat).unwrap();

    let msgs = j["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 4);
    // Both function_calls merged into one assistant message
    assert_eq!(msgs[1]["role"], "assistant");
    assert!(msgs[1]["content"].is_null());
    assert_eq!(msgs[1]["tool_calls"].as_array().unwrap().len(), 2);
    // tool messages in order
    assert_eq!(msgs[2]["role"], "tool");
    assert_eq!(msgs[2]["tool_call_id"], "call_1");
    assert_eq!(msgs[3]["role"], "tool");
    assert_eq!(msgs[3]["tool_call_id"], "call_2");
}
