mod config;
mod convert_request;
mod convert_response;
mod models;
mod streaming;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{
        State,
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event as SseEvent, KeepAlive},
    },
    routing::get,
};
use config::ResolvedConfig;
use convert_request::responses_to_chat;
use convert_response::chat_to_responses;
use futures::StreamExt;
use models::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tower_http::cors::{Any, CorsLayer};

struct AppState {
    http_client: reqwest::Client,
    config: ResolvedConfig,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "responses_proxy=info".into()),
        )
        .init();

    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.yaml".into());
    let resolved = config::load_config(&config_path).expect("Failed to load config");

    tracing::info!(
        "Loaded {} models from {}",
        resolved.model_names.len(),
        config_path
    );

    let state = Arc::new(AppState {
        http_client: reqwest::Client::builder()
            .timeout(Duration::from_secs(resolved.request_timeout))
            .build()
            .expect("Failed to build HTTP client"),
        config: resolved,
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let listen_addr = state.config.listen_addr.clone();
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/v1/models", get(list_models))
        .route("/v1/responses", get(handle_ws).post(handle_responses))
        .layer(cors)
        .with_state(state);

    tracing::info!("Listening on {}", listen_addr);

    let listener = tokio::net::TcpListener::bind(&listen_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> &'static str {
    "OK"
}

async fn handle_ws(State(state): State<Arc<AppState>>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_session(socket, state))
}

/// Accumulated session state for WebSocket turns.
struct WsSession {
    /// Full accumulated input items across turns.
    history: Vec<serde_json::Value>,
    /// Last response ID for continuation.
    last_response_id: Option<String>,
}

async fn handle_ws_session(mut socket: WebSocket, state: Arc<AppState>) {
    tracing::debug!("WebSocket connection established");

    let mut session = WsSession {
        history: Vec::new(),
        last_response_id: None,
    };

    while let Some(Ok(msg)) = socket.recv().await {
        let text = match msg {
            WsMessage::Text(t) => t.to_string(),
            WsMessage::Close(_) => {
                tracing::debug!("WebSocket client sent close frame");
                break;
            }
            _ => continue,
        };

        tracing::debug!("WS raw message: {}", text);

        let event: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!("WebSocket invalid JSON: {e}");
                continue;
            }
        };

        let event_type = event["type"].as_str().unwrap_or("");

        tracing::debug!("WS received event: {event_type}");

        match event_type {
            "response.create" => {
                let model = event["model"]
                    .as_str()
                    .unwrap_or("deepseek-v4-pro")
                    .to_string();
                let generate = event["generate"].as_bool().unwrap_or(true);
                let previous_id = event["previous_response_id"].as_str();

                // Handle continuation: if previous_response_id matches last turn,
                // only append new input items to history.
                let input_items = event.get("input").cloned().unwrap_or_default();
                let input_array = if input_items.is_array() {
                    input_items.as_array().cloned().unwrap_or_default()
                } else if input_items.is_string() {
                    vec![
                        serde_json::json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": input_items.as_str().unwrap()}]}),
                    ]
                } else {
                    vec![]
                };

                if let Some(prev_id) = previous_id {
                    if session.last_response_id.as_deref() == Some(prev_id) {
                        // Continuation: append new items to history
                        session.history.extend(input_array);
                    } else {
                        // Previous response not in cache — send error
                        let _ = socket
                            .send(WsMessage::Text(
                                serde_json::json!({
                                    "type": "error",
                                    "error": {
                                        "type": "invalid_request_error",
                                        "code": "previous_response_not_found",
                                        "message": format!("Previous response with id '{prev_id}' not found."),
                                        "param": "previous_response_id"
                                    }
                                }).to_string().into()
                            )).await;
                        continue;
                    }
                } else {
                    // New turn: start fresh history
                    session.history = input_array;
                }

                // Build the full ResponsesRequest with accumulated history
                let full_input = if session.history.is_empty() {
                    Input::String(String::new())
                } else {
                    Input::Array(
                        session
                            .history
                            .iter()
                            .map(|item| {
                                serde_json::from_value(item.clone())
                                    .unwrap_or(InputItem::Unknown(item.clone()))
                            })
                            .collect(),
                    )
                };

                let responses_req = ResponsesRequest {
                    model: model.clone(),
                    input: full_input,
                    instructions: event["instructions"].as_str().map(|s| s.to_string()),
                    temperature: event["temperature"].as_f64(),
                    top_p: event["top_p"].as_f64(),
                    max_output_tokens: event["max_output_tokens"].as_u64().map(|v| v as u32),
                    tools: event
                        .get("tools")
                        .and_then(|t| serde_json::from_value(t.clone()).ok()),
                    tool_choice: event.get("tool_choice").cloned(),
                    stream: Some(true),
                    stop: None,
                    top_logprobs: event["top_logprobs"].as_u64().map(|v| v as u32),
                    previous_response_id: None,
                    store: None,
                    metadata: None,
                    reasoning: event.get("reasoning").cloned(),
                    text: None,
                };

                let msg_id = format!("msg_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
                let response_id =
                    format!("resp_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));

                // Look up provider
                let provider = match state.config.models.get(&model) {
                    Some(p) => p.clone(),
                    None => {
                        let _ = socket
                            .send(WsMessage::Text(
                                serde_json::json!({"type": "error", "error": {"message": format!("Unknown model: {model}")}}).to_string().into(),
                            )).await;
                        continue;
                    }
                };

                // If generate=false, just return the response ID for warmup
                if !generate {
                    tracing::debug!("WS generate=false, sending warmup response");
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs_f64();
                    let _ = socket
                        .send(WsMessage::Text(
                            serde_json::json!({
                                "type": "response.created",
                                "response": {
                                    "id": response_id,
                                    "object": "response",
                                    "created_at": now,
                                    "model": model,
                                    "status": "in_progress",
                                    "output": []
                                }
                            })
                            .to_string()
                            .into(),
                        ))
                        .await;
                    let _ = socket
                        .send(WsMessage::Text(
                            serde_json::json!({
                                "type": "response.completed",
                                "response": {
                                    "id": response_id,
                                    "object": "response",
                                    "created_at": now,
                                    "model": model,
                                    "status": "completed",
                                    "output": []
                                }
                            })
                            .to_string()
                            .into(),
                        ))
                        .await;
                    session.last_response_id = Some(response_id);
                    continue;
                }

                let mut chat_req =
                    responses_to_chat(responses_req, &state.config.tool_type_allowlist);

                // Replace with downstream model name
                chat_req.model = provider.downstream_model.clone();
                chat_req.stream = Some(true);

                tracing::debug!(
                    "WS Chat API request: {} messages, downstream_model={}",
                    chat_req.messages.len(),
                    chat_req.model
                );

                let url = format!("{}/chat/completions", provider.base_url);

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64();

                // Send response.created
                tracing::debug!("WS sending response.created");
                let _ = socket
                    .send(WsMessage::Text(
                        serde_json::json!({
                            "type": "response.created",
                            "response": {
                                "id": response_id,
                                "object": "response",
                                "created_at": now,
                                "model": model,
                                "status": "in_progress",
                                "output": []
                            }
                        })
                        .to_string()
                        .into(),
                    ))
                    .await;

                // Send response.in_progress
                tracing::debug!("WS sending response.in_progress");
                let _ = socket
                    .send(WsMessage::Text(
                        serde_json::json!({
                            "type": "response.in_progress",
                            "response": {
                                "id": response_id,
                                "object": "response",
                                "created_at": now,
                                "model": model,
                                "status": "in_progress",
                                "output": []
                            }
                        })
                        .to_string()
                        .into(),
                    ))
                    .await;

                // Stream from downstream
                tracing::debug!("WS starting downstream stream request to {url}");
                let stream_resp = match state
                    .http_client
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", provider.api_key))
                    .header("Content-Type", "application/json")
                    .json(&chat_req)
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::debug!("WS downstream request failed: {e}");
                        let _ = socket.send(WsMessage::Text(
                            serde_json::json!({"type": "error", "error": {"message": format!("Downstream error: {e}")}}).to_string().into()
                        )).await;
                        continue;
                    }
                };

                let http_status = stream_resp.status();
                tracing::debug!(
                    "WS downstream stream response: HTTP {}",
                    http_status.as_u16()
                );

                if !http_status.is_success() {
                    let err_body = stream_resp.text().await.unwrap_or_default();
                    tracing::debug!(
                        "WS downstream error: HTTP {}, body={:.500}",
                        http_status.as_u16(),
                        err_body
                    );
                    let _ = socket.send(WsMessage::Text(
                        serde_json::json!({
                            "type": "error",
                            "error": {
                                "type": "downstream_error",
                                "message": format!("Downstream returned {}: {}", http_status.as_u16(), err_body)
                            }
                        }).to_string().into()
                    )).await;
                    continue;
                }

                let mut byte_stream = stream_resp.bytes_stream();
                let mut buffer = String::new();
                let mut stream_state =
                    streaming::StreamState::new(response_id.clone(), msg_id.clone(), model.clone());
                // Already sent response.created + response.in_progress manually.
                stream_state.has_started = true;

                while let Some(chunk_result) = byte_stream.next().await {
                    if let Ok(bytes) = chunk_result {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(event_end) = buffer.find("\n\n") {
                            let event_str = buffer[..event_end].trim().to_string();
                            buffer = buffer[event_end + 2..].to_string();
                            let data_line = event_str
                                .lines()
                                .find(|l| l.starts_with("data:"))
                                .and_then(|l| l.strip_prefix("data:").map(|s| s.trim()));

                            if let Some(data) = data_line
                                && let Some(events) =
                                    streaming::process_chunk(&mut stream_state, data)
                            {
                                for event in events {
                                    let et = event.event_type();
                                    let json = event.to_sse_json();
                                    tracing::debug!("WS event: {et} {}", json);
                                    if socket
                                        .send(WsMessage::Text(json.to_string().into()))
                                        .await
                                        .is_err()
                                    {
                                        tracing::debug!("WS send failed, client disconnected");
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }

                tracing::debug!(
                    "WS downstream stream finished: text={}, reasoning={}, tools={}",
                    stream_state.accumulated_text.len(),
                    stream_state.reasoning_content.len(),
                    stream_state.tool_calls.len()
                );
                tracing::debug!("WS waiting for next event");

                // Store assistant output in session history for continuation.
                // Order matters: reasoning → function_calls → message.
                if !stream_state.reasoning_content.is_empty() {
                    session.history.push(serde_json::json!({
                        "type": "reasoning",
                        "id": format!("rs_{}", uuid::Uuid::new_v4().to_string().replace('-', "")),
                        "summary": [{"type": "summary_text", "text": stream_state.reasoning_content}]
                    }));
                }
                if !stream_state.tool_calls.is_empty() {
                    for tc in &stream_state.tool_calls {
                        if !tc.id.is_empty() {
                            session.history.push(serde_json::json!({
                                "type": "function_call",
                                "call_id": tc.id,
                                "name": tc.name,
                                "arguments": tc.arguments
                            }));
                        }
                    }
                }
                if !stream_state.accumulated_text.is_empty() {
                    session.history.push(serde_json::json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": stream_state.accumulated_text}]
                    }));
                }

                session.last_response_id = Some(response_id);
            }
            "ping" => {
                let _ = socket
                    .send(WsMessage::Text(r#"{"type":"pong"}"#.into()))
                    .await;
            }
            _ => {}
        }
    }
}

fn check_auth(
    config: &ResolvedConfig,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if !config.auth_enabled {
        return Ok(());
    }

    let auth_header = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    if auth_header.is_some_and(|key| config.auth_keys.contains(key)) {
        Ok(())
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": {
                    "type": "authentication_error",
                    "message": "Invalid or missing API key",
                }
            })),
        ))
    }
}

