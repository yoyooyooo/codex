# 实施规划（最小差异化）

阶段 0：准备与基线
- 通读 `codex-rs/core` 与 `codex-rs/tui` 的 Slash、prompt、`TurnContext` 与历史重建逻辑；确认 `Op::OverrideTurnContext { user_instructions }` 已可用。
- 整理 `.codex/modes` 目录约束 checklist（见 files.md）。
- 确认采用 A 案：前端合成 + 覆写；核心不新增协议变体、不持久化模式状态。

阶段 1：前端共享库（Rust）
- 扫描 `.codex/modes` + `$CODEX_HOME/modes`；`IndexMap` 保序；非法 ID 过滤。
- 解析 frontmatter；实现 required/default/enum/pattern/shortcuts 校验。
- 渲染器：输入（base_user_instructions、启用集合、变量值）→ 输出完整 `<user_instructions>`。
- 单测覆盖目录合并、变量校验、渲染稳定性、等价检测。

阶段 2：核心最小接线（现有实现校核）
- `TurnContext` 保留 `base_user_instructions`；接收覆写后 `ConversationHistory::replace` 重建首条 `<user_instructions>`（已实现，回归验证）。

阶段 3：TUI
- Mode 条、变量编辑、错误提示、去抖、快照；内置预览/渲染视图。

阶段 4：文档与迁移
- 完成本目录文档；在旧路径放置跳转指引。
- 如需，提供迁移脚本草案：`scripts/migrate_commands_to_modes.rs`。

阶段 5：验收与质量
- 执行 testplan.md；回归 `codex-core`、`codex-tui` 测试；记录已知限制与后续扩展。

上游对齐策略
- 所有改动限定在前端（Rust）与最小核心接线；协议不新增变体。
- 文档与实现保持“无模式时等价于现状”的兼容原则；便于长期 rebasing。

适配层留口（新增）
- 引入 `ModeEngine` 与 `DataSource` trait：
  - `trait DataSource { fn load_defs(&self) -> Result<IndexMap<Id, ModeDef>, ModesError>; }`
  - 提供 `LocalFsDataSource`（当前落地），预留 `ServerEventDataSource`（未来对接上游事件/持久化）。
- TUI 依赖 `ModeEngine`，不直接触达数据源；确保未来切换数据源时无需 UI/调用层改动。
- 验收新增项：
  - 禁用所有模式时 `<user_instructions>` 哈希与上游一致；
  - 开启/关闭模式不改变除 `<mode_instructions>` 外的任何文本；
  - `is_equivalent` 命中时不发送覆写（可计数打点）。

---

## 分阶段整改（面向当前实现回收差异面）

为降低与上游冲突，建议按“阶段”推进（每个阶段可包含一个或多个 PR）：

- 阶段 A：扩展点注入（宿主 ≤ 60 行）：
  - `BottomPane` 注入 `BottomPaneAddon` 调用点（height/render/keys）；
  - `ChatWidget` 注入 `AppLifecycleHook` 调用；
  - 不改变 UI 行为与快照。
- 阶段 B：迁移 ModeBar/Panel 到扩展：
  - 删除宿主内联渲染与业务分支；
  - 扩展内保持现有键位/行为；
  - 仅 `tui/src/modes/**` 快照变化。
- 阶段 C：事件面收敛：
  - 移除 `UpdateModeSummary`/`UpdatePersistentModeState`；
  - 若保留 `OpenModeBar`，保持为唯一新增事件；
  - 其余通过扩展直接回调宿主 API。
- 阶段 D：库层归口：
  - 在 `codex-modes` 暴露 `normalize_equiv` 与 `Debouncer`；
  - UI 统一引用库层，移除重复实现。
- 阶段 E：Cargo 收敛：
  - 三方依赖从 workspace 根移除，仅保留在 `codex-modes`；
  - 如冲突频发，切换“B 案（内联 engine）”。
- 阶段 F：样例与 CI：
  - 移除根 `.codex/modes/**` 与 `.codex/prompts/**`；
  - 增加 CI 守卫。
- 阶段 G：非目标改动拆分：
  - 将与模式无关的 Core 变更（如 timeout 字段重命名）分离提交或回退。

完成以上切分后，重跑 `cargo test -p codex-tui` 并确认快照边界满足：非 `tui/src/modes/**` 快照无变化。
