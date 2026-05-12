# OpenAI Chat Completions API Complete Reference (with All Object Types Expanded)

> **Source**: OpenAI official API reference (developers.openai.com/api/reference)
> **Compiled**: May 2026
> **Scope**: HTTPS (synchronous + SSE streaming)
> **Document conventions**: Every `object` type field is independently expanded with its full sub-fields below; `*` indicates a required field; `?` indicates an optional field; `| null` indicates the field may be `null`.

> ⚠ **Official recommendation**: New projects should prefer the [Responses API](#). Chat Completions remains fully supported and will be maintained long-term, but the Responses API is the primary entry point for new features (reasoning, built-in tools, continuous conversation state, WebSocket, etc.).

---

## Table of Contents

- [1. Overview](#1-overview)
- [2. HTTPS Request Body Top-level Parameters](#2-https-request-body-top-level-parameters)
- [3. Request Body Embedded Object Definitions](#3-request-body-embedded-object-definitions)
  - [3.1 Message Object (6 Roles)](#31-message-object-6-roles)
  - [3.2 Content Part Object (5 Types)](#32-content-part-object-5-types)
  - [3.3 ToolCall Object (Request Body, 2 Types)](#33-toolcall-object-request-body-2-types)
  - [3.4 Tool Object (2 Types)](#34-tool-object-2-types)
  - [3.5 ToolChoice Object (5 Types)](#35-toolchoice-object-5-types)
  - [3.6 ResponseFormat Object (3 Types)](#36-responseformat-object-3-types)
  - [3.7 Audio Param Object](#37-audio-param-object)
  - [3.8 Prediction Object](#38-prediction-object)
  - [3.9 StreamOptions Object](#39-streamoptions-object)
  - [3.10 WebSearchOptions Object](#310-websearchoptions-object)
  - [3.11 Deprecated: Function and FunctionCall](#311-deprecated-function-and-functioncall)
- [4. HTTPS Response Body (Non-streaming)](#4-https-response-body-non-streaming)
  - [4.1 ChatCompletion Top-level](#41-chatcompletion-top-level)
  - [4.2 Choice Object](#42-choice-object)
  - [4.3 ChatCompletionMessage (Assistant Message in Response)](#43-chatcompletionmessage-assistant-message-in-response)
  - [4.4 ToolCall Object (Response, 2 Types)](#44-toolcall-object-response-2-types)
  - [4.5 Annotation Object](#45-annotation-object)
  - [4.6 Audio Response Object](#46-audio-response-object)
  - [4.7 Logprobs Object](#47-logprobs-object)
  - [4.8 Usage Object](#48-usage-object)
- [5. SSE Streaming Response - ChatCompletionChunk](#5-sse-streaming-response---chatcompletionchunk)
  - [5.1 Chunk Top-level](#51-chunk-top-level)
  - [5.2 ChunkChoice Object](#52-chunkchoice-object)
  - [5.3 Delta Object](#53-delta-object)
  - [5.4 ToolCall Delta in Streaming](#54-toolcall-delta-in-streaming)
- [6. Auxiliary Endpoints](#6-auxiliary-endpoints)
- [7. Error Responses](#7-error-responses)

---

## 1. Overview

| Item | Value |
|---|---|
| HTTPS Base URL | `https://api.openai.com/v1/chat/completions` |
| Primary Endpoint | `POST /v1/chat/completions` |
| Authentication | `Authorization: Bearer <OPENAI_API_KEY>` |
| Content-Type | `application/json` |

### Two Transport Modes

| Dimension | Synchronous HTTPS | SSE Streaming |
|---|---|---|
| Trigger | Default | Add `stream: true` to request body |
| Response | Single `ChatCompletion` JSON | Multiple `ChatCompletionChunk`, ending with `data: [DONE]` |
| Best for | Simple Q&A, batch processing | Long text / tool call real-time display |

> Chat Completions API does **not have a WebSocket mode** (that capability is only in the Responses API).

---

## 2. HTTPS Request Body Top-level Parameters

`POST /v1/chat/completions` request body. If the Type column contains `object` or `array<object>`, refer to §3 for detailed sub-structures.

| Field | Type | Required? | Default | Nullable | Description |
|---|---|---|---|---|---|
| `model` | `string` | * | — | No | Model ID: `gpt-5.2`, `gpt-5.1`, `gpt-5`, `gpt-4.1`, `gpt-4o`, `gpt-4o-mini`, `o3`, `o4-mini`, `o1`, etc. |
| `messages` | `array<Message>` | * | — | No | Conversation history. **At least one required**. See §3.1 |
| `audio` | `object` (AudioParam) | No | `null` | Yes | Audio output parameters. **Only needed when `modalities` includes `"audio"`**. See §3.7 |
| `frequency_penalty` | `number` | No | `0` | Yes | `[-2, 2]`. Positive values reduce repetition |
| `logit_bias` | `object` (Dict[str,int]) | No | `null` | Yes | token id → bias value mapping, `[-100, 100]` |
| `logprobs` | `boolean` | No | `false` | Yes | Whether to return log probabilities |
| `top_logprobs` | `integer` | No | `null` | Yes | `[0, 20]`. **Must also set `logprobs:true`** |
| `max_completion_tokens` | `integer` | No | `null` (unlimited) | Yes | Output token limit, **includes reasoning tokens** |
| `max_tokens` | `integer` | No | `null` (unlimited) | Yes | **Deprecated**, use `max_completion_tokens`; incompatible with o-series |
| `metadata` | `object` (Dict[str,str]) | No | `null` | Yes | Up to 16 pairs, key ≤64, value ≤512 |
| `modalities` | `array<string>` | No | `["text"]` | Yes | Output modalities. Options: `"text"`, `"audio"`. E.g. `["text","audio"]` |
| `n` | `integer` | No | `1` | Yes | `[1, 128]`. Number of candidates generated (billed by total tokens; keeping 1 is recommended) |
| `parallel_tool_calls` | `boolean` | No | `true` | Yes | Whether to allow parallel tool calls |
| `prediction` | `object` (Prediction) | No | `null` | Yes | Predicted output acceleration, see §3.8 |
| `presence_penalty` | `number` | No | `0` | Yes | `[-2, 2]`. Positive values encourage new topics |
| `prompt_cache_key` | `string` | No | `null` | Yes | Improve prompt cache hit rate (replaces deprecated `user`) |
| `prompt_cache_retention` | `string` | No | `"in-memory"` | Yes | `"in-memory"` or `"24h"` |
| `reasoning_effort` | `string` | No | Model-dependent | Yes | `none`/`minimal`/`low`/`medium`/`high`/`xhigh`, **reasoning models only** |
| `response_format` | `object` (ResponseFormat) | No | `{"type":"text"}` | Yes | See §3.6 |
| `safety_identifier` | `string` | No | `null` | Yes | Stable user identifier (length ≤64, hashing recommended) |
| `seed` | `integer` | No | `null` | Yes | **Beta, deprecated**; best-effort deterministic output |
| `service_tier` | `string` | No | `"auto"` | Yes | `auto`, `default`, `flex`, `scale`, `priority` |
| `stop` | `string \| array<string>` | No | `null` | Yes | Up to 4 stop sequences; **not supported by o3/o4-mini** |
| `store` | `boolean` | No | `false` | Yes | Whether to retain output for distillation/evaluation |
| `stream` | `boolean` | No | `false` | Yes | `true` enables SSE |
| `stream_options` | `object` (StreamOptions) | No | `null` | Yes | Only used with `stream:true`, see §3.9 |
| `temperature` | `number` | No | `1` | Yes | `[0, 2]`. Do not use together with `top_p` |
| `tool_choice` | `string \| object` | No | `"auto"` (with tools) / `"none"` (without tools) | Yes | See §3.5 |
| `tools` | `array<Tool>` | No | `null` | Yes | See §3.4 |
| `top_p` | `number` | No | `1` | Yes | `[0, 1]`. Do not use together with `temperature` |
| `user` | `string` | No | `null` | Yes | **Deprecated**, use `safety_identifier` + `prompt_cache_key` |
| `verbosity` | `string` | No | `"medium"` | Yes | `"low"`/`"medium"`/`"high"` |
| `web_search_options` | `object` (WebSearchOptions) | No | `null` | Yes | Built-in web search; **only for `gpt-4o-search-preview` / `gpt-4o-mini-search-preview` series**. See §3.10 |
| `function_call` | `string \| object` | No | — | Yes | **Deprecated**, use `tool_choice`. See §3.11 |
| `functions` | `array<Function>` | No | — | Yes | **Deprecated**, use `tools`. See §3.11 |

---

## 3. Request Body Embedded Object Definitions

### 3.1 Message Object (6 Roles)

Elements of the `messages` array, distinguished by `role`.

#### (a) DeveloperMessage

```ts
DeveloperMessage {
  role: "developer";                                            // *
  content: string | TextContentPart[];                          // *
  name?: string | null;                                         // ?
}
```

| Field | Type | Required | Nullable | Description |
|---|---|---|---|---|
| `role` | `string` | * | No | Fixed `"developer"` |
| `content` | `string \| array<TextContentPart>` | * | No | String or content array containing only `text` type |
| `name` | `string` | ? | Yes | Optional participant name (for distinguishing multiple participants with the same role) |

> **o1 and later models** use `developer` instead of the original `system` role.

##### `TextContentPart` (used here, see §3.2)

```ts
{ type: "text"; text: string }
```

#### (b) SystemMessage

```ts
SystemMessage {
  role: "system";                                               // *
  content: string | TextContentPart[];                          // *
  name?: string | null;
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `role` | `string` | * | Fixed `"system"` |
| `content` | `string \| array<TextContentPart>` | * | Same as developer |
| `name` | `string` | ? | Optional |

> For **gpt-4 / gpt-4o / gpt-3.5** and other legacy models, use `system`; **o1 and later** use `developer`.

#### (c) UserMessage

```ts
UserMessage {
  role: "user";                                                 // *
  content: string | UserContentPart[];                          // *
  name?: string | null;
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `role` | `string` | * | Fixed `"user"` |
| `content` | `string \| array<UserContentPart>` | * | Must be an array for multimodal, see §3.2 |
| `name` | `string` | ? | Optional |

`UserContentPart` can be one of `text` / `image_url` / `input_audio` / `file`, see §3.2 for details.

#### (d) AssistantMessage (replay previous model output)

```ts
AssistantMessage {
  role: "assistant";                                            // *
  content?: string | AssistantContentPart[] | null;
  name?: string | null;
  refusal?: string | null;
  audio?: { id: string } | null;
  tool_calls?: ToolCall[] | null;
  function_call?: FunctionCallDeprecated | null;                // Deprecated
}
```

| Field | Type | Required | Nullable | Description |
|---|---|---|---|---|
| `role` | `string` | * | No | Fixed `"assistant"` |
| `content` | `string \| array \| null` | ? | Yes | **Can be null when `tool_calls` or `function_call` is present** |
| `name` | `string` | ? | Yes | Optional |
| `refusal` | `string` | ? | Yes | Refusal explanation from the previous model response |
| `audio` | `object` | ? | Yes | Reference to the previous audio response ID, see below |
| `tool_calls` | `array<ToolCall>` | ? | Yes | Tool calls generated by the model in the previous turn, see §3.3 |
| `function_call` | `object` | ? | Yes | **Deprecated**, see §3.11 |

##### `AssistantContentPart` — Two options

`text` (see §3.2.a) or `refusal`:

```ts
{ type: "refusal"; refusal: string }
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | `"refusal"` |
| `refusal` | `string` | * | Refusal explanation generated by the model |

##### `audio` sub-object (replay previous audio reference)

```ts
{ id: string }     // ID of the previous audio response
```

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | `string` | * | The previous `audio.id`, used for multi-turn audio conversations |

#### (e) ToolMessage (backfill tool call result)

```ts
ToolMessage {
  role: "tool";                                                 // *
  content: string | TextContentPart[];                          // *
  tool_call_id: string;                                         // *
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `role` | `string` | * | Fixed `"tool"` |
| `content` | `string \| array<TextContentPart>` | * | The result returned by the tool call |
| `tool_call_id` | `string` | * | Corresponding `tool_calls[*].id` |

#### (f) FunctionMessage (**Deprecated**)

```ts
FunctionMessage {
  role: "function";                                             // *
  name: string;                                                 // *
  content: string | null;                                       // *
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `role` | `string` | * | Fixed `"function"` |
| `name` | `string` | * | Function name |
| `content` | `string \| null` | * | Function return value |

> Used for legacy function calling. New code should use `ToolMessage`.

### 3.2 Content Part Object (5 Types)

Elements of the `messages[*].content` array; supported types vary by message role.

#### (a) TextContentPart

```ts
{
  type: "text";        // *
  text: string;        // *
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | `"text"` |
| `text` | `string` | * | Text content |

**Supported by all roles.**

#### (b) ImageContentPart (`image_url`)

```ts
{
  type: "image_url";   // *
  image_url: ImageURL; // *
}
ImageURL {
  url: string;                              // * URI or data: URL
  detail?: "auto" | "low" | "high" | null;
}
```

| Field | Type | Required | Values | Description |
|---|---|---|---|---|
| `type` | `string` | * | `"image_url"` | Fixed |
| `image_url` | `object` | * | — | Wrapper object |
| `image_url.url` | `string` | * | URI | Full URL or base64 data URL |
| `image_url.detail` | `string` | ? | `"auto"`/`"low"`/`"high"` | Default `"auto"` |

**Only supported by the `user` role.**

#### (c) AudioContentPart (`input_audio`)

```ts
{
  type: "input_audio";       // *
  input_audio: InputAudio;   // *
}
InputAudio {
  data: string;              // * base64-encoded audio
  format: "wav" | "mp3";     // *
}
```

| Field | Type | Required | Values | Description |
|---|---|---|---|---|
| `type` | `string` | * | `"input_audio"` | Fixed |
| `input_audio.data` | `string` | * | — | base64 |
| `input_audio.format` | `string` | * | `"wav"` / `"mp3"` | Currently only these two |

**Only supported by the `user` role, and requires `gpt-4o-audio-preview` or similar audio models.**

#### (d) FileContentPart (`file`)

```ts
{
  type: "file";        // *
  file: FileObj;       // *
}
FileObj {
  file_id?: string | null;
  file_data?: string | null;     // base64 file content
  filename?: string | null;
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | `"file"` |
| `file.file_id` | `string` | ? | Uploaded file ID (choose one) |
| `file.file_data` | `string` | ? | base64 file content (choose one) |
| `file.filename` | `string` | ? | Filename (required when `file_data` is used) |

**Only supported by the `user` role.**

#### (e) RefusalContentPart (`refusal`)

```ts
{
  type: "refusal";     // *
  refusal: string;     // *
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | `"refusal"` |
| `refusal` | `string` | * | Refusal explanation generated by the model |

**Only supported by the `assistant` role.**

### 3.3 ToolCall Object (Request Body, 2 Types)

Elements of the `AssistantMessage.tool_calls` array — i.e., tool calls generated by the model in the previous turn that you replay to the model.

#### (a) FunctionToolCall

```ts
FunctionToolCall {
  id: string;                  // *
  type: "function";            // *
  function: {
    name: string;              // *
    arguments: string;         // * JSON string
  };
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | `string` | * | Tool call ID; subsequent `ToolMessage.tool_call_id` must match |
| `type` | `string` | * | `"function"` |
| `function.name` | `string` | * | Name of the called function |
| `function.arguments` | `string` | * | JSON-serialized string. **The model may not generate valid JSON**; validate before use |

#### (b) CustomToolCall

```ts
CustomToolCall {
  id: string;                  // *
  type: "custom";              // *
  custom: {
    name: string;              // *
    input: string;             // *
  };
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | `string` | * | Tool call ID |
| `type` | `string` | * | `"custom"` |
| `custom.name` | `string` | * | Custom tool name |
| `custom.input` | `string` | * | Input generated by the model (arbitrary format) |

### 3.4 Tool Object (2 Types)

Elements of the `tools` array, telling the model which tools it can call.

#### (a) ChatCompletionFunctionTool

```ts
FunctionTool {
  type: "function";            // *
  function: FunctionDefinition;// *
}
FunctionDefinition {
  name: string;                // * 1-64 chars, a-z A-Z 0-9 _ -
  description?: string | null;
  parameters?: object | null;  // JSON Schema
  strict?: boolean | null;
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | `"function"` |
| `function.name` | `string` | * | Function name |
| `function.description` | `string` | ? | Function description; **critical for the model to decide whether to call it** |
| `function.parameters` | `object` | ? | JSON Schema; omit if no parameters |
| `function.strict` | `boolean` | ? | When `true`, strictly match schema; only a subset of JSON Schema is supported |

#### (b) ChatCompletionCustomTool

```ts
CustomTool {
  type: "custom";              // *
  custom: CustomToolDef;       // *
}
CustomToolDef {
  name: string;                // *
  description?: string | null;
  format?: CustomFormat | null;
}
```

##### `CustomFormat` — Two options

**(i) CustomFormatText** (default)

```ts
{ type: "text" }
```

**(ii) CustomFormatGrammar**

```ts
{
  type: "grammar";             // *
  grammar: {
    definition: string;        // * Grammar definition content
    syntax: "lark" | "regex";  // *
  };
}
```

| Field | Type | Required | Values | Description |
|---|---|---|---|---|
| `type` | `string` | * | `"grammar"` | Fixed |
| `grammar.definition` | `string` | * | — | Grammar definition text |
| `grammar.syntax` | `string` | * | `"lark"` or `"regex"` | Grammar style |

### 3.5 ToolChoice Object (5 Types)

`tool_choice` can be a string literal `"none" | "auto" | "required"`, or one of the following objects.

| Literal Value | Meaning |
|---|---|
| `"none"` | Do not call any tools, only generate messages (default when no tools are present) |
| `"auto"` | Model decides whether to call or not (default when tools are present) |
| `"required"` | Must call at least one tool |

#### (a) NamedFunctionToolChoice — Force a specific function call

```ts
{
  type: "function";          // *
  function: {
    name: string;            // *
  };
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | `"function"` |
| `function.name` | `string` | * | Function name |

#### (b) NamedCustomToolChoice — Force a specific custom tool call

```ts
{
  type: "custom";            // *
  custom: {
    name: string;            // *
  };
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | `"custom"` |
| `custom.name` | `string` | * | Custom tool name |

#### (c) AllowedToolsChoice — Restrict available tool set

```ts
{
  type: "allowed_tools";          // *
  allowed_tools: {
    mode: "auto" | "required";    // *
    tools: Array<{                // * Simplified tool definitions
      type: string;
      function?: { name: string };
      custom?: { name: string };
    }>;
  };
}
```

| Field | Type | Required | Values | Description |
|---|---|---|---|---|
| `type` | `string` | * | `"allowed_tools"` | Fixed |
| `allowed_tools.mode` | `string` | * | `"auto"` or `"required"` | Same as top-level literals |
| `allowed_tools.tools[]` | `array<object>` | * | — | E.g. `{ "type":"function", "function":{"name":"get_weather"} }` |

### 3.6 ResponseFormat Object (3 Types)

#### (a) ResponseFormatText — Default

```ts
{ type: "text" }
```

#### (b) ResponseFormatJSONSchema — Structured Output (recommended)

```ts
{
  type: "json_schema";              // *
  json_schema: JSONSchemaSpec;      // *
}
JSONSchemaSpec {
  name: string;                     // * 1-64 chars
  description?: string | null;
  schema?: object | null;           // JSON Schema
  strict?: boolean | null;
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | `"json_schema"` |
| `json_schema.name` | `string` | * | `^[a-zA-Z0-9_-]{1,64}$` |
| `json_schema.description` | `string` | ? | Hint for the model |
| `json_schema.schema` | `object` | ? | JSON Schema |
| `json_schema.strict` | `boolean` | ? | `true` for strict matching; only a subset of JSON Schema is supported |

> ⚠ Note: the schema location here is `response_format.json_schema.schema` (with an extra `json_schema` wrapper); **this differs from the Responses API where `text.format.schema` is placed directly on `format`**. This is the most common point of confusion between Chat Completions and the Responses API.

#### (c) ResponseFormatJSONObject — Legacy JSON Mode

```ts
{ type: "json_object" }
```

> Not recommended. You must explicitly instruct the model to output JSON in a system/user message, otherwise it may generate blank output indefinitely.

### 3.7 Audio Param Object

```ts
AudioParam {
  format: "wav" | "aac" | "mp3" | "flac" | "opus" | "pcm16";   // *
  voice: string;                                                // *
}
```

| Field | Type | Required | Values | Description |
|---|---|---|---|---|
| `format` | `string` | * | `wav`/`aac`/`mp3`/`flac`/`opus`/`pcm16` | Output audio format |
| `voice` | `string` | * | Built-in: `alloy`/`ash`/`ballad`/`coral`/`echo`/`fable`/`nova`/`onyx`/`sage`/`shimmer`/`marin`/`cedar` (custom strings also accepted) | Voice |

> This field is only needed when `modalities` includes `"audio"`, and the model must be from the `gpt-4o-audio-preview` series.

### 3.8 Prediction Object

```ts
Prediction {
  type: "content";                                              // *
  content: string | TextContentPart[];                          // *
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | Currently the only value: `"content"` |
| `content` | `string \| array<TextContentPart>` | * | Expected output text used for matching acceleration (e.g., the full source file content that will be slightly modified) |

> Used for "Predicted Outputs" acceleration: if the generated result largely matches this content, generation is significantly faster.

### 3.9 StreamOptions Object

```ts
StreamOptions {
  include_obfuscation?: boolean | null;
  include_usage?: boolean | null;
}
```

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `include_obfuscation` | `boolean` | ? | `true` | Adds a random `obfuscation` field in deltas to prevent side-channel attacks |
| `include_usage` | `boolean` | ? | `false` | When `true`, an extra chunk is sent before `[DONE]` with full token stats in `usage` and an empty `choices` array. **May not be received if the stream is interrupted** |

### 3.10 WebSearchOptions Object

```ts
WebSearchOptions {
  search_context_size?: "low" | "medium" | "high" | null;       // default medium
  user_location?: UserLocation | null;
}
UserLocation {
  type: "approximate";                                          // * must be approximate
  approximate: {
    city?: string | null;
    country?: string | null;
    region?: string | null;
    timezone?: string | null;
  };
}
```

| Field | Type | Required | Values | Description |
|---|---|---|---|---|
| `search_context_size` | `string` | ? | `low`/`medium`/`high` | Context space allocated for search |
| `user_location.type` | `string` | * | `"approximate"` | Fixed value |
| `user_location.approximate.city` | `string` | ? | — | Free-text city |
| `user_location.approximate.country` | `string` | ? | ISO 3166-1 two-letter | Country code, e.g. `"US"` |
| `user_location.approximate.region` | `string` | ? | — | Free-text province/state |
| `user_location.approximate.timezone` | `string` | ? | IANA | E.g. `"America/Los_Angeles"` |

> Note: the `web_search_options` field is **only valid for** `gpt-4o-search-preview` / `gpt-4o-mini-search-preview` series; for general models, use the Responses API's `web_search` tool.

### 3.11 Deprecated: Function and FunctionCall

#### `functions[*]` (deprecated, use `tools`)

```ts
FunctionDeprecated {
  name: string;                  // *
  description?: string | null;
  parameters?: object | null;
}
```

#### `function_call` (deprecated, use `tool_choice`)

Can be `"none" | "auto"`, or:

```ts
{ name: string }    // Force a specific function call
```

---

## 4. HTTPS Response Body (Non-streaming)

`POST /v1/chat/completions` with `stream:false` returns a `ChatCompletion` object.

### 4.1 ChatCompletion Top-level

```ts
ChatCompletion {
  id: string;                                                   // *
  object: "chat.completion";                                    // *
  created: number;                                              // *
  model: string;                                                // *
  choices: Choice[];                                            // *
  usage?: Usage | null;
  service_tier?: ServiceTier | null;
  system_fingerprint?: string | null;       // deprecated
}
```

| Field | Type | Always present? | Nullable | Description |
|---|---|---|---|---|
| `id` | `string` | ✅ | No | `chatcmpl-xxx` |
| `object` | `string` | ✅ | No | Fixed `"chat.completion"` |
| `created` | `integer` | ✅ | No | Unix seconds |
| `model` | `string` | ✅ | No | Actual model snapshot (e.g. `gpt-4.1-2025-04-14`) |
| `choices` | `array<Choice>` | ✅ | No | Candidate list (length 1 when `n=1`) |
| `usage` | `object` | Usually | ✅ | Token usage; may be null on failure or early cancellation |
| `service_tier` | `string` | Conditional | ✅ | The **actual** tier used |
| `system_fingerprint` | `string` | Conditional | ✅ | **Deprecated**; previously used with `seed` to detect backend changes |

### 4.2 Choice Object

```ts
Choice {
  index: number;                                                // *
  message: ChatCompletionMessage;                               // *
  finish_reason: FinishReason;                                  // *
  logprobs: ChoiceLogprobs | null;                              // *(may be null)
}
type FinishReason =
  | "stop"           // Natural end / hit stop sequence
  | "length"         // Reached max_completion_tokens limit
  | "content_filter" // Blocked by content filter
  | "tool_calls"     // Model chose to call tools
  | "function_call"; // Deprecated legacy function call
```

| Field | Type | Always present? | Nullable | Values / Description |
|---|---|---|---|---|
| `index` | `integer` | ✅ | No | Index in the `choices` array |
| `message` | `object` | ✅ | No | See §4.3 |
| `finish_reason` | `string` | ✅ | No | 5 options |
| `logprobs` | `object \| null` | ✅ | ✅ | Non-null only when `logprobs:true` is requested; see §4.7 |

### 4.3 ChatCompletionMessage (Assistant Message in Response)

```ts
ChatCompletionMessage {
  role: "assistant";                                            // *
  content: string | null;                                       // *
  refusal: string | null;                                       // *
  annotations?: Annotation[] | null;
  audio?: ChatCompletionAudio | null;
  tool_calls?: ToolCall[] | null;
  function_call?: { name: string; arguments: string } | null;   // Deprecated
}
```

| Field | Type | Always present? | Nullable | Description |
|---|---|---|---|---|
| `role` | `string` | ✅ | No | Fixed `"assistant"` |
| `content` | `string \| null` | ✅ | ✅ | **Typically `null` when tool calls are present**; may also be null on refusal |
| `refusal` | `string \| null` | ✅ | ✅ | Explanation on refusal; otherwise null |
| `annotations` | `array` | ? | Yes | e.g., URL citations attached during web search, see §4.5 |
| `audio` | `object` | ? | Yes | When `modalities` includes `audio`, see §4.6 |
| `tool_calls` | `array` | ? | Yes | When the model chooses to call tools, see §4.4 |
| `function_call` | `object` | ? | Yes | **Deprecated**; `{name, arguments}` |

### 4.4 ToolCall Object (Response, 2 Types)

#### (a) FunctionToolCall

```ts
{
  id: string;                  // *
  type: "function";            // *
  function: {
    name: string;              // *
    arguments: string;         // * JSON string
  };
}
```

| Field | Type | Always present? | Description |
|---|---|---|---|
| `id` | `string` | ✅ | e.g., `call_abc123` |
| `type` | `string` | ✅ | `"function"` |
| `function.name` | `string` | ✅ | Function name |
| `function.arguments` | `string` | ✅ | JSON string. **Model may generate invalid JSON or hallucinated fields**; must validate first |

#### (b) CustomToolCall

```ts
{
  id: string;                  // *
  type: "custom";              // *
  custom: {
    name: string;              // *
    input: string;             // *
  };
}
```

| Field | Type | Always present? | Description |
|---|---|---|---|
| `id` | `string` | ✅ | Tool call ID |
| `type` | `string` | ✅ | `"custom"` |
| `custom.name` | `string` | ✅ | Tool name |
| `custom.input` | `string` | ✅ | Raw input generated by the model |

### 4.5 Annotation Object

Currently Chat Completions has only one annotation type:

```ts
{
  type: "url_citation";                                         // *
  url_citation: {
    url: string;                                                // *
    title: string;                                              // *
    start_index: number;                                        // *
    end_index: number;                                          // *
  };
}
```

| Field | Type | Always present? | Description |
|---|---|---|---|
| `type` | `string` | ✅ | `"url_citation"` |
| `url_citation.url` | `string` | ✅ | Web page URL |
| `url_citation.title` | `string` | ✅ | Web page title |
| `url_citation.start_index` | `integer` | ✅ | Start character index in the message |
| `url_citation.end_index` | `integer` | ✅ | End character index |

> Only appears when using web search preview models / `web_search_options`.

### 4.6 Audio Response Object

```ts
ChatCompletionAudio {
  id: string;            // *
  data: string;          // * base64-encoded audio
  expires_at: number;    // * Unix seconds
  transcript: string;    // *
}
```

| Field | Type | Always present? | Description |
|---|---|---|---|
| `id` | `string` | ✅ | Unique ID; can be used as `assistant.audio.id` reference in the next turn |
| `data` | `string` | ✅ | base64; format matches the request `audio.format` |
| `expires_at` | `integer` | ✅ | Audio expiry time on server (Unix seconds) |
| `transcript` | `string` | ✅ | Audio transcript text |

> This field only appears when `modalities` includes `audio`.

### 4.7 Logprobs Object

```ts
ChoiceLogprobs {
  content: TokenLogprob[] | null;
  refusal: TokenLogprob[] | null;
}
TokenLogprob {
  token: string;                                                // *
  bytes: number[] | null;                                       // *(may be null)
  logprob: number;                                              // *
  top_logprobs: TopLogprob[];                                   // *
}
TopLogprob {
  token: string;                                                // *
  bytes: number[] | null;                                       // *
  logprob: number;                                              // *
}
```

| Field | Type | Always present? | Nullable | Description |
|---|---|---|---|---|
| `content` | `array \| null` | ✅ | ✅ | Logprob list for content tokens |
| `refusal` | `array \| null` | ✅ | ✅ | Logprob list for refusal tokens |
| `TokenLogprob.token` | `string` | ✅ | No | Token text |
| `TokenLogprob.bytes` | `array<int> \| null` | ✅ | ✅ | UTF-8 bytes; null when not representable |
| `TokenLogprob.logprob` | `number` | ✅ | No | Log probability; `-9999.0` when not in top-20 |
| `TokenLogprob.top_logprobs` | `array` | ✅ | No | The N most likely tokens at this position (N determined by `top_logprobs` in request) |

### 4.8 Usage Object

```ts
Usage {
  prompt_tokens: number;                                        // *
  completion_tokens: number;                                    // *
  total_tokens: number;                                         // *
  prompt_tokens_details?: PromptTokensDetails | null;
  completion_tokens_details?: CompletionTokensDetails | null;
}
PromptTokensDetails {
  cached_tokens?: number | null;
  audio_tokens?: number | null;
}
CompletionTokensDetails {
  reasoning_tokens?: number | null;
  audio_tokens?: number | null;
  accepted_prediction_tokens?: number | null;
  rejected_prediction_tokens?: number | null;
}
```

| Field | Type | Always present? | Nullable | Description |
|---|---|---|---|---|
| `prompt_tokens` | `integer` | ✅ | No | Number of input tokens |
| `completion_tokens` | `integer` | ✅ | No | Number of output tokens (**including reasoning**) |
| `total_tokens` | `integer` | ✅ | No | Total |
| `prompt_tokens_details.cached_tokens` | `integer` | ? | ✅ | Input tokens that hit the cache |
| `prompt_tokens_details.audio_tokens` | `integer` | ? | ✅ | Audio tokens in input |
| `completion_tokens_details.reasoning_tokens` | `integer` | ? | ✅ | Tokens used for thinking by reasoning models |
| `completion_tokens_details.audio_tokens` | `integer` | ? | ✅ | Output audio tokens |
| `completion_tokens_details.accepted_prediction_tokens` | `integer` | ? | ✅ | Tokens matched in Predicted Outputs |
| `completion_tokens_details.rejected_prediction_tokens` | `integer` | ? | ✅ | Tokens NOT matched in Predicted Outputs but still billed |

> ⚠ When `status` is `failed` or cancelled very early, top-level `usage` may be `null`.

---

## 5. SSE Streaming Response - ChatCompletionChunk

Add `stream:true` to the request body; response header `Content-Type: text/event-stream`. Wire format for each event:

```
data: <JSON>

```

The final one is always:

```
data: [DONE]

```

### 5.1 Chunk Top-level

```ts
ChatCompletionChunk {
  id: string;                                                   // *
  object: "chat.completion.chunk";                              // *
  created: number;                                              // *
  model: string;                                                // *
  choices: ChunkChoice[];                                       // *
  usage?: Usage | null;
  service_tier?: ServiceTier | null;
  system_fingerprint?: string | null;       // Deprecated
}
```

| Field | Type | Always present? | Nullable | Description |
|---|---|---|---|---|
| `id` | `string` | ✅ | No | All chunks in the same request share one ID |
| `object` | `string` | ✅ | No | Fixed `"chat.completion.chunk"` |
| `created` | `integer` | ✅ | No | Unix seconds; all chunks have the same timestamp |
| `model` | `string` | ✅ | No | Model snapshot |
| `choices` | `array` | ✅ | No | **May be empty array** (the final chunk carrying usage is empty) |
| `usage` | `object \| null` | ? | ✅ | **Only appears when `stream_options.include_usage:true` is set in the request**; and **only in the final chunk** carries a complete value; `null` in all other chunks |
| `service_tier` | `string` | ? | ✅ | Actual tier used |
| `system_fingerprint` | `string` | ? | ✅ | Deprecated |

### 5.2 ChunkChoice Object

```ts
ChunkChoice {
  index: number;                                                // *
  delta: Delta;                                                 // *
  finish_reason: FinishReason | null;                           // *(Null in intermediate chunks; specific value in the final chunk)
  logprobs?: ChoiceLogprobs | null;
}
```

| Field | Type | Always present? | Nullable | Description |
|---|---|---|---|---|
| `index` | `integer` | ✅ | No | Choice index |
| `delta` | `object` | ✅ | No | Incremental content, see §5.3 |
| `finish_reason` | `string \| null` | ✅ | ✅ | Null in intermediate chunks; specific value in the final chunk |
| `logprobs` | `object` | ? | ✅ | Only appears when logprobs is requested |

### 5.3 Delta Object

```ts
Delta {
  role?: "developer"|"system"|"user"|"assistant"|"tool" | null; // Usually only in the first chunk
  content?: string | null;
  refusal?: string | null;
  tool_calls?: DeltaToolCall[] | null;
  function_call?: { name?: string; arguments?: string } | null; // Deprecated
}
```

| Field | Type | Always present? | Nullable | Description |
|---|---|---|---|---|
| `role` | `string` | ? | ✅ | **Usually only in the first chunk**,absent in subsequent chunks |
| `content` | `string` | ? | ✅ | Text increment. **Final chunk may be an empty object** |
| `refusal` | `string` | ? | ✅ | Refusal increment |
| `tool_calls` | `array` | ? | ✅ | Tool call increment, see §5.4 |
| `function_call` | `object` | ? | ✅ | **Deprecated** |

> ⚠ **Common pattern**: the first chunk delta is usually `{ "role": "assistant", "content": "" }`, intermediate ones are `{ "content": "..." }`, and the final is `{}` (empty object).

### 5.4 ToolCall Delta in Streaming

```ts
DeltaToolCall {
  index: number;                                                // * Required
  id?: string | null;
  type?: "function" | null;
  function?: {
    name?: string | null;
    arguments?: string | null;     // JSON string "increment"; must be accumulated and concatenated
  } | null;
}
```

| Field | Type | Always present? | Nullable | Description |
|---|---|---|---|---|
| `index` | `integer` | ✅ | No | Index of the tool call in the `tool_calls` array |
| `id` | `string` | ? | ✅ | Usually only given when this index first appears |
| `type` | `string` | ? | ✅ | Usually only on first appearance |
| `function.name` | `string` | ? | ✅ | Usually only on first appearance |
| `function.arguments` | `string` | ? | ✅ | **JSON string increment**; client must accumulate by index to obtain complete arguments |

#### Streaming tool_calls processing pattern (pseudocode)

```python
acc = {}   # index → assembled {id, name, arguments}
for chunk in stream:
    for tc in chunk.choices[0].delta.tool_calls or []:
        slot = acc.setdefault(tc.index, {"arguments": ""})
        if tc.id:                     slot["id"]   = tc.id
        if tc.function and tc.function.name:
            slot["name"] = tc.function.name
        if tc.function and tc.function.arguments:
            slot["arguments"] += tc.function.arguments
    if chunk.choices[0].finish_reason == "tool_calls":
        # at this point acc[i].arguments is the complete JSON string
        ...
```

---

## 6. Auxiliary Endpoints

When `store: true` is set in the request, the response is saved and can be managed through the following endpoints.

### 6.1 Retrieve a completion

```
GET /v1/chat/completions/{completion_id}
```

Returns: `ChatCompletion` object (§4.1).

### 6.2 Update completion metadata

```
POST /v1/chat/completions/{completion_id}
```

Request body:

```ts
{ metadata: { [k:string]:string } }
```

Returns: updated `ChatCompletion` object.

### 6.3 Delete completion

```
DELETE /v1/chat/completions/{completion_id}
```

Returns `DeletedChatCompletion`:

```ts
{
  id: string;                  // *
  object: "chat.completion.deleted";  // *
  deleted: true;               // *
}
```

### 6.4 List completions

```
GET /v1/chat/completions
```

Pagination/filter query parameters:

| Parameter | Type | Description |
|---|---|---|
| `model` | `string` | Filter by model |
| `metadata` | `object` | Filter by metadata (URL-encoded) |
| `after` | `string` | Last ID from the previous page |
| `limit` | `integer` | 1-100, default 20 |
| `order` | `string` | `asc`/`desc` (by `created_at`) |

Returns `ListResponse`:

```ts
{
  object: "list";              // *
  data: ChatCompletion[];      // *
  first_id: string | null;
  last_id: string | null;
  has_more: boolean;           // *
}
```

### 6.5 List messages for a completion

```
GET /v1/chat/completions/{completion_id}/messages
```

Returns `ListResponse`; `data` contains the list of messages (input messages + assistant message).

---

## 7. Error Responses

Non-streaming errors are returned as HTTP error codes + JSON:

```json
{
  "error": {
    "message": "...",
    "type": "invalid_request_error",
    "param": "model",
    "code": "model_not_found"
  }
}
```

#### `ErrorObject` Full Definition

```ts
ErrorObject {
  message: string;          // *
  type: string;             // *
  code?: string | null;
  param?: string | null;
}
```

| Field | Always present? | Nullable | Values |
|---|---|---|---|
| `message` | ✅ | No | Human-readable |
| `type` | ✅ | No | `invalid_request_error`/`authentication_error`/`rate_limit_error`/`server_error`/`api_error` |
| `code` | ❌ | ✅ | Specific code |
| `param` | ❌ | ✅ | Name of the field with the issue |

#### Common error codes

| code | Meaning |
|---|---|
| `invalid_request_error` | Request format error |
| `model_not_found` | Model unavailable or not enabled |
| `context_length_exceeded` | Context length exceeded |
| `rate_limit_exceeded` | Rate/quota exceeded |
| `insufficient_quota` | Insufficient balance |
| `content_filter` | Content filter blocked |
| `invalid_image` / `image_parse_error` | Image processing failed |

#### Streaming errors

If an error occurs during streaming, it is delivered as a `data: { "error": {...} }` event, then the connection is closed (no `[DONE]` will follow).

#### Important Response Headers

| Header | Description |
|---|---|
| `x-request-id` | Request ID; provide to OpenAI for troubleshooting |
| `openai-organization` | Organization ID |
| `openai-processing-ms` | Server-side processing time |
| `openai-version` | API version date |
| `x-ratelimit-limit-requests` / `-tokens` | Rate limits |
| `x-ratelimit-remaining-requests` / `-tokens` | Remaining quota |
| `x-ratelimit-reset-requests` / `-tokens` | Reset time |

---

## Appendix: Minimal Working Templates

### A. Simplest request

```json
{
  "model": "gpt-4o-mini",
  "messages": [
    { "role": "user", "content": "Hello" }
  ]
}
```

### B. Streaming response

```json
{
  "model": "gpt-4o-mini",
  "messages": [
    { "role": "developer", "content": "You are a helpful assistant." },
    { "role": "user", "content": "Hello" }
  ],
  "stream": true,
  "stream_options": { "include_usage": true }
}
```

### C. Vision input

```json
{
  "model": "gpt-4o",
  "messages": [{
    "role": "user",
    "content": [
      { "type": "text", "text": "What's in this image?" },
      { "type": "image_url",
        "image_url": { "url": "https://example.com/cat.jpg", "detail": "high" } }
    ]
  }],
  "max_completion_tokens": 300
}
```

### D. Function calling (Tools)

```json
{
  "model": "gpt-4o-mini",
  "messages": [
    { "role": "user", "content": "What's the weather in Boston?" }
  ],
  "tools": [{
    "type": "function",
    "function": {
      "name": "get_current_weather",
      "description": "Get the current weather in a given location",
      "parameters": {
        "type": "object",
        "properties": {
          "location": { "type": "string" },
          "unit": { "type": "string", "enum": ["celsius", "fahrenheit"] }
        },
        "required": ["location"]
      },
      "strict": true
    }
  }],
  "tool_choice": "auto",
  "parallel_tool_calls": true
}
```

### E. Function calling — backfill results and continue conversation

```json
{
  "model": "gpt-4o-mini",
  "messages": [
    { "role": "user", "content": "What's the weather in Boston?" },
    {
      "role": "assistant",
      "content": null,
      "tool_calls": [{
        "id": "call_abc123",
        "type": "function",
        "function": {
          "name": "get_current_weather",
          "arguments": "{\"location\":\"Boston, MA\"}"
        }
      }]
    },
    {
      "role": "tool",
      "tool_call_id": "call_abc123",
      "content": "{\"temp\":22,\"unit\":\"celsius\",\"condition\":\"sunny\"}"
    }
  ],
  "tools": [ /* same as above */ ]
}
```

### F. Structured output (JSON Schema)

```json
{
  "model": "gpt-4o-mini",
  "messages": [
    { "role": "user", "content": "Extract: John, 30, NYC" }
  ],
  "response_format": {
    "type": "json_schema",
    "json_schema": {
      "name": "person",
      "strict": true,
      "schema": {
        "type": "object",
        "properties": {
          "name": { "type": "string" },
          "age":  { "type": "integer" },
          "city": { "type": "string" }
        },
        "required": ["name", "age", "city"],
        "additionalProperties": false
      }
    }
  }
}
```

### G. Audio output

```json
{
  "model": "gpt-4o-audio-preview",
  "modalities": ["text", "audio"],
  "audio": { "voice": "alloy", "format": "wav" },
  "messages": [
    { "role": "user", "content": "Say hi in Japanese." }
  ]
}
```

### H. Reasoning model

```json
{
  "model": "o3",
  "messages": [
    { "role": "user", "content": "Solve: 2x + 5 = 17" }
  ],
  "reasoning_effort": "medium",
  "max_completion_tokens": 4096
}
```

---

## Appendix B: Quick Comparison with Responses API

| Dimension | Chat Completions | Responses API |
|---|---|---|
| Endpoint | `/v1/chat/completions` | `/v1/responses` |
| Input field | `messages` | `input` (also supports strings) |
| System instructions | via `system`/`developer` message | `instructions` field |
| Multi-turn | Maintain complete `messages[]` yourself | `previous_response_id` or `conversation` |
| Built-in tools | Only web search preview models support `web_search_options` | Multiple built-in tools (file_search, web_search, computer_use, code_interpreter, image_generation, MCP) |
| reasoning encrypted | ❌ Not supported | ✅ `include: ["reasoning.encrypted_content"]` |
| Structured output | `response_format.json_schema.schema` | `text.format.schema` (one fewer wrapper layer) |
| Streaming | SSE (`ChatCompletionChunk`) | SSE semantic events |
| WebSocket | ❌ | ✅ |
| Background mode | ❌ | ✅ `background:true` |
| Persistence | `store:true`, default `false` | `store:true`, default `true` |
| Output body | `choices[*].message.content` | `output[*]` array (can contain reasoning, tool_call, message, etc.) |

---

*End of document.*

