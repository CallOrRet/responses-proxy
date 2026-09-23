use crate::types::{MessageRole, chat, item::*, responses};

// ── Main conversion: Responses API → Chat Completions API ────────────────

/// Convert a Responses API request into a Chat Completions API request.
///
/// Fetches previous conversation history from the store (via
/// `req.previous_response_id`) and handles prepending history + merging
/// `req.instructions` into the leading system message.
///
/// Returns an error with the list of unsupported Responses-only features.
pub async fn responses_to_chat(
    #[allow(unused_variables)] mut req: responses::Request,
    state: &crate::app::State,
) -> Result<chat::Request, Vec<String>> {
    let mut unsupported_features = Vec::new();
    if req.prompt.is_some() {
        unsupported_features.push("prompt".to_string());
    }
    if req.conversation.is_some() {
        unsupported_features.push("conversation".to_string());
    }
    if !unsupported_features.is_empty() {
        return Err(unsupported_features);
    }

    let mut messages: Vec<chat::MessageRequest> = Vec::new();
    let mut pending_reasoning: Option<String> = None;

    // Map reasoning effort — Responses API values match Chat API directly.
    // reasoning.summary has no Chat Completions equivalent and is left for rewrite profiles.
    let reasoning_str: Option<String> = {
        let r = req.reasoning.as_ref();
        let effort = r.and_then(|r| r.effort.as_ref());
        effort.map(|e| {
            match e {
                crate::types::ReasoningEffort::None => "none",
                crate::types::ReasoningEffort::Minimal => "minimal",
                crate::types::ReasoningEffort::Low => "low",
                crate::types::ReasoningEffort::Medium => "medium",
                crate::types::ReasoningEffort::High => "high",
                crate::types::ReasoningEffort::Xhigh => "xhigh",
                crate::types::ReasoningEffort::Max => "max",
                crate::types::ReasoningEffort::Ultra => "ultra",
            }
            .into()
        })
    };

    // Structured output → response_format
    let response_format = req
        .text
        .as_ref()
        .and_then(|t| t.format.as_ref())
        .map(|f| match f {
            responses::TextFormat::JsonSchema {
                name,
                schema,
                strict,
                description,
            } => chat::ResponseFormat::JsonSchema(chat::JsonSchemaFormat {
                format_type: "json_schema".into(),
                json_schema: chat::JsonSchema {
                    name: name.clone(),
                    description: description.clone(),
                    schema: Some(schema.clone()),
                    strict: *strict,
                },
            }),
            responses::TextFormat::JsonObject => {
                chat::ResponseFormat::JsonObject(chat::JsonObjectFormat {
                    format_type: "json_object".into(),
                })
            }
            responses::TextFormat::Text => chat::ResponseFormat::Text(chat::TextFormat {
                format_type: "text".into(),
            }),
        });

    // ── History + instructions ────────────────────────────────────────────
    let instructions = std::mem::take(&mut req.instructions).filter(|i| !i.is_empty());

    let prev_messages: Vec<chat::MessageRequest> = match req.previous_response_id {
        Some(ref prev_id) => {
            let restored = state.store().get(prev_id).await.unwrap_or_default();
            // Diagnostic for cross-session investigations: the resolved id
            // (which carries any store namespace) and how much history it
            // restored. Two independent sessions must never share a prev_id.
            tracing::debug!(
                prev_id = %prev_id,
                restored_messages = restored.len(),
                "resolved previous_response_id history"
            );
            restored
        }
        None => vec![],
    };

    if !prev_messages.is_empty() {
        messages = prev_messages;

        // The first message in stored history is always a system message
        // holding the instructions from the previous turn.
        if let Some(first) = messages.first_mut()
            && matches!(first, chat::MessageRequest::System(_))
        {
            if let Some(new_instructions) = instructions {
                *first = chat::MessageRequest::System(chat::SystemMessage {
                    content: chat::MessageContent::Text(new_instructions),
                    name: None,
                });
            }
        } else if let Some(new_instructions) = instructions {
            messages.insert(
                0,
                chat::MessageRequest::System(chat::SystemMessage {
                    content: chat::MessageContent::Text(new_instructions),
                    name: None,
                }),
            );
        }
    } else if let Some(ref new_instructions) = instructions {
        messages.push(chat::MessageRequest::System(chat::SystemMessage {
            content: chat::MessageContent::Text(new_instructions.clone()),
            name: None,
        }));
    }

    // Codex gpt-5.6 code-mode tool definitions arrive inside `additional_tools`
    // input items; collect their raw entries here and flatten them into Chat
    // function tools further down.
    let mut additional_tool_defs: Vec<serde_json::Value> = Vec::new();

    // Names Codex declared as custom (code-mode) — used to keep replayed history
    // calls consistent with the `{ input }` function schema we present. Restored
    // from the previous response on a continuation turn (see the helper).
    let custom_names = resolve_custom_tool_names(
        state,
        &req.input,
        req.tools.as_deref(),
        req.previous_response_id.as_deref(),
    )
    .await;

    // Walk input items
    let items: Vec<InputItem> = req.input;
    if !items.is_empty() {
        let cutoff = old_tool_output_cutoff(&items, KEEP_LAST_TOOL_OUTPUTS);
        let mut deferred: Vec<chat::MessageRequest> = Vec::new();
        let mut pending_tool_calls: Vec<chat::ToolCallRequest> = Vec::new();

        let flush_tools = |msgs: &mut Vec<chat::MessageRequest>,
                           p: &mut Vec<chat::ToolCallRequest>,
                           r: &mut Option<String>| {
            if !p.is_empty() {
                msgs.push(chat::MessageRequest::Assistant(chat::AssistantMessage {
                    content: None,
                    name: None,
                    refusal: None,
                    audio: None,
                    tool_calls: Some(std::mem::take(p)),
                    function_call: None,
                    reasoning_content: r.take(),
                }));
            }
        };

        for (idx, item) in items.into_iter().enumerate() {
            match item {
                InputItem::FunctionCallOutput(fco) => {
                    let cs = match &fco.output {
                        FunctionOutputValue::String(s) => s.clone(),
                        FunctionOutputValue::Array(blocks) => {
                            extract_text_from_output_blocks(blocks)
                        }
                    };
                    deferred.push(chat::MessageRequest::Tool(chat::ToolMessage {
                        content: chat::MessageContent::Text(tool_output_for_age(cs, idx, cutoff)),
                        tool_call_id: fco.call_id.clone(),
                    }));
                }
                InputItem::Reasoning(r) => {
                    flush_tools(
                        &mut messages,
                        &mut pending_tool_calls,
                        &mut pending_reasoning,
                    );
                    messages.append(&mut deferred);
                    if let Some(t) = extract_reasoning(&r, state.compact_key()) {
                        pending_reasoning = Some(match pending_reasoning.take() {
                            Some(e) if e.contains(&t) => e,
                            Some(e) => format!("{}\n{}", e, t),
                            None => t,
                        });
                    }
                }
                InputItem::Compaction(c) => {
                    flush_tools(
                        &mut messages,
                        &mut pending_tool_calls,
                        &mut pending_reasoning,
                    );
                    messages.append(&mut deferred);
                    // Decrypt encrypted_content into a system message
                    if let Some(ref encrypted) = c.encrypted_content
                        && let Some(key) = state.compact_key()
                        && let Some(decrypted) = crate::crypto::decrypt(key, encrypted)
                        && !decrypted.is_empty()
                    {
                        messages.push(chat::MessageRequest::System(chat::SystemMessage {
                            content: chat::MessageContent::Text(decrypted),
                            name: None,
                        }));
                    }
                }
                InputItem::ContextCompaction(c) => {
                    flush_tools(
                        &mut messages,
                        &mut pending_tool_calls,
                        &mut pending_reasoning,
                    );
                    messages.append(&mut deferred);
                    if let Some(ref encrypted) = c.encrypted_content
                        && let Some(key) = state.compact_key()
                        && let Some(decrypted) = crate::crypto::decrypt(key, encrypted)
                        && !decrypted.is_empty()
                    {
                        messages.push(chat::MessageRequest::System(chat::SystemMessage {
                            content: chat::MessageContent::Text(decrypted),
                            name: None,
                        }));
                    }
                }
                InputItem::AgentMessage(am) => {
                    flush_tools(
                        &mut messages,
                        &mut pending_tool_calls,
                        &mut pending_reasoning,
                    );
                    messages.append(&mut deferred);
                    if let Some(text) = agent_message_text(&am, state.compact_key()) {
                        messages.push(chat::MessageRequest::User(chat::UserMessage {
                            content: chat::UserContent::Text(text),
                            name: None,
                        }));
                    }
                }
                InputItem::FunctionCall(fc) => {
                    pending_tool_calls.push(chat::ToolCallRequest::Function {
                        id: fc.call_id.clone(),
                        function: chat::ToolCallFunction {
                            name: fc.name.clone(),
                            arguments: fc.arguments.clone(),
                        },
                    });
                }
                InputItem::Message(msg) => {
                    flush_tools(
                        &mut messages,
                        &mut pending_tool_calls,
                        &mut pending_reasoning,
                    );
                    messages.append(&mut deferred);
                    let reasoning = match msg.role {
                        MessageRole::Assistant => pending_reasoning.clone(),
                        _ => {
                            pending_reasoning.take();
                            None
                        }
                    };
                    if let Some(chat_msg) = convert_input_message(msg, reasoning) {
                        messages.push(chat_msg);
                    }
                }
                InputItem::CustomToolCall(ctc) => {
                    // We present code-mode custom tools to the model as functions
                    // taking `{ input: string }`, so the replayed assistant call
                    // must use that JSON shape (raw freeform is not valid JSON
                    // arguments). Other custom tools keep their raw input.
                    let arguments = if custom_names.contains(&ctc.name) {
                        serde_json::json!({ "input": ctc.input }).to_string()
                    } else {
                        ctc.input.clone()
                    };
                    pending_tool_calls.push(chat::ToolCallRequest::Function {
                        id: ctc.call_id.clone(),
                        function: chat::ToolCallFunction {
                            name: ctc.name.clone(),
                            arguments,
                        },
                    });
                }
                InputItem::CustomToolCallOutput(ctco) => {
                    let cs = match &ctco.output {
                        CustomToolOutputValue::String(s) => s.clone(),
                        CustomToolOutputValue::Array(blocks) => {
                            extract_text_from_output_blocks(blocks)
                        }
                    };
                    deferred.push(chat::MessageRequest::Tool(chat::ToolMessage {
                        content: chat::MessageContent::Text(tool_output_for_age(cs, idx, cutoff)),
                        tool_call_id: ctco.call_id.clone(),
                    }));
                }
                // Trigger for remote compaction v2: handled before reaching the
                // chat converter, so silently drop if it slips through here.
                InputItem::CompactionTrigger(_) => {}
                // Items Codex replays every turn that have no Chat-Completions
                // equivalent. Drop silently — they carry large opaque payloads
                // (image base64, search results) that must not be logged.
                InputItem::WebSearchCall(_)
                | InputItem::ImageGenerationCall(_)
                | InputItem::ToolSearchCall(_)
                | InputItem::ToolSearchOutput(_) => {}
                // Codex gpt-5.6 code-mode tool definitions — collect for
                // flattening into Chat function tools (not a conversation item).
                InputItem::AdditionalTools(at) => {
                    additional_tool_defs.extend(at.tools);
                }
                other => {
                    tracing::warn!(
                        item = %unconvertible_item_label(&other),
                        "InputItem variant not convertible to Chat API — skipping"
                    );
                }
            }
        }
        flush_tools(
            &mut messages,
            &mut pending_tool_calls,
            &mut pending_reasoning,
        );
        messages.append(&mut deferred);
    }

    // top_logprobs is a Chat Completions concept; Responses API uses include: ["message.output_text.logprobs"]
    let logprobs: Option<bool> = None;
    let top_logprobs_val: Option<i64> = None;

    let reasoning_effort = reasoning_str.as_ref().map(|s| match s.as_str() {
        "none" => crate::types::ReasoningEffort::None,
        "minimal" => crate::types::ReasoningEffort::Minimal,
        "low" => crate::types::ReasoningEffort::Low,
        "medium" => crate::types::ReasoningEffort::Medium,
        "high" => crate::types::ReasoningEffort::High,
        "xhigh" => crate::types::ReasoningEffort::Xhigh,
        // gpt-5.6-class tiers above xhigh — passed through verbatim.
        "max" => crate::types::ReasoningEffort::Max,
        "ultra" => crate::types::ReasoningEffort::Ultra,
        _ => crate::types::ReasoningEffort::None,
    });

    // Responses streaming completion events include final usage. Chat upstreams only
    // provide that reliably when include_usage is enabled.
    let stream_options = if req.stream {
        Some(chat::StreamOptions {
            include_usage: Some(true),
            include_obfuscation: Some(
                req.stream_options
                    .as_ref()
                    .is_none_or(|so| so.include_obfuscation),
            ),
        })
    } else {
        req.stream_options.as_ref().map(|so| chat::StreamOptions {
            include_usage: None,
            include_obfuscation: Some(so.include_obfuscation),
        })
    };

    // Merge verbosity: TextConfig.verbosity takes precedence over top-level verbosity
    let verbosity = req
        .text
        .as_ref()
        .and_then(|t| t.verbosity.as_ref())
        .or(req.verbosity.as_ref())
        .cloned();

    // Reason: summary / generate_summary — log if set (profile decides downstream field)
    if let Some(ref r) = req.reasoning {
        if r.summary.is_some() {
            tracing::debug!(?r.summary, "reasoning.summary set");
        }
        if r.generate_summary.is_some() {
            tracing::debug!(?r.generate_summary, "reasoning.generate_summary set (deprecated)");
        }
    }

    // Build the Chat tool list. Top-level `req.tools` keeps its existing
    // conversion untouched (no behavior change for gpt-5.5 and earlier). Codex
    // gpt-5.6 instead ships tool definitions inside `additional_tools` input
    // items using the code-mode `custom`/`namespace` protocol, which Chat
    // Completions rejects — those are flattened into plain `function` tools and
    // appended. When there are no `additional_tools`, this is a no-op.
    let mut chat_tools: Vec<chat::ToolRequest> = req
        .tools
        .as_ref()
        .map(|tools| {
            let allow = &state.config().allowed_tool_types;
            let allow_fn = allow.iter().any(|a| a == "function");
            let keep_raw_custom = allow.iter().any(|a| a == "custom");
            tools
                .iter()
                .flat_map(|t| tool_request_to_chat_tools(t, keep_raw_custom))
                // The allowlist gates which Chat tool types the upstream accepts.
                // Everything convertible becomes a `function`; raw `custom` is
                // only produced when the upstream opted into it above.
                .filter(|ct| match ct {
                    chat::ToolRequest::Function { .. } => allow_fn,
                    _ => true,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for v in &additional_tool_defs {
        chat_tools.extend(additional_tools_to_chat_functions(v));
    }
    if !additional_tool_defs.is_empty() {
        tracing::debug!(
            hoisted = chat_tools.len(),
            names = ?chat_tools.iter().map(chat_tool_name).collect::<Vec<_>>(),
            "hoisted Codex additional_tools into chat function tools"
        );
    }
    // Codex delivers its gpt-5.6 code-mode `additional_tools` only on a new user
    // turn. A tool-result continuation references `previous_response_id` and
    // omits them, which would leave the model with an empty tool registry — it
    // then reports "no tools available" and stalls after a single call. Restore
    // the registry cached under the previous response so the conversation keeps
    // its tools. Only fires when the current turn brought no tools of its own,
    // so non-code-mode turns (which re-send top-level `tools`) are untouched.
    if chat_tools.is_empty()
        && let Some(ref prev_id) = req.previous_response_id
        && let Some(cached) = state.store().get_tools(prev_id).await
    {
        tracing::debug!(
            restored = cached.len(),
            prev_id = %prev_id,
            "restored code-mode tool registry from previous response"
        );
        chat_tools = cached;
    }
    let tools = if chat_tools.is_empty() {
        None
    } else {
        Some(chat_tools)
    };

    Ok(chat::Request {
        model: req.model,
        messages,
        temperature: Some(req.temperature),
        top_p: Some(req.top_p),
        max_completion_tokens: req.max_output_tokens,
        stream: Some(req.stream),
        stream_options,
        // Pass through request metadata
        prompt_cache_key: req.prompt_cache_key.clone(),
        prompt_cache_retention: req.prompt_cache_retention.clone(),
        safety_identifier: req.safety_identifier.clone(),
        service_tier: req.service_tier.clone(),
        verbosity,
        parallel_tool_calls: Some(req.parallel_tool_calls),
        store: Some(req.store),
        tools,
        tool_choice: req.tool_choice.and_then(convert_tool_choice),
        response_format,
        stop: req.stop,
        logprobs,
        top_logprobs: top_logprobs_val,
        reasoning_effort,
        ..Default::default()
    })
}

/// Names of tools Codex declared as `custom` (code-mode `exec`, `namespace`
/// custom members). Their calls must be mapped back to the `custom_tool_call`
/// shape Codex expects on the response side, and their replayed history calls
/// wrapped to match the `{ input }` function schema we present. Mirrors the
/// naming used by [`additional_tools_to_chat_functions`].
pub fn custom_tool_names(
    input: &[InputItem],
    tools: Option<&[crate::types::tool::ToolRequest]>,
    keep_raw_custom: bool,
) -> std::collections::HashSet<String> {
    use crate::types::tool::ToolRequest as Rt;
    let mut names = std::collections::HashSet::new();
    // Top-level tools we present to the model as `{ input }` functions (freeform
    // custom, apply_patch, custom namespace members) must round-trip back to
    // `custom_tool_call`. A raw `custom` tool the upstream accepts unchanged
    // (`keep_raw_custom`) already emits `custom_tool_call` natively, so exclude it.
    for t in tools.unwrap_or_default() {
        names.extend(custom_function_names(t, !keep_raw_custom));
    }
    // Code-mode `additional_tools` input items carry the same shapes as raw JSON
    // and always collapse custom to functions, so include their custom names.
    for item in input {
        let InputItem::AdditionalTools(at) = item else {
            continue;
        };
        for v in &at.tools {
            let Ok(parsed) = serde_json::from_value::<Rt>(v.clone()) else {
                continue;
            };
            names.extend(custom_function_names(&parsed, true));
        }
    }
    names
}

/// Names of tools that [`tool_request_to_chat_tools`] presents to the model as
/// `{ input }` functions and which therefore need their `function_call`
/// re-emitted as `custom_tool_call`: freeform `custom` tools (when
/// `include_custom`), `apply_patch`, and custom `namespace` members.
fn custom_function_names(t: &crate::types::tool::ToolRequest, include_custom: bool) -> Vec<String> {
    use crate::types::tool::{NamespaceToolItem, ToolRequest as Rt};
    match t {
        Rt::Custom(c) if include_custom => c.name.clone().into_iter().collect(),
        Rt::ApplyPatch(_) => vec!["apply_patch".to_string()],
        Rt::Namespace(ns) => ns
            .tools
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter_map(|m| match m {
                NamespaceToolItem::Custom(nc) => Some(nc.name.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// [`custom_tool_names`] for the current turn, falling back to the set cached
/// under `previous_response_id` when the turn brought none. Codex sends
/// `additional_tools` only on a new user turn; a tool-result continuation omits
/// them, so without the fallback `exec` would round-trip as a `function_call`
/// Codex cancels instead of the `custom_tool_call` it declared. Empty for models
/// below 5.6, which never cache a set.
pub async fn resolve_custom_tool_names(
    state: &crate::app::State,
    input: &[InputItem],
    tools: Option<&[crate::types::tool::ToolRequest]>,
    previous_response_id: Option<&str>,
) -> std::collections::HashSet<String> {
    let keep_raw_custom = state
        .config()
        .allowed_tool_types
        .iter()
        .any(|a| a == "custom");
    let names = custom_tool_names(input, tools, keep_raw_custom);
    if names.is_empty()
        && let Some(prev_id) = previous_response_id
        && let Some(cached) = state.store().get_custom_names(prev_id).await
    {
        return cached;
    }
    names
}

/// Maps every tool name declared inside a Codex `namespace` tool bundle (e.g.
/// `spawn_agent` inside `"collaboration"`) to that bundle's `name`. Codex's own
/// tool-call registry is keyed by `(name, namespace)`, but Chat Completions has
/// no wire field for namespace, so [`tool_request_to_chat_tools`] flattens
/// namespace members into plain functions and this map is threaded separately
/// to re-attach the tag to the `function_call`/`custom_tool_call` output item
/// on the way back — see [`crate::convert::apply_tool_namespaces`]. Empty
/// whenever the current turn declares no `namespace` tool, which is the case
/// for every model that doesn't use them and for Codex sessions with its own
/// multi-agent feature flags turned off.
pub fn tool_namespaces(
    input: &[InputItem],
    tools: Option<&[crate::types::tool::ToolRequest]>,
) -> std::collections::HashMap<String, String> {
    use crate::types::tool::ToolRequest as Rt;
    let mut namespaces = std::collections::HashMap::new();
    for t in tools.unwrap_or_default() {
        collect_namespace_pairs(t, &mut namespaces);
    }
    // Code-mode `additional_tools` input items carry the same shapes as raw JSON.
    for item in input {
        let InputItem::AdditionalTools(at) = item else {
            continue;
        };
        for v in &at.tools {
            let Ok(parsed) = serde_json::from_value::<Rt>(v.clone()) else {
                continue;
            };
            collect_namespace_pairs(&parsed, &mut namespaces);
        }
    }
    namespaces
}

/// Collects `(member name → namespace name)` pairs from one Codex tool
/// definition, if it is a `namespace` bundle. No-op for every other tool type.
/// Unlike [`custom_function_names`], both `Function` and `Custom` namespace
/// members are collected — all six multi-agent actions currently arrive as
/// `Function` members, and namespace tagging applies regardless of shape.
fn collect_namespace_pairs(
    t: &crate::types::tool::ToolRequest,
    out: &mut std::collections::HashMap<String, String>,
) {
    use crate::types::tool::{NamespaceToolItem, ToolRequest as Rt};
    let Rt::Namespace(ns) = t else {
        return;
    };
    let Some(ns_name) = ns.name.clone() else {
        return;
    };
    for m in ns.tools.as_deref().unwrap_or_default() {
        let name = match m {
            NamespaceToolItem::Function(nf) => nf.name.clone(),
            NamespaceToolItem::Custom(nc) => nc.name.clone(),
        };
        out.insert(name, ns_name.clone());
    }
}

/// [`tool_namespaces`] for the current turn, falling back to the map cached
/// under `previous_response_id` when the turn brought none — mirrors
/// [`resolve_custom_tool_names`]: Codex sends `additional_tools`/namespace
/// bundles only on a new user turn, so a tool-result continuation would
/// otherwise lose the namespace tag for tools declared earlier.
pub async fn resolve_tool_namespaces(
    state: &crate::app::State,
    input: &[InputItem],
    tools: Option<&[crate::types::tool::ToolRequest]>,
    previous_response_id: Option<&str>,
) -> std::collections::HashMap<String, String> {
    let namespaces = tool_namespaces(input, tools);
    if namespaces.is_empty()
        && let Some(prev_id) = previous_response_id
        && let Some(cached) = state.store().get_tool_namespaces(prev_id).await
    {
        return cached;
    }
    namespaces
}

/// Flatten one Codex `additional_tools` entry (code-mode JSON) into Chat
/// Completions tools by parsing it and delegating to [`tool_request_to_chat_tools`].
/// Unrecognized entries are skipped. Chat Completions rejects raw freeform
/// `custom` tools, so `additional_tools` always collapse custom to functions.
fn additional_tools_to_chat_functions(v: &serde_json::Value) -> Vec<chat::ToolRequest> {
    let parsed: crate::types::tool::ToolRequest = match serde_json::from_value(v.clone()) {
        Ok(t) => t,
        Err(e) => {
            tracing::debug!(error = %e, "additional_tools: unrecognized tool entry, skipping");
            return Vec::new();
        }
    };
    tool_request_to_chat_tools(&parsed, false)
}

/// Single source of truth for translating one Codex tool definition into the
/// Chat Completions tools it maps to — used for both top-level `tools` and
/// code-mode `additional_tools`. Convertible shapes become `function` tools:
/// freeform `custom` tools and `apply_patch` collapse to a single `input`
/// string, and `namespace` members are flattened (names preserved so the model's
/// call routes back to the matching Codex tool). This includes the
/// `collaboration` namespace's multi-agent actions (`spawn_agent`, …) — Codex
/// CLI has client-side handlers for these that spawn a local sub-agent thread
/// and drive it with its own separate upstream calls, independent of
/// transport, so there's nothing for the proxy to fulfil beyond passing the
/// tool definition and calls through unchanged. Genuinely hosted /
/// Responses-only tools (web/file search, computer, code interpreter, image
/// generation, remote MCP server refs, `tool_search`, local/remote shells)
/// have no Chat Completions equivalent and are dropped. When `keep_raw_custom`
/// is set (the upstream accepts `custom` via `allowed-tool-types`), a freeform
/// `custom` tool is forwarded unchanged.
fn tool_request_to_chat_tools(
    t: &crate::types::tool::ToolRequest,
    keep_raw_custom: bool,
) -> Vec<chat::ToolRequest> {
    use crate::types::tool::{NamespaceToolItem, ToolRequest as Rt};
    match t {
        Rt::Function(f) => {
            let name = f.name.clone().unwrap_or_default();
            vec![function_tool(
                name,
                f.description.clone(),
                f.parameters.clone(),
                f.strict,
            )]
        }
        Rt::Custom(c) => {
            let name = c.name.clone().unwrap_or_default();
            if keep_raw_custom {
                vec![chat::ToolRequest::Custom {
                    custom: chat::CustomTool {
                        name,
                        description: c.description.clone(),
                        format: c.format.as_ref().map(|f| match f {
                            crate::types::tool::CustomToolFormat::Text(_) => {
                                chat::CustomToolFormat::Text
                            }
                            crate::types::tool::CustomToolFormat::Grammar(g) => {
                                chat::CustomToolFormat::Grammar {
                                    grammar: chat::Grammar {
                                        definition: g.definition.clone(),
                                        syntax: g.syntax.clone(),
                                    },
                                }
                            }
                        }),
                    },
                }]
            } else {
                vec![custom_as_function(name, c.description.clone())]
            }
        }
        // Codex's freeform apply_patch (`apply_patch_tool_type = "freeform"`).
        // Chat Completions has no freeform tool type, so present it as a function
        // taking one raw `input` patch string; its calls round-trip back to
        // `custom_tool_call` via the custom-name set.
        Rt::ApplyPatch(_) => vec![custom_as_function("apply_patch".to_string(), None)],
        Rt::Namespace(ns) => ns
            .tools
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|item| match item {
                NamespaceToolItem::Function(nf) => function_tool(
                    nf.name.clone(),
                    nf.description.clone(),
                    nf.parameters.clone(),
                    nf.strict,
                ),
                NamespaceToolItem::Custom(nc) => {
                    custom_as_function(nc.name.clone(), nc.description.clone())
                }
            })
            .collect(),
        // e.g. `mcp` (remote server reference — no per-tool schema to flatten),
        // `tool_search`, `web_search`, `file_search`, `computer`,
        // `code_interpreter`, `image_generation`, `local_shell`, `shell`. Codex
        // delivers usable MCP tools as a `namespace` (handled above); anything
        // here has no Chat Completions function equivalent.
        other => {
            tracing::debug!(
                tool_type = %tool_request_type_label(other),
                "tool type has no Chat function equivalent — skipping"
            );
            Vec::new()
        }
    }
}

/// Short, stable type label for a Codex tool request, used in drop logs.
fn tool_request_type_label(t: &crate::types::tool::ToolRequest) -> &'static str {
    use crate::types::tool::ToolRequest as Rt;
    match t {
        Rt::Function(_) => "function",
        Rt::FileSearch(_) => "file_search",
        Rt::WebSearch(_) => "web_search",
        Rt::WebSearchPreview(_) => "web_search_preview",
        Rt::Computer(_) => "computer",
        Rt::ComputerUsePreview(_) => "computer_use_preview",
        Rt::CodeInterpreter(_) => "code_interpreter",
        Rt::ImageGeneration(_) => "image_generation",
        Rt::Mcp(_) => "mcp",
        Rt::LocalShell(_) => "local_shell",
        Rt::Shell(_) => "shell",
        Rt::Custom(_) => "custom",
        Rt::Namespace(_) => "namespace",
        Rt::ToolSearch(_) => "tool_search",
        Rt::ApplyPatch(_) => "apply_patch",
    }
}

fn function_tool(
    name: String,
    description: Option<String>,
    mut parameters: Option<serde_json::Value>,
    strict: Option<bool>,
) -> chat::ToolRequest {
    if let Some(ref mut params) = parameters {
        strip_codex_schema_extensions(params);
    }
    chat::ToolRequest::Function {
        function: chat::FunctionTool {
            name,
            description,
            parameters,
            strict,
        },
    }
}

/// Codex tags some `collaboration`-namespace parameters (e.g. `spawn_agent`'s
/// `message`) with a non-standard `encrypted` JSON Schema keyword, presumably
/// for its own client-side handling. The upstream backend's schema validator
/// rejects it outright — not with a clean 400, but by failing the whole
/// generation and reporting a misleading `provider_model_down` error. Strip it
/// (and any future Codex-only annotation added the same way) before the
/// schema reaches Chat Completions.
fn strip_codex_schema_extensions(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("encrypted");
            for v in map.values_mut() {
                strip_codex_schema_extensions(v);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                strip_codex_schema_extensions(v);
            }
        }
        _ => {}
    }
}

/// Represent a code-mode `custom` tool (freeform text / code) as a Chat
/// `function` tool taking a single freeform string argument — Chat Completions
/// has no freeform/grammar tool type.
fn custom_as_function(name: String, description: Option<String>) -> chat::ToolRequest {
    function_tool(
        name,
        description,
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Raw tool input (freeform text or code)."
                }
            },
            "required": ["input"],
            "additionalProperties": false
        })),
        Some(false),
    )
}

