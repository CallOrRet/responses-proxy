//! WebSocket handler for the bidirectional Responses API.
//!
//! Submodules — one per event type:
//! - [`create`] — `response.create`
//! - [`cancel`] — `response.cancel`
//! - [`ping`]   — `ping` / `pong` keepalive

mod create;
mod ping;

use crate::types::responses::Error;
use crate::types::websocket::{ClientEvent, ErrorEvent};
use axum::extract::{
    State,
    ws::{Message as WsMsg, WebSocket, WebSocketUpgrade},
};
use axum::response::IntoResponse;
use std::collections::VecDeque;

/// WebSocket upgrade handler for the bidirectional Responses API.
pub async fn websocket(
    State(state): State<crate::app::State>,
    headers: axum::http::HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // Optional per-instance store namespace, taken from the upgrade request and
    // applied to every turn on this connection.
    let ns = super::namespace_from_headers(&headers);
    ws.on_upgrade(move |s| run(s, state, ns))
}

/// Send a text frame with debug logging.
pub(super) async fn send(socket: &mut WebSocket, text: &str) {
    tracing::debug!("WS send: {text}");
    let _ = socket.send(WsMsg::Text(text.into())).await;
}

// ── Main event loop ────────────────────────────────────────────────────────

/// Receive loop — dispatches incoming events to their handlers.
///
/// `pending` holds messages that arrived while a previous `response.create`
/// was still streaming (see `create::run_stream`) — multi-agent mode can push
/// a new turn for another agent while one is in flight on this same
/// connection, and those must be dispatched, not lost. Drained before reading
/// fresh messages off the socket, so order is preserved.
async fn run(mut socket: WebSocket, state: crate::app::State, ns: String) {
    tracing::info!("WebSocket connection established");

    let mut pending: VecDeque<WsMsg> = VecDeque::new();

    loop {
        let msg = match pending.pop_front() {
            Some(m) => m,
            None => match socket.recv().await {
                Some(Ok(m)) => m,
                _ => break,
            },
        };

        let text = match msg {
            WsMsg::Text(t) => t.to_string(),
            WsMsg::Close(_) => {
                tracing::info!("WebSocket client sent close frame");
                break;
            }
            _ => continue,
        };

        tracing::debug!("WS recv: {text}");

        let event: ClientEvent = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!("WS invalid JSON: {e}");
                let ws_err = ErrorEvent::new(
                    400,
                    Error::TYPE_INVALID_REQUEST,
                    "invalid_json",
                    e.to_string(),
                );
                send(&mut socket, &ws_err.to_json_string()).await;
                continue;
            }
        };

        match event {
            ClientEvent::ResponseCreate(req) => {
                tracing::info!("WS received event: response.create");
                let deferred = create::handle(&state, &mut socket, req, &ns).await;
                pending.extend(deferred);
            }

            ClientEvent::ResponseCancel => {
                tracing::info!("WS received event: response.cancel");
            }

            ClientEvent::Ping => {
                ping::handle(&mut socket).await;
            }
        }
    }

    tracing::info!("WebSocket connection closed");
}
