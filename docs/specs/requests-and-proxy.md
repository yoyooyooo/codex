# 请求与自建代理规范（Wire API / SSE / 兼容建议）

本文定义 Codex 与上游 LLM（或自建代理）之间的请求/响应契约，覆盖两类 Wire API（Responses、Chat Completions）、SSE 事件语义、鉴权与错误处理，并给出最小自建代理实现建议。

> 说明：文中“自建代理”仅指你实现的 HTTP 兼容层，用于对接任意 LLM 服务端或多厂商聚合；不代表特定项目名或产品。

## 两类 Wire API

Codex 通过 `model_provider_info.wire_api` 选择具体协议：

- Responses API（OpenAI `/v1/responses`）
  - 事件语义丰富（reasoning、output_item、web_search、custom 等）。
  - 请求体字段与 Chat 不同，包含 `instructions`/`input`/`tools` 等。
- Chat Completions API（OpenAI `/v1/chat/completions`）
  - 生态兼容性最好；工具仅支持 function 形式；消息为 `messages[]`。

两者最终都会被标准化成统一事件流 `ResponseEvent`（见《流式事件映射》）。

## 请求体规范

### Responses API（`POST {base_url}/responses`）

必备字段（Codex 发起）：

- `model`: 例如 `gpt-5`、`o3`、`codex-mini-latest`
- `instructions`: 系统指令（内置 `prompt.md` + 可选覆盖）
- `input`: `ResponseItem[]`，包含对话消息、工具调用/输出、推理等
- `tools`: OpenAI 工具 JSON（详见《工具体系与集成》）
- `tool_choice`: 一般为 `auto`
- `parallel_tool_calls`: 一般为 `false`
- `reasoning`: 可选，用于 o3/gpt‑5 等支持推理摘要的家族
- `store`: `false`
- `stream`: `true`（要求流式）
- `include`: 可选（如 `reasoning.encrypted_content`）
- `prompt_cache_key`: 会话 ID，用于缓存/去重
- `text`: 仅 GPT‑5 家族支持 `text.verbosity`（low/medium/high）

常见头：

- `Authorization: Bearer <token>`
- `OpenAI-Beta: responses=experimental`
- `conversation_id`/`session_id`: 供兼容层与上游使用
- 自定义头：可由 Provider 配置的 `http_headers`、`env_http_headers` 注入

### Chat Completions（`POST {base_url}/chat/completions`）

必备字段（Codex 发起）：

- `model`: 使用 `ModelFamily.slug`
- `messages`: 标准 OpenAI chat 结构（`system`/`user`/`assistant`/`tool`）
- `stream`: `true`
- `tools`: 仅 function 工具（Codex 会将 Responses 工具 JSON 自动转换为 Chat 形态）

提示：Codex 会根据历史与本轮上下文合成 `messages`，并在需要时附加 `tool_calls`/`tool` 消息。

## 流式响应（SSE）

### Responses API 事件（示例）

服务端每条以 `data: <json>\n\n` 推送：

```text
data: {"type":"response.output_text.delta","delta":"..."}
data: {"type":"response.output_item.done","item":{...}}
data: {"type":"response.reasoning_text.delta","delta":"..."}
data: {"type":"response.completed","id":"...","usage":{...}}
```

最小支持集建议：

- `response.output_text.delta`（文本增量）
- `response.output_item.done`（合成 message/function_call 等）
- `response.completed`（必须，结束信号；可带 `usage`）
- 可选：`response.failed`（包含 `error`/`retry-after` 信息）、`response.created`、`response.reasoning_*`

### Chat Completions 事件（示例）

```text
data: {"id":"...","choices":[{"delta":{"content":"..."}}]}
data: {"id":"...","choices":[{"delta":{"tool_calls":[{"id":"...","function":{"name":"fn","arguments":"{...partial...}"}}]}}]}
data: {"id":"...","choices":[{"finish_reason":"tool_calls"}]}
data: {"id":"...","choices":[{"finish_reason":"stop"}]}
data: [DONE]
```

语义：

- 当 `finish_reason=tool_calls`：表示完整函数调用就绪（Codex 会合并前序分片，生成一次 `FunctionCall` 事件）。
- 当 `finish_reason=stop`：普通对话完成（Codex 会合成最终 assistant 消息并发送 Completed）。
- `[DONE]` 视为流结束标记。

## 鉴权与 HTTP 头

- `Authorization: Bearer <token>`：来自 `env_key` 环境变量或 ChatGPT 登录态（Codex 自动处理）。
- 可选组织/项目头：如 `OpenAI-Organization`、`OpenAI-Project`（可通过 `env_http_headers` 注入）。
- Responses API 额外：`OpenAI-Beta: responses=experimental`、`conversation_id`/`session_id`。

## 错误与重试

- HTTP 4xx/5xx：建议返回结构化 JSON 错误体（至少包含 `message`/`type`）；如 429/401/5xx Codex 会按 Provider 配置进行重试。
- `Retry-After`：建议在 429/5xx 场景下附带；Codex 会遵循该头或使用指数退避。
- Responses API 的 `response.failed`：体内 `error` 字段可携带 `message`、可选 `retry-after` 语义（Codex 会解析并透传）。

## 自建代理最小实现

### A. Chat 兼容代理（优先级高，生态广）

- 路由：`POST /chat/completions`，支持 `stream=true`。
- 请求：标准 OpenAI Chat messages/工具格式；仅需 function 工具。
- 响应：SSE 按 OpenAI 规范推送增量 `delta` 与最终 `finish_reason`，末尾 `[DONE]`。
- 适配第三方模型：在代理中将其会话/工具/流事件转换为 OpenAI Chat 形态即可。

### B. Responses 兼容代理（语义丰富）

- 路由：`POST /responses`，支持 `stream=true`。
- 请求：见上文 Responses 字段；可忽略未支持字段（如 `include`）。
- 事件：最小支持 `response.output_text.delta`、`response.output_item.done`、`response.completed`；可选 `response.failed/created/reasoning_*`。
- 完成语义：务必发出 `response.completed`（可带 `usage`），以便 Codex 发出统一的 `Completed` 事件。

### 兼容性建议

- 工具：Chat 仅 function 工具；Responses 可支持 local_shell/web_search/custom 等。
- 消息体大小：建议限制单事件体大小，流内分片输出，避免超时与代理缓冲放大。
- 错误：保持错误体小而明确，利于 CLI 端展示与排障。

## 与 Codex 集成步骤（使用你的代理）

1. 在 `~/.codex/config.toml` 增加 Provider：
   ```toml
   model = "your-model"
   model_provider = "your-proxy"

   [model_providers.your-proxy]
   name = "Your Proxy"
   base_url = "https://api.your-proxy.com/v1"
   env_key = "YOUR_PROXY_API_KEY"
   wire_api = "chat"  # 或 "responses"
   # 可选：query_params/http_headers/env_http_headers/request_max_retries/stream_* 等
   ```
2. 设置 API Key：`export YOUR_PROXY_API_KEY=...`
3. 运行 Codex：根据需要选择 CLI/TUI；如需切换多个模型，可使用 `profiles`。

## 安全与限流要点

- 严格校验长度与速率：代理层应限制输入输出体积与速率，避免单用户/单会话耗尽资源。
- 明确工具白名单：仅暴露必要工具（尤其是 shell 类功能），并在上游执行层再做防护与审计。
- 跨域与重定向：SSE 端点不建议做多级重定向，避免客户端超时；如必须，确保 `cache-control` 与 `connection` 配置稳健。

