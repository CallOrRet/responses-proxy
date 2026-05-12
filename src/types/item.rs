//! Responses API item types — fully typed per the OpenAI Responses API reference.
//!
//! ## Tagged unions
//!
//! - `InputItem` and `OutputItem` have an `Unknown(serde_json::Value)` catch-all
//!   variant for forward compatibility.
//! - `InputContentBlock`, `OutputContentBlock`, and `OutputAnnotation` use
//!   derive-based serde tagged on `type`.

use std::collections::HashMap;

use super::MessageRole;
use serde::{Deserialize, Serialize};

// ══════════════════════════════════════════════════════════════════════════════
// ── Output Annotations (§4.3) ─────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Output text annotation union type.  Doc §4.3.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OutputAnnotation {
    /// File citation annotation.
    #[serde(rename = "file_citation")]
    FileCitation {
        file_id: String,
        filename: String,
        index: i64,
    },
    /// URL citation annotation.
    #[serde(rename = "url_citation")]
    UrlCitation {
        url: String,
        title: String,
        start_index: i64,
        end_index: i64,
    },
    /// Container file citation annotation.
    #[serde(rename = "container_file_citation")]
    ContainerFileCitation {
        container_id: String,
        end_index: i64,
        file_id: String,
        filename: String,
        start_index: i64,
    },
    /// File path annotation.
    #[serde(rename = "file_path")]
    FilePath { file_id: String, index: i64 },
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Input Content Blocks (§3.10) ──────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Content block union type in input messages.  Doc §3.10.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum InputContentBlock {
    /// Text input content block.
    #[serde(rename = "input_text")]
    Text { text: String },

    /// Image input content block.
    #[serde(rename = "input_image")]
    Image {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        image_url: Option<String>,
    },

    /// File input content block.
    #[serde(rename = "input_file")]
    File {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_data: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
    },
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Output Content Blocks ───────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Content block union type in output messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OutputContentBlock {
    /// Model-generated text output.
    #[serde(rename = "output_text")]
    Text {
        text: String,
        /// Doc §3.9.c: always present (may be empty array).
        #[serde(default)]
        annotations: Vec<OutputAnnotation>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<TextLogprob>>,
    },
    /// Model refusal / safety check response.
    #[serde(rename = "refusal")]
    Refusal { refusal: String },
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Logprobs ──────────────────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Per-token log probability information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TextLogprob {
    pub token: String,
    pub bytes: Vec<i64>,
    pub logprob: f64,
    /// Doc §3.9.c: always present.
    #[serde(default)]
    pub top_logprobs: Vec<TopLogprob>,
}

/// Alternative token and its log probability.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TopLogprob {
    pub token: String,
    pub bytes: Vec<i64>,
    pub logprob: f64,
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Computer action types (§3.9.f) ─────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Computer action — 10 variants per doc §3.9.f.  Tagged on `type`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ComputerAction {
    /// Click at (x, y).
    #[serde(rename = "click")]
    Click {
        x: i64,
        y: i64,
        #[serde(default = "default_button_left")]
        button: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
    },
    /// Double-click at (x, y).
    #[serde(rename = "double_click")]
    DoubleClick {
        x: i64,
        y: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
    },
    /// Drag along a path of points.
    #[serde(rename = "drag")]
    Drag {
        path: Vec<DragPathPoint>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
    },
    /// Press keys.
    #[serde(rename = "keypress")]
    Keypress { keys: Vec<String> },
    /// Move cursor to (x, y).
    #[serde(rename = "move")]
    Move {
        x: i64,
        y: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
    },
    /// Take a screenshot.
    #[serde(rename = "screenshot")]
    Screenshot,
    /// Scroll at position.
    #[serde(rename = "scroll")]
    Scroll {
        x: i64,
        y: i64,
        scroll_x: i64,
        scroll_y: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<String>>,
    },
    /// Type text.
    #[serde(rename = "type")]
    Type { text: String },
    /// Wait (no-op).
    #[serde(rename = "wait")]
    Wait,
}

fn default_button_left() -> String {
    "left".into()
}

/// A point on a drag path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DragPathPoint {
    pub x: i64,
    pub y: i64,
}

/// Pending safety check.  Doc §3.9.f.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingSafetyCheck {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Acknowledged safety check.  Doc §3.9.g.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcknowledgedSafetyCheck {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Computer screenshot.  Doc §3.9.g.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ComputerScreenshot {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

impl Default for ComputerScreenshot {
    fn default() -> Self {
        Self {
            type_: "computer_screenshot".into(),
            file_id: None,
            image_url: None,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Shell action types (§3.9.k, §3.9.m) ──────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Local shell action.  Doc §3.9.k.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LocalShellAction {
    #[serde(rename = "type")]
    pub type_: String,
    pub command: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
}

impl Default for LocalShellAction {
    fn default() -> Self {
        Self {
            type_: "exec".into(),
            command: vec![],
            env: HashMap::new(),
            timeout_ms: None,
            user: None,
            working_directory: None,
        }
    }
}

/// Managed shell action.  Doc §3.9.m.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ShellAction {
    pub commands: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_length: Option<i64>,
}

/// Shell output chunk.  Doc §3.9.n.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellOutputChunk {
    pub stdout: String,
    pub stderr: String,
    pub outcome: ShellOutcome,
}

/// Shell execution outcome.  Doc §3.9.n.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShellOutcome {
    /// Command timed out.
    #[serde(rename = "timeout")]
    Timeout,
    /// Normal exit with code.
    #[serde(rename = "exit")]
    Exit { exit_code: i64 },
}

/// Shell execution environment.  Doc §3.9.m.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShellEnvironment {
    /// Local execution.
    #[serde(rename = "local")]
    Local,
    /// Reference to an existing container.
    #[serde(rename = "container_reference")]
    ContainerReference { container_id: String },
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Reasoning types (§3.9.h) ──────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Summary text part.  Doc §3.9.h.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryPart {
    #[serde(rename = "type")]
    pub type_: String,
    pub text: String,
}

impl Default for SummaryPart {
    fn default() -> Self {
        Self {
            type_: "summary_text".into(),
            text: String::new(),
        }
    }
}

/// Reasoning text part.  Doc §3.9.h.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningTextPart {
    #[serde(rename = "type")]
    pub type_: String,
    pub text: String,
}

