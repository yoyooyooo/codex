## 依赖收敛清点（regex / indexmap / serde_yaml）

目标
- 在不影响现有功能与快照前提下，尽量将三方依赖限定在 `codex-modes`（或 crate 内私有模块）以缩小与上游的根工作区差异。

当前使用分布（基于 ripgrep 快照，仅示例）
- regex（含 regex-lite）
  - core：`core/src/client.rs`（重试/节流提示解析）、`core/src/default_client.rs`、`execpolicy/*`（策略解析/匹配）
  - tui：`tui/src/citation_regex.rs`（行内引用高亮）
  - modes：`modes/src/lib.rs`（变量 pattern 预验证）
- serde_yaml
  - tui：`tui/src/modes/*`（变量默认值/交互）、`tui/Cargo.toml`
  - modes：frontmatter 解析/默认值承载（`serde_yaml::Value`）
- indexmap
  - tui：`tui/src/modes/*`（保持变量/模式顺序）、状态缓存
  - modes：`EnabledMode.variables`、扫描结果与顺序语义

风险评估
- regex：广泛用于 execpolicy/core 与 tui 引用渲染，难以内联到 modes；建议保留 workspace 依赖。
- serde_yaml：tui 的变量编辑依赖 `serde_yaml::Value` 表达默认值与显式值的并存；短期难以内联；建议保留。
- indexmap：tui 的状态与顺序一致性（快照稳定）依赖 IndexMap/IndexSet；短期保留。

分阶段方案（建议）
- 阶段 1（文档化 + 对齐）：
  - 明确在 modes 暴露更稳定的 API：`EnabledMode`、`RenderOptions`、`Validation*`、`format_modes_error`，并在 TUI 侧优先使用它们（已完成）。
  - 在 docs 标注三方依赖的使用分布与去耦边界（本文）。
- 阶段 2（轻度内聚）：
  - 已完成：在 `modes` 暴露 re-export：`pub use indexmap::{IndexMap, IndexSet};`，并将 TUI 的 `modes/*` 与 `chatwidget.rs` 中的 IndexMap/IndexSet 使用切换为 `codex_modes::IndexMap/IndexSet`（不改功能，仅收敛依赖源）。
  - 可选：将 `tui/src/modes/*` 中的 `serde_yaml::Value` 使用转为通过 `codex-modes` 的类型/辅助函数（若可行）。
- 阶段 3（择机推进）：
  - 若上游接受，把 TUI 侧对 `regex` 的行内引用高亮独立成一个小模块，并作为可选特性或单独 crate 管理，避免根工作区 churn。

当前决策
- 已移除 `codex-tui` 对 `indexmap` 的直接依赖；其余 workspace 依赖暂不调整（避免大范围 churn）。
- 优先确保行为/快照稳定；依赖收敛在后续独立 PR 中小步推进。

附：统计快照（节选）
- `regex` 出现文件示例：
  - `tui/src/citation_regex.rs`、`core/src/client.rs`、`execpolicy/src/policy*.rs`
- `serde_yaml` 出现文件示例：
  - `tui/src/modes/mode_bar.rs`、`modes/src/lib.rs`
- `indexmap` 出现文件示例：
  - `tui/src/modes/state.rs`、`tui/src/modes/mode_bar.rs`、`modes/src/lib.rs`

备注
- 真正的收敛效果更多体现在“跨仓库合并时的冲突概率下降”，而不是单次提交的代码减少；因此优先保持扩展点稳定、库层承担复杂度，依赖收敛稳步推进即可。
