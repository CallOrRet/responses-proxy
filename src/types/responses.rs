//! Responses API — request/response types for `POST /v1/responses`.
//!
//! Every field is annotated with: required (`*`) / optional (`?`), default value,
//! valid range, and whether the field can be `null`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Request ────────────────────────────────────────────────────────────────

/// Request body for `POST /v1/responses`.
///
/// All field defaults, ranges, and nullability match the
/// [OpenAI Responses API reference](https://developers.openai.com/api/reference).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Request {
    // ── Core ──────────────────────────────────────────────────────────
    /// Model ID to use. **Recommended / effectively required.**
    /// Examples: `"gpt-5.2"`, `"gpt-5.1"`, `"gpt-5"`, `"gpt-5-mini"`,
    /// `"gpt-5-nano"`, `"gpt-4.1"`, `"gpt-4o"`, `"o3"`, `"o4-mini"`, `"o1"`,
    /// `"computer-use-preview"`, etc.
    pub model: String,

    /// Input text or array of input items. A plain string is normalised into a
    /// single user message. **At least one of `input` / `prompt` must be set.**
    #[serde(deserialize_with = "deserialize_input", default)]
    pub input: Vec<super::item::InputItem>,

    /// System / developer instructions.
    /// **Not automatically inherited** when `previous_response_id` is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,

    // ── Conversation ──────────────────────────────────────────────────
    /// Conversation this response belongs to.  If set, input and output items
    /// are automatically added to the conversation.
    ///
    /// **Mutually exclusive with `previous_response_id`.**
    /// Can be a plain conversation ID string *or* `{"id": "conv_…"}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<ConversationRequest>,

    /// ID of the previous response — used to chain turns.
    /// **Mutually exclusive with `conversation`.**
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,

    // ── Generation control ────────────────────────────────────────────
    /// Maximum output tokens (**includes reasoning tokens**).
    /// Default: unlimited (`null`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,

    /// Maximum number of built-in tool calls in this response.
    /// Default: unlimited (`null`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<i64>,

    /// Whether parallel tool calls are allowed.  Default: `true`.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub parallel_tool_calls: bool,

    /// Sampling temperature, range `[0, 2]`.  Default: `1`.
    /// Higher values (e.g. 0.8) make output more random; lower values (e.g. 0.2)
    /// make it more focused.  **Do not set together with `top_p`.**
    #[serde(default = "default_one_f64", skip_serializing_if = "is_one_f64")]
    pub temperature: f64,

    /// Nucleus sampling — only tokens whose cumulative probability mass reaches
    /// `top_p` are considered.  Range `(0, 1]`.  Default: `1`.
    /// **Do not set together with `temperature`.**
    #[serde(default = "default_one_f64", skip_serializing_if = "is_one_f64")]
    pub top_p: f64,

    // ── Streaming ─────────────────────────────────────────────────────
    /// Enable SSE streaming.  Default: `false`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub stream: bool,

    /// Streaming options — only meaningful when `stream: true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,

    // ── Persistence ───────────────────────────────────────────────────
    /// Whether to persist this response (for distillation / evals).
    /// Default: `true`.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub store: bool,

    /// Run the response in the background (requires retrieve or webhook to
    /// collect results later).  Default: `false`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub background: bool,

    // ── Metadata & safety ─────────────────────────────────────────────
    /// Up to 16 key-value pairs.  Key ≤ 64 chars, value ≤ 512 chars.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,

    /// Stable end-user identifier for safety monitoring (recommend hashing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_identifier: Option<String>,

    // ── Prompt caching ────────────────────────────────────────────────
    /// Key used to improve prompt cache hit rate (replaces deprecated `user`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,

    /// Prompt cache retention policy.
    /// Allowed: `"in-memory"` (default), `"24h"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_retention: Option<super::PromptCacheRetention>,

    // ── Service tier ──────────────────────────────────────────────────
    /// Processing service tier.
    /// Allowed: `"auto"` (default), `"default"`, `"flex"`, `"scale"`, `"priority"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<super::ServiceTier>,

    /// Output verbosity.  Allowed: `"low"`, `"medium"` (default), `"high"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<super::Verbosity>,

    // ── Include ───────────────────────────────────────────────────────
    /// Additional data to embed in the response object.
    /// Allowed values: see [`Include`] constants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<Include>>,

    // ── Structured output ─────────────────────────────────────────────
    /// Text / structured-output configuration.
    /// Default: `{"format": {"type": "text"}}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextConfig>,

    /// Reasoning configuration.  **Only for gpt-5 / o-series models.**
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,

    // ── Tools ─────────────────────────────────────────────────────────
    /// Tools the model may call (up to 128).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<super::tool::ToolRequest>>,

    /// Tool choice strategy.  Default: `"auto"`.
    /// Can be a string (`"auto"`, `"none"`, `"required"`) or an object
    /// (see the 7 tool-choice object shapes in the doc).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<super::tool::ToolChoice>,

    // ── Prompt template ───────────────────────────────────────────────
    /// Reference to a reusable prompt template.
    /// [Learn more](https://platform.openai.com/docs/guides/text?api-mode=responses#reusable-prompts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<Prompt>,

    // ── Context management ────────────────────────────────────────────
    /// Context management configuration — currently only `"compaction"` is
    /// supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_management: Option<Vec<ContextManagement>>,

    /// WebSocket-only: set `false` to warm the cache without generating output.
    /// Ignored in HTTP mode.  Default: `true`.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub generate: bool,
}

