//! Tool definitions and tool-choice types for the OpenAI Responses API.
//! Includes the complete set of tool types (function, file search, web search,
//! code interpreter, MCP, shell, custom, and more) with corresponding request
//! and response variants.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// Tool — tagged union for *response* deserialization
// ═══════════════════════════════════════════════════════════════════════════

/// The set of tools available to the model.
///
/// Each variant maps to a distinct `type` discriminator on the wire.
/// The `Unknown` variant captures any tool type not listed below so that
/// deserialization never fails on a new/unknown tool.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum Tool {
    /// Define a function that can be called within code.
    /// [Function calling guide](https://platform.openai.com/docs/guides/function-calling)
    #[serde(rename = "function")]
    Function(FunctionTool),

    /// Search relevant content in uploaded files.
    /// [File search tool](https://platform.openai.com/docs/guides/tools-file-search)
    #[serde(rename = "file_search")]
    FileSearch(FileSearchTool),

    /// Search the internet for sources related to a prompt.
    /// [Web search tool](https://platform.openai.com/docs/guides/tools-web-search)
    #[serde(rename = "web_search")]
    WebSearch(WebSearchTool),

    /// Web search preview tool that displays search results in the response.
    /// [Web search tool](https://platform.openai.com/docs/guides/tools-web-search)
    #[serde(rename = "web_search_preview")]
    WebSearchPreview(WebSearchPreviewTool),

    /// Tool for controlling a virtual computer.
    /// [Computer use tool](https://platform.openai.com/docs/guides/tools-computer-use)
    #[serde(rename = "computer")]
    Computer(ComputerTool),

    /// Computer use preview — virtual desktop with specified display dimensions
    /// and OS environment.
    #[serde(rename = "computer_use_preview")]
    ComputerUsePreview(ComputerUsePreviewTool),

    /// Run Python code to assist in generating responses.
    #[serde(rename = "code_interpreter")]
    CodeInterpreter(CodeInterpreterTool),

    /// Generate images using a GPT image model.
    #[serde(rename = "image_generation")]
    ImageGeneration(ImageGenerationTool),

    /// Provide additional tools via a remote Model Context Protocol (MCP) server.
    /// [MCP guide](https://platform.openai.com/docs/guides/tools-remote-mcp)
    #[serde(rename = "mcp")]
    Mcp(McpTool),

    /// Allow the model to execute shell commands in a local environment.
    #[serde(rename = "local_shell")]
    LocalShell(LocalShellTool),

    /// Remote shell tool, allowing the model to execute commands in a remote environment.
    #[serde(rename = "shell")]
    Shell(ShellTool),

    /// Custom tool that processes input using a specified format.
    /// [Custom tools](https://platform.openai.com/docs/guides/function-calling#custom-tools)
    #[serde(rename = "custom")]
    Custom(CustomTool),

    /// Namespace tool that organizes a group of tools under a namespace.
    #[serde(rename = "namespace")]
    Namespace(NamespaceTool),

    /// Configure search behavior for lazy-loaded tools.
    #[serde(rename = "tool_search")]
    ToolSearch(SearchTool),

    /// Allow the assistant to create, delete, or update files via unified diffs.
    #[serde(rename = "apply_patch")]
    ApplyPatch(ApplyPatchTool),

    /// Unknown tool type. For forward compatibility, captures `type` values not yet defined.
    #[serde(rename = "unknown", skip_serializing)]
    Unknown(serde_json::Value),
}

impl<'de> Deserialize<'de> for Tool {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let tool_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");

        let tool = match tool_type {
            "function" => serde_json::from_value(value.clone())
                .map(Tool::Function)
                .unwrap_or_else(|_| Tool::Unknown(value)),
            "file_search" => serde_json::from_value(value.clone())
                .map(Tool::FileSearch)
                .unwrap_or_else(|_| Tool::Unknown(value)),
            "web_search" | "web_search_2025_08_26" => serde_json::from_value(value.clone())
                .map(Tool::WebSearch)
                .unwrap_or_else(|_| Tool::Unknown(value)),
            "web_search_preview" | "web_search_preview_2025_03_11" => {
                serde_json::from_value(value.clone())
                    .map(Tool::WebSearchPreview)
                    .unwrap_or_else(|_| Tool::Unknown(value))
            }
            "computer" => serde_json::from_value(value.clone())
                .map(Tool::Computer)
                .unwrap_or_else(|_| Tool::Unknown(value)),
            "computer_use_preview" => serde_json::from_value(value.clone())
                .map(Tool::ComputerUsePreview)
                .unwrap_or_else(|_| Tool::Unknown(value)),
            "code_interpreter" => serde_json::from_value(value.clone())
                .map(Tool::CodeInterpreter)
                .unwrap_or_else(|_| Tool::Unknown(value)),
            "image_generation" => serde_json::from_value(value.clone())
                .map(Tool::ImageGeneration)
                .unwrap_or_else(|_| Tool::Unknown(value)),
            "mcp" => serde_json::from_value(value.clone())
                .map(Tool::Mcp)
                .unwrap_or_else(|_| Tool::Unknown(value)),
            "local_shell" => serde_json::from_value(value.clone())
                .map(Tool::LocalShell)
                .unwrap_or_else(|_| Tool::Unknown(value)),
            "shell" => serde_json::from_value(value.clone())
                .map(Tool::Shell)
                .unwrap_or_else(|_| Tool::Unknown(value)),
            "custom" => serde_json::from_value(value.clone())
                .map(Tool::Custom)
                .unwrap_or_else(|_| Tool::Unknown(value)),
            "namespace" => serde_json::from_value(value.clone())
                .map(Tool::Namespace)
                .unwrap_or_else(|_| Tool::Unknown(value)),
            "tool_search" => serde_json::from_value(value.clone())
                .map(Tool::ToolSearch)
                .unwrap_or_else(|_| Tool::Unknown(value)),
            "apply_patch" => serde_json::from_value(value.clone())
                .map(Tool::ApplyPatch)
                .unwrap_or_else(|_| Tool::Unknown(value)),
            _ => Tool::Unknown(value),
        };
        Ok(tool)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ToolRequest — tagged union for *request* serialization
// ═══════════════════════════════════════════════════════════════════════════

/// Request-side tool parameter union. Mirrors [`Tool`] but every field is
/// optional so callers only set the keys they need.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolRequest {
    /// Define a function that can be called within code.
    #[serde(rename = "function")]
    Function(FunctionToolRequest),

