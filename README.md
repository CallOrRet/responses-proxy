# responses-proxy

A proxy that converts **OpenAI Responses API** requests to **Chat Completions API** requests and converts Chat responses back to Responses format. The request and response structs used by the proxy are modeled directly from the official OpenAI Responses API and Chat Completions API definitions. Supports both HTTP SSE and WebSocket streaming, reasoning/thinking content, and tool calling. Works as a drop-in **Codex CLI** backend via DeepSeek or any Chat API-compatible provider.

## Features

- **HTTP SSE & WebSocket** — both `POST /v1/responses` (SSE) and `GET /v1/responses` (WebSocket upgrade)
- **OpenAI-compatible typed boundary** — incoming `/v1/responses` requests must deserialize as official Responses API request bodies, and upstream/downstream Chat bodies use official Chat Completions request/response shapes
- **Reasoning / Thinking** — maps Responses `reasoning.effort` to Chat `reasoning_effort`; provider-specific thinking fields can be added with `rewrite`
- **Tool Calling** — full `function_call` / `function_call_output` roundtrip with correct message ordering
- **Codex CLI Compatible** — handles warmup, `previous_response_id` continuation, and full streaming event chain
- **Multi-Model** — configurable per-model downstream providers

## Codex CLI

After starting responses-proxy, add the following line to `~/.codex/config.toml`:

```toml
openai_base_url = "http://localhost:3000/v1"
```

Then start Codex and it will route all requests through the proxy.

```bash
codex        # uses gpt-5.5 model
codex review # uses codex-auto-review model (if configured)
```

### Dedicated profile (switch backends per session)

`openai_base_url` is global. To flip between the proxy and another backend
per session instead, define a provider and select a profile with
`codex --profile <name>`.

In `~/.codex/config.toml`, declare the provider:

```toml
[model_providers.responses_proxy]
name = "responses-proxy (local)"
base_url = "http://localhost:3000/v1"
# wire_api defaults to "responses" (what the proxy speaks); no env_key needed
# unless you enable server.auth.keys in config.yaml.
```

> **Sub-agents (`features.multi_agent` / `features.multi_agent_v2`)** are
> Codex-local, `Experimental`-stage features: Codex's own client-side thread
> manager drives them, spawning a local sub-agent thread and making its own
> separate upstream Chat Completions call per thread. The proxy does not
> orchestrate any of this — it just passes the `spawn_agent`/`send_message`/…
> tool definitions and calls through like any other tool, restoring the
> `namespace` tag Chat Completions has no wire field for so Codex's own
> `(name, namespace)`-keyed call routing accepts them instead of rejecting
> them as `unsupported call`. Enabling these flags means opting into
> upstream-experimental behavior that can change or break independent of
> this proxy.
>
> **Having trouble with sub-agents? Turn the flags back off** — this is a
> Codex-side setting, not something the proxy controls:
> ```toml
> [features]
> multi_agent = false
>
> [features.multi_agent_v2]
> enabled = false
> ```
> With both off, Codex never advertises the `collaboration` tools and the
> proxy's namespace handling is simply a no-op — no other behavior changes.
>
> **Confirmed: intermittent empty sub-agent task text is a Codex-side bug,
> not this proxy.** Trace-log inspection of a live `multi_agent_v2` session
> showed the root agent's own `spawn_agent`/`followup_task` tool call
> arriving with a full, non-empty `message` field, but the very next
> upstream request — the new sub-agent's own separate call, built entirely
> by Codex's client-side thread manager — already carried an empty
> `Payload` in its `NEW_TASK` envelope. That envelope format is not
> constructed anywhere in this proxy; it is opaque text Codex hands the
> proxy to relay as-is, so the text is lost before it ever reaches the
> proxy over the wire. If sub-agents keep reporting an empty payload after
> a retry, turning the flags off above is the only known workaround.

Codex ≥ 0.134 no longer reads inline `[profiles.<name>]` tables — put the
profile in its own overlay file at `~/.codex/<name>.config.toml` with top-level
keys (it inherits everything else, including `[model_providers.*]`, from the
base config). Create `~/.codex/proxy.config.toml`:

```toml
model = "gpt-5.6-sol"          # must match a model key in config.yaml
model_provider = "responses_proxy"
model_reasoning_effort = "high"
```

```bash
codex --profile proxy
```

> **Pick your backend at the start of a session — don't switch providers
> mid-session.** The replayed history carries backend-specific item ids, so
> moving a live session to a different backend triggers id-prefix rejections.
> Switching models *within* a provider (`/model`) is fine.

> **Multiple parallel instances** are already isolated by their unique response
> ids. If you want strict cache separation regardless, set the optional
> `x-responses-proxy-namespace` header per instance via
> `[model_providers.<id>].http_headers = { "x-responses-proxy-namespace" = "alpha" }`
> — different namespaces get separate stores. Not needed in normal use.

## How It Works

```
Client (Responses API)  →  POST /v1/responses or WS  →  Convert  →  POST /chat/completions  →  Provider
                              ↑                                                              ↓
                              └──────────────── Convert response back ───────────────────────┘
```

## Quick Start

