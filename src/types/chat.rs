//! Chat Completions API — complete request/response types for
//! `POST /chat/completions`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Request ────────────────────────────────────────────────────────────────

/// Request body for `POST /chat/completions`.
///
/// All field defaults, ranges, and nullability match the
/// [OpenAI Chat Completions API reference](https://developers.openai.com/api/reference).
#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Request {
    /// A list of messages comprising the conversation so far.  **At least one
    /// message is required.**
    pub messages: Vec<MessageRequest>,

    /// Model ID to use (e.g. `"gpt-4o"`, `"gpt-4.1"`, `"o4-mini"`). **Required.**
    pub model: String,

    /// Number in `[-2, 2]`.  Positive values penalize new tokens based on
    /// their frequency in the text so far.  **Default: `0`.**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,

    /// Whether to return log probabilities of output tokens.  Requires
    /// `top_logprobs` to be set if enabled.  **Default: `false`.**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,

    /// Maximum tokens to generate (**includes reasoning tokens**).
    /// **Default: `null` (unlimited)**.  Positive integer when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<i64>,

    /// **Deprecated** — use `max_completion_tokens` instead.
    /// Not compatible with o-series models.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,

    /// Number of completion choices to generate.  Charged per token across all
    /// choices.  **Range: `[1, 128]`, default: `1`.**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<i64>,

    /// Number in `[-2, 2]`.  Positive values encourage the model to talk about
    /// new topics.  **Default: `0`.**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,

    /// Best-effort deterministic sampling seed.  **Default: `null`.**
    /// Same seed + same params should produce similar output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,

    /// Whether to store the output for distillation / evals.
    /// **Default: `false`.**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,

    /// Enable SSE streaming.  **Default: `false`.**
    /// Stream terminates with `data: [DONE]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,

    /// Sampling temperature, range `[0, 2]`.  **Default: `1`.**
    /// **Do not set together with `top_p`.**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    /// Number of most-likely tokens to return at each position.
    /// **Range: `[0, 20]`.  Requires `logprobs: true`.  Default: `null`.**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<i64>,

    /// Nucleus sampling — cumulative probability threshold.
    /// **Range: `(0, 1]`, default: `1`.  Do not set together with `temperature`.**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,

    /// Allow parallel function calls during tool use.  **Default: `true`.**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,

    /// Reserved for future use.  Identifier for prompt caching.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,

    /// Stable end-user identifier for safety monitoring (max 64 chars,
    /// recommend hashing).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_identifier: Option<String>,

    /// **Deprecated** — use `safety_identifier` + `prompt_cache_key` instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    /// Parameters for audio output.  Required when `modalities` includes
    /// `"audio"`.  Only for `gpt-4o-audio-preview` series.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<ChatAudio>,

    /// Token bias map — token ID → bias in `[-100, 100]`.
    /// Values near -100/100 ban or force the token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<HashMap<String, i64>>,

    /// Up to 16 key-value pairs.  Key ≤ 64 chars, value ≤ 512 chars.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Map<String, serde_json::Value>>,

    /// Output modalities.  Allowed: `"text"`, `"audio"`.
    /// **Default: `["text"]`.**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Vec<String>>,

    /// Prompt cache retention policy.
    /// Allowed: `"in-memory"` (default), `"24h"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_retention: Option<super::PromptCacheRetention>,

    /// Reasoning effort for o-series / gpt-5 models.
    /// Allowed: `"none"`, `"minimal"`, `"low"`, `"medium"`, `"high"`, `"xhigh"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<super::ReasoningEffort>,

    /// Processing service tier.
    /// Allowed: `"auto"`, `"default"`, `"flex"`, `"scale"`, `"priority"`.
    /// **Default: `"auto"`.**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<super::ServiceTier>,

    /// Up to 4 sequences where the API will stop generating.
    /// **o3/o4-mini do not support this.**  **Default: `null`.**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Stop>,

    /// Streaming options — only meaningful when `stream: true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,

    /// Output verbosity.  Allowed: `"low"`, `"medium"`, `"high"`.
    /// **Default: `"medium"`.**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<super::Verbosity>,

    /// **Deprecated** — use `tool_choice` instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCallOption>,

    /// **Deprecated** — use `tools` instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub functions: Option<Vec<FunctionTool>>,

    /// [Predicted Outputs](https://platform.openai.com/docs/guides/predicted-outputs)
    /// configuration — can greatly improve response times when most of the
    /// response is known ahead of time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prediction: Option<PredictionContent>,

    /// Response format constraint.
    /// Allowed: `{"type": "text"}` (default), `{"type": "json_schema", ...}`,
    /// `{"type": "json_object"}`.
    ///
    /// ⚠ **Note:** Chat Completions nests the JSON Schema inside
    /// `response_format.json_schema.schema`, while Responses API uses
    /// `text.format.schema` directly.  This is the most common confusion
    /// point between the two APIs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,

    /// Tool choice strategy.  `"none"` (default with no tools), `"auto"`
    /// (default with tools), `"required"`, or an object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,

    /// Tools the model may call.  Max 128.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolRequest>>,

    /// Options for the built-in web search tool.
    /// **Only for `gpt-4o-search-preview` / `gpt-4o-mini-search-preview` series.**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search_options: Option<WebSearchOptions>,
}

