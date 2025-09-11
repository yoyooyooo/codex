# 流式事件映射（SSE → ResponseEvent）

本文定义底层 SSE 事件如何被标准化为 Codex 的统一事件枚举 `ResponseEvent`，以及 Chat 与 Responses 的差异与聚合策略。

## 统一事件类型（`ResponseEvent`）

- `Created`：会话/响应创建
- `OutputTextDelta(String)`：纯文本增量
- `ReasoningSummaryDelta(String)` / `ReasoningContentDelta(String)`：推理增量（摘要/正文）
- `OutputItemDone(ResponseItem)`：一段完整输出项（assistant message / function_call / reasoning / 等）
- `Completed { response_id, token_usage }`：本回合结束，附可选 token 用量
- `WebSearchCallBegin { call_id }`：检测到 web_search 调用开始（辅助 UI 显示）

定义位置：`codex-rs/core/src/client_common.rs`

## Responses API 映射

SSE `data: {"type": "...", ...}` → `ResponseEvent`：

- `response.output_text.delta` → `OutputTextDelta`
- `response.reasoning_summary_text.delta` → `ReasoningSummaryDelta`
- `response.reasoning_text.delta` → `ReasoningContentDelta`
- `response.output_item.done`（含 `item`）→ 反序列化为 `ResponseItem` 后发出 `OutputItemDone`
- `response.output_item.added`（检测 `type=web_search_call`）→ 合成 `WebSearchCallBegin`
- `response.created` → `Created`
- `response.completed`（含 `id`/`usage`）→ `Completed`
- `response.failed`（含结构化 `error`）→ 组装错误并终止流；若有 retry-after 语义，会被解析以供上游决策

空闲超时/断流：若在超时前未收到 `response.completed`，则以错误结束（见 `stream_idle_timeout_ms`）。

实现位置：`codex-rs/core/src/client.rs`（`process_sse`）。

## Chat Completions 映射

SSE `data: { id, choices: [{ delta, finish_reason }] }` → `ResponseEvent`：

- `delta.content` → `OutputTextDelta`
- `delta.tool_calls[].function.{name,arguments}` → 累积到函数状态；当 `finish_reason=tool_calls` 时，汇总为一次 `OutputItemDone(FunctionCall)`
- `finish_reason=stop` → 汇总为一次 `OutputItemDone(assistant message)` 并随后发出 `Completed`
- `[DONE]` → 结束（若已完成则正常终止，否则按错误处理）

实现位置：`codex-rs/core/src/chat_completions.rs`（`process_chat_sse`）。

## 聚合模式（Chat 专用，可选）

为减少 UI 噪声，Chat 流支持“聚合模式”（`AggregateStreamExt::aggregate`）：

- 抑制 token 级别增量
- 仅在回合结束时输出一次完整的 `OutputItemDone(assistant message)` 与 `Completed`

默认选择：根据 `Config.show_raw_agent_reasoning` 决定是否走“流式模式”。

## Token 用量映射（Responses）

`response.completed.usage` → `TokenUsage`：

- `input_tokens`、`output_tokens`、`total_tokens`
- `input_tokens_details.cached_tokens` → `cached_input_tokens`
- `output_tokens_details.reasoning_tokens` → `reasoning_output_tokens`

实现：`core/src/client.rs` 中 `ResponseCompletedUsage: Into<TokenUsage>`。

## 可靠性策略（摘要）

- 请求级重试：`request_max_retries`
- 流空闲超时：`stream_idle_timeout_ms`
- 断流/无 `completed`：按错误结束；上层可根据 Provider 配置选择是否重试/重连（参见集成测试 `stream_no_completed.rs`）

