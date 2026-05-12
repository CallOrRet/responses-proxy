//! Typed API schema — complete struct definitions aligned with the
//! [openai-go](https://github.com/openai/openai-go) SDK.
//!
//! Each sub-module corresponds to a logical domain within the OpenAI API.
//! Struct names omit the domain prefix because the module path already
//! provides the scope (e.g. `typed::chat::Request` instead of
//! `ChatCompletionNewParams`).

pub mod chat;
pub mod event;
pub mod item;
pub mod responses;
pub mod streaming;
pub mod tool;
pub mod websocket;

// ── Shared enums used across sub-modules ──────────────────────────────────

use serde::{Deserialize, Serialize};

/// The role of a message participant.
///
/// [openai-go] `constant.System` / `constant.User` / `constant.Assistant` /
/// `constant.Developer` / `constant.Function` / `constant.Tool`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Developer,
    Function,
    Tool,
}

/// Processing service tier.
///
/// [openai-go] `ResponseServiceTier` / `ChatCompletionServiceTier`
///
/// - `Auto`: Use project-configured service tier (default)
/// - `Default`: Standard pricing and performance
/// - `Flex`: [Flex processing](https://platform.openai.com/docs/guides/flex-processing)
/// - `Scale`: Scaled processing
/// - `Priority`: [Priority processing](https://openai.com/api-priority-processing/)
///
/// Note: The served tier may differ from the requested tier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTier {
    Auto,
    Default,
    Flex,
    Scale,
    Priority,
}

/// Prompt cache retention policy.
///
/// [openai-go] `ResponseNewParamsPromptCacheRetention`
///
/// - `InMemory`: Default behavior, cache kept in memory
/// - `24h`: Extend cache retention up to 24 hours
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptCacheRetention {
    #[serde(rename = "in_memory")]
    InMemory,
    #[serde(rename = "24h")]
    Hours24,
}

/// Truncation strategy for model responses.
///
/// [openai-go] `ResponseNewParamsTruncation`
///
/// - `Auto`: Automatically truncate oldest messages when exceeding context window
/// - `Disabled`: Return 400 error when exceeding context window (default)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TruncationStrategy {
    Auto,
    #[default]
    Disabled,
}

/// Reasoning effort levels.
///
/// [openai-go] `shared.ReasoningEffort`
///
/// Note: Only applies to o-series and gpt-5 reasoning models.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

/// Verbosity level for text output.
///
/// [openai-go] `ResponseTextConfigVerbosity`
///
/// - `Low`: Concise output
/// - `Medium`: Moderate detail (default)
/// - `High`: Detailed output
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verbosity {
    Low,
    Medium,
    High,
}

/// Built-in tool choice modes.
///
/// [openai-go] `ToolChoiceOptions`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoiceMode {
    Auto,
    None,
    Required,
}

/// Stream event type label. Each variant maps to a wire-format event string
/// such as `"response.output_text.delta"`.
///
/// The [`AsRef<str>`] impl returns the exact wire string; use it for
/// serialising the `type` field of outgoing SSE / WebSocket frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    // ── Lifecycle ──
    ResponseCreated,
    ResponseQueued,
    ResponseInProgress,
    ResponseCompleted,
    ResponseFailed,
    ResponseIncomplete,
    ResponseCancelled,
    ResponseDone,

    // ── Output items ──
    ResponseOutputItemAdded,
    ResponseOutputItemDone,

    // ── Content parts ──
    ResponseContentPartAdded,
    ResponseContentPartDone,

    // ── Text ──
    ResponseOutputTextDelta,
    ResponseOutputTextDone,
    ResponseOutputTextAnnotationAdded,

    // ── Refusal ──
    ResponseRefusalDelta,
    ResponseRefusalDone,

    // ── Audio ──
    ResponseAudioDelta,
    ResponseAudioDone,
    ResponseAudioTranscriptDelta,
    ResponseAudioTranscriptDone,
    ResponseOutputAudioDelta,
    ResponseOutputAudioDone,
    ResponseOutputAudioTranscriptDelta,
    ResponseOutputAudioTranscriptDone,

    // ── Function call ──
    ResponseFunctionCallArgumentsDelta,
    ResponseFunctionCallArgumentsDone,

    // ── Code interpreter ──
    ResponseCodeInterpreterCallInProgress,
    ResponseCodeInterpreterCallInterpreting,
    ResponseCodeInterpreterCallCodeDelta,
    ResponseCodeInterpreterCallCodeDone,
    ResponseCodeInterpreterCallCompleted,

    // ── File search ──
    ResponseFileSearchCallInProgress,
    ResponseFileSearchCallSearching,
    ResponseFileSearchCallCompleted,

    // ── Web search ──
    ResponseWebSearchCallInProgress,
    ResponseWebSearchCallSearching,
    ResponseWebSearchCallCompleted,

    // ── Image generation ──
    ResponseImageGenerationCallInProgress,
    ResponseImageGenerationCallGenerating,
    ResponseImageGenerationCallPartialImage,
    ResponseImageGenerationCallCompleted,

    // ── MCP ──
    ResponseMcpCallInProgress,
    ResponseMcpCallCompleted,
    ResponseMcpCallFailed,
    ResponseMcpCallArgumentsDelta,
    ResponseMcpCallArgumentsDone,
    ResponseMcpListToolsInProgress,
    ResponseMcpListToolsCompleted,
    ResponseMcpListToolsFailed,

    // ── Reasoning ──
    ResponseReasoningTextDelta,
    ResponseReasoningTextDone,
    ResponseReasoningSummaryTextDelta,
    ResponseReasoningSummaryTextDone,
    ResponseReasoningSummaryPartAdded,
    ResponseReasoningSummaryPartDone,

    // ── Custom tool ──
    ResponseCustomToolCallInputDelta,
    ResponseCustomToolCallInputDone,

    // ── Misc ──
    Error,
    ResponseInputTokens,
    RateLimitsUpdated,
}

