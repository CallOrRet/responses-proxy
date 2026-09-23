use crate::convert::{
    chat_to_responses, items_to_chat_messages, output_to_input_items, responses_to_chat,
};
use crate::types::chat::{Completion as ChatCompletionResponse, Request as ChatRequest};
use crate::types::event::StreamEvent;
use crate::types::responses::{Error, Request as ResponsesRequest, ResponseStatus};
use crate::types::streaming::*;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{
        IntoResponse, Response, Sse,
        sse::{Event as SseEvent, KeepAlive},
    },
};
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

/// POST /v1/responses — handles both streaming and non-streaming responses.
pub async fn responses(
    State(state): State<crate::app::State>,
    headers: axum::http::HeaderMap,
    super::json::ResponsesJson(mut req): super::json::ResponsesJson<ResponsesRequest>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    // Optional per-instance store namespace (default empty = shared behavior).
    let ns = super::namespace_from_headers(&headers);
    let provider = state
        .config()
        .models
        .get(&req.model)
        .cloned()
        .ok_or_else(|| {
            let err = Error::invalid_request(format!(
                "Unknown model: {}. Available: {:?}",
                req.model,
                state.config().models.keys()
            ));
            (StatusCode::BAD_REQUEST, Json(err.to_http_json()))
        })?;

    if !provider.rewrite.responses_in.is_empty() {
        let mut body = serde_json::to_value(&req).map_err(|e| {
            let err = Error::server_error(e.to_string());
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err.to_http_json()))
        })?;
        crate::rewrite::apply_rewrite(&mut body, &provider.rewrite.responses_in).map_err(
            |message| {
                let err = Error::server_error(message);
                (StatusCode::INTERNAL_SERVER_ERROR, Json(err.to_http_json()))
            },
        )?;
        req = serde_json::from_value(body).map_err(|e| {
            let err = Error::invalid_request(e.to_string());
            (StatusCode::BAD_REQUEST, Json(err.to_http_json()))
        })?;
    }

    // Codex remote-compaction-v2 short-circuit: when `input` carries a
    // `compaction_trigger`, run the summary turn through the shared compaction
    // helper and return a single `compaction` output instead of a chat reply.
    if req
        .input
        .iter()
        .any(|i| matches!(i, crate::types::item::InputItem::CompactionTrigger(_)))
    {
        return handle_compaction_trigger(&state, &provider, req, &ns).await;
    }

    let model = req.model.clone();
    let is_stream = req.stream;
    let provider_model = provider.model.clone();
    let endpoint = format!("{}/chat/completions", provider.base_url);

    // Build chat request (responses_to_chat fetches history + handles instructions)
    let (mut chat_req, full_input_messages, truncation_scale) = {
        let mut cr = responses_to_chat(req.clone(), &state)
            .await
            .map_err(|unsupported| {
                let err = Error::invalid_request(format!(
                    "Unsupported features: {}",
                    unsupported.join(", ")
                ));
                (StatusCode::BAD_REQUEST, Json(err.to_http_json()))
            })?;
        cr.model = provider_model.clone();
        // Measure the content-character size BEFORE truncation. Codex compares
        // the server-reported `usage.total_tokens` against its auto-compaction
        // threshold; if we truncate and pass the upstream's smaller count
        // through, that threshold never fires and history grows unbounded. When
        // truncation actually removes content we scale the upstream's real
        // prompt-token count back up by the full/sent character ratio, so the
        // number we report tracks the upstream's own tokenizer instead of a
        // hardcoded chars-per-token guess. (If this ratio proves inaccurate for
        // some languages we can switch to a real tokenizer like tiktoken-rs.)
        let full_chars = super::input_tokens::content_chars(&cr.messages);
        let dropped =
            crate::convert::enforce_message_budget(&mut cr.messages, provider.max_input_messages);
        if dropped > 0 {
            tracing::info!(
                max_messages = provider.max_input_messages,
                dropped_messages = dropped,
                "Input exceeded history.max-input-messages — dropped oldest turns"
            );
        }
        let mut shrunk = 0;
        if let Some(max_chars) = provider.max_input_chars {
            shrunk = crate::convert::enforce_input_budget(&mut cr.messages, max_chars);
            if shrunk > 0 {
                tracing::info!(
                    max_chars,
                    shrunk_tool_outputs = shrunk,
                    "Input exceeded history.max-input-chars — truncated old tool outputs"
                );
            }
        }
        let sent_chars = super::input_tokens::content_chars(&cr.messages);
        let truncation_scale =
            (dropped > 0 || shrunk > 0).then_some((full_chars as u64, sent_chars as u64));
        let input_msgs = cr.messages.clone();
        (cr, input_msgs, truncation_scale)
    };
    // Tool name → namespace name for tools declared inside a Codex `namespace`
    // bundle (e.g. `spawn_agent` → `"collaboration"`), needed to restore the tag
    // Chat Completions can't carry on the wire. Empty whenever Codex's own
    // multi-agent feature flags are off, so this never changes behavior for
    // sessions that don't use namespace tools. Computed before the max-tools
    // check below so it can also prioritize Codex's own namespaced tools over
    // MCP-sourced ones when the budget forces a drop.
    let tool_namespaces = crate::convert::resolve_tool_namespaces(
        &state,
        &req.input,
        req.tools.as_deref(),
        req.previous_response_id.as_deref(),
    )
    .await;
    // Cap the tool list before caching/forwarding: some gateways reject requests
    // carrying more than a fixed number of tools, and Codex code mode can flatten
    // a large MCP/app-tool registry into hundreds of functions. Applied here so
    // the cached registry (restored on continuations) matches what was sent.
    if provider.max_tools > 0
        && let Some(tools) = chat_req.tools.as_mut()
    {
        let dropped =
            crate::convert::enforce_tool_budget(tools, provider.max_tools, &tool_namespaces);
        if !dropped.is_empty() {
            tracing::warn!(
                max_tools = provider.max_tools,
                dropped = dropped.len(),
                names = ?dropped,
                "Tool list exceeded max-tools — dropped overflow tools"
            );
        }
    }
    // Cache the code-mode tool registry so tool-result continuations (which
    // reference this response via `previous_response_id` but omit
    // `additional_tools`) can restore it. Empty for models below 5.6 → no-op.
    let response_tools = chat_req.tools.clone().unwrap_or_default();
    // Names Codex declared as freeform `custom` tools, used to re-emit the
    // model's `function_call` as `custom_tool_call`. Restored from the previous
    // response on a continuation turn (see the helper) — otherwise `exec`
    // returns as a `function_call` Codex cancels before it runs.
    let custom_names = crate::convert::resolve_custom_tool_names(
        &state,
        &req.input,
        req.tools.as_deref(),
        req.previous_response_id.as_deref(),
    )
    .await;
    let (messages_chars, tool_output_chars) = message_size_metrics(&chat_req.messages);
    tracing::info!(
        model = %model,
        upstream = %provider_model,
        messages = chat_req.messages.len(),
        messages_chars,
        tool_output_chars,
        stream = is_stream,
        endpoint = %endpoint,
        truncation_scale = ?truncation_scale,
        "Forwarding request"
    );

    // Handle background mode: return queued status immediately, process in background
    if req.background && !is_stream {
        let response_id = crate::store::namespaced_id(
            &ns,
            &format!("resp_{}", uuid::Uuid::new_v4().to_string().replace('-', "")),
        );
        let queued_resp = crate::types::responses::Response {
            id: response_id.clone(),
            status: ResponseStatus::Queued,
            model: model.clone(),
            output: vec![],
            ..Default::default()
        };

        if req.store {
            let mut qr = queued_resp.clone();
            qr.previous_response_id = req.previous_response_id.clone();

            let messages = items_to_chat_messages(&req.input, &state);
            state.store().put(response_id.clone(), messages).await;
        }

        // Capture full input messages for background task storage
        let bg_full_input = full_input_messages.clone();

        // Spawn background processing
        let bg_state = state.clone();
        let bg_provider = provider.clone();
        let bg_model = model.clone();
        let bg_req = req.clone();
        let bg_rid = response_id.clone();
        let bg_custom_names = custom_names.clone();
        let bg_tool_namespaces = tool_namespaces.clone();

        tokio::spawn(async move {
            let result = execute_upstream_request(
                &bg_state,
                &bg_provider,
                chat_req,
                bg_model,
                &bg_req,
                &bg_custom_names,
                &bg_tool_namespaces,
                truncation_scale,
            )
            .await;

            match result {
                Ok(mut resp) => {
                    if let Some(ref meta) = bg_req.metadata {
                        resp.metadata = Some(meta.clone());
                    }
                    resp.previous_response_id = bg_req.previous_response_id.clone();

                    let mut bg_messages = bg_full_input;
                    let output_inputs = output_to_input_items(&resp.output);
                    bg_messages.extend(items_to_chat_messages(&output_inputs, &bg_state));
                    bg_state.store().put(bg_rid, bg_messages).await;
                }
                Err(err) => {
                    let rid = bg_rid.clone();
                    bg_state.store().put(bg_rid, bg_full_input).await;

                    tracing::error!(
                        response_id = %rid,
                        error = %err,
                        "Background response processing failed"
                    );
                }
            }
        });

        return Ok((StatusCode::ACCEPTED, Json(queued_resp)).into_response());
    }

    // Some upstream gateways reject `stream: true` combined
    // with a structured `response_format` (json_schema/json_object) — Codex's
    // Guardian judge hits exactly that. For providers flagged
    // `stream-structured-output: false`, fetch the response non-streamed and
    // replay it to the client as SSE. Streaming deltas are useless for
    // structured output anyway (partial JSON isn't parseable).
    let buffer_structured = matches!(
        chat_req.response_format,
        Some(crate::types::chat::ResponseFormat::JsonSchema(_))
            | Some(crate::types::chat::ResponseFormat::JsonObject(_))
    ) && !provider.stream_structured_output;

    if is_stream && buffer_structured {
        handle_streaming_structured(
            &state,
            &provider,
            chat_req,
            model,
            req,
            full_input_messages,
            response_tools,
            custom_names,
            tool_namespaces,
            truncation_scale,
            &ns,
        )
        .await
        .map(|s| s.into_response())
    } else if is_stream {
        handle_streaming(
            &state,
            &provider,
            chat_req,
            model,
            req,
            full_input_messages,
            response_tools,
            custom_names,
            tool_namespaces,
            truncation_scale,
            &ns,
        )
        .await
        .map(|s| s.into_response())
    } else {
        handle_non_streaming(
            &state,
            &provider,
            chat_req,
            model,
            req,
            full_input_messages,
            response_tools,
            custom_names,
            tool_namespaces,
            truncation_scale,
            &ns,
        )
        .await
        .map(|j| j.into_response())
    }
}

