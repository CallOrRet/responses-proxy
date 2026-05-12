//! Request validation — enforces field constraints from the
//! [OpenAI Responses API](https://developers.openai.com/api/reference) and
//! [Chat Completions API](https://developers.openai.com/api/reference).
//!
//! Validation runs **after** serde deserialisation and before the request is
//! forwarded upstream.  Errors are returned in the format expected by
//! OpenAI clients (§8 of both API docs):
//!
//! ```json
//! { "error": { "message": "...", "type": "invalid_request_error",
//!               "param": "temperature", "code": "invalid_request_error" } }
//! ```

use crate::types::{chat, responses};

// ── Validation error ───────────────────────────────────────────────────────

/// A validation error wrapping [`responses::Error`].
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub error: responses::Error,
}

impl ValidationError {
    pub fn new(message: String, param: &str) -> Self {
        Self {
            error: responses::Error {
                r#type: Some("invalid_request_error".into()),
                code: "invalid_request_error".into(),
                message,
                param: Some(param.into()),
            },
        }
    }

    /// Convert to the standard HTTP JSON error response body.  Doc §8.
    pub fn to_json(&self) -> serde_json::Value {
        self.error.to_http_json()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Responses API validations (POST /v1/responses)
// ═══════════════════════════════════════════════════════════════════════════

/// Validate a Responses API request against all doc-specified constraints.
///
/// Returns `Ok(())` if the request is valid, or a [`ValidationError`] describing
/// the first violation found.
pub fn validate_responses_request(req: &responses::Request) -> Result<(), ValidationError> {
    // ── temperature: [0, 2] ───────────────────────────────────────────
    if req.temperature < 0.0 || req.temperature > 2.0 {
        return Err(ValidationError::new(
            format!("temperature must be in [0, 2]; got {}", req.temperature),
            "temperature",
        ));
    }

    // ── top_p: (0, 1] ────────────────────────────────────────────────
    if req.top_p <= 0.0 || req.top_p > 1.0 {
        return Err(ValidationError::new(
            format!("top_p must be in (0, 1]; got {}", req.top_p),
            "top_p",
        ));
    }

    // ── temperature & top_p should not both be set ────────────────────
    // (Both at default 1.0 is OK — that means the user didn't touch either.)
    let temp_set = (req.temperature - 1.0).abs() > f64::EPSILON;
    let topp_set = (req.top_p - 1.0).abs() > f64::EPSILON;
    if temp_set && topp_set {
        return Err(ValidationError::new(
            "Do not set both temperature and top_p simultaneously".to_string(),
            "temperature",
        ));
    }

    // ── max_output_tokens: positive ──────────────────────────────────
    if let Some(v) = req.max_output_tokens
        && v <= 0
    {
        return Err(ValidationError::new(
            format!("max_output_tokens must be positive; got {}", v),
            "max_output_tokens",
        ));
    }

    // ── max_tool_calls: non-negative ─────────────────────────────────
    if let Some(v) = req.max_tool_calls
        && v < 0
    {
        return Err(ValidationError::new(
            format!("max_tool_calls must be ≥ 0; got {}", v),
            "max_tool_calls",
        ));
    }

    // ── metadata: ≤ 16 pairs, key ≤ 64, value ≤ 512 ─────────────────
    if let Some(ref meta) = req.metadata {
        if meta.len() > 16 {
            return Err(ValidationError::new(
                format!("metadata can have at most 16 pairs; got {}", meta.len()),
                "metadata",
            ));
        }
        for k in meta.keys() {
            if k.len() > 64 {
                return Err(ValidationError::new(
                    format!("metadata key too long (max 64): '{k}'"),
                    "metadata",
                ));
            }
        }
        for v in meta.values() {
            if v.len() > 512 {
                return Err(ValidationError::new(
                    "metadata value too long (max 512)".to_string(),
                    "metadata",
                ));
            }
        }
    }

    // ── context_management compact_threshold: ≥ 1000 ─────────────────
    if let Some(ref items) = req.context_management {
        for (i, cm) in items.iter().enumerate() {
            if let Some(t) = cm.compact_threshold
                && t < 1000
            {
                return Err(ValidationError::new(
                    format!(
                        "context_management[{}].compact_threshold must be ≥ 1000; got {}",
                        i, t
                    ),
                    "context_management",
                ));
            }
        }
    }

    // ── conversation & previous_response_id are mutually exclusive ───
    if req.conversation.is_some() && req.previous_response_id.is_some() {
        return Err(ValidationError::new(
            "conversation and previous_response_id are mutually exclusive".to_string(),
            "conversation",
        ));
    }

    // ── include: must be from allowed set ────────────────────────────
    if let Some(ref includes) = req.include {
        const ALLOWED: &[&str] = &[
            responses::Include::WEB_SEARCH_CALL_ACTION_SOURCES,
            responses::Include::WEB_SEARCH_CALL_RESULTS,
            responses::Include::CODE_INTERPRETER_CALL_OUTPUTS,
            responses::Include::COMPUTER_CALL_OUTPUT_IMAGE_URL,
            responses::Include::FILE_SEARCH_CALL_RESULTS,
            responses::Include::MESSAGE_INPUT_IMAGE_URL,
            responses::Include::MESSAGE_OUTPUT_TEXT_LOGPROBS,
            responses::Include::REASONING_ENCRYPTED_CONTENT,
        ];
        for inc in includes {
            if !ALLOWED.contains(&inc.as_ref()) {
                return Err(ValidationError::new(
                    format!("unknown include value: '{}'", inc.as_ref()),
                    "include",
                ));
            }
        }
    }

    // ── prompt_cache_retention ────────────────────────────────────────
    if let Some(ref pcr) = req.prompt_cache_retention {
        match pcr {
            crate::types::PromptCacheRetention::InMemory
            | crate::types::PromptCacheRetention::Hours24 => {}
        }
    }

    // ── verbosity ────────────────────────────────────────────────────
    if let Some(ref v) = req.verbosity {
        match v {
            crate::types::Verbosity::Low
            | crate::types::Verbosity::Medium
            | crate::types::Verbosity::High => {}
        }
    }

    // ── service_tier ─────────────────────────────────────────────────
    if let Some(ref st) = req.service_tier {
        match st {
            crate::types::ServiceTier::Auto
            | crate::types::ServiceTier::Default
            | crate::types::ServiceTier::Flex
            | crate::types::ServiceTier::Scale
            | crate::types::ServiceTier::Priority => {}
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Chat Completions API validations (POST /v1/chat/completions)
// ═══════════════════════════════════════════════════════════════════════════

/// Validate a Chat Completions API request.
///
/// Called before forwarding to the upstream provider.
pub fn validate_chat_request(req: &chat::Request) -> Result<(), ValidationError> {
    // ── messages: at least one ───────────────────────────────────────
    if req.messages.is_empty() {
        return Err(ValidationError::new(
            "messages array must contain at least one message".to_string(),
            "messages",
        ));
    }

    // ── temperature: [0, 2] ──────────────────────────────────────────
    if let Some(t) = req.temperature
        && (!(0.0..=2.0).contains(&t))
    {
        return Err(ValidationError::new(
            format!("temperature must be in [0, 2]; got {}", t),
            "temperature",
        ));
    }

    // ── top_p: (0, 1] ────────────────────────────────────────────────
    if let Some(p) = req.top_p {
        if p <= 0.0 || p > 1.0 {
            return Err(ValidationError::new(
                format!("top_p must be in (0, 1]; got {}", p),
                "top_p",
            ));
        }
        // temperature & top_p mutually exclusive
        if req.temperature.is_some() {
            return Err(ValidationError::new(
                "Do not set both temperature and top_p simultaneously".to_string(),
                "temperature",
            ));
        }
    }

    // ── frequency_penalty: [-2, 2] ───────────────────────────────────
    if let Some(fp) = req.frequency_penalty
        && (!(-2.0..=2.0).contains(&fp))
    {
        return Err(ValidationError::new(
            format!("frequency_penalty must be in [-2, 2]; got {}", fp),
            "frequency_penalty",
        ));
    }

    // ── presence_penalty: [-2, 2] ────────────────────────────────────
    if let Some(pp) = req.presence_penalty
        && (!(-2.0..=2.0).contains(&pp))
    {
        return Err(ValidationError::new(
            format!("presence_penalty must be in [-2, 2]; got {}", pp),
            "presence_penalty",
        ));
    }

    // ── n: [1, 128] ──────────────────────────────────────────────────
    if let Some(n) = req.n
        && (!(1..=128).contains(&n))
    {
        return Err(ValidationError::new(
            format!("n must be in [1, 128]; got {}", n),
            "n",
        ));
    }

    // ── max_completion_tokens: positive ──────────────────────────────
    if let Some(mct) = req.max_completion_tokens
        && mct <= 0
    {
        return Err(ValidationError::new(
            format!("max_completion_tokens must be positive; got {}", mct),
            "max_completion_tokens",
        ));
    }

    // ── top_logprobs: [0, 20], requires logprobs: true ───────────────
    if let Some(tlp) = req.top_logprobs {
        if !(0..=20).contains(&tlp) {
            return Err(ValidationError::new(
                format!("top_logprobs must be in [0, 20]; got {}", tlp),
                "top_logprobs",
            ));
        }
        if !req.logprobs.unwrap_or(false) {
            return Err(ValidationError::new(
                "top_logprobs requires logprobs to be true".to_string(),
                "top_logprobs",
            ));
        }
    }

    // ── metadata: ≤ 16 pairs, key ≤ 64, value ≤ 512 ─────────────────
    if let Some(ref meta) = req.metadata {
        if meta.len() > 16 {
            return Err(ValidationError::new(
                format!("metadata can have at most 16 pairs; got {}", meta.len()),
                "metadata",
            ));
        }
        for k in meta.keys() {
            if k.len() > 64 {
                return Err(ValidationError::new(
                    format!("metadata key too long (max 64): '{k}'"),
                    "metadata",
                ));
            }
        }
        for v in meta.values() {
            let s = v.to_string();
            if s.len() > 512 {
                return Err(ValidationError::new(
                    "metadata value too long (max 512)".to_string(),
                    "metadata",
                ));
            }
        }
    }

    // ── stop: ≤ 4 sequences ──────────────────────────────────────────
    match &req.stop {
        Some(chat::Stop::Multiple(v)) if v.len() > 4 => {
            return Err(ValidationError::new(
                format!("stop can have at most 4 sequences; got {}", v.len()),
                "stop",
            ));
        }
        _ => {}
    }

    // ── logit_bias: values in [-100, 100] ────────────────────────────
    if let Some(ref bias) = req.logit_bias {
        for (k, v) in bias {
            if *v < -100 || *v > 100 {
                return Err(ValidationError::new(
                    format!("logit_bias['{k}'] must be in [-100, 100]; got {v}"),
                    "logit_bias",
                ));
            }
        }
    }

    // ── modalities: only "text" / "audio" ────────────────────────────
    if let Some(ref mods) = req.modalities {
        for m in mods {
            if m != "text" && m != "audio" {
                return Err(ValidationError::new(
                    format!("unknown modality: '{m}' — allowed: text, audio"),
                    "modalities",
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Responses API ──────────────────────────────────────────────────────

    fn valid_responses_req() -> responses::Request {
        responses::Request {
            model: "gpt-5.2".to_string(),
            input: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn responses_temperature_out_of_range() {
        let mut req = valid_responses_req();
        req.temperature = 3.0;
        let err = validate_responses_request(&req).unwrap_err();
        assert!(err.error.message.contains("temperature"));
        assert_eq!(err.error.param.as_deref(), Some("temperature"));
    }

    #[test]
    fn responses_temperature_negative() {
        let mut req = valid_responses_req();
        req.temperature = -0.5;
        assert!(validate_responses_request(&req).is_err());
    }

    #[test]
    fn responses_top_p_out_of_range() {
        let mut req = valid_responses_req();
        req.top_p = 1.5;
        assert!(validate_responses_request(&req).is_err());
    }

    #[test]
    fn responses_top_p_zero() {
        let mut req = valid_responses_req();
        req.top_p = 0.0;
        assert!(validate_responses_request(&req).is_err());
    }

    #[test]
    fn responses_temp_and_top_p_both_set() {
        let mut req = valid_responses_req();
        req.temperature = 0.5;
        req.top_p = 0.9;
        let err = validate_responses_request(&req).unwrap_err();
        assert!(err.error.message.contains("both"));
    }

    #[test]
    fn responses_both_default_is_ok() {
        let req = valid_responses_req(); // temperature=1.0, top_p=1.0 (defaults)
        assert!(validate_responses_request(&req).is_ok());
    }

    #[test]
    fn responses_max_output_tokens_zero() {
        let mut req = valid_responses_req();
        req.max_output_tokens = Some(0);
        assert!(validate_responses_request(&req).is_err());
    }

    #[test]
    fn responses_max_output_tokens_negative() {
        let mut req = valid_responses_req();
        req.max_output_tokens = Some(-1);
        assert!(validate_responses_request(&req).is_err());
    }

    #[test]
    fn responses_max_tool_calls_negative() {
        let mut req = valid_responses_req();
        req.max_tool_calls = Some(-1);
        assert!(validate_responses_request(&req).is_err());
    }

    #[test]
    fn responses_metadata_too_many_pairs() {
        let mut req = valid_responses_req();
        let mut m = std::collections::HashMap::new();
        for i in 0..20 {
            m.insert(format!("key{i}"), "val".to_string());
        }
        req.metadata = Some(m);
        assert!(validate_responses_request(&req).is_err());
    }

    #[test]
    fn responses_metadata_key_too_long() {
        let mut req = valid_responses_req();
        let mut m = std::collections::HashMap::new();
        m.insert("a".repeat(65), "v".to_string());
        req.metadata = Some(m);
        assert!(validate_responses_request(&req).is_err());
    }

    #[test]
    fn responses_metadata_value_too_long() {
        let mut req = valid_responses_req();
        let mut m = std::collections::HashMap::new();
        m.insert("k".to_string(), "v".repeat(513));
        req.metadata = Some(m);
        assert!(validate_responses_request(&req).is_err());
    }

    #[test]
    fn responses_compact_threshold_too_low() {
        let mut req = valid_responses_req();
        req.context_management = Some(vec![responses::ContextManagement {
            type_: "compaction".to_string(),
            compact_threshold: Some(500),
        }]);
        assert!(validate_responses_request(&req).is_err());
    }

    #[test]
    fn responses_compact_threshold_ok() {
        let mut req = valid_responses_req();
        req.context_management = Some(vec![responses::ContextManagement {
            type_: "compaction".to_string(),
            compact_threshold: Some(1000),
        }]);
        assert!(validate_responses_request(&req).is_ok());
    }

    #[test]
    fn responses_conversation_and_prev_id_mutually_exclusive() {
        let mut req = valid_responses_req();
        req.conversation = Some(responses::ConversationRequest::Id("conv_123".to_string()));
        req.previous_response_id = Some("resp_456".to_string());
        assert!(validate_responses_request(&req).is_err());
    }

    #[test]
    fn responses_bad_include_value() {
        let mut req = valid_responses_req();
        req.include = Some(vec![responses::Include::from("bogus_value")]);
        assert!(validate_responses_request(&req).is_err());
    }

    #[test]
    fn responses_valid_request_passes() {
        let req = valid_responses_req();
        assert!(validate_responses_request(&req).is_ok());
    }

    // ── Chat Completions API ───────────────────────────────────────────────

    fn valid_chat_req() -> chat::Request {
        chat::Request {
            model: "gpt-4o".to_string(),
            messages: vec![chat::MessageRequest::User(chat::UserMessage {
                content: chat::UserContent::Text("hi".to_string()),
                name: None,
            })],
            ..Default::default()
        }
    }

    #[test]
    fn chat_empty_messages() {
        let req = chat::Request {
            messages: vec![],
            model: "gpt-4o".to_string(),
            ..Default::default()
        };
        assert!(validate_chat_request(&req).is_err());
    }

    #[test]
    fn chat_temperature_out_of_range() {
        let mut req = valid_chat_req();
        req.temperature = Some(3.0);
        assert!(validate_chat_request(&req).is_err());
    }

    #[test]
    fn chat_top_p_out_of_range() {
        let mut req = valid_chat_req();
        req.top_p = Some(2.0);
        assert!(validate_chat_request(&req).is_err());
    }

    #[test]
    fn chat_temp_and_top_p_both_set() {
        let mut req = valid_chat_req();
        req.temperature = Some(0.5);
        req.top_p = Some(0.9);
        assert!(validate_chat_request(&req).is_err());
    }

    #[test]
    fn chat_frequency_penalty_out_of_range() {
        let mut req = valid_chat_req();
        req.frequency_penalty = Some(3.0);
        assert!(validate_chat_request(&req).is_err());

        req.frequency_penalty = Some(-3.0);
        assert!(validate_chat_request(&req).is_err());
    }

    #[test]
    fn chat_presence_penalty_out_of_range() {
        let mut req = valid_chat_req();
        req.presence_penalty = Some(3.0);
        assert!(validate_chat_request(&req).is_err());
    }

    #[test]
    fn chat_n_out_of_range() {
        let mut req = valid_chat_req();
        req.n = Some(0);
        assert!(validate_chat_request(&req).is_err());
        req.n = Some(200);
        assert!(validate_chat_request(&req).is_err());
    }

    #[test]
    fn chat_max_completion_tokens_negative() {
        let mut req = valid_chat_req();
        req.max_completion_tokens = Some(-5);
        assert!(validate_chat_request(&req).is_err());
    }

    #[test]
    fn chat_top_logprobs_without_logprobs() {
        let mut req = valid_chat_req();
        req.top_logprobs = Some(5);
        // logprobs not set (defaults to None)
        assert!(validate_chat_request(&req).is_err());
    }

    #[test]
    fn chat_top_logprobs_with_logprobs_ok() {
        let mut req = valid_chat_req();
        req.top_logprobs = Some(5);
        req.logprobs = Some(true);
        assert!(validate_chat_request(&req).is_ok());
    }

    #[test]
    fn chat_top_logprobs_out_of_range() {
        let mut req = valid_chat_req();
        req.top_logprobs = Some(25);
        req.logprobs = Some(true);
        assert!(validate_chat_request(&req).is_err());
    }

    #[test]
    fn chat_stop_too_many() {
        let mut req = valid_chat_req();
        req.stop = Some(chat::Stop::Multiple(vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
            "e".to_string(),
        ]));
        assert!(validate_chat_request(&req).is_err());
    }

    #[test]
    fn chat_stop_4_is_ok() {
        let mut req = valid_chat_req();
        req.stop = Some(chat::Stop::Multiple(vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ]));
        assert!(validate_chat_request(&req).is_ok());
    }

    #[test]
    fn chat_logit_bias_out_of_range() {
        let mut req = valid_chat_req();
        let mut bias = std::collections::HashMap::new();
        bias.insert("1234".to_string(), 200);
        req.logit_bias = Some(bias);
        assert!(validate_chat_request(&req).is_err());
    }

    #[test]
    fn chat_bad_modality() {
        let mut req = valid_chat_req();
        req.modalities = Some(vec!["video".to_string()]);
        assert!(validate_chat_request(&req).is_err());
    }

    #[test]
    fn chat_valid_request_passes() {
        assert!(validate_chat_request(&valid_chat_req()).is_ok());
    }

    // ── Error JSON format ──────────────────────────────────────────────────

    #[test]
    fn validation_error_json_format() {
        let err = ValidationError::new("bad value".to_string(), "temperature");
        let json = err.to_json();
        let e = &json["error"];
        assert_eq!(e["message"], "bad value");
        assert_eq!(e["type"], "invalid_request_error");
        assert_eq!(e["param"], "temperature");
        assert_eq!(e["code"], "invalid_request_error");
    }
}
