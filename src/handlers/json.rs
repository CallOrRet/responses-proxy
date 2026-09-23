//! Request-body JSON extractor with safe parse-error logging.
//!
//! Unlike `axum::Json`, this extractor never logs raw request bytes on a parse
//! failure. Codex sends compressed bodies (zstd/gzip); if decompression is
//! missing or the body is otherwise malformed, logging the raw buffer produces
//! binary mojibake. Instead we log a lossy, length-capped UTF-8 preview and a
//! byte count, and return a structured OpenAI-style 400.

use axum::{
    Json,
    extract::{FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use serde::de::DeserializeOwned;

use crate::types::responses::Error;

/// Maximum number of characters from the body included in a parse-error log.
const BODY_PREVIEW_CHARS: usize = 200;

/// JSON extractor that logs safe previews instead of raw bytes on failure.
pub struct ResponsesJson<T>(pub T);

impl<T, S> FromRequest<S> for ResponsesJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(req, state).await.map_err(|e| {
            let err = Error::invalid_request(format!("Failed to read request body: {e}"));
            (StatusCode::BAD_REQUEST, Json(err.to_http_json())).into_response()
        })?;

        match serde_json::from_slice::<T>(&bytes) {
            Ok(value) => Ok(ResponsesJson(value)),
            Err(e) => {
                let body_len = bytes.len();
                let preview: String = String::from_utf8_lossy(&bytes)
                    .chars()
                    .take(BODY_PREVIEW_CHARS)
                    .collect();
                tracing::warn!(
                    error = %e,
                    body_len,
                    body_preview = %preview,
                    "Failed to parse request body as JSON"
                );
                let err = Error::invalid_request(format!("Invalid JSON request body: {e}"));
                Err((StatusCode::BAD_REQUEST, Json(err.to_http_json())).into_response())
            }
        }
    }
}
