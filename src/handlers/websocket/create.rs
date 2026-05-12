//! Handler for the `response.create` WebSocket event.

use crate::convert::responses_to_chat;
use crate::types::MessageRole;
use crate::types::event;
use crate::types::item::{
    FunctionCall, InputContentBlock, InputItem, InputMessage, Reasoning as InputReasoning,
};
use crate::types::responses::{Error, Request, Response, ResponseStatus};
use crate::types::streaming::*;
use crate::types::websocket;
use axum::extract::ws::{Message as WsMsg, WebSocket};
use futures::StreamExt;

/// Build a minimal Response for lifecycle events.  Full output / usage are populated later.
fn ws_response(rid: &str, model: &str, now: i64, status: ResponseStatus) -> Response {
    Response {
        id: rid.to_string(),
        model: model.to_string(),
        status,
        created_at: now,
        parallel_tool_calls: true,
        ..Default::default()
    }
}

/// Handle a `response.create` event: parse, forward to upstream, stream back results.
pub(super) async fn handle(
    state: &crate::app::State,
    socket: &mut WebSocket,
    mut req: Request,
    history: &mut Vec<InputItem>,
    last_rid: &mut Option<String>,
) {
    let model = req.model.clone();

    // Merge new input items with restored history
    let new_items = std::mem::take(&mut req.input);

    // Restore previous history if previous_response_id is provided
    let prev_id = req.previous_response_id.clone();
    let prev_len = if let Some(ref pid) = prev_id {
        if let Some(existing) = state.history().get(pid).await {
            let len = existing.len();
            *history = existing;
            len
        } else {
            // Doc: store=false + uncached → previous_response_not_found
            if !req.store {
                let ws_err = websocket::ErrorEvent::with_param(
                    400,
                    Error::TYPE_INVALID_REQUEST,
                    "previous_response_not_found",
                    format!("Previous response with id '{pid}' not found."),
                    "previous_response_id",
                );
                super::send(socket, &ws_err.to_json_string()).await;
                return;
            }
            history.clear();
            0
        }
    } else {
        history.clear();
        0
    };

    history.extend(new_items);
    req.input = history.clone();

    let rid = format!("resp_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let mid = format!("msg_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    *last_rid = Some(rid.clone());

    let provider = match state.config().models.get(&model) {
        Some(p) => p.clone(),
        None => {
            let ws_err = websocket::ErrorEvent::new(
                400,
                Error::TYPE_INVALID_REQUEST,
                "model_not_found",
                format!("Unknown model: {model}"),
            );
            super::send(socket, &ws_err.to_json_string()).await;
            return;
        }
    };

    // If generate=false, just echo lifecycle events without calling upstream
    let generate = req.generate;
    if !generate {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let resp_in_progress = ws_response(&rid, &model, now, ResponseStatus::InProgress);
        let resp_completed = ws_response(&rid, &model, now, ResponseStatus::Completed);

        let created = serde_json::to_string(&event::Created {
            response: resp_in_progress.clone(),
            sequence_number: 0,
        })
        .unwrap_or_default();
        super::send(socket, &created).await;

        let in_progress = serde_json::to_string(&event::InProgress {
            response: resp_in_progress,
            sequence_number: 1,
        })
        .unwrap_or_default();
        super::send(socket, &in_progress).await;

        let completed = serde_json::to_string(&event::Completed {
            response: resp_completed,
            sequence_number: 2,
        })
        .unwrap_or_default();
        super::send(socket, &completed).await;

        let delta = history[prev_len..].to_vec();
        state.history().set(rid, delta, prev_id).await;
        return;
    }

    // Convert and forward to upstream Chat API
    let mut chat_req = responses_to_chat(req, state);
    chat_req.model = provider.model.clone();

    let url = format!("{}/chat/completions", provider.base_url);
    let stream_resp = match state
        .http_client()
        .post(&url)
        .timeout(provider.timeout)
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("Content-Type", "application/json")
        .json(&chat_req)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let ws_err = websocket::ErrorEvent::new(
                502,
                Error::TYPE_SERVER_ERROR,
                Error::CODE_SERVER_ERROR,
                format!("Upstream error: {e}"),
            );
            super::send(socket, &ws_err.to_json_string()).await;
            return;
        }
    };

    if !stream_resp.status().is_success() {
        let status_code = stream_resp.status().as_u16();
        let ws_err = websocket::ErrorEvent::new(
            status_code,
            Error::TYPE_SERVER_ERROR,
            Error::CODE_SERVER_ERROR,
            format!("Upstream returned {status_code}"),
        );
        super::send(socket, &ws_err.to_json_string()).await;
        // Doc: evict previous_response_id from cache on failure
        if let Some(ref pid) = prev_id {
            state.history().remove(pid).await;
        }
        return;
    }

    // Send lifecycle start events (typed, with sequence_number)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let resp = ws_response(&rid, &model, now, ResponseStatus::InProgress);
    super::send(
        socket,
        &serde_json::to_string(&event::Created {
            response: resp.clone(),
            sequence_number: 0,
        })
        .unwrap_or_default(),
    )
    .await;
    super::send(
        socket,
        &serde_json::to_string(&event::InProgress {
            response: resp,
            sequence_number: 1,
        })
        .unwrap_or_default(),
    )
    .await;

    // Stream loop: read SSE chunks + handle cancel
    let (ss, cancelled) = run_stream(socket, stream_resp, &rid, &mid, &model).await;

    // Persist accumulated history
    if !cancelled {
        if !ss.reasoning_content.is_empty() {
            history.push(InputItem::Reasoning(InputReasoning {
                id: format!("rs_{}", uuid::Uuid::new_v4().to_string().replace('-', "")),
                summary: vec![],
                content: None,
                encrypted_content: None,
                status: Some("completed".into()),
            }));
        }

        history.push(InputItem::Message(InputMessage {
            role: MessageRole::Assistant,
            content: if ss.accumulated_text.is_empty() {
                vec![]
            } else {
                vec![InputContentBlock::Text {
                    text: ss.accumulated_text.clone(),
                }]
            },
            status: None,
        }));

        for tc in &ss.tool_calls {
            if !tc.id.is_empty() {
                history.push(InputItem::FunctionCall(FunctionCall {
                    call_id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                    id: None,
                    namespace: None,
                    status: Some("completed".into()),
                }));
            }
        }

        let delta = history[prev_len..].to_vec();
        state.history().set(rid, delta, prev_id).await;
        tracing::info!(history_items = history.len(), "WS: stored history");
    }
}