// ── Telemetry ──────────────────────────────────────────────────────────────

/// Returns `(total_chars, tool_output_chars)` for an outgoing message list.
/// Used to track context bloat — tool outputs dominate replayed history.
fn message_size_metrics(messages: &[crate::types::chat::MessageRequest]) -> (usize, usize) {
    use crate::types::chat::{MessageContent, MessageRequest};
    let content_len = |c: &MessageContent| -> usize {
        match c {
            MessageContent::Text(s) => s.len(),
            MessageContent::Parts(parts) => parts.iter().map(|p| p.text.len()).sum(),
        }
    };
    let mut total = 0;
    let mut tool = 0;
    for m in messages {
        let len = serde_json::to_string(m).map(|s| s.len()).unwrap_or(0);
        total += len;
        if let MessageRequest::Tool(t) = m {
            tool += content_len(&t.content);
        }
    }
    (total, tool)
}

// ── Shared upstream call ──────────────────────────────────────────────────

fn send_chat_request(
    request: reqwest::RequestBuilder,
    chat_req: &ChatRequest,
    provider: &crate::config::ResolvedProvider,
) -> Result<reqwest::RequestBuilder, String> {
    if provider.rewrite.chat_out.is_empty() {
        tracing::debug!(
            "chat request: {}",
            serde_json::to_string(chat_req).unwrap_or_default()
        );
        return Ok(request.json(chat_req));
    }

    let mut body = serde_json::to_value(chat_req).map_err(|e| e.to_string())?;
    crate::rewrite::apply_rewrite(&mut body, &provider.rewrite.chat_out)?;
    tracing::debug!(
        "chat request: {}",
        serde_json::to_string(&body).unwrap_or_default()
    );
    Ok(request.json(&body))
}