    /// Search relevant content in uploaded files.
    #[serde(rename = "file_search")]
    FileSearch(FileSearchToolRequest),

    /// Search the internet for sources related to a prompt.
    #[serde(rename = "web_search")]
    WebSearch(WebSearchToolRequest),

    /// Web search preview tool.
    #[serde(rename = "web_search_preview")]
    WebSearchPreview(WebSearchPreviewToolRequest),

    /// Tool for controlling a virtual computer.
    #[serde(rename = "computer")]
    Computer(ComputerToolRequest),

    /// Computer use preview — virtual desktop with display and OS config.
    #[serde(rename = "computer_use_preview")]
    ComputerUsePreview(ComputerUsePreviewToolRequest),

    /// Run Python code to assist in generating responses.
    #[serde(rename = "code_interpreter")]
    CodeInterpreter(CodeInterpreterToolRequest),

    /// Generate images using a GPT image model.
    #[serde(rename = "image_generation")]
    ImageGeneration(ImageGenerationToolRequest),

    /// Provide additional tools via a remote MCP server.
    #[serde(rename = "mcp")]
    Mcp(McpToolRequest),

    /// Allow the model to execute shell commands in a local environment.
    #[serde(rename = "local_shell")]
    LocalShell(LocalShellToolRequest),

    /// Remote shell tool.
    #[serde(rename = "shell")]
    Shell(ShellToolRequest),

    /// Custom tool that processes input using a specified format.
    #[serde(rename = "custom")]
    Custom(CustomToolRequest),

    /// Namespace tool.
    #[serde(rename = "namespace")]
    Namespace(NamespaceToolRequest),

    /// Configure search behavior for lazy-loaded tools.
    #[serde(rename = "tool_search")]
    ToolSearch(SearchToolRequest),

    /// Allow the assistant to create, delete, or update files via unified diffs.
    #[serde(rename = "apply_patch")]
    ApplyPatch(ApplyPatchToolRequest),
}

// ═══════════════════════════════════════════════════════════════════════════
// Individual tool structs — *response* side (some fields are required)
// ═══════════════════════════════════════════════════════════════════════════

// ── 1. FunctionTool ───────────────────────────────────────────────────────

/// Define a function that can be called within code.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionTool {
    /// Function name.
    pub name: String,
    /// Function description, including its purpose and when to call it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema parameters. Allowed: a valid JSON Schema object.
    pub parameters: serde_json::Value,
    /// Whether to enable strict mode. The API will strictly enforce the function's parameter schema.
    pub strict: bool,
    /// Whether to defer loading this tool. When enabled, the tool is loaded lazily on use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
}

// ── 2. FileSearchTool ─────────────────────────────────────────────────────

/// Search relevant content in uploaded files.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileSearchTool {
    /// Vector store IDs to search.
    pub vector_store_ids: Vec<String>,
    /// Maximum number of results to return.
    pub max_num_results: Option<i64>,
    /// Search filters supporting comparison and compound filtering.
    pub filters: Option<FileSearchFilters>,
    /// Sorting options for controlling result order.
    pub ranking_options: Option<RankingOptions>,
}

// ── 3. WebSearchTool ──────────────────────────────────────────────────────

/// Search the internet for sources related to a prompt.
///
/// Type discriminator: `"web_search"` or `"web_search_2025_08_26"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchTool {
    /// Domains to restrict the search to, e.g. `["pubmed.ncbi.nlm.nih.gov"]`.
    /// Default: `null` (no restriction).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    /// Search context size. Allowed: `"low"`, `"medium"` (default), `"high"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<String>,
    /// User's approximate geographic location for optimising search results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_location: Option<UserLocation>,
    /// Additional search filters (same shape as file-search `Filters`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<serde_json::Value>,
}

// ── 4. WebSearchPreviewTool ───────────────────────────────────────────────

/// Web search preview tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchPreviewTool {
    /// Search context size. Allowed: `"low"`, `"medium"`, `"high"`.
    pub search_context_size: Option<String>,
    /// User's geographic location for optimizing search results.
    pub user_location: Option<UserLocation>,
    /// Content type list to search.
    pub search_content_types: Option<Vec<String>>,
}

// ── 5. ComputerTool ───────────────────────────────────────────────────────

/// Tool for controlling a virtual computer.
///
/// This type only serves as a `type` discriminator; it carries no extra fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputerTool {}

/// Computer use preview — virtual desktop with specified display dimensions
/// and OS environment.
///
/// Type discriminator: `"computer_use_preview"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ComputerUsePreviewTool {
    /// Display width in pixels.  **Required.**
    pub display_width: i64,
    /// Display height in pixels.  **Required.**
    pub display_height: i64,
    /// OS environment.  **Required.**
    /// Allowed: `"windows"`, `"mac"`, `"linux"`, `"ubuntu"`, `"browser"`.
    pub environment: String,
}

// ── 6. CodeInterpreterTool ────────────────────────────────────────────────

/// Run Python code to assist in generating responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CodeInterpreterTool {
    /// Container configuration defining the code execution environment.
    pub container: CodeInterpreterContainer,
}

// ── 7. ImageGenerationTool ────────────────────────────────────────────────

