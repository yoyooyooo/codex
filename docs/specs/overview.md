# Codex LLM 调用链路概览（Rust 版）

本文面向需要理解或扩展 Codex CLI（Rust 版，`codex-rs` 工作区）的同学，梳理从用户输入到 LLM 流式响应的完整链路、关键模块与数据结构，便于在不耦合实现细节的前提下快速导航代码与扩展点。

## 目标读者
- 需要集成/替换上游 LLM 提供方（含自建代理）的工程师
- 需要理解工具调用（function/local_shell/MCP）在不同 Wire API 下的映射规则的工程师
- 需要调试流式响应（SSE）与事件聚合的工程师

## 总体流程
1. 解析配置与上下文（`core/src/config.rs` 等）
   - 读取 `~/.codex/config.toml` 与命令行 `-c` 覆盖，合成 `Config`。
   - 选择 `model`、`model_provider`（Provider 定义可内置/可用户自定义）。
2. 组装 Prompt（`core/src/client_common.rs`）
   - 将系统指令（`prompt.md`）+ 可选覆盖、对话历史（`ResponseItem[]`）、工具列表（OpenAI 工具 JSON）合并。
3. 选择 Wire API（`core/src/model_provider_info.rs`）
   - `wire_api = "responses"` → OpenAI Responses API（`/v1/responses`）
   - `wire_api = "chat"` → Chat Completions API（`/v1/chat/completions`）
4. 发起 HTTP + SSE（`core/src/client.rs`、`core/src/chat_completions.rs`）
   - 构造请求体（两类 API 的字段不同，详见《请求与自建代理规范》）。
   - 统一增加鉴权、附加头、查询参数、重试策略、空闲超时。
   - 以 SSE 流式读取增量响应。
5. 事件映射与聚合（`ResponseEvent`，`core/src/client_common.rs`）
   - 将底层 SSE 事件标准化为统一事件：Created/OutputTextDelta/ReasoningDelta/OutputItemDone/Completed/WebSearchCallBegin。
   - Chat 流可选择“聚合模式”：仅输出最终合成的 assistant 消息与 Completed。
6. 工具调用执行
   - 模型下发 function（含 `arguments`）→ CLI 执行 → 以 tool 消息回传到下一轮。
   - 支持本地 shell、MCP 工具、自定义 freeform 工具、`view_image` 等（详见《工具体系与集成》）。

## 关键代码模块一览
- Prompt 与事件类型：`codex-rs/core/src/client_common.rs`
- 客户端总入口与 Responses 流处理：`codex-rs/core/src/client.rs`
- Chat Completions 请求构造与流处理：`codex-rs/core/src/chat_completions.rs`
- Provider 定义与 URL/头/重试：`codex-rs/core/src/model_provider_info.rs`
- 工具定义/转换（Responses→Chat）、MCP 工具 Schema 归一化：`codex-rs/core/src/openai_tools.rs`
- 模型家族特性（reasoning/text.verbosity/local_shell 等）：`codex-rs/core/src/model_family.rs`

## 数据与行为要点
- Responses 与 Chat 的请求体不同，且 Responses 事件类型更细；二者在进入上层之前被统一成 `ResponseEvent`。
- 工具 JSON 使用受限 JSON Schema 子集；Chat 仅支持 function 工具，Responses 额外支持 local_shell/web_search/custom/view_image 等。
- 错误处理：统一支持 `Retry-After`、请求级重试、流空闲超时（Provider 级可定制）。

## 扩展建议
- 替换/新增 Provider：仅通过 `config.toml` 即可（无需改代码）。
- 接入自建 LLM Proxy：优先实现 Chat 兼容（生态最广），再补 Responses 事件语义（更细粒度的推理/工具体验）。详见《请求与自建代理规范》。

---

配套文档：
- 《请求与自建代理规范》：请求/响应字段、SSE 事件、错误与鉴权、最小代理实现面
- 《Provider 配置规范》：`config.toml` 中 Provider 的全部字段、示例与注意事项
- 《工具体系与集成》：工具 JSON、执行流、MCP Schema 归一化策略
- 《流式事件映射》：SSE 事件到 `ResponseEvent` 的映射与聚合规则