fn chat_tool_name(t: &chat::ToolRequest) -> String {
    match t {
        chat::ToolRequest::Function { function } => function.name.clone(),
        chat::ToolRequest::Custom { custom } => custom.name.clone(),
    }
}

fn convert_tool_choice(tc: crate::types::tool::ToolChoice) -> Option<chat::ToolChoice> {
    match tc {
        crate::types::tool::ToolChoice::String(s) => Some(chat::ToolChoice::Mode(s)),
        crate::types::tool::ToolChoice::Specific(s) if s.tool_type == "custom" => {
            Some(chat::ToolChoice::Custom(chat::ToolChoiceCustom {
                choice_type: "custom".into(),
                custom: chat::ToolChoiceCustomName { name: s.name },
            }))
        }
        crate::types::tool::ToolChoice::Specific(s) if s.tool_type == "function" => {
            Some(chat::ToolChoice::Function(chat::ToolChoiceFunction {
                choice_type: "function".into(),
                function: chat::ToolChoiceFunctionName { name: s.name },
            }))
        }
        crate::types::tool::ToolChoice::Mode(m) => Some(chat::ToolChoice::AllowedTools(
            chat::ToolChoiceAllowedTools {
                choice_type: "allowed_tools".into(),
                mode: m.mode,
                tools: m.tools,
            },
        )),
        other => {
            tracing::warn!(
                ?other,
                "tool_choice variant not convertible to Chat API — skipping"
            );
            None
        }
    }
}

