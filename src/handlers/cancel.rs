use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};

/// Cancel an in-progress response by removing its history chain.
pub async fn cancel(
    State(state): State<crate::app::State>,
    axum::extract::Path(response_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    tracing::info!(response_id=%response_id, "Cancel request");

    state.history().remove(&response_id).await;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    Ok(Json(serde_json::json!({
        "id": response_id,
        "object": "response",
        "created_at": now,
        "status": "cancelled",
        "model": "",
        "output": []
    })))
}