/// Call upstream Chat API, parse the response, convert to Responses format,
/// and apply include-based trimming. Does NOT handle persistence or metadata.
#[allow(clippy::too_many_arguments)]
async fn execute_upstream_request(
    state: &crate::app::State,
    provider: &crate::config::ResolvedProvider,
    chat_req: ChatRequest,
    model: String,
    original_req: &ResponsesRequest,
    custom_names: &std::collections::HashSet<String>,
    tool_namespaces: &std::collections::HashMap<String, String>,
    truncation_scale: Option<(u64, u64)>,
) -> Result<crate::types::responses::Response, String> {
    let url = format!("{}/chat/completions", provider.base_url);

    let request = state
        .http_client()
        .post(&url)
        .timeout(provider.timeout)
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("Content-Type", "application/json");
    let started = std::time::Instant::now();
    let response = send_chat_request(request, &chat_req, provider)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    let body = response.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        let snippet = if body.len() > 200 {
            &body[..200]
        } else {
            &body
        };
        tracing::warn!(
            endpoint = %url,
            model = %provider.model,
            upstream_status = status.as_u16(),
            streaming = false,
            elapsed_ms = started.elapsed().as_millis() as u64,
            snippet = %snippet,
            "Upstream request failed"
        );
        return Err(format!("Upstream returned {status}: {snippet}"));
    }

    let chat_resp: ChatCompletionResponse = if provider.rewrite.chat_in.is_empty() {
        serde_json::from_str(&body).map_err(|e| e.to_string())?
    } else {
        let mut chat_response_body: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| e.to_string())?;
        crate::rewrite::apply_rewrite(&mut chat_response_body, &provider.rewrite.chat_in)?;
        serde_json::from_value(chat_response_body).map_err(|e| e.to_string())?
    };

    let mut resp = chat_to_responses(chat_resp, model, state.compact_key());
    super::input_tokens::apply_input_char_scale(resp.usage.as_mut(), truncation_scale);

    // Restore the namespace tag on tools declared inside a Codex `namespace`
    // bundle before custom-tool remapping copies it onto CustomToolCall items.
    crate::convert::apply_tool_namespaces(&mut resp, tool_namespaces);

    // gpt-5.6 code-mode: map function_call output items back to the
    // custom_tool_call shape Codex expects (no-op for models below 5.6).
    crate::convert::remap_custom_tool_calls(&mut resp, custom_names);

    apply_include_filter(&mut resp, &original_req.include);

    for item in &resp.output {
        match item {
            crate::types::item::OutputItem::FunctionCall(fc) => {
                tracing::debug!(
                    name = %fc.name,
                    namespace = ?fc.namespace,
                    call_id = %fc.call_id,
                    "output function_call"
                );
            }
            crate::types::item::OutputItem::CustomToolCall(ct) => {
                tracing::debug!(
                    name = %ct.name,
                    namespace = ?ct.namespace,
                    call_id = %ct.call_id,
                    "output custom_tool_call"
                );
            }
            _ => {}
        }
    }

    Ok(resp)
}

// ── Non-streaming handler ────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn handle_non_streaming(
    state: &crate::app::State,
    provider: &crate::config::ResolvedProvider,
    chat_req: ChatRequest,
    model: String,
    original_req: ResponsesRequest,
    full_input_messages: Vec<crate::types::chat::MessageRequest>,
    response_tools: Vec<crate::types::chat::ToolRequest>,
    custom_names: std::collections::HashSet<String>,
    tool_namespaces: std::collections::HashMap<String, String>,
    truncation_scale: Option<(u64, u64)>,
    ns: &str,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let mut resp = execute_upstream_request(
        state,
        provider,
        chat_req,
        model,
        &original_req,
        &custom_names,
        &tool_namespaces,
        truncation_scale,
    )
    .await
    .map_err(|msg| {
        let err = Error::server_error(msg);
        (StatusCode::BAD_GATEWAY, Json(err.to_http_json()))
    })?;

    // Namespace the response id so its store entry (and the id Codex echoes as
    // previous_response_id) is isolated per instance. No-op when ns is empty.
    resp.id = crate::store::namespaced_id(ns, &resp.id);

    // Persist to store if store=true (default)
    if original_req.store {
        let mut stored_resp = resp.clone();
        if let Some(ref meta) = original_req.metadata {
            stored_resp.metadata = Some(meta.clone());
        }
        stored_resp.previous_response_id = original_req.previous_response_id.clone();
        let mut store_messages = full_input_messages;
        let output_inputs = output_to_input_items(&resp.output);
        store_messages.extend(items_to_chat_messages(&output_inputs, state));
        state
            .store()
            .put_tools(
                resp.id.clone(),
                response_tools,
                custom_names,
                tool_namespaces,
            )
            .await;
        state.store().put(resp.id.clone(), store_messages).await;
    }

    // Echo back request metadata and other params
    if let Some(ref meta) = original_req.metadata {
        resp.metadata = Some(meta.clone());
    }
    resp.parallel_tool_calls = original_req.parallel_tool_calls;

    if provider.rewrite.responses_out.is_empty() {
        return Ok(Json(resp).into_response());
    }

    let mut response_body = serde_json::to_value(&resp).map_err(|e| {
        let err = Error::server_error(e.to_string());
        (StatusCode::INTERNAL_SERVER_ERROR, Json(err.to_http_json()))
    })?;
    crate::rewrite::apply_rewrite(&mut response_body, &provider.rewrite.responses_out).map_err(
        |message| {
            let err = Error::server_error(message);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err.to_http_json()))
        },
    )?;

    Ok(Json(response_body).into_response())
}

