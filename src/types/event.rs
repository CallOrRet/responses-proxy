//! Streaming event types — SSE / WebSocket receive events for the OpenAI Responses API.
//!
//! Contains the `ResponseStreamEventUnion` union and all its variants.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::item::OutputItem;
use super::responses::Response;

// ── Top-level Union ──────────────────────────────────────────────────────────────

/// Stream event union. The `type` field maps to the wire-format discriminator.
///
/// Contains all possible SSE / WebSocket receive event variants for the Responses API,
/// plus an `UnknownVariant` fallback for forward compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    // ── Lifecycle Events ──
    /// Response created successfully.
    #[serde(rename = "response.created")]
    Created(Created),
    /// Response has been queued for processing.
    #[serde(rename = "response.queued")]
    Queued(Queued),
    /// Response generation is in progress.
    #[serde(rename = "response.in_progress")]
    InProgress(InProgress),
    /// Response completed successfully.
    #[serde(rename = "response.completed")]
    Completed(Completed),
    /// Response failed.
    #[serde(rename = "response.failed")]
    Failed(Failed),
    /// Response ended in an incomplete state (e.g., reached token limit).
    #[serde(rename = "response.incomplete")]
    Incomplete(Incomplete),

    // ── Output Item Events ──
    /// A new output item was added.
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded(OutputItemAdded),
    /// An output item has completed.
    #[serde(rename = "response.output_item.done")]
    OutputItemDone(OutputItemDone),

    // ── Content Part Events ──
    /// A new content part was added.
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded(ContentPartAdded),
    /// A content part has completed.
    #[serde(rename = "response.content_part.done")]
    ContentPartDone(ContentPartDone),

    // ── Text Events ──
    /// Output text delta (streamed chunk).
    #[serde(rename = "response.output_text.delta")]
    TextDelta(TextDelta),
    /// Output text has completed.
    #[serde(rename = "response.output_text.done")]
    TextDone(TextDone),
    /// Annotation added to output text (e.g., citation link).
    #[serde(rename = "response.output_text.annotation.added")]
    TextAnnotationAdded(TextAnnotationAdded),

    // ── Refusal Events ──
    /// Refusal text delta (streamed chunk when the model refuses to answer).
    #[serde(rename = "response.refusal.delta")]
    RefusalDelta(RefusalDelta),
    /// Refusal text has completed.
    #[serde(rename = "response.refusal.done")]
    RefusalDone(RefusalDone),

    // ── Audio Events ──
    /// Audio response delta (base64-encoded).
    #[serde(rename = "response.audio.delta")]
    AudioDelta(AudioDelta),
    /// Audio response has completed.
    #[serde(rename = "response.audio.done")]
    AudioDone(AudioDone),
    /// Audio transcript text delta.
    #[serde(rename = "response.audio.transcript.delta")]
    AudioTranscriptDelta(AudioTranscriptDelta),
    /// Audio transcript text has completed.
    #[serde(rename = "response.audio.transcript.done")]
    AudioTranscriptDone(AudioTranscriptDone),

    // ── Function Call Events ──
    /// Function call arguments delta (streamed chunk).
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta(FunctionCallArgumentsDelta),
    /// Function call arguments have completed.
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone(FunctionCallArgumentsDone),

    // ── Code Interpreter Events ──
    /// Code interpreter code delta (streamed chunk).
    #[serde(rename = "response.code_interpreter_call_code.delta")]
    CodeInterpreterCallCodeDelta(CodeInterpreterCallCodeDelta),
    /// Code interpreter code has completed.
    #[serde(rename = "response.code_interpreter_call_code.done")]
    CodeInterpreterCallCodeDone(CodeInterpreterCallCodeDone),
    /// Code interpreter call is in progress.
    #[serde(rename = "response.code_interpreter_call.in_progress")]
    CodeInterpreterCallInProgress(CodeInterpreterCallInProgress),
    /// Code interpreter is executing code.
    #[serde(rename = "response.code_interpreter_call.interpreting")]
    CodeInterpreterCallInterpreting(CodeInterpreterCallInterpreting),
    /// Code interpreter call has completed.
    #[serde(rename = "response.code_interpreter_call.completed")]
    CodeInterpreterCallCompleted(CodeInterpreterCallCompleted),

    // ── File Search Events ──
    /// File search call is in progress.
    #[serde(rename = "response.file_search_call.in_progress")]
    FileSearchCallInProgress(FileSearchCallInProgress),
    /// File search is querying.
    #[serde(rename = "response.file_search_call.searching")]
    FileSearchCallSearching(FileSearchCallSearching),
    /// File search call has completed.
    #[serde(rename = "response.file_search_call.completed")]
    FileSearchCallCompleted(FileSearchCallCompleted),

    // ── Web Search Events ──
    /// Web search call is in progress.
    #[serde(rename = "response.web_search_call.in_progress")]
    WebSearchCallInProgress(WebSearchCallInProgress),
    /// Web search is querying.
    #[serde(rename = "response.web_search_call.searching")]
    WebSearchCallSearching(WebSearchCallSearching),
    /// Web search call has completed.
    #[serde(rename = "response.web_search_call.completed")]
    WebSearchCallCompleted(WebSearchCallCompleted),

    // ── Image Generation Events ──
    /// Image generation call is in progress.
    #[serde(rename = "response.image_generation_call.in_progress")]
    ImageGenCallInProgress(ImageGenCallInProgress),
    /// Image generation is processing.
    #[serde(rename = "response.image_generation_call.generating")]
    ImageGenCallGenerating(ImageGenCallGenerating),
    /// Partial image from image generation (base64-encoded).
    #[serde(rename = "response.image_generation_call.partial_image")]
    ImageGenCallPartialImage(ImageGenCallPartialImage),
    /// Image generation call has completed.
    #[serde(rename = "response.image_generation_call.completed")]
    ImageGenCallCompleted(ImageGenCallCompleted),

    // ── MCP Call Events ──
    /// MCP tool call is in progress.
    #[serde(rename = "response.mcp_call.in_progress")]
    McpCallInProgress(McpCallInProgress),
    /// MCP tool call has completed.
    #[serde(rename = "response.mcp_call.completed")]
    McpCallCompleted(McpCallCompleted),
    /// MCP tool call failed.
    #[serde(rename = "response.mcp_call.failed")]
    McpCallFailed(McpCallFailed),
    /// MCP tool call arguments delta (streamed chunk).
    #[serde(rename = "response.mcp_call_arguments.delta")]
    McpCallArgumentsDelta(McpCallArgumentsDelta),
    /// MCP tool call arguments have completed.
    #[serde(rename = "response.mcp_call_arguments.done")]
    McpCallArgumentsDone(McpCallArgumentsDone),

    // ── MCP List Tools Events ──
    /// MCP tool list retrieval is in progress.
    #[serde(rename = "response.mcp_list_tools.in_progress")]
    McpListToolsInProgress(McpListToolsInProgress),
    /// MCP tool list retrieval has completed.
    #[serde(rename = "response.mcp_list_tools.completed")]
    McpListToolsCompleted(McpListToolsCompleted),
    /// MCP tool list retrieval failed.
    #[serde(rename = "response.mcp_list_tools.failed")]
    McpListToolsFailed(McpListToolsFailed),

    // ── Reasoning Events ──
    /// Reasoning text delta (streamed chunk).
    #[serde(rename = "response.reasoning_text.delta")]
    ReasoningTextDelta(ReasoningTextDelta),
    /// Reasoning text has completed.
    #[serde(rename = "response.reasoning_text.done")]
    ReasoningTextDone(ReasoningTextDone),
    /// Reasoning summary text delta (streamed chunk).
    #[serde(rename = "response.reasoning_summary_text.delta")]
    ReasoningSummaryTextDelta(ReasoningSummaryTextDelta),
    /// Reasoning summary text has completed.
    #[serde(rename = "response.reasoning_summary_text.done")]
    ReasoningSummaryTextDone(ReasoningSummaryTextDone),
    /// A reasoning summary part was added.
    #[serde(rename = "response.reasoning_summary_part.added")]
    ReasoningSummaryPartAdded(ReasoningSummaryPartAdded),
    /// A reasoning summary part has completed.
    #[serde(rename = "response.reasoning_summary_part.done")]
    ReasoningSummaryPartDone(ReasoningSummaryPartDone),

    // ── Custom Tool Events ──
    /// Custom tool call input delta (streamed chunk).
    #[serde(rename = "response.custom_tool_call_input.delta")]
    CustomToolCallInputDelta(CustomToolCallInputDelta),
    /// Custom tool call input has completed.
    #[serde(rename = "response.custom_tool_call_input.done")]
    CustomToolCallInputDone(CustomToolCallInputDone),

    // ── Error Event ──
    /// An error occurred during streaming.
    #[serde(rename = "error")]
    Error(Error),

    /// Catch-all for unknown event types, for forward compatibility.
    #[serde(untagged)]
    UnknownVariant(Value),
}