// ── Response ───────────────────────────────────────────────────────────────

/// Error response from the Chat Completions API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatError {
    /// Human-readable error message.
    pub message: String,
    /// Error type, e.g. `"invalid_request_error"`.
    #[serde(rename = "type")]
    pub type_: String,
    /// Machine-readable error code.  May be `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Parameter that caused the error.  May be `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
}

/// Represents a chat completion response returned by the model, based on the
/// provided input.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Completion {
    /// Error response from upstream.
    #[serde(default)]
    pub error: Option<ChatError>,
    /// A unique identifier for the chat completion.
    pub id: String,

    /// A list of chat completion choices. Can be more than one if `n` is
    /// greater than 1.
    pub choices: Vec<Choice>,

    /// The Unix timestamp (in seconds) of when the chat completion was created.
    pub created: i64,

    /// The model used for the chat completion.
    pub model: String,

    /// The object type, which is always `chat.completion`.
    #[serde(default = "default_completion_object")]
    pub object: String,

    /// The service tier used for processing the request.
    #[serde(default)]
    pub service_tier: Option<super::ServiceTier>,

    /// Deprecated. This fingerprint represents the backend configuration that
    /// the model runs with. Can be used in conjunction with the `seed` request
    /// parameter to understand when backend changes have been made that might
    /// impact determinism.
    #[serde(default)]
    pub system_fingerprint: Option<String>,

    /// Usage statistics for the completion request.
    #[serde(default)]
    pub usage: Option<Usage>,
}

fn default_completion_object() -> String {
    "chat.completion".to_string()
}

// ── Streaming Chunk ────────────────────────────────────────────────────────

/// Represents a streamed chunk of a chat completion response returned by the
/// model, based on the provided input.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Chunk {
    /// A unique identifier for the chat completion. Each chunk has the same ID.
    pub id: String,

    /// A list of chat completion choices. Can be more than one if `n` is
    /// greater than 1.
    pub choices: Vec<ChunkChoice>,

    /// The Unix timestamp (in seconds) of when the chat completion was created.
    /// Each chunk has the same timestamp.
    pub created: i64,

    /// The model used for the chat completion.
    pub model: String,

    /// The object type, which is always `chat.completion.chunk`.
    #[serde(default = "default_chunk_object")]
    pub object: String,

    /// The service tier used for processing the request.
    #[serde(default)]
    pub service_tier: Option<super::ServiceTier>,

    /// Deprecated fingerprint.
    #[serde(default)]
    pub system_fingerprint: Option<String>,

    /// Usage statistics. Only present on the last chunk when
    /// `stream_options.include_usage` is `true`.
    #[serde(default)]
    pub usage: Option<Usage>,
}

fn default_chunk_object() -> String {
    "chat.completion.chunk".to_string()
}

// ── Choice Types ───────────────────────────────────────────────────────────

/// A chat completion choice.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Choice {
    /// The reason the model stopped generating tokens. Allowed: `"stop"`,
    /// `"length"`, `"tool_calls"`, `"content_filter"`, `"function_call"`.
    pub finish_reason: Option<String>,

    /// The index of the choice in the list of choices.
    pub index: i64,

    /// Log probability information for the choice.
    #[serde(default)]
    pub logprobs: Option<ChoiceLogprobs>,

    /// A chat completion message generated by the model.
    pub message: ResponseMessage,
}

