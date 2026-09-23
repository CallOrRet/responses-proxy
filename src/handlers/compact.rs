use crate::config::ResolvedProvider;
use crate::types::chat::{self, MessageRequest};
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
    super::json::ResponsesJson(req): super::json::ResponsesJson<Request>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let provider = state.config().models.get(&req.model).ok_or_else(|| {
        let err = Error::invalid_request(format!("Unknown model: {}", req.model));
        (StatusCode::BAD_REQUEST, Json(err.to_http_json()))
    })?;

    // The standalone endpoint also carries the live history in `req.input`
    // (Codex replays it every turn under store:false), so convert it and use it
    // as the summary source.
    let current_messages = crate::convert::items_to_chat_messages(&req.input, &state);
    let (output, usage, created_at) = build_compaction_output(
        &state,
        provider,
        req.previous_response_id.as_deref(),
        current_messages,
    )
    .await?;

    Ok(Json(CompactedResponse {
        id: format!("rcmp_{}", uuid::Uuid::new_v4().to_string().replace('-', "")),
        object: "response.compaction".into(),
        created_at,
        output,
        usage,
    }))
}

/// Run a summary turn against the configured provider and wrap the result in a
/// `compaction` output item. Returns `(output, usage, created_at)` so callers
/// can package the data into either a `CompactedResponse` (for the standalone
/// `/v1/responses/compact` endpoint) or a normal `responses::Response` /
/// SSE stream (for the in-band `compaction_trigger` flow on `/v1/responses`).
///
/// `current_messages` is the live history converted from the request's `input`
/// items. Codex CLI runs with `store:false` and replays the full conversation
/// in `input` on every turn, so the request itself is the authoritative source
/// of history; the stored continuation chain (keyed by `previous_response_id`)
/// is only a fallback for clients that rely on server-side state.
pub(crate) async fn build_compaction_output(
    state: &crate::app::State,
    provider: &ResolvedProvider,
    previous_response_id: Option<&str>,
    current_messages: Vec<MessageRequest>,
) -> Result<(Vec<OutputItem>, responses::Usage, i64), (StatusCode, Json<serde_json::Value>)> {
    // Prefer the live history replayed in `input`; fall back to the stored
    // continuation chain only when the request carried no history.
    let mut messages: Vec<MessageRequest> = if !current_messages.is_empty() {
        current_messages
    } else {
        match previous_response_id {
            Some(pid) => state.store().get(pid).await.unwrap_or_default(),
            None => vec![],
        }
    };

    // Append the summary prompt as a user message
    let summary_prompt = state.prompts().get("summary");
    messages.push(chat::MessageRequest::User(chat::UserMessage {
        content: chat::UserContent::Parts(vec![chat::ContentPart::Text {
            text: summary_prompt,
        }]),
        name: None,
    }));

    // Bound the summary request the same way the main request path is bounded:
    // drop oldest turns to fit the message-count limit, then shrink old tool
    // outputs to the char budget. The summary prompt is the trailing user turn,
    // so it is always retained.
    let dropped =
        crate::convert::enforce_message_budget(&mut messages, provider.max_input_messages);
    if dropped > 0 {
        tracing::info!(
            max_messages = provider.max_input_messages,
            dropped_messages = dropped,
            "Compaction input exceeded history.max-input-messages — dropped oldest turns"
        );
    }
    if let Some(max_chars) = provider.max_input_chars {
        crate::convert::enforce_input_budget(&mut messages, max_chars);
    }

    // Build upstream request. Send it **streaming**: the summary can be up to
    // `max_tokens` long, and on a slow origin behind Cloudflare a buffered
    // (non-streaming) call blows past the ~100s origin-timeout window → 524,
    // which kills compaction and traps Codex in a retry loop. Streaming emits
    // the first byte quickly and keeps the connection alive.
    let upstream_req = chat::Request {
        model: provider.model.clone(),
        messages,
        max_tokens: Some(33000),
        stream: Some(true),
        stream_options: Some(chat::StreamOptions {
            include_usage: Some(true),
            include_obfuscation: None,
        }),
        ..Default::default()
    };

    let url = format!("{}/chat/completions", provider.base_url);
    let request =
        crate::upstream::build_typed_chat_request(state.http_client(), provider, &upstream_req)
            .map_err(|msg| {
                let err = Error::server_error(msg);
                (StatusCode::INTERNAL_SERVER_ERROR, Json(err.to_http_json()))
            })?;
    let started = std::time::Instant::now();
    let response = request.send().await.map_err(|e| {
        let err = Error::server_error(e.to_string());
        (StatusCode::BAD_GATEWAY, Json(err.to_http_json()))
    })?;

    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();
        let snippet = if body_text.len() > 200 {
            &body_text[..200]
        } else {
            &body_text
        };
        tracing::warn!(
            endpoint = %url,
            model = %provider.model,
            upstream_status = status.as_u16(),
            streaming = true,
            elapsed_ms = started.elapsed().as_millis() as u64,
            snippet = %snippet,
            "Compaction upstream request failed"
        );
        let err = Error::server_error(format!("Upstream returned {status}: {snippet}"));
        return Err((StatusCode::BAD_GATEWAY, Json(err.to_http_json())));
    }

    // Drain the SSE stream into an accumulator (client-facing events discarded).
    let ss = crate::upstream::drain_chat_stream(response, &provider.rewrite.chat_in)
        .await
        .map_err(|e| {
            tracing::warn!(
                endpoint = %url,
                model = %provider.model,
                streaming = true,
                elapsed_ms = started.elapsed().as_millis() as u64,
                error = %e,
                "Compaction upstream stream drain failed"
            );
            let err = Error::server_error(e);
            (StatusCode::BAD_GATEWAY, Json(err.to_http_json()))
        })?;

    let summary_text = ss.accumulated_text.as_str();

    let usage = match ss.usage {
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
    let created = ss.created;

    let output = build_compaction_item(state.compact_key(), summary_text);

    Ok((output, usage, created))
}

