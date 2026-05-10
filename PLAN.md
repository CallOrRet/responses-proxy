# Multi-Provider Chat API Compatibility Plan

## 1. Prerequisites: Completed Fixes

After comparing the openai-go SDK and DeepSeek API docs, 8 real issues were identified and fixed in the current hardcoded DeepSeek-only codebase:

| # | Severity | Issue | Fixed In |
|---|----------|-------|-----------|
| 1 | P0 | Streaming `response.completed` missing `usage` | `streaming.rs` |
| 2 | P1 | Streaming output_index inconsistent between delta and completed events | `streaming.rs` |
| 3 | P1 | SSE path missing `response.in_progress` event | `streaming.rs`, `main.rs` |
| 4 | P1 | Streaming events missing `sequence_number` | `main.rs` |
| 5 | P2 | `insufficient_system_resource` mapped to invalid value `"server_error"` | `convert_response.rs` |
| 6 | P2 | WS `response.created`/`in_progress` events missing fields | `main.rs` |
| 7 | P2 | WS hardcoded `stop: None, text: None` | `main.rs` |
| 8 | P2 | cached_tokens extraction uses wrong path for DeepSeek | `convert_response.rs` |

Added `src/verification_tests.rs` with 33 tests covering 26 end-to-end conversion scenarios. All passing.

---

## 2. Why Multi-Provider Support

The current code hardcodes DeepSeek API behavior. Key differences between OpenAI Chat API and DeepSeek Chat API:

| Dimension | DeepSeek | OpenAI |
|-----------|----------|--------|
| Reasoning toggle | `thinking: {"type":"enabled"}` top-level field | None; `reasoning_effort:"none"` disables |
| Reasoning effort | `reasoning_effort: "high"/"max"` top-level | `reasoning_effort: "none"/"minimal"/"low"/"medium"/"high"/"xhigh"` top-level |
| Max tokens | `max_tokens` | `max_completion_tokens` (preferred) |
| Message roles | No `developer` role | Supports `developer` role |
| Reasoning content | Response contains `reasoning_content` | Response has no `reasoning_content` |
| Provider-only fields | `thinking`, `user_id` | `verbosity`, `safety_identifier`, `modalities`, `audio`, `seed`, `n`, `service_tier` |

---

## 3. Design Principles

1. **Strongly-typed core untouched** — `responses_to_chat()` / `chat_to_responses()` keep type safety, no generic JSON transformer
2. **Post-processing layer** — after `ChatCompletionRequest` serializes to `serde_json::Value`, apply rename/inject/drop/values via provider profile
3. **Config-driven** — provider differences declared in `config.yaml`; switch/add providers without touching Rust code

---

## 4. config.yaml Design

```yaml
providers:
  deepseek:
    chat:
      # Field rename: internal JSON key → provider JSON key
      rename: {}

      # Inject extra top-level fields (${reasoning_effort} resolved at runtime)
      inject:
        thinking:
          type: "enabled"
          reasoning_effort: "${reasoning_effort}"

      # Drop fields this provider doesn't support
      drop:
        - "frequency_penalty"
        - "max_completion_tokens"

      # Field value mapping (null = remove the field)
      values:
        reasoning_effort:
          none: null
          minimal: "high"
          low: "high"
          medium: "high"
          high: "high"
          xhigh: "max"

      # Role mapping: Responses API role → Chat API role
      roles:
        developer: "system"

      # Response field path
      reasoning_path: "choices.0.message.reasoning_content"

      # Extra finish_reason mappings
      finish_reasons:
        insufficient_system_resource: "incomplete"

  openai:
    chat:
      rename:
        max_tokens: "max_completion_tokens"

      inject: {}

      drop:
        - "thinking"
        - "reasoning_content"
        - "user_id"

      values: {}   # OpenAI natively supports all reasoning_effort values

      roles: {}    # OpenAI natively supports developer role

      reasoning_path: null   # OpenAI Chat API doesn't return reasoning_content

      finish_reasons:
        function_call: "completed"

models:
  - model: gpt-5.5
    provider:
      profile: deepseek
      api_key: $DEEPSEEK_API_KEY
      base_url: https://api.deepseek.com
    downstream_model: deepseek-v4-pro

  - model: gpt-5.5-openai
    provider:
      profile: openai
      api_key: $OPENAI_API_KEY
      base_url: https://api.openai.com
    downstream_model: gpt-5.5
```

