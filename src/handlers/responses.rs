use crate::convert::{chat_to_responses, responses_to_chat};
use crate::types::chat::{self, Completion as ChatCompletionResponse};
use crate::types::responses::{Error, Request as ResponsesRequest};
use crate::types::streaming::*;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{
        IntoResponse, Response, Sse,
        sse::{Event as SseEvent, KeepAlive},
    },
};
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

/// POST /v1/responses — handles both streaming and non-streaming responses.
pub async fn responses(
    State(state): State<crate::app::State>,
    Json(req): Json<ResponsesRequest>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    // Validate against doc-specified constraints
    crate::validation::validate_responses_request(&req)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(e.to_json())))?;

    let model = req.model.clone();
    let provider = state.config().models.get(&model).ok_or_else(|| {
        let err = Error {
            r#type: Some(Error::TYPE_INVALID_REQUEST.into()),
            code: Error::CODE_SERVER_ERROR.into(),
            message: format!(
                "Unknown model: {model}. Available: {:?}",
                state.config().models.keys()
            ),
            param: None,
        };
        (StatusCode::BAD_REQUEST, Json(err.to_http_json()))
    })?;

    let is_stream = false;

    let mut chat_req = responses_to_chat(req, &state);
    chat_req.model = provider.model.clone();

    let endpoint = format!("{}/chat/completions", provider.base_url);
    tracing::info!(
        model = %model,
        upstream = %provider.model,
        messages = chat_req.messages.len(),
        stream = is_stream,
        endpoint = %endpoint,
        "Forwarding request"
    );

    if is_stream {
        handle_streaming(state.http_client(), provider, chat_req, model)
            .await
            .map(|s| s.into_response())
    } else {
        handle_non_streaming(state.http_client(), provider, chat_req, model)
            .await
            .map(|j| j.into_response())
    }
}

// ── Non-streaming handler ────────────────────────────────────────────────

async fn handle_non_streaming(
    client: &reqwest::Client,
    provider: &crate::config::ResolvedProvider,
    chat_req: chat::Request,
    model: String,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let url = format!("{}/chat/completions", provider.base_url);

    let response = client
        .post(&url)
        .timeout(provider.timeout)
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("Content-Type", "application/json")
        .json(&chat_req)
        .send()
        .await
        .map_err(|e| {
            let err = Error {
                r#type: Some(Error::TYPE_SERVER_ERROR.into()),
                code: Error::CODE_SERVER_ERROR.into(),
                message: e.to_string(),
                param: None,
            };
            (StatusCode::BAD_GATEWAY, Json(err.to_http_json()))
        })?;

    let status = response.status();
    let body = response.text().await.map_err(|e| {
        let err = Error {
            r#type: Some(Error::TYPE_SERVER_ERROR.into()),
            code: Error::CODE_SERVER_ERROR.into(),
            message: e.to_string(),
            param: None,
        };
        (StatusCode::BAD_GATEWAY, Json(err.to_http_json()))
    })?;

    if !status.is_success() {
        let err = Error {
            r#type: Some(Error::TYPE_SERVER_ERROR.into()),
            code: Error::CODE_SERVER_ERROR.into(),
            message: format!("Upstream returned {}: {}", status.as_u16(), body),
            param: None,
        };
        return Err((StatusCode::BAD_GATEWAY, Json(err.to_http_json())));
    }

    let chat_resp: ChatCompletionResponse = serde_json::from_str(&body).map_err(|e| {
        let err = Error {
            r#type: Some(Error::TYPE_SERVER_ERROR.into()),
            code: Error::CODE_SERVER_ERROR.into(),
            message: e.to_string(),
            param: None,
        };
        (StatusCode::BAD_GATEWAY, Json(err.to_http_json()))
    })?;

    Ok(Json(chat_to_responses(chat_resp, model)))
}

// ── Streaming (SSE) handler ──────────────────────────────────────────────

async fn handle_streaming(
    client: &reqwest::Client,
    provider: &crate::config::ResolvedProvider,
    chat_req: chat::Request,
    model: String,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let url = format!("{}/chat/completions", provider.base_url);

    let response = client
        .post(&url)
        .timeout(provider.timeout)
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("Content-Type", "application/json")
        .json(&chat_req)
        .send()
        .await
        .map_err(|e| {
            let err = Error {
                r#type: Some(Error::TYPE_SERVER_ERROR.into()),
                code: Error::CODE_SERVER_ERROR.into(),
                message: e.to_string(),
                param: None,
            };
            (StatusCode::BAD_GATEWAY, Json(err.to_http_json()))
        })?;

    if !response.status().is_success() {
        let s = response.status();
        let b = response.text().await.unwrap_or_default();
        let err = Error {
            r#type: Some(Error::TYPE_SERVER_ERROR.into()),
            code: Error::CODE_SERVER_ERROR.into(),
            message: format!("Upstream returned {}: {}", s.as_u16(), b),
            param: None,
        };
        return Err((StatusCode::BAD_GATEWAY, Json(err.to_http_json())));
    }

    // SSE → mpsc channel so we can stream to the client
    let (tx, rx) = mpsc::channel::<Result<SseEvent, std::convert::Infallible>>(64);
    let rid = format!("resp_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let mid = format!("msg_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));

    let mut bytes = response.bytes_stream();

    tokio::spawn(async move {
        let mut buf = String::new();
        let mut ss = StreamState::new(rid.clone(), mid.clone(), model.clone());
        let mut seq: u64 = 0;

        while let Some(Ok(b)) = bytes.next().await {
            buf.push_str(&String::from_utf8_lossy(&b));

            // Parse SSE events (delimited by \n\n)
            while let Some(pos) = buf.find("\n\n") {
                let event = buf[..pos].trim().to_string();
                buf = buf[pos + 2..].to_string();

                if let Some(data) = event
                    .lines()
                    .find(|l| l.starts_with("data:"))
                    .and_then(|l| l.strip_prefix("data:").map(|s| s.trim()))
                {
                    tracing::trace!(%data, "Chat API delta");

                    if let Some(events) = process_chunk(&mut ss, data) {
                        for ev in events {
                            let mut j = serde_json::to_value(&ev).unwrap();
                            j["sequence_number"] = serde_json::json!(seq);
                            seq += 1;

                            if tx
                                .send(Ok(SseEvent::default().json_data(j).unwrap()))
                                .await
                                .is_err()
                            {
                                return; // receiver dropped
                            }
                        }
                    }
                }
            }
        }
    });

    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default()))
}
