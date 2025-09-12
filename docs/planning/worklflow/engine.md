# 执行引擎实现计划（草案）

本节面向实现者，描述 `codex-rs` 内的落地方案与模块划分，并补充“低代码（D 代码思路）”下的能力分层。

## Crate 与模块

- 新增 `codex-workflow`（lib）：
  - `parser`：YAML 读入 + `serde_yaml` 反序列化 + 语义校验（id 唯一、DAG、输入等）。
  - `template`：模板渲染（推荐 `minijinja`），渲染上下文：`inputs/env/vars/steps.*`。
  - `model`：数据结构（Workflow/Step/RetryPolicy/Capture/...）。
  - `scheduler`：基于 `tokio` 的 DAG 调度器（`needs` 拓扑 + 全局并发信号量）。
  - `steps`：步骤执行器集合：`shell / codex / manual / apply_patch / mcp`，统一 `trait Executor`。
  - `logging`：结构化事件流与落盘（`runs/<...>` 目录组织）。

### 步骤类型扩展（低代码思路）
- `render_template`：
  - 用统一模板引擎（推荐 Jinja 家族）渲染多文件代码模板；默认先渲染到临时目录并生成多文件 diff 供用户确认，再写入目标目录。
  - 适用：脚手架生成、批量插入/改写、骨架对齐。
- `script`（JS/TS/SH）：
  - 在 `.codex/scripts` 下组织轻量工具脚本；TS 脚本建议通过 `ts-node` 或 `esbuild-register` 直接运行，避免打包心智负担。
  - 适用：AST/codemod、API 守护、RBAC 校验、i18n/路由扫描等。
  - 安全：仍走现有 spawn 路径与沙箱/审批策略。

- CLI 集成：在 `codex-rs/cli` 增加 `codex workflow` 子命令，调用 `codex-workflow`。
- TUI 集成：在 `tui` 新增 `SlashCommand::Workflow`，解析参数后调 `codex-workflow`。

## 关键执行路径

1) 加载与校验
   - 扫描 `.codex/workflows/*.y{a,}ml`。
   - `serde_yaml` -> `Workflow`，执行语义校验。

2) 渲染与计划
   - 合并 CLI/TUI 参数 -> `inputs`；构造渲染上下文。
   - 对 `env/cwd/run/prompt/...` 做一次性渲染（渲染不可产生副作用）。
   - 生成 DAG 计划与最终命令预览（`--dry-run`/TUI 预览）。

3) 调度与执行
   - `tokio::Semaphore` 控制全局并发；`needs` 就绪后进入执行。
   - 超时：`tokio::time::timeout`。
   - 重试：指数/固定退避（解析 `backoff`）。
   - 取消：监听 `ctrl_c()`，在子任务上调用中断（shell 进程 kill、codex 会话 Interrupt）。

4) 步骤执行器
  - `shell`：复用 `codex_core::spawn::spawn_child_async` 与沙箱策略；采集 stdout/stderr/exit code；可按正则/JSON 指针导出结果。
  - `codex`：`ConversationManager::new_conversation(config)` -> 发送 `Op::UserInput` -> 监听事件直至 `TaskComplete`；按 `capture` 导出（text/json/patch）。
  - `apply_patch`：用 `codex-apply-patch` 应用补丁（直接字符串或从文件读）。
  - `manual`：TUI 弹框确认；非交互（exec/CI）下根据 `skip_on_ci` 行为决定（失败或标记 skipped）。
  - `mcp`：用 `codex-mcp-client` 调用工具，导出结果。
  - `render_template`：渲染 -> `git diff --no-index` 预览 -> 用户确认 -> 写盘；支持 dry‑run 与目标路径冲突检测（详见 [render_template](./render_template.md)）。
  - `script`：根据 shebang/扩展自动选择运行器（sh/node/ts-node）；stdout/stderr/退出码纳入统一事件。

5) 事件与落盘
   - 统一 `StepEvent`：`Started/Stdout/Stderr/Retry/Failed/Skipped/Succeeded`。
   - 实时输出到 TUI/CLI；落盘 `steps/<id>.log`、`steps/<id>.jsonl`。
   - 结束写入 `graph.json` 与 `workflow.yaml`（已渲染）。

## 配置合并与安全边界

- 继承现有 `config.toml`/CLI 覆盖（模型、审批、沙箱、cwd、profile）。
- 禁止在实现中改动或旁路与 `CODEX_SANDBOX_*` 相关逻辑。

## 共享与分发（轻量）
- 组织级共享通过两种方式实现：
  - `shared/` 目录：以 git submodule/subtree 引入公共模板与工作流；合并 `registry.yml` 展示来源信息。
  - `pack/unpack`：将 `.codex/{workflows,templates,registry.yml}` 打成 tar，并生成 `manifest.json`（条目清单、sha256、来源、版本）。`unpack` 到本仓的 `.codex/shared` 并进行校验与合并。
- 策略：MVP 先做 sha256 完整性与来源提示；强敏感场景结合 `manual` gate。

## 失败策略

- 默认：某步失败 -> 标记失败 -> 其后继步骤不再启动 -> 工作流失败并退出码 1。
- 可扩展：后续可引入 `continue_on_error` 与 `always_run` 等控制。

## 对外接口（简要）

```rust
pub struct RunOptions { /* inputs、profile、concurrency、dry_run、json 等 */ }

pub async fn list(root: &Path) -> Result<Vec<WorkflowSummary>>
pub async fn validate(root: &Path, name: Option<&str>) -> Result<()>
pub async fn explain(root: &Path, name: &str, json: bool) -> Result<Plan>
pub async fn run(root: &Path, name: &str, opts: RunOptions) -> Result<RunReport>
```

## 测试策略

- 解析/校验单元测试：YAML -> 结构体 -> 错误用例覆盖。
- 调度器测试：DAG 顺序、并发上限、重试/超时。
- 执行器测试：
  - `shell`：本地 echo/文件写读；
  - `codex`：基于 `CODEX_SANDBOX_NETWORK_DISABLED=1` 的环境，使用内置 mock provider 或跳过网络依赖测试；
  - `apply_patch`：临时目录打补丁并断言；
  - `manual`：CI 下走 `skip_on_ci` 分支；
  - `mcp`：使用本地假服务器或跳过。

> 说明：不新增或修改任何会破坏既有集成测试对 `CODEX_SANDBOX_*` 的假设。