// ── Streaming (SSE) handler ──────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn handle_streaming(
    state: &crate::app::State,
    provider: &crate::config::ResolvedProvider,
    chat_req: ChatRequest,
    model: String,
    original_req: ResponsesRequest,
    full_input_messages: Vec<crate::types::chat::MessageRequest>,
    response_tools: Vec<crate::types::chat::ToolRequest>,
    custom_names: std::collections::HashSet<String>,
    tool_namespaces: std::collections::HashMap<String, String>,
    truncation_scale: Option<(u64, u64)>,
    ns: &str,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let url = format!("{}/chat/completions", provider.base_url);

    let request = state
        .http_client()
        .post(&url)
        .timeout(provider.timeout)
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("Content-Type", "application/json");
    let started = std::time::Instant::now();
    let response = send_chat_request(request, &chat_req, provider)
        .map_err(|message| {
            let err = Error::server_error(message);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err.to_http_json()))
        })?
        .send()
        .await
        .map_err(|e| {
            let err = Error::server_error(e.to_string());
            (StatusCode::BAD_GATEWAY, Json(err.to_http_json()))
        })?;

    if !response.status().is_success() {
        let s = response.status();
        let b = response.text().await.unwrap_or_default();
        let truncated = if b.len() > 200 { &b[..200] } else { &b };
        tracing::warn!(
            endpoint = %url,
            model = %provider.model,
            upstream_status = s.as_u16(),
            streaming = true,
            elapsed_ms = started.elapsed().as_millis() as u64,
            snippet = %truncated,
            "Upstream request failed"
        );
        let err = Error::server_error(format!("Upstream returned {}: {}", s.as_u16(), truncated));
        return Err((StatusCode::BAD_GATEWAY, Json(err.to_http_json())));
    }

    // SSE → mpsc channel so we can stream to the client
    let (tx, rx) = mpsc::channel::<Result<SseEvent, std::convert::Infallible>>(64);
    let rid = crate::store::namespaced_id(
        ns,
        &format!("resp_{}", uuid::Uuid::new_v4().to_string().replace('-', "")),
    );
    let mid = format!("msg_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));

    let mut bytes = response.bytes_stream();
    let store_events = original_req.store;
    let store = state.store().clone();
    let chat_in_rewrite = provider.rewrite.chat_in.clone();
    let responses_out_rewrite = provider.rewrite.responses_out.clone();
    let bg_state = state.clone();

    // Register cancellation token so POST /v1/responses/{id}/cancel can stop this stream
    let cancel_rx = store.register_cancel_token(&rid).await;

    // Cache alongside the tools so the next continuation restores it too.
    let store_custom_names = custom_names.clone();
    let store_tool_namespaces = tool_namespaces.clone();

    tokio::spawn(async move {
        let mut buf = String::new();
        let mut ss = StreamState::new(rid.clone(), mid.clone(), model.clone());
        ss.custom_tool_names = custom_names;
        ss.tool_namespaces = tool_namespaces;
        let seq: u64 = 0;
        let mut collected_events: Vec<StreamEvent> = Vec::new();
        let mut cancel_rx = cancel_rx;
        ss.created = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        ss.has_started = true;
        ss.compact_key = bg_state.compact_key().copied();
        ss.truncation_scale = truncation_scale;

        let start_response =
            build_stream_lifecycle_response(&rid, &model, ss.created, ResponseStatus::InProgress);
        let start_events = [
            StreamEvent::Created(crate::types::event::Created {
                response: start_response.clone(),
                sequence_number: 0,
            }),
            StreamEvent::InProgress(crate::types::event::InProgress {
                response: start_response,
                sequence_number: 1,
            }),
        ];
        for event in start_events {
            let prepared = match prepare_stream_event(event, &responses_out_rewrite) {
                Ok(prepared) => prepared,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to prepare SSE lifecycle event");
                    store.unregister_cancel_token(&rid).await;
                    return;
                }
            };
            tracing::debug!("SSE event: {}", prepared.event_type);
            if store_events {
                collected_events.push(prepared.event);
            }
            let sse_event = match SseEvent::default()
                .event(prepared.event_type)
                .json_data(prepared.body)
            {
                Ok(event) => event,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to build SSE lifecycle event");
                    store.unregister_cancel_token(&rid).await;
                    return;
                }
            };
            if tx.send(Ok(sse_event)).await.is_err() {
                tracing::debug!("SSE send failed");
                store.unregister_cancel_token(&rid).await;
                return;
            }
        }

        loop {
            let chunk = tokio::select! {
                _ = cancel_rx.changed() => {
                    tracing::info!(response_id = %rid, "SSE stream cancelled");
                    break;
                }
                chunk = bytes.next() => chunk,
            };

            match chunk {
                Some(Ok(b)) => {
                    buf.push_str(&String::from_utf8_lossy(&b));
                }
                Some(Err(e)) => {
                    tracing::warn!(error = %e, "SSE upstream body read error");
                    break;
                }
                None => {
                    tracing::debug!("SSE upstream body ended");
                    break;
                }
            }

            // Parse SSE events (delimited by \n\n)
            while let Some(pos) = buf.find("\n\n") {
                let event = buf[..pos].trim().to_string();
                buf = buf[pos + 2..].to_string();

                if let Some(data) = event
                    .lines()
                    .find(|l| l.starts_with("data:"))
                    .and_then(|l| l.strip_prefix("data:").map(|s| s.trim()))
                {
                    tracing::trace!(%data, "Chat API delta");

                    match process_upstream_stream_data(
                        &mut ss,
                        data,
                        &chat_in_rewrite,
                        &responses_out_rewrite,
                    ) {
                        Ok(events) => {
                            for prepared in events {
                                if !prepared.event_type.ends_with("delta") {
                                    tracing::debug!("SSE event: {}", prepared.event_type);
                                    tracing::debug!("SSE event details: {}", prepared.body);
                                }
                                if store_events {
                                    collected_events.push(prepared.event);
                                }

                                let sse_event = match SseEvent::default()
                                    .event(prepared.event_type)
                                    .json_data(prepared.body)
                                {
                                    Ok(e) => e,
                                    Err(e) => {
                                        tracing::error!(error = %e, "Failed to build SSE event");
                                        continue;
                                    }
                                };
                                if tx.send(Ok(sse_event)).await.is_err() {
                                    tracing::debug!("SSE send failed");
                                    // Client disconnected — persist partial events and exit
                                    if store_events && !collected_events.is_empty() {
                                        let final_resp = build_response_from_state(&ss);
                                        let mut msgs = full_input_messages.clone();
                                        let out = output_to_input_items(&final_resp.output);
                                        msgs.extend(items_to_chat_messages(&out, &bg_state));
                                        store
                                            .put_tools(
                                                rid.clone(),
                                                response_tools.clone(),
                                                store_custom_names.clone(),
                                                store_tool_namespaces.clone(),
                                            )
                                            .await;
                                        store.put(rid.clone(), msgs).await;
                                    }
                                    store.unregister_cancel_token(&rid).await;
                                    return;
                                }
                            }
                        }
                        Err(message) => {
                            let error_event = StreamEvent::Error(crate::types::event::Error {
                                code: Some(Error::CODE_SERVER_ERROR.into()),
                                message,
                                param: None,
                                sequence_number: seq as i64,
                            });
                            let prepared = match prepare_stream_event(
                                error_event,
                                &responses_out_rewrite,
                            ) {
                                Ok(e) => e,
                                Err(e) => {
                                    tracing::error!(error = %e, "Failed to prepare SSE error event");
                                    store.unregister_cancel_token(&rid).await;
                                    return;
                                }
                            };
                            let sse_event = match SseEvent::default()
                                .event(prepared.event_type)
                                .json_data(prepared.body)
                            {
                                Ok(e) => e,
                                Err(e) => {
                                    tracing::error!(error = %e, "Failed to build SSE error event");
                                    store.unregister_cancel_token(&rid).await;
                                    return;
                                }
                            };
                            let _ = tx.send(Ok(sse_event)).await;
                            store.unregister_cancel_token(&rid).await;
                            return;
                        }
                    }
                }
            }
        }

        // Persist completed response to store
        if store_events {
            let final_resp = build_response_from_state(&ss);
            let output_count = final_resp.output.len();
            let has_reasoning_output = final_resp
                .output
                .iter()
                .any(|o| matches!(o, crate::types::item::OutputItem::Reasoning(_)));
            let mut msgs = full_input_messages;
            let out = output_to_input_items(&final_resp.output);
            let chat_msgs = items_to_chat_messages(&out, &bg_state);
            let has_reasoning_in_stored = chat_msgs.iter().any(|m| match m {
                crate::types::chat::MessageRequest::Assistant(a) => a.reasoning_content.is_some(),
                _ => false,
            });
            tracing::info!(
                output_count,
                has_reasoning_output,
                has_reasoning_in_stored,
                reasoning_open = %ss.reasoning_content,
                completed = ss.completed_items.len(),
                "SSE: persisting response"
            );
            msgs.extend(chat_msgs);
            store
                .put_tools(
                    rid.clone(),
                    response_tools.clone(),
                    store_custom_names.clone(),
                    store_tool_namespaces.clone(),
                )
                .await;
            store.put(rid.clone(), msgs).await;
        }
        store.unregister_cancel_token(&rid).await;
    });

    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default()))
}

// ── Buffered streaming for structured output ─────────────────────────────