// ── Lifecycle Events ────────────────────────────────────────────────────────────

/// Response created successfully.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Created {
    /// The full response object.
    pub response: Response,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Response has been queued for processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Queued {
    /// The full response object.
    pub response: Response,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Response generation is in progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InProgress {
    /// The full response object.
    pub response: Response,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Response completed successfully.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Completed {
    /// The full response object.
    pub response: Response,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Response failed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Failed {
    /// The full response object.
    pub response: Response,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Response ended in an incomplete state (e.g., reached token limit).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Incomplete {
    /// The full response object.
    pub response: Response,
    /// Event sequence number.
    pub sequence_number: i64,
}

// ── Output Item Events ──────────────────────────────────────────────────────────────

/// A new output item was added.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OutputItemAdded {
    /// The added output item.
    pub item: OutputItem,
    /// Index of the output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// An output item has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OutputItemDone {
    /// The completed output item.
    pub item: OutputItem,
    /// Index of the output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

// ── Content Part Events ────────────────────────────────────────────────────────────

/// A new content part was added.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ContentPartAdded {
    /// Index of the content part in the content list.
    pub content_index: i64,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// The added content part.
    pub part: ContentPart,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// A content part has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ContentPartDone {
    /// Index of the content part in the content list.
    pub content_index: i64,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// The completed content part.
    pub part: ContentPart,
    /// Event sequence number.
    pub sequence_number: i64,
}

// ── Text Events ────────────────────────────────────────────────────────────────

/// Output text delta (streamed chunk).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TextDelta {
    /// Index of the content part in the content list.
    pub content_index: i64,
    /// The delta text content.
    pub delta: String,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
    /// Log probability information for this token. Present only when `logprobs` was specified in the request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Vec<TextLogprob>>,
}

/// Output text has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TextDone {
    /// Index of the content part in the content list.
    pub content_index: i64,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
    /// The full text content.
    pub text: String,
    /// Log probability information for this text. Present only when `logprobs` was specified in the request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Vec<TextLogprob>>,
}

/// Annotation added to output text (e.g., citation link).  Doc §5.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TextAnnotationAdded {
    /// The annotation.  Typed per §4.3.
    pub annotation: super::item::OutputAnnotation,
    /// Index of the annotation within the text.
    pub annotation_index: i64,
    /// Index of the content part in the content list.
    pub content_index: i64,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

// ── Refusal Events ────────────────────────────────────────────────────────────────

/// Refusal text delta (streamed chunk when the model refuses to answer).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RefusalDelta {
    /// Index of the content part in the content list.
    pub content_index: i64,
    /// The delta refusal text.
    pub delta: String,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Refusal text has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RefusalDone {
    /// Index of the content part in the content list.
    pub content_index: i64,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// The full refusal text.
    pub refusal: String,
    /// Event sequence number.
    pub sequence_number: i64,
}

// ── Audio Events ────────────────────────────────────────────────────────────────

/// Audio response delta (base64-encoded).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AudioDelta {
    /// The delta audio data (base64-encoded).
    pub delta: String,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Audio response has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AudioDone {
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Audio transcript text delta.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AudioTranscriptDelta {
    /// The delta transcript text.
    pub delta: String,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Audio transcript text has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AudioTranscriptDone {
    /// Event sequence number.
    pub sequence_number: i64,
}

// ── Function Call Events ────────────────────────────────────────────────────────────

/// Function call arguments delta (streamed chunk).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionCallArgumentsDelta {
    /// The delta arguments JSON fragment.
    pub delta: String,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Function call arguments have completed.  Doc §9.2.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionCallArgumentsDone {
    /// The full arguments string (JSON format).
    pub arguments: String,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

// ── Code Interpreter Events ──────────────────────────────────────────────────────────

/// Code interpreter code delta (streamed chunk).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CodeInterpreterCallCodeDelta {
    /// The delta code fragment.
    pub delta: String,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Code interpreter code has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CodeInterpreterCallCodeDone {
    /// The full code content.
    pub code: String,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Code interpreter call is in progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CodeInterpreterCallInProgress {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Code interpreter is executing code.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CodeInterpreterCallInterpreting {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Code interpreter call has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CodeInterpreterCallCompleted {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

// ── File Search Events ────────────────────────────────────────────────────────────

/// File search call is in progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileSearchCallInProgress {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// File search is querying.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileSearchCallSearching {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// File search call has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileSearchCallCompleted {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

// ── Web Search Events ────────────────────────────────────────────────────────────

/// Web search call is in progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchCallInProgress {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Web search is querying.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchCallSearching {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Web search call has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchCallCompleted {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

// ── Image Generation Events ────────────────────────────────────────────────────────────

/// Image generation call is in progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImageGenCallInProgress {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Image generation is processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImageGenCallGenerating {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Partial image from image generation (base64-encoded).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImageGenCallPartialImage {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Partial image data (base64-encoded).
    pub partial_image_b64: String,
    /// Sequence number of the partial image (for multi-image generation).
    pub partial_image_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Image generation call has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImageGenCallCompleted {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

// ── MCP Call Events ────────────────────────────────────────────────────────────

/// MCP tool call is in progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpCallInProgress {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// MCP tool call has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpCallCompleted {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// MCP tool call failed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpCallFailed {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// MCP tool call arguments delta (streamed chunk).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpCallArgumentsDelta {
    /// The delta arguments JSON fragment.
    pub delta: String,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// MCP tool call arguments have completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpCallArgumentsDone {
    /// The full arguments string (JSON format).
    pub arguments: String,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

// ── MCP List Tools Events ────────────────────────────────────────────────────────

/// MCP tool list retrieval is in progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpListToolsInProgress {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// MCP tool list retrieval has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpListToolsCompleted {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// MCP tool list retrieval failed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpListToolsFailed {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

// ── Reasoning Events ────────────────────────────────────────────────────────────────

/// Reasoning text delta (streamed chunk).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReasoningTextDelta {
    /// Index of the content part in the content list.
    pub content_index: i64,
    /// The delta reasoning text.
    pub delta: String,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Reasoning text has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReasoningTextDone {
    /// Index of the content part in the content list.
    pub content_index: i64,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
    /// The full reasoning text.
    pub text: String,
}

/// Reasoning summary text delta (streamed chunk).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReasoningSummaryTextDelta {
    /// The delta summary text.
    pub delta: String,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
    /// Summary index (a single output item may contain multiple summaries).
    pub summary_index: i64,
}

/// Reasoning summary text has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReasoningSummaryTextDone {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
    /// Summary index (a single output item may contain multiple summaries).
    pub summary_index: i64,
    /// The full summary text.
    pub text: String,
}

/// A reasoning summary part was added.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReasoningSummaryPartAdded {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// The added summary part.
    pub part: SummaryPart,
    /// Event sequence number.
    pub sequence_number: i64,
    /// Summary index (a single output item may contain multiple summaries).
    pub summary_index: i64,
}

/// A reasoning summary part has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReasoningSummaryPartDone {
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// The completed summary part.
    pub part: SummaryPart,
    /// Event sequence number.
    pub sequence_number: i64,
    /// Summary index (a single output item may contain multiple summaries).
    pub summary_index: i64,
}

// ── Custom Tool Events ──────────────────────────────────────────────────────────

/// Custom tool call input delta (streamed chunk).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CustomToolCallInputDelta {
    /// The delta input fragment.
    pub delta: String,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

/// Custom tool call input has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CustomToolCallInputDone {
    /// The full input string.
    pub input: String,
    /// Unique identifier of the owning output item.
    pub item_id: String,
    /// Index of the owning output item in the response output list.
    pub output_index: i64,
    /// Event sequence number.
    pub sequence_number: i64,
}

// ── Error Event ────────────────────────────────────────────────────────────────

/// An error occurred during streaming.  Doc §16.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Error {
    /// Machine-readable error code, e.g. `"server_error"`.  May be `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Human-readable error description.
    pub message: String,
    /// The parameter name that caused the error.  Present only for parameter-specific errors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
    /// Event sequence number.
    pub sequence_number: i64,
}

// ── Helper Types ────────────────────────────────────────────────────────────────

/// Content part union — tagged on `type` for unambiguous wire format.  Doc §5.
///
/// Previously used `#[serde(untagged)]` which relied on field presence to
/// disambiguate.  Now uses `#[serde(tag = "type")]` which is exact.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    /// Output text content part.
    #[serde(rename = "output_text")]
    Text {
        text: String,
        /// Doc §3.9.c: always present (may be empty).
        #[serde(default)]
        annotations: Vec<super::item::OutputAnnotation>,
    },
    /// Refusal content part.
    #[serde(rename = "refusal")]
    Refusal { refusal: String },
    /// Reasoning text content part.
    #[serde(rename = "reasoning_text")]
    ReasoningText { text: String },
}

/// Reasoning summary part.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SummaryPart {
    /// The summary text content.
    pub text: String,
    /// Part type, always `"summary_text"`.
    #[serde(rename = "type")]
    pub type_: String,
}

/// Log probability info for a single token.
///
/// Corresponds to the logprob entry in stream events (`ResponseTextDeltaEventLogprob` /
/// `ResponseTextDoneEventLogprob`). Note: the logprob type in stream events
/// differs from `ResponseOutputTextLogprob` in full output items (the latter includes
/// an extra `bytes` field).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TextLogprob {
    /// The generated token text.
    pub token: String,
    /// Log probability value for this token.
    pub logprob: f64,
    /// Top alternative tokens at this position. Only returned when
    /// `top_logprobs` is specified in the request.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_logprobs: Vec<TextTopLogprob>,
}

