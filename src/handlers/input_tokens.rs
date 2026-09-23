//! `POST /v1/responses/input_tokens` — token counting endpoint.

use crate::convert::responses_to_chat;
use crate::types::chat;
use crate::types::responses::{Error, Request};
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};

/// POST /v1/responses/input_tokens — count tokens without generating.
///
/// Converts the request to Chat API messages and estimates token count.
/// Uses a simple chars/4 heuristic (common approximation for English text).
pub async fn input_tokens(
    State(state): State<crate::app::State>,
    super::json::ResponsesJson(req): super::json::ResponsesJson<Request>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let chat_req = responses_to_chat(req, &state)
        .await
        .map_err(|unsupported| {
            let err =
                Error::invalid_request(format!("Unsupported features: {}", unsupported.join(", ")));
            (StatusCode::BAD_REQUEST, Json(err.to_http_json()))
        })?;

    let estimated = estimate_tokens(&chat_req);

    Ok(Json(serde_json::json!({
        "object": "response.input_tokens",
        "input_tokens": estimated,
    })))
}

/// Estimate token count from a Chat API request.
/// Falls back to chars/4 when no tokenizer is available.
fn estimate_tokens(req: &chat::Request) -> i64 {
    let total_chars = content_chars(&req.messages);
    if total_chars == 0 {
        0
    } else {
        (total_chars / 4).max(1) as i64
    }
}

/// Total content-character count of a converted Chat message list. Used to
/// compute the dropped/kept ratio when reporting the true pre-truncation
/// context size back to Codex (see `handlers::responses`).
pub(crate) fn content_chars(messages: &[chat::MessageRequest]) -> usize {
    messages.iter().map(count_message_chars).sum()
}

/// Scale the upstream `usage` up to reflect the true pre-truncation history
/// size, in place. Codex gates its client-side auto-compaction solely on the
/// server-reported `total_tokens`, so when we shrink the forwarded input the
/// upstream counts only the shrunken payload and under-reports the real context
/// size. `scale = (full_chars, sent_chars)` grows `input_tokens` back up by that
/// ratio (calibrated against the upstream's own tokenizer rather than a
/// hardcoded chars-per-token constant); `total_tokens` is kept in sync so
/// `total == input + output`. No-op when `scale` is `None`, usage is absent, or
/// nothing was truncated (`full <= sent`, or `sent == 0`).
///
/// If the scale ratio proves inaccurate for some languages we can switch to a
/// real tokenizer such as tiktoken-rs.
pub(crate) fn apply_input_char_scale(
    usage: Option<&mut crate::types::responses::Usage>,
    scale: Option<(u64, u64)>,
) {
    let Some(u) = usage else { return };
    if let Some((full_chars, sent_chars)) = scale
        && sent_chars != 0
        && full_chars > sent_chars
    {
        let scaled =
            ((u.input_tokens as f64) * (full_chars as f64) / (sent_chars as f64)).round() as i64;
        u.input_tokens = scaled;
        u.total_tokens = scaled + u.output_tokens;
    }
}

fn count_message_chars(msg: &chat::MessageRequest) -> usize {
    match msg {
        chat::MessageRequest::System(m) => count_msg_content(&m.content),
        chat::MessageRequest::Developer(m) => count_msg_content(&m.content),
        chat::MessageRequest::User(m) => count_user_content(&m.content),
        chat::MessageRequest::Assistant(m) => match &m.content {
            Some(c) => count_assistant_content(c),
            None => 0,
        },
        chat::MessageRequest::Tool(m) => count_msg_content(&m.content),
        chat::MessageRequest::Function(m) => m.content.len(),
    }
}

fn count_msg_content(c: &chat::MessageContent) -> usize {
    match c {
        chat::MessageContent::Text(s) => s.len(),
        chat::MessageContent::Parts(parts) => parts.iter().map(|p| p.text.len()).sum(),
    }
}

fn count_user_content(c: &chat::UserContent) -> usize {
    match c {
        chat::UserContent::Text(s) => s.len(),
        chat::UserContent::Parts(parts) => parts
            .iter()
            .map(|p| match p {
                chat::ContentPart::Text { text } => text.len(),
                chat::ContentPart::Image { .. } => 85,
                chat::ContentPart::File { .. } => 0,
                chat::ContentPart::Audio { .. } => 0,
                chat::ContentPart::Refusal { .. } => 0,
            })
            .sum(),
    }
}

fn count_assistant_content(c: &chat::AssistantContent) -> usize {
    match c {
        chat::AssistantContent::Text(s) => s.len(),
        chat::AssistantContent::Parts(parts) => parts
            .iter()
            .map(|p| match p {
                chat::ContentPart::Text { text } => text.len(),
                chat::ContentPart::Refusal { .. } => 0,
                _ => 0,
            })
            .sum(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::responses::{InputTokensDetails, OutputTokensDetails, Usage};

    fn usage(input: i64, output: i64) -> Usage {
        Usage {
            input_tokens: input,
            output_tokens: output,
            total_tokens: input + output,
            input_tokens_details: InputTokensDetails { cached_tokens: 0 },
            output_tokens_details: OutputTokensDetails {
                reasoning_tokens: 0,
            },
        }
    }

    #[test]
    fn scales_input_tokens_by_full_sent_ratio() {
        let mut u = usage(1000, 200);
        // Sent 4000 chars but the full history was 12000 → 3× → 3000 tokens.
        apply_input_char_scale(Some(&mut u), Some((12000, 4000)));
        assert_eq!(u.input_tokens, 3000);
        assert_eq!(u.total_tokens, 3200);
    }

    #[test]
    fn no_op_when_scale_is_none() {
        let mut u = usage(1000, 200);
        apply_input_char_scale(Some(&mut u), None);
        assert_eq!(u.input_tokens, 1000);
        assert_eq!(u.total_tokens, 1200);
    }

    #[test]
    fn no_op_when_nothing_was_truncated() {
        // full == sent: truncation removed nothing, so leave usage untouched.
        let mut u = usage(1000, 200);
        apply_input_char_scale(Some(&mut u), Some((4000, 4000)));
        assert_eq!(u.input_tokens, 1000);
        assert_eq!(u.total_tokens, 1200);
    }

    #[test]
    fn no_op_when_sent_chars_zero() {
        let mut u = usage(1000, 200);
        apply_input_char_scale(Some(&mut u), Some((4000, 0)));
        assert_eq!(u.input_tokens, 1000);
    }
}
