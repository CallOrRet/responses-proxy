# Codex CLI WebSocket 协议分析

## 基础架构

| 项目 | 值 |
|------|-----|
| **URL** | `https://chatgpt.com/backend-api/codex/responses` |
| **协议** | WSS (WebSocket Secure) |
| **格式** | 基于 OpenAI Responses API + Codex 扩展 |
| **主模型** | `gpt-5.5`（17 工具） |
| **审查模型** | `codex-auto-review`（8 工具） |

## 客户端 → 服务端：`response.create`

```json
{
  "type": "response.create",
  "model": "gpt-5.5",
  "instructions": "<21335字符的系统prompt>",
  "input": [/* 消息、compaction等 */],
  "tools": [/* 17个工具定义 */],
  "tool_choice": "auto",
  "parallel_tool_calls": true,
  "reasoning": {"effort": "medium"},
  "store": false,
  "stream": true,
  "include": ["reasoning.encrypted_content"],
  "prompt_cache_key": "019e16d2-...",
  "client_metadata": {
    "x-codex-installation-id": "...",
    "x-codex-window-id": "...",
    "x-codex-turn-metadata": "{...}"
  }
}
```

### 所有字段

| 字段 | 类型 | 说明 |
|------|------|------|
| `type` | string | `"response.create"` |
| `model` | string | `"gpt-5.5"` 或 `"codex-auto-review"` |
| `instructions` | string | 完整系统 prompt（~21KB） |
| `input` | array | 对话项列表 |
| `tools` | array | 工具定义列表 |
| `tool_choice` | string | `"auto"` |
| `parallel_tool_calls` | bool | `true` |
| `reasoning` | object | `{"effort": "medium"}` |
| `store` | bool | `false` |
| `stream` | bool | `true` |
| `include` | array | `["reasoning.encrypted_content"]` |
| `prompt_cache_key` | string | 服务端 prompt 缓存键 |
| `text` | object | 文本格式配置 |
| `client_metadata` | object | Codex 客户端元数据 |

## Input 消息类型

| 类型 | role | 内容 |
|------|------|------|
| `message` | `developer` | 系统指令分片（permissions、skills、plugins 等） |
| `message` | `user` | `<environment_context>` + 用户输入 |
| `message` | `assistant` | `output_text` / `tool_call` / `tool_result` / `reasoning` |
| `compaction` | - | `encrypted_content`（AES 加密的历史压缩） |

Codex 不通过 `conversation.item.create` 上传 tool 结果，而是把 `tool_call` + `tool_result` 直接嵌在下一轮 `response.create` 的 `input` 数组中。

### User Message 示例

```json
{
  "type": "message",
  "role": "user",
  "content": [
    {
      "type": "input_text",
      "text": "<environment_context>\n  <cwd>/path/to/project</cwd>\n  <shell>bash</shell>\n  <current_date>2026-05-11</current_date>\n</environment_context>"
    }
  ]
}
```

### Assistant Message 示例

```json
{
  "type": "message",
  "role": "assistant",
  "content": [
    {"type": "output_text", "text": "..."},
    {"type": "tool_call", "name": "exec_command", "call_id": "call_xxx", "arguments": "{...}"},
    {"type": "tool_result", "call_id": "call_xxx", "output": "...", "status": "success"}
  ]
}
```

### Compaction 项

```json
{
  "type": "compaction",
  "encrypted_content": "gAAAAABq..."
}
```

## 服务端 → 客户端：Streaming 事件流

每个 response 的生命周期：

```
response.created
 → response.in_progress
 → response.output_item.added    (item 类型: message | function_call | reasoning)
   → response.content_part.added  (part 类型: output_text)
     → response.output_text.delta  (多次，逐 token)
   → response.content_part.done
 → response.output_item.done
 → response.completed              (含 usage、完整 response 对象)
```

穿插 `codex.rate_limits` 事件。

### 事件列表