impl AsRef<str> for EventType {
    fn as_ref(&self) -> &str {
        match self {
            Self::ResponseCreated => "response.created",
            Self::ResponseQueued => "response.queued",
            Self::ResponseInProgress => "response.in_progress",
            Self::ResponseCompleted => "response.completed",
            Self::ResponseFailed => "response.failed",
            Self::ResponseIncomplete => "response.incomplete",
            Self::ResponseCancelled => "response.cancelled",
            Self::ResponseDone => "response.done",
            Self::ResponseOutputItemAdded => "response.output_item.added",
            Self::ResponseOutputItemDone => "response.output_item.done",
            Self::ResponseContentPartAdded => "response.content_part.added",
            Self::ResponseContentPartDone => "response.content_part.done",
            Self::ResponseOutputTextDelta => "response.output_text.delta",
            Self::ResponseOutputTextDone => "response.output_text.done",
            Self::ResponseOutputTextAnnotationAdded => "response.output_text.annotation.added",
            Self::ResponseRefusalDelta => "response.refusal.delta",
            Self::ResponseRefusalDone => "response.refusal.done",
            Self::ResponseAudioDelta => "response.audio.delta",
            Self::ResponseAudioDone => "response.audio.done",
            Self::ResponseAudioTranscriptDelta => "response.audio.transcript.delta",
            Self::ResponseAudioTranscriptDone => "response.audio.transcript.done",
            Self::ResponseOutputAudioDelta => "response.output_audio.delta",
            Self::ResponseOutputAudioDone => "response.output_audio.done",
            Self::ResponseOutputAudioTranscriptDelta => "response.output_audio_transcript.delta",
            Self::ResponseOutputAudioTranscriptDone => "response.output_audio_transcript.done",
            Self::ResponseFunctionCallArgumentsDelta => "response.function_call_arguments.delta",
            Self::ResponseFunctionCallArgumentsDone => "response.function_call_arguments.done",
            Self::ResponseCodeInterpreterCallInProgress => {
                "response.code_interpreter_call.in_progress"
            }
            Self::ResponseCodeInterpreterCallInterpreting => {
                "response.code_interpreter_call.interpreting"
            }
            Self::ResponseCodeInterpreterCallCodeDelta => {
                "response.code_interpreter_call_code.delta"
            }
            Self::ResponseCodeInterpreterCallCodeDone => "response.code_interpreter_call_code.done",
            Self::ResponseCodeInterpreterCallCompleted => {
                "response.code_interpreter_call.completed"
            }
            Self::ResponseFileSearchCallInProgress => "response.file_search_call.in_progress",
            Self::ResponseFileSearchCallSearching => "response.file_search_call.searching",
            Self::ResponseFileSearchCallCompleted => "response.file_search_call.completed",
            Self::ResponseWebSearchCallInProgress => "response.web_search_call.in_progress",
            Self::ResponseWebSearchCallSearching => "response.web_search_call.searching",
            Self::ResponseWebSearchCallCompleted => "response.web_search_call.completed",
            Self::ResponseImageGenerationCallInProgress => {
                "response.image_generation_call.in_progress"
            }
            Self::ResponseImageGenerationCallGenerating => {
                "response.image_generation_call.generating"
            }
            Self::ResponseImageGenerationCallPartialImage => {
                "response.image_generation_call.partial_image"
            }
            Self::ResponseImageGenerationCallCompleted => {
                "response.image_generation_call.completed"
            }
            Self::ResponseMcpCallInProgress => "response.mcp_call.in_progress",
            Self::ResponseMcpCallCompleted => "response.mcp_call.completed",
            Self::ResponseMcpCallFailed => "response.mcp_call.failed",
            Self::ResponseMcpCallArgumentsDelta => "response.mcp_call_arguments.delta",
            Self::ResponseMcpCallArgumentsDone => "response.mcp_call_arguments.done",
            Self::ResponseMcpListToolsInProgress => "response.mcp_list_tools.in_progress",
            Self::ResponseMcpListToolsCompleted => "response.mcp_list_tools.completed",
            Self::ResponseMcpListToolsFailed => "response.mcp_list_tools.failed",
            Self::ResponseReasoningTextDelta => "response.reasoning_text.delta",
            Self::ResponseReasoningTextDone => "response.reasoning_text.done",
            Self::ResponseReasoningSummaryTextDelta => "response.reasoning_summary_text.delta",
            Self::ResponseReasoningSummaryTextDone => "response.reasoning_summary_text.done",
            Self::ResponseReasoningSummaryPartAdded => "response.reasoning_summary_part.added",
            Self::ResponseReasoningSummaryPartDone => "response.reasoning_summary_part.done",
            Self::ResponseCustomToolCallInputDelta => "response.custom_tool_call_input.delta",
            Self::ResponseCustomToolCallInputDone => "response.custom_tool_call_input.done",
            Self::Error => "error",
            Self::ResponseInputTokens => "response.input_tokens",
            Self::RateLimitsUpdated => "rate_limits.updated",
        }
    }
}
