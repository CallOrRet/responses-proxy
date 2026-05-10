# 多 Provider Chat API 兼容方案

## 一、前提：已完成的修复

在 openai-go SDK + DeepSeek 文档对比分析后，确认当前硬编码只支持 DeepSeek 下游，存在 8 个真实问题，已全部修复：

| # | 级别 | 问题 | 修复文件 |
|---|------|------|----------|
| 1 | P0 | 流式 `response.completed` 缺 usage | `streaming.rs` |
| 2 | P1 | 流式 output_index 在 delta 和 completed 之间不一致 | `streaming.rs` |
| 3 | P1 | SSE 路径缺 `response.in_progress` | `streaming.rs`, `main.rs` |
| 4 | P1 | 流式事件缺 `sequence_number` | `main.rs` |
| 5 | P2 | `insufficient_system_resource` 映射为非法值 | `convert_response.rs` |
| 6 | P2 | WS `response.created`/`in_progress` 字段不全 | `main.rs` |
| 7 | P2 | WS 硬编码 `stop: None, text: None` | `main.rs` |
| 8 | P2 | cached_tokens 提取路径不对（DeepSeek 无 `prompt_tokens_details`） | `convert_response.rs` |

新增 `src/verification_tests.rs` 覆盖 26 个端到端转换场景（33 个测试），全部通过。

---

## 二、为什么需要多 Provider 支持

当前代码硬编码 DeepSeek API 行为。OpenAI Chat API 和 DeepSeek Chat API 之间差异见 `issue.md`。核心差异：

| 维度 | DeepSeek | OpenAI |
|------|----------|--------|
| 推理开关 | `thinking: {"type":"enabled"}` 顶层字段 | 无此字段，`reasoning_effort:"none"` 即关闭 |
| 推理力度 | `reasoning_effort: "high"/"max"` 顶层字段 | `reasoning_effort: "none"/"minimal"/"low"/"medium"/"high"/"xhigh"` 顶层字段 |
| Max tokens | `max_tokens` | `max_completion_tokens`（推荐） |
| 消息角色 | 无 `developer` 角色 | 支持 `developer` 角色 |
| 推理内容 | 响应含 `reasoning_content` | 响应无 `reasoning_content` |
| 专属字段 | `thinking`, `user_id` | `verbosity`, `safety_identifier`, `modalities`, `audio`, `seed`, `n`, `service_tier` |

---

## 三、设计原则

1. **强类型核心不变** — `responses_to_chat()` / `chat_to_responses()` 不做 JSON 万能转换器，保持类型安全
2. **post-processing 层** — 在 `ChatCompletionRequest` 序列化成 `serde_json::Value` 后，通过 provider profile 做 rename/inject/drop/values
3. **配置驱动** — provider 差异在 `config.yaml` 声明，不碰 Rust 代码即可切换/新增 provider

---

## 四、config.yaml 设计

