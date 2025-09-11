# CLI 设计（草案）

新增多工具子命令：`codex workflow <subcommand>`。

## 子命令

1) `codex workflow list`
- 作用：列出 `.codex/workflows` 下可用工作流。
- 输出：表格/列表含 `name / description / path`。

2) `codex workflow validate [<name>]`
- 作用：校验全部或指定工作流的 YAML schema、DAG、输入定义。
- 退出码：校验失败时非零，输出错误详情。

3) `codex workflow explain <name>`
- 作用：打印解析后的 DAG、全局/步骤并发、`needs` 关系、输入默认值。
- 选项：`--json` 以结构化 JSON 输出。

4) `codex workflow run <name>`
- 作用：以参数化方式运行指定工作流。
- 选项：
  - `--param k=v`（可多次）：为 `inputs.k` 赋值（TOML 字面量或字符串）。
  - `--dry-run`：仅渲染与计划，不执行；输出计划与最终命令。
  - `--json`：结构化流式事件/总结（便于集成 CI）。
  - `--concurrency N`：覆盖 YAML 中的全局并发上限。
  - `--profile P`：覆盖执行时使用的 Codex Profile。
  - `--preview`：对包含 `render_template` 的步骤，仅生成多文件 diff 并退出（不写盘）。

5) `codex workflow pack` / `codex workflow unpack`
- 作用：在团队间共享工作流/模板。
- `pack`：将 `.codex/{workflows,templates,registry.yml}` 打包为 tar，并生成 `manifest.json`（条目列表、sha256、来源、版本）。
- `unpack`：在目标仓库将包解开到 `.codex/shared`，合并 `registry.yml` 并进行完整性校验。

## 行为细节
- 审批策略：沿用 `config.toml` / CLI 覆盖的 `approval_policy`。
- 沙箱策略：沿用 `sandbox_mode`（CLI > profile > config > 默认）。
- 工作目录：默认项目根；可在 YAML 步骤或 CLI 覆盖。
- 运行产物：写入 `runs/<name>/<timestamp>/`，结束时打印摘要与路径。

## 退出码
- `0`：全部步骤成功（或有明确 `skipped`）。
- `1`：至少一条步骤失败（且未被继续策略覆盖）。
- 其他：CLI 层错误（解析失败、找不到工作流等）。
