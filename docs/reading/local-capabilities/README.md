# 专题：与主会话无关的局部模型调用

本专题聚焦“局部特性/子任务”在 Codex 中的落地做法：在不污染主聊天会话的前提下，调用模型完成一次性文本生成或小型任务（如提示词优化、摘要、标题生成）。

## 文档导航
- 进程内（Rust）：[局部特性：进程内调用（Rust）从 0 到 1](./inprocess-rust.md)
- 跨语言（子进程/协议流）：[局部特性：跨语言集成（子进程/协议流）从 0 到 1](./cross-language.md)

## 建议默认设置（文本类任务）
- 批准策略：`approval_policy=never`
- 沙箱：`sandbox=read-only`
- 禁用非必要工具：
  - `include_plan_tool=false`
  - `include_apply_patch_tool=false`
  - `include_view_image_tool=false`
  - `tools.web_search_request=false`
- 系统提示（完全替换）：在 `base_instructions` 明确“禁止调用任何工具，仅输出最终文本，不要解释”。

更多配置细节参考 docs/config.md 与 docs/sandbox.md。

---

## 为什么选 Codex 搭建上层应用（相对直接 LLM API）

- 安全与可控
  - 内置审批策略与沙箱（Seatbelt/Landlock），可配置只读/工作区写入/全权放开与网络开关；默认“不在受信目录则拒绝”。
  - 将“生成→执行/改码”纳入受控流程，降低把 LLM 变成“远程 RCE”的风险。
- 统一事件协议
  - 标准化 Submission/EQ 事件流（JSON Lines），跨语言一致；`codex proto` + TypeScript 生成器提供强类型。
  - 更容易在进程外集成多个前端/服务，减少自定义协议成本。
- 工具与自动化能力
  - 内置 `shell`（流式输出、审批/提权流程）、`apply_patch`（结构化改码）、`web_search`、`view_image`，并支持 MCP 动态挂载外部工具。
  - 比“直接文本生成”更接近可落地的自动化闭环。
- 配置与可观测
  - 配置分层（config.toml > `-c` > 强类型 Overrides）、模型/提供方抽象（OpenAI/ChatGPT 登录/本地 OSS via ollama）。
  - 流式重试/退避、usage/推理事件、历史与 rollout 记录、`--output-last-message` 等可观测点开箱即用。
- 开发者效率
  - TUI/exec/proto 三位一体；AGENTS.md 与 `base_instructions` 快速定制；一行命令验证 JSONL；CLI 原地复现线上问题。

### 适用场景
- 安全地把“生成→执行/改码”串起来：让模型改代码、跑命令、出差异、再迭代（审批 + 沙箱兜底）。
- 搭建“局部特性/子任务”层：提示词优化、摘要、标题生成等一次性调用，用 `exec`/`proto` 快速封装成内部服务。
- 工具编排与扩展：把你的内部工具通过 MCP 暴露给模型，复用 Codex 的工具注入与审批链路。
- 多模型/多提供方切换：用相同事件协议与工具栈切换模型/本地 OSS，大多只需改配置。

### 潜在权衡
- 如果只是“获取一段文本”且不涉及工具/执行/协议流，直连 LLM API 更轻量。
- `shell` 工具为核心能力，虽可用只读沙箱约束并在 `base_instructions` 明确禁用，但仍需理解其安全模型。
- 抽象层带来少量学习与调试路径开销；建议先用 CLI 复现（`exec --json`/`proto`），再嵌入应用。

---

## 快速上手（JSONL）

最短查看事件类型（依赖 jq）：

```bash
echo 'hi' | codex exec --json - | jq -rc 'select(.msg) | .msg.type'
```

仅提取完整 Agent 文本：

```bash
echo '把这句改写更简洁' | codex exec --json - | jq -rc 'select(.msg.type=="agent_message") | .msg.message'
```

获取最终完整输出（exec 的 JSON 模式不打印 task_complete）：

```bash
tmp=$(mktemp); echo '把这句改写更简洁' | codex exec --json --output-last-message "$tmp" - >/dev/null; cat "$tmp"; rm "$tmp"
```

协议流直接取 last_agent_message：

```bash
printf '%s\n' '{"id":"1","op":{"type":"user_input","items":[{"type":"text","text":"把这句改写更简洁"}]}}' \
  | codex proto \
  | jq -rc 'select(.msg.type=="task_complete") | .msg.last_agent_message'
```

建议同时附加“只输出文本、禁用工具”覆盖项（按需拼接）：

```bash
-c 'base_instructions=禁止调用任何工具，只输出最终文本。' \
-c include_plan_tool=false \
-c include_apply_patch_tool=false \
-c include_view_image_tool=false \
-c tools.web_search_request=false
```

