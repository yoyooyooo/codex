# Workflow 特性规范（草案）

本文定义 Codex 在用户项目下的工作流（Workflow）能力：用户可在仓库内以 YAML 定义固化流程，按需参数化执行，配合 Codex 的审批与沙箱体系，形成可重复、可审计的自动化。

- 放置位置：`.codex/workflows/*.yaml|yml`
- 执行入口：
  - CLI：`codex workflow <subcommand>`
  - TUI：聊天输入框支持斜杠命令：`/workflow ...`
- 运行产物：`.codex/workflows/runs/<workflow>/<timestamp>/`

本规范面向两类读者：
- 使用者：编写 YAML 并运行工作流。
- 实现者：在 codex-rs 中落地解析/调度/执行器与 TUI/CLI 入口。

## 目标
- 用 YAML 固化一组本地/AI/MCP 等步骤，可串并行、可参数化、可条件化。
- 复用 Codex 既有审批策略与沙箱策略，保证一致的安全边界与行为。
- 运行过程结构化记录，便于回溯、对比与分享。

## 非目标（MVP 以外）
- 不支持循环、矩阵编排、长尾控制流（后续可扩展）。
- 不引入远程执行/分布式执行。

## 目录结构与命名
- 工作流定义：`.codex/workflows/<name>.yaml`
  - `name` 默认为文件名（去扩展名），可在 YAML 顶部显式覆盖。
- 运行产物：`.codex/workflows/runs/<name>/<timestamp>/`
  - `workflow.yaml`：本次运行展开后的完整定义（含实参与渲染结果）。
  - `graph.json`：解析后的执行 DAG。
  - `steps/<id>.log`、`steps/<id>.json`：单步日志与结构化结果。
  - `artifacts/`：归档产物（如构建物、报告等）。

## 快速开始
- 在项目根创建 `.codex/workflows/build_and_test.yaml`（示例见下）
- 运行：
  - CLI：`codex workflow run build_and_test --param branch=main --param release=true`
  - TUI：在输入框键入：`/workflow run build_and_test branch=main release=true`

### 最小示例
```yaml
name: build_and_test
description: 在当前仓库拉取分支并运行 Rust 单测
inputs:
  branch: { type: string, default: main }
steps:
  - id: checkout
    uses: shell
    run: git checkout {{ inputs.branch }}

  - id: test-core
    uses: shell
    needs: [checkout]
    run: cargo test -p codex-core
```

## 架构概览

```
用户项目
├── .codex/
│   ├── workflows/           # 工作流定义
│   │   ├── build.yaml
│   │   └── deploy.yaml
│   ├── templates/           # 代码模板
│   │   └── code/
│   └── scripts/             # 自定义脚本
└── .codex/workflows/runs/   # 执行产物
    └── build/
        └── 2024-03-15-143022/
            ├── workflow.yaml    # 渲染后的定义
            ├── graph.json       # 执行 DAG
            ├── steps/           # 单步日志
            └── artifacts/       # 输出产物
```

## 运行时行为总览
- **解析**：读取 `.codex/workflows` 目录，使用 YAML->结构体的方式解析规范。
- **模板**：字段支持模板插值（如 `{{ inputs.branch }}`），基于 Jinja 语法。
- **DAG**：根据 `needs` 拓扑排序；支持全局 `concurrency` 并发限制。
- **条件**：按 `if` 表达式求值决定执行/跳过。
- **重试/超时**：按每步 `retry/timeout` 策略执行。
- **审批/沙箱**：复用 Codex 配置的 `approval_policy` 与 `sandbox_mode`。
- **失败处理**：默认失败即中止其依赖链（可后续引入 `continue_on_error`）。
- **产物记录**：写入 `runs/<...>/steps/*` 与汇总报告，便于审计。

## CLI 与 TUI 入口（摘要）
- CLI：`codex workflow list | validate | explain | run`
- TUI：`/workflow list | validate <name?> | explain <name> | run <name> [k=v ...]`
  - TUI 会在参数缺失时交互式补齐（见 tui.md）。

## YAML 规范与模板语法
详见 `schema.md`。

## 执行引擎与集成
- 新增 `codex-workflow` crate：解析、模板渲染、执行调度与步骤执行器。
- 复用 `codex-core` 的会话/事件流与本地命令执行（含沙箱/审批）。
- 结构化日志与产物落盘。细节见 `engine.md`。

## 兼容与安全
- 不修改任何 `CODEX_SANDBOX_*` 相关逻辑；所有命令通过现有 spawn 路径。
- `approval_policy`、`sandbox_mode` 保持与现有 CLI/TUI 一致的优先级合并。

## 后续扩展（开放问题）
- 表达式引擎选择（Jinja/Handlebars）与可用内置函数集合。
- `codex` 步骤输出抓取策略的标准化（纯文本/JSON/patch）。
- 资源锁/互斥编排（跨步骤串行化共享资源）。

---

- 详细 CLI 交互：见 `cli.md`
- 详细 TUI 交互：见 `tui.md`
- YAML Schema：见 `schema.md`
- 引擎实现计划：见 `engine.md`
- 示例与最佳实践：见 `examples.md`
- 团队协作与落地路径：见 `team.md`
- 2B 业务场景优先事项：见 `b2b.md`
- Top3 不确定性与最小实验：见 `experiments.md`
 - （低代码思路已拆分到各文档：engine/schema/cli/tui/team/examples）
