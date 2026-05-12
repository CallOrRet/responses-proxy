# OpenAI Responses API — Streaming Events Complete Reference

> **Source**: OpenAI official API reference (developers.openai.com/api/reference/resources/responses/streaming-events)
> **Compiled**: May 2026
> **Scope**: All SSE events sent by the server when `stream: true` in `POST /v1/responses` (or in WebSocket mode).
> **Document conventions**: Every `object` type field is expanded below with its complete sub-fields; `*` denotes a required field; `?` denotes an optional field; `| null` denotes that the field may be `null`.

---

## Table of Contents

- [1. Overview](#1-overview)
- [2. Common Fields for All Events](#2-common-fields-for-all-events)
- [3. Lifecycle Events (6 types)](#3-lifecycle-events-6-types)
- [4. Complete Response Object Expanded (for lifecycle event reference)](#4-complete-response-object-expanded-for-lifecycle-event-reference)
- [5. Output Item-Level Events (2 types)](#5-output-item-level-events-2-types)
- [6. Content Part-Level Events (2 types)](#6-content-part-level-events-2-types)
- [7. Text Delta Events (5 types)](#7-text-delta-events-5-types)
- [8. Refusal Events (2 types)](#8-refusal-events-2-types)
- [9. Function Call and Custom Tool Events (4 types)](#9-function-call-and-custom-tool-events-4-types)
- [10. File Search Events (3 types)](#10-file-search-events-3-types)
- [11. Web Search Events (3 types)](#11-web-search-events-3-types)
- [12. Code Interpreter Events (5 types)](#12-code-interpreter-events-5-types)
- [13. Image Generation Events (4 types)](#13-image-generation-events-4-types)
- [14. MCP Events (8 types)](#14-mcp-events-8-types)
- [15. Reasoning Events (6 types)](#15-reasoning-events-6-types)
- [16. Error Events](#16-error-events)
- [17. Field Nullability / Occurrence Pattern Quick Reference](#17-field-nullability--occurrence-pattern-quick-reference)
- [18. Typical Event Sequences](#18-typical-event-sequences)
- [Streaming API responses](#streaming-api-responses)

---

## 1. Overview

| Item | Value |
|---|---|
| Trigger condition | `POST /v1/responses` with `stream: true` in body, or using `wss://api.openai.com/v1/responses` |
| Content-Type | `text/event-stream` |
| Wire format | Each event: `event: <type>\ndata: <JSON>\n\n` (under WebSocket there is no `event:` header, just JSON frames) |
| Termination signal | `response.completed`, `response.failed`, `response.incomplete`, or a stream-level `error` event |

**SSE wire example**:

```
event: response.output_text.delta
data: {"type":"response.output_text.delta","sequence_number":12,"item_id":"msg_001","output_index":0,"content_index":0,"delta":"你好"}

```

---

## 2. Common Fields for All Events

Every event JSON always has these two fields:

```ts
StreamEventBase {
  type: string;            // *
  sequence_number: number; // * Monotonically increasing, starting from 0 or 1
}
```

| Field | Type | Required | Always present? | Description |
|---|---|---|---|---|
| `type` | `string` | * | ✅ | Event type, see each chapter |
| `sequence_number` | `integer` | * | ✅ | Monotonically increasing. Used for detecting out-of-order events and reconnection (`GET /responses/{id}?stream=true&starting_after=N`) |

---

## 3. Lifecycle Events (6 types)

These 6 events all carry a **complete Response object** as the `response` field. See [§4](#4-complete-response-object-expanded-for-lifecycle-event-reference) for the complete Response object structure.

### 3.1 `response.created`

The response object has just been created; the model has not yet started producing output.

```ts
{
  type: "response.created";   // *
  sequence_number: number;    // *
  response: Response;         // * Complete Response, but `output` is typically [] empty array, `usage` is null
}
```

| Field | Type | Always present? | Nullable | Description |
|---|---|---|---|---|
| `type` | `string` | ✅ | No | Fixed `"response.created"` |
| `sequence_number` | `integer` | ✅ | No | — |
| `response` | `object` | ✅ | No | Complete Response; `output:[]`, `usage:null`, `status:"in_progress"` |

#### Example

```json
{
  "type": "response.created",
  "sequence_number": 1,
  "response": {
    "id": "resp_67ccfcdd16748190a91872c75d38539e09e4d4aac714747c",
    "object": "response",
    "created_at": 1741487325,
    "status": "in_progress",
    "completed_at": null,
    "error": null,
    "incomplete_details": null,
    "instructions": null,
    "max_output_tokens": null,
    "model": "gpt-4o-2024-08-06",
    "output": [],
    "parallel_tool_calls": true,
    "previous_response_id": null,
    "reasoning": { "effort": null, "summary": null },
    "store": true,
    "temperature": 1,
    "text": { "format": { "type": "text" } },
    "tool_choice": "auto",
    "tools": [],
    "top_p": 1,
    "truncation": "disabled",
    "usage": null,
    "user": null,
    "metadata": {}
  }
}
```

### 3.2 `response.in_progress`

The response has entered the generation phase. Structure is identical to `response.created`, only `type` differs.

```ts
{
  type: "response.in_progress";
  sequence_number: number;
  response: Response;
}
```

### 3.3 `response.queued`

Only appears when **`background: true`**, indicating the response has entered the background queue and has not yet been picked up by the server for processing.

```ts
{
  type: "response.queued";
  sequence_number: number;
  response: Response;        // status: "queued"
}
```

### 3.4 `response.completed`

The final event when the response is fully complete. **This is the only event guaranteed to have `response.usage` non-null**.

```ts
{
  type: "response.completed";
  sequence_number: number;
  response: Response;       // Complete, including usage, completed_at; status: "completed"
}
```

| Field | Always present | Description |
|---|---|---|
| `response.status` | ✅ | `"completed"` |
| `response.usage` | ✅ | Non-null |
| `response.completed_at` | ✅ | Unix seconds |
| `response.output` | ✅ | All output items are ready |

### 3.5 `response.failed`

The response terminated due to failure.

```ts
{
  type: "response.failed";
  sequence_number: number;
  response: Response;       // status: "failed"; error is non-null
}
```

| Field | Always present | Description |
|---|---|---|
| `response.status` | ✅ | `"failed"` |
| `response.error` | ✅ | Non-null, see §4.3 |
| `response.usage` | Conditional | May be null on early failure |

### 3.6 `response.incomplete`

The response was truncated (reached token limit, filtered by content, etc.).

```ts
{
  type: "response.incomplete";
  sequence_number: number;
  response: Response;       // status: "incomplete"; incomplete_details is non-null
}
```

| Field | Always present | Description |
|---|---|---|
| `response.status` | ✅ | `"incomplete"` |
| `response.incomplete_details` | ✅ | Non-null, see §4.4 |

---

## 4. Complete Response Object Expanded (for lifecycle event reference)

### 4.1 Response Top-level

```ts
Response {
  id: string;                                                       // *
  object: "response";                                               // *
  created_at: number;                                               // *
  model: string;                                                    // *
  output: OutputItem[];                                             // *(may be [] early on)
  parallel_tool_calls: boolean;                                     // *
  tool_choice: ToolChoice;                                          // *
  tools: Tool[];                                                    // *
  temperature: number;                                              // *
  top_p: number;                                                    // *
  metadata: { [k: string]: string };                                // *(may be empty object)
  error: ResponseError | null;                                      // *
  incomplete_details: IncompleteDetails | null;                     // *
  instructions: string | InputItem[] | null;                        // *

  // The following fields may be absent or null (depending on model, status, request options)
  status?: ResponseStatus | null;
  completed_at?: number | null;
  background?: boolean | null;
  conversation?: { id: string } | null;
  max_output_tokens?: number | null;
  max_tool_calls?: number | null;
  output_text?: string | null;                                      // SDK convenience field, typically absent in raw JSON
  previous_response_id?: string | null;
  prompt?: ResponsePrompt | null;
  prompt_cache_key?: string | null;
  prompt_cache_retention?: "in-memory" | "24h" | null;
  reasoning?: Reasoning | null;
  safety_identifier?: string | null;
  service_tier?: "auto" | "default" | "flex" | "scale" | "priority" | null;
  text?: ResponseTextConfig | null;
  top_logprobs?: number | null;
  truncation?: "auto" | "disabled" | null;
  usage?: ResponseUsage | null;
  user?: string | null;                                              // Deprecated
}
```

| Field | Always present? | Nullable | Description |
|---|---|---|---|
| `id` | ✅ | No | `resp_xxx` |
| `object` | ✅ | No | Fixed `"response"` |
| `created_at` | ✅ | No | Unix seconds |
| `model` | ✅ | No | Actual model snapshot |
| `output` | ✅ | No | Array, empty in early lifecycle |
| `parallel_tool_calls` | ✅ | No | Echoed |
| `tool_choice` | ✅ | No | Echoed |
| `tools` | ✅ | No | Echoed array |
| `temperature` / `top_p` | ✅ | No | Echoed |
| `metadata` | ✅ | No | Nullable object `{}` |
| `error` | ✅ | ✅ | Non-null only when `failed` |
| `incomplete_details` | ✅ | ✅ | Non-null only when `incomplete` |
| `instructions` | ✅ | ✅ | String or InputItem array |
| `usage` | ❌ | ✅ | Non-null only in `response.completed` event (and some `failed`) |
| `completed_at` | ❌ | ✅ | Only present when `status:"completed"` |
| Rest | Conditional | ✅ | Mostly request echoes |

### 4.2 `ResponseStatus` Enum

```ts
type ResponseStatus =
  | "queued"
  | "in_progress"
  | "completed"
  | "failed"
  | "incomplete"
  | "cancelled";
```

### 4.3 `ResponseError` Object

```ts
ResponseError {
  code: ErrorCode;    // *
  message: string;    // *
}
```

##### `ErrorCode` — 18 enum values

| Error Code | Meaning |
|---|---|
| `server_error` | Server error |
| `rate_limit_exceeded` | Rate limit exceeded |
| `invalid_prompt` | Invalid prompt |
| `vector_store_timeout` | Vector store timeout |
| `invalid_image` | Invalid image |
| `invalid_image_format` | Invalid image format |
| `invalid_base64_image` | Base64 decoding failed |
| `invalid_image_url` | Invalid image URL |
| `image_too_large` | Image too large |
| `image_too_small` | Image too small |
| `image_parse_error` | Image parse error |
| `image_content_policy_violation` | Image content policy violation |
| `invalid_image_mode` | Image mode not supported |
| `image_file_too_large` | Image file too large |
| `unsupported_image_media_type` | Unsupported media type |
| `empty_image_file` | Empty image file |
| `failed_to_download_image` | Image download failed |
| `image_file_not_found` | Image file not found |

### 4.4 `IncompleteDetails` Object

```ts
IncompleteDetails {
  reason?: "max_output_tokens" | "content_filter" | null;
}
```

| Field | Type | Nullable | Values |
|---|---|---|---|
| `reason` | `string` | ✅ | `max_output_tokens` / `content_filter` |

### 4.5 `ResponseUsage` Object

```ts
ResponseUsage {
  input_tokens: number;                                    // *
  input_tokens_details: InputTokensDetails;                // *
  output_tokens: number;                                   // *
  output_tokens_details: OutputTokensDetails;              // *
  total_tokens: number;                                    // *
}
InputTokensDetails  { cached_tokens: number }              // * Cache hits
OutputTokensDetails { reasoning_tokens: number }           // * Consumed by reasoning model
```

| Field | Always present? | Description |
|---|---|---|
| `input_tokens` | ✅ | Total input tokens |
| `input_tokens_details.cached_tokens` | ✅ | Cached tokens hit |
| `output_tokens` | ✅ | Total output tokens (including reasoning) |
| `output_tokens_details.reasoning_tokens` | ✅ | Tokens used by reasoning model for thinking |
| `total_tokens` | ✅ | input + output |

### 4.6 `Reasoning` Object

```ts
Reasoning {
  effort?: "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | null;
  summary?: "auto" | "concise" | "detailed" | null;
  generate_summary?: "auto" | "concise" | "detailed" | null;  // Deprecated
}
```

### 4.7 `ResponseTextConfig` Object

```ts
ResponseTextConfig {
  format?: ResponseFormatTextConfig | null;
  verbosity?: "low" | "medium" | "high" | null;
}
```

`ResponseFormatTextConfig` — one of three options:

```ts
// (a) Plain text (default)
{ type: "text" }

// (b) JSON Schema structured output
{
  type: "json_schema";              // *
  name: string;                     // * Length ≤64
  schema: object;                   // * JSON Schema
  strict?: boolean | null;
  description?: string | null;
}

// (c) Legacy JSON mode
{ type: "json_object" }
```

### 4.8 `ResponsePrompt` Object

```ts
ResponsePrompt {
  id: string;                                             // *
  variables?: { [k: string]: VariableValue } | null;
  version?: string | null;
}
type VariableValue = string | InputText | InputImage | InputFile;
```

### 4.9 `ToolChoice` Union Type (7 kinds)

Literals: `"none" | "auto" | "required"`, or any of the following objects:

```ts
ToolChoiceAllowed    { type:"allowed_tools"; mode:"auto"|"required"; tools:Array<{type:string; name?:string; server_label?:string}> }
ToolChoiceTypes      { type:"file_search"|"web_search_preview"|"web_search_preview_2025_03_11"|"computer_use_preview"|"image_generation"|"code_interpreter" }
ToolChoiceFunction   { type:"function"; name:string }
ToolChoiceMcp        { type:"mcp"; server_label:string; name?:string }
ToolChoiceCustom     { type:"custom"; name:string }
ToolChoiceApplyPatch { type:"apply_patch" }
ToolChoiceShell      { type:"shell" }
```

### 4.10 `Tool` Union Type (13+ kinds)

Each element in the `tools` array is distinguished by `type`. Common type list (see the series "Responses API Complete Reference" §3.8 for detailed structure):

| `type` | Key Fields |
|---|---|
| `function` | `name`, `description?`, `parameters?`, `strict?` |
| `file_search` | `vector_store_ids`, `max_num_results?`, `filters?`, `ranking_options?` |
| `computer_use_preview` | `display_width`, `display_height`, `environment` |
| `computer` | (type only) |
| `web_search` / `web_search_2025_08_26` | `filters?`, `search_context_size?`, `user_location?` |
| `web_search_preview` / `web_search_preview_2025_03_11` | `search_context_size?`, `user_location?` |
| `mcp` | `server_label`, `server_url?`/`connector_id?`, `allowed_tools?`, `require_approval?`, `authorization?`, `headers?` |
| `code_interpreter` | `container` (string or `ContainerAuto`) |
| `image_generation` | `model?`, `quality?`, `size?`, `partial_images?`, … |
| `local_shell` | (type only) |
| `shell` | `environment?` |
| `custom` | `name`, `description?`, `format?` |
| `apply_patch` | (type only) |

### 4.11 `OutputItem` Union Type (20+ kinds)

Each element of the `output` array is distinguished by `type`.

List of possible `type` values:

```
message            reasoning            function_call          custom_tool_call
file_search_call   web_search_call      computer_call          computer_call_output
function_call_output  custom_tool_call_output
code_interpreter_call   image_generation_call
local_shell_call   local_shell_call_output
shell_call         shell_call_output
apply_patch_call   apply_patch_call_output
mcp_call           mcp_list_tools       mcp_approval_request   mcp_approval_response
compaction         item_reference
tool_search_call   tool_search_output
```

For field definitions of each item type, see the events triggered by these items in subsequent chapters; for complete definitions, refer to the series "Responses API Complete Reference" §3.9 and §4.2.

---

## 5. Output Item-Level Events (2 types)

Triggered whenever an item is added to or completed in the output array.

### 5.1 `response.output_item.added`

```ts
{
  type: "response.output_item.added";       // *
  sequence_number: number;                  // *
  output_index: number;                     // * Index of this item in output[]
  item: OutputItem;                         // * Complete item object; most fields are already present, but `status` is typically "in_progress"
}
```

| Field | Type | Always present | Nullable | Description |
|---|---|---|---|---|
| `type` | `string` | ✅ | No | Fixed |
| `sequence_number` | `integer` | ✅ | No | — |
| `output_index` | `integer` | ✅ | No | Index |
| `item` | `object` | ✅ | No | Complete OutputItem (one of 20+ types, see §4.11) |

### 5.2 `response.output_item.done`

```ts
{
  type: "response.output_item.done";        // *
  sequence_number: number;                  // *
  output_index: number;                     // *
  item: OutputItem;                         // * Item is now "completed"
}
```

Field meanings same as `.added`. `item.status` is typically `"completed"`.

---

## 6. Content Part-Level Events (2 types)

Only for sub-parts within the `content[]` of `message`-type items.

### 6.1 `response.content_part.added` / `.done`

```ts
{
  type: "response.content_part.added" | "response.content_part.done";
  sequence_number: number;
  item_id: string;          // * ID of the owning message item
  output_index: number;     // * Index in output[]
  content_index: number;    // * Index in message.content[]
  part: OutputText | OutputRefusal;  // * (final content when .done)
}
```

| Field | Type | Always present | Description |
|---|---|---|---|
| `type` | `string` | ✅ | One of two |
| `item_id` | `string` | ✅ | `msg_xxx` |
| `output_index` | `integer` | ✅ | output array index |
| `content_index` | `integer` | ✅ | content array index |
| `part` | `object` | ✅ | see below |

#### `OutputText` Object

```ts
{
  type: "output_text";                                              // *
  text: string;                                                     // *
  annotations: Annotation[];                                        // *(can be empty array)
  logprobs?: Logprob[] | null;
}
```

See [§7.3](#73-responseoutput_textannotationadded) for the 4 sub-types of `Annotation`.

#### `OutputRefusal` Object

```ts
{
  type: "refusal";      // *
  refusal: string;      // *
}
```

---

## 7. Text Delta Events (5 types)

### 7.1 `response.output_text.delta`

```ts
{
  type: "response.output_text.delta";       // *
  sequence_number: number;                  // *
  item_id: string;                          // *
  output_index: number;                     // *
  content_index: number;                    // *
  delta: string;                            // * Text increment for this event
  obfuscation?: string | null;
  logprobs?: Logprob[] | null;
}
```

| Field | Type | Always present | Nullable | Description |
|---|---|---|---|---|
| `type` | `string` | ✅ | No | Fixed |
| `item_id` | `string` | ✅ | No | `msg_xxx` |
| `output_index` | `integer` | ✅ | No | output index |
| `content_index` | `integer` | ✅ | No | content index |
| `delta` | `string` | ✅ | No | Incremental fragment |
| `obfuscation` | `string` | ❌ | ✅ | **Only appears when request includes `stream_options.include_obfuscation: true` (default true)**; field absent when disabled |
| `logprobs` | `array` | ❌ | ✅ | **Only appears when request includes `include: ["message.output_text.logprobs"]`** |

### 7.2 `response.output_text.done`

```ts
{
  type: "response.output_text.done";
  sequence_number: number;
  item_id: string;
  output_index: number;
  content_index: number;
  text: string;                  // * Complete final text of this content part
  logprobs?: Logprob[] | null;
}
```

### 7.3 `response.output_text.annotation.added`

Annotations do not appear alongside deltas; they are delivered via dedicated events.

```ts
{
  type: "response.output_text.annotation.added";
  sequence_number: number;
  item_id: string;
  output_index: number;
  content_index: number;
  annotation_index: number;      // * Index in annotations[] array
  annotation: Annotation;        // * One of 4 types
}
```

#### 4 Sub-types of `Annotation`

##### (a) FileCitation

```ts
{ type: "file_citation"; file_id: string; filename: string; index: number }
```

##### (b) UrlCitation

```ts
{
  type: "url_citation";
  url: string;
  title: string;
  start_index: number;
  end_index: number;
}
```

##### (c) ContainerFileCitation

```ts
{
  type: "container_file_citation";
  container_id: string;
  file_id: string;
  filename: string;
  start_index: number;
  end_index: number;
}
```

##### (d) FilePath

```ts
{ type: "file_path"; file_id: string; index: number }
```

#### `Logprob` Object

```ts
Logprob {
  token: string;
  bytes: number[];
  logprob: number;
  top_logprobs: Array<{
    token: string;
    bytes: number[];
    logprob: number;
  }>;
}
```

---

## 8. Refusal Events (2 types)

### 8.1 `response.refusal.delta`

```ts
{
  type: "response.refusal.delta";
  sequence_number: number;
  item_id: string;
  output_index: number;
  content_index: number;
  delta: string;                 // * Increment of refusal explanation
}
```

### 8.2 `response.refusal.done`

```ts
{
  type: "response.refusal.done";
  sequence_number: number;
  item_id: string;
  output_index: number;
  content_index: number;
  refusal: string;               // * Complete refusal explanation
}
```

---

## 9. Function Call and Custom Tool Events (4 types)

### 9.1 `response.function_call_arguments.delta`

Incremental `arguments` (JSON string) generated by the model for a function call.

```ts
{
  type: "response.function_call_arguments.delta";
  sequence_number: number;
  item_id: string;
  output_index: number;
  delta: string;                 // * Arguments JSON string increment
}
```

### 9.2 `response.function_call_arguments.done`

```ts
{
  type: "response.function_call_arguments.done";
  sequence_number: number;
  item_id: string;
  output_index: number;
  arguments: string;             // * Complete JSON string. Note: the model may produce invalid JSON; validate before use
}
```

### 9.3 `response.custom_tool_call_input.delta`

Incremental `input` (free-form string) for a custom tool call.

```ts
{
  type: "response.custom_tool_call_input.delta";
  sequence_number: number;
  item_id: string;
  output_index: number;
  delta: string;
}
```

### 9.4 `response.custom_tool_call_input.done`

```ts
{
  type: "response.custom_tool_call_input.done";
  sequence_number: number;
  item_id: string;
  output_index: number;
  input: string;                 // *
}
```

---

## 10. File Search Events (3 types)

All carry only positioning fields.

### 10.1 `response.file_search_call.in_progress`

```ts
{
  type: "response.file_search_call.in_progress";
  sequence_number: number;
  item_id: string;
  output_index: number;
}
```

### 10.2 `response.file_search_call.searching`

```ts
{
  type: "response.file_search_call.searching";
  sequence_number: number;
  item_id: string;
  output_index: number;
}
```

### 10.3 `response.file_search_call.completed`

```ts
{
  type: "response.file_search_call.completed";
  sequence_number: number;
  item_id: string;
  output_index: number;
}
```

> The final `results` are in the item carried by `response.output_item.done` (when `include: ["file_search_call.results"]` is enabled).

#### `FileSearchResult` Object (inside item)

```ts
FileSearchResult {
  file_id?: string | null;
  filename?: string | null;
  score?: number | null;                  // 0-1
  text?: string | null;
  attributes?: { [k: string]: string | number | boolean } | null;  // Up to 16 metadata pairs
}
```

---

## 11. Web Search Events (3 types)

```ts
{ type: "response.web_search_call.in_progress"; sequence_number; item_id; output_index }
{ type: "response.web_search_call.searching";   sequence_number; item_id; output_index }
{ type: "response.web_search_call.completed";   sequence_number; item_id; output_index }
```

| Field | Type | Always present | Description |
|---|---|---|---|
| `type` | `string` | ✅ | One of three |
| `sequence_number` | `integer` | ✅ | — |
| `item_id` | `string` | ✅ | Web search call ID |
| `output_index` | `integer` | ✅ | output index |

The final `action` (and its internal `queries` / `sources` etc.) are in the item of `response.output_item.done`.

#### Three Types of `WebSearchAction`

```ts
ActionSearch     { type:"search";       query:string; queries?:string[]; sources?:Array<{type:"url"; url:string}> }
ActionOpenPage   { type:"open_page";    url?:string }
ActionFindInPage { type:"find_in_page"; url:string; pattern:string }
```

---

## 12. Code Interpreter Events (5 types)

```ts
{ type: "response.code_interpreter_call.in_progress"; sequence_number; item_id; output_index }
{
  type: "response.code_interpreter_call_code.delta";
  sequence_number; item_id; output_index;
  delta: string;                     // * Code snippet increment
}
{
  type: "response.code_interpreter_call_code.done";
  sequence_number; item_id; output_index;
  code: string;                      // * Complete code
}
{ type: "response.code_interpreter_call.interpreting"; sequence_number; item_id; output_index }
{ type: "response.code_interpreter_call.completed";    sequence_number; item_id; output_index }
```

The final `outputs[]` (logs / image) are in the item of `response.output_item.done`.

#### Two Types of `CodeInterpreterOutput`

```ts
Logs  { type: "logs";  logs: string }
Image { type: "image"; url: string }
```

---

## 13. Image Generation Events (4 types)

Supports intermediate delivery of partial images (`partial_images` 0-3).

```ts
{ type: "response.image_generation_call.in_progress"; sequence_number; item_id; output_index }
{ type: "response.image_generation_call.generating";  sequence_number; item_id; output_index }
{
  type: "response.image_generation_call.partial_image";
  sequence_number; item_id; output_index;
  partial_image_index: number;       // * 0, 1, 2
  partial_image_b64: string;         // * base64 PNG
}
{ type: "response.image_generation_call.completed";   sequence_number; item_id; output_index }
```

| Field | Type | Always present | Description |
|---|---|---|---|
| `partial_image_index` | `integer` | ✅ | 0 / 1 / 2 |
| `partial_image_b64` | `string` | ✅ | base64 PNG bytes |

The final `result` (complete base64 image) is in the item of `response.output_item.done`.

---

## 14. MCP Events (8 types)

### 14.1 Tool Call Phase (5 types)

```ts
{ type: "response.mcp_call.in_progress"; sequence_number; item_id; output_index }
{
  type: "response.mcp_call_arguments.delta";
  sequence_number; item_id; output_index;
  delta: string;                     // * Arguments JSON string increment
}
{
  type: "response.mcp_call_arguments.done";
  sequence_number; item_id; output_index;
  arguments: string;                 // * Complete JSON string
}
{ type: "response.mcp_call.completed"; sequence_number; item_id; output_index }
{ type: "response.mcp_call.failed";    sequence_number; item_id; output_index }
```

The final `output` / `error` fields are in the `mcp_call` item of `response.output_item.done`.

### 14.2 List and Approval Phase (3 types)

```ts
{ type: "response.mcp_list_tools.in_progress"; sequence_number; item_id; output_index }
{ type: "response.mcp_list_tools.completed";   sequence_number; item_id; output_index }
{ type: "response.mcp_list_tools.failed";      sequence_number; item_id; output_index }
```

The final `tools[]` list is in the `mcp_list_tools` item of `response.output_item.done`:

#### `McpToolInfo` Object

```ts
McpToolInfo {
  name: string;                  // *
  input_schema: object;          // * JSON Schema describing tool input
  description?: string | null;
  annotations?: object | null;
}
```

---

## 15. Reasoning Events (6 types)

Thought summaries and text streams produced by reasoning models (gpt-5, o-series).

### 15.1 Summary Part-Level (2 types)

```ts
{
  type: "response.reasoning_summary_part.added";
  sequence_number; item_id; output_index;
  summary_index: number;
  part: { type: "summary_text"; text: string };
}
{
  type: "response.reasoning_summary_part.done";
  sequence_number; item_id; output_index;
  summary_index: number;
  part: { type: "summary_text"; text: string };
}
```

| Field | Type | Always present | Description |
|---|---|---|---|
| `summary_index` | `integer` | ✅ | summary[] array index |
| `part.type` | `string` | ✅ | Fixed `"summary_text"` |
| `part.text` | `string` | ✅ | Summary text |

### 15.2 Summary Text Increments (2 types)

```ts
{
  type: "response.reasoning_summary_text.delta";
  sequence_number; item_id; output_index;
  summary_index: number;
  delta: string;
}
{
  type: "response.reasoning_summary_text.done";
  sequence_number; item_id; output_index;
  summary_index: number;
  text: string;
}
```

### 15.3 Reasoning Text Increments (2 types)

Only produced by some models (those supporting detailed reasoning text).

```ts
{
  type: "response.reasoning_text.delta";
  sequence_number; item_id; output_index;
  content_index: number;
  delta: string;
}
{
  type: "response.reasoning_text.done";
  sequence_number; item_id; output_index;
  content_index: number;
  text: string;
}
```

---

## 16. Error Events

**SSE-level error** (different from `response.failed` — that is a response-level failure):

```ts
{
  type: "error";              // *
  sequence_number: number;    // *
  code: string | null;        // *(may be null)
  message: string;            // *
  param?: string | null;
}
```

| Field | Type | Always present | Nullable | Description |
|---|---|---|---|---|
| `type` | `string` | ✅ | No | Fixed `"error"` |
| `sequence_number` | `integer` | ✅ | No | — |
| `code` | `string \| null` | ✅ | ✅ | Error code |
| `message` | `string` | ✅ | No | Human readable |
| `param` | `string` | ❌ | ✅ | Name of the field that triggered the error |

> After an `error` event appears, the server typically closes the stream; `response.completed` will **not** be sent.

---

## 17. Field Nullability / Occurrence Pattern Quick Reference

| Field | Where it appears |
|---|---|
| `response.usage` (top-level reference) | Non-null only in `response.completed` event (and some `response.failed`); null or absent in early events |
| `response.completed_at` | Only in `response.completed` event |
| `response.error` (non-null) | Only in `response.failed` |
| `response.incomplete_details` (non-null) | Only in `response.incomplete` |
| `obfuscation` (delta events) | Only appears when `stream_options.include_obfuscation: true` (enabled by default) |
| `logprobs` (`output_text.*` events) | Only appears when request includes `include: ["message.output_text.logprobs"]` |
| `annotations` array | Not present in `output_text.delta`; delivered via dedicated `response.output_text.annotation.added` |
| `partial_image_b64` | Only in `image_generation_call.partial_image` event |
| `delta` / `text` / `arguments` etc. increment fields | Only in corresponding delta / done events |
| All events `type` and `sequence_number` | **Always present** |

---

## 18. Typical Event Sequences

### 18.1 Plain Text Generation (No Tools)

```
response.created                          (response.output: [])
response.in_progress
response.output_item.added                (item: message, status: in_progress)
response.content_part.added               (part: {type:"output_text", text:""})
response.output_text.delta                ("Hello")
response.output_text.delta                (", world")
response.output_text.delta                ("!")
response.output_text.done                 (text:"Hello, world!")
response.content_part.done
response.output_item.done                 (item.status: completed)
response.completed                        (usage non-null)
```

### 18.2 Reasoning Model Generation

```
response.created
response.in_progress
response.output_item.added                (item: reasoning)
response.reasoning_summary_part.added
response.reasoning_summary_text.delta     ×N
response.reasoning_summary_text.done
response.reasoning_summary_part.done
response.output_item.done                 (reasoning completed)
response.output_item.added                (item: message)
response.content_part.added
response.output_text.delta                ×N
response.output_text.done
response.content_part.done
response.output_item.done
response.completed
```

### 18.3 Function Call

```
response.created
response.in_progress
response.output_item.added                (item: function_call, arguments: "")
response.function_call_arguments.delta    ('{"location":"')
response.function_call_arguments.delta    ('Boston"}')
response.function_call_arguments.done     (arguments: '{"location":"Boston"}')
response.output_item.done                 (function_call completed)
response.completed                        (output[0] = function_call; after processing, submit back with a new request)
```

### 18.4 Web Search

```
response.created
response.in_progress
response.output_item.added                (item: web_search_call)
response.web_search_call.in_progress
response.web_search_call.searching
response.web_search_call.completed
response.output_item.done                 (web_search_call completed, action ready)
response.output_item.added                (item: message)
... (output text + URL citations delivered via annotation.added)
response.completed
```

### 18.5 Background Mode (background: true)

```
response.created                          (status: queued or in_progress)
response.queued                           (status: queued)
... (after server processing, continues with in_progress / completed, or client reconnects via
     GET /v1/responses/{id}?stream=true&starting_after=N)
```

---

## Streaming API responses

By default, when you make a request to the OpenAI API, we generate the model's entire output before sending it back in a single HTTP response. When generating long outputs, waiting for a response can take time. Streaming responses lets you start printing or processing the beginning of the model's output while it continues generating the full response.

This guide focuses on HTTP streaming (`stream=true`) over server-sent events (SSE). For persistent WebSocket transport with incremental inputs via `previous_response_id`, see [the Responses API WebSocket mode](https://developers.openai.com/api/docs/guides/websocket-mode).

### Enable streaming


To start streaming responses, set `stream=True` in your request to the Responses endpoint:

The Responses API uses semantic events for streaming. Each event is typed with a predefined schema, so you can listen for events you care about.

For a full list of event types, see the [API reference for streaming](https://developers.openai.com/api/docs/api-reference/responses-streaming). Here are a few examples:

```python
type StreamingEvent =
	| ResponseCreatedEvent
	| ResponseInProgressEvent
	| ResponseFailedEvent
	| ResponseCompletedEvent
	| ResponseOutputItemAdded
	| ResponseOutputItemDone
	| ResponseContentPartAdded
	| ResponseContentPartDone
	| ResponseOutputTextDelta
	| ResponseOutputTextAnnotationAdded
	| ResponseTextDone
	| ResponseRefusalDelta
	| ResponseRefusalDone
	| ResponseFunctionCallArgumentsDelta
	| ResponseFunctionCallArgumentsDone
	| ResponseFileSearchCallInProgress
	| ResponseFileSearchCallSearching
	| ResponseFileSearchCallCompleted
	| ResponseCodeInterpreterInProgress
	| ResponseCodeInterpreterCallCodeDelta
	| ResponseCodeInterpreterCallCodeDone
	| ResponseCodeInterpreterCallInterpreting
	| ResponseCodeInterpreterCallCompleted
	| Error
```

### Read the responses


If you're using our SDK, every event is a typed instance. You can also identity individual events using the `type` property of the event.

Some key lifecycle events are emitted only once, while others are emitted multiple times as the response is generated. Common events to listen for when streaming text are:

```
- `response.created`
- `response.output_text.delta`
- `response.completed`
- `error`
```

For a full list of events you can listen for, see the [API reference for streaming](https://developers.openai.com/api/docs/api-reference/responses-streaming).



### Advanced use cases

For more advanced use cases, like streaming tool calls, check out the following dedicated guides:

- [Streaming function calls](https://developers.openai.com/api/docs/guides/function-calling#streaming)
- [Streaming structured output](https://developers.openai.com/api/docs/guides/structured-outputs#streaming)

### Moderation risk

Note that streaming the model's output in a production application makes it more difficult to moderate the content of the completions, as partial completions may be more difficult to evaluate. This may have implications for approved usage.

*End of document.*
