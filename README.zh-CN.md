# responses-proxy

将 **OpenAI Responses API** 请求转换为 **Chat Completions API** 请求，并把 Chat 响应转换回 Responses 格式的代理。代理内部使用的 Responses、Chat Completions 请求体和响应体结构体，都直接参照 OpenAI 官网对应 API 定义建模。支持 HTTP SSE 和 WebSocket 流式传输、推理/思考内容以及工具调用。可作为 **Codex CLI** 的后端直接使用。

## 特性

- **HTTP SSE & WebSocket** — 同时支持 `POST /v1/responses`（SSE）和 `GET /v1/responses`（WebSocket 升级）
- **OpenAI 兼容 typed 边界** — 公开 `/v1/responses` 入口必须先能反序列化成 OpenAI Responses API 请求体；上游/下游 Chat body 使用 OpenAI Chat Completions 请求/响应结构
- **推理 / 思考** — 将 Responses `reasoning.effort` 映射为 Chat `reasoning_effort`；DeepSeek 等 provider 专用 thinking 字段可通过 `rewrite` 添加
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

### 独立 profile（按会话切换后端）

`openai_base_url` 是全局设置。如果想按会话在代理与其他后端之间切换，可以定义一个
provider，并用 `codex --profile <name>` 选择对应 profile。

在 `~/.codex/config.toml` 中声明 provider：

```toml
[model_providers.responses_proxy]
name = "responses-proxy (local)"
base_url = "http://localhost:3000/v1"
# wire_api 默认为 "responses"（正是代理所说的协议）；除非在 config.yaml 中启用
# server.auth.keys，否则无需 env_key。
```

> **子代理（`features.multi_agent` / `features.multi_agent_v2`）**是 Codex
> 本地的 `Experimental`（实验性）功能：由 Codex 自身的客户端线程管理器驱动，
> 会在本地派生一个子代理线程，并为该线程单独发起一次上游 Chat Completions
> 调用。代理（proxy）并不参与编排这一切——它只是像转发其他工具一样，原样转发
> `spawn_agent`/`send_message`/… 这些工具定义与调用，并恢复 Chat Completions
> 协议本身不携带的 `namespace` 标记，好让 Codex 自身按 `(name, namespace)`
> 路由调用时能够识别，而不是拒绝为 `unsupported call`。启用这些开关意味着你在
> 使用上游的实验性行为，其表现可能独立于本代理发生变化甚至失效。
>
> **如果子代理出现问题，把开关关掉即可**——这是 Codex 一侧的设置，本代理并不
> 控制它：
> ```toml
> [features]
> multi_agent = false
>
> [features.multi_agent_v2]
> enabled = false
> ```
> 两者都关闭后，Codex 就不会再下发 `collaboration` 工具集，代理的 namespace
> 处理逻辑自然变成空操作——不影响其他任何行为。

Codex ≥ 0.134 不再读取内联的 `[profiles.<name>]` 表——请把 profile 放到独立的
overlay 文件 `~/.codex/<name>.config.toml` 中，使用顶层键（其余设置，包括
`[model_providers.*]`，都从基础配置继承）。创建 `~/.codex/proxy.config.toml`：

```toml
model = "gpt-5.6-sol"          # 必须与 config.yaml 中的某个模型键一致
model_provider = "responses_proxy"
model_reasoning_effort = "high"
```

```bash
codex --profile proxy
```

> **在会话开始时选定后端——不要在会话中途切换 provider。** 回放的历史携带
> backend-specific 的 item id，将进行中的会话切换到其他后端会触发 id 前缀拒绝。
> 在同一 provider 内切换模型（`/model`）则是安全的。

> **多个并行实例**已经通过各自唯一的 response id 天然隔离。若仍想严格隔离缓存，可
> 通过 `[model_providers.<id>].http_headers = { "x-responses-proxy-namespace" = "alpha" }`
> 为每个实例设置可选的 `x-responses-proxy-namespace` 头——不同 namespace 使用各自
> 独立的存储。常规使用无需配置。

## 工作原理

```
客户端 (Responses API)  →  POST /v1/responses 或 WS  →  转换  →  POST /chat/completions  →  服务商
                              ↑                                                              ↓
                              └───────────────────────── 转换响应并返回 ───────────────────────┘
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
  listen: "0.0.0.0:3000"       # 默认值
  timeout: 600                  # 默认请求超时（秒）

  # 日志级别: trace, debug, info, warn, error（默认: info）
  # 可通过 RUST_LOG 环境变量覆盖。
  log-level: info

  # 鉴权 — 只要 keys 非空即启用鉴权
  auth:
    keys: []
    # 示例（启用鉴权）:
    # keys:
    #   - sk-你的密钥

  # CORS 允许来源。空 = 允许任意来源。
  cors:
    allow-origins: []

  # 允许的工具类型（默认只允许 function）
  allowed-tool-types:
    - function

models:
  gpt-5.5: # 暴露给 Responses API 客户端的模型名
    provider:
      base-url: https://api.deepseek.com
      api-key: $DEEPSEEK_API_KEY # 或直接写静态密钥
    model: deepseek-v4-pro # 可选，默认等于键名
    # 可选：按阶段重写 JSON body，用于适配不同上游。
    # response-in  -> OpenAI 兼容 Responses 请求解析完成后
    # chat-out     -> 转换后的 Chat 请求发给上游前
    # chat-in      -> 收到上游 Chat 响应后、转 Responses 前
    # response-out -> 最终 Responses 响应返回客户端前
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
    # 若上游拒绝 `stream: true` 与结构化 `response_format` 组合，设为 false；
    # 代理会以非流式方式获取响应，再作为 SSE 回放给客户端。默认 true（原样透传流式）。
    # stream-structured-output: true
```