/// A streaming chat completion choice.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChunkChoice {
    /// A chat completion delta generated by streamed model responses.
    pub delta: Delta,

    /// The reason the model stopped generating tokens. Allowed: `"stop"`,
    /// `"length"`, `"tool_calls"`, `"content_filter"`, `"function_call"`.
    #[serde(default)]
    pub finish_reason: Option<String>,

    /// The index of the choice in the list of choices.
    pub index: i64,

    /// Log probability information for the choice.
    #[serde(default)]
    pub logprobs: Option<ChoiceLogprobs>,
}

/// Log probability information for a choice.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChoiceLogprobs {
    /// A list of message content tokens with log probability information.
    #[serde(default)]
    pub content: Option<Vec<TokenLogprob>>,

    /// A list of message refusal tokens with log probability information.
    #[serde(default)]
    pub refusal: Option<Vec<TokenLogprob>>,
}

// ── Delta Types ────────────────────────────────────────────────────────────

/// A chat completion delta generated by streamed model responses.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Delta {
    /// The contents of the chunk message.
    #[serde(default)]
    pub content: Option<String>,

    /// The role of the author of this message. Allowed: `"assistant"`.
    #[serde(default)]
    pub role: Option<String>,

    /// The refusal message generated by the model.
    #[serde(default)]
    pub refusal: Option<String>,

    /// Reasoning content for o-series reasoning models (streaming delta).
    #[serde(default)]
    pub reasoning_content: Option<String>,

    /// The tool calls generated by the model, such as function calls.
    #[serde(default)]
    pub tool_calls: Option<Vec<DeltaToolCall>>,

    /// Deprecated and replaced by `tool_calls`. The name and arguments of a
    /// function that should be called, as generated by the model.
    #[serde(default)]
    pub function_call: Option<DeltaFunctionCall>,
}

/// A tool call in a streaming delta.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeltaToolCall {
    /// The index of the tool call in the list of tool calls.
    pub index: i64,

    /// The ID of the tool call.
    #[serde(default)]
    pub id: Option<String>,

    /// The function that the model called.
    #[serde(default)]
    pub function: Option<DeltaFunction>,

    /// The type of the tool. Allowed: `"function"`.
    #[serde(rename = "type", default)]
    pub tool_type: Option<String>,
}

/// A function definition in a streaming delta tool call.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeltaFunction {
    /// The name of the function to call.
    #[serde(default)]
    pub name: Option<String>,

    /// The arguments to call the function with, as generated by the model in
    /// JSON format. Unlike the non-streaming variant, the arguments in a delta
    /// are a partial fragment.
    #[serde(default)]
    pub arguments: Option<String>,
}

/// Deprecated and replaced by `tool_calls`. A function call in a streaming delta.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeltaFunctionCall {
    /// The name of the function to call.
    #[serde(default)]
    pub name: Option<String>,

    /// The arguments to call the function with, as generated by the model in
    /// JSON format.
    #[serde(default)]
    pub arguments: Option<String>,
}

// ── Message Param (tagged union) ───────────────────────────────────────────

/// A message in the chat conversation. Tagged by `role`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum MessageRequest {
    /// Developer-provided instructions that the model should follow. With o1
    /// models and newer, use `developer` messages instead of `system`.
    #[serde(rename = "developer")]
    Developer(DeveloperMessage),

    /// Developer-provided instructions (legacy). With o1 models and newer, use
    /// `developer` messages for this purpose instead.
    #[serde(rename = "system")]
    System(SystemMessage),

    /// Messages sent by an end user, containing prompts or additional context.
    #[serde(rename = "user")]
    User(UserMessage),

    /// Messages sent by the model in response to user messages.
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),

    /// Tool call results sent back to the model.
    #[serde(rename = "tool")]
    Tool(ToolMessage),

    /// Deprecated. Function call results.
    #[serde(rename = "function")]
    Function(FunctionMessage),
}

// ── Individual Message Types ───────────────────────────────────────────────