```yaml
providers:
  deepseek:
    chat:
      # 字段重命名: 内部 JSON key → provider JSON key
      rename: {}

      # 注入额外顶层字段（${reasoning_effort} 运行时替换）
      inject:
        thinking:
          type: "enabled"
          reasoning_effort: "${reasoning_effort}"

      # 删除 provider 不支持的字段
      drop:
        - "frequency_penalty"
        - "max_completion_tokens"

      # 字段值映射 (null = 删除该字段)
      values:
        reasoning_effort:
          none: null
          minimal: "high"
          low: "high"
          medium: "high"
          high: "high"
          xhigh: "max"

      # 角色映射: Responses API role → Chat API role
      roles:
        developer: "system"

      # 响应字段路径
      reasoning_path: "choices.0.message.reasoning_content"

      # 额外 finish_reason
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

      values: {}   # OpenAI 原生支持所有 reasoning_effort 值，不需要映射

      roles: {}    # OpenAI 原生支持 developer，不需要映射

      reasoning_path: null   # OpenAI Chat API 不返回 reasoning_content

      finish_reasons:
        function_call: "completed"

models:
  - model: gpt-5.5
    provider:
      profile: deepseek          # ← 引用 provider profile
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

## 五、Rust 数据结构

```rust
// config.rs

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderChatProfile {
    /// 字段重命名: 内部 JSON key → provider JSON key
    #[serde(default)]
    pub rename: HashMap<String, String>,

    /// 注入额外字段。静态值 + ${var} 模板（${reasoning_effort} 等）
    #[serde(default)]
    pub inject: serde_json::Value,

    /// 删除 provider 不支持的字段
    #[serde(default)]
    pub drop: Vec<String>,

    /// 字段值映射: field_name → { internal_value → provider_value }（null=移除字段）
    #[serde(default)]
    pub values: HashMap<String, HashMap<String, Option<String>>>,

    /// 角色映射: Responses API role → Chat API role
    #[serde(default)]
    pub roles: HashMap<String, String>,

    /// 响应中 reasoning_content 的 JSON path，null 表示无
    #[serde(default)]
    pub reasoning_path: Option<String>,

    /// 额外 finish_reason → output item status 映射
    #[serde(default)]
    pub finish_reasons: HashMap<String, String>,
}
```

内置配置：`ProviderChatProfile::deepseek()` 和 `ProviderChatProfile::openai()` 提供默认值，`config.yaml` 的 `providers` 段允许覆盖/自定义。

```rust
pub struct ResolvedProvider {
    pub base_url: String,
    pub api_key: String,
    pub downstream_model: String,
    pub profile: ProviderChatProfile,   // ← 新增
}
```

---

## 六、数据流

```
ResponsesRequest
       │
       ▼
  responses_to_chat()       ← 强类型核心转换（不改）
       │
       ▼
  ChatCompletionRequest
       │
       ▼
  serde_json::to_value()    ← 序列化为 JSON
       │
       ▼
  profile.apply_request()   ← post-processing:
       │                      1. values 映射 reasoning_effort
       │                      2. rename 重命名字段
       │                      3. inject 注入 thinking/${var}
       │                      4. drop 删除不支持的字段
       ▼
  POST /chat/completions
```

```
Chat API Response JSON
       │
       ▼
  profile.parse_response()  ← pre-processing:
       │                      1. reasoning_path 提取推理内容
       │                      2. finish_reasons 合并
       │                      3. 反序列化为 ChatCompletionResponse
       ▼
  ChatCompletionResponse
       │
       ▼
  chat_to_responses()       ← 强类型核心转换（不改）
       │
       ▼
  ResponsesResponse
```

---

## 七、调用方式示例

```rust
// 非流式
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

## 八、改动范围

| 文件 | 改动 | 行数 |
|------|------|------|
| `config.rs` | 定义 `ProviderChatProfile` + `default()` + `deepseek()` + `openai()` + `apply_request()` + `parse_response()` | +100 |
| `main.rs` | 两处 handler 序列化后调 `apply_request` / 反序列化前调 `parse_response` | +10 |
| `convert_request.rs` | reasoning_effort 值映射从 profile.value_map 读取，role 映射从 profile.roles 读取 | -10+10 |
| `convert_response.rs` | reasoning_content / finish_reason 从 profile 读取 | -5+5 |
| `config.yaml` | 新增 `providers` 段 | +40 |

---

## 九、实施步骤

| 步骤 | 内容 |
|------|------|
| 1 | 定义 `ProviderChatProfile` struct（含 DeepSeek/OpenAI 默认配置） |
| 2 | `ResolvedProvider` 加 `profile` 字段，`Config::load` 增加 provider profile 解析 |
| 3 | 实现 `ProviderChatProfile::apply_request()` |
| 4 | 实现 `ProviderChatProfile::parse_response()` |
| 5 | `handle_responses` 非流式路径调用 profile |
| 6 | `handle_responses` 流式路径调用 profile |
| 7 | WS handler 路径调用 profile |
| 8 | 移除 `convert_request.rs` 硬编码 DeepSeek 映射 |
| 9 | 移除 `convert_response.rs` 硬编码 logic |
| 10 | `config.yaml` 加 `providers` 段 |
| 11 | `verification_tests.rs` 加 OpenAI profile 测试 |