// ── Response ───────────────────────────────────────────────────────────────

/// A response returned by the model, including output items, usage stats, and
/// status information.
///
/// Note: the `output_text` convenience field is an **SDK-only** concept; the
/// raw API JSON does **not** carry it — clients must assemble it from
/// `output[*].content[*].text`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Response {
    /// Unique response ID, e.g. `"resp_xxx"`.  **Always present.**
    pub id: String,

    /// Object type — always `"response"`.  **Always present.**
    #[serde(default = "default_response_object")]
    pub object: String,

    /// Unix timestamp (seconds) when the response was created.  **Always present.**
    #[serde(default)]
    pub created_at: i64,

    /// Current status.  **Always present.**
    /// Allowed: `"queued"`, `"in_progress"`, `"completed"`, `"failed"`,
    /// `"incomplete"`, `"cancelled"`.
    #[serde(default)]
    pub status: ResponseStatus,

    /// Actual model snapshot used, e.g. `"gpt-5-2025-08-07"`.  **Always present.**
    #[serde(default)]
    pub model: String,

    /// Output items generated by the model.  **Always present** (may be empty).
    #[serde(default)]
    pub output: Vec<super::item::OutputItem>,

    /// Token usage.  **Nullable** — `null` on early failure / cancellation.
    #[serde(default)]
    pub usage: Option<Usage>,

    /// Error details.  Only non-null when `status` is `"failed"`.
    #[serde(default)]
    pub error: Option<Error>,

    /// Why the response is incomplete.  Only non-null when `status` is `"incomplete"`.
    #[serde(default)]
    pub incomplete_details: Option<IncompleteDetails>,

    // ── Echoed request params (present when applicable) ───────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<i64>,
    #[serde(default)]
    pub parallel_tool_calls: bool,
    #[serde(default)]
    pub tools: Vec<super::tool::Tool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<super::tool::ToolChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<ConversationResponse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<super::ServiceTier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<super::Verbosity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_retention: Option<super::PromptCacheRetention>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_identifier: Option<String>,
}

