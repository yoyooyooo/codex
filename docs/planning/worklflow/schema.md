# YAML Schema（草案）

本文约定工作流 YAML 的键结构、字段语义与模板插值规则。

## 顶层字段
- `name`（string，可选）：工作流名；缺省取文件名。
- `description`（string，可选）：用途简介。
- `inputs`（map，可选）：参数定义。
  - 形如：`<key>: { type: string|number|bool|enum, default?: any, required?: bool, enum?: [..] }`
- `env`（map<string,string>，可选）：为所有步骤注入的环境变量（支持模板）。
- `vars`（map，可选）：中间变量（渲染只读，不会在运行时改变）。
- `concurrency`（number，可选，默认 1）：全局并发上限。
- `steps`（数组，必填）：步骤列表，按 `id` 唯一标识。

## 步骤通用字段
- `id`（string，必填）：唯一标识符。
- `name`（string，可选）：展示名。
- `uses`（enum，必填）：`shell | codex | manual | apply_patch | mcp`
- `needs`（array<string>，可选）：前置依赖，形成 DAG。
- `if`（string，模板表达式，可选）：条件为 truthy 时执行，否则跳过。
- `env`（map<string,string>，可选）：步骤级环境变量（支持模板）。
- `cwd`（string，可选）：步骤执行目录（支持模板）。
- `timeout`（string，可选）：超时，如 `30s`、`5m`。
- `retry`（object，可选）：`{ max_attempts: number, backoff?: string }`。
- `artifacts`（array<string>，可选）：希望归档的文件/目录路径（支持模板）。

## steps.uses=shell
- 运行本地命令（走 Codex 既有沙箱/审批与 spawn 路径）。
- 字段：
  - `run`（string | array<string>，必填）：命令；字符串数组表示 argv。
  - `approval_hint`（string，可选）：用于审批提示的人类可读原因。
  - `capture`（object，可选）：输出提取策略：
    - `stdout_regex`：从 stdout 中以正则命名分组提取，如：`(?m)^RESULT=(?P<value>.+)$`
    - `stdout_json_pointer`：若 stdout 为 JSON，可用 JSON Pointer 读取。
    - `to_file`：写入路径；或从特定文件读取导出（配合 `outputs`）。
  - `outputs`（map<string,string>，可选）：将 `capture` 的命名结果映射为 `steps.<id>.outputs.*`。

示例：
```yaml
- id: unit
  uses: shell
  run: cargo test -p codex-core
  retry: { max_attempts: 2, backoff: 2s }
```

## steps.uses=codex
- 通过 Codex Agent 执行一段 prompt，监听事件直到 TaskComplete。
- 字段：
  - `prompt`（string，必填）：发送给 Agent 的文本（支持模板）。
  - `images`（array<string>，可选）：附带图片路径。
  - `profile`（string，可选）：使用的配置 profile（覆盖默认）。
  - `capture`（object，可选）：定义如何导出结果：
    - `text`（bool）：导出最后一条 agent 消息的纯文本。
    - `json_pointer`（string）：当消息体为 JSON 时，取对应字段。
    - `patch`（enum）：`apply_patch`｜`none`；若为 `apply_patch` 则将本轮补丁作为输出（若存在）。
  - `outputs`（map<string,string>，可选）：命名导出。

示例：
```yaml
- id: fix
  uses: codex
  if: "{{ steps.unit.status == 'failure' }}"
  prompt: |
    单测失败了。请生成补丁修复它，并保证测试通过。
  capture:
    patch: apply_patch
```

## steps.uses=apply_patch
- 应用补丁，复用 `codex-apply-patch` 库。
- 字段：
  - `patch`（string，必填）：统一 diff 文本或文件路径（可模板）。

示例：
```yaml
- id: apply
  uses: apply_patch
  if: "{{ steps.fix.outputs.patch is defined }}"
  patch: "{{ steps.fix.outputs.patch }}"
```

## steps.uses=manual
- 人工确认或外部旁路操作。
- 字段：
  - `message`（string，必填）：展示给用户的说明。
  - `skip_on_ci`（bool，可选，默认 false）：在非交互环境下跳过（并标注 `skipped`）。

## steps.uses=mcp
- 调用 MCP 工具。
- 字段：
  - `server`（string，必填）：服务器标识（对应配置）。
  - `tool`（string，必填）：工具名。
  - `args`（object，可选）：参数对象。
  - `outputs`（map<string,string>，可选）：命名导出。

## steps.uses=render_template
- 渲染代码模板（多文件），提供 diff 预览与确认后写盘（支持 dry‑run）。
- **适用场景**：脚手架生成、批量插入/改写、骨架对齐。
- 字段：
  - `template`（string，必填）：模板名称或路径（位于 `.codex/templates/code/<name>`）。
  - `target_dir`（string，必填）：渲染输出的目标根目录。
  - `params`（map，可选）：传入模板的变量（可与 `inputs` 组合）。
  - `mode`（enum，可选）：`preview`（默认，生成 diff 供确认）| `apply`（直接写盘，谨慎）。
  - `on_conflict`（enum，可选）：`fail`（默认）| `overwrite` | `skip`。

示例：
```yaml
- id: page
  uses: render_template
  template: page-basic
  target_dir: apps/admin
  params: { name: Orders, route: /orders, permissions: [order.read] }
```

## steps.uses=script（JS/TS/SH）
- 运行 `.codex/scripts` 下的脚本，适合 AST/codemod 与领域校验逻辑。
- **适用场景**：API 守护、RBAC 校验、i18n/路由扫描、自定义业务逻辑。
- **安全**：仍走现有 spawn 路径与沙箱/审批策略。
- 字段：
  - `entry`（string，必填）：脚本路径（相对 `.codex/scripts` 或工作目录）。
  - `args`（array，可选）：传给脚本的参数（字符串数组）。
  - `runner`（enum，可选）：`auto`（默认）| `node` | `ts-node` | `sh`。
  - `capture`（同 shell，可选）：从 stdout/文件提取结构化输出。

示例：
```yaml
- id: inject-route
  uses: script
  entry: ts/inject_route.ts
  args: ["apps/admin/src/router.tsx", "/orders", "OrdersPage"]
```

## 智能节点约束（LLM/MCP 最佳实践）
- LLM 输出必须为 patch 或 schema 校验后的 JSON；文本仅作说明。
- 默认低温度（如可）与固定 seed（如可）；失败不盲重试，转入确定性校验（`typecheck/test`）。
- MCP 仅在确有价值时介入；其结果需落盘并被后续步骤消费。

## 模板与表达式
- 语法：推荐 Jinja 风格（如 `{{ inputs.branch }}`，`{{ steps.build.status }}`）。
- 上下文：`inputs.*`、`env.*`、`vars.*`、`steps.<id>.outputs.*`、`steps.<id>.status`。
- 布尔表达式：支持 `== != && || !` 以及 `is defined`。
- 安全：模板仅能访问暴露上下文，不可执行任意代码。

## 状态与输出约定
- `steps.<id>.status`：`success | failure | skipped`。
- `steps.<id>.started_at / ended_at / duration_ms`：时间元数据。
- `steps.<id>.outputs`：本步定义的命名导出（key/value）。

## 校验规则
- `id` 全局唯一。
- `needs` 形成有向无环图（DAG）。
- `inputs` 的 required/default 关系合理；enum 值合法。
- `timeout/backoff` 字符串应满足如 `\d+(ms|s|m|h)` 正则。