// ── Tool-output truncation (age-based) ────────────────────────────────────
//
// Tool outputs dominate replayed context (~78% of bytes in real Codex
// sessions). "Age" is measured in tool-output depth, not user turns: a single
// Codex prompt can spawn dozens of sequential tool calls with no new user
// message in between, so counting user turns left old, huge outputs in the
// replayed history unbounded. The last `KEEP_LAST_TOOL_OUTPUTS` tool results
// are kept verbatim because the model is likely still acting on them; older
// ones are truncated to head+tail with a marker.

const KEEP_LAST_TOOL_OUTPUTS: usize = 8;
const MAX_OLD_TOOL_OUTPUT_CHARS: usize = 4096;
const OLD_TOOL_OUTPUT_HEAD: usize = 2048;
const OLD_TOOL_OUTPUT_TAIL: usize = 1024;

/// Index in `items` before which tool outputs are "old" and may be truncated.
/// Everything from this index onward belongs to the last `keep_last` tool
/// outputs (`FunctionCallOutput`/`CustomToolCallOutput`) and is preserved
/// verbatim, regardless of how many user messages are interleaved. Returns 0
/// when there are not enough tool outputs to truncate anything.
fn old_tool_output_cutoff(items: &[InputItem], keep_last: usize) -> usize {
    let output_positions: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, it)| {
            matches!(
                it,
                InputItem::FunctionCallOutput(_) | InputItem::CustomToolCallOutput(_)
            )
        })
        .map(|(i, _)| i)
        .collect();
    if output_positions.len() <= keep_last {
        return 0;
    }
    output_positions[output_positions.len() - keep_last]
}