impl Default for Response {
    fn default() -> Self {
        Self {
            id: String::new(),
            object: default_response_object(),
            created_at: 0,
            status: ResponseStatus::Queued,
            model: String::new(),
            output: Vec::new(),
            usage: None,
            error: None,
            incomplete_details: None,
            metadata: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            max_tool_calls: None,
            parallel_tool_calls: false,
            tools: Vec::new(),
            tool_choice: None,
            text: None,
            reasoning: None,
            previous_response_id: None,
            conversation: None,
            instructions: None,
            service_tier: None,
            verbosity: None,
            background: None,
            prompt_cache_key: None,
            prompt_cache_retention: None,
            safety_identifier: None,
        }
    }
}

fn default_response_object() -> String {
    "response".into()
}

fn default_compaction_object() -> String {
    "response.compaction".into()
}

// ── Serde helpers ──────────────────────────────────────────────────────────

fn default_true() -> bool {
    true
}
fn default_one_f64() -> f64 {
    1.0
}
fn is_true(b: &bool) -> bool {
    *b
}
fn is_false(b: &bool) -> bool {
    !*b
}
fn is_one_f64(v: &f64) -> bool {
    (*v - 1.0).abs() < f64::EPSILON
}

// ── Error (§4.5 ResponseError / §8 ErrorObject / §6.6 WsErrorInner) ──

/// Error object used in `Response.error` (§4.5), HTTP error responses (§8),
/// and as the inner error of WebSocket error frames (§6.6).
///
/// `type` and `param` are optional — in `Response.error` they are `None` and
/// skipped during serialisation, producing the `{code, message}` shape.  In
/// HTTP/WS error bodies they carry `"invalid_request_error"` and the offending
/// parameter name.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Error {
    /// Machine-readable error code.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
    /// Error class, e.g. `"invalid_request_error"`.  Skipped when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    /// Name of the parameter that caused the error.  Skipped when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
}

/// Well-known `code` values from the API docs.  The `code` field is optional;
/// a missing or `null` code indicates no specific error code applies.
impl Error {
    pub const CODE_SERVER_ERROR: &'static str = "server_error";
    pub const CODE_RATE_LIMIT_EXCEEDED: &'static str = "rate_limit_exceeded";
    pub const CODE_INVALID_PROMPT: &'static str = "invalid_prompt";
    pub const CODE_VECTOR_STORE_TIMEOUT: &'static str = "vector_store_timeout";
    pub const CODE_INVALID_IMAGE: &'static str = "invalid_image";
    pub const CODE_INVALID_IMAGE_FORMAT: &'static str = "invalid_image_format";
    pub const CODE_INVALID_BASE64_IMAGE: &'static str = "invalid_base64_image";
    pub const CODE_INVALID_IMAGE_URL: &'static str = "invalid_image_url";
    pub const CODE_IMAGE_TOO_LARGE: &'static str = "image_too_large";
    pub const CODE_IMAGE_TOO_SMALL: &'static str = "image_too_small";
    pub const CODE_IMAGE_PARSE_ERROR: &'static str = "image_parse_error";
    pub const CODE_IMAGE_CONTENT_POLICY_VIOLATION: &'static str = "image_content_policy_violation";
    pub const CODE_INVALID_IMAGE_MODE: &'static str = "invalid_image_mode";
    pub const CODE_IMAGE_FILE_TOO_LARGE: &'static str = "image_file_too_large";
    pub const CODE_UNSUPPORTED_IMAGE_MEDIA_TYPE: &'static str = "unsupported_image_media_type";
    pub const CODE_EMPTY_IMAGE_FILE: &'static str = "empty_image_file";
    pub const CODE_FAILED_TO_DOWNLOAD_IMAGE: &'static str = "failed_to_download_image";
    pub const CODE_IMAGE_FILE_NOT_FOUND: &'static str = "image_file_not_found";

    /// Well-known `type` values.
    pub const TYPE_INVALID_REQUEST: &'static str = "invalid_request_error";
    pub const TYPE_AUTH_ERROR: &'static str = "authentication_error";
    pub const TYPE_RATE_LIMIT_ERROR: &'static str = "rate_limit_error";
    pub const TYPE_SERVER_ERROR: &'static str = "server_error";
    pub const TYPE_API_ERROR: &'static str = "api_error";

