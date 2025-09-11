# 实现技能点与落地清单（按难度分解）

本节基于“确定性编排 + 受控的非确定性节点”的总体方案，对实现所需的关键技能点进行拆解，给出能力要求、实现要点、常见坑、验收要点与建议测试。用于指导成员分工与推进实施。

---

## 事件捕获与补丁提取（codex 步骤）
- 核心能力：
  - 从 `codex-core` 的会话/事件流中稳定获取“最后一条 agent 输出”“补丁输出（apply_patch 工具轨迹）”“结构化 JSON”。
  - 在 Responses/Chat 线下差异下兼容不同模型的流式事件边界。
- 实现要点：
  - 复用 `codex_core::ConversationManager`、`Op::UserInput`、`EventMsg::TaskComplete`；监听直至完成。
  - 捕获策略：`capture.text`（最后 agent 文本）、`capture.json_pointer`（当文本为 JSON 时指针取值）、`capture.patch`（来自 apply_patch 工具的最终补丁）。
  - 针对 patch：以“本轮最终 apply_patch 审批通过且成功应用的 unified diff”为准；如无补丁则返回 none。
- 常见坑：
  - 流中含有中间思考消息与工具事件，需正确归因“最后一条 agent 消息”。
  - 模型侧可能输出非严格 JSON，需要宽松解析或在提示中约束格式。
- 验收要点：
  - 人工注入多事件序列用例，确保 capture 结果稳定；无补丁时不误报。
  - 失败或中断时状态应为 `failure`，并记录错误详情。
- 参考入口：`codex-rs/core/src/conversation_manager.rs`、`codex-rs/exec/src/lib.rs` 事件循环。

## DAG 调度器（needs/并发/超时/重试/取消）
- 核心能力：
  - 解析 steps 形成 DAG；拓扑调度，受全局并发 `concurrency` 限制。
  - 每步 `timeout`、`retry {max_attempts, backoff}` 与失败传播（阻断后继）。
- 实现要点：
  - `tokio::Semaphore` 控制并发；`tokio::time::timeout` 实现超时；退避解析 `1s/2m/…`。
  - 取消：监听 Ctrl‑C，向 shell 子进程与 codex 会话传播中断。
- 常见坑：
  - 环依赖未检测造成死锁；并发异常放大日志、资源泄漏。
- 验收要点：
  - 环检测、深链、菱形依赖与高并发回归；重试次数与时间线正确。

## 模板与表达式引擎（Jinja/Handlebars）
- 核心能力：
  - 对 `env/cwd/run/prompt` 等做一次性渲染；上下文只读：`inputs/env/vars/steps.*`。
- 实现要点：
  - 选型 `minijinja` 或 `handlebars`；提供 `is defined`、布尔逻辑、字符串操作等常用函数。
  - 渲染错误要定位到字段与表达式片段。
- 常见坑：
  - 允许执行任意代码或访问系统函数（必须禁用）。
- 验收要点：
  - 复杂表达式与缺失变量报错友好；渲染结果与预期一致。

## 审批与沙箱贯穿
- 核心能力：
  - shell/codex/mcp 执行均沿用现有 `approval_policy` 与 `sandbox_mode`，不改动任何 `CODEX_SANDBOX_*` 行为。
- 实现要点：
  - shell 统一走 `codex_core::spawn::spawn_child_async`；codex 步骤用 `ConversationManager` 默认策略。
- 常见坑：
  - 错误地在步骤内绕过 sandbox 或直接 `std::process::Command`。
- 验收要点：
  - 在不同审批策略下（never/on-request/on-failure）行为一致可预期。

## MCP 集成
- 核心能力：
  - 连接已配置的 MCP server，调用 tool 并采集结果。
- 实现要点：
  - 复用 `codex-mcp-client`；定义 `outputs` 映射规则（原样 JSON 或字段子集）。
- 常见坑：
  - 会话寿命管理、超时与重试；tool 错误导致的长时间挂起。
- 验收要点：
  - 正常/超时/错误三类用例；结果在后续步骤可被模板消费。

## TUI `/workflow` 交互
- 核心能力：
  - 新增 Slash 命令，支持 `list/validate/explain/run` 与参数补齐。
- 实现要点：
  - 在 `tui/src/slash_command.rs` 增加枚举与描述；`chatwidget.rs` 派发执行；参数缺失弹窗补齐。
  - 运行中禁用 `run`，允许 `list/explain/validate`。
- 常见坑：
  - insta 快照易抖动（色彩/换行）；注意遵循 `tui/styles.md`。
- 验收要点：
  - 快照测试覆盖命令弹窗、运行进度与结束总结。

## 日志与落盘（runs 目录）
- 核心能力：
  - `runs/<name>/<ts>/` 写入 `workflow.yaml(graph)`、每步 `*.log/*.jsonl`、`artifacts/`。
- 实现要点：
  - 结构化事件：`Started/Stdout/Stderr/Retry/Failed/Skipped/Succeeded`；大输出滚动截断。
- 常见坑：
  - 路径冲突/并发写入；日志体积失控。
- 验收要点：
  - 并发运行互不覆盖；产物完整可读。

## 输出 capture 规则
- 核心能力：
  - 支持 `stdout_regex`、`stdout_json_pointer`、`to_file` 三种提取；映射到 `steps.<id>.outputs.*`。
- 实现要点：
  - 正则使用命名分组；JSON Pointer 错误提示清晰；文件不存在时 fail。
- 常见坑：
  - 多行匹配/编码问题；错误吞掉。
- 验收要点：
  - 三种提取方式的正反用例覆盖。

