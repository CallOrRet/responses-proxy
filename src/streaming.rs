#![allow(dead_code)]
//! Streaming conversion: Chat API SSE chunks → Responses API SSE events.
//!
//! Handles incremental text deltas and tool call deltas, accumulating state
//! across chunks and emitting properly-typed Responses API streaming events.

use serde_json::{Value, json};

/// Accumulated state across a streaming response.
#[derive(Debug, Default)]
pub struct StreamState {
    pub response_id: String,
    pub msg_id: String,
    pub model: String,
    pub accumulated_text: String,
    /// Accumulated reasoning/thinking content from delta.reasoning_content.
    pub reasoning_content: String,
    /// Accumulated tool calls keyed by index.
    pub tool_calls: Vec<ToolCallAccumulator>,
    pub has_started: bool,
    pub created: u64,
    /// Whether we've emitted output_item.added for the message.
    pub message_item_added: bool,
    /// The output_index of the message item (set when output_item.added is emitted).
    pub msg_output_index: usize,
    /// Whether we've emitted output_item.added for reasoning.
    pub reasoning_item_added: bool,
    pub reasoning_id: String,
    /// Monotonically increasing counter for assigning output_index to new items.
    next_output_index: usize,
    /// Usage data extracted from the final stream chunk (if provider sends it).
    pub usage: Option<Value>,
}

#[derive(Debug, Default, Clone)]
pub struct ToolCallAccumulator {
    pub id: String,
    pub name: String,
    pub arguments: String,
    pub index: u32,
    /// Whether we've emitted output_item.added for this tool call.
    pub item_added: bool,
    /// Generated output item ID.
    pub fc_id: String,
    /// The output_index assigned when this item was first added.
    pub output_index: usize,
}

impl StreamState {
    pub fn new(response_id: String, msg_id: String, model: String) -> Self {
        Self {
            response_id: response_id.clone(),
            msg_id,
            model,
            reasoning_id: format!("rs_{}", uuid::Uuid::new_v4().to_string().replace('-', "")),
            ..Default::default()
        }
    }
}

/// Events emitted by the streaming converter.
#[derive(Debug)]
pub enum StreamEvent {
    Created(Value),
    InProgress(Value),
    OutputItemAdded(Value),
    ContentPartAdded(Value),
    OutputTextDelta(Value),
    ReasoningTextDelta(Value),
    OutputTextDone(Value),
    ReasoningTextDone(Value),
    FunctionCallArgumentsDelta(Value),
    OutputItemDone(Value),
    ContentPartDone(Value),
    FunctionCallArgumentsDone(Value),
    Completed(Value),
    Failed(Value),
}

impl StreamEvent {
    pub fn to_sse_json(&self) -> Value {
        match self {
            StreamEvent::Created(v)
            | StreamEvent::InProgress(v)
            | StreamEvent::OutputItemAdded(v)
            | StreamEvent::ContentPartAdded(v)
            | StreamEvent::OutputTextDelta(v)
            | StreamEvent::ReasoningTextDelta(v)
            | StreamEvent::OutputTextDone(v)
            | StreamEvent::ReasoningTextDone(v)
            | StreamEvent::FunctionCallArgumentsDelta(v)
            | StreamEvent::OutputItemDone(v)
            | StreamEvent::ContentPartDone(v)
            | StreamEvent::FunctionCallArgumentsDone(v)
            | StreamEvent::Completed(v)
            | StreamEvent::Failed(v) => v.clone(),
        }
    }

    pub fn event_type(&self) -> &str {
        match self {
            StreamEvent::Created(_) => "response.created",
            StreamEvent::InProgress(_) => "response.in_progress",
            StreamEvent::OutputItemAdded(_) => "response.output_item.added",
            StreamEvent::ContentPartAdded(_) => "response.content_part.added",
            StreamEvent::OutputTextDelta(_) => "response.output_text.delta",
            StreamEvent::ReasoningTextDelta(_) => "response.reasoning_text.delta",
            StreamEvent::OutputTextDone(_) => "response.output_text.done",
            StreamEvent::ReasoningTextDone(_) => "response.reasoning_text.done",
            StreamEvent::FunctionCallArgumentsDelta(_) => "response.function_call_arguments.delta",
            StreamEvent::OutputItemDone(_) => "response.output_item.done",
            StreamEvent::ContentPartDone(_) => "response.content_part.done",
            StreamEvent::FunctionCallArgumentsDone(_) => "response.function_call_arguments.done",
            StreamEvent::Completed(_) => "response.completed",
            StreamEvent::Failed(_) => "response.failed",
        }
    }
}

