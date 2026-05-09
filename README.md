# responses-proxy

[English](#english) | [中文](#chinese)

---

<a name="english"></a>
## English

A proxy that converts **OpenAI Responses API** requests into **Chat Completions API** format and back, enabling any Chat API-compatible provider (e.g. DeepSeek) to serve Responses API clients.

### How It Works

```
Client (Responses API)  →  POST /v1/responses  →  Convert  →  POST /chat/completions  →  Provider (Chat API)
                                                  ↑                                  ↓
                                                  └──── Convert response back ───────┘
```

### Quick Start

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
    "model": "deepseek-v4-pro",
    "input": "What is 2+2? Reply with just the number."
  }'
```

### Configuration (`config.yaml`)

```yaml
server:
  listen_addr: "0.0.0.0:3000"
  request_timeout_secs: 120

  # Authentication (optional)
  auth:
    enabled: false          # Set to true to require API key
    keys:
      - sk-your-key-here

  # Tool type allowlist (default: ["function"])
  tool_type_allowlist:
    - function

models:
  - model: deepseek-v4-pro
    provider:
      base_url: https://api.deepseek.com
      api_key: sk-xxx                    # or: os.environ/DEEPSEEK_API_KEY
    downstream_model: deepseek-chat      # optional, defaults to model

  - model: deepseek-v4-flash
    provider:
      base_url: https://api.deepseek.com
      api_key: sk-xxx
```

### Endpoints

| Method | Path | Auth | Description |
|---|---|---|---|
| `GET` | `/health` | No | Health check |
| `GET` | `/v1/models` | Optional | List configured models (OpenAI-compatible format) |
| `POST` | `/v1/responses` | Optional | Main proxy endpoint |

### Supported Conversions

#### Request: Responses API → Chat API

| Responses Field | Chat Field | Notes |
|---|---|---|
| `input` (string or array) | `messages` | String → `[{role:"user", content}]`. Array → converts messages, function_call, function_call_output items |
| `instructions` | system message | Prepended; merged with existing system/developer messages in input |
| `reasoning` | `thinking` | Maps to DeepSeek `thinking: {type: "enabled"}` |
| `max_output_tokens` | `max_tokens` | |
| `tools` (flat) | `tools` (nested) | Wraps fields under `function` key; filtered by `tool_type_allowlist` |
| `tool_choice` | `tool_choice` | Passthrough |
| `temperature`, `top_p`, `stream`, `stop`, `top_logprobs` | same | Passthrough |

#### Response: Chat API → Responses API

| Chat Field | Responses Field | Notes |
|---|---|---|
| `choices[0].message.content` | `output[{type:"message"}]` | Wrapped in `output_text` content blocks |
| `choices[0].message.tool_calls` | `output[{type:"function_call"}]` | |
| `finish_reason=content_filter` + null content | `output[{type:"refusal"}]` | |
| `usage.prompt_tokens` | `usage.input_tokens` | |
| `prompt_cache_hit/miss_tokens` | `usage.input_tokens_details.cached_tokens` | Sum of hit + miss |

### Streaming

Set `"stream": true` in the Responses API request. The proxy converts Chat API SSE chunks into Responses API streaming events (`response.created` → `response.output_text.delta` → `response.completed`). Tool call deltas are accumulated across chunks and emitted in the final event.

### Authentication

When `server.auth.enabled: true`, requests to `/v1/models` and `/v1/responses` require an `Authorization: Bearer <key>` header. The key must match one of the keys in `server.auth.keys`. `/health` is always open.

### Tool Type Allowlist

`server.tool_type_allowlist` controls which tool types pass through to the downstream provider. Default is `["function"]`. Any tool in the Responses API request whose `type` is not in this list is silently dropped. For example, to also allow web search tools from compatible providers:

```yaml
server:
  tool_type_allowlist:
    - function
    - web_search_preview
