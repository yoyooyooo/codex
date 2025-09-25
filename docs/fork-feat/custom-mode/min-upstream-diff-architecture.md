# 最小上游差异架构方案（自定义模式 / 全栈，保持现有逻辑）

> 实施进展更新（当前状态/下一步）

以下是当前“最小上游差异架构（自定义模式）”的实现现状、进展与下一步规划。

## 现状总览

- 扩展点已落地并启用
  - 通用视图工厂 UiViewFactory（兼容 ModeUiFactory，经适配器统一入队）＋默认 ModesUiDefaultFactory 已接管 ModeBar/ModePanel 构建；宿主仅挂载扩展，内联回退逻辑已移除（保留提示）。
  - ChatWidget 内不再维护独立 `mode_ui_factories` 队列；如需沿用旧接口，调用 `register_mode_ui_factory(..)` 即会通过适配器注入到通用工厂队列。
  - BottomPane 扩展点 BottomPaneAddon、生命周期钩子 AppLifecycleHook 已具备并接入渲染/按键/高度的最小插桩。
- 事件面收敛完成
  - 模式摘要/持久状态更新不再依赖专用事件；采用 ChatWidget 内部 UI 任务队列（主线程直连），App/TUI 事件处理结束后统一 drain_ui_tasks()。
- 模式逻辑统一至 codex-modes
  - 规范化等价：normalize_equiv + is_equivalent
  - 去抖：DebounceGen（generation 判定）
  - 校验与错误码：ValidationError（E3101/E3102/E3106/E3107/E3108）、validate_var_value、validate_enabled、format_validation_error
  - 渲染与错误映射：render_user_instructions + format_modes_error（E1001/E1004/E2001/E2201/E1201）
  - 文案与摘要：enabled_labels、applied_message、format_mode_summary
- 行为与快照
  - 交互、快照未变（ModeBar/ModePanel/摘要/快捷键/等价短路/去抖等保持不变）；本地与 CI 均通过