/// Process a Chat API SSE data line and return any Responses API streaming events.
///
/// Returns `None` if the SSE data is not relevant (e.g., role-only delta, empty chunk).
pub fn process_chunk(state: &mut StreamState, data: &str) -> Option<Vec<StreamEvent>> {
    if data == "[DONE]" {
        return Some(build_completion_events(state));
    }

    let chunk: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(_) => return None,
    };

    // Update created timestamp from first chunk that has it
    if let Some(created) = chunk["created"].as_u64() {
        state.created = created;
    }

    // Capture usage from empty-choices chunk (DeepSeek sends this before [DONE])
    if chunk["choices"].as_array().map_or(false, |c| c.is_empty()) {
        if let Some(usage) = chunk.get("usage") {
            state.usage = Some(usage.clone());
        }
        return None;
    }

    let mut events = Vec::new();
    let mut has_content = false;

    // Process choices. Each choice has its own index (relevant when n > 1).
    // For DeepSeek (n=1), choice.index is always 0.
    if let Some(choices) = chunk["choices"].as_array() {
        for choice in choices {
            let _choice_index = choice.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
            let delta = match choice.get("delta") {
                Some(d) => d,
                None => continue,
            };

            // Reasoning content delta (DeepSeek thinking mode CoT)
            if let Some(reasoning) = delta["reasoning_content"].as_str()
                && !reasoning.is_empty()
            {
                has_content = true;
                if !state.reasoning_item_added {
                    let idx = state.next_output_index;
                    state.next_output_index += 1;
                    events.push(StreamEvent::OutputItemAdded(json!({
                        "type": "response.output_item.added",
                        "output_index": idx,
                        "item": {
                            "type": "reasoning",
                            "id": state.reasoning_id,
                            "status": "in_progress",
                            "summary": [],
                            "content": []
                        }
                    })));
                    events.push(StreamEvent::ContentPartAdded(json!({
                        "type": "response.content_part.added",
                        "item_id": state.reasoning_id,
                        "output_index": idx,
                        "content_index": 0,
                        "part": {
                            "type": "reasoning_text",
                            "text": ""
                        }
                    })));
                    state.reasoning_item_added = true;
                }
                state.reasoning_content.push_str(reasoning);
                events.push(StreamEvent::ReasoningTextDelta(json!({
                    "type": "response.reasoning_text.delta",
                    "item_id": state.reasoning_id,
                    "output_index": 0,
                    "content_index": 0,
                    "delta": reasoning
                })));
            }

            // Text content delta
            if let Some(content) = delta["content"].as_str()
                && !content.is_empty()
            {
                has_content = true;
                if !state.message_item_added {
                    let idx = state.next_output_index;
                    state.next_output_index += 1;
                    state.msg_output_index = idx;
                    events.push(StreamEvent::OutputItemAdded(json!({
                        "type": "response.output_item.added",
                        "output_index": idx,
                        "item": {
                            "type": "message",
                            "id": state.msg_id,
                            "role": "assistant",
                            "status": "in_progress",
                            "content": []
                        }
                    })));
                    events.push(StreamEvent::ContentPartAdded(json!({
                        "type": "response.content_part.added",
                        "item_id": state.msg_id,
                        "output_index": idx,
                        "content_index": 0,
                        "part": {
                            "type": "output_text",
                            "text": "",
                            "annotations": []
                        }
                    })));
                    state.message_item_added = true;
                }
                state.accumulated_text.push_str(content);
                events.push(StreamEvent::OutputTextDelta(json!({
                    "type": "response.output_text.delta",
                    "item_id": state.msg_id,
                    "output_index": state.msg_output_index,
                    "content_index": 0,
                    "delta": content
                })));
            }

            // Tool call deltas (arrive incrementally in Chat API streaming)
            if let Some(tool_calls) = delta["tool_calls"].as_array() {
                for tc in tool_calls {
                    has_content = true;
                    let index = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                    // Ensure accumulator exists for this index
                    while state.tool_calls.len() <= index as usize {
                        state.tool_calls.push(ToolCallAccumulator::default());
                    }
                    let acc = &mut state.tool_calls[index as usize];
                    acc.index = index;

                    if let Some(id) = tc["id"].as_str() {
                        acc.id = id.to_string();
                    }

                    // Generate fc_id on first encounter
                    if acc.fc_id.is_empty() {
                        acc.fc_id =
                            format!("fc_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
                    }

                    // Emit output_item.added on first encounter
                    if !acc.item_added && !acc.id.is_empty() {
                        let idx = state.next_output_index;
                        state.next_output_index += 1;
                        acc.output_index = idx;
                        events.push(StreamEvent::OutputItemAdded(json!({
                            "type": "response.output_item.added",
                            "output_index": idx,
                            "item": {
                                "type": "function_call",
                                "id": acc.fc_id,
                                "call_id": acc.id,
                                "name": "",
                                "arguments": "",
                                "status": "in_progress"
                            }
                        })));
                        acc.item_added = true;
                    }

                    if let Some(func) = tc.get("function") {
                        if let Some(name) = func["name"].as_str() {
                            acc.name.push_str(name);
                        }
                        if let Some(args) = func["arguments"].as_str() {
                            acc.arguments.push_str(args);
                            if acc.item_added {
                                events.push(StreamEvent::FunctionCallArgumentsDelta(json!({
                                    "type": "response.function_call_arguments.delta",
                                    "item_id": acc.fc_id,
                                    "output_index": acc.output_index,
                                    "delta": args
                                })));
                            }
                        }
                    }
                }
            }
        }
    }

    // Emit created + in_progress on first content-bearing chunk
    if has_content && !state.has_started {
        events.insert(
            0,
            StreamEvent::Created(json!({
                "type": "response.created",
                "response": {
                    "id": state.response_id,
                    "object": "response",
                    "created_at": state.created,
                    "model": state.model,
                    "status": "in_progress",
                    "output": []
                }
            })),
        );
        events.insert(
            1,
            StreamEvent::InProgress(json!({
                "type": "response.in_progress",
                "response": {
                    "id": state.response_id,
                    "object": "response",
                    "created_at": state.created,
                    "model": state.model,
                    "status": "in_progress",
                    "output": []
                }
            })),
        );
        state.has_started = true;
    }

    if events.is_empty() {
        None
    } else {
        Some(events)
    }
}

