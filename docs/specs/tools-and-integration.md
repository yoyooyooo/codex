# 工具体系与集成（function / local_shell / MCP / freeform）

本文定义 Codex 的工具建模、与 Wire API 的映射关系、MCP 工具 Schema 归一化策略，以及工具调用的完整执行生命周期。

## 工具类型（Responses 原生）

- function：标准函数调用，带 JSON Schema 参数
- local_shell：本地 Shell 调用（由模型原生理解或通过工具定义说明）
- web_search：Web 搜索（可选）
- custom/freeform：自由格式工具（`type: "custom"`）
- view_image：将本地图片路径加入上下文（仅路径字符串参数）

> Chat Completions 仅支持 function 工具。Codex 会自动把 Responses 工具集转换为 Chat 格式（非 function 会被过滤）。

## JSON Schema 子集

Codex 使用受限的 JSON Schema 子集（`core/src/openai_tools.rs` 中的 `JsonSchema`）：

- 标量：`string` / `number`（含 `integer` 归一化）/ `boolean`
- `object`：`properties`（必有；可为空）、`required`、`additionalProperties`
- `array`：`items`（若缺失，默认 `{type:string}`）

归一化策略（`sanitize_json_schema`）：

- 若缺失 `type`，依据 `properties/items/enum/const/format/...` 推断，否则默认 `string`
- `integer` 归一化为 `number`
- 允许 `additionalProperties` 为 `boolean` 或内嵌 schema（会继续归一化）

## MCP 工具映射

- MCP 工具以“全限定名”注册（例如 `server/name`），并按名称排序以稳定 Prompt（提升缓存命中）。
- `mcp_tool_to_openai_tool` 将 MCP 工具的 `ToolInputSchema` 转为 `ResponsesApiTool`，并套用上文归一化策略。
- 最终进入工具集由 `get_openai_tools` 汇总，结合 Codex 的内置工具（shell/plan/apply_patch/view_image 等）。

## Chat 与 Responses 的工具差异

- Chat：仅 function 工具，格式：
  ```json
  { "type": "function", "function": { "name": "...", "description": "...", "parameters": { ... } } }
  ```
- Responses：支持 function/local_shell/web_search/custom/freeform/view_image 等；直接序列化为工具数组。

Codex 会将 Responses 工具 JSON 先生成，然后在 Chat 分支中过滤+改写为 Chat 兼容格式。

## 工具执行生命周期

1. 模型下发工具调用（流式）：
   - Chat：`choices[].delta.tool_calls[].function.{name,arguments}` 分片增量；`finish_reason=tool_calls` 表示完整函数调用就绪。
   - Responses：最终以 `response.output_item.done` 携带完整工具调用项。
2. Codex 执行：
   - 解析 `arguments`，执行对应本地动作（例如 shell/MCP/自定义工具）。
   - 将输出作为 `role=tool` 的消息（含 `tool_call_id`）写回下一轮上下文。
3. 模型继续：基于工具输出继续生成文本或下一次工具调用，直至回到普通回答并完成回合。

## 可配置项（影响工具集）

来自 `Config` 与模型家族（`ModelFamily`）的影响：

- `include_plan_tool`：是否加入计划更新工具
- `include_apply_patch_tool`：是否加入 `apply_patch`（freeform/function 形态由模型家族或开关决定）
- `tools_web_search_request`：是否加入 `web_search` 请求工具
- `include_view_image_tool`：是否加入 `view_image`
- `use_experimental_streamable_shell_tool`：是否使用可交互的流式 shell 工具
- `approval_policy`/`sandbox_policy`：决定 shell 工具描述与是否允许“请求升级权限”参数
- `ModelFamily`：决定是否使用 `local_shell`、是否需要额外 `apply_patch` 说明、是否支持 reasoning 参数等

## 参考代码

- 工具定义与序列化：`codex-rs/core/src/openai_tools.rs`
- 工具集组装：`get_openai_tools` / `ToolsConfig`
- Chat 工具转换：`create_tools_json_for_chat_completions_api`
- Responses 工具序列化：`create_tools_json_for_responses_api`

