# OpenAI Responses API Complete Reference (with All Object Types Expanded)

> **Source**: OpenAI Official API Reference (developers.openai.com/api/reference)
> **Compiled**: May 2026
> **Scope**: HTTPS (sync + SSE) and WebSocket, three transport modes
> **Document conventions**: Every `object` type field is independently expanded below with its full sub-fields; `*` indicates a required field; `?` indicates an optional field; `| null` indicates the field may be `null`.

---

## Table of Contents

- [1. Overview](#1-overview)
- [2. HTTPS Request Body Top-level Parameters](#2-https-request-body-top-level-parameters)
- [3. Request Body Embedded Object Complete Definitions](#3-request-body-embedded-object-complete-definitions)
  - [3.1 Conversation Object](#31-conversation-object)
  - [3.2 Prompt Object](#32-prompt-object)
  - [3.3 ContextManagement Item Object](#33-contextmanagement-item-object)
  - [3.4 StreamOptions Object](#34-streamoptions-object)
  - [3.5 Reasoning Object](#35-reasoning-object)
  - [3.6 Text Object and Format Subtypes](#36-text-object-and-format-subtypes)
  - [3.7 ToolChoice Object (7 types)](#37-toolchoice-object-7-types)
  - [3.8 Tool Object (13+ types)](#38-tool-object-13-types)
  - [3.9 Input Item Object (20+ types)](#39-input-item-object-20-types)
  - [3.10 Content Part Object](#310-content-part-object)
- [4. HTTPS Response Body](#4-https-response-body)
  - [4.1 Response Object Top-level](#41-response-object-top-level)
  - [4.2 Output Item Object (20+ types)](#42-output-item-object-20-types)
  - [4.3 Annotation Object (4 types)](#43-annotation-object-4-types)
  - [4.4 Usage Object](#44-usage-object)
  - [4.5 Error / IncompleteDetails Object](#45-error--incompletedetails-object)
- [5. SSE Stream Events — All Event Payloads Expanded](#5-sse-stream-events--all-event-payloads-expanded)
- [6. WebSocket Mode](#6-websocket-mode)
- [7. Auxiliary Endpoints](#7-auxiliary-endpoints)
- [8. Error Responses](#8-error-responses)

---

## 1. Overview

| Item | Value |
|---|---|
| HTTPS Base URL | `https://api.openai.com/v1/responses` |
| WebSocket URL | `wss://api.openai.com/v1/responses` |
| Primary Endpoint | `POST /v1/responses` to create; `GET /v1/responses/{id}` to retrieve |
| Authentication | `Authorization: Bearer <OPENAI_API_KEY>` |
| Content-Type | `application/json` |

### Comparison of the Three Modes

| Dimension | Sync HTTPS | SSE Streaming | WebSocket |
|---|---|---|---|
| Trigger | Default | Add `stream: true` to request body | `wss://` + `response.create` event |
| Response | Single JSON | `text/event-stream` incremental | Bidirectional JSON frame stream |
| Best for | Short tasks | Real-time display of long outputs | Multi-turn, tool-call-intensive agents |
| Connection Lifetime | Single use | Single use | Single connection, 60 minutes, serial execution |

---

## 2. HTTPS Request Body Top-level Parameters

`POST /v1/responses` request body structure. In the table below, if the "Type" column contains `object` or `array<object>`, refer to §3 of this document for the detailed sub-structure.

| Field | Type | Required? | Default | Nullable | Description / Allowed Values |
|---|---|---|---|---|---|
| `model` | `string` | Recommended | — | No | Model ID: `gpt-5.2`, `gpt-5.1`, `gpt-5`, `gpt-5-mini`, `gpt-5-nano`, `gpt-4.1`, `gpt-4o`, `o3`, `o4-mini`, `o1`, `computer-use-preview`, etc. |
| `input` | `string \| array<InputItem>` | Effectively required | — | No | See §3.9. At least one of `input` or `prompt` must be provided |
| `prompt` | `object` (PromptObject) | No | `null` | Yes | See §3.2 |
| `instructions` | `string` | No | `null` | Yes | System/developer instructions; **not automatically inherited from the previous turn when using `previous_response_id`** |
| `conversation` | `string \| object` (ConversationObject) | No | `null` | Yes | See §3.1; **mutually exclusive with `previous_response_id`** |
| `previous_response_id` | `string` | No | `null` | Yes | Previous response ID, used for multi-turn; **mutually exclusive with `conversation`** |
| `background` | `boolean` | No | `false` | Yes | `true` runs in background; must retrieve or use webhook to get results |
| `stream` | `boolean` | No | `false` | Yes | `true` enables SSE |
| `stream_options` | `object` (StreamOptions) | No | `null` | Yes | See §3.4 |
| `store` | `boolean` | No | `true` | Yes | Whether to persist the response |
| `include` | `array<string>` | No | `null` | Yes | See table below |
| `metadata` | `object` (Dict[str,str]) | No | `null` | Yes | Up to 16 pairs, key ≤64, value ≤512 |
| `max_output_tokens` | `integer` | No | `null` (unlimited) | Yes | **Includes reasoning tokens** |
| `max_tool_calls` | `integer` | No | `null` (unlimited) | Yes | Maximum total tool call count |
| `parallel_tool_calls` | `boolean` | No | `true` | Yes | Whether parallel tool calls are allowed |
| `temperature` | `number` | No | `1` | Yes | `[0, 2]`; most reasoning models ignore this |
| `top_p` | `number` | No | `1` | Yes | `(0, 1]`; do not use simultaneously with `temperature` |
| `tools` | `array<object>` (Tool) | No | `null` | Yes | See §3.8 |
| `tool_choice` | `string \| object` (ToolChoice) | No | `"auto"` | Yes | See §3.7 |
| `text` | `object` (TextConfig) | No | `{"format":{"type":"text"}}` | Yes | See §3.6 |
| `reasoning` | `object` (Reasoning) | No | `null` | Yes | **Only for gpt-5 / o-series**; see §3.5 |
| `verbosity` | `string` | No | `"medium"` | Yes | `"low"`, `"medium"`, `"high"` |
| `service_tier` | `string` | No | `"auto"` | Yes | `auto`, `default`, `flex`, `scale`, `priority` |
| `safety_identifier` | `string` | No | `null` | Yes | Stable user identifier (hashing recommended) |
| `prompt_cache_key` | `string` | No | `null` | Yes | Improves prompt cache hit rate (replaces the old `user`) |
| `prompt_cache_retention` | `string` | No | `"in-memory"` | Yes | `"in-memory"` or `"24h"` |
| `context_management` | `array<object>` (ContextManagementItem) | No | `null` | Yes | See §3.3 |

#### `include` Field Allowed Values (`array<string>`)

| Value | When enabled, the response will additionally include |
|---|---|
| `"web_search_call.action.sources"` | `action.sources` of the web search item |
| `"web_search_call.results"` | `results` of the web search item |
| `"code_interpreter_call.outputs"` | Code interpreter `outputs` |
| `"computer_call_output.output.image_url"` | Computer use screenshot URL |
| `"file_search_call.results"` | File search `results` |
| `"message.input_image.image_url"` | Input image URL |
| `"message.output_text.logprobs"` | Output text logprobs |
| `"reasoning.encrypted_content"` | Encrypted reasoning content (required for cross-turn use when `store: false`) |

---

## 3. Request Body Embedded Object Complete Definitions

### 3.1 Conversation Object

The `conversation` field can be a `string` (an ID literal) or the following object:

```ts
ConversationObject {
  id: string;   // * Required, conversation ID, e.g. "conv_xyz"
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | `string` | * | Unique conversation ID |

### 3.2 Prompt Object

References a saved prompt template.

```ts
PromptObject {
  id: string;                                          // *
  version?: string | null;                             // ?
  variables?: { [key: string]: VariableValue } | null; // ?
}
```

| Field | Type | Required | Nullable | Description |
|---|---|---|---|---|
| `id` | `string` | * | No | Unique ID of the prompt template |
| `version` | `string` | ? | Yes | Optional version number |
| `variables` | `object` | ? | Yes | Key→value mapping; value types see below |

#### `VariableValue` (prompt variable value)

Can be a string or any input content type (`input_text` / `input_image` / `input_file`, defined in §3.10).

```ts
type VariableValue = string | InputText | InputImage | InputFile;
```

### 3.3 ContextManagement Item Object

`context_management` is an array; each item currently supports only one type — "compaction":

```ts
ContextManagementItem {
  type: "compaction";          // *
  compact_threshold?: number;  // ?
}
```

| Field | Type | Required | Allowed Values | Description |
|---|---|---|---|---|
| `type` | `string` | * | Only `"compaction"` | Currently the only supported type |
| `compact_threshold` | `integer` | ? | `≥ 1000` | Token count at which auto-compaction is triggered |

### 3.4 StreamOptions Object

```ts
StreamOptions {
  include_obfuscation?: boolean;  // Default true
}
```

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `include_obfuscation` | `boolean` | ? | `true` | Adds a random `obfuscation` field to prevent side-channel attacks. If you trust the network, set to `false` to save bandwidth |

### 3.5 Reasoning Object

```ts
Reasoning {
  effort?: "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | null;
  summary?: "auto" | "concise" | "detailed" | null;
  generate_summary?: "auto" | "concise" | "detailed" | null;  // Deprecated, use summary
}
```

| Field | Type | Required | Allowed Values | Default | Description |
|---|---|---|---|---|---|
| `effort` | `string \| null` | ? | `none`/`minimal`/`low`/`medium`/`high`/`xhigh` | gpt-5.1 defaults to `none`; earlier models default to `medium`; `gpt-5-pro` forces `high`; `xhigh` only available on `gpt-5.1-codex-max` and later | Reasoning effort level |
| `summary` | `string \| null` | ? | `auto`/`concise`/`detailed` | `null` | Reasoning summary verbosity |
| `generate_summary` | `string \| null` | ? | Same as `summary` | — | **Deprecated**, use `summary` instead |

### 3.6 Text Object and Format Subtypes

```ts
TextConfig {
  format?: TextFormat;   // Pick one of three, see below
}
```

#### `TextFormat` Subtypes (3 types)

**(a) TextFormatText** — Default

```ts
{ type: "text" }
```

**(b) TextFormatJSONSchema** — Structured Output (recommended)

```ts
{
  type: "json_schema";    // *
  name: string;           // * Max length ≤64, a-z A-Z 0-9 _ -
  schema: object;         // * JSON Schema
  strict?: boolean;       // ? Recommended true
  description?: string;   // ?
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | Must be `"json_schema"` |
| `name` | `string` | * | Name, `^[a-zA-Z0-9_-]{1,64}$` |
| `schema` | `object` | * | JSON Schema object |
| `strict` | `boolean` | ? | When `true`, strict matching; only a subset of JSON Schema is allowed |
| `description` | `string` | ? | Hint for the model |

**(c) TextFormatJSONObject** — Legacy JSON Mode

```ts
{ type: "json_object" }
```

### 3.7 ToolChoice Object (7 types)

`tool_choice` can be a string literal `"none" | "auto" | "required"`, or one of the following seven object types.

#### (a) ToolChoiceAllowed — Restrict the set of available tools

```ts
{
  type: "allowed_tools";              // *
  mode: "auto" | "required";          // *
  tools: Array<{ type: string; ... }>;// * Simplified tool definition list
}
```

| Field | Type | Required | Allowed Values | Description |
|---|---|---|---|---|
| `type` | `string` | * | `"allowed_tools"` | Fixed |
| `mode` | `string` | * | `"auto"` or `"required"` | auto allows the model to choose 0 or more; required demands at least 1 |
| `tools` | `array<object>` | * | — | E.g. `{ "type":"function", "name":"get_weather" }` or `{ "type":"mcp", "server_label":"..." }` |

#### (b) ToolChoiceTypes — Force use of a built-in tool

```ts
{ type: "file_search" | "web_search_preview" | "web_search_preview_2025_03_11"
      | "computer_use_preview" | "image_generation" | "code_interpreter" }
```

| Field | Type | Required | Allowed Values |
|---|---|---|---|
| `type` | `string` | * | `file_search`, `web_search_preview`, `web_search_preview_2025_03_11`, `computer_use_preview`, `image_generation`, `code_interpreter` |

#### (c) ToolChoiceFunction

```ts
{ type: "function"; name: string; }
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | `"function"` |
| `name` | `string` | * | Function name |

#### (d) ToolChoiceMcp

```ts
{
  type: "mcp";          // *
  server_label: string; // *
  name?: string;        // ?
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | `"mcp"` |
| `server_label` | `string` | * | MCP server label |
| `name` | `string` | ? | Specific tool name |

#### (e) ToolChoiceCustom

```ts
{ type: "custom"; name: string; }
```

#### (f) ToolChoiceApplyPatch

```ts
{ type: "apply_patch" }
```

#### (g) ToolChoiceShell

```ts
{ type: "shell" }
```

### 3.8 Tool Object (13+ types)

Each element in the `tools` array is differentiated by `type`.

#### (a) FunctionTool

```ts
FunctionTool {
  type: "function";              // *
  name: string;                  // *
  description?: string;          // ?
  parameters?: object | null;    // ? JSON Schema
  strict?: boolean | null;       // ? Default true
  defer_loading?: boolean | null;// ?
}
```

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `type` | `string` | * | — | `"function"` |
| `name` | `string` | * | — | Function name, max 128, `a-z A-Z 0-9 _ -` |
| `description` | `string` | ? | `null` | Description for the model |
| `parameters` | `object` | ? | `null` | JSON Schema |
| `strict` | `boolean` | ? | `true` | Strict parameter validation |
| `defer_loading` | `boolean` | ? | `false` | Whether to defer loading via tool search |

#### (b) FileSearchTool

```ts
FileSearchTool {
  type: "file_search";              // *
  vector_store_ids: string[];       // *
  max_num_results?: number | null;  // ? 1-50
  filters?: Filters | null;
  ranking_options?: RankingOptions | null;
}
```

##### `Filters` Subtypes — Pick one of two

**ComparisonFilter**

```ts
{
  key: string;                                              // *
  type: "eq"|"ne"|"gt"|"gte"|"lt"|"lte"|"in"|"nin";        // *
  value: string | number | boolean | Array<string|number>;  // *
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `key` | `string` | * | Attribute name |
| `type` | `string` | * | 8 comparison operators |
| `value` | `string \| number \| boolean \| array` | * | Comparison value |

**CompoundFilter**

```ts
{
  type: "and" | "or";              // *
  filters: Filter[];                // * Can nest Comparison or Compound inside
}
```

##### `RankingOptions`

```ts
RankingOptions {
  ranker?: "auto" | "default-2024-11-15" | null;
  score_threshold?: number | null;   // 0-1
  hybrid_search?: {
    embedding_weight: number;        // *
    text_weight: number;             // *
  } | null;
}
```

| Field | Type | Required | Allowed Values | Description |
|---|---|---|---|---|
| `ranker` | `string` | ? | `auto` or `default-2024-11-15` | Ranking algorithm |
| `score_threshold` | `number` | ? | `[0, 1]` | Score threshold; the closer to 1, the stricter |
| `hybrid_search` | `object` | ? | — | RRF fusion weights |

#### (c) ComputerTool

```ts
{ type: "computer" }
```
Only the `type` field, fixed to `"computer"`.

#### (d) ComputerUsePreviewTool

```ts
ComputerUsePreviewTool {
  type: "computer_use_preview";                                 // *
  display_width: number;                                        // *
  display_height: number;                                       // *
  environment: "windows"|"mac"|"linux"|"ubuntu"|"browser";     // *
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | Fixed `"computer_use_preview"` |
| `display_width` / `display_height` | `integer` | * | Screen pixels |
| `environment` | `string` | * | Pick 1 of 5 |

#### (e) WebSearchTool

```ts
WebSearchTool {
  type: "web_search" | "web_search_2025_08_26";  // *
  allowed_domains?: string[] | null;             // ? e.g. ["pubmed.ncbi.nlm.nih.gov"]
  filters?: Filters | null;
  search_context_size?: "low"|"medium"|"high" | null;  // Default medium
  user_location?: UserLocation | null;
}
```

##### `UserLocation` Object

```ts
UserLocation {
  type?: "approximate" | null;       // Fixed approximate
  city?: string | null;
  country?: string | null;            // ISO two-letter country code, e.g. "US"
  region?: string | null;
  timezone?: string | null;           // IANA, e.g. "America/Los_Angeles"
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | ? | Currently only `"approximate"` |
| `city` | `string` | ? | Free-text city name |
| `country` | `string` | ? | ISO 3166-1 two-letter |
| `region` | `string` | ? | Free-text province/state |
| `timezone` | `string` | ? | IANA timezone |

#### (f) WebSearchPreviewTool

```ts
WebSearchPreviewTool {
  type: "web_search_preview" | "web_search_preview_2025_03_11";  // *
  search_content_types?: ("text"|"image")[] | null;
  search_context_size?: "low"|"medium"|"high" | null;
  user_location?: UserLocation | null;
}
```

#### (g) Mcp (MCP Remote Tool)

```ts
Mcp {
  type: "mcp";                               // *
  server_label: string;                      // *
  server_url?: string | null;                // Pick one of two
  connector_id?: ConnectorId | null;         // Pick one of two
  authorization?: string | null;             // OAuth token
  headers?: { [k: string]: string } | null;
  allowed_tools?: McpAllowedTools | null;
  require_approval?: McpRequireApproval | null;
  server_description?: string | null;
  defer_loading?: boolean | null;
}
```

`ConnectorId` allowed values:
- `connector_dropbox`
- `connector_gmail`
- `connector_googlecalendar`
- `connector_googledrive`
- `connector_microsoftteams`
- `connector_outlookcalendar`
- `connector_outlookemail`
- `connector_sharepoint`

##### `McpAllowedTools` — Pick one of two

**(i)** String array: `string[]`, directly lists allowed tool names.

**(ii)** Filter object:

```ts
McpAllowedToolsFilter {
  tool_names?: string[] | null;
  read_only?: boolean | null;        // Only matches tools with readOnlyHint
}
```

##### `McpRequireApproval` — Pick one of two

**(i)** String literal: `"always"` or `"never"`.

**(ii)** Fine-grained object:

```ts
McpRequireApprovalFilter {
  always?: { tool_names?: string[]; read_only?: boolean } | null;
  never?:  { tool_names?: string[]; read_only?: boolean } | null;
}
```

#### (h) CodeInterpreter

```ts
CodeInterpreter {
  type: "code_interpreter";          // *
  container: string | ContainerAuto; // *
}
```

`container` can be a string ID of an existing container, or the following object:

##### `ContainerAuto` (`type:"auto"`)

```ts
ContainerAuto {
  type: "auto";                                   // *
  file_ids?: string[] | null;
  memory_limit?: "1g"|"4g"|"16g"|"64g" | null;
  network_policy?: NetworkPolicy | null;
}
```

| Field | Type | Required | Allowed Values | Description |
|---|---|---|---|---|
| `type` | `string` | * | `"auto"` | Fixed |
| `file_ids` | `string[]` | ? | — | List of available file IDs |
| `memory_limit` | `string` | ? | `1g`/`4g`/`16g`/`64g` | Memory limit |
| `network_policy` | `object` | ? | — | See below |

##### `NetworkPolicy` — Pick one of two

**Disabled**:

```ts
{ type: "disabled" }
```

**Allowlist**:

```ts
{
  type: "allowlist";              // *
  allowed_domains: string[];      // *
  domain_secrets?: Array<{ domain: string; name: string; value: string }> | null;
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | `"allowlist"` |
| `allowed_domains` | `string[]` | * | Domains allowed for outbound traffic |
| `domain_secrets[]` | `array<object>` | ? | Secrets injected for specific domains |
| `domain_secrets[].domain` | `string` | * | Domain name |
| `domain_secrets[].name` | `string` | * | Secret name |
| `domain_secrets[].value` | `string` | * | Secret value (≤10MB) |

#### (i) ImageGeneration

```ts
ImageGeneration {
  type: "image_generation";                        // *
  action?: "generate"|"edit"|"auto" | null;        // Default auto
  background?: "transparent"|"opaque"|"auto" | null;
  input_fidelity?: "high"|"low" | null;            // Only for gpt-image-1/1.5
  input_image_mask?: ImageMask | null;
  model?: "gpt-image-1"|"gpt-image-1-mini"|"gpt-image-1.5" | null;
  moderation?: "auto"|"low" | null;
  output_compression?: number | null;              // 0-100
  output_format?: "png"|"webp"|"jpeg" | null;
  partial_images?: number | null;                  // 0-3, streaming phased images
  quality?: "low"|"medium"|"high"|"auto" | null;
  size?: "1024x1024"|"1024x1536"|"1536x1024"|"auto" | null;
}
```

##### `ImageMask` Object

```ts
ImageMask {
  file_id?: string | null;
  image_url?: string | null;     // base64-encoded mask image
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `file_id` | `string` | ? | File ID of the mask image |
| `image_url` | `string` | ? | base64 mask image (pick one of two) |

#### (j) LocalShell

```ts
{ type: "local_shell" }
```
Only the `type` field.

#### (k) FunctionShellTool (Managed Shell)

```ts
FunctionShellTool {
  type: "shell";                          // *
  environment?: Environment | null;
}
```

##### `Environment` — Pick one of three

**(i) ContainerAuto** (Note: the type here is `"container_auto"`, different from code_interpreter's `"auto"`)

```ts
{
  type: "container_auto";       // *
  file_ids?: string[];
  memory_limit?: "1g"|"4g"|"16g"|"64g";
  network_policy?: NetworkPolicy;
  skills?: Skill[];
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | `"container_auto"` |
| `file_ids` | `string[]` | ? | — |
| `memory_limit` | `string` | ? | Pick 1 of 4 |
| `network_policy` | `object` | ? | Same as §3.8.h |
| `skills` | `array<object>` | ? | See below |

**(ii) LocalEnvironment**

```ts
{
  type: "local";                // *
  skills?: LocalSkill[];
}
LocalSkill {
  name: string;                 // *
  description: string;          // *
  path: string;                 // *
}
```

**(iii) ContainerReference**

```ts
{
  type: "container_reference";  // *
  container_id: string;         // *
}
```

##### `Skill` — Pick one of two

**SkillReference**

```ts
{
  type: "skill_reference";        // *
  skill_id: string;               // * 1-64 characters
  version?: string;               // Positive integer or "latest"
}
```

**InlineSkill**

```ts
{
  type: "inline";                 // *
  name: string;                   // *
  description: string;            // *
  source: object;                 // * Inline skill content
}
```

#### (l) CustomTool

```ts
CustomTool {
  type: "custom";                                          // *
  name: string;                                            // *
  description?: string | null;
  defer_loading?: boolean | null;
  format?: CustomToolInputFormat | null;                   // Default free text
}
```

#### (m) NamespaceTool — Namespace Grouping

```ts
NamespaceTool {
  type: "namespace";                          // *
  name: string;                               // * e.g. "crm"
  description: string;                        // * ≥1 character
  tools: Array<FunctionToolMini | CustomTool>; // *
}
FunctionToolMini {
  type: "function";
  name: string;                  // * 1-128
  description?: string;
  parameters?: object;
  strict?: boolean;
  defer_loading?: boolean;
}
```

#### (n) ToolSearchTool

```ts
ToolSearchTool {
  type: "tool_search";                  // *
  execution?: "server" | "client" | null;
  description?: string | null;
  parameters?: object | null;
}
```

#### (o) ApplyPatchTool

```ts
{ type: "apply_patch" }
```

### 3.9 Input Item Object (20+ types)

Each element in the `input` array is an item, differentiated by `type`.

#### (a) EasyInputMessage (Most Common)

```ts
EasyInputMessage {
  role: "user"|"assistant"|"system"|"developer";  // *
  content: string | InputContent[];               // *
  type?: "message" | null;
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `role` | `string` | * | 4 roles |
| `content` | `string \| array` | * | String or `InputContent[]`, see §3.10 |
| `type` | `string` | ? | `"message"` or omitted |

#### (b) Message (Full Version)

```ts
Message {
  role: "user"|"system"|"developer";   // *(no assistant)
  content: InputContent[];             // * Must be an array
  status?: "in_progress"|"completed"|"incomplete" | null;
  type?: "message" | null;
}
```

#### (c) ResponseOutputMessage (Replay the previous turn's assistant output)

```ts
ResponseOutputMessage {
  id: string;                                                       // *
  type: "message";                                                  // *
  role: "assistant";                                                // *
  status: "in_progress"|"completed"|"incomplete";                   // *
  content: (OutputText | OutputRefusal)[];                          // *
  phase?: "commentary"|"final_answer" | null;
}
```

`phase` is only meaningful for models after `gpt-5.3-codex`; if present, it must be passed back in subsequent requests to avoid performance degradation.

##### `OutputText` Object

```ts
OutputText {
  type: "output_text";                                              // *
  text: string;                                                     // *
  annotations: Annotation[];                                        // *(array, may be empty)
  logprobs?: Logprob[] | null;
}
```

| Field | Type | Required | Nullable | Description |
|---|---|---|---|---|
| `type` | `string` | * | No | `"output_text"` |
| `text` | `string` | * | No | Complete text |
| `annotations` | `array<Annotation>` | * | No | May be an empty array; 4 types see §4.3 |
| `logprobs` | `array<Logprob>` | ? | Yes | Only when include is enabled |

##### `Logprob` Object

```ts
Logprob {
  token: string;                  // *
  bytes: number[];                // *
  logprob: number;                // *
  top_logprobs: Array<{           // *
    token: string;
    bytes: number[];
    logprob: number;
  }>;
}
```

##### `OutputRefusal` Object

```ts
OutputRefusal {
  type: "refusal";       // *
  refusal: string;       // * Refusal explanation
}
```

#### (d) FunctionCall (Replay the previous turn's function call)

```ts
ResponseFunctionToolCall {
  type: "function_call";                                            // *
  call_id: string;                                                  // *
  name: string;                                                     // *
  arguments: string;                                                // * Serialized JSON
  id?: string;
  namespace?: string;
  status?: "in_progress"|"completed"|"incomplete";
}
```

#### (e) FunctionCallOutput (Fill in function call results)

```ts
FunctionCallOutput {
  type: "function_call_output";                                     // *
  call_id: string;                                                  // * 1-64 characters
  output: string | OutputContent[];                                 // *
  id?: string;
  status?: "in_progress"|"completed"|"incomplete";
}
```

`output` can be a string (JSON-serialized) or a multimodal array whose elements have the same shape as `InputContent` in §3.10 (`input_text` / `input_image` / `input_file`).

#### (f) ComputerCall

```ts
ResponseComputerToolCall {
  type: "computer_call";                                                  // *
  id: string;                                                             // *
  call_id: string;                                                        // *
  status: "in_progress"|"completed"|"incomplete";                         // *
  action?: ComputerAction | null;                                         // See below
  actions?: ComputerAction[] | null;                                      // Batch
  pending_safety_checks: PendingSafetyCheck[];                            // *
}
```

##### `PendingSafetyCheck` Object

```ts
PendingSafetyCheck {
  id: string;             // *
  code?: string | null;   // Type
  message?: string | null;
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | `string` | * | Safety check ID |
| `code` | `string` | ? | Type |
| `message` | `string` | ? | Details |

##### `ComputerAction` Subtypes (10 types)

| `type` | Fields |
|---|---|
| `click` | `x:int`, `y:int`, `button: "left"\|"right"\|"wheel"\|"back"\|"forward"`, `keys?: string[]` |
| `double_click` | `x:int`, `y:int`, `keys?: string[]` |
| `drag` | `path: Array<{x:int, y:int}>`, `keys?: string[]` |
| `keypress` | `keys: string[]` |
| `move` | `x:int`, `y:int`, `keys?: string[]` |
| `screenshot` | (no additional fields) |
| `scroll` | `x:int`, `y:int`, `scroll_x:int`, `scroll_y:int`, `keys?: string[]` |
| `type` | `text: string` |
| `wait` | (no additional fields) |

#### (g) ComputerCallOutput

```ts
ComputerCallOutput {
  type: "computer_call_output";                                       // *
  call_id: string;                                                    // * 1-64
  output: ComputerScreenshot;                                         // *
  id?: string;
  status?: "completed"|"incomplete"|"failed"|"in_progress";
  acknowledged_safety_checks?: AcknowledgedSafetyCheck[] | null;
}
```

##### `ComputerScreenshot` Object

```ts
ComputerScreenshot {
  type: "computer_screenshot";       // *
  file_id?: string | null;
  image_url?: string | null;
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | Fixed `"computer_screenshot"` |
| `file_id` | `string` | ? | Pick one of two |
| `image_url` | `string` | ? | Pick one of two |

##### `AcknowledgedSafetyCheck` Object

```ts
AcknowledgedSafetyCheck {
  id: string;                  // *
  code?: string | null;
  message?: string | null;
}
```

#### (h) ReasoningItem

```ts
ResponseReasoningItem {
  type: "reasoning";                                                  // *
  id: string;                                                         // *
  summary: SummaryPart[];                                             // *
  content?: ReasoningTextPart[] | null;
  encrypted_content?: string | null;
  status?: "in_progress"|"completed"|"incomplete";
}
```

##### `SummaryPart` Object

```ts
SummaryPart {
  type: "summary_text";    // *
  text: string;            // *
}
```

##### `ReasoningTextPart` Object

```ts
ReasoningTextPart {
  type: "reasoning_text";  // *
  text: string;            // *
}
```

#### (i) ImageGenerationCall

```ts
{
  type: "image_generation_call";                                      // *
  id: string;                                                         // *
  status: "in_progress"|"completed"|"generating"|"failed";            // *
  result: string | null;                                              // base64 PNG
}
```

#### (j) CodeInterpreterCall

```ts
{
  type: "code_interpreter_call";                                      // *
  id: string;                                                         // *
  container_id: string;                                               // *
  status: "in_progress"|"completed"|"incomplete"|"interpreting"|"failed";
  code?: string | null;
  outputs?: Array<LogsOutput | ImageOutput> | null;
}
```

##### `LogsOutput` Object

```ts
LogsOutput {
  type: "logs";    // *
  logs: string;    // *
}
```

##### `ImageOutput` Object

```ts
ImageOutput {
  type: "image";   // *
  url: string;     // *
}
```

#### (k) LocalShellCall

```ts
{
  type: "local_shell_call";                                           // *
  id: string;                                                         // *
  call_id: string;                                                    // *
  status: "in_progress"|"completed"|"incomplete";                     // *
  action: LocalShellAction;                                           // *
}
```

##### `LocalShellAction` Object

```ts
LocalShellAction {
  type: "exec";                              // *
  command: string[];                         // *
  env: { [k: string]: string };              // *
  timeout_ms?: number | null;
  user?: string | null;
  working_directory?: string | null;
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | Fixed `"exec"` |
| `command` | `string[]` | * | Command array |
| `env` | `object` | * | Environment variables (may be an empty object) |
| `timeout_ms` | `integer` | ? | Timeout in milliseconds |
| `user` | `string` | ? | User to run as |
| `working_directory` | `string` | ? | Working directory |

#### (l) LocalShellCallOutput

```ts
{
  type: "local_shell_call_output";   // *
  id: string;                        // *
  output: string;                    // * JSON string
  status?: "in_progress"|"completed"|"incomplete";
}
```

#### (m) ShellCall (Managed Shell)

```ts
{
  type: "shell_call";                                                 // *
  call_id: string;                                                    // *
  action: ShellAction;                                                // *
  id?: string;
  environment?: ShellEnvironment | null;
  status?: "in_progress"|"completed"|"incomplete";
}
```

##### `ShellAction` Object

```ts
ShellAction {
  commands: string[];                  // *
  timeout_ms?: number | null;
  max_output_length?: number | null;
}
```

##### `ShellEnvironment` — Pick one of two

```ts
{ type: "local" } | { type: "container_reference"; container_id: string }
```

#### (n) ShellCallOutput

```ts
{
  type: "shell_call_output";                                          // *
  call_id: string;                                                    // *
  output: ShellOutputChunk[];                                         // *
  id?: string;
  max_output_length?: number | null;
  status?: "in_progress"|"completed"|"incomplete";
}
```

##### `ShellOutputChunk` Object

```ts
ShellOutputChunk {
  stdout: string;        // * ≤10MB
  stderr: string;        // * ≤10MB
  outcome: Outcome;      // *
}
```

##### `Outcome` — Pick one of two

```ts
{ type: "timeout" } | { type: "exit"; exit_code: number }
```

| Type | Fields |
|---|---|
| `{ type:"timeout" }` | Only type, indicates timeout |
| `{ type:"exit", exit_code:int }` | Normal exit, with exit code |

#### (o) ApplyPatchCall

```ts
{
  type: "apply_patch_call";                                           // *
  call_id: string;                                                    // *
  operation: CreateFileOp | DeleteFileOp | UpdateFileOp;              // *
  status: "in_progress"|"completed";                                  // *
  id?: string;
}
```

##### `CreateFileOp` Object

```ts
{ type: "create_file"; path: string; diff: string }
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | `"create_file"` |
| `path` | `string` | * | File path (≥1) |
| `diff` | `string` | * | Unified diff (≤10MB) |

##### `DeleteFileOp` Object

```ts
{ type: "delete_file"; path: string }
```

##### `UpdateFileOp` Object

```ts
{ type: "update_file"; path: string; diff: string }
```

#### (p) ApplyPatchCallOutput

```ts
{
  type: "apply_patch_call_output";                                    // *
  call_id: string;                                                    // *
  status: "completed" | "failed";                                     // *
  output?: string | null;
  id?: string;
}
```

#### (q) MCP Series

##### `McpCall` Object

```ts
McpCall {
  type: "mcp_call";                                                   // *
  id: string;                                                         // *
  server_label: string;                                               // *
  name: string;                                                       // *
  arguments: string;                                                  // * JSON string
  output?: string | null;
  error?: string | null;
  approval_request_id?: string | null;
  status?: "in_progress"|"completed"|"incomplete"|"calling"|"failed";
}
```

##### `McpListTools` Object

```ts
McpListTools {
  type: "mcp_list_tools";                                             // *
  id: string;                                                         // *
  server_label: string;                                               // *
  tools: McpToolInfo[];                                               // *
  error?: string | null;
}
```

##### `McpToolInfo` Object

```ts
McpToolInfo {
  name: string;                  // *
  input_schema: object;          // *
  description?: string | null;
  annotations?: object | null;
}
```

##### `McpApprovalRequest` Object

```ts
McpApprovalRequest {
  type: "mcp_approval_request";                                       // *
  id: string;                                                         // *
  server_label: string;                                               // *
  name: string;                                                       // *
  arguments: string;                                                  // *
}
```

##### `McpApprovalResponse` Object

```ts
McpApprovalResponse {
  type: "mcp_approval_response";                                      // *
  approval_request_id: string;                                        // *
  approve: boolean;                                                   // *
  id?: string;
  reason?: string | null;
}
```

#### (r) CustomToolCall / CustomToolCallOutput

```ts
CustomToolCall {
  type: "custom_tool_call";       // *
  call_id: string;                // *
  name: string;                   // *
  input: string;                  // *
  id?: string;
  namespace?: string;
}
CustomToolCallOutput {
  type: "custom_tool_call_output"; // *
  call_id: string;                 // *
  output: string | OutputContent[];// *
  id?: string;
}
```

#### (s) CompactionItem

```ts
{
  type: "compaction";                  // *
  encrypted_content: string;           // * Max ~10MB
  id?: string;
}
```

#### (t) ItemReference

```ts
{ type: "item_reference"; id: string }
```

### 3.10 Content Part Object

Items within the `content` array inside `message`, `function_call_output`, and similar items are collectively referred to as InputContent.

#### (a) InputText

```ts
{
  type: "input_text";   // *
  text: string;         // * Max ~10MB
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | `"input_text"` |
| `text` | `string` | * | Text content |

#### (b) InputImage

```ts
{
  type: "input_image";                              // *
  detail?: "low" | "high" | "auto" | "original";   // Default auto
  image_url?: string | null;                        // Pick one of two (can be a full URL or data: URL), ≤20MB
  file_id?: string | null;                          // Pick one of two
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | `"input_image"` |
| `detail` | `string` | ? | Pick 1 of 4, default `"auto"` |
| `image_url` | `string` | ? | Full URL or base64 data URL |
| `file_id` | `string` | ? | Uploaded file ID |

> At least one of `image_url` / `file_id` must be provided.

#### (c) InputFile

```ts
{
  type: "input_file";       // *
  file_id?: string | null;
  file_url?: string | null;
  file_data?: string | null;  // base64, ≤32MB
  filename?: string | null;
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | * | `"input_file"` |
| `file_id` | `string` | ? | Uploaded file ID (pick one of three) |
| `file_url` | `string` | ? | URL (pick one of three) |
| `file_data` | `string` | ? | base64 content, ≤32MB (pick one of three) |
| `filename` | `string` | ? | Filename |

---

## 4. HTTPS Response Body

In non-streaming mode, a `Response` object is returned directly; in streaming mode, it is delivered via SSE events (see §5).

### 4.1 Response Object Top-level

| Field | Type | Always Present? | Nullable? | Description |
|---|---|---|---|---|
| `id` | `string` | ✅ | No | `resp_xxx` |
| `object` | `string` | ✅ | No | Fixed `"response"` |
| `created_at` | `number` | ✅ | No | Unix seconds |
| `status` | `string` | ✅ | No | `"queued"`/`"in_progress"`/`"completed"`/`"failed"`/`"incomplete"`/`"cancelled"` |
| `model` | `string` | ✅ | No | Actual model snapshot (e.g. `gpt-5-2025-08-07`) |
| `output` | `array<OutputItem>` | ✅ | No | May be an empty array, see §4.2 |
| `usage` | `object` (Usage) | Conditional | ✅ | May be `null` on failure/early cancellation, see §4.4 |
| `error` | `object` (ResponseError) | Conditional | ✅ | Non-null only when `status:"failed"` |
| `incomplete_details` | `object` | Conditional | ✅ | Non-null only when `status:"incomplete"` |
| `metadata` | `object` (Dict[str,str]) | Conditional | ✅ | Echoes the request |
| `temperature` | `number` | Conditional | ✅ | Echo |
| `top_p` | `number` | Conditional | ✅ | Echo |
| `max_output_tokens` | `integer` | Conditional | ✅ | Echo |
| `max_tool_calls` | `integer` | Conditional | ✅ | Echo |
| `parallel_tool_calls` | `boolean` | ✅ | No | Echo |
| `tools` | `array<Tool>` | ✅ | No | Echo |
| `tool_choice` | `string \| object` | ✅ | No | Echo |
| `text` | `object` (TextConfig) | ✅ | No | Echo |
| `reasoning` | `object` (Reasoning) | Conditional | ✅ | Echo |
| `previous_response_id` | `string` | Conditional | ✅ | Echo |
| `conversation` | `object \| string` | Conditional | ✅ | Associated conversation |
| `instructions` | `string \| array` | Conditional | ✅ | Echo |
| `service_tier` | `string` | Conditional | ✅ | **Actual** tier used (may differ from the request) |
| `background` | `boolean` | Conditional | ✅ | Echo |
| `prompt_cache_key` / `prompt_cache_retention` | `string` | Conditional | ✅ | Echo |
| `safety_identifier` | `string` | Conditional | ✅ | Echo |
| `verbosity` | `string` | Conditional | ✅ | Echo |
| `output_text` | `string` | ❌ | — | **SDK convenience field**; typically **not present** in the raw API JSON; must be assembled from `output[*].content[*].text` |

> ⚠ **"Always Present?" convention**: In OpenAI's JSON, "field absent" and "field is null" are semantically equivalent. Aside from core fields explicitly marked with ✅, other fields may not appear in the JSON at all under different models and states; the SDK layer normalizes them to `null`.

### 4.2 Output Item Object (20+ types)

Items in the `output` array have the same shape as input items in §3.9, but you will only **see** types produced by the model in the response (you will not see input-side types like `item_reference`). All output items have at minimum:

| Common Field | Type | Always Present? | Description |
|---|---|---|---|
| `id` | `string` | ✅ | Item ID |
| `type` | `string` | ✅ | Type identifier |
| `status` | `string` | Depends on type | See each subtype |

The table below lists common output item types in responses; **for detailed field definitions, refer back to the corresponding subsection in §3.9** (item structures in responses are the same as in requests):

| `type` | Details in | Field Nullability |
|---|---|---|
| `message` (role:"assistant") | §3.9.c | `phase` may be null; `content` is always present |
| `reasoning` | §3.9.h | `content`, `encrypted_content` may be null; `summary` is always present (may be an empty array) |
| `function_call` | §3.9.d | Same as §3.9.d |
| `custom_tool_call` | §3.9.r | Same as §3.9.r |
| `file_search_call` | See below | `results` may be null (unless include is enabled) |
| `web_search_call` | See below | `action.sources`/`queries` may be null |
| `computer_call` | §3.9.f | Same |
| `code_interpreter_call` | §3.9.j | `code` may be null; `outputs` may be null |
| `image_generation_call` | §3.9.i | `result` is null when in_progress |
| `local_shell_call` / `shell_call` / `apply_patch_call` | §3.9.k-p | Same |
| `mcp_call` / `mcp_list_tools` / `mcp_approval_request` | §3.9.q | `output`, `error` may both be null |
| `tool_search_call` / `tool_search_output` | See below | — |
| `compaction` | §3.9.s | — |

#### `ResponseFileSearchToolCall` Complete Object

```ts
ResponseFileSearchToolCall {
  type: "file_search_call";                              // *
  id: string;                                            // *
  queries: string[];                                     // *
  status: "in_progress"|"searching"|"completed"|"incomplete"|"failed";
  results?: FileSearchResult[] | null;
}
```

##### `FileSearchResult` Object

```ts
FileSearchResult {
  file_id?: string | null;
  filename?: string | null;
  score?: number | null;                                 // 0-1
  text?: string | null;
  attributes?: { [k:string]: string|number|boolean } | null;
}
```

| Field | Type | Required | Nullable | Description |
|---|---|---|---|---|
| `file_id` | `string` | ? | Yes | File ID |
| `filename` | `string` | ? | Yes | Filename |
| `score` | `float` | ? | Yes | Relevance score `[0,1]` |
| `text` | `string` | ? | Yes | Text snippet retrieved from the file |
| `attributes` | `object` | ? | Yes | Up to 16 metadata pairs, key ≤64, value ≤512 |

#### `ResponseFunctionWebSearch` Complete Object

```ts
ResponseFunctionWebSearch {
  type: "web_search_call";                                          // *
  id: string;                                                       // *
  status: "in_progress"|"searching"|"completed"|"failed";
  action: ActionSearch | ActionOpenPage | ActionFindInPage;         // *
}
```

##### `ActionSearch` Object

```ts
ActionSearch {
  type: "search";                                          // *
  query: string;                                           // * (Deprecated, still returned)
  queries?: string[] | null;                               // New field
  sources?: Array<{ type: "url"; url: string }> | null;
}
```

##### `ActionOpenPage` Object

```ts
ActionOpenPage {
  type: "open_page";                                       // *
  url?: string | null;                                     // URI
}
```

##### `ActionFindInPage` Object

```ts
ActionFindInPage {
  type: "find_in_page";                                    // *
  url: string;                                             // * URI
  pattern: string;                                         // *
}
```

#### `ResponseToolSearchCall` and `ResponseToolSearchOutputItem`

```ts
ResponseToolSearchCall {
  type: "tool_search_call";                                // *
  id: string;                                              // *
  arguments: object;                                       // *
  execution: "server" | "client";                          // *
  status: "in_progress" | "completed" | "incomplete";      // *
  call_id?: string | null;
  created_by?: string | null;
}
ResponseToolSearchOutputItem {
  type: "tool_search_output";                              // *
  id: string;                                              // *
  execution: "server" | "client";                          // *
  status: "in_progress" | "completed" | "incomplete";      // *
  tools: Tool[];                                           // * See §3.8
  call_id?: string | null;
  created_by?: string | null;
}
```

### 4.3 Annotation Object (4 types)

Items in the `annotations` array carried by `output_text` within `message.content[*]`:

#### (a) FileCitation

```ts
{ type: "file_citation"; file_id: string; filename: string; index: number }
```

| Field | Type | Always Present | Description |
|---|---|---|---|
| `type` | `string` | ✅ | `"file_citation"` |
| `file_id` | `string` | ✅ | File ID |
| `filename` | `string` | ✅ | Filename |
| `index` | `integer` | ✅ | Index in the file list |

#### (b) UrlCitation

```ts
{
  type: "url_citation";
  url: string;
  title: string;
  start_index: number;
  end_index: number;
}
```

| Field | Type | Always Present | Description |
|---|---|---|---|
| `type` | `string` | ✅ | `"url_citation"` |
| `url` | `string` | ✅ | Web page URL |
| `title` | `string` | ✅ | Web page title |
| `start_index` | `integer` | ✅ | Starting character index in the message |
| `end_index` | `integer` | ✅ | Ending character index |

#### (c) ContainerFileCitation

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

| Field | Type | Always Present | Description |
|---|---|---|---|
| `type` | `string` | ✅ | `"container_file_citation"` |
| `container_id` | `string` | ✅ | Container ID |
| `file_id` | `string` | ✅ | File ID |
| `filename` | `string` | ✅ | Filename |
| `start_index` / `end_index` | `integer` | ✅ | Character index range |

#### (d) FilePath

```ts
{ type: "file_path"; file_id: string; index: number }
```

| Field | Type | Always Present | Description |
|---|---|---|---|
| `type` | `string` | ✅ | `"file_path"` |
| `file_id` | `string` | ✅ | File ID |
| `index` | `integer` | ✅ | File list index |

### 4.4 Usage Object

```ts
Usage {
  input_tokens: number;                                    // *
  input_tokens_details: InputTokensDetails;                // *
  output_tokens: number;                                   // *
  output_tokens_details: OutputTokensDetails;              // *
  total_tokens: number;                                    // *
}
```

##### `InputTokensDetails` Object

```ts
InputTokensDetails {
  cached_tokens: number;          // *
}
```

##### `OutputTokensDetails` Object

```ts
OutputTokensDetails {
  reasoning_tokens: number;       // *
}
```

| Field | Type | Always Present? | Description |
|---|---|---|---|
| `input_tokens` | `integer` | ✅ | Total input token count |
| `input_tokens_details.cached_tokens` | `integer` | ✅ | Number of cache-hit tokens |
| `output_tokens` | `integer` | ✅ | Total output token count (**includes reasoning**) |
| `output_tokens_details.reasoning_tokens` | `integer` | ✅ | Tokens used for thinking |
| `total_tokens` | `integer` | ✅ | input + output |

> ⚠ When `status` is `failed` or the response is cancelled very early, the top-level `usage` field may be `null` in its entirety.

### 4.5 Error / IncompleteDetails Object

#### `ResponseError` Object

```ts
ResponseError {
  code: string;     // *
  message: string;  // *
}
```

| Field | Type | Always Present | Description |
|---|---|---|---|
| `code` | `string` | ✅ | e.g. `"model_error"`, `"rate_limit_exceeded"` |
| `message` | `string` | ✅ | Human-readable description |

This object only appears when `status:"failed"`; in all other cases the entire `error` field is `null`.

#### `IncompleteDetails` Object

```ts
IncompleteDetails {
  reason: "max_output_tokens" | "content_filter" | string;  // *
}
```

| Field | Type | Always Present | Allowed Values |
|---|---|---|---|
| `reason` | `string` | ✅ | Common: `max_output_tokens`, `content_filter` |

This object only appears when `status:"incomplete"`.

---

## 5. SSE Stream Events — All Event Payloads Expanded

Add `stream: true` to the request body; response header `Content-Type: text/event-stream`. The wire format of each event:

```
event: <type>
data: <JSON>

```

### 5.1 Common Fields

All events have:

```ts
StreamEventBase {
  type: string;            // * See each subtype
  sequence_number: number; // * Monotonically increasing, starting from 0
}
```

### 5.2 Lifecycle Events

#### `response.created` / `response.in_progress` / `response.queued`

```ts
{
  type: "response.created" | "response.in_progress" | "response.queued";
  sequence_number: number;
  response: Response;   // * Complete Response object (§4.1), but `output` is typically an empty array
}
```

#### `response.completed` / `response.failed` / `response.incomplete`

```ts
{
  type: "response.completed" | "response.failed" | "response.incomplete";
  sequence_number: number;
  response: Response;   // * Complete, includes usage / error / incomplete_details
}
```

#### `error` (SSE-level error)

```ts
{
  type: "error";
  sequence_number: number;
  code: string | null;
  message: string;
  param?: string | null;
}
```

| Field | Type | Always Present | Nullable |
|---|---|---|---|
| `type` | `string` | ✅ | No |
| `sequence_number` | `integer` | ✅ | No |
| `code` | `string` | ✅ | Yes |
| `message` | `string` | ✅ | No |
| `param` | `string` | ? | Yes |

### 5.3 Output Item Level

#### `response.output_item.added` / `.done`

```ts
{
  type: "response.output_item.added" | "response.output_item.done";
  sequence_number: number;
  output_index: number;     // * Index of the item in the output array
  item: OutputItem;         // * Complete item object (see §4.2)
}
```

### 5.4 Content Part Level

```ts
{
  type: "response.content_part.added" | "response.content_part.done";
  sequence_number: number;
  item_id: string;
  output_index: number;
  content_index: number;
  part: OutputText | OutputRefusal;
}
```

### 5.5 Text Incrementals

#### `response.output_text.delta`

```ts
{
  type: "response.output_text.delta";
  sequence_number: number;
  item_id: string;
  output_index: number;
  content_index: number;
  delta: string;                  // * Incremental text for this segment
  obfuscation?: string | null;    // Only present if include_obfuscation:true
  logprobs?: Logprob[] | null;    // Only if include enables logprobs
}
```

#### `response.output_text.done`

```ts
{
  type: "response.output_text.done";
  sequence_number: number;
  item_id: string;
  output_index: number;
  content_index: number;
  text: string;                   // * Final full text of this content part
  logprobs?: Logprob[] | null;
}
```

#### `response.output_text.annotation.added`

```ts
{
  type: "response.output_text.annotation.added";
  sequence_number: number;
  item_id: string;
  output_index: number;
  content_index: number;
  annotation_index: number;
  annotation: Annotation;         // One of 4 types, see §4.3
}
```

#### `response.refusal.delta` / `.done`

```ts
{
  type: "response.refusal.delta";
  sequence_number; item_id; output_index; content_index;
  delta: string;
}
{
  type: "response.refusal.done";
  sequence_number; item_id; output_index; content_index;
  refusal: string;
}
```

### 5.6 Function Call / Custom Tool

```ts
// Function call argument increments
{
  type: "response.function_call_arguments.delta";
  sequence_number; item_id; output_index;
  delta: string;
}
{
  type: "response.function_call_arguments.done";
  sequence_number; item_id; output_index;
  arguments: string;          // Complete JSON string
}

// Custom tool input increments
{
  type: "response.custom_tool_call_input.delta";
  sequence_number; item_id; output_index;
  delta: string;
}
{
  type: "response.custom_tool_call_input.done";
  sequence_number; item_id; output_index;
  input: string;
}
```

### 5.7 File Search Events

```ts
{ type: "response.file_search_call.in_progress"; sequence_number; item_id; output_index }
{ type: "response.file_search_call.searching";   sequence_number; item_id; output_index }
{ type: "response.file_search_call.completed";   sequence_number; item_id; output_index }
```

### 5.8 Web Search Events

```ts
{ type: "response.web_search_call.in_progress"; sequence_number; item_id; output_index }
{ type: "response.web_search_call.searching";   sequence_number; item_id; output_index }
{ type: "response.web_search_call.completed";   sequence_number; item_id; output_index }
```

### 5.9 Code Interpreter Events

```ts
{ type: "response.code_interpreter_call.in_progress"; sequence_number; item_id; output_index }
{
  type: "response.code_interpreter_call_code.delta";
  sequence_number; item_id; output_index;
  delta: string;
}
{
  type: "response.code_interpreter_call_code.done";
  sequence_number; item_id; output_index;
  code: string;
}
{ type: "response.code_interpreter_call.interpreting"; sequence_number; item_id; output_index }
{ type: "response.code_interpreter_call.completed";    sequence_number; item_id; output_index }
```

### 5.10 Image Generation Events

```ts
{ type: "response.image_generation_call.in_progress"; sequence_number; item_id; output_index }
{ type: "response.image_generation_call.generating";  sequence_number; item_id; output_index }
{
  type: "response.image_generation_call.partial_image";
  sequence_number; item_id; output_index;
  partial_image_index: number;
  partial_image_b64: string;                 // base64 PNG
}
{ type: "response.image_generation_call.completed";   sequence_number; item_id; output_index }
```

### 5.11 MCP Events

```ts
{ type: "response.mcp_call.in_progress";       sequence_number; item_id; output_index }
{
  type: "response.mcp_call_arguments.delta";
  sequence_number; item_id; output_index;
  delta: string;
}
{
  type: "response.mcp_call_arguments.done";
  sequence_number; item_id; output_index;
  arguments: string;
}
{ type: "response.mcp_call.completed";         sequence_number; item_id; output_index }
{ type: "response.mcp_call.failed";            sequence_number; item_id; output_index }
{ type: "response.mcp_list_tools.in_progress"; sequence_number; item_id; output_index }
{ type: "response.mcp_list_tools.completed";   sequence_number; item_id; output_index }
{ type: "response.mcp_list_tools.failed";      sequence_number; item_id; output_index }
```

### 5.12 Reasoning Events

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

### 5.13 Field Nullability / Absence Notes

- `obfuscation`: Only appears in `*.delta` events when `stream_options.include_obfuscation:true`; when `false`, it **will not appear**.
- `usage`: **Only** appears in the `response` object carried by `response.completed` (and some `response.failed`) events; in earlier events, `response.usage` may be `null` or absent.
- `annotations`: Will not appear in `output_text.delta`; delivered via separate `annotation.added` events.
- `logprobs`: Only appears with delta/done when the request `include` enables `"message.output_text.logprobs"`.
- All events **must have** `type` and `sequence_number`.

---

## 6. WebSocket Mode

### 6.1 Connection

```
URL:    wss://api.openai.com/v1/responses
Header: Authorization: Bearer <OPENAI_API_KEY>
```

### 6.2 Client Event `response.create`

JSON frame. The payload is **almost identical** to the HTTPS body, with the following differences:

| Field | WebSocket Mode |
|---|---|
| `type` | * **Required, fixed to `"response.create"`** |
| `stream` | Not supported (transport is already streaming) |
| `background` | Not supported |
| `generate` | * **WebSocket-specific**, optional boolean. `false` means warm up only without generating, speeding up subsequent turns |
| All other fields | Identical to HTTPS |

Full TS structure:

```ts
WsResponseCreate {
  type: "response.create";              // *
  model?: string;
  input?: string | InputItem[];
  tools?: Tool[];
  tool_choice?: ToolChoice;
  previous_response_id?: string | null;
  conversation?: string | ConversationObject | null;
  instructions?: string | null;
  include?: string[];                   // include options
  store?: boolean;
  generate?: boolean;                   // ws-specific
  metadata?: { [k: string]: string };
  max_output_tokens?: number;
  max_tool_calls?: number;
  parallel_tool_calls?: boolean;
  temperature?: number;
  top_p?: number;
  text?: TextConfig;
  reasoning?: Reasoning;
  verbosity?: "low" | "medium" | "high";
  service_tier?: "auto" | "default" | "flex" | "scale" | "priority";
  safety_identifier?: string;
  prompt_cache_key?: string;
  prompt_cache_retention?: "in-memory" | "24h";
  prompt?: PromptObject;
  context_management?: ContextManagementItem[];
  stream_options?: StreamOptions;       // Can still set include_obfuscation
  // ⚠ Do not pass stream / background
}
```

### 6.3 Server Events

**Exactly the same as SSE** (all events in §5). Each event is sent as a text frame; the frame contains JSON with identical fields.

### 6.4 Connection Behavior and Limits

| Item | Limit |
|---|---|
| Connection lifetime | **60 minutes** |
| Concurrency | Single connection has only 1 in-flight response at a time; **no multiplexing** |
| Parallelism | Requires multiple connections |
| Sequencing | Multiple `response.create` events execute serially |

### 6.5 `previous_response_id` Caching Mechanism

- The server retains the **most recent** response state in **connection-level memory**.
- `store=true`: Older IDs can be hydrated from the persistence layer, but lose memory-level latency.
- `store=false` / ZDR: No persistent fallback; if the ID is not in the cache, `previous_response_not_found` is returned.
- After any turn failure (`4xx`/`5xx`), the server **evicts** the corresponding `previous_response_id` to prevent use of stale state.

### 6.6 Error Event Object

```ts
WsError {
  type: "error";        // *
  status: number;       // * HTTP status code
  error: WsErrorInner;  // *
}
WsErrorInner {
  type?: string;        // e.g. "invalid_request_error"
  code: string;         // e.g. "previous_response_not_found"
  message: string;
  param?: string | null;
}
```

| Field | Type | Always Present | Description |
|---|---|---|---|
| `type` | `string` | ✅ | Fixed `"error"` |
| `status` | `integer` | ✅ | HTTP status code |
| `error.type` | `string` | ? | Error category |
| `error.code` | `string` | ✅ | Error code |
| `error.message` | `string` | ✅ | Human-readable |
| `error.param` | `string` | ? | Triggering field |

Typical error codes:
- `previous_response_not_found`
- `websocket_connection_limit_reached` (60 minutes expired)
- Any HTTP request-level error code

---

## 7. Auxiliary Endpoints

### 7.1 Retrieve Response

```
GET /v1/responses/{response_id}
```

Optional query parameters:

| Parameter | Type | Description |
|---|---|---|
| `include` | `array<string>` | Same as request body |
| `stream` | `boolean` | When `true`, replays events in SSE format |
| `starting_after` | `integer` | When used with stream, starts after the specified `sequence_number` |

Returns: `Response` object (§4.1).

### 7.2 Cancel Background Response

```
POST /v1/responses/{response_id}/cancel
```

Only responses with `background:true` can be cancelled. Returns `Response` with `status:"cancelled"`.

### 7.3 Delete Response

```
DELETE /v1/responses/{response_id}
```

Returns a `DeleteResponse` object:

```ts
DeleteResponse {
  id: string;            // *
  object: "response";    // *
  deleted: true;         // *
}
```

### 7.4 List Input Items

```
GET /v1/responses/{response_id}/input_items
```

Pagination parameters: `limit` (1-100, default 20), `order` (`asc`/`desc`), `after`, `before`.

Returns a `ListResponse` object:

```ts
ListResponse {
  object: "list";              // *
  data: InputItem[];           // *
  first_id: string | null;
  last_id: string | null;
  has_more: boolean;           // *
}
```

### 7.5 Count Tokens

```
POST /v1/responses/input_tokens
```

The request body is roughly the same as a create request (primarily `model`, `input`, `tools`, `instructions`). Returns:

```ts
{
  object: "response.input_tokens";   // *
  input_tokens: number;              // *
}
```

### 7.6 Compact Response

```
POST /v1/responses/compact
```

Returns `CompactedResponse`:

```ts
CompactedResponse {
  id: string;                        // "rcmp_xxx"
  object: "response.compaction";     // *
  created_at: number;                // *
  output: OutputItem[];              // * Compacted items
  usage: Usage;                      // * Token accounting for the compaction itself
}
```

---

## 8. Error Responses

Non-streaming errors are returned with an HTTP error code + JSON:

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

#### `ErrorObject` Complete Definition

```ts
ErrorObject {
  message: string;          // *
  type: string;             // *
  code?: string | null;     // May be null
  param?: string | null;    // May be null
}
```

| Field | Always Present? | Nullable? | Allowed Values |
|---|---|---|---|
| `message` | ✅ | No | Human-readable |
| `type` | ✅ | No | `invalid_request_error`/`authentication_error`/`rate_limit_error`/`server_error`/`api_error` |
| `code` | ❌ | ✅ | Specific code |
| `param` | ❌ | ✅ | The name of the offending field |

#### Common Error Codes

| code | Meaning |
|---|---|
| `invalid_request_error` | Malformed request |
| `model_not_found` | Model unavailable or not enabled |
| `context_length_exceeded` | Context too long |
| `rate_limit_exceeded` | Rate/quota exceeded |
| `insufficient_quota` | Insufficient balance |
| `previous_response_not_found` | Previous response not found |
| `websocket_connection_limit_reached` | WebSocket 60-minute limit expired |
| `content_filter` | Blocked by content filter |

#### Important Response Headers

| Header | Description |
|---|---|
| `x-request-id` | Request ID; provide to OpenAI when troubleshooting |
| `x-ratelimit-limit-requests` / `-tokens` | Rate limit cap |
| `x-ratelimit-remaining-requests` / `-tokens` | Remaining quota |
| `x-ratelimit-reset-requests` / `-tokens` | Reset time |

Clients can proactively add `X-Client-Request-Id: <uuid>` to set a custom request ID.

---

## Appendix: Minimal Usable Templates

### A. Simplest Request

```json
{ "model": "gpt-5.2", "input": "Hello" }
```

### B. Streaming + Structured Output

```json
{
  "model": "gpt-5.2",
  "input": "Extract: John, 30, NYC",
  "stream": true,
  "text": {
    "format": {
      "type": "json_schema",
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

### C. Multi-turn (Explicit ID)

```json
{ "model":"gpt-5.2", "input":"Say it again", "previous_response_id":"resp_abc" }
```

### D. Multi-turn (Conversation)

```json
{ "model":"gpt-5.2", "input":"Say it again", "conversation":"conv_xyz" }
```

### E. WebSocket — One Complete Turn

```json
{
  "type": "response.create",
  "model": "gpt-5.2",
  "store": false,
  "previous_response_id": "resp_abc",
  "input": [
    { "type": "function_call_output", "call_id": "call_123", "output": "{\"temp\":24}" },
    { "type": "message", "role": "user",
      "content": [{ "type": "input_text", "text": "Continue optimizing" }] }
  ],
  "tools": []
}
```

---

*End of document.*

