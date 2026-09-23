/// Verification tests: real code with realistic payloads.
/// Run with: cargo test verification
use responses_proxy::convert::{chat_to_responses, responses_to_chat};
use responses_proxy::types::streaming::{
    StreamEvent, StreamState, build_completion_events, process_chunk_value,
};
use responses_proxy::types::{chat, responses};
use serde_json::json;

fn test_state() -> responses_proxy::app::State {
    use responses_proxy::config::ResolvedConfig;
    let config = ResolvedConfig {
        listen: String::new(),
        timeout: 30,
        auth_keys: std::collections::HashSet::new(),
        cors_allow_origins: vec![],
        allowed_tool_types: vec!["function".into()],
        log_level: "info".into(),
        models: std::collections::HashMap::new(),
        model_names: vec![],
        compact_encryption_key: String::new(),
        max_body_bytes: 100 * 1024 * 1024,
    };
    responses_proxy::app::State::new(config)
}

fn test_state_with_rewrite(
    rewrite: responses_proxy::config::RewriteConfig,
) -> responses_proxy::app::State {
    use responses_proxy::config::{ResolvedConfig, ResolvedProvider, RewriteProfile};
    use std::time::Duration;

    let mut models = std::collections::HashMap::new();
    models.insert(
        "gpt-5.5".into(),
        ResolvedProvider {
            base_url: "http://example.test".into(),
            api_key: "test-key".into(),
            model: "upstream-model".into(),
            timeout: Duration::from_secs(30),
            rewrite: RewriteProfile {
                chat_out: rewrite,
                ..Default::default()
            },
            max_input_chars: None,
            max_input_messages: 1000,
            max_tools: 0,
            stream_structured_output: true,
            context_window: None,
            reasoning_levels: vec![responses_proxy::types::ReasoningEffort::Medium],
            default_reasoning_level: responses_proxy::types::ReasoningEffort::Medium,
        },
    );

    let config = ResolvedConfig {
        listen: String::new(),
        timeout: 30,
        auth_keys: std::collections::HashSet::new(),
        cors_allow_origins: vec![],
        allowed_tool_types: vec!["function".into()],
        log_level: "info".into(),
        models,
        model_names: vec!["gpt-5.5".into()],
        compact_encryption_key: String::new(),
        max_body_bytes: 100 * 1024 * 1024,
    };
    responses_proxy::app::State::new(config)
}

fn deepseek_chat_out_rewrite() -> responses_proxy::config::RewriteConfig {
    let config = responses_proxy::config::load_config("deepseek.config.yaml").unwrap();
    config.models["gpt-5.5"].rewrite.chat_out.clone()
}

// ── Scenario 1: Simple text, no streaming ────────────────────────────

#[tokio::test]
async fn s1_simple_text_request() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "What is 2+2? Reply with just the number."
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
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