    /// Wrap in the HTTP error envelope `{"error": {...}}`.  Doc §8.
    pub fn to_http_json(&self) -> serde_json::Value {
        serde_json::json!({ "error": self })
    }
}

// ── IncompleteDetails ──────────────────────────────────────────────────────

/// Why the response is incomplete.  Only present when `status` is `"incomplete"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IncompleteDetails {
    /// Reason.  Common: `"max_output_tokens"`, `"content_filter"`.
    pub reason: IncompleteReason,
}

// ── Usage ──────────────────────────────────────────────────────────────────

/// Token usage breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Usage {
    /// Input token count.  **Always present.**
    pub input_tokens: i64,
    /// Cached-input token details.  **Always present.**
    pub input_tokens_details: InputTokensDetails,
    /// Output token count (**includes reasoning tokens**).  **Always present.**
    pub output_tokens: i64,
    /// Output token details.  **Always present.**
    pub output_tokens_details: OutputTokensDetails,
    /// Total tokens (input + output).  **Always present.**
    pub total_tokens: i64,
}

/// Input token detail — only `cached_tokens` for now.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InputTokensDetails {
    /// Tokens served from the prompt cache.  **Always present.**
    pub cached_tokens: i64,
}

/// Output token detail — only `reasoning_tokens` for now.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OutputTokensDetails {
    /// Tokens consumed by internal reasoning.  **Always present.**
    pub reasoning_tokens: i64,
}

// ── TextConfig / TextFormat ────────────────────────────────────────────────

/// Output text configuration.
///
/// Default: `{"format": {"type": "text"}}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TextConfig {
    /// Output format — one of: `{"type": "text"}` (default),
    /// `{"type": "json_schema", "name": …, "schema": …}`,
    /// or `{"type": "json_object"}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<TextFormat>,
}

/// Output format discriminator — three variants.
///
/// Tagged on `type` field: `"text"`, `"json_schema"`, or `"json_object"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TextFormat {
    /// Default plain-text format.
    #[serde(rename = "text")]
    Text,

    /// JSON Schema structured-output format (recommended).
    ///
    /// `strict: true` restricts the model to a JSON Schema subset for
    /// guaranteed valid output.
    #[serde(rename = "json_schema")]
    JsonSchema {
        /// Schema name — `^[a-zA-Z0-9_-]{1,64}$`.  **Required.**
        name: String,
        /// JSON Schema definition.  **Required.**
        #[serde(rename = "schema")]
        schema: Value,
        /// Enable strict schema adherence.  Recommended: `true`.
        /// Only a JSON Schema subset is supported in strict mode.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        strict: Option<bool>,
        /// Hint for the model about the intended output shape.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },

    /// Legacy JSON mode — guarantees valid JSON but no schema enforcement.
    #[serde(rename = "json_object")]
    JsonObject,
}

// ── Reasoning ──────────────────────────────────────────────────────────────

/// Reasoning configuration.  **Only applies to gpt-5 / o-series models.**
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Reasoning {
    /// Reasoning effort.
    /// Allowed: `"none"`, `"minimal"`, `"low"`, `"medium"`, `"high"`, `"xhigh"`.
    ///
    /// Default: gpt-5.1 defaults to `"none"`; earlier models default to
    /// `"medium"`.  `gpt-5-pro` forces `"high"`.  `"xhigh"` is only available
    /// on `gpt-5.1-codex-max` and later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<super::ReasoningEffort>,

    /// Reasoning summary verbosity.
    /// Allowed: `"auto"`, `"concise"`, `"detailed"`.  Default: `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<ReasoningSummary>,

    /// **Deprecated** — use `summary` instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generate_summary: Option<ReasoningSummary>,
}

/// Reasoning summary verbosity level.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningSummary {
    /// Let the model decide whether to include a summary.
    Auto,
    /// Short summary only.
    Concise,
    /// Full detailed summary.
    Detailed,
}