/// Streaming path for upstreams that reject `stream: true` combined with a
/// structured `response_format` (see `buffer_structured` at the dispatcher).
/// Fetches the reply non-streamed via `execute_upstream_request`, then replays
/// it to the client as the canonical SSE lifecycle. Store/echo behaviour
/// mirrors `handle_non_streaming`.
#[allow(clippy::too_many_arguments)]
async fn handle_streaming_structured(
    state: &crate::app::State,
    provider: &crate::config::ResolvedProvider,
    mut chat_req: ChatRequest,
    model: String,
    original_req: ResponsesRequest,
    full_input_messages: Vec<crate::types::chat::MessageRequest>,
    response_tools: Vec<crate::types::chat::ToolRequest>,
    custom_names: std::collections::HashSet<String>,
    tool_namespaces: std::collections::HashMap<String, String>,
    truncation_scale: Option<(u64, u64)>,
    ns: &str,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // Fetch non-streamed — this path exists precisely because the upstream
    // rejects `stream: true` with a structured response_format.
    chat_req.stream = Some(false);
    chat_req.stream_options = None;

    let mut resp = execute_upstream_request(
        state,
        provider,
        chat_req,
        model,
        &original_req,
        &custom_names,
        &tool_namespaces,
        truncation_scale,
    )
    .await
    .map_err(|msg| {
        let err = Error::server_error(msg);
        (StatusCode::BAD_GATEWAY, Json(err.to_http_json()))
    })?;

    resp.id = crate::store::namespaced_id(ns, &resp.id);

    // Persist to store if store=true (mirrors handle_non_streaming).
    if original_req.store {
        let mut store_messages = full_input_messages;
        let output_inputs = output_to_input_items(&resp.output);
        store_messages.extend(items_to_chat_messages(&output_inputs, state));
        state
            .store()
            .put_tools(
                resp.id.clone(),
                response_tools,
                custom_names,
                tool_namespaces,
            )
            .await;
        state.store().put(resp.id.clone(), store_messages).await;
    }

    // Echo back request metadata and other params.
    if let Some(ref meta) = original_req.metadata {
        resp.metadata = Some(meta.clone());
    }
    resp.parallel_tool_calls = original_req.parallel_tool_calls;

    // Synthesize the SSE lifecycle from the complete response.
    let responses_out = provider.rewrite.responses_out.clone();
    let events = response_to_stream_events(resp);

    // Size the channel to the event count so the inline sends never block
    // before the receiver stream is returned.
    let (tx, rx) = mpsc::channel::<Result<SseEvent, std::convert::Infallible>>(events.len() + 1);
    for event in events {
        let prepared = crate::types::streaming::prepare_stream_event(event, &responses_out)
            .map_err(|msg| {
                let err = Error::server_error(msg);
                (StatusCode::INTERNAL_SERVER_ERROR, Json(err.to_http_json()))
            })?;
        tracing::debug!("SSE event (structured): {}", prepared.event_type);
        tracing::debug!("SSE event details (structured): {}", prepared.body);
        let sse_event = SseEvent::default()
            .event(prepared.event_type)
            .json_data(prepared.body)
            .map_err(|e| {
                let err = Error::server_error(e.to_string());
                (StatusCode::INTERNAL_SERVER_ERROR, Json(err.to_http_json()))
            })?;
        if tx.send(Ok(sse_event)).await.is_err() {
            tracing::debug!("SSE send failed (structured)");
            break;
        }
    }
    drop(tx);

    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default()))
}

/// Expand a complete `Response` into the canonical SSE lifecycle event
/// sequence, so a non-streamed upstream reply can be replayed as streaming.
/// Shared with the WebSocket handler's buffered-structured path.
pub(crate) fn response_to_stream_events(
    resp: crate::types::responses::Response,
) -> Vec<StreamEvent> {
    use crate::types::event::{
        Completed, ContentPart, ContentPartAdded, ContentPartDone, Created, InProgress,
        OutputItemAdded, OutputItemDone, TextDelta, TextDone,
    };
    use crate::types::item::{OutputContentBlock, OutputItem};

    let lifecycle = crate::types::responses::Response {
        status: ResponseStatus::InProgress,
        output: vec![],
        usage: None,
        ..resp.clone()
    };

    let mut events: Vec<StreamEvent> = Vec::new();
    let mut seq: i64 = 0;
    let mut next = || {
        let s = seq;
        seq += 1;
        s
    };

    events.push(StreamEvent::Created(Created {
        response: lifecycle.clone(),
        sequence_number: next(),
    }));
    events.push(StreamEvent::InProgress(InProgress {
        response: lifecycle,
        sequence_number: next(),
    }));

    for (output_index, item) in resp.output.iter().enumerate() {
        let output_index = output_index as i64;
        events.push(StreamEvent::OutputItemAdded(OutputItemAdded {
            item: item.clone(),
            output_index,
            sequence_number: next(),
        }));

        // For a text message, emit the content-part + text lifecycle so clients
        // reconstructing from deltas still receive the full text.
        if let OutputItem::Message(msg) = item {
            let item_id = msg.id.clone();
            for (content_index, block) in msg.content.iter().enumerate() {
                let content_index = content_index as i64;
                if let OutputContentBlock::Text {
                    text, annotations, ..
                } = block
                {
                    events.push(StreamEvent::ContentPartAdded(ContentPartAdded {
                        content_index,
                        item_id: item_id.clone(),
                        output_index,
                        part: ContentPart::Text {
                            text: String::new(),
                            annotations: vec![],
                        },
                        sequence_number: next(),
                    }));
                    events.push(StreamEvent::TextDelta(TextDelta {
                        content_index,
                        delta: text.clone(),
                        item_id: item_id.clone(),
                        output_index,
                        sequence_number: next(),
                        logprobs: None,
                    }));
                    events.push(StreamEvent::TextDone(TextDone {
                        content_index,
                        item_id: item_id.clone(),
                        output_index,
                        sequence_number: next(),
                        text: text.clone(),
                        logprobs: None,
                    }));
                    events.push(StreamEvent::ContentPartDone(ContentPartDone {
                        content_index,
                        item_id: item_id.clone(),
                        output_index,
                        part: ContentPart::Text {
                            text: text.clone(),
                            annotations: annotations.clone(),
                        },
                        sequence_number: next(),
                    }));
                }
            }
        }

        events.push(StreamEvent::OutputItemDone(OutputItemDone {
            item: item.clone(),
            output_index,
            sequence_number: next(),
        }));
    }

    events.push(StreamEvent::Completed(Completed {
        response: resp,
        sequence_number: next(),
    }));

    events
}

// ── Remote compaction v2 trigger ─────────────────────────────────────────