impl Default for ReasoningTextPart {
    fn default() -> Self {
        Self {
            type_: "reasoning_text".into(),
            text: String::new(),
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Web search action types (§4.2) ─────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Web search call action.  Doc §4.2 ResponseFunctionWebSearch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebSearchAction {
    /// Search query.
    #[serde(rename = "search")]
    Search {
        /// Deprecated — still returned.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        /// Search queries.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        queries: Option<Vec<String>>,
        /// Source URLs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sources: Option<Vec<WebSearchSource>>,
    },
    /// Open a specific page.
    #[serde(rename = "open_page")]
    OpenPage {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    /// Find text within a page.
    #[serde(rename = "find_in_page")]
    FindInPage { url: String, pattern: String },
}

/// URL source in a web search action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchSource {
    #[serde(rename = "type")]
    pub type_: String,
    pub url: String,
}

// ══════════════════════════════════════════════════════════════════════════════
// ── File search result (§4.2) ─────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// File search result item.  Doc §4.2 ResponseFileSearchToolCall.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileSearchResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// Relevance score `[0, 1]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Up to 16 key-value pairs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<HashMap<String, serde_json::Value>>,
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Apply-patch operation types (§3.9.o) ──────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Apply-patch file operation.  Doc §3.9.o.  Tagged on `type`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApplyPatchOperation {
    /// Create a new file.
    #[serde(rename = "create_file")]
    CreateFile { path: String, diff: String },
    /// Delete a file.
    #[serde(rename = "delete_file")]
    DeleteFile { path: String },
    /// Update an existing file.
    #[serde(rename = "update_file")]
    UpdateFile { path: String, diff: String },
}

// ══════════════════════════════════════════════════════════════════════════════
// ── MCP and code-interpreter output types ──────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Code interpreter log output.  Doc §3.9.j.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeInterpreterLogs {
    #[serde(rename = "type", default = "default_logs_type")]
    pub type_: String,
    pub logs: String,
}

fn default_logs_type() -> String {
    "logs".into()
}

/// Code interpreter image output.  Doc §3.9.j.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeInterpreterImage {
    #[serde(rename = "type", default = "default_image_type")]
    pub type_: String,
    pub url: String,
}

fn default_image_type() -> String {
    "image".into()
}

/// Code interpreter output — logs or image.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CodeInterpreterOutput {
    /// Log output.
    #[serde(rename = "logs")]
    Logs(CodeInterpreterLogs),
    /// Image output.
    #[serde(rename = "image")]
    Image(CodeInterpreterImage),
}

