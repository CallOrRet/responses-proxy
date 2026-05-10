# responses-proxy

将 **OpenAI Responses API** 请求转换为 **Chat Completions API** 格式的代理，使任何兼容 Chat API 的服务商（如 DeepSeek）都能服务 Responses API 客户端。支持 HTTP SSE 和 WebSocket 流式传输、推理/思考内容以及工具调用。可作为 **Codex CLI** 的后端直接使用。

## 特性

- **HTTP SSE & WebSocket** — 同时支持 `POST /v1/responses`（SSE）和 `GET /v1/responses`（WebSocket 升级）
- **推理 / 思考** — 将 `reasoning.effort` 映射为 DeepSeek 思考模式，流式输出 `reasoning_text.delta` 事件
- **工具调用** — 完整的 `function_call` / `function_call_output` 往返，消息顺序正确
- **Codex CLI 兼容** — 处理 warmup、`previous_response_id` 续接以及完整的流式事件链
- **多模型** — 支持按模型配置不同的下游服务商

## 与 Codex CLI 配合使用

启动 responses-proxy 后，在 `~/.codex/config.toml` 文件头部添加以下配置：

```toml
openai_base_url = "http://localhost:3000/v1"
```

之后启动 Codex 即可通过代理使用。

```bash
codex        # 使用 gpt-5.5 模型
codex review # 使用 codex-auto-review 模型（如已配置）
```

## 工作原理

```
客户端 (Responses API)  →  POST /v1/responses 或 WS  →  转换  →  POST /chat/completions  →  服务商
                              ↑                                                              ↓
                              └──────────────── 转换响应并返回 ──────────────────────────────┘
```

## 快速开始

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
    "model": "gpt-5.5",
    "input": "1+1等于几？只回复数字。"
  }'
```

## 配置说明 (`config.yaml`)

```yaml
server:
  listen_addr: "0.0.0.0:3000"
  request_timeout: 30

  # 日志级别: trace, debug, info, warn, error（默认: info）
  # 可通过 RUST_LOG 环境变量覆盖。
  log_level: info

  # 鉴权配置（可选）
  auth:
    enabled: false # 设为 true 则要求 API Key
    keys:
      - sk-你的密钥

  # 工具类型白名单（默认只允许 function）
  tool_type_allowlist:
    - function

models:
  - model: gpt-5.5 # 暴露给 Responses API 客户端的模型名
    provider:
      base_url: https://api.deepseek.com
      api_key: $DEEPSEEK_API_KEY # 或直接写静态密钥
    downstream_model: deepseek-v4-pro # 可选，默认等于 model

  - model: codex-auto-review
    provider:
      base_url: https://api.deepseek.com
      api_key: $DEEPSEEK_API_KEY
    downstream_model: deepseek-v4-flash
```

## 端点

| 方法   | 路径            | 鉴权 | 说明                        |
| ------ | --------------- | ---- | --------------------------- |
| `GET`  | `/health`       | 否   | 健康检查                    |
| `GET`  | `/v1/models`    | 可选 | 模型列表（OpenAI 兼容格式） |
| `POST` | `/v1/responses` | 可选 | 主代理端点                  |

## 请求转换

| Responses 字段                                                          | Chat 字段       | 说明                                                                          |
| ----------------------------------------------------------------------- | --------------- | ----------------------------------------------------------------------------- |
| `input`（字符串或数组）                                                 | `messages`      | 字符串→单条 user 消息；数组→转换 message、function_call、function_call_output |
| `instructions`                                                          | system 消息     | 前置插入，与 input 中已有的 system/developer 消息合并                         |
| `reasoning`                                                             | `thinking`      | 映射为 DeepSeek 的 `thinking: {type: "enabled"}`                              |
| `max_output_tokens`                                                     | `max_tokens`    |                                                                               |
| `tools`（扁平）                                                         | `tools`（嵌套） | 收进 `function` 键下，受 `tool_type_allowlist` 过滤                           |
| `tool_choice`、`temperature`、`top_p`、`stream`、`stop`、`top_logprobs` | 同              | 透传                                                                          |

## 响应转换

| Chat 字段                               | Responses 字段                             | 说明                        |
| --------------------------------------- | ------------------------------------------ | --------------------------- |
| `choices[0].message.content`            | `output[{type:"message"}]`                 | 包裹为 `output_text` 内容块 |
| `choices[0].message.tool_calls`         | `output[{type:"function_call"}]`           |                             |
| `finish_reason=content_filter` + 空内容 | `output[{type:"refusal"}]`                 |                             |
| `usage.prompt_tokens`                   | `usage.input_tokens`                       |                             |
| `prompt_cache_hit/miss_tokens`          | `usage.input_tokens_details.cached_tokens` | hit + miss 求和             |

## 流式传输

在 Responses API 请求中设置 `"stream": true`。代理会将 Chat API SSE 数据块转换为 Responses API 流式事件（`response.created` → `response.output_text.delta` → `response.completed`）。Tool call 增量数据跨 chunk 累积后在最终事件中完整输出。

## 鉴权

当 `server.auth.enabled: true` 时，`/v1/models` 和 `/v1/responses` 需要在请求头中携带 `Authorization: Bearer <key>`，且 key 必须在 `server.auth.keys` 列表中。`/health` 始终免鉴权。

## 工具类型白名单

`server.tool_type_allowlist` 控制哪些工具类型能够透传到下游。默认只有 `function`。请求中 type 不在白名单内的 tool 会被静默过滤。例如，如果下游支持联网搜索：

```yaml
server:
  tool_type_allowlist:
    - function
    - web_search_preview
```

## 环境变量引用

`base_url` 和 `api_key` 支持 `$变量名` 方式引用环境变量：

```yaml
provider:
  base_url: $MY_BASE_URL        # 从环境变量读取
  api_key: $DEEPSEEK_API_KEY    # 从环境变量读取
  api_key: sk-明文密钥           # 静态密钥
```