#[tokio::test]
async fn s1_simple_text_response() {
    let chat: chat::Completion = serde_json::from_value(json!({
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

    let resp = chat_to_responses(chat, "gpt-5.5".into(), None);
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

#[tokio::test]
async fn s2_instructions_and_reasoning_request() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Solve the complex equation.",
        "instructions": "You are a math tutor. Always show your work.",
        "reasoning": {"effort": "xhigh"}
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();
    let msgs = j["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0]["role"], "system");
    assert_eq!(
        msgs[0]["content"],
        "You are a math tutor. Always show your work."
    );
    assert_eq!(msgs[1]["role"], "user");

    assert_eq!(j["reasoning_effort"], "xhigh");
}

#[tokio::test]
async fn s2_reasoning_content_response() {
    let chat: chat::Completion = serde_json::from_value(json!({
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

    let resp = chat_to_responses(chat, "gpt-5.5".into(), None);
    let j = serde_json::to_value(&resp).unwrap();

    let output = j["output"].as_array().unwrap();
    assert_eq!(output.len(), 2);
    // reasoning item first
    assert_eq!(output[0]["type"], "reasoning");
    assert_eq!(output[0]["content"][0]["type"], "reasoning_text");
    assert_eq!(output[0]["content"][0]["text"], "First, we isolate x by...");
    // message second
    assert_eq!(output[1]["type"], "message");
    assert_eq!(output[1]["content"][0]["text"], "x = 5");
    // usage
    assert_eq!(j["usage"]["output_tokens_details"]["reasoning_tokens"], 30);
}

// ── Scenario 3: All reasoning effort levels ──────────────────────────

#[tokio::test]
async fn s3_reasoning_effort_all_levels() {
    let cases = &[
        ("none", true, Some("none")),
        ("minimal", true, Some("minimal")),
        ("low", true, Some("low")),
        ("medium", true, Some("medium")),
        ("high", true, Some("high")),
        ("xhigh", true, Some("xhigh")),
        // gpt-5.6-class tiers above xhigh — passed through verbatim.
        ("max", true, Some("max")),
        ("ultra", true, Some("ultra")),
    ];

    for (effort, expect_think, expect_re) in cases {
        let req: responses::Request = serde_json::from_value(json!({
            "model": "gpt-5.5",
            "input": "Hi",
            "reasoning": {"effort": effort}
        }))
        .unwrap();

        let chat = responses_to_chat(req, &test_state()).await.unwrap();
        let j = serde_json::to_value(&chat).unwrap();

        if *expect_think {
            assert_eq!(j["reasoning_effort"], expect_re.unwrap(), "effort={effort}");
        } else {
            assert!(
                j.get("reasoning_effort").is_none(),
                "effort={effort} should have no reasoning_effort"
            );
        }
    }
}

// ── Codex gpt-5.6 additional_tools (code-mode) → Chat function tools ─
#[tokio::test]
async fn additional_tools_flattened_to_chat_functions() {
    // gpt-5.6 delivers its tools inside an `additional_tools` input item using
    // the code-mode custom/namespace protocol. Chat Completions can't take
    // those, so every entry must be flattened into a plain `function` tool.
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.6-sol",
        "input": [
            {
                "type": "additional_tools",
                "role": "developer",
                "tools": [
                    {"type": "custom", "name": "exec", "description": "Run JS"},
                    {"type": "namespace", "name": "shell", "tools": [
                        {"type": "function", "name": "exec_command", "description": "run",
                         "parameters": {"type": "object",
                                        "properties": {"cmd": {"type": "string"}},
                                        "required": ["cmd"]}},
                        {"type": "custom", "name": "apply_patch", "description": "patch"}
                    ]}
                ]
            }
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();
    let tools = j["tools"].as_array().expect("tools present");

    // namespace expands in place; names preserved verbatim for Codex routing.
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["exec", "exec_command", "apply_patch"]);

    // Every hoisted tool is function-typed (nothing left as custom/namespace).
    for t in tools {
        assert_eq!(t["type"], "function");
    }
    // A real function keeps its own schema.
    assert_eq!(
        j["tools"][1]["function"]["parameters"]["properties"]["cmd"]["type"],
        "string"
    );
    // Freeform custom tools collapse to a single string `input` argument.
    assert_eq!(
        j["tools"][0]["function"]["parameters"]["properties"]["input"]["type"],
        "string"
    );
    assert_eq!(
        j["tools"][2]["function"]["parameters"]["properties"]["input"]["type"],
        "string"
    );
}

#[tokio::test]
async fn mcp_namespace_tools_flattened_to_chat_functions() {
    // gpt-5.6 wraps MCP server tools in a proprietary `namespace` entry (member
    // names like `mcp__server__tool`). Chat Completions can't unwrap that, so
    // the members must be flattened to plain function tools with names intact.
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.6-sol",
        "input": [
            {
                "type": "additional_tools",
                "role": "developer",
                "tools": [
                    {"type": "namespace", "name": "mcp", "tools": [
                        {"type": "function", "name": "mcp__github__list_issues",
                         "description": "List issues",
                         "parameters": {"type": "object",
                                        "properties": {"repo": {"type": "string"}},
                                        "required": ["repo"]}}
                    ]},
                    // A remote MCP server reference carries no per-tool schema
                    // and must be dropped, not crash the request.
                    {"type": "mcp", "server_label": "github",
                     "server_url": "https://example.invalid/mcp"}
                ]
            }
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();
    let tools = j["tools"].as_array().expect("tools present");

    assert_eq!(
        tools.len(),
        1,
        "namespace member kept, mcp server ref dropped"
    );
    assert_eq!(tools[0]["type"], "function");
    assert_eq!(tools[0]["function"]["name"], "mcp__github__list_issues");
    assert_eq!(
        tools[0]["function"]["parameters"]["properties"]["repo"]["type"],
        "string"
    );
}

#[tokio::test]
async fn multi_agent_collaboration_tools_pass_through() {
    // gpt-5.6 multi-agent mode delivers the collaboration actions
    // (spawn_agent, …) inside a `collaboration` namespace. Codex CLI has
    // client-side handlers for these that spawn a local sub-agent thread and
    // drive it with its own separate upstream calls, so the proxy passes them
    // through like any other namespace member — same as the plain
    // client-executable tools (exec/wait/request_user_input).
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.6-sol",
        "input": [
            {
                "type": "additional_tools",
                "role": "developer",
                "tools": [
                    {"type": "custom", "name": "exec", "description": "Run JS"},
                    {"type": "function", "name": "wait", "description": "wait",
                     "parameters": {"type": "object", "properties": {}}},
                    {"type": "function", "name": "request_user_input",
                     "description": "ask",
                     "parameters": {"type": "object", "properties": {}}},
                    {"type": "namespace", "name": "collaboration",
                     "description": "Tools for spawning and managing sub-agents.",
                     "tools": [
                        {"type": "function", "name": "spawn_agent",
                         "parameters": {"type": "object", "properties": {
                            "model": {"type": "string"}
                         }}},
                        {"type": "function", "name": "followup_task",
                         "parameters": {"type": "object", "properties": {}}},
                        {"type": "function", "name": "interrupt_agent",
                         "parameters": {"type": "object", "properties": {}}},
                        {"type": "function", "name": "list_agents",
                         "parameters": {"type": "object", "properties": {}}},
                        {"type": "function", "name": "send_message",
                         "parameters": {"type": "object", "properties": {}}},
                        {"type": "function", "name": "wait_agent",
                         "parameters": {"type": "object", "properties": {}}}
                    ]}
                ]
            }
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();
    let tools = j["tools"].as_array().expect("tools present");
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec![
            "exec",
            "wait",
            "request_user_input",
            "spawn_agent",
            "followup_task",
            "interrupt_agent",
            "list_agents",
            "send_message",
            "wait_agent",
        ]
    );
    let spawn_agent = tools
        .iter()
        .find(|t| t["function"]["name"] == "spawn_agent")
        .expect("spawn_agent present");
    assert_eq!(
        spawn_agent["function"]["parameters"]["properties"]["model"]["type"], "string",
        "schema must survive verbatim, not just the name"
    );
}

// Regression: `spawn_agent`/`send_message`/`followup_task` tag their `message`
// parameter with a non-standard `encrypted` JSON Schema keyword. The upstream
// backend's schema validator doesn't tolerate unknown keywords there — it
// doesn't return a clean 400, it fails the whole generation and reports a
// misleading `provider_model_down` error (confirmed live: stripping this key
// from the request is the only change needed to make the exact same request
// succeed). It must be dropped before the schema reaches Chat Completions.
#[tokio::test]
async fn multi_agent_encrypted_schema_keyword_stripped() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.6-sol",
        "input": [
            {
                "type": "additional_tools",
                "role": "developer",
                "tools": [
                    {"type": "namespace", "name": "collaboration",
                     "description": "Tools for spawning and managing sub-agents.",
                     "tools": [
                        {"type": "function", "name": "spawn_agent",
                         "parameters": {"type": "object", "properties": {
                            "message": {"type": "string", "encrypted": true},
                            "task_name": {"type": "string"}
                         }, "required": ["task_name", "message"]}}
                    ]}
                ]
            }
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();
    let tools = j["tools"].as_array().expect("tools present");
    let spawn_agent = tools
        .iter()
        .find(|t| t["function"]["name"] == "spawn_agent")
        .expect("spawn_agent present");
    let message_schema = &spawn_agent["function"]["parameters"]["properties"]["message"];
    assert!(
        message_schema.get("encrypted").is_none(),
        "the non-standard `encrypted` keyword must be stripped: {message_schema:?}"
    );
    assert_eq!(
        message_schema["type"], "string",
        "the rest of the schema must survive untouched"
    );
}

// Regression: multi-agent mode delivers a sub-agent's task assignment (and
// inter-agent messages generally) as an `agent_message` input item — not a
// plain `message` — so it can carry `author`/`recipient` agent-path routing.
// The converter had no arm for this type, so it fell into the generic
// unconvertible-item warn-and-skip path: the sub-agent's Chat Completions
// request ended up with no task content at all, and the model had nothing to
// do (observed live: the sub-agent immediately reported "done" without acting
// on the task). It must convert to a real chat message instead of vanishing.
#[tokio::test]
async fn multi_agent_agent_message_item_converts_to_user_message() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.6-sol",
        "input": [
            {
                "type": "agent_message",
                "author": "root",
                "recipient": "m2c_reddy_surface",
                "content": [
                    {"type": "input_text", "text": "Message Type: NEW_TASK\nPayload:\nImplement the reddy surface renderer."}
                ]
            }
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();
    let messages = j["messages"].as_array().expect("messages present");
    let delivered = messages
        .iter()
        .find(|m| {
            m["content"]
                .as_str()
                .is_some_and(|c| c.contains("Implement the reddy surface renderer."))
        })
        .expect("the agent_message task text must reach the model, not be dropped");
    assert_eq!(delivered["role"], "user");
}

// An agent_message content block Codex sends in a shape this proxy doesn't
// (yet) recognize must not blow up the whole request — it should fall into
// AgentMessageContent::Unknown and be skipped, while the rest of the message
// still gets through, and the WS ClientEvent envelope still deserializes.
#[tokio::test]
async fn multi_agent_agent_message_unknown_content_block_does_not_reject_request() {
    let event: responses_proxy::types::websocket::ClientEvent = serde_json::from_value(json!({
        "type": "response.create",
        "model": "gpt-5.6-sol",
        "input": [
            {
                "type": "agent_message",
                "author": "root",
                "recipient": "m2c_reddy_surface",
                "content": [
                    {"type": "some_future_block", "foo": "bar"},
                    {"type": "input_text", "text": "Implement the reddy surface renderer."}
                ]
            }
        ]
    }))
    .expect("an unrecognized content-block shape must not reject the whole ClientEvent");

    let req = match event {
        responses_proxy::types::websocket::ClientEvent::ResponseCreate(req) => req,
        _ => panic!("expected response.create"),
    };
    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();
    let messages = j["messages"].as_array().expect("messages present");
    assert!(
        messages.iter().any(|m| m["content"]
            .as_str()
            .is_some_and(|c| c.contains("Implement the reddy surface renderer."))),
        "the readable block must still reach the model despite the unrecognized sibling block"
    );
}

// When every content block is empty, unrecognized, or fails to decrypt, no
// message should be pushed at all (silent drop is intentional — the model
// gets nothing to act on either way — but it must not panic or inject a
// stray empty message).
#[tokio::test]
async fn multi_agent_agent_message_all_unreadable_produces_no_message() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.6-sol",
        "input": [
            {
                "type": "agent_message",
                "author": "root",
                "recipient": "m2c_reddy_surface",
                "content": [
                    {"type": "some_future_block", "foo": "bar"},
                    {"type": "encrypted_content", "encrypted_content": "not-valid-ciphertext"}
                ]
            }
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();
    let messages = j["messages"].as_array().expect("messages present");
    assert!(
        messages.is_empty(),
        "no readable content should push a message: {messages:?}"
    );
}

#[tokio::test]
async fn multi_agent_hosted_action_passes_through_as_plain_function() {
    // A multi-agent action arriving as a bare top-level tool, or inside a
    // differently-named namespace, passes through like any other function.
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.6-sol",
        "input": [
            {
                "type": "additional_tools",
                "role": "developer",
                "tools": [
                    {"type": "function", "name": "spawn_agent",
                     "parameters": {"type": "object", "properties": {}}},
                    {"type": "namespace", "name": "misc", "tools": [
                        {"type": "function", "name": "list_agents",
                         "parameters": {"type": "object", "properties": {}}},
                        {"type": "function", "name": "keep_me",
                         "parameters": {"type": "object", "properties": {}}}
                    ]}
                ]
            }
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();
    let names: Vec<&str> = j["tools"]
        .as_array()
        .expect("tools present")
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["spawn_agent", "list_agents", "keep_me"]);
}

#[tokio::test]
async fn hosted_only_tool_types_still_dropped() {
    // Regression: genuinely hosted/Responses-only tool types (no Chat
    // Completions equivalent) must still be dropped — this change only
    // affects the six named multi-agent actions, not the real catch-all.
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.6-sol",
        "input": "hi",
        "tools": [
            {"type": "file_search", "vector_store_ids": ["vs_1"]},
            {"type": "web_search"},
            {"type": "computer_use_preview", "display_width": 1024,
             "display_height": 768, "environment": "browser"}
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();
    assert!(
        j["tools"].as_array().map(|a| a.is_empty()).unwrap_or(true),
        "hosted-only tool types must still produce zero Chat tools"
    );
}

#[tokio::test]
async fn multi_agent_action_declared_custom_round_trips_as_custom_tool_call() {
    // If a multi-agent action is ever declared as a Custom tool rather than
    // Function, its `function_call` must still be remapped back to
    // `custom_tool_call` on the response side — proving the
    // `custom_function_names` fix (the fourth call site) actually wires
    // through end to end, not just that the tool list contains the name.
    use responses_proxy::types::item::InputItem;

    let input: Vec<InputItem> = serde_json::from_value(json!([
        {
            "type": "additional_tools",
            "role": "developer",
            "tools": [
                {"type": "custom", "name": "send_message",
                 "description": "Send a message to an existing agent."}
            ]
        }
    ]))
    .unwrap();

    let custom = responses_proxy::convert::custom_tool_names(&input, None, false);
    assert!(
        custom.contains("send_message"),
        "send_message must be tracked as a custom-shaped tool"
    );

    let chat: chat::Completion = serde_json::from_value(json!({
        "id": "c", "object": "chat.completion", "created": 1u64, "model": "m",
        "choices": [{"index": 0, "finish_reason": "tool_calls", "message": {
            "role": "assistant",
            "tool_calls": [
                {"id": "call_1", "type": "function",
                 "function": {"name": "send_message", "arguments": "{\"input\":\"hi\"}"}}
            ]
        }}]
    }))
    .unwrap();
    let mut resp = chat_to_responses(chat, "gpt-5.6-sol".into(), None);
    responses_proxy::convert::remap_custom_tool_calls(&mut resp, &custom);
    let j = serde_json::to_value(&resp).unwrap();
    let out = j["output"].as_array().unwrap();
    let send_message = out.iter().find(|i| i["name"] == "send_message").unwrap();
    assert_eq!(send_message["type"], "custom_tool_call");
}

#[tokio::test]
async fn spawn_agent_namespace_tag_restored_on_function_call() {
    // Chat Completions carries no `namespace` field, so `chat_to_responses`
    // always emits `namespace: None` for a tool call. `apply_tool_namespaces`
    // must restore the tag from the request's `namespace` bundle before
    // Codex's `(name, namespace)`-keyed registry sees it — otherwise it
    // rejects the call as `"unsupported call: spawn_agent"`.
    use responses_proxy::types::item::InputItem;

    let input: Vec<InputItem> = serde_json::from_value(json!([
        {
            "type": "additional_tools",
            "role": "developer",
            "tools": [
                {"type": "namespace", "name": "collaboration",
                 "description": "Tools for spawning and managing sub-agents.",
                 "tools": [
                    {"type": "function", "name": "spawn_agent",
                     "parameters": {"type": "object", "properties": {}}}
                 ]}
            ]
        }
    ]))
    .unwrap();

    let namespaces = responses_proxy::convert::tool_namespaces(&input, None);
    assert_eq!(
        namespaces.get("spawn_agent").map(String::as_str),
        Some("collaboration")
    );

    let chat: chat::Completion = serde_json::from_value(json!({
        "id": "c", "object": "chat.completion", "created": 1u64, "model": "m",
        "choices": [{"index": 0, "finish_reason": "tool_calls", "message": {
            "role": "assistant",
            "tool_calls": [
                {"id": "call_1", "type": "function",
                 "function": {"name": "spawn_agent", "arguments": "{}"}}
            ]
        }}]
    }))
    .unwrap();
    let mut resp = chat_to_responses(chat, "gpt-5.6-sol".into(), None);
    let before = serde_json::to_value(&resp).unwrap();
    assert_eq!(
        before["output"][0]["namespace"],
        serde_json::Value::Null,
        "chat_to_responses never invents a namespace on its own"
    );

    responses_proxy::convert::apply_tool_namespaces(&mut resp, &namespaces);
    let j = serde_json::to_value(&resp).unwrap();
    assert_eq!(j["output"][0]["namespace"], "collaboration");
}

#[tokio::test]
async fn apply_tool_namespaces_is_noop_when_empty() {
    // Zero regression when the turn declared no `namespace` tools — e.g.
    // Codex's own multi-agent feature flags are off. `tool_namespaces` is then
    // empty and every function_call keeps the `namespace: None` it already had.
    let chat: chat::Completion = serde_json::from_value(json!({
        "id": "c", "object": "chat.completion", "created": 1u64, "model": "m",
        "choices": [{"index": 0, "finish_reason": "tool_calls", "message": {
            "role": "assistant",
            "tool_calls": [
                {"id": "call_1", "type": "function",
                 "function": {"name": "read_file", "arguments": "{}"}}
            ]
        }}]
    }))
    .unwrap();
    let mut resp = chat_to_responses(chat, "gpt-5.6-sol".into(), None);
    responses_proxy::convert::apply_tool_namespaces(&mut resp, &std::collections::HashMap::new());
    let j = serde_json::to_value(&resp).unwrap();
    assert_eq!(j["output"][0]["namespace"], serde_json::Value::Null);
}

#[tokio::test]
async fn tool_namespaces_restored_on_continuation_turn() {
    // Mirrors `code_mode_tools_restored_on_continuation_turn`: the namespace
    // map is cached under the response id and restored on a tool-result
    // continuation that omits `additional_tools`.
    use responses_proxy::types::item::InputItem;
    let state = test_state();

    let fresh: Vec<InputItem> = serde_json::from_value(json!([
        {"type": "additional_tools", "role": "developer", "tools": [
            {"type": "namespace", "name": "collaboration", "tools": [
                {"type": "function", "name": "spawn_agent",
                 "parameters": {"type": "object", "properties": {}}}
            ]}
        ]}
    ]))
    .unwrap();
    let namespaces =
        responses_proxy::convert::resolve_tool_namespaces(&state, &fresh, None, None).await;
    assert_eq!(
        namespaces.get("spawn_agent").map(String::as_str),
        Some("collaboration")
    );
    state
        .store()
        .put_tools(
            "resp_ns1".to_string(),
            vec![],
            std::collections::HashSet::new(),
            namespaces,
        )
        .await;

    let cont: Vec<InputItem> = serde_json::from_value(json!([
        {"type": "function_call_output", "call_id": "c1", "output": "ok"}
    ]))
    .unwrap();
    let restored =
        responses_proxy::convert::resolve_tool_namespaces(&state, &cont, None, Some("resp_ns1"))
            .await;
    assert_eq!(
        restored.get("spawn_agent").map(String::as_str),
        Some("collaboration")
    );

    // Unknown previous id → no invention, empty map (no regression).
    let none = responses_proxy::convert::resolve_tool_namespaces(
        &state,
        &cont,
        None,
        Some("resp_unknown"),
    )
    .await;
    assert!(none.is_empty());
}

#[tokio::test]
async fn code_mode_tools_restored_on_continuation_turn() {
    // Codex delivers `additional_tools` only on a new user turn; a tool-result
    // continuation references `previous_response_id` and omits them. The proxy
    // caches the derived registry under the response id and restores it on the
    // continuation so the model isn't left with an empty tool set (which made it
    // stall after a single call).
    let state = test_state();

    // Turn 1: additional_tools present → tools built and cached under a rid.
    let turn1: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.6-sol",
        "input": [
            {"type": "additional_tools", "role": "developer", "tools": [
                {"type": "custom", "name": "exec", "description": "Run JS"},
                {"type": "function", "name": "wait",
                 "parameters": {"type": "object", "properties": {}}}
            ]}
        ]
    }))
    .unwrap();
    let turn1_input = turn1.input.clone();
    let chat1 = responses_to_chat(turn1, &state).await.unwrap();
    let tools1 = chat1.tools.clone().expect("turn 1 has tools");
    assert_eq!(tools1.len(), 2);
    // Cache exactly as the handler does: tools + the custom-name set.
    let custom_names1 = responses_proxy::convert::custom_tool_names(&turn1_input, None, false);
    assert!(custom_names1.contains("exec"));
    state
        .store()
        .put_tools(
            "resp_turn1".to_string(),
            tools1,
            custom_names1,
            std::collections::HashMap::new(),
        )
        .await;

    // Turn 2: continuation — no additional_tools, references the prior response.
    let turn2: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.6-sol",
        "previous_response_id": "resp_turn1",
        "input": [
            {"type": "function_call_output", "call_id": "call_1", "output": "ok"}
        ]
    }))
    .unwrap();
    let chat2 = responses_to_chat(turn2, &state).await.unwrap();
    let j = serde_json::to_value(&chat2).unwrap();
    let names: Vec<&str> = j["tools"]
        .as_array()
        .expect("continuation restored tools")
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["exec", "wait"]);

    // The custom-name set is restorable too, so the continuation's `exec`
    // response is re-emitted as a custom_tool_call rather than a function_call
    // (which Codex would cancel).
    let restored = state
        .store()
        .get_custom_names("resp_turn1")
        .await
        .expect("custom names cached");
    assert!(restored.contains("exec"));
}

#[tokio::test]
async fn resolve_custom_tool_names_falls_back_to_cached() {
    use responses_proxy::types::item::InputItem;
    let state = test_state();

    // Fresh turn with additional_tools resolves directly and is cached.
    let fresh: Vec<InputItem> = serde_json::from_value(json!([
        {"type": "additional_tools", "role": "developer", "tools": [
            {"type": "custom", "name": "exec"}
        ]}
    ]))
    .unwrap();
    let names =
        responses_proxy::convert::resolve_custom_tool_names(&state, &fresh, None, None).await;
    assert!(names.contains("exec"));
    state
        .store()
        .put_tools(
            "resp_a".to_string(),
            vec![],
            names,
            std::collections::HashMap::new(),
        )
        .await;

    // Continuation (no additional_tools) falls back to the cached set.
    let cont: Vec<InputItem> = serde_json::from_value(json!([
        {"type": "function_call_output", "call_id": "c1", "output": "ok"}
    ]))
    .unwrap();
    let restored =
        responses_proxy::convert::resolve_custom_tool_names(&state, &cont, None, Some("resp_a"))
            .await;
    assert!(restored.contains("exec"));

    // Unknown previous id → no invention.
    let none =
        responses_proxy::convert::resolve_custom_tool_names(&state, &cont, None, Some("resp_x"))
            .await;
    assert!(none.is_empty());
}

#[tokio::test]
async fn continuation_without_cached_tools_stays_toolless() {
    // No regression: a continuation that never had code-mode tools (e.g. models
    // below 5.6) resolves to no tools rather than inventing any.
    let state = test_state();
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.6-sol",
        "previous_response_id": "resp_unknown",
        "input": [
            {"type": "function_call_output", "call_id": "call_1", "output": "ok"}
        ]
    }))
    .unwrap();
    let chat = responses_to_chat(req, &state).await.unwrap();
    assert!(chat.tools.is_none());
}

