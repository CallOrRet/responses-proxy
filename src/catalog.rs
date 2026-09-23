//! Codex-facing model catalog entries (the proprietary `{"models":[...]}` shape
//! Codex reads from `GET /v1/models?client_version=...`).

use crate::config::ResolvedProvider;
use crate::types::ReasoningEffort;

/// Build one Codex `ModelInfo` entry. `name` is the logical config key (e.g.
/// `gpt-5.6-sol`); `context_window` is pre-resolved by the caller (config
/// override → upstream value → `None`). Every required field must be present or
/// Codex silently discards the entry.
pub fn codex_model_entry(
    name: &str,
    provider: &ResolvedProvider,
    context_window: Option<i64>,
) -> serde_json::Value {
    let supported_reasoning_levels: Vec<serde_json::Value> = provider
        .reasoning_levels
        .iter()
        .map(|level| {
            serde_json::json!({
                "effort": level,
                "description": reasoning_description(level),
            })
        })
        .collect();

    // The gpt-5.6 and gpt-6 families drive Codex's code-mode protocol; advertising
    // it makes Codex send its tool set via `additional_tools`, which the proxy
    // flattens to Chat functions. Older families keep the plain per-call function
    // contract.
    let is_code_mode_family = name.starts_with("gpt-5.6") || name.starts_with("gpt-6");
    let apply_patch_tool_type = if is_code_mode_family {
        Some("freeform")
    } else {
        None
    };

    let mut entry = serde_json::json!({
        "slug": name,
        "display_name": name,
        "description": null,
        "default_reasoning_level": &provider.default_reasoning_level,
        "supported_reasoning_levels": supported_reasoning_levels,
        "shell_type": "shell_command",
        "visibility": "list",
        "supported_in_api": true,
        "priority": 1,
        "availability_nux": null,
        "upgrade": null,
        "base_instructions": "You are Codex, an agent based on GPT-5.",
        "support_verbosity": true,
        "default_verbosity": null,
        "apply_patch_tool_type": apply_patch_tool_type,
        "truncation_policy": {"mode": "tokens", "limit": 10000},
        "supports_parallel_tool_calls": true,
        "context_window": context_window,
        "max_context_window": context_window,
        "effective_context_window_percent": 95,
        "experimental_supported_tools": [],
    });

    if is_code_mode_family {
        entry["tool_mode"] = serde_json::json!("code_mode");
    }

    entry
}

/// Short UI label Codex shows next to each reasoning tier in its `/model` picker.
fn reasoning_description(level: &ReasoningEffort) -> &'static str {
    match level {
        ReasoningEffort::None => "Off",
        ReasoningEffort::Minimal => "Minimal",
        ReasoningEffort::Low => "Fast",
        ReasoningEffort::Medium => "Balanced",
        ReasoningEffort::High => "Deep",
        ReasoningEffort::Xhigh => "Extra deep",
        ReasoningEffort::Max => "Very deep",
        ReasoningEffort::Ultra => "Maximum",
    }
}