### Rewrite 规则

`rewrite` 可以在每个模型下配置，按代理流程中的四个位置修改 JSON body。公开接口仍然以 OpenAI 兼容 typed body 为边界；rewrite 是 provider 兼容层，会在当前阶段的 body 已经符合对应 OpenAI 结构后再执行。

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

每个 stage 内的规则是链式执行，并且按配置书写顺序执行。后面的规则会看到前面规则已经改过的值；同一个 path 上更具体的匹配应该放在更宽泛的匹配前面。

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

路径支持字段名、点路径和 JSON Pointer。`reset` 只在目标路径已存在时覆盖，`rename` 移动字段，`remove` 删除字段，`replace` 在当前值匹配 `if` 时把 `set` 写入当前 replace key。字符串形式的 `if` 是 Rust regex 正则表达式；需要精确匹配时用 `^...$`。如果一次匹配还要写其它位置，可以配置 `with`；`with` 是有序的 target path 到 JSON value 的 map。使用 `with` 里的 `""` key 可以指向整个 body。`reset`、`replace.set` 和 `replace.with` 都可以写入任意 JSON 值，包括 map，以及不属于 OpenAI typed struct 的 provider 专用字段，因为 rewrite 在当前阶段完成 OpenAI 兼容 typed 解析/序列化后操作 JSON body。

配置是严格校验的：未知的 proxy 配置字段、未被引用的 rewrite profile、空 rewrite stage、非法的 `if` 正则，以及同一条 replace 里通过 `with` 再次写当前 path，都会在启动解析配置时失败。

可复用规则可以放在顶层 `rewrites` 下，然后在 model 里用名称引用：

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

## 端点

| 方法   | 路径            | 鉴权 | 说明                        |
| ------ | --------------- | ---- | --------------------------- |
| `GET`  | `/health`       | 否   | 健康检查                    |
| `GET`  | `/v1/models`    | 可选 | 模型列表（OpenAI 兼容格式） |
| `POST` | `/v1/responses` | 可选 | 主代理端点                  |

## 请求转换

| Responses 字段                                                          | Chat 字段               | 说明                                                                          |
| ----------------------------------------------------------------------- | ----------------------- | ----------------------------------------------------------------------------- |
| `input`（字符串或数组）                                                 | `messages`              | 字符串→单条 user 消息；数组→转换 message、function_call、function_call_output |
| `instructions`                                                          | system 消息             | 前置插入，与 input 中已有的 system/developer 消息合并                         |
| `reasoning`                                                             | `reasoning_effort`      | DeepSeek `thinking` 等 provider 专用字段可通过 `chat-out` rewrite `replace` 添加 |
| `max_output_tokens`                                                     | `max_completion_tokens` | 旧上游需要 `max_tokens` 时可用 `chat-out` rewrite `rename` 适配                  |
| `tools`（扁平）                                                         | `tools`（嵌套）         | 收进 `function` 键下，受 `allowed-tool-types` 过滤                           |
| `tool_choice`、`temperature`、`top_p`、`stream`、`stop`、`top_logprobs` | 同                      | 透传                                                                          |

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

**结构化输出的流式传输。** 部分网关会拒绝 `stream: true` 与结构化 `response_format`（`json_schema` / `json_object`）的组合——Codex 的 Guardian 审查正是这种请求。为受影响的模型设置 `stream-structured-output: false`，代理便会以非流式方式获取该响应，再作为单次 SSE 回放给客户端（`response.created` → `response.output_text.delta` → `response.completed`）。对客户端完全透明，而结构化输出本就无法利用流式增量（不完整的 JSON 无法解析）。默认值为 `true`，因此原生支持结构化流式输出的提供方（如 OpenAI）不受影响。

## 鉴权

当 `server.auth.keys` 至少包含一个 key 时，需要鉴权的端点必须携带 `Authorization: Bearer <key>` 请求头，且 key 必须在配置的 keys 列表中。`/health` 始终免鉴权。

## 允许的工具类型

`server.allowed-tool-types` 控制哪些工具类型能够透传到下游。默认只有 `function`。请求中 type 不在允许列表内的 tool 会被静默过滤。例如，如果下游支持联网搜索：

```yaml
server:
  allowed-tool-types:
    - function
    - web_search_preview
```

## 环境变量引用

`base-url` 和 `api-key` 支持 `$变量名` 方式引用环境变量：

```yaml
provider:
  base-url: $MY_BASE_URL        # 从环境变量读取
  api-key: $DEEPSEEK_API_KEY    # 从环境变量读取
  api-key: sk-明文密钥           # 静态密钥
```