/// Developer-provided instructions that the model should follow, regardless of
/// messages sent by the user. With o1 models and newer, use `developer` messages
/// instead of `system`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeveloperMessage {
    /// The contents of the developer message.
    pub content: MessageContent,

    /// An optional name for the participant. Provides the model information to
    /// differentiate between participants of the same role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Developer-provided instructions that the model should follow (legacy). With
/// o1 models and newer, use `developer` messages for this purpose instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SystemMessage {
    /// The contents of the system message.
    pub content: MessageContent,

    /// An optional name for the participant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Messages sent by an end user, containing prompts or additional context
/// information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UserMessage {
    /// The contents of the user message.
    pub content: UserContent,

    /// An optional name for the participant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Messages sent by the model in response to user messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AssistantMessage {
    /// The contents of the assistant message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<AssistantContent>,

    /// An optional name for the participant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// The refusal message by the assistant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,

    /// Data about a previous audio response from the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<AssistantAudio>,

    /// Reasoning content for o-series reasoning models.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,

    /// The tool calls generated by the model, such as function calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallRequest>>,

    /// Deprecated and replaced by `tool_calls`. The name and arguments of a
    /// function that should be called, as generated by the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCall>,
}

/// Tool call results sent back to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolMessage {
    /// The contents of the tool message.
    pub content: MessageContent,

    /// Tool call that this message is responding to.
    pub tool_call_id: String,
}

/// Deprecated. Function call results sent back to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionMessage {
    /// The contents of the function message.
    pub content: String,

    /// The name of the function to call.
    pub name: String,
}

// ── Content Types (untagged unions) ────────────────────────────────────────

/// Content for system and developer messages.
///
/// Can be a simple text string, or an array of text content parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// A plain text string.
    Text(String),
    /// An array of text content parts.
    Parts(Vec<TextContentPart>),
}

/// Content for user messages.
///
/// Can be a simple text string, or an array of multimodal content parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UserContent {
    /// A plain text string.
    Text(String),
    /// An array of multimodal content parts.
    Parts(Vec<ContentPart>),
}

/// Content for assistant messages.
///
/// Can be a simple text string, or an array of content parts (including
/// refusal parts when the assistant declines to answer).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AssistantContent {
    /// A plain text string.
    Text(String),
    /// An array of content parts.
    Parts(Vec<ContentPart>),
}

// ── Content Parts (tagged union) ───────────────────────────────────────────

/// A multimodal content part in a user or assistant message.
/// Tagged by `type`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    /// A text content part.
    /// [Learn more](https://platform.openai.com/docs/guides/text-generation).
    #[serde(rename = "text")]
    Text {
        /// The text content.
        text: String,
    },

    /// An image content part.
    /// [Learn more](https://platform.openai.com/docs/guides/vision).
    #[serde(rename = "image_url")]
    Image {
        /// The image URL data.
        image_url: ImageUrl,
    },

    /// An audio content part.
    /// [Learn more](https://platform.openai.com/docs/guides/audio).
    #[serde(rename = "input_audio")]
    Audio {
        /// The input audio data.
        input_audio: InputAudio,
    },

    /// A file content part.
    /// [Learn more](https://platform.openai.com/docs/guides/text).
    #[serde(rename = "file")]
    File {
        /// The file data.
        file: FileData,
    },

    /// A refusal content part (for assistant message content arrays).
    #[serde(rename = "refusal")]
    Refusal {
        /// The refusal reason text.
        refusal: String,
    },
}

// ── Content Sub-Types ──────────────────────────────────────────────────────

/// A text-only content part (used in system/developer message arrays).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TextContentPart {
    /// The text content.
    pub text: String,

    /// The type of the content part. Always `"text"`.
    #[serde(rename = "type", default = "default_text_type")]
    pub content_type: String,
}

fn default_text_type() -> String {
    "text".to_string()
}

/// An image URL used in image content parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImageUrl {
    /// Either a URL of the image or the base64 encoded image data.
    pub url: String,

    /// Specifies the detail level of the image. Allowed: `"auto"`, `"low"`,
    /// `"high"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Input audio data for audio content parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InputAudio {
    /// Base64 encoded audio data.
    pub data: String,

    /// The format of the encoded audio data. Allowed: `"wav"`, `"mp3"`.
    pub format: String,
}

