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

  # Maximum accepted request body size in bytes (default: 33554432, 32 MiB).
  # The HTTP stack rejects larger bodies with 413 Payload Too Large before the
  # proxy sees them. Raise it when long sessions, large tool outputs, or image
  # parts trip "Failed to buffer the request body: length limit exceeded".
  max-body-bytes: 33554432

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