/// MCP tool info.  Doc §3.9.q McpToolInfo.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpToolInfo {
    /// Tool name.  **Required.**
    pub name: String,
    /// JSON Schema for tool input.  **Required.**
    pub input_schema: serde_json::Value,
    /// Tool description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional annotations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<serde_json::Value>,
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Union types for function / custom tool output ──────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Function call output value — string or content blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FunctionOutputValue {
    String(String),
    Array(Vec<InputContentBlock>),
}

/// Custom tool call output value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CustomToolOutputValue {
    String(String),
    Array(Vec<InputContentBlock>),
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Item structs (alphabetical) ────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Apply-patch call.  Doc §3.9.o.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApplyPatchCall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub operation: ApplyPatchOperation,
    /// Status. Allowed: `in_progress`, `completed`.
    pub status: String,
}

/// Apply-patch call output.  Doc §3.9.p.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApplyPatchCallOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    /// Status. Allowed: `completed`, `failed`.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

/// Code interpreter call.  Doc §3.9.j.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CodeInterpreterCall {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub container_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<CodeInterpreterOutput>>,
    /// Status. Allowed: `in_progress`, `interpreting`, `completed`, `failed`.
    pub status: String,
}

/// Compaction item.  Doc §3.9.s.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Compaction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Encrypted compacted context (up to ~10 MB).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    // ── Proxy-internal fields (not on OpenAI wire) ──
    #[serde(default)]
    pub output: Vec<OutputItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

/// Computer call.  Doc §3.9.f.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ComputerCall {
    pub id: String,
    pub call_id: String,
    #[serde(default)]
    pub pending_safety_checks: Vec<PendingSafetyCheck>,
    /// Status. Allowed: `in_progress`, `completed`, `incomplete`.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<ComputerAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<ComputerAction>>,
}

/// Computer call output.  Doc §3.9.g.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ComputerCallOutput {
    pub call_id: String,
    pub output: ComputerScreenshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acknowledged_safety_checks: Option<Vec<AcknowledgedSafetyCheck>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Custom tool call.  Doc §3.9.r.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CustomToolCall {
    pub call_id: String,
    pub input: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

/// Custom tool call output.  Doc §3.9.r.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CustomToolCallOutput {
    pub call_id: String,
    pub output: CustomToolOutputValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// File search call.  Doc §4.2 ResponseFileSearchToolCall.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileSearchCall {
    pub id: String,
    pub queries: Vec<String>,
    /// Status. Allowed: `in_progress`, `searching`, `completed`, `incomplete`, `failed`.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<FileSearchResult>>,
}

/// Function call.  Doc §3.9.d.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionCall {
    pub call_id: String,
    pub name: String,
    pub arguments: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Function call output.  Doc §3.9.e.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionCallOutput {
    pub call_id: String,
    pub output: FunctionOutputValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Image generation call.  Doc §3.9.i.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImageGenerationCall {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// Status. Allowed: `in_progress`, `generating`, `completed`, `failed`.
    pub status: String,
}

/// Input message.  Doc §3.9.a.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InputMessage {
    pub role: MessageRole,
    #[serde(default)]
    pub content: Vec<InputContentBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Item reference.  Doc §3.9.t.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemReference {
    pub id: String,
}

/// Local shell call.  Doc §3.9.k.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LocalShellCall {
    pub id: String,
    pub call_id: String,
    pub action: LocalShellAction,
    /// Status. Allowed: `in_progress`, `completed`, `incomplete`.
    pub status: String,
}