/// Handle a `/v1/responses` request whose `input` contains a
/// `compaction_trigger`. Builds the summary via the shared compaction helper
/// and returns a single `compaction` output item as either JSON (non-stream)
/// or a three-event SSE stream (`response.created` →
/// `response.output_item.done` → `response.completed`).
async fn handle_compaction_trigger(
    state: &crate::app::State,
    provider: &crate::config::ResolvedProvider,
    req: ResponsesRequest,
    ns: &str,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    // Codex replays the full history in `input` alongside the trigger
    // (store:false), so the request itself is the summary source.
    let current_input: Vec<crate::types::item::InputItem> = req
        .input
        .iter()
        .filter(|i| !matches!(i, crate::types::item::InputItem::CompactionTrigger(_)))
        .cloned()
        .collect();
    let current_messages = items_to_chat_messages(&current_input, state);
    let (output, usage, created_at) = crate::handlers::compact::build_compaction_output(
        state,
        provider,
        req.previous_response_id.as_deref(),
        current_messages,
    )
    .await?;

    let response_id = crate::store::namespaced_id(
        ns,
        &format!("resp_{}", uuid::Uuid::new_v4().to_string().replace('-', "")),
    );
    let mut resp = crate::types::responses::Response {
        id: response_id.clone(),
        model: req.model.clone(),
        status: ResponseStatus::Completed,
        created_at,
        output,
        usage: Some(usage),
        ..Default::default()
    };
    if let Some(ref meta) = req.metadata {
        resp.metadata = Some(meta.clone());
    }
    resp.previous_response_id = req.previous_response_id.clone();
    resp.parallel_tool_calls = req.parallel_tool_calls;

    // Persist the compaction summary so a follow-up `previous_response_id`
    // lookup replays it (clients using server-side state). Codex itself runs
    // store:false and rebuilds history locally, so this only matters for
    // store:true clients.
    if req.store {
        let store_messages = compaction_output_to_chat_messages(&resp.output, state);
        state.store().put(response_id.clone(), store_messages).await;
    }

    if req.stream {
        return stream_compaction_trigger(provider, resp).await;
    }

    if provider.rewrite.responses_out.is_empty() {
        return Ok(Json(resp).into_response());
    }

    let mut body = serde_json::to_value(&resp).map_err(|e| {
        let err = Error::server_error(e.to_string());
        (StatusCode::INTERNAL_SERVER_ERROR, Json(err.to_http_json()))
    })?;
    crate::rewrite::apply_rewrite(&mut body, &provider.rewrite.responses_out).map_err(|msg| {
        let err = Error::server_error(msg);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(err.to_http_json()))
    })?;
    Ok(Json(body).into_response())
}