/// Generate images using a GPT image model.
///
/// Type discriminator: `"image_generation"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImageGenerationTool {
    /// Generation action. Allowed: `"generate"`, `"edit"`, `"auto"` (default).
    /// Default: `"auto"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Background mode. Allowed: `"transparent"`, `"opaque"`, `"auto"`.
    /// Default: `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    /// Input fidelity. Allowed: `"high"`, `"low"`.
    /// **Only for `gpt-image-1` / `gpt-image-1.5`.** Default: `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_fidelity: Option<String>,
    /// Mask image for guided generation.  Default: `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_image_mask: Option<ImageMask>,
    /// Model override. Allowed: `"gpt-image-1"`, `"gpt-image-1-mini"`,
    /// `"gpt-image-1.5"`.  Default: `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Content moderation level. Allowed: `"auto"`, `"low"`.  Default: `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moderation: Option<String>,
    /// Output compression level, `0`–`100`.  Default: `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_compression: Option<i64>,
    /// Output image format. Allowed: `"png"`, `"webp"`, `"jpeg"`.
    /// Default: `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
    /// Number of partial preview images (streaming), `0`–`3`.
    /// Default: `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_images: Option<i64>,
    /// Image quality. Allowed: `"low"`, `"medium"`, `"high"`, `"auto"`.
    /// Default: `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    /// Output resolution. Allowed: `"1024x1024"`, `"1024x1536"`,
    /// `"1536x1024"`, `"auto"`.  Default: `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
}

// ── 8. McpTool ────────────────────────────────────────────────────────────

/// Provide additional tools via a remote Model Context Protocol (MCP) server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpTool {
    /// Server label for identifying the MCP server.
    pub server_label: String,
    /// Server URL. Format: URI.
    pub server_url: Option<String>,
    /// Connector ID. Allowed: `"connector_dropbox"`, `"connector_gmail"`, etc.
    pub connector_id: Option<String>,
    /// Authorization header.
    pub authorization: Option<String>,
    /// Custom HTTP headers.
    pub headers: Option<HashMap<String, String>>,
    /// Server description.
    pub server_description: Option<String>,
    /// Whether to defer loading this tool.
    pub defer_loading: Option<bool>,
    /// Tool manifest for allowed tools.
    pub allowed_tools: Option<McpAllowedTools>,
    /// Approval requirement settings.
    pub require_approval: Option<McpRequireApproval>,
}

// ── 9. LocalShellTool ─────────────────────────────────────────────────────

/// Allow the model to execute shell commands in a local environment.
///
/// This type only serves as a `type` discriminator; it carries no extra fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalShellTool {}

// ── 10. ShellTool ─────────────────────────────────────────────────────────

/// Remote shell tool, allowing the model to execute commands in a remote environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ShellTool {
    /// Execution environment configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<Environment>,
}

// ── 11. CustomTool ────────────────────────────────────────────────────────

/// Custom tool that processes input using a specified format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CustomTool {
    /// Custom tool name.
    pub name: String,
    /// Tool description.
    pub description: Option<String>,
    /// Whether to defer loading this tool.
    pub defer_loading: Option<bool>,
    /// Input format definition.
    pub format: Option<CustomToolFormat>,
}

// ── 12. NamespaceTool ─────────────────────────────────────────────────────

/// Namespace tool that organizes a group of tools under a namespace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NamespaceTool {
    /// Namespace name.
    pub name: String,
    /// Namespace description.
    pub description: Option<String>,
    /// List of tools within this namespace.
    pub tools: Vec<NamespaceToolItem>,
}

// ── 13. SearchTool ────────────────────────────────────────────────────────

/// Configure search behavior for lazy-loaded tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SearchTool {
    /// Execution environment. Allowed: `"server"`, `"client"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<String>,
    /// Tool description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Parameters field (JSON Schema fragment).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

// ── 14. ApplyPatchTool ────────────────────────────────────────────────────

/// Allow the assistant to create, delete, or update files via unified diffs.
///
/// This type only serves as a `type` discriminator; it carries no extra fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPatchTool {}

// ═══════════════════════════════════════════════════════════════════════════
// Individual tool structs — *request* side (all fields optional)
// ═══════════════════════════════════════════════════════════════════════════

// ── 1. FunctionToolRequest ──────────────────────────────────────────────────

/// Define a function that can be called within code (request params).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionToolRequest {
    /// Function name.
    pub name: Option<String>,
    /// Function description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
    /// Whether to enable strict mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    /// Whether to defer loading this tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
}

// ── 2. FileSearchToolRequest ────────────────────────────────────────────────

/// Search relevant content in uploaded files (request params).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileSearchToolRequest {
    /// Vector store IDs to search.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector_store_ids: Option<Vec<String>>,
    /// Maximum number of results to return.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_num_results: Option<i64>,
    /// Search filters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<FileSearchFilters>,
    /// Sorting options.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranking_options: Option<RankingOptions>,
}

// ── 3. WebSearchToolRequest ─────────────────────────────────────────────────

/// Search the internet for sources related to a prompt (request params).
///
/// Type discriminator: `"web_search"` or `"web_search_2025_08_26"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchToolRequest {
    /// Domains to restrict the search to. Default: `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    /// Search context size. Allowed: `"low"`, `"medium"` (default), `"high"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<String>,
    /// User's approximate geographic location.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_location: Option<UserLocation>,
    /// Additional search constraints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<serde_json::Value>,
}

// ── 4. WebSearchPreviewToolRequest ──────────────────────────────────────────

/// Web search preview tool (request params).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchPreviewToolRequest {
    /// Search context size. Allowed: `"low"`, `"medium"`, `"high"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<String>,
    /// User's geographic location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<UserLocation>,
    /// Content type list to search.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_content_types: Option<Vec<String>>,
}

// ── 5. ComputerToolRequest ──────────────────────────────────────────────────

/// Tool for controlling a virtual computer (request params).
///
/// This type only serves as a `type` discriminator; it carries no extra fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputerToolRequest {}