/// Local shell call output.  Doc §3.9.l.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LocalShellCallOutput {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// MCP approval request.  Doc §3.9.q.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpApprovalRequest {
    pub id: String,
    pub arguments: String,
    pub name: String,
    pub server_label: String,
}

/// MCP approval response.  Doc §3.9.q.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpApprovalResponse {
    pub approval_request_id: String,
    pub approve: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// MCP call.  Doc §3.9.q.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpCall {
    pub id: String,
    pub name: String,
    pub server_label: String,
    pub arguments: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// MCP list tools.  Doc §3.9.q.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpListTools {
    pub id: String,
    pub server_label: String,
    #[serde(default)]
    pub tools: Vec<McpToolInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Output message from the assistant.  Doc §3.9.c.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OutputMessage {
    pub id: String,
    #[serde(default)]
    pub content: Vec<OutputContentBlock>,
    pub role: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}

/// Reasoning content.  Doc §3.9.h.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Reasoning {
    pub id: String,
    #[serde(default)]
    pub summary: Vec<SummaryPart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<ReasoningTextPart>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Shell call (managed).  Doc §3.9.m.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ShellCall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub action: ShellAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<ShellEnvironment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Shell call output.  Doc §3.9.n.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ShellCallOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub output: Vec<ShellOutputChunk>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_length: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Tool search call.  Doc §4.2 ResponseToolSearchCall.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolSearchCall {
    pub arguments: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

/// Tool search output.  Doc §4.2 ResponseToolSearchOutputItem.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolSearchOutput {
    pub id: String,
    #[serde(default)]
    pub execution: String,
    #[serde(default)]
    pub tools: Vec<McpToolInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Web search call.  Doc §4.2 ResponseFunctionWebSearch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchCall {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<WebSearchAction>,
    /// Status. Allowed: `in_progress`, `searching`, `completed`, `failed`.
    pub status: String,
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Tagged Enum: InputItem ──────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Input item union type — elements of the `input` array in API requests.
/// Dispatched by `type` field.  26 variants + Unknown catch-all.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputItem {
    Message(InputMessage),
    FunctionCall(FunctionCall),
    FunctionCallOutput(FunctionCallOutput),
    CustomToolCall(CustomToolCall),
    CustomToolCallOutput(CustomToolCallOutput),
    ComputerCall(ComputerCall),
    ComputerCallOutput(ComputerCallOutput),
    CodeInterpreterCall(CodeInterpreterCall),
    FileSearchCall(FileSearchCall),
    WebSearchCall(WebSearchCall),
    ImageGenerationCall(ImageGenerationCall),
    McpCall(McpCall),
    McpListTools(McpListTools),
    McpApprovalRequest(McpApprovalRequest),
    McpApprovalResponse(McpApprovalResponse),
    Reasoning(Reasoning),
    Compaction(Compaction),
    LocalShellCall(LocalShellCall),
    LocalShellCallOutput(LocalShellCallOutput),
    ShellCall(ShellCall),
    ShellCallOutput(ShellCallOutput),
    ApplyPatchCall(ApplyPatchCall),
    ApplyPatchCallOutput(ApplyPatchCallOutput),
    ToolSearchCall(ToolSearchCall),
    ToolSearchOutput(ToolSearchOutput),
    ItemReference(ItemReference),
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Tagged Enum: OutputItem ─────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

/// Output item union type — elements of the `output` array in API responses.
/// Dispatched by `type` field.  22 variants + Unknown catch-all.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputItem {
    Message(OutputMessage),
    FunctionCall(FunctionCall),
    CustomToolCall(CustomToolCall),
    ComputerCall(ComputerCall),
    CodeInterpreterCall(CodeInterpreterCall),
    FileSearchCall(FileSearchCall),
    WebSearchCall(WebSearchCall),
    ImageGenerationCall(ImageGenerationCall),
    McpCall(McpCall),
    McpListTools(McpListTools),
    McpApprovalRequest(McpApprovalRequest),
    Reasoning(Reasoning),
    Compaction(Compaction),
    LocalShellCall(LocalShellCall),
    LocalShellCallOutput(LocalShellCallOutput),
    ShellCall(ShellCall),
    ShellCallOutput(ShellCallOutput),
    ApplyPatchCall(ApplyPatchCall),
    ApplyPatchCallOutput(ApplyPatchCallOutput),
    ToolSearchCall(ToolSearchCall),
    ToolSearchOutput(ToolSearchOutput),
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

// ══════════════════════════════════════════════════════════════════════════════
// ── Tests ─────────────────────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── InputItem tests ────────────────────────────────────────────────

    #[test]
    fn input_item_message_roundtrip() {
        let json = serde_json::json!({
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": "hello"}]
        });
        let item: InputItem = serde_json::from_value(json.clone()).unwrap();
        match &item {
            InputItem::Message(m) => {
                assert_eq!(m.role, MessageRole::User);
                assert_eq!(m.content.len(), 1);
            }
            _ => panic!("expected Message variant"),
        }
        let roundtripped = serde_json::to_value(&item).unwrap();
        assert_eq!(roundtripped["type"], "message");
        assert_eq!(roundtripped["content"][0]["text"], "hello");
    }

    #[test]
    fn input_item_function_call_roundtrip() {
        let json = serde_json::json!({
            "type": "function_call",
            "call_id": "fc_123",
            "name": "get_weather",
            "arguments": "{\"city\":\"Beijing\"}"
        });
        let item: InputItem = serde_json::from_value(json.clone()).unwrap();
        match &item {
            InputItem::FunctionCall(f) => {
                assert_eq!(f.call_id, "fc_123");
                assert_eq!(f.name, "get_weather");
            }
            _ => panic!("expected FunctionCall variant"),
        }
        let roundtripped = serde_json::to_value(&item).unwrap();
        assert_eq!(roundtripped["type"], "function_call");
        assert_eq!(roundtripped["call_id"], "fc_123");
    }

    #[test]
    fn input_item_unknown_preserves_tag() {
        let json = serde_json::json!({
            "type": "some_future_type",
            "custom_field": 42,
            "nested": {"key": "value"}
        });
        let item: InputItem = serde_json::from_value(json.clone()).unwrap();
        match &item {
            InputItem::Unknown(v) => {
                assert_eq!(v["type"], "some_future_type");
                assert_eq!(v["custom_field"], 42);
            }
            _ => panic!("expected Unknown variant"),
        }
        let roundtripped = serde_json::to_value(&item).unwrap();
        assert_eq!(roundtripped["type"], "some_future_type");
        assert_eq!(roundtripped["custom_field"], 42);
    }

    #[test]
    fn function_call_output_string_roundtrip() {
        let json = serde_json::json!({
            "type": "function_call_output",
            "call_id": "fc_abc",
            "output": "result string"
        });
        let item: InputItem = serde_json::from_value(json).unwrap();
        match &item {
            InputItem::FunctionCallOutput(f) => {
                assert_eq!(f.call_id, "fc_abc");
                match &f.output {
                    FunctionOutputValue::String(s) => assert_eq!(s, "result string"),
                    _ => panic!("expected string output"),
                }
            }
            _ => panic!("expected FunctionCallOutput variant"),
        }
    }

    #[test]
    fn function_call_output_array_roundtrip() {
        let json = serde_json::json!({
            "type": "function_call_output",
            "call_id": "fc_arr",
            "output": [
                {"type": "input_text", "text": "part 1"},
                {"type": "input_text", "text": "part 2"}
            ]
        });
        let item: InputItem = serde_json::from_value(json).unwrap();
        match &item {
            InputItem::FunctionCallOutput(f) => match &f.output {
                FunctionOutputValue::Array(arr) => assert_eq!(arr.len(), 2),
                _ => panic!("expected array output"),
            },
            _ => panic!("expected FunctionCallOutput variant"),
        }
    }

    #[test]
    fn custom_tool_call_roundtrip() {
        let json = serde_json::json!({
            "type": "custom_tool_call",
            "call_id": "cct_1",
            "name": "my_tool",
            "input": "{\"param\": 1}"
        });
        let item: InputItem = serde_json::from_value(json).unwrap();
        match &item {
            InputItem::CustomToolCall(c) => {
                assert_eq!(c.call_id, "cct_1");
                assert_eq!(c.name, "my_tool");
                assert_eq!(c.input, "{\"param\": 1}");
            }
            _ => panic!("expected CustomToolCall variant"),
        }
    }

    // ── OutputItem tests ───────────────────────────────────────────────

    #[test]
    fn output_item_message_roundtrip() {
        let json = serde_json::json!({
            "type": "message",
            "id": "msg_123",
            "role": "assistant",
            "status": "completed",
            "content": [{
                "type": "output_text",
                "text": "Hello!",
                "annotations": []
            }]
        });
        let item: OutputItem = serde_json::from_value(json.clone()).unwrap();
        match &item {
            OutputItem::Message(m) => {
                assert_eq!(m.id, "msg_123");
                assert_eq!(m.role, "assistant");
                assert_eq!(m.status, "completed");
            }
            _ => panic!("expected Message variant"),
        }
        let roundtripped = serde_json::to_value(&item).unwrap();
        assert_eq!(roundtripped["type"], "message");
        assert_eq!(roundtripped["content"][0]["text"], "Hello!");
    }

    #[test]
    fn output_item_unknown_preserves_tag() {
        let json = serde_json::json!({
            "type": "future_output_type",
            "data": "test"
        });
        let item: OutputItem = serde_json::from_value(json.clone()).unwrap();
        match &item {
            OutputItem::Unknown(v) => {
                assert_eq!(v["type"], "future_output_type");
            }
            _ => panic!("expected Unknown variant"),
        }
        let roundtripped = serde_json::to_value(&item).unwrap();
        assert_eq!(roundtripped["type"], "future_output_type");
    }

    #[test]
    fn output_item_refusal_content() {
        let json = serde_json::json!({
            "type": "message",
            "id": "msg_refuse",
            "role": "assistant",
            "status": "completed",
            "content": [{
                "type": "refusal",
                "refusal": "I cannot help with that request."
            }]
        });
        let item: OutputItem = serde_json::from_value(json).unwrap();
        match &item {
            OutputItem::Message(m) => {
                assert_eq!(m.content.len(), 1);
                match &m.content[0] {
                    OutputContentBlock::Refusal { refusal } => {
                        assert_eq!(refusal, "I cannot help with that request.");
                    }
                    _ => panic!("expected Refusal content block"),
                }
            }
            _ => panic!("expected Message variant"),
        }
    }

    #[test]
    fn compaction_recursive_roundtrip() {
        let json = serde_json::json!({
            "type": "compaction",
            "id": "comp_1",
            "output": [{
                "type": "message",
                "id": "msg_1",
                "role": "assistant",
                "status": "completed",
                "content": [{
                    "type": "output_text",
                    "text": "compacted text",
                    "annotations": []
                }]
            }]
        });
        let item: OutputItem = serde_json::from_value(json.clone()).unwrap();
        match &item {
            OutputItem::Compaction(c) => {
                assert_eq!(c.id.as_deref(), Some("comp_1"));
                assert_eq!(c.output.len(), 1);
                match &c.output[0] {
                    OutputItem::Message(m) => {
                        assert_eq!(m.id, "msg_1");
                    }
                    _ => panic!("expected nested Message"),
                }
            }
            _ => panic!("expected Compaction variant"),
        }
        let roundtripped = serde_json::to_value(&item).unwrap();
        assert_eq!(roundtripped["type"], "compaction");
        assert_eq!(roundtripped["output"][0]["type"], "message");
    }

    // ── Content block tests ────────────────────────────────────────────

    #[test]
    fn input_content_block_image_roundtrip() {
        let json = serde_json::json!({
            "type": "input_image",
            "image_url": "https://example.com/img.png",
            "detail": "high"
        });
        let block: InputContentBlock = serde_json::from_value(json).unwrap();
        match &block {
            InputContentBlock::Image {
                image_url, detail, ..
            } => {
                assert_eq!(image_url.as_deref(), Some("https://example.com/img.png"));
                assert_eq!(detail.as_deref(), Some("high"));
            }
            _ => panic!("expected Image"),
        }
    }

    #[test]
    fn input_content_block_file_roundtrip() {
        let json = serde_json::json!({
            "type": "input_file",
            "file_id": "file-abc123",
            "filename": "data.pdf"
        });
        let block: InputContentBlock = serde_json::from_value(json).unwrap();
        match &block {
            InputContentBlock::File {
                file_id, filename, ..
            } => {
                assert_eq!(file_id.as_deref(), Some("file-abc123"));
                assert_eq!(filename.as_deref(), Some("data.pdf"));
            }
            _ => panic!("expected File"),
        }
    }

    // ── Annotation tests ───────────────────────────────────────────────

    #[test]
    fn output_annotation_file_citation_roundtrip() {
        let json = serde_json::json!({
            "type": "file_citation",
            "file_id": "file_abc",
            "filename": "report.pdf",
            "index": 0
        });
        let ann: OutputAnnotation = serde_json::from_value(json).unwrap();
        match &ann {
            OutputAnnotation::FileCitation {
                file_id,
                filename,
                index,
            } => {
                assert_eq!(file_id, "file_abc");
                assert_eq!(filename, "report.pdf");
                assert_eq!(*index, 0);
            }
            _ => panic!("expected FileCitation"),
        }
    }

    #[test]
    fn output_annotation_url_citation_roundtrip() {
        let json = serde_json::json!({
            "type": "url_citation",
            "url": "https://example.com",
            "title": "Example Page",
            "start_index": 10,
            "end_index": 30
        });
        let ann: OutputAnnotation = serde_json::from_value(json).unwrap();
        match &ann {
            OutputAnnotation::UrlCitation {
                url,
                title,
                start_index,
                end_index,
            } => {
                assert_eq!(url, "https://example.com");
                assert_eq!(title, "Example Page");
                assert_eq!(*start_index, 10);
                assert_eq!(*end_index, 30);
            }
            _ => panic!("expected UrlCitation"),
        }
    }

    // ── Computer action tests ────────────────────────────────────────

    #[test]
    fn computer_action_click_roundtrip() {
        let json = serde_json::json!({
            "type": "click",
            "x": 100,
            "y": 200,
            "button": "left"
        });
        let action: ComputerAction = serde_json::from_value(json).unwrap();
        match &action {
            ComputerAction::Click { x, y, button, .. } => {
                assert_eq!(*x, 100);
                assert_eq!(*y, 200);
                assert_eq!(button, "left");
            }
            _ => panic!("expected Click"),
        }
    }

    #[test]
    fn computer_action_screenshot_roundtrip() {
        let json = serde_json::json!({"type": "screenshot"});
        let action: ComputerAction = serde_json::from_value(json).unwrap();
        assert!(matches!(action, ComputerAction::Screenshot));
    }

    // ── Apply-patch operation tests ────────────────────────────────────

    #[test]
    fn apply_patch_create_file_roundtrip() {
        let json = serde_json::json!({
            "type": "create_file",
            "path": "/tmp/test.rs",
            "diff": "+fn main() {}"
        });
        let op: ApplyPatchOperation = serde_json::from_value(json).unwrap();
        match &op {
            ApplyPatchOperation::CreateFile { path, diff } => {
                assert_eq!(path, "/tmp/test.rs");
                assert_eq!(diff, "+fn main() {}");
            }
            _ => panic!("expected CreateFile"),
        }
    }
}