/// File data for file content parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileData {
    /// Base64 encoded file data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_data: Option<String>,

    /// The ID of an uploaded file to use as input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,

    /// The name of the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

// ── Response Message ───────────────────────────────────────────────────────

/// A chat completion message generated by the model.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseMessage {
    /// The contents of the message.
    #[serde(default)]
    pub content: Option<String>,

    /// The refusal message generated by the model.
    #[serde(default)]
    pub refusal: Option<String>,

    /// The role of the author of this message. Always `"assistant"`.
    #[serde(default = "default_assistant_role")]
    pub role: String,

    /// Annotations for the message, such as URL citations when using web search.
    #[serde(default)]
    pub annotations: Option<Vec<Annotation>>,

    /// If the audio output modality is requested, this object contains data
    /// about the audio response from the model.
    #[serde(default)]
    pub audio: Option<ChatAudioResponse>,

    /// Deprecated and replaced by `tool_calls`. The name and arguments of a
    /// function that should be called, as generated by the model.
    #[serde(default)]
    pub function_call: Option<FunctionCall>,

    /// Reasoning content for o-series reasoning models.
    #[serde(default)]
    pub reasoning_content: Option<String>,

    /// The tool calls generated by the model, such as function calls.
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCallResponse>>,
}

fn default_assistant_role() -> String {
    "assistant".to_string()
}

// ── Tool Call Types (Response) ─────────────────────────────────────────────

/// A tool call in the response message. Tagged by `type`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ToolCallResponse {
    /// A function tool call.
    #[serde(rename = "function")]
    Function {
        /// The ID of the tool call.
        id: String,
        /// The function that the model called.
        function: ToolCallFunction,
    },

    /// A custom tool call.
    #[serde(rename = "custom")]
    Custom {
        /// The ID of the tool call.
        id: String,
        /// The custom tool data.
        custom: ToolCallCustom,
    },
}

/// The function that the model called.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolCallFunction {
    /// The name of the function to call.
    pub name: String,

    /// The arguments to call the function with, as generated by the model in
    /// JSON format. Note that the model may emit invalid JSON — you must
    /// validate it.
    pub arguments: String,
}

/// Custom tool call data returned by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolCallCustom {
    /// The name of the custom tool.
    pub name: String,

    /// The input to the custom tool, as generated by the model.
    pub input: String,
}

// ── Tool Call Param Types (Request / Assistant Message) ────────────────────

/// A tool call in an assistant message request. Tagged by `type`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolCallRequest {
    /// A function tool call.
    #[serde(rename = "function")]
    Function {
        /// The ID of the tool call.
        id: String,
        /// The function that the model called.
        function: ToolCallFunction,
    },

    /// A custom tool call.
    #[serde(rename = "custom")]
    Custom {
        /// The ID of the tool call.
        id: String,
        /// The custom tool data.
        custom: ToolCallCustom,
    },
}

// ── Tool Definitions (Request) ─────────────────────────────────────────────

/// A tool the model may call. Tagged by `type`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolRequest {
    /// A function tool.
    #[serde(rename = "function")]
    Function {
        /// The function definition.
        function: FunctionTool,
    },

    /// A custom tool.
    #[serde(rename = "custom")]
    Custom {
        /// The custom tool definition.
        custom: CustomTool,
    },
}

/// A function tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionTool {
    /// The name of the function to be called. Must be a-z, A-Z, 0-9, or
    /// contain underscores and dashes, with a maximum length of 64.
    pub name: String,

    /// A description of what the function does, used by the model to choose
    /// when and how to call the function.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// The parameters the functions accepts, described as a JSON Schema object.
    /// See the [guide](https://platform.openai.com/docs/guides/function-calling)
    /// for examples.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,

    /// Whether to enable strict schema adherence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// A custom tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CustomTool {
    /// The name of the custom tool.
    pub name: String,

    /// A description of what the custom tool does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// The format specification for the custom tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<CustomToolFormat>,
}

// ── Tool Choice (untagged union) ───────────────────────────────────────────

