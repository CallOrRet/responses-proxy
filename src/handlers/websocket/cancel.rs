//! Handler for the `response.cancel` WebSocket event.

/// Clear conversation history.
pub(super) async fn handle(
    state: &crate::app::State,
    history: &mut Vec<crate::types::item::InputItem>,
    last_rid: &mut Option<String>,
) {
    tracing::info!("WS cancel request received, clearing history");
    if let Some(rid) = last_rid.as_deref() {
        state.history().remove(rid).await;
    }
    *last_rid = None;
    history.clear();
}