fn floor_char_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    let mut i = idx;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_char_boundary(s: &str, idx: usize) -> usize {
    let mut i = idx.min(s.len());
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Truncate an old tool output to head+tail with a marker, respecting UTF-8
/// boundaries. Returns the input unchanged when within budget or when the
/// head+tail would not actually save anything.
fn truncate_old_tool_output(s: String) -> String {
    if s.len() <= MAX_OLD_TOOL_OUTPUT_CHARS {
        return s;
    }
    let head_end = floor_char_boundary(&s, OLD_TOOL_OUTPUT_HEAD);
    let tail_start = ceil_char_boundary(&s, s.len().saturating_sub(OLD_TOOL_OUTPUT_TAIL));
    if tail_start <= head_end {
        return s;
    }
    let omitted = tail_start - head_end;
    format!(
        "{}\n…[truncated {} bytes]…\n{}",
        &s[..head_end],
        omitted,
        &s[tail_start..]
    )
}

/// Apply age-based truncation to a tool-output string given its item index and
/// the precomputed cutoff.
fn tool_output_for_age(cs: String, idx: usize, cutoff: usize) -> String {
    if idx < cutoff {
        truncate_old_tool_output(cs)
    } else {
        cs
    }
}

/// Serialized character size of a converted Chat message list.
fn messages_total_chars(messages: &[chat::MessageRequest]) -> usize {
    messages
        .iter()
        .map(|m| serde_json::to_string(m).map(|s| s.len()).unwrap_or(0))
        .sum()
}

/// Enforce a total character ceiling on a converted Chat request. Age-based
/// truncation shrinks per-output; this is the global backstop the per-model
/// `history.max-input-chars` config drives. It walks tool messages from oldest
/// to newest, replacing their content with a head+tail marker, until the total
/// serialized size fits under `max_chars` (or no tool output remains to shrink).
/// Tool-call/output pairing is never broken — only the textual content shrinks.
/// Returns the number of messages whose content was reduced.
pub fn enforce_input_budget(messages: &mut [chat::MessageRequest], max_chars: usize) -> usize {
    if messages_total_chars(messages) <= max_chars {
        return 0;
    }
    let mut shrunk = 0;
    for idx in 0..messages.len() {
        if let chat::MessageRequest::Tool(t) = &messages[idx] {
            let text = match &t.content {
                chat::MessageContent::Text(s) => s.clone(),
                chat::MessageContent::Parts(parts) => {
                    parts.iter().map(|p| p.text.as_str()).collect::<String>()
                }
            };
            if text.len() > MAX_OLD_TOOL_OUTPUT_CHARS {
                let truncated = truncate_old_tool_output(text);
                if let chat::MessageRequest::Tool(t) = &mut messages[idx] {
                    t.content = chat::MessageContent::Text(truncated);
                }
                shrunk += 1;
                if messages_total_chars(messages) <= max_chars {
                    break;
                }
            }
        }
    }
    shrunk
}

/// Trim the converted Chat message list to at most `max_messages` by dropping
/// the oldest conversation turns. Leading System/Developer messages (the
/// instructions block) are always kept. Cuts only at user-message boundaries so
/// assistant tool_calls stay paired with their tool responses. Returns the
/// number of messages dropped. `max_messages == 0` means unlimited.
pub fn enforce_message_budget(
    messages: &mut Vec<chat::MessageRequest>,
    max_messages: usize,
) -> usize {
    if max_messages == 0 || messages.len() <= max_messages {
        return 0;
    }

    // Leading run of System/Developer messages — always retained.
    let prefix_len = messages
        .iter()
        .take_while(|m| {
            matches!(
                m,
                chat::MessageRequest::System(_) | chat::MessageRequest::Developer(_)
            )
        })
        .count();

    // User-message positions after the prefix — each starts a conversation turn.
    let user_starts: Vec<usize> = messages
        .iter()
        .enumerate()
        .skip(prefix_len)
        .filter(|(_, m)| matches!(m, chat::MessageRequest::User(_)))
        .map(|(i, _)| i)
        .collect();

    // Earliest turn start that makes prefix + tail fit under the limit. Falls
    // back to the last turn if even one turn plus prefix still overflows (a
    // single enormous turn we cannot split without breaking tool pairing).
    let start = match user_starts
        .iter()
        .copied()
        .find(|&s| prefix_len + (messages.len() - s) <= max_messages)
    {
        Some(s) => s,
        None => match user_starts.last() {
            Some(&s) => s,
            None => return 0,
        },
    };

    if start <= prefix_len {
        return 0;
    }
    let removed = start - prefix_len;
    messages.drain(prefix_len..start);
    removed
}

/// Namespace names Codex uses for its own tools (as opposed to MCP/app-registry
/// tools flattened into a namespace bundle by [`tool_request_to_chat_tools`]).
/// Sourced from [`tool_namespaces`]'s member→namespace map. May need extending
/// if Codex introduces new core-tool namespace bundles.
const CODEX_OWNED_NAMESPACES: &[&str] = &["shell", "collaboration"];

/// Cap the number of tools forwarded to the upstream. Some gateways reject a
/// request carrying more than a fixed number of tools (e.g. Codex code mode can
/// flatten a large MCP/app-tool registry into hundreds of functions). When the
/// converted list exceeds `max`, prefer keeping Codex's own tools over
/// MCP-sourced ones: a tool is "protected" if `namespaces` has no entry for it
/// (declared directly by Codex, not inside a namespace bundle) or its namespace
/// is in [`CODEX_OWNED_NAMESPACES`]; everything else is dropped first. Only
/// reaches into the protected set if it alone still exceeds `max`, in which
/// case this degrades to a plain tail-cut. Original relative order is preserved
/// among survivors either way. Returns the dropped names (original order) for
/// logging. No-op when `max` is 0 or the list already fits.
pub fn enforce_tool_budget(
    tools: &mut Vec<chat::ToolRequest>,
    max: usize,
    namespaces: &std::collections::HashMap<String, String>,
) -> Vec<String> {
    if max == 0 || tools.len() <= max {
        return Vec::new();
    }
    let tier = |t: &chat::ToolRequest| -> u8 {
        match namespaces.get(&chat_tool_name(t)) {
            None => 0,
            Some(ns) if CODEX_OWNED_NAMESPACES.contains(&ns.as_str()) => 0,
            Some(_) => 1,
        }
    };
    let mut order: Vec<usize> = (0..tools.len()).collect();
    order.sort_by_key(|&i| tier(&tools[i]));
    let keep: std::collections::HashSet<usize> = order.into_iter().take(max).collect();
    let dropped: Vec<String> = tools
        .iter()
        .enumerate()
        .filter(|(i, _)| !keep.contains(i))
        .map(|(_, t)| chat_tool_name(t))
        .collect();
    let mut kept = Vec::with_capacity(max);
    for (i, t) in tools.drain(..).enumerate() {
        if keep.contains(&i) {
            kept.push(t);
        }
    }
    *tools = kept;
    dropped
}

/// Joins an [`AgentMessage`]'s content blocks into plain text. Encrypted
/// blocks are decrypted with the compaction key (same mechanism as
/// `Compaction`/`ContextCompaction`) when available; a block that can't be
/// decrypted is dropped rather than failing the whole message. Returns `None`
/// if nothing readable survives, so the caller can skip pushing an empty
/// message.
fn agent_message_text(am: &AgentMessage, key: Option<&[u8; 32]>) -> Option<String> {
    let mut parts = Vec::with_capacity(am.content.len());
    for block in &am.content {
        match block {
            AgentMessageContent::InputText { text } => parts.push(text.clone()),
            AgentMessageContent::EncryptedContent { encrypted_content } => {
                if let Some(key) = key
                    && let Some(decrypted) = crate::crypto::decrypt(key, encrypted_content)
                    && !decrypted.is_empty()
                {
                    parts.push(decrypted);
                }
            }
            AgentMessageContent::Unknown(_) => {}
        }
    }
    if parts.is_empty() {
        tracing::warn!(
            author = %am.author,
            recipient = %am.recipient,
            blocks = am.content.len(),
            "agent_message produced no readable text — dropping (empty, undecryptable, or unrecognized content)"
        );
        None
    } else {
        Some(parts.join("\n"))
    }
}

/// A short, log-safe label for an input item that the converter does not map to
/// a Chat message. Codex replays large items (image base64, encrypted
/// summaries) that must never be logged verbatim — only the discriminant `type`
/// and length are emitted.
fn unconvertible_item_label(item: &InputItem) -> String {
    match serde_json::to_value(item) {
        Ok(value) => {
            let kind = value
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("unknown")
                .to_string();
            let bytes = value.to_string().len();
            format!("{kind} ({bytes} bytes)")
        }
        Err(_) => "unserializable".to_string(),
    }
}

// ── Bulk conversion: Vec<InputItem> → Vec<MessageRequest> ─

pub fn items_to_chat_messages(
    items: &[InputItem],
    state: &crate::app::State,
) -> Vec<chat::MessageRequest> {
    let cutoff = old_tool_output_cutoff(items, KEEP_LAST_TOOL_OUTPUTS);
    let mut messages: Vec<chat::MessageRequest> = Vec::new();
    let mut pending_reasoning: Option<String> = None;
    let mut deferred: Vec<chat::MessageRequest> = Vec::new();
    let mut pending_tool_calls: Vec<chat::ToolCallRequest> = Vec::new();

    let flush = |msgs: &mut Vec<chat::MessageRequest>,
                 p: &mut Vec<chat::ToolCallRequest>,
                 r: &mut Option<String>| {
        if !p.is_empty() {
            msgs.push(chat::MessageRequest::Assistant(chat::AssistantMessage {
                content: None,
                name: None,
                refusal: None,
                audio: None,
                tool_calls: Some(std::mem::take(p)),
                function_call: None,
                reasoning_content: r.take(),
            }));
        }
    };

    for (idx, item) in items.iter().enumerate() {
        match item {
            InputItem::FunctionCallOutput(fco) => {
                let cs = match &fco.output {
                    FunctionOutputValue::String(s) => s.clone(),
                    FunctionOutputValue::Array(blocks) => extract_text_from_output_blocks(blocks),
                };
                deferred.push(chat::MessageRequest::Tool(chat::ToolMessage {
                    content: chat::MessageContent::Text(tool_output_for_age(cs, idx, cutoff)),
                    tool_call_id: fco.call_id.clone(),
                }));
            }
            InputItem::Reasoning(r) => {
                flush(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                );
                messages.append(&mut deferred);
                if let Some(t) = extract_reasoning(r, state.compact_key()) {
                    pending_reasoning = Some(match pending_reasoning.take() {
                        Some(e) if e.contains(&t) => e,
                        Some(e) => format!("{}\n{}", e, t),
                        None => t,
                    });
                }
            }
            InputItem::Compaction(c) => {
                flush(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                );
                messages.append(&mut deferred);
                // Decrypt encrypted_content into a system message
                if let Some(ref encrypted) = c.encrypted_content
                    && let Some(key) = state.compact_key()
                    && let Some(text) = crate::crypto::decrypt(key, encrypted)
                    && !text.is_empty()
                {
                    messages.push(chat::MessageRequest::System(chat::SystemMessage {
                        content: chat::MessageContent::Text(text),
                        name: None,
                    }));
                }
            }
            InputItem::ContextCompaction(c) => {
                flush(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                );
                messages.append(&mut deferred);
                if let Some(ref encrypted) = c.encrypted_content
                    && let Some(key) = state.compact_key()
                    && let Some(text) = crate::crypto::decrypt(key, encrypted)
                    && !text.is_empty()
                {
                    messages.push(chat::MessageRequest::System(chat::SystemMessage {
                        content: chat::MessageContent::Text(text),
                        name: None,
                    }));
                }
            }
            InputItem::AgentMessage(am) => {
                flush(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                );
                messages.append(&mut deferred);
                if let Some(text) = agent_message_text(am, state.compact_key()) {
                    messages.push(chat::MessageRequest::User(chat::UserMessage {
                        content: chat::UserContent::Text(text),
                        name: None,
                    }));
                }
            }
            InputItem::FunctionCall(fc) => {
                pending_tool_calls.push(chat::ToolCallRequest::Function {
                    id: fc.call_id.clone(),
                    function: chat::ToolCallFunction {
                        name: fc.name.clone(),
                        arguments: fc.arguments.clone(),
                    },
                });
            }
            InputItem::Message(msg) => {
                flush(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                );
                messages.append(&mut deferred);
                let r = match msg.role {
                    MessageRole::Assistant => pending_reasoning.clone(),
                    _ => {
                        pending_reasoning.take();
                        None
                    }
                };
                if let Some(chat_msg) = convert_input_message(msg.clone(), r) {
                    messages.push(chat_msg);
                }
            }
            InputItem::CustomToolCall(ctc) => {
                pending_tool_calls.push(chat::ToolCallRequest::Function {
                    id: ctc.call_id.clone(),
                    function: chat::ToolCallFunction {
                        name: ctc.name.clone(),
                        arguments: ctc.input.clone(),
                    },
                });
            }
            InputItem::CustomToolCallOutput(ctco) => {
                let cs = match &ctco.output {
                    CustomToolOutputValue::String(s) => s.clone(),
                    CustomToolOutputValue::Array(blocks) => extract_text_from_output_blocks(blocks),
                };
                deferred.push(chat::MessageRequest::Tool(chat::ToolMessage {
                    content: chat::MessageContent::Text(tool_output_for_age(cs, idx, cutoff)),
                    tool_call_id: ctco.call_id.clone(),
                }));
            }
            // Trigger for remote compaction v2 — handled upstream of conversion.
            InputItem::CompactionTrigger(_) => {}
            // Items Codex replays every turn with no Chat-Completions
            // equivalent. Drop silently — large opaque payloads. `additional_tools`
            // carries tool definitions (hoisted into chat tools elsewhere), not a
            // message, so it belongs here rather than in the warn fallback.
            InputItem::WebSearchCall(_)
            | InputItem::ImageGenerationCall(_)
            | InputItem::ToolSearchCall(_)
            | InputItem::ToolSearchOutput(_)
            | InputItem::AdditionalTools(_) => {}
            other => {
                tracing::warn!(
                    item = %unconvertible_item_label(other),
                    "InputItem variant not convertible to Chat API in items_to_chat_messages — skipping"
                );
            }
        }
    }
    flush(
        &mut messages,
        &mut pending_tool_calls,
        &mut pending_reasoning,
    );
    messages.append(&mut deferred);
    messages
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn convert_input_message(
    msg: InputMessage,
    reasoning: Option<String>,
) -> Option<chat::MessageRequest> {
    if msg.content.is_empty() && msg.role != MessageRole::Assistant {
        return None;
    }
    let mapped_role = match msg.role {
        MessageRole::User => "user",
        MessageRole::System => "system",
        MessageRole::Developer => "developer",
        MessageRole::Assistant => "assistant",
        _ => return None,
    };

    match mapped_role {
        "system" => {
            let text = extract_text(&msg.content);
            Some(chat::MessageRequest::System(chat::SystemMessage {
                content: chat::MessageContent::Text(text),
                name: None,
            }))
        }
        "developer" => {
            let text = extract_text(&msg.content);
            Some(chat::MessageRequest::Developer(chat::DeveloperMessage {
                content: chat::MessageContent::Text(text),
                name: None,
            }))
        }
        "user" => {
            let parts = convert_content_to_user_parts(&msg.content);
            if parts.is_empty() {
                None
            } else if parts
                .iter()
                .all(|p| matches!(p, chat::ContentPart::Text { .. }))
            {
                // All-text content → join into plain string format for compatibility
                let text: String = parts
                    .iter()
                    .filter_map(|p| match p {
                        chat::ContentPart::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Some(chat::MessageRequest::User(chat::UserMessage {
                    content: chat::UserContent::Text(text),
                    name: None,
                }))
            } else {
                // Multimodal content → Parts format
                Some(chat::MessageRequest::User(chat::UserMessage {
                    content: chat::UserContent::Parts(parts),
                    name: None,
                }))
            }
        }
        "assistant" => {
            let text = extract_text(&msg.content);
            Some(chat::MessageRequest::Assistant(chat::AssistantMessage {
                content: if text.is_empty() {
                    None
                } else {
                    Some(chat::AssistantContent::Text(text))
                },
                name: None,
                refusal: None,
                audio: None,
                tool_calls: None,
                function_call: None,
                reasoning_content: reasoning,
            }))
        }
        _ => None,
    }
}

/// Convert Responses input content blocks to Chat user content parts.
fn convert_content_to_user_parts(blocks: &[InputContentBlock]) -> Vec<chat::ContentPart> {
    blocks
        .iter()
        .filter_map(|b| match b {
            InputContentBlock::Text { text } => {
                Some(chat::ContentPart::Text { text: text.clone() })
            }
            InputContentBlock::Image {
                image_url, detail, ..
            } => {
                let url = image_url.clone().unwrap_or_default();
                if url.is_empty() {
                    None
                } else {
                    Some(chat::ContentPart::Image {
                        image_url: chat::ImageUrl {
                            url,
                            detail: detail.clone(),
                        },
                    })
                }
            }
            InputContentBlock::File {
                file_id,
                file_url,
                file_data,
                filename,
            } => {
                if file_id.is_none() && file_url.is_none() && file_data.is_none() {
                    None
                } else {
                    Some(chat::ContentPart::File {
                        file: chat::FileData {
                            file_data: file_data.clone(),
                            file_id: file_id.clone(),
                            filename: filename.clone(),
                        },
                    })
                }
            }
            InputContentBlock::Audio { data, format } => Some(chat::ContentPart::Audio {
                input_audio: chat::InputAudio {
                    data: data.clone(),
                    format: format.clone(),
                },
            }),
        })
        .collect()
}

fn extract_text(blocks: &[InputContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            InputContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_text_from_output_blocks(blocks: &[InputContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            InputContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_reasoning(r: &Reasoning, decrypt_key: Option<&[u8; 32]>) -> Option<String> {
    // Try encrypted_content first (with decryption), then plain content, then summary.
    if let Some(ref encrypted) = r.encrypted_content
        && let Some(key) = decrypt_key
        && let Some(decrypted) = crate::crypto::decrypt(key, encrypted)
        && !decrypted.is_empty()
    {
        return Some(decrypted);
    }
    let mut parts = Vec::new();
    for v in &r.summary {
        parts.push(v.text.clone());
    }
    if let Some(ref content) = r.content {
        for v in content {
            parts.push(v.text.clone());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(text: &str) -> InputItem {
        InputItem::Message(InputMessage {
            role: MessageRole::User,
            content: vec![InputContentBlock::Text {
                text: text.to_string(),
            }],
            status: None,
        })
    }

    fn tool_out(call_id: &str, output: &str) -> InputItem {
        InputItem::FunctionCallOutput(FunctionCallOutput {
            call_id: call_id.to_string(),
            output: FunctionOutputValue::String(output.to_string()),
            id: None,
            status: None,
        })
    }

    fn custom_tool_out(call_id: &str, output: &str) -> InputItem {
        InputItem::CustomToolCallOutput(CustomToolCallOutput {
            call_id: call_id.to_string(),
            output: CustomToolOutputValue::String(output.to_string()),
            id: None,
        })
    }

    fn call_id_at(items: &[InputItem], idx: usize) -> &str {
        match &items[idx] {
            InputItem::FunctionCallOutput(o) => &o.call_id,
            InputItem::CustomToolCallOutput(o) => &o.call_id,
            other => panic!("expected a tool output at index {idx}, got {other:?}"),
        }
    }

    #[test]
    fn cutoff_counts_tool_outputs_not_user_turns() {
        // 1 user message + 20 tool outputs: the first 12 are old, the last 8
        // (outputs #13-#20, item indices 13-20) are kept verbatim.
        let mut items = vec![user("u")];
        for i in 0..20 {
            items.push(tool_out(&format!("c{i}"), "x"));
        }
        let cutoff = old_tool_output_cutoff(&items, KEEP_LAST_TOOL_OUTPUTS);
        // Output positions are 1..=20; keep last 8 → boundary at position 13.
        assert_eq!(cutoff, 13);
        for idx in 1..=12 {
            assert!(idx < cutoff, "output at index {idx} should be old");
        }
        for idx in 13..=20 {
            assert!(idx >= cutoff, "output at index {idx} should be recent");
        }
    }

    #[test]
    fn cutoff_zero_when_at_most_keep_last_outputs() {
        // 10 user turns interleaved with only 5 tool outputs: the old
        // user-turn-based rule would have truncated here, but tool-output
        // depth is what matters now, and 5 <= KEEP_LAST_TOOL_OUTPUTS (8).
        let mut items = Vec::new();
        for i in 0..5 {
            items.push(user("u"));
            items.push(tool_out(&format!("c{i}"), "x"));
        }
        for _ in 0..5 {
            items.push(user("u"));
        }
        assert_eq!(old_tool_output_cutoff(&items, KEEP_LAST_TOOL_OUTPUTS), 0);
    }

    #[test]
    fn cutoff_ignores_user_turn_placement() {
        // Same 10 tool outputs, once behind a single leading user message and
        // once with a user message between every output — the cutoff must
        // land on the same tool-output boundary (3rd-from-last output) in
        // both cases.
        let mut single_leading_user = vec![user("u")];
        let mut interleaved = Vec::new();
        for i in 0..10 {
            single_leading_user.push(tool_out(&format!("c{i}"), "x"));
            interleaved.push(user("u"));
            interleaved.push(tool_out(&format!("c{i}"), "x"));
        }
        let keep = 8;
        let cutoff_a = old_tool_output_cutoff(&single_leading_user, keep);
        let cutoff_b = old_tool_output_cutoff(&interleaved, keep);
        // In both histories the first 2 outputs (of 10) are old, so the
        // cutoff lands exactly on the 3rd output ("c2").
        assert_eq!(call_id_at(&single_leading_user, cutoff_a), "c2");
        assert_eq!(call_id_at(&interleaved, cutoff_b), "c2");
    }

    #[test]
    fn cutoff_counts_both_output_variants() {
        // FunctionCallOutput and CustomToolCallOutput both count toward the
        // last-8 window.
        let mut items = Vec::new();
        for i in 0..10 {
            if i % 2 == 0 {
                items.push(tool_out(&format!("c{i}"), "x"));
            } else {
                items.push(custom_tool_out(&format!("c{i}"), "x"));
            }
        }
        let cutoff = old_tool_output_cutoff(&items, KEEP_LAST_TOOL_OUTPUTS);
        assert_eq!(cutoff, 2, "first 2 of 10 mixed outputs should be old");
    }

    #[test]
    fn truncate_old_output_adds_marker() {
        let big = "A".repeat(10_000);
        let out = truncate_old_tool_output(big);
        assert!(out.contains("…[truncated"));
        assert!(out.starts_with(&"A".repeat(OLD_TOOL_OUTPUT_HEAD)));
        assert!(out.ends_with(&"A".repeat(OLD_TOOL_OUTPUT_TAIL)));
        assert!(out.len() < 10_000);
        // head + marker + tail must fit comfortably under the cap.
        assert!(out.len() <= MAX_OLD_TOOL_OUTPUT_CHARS + 64);
    }

    #[test]
    fn truncate_keeps_small_output() {
        let small = "tiny".to_string();
        assert_eq!(truncate_old_tool_output(small.clone()), small);
    }

    #[test]
    fn truncate_keeps_output_at_or_under_cap() {
        // Exactly at the cap, and comfortably under it, must be untouched.
        let at_cap = "x".repeat(MAX_OLD_TOOL_OUTPUT_CHARS);
        assert_eq!(truncate_old_tool_output(at_cap.clone()), at_cap);
        let under_cap = "x".repeat(2048);
        assert_eq!(truncate_old_tool_output(under_cap.clone()), under_cap);
    }

    #[test]
    fn truncate_respects_utf8_boundaries() {
        // Multi-byte chars around the cut points must not panic or split.
        let s = "𝔘".repeat(5_000); // 4 bytes each = 20_000 bytes
        let out = truncate_old_tool_output(s);
        assert!(out.contains("…[truncated"));
        // Round-trips as valid UTF-8 (String guarantees it; assert non-empty cut).
        assert!(out.len() < 20_000);
    }

    #[test]
    fn truncate_respects_utf8_boundaries_three_byte_chars() {
        // "€" is 3 bytes; the raw byte offsets OLD_TOOL_OUTPUT_HEAD (2048) and
        // len - OLD_TOOL_OUTPUT_TAIL (4976) both fall mid-character, exercising
        // both the floor (head) and ceil (tail) boundary adjustments.
        let s = "€".repeat(2_000); // 6_000 bytes
        let out = truncate_old_tool_output(s);
        assert!(out.contains("…[truncated"));
        assert!(out.starts_with(&"€".repeat(682))); // floor_char_boundary(2048) == 2046
        assert!(out.ends_with(&"€".repeat(341))); // ceil_char_boundary(4976) == 4977
    }

    #[test]
    fn tool_output_for_age_truncates_only_old() {
        let big = "B".repeat(10_000);
        // idx < cutoff → truncated
        let old = tool_output_for_age(big.clone(), 0, 5);
        assert!(old.contains("…[truncated"));
        // idx >= cutoff → verbatim
        let fresh = tool_output_for_age(big.clone(), 5, 5);
        assert_eq!(fresh, big);
    }

    fn tool_msg(call_id: &str, text: &str) -> chat::MessageRequest {
        chat::MessageRequest::Tool(chat::ToolMessage {
            content: chat::MessageContent::Text(text.to_string()),
            tool_call_id: call_id.to_string(),
        })
    }

    fn sys_msg(text: &str) -> chat::MessageRequest {
        chat::MessageRequest::System(chat::SystemMessage {
            content: chat::MessageContent::Text(text.to_string()),
            name: None,
        })
    }

    fn user_msg(text: &str) -> chat::MessageRequest {
        chat::MessageRequest::User(chat::UserMessage {
            content: chat::UserContent::Text(text.to_string()),
            name: None,
        })
    }

    fn assistant_msg(text: &str) -> chat::MessageRequest {
        chat::MessageRequest::Assistant(chat::AssistantMessage {
            content: Some(chat::AssistantContent::Text(text.to_string())),
            name: None,
            refusal: None,
            audio: None,
            tool_calls: None,
            function_call: None,
            reasoning_content: None,
        })
    }

    #[test]
    fn message_budget_noop_when_within_limit() {
        let mut msgs = vec![sys_msg("s"), user_msg("u"), assistant_msg("a")];
        assert_eq!(enforce_message_budget(&mut msgs, 10), 0);
        assert_eq!(msgs.len(), 3);
    }

    #[test]
    fn message_budget_zero_is_unlimited() {
        let mut msgs = vec![user_msg("u1"), user_msg("u2"), user_msg("u3")];
        assert_eq!(enforce_message_budget(&mut msgs, 0), 0);
        assert_eq!(msgs.len(), 3);
    }

    #[test]
    fn message_budget_drops_oldest_turns_keeps_prefix() {
        // system prefix + 4 turns of (user, assistant). Limit 5 → keep prefix
        // (1) + last 2 turns (4) = 5.
        let mut msgs = vec![sys_msg("s")];
        for i in 0..4 {
            msgs.push(user_msg(&format!("u{i}")));
            msgs.push(assistant_msg(&format!("a{i}")));
        }
        assert_eq!(msgs.len(), 9);
        let dropped = enforce_message_budget(&mut msgs, 5);
        assert_eq!(dropped, 4);
        assert_eq!(msgs.len(), 5);
        // Prefix retained, first surviving turn starts at the u2 user message.
        assert!(matches!(msgs[0], chat::MessageRequest::System(_)));
        match &msgs[1] {
            chat::MessageRequest::User(u) => match &u.content {
                chat::UserContent::Text(t) => assert_eq!(t, "u2"),
                _ => panic!("expected text"),
            },
            _ => panic!("expected user message at turn boundary"),
        }
    }

    #[test]
    fn message_budget_never_leaves_orphan_tool_message() {
        // Turn: user, assistant(tool_call), tool. Cutting mid-turn would orphan
        // the tool message; trimmer must cut only at user boundaries.
        let mut msgs = vec![sys_msg("s")];
        for i in 0..3 {
            msgs.push(user_msg(&format!("u{i}")));
            msgs.push(assistant_msg(&format!("a{i}")));
            msgs.push(tool_msg(&format!("c{i}"), "out"));
        }
        // 1 + 3*3 = 10 messages. Limit 5.
        let dropped = enforce_message_budget(&mut msgs, 5);
        assert!(dropped > 0);
        // First message after prefix must be a User (turn start), never a Tool.
        assert!(matches!(msgs[0], chat::MessageRequest::System(_)));
        assert!(
            matches!(msgs[1], chat::MessageRequest::User(_)),
            "first non-prefix message must be a user turn start, not an orphan"
        );
    }

    #[test]
    fn message_budget_keeps_last_turn_when_single_turn_overflows() {
        // Prefix + one giant turn that alone exceeds the limit → keep it.
        let mut msgs = vec![sys_msg("s"), user_msg("u0")];
        for i in 0..5 {
            msgs.push(assistant_msg(&format!("a{i}")));
            msgs.push(tool_msg(&format!("c{i}"), "out"));
        }
        let len_before = msgs.len();
        // Only one user turn exists, so nothing can be dropped without breaking it.
        let dropped = enforce_message_budget(&mut msgs, 3);
        assert_eq!(dropped, 0);
        assert_eq!(msgs.len(), len_before);
    }

    #[test]
    fn enforce_budget_noop_when_within_limit() {
        let mut msgs = vec![tool_msg("c1", "small")];
        assert_eq!(enforce_input_budget(&mut msgs, 1_000_000), 0);
        match &msgs[0] {
            chat::MessageRequest::Tool(t) => match &t.content {
                chat::MessageContent::Text(s) => assert_eq!(s, "small"),
                _ => panic!("expected text content"),
            },
            _ => panic!("expected tool message"),
        }
    }

    #[test]
    fn enforce_budget_shrinks_oldest_first_and_preserves_pairing() {
        // Two big tool outputs; budget only fits one truncated + one full.
        let mut msgs = vec![
            tool_msg("call_old", &"O".repeat(20_000)),
            tool_msg("call_new", &"N".repeat(5_000)),
        ];
        let before = messages_total_chars(&msgs);
        let shrunk = enforce_input_budget(&mut msgs, before - 10_000);
        assert!(shrunk >= 1, "should have truncated at least one output");
        // tool_call_id pairing untouched.
        match &msgs[0] {
            chat::MessageRequest::Tool(t) => {
                assert_eq!(t.tool_call_id, "call_old");
                match &t.content {
                    chat::MessageContent::Text(s) => assert!(s.contains("…[truncated")),
                    _ => panic!("expected text"),
                }
            }
            _ => panic!("expected tool message"),
        }
        assert!(messages_total_chars(&msgs) < before);
    }
}