/// Computer use preview (request params).
///
/// Type discriminator: `"computer_use_preview"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ComputerUsePreviewToolRequest {
    /// Display width in pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_width: Option<i64>,
    /// Display height in pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_height: Option<i64>,
    /// OS environment: `"windows"`, `"mac"`, `"linux"`, `"ubuntu"`, `"browser"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
}

// ── 6. CodeInterpreterToolRequest ───────────────────────────────────────────

/// Run Python code to assist in generating responses (request params).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CodeInterpreterToolRequest {
    /// Container configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<CodeInterpreterContainer>,
}

// ── 7. ImageGenerationToolRequest ───────────────────────────────────────────

/// Generate images using a GPT image model (request params).
///
/// Type discriminator: `"image_generation"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImageGenerationToolRequest {
    /// Generation action: `"generate"`, `"edit"`, `"auto"`.  Default: `"auto"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Background mode: `"transparent"`, `"opaque"`, `"auto"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    /// Input fidelity: `"high"`, `"low"`.  Only for gpt-image-1/1.5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_fidelity: Option<String>,
    /// Mask image.  Default: `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_image_mask: Option<ImageMask>,
    /// Model override: `"gpt-image-1"`, `"gpt-image-1-mini"`, `"gpt-image-1.5"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Moderation level: `"auto"`, `"low"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moderation: Option<String>,
    /// Output compression `0`–`100`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_compression: Option<i64>,
    /// Output format: `"png"`, `"webp"`, `"jpeg"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
    /// Partial preview images: `0`–`3` (streaming).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_images: Option<i64>,
    /// Quality: `"low"`, `"medium"`, `"high"`, `"auto"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    /// Resolution: `"1024x1024"`, `"1024x1536"`, `"1536x1024"`, `"auto"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
}

// ── 8. McpToolRequest ───────────────────────────────────────────────────────

/// Provide additional tools via a remote MCP server (request params).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpToolRequest {
    /// Server label.
    pub server_label: Option<String>,
    /// Server URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
    /// Connector ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<String>,
    /// Authorization header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization: Option<String>,
    /// Custom HTTP headers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    /// Server description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_description: Option<String>,
    /// Whether to defer loading this tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    /// Tool manifest for allowed tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<McpAllowedTools>,
    /// Approval requirement settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_approval: Option<McpRequireApproval>,
}

// ── 9. LocalShellToolRequest ────────────────────────────────────────────────

/// Allow the model to execute shell commands in a local environment (request params).
///
/// This type only serves as a `type` discriminator; it carries no extra fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalShellToolRequest {}

// ── 10. ShellToolRequest ────────────────────────────────────────────────────

/// Remote shell tool (request params).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ShellToolRequest {
    /// Execution environment configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<Environment>,
}

// ── 11. CustomToolRequest ───────────────────────────────────────────────────

/// Custom tool that processes input using a specified format (request params).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CustomToolRequest {
    /// Custom tool name.
    pub name: Option<String>,
    /// Tool description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether to defer loading this tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    /// Input format definition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<CustomToolFormat>,
}

// ── 12. NamespaceToolRequest ────────────────────────────────────────────────

/// Namespace tool (request params).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NamespaceToolRequest {
    /// Namespace name.
    pub name: Option<String>,
    /// Namespace description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// List of tools within this namespace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<NamespaceToolItem>>,
}

// ── 13. SearchToolRequest ───────────────────────────────────────────────────

/// Configure search behavior for lazy-loaded tools (request params).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SearchToolRequest {
    /// Execution environment. Allowed: `"server"`, `"client"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<String>,
    /// Tool description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Parameters field (JSON Schema fragment).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

// ── 14. ApplyPatchToolRequest ───────────────────────────────────────────────

/// Allow the assistant to create, delete, or update files via unified diffs (request params).
///
/// This type only serves as a `type` discriminator; it carries no extra fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPatchToolRequest {}

// ═══════════════════════════════════════════════════════════════════════════
// Tool sub-types
// ═══════════════════════════════════════════════════════════════════════════

// ── FileSearchFilters ─────────────────────────────────────────────────────

/// File search filter supporting comparison and compound conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FileSearchFilters {
    /// Comparison filter that compares a single field against a value.
    Comparison(FileSearchComparisonFilter),
    /// Compound filter combining multiple filters with a logical operator.
    Compound(FileSearchCompoundFilter),
}

/// Comparison filter on a single field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileSearchComparisonFilter {
    /// Comparison operator. Allowed: `"eq"`, `"ne"`, `"gt"`, `"gte"`, `"lt"`, `"lte"`.
    #[serde(rename = "type")]
    pub filter_type: String,
    /// Field name to compare.
    pub key: String,
    /// Target value to compare against.
    pub value: serde_json::Value,
}

/// Compound condition combining multiple filters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileSearchCompoundFilter {
    /// Logical operator. Allowed: `"and"`, `"or"`.
    #[serde(rename = "type")]
    pub filter_type: String,
    /// List of sub-filters. May nest comparison or compound filters.
    pub filters: Vec<FileSearchFilters>,
}

// ── RankingOptions ────────────────────────────────────────────────────────

/// Ranking options for search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RankingOptions {
    /// Ranker type. Allowed: `"default_2024_08_21"`, `"hybrid"`.
    pub ranker: Option<String>,
    /// Score threshold to filter out low-scoring results. Range: 0.0 to 1.0.
    pub score_threshold: Option<f64>,
    /// Hybrid search weight config, valid when ranker is `"hybrid"`.
    pub hybrid_search: Option<HybridSearchWeights>,
}

/// Hybrid search weight config, balancing dense embedding and sparse keyword matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HybridSearchWeights {
    /// Weight for dense embedding matching. Range: 0.0 to 1.0.
    pub embedding_weight: f64,
    /// Weight for text keyword matching. Range: 0.0 to 1.0.
    pub text_weight: f64,
}