```bash
# Edit config.yaml with your provider details, then start
cargo run
# Listening on 0.0.0.0:3000
```

```bash
# List configured models
curl http://localhost:3000/v1/models

# Send a Responses API request
curl http://localhost:3000/v1/responses \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-5.5",
    "input": "What is 2+2? Reply with just the number."
  }'
```

## Configuration (`config.yaml`)

```yaml
server:
  listen: "0.0.0.0:3000"       # default
  timeout: 600                  # default request timeout in seconds

  # Log level: trace, debug, info, warn, error (default: info)
  # Overridden by RUST_LOG env var if set.
  log-level: info

  # Authentication – if any keys are set, auth is required
  auth:
    keys: []
    # Example with keys:
    # keys:
    #   - sk-your-key-here

  # CORS allow origins. Empty = allow any.
  cors:
    allow-origins: []

  # Allowed tool types (default: ["function"])
  allowed-tool-types:
    - function

models:
  gpt-5.5:
    provider:
      base-url: https://api.deepseek.com
      api-key: $DEEPSEEK_API_KEY # or static key
    model: deepseek-v4-pro # optional, defaults to the key name
    # Optional JSON rewrites for provider compatibility.
    # Stages run at fixed points in the proxy pipeline:
    # response-in  -> after the OpenAI-compatible Responses request is parsed
    # chat-out     -> before sending the converted Chat request upstream
    # chat-in      -> after receiving Chat response, before Chat -> Responses conversion
    # response-out -> before returning the final Responses body
    rewrite:
      - at: chat-out
        rename:
          max_completion_tokens: max_tokens
      - at: response-out
        remove:
          - parallel_tool_calls

  codex-auto-review:
    provider:
      base-url: https://api.deepseek.com
      api-key: $DEEPSEEK_API_KEY
    model: deepseek-v4-flash
    # Set to false if the upstream rejects `stream: true` together with a
    # structured `response_format`; the proxy then buffers a non-streamed
    # reply and replays it as SSE. Defaults to true (stream through unchanged).
    # stream-structured-output: true
```

### Rewrite Rules

`rewrite` lets each configured model patch JSON bodies at four points in the pipeline. The proxy still accepts and emits OpenAI-compatible typed bodies at its public boundary; rewrite is a provider-compatibility layer applied after a body has already matched the relevant OpenAI-shaped struct.

```text
client
  -> Responses typed request
  -> response-in
  -> Chat typed request
  -> chat-out
  -> upstream provider
  -> chat-in
  -> Chat typed response
  -> Responses typed response
  -> response-out
  -> client
```

Inside each stage, rules are chained and run in the order they are written. Later rules see changes made by earlier rules, so put more specific matches before broader matches when they touch the same path. Supported operations:

```yaml
rewrite:
  - at: response-in
    reset:
      max_output_tokens: 2048
  - at: chat-out
    rename:
      max_completion_tokens: max_tokens
    replace:
      reasoning_effort:
        - if: "^(high|xhigh)$"
          set: max
          with:
            thinking:
              type: enable
        - if: "^(minimal|low|medium)$"
          set: high
          with:
            provider_options.reasoning:
              enabled: true
              budget: 4096
  - at: chat-in
    remove:
      - system_fingerprint
  - at: response-out
    replace:
      service_tier:
        - if: auto
          set: priority
        - if: priority
          set: default
```

Paths can be field names (`model`), dotted paths (`response_format.type`), or JSON pointers (`/response_format/type`). `reset` overwrites a value only when the target path already exists, `rename` moves a value, `remove` deletes paths, and `replace` writes `set` to the current replace key when the current value matches `if`. String `if` values are Rust regex patterns; use `^...$` for exact matches. Add `with` when one match should also write other target paths; `with` is an ordered map of target path to JSON value. Use a `with` key of `""` to target the whole body. `reset`, `replace.set`, and `replace.with` can write any JSON value, including maps and provider-specific fields that are not part of the OpenAI typed structs, because rewrites operate on JSON bodies after the typed OpenAI-compatible parse/serialization step for that stage.

Configuration is strict: unknown proxy config fields, unused rewrite profiles, empty rewrite stages, invalid `if` regex patterns, and replace rules that write the current path again through `with` fail at startup.

Reusable profiles can be placed under top-level `rewrites` and referenced by model:

```yaml
models:
  gpt-5.5:
    model: deepseek-v4-pro
    rewrite: deepseek
    provider:
      api-key: $DEEPSEEK_API_KEY
      base-url: https://api.deepseek.com

rewrites:
  deepseek:
    at: chat-out
    rename:
      max_completion_tokens: max_tokens
```

## Endpoints

| Method | Path            | Auth     | Description                                       |
| ------ | --------------- | -------- | ------------------------------------------------- |
| `GET`  | `/health`       | No       | Health check                                      |
| `GET`  | `/v1/models`    | Optional | List configured models (OpenAI-compatible format) |
| `POST` | `/v1/responses` | Optional | Main proxy endpoint                               |

## Supported Conversions

### Request: Responses API → Chat API