## apply_patch 执行器
- 核心能力：
  - 接收字符串/文件统一 diff，复用 `codex-apply-patch` 应用。
- 实现要点：
  - 失败时标记 `failure` 并输出上下文；不做隐式回滚。
- 常见坑：
  - patch 与工作目录不一致；文件编码问题。
- 验收要点：
  - 典型新增/更新/移动/冲突用例覆盖。

## 运行目录与产物管理
- 核心能力：
  - 时间戳+随机后缀避免冲突；`artifacts/` 收集。
- 实现要点：
  - 可配置清理策略（仅保留 N 次或按体积）。
- 常见坑：
  - 深层目录权限与跨平台路径。
- 验收要点：
  - 并发下命名不冲突；清理策略有效。

## CLI 子命令与 dry‑run/explain
- 核心能力：
  - `list/validate/explain/run`，`--param/--dry-run/--json/--concurrency/--profile`。
- 实现要点：
  - explain 输出 DAG/并发控制图；dry‑run 展示渲染后的命令与条件评估结果。
- 常见坑：
  - 参数覆盖顺序与类型解析（TOML 风格值）。
- 验收要点：
  - 错误码契约；JSON 输出 schema 稳定。

## YAML Schema 与校验
- 核心能力：
  - 反序列化、字段合法性、`needs` DAG 环检测、类型/默认值检查。
- 实现要点：
  - `serde_yaml` + 自定义校验；错误包含路径（`steps[3].run`）。
- 常见坑：
  - 宽松解析导致运行期失败；错误定位不清晰。
- 验收要点：
  - 负例集：重复 id、环依赖、缺失必填、非法枚举等。

## 取消/中断传播
- 核心能力：
  - Ctrl‑C 传递到 shell 子进程与 codex 会话（`Op::Interrupt`）。
- 实现要点：
  - `kill_on_drop(true)`；清理子任务；状态标记为 `failure`/`cancelled`（二选一）。
- 常见坑：
  - 僵尸进程；多层并发下泄漏。
- 验收要点：
  - 压测下无残留进程；退出时间受控。

## 配置合并与 profile
- 核心能力：
  - 遵循 CLI > profile > config > 默认的优先级；workflow 运行期统一生效。
- 实现要点：
  - 透传 `model/approval_policy/sandbox_mode/cwd/profile` 到执行。
- 常见坑：
  - TUI 与 CLI 行为不一致。
- 验收要点：
  - 多组合用例覆盖并比对。

## 跨平台兼容
- 核心能力：
  - Windows/Unix 路径与 shell 差异；尽量使用 argv 方式执行命令。
- 实现要点：
  - 仅在需要时拼接 `sh -lc`/`bash -lc`；路径一律 `PathBuf` 处理。
- 常见坑：
  - 引号转义/编码；临时文件权限。
- 验收要点：
  - Windows CI 验证（可阶段性跳过部分用例）。

## 测试策略与工具
- 核心能力：
  - 单元/集成/端到端分层；快照用于 TUI；mock provider 降低外部依赖。
- 实现要点：
  - 针对调度器/执行器/模板/捕获等关键模块建独立测试。
- 常见坑：
  - 依赖网络环境（遵守 `CODEX_SANDBOX_NETWORK_DISABLED=1` 下的门控）。
- 验收要点：
  - 关键路径测试在无网络环境也可通过或优雅跳过。

## TUI 快照稳定性
- 核心能力：
  - 控制颜色/换行/时序使快照稳定。
- 实现要点：
  - 按 `tui/styles.md` 使用 stylize helpers；文本换行统一 `textwrap`；避免随机数。
- 常见坑：
  - 终端宽度变化导致快照差异。
- 验收要点：
  - 多终端宽度回归；`cargo insta` 流程文档化。

## 性能与资源
- 核心能力：
  - 大量日志与并发任务的背压与截断；默认并发合理。
- 实现要点：
  - 对 stdout/stderr 设置最大保留；长行分块写入；事件队列有界。
- 常见坑：
  - OOM 或磁盘暴涨。
- 验收要点：
  - 压测：长日志/高并发/大产物仍可控。

## 文档与引导
- 核心能力：
  - 错误对照表、最佳实践、故障排查、产物说明与 FAQ。
- 实现要点：
  - 将“LLM 步骤默认不重试/建议固定 seed（若可用）”纳入最佳实践。
- 验收要点：
  - 新人可按文档独立完成一个 workflow 开发与运行。

## 示例模板完善
- 核心能力：
  - 涵盖后端/Rust、前端/CI、E2E、性能审计、发布前检查等常见场景。
- 实现要点：
  - 与 examples 中目录联动，保持可直接复制运行；必要时标注前置依赖。
- 验收要点：
  - 例子可在干净仓库里最小改动跑通（dry‑run 与实跑）。

---

## 落地任务总清单（建议顺序）
1) YAML Schema 与校验（含 DAG 环检测）
2) 模板/表达式引擎（最小可用：插值 + `is defined` + 布尔）
3) 调度器（并发/超时/重试/取消）
4) 执行器：shell（spawn 统一）、apply_patch
5) 执行器：codex（事件捕获 + text/json/patch 提取）
6) 执行器：manual、mcp（最小可用）
7) 日志与落盘（runs 目录）
8) CLI：list/validate/explain/run + dry‑run
9) TUI：/workflow 交互与快照
10) 性能/资源与跨平台收尾
11) 文档完善与示例扩充

> 实施过程中严格遵守：不修改任何 `CODEX_SANDBOX_*` 相关逻辑；shell 一律走现有 spawn 与 sandbox 路径。