async fn list_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_auth(&state.config, &headers)?;
    let data: Vec<serde_json::Value> = state
        .config
        .model_names
        .iter()
        .map(|name| {
            serde_json::json!({
                "id": name,
                "object": "model",
                "created": 0,
                "owned_by": "responses-proxy"
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "object": "list",
        "data": data
    })))
}

async fn handle_responses(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    // Auth check
    check_auth(&state.config, &headers)?;

    tracing::debug!(
        "Received request: {}",
        String::from_utf8_lossy(&body).trim()
    );

    let responses_req: ResponsesRequest = serde_json::from_slice(&body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": {
                    "type": "invalid_request_error",
                    "message": format!("Failed to parse request: {}", e),
                }
            })),
        )
    })?;

    let original_model = responses_req.model.clone();

    // Look up the model in config to get provider details.
    let provider = state.config.models.get(&original_model).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": {
                    "type": "invalid_request_error",
                    "message": format!("Unknown model: {}. Available: {:?}", original_model, state.config.model_names),
                }
            })),
        )
    })?;

    let is_stream = responses_req.stream.unwrap_or(false);
    let mut chat_req = responses_to_chat(responses_req, &state.config.tool_type_allowlist);

    // Override the model name with the downstream model.
    chat_req.model = provider.downstream_model.clone();

    tracing::info!(
        model = %original_model,
        downstream = %provider.downstream_model,
        messages = chat_req.messages.len(),
        stream = is_stream,
        "Forwarding request"
    );

    if is_stream {
        handle_streaming(
            &state.http_client,
            &provider.base_url,
            &provider.api_key,
            chat_req,
            original_model,
        )
        .await
        .map(|sse| sse.into_response())
    } else {
        handle_non_streaming(
            &state.http_client,
            &provider.base_url,
            &provider.api_key,
            chat_req,
            original_model,
        )
        .await
        .map(|json| json.into_response())
    }
}