- 差异面收敛
  - TUI 热点文件（chatwidget.rs、bottom_pane、modes/*）逻辑显著变薄；上游 churn 时冲突面更小
  - 仓库根示例 .codex/modes/** 与 codex-rs/.codex/prompts/** 已移除
  - CI 守卫已加入：禁止仓库根目录新增 `.codex/modes/**`、`.codex/prompts/**`（见 `.github/workflows/ci-guards.yml`）
  - 依赖差异：`codex-tui` 不再直接依赖 `indexmap`，改为复用 `codex-modes` 的 re-export（IndexMap/IndexSet）

## 已落地的关键点

- 架构分层（协议保持不变，仅用 Op::OverrideTurnContext{ user_instructions }）
- 扩展可插拔（UiViewFactory/BottomPaneAddon/AppLifecycleHook）＋默认模式工厂
- UI 更新直连（线程安全）：UI 队列＋主线程 drain
- 模式逻辑下沉（渲染/等价/去抖/校验/文案）至 codex-modes，TUI 仅做 UI 与交互
- 测试与快照均通过（cargo test -p codex-tui 349/0/1）

## 遗留与可选优化

- 轻微未用项（非功能问题）：
  - ModeUiFactoryAdapter、ModeUiContext.frame_requester、BottomPane::register_addon 在当前默认路径未直接使用（保留以支持后续扩展/上游兼容）
- 依赖收敛（PR‑5）：workspace 顶层的 indexmap/serde_yaml 等广泛被其他 crate 使用；进一步收敛需分支谨慎推进

## 下一步规划（一次性完成版）

- 依赖差异缩小（PR‑5）
  - 目标：将三方依赖（如 regex/indexmap/serde_yaml）尽可能仅出现在 codex-modes/Cargo.toml；评估对其他 crate 的引用，尽量避免 workspace 根变动
  - 步骤：
    - 清点 TUI/其他 crate 对这些依赖的直接/间接使用
    - 能隔离的下沉到 codex-modes（或本地 module 复制小工具）
    - 对无法隔离的，保留 workspace 声明，避免引发大范围 churn
- CI 守卫与文档
  - CI：禁止仓库根目录提交 .codex/modes/**、.codex/prompts/**
  - 文档：补充扩展指南（UiViewFactory/BottomPaneAddon 接入方式，见 `docs/fork-feat/custom-mode/ui-extension-guide.md`）；codex-modes API 使用示例与错误码表
- 库层 API 打磨（便于上游复用）
  - 将更多文案与结构化返回聚合到 codex-modes（例如将 ModeBar/Panel 的“变量区块串”进一步规范化输出，以便 UI 仅负责渲染）
  - 为 render_user_instructions 增加更细粒度错误类型（当前 format_modes_error 兼容映射）
- 代码小清理（非功能性）
  - 根据需要去除当前未用适配器/字段（或加上 #[allow] 注释说明）


本方案在“功能优先”的前提下追求最小上游差异：任何会削弱现有/规划功能扩展能力的“最小化差异”都不会被采纳。我们通过“功能优先级 → 扩展点封装 → Host 触点收敛”的顺序实现平衡：不牺牲能力与可扩展性的前提下，把不可避免的差异面收敛到少量、稳定、可控的插桩位。

目标：在“保留当前所有功能与交互逻辑不变”（含底部摘要行、ModeBar、面板、变量编辑、去抖与等价短路等）的前提下，通过架构分层与扩展点抽象，尽可能缩小与上游的长期差异面（协议/Core/TUI/CLI/SDK），降低同步与合并冲突成本。

功能优先的基本盘（不打折扣）
- ModeBar/ModePanel 的交互手感与键位（Alt+B/Tab/←→/Space/Enter/d/Esc）
- 底部模式摘要行常驻显示（启用>0 时）与详情折叠
- 等价短路与去抖（避免历史污染与抖动）
- 会话内持久状态（resume 后可复现）

## 总体原则
- 不新增协议枚举或事件；复用 `Op::OverrideTurnContext` 的 `user_instructions` 字段携带完整文本。
- Core 不做模式扫描/持久化；不引入服务端真源，所有模式来源与渲染在前端完成。
- TUI/UI 改动可拔插，通过 Addon 隔离与最小插桩，尽量贴合上游行为/快照。
- 功能“保留不删”，仅迁移代码位置与依赖方向，做到“形变不改意”（behavioral parity）。
- 示例/演示文件不进仓库根目录（放到 docs 或 tests/fixtures 中）。

平衡约束（功能优先 vs 最小差异）
- 若“最小差异”与关键功能/扩展性发生冲突，优先保证功能，但同时将差异封装在稳定扩展点内，避免扩散到 Host 热点文件。
- 对“不可少的”Host 触点，采用“固定插点 + 小步稳定 API”的方式，确保未来上游 churn 时仅需调整少量行数即可适配。

强约束（最小化上游差异的硬前提）：
- 触点收敛：Host 改动严格控量，模式逻辑下沉到 `tui/src/modes/` 与 `codex-modes/`。
- 策略统一：等价规范化与去抖由共享库/扩展承担，Host 不重复实现。

与上游变更的现实背景（用于指导收敛）
- 上游 `tui` 层热点文件（`chatwidget.rs`、`bottom_pane/mod.rs`、`app.rs`、`app_event.rs`）改动频繁。一旦在这些文件内直接新增字段/分支/渲染逻辑，后续 rebase/merge 的冲突概率与成本会显著上升。
- 目前实现对比 upstream/main 的差异面集中在：
  - `chatwidget.rs`：大量新增字段/方法（百行级改动）；
  - `bottom_pane/mod.rs`：内联“摘要行渲染+高度计算”的分支；
  - `app_event.rs`：新增枚举项（结构性冲突源）；
  - 工作区 Cargo：新增 `modes` crate 与依赖扩散；
→ 通过“Addon 扩展点 + 事件面收敛 + Cargo 变更减面”，将上述差异回收到可控范围。

已存在并需保留的 TUI 事件与行为（保持但限缩触点）：
- `AppEvent::UpdateModeSummary`：更新底部摘要行文案。
- `AppEvent::OpenModeBar`：请求打开 ModeBar。
- `AppEvent::UpdatePersistentModeState`：回传持久模式状态缓存。
- `AppEvent::CodexOp(OverrideTurnContext{ user_instructions })`：一次性覆写。
上述事件仅在 `app.rs`/`chatwidget.rs` 的小型分支内处理，避免扩散更多事件类型。

## 分层与职责

1) 协议层（protocol）
- 保持现状：仅在 `Op::OverrideTurnContext` 暴露 `user_instructions: Option<String>`。
- 不引入 `ListCustomModes/SetModeState` 等新事件（后续若上游提供，再通过适配层接入）。

2) 共享库（codex-modes）
- 承担“文件发现→frontmatter 解析→变量校验→渲染→等价判定/去抖”的全部复杂度。
- API 建议：
  - `scan_modes(cwd, codex_home) -> Vec<ModeDefinition>`
  - `render_user_instructions(base, enabled, defs) -> String`
  - `is_equivalent(a, b) -> bool` + `normalize(text) -> String`
  - 可选：`ModeEngine`（管理 enabled 集与变量），`Debouncer`（节流覆盖）。
- 错误码对齐 `docs/fork-feat/custom-mode/errors.md`（E100x/E210x/E31xx/E32xx）。
- 预留数据源抽象（Spec 里的 `DataSource`/`ServerEventDataSource`），未来可平滑切换到上游事件。

依赖与 Cargo 差异缩小策略（两案并行评估）
- A 案（当前）：独立 crate `codex-modes`，`tui` 依赖之。优点：边界清晰；缺点：workspace 更改与依赖扩散易与上游冲突。
- B 案（备选）：阶段性将 modes 逻辑内联到 `tui/src/modes/engine.rs`，待上游稳定后再抽 crate。优点：降低根 `Cargo.toml` 冲突；缺点：短期复用度略降。
- 折中：保持独立 crate，但将 `regex/indexmap/serde_yaml` 等三方依赖仅留在 `codex-modes/Cargo.toml`，避免进入 `[workspace.dependencies]`。

现状对齐：等价判定与 200ms 去抖目前在 ModeBar/ModePanel 内实现，短期保持；中期可上移到库层统一实现，Host 侧仅感知“是否需要发送一次覆写”。

3) Core（codex-core）
- 仅在接收到 `Op::OverrideTurnContext{ user_instructions: Some }` 时：
  - 更新 `TurnContext.user_instructions`、保留 `base_user_instructions`；
  - 用现有 `build_initial_context` 重建首段 `<user_instructions>`；
  - 不落地“模式状态”；不存储变量；无模式相关文件 IO。
- 不做与模式无关的改动（例如 config 写入键名、额外日志接口等），避免扩大变更面。
- 约束：保持 `conversation/history` 辅助函数稳定，供前端依赖（当前已满足）。

- 4) TUI/CLI（codex-tui）
- UI 策略（保留现有逻辑，收敛接触面）：
  - 保留“底部模式摘要行 + 详情展开 + ModeBar 内联编辑 + ModePanel 弹窗”等现有交互与视觉；
  - 将“摘要行渲染、详情渲染、按键分发、内联编辑、面板打开/应用”整体封装为一个独立的 UI 扩展（Addon），放置于 `tui/src/modes/`；
  - 底部 Pane 与 ChatWidget 仅提供“扩展点”钩子：
    - `BottomPaneAddon`：查询额外高度、渲染额外行、接收 Key 事件、获知任务状态；
    - `AppLifecycleHook`（可选）：在 `SessionConfigured`、`NewSession` 时初始化/恢复状态；
  - 通过在 `bottom_pane/mod.rs` 与 `chatwidget.rs` 增加极少量“调用扩展点”的代码，即可挂载/卸载整个模式 UI，减少对上游大文件的直接修改行数；
  - 输入/快捷键：在扩展内部处理 `Alt+B/Tab/←→/Space/Enter/Esc/↓` 等；底层将 KeyEvent 先分发给 Addon，未消费再交给原有逻辑（不改变现有行为，仅改变实现位置）。
- 入口：保留现有快捷键与 Slash 项，注册逻辑内聚到扩展模块；`app.rs` 不新增大段 key 匹配分支。
- 等价/去抖：现已在视图内部实现，短期保持；中期移入库层/扩展内部统一实现，Host 仅感知“是否需要发送一次覆写”。
- 基线文本：当前在 ChatWidget 内部保留“基线提取与项目文档拼接”，短期不动；中期迁移到库层/扩展内部（不改行为）。
  

> 扩展点草案（示意）
```
pub trait BottomPaneAddon {
    fn desired_extra_height(&self, width: u16) -> u16;
    fn render_extra(&self, area: Rect, buf: &mut Buffer);
    fn handle_key_event(&mut self, key: KeyEvent) -> bool; // true=consumed
    fn on_task_running(&mut self, running: bool) {}
}

pub trait AppLifecycleHook {
    fn on_session_configured(&mut self, ev: &SessionConfiguredEvent) {}
    fn on_new_session(&mut self) {}
}
```
Host 侧改动仅为：
- 在 `BottomPane` 中维护 `Vec<Box<dyn BottomPaneAddon>>`，按序计算高度与渲染；
- 在 `handle_key_event` 前先询问 Addon 是否消费；
- 在 `ChatWidget::on_session_configured` 时调用已注册的 `AppLifecycleHook`。

持久模式状态（新增并需保留）
- `tui/src/modes/state.rs` 中的 `PersistentModeState { enabled, enable_order, var_values }`：
  - 在 ModeBar/ModePanel 与 App/ChatWidget 之间传递“启用集合、启用顺序、变量值”；
  - `sanitize(defs)` 在 defs 变化时剔除失效 id、过滤变量名，保证状态与扫描结果一致；
  - 重新打开 ModeBar/ModePanel 时使用 ChatWidget 缓存的 `PersistentModeState` 作为初始值，避免“二次进入被重置”。
- 状态流转：
  1) ChatWidget 初始化/自动应用默认模式时构造并写入 `persistent_mode_state`；
  2) ModeBar/ModePanel 渲染或应用变更时，通过 `AppEvent::UpdatePersistentModeState` 回传；
  3) App 在 `AppEvent` 分支中调用 `chat_widget.set_persistent_mode_state(state)` 写回；
  4) 再次打开 ModeBar/Panel 时直接复用，保证 UI 与真实启用列表一致。
- 变量值策略：ModeBar 支持变量值编辑并同步 `var_values`；ModePanel 仅做启用/禁用，不携带 `var_values`（保持现状）。

5) SDK/后续 CLI（可选）
- 如需 CLI 操作模式，沿用“前端渲染、一次性覆写”的流程，不引入新协议。
- SDK 侧仅提供“扫描 → 渲染 → OverrideTurnContext”的封装。

## 代码组织
- 目录：
  - 共享库：`codex-rs/modes`（已存在）。
  - TUI：`codex-rs/tui/src/modes/`（仅此目录新增/修改）。
 

## 现有提交的“差异收敛”建议（保持功能等价）
- 移除仓库内示例：`.codex/modes/*.md`、`codex-rs/.codex/prompts/*.md` 改为 docs 示例或 tests/fixtures。
- Core 中与模式无关的改动（例如 `startup_timeout_ms` 写入）建议回退或拆分独立 PR。
- TUI 侧：
  - 保留“模式摘要栏 + ModeBar + 面板 + 持久状态”完整功能，但把其实现迁移到 `BottomPaneAddon`/`AppLifecycleHook` 扩展模块内；
  - 将规范化比较和去抖迁移到 `codex-modes`/扩展内部；
  - 已存在的 `AppEvent::{UpdateModeSummary, OpenModeBar, UpdatePersistentModeState}` 暂保留并集中在少量 switch 分支；若采用扩展接口，可将其中部分改为扩展内部渲染与状态管理，进一步减少 Host 触点（行为不变）。

## 迁移步骤（执行顺序，行为不变）
1) 代码清理：删除仓库内示例配置文件；回退无关 Core 改动（不影响模式功能）。
2) 抽象扩展点：在 `BottomPane` 与 `ChatWidget` 引入极少量扩展钩子接口；不改变现有渲染/路由顺序。
3) 迁移 UI 实现：将“摘要行、详情、ModeBar、面板、持久状态同步”的现有代码整体搬迁到扩展模块，对外暴露与原先一致的键位与行为；Host 侧删除对应内联实现，仅保留扩展挂载与事件转发。
4) 逻辑下沉：把等价/去抖搬到 `codex-modes`/扩展内部，Host 仅发送一次 `CodexOp`；
5) 基线策略：短期保留 ChatWidget 的基线提取/项目文档拼接；中期由扩展/库层完成（接口不变、行为不变）。
6) （可选）将等价/去抖策略统一搬至 `codex-modes`；通过库 API 保证跨视图一致性；
7) 测试：
   - `codex-modes`：扫描/渲染/错误码/等价判定单测；
   - TUI：仅 `tui/src/modes/` 下的 snapshot；Host 原有快照保持不变（或仅对扩展点注入影响最小的对齐）。

---

## 基于当前实现的“整改路线图”（面向最小差异）

目标：在不牺牲现有功能的前提下，快速将差异面回收到扩展模块与 `codex-modes`，把 Host 热点文件的结构性 diff 降到最低。

建议按 PR 切片推进，每个 PR 保持可运行、快照通过：

1) PR-1 扩展点落地（宿主最小插桩）
- 在 `tui/src/bottom_pane/mod.rs` 增加扩展挂载点：
  - 字段：`addons: Vec<Box<dyn BottomPaneAddon>>`
  - 三个调用点：`desired_height`/`render_ref`/`handle_key_event`
- 在 `tui/src/chatwidget.rs` 增加生命周期钩子调用：
  - `on_session_configured` 尾部调用 `AppLifecycleHook::on_session_configured`
  - 打开模式入口改为通过扩展工厂创建视图
- 验收：Host 改动合计 ≤ 60 行；非 modes 快照不变。

2) PR-2 逻辑迁移（UI 下沉到扩展）
- 将现有 ModeBar/ModePanel/摘要/变量编辑/去抖/等价判定整体搬迁到扩展模块（`tui/src/modes/…`）内部；
- Host 删除内联渲染与业务分支，仅保留扩展挂载与事件/按键转发；
- 验收：行为等价；快照变化仅限 `tui/src/modes/snapshots/**`。

3) PR-3 事件面收敛
- 首选移除 `UpdateModeSummary`/`UpdatePersistentModeState` 两个 AppEvent，改为扩展直接调用 `BottomPane.set_mode_summary()` 与内部状态；
- `OpenModeBar` 若短期保留，合并为单一入口；后续可下沉到扩展按键处理；
- 验收：`app_event.rs`/`app.rs` 结构性 diff 收敛为 0–1 个事件。

4) PR-4 归一化与去抖归口（库层）
- 在 `codex-modes` 提供 `normalize_equiv(a,b)` 与 `Debouncer`；ModeBar/ModePanel 改为调用库层；
- 移除 `tui/src/modes/mod.rs` 中临时 `normalize_for_equivalence`；
- 验收：等价/去抖在两个视图结果一致；无 UI 差异。

5) PR-5 Cargo 差异缩小
- 将 `indexmap/regex/serde_yaml` 等依赖从根 workspace 移除，仅保留在 `codex-modes/Cargo.toml`；
- 若上游根 Cargo 变更频发，考虑短期“B 案：内联 engine 到 tui/src/modes/engine.rs”，待稳定后再回抽 crate；
- 验收：根 `Cargo.toml` 与上游 diff 明显缩小。

6) PR-6 样例与 CI 守卫
- 移除根目录样例 `.codex/modes/**` 与 `.codex/prompts/**`；样例放 `docs/` 或 `tests/fixtures/`；
- 在 CI 增加守卫，禁止根目录出现上述路径；
- 验收：仓库根无样例资源，CI 守卫有效。

7) PR-7 非目标改动拆分
- 与模式无关的 Core 改动（例如 `startup_timeout_ms` 写入重命名）拆分独立 PR 或回退，避免在同一提交扩大冲突面；
- 验收：本功能 PR 仅包含与模式相关的最小改动。

里程碑验收（阶段性）
- 与上游兼容性：Host 热点文件结构性 diff 显著下降；仅剩扩展插点；
- 快照边界：非模式快照保持稳定；
- 代码预算：Host 插桩 ≤ 60 行；根 Cargo 近等于上游；
- 功能：ModeBar/Panel/摘要/自动启用/等价短路/去抖交互与现状一致。

## 发布与回滚
- 发布：默认启用；通过快照边界与触点收敛保持与上游兼容。
- 回滚：如遇大规模 upstream 变更，可暂时停用 Addon 注册（或挂载空实现），后按 API 适配层慢迁。

## 验收标准
- 与上游兼容性：非模式相关快照保持不变；改动仅集中在 `tui/src/modes/**`（前缀 `modes__*`）。
- 功能性：
  - ModePanel 可列出/编辑/应用模式；
  - 覆写仅在渲染产物与已生效文本不等价时发生；
  - 历史中只出现简短反馈，不新增持久 UI 行。

## 扩展点 API（最终签名）
```
pub trait BottomPaneAddon {
    /// 需要占用的额外高度（行）。
    fn desired_extra_height(&self, width: u16) -> u16;
    /// 在底部 Pane 的预留区域渲染（只负责自身区域）。
    fn render_extra(&self, area: Rect, buf: &mut Buffer);
    /// 处理键盘事件；返回 true 表示事件已被消费。
    fn handle_key_event(&mut self, key: KeyEvent) -> bool;
    /// 通知当前是否有任务在执行（可用来调整提示文案）。
    fn on_task_running(&mut self, running: bool) {}
}

pub trait AppLifecycleHook {
    /// 会话配置完成（含初始消息回放）时回调，可用于同步当前渲染态。
    fn on_session_configured(&mut self, ev: &SessionConfiguredEvent) {}
    /// 新建会话时回调（与 resume 区分）。
    fn on_new_session(&mut self) {}
}
```

分发与时序约束：
- 键盘事件优先投递给 Addon，未消费再交给原有逻辑（不改变原有按键行为）。
- 渲染顺序：Status → Composer → Addon（Addon 区域固定在最底部上方，或由 Addon 自行绘制分隔线）。
- 线程模型：Addon 仅在 UI 线程运行；重 IO 放库层/异步，完成后通过 `AppEvent` 回到 UI。

宿主插入点（参考实现与预算）：
- `tui/src/bottom_pane/mod.rs`
  - 在 `desired_height` 叠加 `addons` 高度（≤3 行）。
  - 在 `render_ref` 中于 composer 下方渲染 Addon（≤6 行）。
  - 在 `handle_key_event` 前置遍历 `addons` 并短路（≤6 行）。
- `tui/src/chatwidget.rs`
  - 在 `on_session_configured` 尾部调用 `AppLifecycleHook::on_session_configured`（≤2 行）。
  - 在打开模式 UI 的入口构造并注册 Addon（≤4 行）。
- `tui/src/app.rs`
  - 首选仅处理 `OpenModeBar`（若保留）并转发；`UpdateModeSummary`/`UpdatePersistentModeState` 建议在 Addon 内直接调用宿主 API/回调，减少结构性分支（≤10–25 行）。

目标：三处 Host 文件新增/改动合计 ≤ 60 行；其余全部位于 `tui/src/modes/` 与 `codex-modes/`。

## 事件面收敛（减少结构性冲突）
- 首选移除 `UpdateModeSummary`/`UpdatePersistentModeState` 两个 AppEvent：
  - 由 Addon 直接调用 `BottomPane.set_mode_summary(...)`，不经过 `AppEvent`；
  - 持久模式状态由 Addon/库层私有维护，如需共享，用细粒度回调（而非枚举）传递；
- “Down 在历史末尾打开 ModeBar”下沉到 Addon 的 `handle_key_event`，消除 `OpenModeBar` 事件；
- 若短期保留 `OpenModeBar` 以兼容现有实现/测试资产，也可，只要总体分支规模 ≤ 1 个事件；
- 目标：`app_event.rs` 与 `app.rs` 的结构性 diff 收敛为 0–1 个枚举/分支。

事件迁移小贴士：
- 在扩展内部维护 `PersistentModeState` 与最近一次渲染文本；
- 需要 UI 更新时，优先直调宿主 API/回调（函数调用）而非新增 `AppEvent`；
- 键盘优先分发给扩展，未消费再回落原逻辑。

## Host 触点（定量化约束）
- 允许改动文件与位置（建议上限）：
  - `codex-rs/tui/src/bottom_pane/mod.rs`
    - 新增：`addons: Vec<Box<dyn BottomPaneAddon>>` 字段（1 行）。
    - 在 `desired_height/render_ref/handle_key_event` 各插入最多 1 个调用点（≤ 6 行）。
  - `codex-rs/tui/src/chatwidget.rs`
    - 在 `on_session_configured` 调用 `AppLifecycleHook::on_session_configured`（≤ 2 行）。
    - 打开面板/模式条入口处通过扩展工厂创建视图（≤ 4 行）。
  - `codex-rs/tui/src/app.rs`
    - 首选仅保留：`OpenModeBar`（或全部回收到 Addon）；`CodexOp` 保持上游处理；其余移除。
- 目标：Host 三个文件合计新增/改动 ≤ 60 行；其余逻辑全部位于 `tui/src/modes/` 与 `codex-modes/`。

## 扩展点 API 定稿
- BottomPaneAddon（最终签名建议）
  - `fn desired_extra_height(&self, width: u16) -> u16;`
  - `fn render_extra(&self, area: Rect, buf: &mut Buffer);`
  - `fn handle_key_event(&mut self, key: KeyEvent) -> bool; // true=消费`
  - `fn on_task_running(&mut self, running: bool) {}`
- AppLifecycleHook（可选）
  - `fn on_session_configured(&mut self, ev: &SessionConfiguredEvent) {}`
  - `fn on_new_session(&mut self) {}`
- 分发约束
  - 键盘事件优先给 Addon；未消费再传原逻辑。
  - 渲染顺序：Status → Composer → Addon（底部区域预留 1 分隔行 + 1 内容行）。
  - 线程/异步：Addon 只在 UI 线程使用；耗时 IO 放到库层/异步任务后回主线程发 `AppEvent`。

## 构建与隔离策略（无 feature gate）
- 默认启用自定义模式能力，不使用 feature gate 控制。
- 通过模块边界与 Addon 插桩实现隔离：
  - 模式 UI 与逻辑集中在 `tui/src/modes/`，Host 仅保留少量挂载与转发代码。
  - 共享库 `codex-modes` 承担扫描、解析、渲染与策略（去抖/等价）。
- 新增 crate/依赖遵循“只加不改”原则，避免扰动上游对等 section 的排序与注释。

Cargo/workspace 差异控制（建议）
- 避免在根 `[workspace.dependencies]` 添加 `codex-modes` 的三方依赖（如 regex/indexmap/serde_yaml），改为仅在 `codex-modes/Cargo.toml` 管理；
- 如遇上游对根 `Cargo.toml` 变更较大、冲突频繁，可选择“B 案（内联 engine）”阶段性收敛，待上游稳定再回抽独立 crate；

最小化冲突的具体做法：
- 不改变根 `members` 的顺序与注释；新增成员放末尾且不重排；
- 如必须新增成员，先评估“B 案”可行性，避免 workspace 层新增依赖。

## 归一化与去抖归口（codex-modes）
- 统一提供：
  - `fn normalize_equiv(a: &str, b: &str) -> bool`：内部执行 CRLF→LF、逐行去尾空格、空行折叠；
  - `struct Debouncer { … }`：基于递增代号/时间窗口，仅提交最新一次。
- ModeBar/ModePanel 改为调用库层 helper；Host 无须关心具体策略。
- 现状兼容：短期保留视图内部实现，迁移时仅替换为库层调用（结果一致）。

## 测试与快照边界
- 新增 snapshot 仅放在 `tui/src/modes/…`；命名含前缀 `modes__` 以便隔离。
- 单测侧重：
  - `PersistentModeState::sanitize` 行为；
  - 等价短路与去抖一致性；
  - 事件流（回传/写回）不丢失、顺序稳定；
  - 渲染顺序/换行/分隔线样式（ratatui Stylize 约定）。

命令约定：
- `cargo test -p codex-tui`：新增/变化快照仅应出现在 `modes__*` 前缀文件。

守护性检查（建议纳入 CI）：
- 断言非 `tui/src/modes/**` 的快照未变化；
- 检查仓库根不得包含 `.codex/modes/**` 或 `.codex/prompts/**`；
- 若改动触达 `core/protocol`，需通过完整测试 `cargo test --all-features` 并说明与上游兼容性。

## 仓库内容与提交约束
- 禁止将 `.codex/modes/*.md`、`codex-rs/.codex/prompts/*.md` 等示例文件提交到仓库根目录；样例放到 `docs/` 或测试夹具。
- 与功能无关的 Core 改动（如 config 键名/日志格式）单独 PR，不与模式功能耦合，降低冲突面。

额外：建议在 CI 增加守卫，禁止根目录出现 `.codex/modes/**` 或 `.codex/prompts/**`。

## 风险登记与回退路径
- 高风险触点：`chatwidget.rs`、`bottom_pane/mod.rs`、`app.rs`（上游 churn 热点），`Cargo.toml`（成员/依赖排序）。
- 回退策略：
  - 保留扩展点挂载空实现：如需回退，可停止注册 Addon 或挂载空实现，Host 改动最小；
  - 若上游引入官方模式事件：在 `codex-modes` 替换数据源（DataSource→ServerEventDataSource），TUI/UI 保持不变。

合并冲突减缓：优先通过 Addon/特性隔离“常改文件”的改动，尽量在扩展模块内部 churn，减少与上游的直连冲突。

## 上线前 Checklist
- [ ] `-p codex-tui` 测试与 snapshot 验收；非 `modes__*` 快照无无关变更；
- [ ] 等价/去抖 helper 由 `codex-modes` 提供并被两视图共用；
- [ ] AppEvent 收敛：优先仅保留 `OpenModeBar`（或全部回收到 Addon）；
- [ ] 仓库无示例资源文件混入；
- [ ] 文档与帮助（ui-tui/spec/errors/rendering）已更新。

验收补充：
- [ ] Host 插桩行数统计 ≤ 60 行；`git diff --stat` 有据可查。
- [ ] 上游 rebase 试跑：与上游在热点文件处冲突可控，主要集中在扩展模块。

---

## 事件时序（端到端）
- 用户在 ModeBar/Panel 切换/编辑 → 视图拼装 `EnabledMode` → 调用库层渲染得到新 `<user_instructions>`。
- 去抖窗口内多次变更仅保留最后一次；若 `normalize_equiv(last, now)` 为真则短路（跳过发送）。
- 发送 `AppEvent::CodexOp(OverrideTurnContext{ user_instructions: Some(...) })`。
- Core 接收后替换会话首段 `<user_instructions>`，保留 `<environment_context>` 并重建历史首段。
- TUI 收到局部反馈（Info/错误）并更新摘要栏（`UpdateModeSummary`）与持久状态（`UpdatePersistentModeState`）。

## 实施路线图（建议）
- P0（最小可用）：库层提供 `normalize_equiv` 与 `Debouncer`；样例迁移；保留现有视图实现（功能零牺牲）。
- P1（触点收敛）：引入 BottomPane Addon；Host 插桩 ≤ 60 行；键盘优先分发到 Addon；视图内部逻辑不再散落到 Host（功能等价）。
- P2（事件面收敛）：移除 `UpdateModeSummary`/`UpdatePersistentModeState`，改为 Addon 直接调用宿主 API；视需要保留/移除 `OpenModeBar`。
- P3（Cargo 差异缩小）：将第三方依赖回收至 `codex-modes/Cargo.toml`；必要时切换到“B 案（内联 engine）”。
- P4（数据源抽象）：`DataSource` 接口化，若上游提供官方模式事件，替换为 `ServerEventDataSource`，TUI 与 Host 无需改动。

## 当前实现与目标对齐说明
- 协议：仅复用 `OverrideTurnContext.user_instructions`（不新增事件）— 符合本方案。
- 归一化/去抖：短期视图内实现允许存在；目标是迁移到 `codex-modes` 并统一调用。
- 样例：根目录 `.codex/modes/**` 禁止提交；迁至 `docs/` 或 `tests/fixtures/`。

## 功能优先的扩展保障（不牺牲能力的最小差异）
- Addon 的渲染/键盘/生命周期钩子是“能力超集”，不限制现有或未来的交互扩展；
- 事件面收敛通过“直连宿主 API/回调”实现，不剥夺跨层通信能力，仅将通信方式切换到更稳定的函数调用；
- Cargo 收敛提供“独立 crate / 内联 engine”双轨路径，允许按阶段选择最利于功能推进或最利于同步的形态；
- 上游若引入官方模式事件，仅替换 `DataSource`，UI/交互与宿主插桩保持不变；
- 若出现功能与最小差异冲突：优先实现功能，同时把新增行为封装为 Addon 层局部 churn，避免把差异扩散到 Host 热点文件。
