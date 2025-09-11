# Codex 是如何实现工具调用的

本文系统说明 Codex（开源 CLI 代理）在“模型 ↔ 外部工具”之间的调用链路与实现细节，回答它是靠提示词、还是靠 function call，或其他方案。

## 核心结论

- 主要机制：以模型原生的“函数调用（function calling）”为核心，通过在请求中声明 `tools`（带 JSON Schema 或原生类型）来驱动工具调用，而不是仅依赖提示词解析。
- 多协议适配：
  - OpenAI Responses API：支持 `function`、`local_shell`、`web_search`、`custom`（freeform）等工具类型；事件以 SSE 流式返回。
  - Chat Completions API：使用经典的 `tool_calls`/`role=tool` 消息形式，自动转换为函数调用格式。
- 工具来源：
  - 内置工具：`shell`/`exec_command`、`apply_patch`、`update_plan`、`view_image`、`web_search`。
  - MCP 外部工具：通过 Model Context Protocol 动态发现的第三方工具，按需桥接为 OpenAI 工具。
- 安全与审批：所有会执行系统命令/改动文件的工具，统一走沙箱与审批策略，必要时向用户请求“提权/无沙箱重试”。

## 请求侧：如何向模型“暴露工具”

- 入口与拼装：
  - 工具描述构建：`codex-rs/core/src/openai_tools.rs` 内的 `get_openai_tools()` 汇总本回合可用工具（含内置 + MCP）。
  - Responses API 载荷：`create_tools_json_for_responses_api()` 直接序列化为 `tools` 数组；附带 `tool_choice: "auto"`、`parallel_tool_calls: false`，并设置 `prompt_cache_key` 以复用对话缓存（`codex-rs/core/src/client.rs`）。
  - Chat Completions 载荷：`create_tools_json_for_chat_completions_api()` 将工具转换成兼容 `tool_calls` 的 `{"type":"function","function":{...}}`（非 function 类型会被过滤），`codex-rs/core/src/chat_completions.rs` 负责消息阵列构建与流解析。
- 工具形态与描述：
  - `shell`：标准函数工具（或 `local_shell` 原生类型，取决于模型族与配置）。在沙箱场景下，工具描述会动态包含可写根、是否需 `with_escalated_permissions`/`justification` 字段等限制说明。
  - `exec_command`/`write_stdin`：Responses API 的“可流式交互 Shell”工具（见 `codex-rs/core/src/exec_command/responses_api.rs`）。
  - `apply_patch`：既支持 `function` 形态，也支持 `custom`/freeform（Lark 语法）形态，用于更鲁棒的补丁文本解析（`codex-rs/core/src/tool_apply_patch.rs`）。
  - `update_plan`：内部状态工具；`view_image`：向会话附加本地图片路径；`web_search`：按配置启用。
  - MCP 工具：通过 `mcp_tool_to_openai_tool()` 将 MCP 的 `Tool`（带 `input_schema`）转换为 OpenAI 的函数工具（必要时补全/规范化 Schema）。

提示词在这里的角色是“工具说明文字”和“少量使用规范”；真正的调用契约靠结构化工具定义与模型原生 function-calling 能力实现，避免纯文本解析的脆弱性。

## 回应侧：如何接收模型的工具调用

- Responses API 流式事件：`codex-rs/core/src/client.rs` 解析 SSE（如 `response.function_call_arguments.delta`、`response.output_item.done`、`response.completed`），归并为统一的 `ResponseItem` 序列。
- Chat Completions：`codex-rs/core/src/chat_completions.rs` 解析 `delta.tool_calls` 流并聚合，模拟单回合（工具/消息）完成后的最终项。
- 回合驱动：`run_turn()`/`try_run_turn()`（`codex-rs/core/src/codex.rs`）遍历 `ResponseItem`：
  - `FunctionCall`/`LocalShellCall`/`CustomToolCall` → 分发到 `handle_function_call()` / `handle_custom_tool_call()`；
  - `Message`/`Reasoning` → 映射为 UI 事件并记录到历史；
  - 未知调用名 → 以结构化失败返回给模型，允许其自恢复重采样。

## 执行侧：如何真的“跑”起工具

### 1) shell/exec_command：执行系统命令

- 参数解析：
  - `shell`/`container.exec` 使用 `ShellToolCallParams`（命令数组、`workdir`、`timeout_ms`、以及可选 `with_escalated_permissions`/`justification`）。
  - `exec_command`/`write_stdin` 使用专属参数（命令串、时间窗口、输出上限、`session_id` 等）。
- 安全与审批：
  - 通过 `assess_command_safety()`/`assess_safety_for_untrusted_command()` 结合 `AskForApproval` 与 `SandboxPolicy` 决定：直接跑沙箱、请求用户审批、或拒绝。
  - 失败（如 `SandboxErr::Denied`/`Timeout`）时，可能向用户发起“无沙箱重试”的审批请求；`ApprovedForSession` 会把该命令加入当前会话的白名单。