async fn handle_non_streaming(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    chat_req: ChatCompletionRequest,
    original_model: String,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let url = format!("{}/chat/completions", base_url);

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&chat_req)
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": {
                        "type": "proxy_error",
                        "message": format!("Failed to reach downstream: {}", e),
                    }
                })),
            )
        })?;

    let status = response.status();
    let body = response.text().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": {
                    "type": "proxy_error",
                    "message": format!("Failed to read downstream response: {}", e),
                }
            })),
        )
    })?;

    tracing::debug!(
        "Downstream response: HTTP {}, body={:.500}",
        status.as_u16(),
        body
    );

    if !status.is_success() {
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": {
                    "type": "downstream_error",
                    "message": format!("Downstream returned {}: {}", status.as_u16(), body),
                }
            })),
        ));
    }

    let chat_resp: ChatCompletionResponse = serde_json::from_str(&body).map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": {
                    "type": "proxy_error",
                    "message": format!("Failed to parse downstream response: {}", e),
                }
            })),
        )
    })?;

    tracing::debug!(
        "Downstream response: status={}, model={}, choices={}",
        status,
        chat_resp.model,
        chat_resp.choices.len(),
    );

    let responses_resp = chat_to_responses(chat_resp, original_model);
    tracing::debug!(
        "Proxy response: {}",
        serde_json::to_string(&responses_resp).unwrap_or_else(|e| format!("serialize error: {e}"))
    );
    Ok(Json(responses_resp))
}

