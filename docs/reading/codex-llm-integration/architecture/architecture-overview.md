# Codex LLM 调用链路架构概览

本文面向需要理解或扩展 Codex CLI（Rust 版，`codex-rs` 工作区）的同学，梳理从用户输入到 LLM 流式响应的完整链路、关键模块与数据结构，便于在不耦合实现细节的前提下快速导航代码与扩展点。

## 目标读者

- 需要集成/替换上游 LLM 提供方（含自建代理）的工程师
- 需要理解工具调用（function/local_shell/MCP）在不同 Wire API 下的映射规则的工程师
- 需要调试流式响应（SSE）与事件聚合的工程师

## 系统总体流程

### 1. 配置解析与上下文初始化
- **位置**: `core/src/config.rs` 等
- **功能**: 读取 `~/.codex/config.toml` 与命令行 `-c` 覆盖，合成 `Config`
- **输出**: 选择 `model`、`model_provider`（Provider 定义可内置/可用户自定义）

### 2. Prompt 组装
- **位置**: `core/src/client_common.rs`
- **功能**: 将系统指令（`prompt.md`）+ 可选覆盖、对话历史（`ResponseItem[]`）、工具列表（OpenAI 工具 JSON）合并
- **输出**: 完整的上下文结构

### 3. Wire API 选择
- **位置**: `core/src/model_provider_info.rs`
- **选项**:
  - `wire_api = "responses"` → OpenAI Responses API（`/v1/responses`）
  - `wire_api = "chat"` → Chat Completions API（`/v1/chat/completions`）

### 4. HTTP + SSE 请求处理
- **位置**: `core/src/client.rs`、`core/src/chat_completions.rs`
- **功能**: 
  - 构造请求体（两类 API 的字段不同）
  - 统一增加鉴权、附加头、查询参数、重试策略、空闲超时
  - 以 SSE 流式读取增量响应

### 5. 事件映射与聚合
- **位置**: `core/src/client_common.rs`
- **功能**: 将底层 SSE 事件标准化为统一事件：
  - `Created`/`OutputTextDelta`/`ReasoningDelta`/`OutputItemDone`/`Completed`/`WebSearchCallBegin`
- **特性**: Chat 流可选择"聚合模式"：仅输出最终合成的 assistant 消息与 Completed

### 6. 工具调用执行
- **流程**: 模型下发 function（含 `arguments`）→ CLI 执行 → 以 tool 消息回传到下一轮
- **支持**: 本地 shell、MCP 工具、自定义 freeform 工具、`view_image` 等

## 关键代码模块一览

| 模块 | 文件路径 | 职责 |
|------|---------|------|
| 事件类型定义 | `codex-rs/core/src/client_common.rs` | Prompt 与事件类型 |
| 客户端入口 | `codex-rs/core/src/client.rs` | 总入口与 Responses 流处理 |
| Chat 处理 | `codex-rs/core/src/chat_completions.rs` | Chat Completions 请求构造与流处理 |
| Provider 配置 | `codex-rs/core/src/model_provider_info.rs` | Provider 定义与 URL/头/重试 |
| 工具系统 | `codex-rs/core/src/openai_tools.rs` | 工具定义/转换、MCP 工具 Schema 归一化 |
| 模型特性 | `codex-rs/core/src/model_family.rs` | 模型家族特性（reasoning/text.verbosity/local_shell 等） |

## 核心数据结构与行为

### API 差异
- **Responses vs Chat**: 请求体不同，且 Responses 事件类型更细；二者在进入上层之前被统一成 `ResponseEvent`
- **工具支持**: Chat 仅支持 function 工具，Responses 额外支持 local_shell/web_search/custom/view_image 等

### 错误处理机制
- 统一支持 `Retry-After`、请求级重试、流空闲超时（Provider 级可定制）
- 工具 JSON 使用受限 JSON Schema 子集

### 扩展能力
- **替换/新增 Provider**: 仅通过 `config.toml` 即可（无需改代码）
- **接入自建 LLM Proxy**: 优先实现 Chat 兼容（生态最广），再补 Responses 事件语义（更细粒度的推理/工具体验）

## 架构特点与设计理念

### 可替换性
- Provider 层抽象：通过配置文件即可接入不同的 LLM 服务
- Wire API 抽象：支持 Chat 和 Responses 两种协议，自动适配
- 工具系统抽象：统一的工具接口，支持多种工具类型

### 可扩展性
- 模块化设计：各功能模块职责清晰，便于独立扩展
- 事件统一化：底层多样化的 SSE 事件被统一成标准 ResponseEvent
- 配置驱动：大部分行为可通过配置调整，无需修改代码

### 可观测性
- 结构化事件流：便于调试和监控
- 错误处理统一：标准化的错误处理和重试机制
- 配置透明：配置解析和应用过程可追踪

## 扩展建议

- **替换/新增 Provider**: 仅通过 `config.toml` 即可（无需改代码）
- **接入自建 LLM Proxy**: 优先实现 Chat 兼容（生态最广），再补 Responses 事件语义（更细粒度的推理/工具体验）

## 相关文档

- [API 规范](../api-specs/) - 详细的 API 规范和协议定义
- [配置指南](../configuration/) - Provider 配置的全部字段、示例与注意事项  
- [工具集成](../tools/) - 工具 JSON、执行流、MCP Schema 归一化策略
- [实现方案](../implementation/) - 具体的技术实现方案
- [测试验证](../testing/) - 测试和验证相关文档