| Responses Field                                          | Chat Field              | Notes                                                                                                     |
| -------------------------------------------------------- | ----------------------- | --------------------------------------------------------------------------------------------------------- |
| `input` (string or array)                                | `messages`              | String → `[{role:"user", content}]`. Array → converts messages, function_call, function_call_output items |
| `instructions`                                           | system message          | Prepended; merged with existing system/developer messages in input                                        |
| `reasoning`                                              | `reasoning_effort`      | Provider-specific fields such as DeepSeek `thinking` can be added with a `chat-out` rewrite `replace`      |
| `max_output_tokens`                                      | `max_completion_tokens` | Use a `chat-out` rewrite `rename` for legacy providers that require `max_tokens`                           |
| `tools` (flat)                                           | `tools` (nested)        | Wraps fields under `function` key; filtered by `allowed-tool-types`                                      |
| `tool_choice`                                            | `tool_choice`           | Passthrough                                                                                               |
| `temperature`, `top_p`, `stream`, `stop`, `top_logprobs` | same                    | Passthrough                                                                                               |

### Response: Chat API → Responses API

| Chat Field                                    | Responses Field                            | Notes                                   |
| --------------------------------------------- | ------------------------------------------ | --------------------------------------- |
| `choices[0].message.content`                  | `output[{type:"message"}]`                 | Wrapped in `output_text` content blocks |
| `choices[0].message.tool_calls`               | `output[{type:"function_call"}]`           |                                         |
| `finish_reason=content_filter` + null content | `output[{type:"refusal"}]`                 |                                         |
| `usage.prompt_tokens`                         | `usage.input_tokens`                       |                                         |
| `prompt_cache_hit/miss_tokens`                | `usage.input_tokens_details.cached_tokens` | Sum of hit + miss                       |

## Streaming

Set `"stream": true` in the Responses API request. The proxy converts Chat API SSE chunks into Responses API streaming events (`response.created` → `response.output_text.delta` → `response.completed`). Tool call deltas are accumulated across chunks and emitted in the final event.

**Structured output over streaming.** Some gateways reject `stream: true` combined with a structured `response_format` (`json_schema` / `json_object`) — a request pattern Codex's Guardian judge uses. Set `stream-structured-output: false` on the affected model and the proxy fetches that reply non-streamed, then replays it to the client as a single SSE burst (`response.created` → `response.output_text.delta` → `response.completed`). This is transparent to the client, and streaming deltas are of no use for structured output anyway (partial JSON isn't parseable). The default is `true`, so providers that stream structured output natively (e.g. OpenAI) are unaffected.

## History & Compaction

Codex CLI runs with `store: false` and replays the **entire conversation history inline** in `input` on every turn — tool calls, tool outputs, reasoning, and compaction items included. The proxy expands that `input` into Chat API `messages` per request; it does not rely on server-side accumulation for Codex traffic.

Because tool outputs dominate replayed history (~78% of bytes in real sessions), the proxy applies **age-based tool-output truncation** on the Responses → Chat path. "Age" is measured in tool-output depth, not user turns — a single prompt can spawn dozens of sequential tool calls with no new user message in between, so the last 8 tool results (`KEEP_LAST_TOOL_OUTPUTS`) are kept verbatim regardless of how many user turns they span; older outputs larger than 4 KB are reduced to `head (~2 KB) + …[truncated N bytes]… + tail (~1 KB)`, preserving the start and end of each output while cutting the bulk. Tool-call/tool-output pairing (`tool_call_id`) is never broken — only the textual content of old outputs shrinks. Truncation is UTF-8-boundary safe. This runs independently of, and before, the `history.max-input-chars` hard budget below, which still applies as a separate backstop over the whole converted request.

Compaction is driven by Codex's **remote compaction v2**: it sends a `compaction_trigger` item (with the full history inline) and expects a single `compaction` output item back. The proxy summarizes the **inline `input` history** (not the empty server-side store) and returns the encrypted/plain summary. Codex then rebuilds its post-compaction history locally. The standalone `POST /v1/responses/compact` endpoint behaves the same way. Server-side store is only populated when a client opts in with `store: true`.

Requests are transparently decompressed (gzip, brotli, zstd, deflate) — Codex sends zstd-compressed bodies. Malformed request bodies produce a structured OpenAI-style `400 invalid_request_error`; the proxy logs only a length-capped, lossy-UTF-8 preview, never raw bytes.

## Authentication

When `server.auth.keys` contains at least one key, requests to authenticated endpoints require an `Authorization: Bearer <key>` header that matches one of the configured keys. `/health` is always open.

## Tool Type Allowlist

`server.allowed-tool-types` controls which tool types pass through to the downstream provider. Default is `["function"]`. Any tool in the Responses API request whose `type` is not in this list is silently dropped. For example, to also allow web search tools from compatible providers:

```yaml
server:
  allowed-tool-types:
    - function
    - web_search_preview
```

## Environment Variable References

`base-url` and `api-key` support `$VAR` environment variable references:

```yaml
provider:
  base-url: $MY_BASE_URL        # reads from $MY_BASE_URL
  api-key: $DEEPSEEK_API_KEY    # reads from $DEEPSEEK_API_KEY
  api-key: sk-plain-text-key    # static key
```