// ── UserLocation ──────────────────────────────────────────────────────────

/// Approximate geographic location of the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UserLocation {
    /// Location type. Allowed: `"approximate"`.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub location_type: Option<String>,
    /// City name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    /// Country code. Format: ISO-2 two-letter code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// Region name (e.g., state, province).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// Timezone identifier. Format: IANA timezone (e.g., `"America/New_York"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

// ── CodeInterpreterContainer ──────────────────────────────────────────────

/// Container configuration for the code interpreter execution environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CodeInterpreterContainer {
    /// Auto container configuration; the platform manages the environment.
    Auto(CodeInterpreterContainerAuto),
    /// Configuration referencing an existing container.
    Reference(CodeInterpreterContainerReference),
}

/// Automatically managed code execution container.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CodeInterpreterContainerAuto {
    /// Container type. Fixed value: `"auto"`.
    #[serde(rename = "type")]
    pub container_type: String,
    /// File IDs that the container can access.  Default: `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_ids: Option<Vec<String>>,
    /// Memory limit. Allowed: `"1g"`, `"4g"`, `"16g"`, `"64g"`.  Default: `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_limit: Option<String>,
    /// Network policy.  Default: `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_policy: Option<NetworkPolicy>,
}

/// Configuration referencing an existing container by container ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CodeInterpreterContainerReference {
    /// Container type. Fixed value: `"reference"`.
    #[serde(rename = "type")]
    pub container_type: String,
    /// ID of an existing container.
    pub container_id: String,
}

// ── ImageMask ─────────────────────────────────────────────────────────────

/// Mask image for guided image generation.
///
/// `file_id` and `image_url` are mutually exclusive — provide exactly one.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImageMask {
    /// File ID of an uploaded mask image.  Mutually exclusive with `image_url`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    /// Base64-encoded mask image (data URL).  Mutually exclusive with `file_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

// ── McpAllowedTools ───────────────────────────────────────────────────────

/// Tool manifest of allowed tools for the MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpAllowedTools {
    /// List all allowed tool names.
    All(Vec<String>),
    /// Filter by read-only flag and name list.
    Filter(McpAllowedToolsFilter),
}

/// MCP tool manifest filtered by conditions and names.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpAllowedToolsFilter {
    /// Read-only tools only.
    pub read_only: Option<bool>,
    /// List of allowed tool names.
    pub tool_names: Vec<String>,
}

// ── McpRequireApproval ────────────────────────────────────────────────────

/// Approval requirement settings for MCP tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpRequireApproval {
    /// Global approval policy. Allowed: `"always"`, `"never"`.
    Setting(String),
    /// Approval policy filtered by category.
    Filter(McpRequireApprovalFilter),
}

/// MCP approval policy filtered by category, controlling always-approved and
/// never-approved tools separately.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpRequireApprovalFilter {
    /// Tools that always require approval.
    pub always: McpApprovalFilter,
    /// Tools that never require approval.
    pub never: McpApprovalFilter,
}

/// MCP approval filter controlling approval behavior for specific tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpApprovalFilter {
    /// Applies to read-only tools only.
    pub read_only: Option<bool>,
    /// List of tool names this applies to.
    pub tool_names: Vec<String>,
}

// ── CustomToolFormat ──────────────────────────────────────────────────────

/// Input format definition for custom tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CustomToolFormat {
    /// Plain text format, no extra structure.
    Text(CustomToolFormatText),
    /// Grammar-defined format using Lark grammar or regex to constrain input.
    Grammar(CustomToolFormatGrammar),
}

/// Plain text format marker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CustomToolFormatText {
    /// Format type. Fixed value: `"text"`.
    #[serde(rename = "type")]
    pub format_type: String,
}

/// Grammar-constrained format definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CustomToolFormatGrammar {
    /// Format type. Fixed value: `"grammar"`.
    #[serde(rename = "type")]
    pub format_type: String,
    /// Grammar definition content.
    pub definition: String,
    /// Syntax type. Allowed: `"lark"`, `"regex"`.
    pub syntax: String,
}

// ── NamespaceToolItem ─────────────────────────────────────────────────────

/// Single tool entry within a namespace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NamespaceToolItem {
    /// Function tool entry.
    #[serde(rename = "function")]
    Function(NamespaceToolFunction),
    /// Custom tool entry.
    #[serde(rename = "custom")]
    Custom(NamespaceToolCustom),
}

/// Function tool entry within a namespace.
///
/// `item_type` is written by the enclosing [`NamespaceToolItem`] tagged enum;
/// the field is skipped during (de)serialization here to avoid "type" name
/// collision.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct NamespaceToolFunction {
    /// Entry type. Fixed value: `"function"`. Managed by the outer enum tag.
    #[serde(rename = "type", skip, default = "default_item_type_function")]
    pub item_type: String,
    /// Function name.
    pub name: String,
    /// Function description.
    pub description: Option<String>,
    /// JSON Schema parameters.
    pub parameters: Option<serde_json::Value>,
}

fn default_item_type_function() -> String {
    "function".to_string()
}

/// Custom tool entry within a namespace.
///
/// `item_type` is written by the enclosing [`NamespaceToolItem`] tagged enum;
/// the field is skipped during (de)serialization here to avoid "type" name
/// collision.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct NamespaceToolCustom {
    /// Entry type. Fixed value: `"custom"`. Managed by the outer enum tag.
    #[serde(rename = "type", skip, default = "default_item_type_custom")]
    pub item_type: String,
    /// Custom tool name.
    pub name: String,
    /// Tool description.
    pub description: Option<String>,
    /// Input format definition.
    pub format: Option<CustomToolFormat>,
}

fn default_item_type_custom() -> String {
    "custom".to_string()
}

// ═══════════════════════════════════════════════════════════════════════════
// Network policy & environment sub-types
// ═══════════════════════════════════════════════════════════════════════════

