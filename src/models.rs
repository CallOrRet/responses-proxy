#![allow(dead_code)]
use serde::{Deserialize, Serialize};

// ── Responses API Request ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponsesRequest {
    pub model: String,
    pub input: Input,
    #[serde(default)]
    pub instructions: Option<String>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
    #[serde(default)]
    pub tools: Option<Vec<ToolParam>>,
    #[serde(default)]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(default)]
    pub stream: Option<bool>,
    #[serde(default)]
    pub stop: Option<Stop>,
    #[serde(default)]
    pub top_logprobs: Option<u32>,
    #[serde(default)]
    pub previous_response_id: Option<String>,
    #[serde(default)]
    pub store: Option<bool>,
    #[serde(default)]
    pub metadata: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    pub reasoning: Option<serde_json::Value>,
    #[serde(default)]
    pub text: Option<serde_json::Value>,
}

/// The `input` field: either a plain string or an array of typed input items.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Input {
    String(String),
    Array(Vec<InputItem>),
}

impl Input {
    pub fn is_empty(&self) -> bool {
        match self {
            Input::String(s) => s.is_empty(),
            Input::Array(a) => a.is_empty(),
        }
    }
}

/// A single item in the `input` array, discriminated by `type`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum InputItem {
    #[serde(rename = "message")]
    Message(InputMessage),
    #[serde(rename = "function_call")]
    FunctionCall(FunctionCallItem),
    #[serde(rename = "function_call_output")]
    FunctionCallOutput(FunctionCallOutputItem),
    #[serde(rename = "reasoning")]
    Reasoning(InputReasoning),
    // Catch-all for other item types we don't explicitly handle
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

/// A message input item: `{"type": "message", "role": "...", "content": [...]}`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InputMessage {
    pub role: MessageRole,
    #[serde(default)]
    pub content: Vec<InputContentBlock>,
    #[serde(default)]
    pub status: Option<String>,
}

/// Roles allowed in Responses API input messages.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    System,
    Developer,
    Assistant,
}

/// One content block inside a message.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum InputContentBlock {
    #[serde(rename = "input_text")]
    Text { text: String },
    #[serde(rename = "input_image")]
    Image(serde_json::Value),
    #[serde(rename = "input_file")]
    File(serde_json::Value),
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

/// `{"type": "function_call", "call_id": "...", "name": "...", "arguments": "..."}`
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionCallItem {
    pub call_id: String,
    pub name: String,
    pub arguments: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

/// `{"type": "function_call_output", "call_id": "...", "output": "..."}`
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionCallOutputItem {
    pub call_id: String,
    pub output: FunctionCallOutputValue,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

/// The `output` field of a function_call_output item: either a string or an array.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum FunctionCallOutputValue {
    String(String),
    Array(Vec<serde_json::Value>),
}

/// A reasoning input item: `{"type": "reasoning", "id": "...", "summary": [...]}`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InputReasoning {
    pub id: String,
    #[serde(default)]
    pub summary: Vec<serde_json::Value>,
}

/// Tool definition in Responses API format (flattened).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolParam {
    #[serde(rename = "type")]
    pub tool_type: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parameters: Option<serde_json::Value>,
    #[serde(default)]
    pub strict: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Stop {
    Single(String),
    Multiple(Vec<String>),
}

// ── Responses API Response ─────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponsesResponse {
    pub id: String,
    pub object: &'static str, // always "response"
    pub created_at: f64,
    pub status: String,
    pub model: String,
    pub output: Vec<OutputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ResponseUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incomplete_details: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
}

/// An output item, discriminated by `type`.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum OutputItem {
    #[serde(rename = "message")]
    Message(OutputMessage),
    #[serde(rename = "function_call")]
    FunctionCall(OutputFunctionCall),
    #[serde(rename = "reasoning")]
    Reasoning(OutputReasoning),
}

/// A reasoning output item (DeepSeek thinking mode).
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct OutputReasoning {
    pub id: String,
    pub summary: Vec<serde_json::Value>,
}

/// A message output item.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct OutputMessage {
    pub id: String,
    pub role: &'static str, // always "assistant"
    pub status: &'static str,
    pub content: Vec<OutputContentBlock>,
}

/// One content block in an output message.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum OutputContentBlock {
    #[serde(rename = "output_text")]
    Text {
        text: String,
        annotations: Vec<serde_json::Value>,
    },
    #[serde(rename = "refusal")]
    Refusal { refusal: String },
}

/// A function call output item.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct OutputFunctionCall {
    pub id: String,
    pub call_id: String,
    pub name: String,
    pub arguments: String,
    pub status: &'static str,
}

/// Responses API usage stats.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    pub input_tokens_details: InputTokensDetails,
    pub output_tokens_details: OutputTokensDetails,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct InputTokensDetails {
    pub cached_tokens: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct OutputTokensDetails {
    pub reasoning_tokens: u32,
}

// ── Chat Completions API Request ───────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ChatTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Stop>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
}

#[derive(Debug, Serialize)]
pub struct ThinkingConfig {
    #[serde(rename = "type")]
    pub thinking_type: String,
}

#[derive(Debug, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: ChatMessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChatToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ChatMessageContent {
    String(String),
    Null, // for assistant messages with only tool_calls
}

impl ChatMessageContent {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            ChatMessageContent::String(s) => Some(s),
            ChatMessageContent::Null => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String, // "function"
    pub function: ChatFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Serialize)]
pub struct ChatTool {
    #[serde(rename = "type")]
    pub tool_type: String, // "function"
    pub function: ChatFunctionDef,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatFunctionDef {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

// ── Chat Completions API Response ──────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    #[serde(default)]
    pub usage: Option<ChatUsage>,
    #[serde(default)]
    pub error: Option<ChatError>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatResponseMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatResponseMessage {
    pub role: Option<String>,
    pub content: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ChatToolCall>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    #[serde(default)]
    pub completion_tokens_details: Option<serde_json::Value>,
    #[serde(default)]
    pub prompt_tokens_details: Option<serde_json::Value>,
    #[serde(default)]
    pub prompt_cache_hit_tokens: Option<u32>,
    #[serde(default)]
    pub prompt_cache_miss_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct ChatError {
    pub message: String,
    #[serde(default)]
    pub code: Option<String>,
}

// ── SSE Streaming Types ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatCompletionChunk {
    pub id: Option<String>,
    pub object: Option<String>,
    pub created: Option<u64>,
    pub model: Option<String>,
    #[serde(default)]
    pub choices: Vec<ChatChunkChoice>,
    #[serde(default)]
    pub usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatChunkChoice {
    pub index: u32,
    pub delta: Option<ChatChunkDelta>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatChunkDelta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<serde_json::Value>>,
}