/// Controls which (if any) tool is called by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    /// `"none"` means the model will not call any tool. `"auto"` means the
    /// model can pick between generating a message or calling a tool.
    /// `"required"` means the model must call a tool.
    Mode(String),

    /// Force the model to call a specific function.
    Function(ToolChoiceFunction),

    /// Force the model to call a specific custom tool.
    Custom(ToolChoiceCustom),

    /// Constrain the tools available to the model to a pre-defined set.
    AllowedTools(ToolChoiceAllowedTools),
}

/// Specifies a function tool the model should use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolChoiceFunction {
    /// The type of tool choice. Always `"function"`.
    #[serde(rename = "type")]
    pub choice_type: String,

    /// The function to force.
    pub function: ToolChoiceFunctionName,
}

/// The name of the function to force.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolChoiceFunctionName {
    /// The name of the function to call.
    pub name: String,
}

/// Specifies a custom tool the model should use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolChoiceCustom {
    /// The type of tool choice. Always `"custom"`.
    #[serde(rename = "type")]
    pub choice_type: String,

    /// The custom tool to force.
    pub custom: ToolChoiceCustomName,
}

/// The name of the custom tool to force.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolChoiceCustomName {
    /// The name of the custom tool.
    pub name: String,
}

/// Constrains the tools available to the model to a pre-defined set.
/// Wire format: `{"type": "allowed_tools", "mode": "...", "tools": [...]}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolChoiceAllowedTools {
    /// Always `"allowed_tools"`.
    #[serde(rename = "type")]
    pub choice_type: String,
    /// Selection mode. Allowed: `"auto"`, `"required"`.
    pub mode: String,
    /// The set of allowed tools.
    pub tools: Vec<super::tool::SimplifiedTool>,
}

// ── Custom Tool Format ─────────────────────────────────────────────────────

/// Format specification for a custom tool. Tagged by `type`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CustomToolFormat {
    /// Free-form text format.
    #[serde(rename = "text")]
    Text,

    /// A grammar-based format constraint.
    #[serde(rename = "grammar")]
    Grammar {
        /// The grammar definition.
        grammar: Grammar,
    },
}

/// A grammar definition for custom tool format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Grammar {
    /// The grammar definition string.
    pub definition: String,

    /// The syntax of the grammar definition. Allowed: `"lark"`, `"regex"`.
    pub syntax: String,
}

// ── Audio Types ────────────────────────────────────────────────────────────

/// Parameters for audio output. Required when audio output is requested with
/// `modalities: ["audio"]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatAudio {
    /// The format of the output audio. Allowed: `"wav"`, `"aac"`, `"mp3"`,
    /// `"flac"`, `"opus"`, `"pcm16"`.
    pub format: String,

    /// The voice the model uses to respond.
    pub voice: AudioVoice,
}

/// The voice the model uses to respond. Can be a built-in voice name or a
/// custom voice ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AudioVoice {
    /// A built-in voice. Allowed: `"alloy"`, `"ash"`, `"ballad"`, `"coral"`,
    /// `"echo"`, `"sage"`, `"shimmer"`, `"verse"`, `"marin"`, `"cedar"`.
    BuiltIn(String),

    /// A custom voice identified by ID (e.g. `"voice_1234"`).
    Id(VoiceId),
}

/// A custom voice reference identified by ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VoiceId {
    /// The ID of the custom voice (e.g. `"voice_1234"`).
    pub id: String,
}

/// Audio response data from the model.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatAudioResponse {
    /// Unique identifier for this audio response.
    pub id: String,

    /// Base64 encoded audio data generated by the model.
    pub data: String,

    /// The Unix timestamp (in seconds) when this audio response will expire.
    pub expires_at: i64,

    /// Transcript of the audio generated by the model.
    pub transcript: String,
}

/// Reference to a previous audio response from the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AssistantAudio {
    /// The ID of the previous audio response to reference.
    pub id: String,
}

// ── Token Logprobs ─────────────────────────────────────────────────────────

