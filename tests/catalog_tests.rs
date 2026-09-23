//! Regression tests for the Codex model catalog contract
//! (`responses_proxy::catalog::codex_model_entry`).

use responses_proxy::catalog::codex_model_entry;
use responses_proxy::config::{ResolvedProvider, RewriteProfile};
use responses_proxy::types::ReasoningEffort;
use std::time::Duration;

fn provider() -> ResolvedProvider {
    ResolvedProvider {
        base_url: "http://example.test".into(),
        api_key: "test-key".into(),
        model: "upstream-model".into(),
        timeout: Duration::from_secs(30),
        rewrite: RewriteProfile::default(),
        max_input_chars: None,
        max_input_messages: 1000,
        max_tools: 0,
        stream_structured_output: true,
        context_window: None,
        reasoning_levels: vec![ReasoningEffort::Medium, ReasoningEffort::High],
        default_reasoning_level: ReasoningEffort::Medium,
    }
}

#[test]
fn code_mode_family_advertises_code_mode() {
    for name in ["gpt-5.6-sol", "gpt-5.6-terra", "gpt-6-astra"] {
        let entry = codex_model_entry(name, &provider(), Some(272_000));

        assert_eq!(
            entry["tool_mode"], "code_mode",
            "{name} must advertise code_mode"
        );
        assert_eq!(
            entry["apply_patch_tool_type"], "freeform",
            "{name} must advertise freeform apply_patch"
        );
        assert_eq!(
            entry["experimental_supported_tools"],
            serde_json::json!([]),
            "{name} must not advertise experimental tools"
        );
        assert_eq!(
            entry["supported_reasoning_levels"],
            serde_json::json!([
                {"effort": "medium", "description": "Balanced"},
                {"effort": "high", "description": "Deep"},
            ]),
            "{name} must advertise the provider's reasoning ladder"
        );
        assert_eq!(entry["slug"], name);
    }
}

#[test]
fn non_code_mode_family_omits_code_mode() {
    let entry = codex_model_entry("gpt-5.5", &provider(), Some(200_000));

    assert!(
        entry.get("tool_mode").is_none(),
        "gpt-5.5 must not declare a tool_mode"
    );
    assert!(
        entry["apply_patch_tool_type"].is_null(),
        "gpt-5.5 must keep apply_patch_tool_type null"
    );
    assert_eq!(
        entry["experimental_supported_tools"],
        serde_json::json!([]),
        "gpt-5.5 must not advertise experimental tools"
    );
}