// ── Conversation ───────────────────────────────────────────────────────────

/// Conversation reference — **request side**.
///
/// Can be a plain conversation ID string *or* an object `{"id": "conv_…"}`.
/// **Mutually exclusive with `previous_response_id`.**
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConversationRequest {
    /// Plain conversation ID, e.g. `"conv_xyz"`.
    Id(String),
    /// Object form: `{"id": "conv_xyz"}`.
    Object { id: String },
}

/// Conversation reference — **response side**.
///
/// Can be a plain conversation ID string *or* an object `{"id": "conv_…"}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConversationResponse {
    /// Plain conversation ID.
    Id(String),
    /// Object form.
    Object { id: String },
}

// ── Prompt ─────────────────────────────────────────────────────────────────

/// Reference to a reusable prompt template.
///
/// Can be a plain prompt ID string *or* an object with variables.
///
/// `VariableValue` can be a string, `InputText`, `InputImage`, or `InputFile`
/// (see [`super::item::InputContentBlock`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Prompt {
    /// Plain prompt template ID, e.g. `"prompt_abc"`.
    Id(String),
    /// Object form — `id` is **required**; `version` and `variables` are
    /// optional.
    Object {
        /// Prompt template ID.  **Required.**
        id: String,
        /// Optional version pin.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<String>,
        /// Variable map — key → value, where value can be a string or an
        /// `input_text` / `input_image` / `input_file` content block.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        variables: Option<HashMap<String, Value>>,
    },
}

/// Response-side prompt reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PromptResponse {
    /// Prompt template ID.
    pub id: String,
    /// Variable map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<HashMap<String, Value>>,
    /// Version identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

// ── ContextManagement ──────────────────────────────────────────────────────

/// Context management item — currently only `"compaction"` is supported.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ContextManagement {
    /// Fixed value: `"compaction"`.
    #[serde(rename = "type")]
    pub type_: String,

    /// Token count at which auto-compaction triggers.  Must be **≥ 1000**.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact_threshold: Option<i64>,
}

// ── StreamOptions ──────────────────────────────────────────────────────────

/// Streaming options — only meaningful when `stream: true`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StreamOptions {
    /// Include random `obfuscation` field in delta events to prevent
    /// side-channel attacks.  Default: `true`.  Set to `false` to save
    /// bandwidth if you trust the network.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub include_obfuscation: bool,
}

// ── CompactedResponse ──────────────────────────────────────────────────────

/// Response from the compaction endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CompactedResponse {
    /// Compaction ID, e.g. `"rcmp_xxx"`.
    pub id: String,
    /// Object type — always `"response.compaction"`.
    #[serde(default = "default_compaction_object")]
    pub object: String,
    /// Unix timestamp (seconds).
    pub created_at: i64,
    /// Compressed output items.
    #[serde(default)]
    pub output: Vec<super::item::OutputItem>,
    /// Token usage for the compaction itself.
    pub usage: Usage,
}

// ── ResponseStatus ─────────────────────────────────────────────────────────

/// Response lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    /// Response is queued for processing.
    Queued,
    /// Generation is in progress.
    #[default]
    InProgress,
    /// Response completed successfully.
    Completed,
    /// Response generation failed (see `error` field).
    Failed,
    /// Response is incomplete (see `incomplete_details` field).
    Incomplete,
    /// Response was cancelled.
    Cancelled,
}

// ── IncompleteReason ───────────────────────────────────────────────────────

/// Why the response is incomplete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncompleteReason {
    /// Stopped early — hit `max_output_tokens` limit.
    #[serde(rename = "max_output_tokens")]
    MaxOutputTokens,
    /// Blocked by the content filter.
    #[serde(rename = "content_filter")]
    ContentFilter,
}

// ── Include ────────────────────────────────────────────────────────────────

/// Transparent string newtype for the `include` request field.
///
/// Each constant below is a valid value recognised by the API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Include(pub String);

