//! WebSocket types for the bidirectional Responses API (§6).
//!
//! - [`ClientEvent`] — client-to-server frames (§6.2)
//! - [`ErrorEvent`] — server-to-client error frames (§6.6)

use serde::{Deserialize, Serialize};

use super::responses::{Error, Request};

// ── ClientEvent (§6.2) ──────────────────────────────────────────────────

/// Client-to-server WebSocket event.  Tagged on `type` for single-pass
/// deserialisation.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum ClientEvent {
    /// Create a new response.  Payload mirrors [`Request`].
    #[serde(rename = "response.create")]
    ResponseCreate(Request),
    /// Cancel the in-flight response.
    #[serde(rename = "response.cancel")]
    ResponseCancel,
    /// Heartbeat ping.
    #[serde(rename = "ping")]
    Ping,
}

// ── ErrorEvent (§6.6) ───────────────────────────────────────────────────

/// WebSocket error frame — wraps [`Error`] with `type: "error"` and an
/// HTTP status code.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ErrorEvent {
    /// Always `"error"`.
    #[serde(rename = "type")]
    pub type_: String,
    /// HTTP status code.
    pub status: u16,
    /// Inner error — same shape as HTTP error responses (§8).
    pub error: Error,
}

impl ErrorEvent {
    pub fn new(status: u16, error_type: &str, code: &str, message: String) -> Self {
        Self {
            type_: "error".into(),
            status,
            error: Error {
                code: code.into(),
                message,
                r#type: Some(error_type.into()),
                param: None,
            },
        }
    }

    pub fn with_param(
        status: u16,
        error_type: &str,
        code: &str,
        message: String,
        param: &str,
    ) -> Self {
        Self {
            type_: "error".into(),
            status,
            error: Error {
                code: code.into(),
                message,
                r#type: Some(error_type.into()),
                param: Some(param.into()),
            },
        }
    }

    /// Serialise to JSON string for sending over the WebSocket.
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_response_create_minimal() {
        let json = r#"{"type":"response.create","model":"gpt-5.2"}"#;
        let ev: ClientEvent = serde_json::from_str(json).unwrap();
        match ev {
            ClientEvent::ResponseCreate(req) => {
                assert_eq!(req.model, "gpt-5.2");
                assert!(req.generate);
            }
            _ => panic!("expected ResponseCreate"),
        }
    }

    #[test]
    fn ws_ping() {
        let ev: ClientEvent = serde_json::from_str(r#"{"type":"ping"}"#).unwrap();
        assert!(matches!(ev, ClientEvent::Ping));
    }

    #[test]
    fn ws_cancel() {
        let ev: ClientEvent = serde_json::from_str(r#"{"type":"response.cancel"}"#).unwrap();
        assert!(matches!(ev, ClientEvent::ResponseCancel));
    }

    #[test]
    fn ws_error_event_format() {
        let err = ErrorEvent::with_param(
            400,
            Error::TYPE_INVALID_REQUEST,
            "previous_response_not_found",
            "not found".into(),
            "previous_response_id",
        );
        let json = err.to_json_string();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "error");
        assert_eq!(v["status"], 400);
        assert_eq!(v["error"]["code"], "previous_response_not_found");
        assert_eq!(v["error"]["param"], "previous_response_id");
    }
}
