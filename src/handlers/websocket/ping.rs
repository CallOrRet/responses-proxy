//! Handler for the `ping` WebSocket event — keepalive.

use axum::extract::ws::WebSocket;

/// Respond with a `pong` frame.
pub(super) async fn handle(socket: &mut WebSocket) {
    super::send(socket, r#"{"type":"pong"}"#).await;
}