/// Relay SSE chunks from upstream to WebSocket, with cancel detection.
async fn run_stream(
    socket: &mut WebSocket,
    stream_resp: reqwest::Response,
    rid: &str,
    mid: &str,
    model: &str,
) -> (StreamState, bool) {
    let mut buf = String::new();
    let mut ss = StreamState::new(rid.to_string(), mid.to_string(), model.to_string());
    let mut seq: u64 = 2;
    let mut byte_stream = stream_resp.bytes_stream();
    let mut cancelled = false;

    loop {
        tokio::select! {
            chunk = byte_stream.next() => {
                match chunk {
                    Some(Ok(b)) => {
                        buf.push_str(&String::from_utf8_lossy(&b));
                        while let Some(pos) = buf.find("\n\n") {
                            let ev = buf[..pos].trim().to_string();
                            buf = buf[pos + 2..].to_string();
                            if let Some(data) = ev.lines()
                                .find(|l| l.starts_with("data:"))
                                .and_then(|l| l.strip_prefix("data:").map(|s| s.trim()))
                            {
                                tracing::trace!(%data, "Chat API delta");
                                if let Some(events) = process_chunk(&mut ss, data) {
                                    for evt in events {
                                        let mut j = serde_json::to_value(&evt).unwrap();
                                        let et_name = j["type"].as_str().unwrap_or("").to_string();
                                        j["sequence_number"] = serde_json::json!(seq);
                                        seq += 1;
                                        if !et_name.ends_with("delta") {
                                            tracing::info!("WS event: {et_name}");
                                            tracing::debug!("WS event details: {}", j);
                                        }
                                        let msg = j.to_string();
                                        tracing::debug!("WS send: {msg}");
                                        if socket.send(WsMsg::Text(msg.into())).await.is_err() {
                                            tracing::info!("WS send failed");
                                            return (ss, false);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => break,
                }
            }
            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(WsMsg::Text(t)))
                        if t.trim() == r#"{"type":"response.cancel"}"# =>
                    {
                        tracing::info!("WS cancel received during streaming");
                        cancelled = true;
                        break;
                    }
                    _ => break,
                }
            }
        }
    }
    (ss, cancelled)
}