| 事件 | 说明 |
|------|------|
| `response.created` | 响应已创建 |
| `response.in_progress` | 响应生成中 |
| `response.output_item.added` | 新输出项（message / function_call / reasoning） |
| `response.content_part.added` | 新内容片段 |
| `response.output_text.delta` | 文本增量（含 `delta` 字段） |
| `response.function_call_arguments.delta` | 函数调用参数增量（含 `delta` 字段） |
| `response.function_call_arguments.done` | 函数调用参数完成（含完整 `arguments`） |
| `response.output_text.done` | 文本输出完成 |
| `response.content_part.done` | 内容片段完成 |
| `response.output_item.done` | 输出项完成 |
| `response.completed` | 响应完成（含完整 response 对象 + usage） |
| `codex.rate_limits` | 速率限制信息（Codex 自定义） |

### codex.rate_limits 结构

```json
{
  "type": "codex.rate_limits",
  "plan_type": "plus",
  "rate_limits": {
    "allowed": false,
    "limit_reached": true,
    "primary": {
      "used_percent": 1,
      "window_minutes": 300,
      "reset_after_seconds": 17583,
      "reset_at": 1778516991
    },
    "secondary": {
      "used_percent": 100,
      "window_minutes": 10080,
      "reset_after_seconds": 196146,
      "reset_at": 1778695553
    }
  },
  "code_review_rate_limits": null,
  "additional_rate_limits": null
}
```

### response.completed 结构

```json
{
  "type": "response.completed",
  "response": {
    "id": "resp_...",
    "object": "response",
    "status": "completed",
    "model": "gpt-5.5",
    "output": [/* 完整的输出项列表 */],
    "usage": {
      "input_tokens": ...,
      "output_tokens": ...,
      "total_tokens": ...,
      "input_tokens_details": {
        "cached_tokens": ...,
        "cache_creation_input_tokens": ...
      },
      "output_tokens_details": {
        "reasoning_tokens": ...
      }
    }
  }
}
```

## 工具列表

### 主模型（gpt-5.5）— 17 个工具

| 工具 | 用途 |
|------|------|
| `exec_command` | 在 PTY 中执行命令，支持 sandbox、login shell、TTY |
| `write_stdin` | 向交互式 session 写入字符并获取输出 |
| `apply_patch` | 编辑文件（FREEFORM 工具） |
| `update_plan` | 更新任务计划 |
| `request_user_input` | 向用户提问（1-3 个问题） |
| `view_image` | 查看本地图片 |
| `spawn_agent` | 创建子 agent，支持 model override |
| `send_input` | 向子 agent 发送消息 |
| `resume_agent` | 恢复已关闭的 agent |
| `wait_agent` | 等待 agent 完成 |
| `close_agent` | 关闭 agent 及后代 |
| `list_mcp_resources` | 列出 MCP 资源 |
| `list_mcp_resource_templates` | 列出 MCP 资源模板 |
| `read_mcp_resource` | 读取 MCP 资源 |
| `mcp__codex_apps__github` | GitHub 集成 |
| `web_search` | 网络搜索（text + image） |
| `image_generation` | 图片生成（PNG 格式） |

### 审查模型（codex-auto-review）— 8 个工具

仅包含 `apply_patch`、`update_plan`、`request_user_input` 等基础工具，无 exec_command、agent 和 MCP 工具。

## Codex 特有扩展（vs 标准 Responses API）

1. **`reasoning` 输出项** — 含 `encrypted_content` 字段（AES-256-GCM 加密），承载模型的思考过程
2. **`compaction` 输入项** — 同样使用 `encrypted_content`，用于上下文压缩/摘要
3. **`codex.rate_limits` 事件** — 自定义服务器推送，含 `plan_type`、双级限流（primary/secondary）
4. **`phase` 字段** — message item 上标记 `"final_answer"`，区分思考阶段和最终回复
5. **`client_metadata`** — 含 `x-codex-installation-id`、`x-codex-window-id`、`x-codex-turn-metadata`
6. **`prompt_cache_key`** — 用于服务端 prompt caching
7. **`codex-auto-review` 模型** — 独立的代码审查模型，工具集更少

## 与 responses-proxy 的适配点

- **reasoning 输出** — 需支持 `encrypted_content` 加密/解密。proxy 中已有 `compact_key` 的 crypto 模块，可能是同一套 AES-GCM 方案
- **compaction 输入** — 需正确处理 `type: "compaction"` 的 input 项及其加密内容
- **`codex.rate_limits`** — 需透传此非标准事件类型
- **`codex-auto-review` 模型** — tool set 不同（8 个 vs 17 个），需按 model 区分
- **`phase` 字段** — 需在 message output item 上支持
