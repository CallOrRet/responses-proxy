# responses-proxy

[English](#english) | [中文](#chinese)

---

<a name="english"></a>
## English

A proxy that converts **OpenAI Responses API** requests into **Chat Completions API** format and back, enabling any Chat API-compatible provider (e.g. DeepSeek, OpenAI, Azure) to serve Responses API clients.

### How It Works

```
Client (Responses API)  →  POST /v1/responses  →  Convert  →  POST /chat/completions  →  Provider (Chat API)
                                                  ↑                                  ↓
                                                  └──── Convert response back ───────┘
```

### Quick Start

```bash
# Set your downstream API key
export DOWNSTREAM_API_KEY="sk-your-key-here"

# Optional: customize downstream URL (default: DeepSeek)
export DOWNSTREAM_URL="https://api.deepseek.com"

# Start the proxy
cargo run
# Listening on 0.0.0.0:3000
```

```bash
# Use it — send a Responses API request
curl http://localhost:3000/v1/responses \
  -H "Content-Type: application/json" \
  -d '{
    "model": "deepseek-chat",
    "input": "What is 2+2? Reply with just the number."
  }'
```

Response:
```json
{
  "id": "resp_...",
  "object": "response",
  "status": "completed",
  "model": "deepseek-chat",
  "output": [{
    "type": "message",
    "role": "assistant",
    "content": [{"type": "output_text", "text": "4", "annotations": []}]
  }],
  "usage": {
    "input_tokens": 14,
    "output_tokens": 1,
    "total_tokens": 15,
    "input_tokens_details": {"cached_tokens": 0},
    "output_tokens_details": {"reasoning_tokens": 0}
  }
}
```

### Supported Conversions

#### Request: Responses API → Chat API

| Responses Field | Chat Field | Notes |
|---|---|---|
| `input` (string or array) | `messages` | String → `[{role:"user", content}]`. Array → converts messages, function_call, function_call_output items |
| `instructions` | system message | Prepended; merged with existing system messages in input |
| `reasoning` | `thinking` | Maps to DeepSeek `thinking: {type: "enabled"}` |
| `max_output_tokens` | `max_tokens` | |
| `tools` (flat) | `tools` (nested) | Wraps fields under `function` key; non-`function` types filtered |
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

### Configuration

| Env Variable | Default | Description |
|---|---|---|
| `DOWNSTREAM_API_KEY` | *(required)* | API key for the downstream provider |
| `DOWNSTREAM_URL` | `https://api.deepseek.com` | Downstream Chat Completions API base URL |
| `LISTEN_ADDR` | `0.0.0.0:3000` | Proxy listen address |
| `REQUEST_TIMEOUT_SECS` | `120` | Downstream request timeout in seconds |

### Streaming

Set `"stream": true` in the Responses API request. The proxy converts Chat API SSE chunks into Responses API streaming events (`response.created` → `response.output_text.delta` → `response.completed`). Tool call deltas are accumulated across chunks and emitted in the final event.

### Endpoints

| Method | Path | Description |
|---|---|---|
| `POST` | `/v1/responses` | Main proxy endpoint |
| `GET` | `/health` | Health check — returns `"OK"` |

---

<a name="chinese"></a>
## 中文

将 **OpenAI Responses API** 请求转换为 **Chat Completions API** 格式的代理，使任何兼容 Chat API 的服务商（如 DeepSeek、OpenAI、Azure）都能服务 Responses API 客户端。

### 工作原理

```
客户端 (Responses API)  →  POST /v1/responses  →  请求转换  →  POST /chat/completions  →  下游服务商 (Chat API)
                                                      ↑                                      ↓
                                                      └──────── 响应转换回 Responses 格式 ─────┘
```

### 快速开始

```bash
# 设置下游 API Key
export DOWNSTREAM_API_KEY="sk-你的密钥"

# 可选：自定义下游 URL（默认使用 DeepSeek）
export DOWNSTREAM_URL="https://api.deepseek.com"

# 启动代理
cargo run
# 监听在 0.0.0.0:3000
```

```bash
# 发送 Responses API 格式的请求
curl http://localhost:3000/v1/responses \
  -H "Content-Type: application/json" \
  -d '{
    "model": "deepseek-chat",
    "input": "1+1等于几？只回复数字。"
  }'
```

### 支持的转换

#### 请求：Responses API → Chat API

| Responses 字段 | Chat 字段 | 说明 |
|---|---|---|
| `input`（字符串或数组） | `messages` | 字符串→单条 user 消息；数组→转换 message、function_call、function_call_output 项 |
| `instructions` | system 消息 | 前置插入；与 input 中已有的 system 消息合并 |
| `reasoning` | `thinking` | 映射为 DeepSeek 的 `thinking: {type: "enabled"}` |
| `max_output_tokens` | `max_tokens` | |
| `tools`（扁平） | `tools`（嵌套） | 字段收进 `function` 键下；非 `function` 类型被过滤 |
| `tool_choice` | `tool_choice` | 透传 |
| `temperature`、`top_p`、`stream`、`stop`、`top_logprobs` | 同 | 透传 |

#### 响应：Chat API → Responses API

| Chat 字段 | Responses 字段 | 说明 |
|---|---|---|
| `choices[0].message.content` | `output[{type:"message"}]` | 包裹为 `output_text` 内容块 |
| `choices[0].message.tool_calls` | `output[{type:"function_call"}]` | |
| `finish_reason=content_filter` + 空内容 | `output[{type:"refusal"}]` | |
| `usage.prompt_tokens` | `usage.input_tokens` | |
| `prompt_cache_hit/miss_tokens` | `usage.input_tokens_details.cached_tokens` | hit + miss 求和 |

### 环境变量

| 变量 | 默认值 | 说明 |
|---|---|---|
| `DOWNSTREAM_API_KEY` | *(必填)* | 下游 API 密钥 |
| `DOWNSTREAM_URL` | `https://api.deepseek.com` | 下游 Chat API 地址 |
| `LISTEN_ADDR` | `0.0.0.0:3000` | 代理监听地址 |
| `REQUEST_TIMEOUT_SECS` | `120` | 下游请求超时（秒） |

### 流式传输

在 Responses API 请求中设置 `"stream": true` 即可。代理会将 Chat API 的 SSE 数据块转换为 Responses API 的流式事件（`response.created` → `response.output_text.delta` → `response.completed`）。Tool call 的增量数据会跨数据块累积，在最终事件中完整输出。

### 端点

| 方法 | 路径 | 说明 |
|---|---|---|
| `POST` | `/v1/responses` | 主代理端点 |
| `GET` | `/health` | 健康检查 — 返回 `"OK"` |