/// Alternative token and its log probability.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TextTopLogprob {
    /// The alternative token text.
    pub token: String,
    /// Log probability value for this token.
    pub logprob: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify `StreamEvent` correctly deserializes a `response.created` event.
    #[test]
    fn deserialize_created_event() {
        let json = r#"{
            "type": "response.created",
            "response": {
                "id": "resp_001",
                "object": "response",
                "model": "gpt-4o",
                "status": "in_progress",
                "output": []
            },
            "sequence_number": 0
        }"#;
        let event: StreamEvent = serde_json::from_str(json).unwrap();
        match event {
            StreamEvent::Created(c) => {
                assert_eq!(c.sequence_number, 0);
                assert_eq!(c.response.id, "resp_001");
            }
            _ => panic!("expected Created event"),
        }
    }

    /// Verify `StreamEvent` correctly deserializes a `response.output_text.delta` event
    /// (without logprobs).
    #[test]
    fn deserialize_text_delta_event() {
        let json = r#"{
            "type": "response.output_text.delta",
            "content_index": 0,
            "delta": "Hello",
            "item_id": "item_001",
            "output_index": 0,
            "sequence_number": 5
        }"#;
        let event: StreamEvent = serde_json::from_str(json).unwrap();
        match event {
            StreamEvent::TextDelta(td) => {
                assert_eq!(td.delta, "Hello");
                assert_eq!(td.content_index, 0);
                assert_eq!(td.item_id, "item_001");
                assert!(td.logprobs.is_none());
            }
            _ => panic!("expected TextDelta event"),
        }
    }

    /// Verify `StreamEvent` correctly deserializes a `response.output_text.delta` event with logprobs.
    #[test]
    fn deserialize_text_delta_event_with_logprobs() {
        let json = r#"{
            "type": "response.output_text.delta",
            "content_index": 0,
            "delta": "Hello",
            "item_id": "item_001",
            "output_index": 0,
            "sequence_number": 5,
            "logprobs": [
                {
                    "token": "Hello",
                    "logprob": -1.5,
                    "top_logprobs": [
                        {"token": "Hi", "logprob": -3.0}
                    ]
                }
            ]
        }"#;
        let event: StreamEvent = serde_json::from_str(json).unwrap();
        match event {
            StreamEvent::TextDelta(td) => {
                let lp = td.logprobs.as_ref().unwrap();
                assert_eq!(lp.len(), 1);
                assert_eq!(lp[0].token, "Hello");
                assert_eq!(lp[0].logprob, -1.5);
                assert_eq!(lp[0].top_logprobs.len(), 1);
                assert_eq!(lp[0].top_logprobs[0].token, "Hi");
            }
            _ => panic!("expected TextDelta event with logprobs"),
        }
    }

    /// Verify `StreamEvent` correctly deserializes an error event.
    #[test]
    fn deserialize_error_event() {
        let json = r#"{
            "type": "error",
            "code": "server_error",
            "message": "An internal error occurred",
            "sequence_number": 0
        }"#;
        let event: StreamEvent = serde_json::from_str(json).unwrap();
        match event {
            StreamEvent::Error(e) => {
                assert_eq!(e.code.as_deref(), Some("server_error"));
                assert_eq!(e.message, "An internal error occurred");
                assert!(e.param.is_none());
            }
            _ => panic!("expected Error event"),
        }
    }

    /// Verify unknown event types are captured by `UnknownVariant`.
    #[test]
    fn deserialize_unknown_event() {
        let json = r#"{
            "type": "response.future_event",
            "some_new_field": "hello",
            "sequence_number": 99
        }"#;
        let event: StreamEvent = serde_json::from_str(json).unwrap();
        match event {
            StreamEvent::UnknownVariant(v) => {
                assert_eq!(v["type"], "response.future_event");
                assert_eq!(v["some_new_field"], "hello");
            }
            _ => panic!("expected UnknownVariant"),
        }
    }

    /// Verify `TextLogprob` serialize/deserialize round-trip.
    #[test]
    fn text_logprob_roundtrip() {
        let logprob = TextLogprob {
            token: "hello".to_string(),
            logprob: -1.5,
            top_logprobs: vec![TextTopLogprob {
                token: "hi".to_string(),
                logprob: -3.0,
            }],
        };
        let json = serde_json::to_string(&logprob).unwrap();
        let parsed: TextLogprob = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.token, "hello");
        assert_eq!(parsed.logprob, -1.5);
        assert_eq!(parsed.top_logprobs[0].token, "hi");
    }

    /// Verify serialization of `ContentPart::Text`.
    #[test]
    fn content_part_text_serialize() {
        let part = ContentPart::Text {
            text: "Hello world".to_string(),
            annotations: vec![],
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("Hello world"));
    }

    /// Verify deserialization of `ContentPart::Refusal` (tagged on `type`).
    #[test]
    fn content_part_refusal_deserialize() {
        let json = r#"{"type": "refusal", "refusal": "I cannot answer that."}"#;
        let part: ContentPart = serde_json::from_str(json).unwrap();
        match part {
            ContentPart::Refusal { refusal } => {
                assert_eq!(refusal, "I cannot answer that.");
            }
            _ => panic!("expected Refusal"),
        }
    }

    /// Verify deserialization of `ContentPart::ReasoningText`.
    #[test]
    fn content_part_reasoning_text_deserialize() {
        let json = r#"{"type": "reasoning_text", "text": "Let me think..."}"#;
        let part: ContentPart = serde_json::from_str(json).unwrap();
        match part {
            ContentPart::ReasoningText { text } => {
                assert_eq!(text, "Let me think...");
            }
            _ => panic!("expected ReasoningText"),
        }
    }

    /// Verify `output_text` (with annotations) is not misidentified as `reasoning_text`.
    #[test]
    fn content_part_text_with_annotations_distinct_from_reasoning() {
        let json = r#"{"type": "output_text", "text": "Hello", "annotations": []}"#;
        let part: ContentPart = serde_json::from_str(json).unwrap();
        match part {
            ContentPart::Text { text, annotations } => {
                assert_eq!(text, "Hello");
                assert!(annotations.is_empty());
            }
            _ => panic!("expected Text, got ReasoningText or Refusal"),
        }
    }

    /// Verify `ContentPart::Text` round-trip serialization (empty annotations array).
    #[test]
    fn content_part_text_roundtrip_empty_annotations() {
        let part = ContentPart::Text {
            text: "Roundtrip".to_string(),
            annotations: vec![],
        };
        let json = serde_json::to_string(&part).unwrap();
        let parsed: ContentPart = serde_json::from_str(&json).unwrap();
        match parsed {
            ContentPart::Text { text, annotations } => {
                assert_eq!(text, "Roundtrip");
                assert!(annotations.is_empty());
            }
            _ => panic!("round-trip failed"),
        }
    }

    /// Verify None logprobs is omitted when serializing.
    #[test]
    fn text_delta_logprobs_none_omitted() {
        let td = TextDelta {
            content_index: 0,
            delta: "test".to_string(),
            item_id: "i1".to_string(),
            output_index: 0,
            sequence_number: 1,
            logprobs: None,
        };
        let json = serde_json::to_string(&td).unwrap();
        assert!(!json.contains("logprobs"));
    }
}
