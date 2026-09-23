use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use responses_proxy::config::{ResolvedConfig, ResolvedProvider};
use responses_proxy::handlers;
use responses_proxy::types::MessageRole;
use responses_proxy::types::chat;
use responses_proxy::types::item::{InputContentBlock, InputItem, OutputContentBlock};
use responses_proxy::types::responses;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[derive(Clone, Default)]
struct MockChatState {
    seen: Arc<Mutex<Vec<chat::Request>>>,
    scripted: Arc<Mutex<VecDeque<chat::Completion>>>,
}

async fn mock_chat_completions(
    State(state): State<MockChatState>,
    Json(req): Json<chat::Request>,
) -> Json<chat::Completion> {
    let mut seen = state.seen.lock().await;
    seen.push(req.clone());
    let turn = seen.len();
    drop(seen);

    if let Some(completion) = state.scripted.lock().await.pop_front() {
        return Json(completion);
    }

    Json(text_completion(turn, &format!("mock answer {turn}")))
}

fn text_completion(turn: usize, content: &str) -> chat::Completion {
    chat::Completion {
        error: None,
        id: format!("chatcmpl_mock_{turn}"),
        choices: vec![chat::Choice {
            finish_reason: Some("stop".into()),
            index: 0,
            logprobs: None,
            message: chat::ResponseMessage {
                content: Some(content.to_string()),
                refusal: None,
                role: "assistant".into(),
                annotations: None,
                audio: None,
                function_call: None,
                reasoning_content: None,
                tool_calls: None,
            },
        }],
        created: 1_700_000_000 + turn as i64,
        model: "mock-chat-model".into(),
        object: "chat.completion".into(),
        service_tier: None,
        system_fingerprint: None,
        usage: Some(chat::Usage {
            prompt_tokens: 10,
            completion_tokens: 3,
            total_tokens: 13,
            prompt_cache_hit_tokens: None,
            prompt_cache_miss_tokens: None,
            prompt_tokens_details: Some(chat::PromptTokensDetails {
                cached_tokens: 2,
                audio_tokens: 0,
            }),
            completion_tokens_details: Some(chat::CompletionTokensDetails {
                reasoning_tokens: 0,
                audio_tokens: 0,
                accepted_prediction_tokens: 0,
                rejected_prediction_tokens: 0,
            }),
        }),
    }
}

fn tool_call_completion(
    turn: usize,
    call_id: &str,
    name: &str,
    arguments: &str,
) -> chat::Completion {
    chat::Completion {
        error: None,
        id: format!("chatcmpl_tool_{turn}"),
        choices: vec![chat::Choice {
            finish_reason: Some("tool_calls".into()),
            index: 0,
            logprobs: None,
            message: chat::ResponseMessage {
                content: None,
                refusal: None,
                role: "assistant".into(),
                annotations: None,
                audio: None,
                function_call: None,
                reasoning_content: None,
                tool_calls: Some(vec![chat::ToolCallResponse::Function {
                    id: call_id.to_string(),
                    function: chat::ToolCallFunction {
                        name: name.to_string(),
                        arguments: arguments.to_string(),
                    },
                }]),
            },
        }],
        created: 1_700_001_000 + turn as i64,
        model: "mock-chat-model".into(),
        object: "chat.completion".into(),
        service_tier: None,
        system_fingerprint: None,
        usage: Some(chat::Usage {
            prompt_tokens: 12,
            completion_tokens: 4,
            total_tokens: 16,
            prompt_cache_hit_tokens: None,
            prompt_cache_miss_tokens: None,
            prompt_tokens_details: Some(chat::PromptTokensDetails {
                cached_tokens: 0,
                audio_tokens: 0,
            }),
            completion_tokens_details: Some(chat::CompletionTokensDetails {
                reasoning_tokens: 0,
                audio_tokens: 0,
                accepted_prediction_tokens: 0,
                rejected_prediction_tokens: 0,
            }),
        }),
    }
}