/// Build the final events when the stream ends:
/// content_part.done + output_item.done for each item, then response.completed.
fn build_completion_events(state: &StreamState) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    let mut output_items: Vec<Value> = Vec::new();

    // Close reasoning item if we started one
    if state.reasoning_item_added {
        events.push(StreamEvent::ReasoningTextDone(json!({
            "type": "response.reasoning_text.done",
            "item_id": state.reasoning_id,
            "output_index": 0,
            "content_index": 0,
            "text": state.reasoning_content
        })));
        events.push(StreamEvent::ContentPartDone(json!({
            "type": "response.content_part.done",
            "item_id": state.reasoning_id,
            "output_index": 0,
            "content_index": 0,
            "part": {
                "type": "reasoning_text",
                "text": state.reasoning_content
            }
        })));
        events.push(StreamEvent::OutputItemDone(json!({
            "type": "response.output_item.done",
            "output_index": 0,
            "item": {
                "type": "reasoning",
                "id": state.reasoning_id,
                "status": "completed",
                "summary": [],
                "content": [{
                    "type": "reasoning_text",
                    "text": state.reasoning_content
                }]
            }
        })));

        output_items.push(json!({
            "type": "reasoning",
            "id": state.reasoning_id,
            "status": "completed",
            "summary": [],
            "content": [{
                "type": "reasoning_text",
                "text": state.reasoning_content
            }]
        }));
    }

    // Close function call items (before message, matching output_index order)
    for tc in &state.tool_calls {
        if !tc.id.is_empty() {
            let fc_id = if tc.fc_id.is_empty() {
                format!("fc_{}", uuid::Uuid::new_v4().to_string().replace('-', ""))
            } else {
                tc.fc_id.clone()
            };
            if tc.item_added {
                events.push(StreamEvent::FunctionCallArgumentsDone(json!({
                    "type": "response.function_call_arguments.done",
                    "item_id": fc_id,
                    "output_index": tc.output_index,
                    "arguments": tc.arguments,
                    "name": tc.name
                })));
                events.push(StreamEvent::OutputItemDone(json!({
                    "type": "response.output_item.done",
                    "output_index": tc.output_index,
                    "item": {
                        "type": "function_call",
                        "id": fc_id,
                        "call_id": tc.id,
                        "name": tc.name,
                        "arguments": tc.arguments,
                        "status": "completed"
                    }
                })));
            }
            output_items.push(json!({
                "type": "function_call",
                "id": fc_id,
                "call_id": tc.id,
                "name": tc.name,
                "arguments": tc.arguments,
                "status": "completed"
            }));
        }
    }

    // Close message item
    if state.message_item_added {
        events.push(StreamEvent::OutputTextDone(json!({
            "type": "response.output_text.done",
            "item_id": state.msg_id,
            "output_index": state.msg_output_index,
            "content_index": 0,
            "text": state.accumulated_text
        })));
        events.push(StreamEvent::ContentPartDone(json!({
            "type": "response.content_part.done",
            "item_id": state.msg_id,
            "output_index": state.msg_output_index,
            "content_index": 0,
            "part": {
                "type": "output_text",
                "text": state.accumulated_text,
                "annotations": []
            }
        })));
        events.push(StreamEvent::OutputItemDone(json!({
            "type": "response.output_item.done",
            "output_index": state.msg_output_index,
            "item": {
                "type": "message",
                "id": state.msg_id,
                "role": "assistant",
                "status": "completed",
                "content": [{
                    "type": "output_text",
                    "text": state.accumulated_text,
                    "annotations": []
                }]
            }
        })));

        output_items.push(json!({
            "type": "message",
            "id": state.msg_id,
            "role": "assistant",
            "status": "completed",
            "content": [{
                "type": "output_text",
                "text": state.accumulated_text,
                "annotations": []
            }]
        }));
    } else if !state.accumulated_text.is_empty() || state.tool_calls.is_empty() {
        output_items.push(json!({
            "type": "message",
            "id": state.msg_id,
            "role": "assistant",
            "status": "completed",
            "content": [{
                "type": "output_text",
                "text": state.accumulated_text,
                "annotations": []
            }]
        }));
    }

    let mut completed_resp = json!({
        "id": state.response_id,
        "object": "response",
        "model": state.model,
        "status": "completed",
        "output": output_items
    });
    if let Some(ref usage) = state.usage {
        completed_resp["usage"] = usage.clone();
    }

    events.push(StreamEvent::Completed(json!({
        "type": "response.completed",
        "response": completed_resp
    })));

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> StreamState {
        StreamState::new(
            "resp_test".into(),
            "msg_test".into(),
            "deepseek-v4-pro".into(),
        )
    }

    /// Find the Completed event in a vec of events from [DONE] processing.
    fn find_completed(events: &[StreamEvent]) -> &StreamEvent {
        events
            .iter()
            .find(|e| matches!(e, StreamEvent::Completed(_)))
            .expect("Expected a Completed event")
    }

    #[test]
    fn test_single_text_chunk() {
        let mut state = make_state();
        let data = r#"{"id":"chatcmpl-1","object":"chat.completion.chunk","created":1715550000,"model":"deepseek-v4-pro","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;

        let events = process_chunk(&mut state, data).unwrap();
        assert!(events.iter().any(|e| matches!(e, StreamEvent::Created(_))));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::OutputTextDelta(_)))
        );
        assert_eq!(state.accumulated_text, "Hello");
    }

    #[test]
    fn test_multiple_text_chunks_accumulate() {
        let mut state = make_state();

        process_chunk(&mut state, r#"{"choices":[{"delta":{"content":"Hello"}}]}"#);
        process_chunk(
            &mut state,
            r#"{"choices":[{"delta":{"content":" World"}}]}"#,
        );
        process_chunk(&mut state, r#"{"choices":[{"delta":{"content":"!"}}]}"#);

        assert_eq!(state.accumulated_text, "Hello World!");
    }

    #[test]
    fn test_created_event_only_once() {
        let mut state = make_state();

        let events1 =
            process_chunk(&mut state, r#"{"choices":[{"delta":{"content":"A"}}]}"#).unwrap();
        let has_created1 = events1.iter().any(|e| matches!(e, StreamEvent::Created(_)));
        assert!(has_created1);

        let events2 =
            process_chunk(&mut state, r#"{"choices":[{"delta":{"content":"B"}}]}"#).unwrap();
        let has_created2 = events2.iter().any(|e| matches!(e, StreamEvent::Created(_)));
        assert!(!has_created2, "Created event should only be emitted once");
    }

    #[test]
    fn test_done_produces_completed_event() {
        let mut state = make_state();
        state.accumulated_text = "Full response".into();

        let events = process_chunk(&mut state, "[DONE]").unwrap();
        match find_completed(&events) {
            StreamEvent::Completed(v) => {
                assert_eq!(v["type"], "response.completed");
                let output = v["response"]["output"].as_array().unwrap();
                assert_eq!(output[0]["content"][0]["text"], "Full response");
                assert_eq!(output[0]["type"], "message");
            }
            other => panic!("Expected Completed, got {:?}", other),
        }
    }

    #[test]
    fn test_tool_call_delta_accumulation() {
        let mut state = make_state();

        // First chunk: id and partial name/arguments
        process_chunk(
            &mut state,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"get_we","arguments":"{\"city\":"}}]}}]}"#,
        );
        assert_eq!(state.tool_calls.len(), 1);
        assert_eq!(state.tool_calls[0].id, "call_abc");
        assert_eq!(state.tool_calls[0].name, "get_we");
        assert!(state.tool_calls[0].arguments.starts_with(r#"{"city":"#));

        // Second chunk: more arguments
        process_chunk(
            &mut state,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"NYC\"}"}}]}}]}"#,
        );
        assert_eq!(state.tool_calls[0].name, "get_we"); // name unchanged
        assert!(state.tool_calls[0].arguments.contains("NYC"));

        // Done
        let events = process_chunk(&mut state, "[DONE]").unwrap();
        match find_completed(&events) {
            StreamEvent::Completed(v) => {
                let output = v["response"]["output"].as_array().unwrap();
                // Should have a function_call item
                let fc = output
                    .iter()
                    .find(|o| o["type"] == "function_call")
                    .unwrap();
                assert_eq!(fc["call_id"], "call_abc");
                assert_eq!(fc["name"], "get_we");
                assert!(fc["arguments"].as_str().unwrap().contains("NYC"));
            }
            other => panic!("Expected Completed, got {:?}", other),
        }
    }

    #[test]
    fn test_multiple_tool_calls_in_streaming() {
        let mut state = make_state();

        // Tool call 0
        process_chunk(
            &mut state,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"get_weather","arguments":"{\"city\":\"NYC\"}"}}]}}]}"#,
        );

        // Tool call 1 (in same or subsequent chunk)
        process_chunk(
            &mut state,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":1,"id":"call_2","type":"function","function":{"name":"get_time","arguments":"{\"tz\":\"EST\"}"}}]}}]}"#,
        );

        assert_eq!(state.tool_calls.len(), 2);
        assert_eq!(state.tool_calls[0].name, "get_weather");
        assert_eq!(state.tool_calls[1].name, "get_time");

        let events = process_chunk(&mut state, "[DONE]").unwrap();
        match find_completed(&events) {
            StreamEvent::Completed(v) => {
                let output = v["response"]["output"].as_array().unwrap();
                let fc_names: Vec<&str> = output
                    .iter()
                    .filter(|o| o["type"] == "function_call")
                    .map(|o| o["name"].as_str().unwrap())
                    .collect();
                assert_eq!(fc_names, vec!["get_weather", "get_time"]);
            }
            other => panic!("Expected Completed, got {:?}", other),
        }
    }

    #[test]
    fn test_text_and_tool_calls_together() {
        let mut state = make_state();

        // Text + tool call in same chunk
        process_chunk(
            &mut state,
            r#"{"choices":[{"delta":{"content":"Let me check.","tool_calls":[{"index":0,"id":"call_x","type":"function","function":{"name":"search","arguments":"{}"}}]}}]}"#,
        );

        assert_eq!(state.accumulated_text, "Let me check.");
        assert_eq!(state.tool_calls[0].name, "search");

        let events = process_chunk(&mut state, "[DONE]").unwrap();
        match find_completed(&events) {
            StreamEvent::Completed(v) => {
                let output = v["response"]["output"].as_array().unwrap();
                let msg = output.iter().find(|o| o["type"] == "message").unwrap();
                assert_eq!(msg["content"][0]["text"], "Let me check.");
                let fc = output
                    .iter()
                    .find(|o| o["type"] == "function_call")
                    .unwrap();
                assert_eq!(fc["name"], "search");
            }
            other => panic!("Expected Completed, got {:?}", other),
        }
    }

    #[test]
    fn test_output_index_no_duplicates_message_then_tool() {
        let mut state = make_state();

        // Text chunk arrives first
        let events1 = process_chunk(
            &mut state,
            r#"{"choices":[{"delta":{"content":"Let me check."}}]}"#,
        )
        .unwrap();
        let text_added = events1
            .iter()
            .find(|e| matches!(e, StreamEvent::OutputItemAdded(_)))
            .unwrap();
        let msg_idx = match text_added {
            StreamEvent::OutputItemAdded(v) => v["output_index"].as_u64().unwrap(),
            _ => panic!(),
        };

        // Tool call arrives in next chunk
        let events2 = process_chunk(
            &mut state,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_x","type":"function","function":{"name":"search","arguments":"{}"}}]}}]}"#,
        )
        .unwrap();
        let tc_added = events2
            .iter()
            .find(|e| matches!(e, StreamEvent::OutputItemAdded(_)))
            .unwrap();
        let tc_idx = match tc_added {
            StreamEvent::OutputItemAdded(v) => v["output_index"].as_u64().unwrap(),
            _ => panic!(),
        };

        assert_ne!(
            msg_idx, tc_idx,
            "message and tool_call must have different output_index"
        );
        assert_eq!(tc_idx, msg_idx + 1, "tool_call should come after message");
    }

    #[test]
    fn test_output_index_no_duplicates_reasoning_text_tool() {
        let mut state = make_state();

        process_chunk(
            &mut state,
            r#"{"choices":[{"delta":{"reasoning_content":"Let me think"}}]}"#,
        );
        process_chunk(
            &mut state,
            r#"{"choices":[{"delta":{"content":"Answer","tool_calls":[{"index":0,"id":"call_x","type":"function","function":{"name":"search","arguments":"{}"}}]}}]}"#,
        );

        assert!(state.reasoning_item_added);
        assert!(state.message_item_added);
        assert_eq!(state.tool_calls[0].output_index, 2);
        assert!(state.msg_output_index < state.tool_calls[0].output_index);
    }

    #[test]
    fn test_role_only_delta_produces_no_events() {
        let mut state = make_state();

        // Role-only delta (first chunk in some Chat API implementations)
        let events = process_chunk(
            &mut state,
            r#"{"choices":[{"delta":{"role":"assistant"},"finish_reason":null}]}"#,
        );

        // No text, no tool calls → no events, but state is still uninitialized
        // The created event hasn't been emitted yet since there's no content
        assert!(events.is_none());
        assert!(!state.has_started);
    }

    #[test]
    fn test_empty_choices_produces_no_events() {
        let mut state = make_state();
        let events = process_chunk(&mut state, r#"{"choices":[]}"#);
        assert!(events.is_none());
    }

    #[test]
    fn test_invalid_json_produces_no_events() {
        let mut state = make_state();
        let events = process_chunk(&mut state, "not valid json {{{");
        assert!(events.is_none());
    }

    #[test]
    fn test_sse_event_split_across_chunks_reassembly() {
        // Simulate how the main loop reassembles split SSE events.
        // This tests the buffer logic pattern.
        let mut state = make_state();
        let mut buffer = String::new();

        // Simulate a chunk that contains half an SSE event
        let chunk1 = r#"data: {"choices":[{"del"#;
        buffer.push_str(chunk1);
        // No complete event yet
        assert!(buffer.find("\n\n").is_none());

        // Second chunk completes the event
        let chunk2 = r#"ta":{"content":"Hello"}}]}

data: [DONE]

"#;
        buffer.push_str(chunk2);

        // Now extract complete events
        let mut final_events = Vec::new();
        while let Some(event_end) = buffer.find("\n\n") {
            let event_str = buffer[..event_end].trim().to_string();
            buffer = buffer[event_end + 2..].to_string();

            let data_line = event_str
                .lines()
                .find(|l| l.starts_with("data:"))
                .and_then(|l| l.strip_prefix("data:").map(|s| s.trim()));

            if let Some(data) = data_line
                && let Some(events) = process_chunk(&mut state, data)
            {
                final_events.extend(events);
            }
        }

        assert_eq!(state.accumulated_text, "Hello");
        // Should have created + text_delta + completed events
        assert!(
            final_events
                .iter()
                .any(|e| matches!(e, StreamEvent::Created(_)))
        );
        assert!(
            final_events
                .iter()
                .any(|e| matches!(e, StreamEvent::OutputTextDelta(_)))
        );
        assert!(
            final_events
                .iter()
                .any(|e| matches!(e, StreamEvent::Completed(_)))
        );
    }

    #[test]
    fn test_usage_chunk_captured() {
        let mut state = make_state();
        state.accumulated_text = "Answer".into();

        // DeepSeek sends usage chunk before [DONE]
        let events = process_chunk(
            &mut state,
            r#"{"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15,"completion_tokens_details":{"reasoning_tokens":3}}}"#,
        );
        // Usage-only chunk should not emit events
        assert!(events.is_none());
        // But usage should be captured
        assert!(state.usage.is_some());
        assert_eq!(state.usage.as_ref().unwrap()["prompt_tokens"], 10);

        // [DONE] should include usage in completed
        let events = process_chunk(&mut state, "[DONE]").unwrap();
        match find_completed(&events) {
            StreamEvent::Completed(v) => {
                let usage = &v["response"]["usage"];
                assert_eq!(usage["prompt_tokens"], 10);
                assert_eq!(usage["completion_tokens"], 5);
                assert_eq!(usage["total_tokens"], 15);
            }
            other => panic!("Expected Completed, got {:?}", other),
        }
    }

    #[test]
    fn test_content_and_finish_reason_in_same_chunk() {
        let mut state = make_state();

        // Last content chunk often has finish_reason
        process_chunk(
            &mut state,
            r#"{"choices":[{"delta":{"content":"End"},"finish_reason":"stop"}]}"#,
        );

        assert_eq!(state.accumulated_text, "End");

        let events = process_chunk(&mut state, "[DONE]").unwrap();
        match find_completed(&events) {
            StreamEvent::Completed(v) => {
                assert_eq!(v["response"]["status"], "completed");
            }
            other => panic!("Expected Completed, got {:?}", other),
        }
    }

    #[test]
    fn test_reasoning_content_accumulates_and_appears_in_completed() {
        let mut state = make_state();

        // Simulate thinking mode streaming: reasoning_content comes before content
        process_chunk(
            &mut state,
            r#"{"choices":[{"delta":{"reasoning_content":"Let me"}}]}"#,
        );
        process_chunk(
            &mut state,
            r#"{"choices":[{"delta":{"reasoning_content":" think about this."}}]}"#,
        );
        process_chunk(
            &mut state,
            r#"{"choices":[{"delta":{"content":"The answer is 42."}}]}"#,
        );

        assert_eq!(state.reasoning_content, "Let me think about this.");
        assert_eq!(state.accumulated_text, "The answer is 42.");

        let events = process_chunk(&mut state, "[DONE]").unwrap();
        match find_completed(&events) {
            StreamEvent::Completed(v) => {
                let output = v["response"]["output"].as_array().unwrap();
                // First output item should be reasoning
                let reasoning = output.iter().find(|o| o["type"] == "reasoning").unwrap();
                assert_eq!(reasoning["content"][0]["type"], "reasoning_text");
                assert_eq!(reasoning["content"][0]["text"], "Let me think about this.");
                // Second output item should be the message
                let msg = output.iter().find(|o| o["type"] == "message").unwrap();
                assert_eq!(msg["content"][0]["text"], "The answer is 42.");
            }
            other => panic!("Expected Completed, got {:?}", other),
        }
    }

    #[test]
    fn test_no_reasoning_when_not_present() {
        let mut state = make_state();
        state.accumulated_text = "Plain answer without thinking.".into();

        let events = process_chunk(&mut state, "[DONE]").unwrap();
        match find_completed(&events) {
            StreamEvent::Completed(v) => {
                let output = v["response"]["output"].as_array().unwrap();
                // Should NOT have a reasoning item
                assert!(
                    !output.iter().any(|o| o["type"] == "reasoning"),
                    "No reasoning item when reasoning_content is empty"
                );
            }
            other => panic!("Expected Completed, got {:?}", other),
        }
    }

    #[test]
    fn test_in_progress_emitted_after_created() {
        let mut state = make_state();

        let events =
            process_chunk(&mut state, r#"{"choices":[{"delta":{"content":"Hi"}}]}"#).unwrap();

        // Created first, then InProgress
        let created_pos = events
            .iter()
            .position(|e| matches!(e, StreamEvent::Created(_)))
            .unwrap();
        let in_progress_pos = events
            .iter()
            .position(|e| matches!(e, StreamEvent::InProgress(_)))
            .unwrap();
        assert!(
            created_pos < in_progress_pos,
            "created must come before in_progress"
        );
    }
}