/// Network policy for code interpreter containers.  Doc §3.8.h.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NetworkPolicy {
    /// No network access.
    #[serde(rename = "disabled")]
    Disabled,
    /// Allowlist specific domains.
    #[serde(rename = "allowlist")]
    Allowlist {
        allowed_domains: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        domain_secrets: Option<Vec<DomainSecret>>,
    },
}

/// Domain secret injected into network requests.  Doc §3.8.h.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DomainSecret {
    pub domain: String,
    pub name: String,
    pub value: String,
}

/// Execution environment for shell tools.  Doc §3.8.k.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Environment {
    /// Auto-created container.
    #[serde(rename = "container_auto")]
    ContainerAuto {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_ids: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        memory_limit: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        network_policy: Option<NetworkPolicy>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        skills: Option<Vec<Skill>>,
    },
    /// Local execution.
    #[serde(rename = "local")]
    Local {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        skills: Option<Vec<LocalSkill>>,
    },
    /// Reference to an existing container.
    #[serde(rename = "container_reference")]
    ContainerReference { container_id: String },
}

/// Local skill definition.  Doc §3.8.k.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LocalSkill {
    pub name: String,
    pub description: String,
    pub path: String,
}

/// Skill reference or inline definition.  Doc §3.8.k.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Skill {
    /// Reference to a stored skill.
    #[serde(rename = "skill_reference")]
    SkillReference {
        skill_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<String>,
    },
    /// Inline skill content.
    #[serde(rename = "inline")]
    Inline {
        name: String,
        description: String,
        source: serde_json::Value,
    },
}

// ═══════════════════════════════════════════════════════════════════════════
// ToolChoice — untagged union for tool selection
// ═══════════════════════════════════════════════════════════════════════════

/// Controls which tools the model may invoke.
///
/// Deserialization order is carefully designed to match each JSON shape to
/// the correct variant:
/// 1. Plain string (`"auto"`, `"none"`, `"required"`) → [`ToolChoice::String`]
/// 2. Object with `mode` field → [`ToolChoice::Mode`]
/// 3. Object with `type: "mcp"` and `server_label` → [`ToolChoice::Mcp`]
/// 4. Object with only `type: "apply_patch"` → [`ToolChoice::ApplyPatch`]
/// 5. Object with only `type: "shell"` → [`ToolChoice::Shell`]
/// 6. Object with `type` and `name` → [`ToolChoice::Specific`]
/// 7. Object with only `type` (built-in tool choice) → [`ToolChoice::Types`]
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ToolChoice {
    /// Plain string mode. Allowed: `"auto"`, `"none"`, `"required"`.
    String(String),
    /// Tool choice with a `mode` field (e.g., `allowed_tools` form).
    Mode(ToolChoiceMode),
    /// Specify a specific tool on an MCP server.
    Mcp(ToolChoiceMcp),
    /// Force the model to invoke the `apply_patch` tool.
    ApplyPatch(ToolChoiceApplyPatch),
    /// Force the model to invoke the `shell` tool.
    Shell(ToolChoiceShell),
    /// Specify a particular tool (function, custom, or shell).
    Specific(ToolChoiceSpecific),
    /// Specify a built-in tool (type-only, e.g., `"web_search_preview"`).
    Types(ToolChoiceTypes),
}

impl<'de> Deserialize<'de> for ToolChoice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;

        // 1. Try plain string first.
        if let Some(s) = value.as_str() {
            return Ok(ToolChoice::String(s.to_owned()));
        }

        // 2. Object -- check the shape.
        if let Some(obj) = value.as_object() {
            // "mode" field signals allowed-tools mode.
            if obj.contains_key("mode") {
                return serde_json::from_value(value)
                    .map(ToolChoice::Mode)
                    .map_err(serde::de::Error::custom);
            }

            // Dispatch by "type" field.
            if let Some(type_val) = obj.get("type").and_then(|v| v.as_str()) {
                match type_val {
                    // "mcp" with server_label → Mcp
                    "mcp" if obj.contains_key("server_label") => {
                        return serde_json::from_value(value)
                            .map(ToolChoice::Mcp)
                            .map_err(serde::de::Error::custom);
                    }
                    // Constant discriminator-only structs.
                    "apply_patch" => {
                        return serde_json::from_value(value)
                            .map(ToolChoice::ApplyPatch)
                            .map_err(serde::de::Error::custom);
                    }
                    "shell" if obj.len() == 1 => {
                        return serde_json::from_value(value)
                            .map(ToolChoice::Shell)
                            .map_err(serde::de::Error::custom);
                    }
                    // Has "name" → Specific (function, custom, named shell).
                    _ if obj.contains_key("name") => {
                        return serde_json::from_value(value)
                            .map(ToolChoice::Specific)
                            .map_err(serde::de::Error::custom);
                    }
                    // Type-only (no name) → Types (built-in tool choice).
                    _ => {
                        return serde_json::from_value(value)
                            .map(ToolChoice::Types)
                            .map_err(serde::de::Error::custom);
                    }
                }
            }

            return Err(serde::de::Error::custom(
                "ToolChoice object must have a 'mode' or 'type' field",
            ));
        }

        Err(serde::de::Error::custom(
            "ToolChoice must be a string or an object",
        ))
    }
}

// ── ToolChoice sub-types ──────────────────────────────────────────────────

/// Tool choice with `type: "allowed_tools"`.  Doc §3.7.a.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolChoiceMode {
    /// Fixed value: `"allowed_tools"`.
    #[serde(rename = "type", default = "default_allowed_tools_type")]
    pub type_: String,
    /// Selection mode. Allowed: `"auto"`, `"required"`.
    pub mode: String,
    /// List of allowed tool definitions.
    pub tools: Vec<SimplifiedTool>,
}

fn default_allowed_tools_type() -> String {
    "allowed_tools".into()
}

