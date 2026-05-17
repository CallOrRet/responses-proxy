use crate::config::{RewriteCondition, RewriteConfig, RewriteStep};
use serde_json::{Map, Value};

pub fn apply_rewrite(body: &mut Value, rewrite: &RewriteConfig) -> Result<(), String> {
    if rewrite.is_empty() {
        return Ok(());
    }

    for step in &rewrite.steps {
        match step {
            RewriteStep::Rename(rules) => {
                for (from, to) in rules {
                    let from = normalize_path(from);
                    let to = normalize_path(to);
                    if let Some(value) = get_path(body, &from).cloned() {
                        remove_path(body, &from);
                        set_path(body, &to, value)?;
                    }
                }
            }
            RewriteStep::Remove(paths) => {
                for path in paths {
                    let path = normalize_path(path);
                    if has_wildcard(&path) {
                        for concrete in expand_paths(body, &path) {
                            remove_path(body, &concrete);
                        }
                    } else {
                        remove_path(body, &path);
                    }
                }
            }
            RewriteStep::Replace(rules) => {
                for (path, path_rules) in rules {
                    let path = normalize_path(path);
                    let targets: Vec<String> = if has_wildcard(&path) {
                        expand_paths(body, &path)
                    } else {
                        vec![path]
                    };
                    for target in targets {
                        for rule in path_rules {
                            if get_path(body, &target)
                                .map(|value| value_matches(value, &rule.condition))
                                .unwrap_or(false)
                            {
                                if let Some(value) = &rule.set {
                                    set_path(body, &target, value.clone())?;
                                }
                                for (replacement_path, value) in &rule.with {
                                    set_path(
                                        body,
                                        &normalize_path(replacement_path),
                                        value.clone(),
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
            RewriteStep::Reset(rules) => {
                for (path, value) in rules {
                    let path = normalize_path(path);
                    if has_wildcard(&path) {
                        for concrete in expand_paths(body, &path) {
                            if get_path(body, &concrete).is_some() {
                                set_path(body, &concrete, value.clone())?;
                            }
                        }
                    } else if get_path(body, &path).is_some() {
                        set_path(body, &path, value.clone())?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn value_matches(value: &Value, condition: &RewriteCondition) -> bool {
    match condition {
        RewriteCondition::Regex { regex, .. } => regex.is_match(&match_text(value)),
        RewriteCondition::Any(candidates) => {
            for candidate in candidates {
                if value_matches(value, candidate) {
                    return true;
                }
            }
            false
        }
        RewriteCondition::Value(candidate) => value == candidate,
    }
}

fn match_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        _ => value.to_string(),
    }
}

fn pointer_tokens(path: &str) -> Result<Vec<String>, String> {
    if path.is_empty() {
        return Ok(vec![]);
    }
    if !path.starts_with('/') {
        return Err(format!("rewrite path must be a JSON pointer: {path}"));
    }
    Ok(path
        .split('/')
        .skip(1)
        .map(|token| token.replace("~1", "/").replace("~0", "~"))
        .collect())
}

fn has_wildcard(path: &str) -> bool {
    path.split('/').any(|t| t == "*")
}

/// Expand a JSON pointer with `*` wildcards into concrete paths.
/// `/messages/*/role` → `["/messages/0/role", "/messages/1/role"]`
fn expand_paths(body: &Value, path: &str) -> Vec<String> {
    let tokens: Vec<&str> = path.split('/').skip(1).collect();
    let mut results = Vec::new();
    expand_paths_impl(body, &tokens, 0, &mut vec![], &mut results);
    results
        .into_iter()
        .map(|tokens| {
            format!(
                "/{}",
                tokens
                    .iter()
                    .map(|t| t.replace('~', "~0").replace('/', "~1"))
                    .collect::<Vec<_>>()
                    .join("/")
            )
        })
        .collect()
}

fn expand_paths_impl(
    current: &Value,
    tokens: &[&str],
    pos: usize,
    prefix: &mut Vec<String>,
    results: &mut Vec<Vec<String>>,
) {
    if pos >= tokens.len() {
        results.push(prefix.clone());
        return;
    }
    let token = tokens[pos];
    if token == "*" {
        if let Some(array) = current.as_array() {
            for (i, element) in array.iter().enumerate() {
                prefix.push(i.to_string());
                expand_paths_impl(element, tokens, pos + 1, prefix, results);
                prefix.pop();
            }
        }
    } else {
        let resolved = token.replace("~1", "/").replace("~0", "~");
        if let Ok(index) = resolved.parse::<usize>() {
            if let Some(element) = current.as_array().and_then(|a| a.get(index)) {
                prefix.push(resolved);
                expand_paths_impl(element, tokens, pos + 1, prefix, results);
                prefix.pop();
            }
        } else if let Some(element) = current.as_object().and_then(|o| o.get(&resolved)) {
            prefix.push(resolved);
            expand_paths_impl(element, tokens, pos + 1, prefix, results);
            prefix.pop();
        }
    }
}

fn normalize_path(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    if path.starts_with('/') {
        return path.to_string();
    }
    format!(
        "/{}",
        path.split('.')
            .map(|token| token.replace('~', "~0").replace('/', "~1"))
            .collect::<Vec<_>>()
            .join("/")
    )
}

fn get_path<'a>(body: &'a Value, path: &str) -> Option<&'a Value> {
    body.pointer(path)
}

fn set_path(body: &mut Value, path: &str, value: Value) -> Result<(), String> {
    if has_wildcard(path) {
        for concrete in expand_paths(body, path) {
            set_path_impl(body, &pointer_tokens(&concrete)?, value.clone())?;
        }
        return Ok(());
    }
    let tokens = pointer_tokens(path)?;
    set_path_impl(body, &tokens, value)
}

fn set_path_impl(body: &mut Value, tokens: &[String], value: Value) -> Result<(), String> {
    if tokens.is_empty() {
        *body = value;
        return Ok(());
    }

    let mut current = body;
    for token in &tokens[..tokens.len() - 1] {
        if let Ok(index) = token.parse::<usize>() {
            let array = current
                .as_array_mut()
                .ok_or_else(|| format!("rewrite path expects array before segment: {token}"))?;
            current = array
                .get_mut(index)
                .ok_or_else(|| format!("rewrite array index out of bounds: {token}"))?;
        } else {
            if !current.is_object() {
                *current = Value::Object(Map::new());
            }
            current = current
                .as_object_mut()
                .expect("object created above")
                .entry(token.clone())
                .or_insert_with(|| Value::Object(Map::new()));
        }
    }

    let last = tokens.last().expect("non-empty tokens");
    if let Ok(index) = last.parse::<usize>() {
        let array = current
            .as_array_mut()
            .ok_or_else(|| format!("rewrite path expects array at final segment: {last}"))?;
        if index >= array.len() {
            return Err(format!("rewrite array index out of bounds: {last}"));
        }
        array[index] = value;
    } else {
        if !current.is_object() {
            *current = Value::Object(Map::new());
        }
        current
            .as_object_mut()
            .expect("object created above")
            .insert(last.clone(), value);
    }
    Ok(())
}

fn remove_path(body: &mut Value, path: &str) {
    let Ok(tokens) = pointer_tokens(path) else {
        return;
    };
    if tokens.is_empty() {
        *body = Value::Null;
        return;
    }

    let parent_path = format!(
        "/{}",
        tokens[..tokens.len() - 1]
            .iter()
            .map(|token| token.replace('~', "~0").replace('/', "~1"))
            .collect::<Vec<_>>()
            .join("/")
    );
    let parent = if tokens.len() == 1 {
        Some(body)
    } else {
        body.pointer_mut(&parent_path)
    };
    let Some(parent) = parent else {
        return;
    };

    let last = tokens.last().expect("non-empty tokens");
    if let Some(object) = parent.as_object_mut() {
        object.remove(last);
    } else if let Some(array) = parent.as_array_mut()
        && let Ok(index) = last.parse::<usize>()
        && index < array.len()
    {
        array.remove(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ReplaceRule, RewriteConfig, RewriteStep};
    use serde_json::{Value, json};

    fn replace_value(when: Value, value: Value) -> ReplaceRule {
        ReplaceRule {
            condition: RewriteCondition::from_value(when).unwrap(),
            set: Some(value),
            with: Vec::new(),
        }
    }

    fn replace_paths(when: Value, paths: Vec<(&str, Value)>) -> ReplaceRule {
        ReplaceRule {
            condition: RewriteCondition::from_value(when).unwrap(),
            set: None,
            with: paths
                .into_iter()
                .map(|(path, value)| (path.to_string(), value))
                .collect(),
        }
    }

    #[test]
    fn moves_and_sets_fields() {
        let mut body = json!({
            "max_completion_tokens": 64,
            "parallel_tool_calls": true,
            "service_tier": "auto",
            "response_format": {"type": "json_schema"},
            "provider_options": {"cache": false}
        });
        let rewrite = RewriteConfig {
            steps: vec![
                RewriteStep::Rename(vec![("max_completion_tokens".into(), "max_tokens".into())]),
                RewriteStep::Remove(vec!["parallel_tool_calls".into()]),
                RewriteStep::Replace(vec![
                    (
                        "service_tier".into(),
                        vec![
                            replace_value(json!("auto"), json!("priority")),
                            replace_value(json!("priority"), json!("default")),
                        ],
                    ),
                    (
                        "response_format".into(),
                        vec![replace_value(
                            json!({"type": "json_schema"}),
                            json!({"type": "json_object"}),
                        )],
                    ),
                ]),
                RewriteStep::Reset(vec![("provider_options.cache".into(), json!(true))]),
            ],
        };
        apply_rewrite(&mut body, &rewrite).unwrap();

        assert_eq!(body["max_tokens"], 64);
        assert!(body.get("max_completion_tokens").is_none());
        assert!(body.get("parallel_tool_calls").is_none());
        assert_eq!(body["service_tier"], "default");
        assert_eq!(body["response_format"], json!({"type": "json_object"}));
        assert_eq!(body["provider_options"]["cache"], true);
    }

    #[test]
    fn reset_only_updates_existing_paths() {
        let mut body = json!({"existing": "old"});
        let rewrite = RewriteConfig {
            steps: vec![RewriteStep::Reset(vec![
                ("existing".into(), json!("new")),
                ("missing".into(), json!("created")),
            ])],
        };

        apply_rewrite(&mut body, &rewrite).unwrap();

        assert_eq!(body["existing"], "new");
        assert!(body.get("missing").is_none());
    }

    #[test]
    fn follows_configured_step_order() {
        let mut body = json!({"a": "old"});
        let rewrite = RewriteConfig {
            steps: vec![
                RewriteStep::Reset(vec![("a".into(), json!("configured"))]),
                RewriteStep::Rename(vec![("a".into(), "b".into())]),
                RewriteStep::Replace(vec![(
                    "b".into(),
                    vec![replace_value(json!("configured"), json!("replaced"))],
                )]),
            ],
        };

        apply_rewrite(&mut body, &rewrite).unwrap();

        assert!(body.get("a").is_none());
        assert_eq!(body["b"], "replaced");
    }

    #[test]
    fn supports_plain_dotted_and_json_pointer_paths() {
        let mut body = json!({
            "model": "local-model",
            "reasoning": {"effort": "high"},
            "choices": [
                {
                    "finish_reason": "length",
                    "message": {"content": "hello"}
                }
            ],
            "response_format": {"type": "json_schema"},
            "tools": [
                {"function": {"name": "old_tool"}}
            ]
        });
        let rewrite = RewriteConfig {
            steps: vec![
                RewriteStep::Reset(vec![("model".into(), json!("provider-model"))]),
                RewriteStep::Replace(vec![
                    (
                        "reasoning.effort".into(),
                        vec![replace_value(json!("high"), json!("medium"))],
                    ),
                    (
                        "choices.0.finish_reason".into(),
                        vec![replace_value(json!("length"), json!("stop"))],
                    ),
                ]),
                RewriteStep::Rename(vec![(
                    "/response_format/type".into(),
                    "/text/format/type".into(),
                )]),
                RewriteStep::Reset(vec![("/tools/0/function/name".into(), json!("new_tool"))]),
            ],
        };

        apply_rewrite(&mut body, &rewrite).unwrap();

        assert_eq!(body["model"], "provider-model");
        assert_eq!(body["reasoning"]["effort"], "medium");
        assert_eq!(body["choices"][0]["finish_reason"], "stop");
        assert!(body["response_format"].as_object().unwrap().is_empty());
        assert_eq!(body["text"]["format"]["type"], "json_schema");
        assert_eq!(body["tools"][0]["function"]["name"], "new_tool");
    }

    #[test]
    fn json_pointer_paths_support_escaping_and_array_remove() {
        let mut body = json!({
            "a/b": {"~key": "old"},
            "items": ["first", "second", "third"]
        });
        let rewrite = RewriteConfig {
            steps: vec![
                RewriteStep::Reset(vec![("/a~1b/~0key".into(), json!("new"))]),
                RewriteStep::Remove(vec!["/items/1".into()]),
            ],
        };

        apply_rewrite(&mut body, &rewrite).unwrap();

        assert_eq!(body["a/b"]["~key"], "new");
        assert_eq!(body["items"], json!(["first", "third"]));
    }

    #[test]
    fn replace_value_can_be_a_map() {
        let mut body = json!({"provider_options": "enabled"});
        let rewrite = RewriteConfig {
            steps: vec![RewriteStep::Replace(vec![(
                "provider_options".into(),
                vec![replace_value(
                    json!("enabled"),
                    json!({
                        "cache": true,
                        "priority": "high"
                    }),
                )],
            )])],
        };

        apply_rewrite(&mut body, &rewrite).unwrap();

        assert_eq!(body["provider_options"]["cache"], true);
        assert_eq!(body["provider_options"]["priority"], "high");
    }

    #[test]
    fn replace_can_write_to_an_explicit_path() {
        let mut body = json!({
            "reasoning_effort": "high",
            "provider_options": {}
        });
        let rewrite = RewriteConfig {
            steps: vec![RewriteStep::Replace(vec![(
                "reasoning_effort".into(),
                vec![replace_paths(
                    json!("high"),
                    vec![(
                        "provider_options.reasoning",
                        json!({
                            "enabled": true,
                            "budget": 4096
                        }),
                    )],
                )],
            )])],
        };

        apply_rewrite(&mut body, &rewrite).unwrap();

        assert_eq!(body["reasoning_effort"], "high");
        assert_eq!(body["provider_options"]["reasoning"]["enabled"], true);
        assert_eq!(body["provider_options"]["reasoning"]["budget"], 4096);
    }

    #[test]
    fn replace_can_write_value_and_multiple_paths_from_one_match() {
        let mut body = json!({
            "reasoning_effort": "high",
            "thinking": {"type": "disabled"},
            "provider_options": {}
        });
        let rewrite = RewriteConfig {
            steps: vec![RewriteStep::Replace(vec![(
                "reasoning_effort".into(),
                vec![ReplaceRule {
                    condition: RewriteCondition::from_value(json!("^high$")).unwrap(),
                    set: Some(json!("max")),
                    with: vec![
                        ("thinking".into(), json!({"type": "enable"})),
                        ("provider_options.reasoning".into(), json!({"budget": 4096})),
                    ],
                }],
            )])],
        };

        apply_rewrite(&mut body, &rewrite).unwrap();

        assert_eq!(body["reasoning_effort"], "max");
        assert_eq!(body["thinking"], json!({"type": "enable"}));
        assert_eq!(
            body["provider_options"]["reasoning"],
            json!({"budget": 4096})
        );
    }

    #[test]
    fn replace_can_write_to_root_with_empty_path() {
        let mut body = json!({"mode": "legacy", "old": true});
        let rewrite = RewriteConfig {
            steps: vec![RewriteStep::Replace(vec![(
                "mode".into(),
                vec![replace_paths(
                    json!("legacy"),
                    vec![(
                        "",
                        json!({
                            "mode": "modern",
                            "new": true
                        }),
                    )],
                )],
            )])],
        };

        apply_rewrite(&mut body, &rewrite).unwrap();

        assert_eq!(body, json!({"mode": "modern", "new": true}));
    }

    #[test]
    fn replace_if_supports_regex_values() {
        let mut body = json!({"reasoning_effort": "medium"});
        let rewrite = RewriteConfig {
            steps: vec![RewriteStep::Replace(vec![(
                "reasoning_effort".into(),
                vec![replace_value(json!("minimal|low|medium"), json!("high"))],
            )])],
        };

        apply_rewrite(&mut body, &rewrite).unwrap();

        assert_eq!(body["reasoning_effort"], "high");
    }

    #[test]
    fn replace_if_regex_can_be_anchored() {
        let mut body = json!({"reasoning_effort": "xhigh"});
        let rewrite = RewriteConfig {
            steps: vec![RewriteStep::Replace(vec![(
                "reasoning_effort".into(),
                vec![replace_value(json!("^(high|max|xhigh)$"), json!("max"))],
            )])],
        };

        apply_rewrite(&mut body, &rewrite).unwrap();

        assert_eq!(body["reasoning_effort"], "max");
    }

    #[test]
    fn wildcard_replace_messages_role() {
        let mut body = json!({
            "messages": [
                {"role": "developer", "content": "a"},
                {"role": "user", "content": "b"},
                {"role": "developer", "content": "c"}
            ]
        });
        let rewrite = RewriteConfig {
            steps: vec![RewriteStep::Replace(vec![(
                "messages.*.role".into(),
                vec![replace_value(json!("developer"), json!("system"))],
            )])],
        };

        apply_rewrite(&mut body, &rewrite).unwrap();

        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user"); // unchanged
        assert_eq!(body["messages"][2]["role"], "system");
    }

    #[test]
    fn wildcard_remove_array_elements() {
        let mut body = json!({
            "messages": [
                {"role": "developer", "content": "a"},
                {"role": "user", "content": "b"},
                {"role": "developer", "content": "c"}
            ]
        });
        let rewrite = RewriteConfig {
            steps: vec![RewriteStep::Remove(vec!["messages.*.role".into()])],
        };

        apply_rewrite(&mut body, &rewrite).unwrap();

        assert!(body["messages"][0].get("role").is_none());
        assert!(body["messages"][1].get("role").is_none());
        assert!(body["messages"][2].get("role").is_none());
    }

    #[test]
    fn wildcard_reset_existing_paths() {
        let mut body = json!({
            "messages": [
                {"role": "developer", "content": "a"},
                {"role": "user", "content": "b"}
            ]
        });
        let rewrite = RewriteConfig {
            steps: vec![RewriteStep::Reset(vec![(
                "messages.*.role".into(),
                json!("system"),
            )])],
        };

        apply_rewrite(&mut body, &rewrite).unwrap();

        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "system");
    }

    #[test]
    fn wildcard_nested_arrays() {
        let mut body = json!({
            "choices": [
                {"message": {"role": "assistant", "tool_calls": [
                    {"type": "function"},
                    {"type": "custom"}
                ]}}
            ]
        });
        let rewrite = RewriteConfig {
            steps: vec![RewriteStep::Replace(vec![(
                "choices.*.message.tool_calls.*.type".into(),
                vec![replace_value(json!("custom"), json!("function"))],
            )])],
        };

        apply_rewrite(&mut body, &rewrite).unwrap();

        assert_eq!(
            body["choices"][0]["message"]["tool_calls"][0]["type"],
            "function"
        );
        assert_eq!(
            body["choices"][0]["message"]["tool_calls"][1]["type"],
            "function" // was "custom", now "function"
        );
    }

    #[test]
    fn wildcard_no_match_on_non_array() {
        let mut body = json!({"messages": "not-an-array"});
        let rewrite = RewriteConfig {
            steps: vec![RewriteStep::Replace(vec![(
                "messages.*.role".into(),
                vec![replace_value(json!("developer"), json!("system"))],
            )])],
        };

        apply_rewrite(&mut body, &rewrite).unwrap();

        // unchanged — * expands to nothing on non-array
        assert_eq!(body, json!({"messages": "not-an-array"}));
    }

    #[test]
    fn deepseek_config_wildcard_developer_to_system() {
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "max_completion_tokens": 64,
            "store": true,
            "verbosity": "low",
            "parallel_tool_calls": true,
            "reasoning_effort": "none",
            "response_format": {"type": "json_schema", "json_schema": {}},
            "messages": [
                {"role": "developer", "content": "You are helpful"},
                {"role": "user", "content": "hello"}
            ]
        });
        let rewrite = RewriteConfig {
            steps: vec![
                RewriteStep::Rename(vec![("max_completion_tokens".into(), "max_tokens".into())]),
                RewriteStep::Replace(vec![
                    (
                        "messages.*.role".into(),
                        vec![replace_value(json!("developer"), json!("system"))],
                    ),
                    (
                        "reasoning_effort".into(),
                        vec![replace_paths(
                            json!("^none$"),
                            vec![("thinking.type", json!("disabled"))],
                        )],
                    ),
                    (
                        "thinking.type".into(),
                        vec![replace_paths(
                            json!("^disabled$"),
                            vec![("reasoning_effort", json!(null))],
                        )],
                    ),
                    (
                        "response_format.type".into(),
                        vec![replace_value(json!("^json_schema$"), json!("json_object"))],
                    ),
                ]),
                RewriteStep::Remove(vec![
                    "store".into(),
                    "verbosity".into(),
                    "parallel_tool_calls".into(),
                    "response_format.json_schema".into(),
                ]),
            ],
        };

        apply_rewrite(&mut body, &rewrite).unwrap();

        // Rename
        assert!(body.get("max_completion_tokens").is_none());
        assert_eq!(body["max_tokens"], 64);

        // Wildcard replace: developer → system
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user"); // unchanged

        // reasoning_effort none → thinking.type=disabled → reasoning_effort=null
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("reasoning_effort").unwrap().is_null());

        // response_format.type
        assert_eq!(body["response_format"]["type"], "json_object");

        // Removed
        assert!(body.get("store").is_none());
        assert!(body.get("verbosity").is_none());
        assert!(body.get("parallel_tool_calls").is_none());
        assert!(body["response_format"].get("json_schema").is_none());
    }
}
