# End-to-End Conversion Verification

Based on OpenAI Go SDK + DeepSeek API docs, tracing actual code behavior for every conversion path.

---

## Scenario 1: Simple Text (no streaming, no tools, no reasoning)

### Client → Proxy (Responses API)
```json
{
  "model": "gpt-5.5",
  "input": "What is 2+2? Reply with just the number."
}
```

### Proxy → DeepSeek (Chat API)
```json
{
  "model": "deepseek-v4-pro",
  "messages": [
    {"role": "user", "content": "What is 2+2? Reply with just the number."}
  ]
}
```

Tracing: `input: String("...")` → single user message. No `instructions` → no system message. No `reasoning` → `thinking` and `reasoning_effort` omitted via `skip_serializing_if`.

Compliance: ✅ Fully conformant. Only required fields `model` + `messages` are sent.

### DeepSeek → Proxy (Chat API Response)
```json
{
  "id": "chatcmpl-abc123",
  "object": "chat.completion",
  "created": 1715550000,
  "model": "deepseek-v4-pro",
  "choices": [{
    "index": 0,
    "message": {"role": "assistant", "content": "4"},
    "finish_reason": "stop"
  }],
  "usage": {
    "prompt_tokens": 12,
    "completion_tokens": 1,
    "total_tokens": 13,
    "completion_tokens_details": {"reasoning_tokens": 0}
  }
}
```

### Proxy → Client (Responses API)
```json
{
  "id": "resp_...",
  "object": "response",
  "created_at": 1715550000.0,
  "status": "completed",
  "model": "gpt-5.5",
  "output": [{
    "type": "message",
    "id": "msg_...",
    "role": "assistant",
    "status": "completed",
    "content": [{"type": "output_text", "text": "4", "annotations": []}]
  }],
  "usage": {
    "input_tokens": 12,
    "output_tokens": 1,
    "total_tokens": 13,
    "input_tokens_details": {"cached_tokens": 0},
    "output_tokens_details": {"reasoning_tokens": 0}
  }
}
```

Compliance: ✅ Output item type and content structure match OpenAI `ResponseOutputMessage`. Usage field mapping matches `ResponseUsage`.

---

## Scenario 2: Instructions + Reasoning Mode (non-streaming)

### Client → Proxy
```json
{
  "model": "gpt-5.5",
  "input": "Solve the complex equation.",
  "instructions": "You are a math tutor. Always show your work.",
  "reasoning": {"effort": "xhigh"}
}
```

### Proxy → DeepSeek
```json
{
  "model": "deepseek-v4-pro",
  "messages": [
    {"role": "system", "content": "You are a math tutor. Always show your work."},
    {"role": "user", "content": "Solve the complex equation."}
  ],
  "thinking": {"type": "enabled"},
  "reasoning_effort": "max"
}
```

Tracing: `instructions` → prepended system message. `reasoning.effort="xhigh"` → `reasoning_effort="max"` + `thinking={"type":"enabled"}`. Both are top-level fields.

Compliance: ✅ Matches DeepSeek API spec exactly.

### DeepSeek → Proxy
```json
{
  "id": "chatcmpl-def456",
  "object": "chat.completion",
  "created": 1715550000,
  "model": "deepseek-v4-pro",
  "choices": [{
    "index": 0,
    "message": {
      "role": "assistant",
      "content": "x = 5",
      "reasoning_content": "First, we isolate x by..."
    },
    "finish_reason": "stop"
  }],
  "usage": {
    "prompt_tokens": 40,
    "completion_tokens": 50,
    "total_tokens": 90,
    "completion_tokens_details": {"reasoning_tokens": 30}
  }
}
```

### Proxy → Client
```json
{
  "id": "resp_...",
  "object": "response",
  "created_at": 1715550000.0,
  "status": "completed",
  "model": "gpt-5.5",
  "output": [
    {
      "type": "reasoning",
      "id": "rs_...",
      "summary": [{"type": "summary_text", "text": "First, we isolate x by..."}]
    },
    {
      "type": "message",
      "id": "msg_...",
      "role": "assistant",
      "status": "completed",
      "content": [{"type": "output_text", "text": "x = 5", "annotations": []}]
    }
  ],
  "usage": {
    "input_tokens": 40,
    "output_tokens": 50,
    "total_tokens": 90,
    "input_tokens_details": {"cached_tokens": 0},
    "output_tokens_details": {"reasoning_tokens": 30}
  }
}
```

Compliance: ✅ Reasoning extracted from `reasoning_content` into `OutputReasoning`. Non-streaming uses `summary` field — matches OpenAI `ResponseReasoningItem`.

---

## Scenario 3: Streaming Reasoning + Text

### DeepSeek SSE Stream (annotated)

```
Chunk 1: reasoning_content: "Let me"
Chunk 2: reasoning_content: " think about relativity."
Chunk 3: content: "Einstein's theory"
Chunk 4: content: " of relativity..." + finish_reason: "stop"
Chunk 5: choices: [], usage: {prompt_tokens:10, completion_tokens:25, ...}
[DONE]
```

### Proxy SSE → Client (post-fix event sequence)