/// Simplified tool definition used inside `allowed_tools`.  Doc §3.7.a.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SimplifiedTool {
    /// Tool type, e.g. `"function"`, `"mcp"`.
    #[serde(rename = "type")]
    pub type_: String,
    /// Function name (for `function` type).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// MCP server label (for `mcp` type).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_label: Option<String>,
}

/// Specific tool selector (function, custom, or built-in tool).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolChoiceSpecific {
    /// Tool type, e.g., `"function"`, `"custom"`, `"shell"`, etc.
    #[serde(rename = "type")]
    pub tool_type: String,
    /// Tool name.
    pub name: String,
}

/// Specify a specific tool on an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolChoiceMcp {
    /// Fixed value `"mcp"`.
    #[serde(rename = "type")]
    pub tool_type: String,
    /// MCP server label.
    pub server_label: String,
    /// Tool name to select (optional; omitting selects the entire server).
    pub name: Option<String>,
}

/// Force the model to invoke the `apply_patch` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolChoiceApplyPatch {
    /// Fixed value `"apply_patch"`.
    #[serde(rename = "type")]
    pub tool_type: String,
}

/// Force the model to invoke the `shell` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolChoiceShell {
    /// Fixed value `"shell"`.
    #[serde(rename = "type")]
    pub tool_type: String,
}

