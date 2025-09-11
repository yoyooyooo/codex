# TUI 设计（草案）：/workflow 斜杠命令

在聊天输入框中支持 `/workflow` 前缀命令，便于快速发现与执行工作流。

## 命令格式

- `/workflow list`
  - 列出可用工作流（名称、描述、路径）。
- `/workflow validate [<name>]`
  - 校验全部或指定工作流。
- `/workflow explain <name>`
  - 展示执行 DAG、`needs` 关系与默认参数。
- `/workflow run <name> [k=v ...]`
  - 运行指定工作流；缺失的必填 `inputs` 将弹框交互补齐。
  - 对于 `render_template` 步骤，在提交前显示多文件 diff 预览；支持直接从预览页批准/取消。

> 说明：当子功能进一步增多时，可保留 `/workflow` 作为命名空间，后接子命令。

## 交互与提示

- Slash 弹窗：`/work...` 自动补全到 `/workflow`，并显示子命令帮助。
- 参数补齐：对于 `run`，若有 `inputs` 未提供，弹出参数对话框：
  - 支持 string/number/bool/enum 类型输入；显示默认值与是否必填。
  - 校验失败时显示内联错误提示。
- 运行态约束：与现有 Slash 命令一致，若正在执行任务：
  - `list/explain/validate` 可用；`run` 默认禁用（避免并发干扰）。
- 结果呈现：
  - 执行开始：在历史区打印“计划摘要”（步数/并发/条件）。
  - 过程事件：以“进度行”方式逐步刷新（成功/失败/跳过/重试）。
  - 结束总结：展示成功/失败统计与产物文件夹路径。

## 错误与取消

- 校验失败：在历史区打印错误详情，不进入执行态。
- 运行失败：定位到首个失败步骤并展示日志入口；下游自动中止。
- 取消：`Ctrl-C` 或 UI 退出触发取消，向子步骤传播信号。

## 快捷示例

- `/workflow list`
- `/workflow explain build_and_test`
- `/workflow run build_and_test branch=main release=true`

## 备注

- 命令解析遵循现有 SlashCommand 架构，新增 `Workflow` 分支，子命令参数在 TUI 侧解析并传递给后台执行器。
- UI 风格遵循 `tui/styles.md` 规范（色彩、.dim/.bold 等 stylize helper）。