/// Log probability information for a token.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TokenLogprob {
    /// The token.
    pub token: String,

    /// A list of integers representing the UTF-8 bytes representation of the
    /// token. Useful in instances where characters are represented by multiple
    /// tokens and their byte representations must be combined to generate the
    /// correct text representation. Can be null if the token has no bytes
    /// representation.
    #[serde(default)]
    pub bytes: Option<Vec<i64>>,

    /// The log probability of this token, if it is within the top 20 most
    /// likely tokens.
    pub logprob: f64,

    /// List of the most likely tokens and their log probability, at this token
    /// position.
    pub top_logprobs: Vec<TopLogprob>,
}

/// A top log probability token.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TopLogprob {
    /// The token.
    pub token: String,

    /// A list of integers representing the UTF-8 bytes representation of the
    /// token. Useful in instances where characters are represented by multiple
    /// tokens.
    #[serde(default)]
    pub bytes: Option<Vec<i64>>,

    /// The log probability of this token.
    pub logprob: f64,
}

// ── Usage ──────────────────────────────────────────────────────────────────

/// Usage statistics for the completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Usage {
    /// Number of tokens in the prompt.
    pub prompt_tokens: i64,

    /// Number of tokens in the generated completion.
    pub completion_tokens: i64,

    /// Total number of tokens used in the request (prompt + completion).
    pub total_tokens: i64,

    /// DeepSeek-style: number of tokens fetched from prompt cache (hit).
    #[serde(default)]
    pub prompt_cache_hit_tokens: Option<i64>,

    /// DeepSeek-style: number of tokens not in prompt cache (miss).
    #[serde(default)]
    pub prompt_cache_miss_tokens: Option<i64>,

    /// Breakdown of tokens in the prompt.
    #[serde(default)]
    pub prompt_tokens_details: Option<PromptTokensDetails>,

    /// Breakdown of tokens in the completion.
    #[serde(default)]
    pub completion_tokens_details: Option<CompletionTokensDetails>,
}

/// Details about tokens in the prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PromptTokensDetails {
    /// Number of tokens that were served from the cache.
    pub cached_tokens: i64,

    /// Number of audio input tokens.
    #[serde(default)]
    pub audio_tokens: i64,
}

/// Details about tokens in the completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CompletionTokensDetails {
    /// Number of tokens used for reasoning.
    pub reasoning_tokens: i64,

    /// Number of audio output tokens.
    #[serde(default)]
    pub audio_tokens: i64,

    /// Number of tokens generated as part of accepted predicted outputs.
    #[serde(default)]
    pub accepted_prediction_tokens: i64,

    /// Number of tokens generated as part of rejected predicted outputs.
    #[serde(default)]
    pub rejected_prediction_tokens: i64,
}

// ── Annotation ─────────────────────────────────────────────────────────────

/// A URL citation when using web search.
#[derive(Debug, Clone, Deserialize)]
pub struct Annotation {
    /// The type of annotation. Always `"url_citation"`.
    #[serde(rename = "type", default = "default_url_citation_type")]
    pub annotation_type: String,

    /// The URL citation details.
    pub url_citation: UrlCitation,
}

fn default_url_citation_type() -> String {
    "url_citation".to_string()
}

/// A URL citation detail.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UrlCitation {
    /// The index of the last character of the cited text in the message content.
    pub end_index: i64,

    /// The index of the first character of the cited text in the message content.
    pub start_index: i64,

    /// The title of the URL citation.
    pub title: String,

    /// The URL being cited.
    pub url: String,
}

// ── Stream Options ─────────────────────────────────────────────────────────

/// Options for streaming response. Only set this when `stream: true`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StreamOptions {
    /// If set, an additional chunk will be streamed before the `data: [DONE]`
    /// message. The `usage` field on this chunk shows the token usage
    /// statistics for the entire request, and the `choices` field will always
    /// be an empty array. All other chunks will also include a `usage` field,
    /// but with a null value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_usage: Option<bool>,

    /// If set, the response will include obfuscation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_obfuscation: Option<bool>,
}

// ── Stop (untagged union) ──────────────────────────────────────────────────

/// Up to 4 sequences where the API will stop generating further tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Stop {
    /// A single stop sequence.
    Single(String),
    /// Up to 4 stop sequences.
    Multiple(Vec<String>),
}

// ── Function Call (deprecated) ─────────────────────────────────────────────

