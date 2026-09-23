//! HTTP and WebSocket request handlers.

mod auth;
mod cancel;
mod compact;
mod input_tokens;
mod json;
mod responses;
mod websocket;

pub use auth::check;
pub use cancel::cancel;
pub use compact::compact;
pub(crate) use input_tokens::apply_input_char_scale;
pub use input_tokens::input_tokens;
pub(crate) use responses::compaction_output_to_chat_messages;
pub(crate) use responses::response_to_stream_events;
pub use responses::responses;
pub use websocket::websocket;

/// Header a client sets (per Codex profile via `[model_providers.<id>]
/// .http_headers`) to isolate its response-store namespace from other parallel
/// instances. Absent → the shared default namespace (unchanged behavior).
pub(crate) const NAMESPACE_HEADER: &str = "x-responses-proxy-namespace";

/// Extract and sanitize the store namespace from request headers. Returns an
/// empty string when the header is absent, so callers get today's behavior.
pub(crate) fn namespace_from_headers(headers: &axum::http::HeaderMap) -> String {
    headers
        .get(NAMESPACE_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}
