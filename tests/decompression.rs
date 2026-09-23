//! End-to-end tests for request decompression + safe parse-error handling.
//!
//! Codex sends compressed request bodies (zstd). The proxy must transparently
//! decompress them before JSON parsing, and on a genuine parse failure it must
//! return a structured OpenAI-style 400 (never raw body bytes in the response).

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use axum::routing::post;
use axum::{Json, Router};
use responses_proxy::config::{ResolvedConfig, ResolvedProvider};
use responses_proxy::handlers;
use responses_proxy::types::chat;
use tokio::net::TcpListener;
use tower_http::decompression::RequestDecompressionLayer;

async fn mock_chat_completions() -> Json<chat::Completion> {
    Json(chat::Completion {
        error: None,
        id: "chatcmpl_mock".into(),
        choices: vec![chat::Choice {
            finish_reason: Some("stop".into()),
            index: 0,
            logprobs: None,
            message: chat::ResponseMessage {
                content: Some("hello".into()),
                refusal: None,
                role: "assistant".into(),
                annotations: None,
                audio: None,
                function_call: None,
                reasoning_content: None,
                tool_calls: None,
            },
        }],
        created: 1_700_000_000,
        model: "mock-chat-model".into(),
        object: "chat.completion".into(),
        service_tier: None,
        system_fingerprint: None,
        usage: None,
    })
}

async fn spawn_mock_chat_api() -> String {
    let app = Router::new().route("/chat/completions", post(mock_chat_completions));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
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
        .layer(
            RequestDecompressionLayer::new()
                .gzip(true)
                .br(true)
                .zstd(true)
                .deflate(true),
        )
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn zstd_compressed_request_body_is_decompressed() {
    let chat_base_url = spawn_mock_chat_api().await;
    let proxy_base_url = spawn_proxy(chat_base_url).await;
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "model": "gpt-proxy-test",
        "input": [{
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": "ping"}]
        }],
        "stream": false
    });
    let raw = serde_json::to_vec(&body).unwrap();
    let compressed = zstd::encode_all(&raw[..], 0).unwrap();

    let resp = client
        .post(format!("{proxy_base_url}/v1/responses"))
        .header("content-type", "application/json")
        .header("content-encoding", "zstd")
        .body(compressed)
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        200,
        "zstd body should be decompressed and processed"
    );
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["status"], "completed");
}

#[tokio::test]
async fn malformed_json_returns_structured_400() {
    let chat_base_url = spawn_mock_chat_api().await;
    let proxy_base_url = spawn_proxy(chat_base_url).await;
    let client = reqwest::Client::new();

    // Raw binary that is not valid JSON (and not a recognized encoding).
    let garbage: Vec<u8> = vec![0x28, 0xB5, 0x2F, 0xFD, 0x00, 0x01, 0x02, 0x03];

    let resp = client
        .post(format!("{proxy_base_url}/v1/responses"))
        .header("content-type", "application/json")
        .body(garbage)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["error"]["type"], "invalid_request_error");
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Invalid JSON"),
        "expected structured invalid-JSON error, got {json}"
    );
}
