# responses-proxy

A proxy that converts **OpenAI Responses API** to **Chat Completions API** and back. Supports both HTTP SSE and WebSocket streaming, reasoning/thinking content, and tool calling. Works as a drop-in **Codex CLI** backend via DeepSeek or any Chat API-compatible provider.

## Features

- **HTTP SSE & WebSocket** — both `POST /v1/responses` (SSE) and `GET /v1/responses` (WebSocket upgrade)
- **Reasoning / Thinking** — maps `reasoning.effort` to DeepSeek thinking mode, streams `reasoning_text.delta` events
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
  log_level: info

  # Authentication – if any keys are set, auth is required
  auth:
    keys: []
    # Example with keys:
    # keys:
    #   - sk-your-key-here

  # CORS allow origins. Empty = allow any.
  cors:
    allow_origins: []

  # Tool type allowlist (default: ["function"])
  tool_type_allowlist:
    - function

models:
  gpt-5.5:
    provider:
      base_url: https://api.deepseek.com
      api_key: $DEEPSEEK_API_KEY # or static key
    model: deepseek-v4-pro # optional, defaults to the key name

  codex-auto-review:
    provider:
      base_url: https://api.deepseek.com
      api_key: $DEEPSEEK_API_KEY
    model: deepseek-v4-flash
```

## Endpoints

| Method | Path            | Auth     | Description                                       |
| ------ | --------------- | -------- | ------------------------------------------------- |
| `GET`  | `/health`       | No       | Health check                                      |
| `GET`  | `/v1/models`    | Optional | List configured models (OpenAI-compatible format) |
| `POST` | `/v1/responses` | Optional | Main proxy endpoint                               |

## Supported Conversions

### Request: Responses API → Chat API

| Responses Field                                          | Chat Field       | Notes                                                                                                     |
| -------------------------------------------------------- | ---------------- | --------------------------------------------------------------------------------------------------------- |
| `input` (string or array)                                | `messages`       | String → `[{role:"user", content}]`. Array → converts messages, function_call, function_call_output items |
| `instructions`                                           | system message   | Prepended; merged with existing system/developer messages in input                                        |
| `reasoning`                                              | `thinking`       | Maps to DeepSeek `thinking: {type: "enabled"}`                                                            |
| `max_output_tokens`                                      | `max_tokens`     |                                                                                                           |
| `tools` (flat)                                           | `tools` (nested) | Wraps fields under `function` key; filtered by `tool_type_allowlist`                                      |
| `tool_choice`                                            | `tool_choice`    | Passthrough                                                                                               |
| `temperature`, `top_p`, `stream`, `stop`, `top_logprobs` | same             | Passthrough                                                                                               |

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

`server.tool_type_allowlist` controls which tool types pass through to the downstream provider. Default is `["function"]`. Any tool in the Responses API request whose `type` is not in this list is silently dropped. For example, to also allow web search tools from compatible providers:

```yaml
server:
  tool_type_allowlist:
    - function
    - web_search_preview
```

## Environment Variable References

`base_url` and `api_key` support `$VAR` environment variable references:

```yaml
provider:
  base_url: $MY_BASE_URL        # reads from $MY_BASE_URL
  api_key: $DEEPSEEK_API_KEY    # reads from $DEEPSEEK_API_KEY
  api_key: sk-plain-text-key    # static key
```