/// Convert a compaction trigger's output items into the chat messages to store
/// under the response id, so a follow-up `previous_response_id` continuation
/// replays the summary as a leading system message. Mirrors how
/// `InputItem::Compaction` is decrypted in `items_to_chat_messages`.
pub(crate) fn compaction_output_to_chat_messages(
    output: &[crate::types::item::OutputItem],
    state: &crate::app::State,
) -> Vec<crate::types::chat::MessageRequest> {
    use crate::types::chat::{MessageContent, MessageRequest, SystemMessage};
    use crate::types::item::{Compaction, InputItem, OutputContentBlock, OutputItem};

    let mut messages = Vec::new();
    for item in output {
        let OutputItem::Compaction(compaction) = item else {
            continue;
        };
        if compaction.encrypted_content.is_some() {
            // Reuse the input-side decrypt path for parity.
            let input = vec![InputItem::Compaction(Compaction {
                id: compaction.id.clone(),
                encrypted_content: compaction.encrypted_content.clone(),
                status: compaction.status.clone(),
                output: vec![],
                created_by: compaction.created_by.clone(),
            })];
            messages.extend(items_to_chat_messages(&input, state));
        } else {
            // Plain-text summary embedded in nested output message(s).
            let text = compaction
                .output
                .iter()
                .filter_map(|o| match o {
                    OutputItem::Message(m) => Some(
                        m.content
                            .iter()
                            .filter_map(|b| match b {
                                OutputContentBlock::Text { text, .. } => Some(text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n"),
                    ),
                    _ => None,
                })
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if !text.is_empty() {
                messages.push(MessageRequest::System(SystemMessage {
                    content: MessageContent::Text(text),
                    name: None,
                }));
            }
        }
    }
    messages
}

async fn stream_compaction_trigger(
    provider: &crate::config::ResolvedProvider,
    resp: crate::types::responses::Response,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    use crate::types::event::{Completed, Created, InProgress, OutputItemDone};
    let responses_out = provider.rewrite.responses_out.clone();

    // The compaction item lives at output_index 0 — there is only ever one in
    // this response (Codex's collect_compaction_output asserts exactly one).
    let item = resp.output.first().cloned().ok_or_else(|| {
        let err = Error::server_error("compaction helper returned no output");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(err.to_http_json()))
    })?;
    // Lifecycle payload mirrors the normal streaming flow: created/in_progress
    // carry an empty output and `in_progress` status; completed carries the
    // full response.
    let lifecycle = crate::types::responses::Response {
        status: ResponseStatus::InProgress,
        output: vec![],
        usage: None,
        ..resp.clone()
    };
    let events = vec![
        StreamEvent::Created(Created {
            response: lifecycle.clone(),
            sequence_number: 0,
        }),
        StreamEvent::InProgress(InProgress {
            response: lifecycle,
            sequence_number: 1,
        }),
        StreamEvent::OutputItemDone(OutputItemDone {
            item,
            output_index: 0,
            sequence_number: 2,
        }),
        StreamEvent::Completed(Completed {
            response: resp,
            sequence_number: 3,
        }),
    ];

    let (tx, rx) = mpsc::channel::<Result<SseEvent, std::convert::Infallible>>(8);
    for event in events {
        let prepared = crate::types::streaming::prepare_stream_event(event, &responses_out)
            .map_err(|msg| {
                let err = Error::server_error(msg);
                (StatusCode::INTERNAL_SERVER_ERROR, Json(err.to_http_json()))
            })?;
        let sse_event = SseEvent::default()
            .event(prepared.event_type)
            .json_data(prepared.body)
            .map_err(|e| {
                let err = Error::server_error(e.to_string());
                (StatusCode::INTERNAL_SERVER_ERROR, Json(err.to_http_json()))
            })?;
        if tx.send(Ok(sse_event)).await.is_err() {
            break;
        }
    }
    drop(tx);

    Ok(Sse::new(ReceiverStream::new(rx))
        .keep_alive(KeepAlive::default())
        .into_response())
}

fn build_stream_lifecycle_response(
    response_id: &str,
    model: &str,
    created_at: i64,
    status: ResponseStatus,
) -> crate::types::responses::Response {
    crate::types::responses::Response {
        id: response_id.to_string(),
        model: model.to_string(),
        status,
        created_at,
        output: vec![],
        ..Default::default()
    }
}

/// Build a Response from the streaming state for store persistence.
pub(crate) fn build_response_from_state(ss: &StreamState) -> crate::types::responses::Response {
    // Start with items that were closed mid-stream (e.g. reasoning→text transitions)
    let mut output_items: Vec<crate::types::item::OutputItem> = ss.completed_items.clone();

    // Reasoning item — still open if content remains
    if !ss.reasoning_content.is_empty() {
        output_items.push(crate::types::item::OutputItem::Reasoning(
            crate::types::streaming::build_reasoning_item(
                ss.reasoning_id.clone(),
                &ss.reasoning_content,
                ss.compact_key.as_ref(),
            ),
        ));
    }

    // Function call items — open if id is set (not cleared by close)
    for tc in &ss.tool_calls {
        if tc.id.is_empty() {
            continue;
        }
        use crate::types::item::{FunctionCall, OutputItem};
        output_items.push(OutputItem::FunctionCall(FunctionCall {
            call_id: tc.id.clone(),
            name: tc.name.clone(),
            arguments: tc.arguments.clone(),
            id: Some(tc.fc_id.clone()),
            namespace: tc.namespace.clone(),
            status: Some("completed".into()),
        }));
    }

    // Message item — still open if content remains
    if !ss.accumulated_text.is_empty() {
        use crate::types::item::{OutputContentBlock, OutputItem, OutputMessage};
        let content = if ss.has_refusal {
            vec![OutputContentBlock::Refusal {
                refusal: ss.accumulated_text.clone(),
            }]
        } else {
            vec![OutputContentBlock::Text {
                text: ss.accumulated_text.clone(),
                annotations: vec![],
                logprobs: None,
            }]
        };
        output_items.push(OutputItem::Message(OutputMessage {
            id: ss.msg_id.clone(),
            role: "assistant".into(),
            status: "completed".into(),
            content,
            phase: None,
        }));
    }

    let status = if ss.has_refusal {
        ResponseStatus::Incomplete
    } else {
        ResponseStatus::Completed
    };

    let usage = ss
        .usage
        .as_ref()
        .map(|u| crate::types::responses::Usage::from(u.clone()));

    crate::types::responses::Response {
        id: ss.response_id.clone(),
        model: ss.model.clone(),
        status,
        created_at: ss.created,
        output: output_items,
        usage,
        ..Default::default()
    }
}

/// Post-process a Response to trim fields based on the `include` values.
///
/// If `include` is None or empty, no trimming occurs.
/// If `include` does NOT contain `message.output_text.logprobs`, logprobs are
/// stripped from all output text blocks.
fn apply_include_filter(
    resp: &mut crate::types::responses::Response,
    include: &Option<Vec<crate::types::responses::Include>>,
) {
    let Some(includes) = include else { return };
    if includes.is_empty() {
        return;
    }

    let want_logprobs = includes
        .iter()
        .any(|inc| inc.as_ref() == crate::types::responses::Include::MESSAGE_OUTPUT_TEXT_LOGPROBS);

    if !want_logprobs {
        for item in &mut resp.output {
            if let crate::types::item::OutputItem::Message(msg) = item {
                for block in &mut msg.content {
                    if let crate::types::item::OutputContentBlock::Text { logprobs, .. } = block {
                        *logprobs = None;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RewriteConfig, RewriteStep};
    use crate::types::chat::{CompletionTokensDetails, PromptTokensDetails, Usage};
    use crate::types::item::{OutputContentBlock, OutputItem, OutputMessage};
    use crate::types::responses::{Include, Response, ResponseStatus};

    // ── apply_include_filter ────────────────────────────────────────────────

    fn make_text_output_resp() -> Response {
        Response {
            id: "resp_1".into(),
            model: "gpt-5".into(),
            status: ResponseStatus::Completed,
            output: vec![OutputItem::Message(OutputMessage {
                id: "msg_1".into(),
                role: "assistant".into(),
                status: "completed".into(),
                content: vec![OutputContentBlock::Text {
                    text: "hello".into(),
                    annotations: vec![],
                    logprobs: Some(vec![]),
                }],
                phase: None,
            })],
            ..Default::default()
        }
    }

    #[test]
    fn include_filter_none_does_nothing() {
        let mut resp = make_text_output_resp();
        apply_include_filter(&mut resp, &None);
        // logprobs should be preserved
        if let OutputItem::Message(msg) = &resp.output[0]
            && let OutputContentBlock::Text { logprobs, .. } = &msg.content[0]
        {
            assert!(
                logprobs.is_some(),
                "logprobs should be preserved when include is None"
            );
        }
    }

    #[test]
    fn include_filter_empty_does_nothing() {
        let mut resp = make_text_output_resp();
        apply_include_filter(&mut resp, &Some(vec![]));
        if let OutputItem::Message(msg) = &resp.output[0]
            && let OutputContentBlock::Text { logprobs, .. } = &msg.content[0]
        {
            assert!(
                logprobs.is_some(),
                "logprobs should be preserved when include is empty"
            );
        }
    }

    #[test]
    fn include_filter_requested_logprobs_preserves_them() {
        let mut resp = make_text_output_resp();
        apply_include_filter(
            &mut resp,
            &Some(vec![Include(
                Include::MESSAGE_OUTPUT_TEXT_LOGPROBS.to_string(),
            )]),
        );
        if let OutputItem::Message(msg) = &resp.output[0]
            && let OutputContentBlock::Text { logprobs, .. } = &msg.content[0]
        {
            assert!(
                logprobs.is_some(),
                "logprobs should be preserved when requested"
            );
        }
    }

    #[test]
    fn include_filter_without_logprobs_strips_them() {
        let mut resp = make_text_output_resp();
        apply_include_filter(
            &mut resp,
            &Some(vec![Include("web_search_call.results".into())]),
        );
        if let OutputItem::Message(msg) = &resp.output[0]
            && let OutputContentBlock::Text { logprobs, .. } = &msg.content[0]
        {
            assert!(
                logprobs.is_none(),
                "logprobs should be stripped when not requested"
            );
        }
    }

    #[test]
    fn stream_data_applies_chat_in_before_chunk_parse() {
        let mut ss = StreamState::new("resp_s".into(), "msg_s".into(), "gpt-5".into());
        let chat_in = RewriteConfig {
            steps: vec![RewriteStep::Reset(vec![(
                "created".into(),
                serde_json::json!(42),
            )])],
        };
        let events = process_upstream_stream_data(
            &mut ss,
            r#"{"id":"c1","object":"chat.completion.chunk","created":1,"model":"t","choices":[{"index":0,"delta":{"content":"Hello"}}]}"#,
            &chat_in,
            &RewriteConfig::default(),
        )
        .unwrap();

        assert_eq!(ss.created, 42);
        assert_eq!(events.len(), 5);
        assert_eq!(events[0].body["sequence_number"], 3);
        assert_eq!(events[4].body["type"], "response.output_text.delta");
    }

    #[test]
    fn stream_data_applies_response_out_to_event_body_only() {
        let mut ss = StreamState::new("resp_s".into(), "msg_s".into(), "gpt-5".into());
        let responses_out = RewriteConfig {
            steps: vec![RewriteStep::Remove(vec!["response.output".into()])],
        };
        let events = process_upstream_stream_data(
            &mut ss,
            r#"{"id":"c1","object":"chat.completion.chunk","created":1,"model":"t","choices":[{"index":0,"delta":{"content":"Hello"}}]}"#,
            &RewriteConfig::default(),
            &responses_out,
        )
        .unwrap();

        assert!(events[0].body["response"].get("output").is_none());
        if let StreamEvent::Created(created) = &events[0].event {
            assert!(created.response.output.is_empty());
        } else {
            panic!("expected response.created event");
        }
    }

    // ── build_response_from_state ───────────────────────────────────────────

    fn basic_stream_state() -> StreamState {
        let mut ss = StreamState::new("resp_s".into(), "msg_s".into(), "gpt-5".into());
        ss.accumulated_text.push_str("test");
        ss.accumulated_text = "hello world".into();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        ss.created = now;
        ss
    }

    #[test]
    fn build_response_basic_text() {
        let ss = basic_stream_state();
        let resp = build_response_from_state(&ss);
        assert_eq!(resp.id, "resp_s");
        assert_eq!(resp.model, "gpt-5");
        assert_eq!(resp.status, ResponseStatus::Completed);
        assert_eq!(resp.output.len(), 1);
        if let OutputItem::Message(msg) = &resp.output[0] {
            assert_eq!(msg.id, "msg_s");
            assert_eq!(msg.role, "assistant");
            assert_eq!(msg.status, "completed");
            assert_eq!(msg.content.len(), 1);
            if let OutputContentBlock::Text { text, .. } = &msg.content[0] {
                assert_eq!(text, "hello world");
            }
        }
    }

    #[test]
    fn build_response_with_reasoning() {
        use crate::types::streaming::StreamState as Ss;
        let mut ss = Ss::new("resp_r".into(), "msg_r".into(), "gpt-5".into());
        ss.reasoning_content.push_str("test");
        ss.reasoning_id = "rsn_1".into();
        ss.reasoning_content = "I need to think...".into();
        ss.accumulated_text.push_str("test");
        ss.accumulated_text = "answer".into();

        let resp = build_response_from_state(&ss);
        assert_eq!(resp.output.len(), 2);
        // First output should be reasoning
        if let OutputItem::Reasoning(r) = &resp.output[0] {
            assert_eq!(r.id.as_deref(), Some("rsn_1"));
            assert_eq!(r.status.as_deref(), Some("completed"));
            assert!(r.content.is_some());
        } else {
            panic!("expected reasoning item, got {:?}", resp.output[0]);
        }
        // Second should be message
        if let OutputItem::Message(msg) = &resp.output[1]
            && let OutputContentBlock::Text { text, .. } = &msg.content[0]
        {
            assert_eq!(text, "answer");
        }
    }

    #[test]
    fn build_response_with_refusal() {
        let mut ss = StreamState::new("resp_ref".into(), "msg_ref".into(), "gpt-5".into());
        ss.accumulated_text.push_str("test");
        ss.has_refusal = true;
        ss.accumulated_text = "I cannot answer that.".into();

        let resp = build_response_from_state(&ss);
        assert_eq!(resp.status, ResponseStatus::Incomplete);
        if let OutputItem::Message(msg) = &resp.output[0]
            && let OutputContentBlock::Refusal { refusal } = &msg.content[0]
        {
            assert_eq!(refusal, "I cannot answer that.");
        }
    }

    #[test]
    fn build_response_with_function_calls() {
        let mut ss = StreamState::new("resp_fc".into(), "msg_fc".into(), "gpt-5".into());
        ss.tool_calls = vec![
            crate::types::streaming::ToolCallAccumulator {
                id: "call_1".into(),
                name: "get_weather".into(),
                arguments: r#"{"city":"NYC"}"#.into(),
                fc_id: "fc_1".into(),
                index: 0,
                output_index: 0,
                ..Default::default()
            },
            crate::types::streaming::ToolCallAccumulator {
                id: String::new(), // skipped — empty id
                name: "ignored".into(),
                arguments: String::new(),
                fc_id: String::new(),
                index: 1,
                output_index: 1,
                ..Default::default()
            },
        ];

        let resp = build_response_from_state(&ss);
        // Only the function call (no text content was added)
        assert_eq!(resp.output.len(), 1);
        if let OutputItem::FunctionCall(fc) = &resp.output[0] {
            assert_eq!(fc.name, "get_weather");
            assert_eq!(fc.arguments, r#"{"city":"NYC"}"#);
        } else {
            panic!("expected function call");
        }
        // Empty-id tool call should be skipped
        let fc_count = resp
            .output
            .iter()
            .filter(|o| matches!(o, OutputItem::FunctionCall(_)))
            .count();
        assert_eq!(fc_count, 1);
    }

    #[test]
    fn build_response_with_usage() {
        let mut ss = basic_stream_state();
        ss.usage = Some(Usage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            prompt_cache_hit_tokens: None,
            prompt_cache_miss_tokens: None,
            prompt_tokens_details: Some(PromptTokensDetails {
                cached_tokens: 20,
                audio_tokens: 0,
            }),
            completion_tokens_details: Some(CompletionTokensDetails {
                reasoning_tokens: 30,
                audio_tokens: 0,
                accepted_prediction_tokens: 0,
                rejected_prediction_tokens: 0,
            }),
        });

        let resp = build_response_from_state(&ss);
        let u = resp.usage.unwrap();
        assert_eq!(u.input_tokens, 100);
        assert_eq!(u.output_tokens, 50);
        assert_eq!(u.total_tokens, 150);
        assert_eq!(u.input_tokens_details.cached_tokens, 20);
        assert_eq!(u.output_tokens_details.reasoning_tokens, 30);
    }

    #[test]
    fn build_response_empty_state() {
        let ss = StreamState::new("resp_empty".into(), "msg_e".into(), "gpt-5".into());
        let resp = build_response_from_state(&ss);
        assert_eq!(resp.id, "resp_empty");
        assert_eq!(resp.output.len(), 0);
        assert!(resp.usage.is_none());
    }

    // ── compaction_trigger detection ────────────────────────────────────────

    #[test]
    fn detects_compaction_trigger_in_input() {
        use crate::types::item::{CompactionTrigger, InputItem};
        let input = [InputItem::CompactionTrigger(CompactionTrigger::default())];
        assert!(
            input
                .iter()
                .any(|i| matches!(i, InputItem::CompactionTrigger(_)))
        );
    }

    #[test]
    fn plain_input_is_not_a_compaction_trigger() {
        let json = serde_json::json!([
            { "type": "message", "role": "user", "content": "hi" }
        ]);
        let input: Vec<crate::types::item::InputItem> = serde_json::from_value(json).unwrap();
        assert!(
            !input
                .iter()
                .any(|i| matches!(i, crate::types::item::InputItem::CompactionTrigger(_)))
        );
    }
}