// ── Codex gpt-5.6 code-mode: custom ↔ function round-trip ────────────
#[tokio::test]
async fn custom_tool_names_extracted_from_additional_tools() {
    use responses_proxy::types::item::InputItem;
    let input: Vec<InputItem> = serde_json::from_value(json!([
        {"type": "additional_tools", "role": "developer", "tools": [
            {"type": "custom", "name": "exec"},
            {"type": "namespace", "name": "mcp", "tools": [
                {"type": "function", "name": "mcp__gh__list", "parameters": {"type": "object"}},
                {"type": "custom", "name": "collab"}
            ]}
        ]}
    ]))
    .unwrap();
    let names = responses_proxy::convert::custom_tool_names(&input, None, false);
    assert!(names.contains("exec"));
    assert!(names.contains("collab"));
    // namespace *function* members stay function → not in the custom set.
    assert!(!names.contains("mcp__gh__list"));
}

// ── Codex code-mode: top-level tools convert generically ─────────────
#[tokio::test]
async fn top_level_code_mode_tools_converted_and_dropped() {
    // Codex can deliver its code-mode tools as top-level `tools` (not only via
    // `additional_tools`): a plain function, a freeform `custom` apply_patch, a
    // `namespace` of MCP functions, and hosted tools with no Chat equivalent.
    // With `allowed-tool-types: [function]` (the default), every convertible tool
    // must collapse to a `function` and hosted ones must be dropped.
    let state = test_state();
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.6-sol",
        "tools": [
            {"type": "function", "name": "exec_command",
             "parameters": {"type": "object", "properties": {}}},
            {"type": "custom", "name": "apply_patch",
             "format": {"type": "grammar", "syntax": "lark", "definition": "start: /.+/"}},
            {"type": "namespace", "name": "mcp", "tools": [
                {"type": "function", "name": "mcp__gh__list", "parameters": {"type": "object"}}
            ]},
            {"type": "web_search"}
        ],
        "input": [
            {"type": "message", "role": "user",
             "content": [{"type": "input_text", "text": "hi"}]}
        ]
    }))
    .unwrap();
    let req_tools = req.tools.clone();
    let chat = responses_to_chat(req, &state).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();
    let names: Vec<&str> = j["tools"]
        .as_array()
        .expect("tools present")
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap())
        .collect();
    // apply_patch (freeform custom) and the MCP namespace function survive as
    // functions; web_search (hosted) is dropped.
    assert_eq!(names, vec!["exec_command", "apply_patch", "mcp__gh__list"]);

    // The freeform apply_patch must round-trip: its name is recorded so the
    // model's function_call is re-emitted as a custom_tool_call.
    let custom = responses_proxy::convert::custom_tool_names(&[], req_tools.as_deref(), false);
    assert!(custom.contains("apply_patch"));
    assert!(!custom.contains("exec_command"));
    assert!(!custom.contains("mcp__gh__list"));
}