async fn handle_streaming(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    chat_req: ChatCompletionRequest,
    original_model: String,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let url = format!("{}/chat/completions", base_url);

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&chat_req)
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": {
                        "type": "proxy_error",
                        "message": format!("Failed to reach downstream: {}", e),
                    }
                })),
            )
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": {
                    "type": "downstream_error",
                    "message": format!("Downstream returned {}: {}", status.as_u16(), body),
                }
            })),
        ));
    }

    let (tx, rx) = mpsc::channel::<Result<SseEvent, std::convert::Infallible>>(64);

    let response_id = format!("resp_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let msg_id = format!("msg_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let mut byte_stream = response.bytes_stream();
    let model = original_model.clone();

    tokio::spawn(async move {
        let mut buffer = String::new();
        let mut stream_state =
            streaming::StreamState::new(response_id.clone(), msg_id.clone(), model);

        while let Some(chunk_result) = byte_stream.next().await {
            match chunk_result {
                Ok(bytes) => {
                    let chunk_str = String::from_utf8_lossy(&bytes);
                    buffer.push_str(&chunk_str);

                    while let Some(event_end) = buffer.find("\n\n") {
                        let event_str = buffer[..event_end].trim().to_string();
                        buffer = buffer[event_end + 2..].to_string();

                        let data_line = event_str
                            .lines()
                            .find(|l| l.starts_with("data:"))
                            .and_then(|l| l.strip_prefix("data:").map(|s| s.trim()));

                        if let Some(data) = data_line
                            && let Some(events) = streaming::process_chunk(&mut stream_state, data)
                        {
                            for event in events {
                                let sse_event =
                                    SseEvent::default().json_data(event.to_sse_json()).unwrap();
                                if tx.send(Ok(sse_event)).await.is_err() {
                                    return; // client disconnected
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    let failed = serde_json::json!({
                        "type": "response.failed",
                        "response": {
                            "id": response_id,
                            "object": "response",
                            "model": original_model,
                            "status": "failed",
                            "output": []
                        }
                    });
                    let _ = tx
                        .send(Ok(SseEvent::default().json_data(failed).unwrap()))
                        .await;
                    return;
                }
            }
        }
    });

    let stream = ReceiverStream::new(rx);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