```
response.created         (seq=0)
response.in_progress     (seq=1)    ← newly added
output_item.added        (seq=2, reasoning, output_index=0)
content_part.added       (seq=3, reasoning_text, output_index=0)
reasoning_text.delta     (seq=4, delta:"Let me")
reasoning_text.delta     (seq=5, delta:" think about relativity.")
output_item.added        (seq=6, message, output_index=1)
content_part.added       (seq=7, output_text, output_index=1)
output_text.delta        (seq=8, delta:"Einstein's theory")
output_text.delta        (seq=9, delta:" of relativity...")
reasoning_text.done      (seq=10)
content_part.done        (seq=11)
output_item.done         (seq=12, reasoning completed)
output_text.done         (seq=13)
content_part.done        (seq=14)
output_item.done         (seq=15, message completed)
response.completed       (seq=16, with usage)   ← usage now included
```

Compliance checklist:
- Event order: created → in_progress → item.added → content_part.added → delta → done → completed ✅
- `sequence_number` increments per event ✅
- `output_index` unique and sequential (reasoning=0, message=1) ✅
- `response.completed` includes `usage` ✅
- Reasoning in `content: [{type:"reasoning_text"}]`, `summary: []` ✅

---

## Scenario 4: Text + Tool Call in Same Chunk (output_index fix validation)

### Chunk
```json
{"choices":[{"delta":{"content":"Let me check.","tool_calls":[{"index":0,"id":"call_x","type":"function","function":{"name":"search","arguments":"{}"}}]}}]}
```

### process_chunk tracing (post-fix)

Processing `content`:
- `next_output_index=0` → message `output_index=0`, next becomes 1

Processing `tool_calls[0]`:
- `next_output_index=1` → function_call `output_index=1`, next becomes 2

Result: message index=0, tool_call index=1. **No duplicates.** ✅ (Previously both were 0.)

---

## Scenario 5: Content Filter

### DeepSeek → Proxy
```json
{"choices":[{"message":{"content":null},"finish_reason":"content_filter"}]}
```

### Proxy → Client
```json
{
  "output": [{
    "type": "message",
    "status": "incomplete",
    "content": [{"type": "refusal", "refusal": "content_filter"}]
  }],
  "incomplete_details": {"reason": "content_filter"}
}
```

Compliance: ✅ Matches OpenAI `ResponseOutputRefusal` + `ResponseIncompleteDetails`.

---

## Scenario 6: Error Response

### DeepSeek → Proxy
```json
{"error": {"message": "Invalid API key", "code": "invalid_api_key"}}
```

### Proxy → Client
```json
{"status": "failed", "output": [], "error": {"code": "invalid_api_key", "message": "Invalid API key"}}
```

Compliance: ✅ `status="failed"`, empty output, `error` object with `code` + `message`.

---

## Scenario 7: Multi-Turn Function Call (WebSocket)

### Turn 1 — Client sends user message with tools
```json
{
  "type": "response.create",
  "model": "gpt-5.5",
  "input": [{"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Weather in NYC?"}]}],
  "tools": [{"type": "function", "name": "get_weather", "parameters": {...}}]
}
```

### Turn 1 — Proxy → DeepSeek
```json
{
  "model": "deepseek-v4-pro",
  "messages": [{"role": "user", "content": "Weather in NYC?"}],
  "tools": [{"type": "function", "function": {"name": "get_weather", ...}}],
  "stream": true
}
```

Tools normalized from Responses flat format to Chat nested format. ✅

### Turn 1 — Session history accumulated
```json
[
  {"type": "message", "role": "user", ...},
  {"type": "function_call", "call_id": "call_abc", "name": "get_weather", "arguments": "{\"city\":\"NYC\"}"}
]
```

### Turn 2 — Client sends function_call_output
```json
{
  "type": "response.create",
  "previous_response_id": "resp_...",
  "input": [{"type": "function_call_output", "call_id": "call_abc", "output": "Sunny, 72F"}]
}
```

### Turn 2 — Proxy → DeepSeek
```json
{
  "messages": [
    {"role": "user", "content": "Weather in NYC?"},
    {"role": "assistant", "content": null, "tool_calls": [{"id":"call_abc", "function":{"name":"get_weather",...}}]},
    {"role": "tool", "content": "Sunny, 72F", "tool_call_id": "call_abc"}
  ],
  "stream": true
}
```

Tool message correctly follows assistant(tool_calls). ✅

---

## Summary

| Scenario | Request Conv. | Response Conv. | Streaming Events | Spec Compliant |
|----------|:---:|:---:|:---:|:---:|
| 1. Simple text | ✅ | ✅ | N/A | ✅ |
| 2. Instructions + Reasoning | ✅ | ✅ | N/A | ✅ |
| 3. Streaming Reasoning + Text | ✅ | N/A | ✅ | ✅ |
| 4. Text + ToolCall same chunk | ✅ | N/A | ✅ (index fixed) | ✅ |
| 5. Content filter | N/A | ✅ | N/A | ✅ |
| 6. Error response | N/A | ✅ | N/A | ✅ |
| 7. Multi-turn WS | ✅ | N/A | ✅ | ✅ |