fn make_tool(name: &str) -> chat::ToolRequest {
    chat::ToolRequest::Function {
        function: chat::FunctionTool {
            name: name.to_string(),
            description: None,
            parameters: None,
            strict: None,
        },
    }
}

#[test]
fn enforce_tool_budget_caps_and_reports_overflow() {
    let mut tools: Vec<chat::ToolRequest> =
        (0..10).map(|i| make_tool(&format!("tool_{i}"))).collect();
    let namespaces = std::collections::HashMap::new();

    // 0 disables the cap.
    assert!(responses_proxy::convert::enforce_tool_budget(&mut tools, 0, &namespaces).is_empty());
    assert_eq!(tools.len(), 10);

    // No namespace info at all → degrades to plain tail-cut, unchanged from before.
    let dropped = responses_proxy::convert::enforce_tool_budget(&mut tools, 4, &namespaces);
    assert_eq!(tools.len(), 4);
    assert_eq!(
        dropped,
        vec!["tool_4", "tool_5", "tool_6", "tool_7", "tool_8", "tool_9"]
    );

    // Already fits → no-op.
    assert!(responses_proxy::convert::enforce_tool_budget(&mut tools, 4, &namespaces).is_empty());
    assert_eq!(tools.len(), 4);
}

#[test]
fn enforce_tool_budget_prefers_codex_owned_tools_over_namespaced_ones() {
    // Order deliberately interleaves bare/Codex-owned and MCP-ish tools so a
    // plain tail-cut would drop some of the bare ones — this test proves the
    // namespace-aware priority overrides that positional behavior.
    let mut tools: Vec<chat::ToolRequest> = vec![
        make_tool("mcp_tool_1"),
        make_tool("exec"),
        make_tool("mcp_tool_2"),
        make_tool("spawn_agent"),
        make_tool("weird_namespace_tool"),
        make_tool("wait"),
    ];
    let mut namespaces = std::collections::HashMap::new();
    namespaces.insert("mcp_tool_1".to_string(), "mcp".to_string());
    namespaces.insert("mcp_tool_2".to_string(), "mcp".to_string());
    namespaces.insert("spawn_agent".to_string(), "collaboration".to_string());
    namespaces.insert(
        "weird_namespace_tool".to_string(),
        "some_unknown_app".to_string(),
    );
    // "exec" and "wait" have no entry at all — bare Codex core tools.

    let dropped = responses_proxy::convert::enforce_tool_budget(&mut tools, 3, &namespaces);

    // All three prunable (namespaced, non-Codex-owned) tools are dropped, in
    // their original relative order, regardless of where they sat in the list.
    assert_eq!(
        dropped,
        vec!["mcp_tool_1", "mcp_tool_2", "weird_namespace_tool"]
    );
    // Survivors are exactly the bare + "collaboration" tools, in original order.
    let kept: Vec<String> = tools.iter().map(chat_tool_name_for_test).collect();
    assert_eq!(kept, vec!["exec", "spawn_agent", "wait"]);
}

#[test]
fn enforce_tool_budget_falls_back_to_tail_cut_when_protected_set_overflows() {
    // Every tool here is protected (bare or a Codex-owned namespace) — there's
    // nothing prunable to sacrifice, so this must behave exactly like the old
    // plain tail-cut instead of refusing to drop anything.
    let mut tools: Vec<chat::ToolRequest> = vec![
        make_tool("exec"),
        make_tool("wait"),
        make_tool("spawn_agent"),
        make_tool("send_message"),
    ];
    let mut namespaces = std::collections::HashMap::new();
    namespaces.insert("spawn_agent".to_string(), "collaboration".to_string());
    namespaces.insert("send_message".to_string(), "collaboration".to_string());

    let dropped = responses_proxy::convert::enforce_tool_budget(&mut tools, 2, &namespaces);

    assert_eq!(dropped, vec!["spawn_agent", "send_message"]);
    let kept: Vec<String> = tools.iter().map(chat_tool_name_for_test).collect();
    assert_eq!(kept, vec!["exec", "wait"]);
}

fn chat_tool_name_for_test(t: &chat::ToolRequest) -> String {
    match t {
        chat::ToolRequest::Function { function } => function.name.clone(),
        chat::ToolRequest::Custom { custom } => custom.name.clone(),
    }
}