async fn spawn_mock_chat_api() -> (String, Arc<Mutex<Vec<chat::Request>>>) {
    spawn_mock_chat_api_with_script(vec![]).await
}

async fn spawn_mock_chat_api_with_script(
    completions: Vec<chat::Completion>,
) -> (String, Arc<Mutex<Vec<chat::Request>>>) {
    let state = MockChatState::default();
    *state.scripted.lock().await = completions.into();
    let seen = state.seen.clone();
    let app = Router::new()
        .route("/chat/completions", post(mock_chat_completions))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), seen)
}

async fn spawn_proxy(chat_base_url: String) -> String {
    let mut models = HashMap::new();
    models.insert(
        "gpt-proxy-test".into(),
        ResolvedProvider {
            base_url: chat_base_url,
            api_key: "mock-key".into(),
            model: "mock-chat-model".into(),
            timeout: Duration::from_secs(10),
            rewrite: Default::default(),
            max_input_chars: None,
            max_input_messages: 1000,
            max_tools: 0,
            stream_structured_output: true,
            context_window: None,
            reasoning_levels: vec![responses_proxy::types::ReasoningEffort::Medium],
            default_reasoning_level: responses_proxy::types::ReasoningEffort::Medium,
        },
    );

    let state = responses_proxy::app::State::new(ResolvedConfig {
        listen: String::new(),
        timeout: 10,
        auth_keys: HashSet::new(),
        cors_allow_origins: vec![],
        allowed_tool_types: vec!["function".into(), "custom".into()],
        log_level: "info".into(),
        model_names: vec!["gpt-proxy-test".into()],
        models,
        compact_encryption_key: String::new(),
        max_body_bytes: 100 * 1024 * 1024,
    });

    let app = Router::new()
        .route("/v1/responses", post(handlers::responses))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn user_text(text: &str) -> InputItem {
    InputItem::Message(responses_proxy::types::item::InputMessage {
        role: MessageRole::User,
        content: vec![InputContentBlock::Text {
            text: text.to_string(),
        }],
        status: None,
    })
}

fn function_call_output(call_id: &str, output: &str) -> InputItem {
    InputItem::FunctionCallOutput(responses_proxy::types::item::FunctionCallOutput {
        call_id: call_id.to_string(),
        output: responses_proxy::types::item::FunctionOutputValue::String(output.to_string()),
        id: None,
        status: Some("completed".into()),
    })
}

fn output_function_call(resp: &responses::Response) -> (String, String, String) {
    resp.output
        .iter()
        .find_map(|item| match item {
            responses_proxy::types::item::OutputItem::FunctionCall(call) => Some((
                call.call_id.clone(),
                call.name.clone(),
                call.arguments.clone(),
            )),
            _ => None,
        })
        .unwrap()
}

fn output_text(resp: &responses::Response) -> String {
    resp.output
        .iter()
        .find_map(|item| match item {
            responses_proxy::types::item::OutputItem::Message(message) => {
                message.content.iter().find_map(|part| match part {
                    OutputContentBlock::Text { text, .. } => Some(text.clone()),
                    OutputContentBlock::Refusal { .. } => None,
                })
            }
            _ => None,
        })
        .unwrap()
}

fn user_content_text(content: &chat::UserContent) -> &str {
    match content {
        chat::UserContent::Text(text) => text,
        other => panic!("expected text user content, got {other:?}"),
    }
}

fn assistant_content_text(content: &Option<chat::AssistantContent>) -> &str {
    match content {
        Some(chat::AssistantContent::Text(text)) => text,
        other => panic!("expected text assistant content, got {other:?}"),
    }
}

fn assert_user_message(message: &chat::MessageRequest, expected: &str) {
    match message {
        chat::MessageRequest::User(message) => {
            assert_eq!(user_content_text(&message.content), expected);
        }
        other => panic!("expected user message '{expected}', got {other:?}"),
    }
}

fn assert_assistant_message(message: &chat::MessageRequest, expected: &str) {
    match message {
        chat::MessageRequest::Assistant(message) => {
            assert_eq!(assistant_content_text(&message.content), expected);
        }
        other => panic!("expected assistant message '{expected}', got {other:?}"),
    }
}

fn assert_tool_message(message: &chat::MessageRequest, call_id: &str, expected: &str) {
    match message {
        chat::MessageRequest::Tool(message) => {
            assert_eq!(message.tool_call_id, call_id);
            match &message.content {
                chat::MessageContent::Text(text) => assert_eq!(text, expected),
                other => panic!("expected text tool content, got {other:?}"),
            }
        }
        other => panic!("expected tool message for '{call_id}', got {other:?}"),
    }
}

fn assert_assistant_tool_call(
    message: &chat::MessageRequest,
    call_id: &str,
    name: &str,
    arguments: &str,
) {
    match message {
        chat::MessageRequest::Assistant(message) => {
            let calls = message.tool_calls.as_ref().unwrap();
            assert_eq!(calls.len(), 1);
            match &calls[0] {
                chat::ToolCallRequest::Function { id, function } => {
                    assert_eq!(id, call_id);
                    assert_eq!(function.name, name);
                    assert_eq!(function.arguments, arguments);
                }
                other => panic!("expected function tool call, got {other:?}"),
            }
        }
        other => panic!("expected assistant tool call message, got {other:?}"),
    }
}

#[tokio::test]
async fn proxy_agent_maps_typed_responses_request_to_typed_chat_request() {
    let (chat_base_url, seen_chat_requests) = spawn_mock_chat_api().await;
    let proxy_base_url = spawn_proxy(chat_base_url).await;
    let client = reqwest::Client::new();

    let req = responses::Request {
        model: "gpt-proxy-test".into(),
        input: vec![user_text("return a typed object")],
        max_output_tokens: Some(64),
        parallel_tool_calls: false,
        text: Some(responses::TextConfig {
            format: Some(responses::TextFormat::JsonSchema {
                name: "answer".into(),
                schema: serde_json::json!({
                    "type": "object",
                    "properties": {"answer": {"type": "string"}},
                    "required": ["answer"],
                    "additionalProperties": false
                }),
                strict: Some(true),
                description: Some("A typed answer".into()),
            }),
            verbosity: None,
        }),
        ..Default::default()
    };

    let resp = client
        .post(format!("{proxy_base_url}/v1/responses"))
        .json(&req)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json::<responses::Response>()
        .await
        .unwrap();

    assert_eq!(resp.status, responses::ResponseStatus::Completed);
    assert_eq!(output_text(&resp), "mock answer 1");
    assert_eq!(resp.usage.unwrap().input_tokens_details.cached_tokens, 2);

    let seen = seen_chat_requests.lock().await;
    assert_eq!(seen.len(), 1);
    let chat_req = &seen[0];
    assert_eq!(chat_req.model, "mock-chat-model");
    assert_eq!(chat_req.max_completion_tokens, Some(64));
    assert_eq!(chat_req.max_tokens, None);
    assert_eq!(chat_req.parallel_tool_calls, Some(false));

    match chat_req.response_format.as_ref().unwrap() {
        chat::ResponseFormat::JsonSchema(format) => {
            assert_eq!(format.json_schema.name, "answer");
            assert_eq!(format.json_schema.strict, Some(true));
            assert_eq!(
                format.json_schema.schema.as_ref().unwrap()["properties"]["answer"]["type"],
                "string"
            );
        }
        other => panic!("expected json_schema response_format, got {other:?}"),
    }
}

#[tokio::test]
async fn proxy_agent_replays_previous_response_context_to_chat_server() {
    let (chat_base_url, seen_chat_requests) = spawn_mock_chat_api().await;
    let proxy_base_url = spawn_proxy(chat_base_url).await;
    let client = reqwest::Client::new();

    let first = responses::Request {
        model: "gpt-proxy-test".into(),
        input: vec![user_text("first user turn")],
        ..Default::default()
    };
    let first_resp = client
        .post(format!("{proxy_base_url}/v1/responses"))
        .json(&first)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json::<responses::Response>()
        .await
        .unwrap();
    assert_eq!(output_text(&first_resp), "mock answer 1");

    let second = responses::Request {
        model: "gpt-proxy-test".into(),
        previous_response_id: Some(first_resp.id.clone()),
        input: vec![user_text("second user turn")],
        ..Default::default()
    };
    let second_resp = client
        .post(format!("{proxy_base_url}/v1/responses"))
        .json(&second)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json::<responses::Response>()
        .await
        .unwrap();
    assert_eq!(output_text(&second_resp), "mock answer 2");

    let seen = seen_chat_requests.lock().await;
    assert_eq!(seen.len(), 2);
    let second_chat = &seen[1];
    assert_eq!(second_chat.messages.len(), 3);

    assert_user_message(&second_chat.messages[0], "first user turn");
    assert_assistant_message(&second_chat.messages[1], "mock answer 1");
    assert_user_message(&second_chat.messages[2], "second user turn");
}

#[tokio::test]
async fn proxy_agent_replays_three_turn_text_history() {
    let (chat_base_url, seen_chat_requests) = spawn_mock_chat_api().await;
    let proxy_base_url = spawn_proxy(chat_base_url).await;
    let client = reqwest::Client::new();

    let first_resp = post_response(
        &client,
        &proxy_base_url,
        responses::Request {
            model: "gpt-proxy-test".into(),
            input: vec![user_text("turn one")],
            ..Default::default()
        },
    )
    .await;
    let second_resp = post_response(
        &client,
        &proxy_base_url,
        responses::Request {
            model: "gpt-proxy-test".into(),
            previous_response_id: Some(first_resp.id.clone()),
            input: vec![user_text("turn two")],
            ..Default::default()
        },
    )
    .await;
    let third_resp = post_response(
        &client,
        &proxy_base_url,
        responses::Request {
            model: "gpt-proxy-test".into(),
            previous_response_id: Some(second_resp.id.clone()),
            input: vec![user_text("turn three")],
            ..Default::default()
        },
    )
    .await;

    assert_eq!(output_text(&third_resp), "mock answer 3");

    let seen = seen_chat_requests.lock().await;
    assert_eq!(seen.len(), 3);
    let third_chat = &seen[2];
    assert_eq!(third_chat.messages.len(), 5);
    assert_user_message(&third_chat.messages[0], "turn one");
    assert_assistant_message(&third_chat.messages[1], "mock answer 1");
    assert_user_message(&third_chat.messages[2], "turn two");
    assert_assistant_message(&third_chat.messages[3], "mock answer 2");
    assert_user_message(&third_chat.messages[4], "turn three");
}

#[tokio::test]
async fn proxy_agent_replays_tool_call_and_tool_output_history() {
    let (chat_base_url, seen_chat_requests) = spawn_mock_chat_api_with_script(vec![
        tool_call_completion(1, "call_weather", "get_weather", r#"{"city":"Paris"}"#),
        text_completion(2, "Paris is mild."),
    ])
    .await;
    let proxy_base_url = spawn_proxy(chat_base_url).await;
    let client = reqwest::Client::new();

    let first_resp = post_response(
        &client,
        &proxy_base_url,
        responses::Request {
            model: "gpt-proxy-test".into(),
            input: vec![user_text("weather in Paris?")],
            tools: Some(vec![responses_proxy::types::tool::ToolRequest::Function(
                responses_proxy::types::tool::FunctionToolRequest {
                    name: Some("get_weather".into()),
                    description: Some("Get weather".into()),
                    parameters: Some(serde_json::json!({
                        "type": "object",
                        "properties": {"city": {"type": "string"}},
                        "required": ["city"]
                    })),
                    strict: Some(true),
                    defer_loading: None,
                },
            )]),
            ..Default::default()
        },
    )
    .await;
    let (call_id, name, arguments) = output_function_call(&first_resp);
    assert_eq!(call_id, "call_weather");
    assert_eq!(name, "get_weather");
    assert_eq!(arguments, r#"{"city":"Paris"}"#);

    let second_resp = post_response(
        &client,
        &proxy_base_url,
        responses::Request {
            model: "gpt-proxy-test".into(),
            previous_response_id: Some(first_resp.id.clone()),
            input: vec![function_call_output("call_weather", r#"{"temp_c":18}"#)],
            ..Default::default()
        },
    )
    .await;
    assert_eq!(output_text(&second_resp), "Paris is mild.");

    let seen = seen_chat_requests.lock().await;
    assert_eq!(seen.len(), 2);
    let second_chat = &seen[1];
    assert_eq!(second_chat.messages.len(), 3);
    assert_user_message(&second_chat.messages[0], "weather in Paris?");
    assert_assistant_tool_call(
        &second_chat.messages[1],
        "call_weather",
        "get_weather",
        r#"{"city":"Paris"}"#,
    );
    assert_tool_message(&second_chat.messages[2], "call_weather", r#"{"temp_c":18}"#);
}

async fn post_response(
    client: &reqwest::Client,
    proxy_base_url: &str,
    req: responses::Request,
) -> responses::Response {
    client
        .post(format!("{proxy_base_url}/v1/responses"))
        .json(&req)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json::<responses::Response>()
        .await
        .unwrap()
}

fn compaction_trigger() -> InputItem {
    InputItem::CompactionTrigger(responses_proxy::types::item::CompactionTrigger::default())
}

/// B0 regression: Codex sends `compaction_trigger` with the full history inline
/// in `input` (store:false). The proxy must summarize that inline history — not
/// the empty server-side store — so the upstream summary request must contain
/// the replayed conversation, not just the summary prompt.
#[tokio::test]
async fn compaction_trigger_summarizes_inline_input_history() {
    let (chat_base_url, seen_chat_requests) = spawn_mock_chat_api().await;
    let proxy_base_url = spawn_proxy(chat_base_url).await;
    let client = reqwest::Client::new();

    let req = responses::Request {
        model: "gpt-proxy-test".into(),
        // No previous_response_id — history lives entirely in `input`.
        input: vec![
            user_text("first user turn"),
            user_text("second user turn"),
            compaction_trigger(),
        ],
        ..Default::default()
    };
    let resp = client
        .post(format!("{proxy_base_url}/v1/responses"))
        .json(&req)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json::<responses::Response>()
        .await
        .unwrap();

    // Response carries exactly one compaction output item.
    assert_eq!(resp.output.len(), 1);
    assert!(matches!(
        resp.output[0],
        responses_proxy::types::item::OutputItem::Compaction(_)
    ));

    // The upstream summary request must include the inline history followed by
    // the summary prompt — proving B0 reads `input`, not the empty store.
    let seen = seen_chat_requests.lock().await;
    assert_eq!(
        seen.len(),
        1,
        "compaction should make exactly one upstream call"
    );
    let summary_req = &seen[0];
    assert!(
        summary_req.messages.len() >= 3,
        "expected inline history + summary prompt, got {} messages",
        summary_req.messages.len()
    );
    assert_user_message(&summary_req.messages[0], "first user turn");
    assert_user_message(&summary_req.messages[1], "second user turn");
    // Last message is the summary prompt (a user message; content may be Parts).
    let last = summary_req.messages.last().unwrap();
    match last {
        chat::MessageRequest::User(m) => {
            let text = match &m.content {
                chat::UserContent::Text(t) => t.clone(),
                chat::UserContent::Parts(parts) => parts
                    .iter()
                    .filter_map(|p| match p {
                        chat::ContentPart::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(""),
            };
            assert!(
                text.contains("summary") || text.contains("<summary>"),
                "last message should be the summary prompt"
            );
        }
        other => panic!("expected summary-prompt user message, got {other:?}"),
    }
}