/// Deprecated. Controls which (if any) function is called by the model.
/// Can be a string mode or a specific named function.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FunctionCallOption {
    /// `"none"` means the model will not call a function. `"auto"` means the
    /// model can pick between generating a message or calling a function.
    Mode(String),

    /// Specifying a particular function via `{"name": "my_function"}` forces
    /// the model to call that function.
    Named {
        /// The name of the function to call.
        name: String,
    },
}

/// Deprecated and replaced by `tool_calls`. The name and arguments of a
/// function that should be called, as generated by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionCall {
    /// The name of the function to call.
    pub name: String,

    /// The arguments to call the function with, as generated by the model in
    /// JSON format.
    pub arguments: String,
}

// ── Prediction ─────────────────────────────────────────────────────────────

/// Configuration for a Predicted Output, which can greatly improve response
/// times when large parts of the model response are known ahead of time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionContent {
    /// The type of the predicted content. Always `"content"`.
    #[serde(rename = "type", default = "default_prediction_type")]
    pub content_type: String,

    /// The predicted output content.
    pub content: PredictionContentValue,
}

fn default_prediction_type() -> String {
    "content".to_string()
}

/// The value of predicted content. Can be a simple string or an array of text
/// content parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PredictionContentValue {
    /// A plain text string.
    Text(String),
    /// An array of text content parts.
    Parts(Vec<TextContentPart>),
}

// ── Response Format (untagged union) ───────────────────────────────────────

/// Specifies the format that the model must output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseFormat {
    /// Default text format.
    Text(TextFormat),
    /// JSON Schema format for structured outputs.
    JsonSchema(JsonSchemaFormat),
    /// JSON object format (legacy mode).
    JsonObject(JsonObjectFormat),
}

/// Text response format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextFormat {
    /// The type of response format. Always `"text"`.
    #[serde(rename = "type", default = "default_text_format")]
    pub format_type: String,
}

fn default_text_format() -> String {
    "text".to_string()
}

/// JSON Schema response format for
/// [Structured Outputs](https://platform.openai.com/docs/guides/structured-outputs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSchemaFormat {
    /// The type of response format. Always `"json_schema"`.
    #[serde(rename = "type", default = "default_json_schema_format")]
    pub format_type: String,

    /// The JSON Schema definition.
    pub json_schema: JsonSchema,
}

fn default_json_schema_format() -> String {
    "json_schema".to_string()
}

/// JSON object response format (legacy mode). Enables JSON mode, which
/// guarantees the message the model generates is valid JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonObjectFormat {
    /// The type of response format. Always `"json_object"`.
    #[serde(rename = "type", default = "default_json_object_format")]
    pub format_type: String,
}

fn default_json_object_format() -> String {
    "json_object".to_string()
}

/// A JSON Schema definition for structured outputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct JsonSchema {
    /// The name of the response format. Must be a-z, A-Z, 0-9, or contain
    /// underscores and dashes, with a maximum length of 64.
    pub name: String,

    /// A description of what the response format is for, used by the model to
    /// determine how to generate responses in the format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// The JSON Schema definition. Omit for JSON mode (in this case the model
    /// can output any valid JSON).
    #[serde(rename = "schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,

    /// Whether to enable strict schema adherence when generating the output.
    /// If set to `true`, the model will always follow the exact JSON Schema
    /// in the generated output. Only supported when using Structured Outputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

// ── Web Search Options ─────────────────────────────────────────────────────

/// Options for the web search tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchOptions {
    /// Approximate location parameters for the search.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<ChatUserLocation>,

    /// The amount of context to use for web search. Allowed: `"low"`,
    /// `"medium"`, `"high"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<String>,
}

/// Approximate location parameters for web search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatUserLocation {
    /// The type of user location. Always `"approximate"`.
    #[serde(rename = "type", default = "default_user_location_type")]
    pub location_type: String,

    /// Approximate location details.
    pub approximate: UserLocationApproximate,
}

fn default_user_location_type() -> String {
    "approximate".to_string()
}

/// Approximate location parameters for the search.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UserLocationApproximate {
    /// Free text input for the city of the user (e.g. `"San Francisco"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,

    /// Two-letter country code of the user (e.g. `"US"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,

    /// Free text input for the region of the user (e.g. `"California"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,

    /// IANA timezone of the user (e.g. `"America/Los_Angeles"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}