#[tokio::test]
async fn nonstreaming_function_call_remapped_to_custom_by_name() {
    let chat: chat::Completion = serde_json::from_value(json!({
        "id": "c", "object": "chat.completion", "created": 1u64, "model": "m",
        "choices": [{"index": 0, "finish_reason": "tool_calls", "message": {
            "role": "assistant",
            "tool_calls": [
                {"id": "call_1", "type": "function",
                 "function": {"name": "exec", "arguments": "{\"input\":\"pwd\"}"}},
                {"id": "call_2", "type": "function",
                 "function": {"name": "update_plan", "arguments": "{\"x\":1}"}}
            ]
        }}]
    }))
    .unwrap();
    let mut resp = chat_to_responses(chat, "gpt-5.6-sol".into(), None);
    let custom: std::collections::HashSet<String> = ["exec".to_string()].into_iter().collect();
    responses_proxy::convert::remap_custom_tool_calls(&mut resp, &custom);

    let j = serde_json::to_value(&resp).unwrap();
    let out = j["output"].as_array().unwrap();
    let exec = out.iter().find(|i| i["name"] == "exec").unwrap();
    assert_eq!(exec["type"], "custom_tool_call");
    assert_eq!(exec["input"], "pwd", "the {{input}} wrapper is unwrapped");
    let plan = out.iter().find(|i| i["name"] == "update_plan").unwrap();
    assert_eq!(
        plan["type"], "function_call",
        "non-custom names stay function"
    );
}

#[tokio::test]
async fn nonstreaming_remap_is_noop_without_custom_names() {
    // Regression guard for models below 5.6: empty set leaves function calls intact.
    let chat: chat::Completion = serde_json::from_value(json!({
        "id": "c", "object": "chat.completion", "created": 1u64, "model": "m",
        "choices": [{"index": 0, "finish_reason": "tool_calls", "message": {
            "role": "assistant",
            "tool_calls": [{"id": "call_1", "type": "function",
                            "function": {"name": "exec", "arguments": "{}"}}]
        }}]
    }))
    .unwrap();
    let mut resp = chat_to_responses(chat, "gpt-5.5".into(), None);
    responses_proxy::convert::remap_custom_tool_calls(&mut resp, &std::collections::HashSet::new());
    let j = serde_json::to_value(&resp).unwrap();
    assert_eq!(j["output"][0]["type"], "function_call");
}

#[tokio::test]
async fn streaming_custom_tool_emits_custom_tool_call_events() {
    let mut state = StreamState::new("resp".into(), "msg".into(), "gpt-5.6-sol".into());
    state.custom_tool_names.insert("exec".to_string());

    let mut events = process_chunk_value(
        &mut state,
        serde_json::from_str(r#"{"id":"c1","object":"chat.completion.chunk","created":1,"model":"t","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_x","type":"function","function":{"name":"exec","arguments":"{\"input\":\"pwd\"}"}}]}}]}"#).unwrap(),
    )
    .unwrap_or_default();
    events.extend(build_completion_events(&mut state));

    let types: Vec<String> = events
        .iter()
        .map(|e| {
            serde_json::to_value(e).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    // Custom tool call, not a function call, on the wire.
    assert!(
        types
            .iter()
            .any(|t| t == "response.custom_tool_call_input.done")
    );
    assert!(
        !types
            .iter()
            .any(|t| t == "response.function_call_arguments.delta"),
        "function arg deltas must be suppressed for custom tools"
    );

    let jsons: Vec<serde_json::Value> = events
        .iter()
        .map(|e| serde_json::to_value(e).unwrap())
        .collect();
    let added = jsons
        .iter()
        .find(|e| e["type"] == "response.output_item.added")
        .unwrap();
    assert_eq!(added["item"]["type"], "custom_tool_call");
    let done = jsons
        .iter()
        .find(|e| e["type"] == "response.output_item.done")
        .unwrap();
    assert_eq!(done["item"]["type"], "custom_tool_call");
    assert_eq!(done["item"]["input"], "pwd");
    // completed_items keeps the function shape for upstream replay.
    assert!(matches!(
        state.completed_items.first(),
        Some(responses_proxy::types::item::OutputItem::FunctionCall(_))
    ));
}

#[tokio::test]
async fn history_custom_tool_call_wrapped_as_input_json() {
    // A replayed code-mode call must use the `{ input }` function-arg shape.
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.6-sol",
        "input": [
            {"type": "additional_tools", "role": "developer",
             "tools": [{"type": "custom", "name": "exec"}]},
            {"type": "custom_tool_call", "call_id": "call_1", "name": "exec",
             "input": "tools.exec_command({cmd:[\"pwd\"]})"},
            {"type": "custom_tool_call_output", "call_id": "call_1", "output": "ok"}
        ]
    }))
    .unwrap();
    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();
    let msgs = j["messages"].as_array().unwrap();
    let assistant = msgs.iter().find(|m| m["role"] == "assistant").unwrap();
    let args = assistant["tool_calls"][0]["function"]["arguments"]
        .as_str()
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(args).unwrap();
    assert_eq!(parsed["input"], "tools.exec_command({cmd:[\"pwd\"]})");
}

#[tokio::test]
async fn s3_reasoning_summary_without_effort_does_not_force_effort() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Hi",
        "reasoning": {"summary": "auto"}
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();

    assert!(j.get("reasoning_effort").is_none());
}

// ── Scenario 4: Thinking mode with reasoning ────────────────────────

#[tokio::test]
async fn s4_thinking_disables_logprobs() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Hi",
        "reasoning": {"effort": "high"}
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();

    // Reasoning present -> no logprobs fields sent downstream
    assert!(j.get("logprobs").is_none());
    assert!(j.get("top_logprobs").is_none());
}

// ── Scenario 5: Full tool conversation roundtrip ─────────────────────

#[tokio::test]
async fn s5_tool_conversation_request() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": [
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Weather in NYC?"}]},
            {"type": "function_call", "call_id": "call_1", "name": "get_weather", "arguments": "{\"city\":\"New York\"}"},
            {"type": "function_call_output", "call_id": "call_1", "output": "Sunny, 72F"},
            {"type": "message", "role": "assistant", "content": [{"type": "input_text", "text": "NYC is sunny, 72F."}]}
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
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

#[tokio::test]
async fn s5_tool_call_response() {
    let chat: chat::Completion = serde_json::from_value(json!({
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

    let resp = chat_to_responses(chat, "gpt-5.5".into(), None);
    let j = serde_json::to_value(&resp).unwrap();

    let output = j["output"].as_array().unwrap();
    // Only function_call, no message (content=null + tool_calls present)
    assert_eq!(output.len(), 1);
    assert_eq!(output[0]["type"], "function_call");
    assert_eq!(output[0]["call_id"], "call_xyz");
    assert_eq!(output[0]["name"], "search");
}

// ── Scenario 6: Content filter ───────────────────────────────────────

#[tokio::test]
async fn s6_content_filter_response() {
    let chat: chat::Completion = serde_json::from_value(json!({
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

    let resp = chat_to_responses(chat, "gpt-5.5".into(), None);
    let j = serde_json::to_value(&resp).unwrap();

    // content_filter with incomplete_details → overall status is "incomplete" per doc
    assert_eq!(j["status"], "incomplete");
    assert_eq!(j["output"][0]["status"], "incomplete");
    assert_eq!(j["output"][0]["content"][0]["type"], "refusal");
    assert_eq!(j["output"][0]["content"][0]["refusal"], "content_filter");
    assert_eq!(j["incomplete_details"]["reason"], "content_filter");
}

// ── Scenario 7: Error response ───────────────────────────────────────

#[tokio::test]
async fn s7_error_response() {
    let chat: chat::Completion = serde_json::from_value(json!({
        "id": "",
        "object": "error",
        "created": 0,
        "model": "",
        "choices": [],
        "error": {"message": "Invalid API key", "type": "invalid_request_error", "code": "invalid_api_key"}
    }))
    .unwrap();

    let resp = chat_to_responses(chat, "gpt-5.5".into(), None);
    let j = serde_json::to_value(&resp).unwrap();

    assert_eq!(j["status"], "failed");
    assert!(j["output"].as_array().unwrap().is_empty());
    assert_eq!(j["error"]["code"], "invalid_api_key");
    assert_eq!(j["error"]["message"], "Invalid API key");
}

// ── Scenario 8: All finish reasons ───────────────────────────────────

#[tokio::test]
async fn s8_finish_reason_all() {
    let cases = &[
        ("stop", "completed", &None),
        ("tool_calls", "completed", &None),
        ("length", "completed", &Some("max_output_tokens")),
        ("content_filter", "incomplete", &Some("content_filter")),
        ("insufficient_system_resource", "completed", &None),
    ];

    for (reason, exp_status, exp_details) in cases {
        let chat: chat::Completion = serde_json::from_value(json!({
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

        let resp = chat_to_responses(chat, "gpt-5.5".into(), None);
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
                    j.get("incomplete_details").is_none_or(|v| v.is_null()),
                    "finish_reason={reason} expected no incomplete_details, got {:?}",
                    j.get("incomplete_details")
                );
            }
        }
    }
}

// ── Scenario 9: Tool normalization (flat→nested, allowlist filter) ──

#[tokio::test]
async fn s9_tool_normalization() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Hi",
        "tools": [
            {"type": "function", "name": "get_weather", "description": "Weather", "parameters": {"type": "object"}, "strict": true},
            {"type": "web_search_preview"}
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();

    let tools = j["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["type"], "function");
    assert_eq!(tools[0]["function"]["name"], "get_weather");
}

// ── Scenario 10: Response format from text.format ────────────────────

#[tokio::test]
async fn s10_text_format_to_response_format() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Output JSON",
        "text": {"format": {"type": "json_object"}}
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["response_format"]["type"], "json_object");
}

#[tokio::test]
async fn s10_json_schema_is_preserved() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Output JSON",
        "text": {
            "format": {
                "type": "json_schema",
                "name": "answer",
                "description": "An answer object",
                "schema": {
                    "type": "object",
                    "properties": {"answer": {"type": "string"}},
                    "required": ["answer"],
                    "additionalProperties": false
                },
                "strict": true
            }
        }
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["response_format"]["type"], "json_schema");
    assert_eq!(j["response_format"]["json_schema"]["name"], "answer");
    assert_eq!(
        j["response_format"]["json_schema"]["schema"]["properties"]["answer"]["type"],
        "string"
    );
    assert_eq!(j["response_format"]["json_schema"]["strict"], true);
}

#[tokio::test]
async fn s10_max_output_and_parallel_false_are_preserved() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Hi",
        "max_output_tokens": 123,
        "parallel_tool_calls": false
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["max_completion_tokens"], 123);
    assert!(j.get("max_tokens").is_none());
    assert_eq!(j["parallel_tool_calls"], false);
}

#[tokio::test]
async fn s10_max_output_tokens_can_map_to_deprecated_max_tokens_by_config() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Hi",
        "max_output_tokens": 123
    }))
    .unwrap();
    let state = test_state_with_rewrite(responses_proxy::config::RewriteConfig {
        steps: vec![responses_proxy::config::RewriteStep::Rename(vec![(
            "max_completion_tokens".into(),
            "max_tokens".into(),
        )])],
    });

    let chat = responses_to_chat(req, &state).await.unwrap();
    let provider = state.config().models.get("gpt-5.5").unwrap();
    let mut j = serde_json::to_value(&chat).unwrap();
    responses_proxy::rewrite::apply_rewrite(&mut j, &provider.rewrite.chat_out).unwrap();

    assert_eq!(j["max_tokens"], 123);
    assert!(j.get("max_completion_tokens").is_none());
}