impl Include {
    /// Include web search action sources in web search call items.
    pub const WEB_SEARCH_CALL_ACTION_SOURCES: &'static str = "web_search_call.action.sources";
    /// Include web search results in web search call items.
    pub const WEB_SEARCH_CALL_RESULTS: &'static str = "web_search_call.results";
    /// Include code interpreter outputs.
    pub const CODE_INTERPRETER_CALL_OUTPUTS: &'static str = "code_interpreter_call.outputs";
    /// Include computer-use screenshot URLs.
    pub const COMPUTER_CALL_OUTPUT_IMAGE_URL: &'static str =
        "computer_call_output.output.image_url";
    /// Include file search results.
    pub const FILE_SEARCH_CALL_RESULTS: &'static str = "file_search_call.results";
    /// Include input image URLs from messages.
    pub const MESSAGE_INPUT_IMAGE_URL: &'static str = "message.input_image.image_url";
    /// Include output-text logprobs.
    pub const MESSAGE_OUTPUT_TEXT_LOGPROBS: &'static str = "message.output_text.logprobs";
    /// Include encrypted reasoning content (needed for `store: false`
    /// cross-turn chains).
    pub const REASONING_ENCRYPTED_CONTENT: &'static str = "reasoning.encrypted_content";
}

impl AsRef<str> for Include {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Include {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<String> for Include {
    fn from(s: String) -> Self {
        Include(s)
    }
}

impl From<&str> for Include {
    fn from(s: &str) -> Self {
        Include(s.to_owned())
    }
}

// ── Internal convenience types ─────────────────────────────────────────────

/// Input union — plain text string or list of input items.
///
/// Used internally by the custom deserializer; not exposed in the public API.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
#[derive(Default)]
enum Input {
    String(String),
    Array(Vec<super::item::InputItem>),
    #[serde(skip)]
    #[default]
    EmptyNone,
}

fn deserialize_input<'de, D>(deserializer: D) -> Result<Vec<super::item::InputItem>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let input: Input = Input::deserialize(deserializer)?;
    match input {
        Input::String(s) => Ok(vec![super::item::InputItem::Message(
            super::item::InputMessage {
                role: crate::types::MessageRole::User,
                content: vec![super::item::InputContentBlock::Text { text: s }],
                status: None,
            },
        )]),
        Input::Array(items) => Ok(items),
        Input::EmptyNone => Ok(vec![]),
    }
}

impl Default for Request {
    fn default() -> Self {
        Self {
            model: String::new(),
            input: Vec::new(),
            instructions: None,
            conversation: None,
            previous_response_id: None,
            max_output_tokens: None,
            max_tool_calls: None,
            parallel_tool_calls: true,
            temperature: 1.0,
            top_p: 1.0,
            stream: false,
            stream_options: None,
            store: true,
            background: false,
            metadata: None,
            safety_identifier: None,
            prompt_cache_key: None,
            prompt_cache_retention: None,
            service_tier: None,
            verbosity: None,
            include: None,
            text: None,
            reasoning: None,
            tools: None,
            tool_choice: None,
            prompt: None,
            context_management: None,
            generate: true,
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Request serialisation ──────────────────────────────────────────

    #[test]
    fn test_new_params_minimal() {
        let p = Request {
            model: "gpt-5.2".into(),
            ..Default::default()
        };
        let json = serde_json::to_string(&p).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["model"], "gpt-5.2");
        // Optional fields with defaults should still be absent when
        // skip_serializing_if triggers.
        assert!(!v.as_object().unwrap().contains_key("temperature"));
    }

    #[test]
    fn test_request_with_string_input() {
        let json = r#"{"model": "gpt-5.2", "input": "Hello, world!"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "gpt-5.2");
        assert_eq!(req.input.len(), 1);
        match &req.input[0] {
            super::super::item::InputItem::Message(m) => {
                assert_eq!(m.role, crate::types::MessageRole::User);
            }
            _ => panic!("expected Message variant"),
        }
    }

    #[test]
    fn test_request_with_array_input() {
        let json = r#"{"model": "gpt-5.2", "input": []}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert!(req.input.is_empty());
    }

    // ── Default values ─────────────────────────────────────────────────

    #[test]
    fn test_defaults_on_deser() {
        let json = r#"{"model": "gpt-5.2"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert!(req.store); // default true
        assert!(!req.stream); // default false
        assert!(req.parallel_tool_calls); // default true
        assert!((req.temperature - 1.0).abs() < f64::EPSILON); // default 1
        assert!((req.top_p - 1.0).abs() < f64::EPSILON); // default 1
    }

    // ── Unknown fields are accepted (validation layer catches errors) ──

    #[test]
    fn test_new_params_allows_extra_fields() {
        let json = r#"{"model": "gpt-5.2", "input": [], "bogus": 42}"#;
        let req = serde_json::from_str::<Request>(json).unwrap();
        assert_eq!(req.model, "gpt-5.2");
    }

    // ── ConversationRequest ────────────────────────────────────────────

    #[test]
    fn test_conversation_string() {
        let cr: ConversationRequest = serde_json::from_str(r#""conv_abc""#).unwrap();
        match cr {
            ConversationRequest::Id(s) => assert_eq!(s, "conv_abc"),
            _ => panic!("expected Id"),
        }
    }

    #[test]
    fn test_conversation_object() {
        let cr: ConversationRequest = serde_json::from_str(r#"{"id": "conv_abc"}"#).unwrap();
        match cr {
            ConversationRequest::Object { id } => assert_eq!(id, "conv_abc"),
            _ => panic!("expected Object"),
        }
    }

    // ── Prompt untagged ────────────────────────────────────────────────

    #[test]
    fn test_prompt_string() {
        let p: Prompt = serde_json::from_str(r#""prompt_abc""#).unwrap();
        match p {
            Prompt::Id(s) => assert_eq!(s, "prompt_abc"),
            _ => panic!("expected Id"),
        }
    }

    #[test]
    fn test_prompt_object() {
        let json = r#"{"id": "prompt_abc", "version": "2", "variables": {"topic": "Rust"}}"#;
        let p: Prompt = serde_json::from_str(json).unwrap();
        match p {
            Prompt::Object {
                id,
                version,
                variables,
            } => {
                assert_eq!(id, "prompt_abc");
                assert_eq!(version, Some("2".into()));
                let vars = variables.unwrap();
                assert_eq!(vars["topic"], "Rust");
            }
            _ => panic!("expected Object"),
        }
    }

    #[test]
    fn test_prompt_object_minimal() {
        let json = r#"{"id": "prompt_abc"}"#;
        let p: Prompt = serde_json::from_str(json).unwrap();
        match p {
            Prompt::Object {
                id,
                version,
                variables,
            } => {
                assert_eq!(id, "prompt_abc");
                assert!(version.is_none());
                assert!(variables.is_none());
            }
            _ => panic!("expected Object"),
        }
    }

    // ── Response deserialisation ───────────────────────────────────────

    #[test]
    fn test_response_basic() {
        let json = r#"{
            "id": "resp_abc123",
            "status": "completed",
            "model": "gpt-5.2",
            "output": []
        }"#;
        let r: Response = serde_json::from_str(json).unwrap();
        assert_eq!(r.id, "resp_abc123");
        assert_eq!(r.object, "response");
        assert_eq!(r.status, ResponseStatus::Completed);
        assert_eq!(r.model, "gpt-5.2");
        assert!(r.output.is_empty());
    }

    #[test]
    fn test_response_full() {
        let json = r#"{
            "id": "resp_abc123",
            "created_at": 1700000000,
            "object": "response",
            "status": "completed",
            "model": "gpt-5.2",
            "output": [],
            "temperature": 0.7,
            "top_p": 1.0,
            "max_output_tokens": 4096,
            "parallel_tool_calls": true,
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "total_tokens": 150,
                "input_tokens_details": {
                    "cached_tokens": 20
                },
                "output_tokens_details": {
                    "reasoning_tokens": 10
                }
            }
        }"#;
        let r: Response = serde_json::from_str(json).unwrap();
        assert_eq!(r.temperature, Some(0.7));
        assert_eq!(r.top_p, Some(1.0));
        let usage = r.usage.unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens_details.reasoning_tokens, 10);
    }