/// Build the `compaction` output item from a summary. Extracted so the id prefix
/// and the encrypted/plaintext branches are unit-testable without a live
/// upstream. The item id uses OpenAI's `cmp_` convention — the other minted ids
/// already use OpenAI's real prefixes (resp_/msg_/rs_/fc_), and a `comp_` outlier
/// here is rejected by strict Responses backends when a session started on this
/// proxy is later replayed against them. With a compaction key the summary is
/// encrypted into `encrypted_content`; otherwise it is embedded as a plain-text
/// message.
fn build_compaction_item(compact_key: Option<&[u8; 32]>, summary_text: &str) -> Vec<OutputItem> {
    let compaction_id = format!("cmp_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    if let Some(key) = compact_key {
        let encrypted = crate::crypto::encrypt(key, summary_text);
        vec![OutputItem::Compaction(Compaction {
            id: Some(compaction_id),
            encrypted_content: encrypted,
            status: Some("completed".into()),
            output: vec![],
            created_by: None,
        })]
    } else {
        let msg_id = format!("msg_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        vec![OutputItem::Compaction(Compaction {
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
        })]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compaction_item_id_uses_cmp_prefix() {
        // Plaintext branch (no key): id starts with `cmp_` and the summary is
        // embedded as a nested message.
        let out = build_compaction_item(None, "a summary");
        let OutputItem::Compaction(c) = &out[0] else {
            panic!("expected compaction item");
        };
        assert!(
            c.id.as_deref().unwrap_or_default().starts_with("cmp_"),
            "id = {:?}",
            c.id
        );
        assert!(c.encrypted_content.is_none());
        assert_eq!(c.output.len(), 1);
    }

    #[test]
    fn compaction_item_encrypts_with_key() {
        // Encrypted branch: id still `cmp_`; the summary rides in
        // encrypted_content and round-trips back to the plaintext.
        let key = [7u8; 32];
        let out = build_compaction_item(Some(&key), "secret summary");
        let OutputItem::Compaction(c) = &out[0] else {
            panic!("expected compaction item");
        };
        assert!(c.id.as_deref().unwrap_or_default().starts_with("cmp_"));
        assert!(c.output.is_empty());
        let decrypted = crate::crypto::decrypt(&key, c.encrypted_content.as_ref().unwrap());
        assert_eq!(decrypted.as_deref(), Some("secret summary"));
    }
}