#[tokio::test]
async fn deepseek_rewrite_preview() {
    let rewrite = deepseek_chat_out_rewrite();

    for effort in ["none", "minimal", "low", "medium", "high", "xhigh"] {
        let req: responses::Request = serde_json::from_value(json!({
            "model": "gpt-5.5",
            "input": "Output JSON",
            "max_output_tokens": 123,
            "reasoning": {"effort": effort},
            "text": {
                "format": {
                    "type": "json_schema",
                    "name": "answer",
                    "schema": {
                        "type": "object",
                        "properties": {"answer": {"type": "string"}},
                        "required": ["answer"],
                        "additionalProperties": false
                    },
                    "strict": true
                }
            }
        }))
        .unwrap();

        let chat = responses_to_chat(req, &test_state()).await.unwrap();
        let mut body = serde_json::to_value(&chat).unwrap();
        responses_proxy::rewrite::apply_rewrite(&mut body, &rewrite).unwrap();

        println!(
            "{effort}:\n{}",
            serde_json::to_string_pretty(&body).unwrap()
        );
    }
}

#[tokio::test]
async fn deepseek_rewrite_reasoning_request_before_after() {
    let rewrite = deepseek_chat_out_rewrite();

    let request_body = json!({
        "model": "gpt-5.5",
        "input": "Output JSON",
        "max_output_tokens": 123,
        "reasoning": {"effort": "high"},
        "text": {
            "format": {
                "type": "json_schema",
                "name": "answer",
                "schema": {
                    "type": "object",
                    "properties": {"answer": {"type": "string"}},
                    "required": ["answer"],
                    "additionalProperties": false
                },
                "strict": true
            }
        }
    });

    let req: responses::Request = serde_json::from_value(request_body.clone()).unwrap();
    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let before_rewrite = serde_json::to_value(&chat).unwrap();
    let mut after_rewrite = before_rewrite.clone();
    responses_proxy::rewrite::apply_rewrite(&mut after_rewrite, &rewrite).unwrap();

    println!(
        "responses request:\n{}",
        serde_json::to_string_pretty(&request_body).unwrap()
    );
    println!(
        "chat before rewrite:\n{}",
        serde_json::to_string_pretty(&before_rewrite).unwrap()
    );
    println!(
        "chat after rewrite:\n{}",
        serde_json::to_string_pretty(&after_rewrite).unwrap()
    );
}

#[tokio::test]
async fn s10_prompt_is_rejected_until_supported() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "prompt": {"id": "pmpt_123"}
    }))
    .unwrap();

    let err = responses_to_chat(req, &test_state()).await.unwrap_err();
    assert!(err.contains(&"prompt".to_string()));
}

#[tokio::test]
async fn s10_no_text_no_response_format() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Hi"
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();

    assert!(j.get("response_format").is_none());
}

// ── Scenario 11: Passthrough fields (temperature, top_p, etc.) ──────

#[tokio::test]
async fn s11_passthrough_fields() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Hi",
        "temperature": 0.7,
        "top_p": 0.9,
        "max_output_tokens": 2048,
        "stop": ["END"],
        "tool_choice": "required"
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["temperature"], 0.7);
    assert_eq!(j["top_p"], 0.9);
    assert_eq!(j["max_completion_tokens"], 2048);
    assert!(j.get("max_tokens").is_none());
    assert_eq!(j["stop"][0], "END");
    assert_eq!(j["tool_choice"], "required");
}

// ── Scenario 12: Streaming usage chunk ───────────────────────────────

