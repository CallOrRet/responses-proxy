use crate::types::chat::{self, Completion, MessageRequest};
use crate::types::item::{Compaction, OutputContentBlock, OutputItem, OutputMessage};
use crate::types::responses::{self, CompactedResponse, Error, Request};
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};

/// POST /v1/responses/compact — summarize conversation history.
///
/// Loads the full conversation chain from `state.store()` (keyed by
/// `previous_response_id`), appends the summary prompt (from
/// `state.prompts().get("summary")`) as a user message, sends everything to
/// the upstream Chat API, and returns the model response wrapped in a
/// compaction output item.
pub async fn compact(
    State(state): State<crate::app::State>,
    Json(req): Json<Request>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // Walk the stored continuation chain so compaction sees the same replay
    // history that previous_response_id continuation would use.
    let mut messages: Vec<MessageRequest> = match req.previous_response_id {
        Some(ref pid) => state
            .store()
            .get(pid)
            .await
            .unwrap_or(Vec::with_capacity(req.input.len())),
        None => vec![],
    };

    // Append the summary prompt as a user message
    let summary_prompt = state.prompts().get("summary");
    messages.push(chat::MessageRequest::User(chat::UserMessage {
        content: chat::UserContent::Parts(vec![chat::ContentPart::Text {
            text: summary_prompt,
        }]),
        name: None,
    }));

    // Look up the model provider
    let provider = state.config().models.get(&req.model).ok_or_else(|| {
        let err = Error::invalid_request(format!("Unknown model: {}", req.model));
        (StatusCode::BAD_REQUEST, Json(err.to_http_json()))
    })?;

    // Build upstream request (non-streaming, no tools, reasoning disabled)
    let upstream_req = chat::Request {
        model: provider.model.clone(),
        messages,
        max_tokens: Some(33000),
        ..Default::default()
    };

    // Send to upstream Chat API (apply chat-out rewrite if configured)
    let url = format!("{}/chat/completions", provider.base_url);
    let request = state
        .http_client()
        .post(&url)
        .timeout(provider.timeout)
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("Content-Type", "application/json");
    let request = if provider.rewrite.chat_out.is_empty() {
        request.json(&upstream_req)
    } else {
        let mut body = serde_json::to_value(&upstream_req).map_err(|e| {
            let err = Error::server_error(e.to_string());
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err.to_http_json()))
        })?;
        crate::rewrite::apply_rewrite(&mut body, &provider.rewrite.chat_out).map_err(|msg| {
            let err = Error::server_error(msg);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err.to_http_json()))
        })?;
        request.json(&body)
    };
    let response = request.send().await.map_err(|e| {
        let err = Error::server_error(e.to_string());
        (StatusCode::BAD_GATEWAY, Json(err.to_http_json()))
    })?;

    let status = response.status();
    let body_text = response.text().await.map_err(|e| {
        let err = Error::server_error(e.to_string());
        (StatusCode::BAD_GATEWAY, Json(err.to_http_json()))
    })?;

    if !status.is_success() {
        let err = Error::server_error(format!("Upstream returned {status}: {body_text}"));
        return Err((StatusCode::BAD_GATEWAY, Json(err.to_http_json())));
    }

    // Apply chat-in rewrite if configured
    let chat_resp: Completion = if provider.rewrite.chat_in.is_empty() {
        serde_json::from_str(&body_text)
    } else {
        let mut resp_value: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
            let err = Error::server_error(e.to_string());
            (StatusCode::BAD_GATEWAY, Json(err.to_http_json()))
        })?;
        crate::rewrite::apply_rewrite(&mut resp_value, &provider.rewrite.chat_in).map_err(
            |msg| {
                let err = Error::server_error(msg);
                (StatusCode::BAD_GATEWAY, Json(err.to_http_json()))
            },
        )?;
        serde_json::from_value(resp_value)
    }
    .map_err(|e| {
        let err = Error::server_error(e.to_string());
        (StatusCode::BAD_GATEWAY, Json(err.to_http_json()))
    })?;

    // Extract the summary text from the upstream response
    let summary_text = chat_resp
        .choices
        .first()
        .and_then(|c| c.message.content.as_deref())
        .unwrap_or("");

    let usage = match chat_resp.usage {
        Some(u) => responses::Usage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
            input_tokens_details: responses::InputTokensDetails { cached_tokens: 0 },
            output_tokens_details: responses::OutputTokensDetails {
                reasoning_tokens: 0,
            },
        },
        None => responses::Usage {
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            input_tokens_details: responses::InputTokensDetails { cached_tokens: 0 },
            output_tokens_details: responses::OutputTokensDetails {
                reasoning_tokens: 0,
            },
        },
    };

    let compaction_id = format!("comp_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let msg_id = format!("msg_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));

    // Wrap the summary as a message inside the compaction output item
    let output = vec![OutputItem::Compaction(Compaction {
        id: Some(compaction_id),
        encrypted_content: None,
        status: Some("completed".into()),
        output: vec![OutputItem::Message(OutputMessage {
            id: msg_id,
            role: "assistant".into(),
            status: "completed".into(),
            content: vec![OutputContentBlock::Text {
                text: summary_text.to_string(),
                annotations: vec![],
                logprobs: None,
            }],
            phase: None,
        })],
        created_by: None,
    })];

    Ok(Json(CompactedResponse {
        id: format!("rcmp_{}", uuid::Uuid::new_v4().to_string().replace('-', "")),
        object: "response.compaction".into(),
        created_at: chat_resp.created,
        output,
        usage,
    }))
}
