use axum::http::HeaderMap;
use axum::{Json, extract::State, http::StatusCode, middleware::Next, response::Response};

/// Axum middleware: checks `Authorization: Bearer <key>` against configured keys.
/// If no auth keys are configured, all requests pass through.
pub async fn check(
    State(state): State<crate::app::State>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    // No keys configured → skip auth
    if !state.config().auth_enabled() {
        return Ok(next.run(request).await);
    }

    let valid = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|key| state.config().auth_keys.contains(key));

    if valid {
        Ok(next.run(request).await)
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": {
                    "type": "authentication_error",
                    "message": "Invalid or missing API key"
                }
            })),
        ))
    }
}