#[tokio::test]
async fn s12_streaming_usage_captured() {
    let mut state = StreamState::new(
        "resp_test".into(),
        "msg_test".into(),
        "deepseek-v4-pro".into(),
    );
    state.accumulated_text = "Answer".into();

    // Usage-only chunk (needs all required ChatCompletionChunk fields)
    let events = process_chunk_value(
        &mut state,
        serde_json::from_str(r#"{"id":"chatcmpl-1","object":"chat.completion.chunk","created":1715550000,"model":"test","choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15,"completion_tokens_details":{"reasoning_tokens":3,"audio_tokens":0,"accepted_prediction_tokens":0,"rejected_prediction_tokens":0}}}"#).unwrap(),
    );
    assert!(events.is_none());
    assert!(state.usage.is_some());
    assert_eq!(state.usage.as_ref().unwrap().prompt_tokens, 10);

    // [DONE] includes usage
    let events = build_completion_events(&mut state);
    let completed = events
        .iter()
        .find(|e| matches!(e, StreamEvent::Completed(_)))
        .unwrap();
    let c = match completed {
        StreamEvent::Completed(v) => v,
        _ => panic!(),
    };
    let j = serde_json::to_value(c).unwrap();
    assert_eq!(j["response"]["usage"]["input_tokens"], 10);
    assert_eq!(j["response"]["usage"]["output_tokens"], 5);
    assert_eq!(j["response"]["usage"]["total_tokens"], 15);
    assert_eq!(
        j["response"]["usage"]["output_tokens_details"]["reasoning_tokens"],
        3
    );
}

// Regression: a tool call that's still open when the stream ends (the model's
// last action in the turn, no trailing text/reasoning to trigger a mid-stream
// close) must appear exactly once in the final `response.completed` output —
// not once from the "close still-open items" step and once more from a
// leftover re-derivation off the same accumulator.
#[tokio::test]
async fn streaming_trailing_tool_call_not_duplicated_in_completed_output() {
    let mut state = StreamState::new("resp_test".into(), "msg_test".into(), "gpt-5.6-sol".into());

    process_chunk_value(
        &mut state,
        serde_json::from_str(r#"{"id":"c1","object":"chat.completion.chunk","created":1,"model":"t","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"spawn_agent","arguments":""}}]}}]}"#).unwrap(),
    );

    let events = build_completion_events(&mut state);
    let completed = events
        .iter()
        .find_map(|e| match e {
            StreamEvent::Completed(v) => Some(v),
            _ => None,
        })
        .unwrap();
    let j = serde_json::to_value(completed).unwrap();
    let output = j["response"]["output"].as_array().unwrap();
    let function_calls: Vec<_> = output
        .iter()
        .filter(|o| o["type"] == "function_call")
        .collect();
    assert_eq!(
        function_calls.len(),
        1,
        "spawn_agent must appear exactly once, got: {output:?}"
    );
}

// ── Scenario 13: Streaming output_index no duplicates ────────────────

#[tokio::test]
async fn s13_streaming_output_index_unique() {
    let mut state = StreamState::new(
        "resp_test".into(),
        "msg_test".into(),
        "deepseek-v4-pro".into(),
    );

    let events = process_chunk_value(
        &mut state,
        serde_json::from_str(r#"{"id":"c1","object":"chat.completion.chunk","created":1,"model":"t","choices":[{"index":0,"delta":{"content":"Let me check.","tool_calls":[{"index":0,"id":"call_x","type":"function","function":{"name":"search","arguments":"{}"}}]}}]}"#).unwrap(),
    )
    .unwrap();

    let indices: Vec<u64> = events
        .iter()
        .filter_map(|e| match e {
            StreamEvent::OutputItemAdded(v) => Some(v.output_index as u64),
            _ => None,
        })
        .collect();
    assert_eq!(indices.len(), 2);
    assert_ne!(indices[0], indices[1]);
    assert_eq!(indices[1], indices[0] + 1);
}

#[tokio::test]
async fn s13_streaming_output_index_with_reasoning() {
    let mut state = StreamState::new(
        "resp_test".into(),
        "msg_test".into(),
        "deepseek-v4-pro".into(),
    );

    process_chunk_value(
        &mut state,
        serde_json::from_str(r#"{"id":"c1","object":"chat.completion.chunk","created":1,"model":"t","choices":[{"index":0,"delta":{"reasoning_content":"Let me think"}}]}"#).unwrap(),
    );
    process_chunk_value(
        &mut state,
        serde_json::from_str(r#"{"id":"c2","object":"chat.completion.chunk","created":1,"model":"t","choices":[{"index":0,"delta":{"content":"Answer","tool_calls":[{"index":0,"id":"call_x","type":"function","function":{"name":"search","arguments":"{}"}}]}}]}"#).unwrap(),
    );

    // reasoning was closed at transition, message=1, tool_call=2
    assert!(state.reasoning_content.is_empty()); // cleared when text started
    assert!(!state.accumulated_text.is_empty());
    assert_eq!(state.msg_output_index, 1);
    assert_eq!(state.tool_calls[0].output_index, 2);
}

// ── Scenario 13b: Preamble text preserved when a tool call follows ───
// Regression: a text→tool_calls transition moves the assistant message into
// `completed_items` and clears `accumulated_text`. `to_response_message`
// (WebSocket persistence) must recover the preamble from `completed_items`,
// otherwise the stored history drops it and the model re-acknowledges the
// user's message on every subsequent tool-call turn.
#[tokio::test]
async fn s13b_preamble_text_survives_tool_call_transition() {
    let mut state = StreamState::new("resp_test".into(), "msg_test".into(), "test".into());

    // Chunk 1: assistant preamble text only.
    process_chunk_value(
        &mut state,
        serde_json::from_str(r#"{"id":"c1","object":"chat.completion.chunk","created":1,"model":"t","choices":[{"index":0,"delta":{"content":"On it."}}]}"#).unwrap(),
    );
    // Chunk 2: tool call with no content — triggers the text→tool_calls transition.
    process_chunk_value(
        &mut state,
        serde_json::from_str(r#"{"id":"c2","object":"chat.completion.chunk","created":1,"model":"t","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_x","type":"function","function":{"name":"search","arguments":"{}"}}]}}]}"#).unwrap(),
    );

    // The transition cleared accumulated_text and stored the message.
    assert!(state.accumulated_text.is_empty());

    // Persisted message must still carry the preamble AND the tool call.
    let msg = state.to_response_message();
    assert_eq!(msg.content.as_deref(), Some("On it."));
    assert!(msg.tool_calls.as_ref().is_some_and(|tc| !tc.is_empty()));
}

// ── Scenario 14: Streaming in_progress after created ─────────────────

#[tokio::test]
async fn s14_streaming_in_progress_emitted() {
    let mut state = StreamState::new(
        "resp_test".into(),
        "msg_test".into(),
        "deepseek-v4-pro".into(),
    );

    let events =
        process_chunk_value(&mut state, serde_json::from_str(r#"{"id":"c1","object":"chat.completion.chunk","created":1,"model":"t","choices":[{"index":0,"delta":{"content":"Hello"}}]}"#).unwrap()).unwrap();

    let types: Vec<String> = events
        .iter()
        .map(|e| {
            serde_json::to_value(e).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    let created = types.iter().position(|t| *t == "response.created").unwrap();
    let in_progress = types
        .iter()
        .position(|t| *t == "response.in_progress")
        .unwrap();
    assert!(created < in_progress);
}

#[tokio::test]
async fn s14_streaming_in_progress_only_once() {
    let mut state = StreamState::new(
        "resp_test".into(),
        "msg_test".into(),
        "deepseek-v4-pro".into(),
    );

    process_chunk_value(
        &mut state,
        serde_json::from_str(r#"{"id":"c1","object":"chat.completion.chunk","created":1,"model":"t","choices":[{"index":0,"delta":{"content":"A"}}]}"#).unwrap(),
    );
    // second chunk — no duplicate in_progress
    let events = process_chunk_value(&mut state, serde_json::from_str(r#"{"id":"c2","object":"chat.completion.chunk","created":1,"model":"t","choices":[{"index":0,"delta":{"content":"B"}}]}"#).unwrap()).unwrap();

    let has_in_progress = events
        .iter()
        .any(|e| matches!(e, StreamEvent::InProgress(_)));
    assert!(!has_in_progress);
}

// ── Scenario 15: Cached tokens ───────────────────────────────────────

#[tokio::test]
async fn s15_cached_tokens_openai_style() {
    let chat: chat::Completion = serde_json::from_value(json!({
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

    let resp = chat_to_responses(chat, "gpt-5.5".into(), None);
    let j = serde_json::to_value(&resp).unwrap();

    assert_eq!(j["usage"]["input_tokens"], 100);
    assert_eq!(j["usage"]["input_tokens_details"]["cached_tokens"], 80);
}

// ── Scenario 16: Instructions merged with input system message ───────

#[tokio::test]
async fn s16_instructions_merge_with_system() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": [
            {"type": "message", "role": "system", "content": [{"type": "input_text", "text": "You are helpful."}]},
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Hi"}]}
        ],
        "instructions": "Top-level instructions."
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();

    let msgs = j["messages"].as_array().unwrap();
    // instructions → system, input system → system, user → user = 3 messages
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[0]["role"], "system");
    assert_eq!(msgs[0]["content"], "Top-level instructions.");
    assert_eq!(msgs[1]["role"], "system");
    assert_eq!(msgs[1]["content"], "You are helpful.");
    assert_eq!(msgs[2]["role"], "user");
}

// ── Scenario 17: Developer role preserved ─────────────────────────────

#[tokio::test]
async fn s17_developer_to_system() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": [
            {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "Dev rules"}]},
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Hi"}]}
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();

    // Developer message stays as developer (not converted to system)
    assert_eq!(j["messages"][0]["role"], "developer");
    assert_eq!(j["messages"][0]["content"], "Dev rules");
}

// ── Scenario 18: Reasoning in input history ──────────────────────────

#[tokio::test]
async fn s18_reasoning_item_in_input() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": [
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "What's weather?"}]},
            {"type": "reasoning", "id": "rs_1", "content": [{"type": "reasoning_text", "text": "Let me check the API."}]},
            {"type": "function_call", "call_id": "call_1", "name": "get_weather", "arguments": "{\"city\":\"NYC\"}"}
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
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

#[tokio::test]
async fn s19_multiple_content_blocks_joined() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": [
            {"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": "Hello"},
                {"type": "input_text", "text": "World"}
            ]}
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["messages"][0]["content"], "Hello\nWorld");
}

// ── Scenario 20: Empty instructions ignored ──────────────────────────

#[tokio::test]
async fn s20_empty_instructions_ignored() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Hi",
        "instructions": ""
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["messages"].as_array().unwrap().len(), 1);
    assert_eq!(j["messages"][0]["role"], "user");
}

// ── Scenario 21: Image/file blocks silently dropped ──────────────────

#[tokio::test]
async fn s21_image_file_blocks_dropped() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": [{"type": "message", "role": "user", "content": [
            {"type": "input_text", "text": "Describe:"},
            {"type": "input_image", "image_url": "https://example.com/img.png"},
            {"type": "input_file", "file_id": "file-abc"}
        ]}]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();

    // Multimodal content is now passed through as Parts array
    let content = j["messages"][0]["content"].as_array().unwrap();
    assert_eq!(content.len(), 3);
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[0]["text"], "Describe:");
    assert_eq!(content[1]["type"], "image_url");
    assert_eq!(
        content[1]["image_url"]["url"],
        "https://example.com/img.png"
    );
    assert_eq!(content[2]["type"], "file");
    assert_eq!(content[2]["file"]["file_id"], "file-abc");
}

// ── Scenario 22: Unknown item/content silently skipped ───────────────

#[tokio::test]
async fn s22_unknown_items_skipped() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": [
            {"type": "item_reference", "id": "item_abc"},
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Hello"}]},
            {"type": "future_item", "data": "ignored"}
        ]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["messages"].as_array().unwrap().len(), 1);
    assert_eq!(j["messages"][0]["content"], "Hello");
}

// ── Scenario 23: String input with instructions ──────────────────────

#[tokio::test]
async fn s23_string_input_with_instructions() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "What is Rust?",
        "instructions": "You are a helpful assistant."
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();

    let msgs = j["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0]["role"], "system");
    assert_eq!(msgs[0]["content"], "You are a helpful assistant.");
    assert_eq!(msgs[1]["role"], "user");
    assert_eq!(msgs[1]["content"], "What is Rust?");
}