```

### API Key Resolution

The `api_key` field supports LiteLLM-style environment variable references:

```yaml
api_key: os.environ/DEEPSEEK_API_KEY   # reads from $DEEPSEEK_API_KEY
api_key: sk-plain-text-key             # static key
```

---

<a name="chinese"></a>
## 中文

将 **OpenAI Responses API** 请求转换为 **Chat Completions API** 格式的代理，使任何兼容 Chat API 的服务商（如 DeepSeek）都能服务 Responses API 客户端。

### 快速开始

```bash
# 编辑 config.yaml 配置下游服务商信息，然后启动
cargo run
# 监听在 0.0.0.0:3000
```

```bash
# 查看已配置的模型列表
curl http://localhost:3000/v1/models

# 发送 Responses API 格式的请求
curl http://localhost:3000/v1/responses \
  -H "Content-Type: application/json" \
  -d '{
    "model": "deepseek-v4-pro",
    "input": "1+1等于几？只回复数字。"
  }'
```

### 配置说明 (`config.yaml`)

```yaml
server:
  listen_addr: "0.0.0.0:3000"
  request_timeout_secs: 120

  # 鉴权配置（可选）
  auth:
    enabled: false          # 设为 true 则要求 API Key
    keys:
      - sk-你的密钥

  # 工具类型白名单（默认只允许 function）
  tool_type_allowlist:
    - function

models:
  - model: deepseek-v4-pro      # 暴露给 Responses API 客户端的模型名
    provider:
      base_url: https://api.deepseek.com
      api_key: sk-xxx                # 或使用 os.environ/环境变量名
    downstream_model: deepseek-chat  # 可选，默认等于 model
```

### 端点

| 方法 | 路径 | 鉴权 | 说明 |
|---|---|---|---|
| `GET` | `/health` | 否 | 健康检查 |
| `GET` | `/v1/models` | 可选 | 模型列表（OpenAI 兼容格式） |
| `POST` | `/v1/responses` | 可选 | 主代理端点 |

### 鉴权

当 `server.auth.enabled: true` 时，`/v1/models` 和 `/v1/responses` 需要在请求头中携带 `Authorization: Bearer <key>`，且 key 必须在 `server.auth.keys` 列表中。`/health` 始终免鉴权。

### 工具类型白名单

`server.tool_type_allowlist` 控制哪些工具类型能够透传到下游。默认只有 `function`。请求中 type 不在白名单内的 tool 会被静默过滤。例如，如果下游支持联网搜索：

```yaml
server:
  tool_type_allowlist:
    - function
    - web_search_preview
```

### API Key 解析

`api_key` 支持 LiteLLM 风格的环境变量引用：

```yaml
api_key: os.environ/DEEPSEEK_API_KEY   # 从环境变量读取
api_key: sk-明文密钥                    # 静态密钥
```

### 请求转换

| Responses 字段 | Chat 字段 | 说明 |
|---|---|---|
| `input`（字符串或数组） | `messages` | 字符串→单条 user 消息；数组→转换 message、function_call、function_call_output |
| `instructions` | system 消息 | 前置插入，与 input 中已有的 system/developer 消息合并 |
| `reasoning` | `thinking` | 映射为 DeepSeek 的 `thinking: {type: "enabled"}` |
| `max_output_tokens` | `max_tokens` | |
| `tools`（扁平） | `tools`（嵌套） | 收进 `function` 键下，受 `tool_type_allowlist` 过滤 |
| `tool_choice`、`temperature`、`top_p`、`stream`、`stop`、`top_logprobs` | 同 | 透传 |

### 响应转换

| Chat 字段 | Responses 字段 | 说明 |
|---|---|---|
| `choices[0].message.content` | `output[{type:"message"}]` | 包裹为 `output_text` 内容块 |
| `choices[0].message.tool_calls` | `output[{type:"function_call"}]` | |
| `finish_reason=content_filter` + 空内容 | `output[{type:"refusal"}]` | |
| `usage.prompt_tokens` | `usage.input_tokens` | |
| `prompt_cache_hit/miss_tokens` | `usage.input_tokens_details.cached_tokens` | hit + miss 求和 |

### 流式传输

在 Responses API 请求中设置 `"stream": true`。代理会将 Chat API SSE 数据块转换为 Responses API 流式事件（`response.created` → `response.output_text.delta` → `response.completed`）。Tool call 增量数据跨 chunk 累积后在最终事件中完整输出。