---

## 5. Rust Data Structures

```rust
// config.rs

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderChatProfile {
    /// Field rename: internal JSON key → provider JSON key
    #[serde(default)]
    pub rename: HashMap<String, String>,

    /// Extra fields to inject. Static values + ${var} templates
    #[serde(default)]
    pub inject: serde_json::Value,

    /// Fields to drop from the request
    #[serde(default)]
    pub drop: Vec<String>,

    /// Value maps: field_name → { internal_value → provider_value } (null=remove field)
    #[serde(default)]
    pub values: HashMap<String, HashMap<String, Option<String>>>,

    /// Role mapping: Responses API role → Chat API role
    #[serde(default)]
    pub roles: HashMap<String, String>,

    /// JSON path to reasoning_content in response, null if none
    #[serde(default)]
    pub reasoning_path: Option<String>,

    /// Extra finish_reason → output item status mappings
    #[serde(default)]
    pub finish_reasons: HashMap<String, String>,
}
```

Built-in defaults: `ProviderChatProfile::deepseek()` and `ProviderChatProfile::openai()` provide defaults. The `providers` section in `config.yaml` allows overriding/customizing.

```rust
pub struct ResolvedProvider {
    pub base_url: String,
    pub api_key: String,
    pub downstream_model: String,
    pub profile: ProviderChatProfile,   // ← new
}
```

---

## 6. Data Flow

**Request direction:**

```
ResponsesRequest → responses_to_chat() → ChatCompletionRequest → to_value()
    → profile.apply_request() (rename/inject/drop/values) → POST /chat/completions
```

**Response direction:**

```
Chat API Response JSON → profile.parse_response() (extract reasoning/finish_reasons)
    → ChatCompletionResponse → chat_to_responses() → ResponsesResponse
```

---

## 7. Usage

```rust
// Non-streaming
let mut chat_req = responses_to_chat(responses_req, &state.config.tool_type_allowlist);
chat_req.model = provider.downstream_model.clone();
let mut json = serde_json::to_value(&chat_req)?;
provider.profile.apply_request(&mut json, responses_req.reasoning.as_ref());

let response = client.post(&url).json(&json).send().await?;
let body = response.text().await?;
let chat_resp = provider.profile.parse_response(&body)?;
let resp = chat_to_responses(chat_resp, original_model);
```

---

## 8. Files Changed

| File | Change | Est. Lines |
|------|--------|------------|
| `config.rs` | Define `ProviderChatProfile` + `apply_request()` + `parse_response()` | +100 |
| `main.rs` | Call `apply_request` / `parse_response` in both handlers | +10 |
| `convert_request.rs` | Read reasoning_effort map + role map from profile | -10 +10 |
| `convert_response.rs` | Read reasoning_content / finish_reason paths from profile | -5 +5 |
| `config.yaml` | Add `providers` section | +40 |

---

## 9. Implementation Steps

| Step | Task |
|------|------|
| 1 | Define `ProviderChatProfile` struct with DeepSeek/OpenAI defaults |
| 2 | Add `profile` field to `ResolvedProvider`, parse in `Config::load` |
| 3 | Implement `ProviderChatProfile::apply_request()` |
| 4 | Implement `ProviderChatProfile::parse_response()` |
| 5 | Integrate into non-streaming `handle_responses` |
| 6 | Integrate into streaming `handle_responses` |
| 7 | Integrate into WS handler |
| 8 | Remove hardcoded DeepSeek mappings from `convert_request.rs` |
| 9 | Remove hardcoded logic from `convert_response.rs` |
| 10 | Add `providers` section to `config.yaml` |
| 11 | Add OpenAI profile tests to `verification_tests.rs` |