// ── Scenario 24: Array input with username ──── (tests position) ─────

#[tokio::test]
async fn s24_function_call_output_array() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": [{"type": "function_call_output", "call_id": "call_1", "output": [
            {"type": "input_text", "text": "Result text here"}
        ]}]
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["messages"][0]["role"], "tool");
    assert_eq!(j["messages"][0]["content"], "Result text here");
    assert_eq!(j["messages"][0]["tool_call_id"], "call_1");
}

// ── Scenario 25: Stream options passthrough ──────────────────────────

#[tokio::test]
async fn s25_stream_true() {
    let req: responses::Request = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "input": "Hi",
        "stream": true
    }))
    .unwrap();

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
    let j = serde_json::to_value(&chat).unwrap();

    assert_eq!(j["stream"], true);
}

// ── Scenario 26: Consecutive function calls merge ────────────────────

#[tokio::test]
async fn s26_consecutive_function_calls_merge() {
    let req: responses::Request = serde_json::from_value(json!({
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

    let chat = responses_to_chat(req, &test_state()).await.unwrap();
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

// ── Scenario 27: Age-based tool-output truncation (KEEP_LAST_TOOL_OUTPUTS) ──
// A single Codex prompt can spawn dozens of sequential tool calls with no new
// user message in between. These exercise the real end-to-end path
// (items_to_chat_messages) with a history built the way Codex actually
// produces it, rather than the unit-level helpers tested in
// src/convert/request.rs.

fn big_tool_payload(tag: &str) -> String {
    format!("{tag}:{}", "Z".repeat(30_000))
}

/// One user message followed by 12 call/output pairs (alternating
/// function/custom tool calls), each output a distinct 30 KB payload tagged
/// with its call id so truncated-vs-verbatim can be told apart.
fn history_with_twelve_tool_pairs() -> Vec<responses_proxy::types::item::InputItem> {
    let mut raw = vec![json!({
        "type": "message", "role": "user",
        "content": [{"type": "input_text", "text": "kick off many tool calls"}]
    })];
    for i in 0..12 {
        let call_id = format!("call_{i}");
        let payload = big_tool_payload(&call_id);
        if i % 2 == 0 {
            raw.push(json!({
                "type": "function_call", "call_id": call_id,
                "name": "run_tool", "arguments": "{}"
            }));
            raw.push(json!({
                "type": "function_call_output", "call_id": call_id, "output": payload
            }));
        } else {
            raw.push(json!({
                "type": "custom_tool_call", "call_id": call_id,
                "name": "run_custom_tool", "input": "do work"
            }));
            raw.push(json!({
                "type": "custom_tool_call_output", "call_id": call_id, "output": payload
            }));
        }
    }
    serde_json::from_value(json!(raw)).unwrap()
}

#[test]
fn age_truncation_keeps_last_eight_tool_outputs_verbatim() {
    let items = history_with_twelve_tool_pairs();
    let messages = responses_proxy::convert::items_to_chat_messages(&items, &test_state());
    let j = serde_json::to_value(&messages).unwrap();

    // [user, assistant(12 tool_calls), tool x12] — no flush point exists
    // between the calls/outputs, so they land in one merged assistant
    // message followed by the 12 deferred tool messages, in call order.
    assert_eq!(j.as_array().unwrap().len(), 14);
    assert_eq!(j[0]["role"], "user");
    assert_eq!(j[1]["role"], "assistant");
    assert_eq!(j[1]["tool_calls"].as_array().unwrap().len(), 12);

    for i in 0..12 {
        let call_id = format!("call_{i}");
        let msg = &j[i + 2];
        assert_eq!(msg["role"], "tool");
        assert_eq!(msg["tool_call_id"], call_id);
        let content = msg["content"].as_str().unwrap();
        if i < 4 {
            assert!(
                content.contains("…[truncated"),
                "output #{i} (of 12) should be old and truncated"
            );
            assert!(content.len() <= MAX_OLD_TOOL_OUTPUT_CHARS_TEST);
        } else {
            assert_eq!(
                content,
                big_tool_payload(&call_id),
                "output #{i} is within the last 8 and must stay verbatim"
            );
        }
    }
}

// Mirrors the private MAX_OLD_TOOL_OUTPUT_CHARS constant in src/convert/request.rs
// (not exported); kept in sync manually since it only bounds a `<=` assertion.
const MAX_OLD_TOOL_OUTPUT_CHARS_TEST: usize = 4096;

#[test]
fn age_truncation_preserves_tool_call_pairing() {
    let items = history_with_twelve_tool_pairs();
    let messages = responses_proxy::convert::items_to_chat_messages(&items, &test_state());
    let j = serde_json::to_value(&messages).unwrap();

    let tool_call_ids: Vec<String> = j[1]["tool_calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tc| tc["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(tool_call_ids.len(), 12);

    for (i, expected_id) in tool_call_ids.iter().enumerate() {
        let tool_msg = &j[i + 2];
        assert_eq!(
            tool_msg["tool_call_id"].as_str().unwrap(),
            expected_id,
            "tool message #{i} must pair with the assistant tool_call at the same position, truncated or not"
        );
    }
}

#[test]
fn hard_budget_applies_on_top_of_age_truncation() {
    let items = history_with_twelve_tool_pairs();
    let mut messages = responses_proxy::convert::items_to_chat_messages(&items, &test_state());

    // Snapshot the already age-truncated outputs (the first 4) before the
    // hard budget runs — they must not be touched again since they are
    // already <= MAX_OLD_TOOL_OUTPUT_CHARS.
    let pre_truncated: Vec<String> = messages[2..6]
        .iter()
        .map(|m| match m {
            chat::MessageRequest::Tool(t) => match &t.content {
                chat::MessageContent::Text(s) => s.clone(),
                chat::MessageContent::Parts(_) => panic!("expected text content"),
            },
            _ => panic!("expected a tool message"),
        })
        .collect();

    let total_before: usize = messages
        .iter()
        .map(|m| serde_json::to_string(m).unwrap().len())
        .sum();
    // Small enough to force shrinking several of the still-verbatim last-8
    // outputs, on top of the age truncation that already ran.
    let budget = total_before / 2;

    let shrunk = responses_proxy::convert::enforce_input_budget(&mut messages, budget);
    assert!(shrunk > 0, "hard budget must shrink additional messages");

    let total_after: usize = messages
        .iter()
        .map(|m| serde_json::to_string(m).unwrap().len())
        .sum();
    assert!(total_after < total_before);

    // Already-shrunk age-truncated outputs are left alone by the hard budget.
    let post_truncated: Vec<String> = messages[2..6]
        .iter()
        .map(|m| match m {
            chat::MessageRequest::Tool(t) => match &t.content {
                chat::MessageContent::Text(s) => s.clone(),
                chat::MessageContent::Parts(_) => panic!("expected text content"),
            },
            _ => panic!("expected a tool message"),
        })
        .collect();
    assert_eq!(pre_truncated, post_truncated);

    // Pairing survives the hard budget too.
    let j = serde_json::to_value(&messages).unwrap();
    let tool_call_ids: Vec<String> = j[1]["tool_calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tc| tc["id"].as_str().unwrap().to_string())
        .collect();
    for (i, expected_id) in tool_call_ids.iter().enumerate() {
        assert_eq!(j[i + 2]["tool_call_id"].as_str().unwrap(), expected_id);
    }
}

// Regression: some upstream providers report an outage mid-stream by sending
// a chunk with a top-level `error` field instead of a normal delta, alongside
// `finish_reason: "error"`. The old code had no `error` field on `Chunk` at
// all, so this was silently dropped, `finish_reason` fell through to the
// default match arm, and the turn was reported as `response.completed` with
// empty output — Codex had no way to tell the turn had actually failed.
#[tokio::test]
async fn streaming_upstream_error_chunk_reported_as_failed() {
    let mut state = StreamState::new("resp_test".into(), "msg_test".into(), "gpt-5.6-sol".into());

    process_chunk_value(
        &mut state,
        serde_json::from_str(r#"{"id":"cmpl-1","object":"chat.completion.chunk","created":1,"model":"gpt-5.6-sol","provider":"openai","error":{"code":"provider_model_down","message":"Provider model is down"},"choices":[{"index":0,"delta":{"content":""},"finish_reason":"error"}]}"#).unwrap(),
    );

    let events = build_completion_events(&mut state);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, StreamEvent::Completed(_))),
        "an upstream error must not be reported as a normal completion"
    );
    let failed = events
        .iter()
        .find_map(|e| match e {
            StreamEvent::Failed(v) => Some(v),
            _ => None,
        })
        .expect("expected a response.failed event");
    let j = serde_json::to_value(failed).unwrap();
    assert_eq!(j["response"]["status"], "failed");
    assert_eq!(j["response"]["error"]["code"], "provider_model_down");
    assert_eq!(j["response"]["error"]["message"], "Provider model is down");
}