- 沙箱运行：
  - macOS 使用 Seatbelt（`spawn_command_under_seatbelt`）；Linux 使用 Landlock+seccomp（`spawn_command_under_linux_sandbox`）；均在 `codex-rs/core/src/exec.rs`。
  - 输出采集：`consume_truncated_output()/read_capped()` 并在执行中以 `ExecCommandOutputDeltaEvent` 流式推送 stdout/stderr 片段（限流上限），收尾生成完整聚合输出。
- 交互式会话（Responses API 专属）：
  - `exec_command` 会在 PTY 中启动 shell（`portable_pty`），由 `ExecSessionManager`（`codex-rs/core/src/exec_command/session_manager.rs`）维持会话、分配 `session_id`，`write_stdin` 支持后续输入（含 Ctrl‑C 等控制字符），每次都在 `yield_time_ms` 时间窗内拉取合并输出并返回。
- 返回模型：将格式化后的结果包装为 `FunctionCallOutputPayload { content, success }`，并把这条“工具输出”作为下一回合的输入项之一，供模型继续推理。

### 2) apply_patch：安全地修改文件

- 双路径支持：
  - freeform/custom：模型按 Lark 语法生成补丁文本 → `handle_custom_tool_call()` → 走内部 `apply_patch` 流程；
  - function：模型以 JSON 入参调用 `apply_patch` → 构造 `ExecParams` 运行（或转换为内部 `codex --codex-run-as-apply-patch` 以确保一致的补丁语义）。
- 审批与差异：在 `run_exec_with_events()` 里，开始/结束分别发送 `PatchApplyBegin`/`PatchApplyEnd` 事件，并在成功后追加整回合的统一 diff（`TurnDiffEvent`）。

### 3) update_plan / view_image：无副作用或轻量副作用

- `update_plan`：仅更新内部计划状态，不触发外部进程。
- `view_image`：把本地图片路径注入到当回合输入上下文（便于模型在多模态场景引用），非执行型。

### 4) MCP 外部工具：通过 JSON‑RPC 桥接

- 发现与注册：`McpConnectionManager` 启动并列出所有服务器工具，转换为 OpenAI 工具（带规范化后的 JSON Schema）。
- 调用执行：模型请求形如 `server/tool` 的函数名时，`handle_mcp_tool_call()` 通过 JSON‑RPC `callTool` 发起调用并测量耗时，生成 `McpToolCallBegin/End` 事件；结果包装为 `FunctionCallOutput` 或 `McpToolCallOutput` 进入下一回合。

## 工具与提示词的分工

- 工具“协议”层：依赖结构化工具定义（函数签名/Schema/原生类型），非纯提示词。
- 提示词“语义”层：
  - 每个工具都带简明 `description`，提示如何与何时使用（在工具 JSON 中）。
  - 某些模型/策略相关的使用须知会动态注入到描述里（如沙箱写权限、是否需要 `with_escalated_permissions`）。
  - `apply_patch` 的 freeform 模式用 Lark 语法作为强约束，进一步降低歧义。

## 数据循环与对话历史

- 每回合输出的工具结果会作为 `ResponseInputItem::*` 追加回下一次请求的 `input`（Responses）或 `role=tool` 消息（Chat Completions，带 `tool_call_id` 对齐），确保模型能“看到”工具产出继续推理。
- `ConversationHistory` 只记录新产生的项，便于回放与快照（rollout）。

## 关键实现文件（便于深入）

- 工具声明与转换：`codex-rs/core/src/openai_tools.rs`
- Responses API 客户端与流解析：`codex-rs/core/src/client.rs`
- Chat Completions 适配与聚合：`codex-rs/core/src/chat_completions.rs`
- 回合/任务主循环与工具分发：`codex-rs/core/src/codex.rs`
- 系统命令执行与沙箱：`codex-rs/core/src/exec.rs`、`codex-rs/core/src/landlock.rs`、`codex-rs/core/src/seatbelt.rs`、`codex-rs/core/src/spawn.rs`
- 交互式 Shell 会话（Responses）：`codex-rs/core/src/exec_command/*`
- MCP 客户端与桥接：`codex-rs/mcp-client/*`、`codex-rs/core/src/mcp_tool_call.rs`

## 小结

Codex 的工具调用以“函数调用 + 结构化工具定义”为主轴，辅以最小必要的提示词说明，并通过沙箱与审批统一治理执行风险。对上层（模型侧）既兼容 Chat Completions，也拥抱 Responses 的原生工具类型；对下层（系统与外部生态）既可本地执行命令/修改文件，也能以 MCP 方式调用第三方工具，实现开放而可控的代理能力。