/// Specify a built-in tool (e.g., `"web_search_preview"`, `"code_interpreter"`, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolChoiceTypes {
    /// Tool type, e.g., `"web_search_preview"`, `"code_interpreter"`, etc.
    #[serde(rename = "type")]
    pub tool_type: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── Tool deserialization ──────────────────────────────────────────

    #[test]
    fn deserialize_function_tool() {
        let json = r#"{"type":"function","name":"get_weather","description":"Get weather","parameters":{"type":"object"},"strict":true}"#;
        let tool: Tool = serde_json::from_str(json).unwrap();
        match tool {
            Tool::Function(f) => {
                assert_eq!(f.name, "get_weather");
                assert!(f.strict);
            }
            _ => panic!("expected Function variant"),
        }
    }

    #[test]
    fn deserialize_file_search_tool() {
        let json =
            r#"{"type":"file_search","vector_store_ids":["vs_abc123"],"max_num_results":10}"#;
        let tool: Tool = serde_json::from_str(json).unwrap();
        match tool {
            Tool::FileSearch(f) => {
                assert_eq!(f.vector_store_ids, vec!["vs_abc123"]);
                assert_eq!(f.max_num_results, Some(10));
            }
            _ => panic!("expected FileSearch variant"),
        }
    }

    #[test]
    fn deserialize_code_interpreter_tool() {
        let json = r#"{"type":"code_interpreter","container":{"type":"auto"}}"#;
        let tool: Tool = serde_json::from_str(json).unwrap();
        match tool {
            Tool::CodeInterpreter(c) => {
                // container is a required field
                match c.container {
                    CodeInterpreterContainer::Auto(_) => {}
                    _ => panic!("expected Auto container variant"),
                }
            }
            _ => panic!("expected CodeInterpreter variant"),
        }
    }

    #[test]
    fn deserialize_apply_patch_tool() {
        let json = r#"{"type":"apply_patch"}"#;
        let tool: Tool = serde_json::from_str(json).unwrap();
        assert!(matches!(tool, Tool::ApplyPatch(_)));
    }

    #[test]
    fn deserialize_unknown_tool() {
        let json = r#"{"type":"future_tool_v2","some_field":42}"#;
        let tool: Tool = serde_json::from_str(json).unwrap();
        assert!(matches!(tool, Tool::Unknown(_)));
    }

    #[test]
    fn roundtrip_tool_param_function() {
        let param = ToolRequest::Function(FunctionToolRequest {
            name: Some("my_func".into()),
            description: Some("Does a thing".into()),
            parameters: Some(serde_json::json!({"type": "object"})),
            strict: Some(true),
            defer_loading: None,
        });
        let json = serde_json::to_string(&param).unwrap();
        let deser: ToolRequest = serde_json::from_str(&json).unwrap();
        match deser {
            ToolRequest::Function(f) => {
                assert_eq!(f.name.as_deref(), Some("my_func"));
                assert_eq!(f.strict, Some(true));
                assert!(f.defer_loading.is_none());
            }
            _ => panic!("expected Function variant"),
        }
        // Verify serialized JSON contains the type tag.
        assert!(json.contains(r#""type":"function""#));
    }

    // ── ToolChoice deserialization ────────────────────────────────────

    #[test]
    fn deserialize_tool_choice_string() {
        let json = r#""auto""#;
        let tc: ToolChoice = serde_json::from_str(json).unwrap();
        assert!(matches!(tc, ToolChoice::String(s) if s == "auto"));
    }

    #[test]
    fn deserialize_tool_choice_mode() {
        let json = r#"{"mode":"auto","tools":[]}"#;
        let tc: ToolChoice = serde_json::from_str(json).unwrap();
        match tc {
            ToolChoice::Mode(m) => {
                assert_eq!(m.mode, "auto");
                assert!(m.tools.is_empty());
            }
            _ => panic!("expected Mode variant"),
        }
    }

    #[test]
    fn deserialize_tool_choice_specific() {
        let json = r#"{"type":"function","name":"get_weather"}"#;
        let tc: ToolChoice = serde_json::from_str(json).unwrap();
        match tc {
            ToolChoice::Specific(s) => {
                assert_eq!(s.tool_type, "function");
                assert_eq!(s.name, "get_weather");
            }
            _ => panic!("expected Specific variant"),
        }
    }

    #[test]
    fn deserialize_tool_choice_mcp() {
        let json = r#"{"type":"mcp","server_label":"my_server","name":"my_tool"}"#;
        let tc: ToolChoice = serde_json::from_str(json).unwrap();
        match tc {
            ToolChoice::Mcp(m) => {
                assert_eq!(m.tool_type, "mcp");
                assert_eq!(m.server_label, "my_server");
                assert_eq!(m.name.as_deref(), Some("my_tool"));
            }
            _ => panic!("expected Mcp variant"),
        }
    }

    #[test]
    fn deserialize_tool_choice_apply_patch() {
        let json = r#"{"type":"apply_patch"}"#;
        let tc: ToolChoice = serde_json::from_str(json).unwrap();
        assert!(matches!(tc, ToolChoice::ApplyPatch(_)));
    }

    #[test]
    fn deserialize_tool_choice_shell() {
        let json = r#"{"type":"shell"}"#;
        let tc: ToolChoice = serde_json::from_str(json).unwrap();
        assert!(matches!(tc, ToolChoice::Shell(_)));
    }

    #[test]
    fn deserialize_tool_choice_types() {
        let json = r#"{"type":"web_search_preview"}"#;
        let tc: ToolChoice = serde_json::from_str(json).unwrap();
        match tc {
            ToolChoice::Types(t) => {
                assert_eq!(t.tool_type, "web_search_preview");
            }
            _ => panic!("expected Types variant"),
        }
    }

    // ── Sub-type deserialization ──────────────────────────────────────

    #[test]
    fn deserialize_file_search_comparison_filter() {
        let json = r#"{"type":"eq","key":"filename","value":"report.pdf"}"#;
        let filter: FileSearchFilters = serde_json::from_str(json).unwrap();
        match filter {
            FileSearchFilters::Comparison(c) => {
                assert_eq!(c.filter_type, "eq");
                assert_eq!(c.key, "filename");
            }
            _ => panic!("expected Comparison variant"),
        }
    }

    #[test]
    fn deserialize_file_search_compound_filter() {
        let json =
            r#"{"type":"and","filters":[{"type":"eq","key":"filename","value":"report.pdf"}]}"#;
        let filter: FileSearchFilters = serde_json::from_str(json).unwrap();
        match filter {
            FileSearchFilters::Compound(c) => {
                assert_eq!(c.filter_type, "and");
                assert_eq!(c.filters.len(), 1);
            }
            _ => panic!("expected Compound variant"),
        }
    }

    #[test]
    fn deserialize_code_interpreter_container_auto() {
        let json = r#"{"type":"auto","memory_limit":"4g"}"#;
        let container: CodeInterpreterContainer = serde_json::from_str(json).unwrap();
        match container {
            CodeInterpreterContainer::Auto(a) => {
                assert_eq!(a.container_type, "auto");
                assert_eq!(a.memory_limit.as_deref(), Some("4g"));
            }
            _ => panic!("expected Auto variant"),
        }
    }

    #[test]
    fn deserialize_code_interpreter_container_reference() {
        let json = r#"{"type":"reference","container_id":"ctr_123"}"#;
        let container: CodeInterpreterContainer = serde_json::from_str(json).unwrap();
        match container {
            CodeInterpreterContainer::Reference(r) => {
                assert_eq!(r.container_type, "reference");
                assert_eq!(r.container_id, "ctr_123");
            }
            _ => panic!("expected Reference variant"),
        }
    }

    #[test]
    fn deserialize_mcp_allowed_tools_all() {
        let json = r#"["tool_a","tool_b"]"#;
        let allowed: McpAllowedTools = serde_json::from_str(json).unwrap();
        match allowed {
            McpAllowedTools::All(names) => assert_eq!(names, vec!["tool_a", "tool_b"]),
            _ => panic!("expected All variant"),
        }
    }

    #[test]
    fn deserialize_mcp_allowed_tools_filter() {
        let json = r#"{"read_only":true,"tool_names":["tool_a"]}"#;
        let allowed: McpAllowedTools = serde_json::from_str(json).unwrap();
        match allowed {
            McpAllowedTools::Filter(f) => {
                assert_eq!(f.read_only, Some(true));
                assert_eq!(f.tool_names, vec!["tool_a"]);
            }
            _ => panic!("expected Filter variant"),
        }
    }

    #[test]
    fn deserialize_mcp_require_approval_setting() {
        let json = r#""always""#;
        let approval: McpRequireApproval = serde_json::from_str(json).unwrap();
        match approval {
            McpRequireApproval::Setting(s) => assert_eq!(s, "always"),
            _ => panic!("expected Setting variant"),
        }
    }

    #[test]
    fn deserialize_custom_tool_format_text() {
        let json = r#"{"type":"text"}"#;
        let fmt: CustomToolFormat = serde_json::from_str(json).unwrap();
        assert!(matches!(fmt, CustomToolFormat::Text(_)));
    }

    #[test]
    fn deserialize_custom_tool_format_grammar() {
        let json = r#"{"type":"grammar","definition":"start: rule","syntax":"lark"}"#;
        let fmt: CustomToolFormat = serde_json::from_str(json).unwrap();
        match fmt {
            CustomToolFormat::Grammar(g) => {
                assert_eq!(g.format_type, "grammar");
                assert_eq!(g.syntax, "lark");
            }
            _ => panic!("expected Grammar variant"),
        }
    }

    #[test]
    fn deserialize_namespace_tool_item_function() {
        let json = r#"{"type":"function","name":"my_func","description":"A helper"}"#;
        let item: NamespaceToolItem = serde_json::from_str(json).unwrap();
        match item {
            NamespaceToolItem::Function(f) => {
                assert_eq!(f.item_type, "function");
                assert_eq!(f.name, "my_func");
            }
            _ => panic!("expected Function variant"),
        }
    }

    #[test]
    fn deserialize_namespace_tool_item_custom() {
        let json = r#"{"type":"custom","name":"my_custom"}"#;
        let item: NamespaceToolItem = serde_json::from_str(json).unwrap();
        match item {
            NamespaceToolItem::Custom(c) => {
                assert_eq!(c.item_type, "custom");
                assert_eq!(c.name, "my_custom");
            }
            _ => panic!("expected Custom variant"),
        }
    }
}
