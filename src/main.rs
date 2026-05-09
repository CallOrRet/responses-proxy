mod convert_request;
mod convert_response;
mod models;
mod streaming;

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event as SseEvent, KeepAlive},
        IntoResponse, Response, Sse,
    },
    routing::{get, post},
    Json, Router,
};
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
    downstream_url: String,
    api_key: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "responses_proxy=info".into()),
        )
        .init();

    let downstream_url = std::env::var("DOWNSTREAM_URL")
        .unwrap_or_else(|_| "https://api.deepseek.com".into());
    let api_key = std::env::var("DOWNSTREAM_API_KEY")
        .expect("DOWNSTREAM_API_KEY environment variable must be set");
    let listen_addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".into());
    let request_timeout = std::env::var("REQUEST_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(120));

    let state = Arc::new(AppState {
        http_client: reqwest::Client::builder()
            .timeout(request_timeout)
            .build()
            .expect("Failed to build HTTP client"),
        downstream_url,
        api_key,
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/v1/responses", post(handle_responses))
        .layer(cors)
        .with_state(state);

    tracing::info!("Listening on {}", listen_addr);

    let listener = tokio::net::TcpListener::bind(&listen_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> &'static str {
    "OK"
}

async fn handle_responses(
    State(state): State<Arc<AppState>>,
    _headers: HeaderMap,
    body: Bytes,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
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
    let is_stream = responses_req.stream.unwrap_or(false);
    let chat_req = responses_to_chat(responses_req);
    let model_name = chat_req.model.clone();

    tracing::info!(
        model = %model_name,
        messages = chat_req.messages.len(),
        stream = is_stream,
        "Forwarding request"
    );

    if is_stream {
        handle_streaming(state, chat_req, original_model).await.map(|sse| sse.into_response())
    } else {
        handle_non_streaming(state, chat_req, original_model).await.map(|json| json.into_response())
    }
}

async fn handle_non_streaming(
    state: Arc<AppState>,
    chat_req: ChatCompletionRequest,
    original_model: String,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let url = format!("{}/chat/completions", state.downstream_url);

    let response = state
        .http_client
        .post(&url)
        .header("Authorization", format!("Bearer {}", state.api_key))
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

    let responses_resp = chat_to_responses(chat_resp, original_model);
    Ok(Json(responses_resp))
}

async fn handle_streaming(
    state: Arc<AppState>,
    chat_req: ChatCompletionRequest,
    original_model: String,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let url = format!("{}/chat/completions", state.downstream_url);

    let response = state
        .http_client
        .post(&url)
        .header("Authorization", format!("Bearer {}", state.api_key))
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

                        if let Some(data) = data_line {
                            if let Some(events) =
                                streaming::process_chunk(&mut stream_state, data)
                            {
                                for event in events {
                                    let sse_event = SseEvent::default()
                                        .json_data(event.to_sse_json())
                                        .unwrap();
                                    if tx.send(Ok(sse_event)).await.is_err() {
                                        return; // client disconnected
                                    }
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