    // ── Status enum ────────────────────────────────────────────────────

    #[test]
    fn test_status_serde() {
        let s = ResponseStatus::InProgress;
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, r#""in_progress""#);

        let d: ResponseStatus = serde_json::from_str(r#""completed""#).unwrap();
        assert_eq!(d, ResponseStatus::Completed);
    }

    // ── Reasoning ─────────────────────────────────────────────────────

    #[test]
    fn test_reasoning_summary() {
        let r = Reasoning {
            effort: Some(crate::types::ReasoningEffort::High),
            summary: Some(ReasoningSummary::Concise),
            generate_summary: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains(r#""summary":"concise""#));
        assert!(json.contains(r#""effort":"high""#));
    }

    #[test]
    fn test_reasoning_generate_summary_deprecated() {
        let json = r#"{"generate_summary": "detailed"}"#;
        let r: Reasoning = serde_json::from_str(json).unwrap();
        assert!(matches!(
            r.generate_summary,
            Some(ReasoningSummary::Detailed)
        ));
    }

    // ── Include ────────────────────────────────────────────────────────

    #[test]
    fn test_include() {
        let i = Include("file_search_call.results".into());
        let json = serde_json::to_string(&i).unwrap();
        assert_eq!(json, r#""file_search_call.results""#);
    }

    #[test]
    fn test_include_deser() {
        let i: Include = serde_json::from_str(r#""message.input_image.image_url""#).unwrap();
        assert_eq!(i.as_ref(), "message.input_image.image_url");
    }

    // ── ContextManagement ──────────────────────────────────────────────

    #[test]
    fn test_context_management() {
        let cm = ContextManagement {
            type_: "compaction".into(),
            compact_threshold: Some(10000),
        };
        let json = serde_json::to_string(&cm).unwrap();
        assert!(json.contains(r#""type":"compaction""#));
        assert!(json.contains("10000"));
    }

    // ── Usage ──────────────────────────────────────────────────────────

    #[test]
    fn test_usage_serde() {
        let u = Usage {
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            input_tokens_details: InputTokensDetails { cached_tokens: 20 },
            output_tokens_details: OutputTokensDetails {
                reasoning_tokens: 30,
            },
        };
        let json = serde_json::to_string(&u).unwrap();
        assert!(json.contains(r#""input_tokens":100"#));
        assert!(json.contains(r#""cached_tokens":20"#));
        assert!(json.contains(r#""reasoning_tokens":30"#));
    }

    // ── TextFormat ─────────────────────────────────────────────────────

    #[test]
    fn test_text_format_json_schema() {
        let tf = TextFormat::JsonSchema {
            name: "person".into(),
            schema: serde_json::json!({"type": "object"}),
            strict: Some(true),
            description: None,
        };
        let json = serde_json::to_string(&tf).unwrap();
        assert!(json.contains(r#""type":"json_schema""#));
        assert!(json.contains(r#""name":"person""#));
    }

    #[test]
    fn test_text_format_text() {
        let tf = TextFormat::Text;
        let json = serde_json::to_string(&tf).unwrap();
        assert_eq!(json, r#"{"type":"text"}"#);
    }

    #[test]
    fn test_text_format_json_object() {
        let tf = TextFormat::JsonObject;
        let json = serde_json::to_string(&tf).unwrap();
        assert_eq!(json, r#"{"type":"json_object"}"#);
    }
}
