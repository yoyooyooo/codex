# TUI Esc 双击清空与历史回退

## 背景与约束
- 目标：解决“误触 Esc 清空输入”“快速回退历史”痛点，同时最小化对上游状态机的侵入。
- 改动集中在 `codex-rs/tui/src/app.rs`、`app_backtrack.rs`、`bottom_pane/chat_composer.rs`，所有定制逻辑均以 `// !Modify:` 标记。
- 任何时候可通过移除这些标记块恢复上游 Esc 行为，不影响其他按键映射。

## 行为矩阵概览
- **输入框非空**：
  - `ChatComposer::handle_key_event`（`chat_composer.rs:360+`）在 `ActivePopup::None` 分支检测 Esc，调用 `backtrack_state.arm_clear_hint(...)`。
  - 1 秒内再次 Esc ⇒ `clear_text()` 并清除 hint；超时或其他按键 ⇒ 自动取消。
- **输入框为空**：
  - `app.rs:455` 的 `// !Modify: Esc normal mode` 钩子将首次 Esc 标记为 primed。
  - 第二次 Esc 调用 `App::open_user_nodes_picker()`，切换到历史用户消息列表；若 transcript overlay 已经打开，则改由 `begin_overlay_backtrack_preview()` 启动预览。
- Transcript overlay (`Ctrl+T`) 路径：`app_backtrack.rs` 中的 `handle_overlay_event` 将 Esc 映射为“向更早的用户消息移动”，保持与上游步进逻辑一致。

## 状态结构
- `ChatBacktrackState`（`app_backtrack.rs:20+`）新增字段：
  - `clear_hint_deadline: Option<Instant>` 控制双击窗口，`should_render_clear_hint()` 在 footer 渲染阶段读取。
  - `primed: bool` 表示是否捕获到“第一次 Esc”；任何非 Esc 的按键都会调用 `reset_prime()`。
- 组件交互：
  - `ChatComposer` 持有 `clear_hint_deadline`，在 `render` 时追加提示行。
  - `AppBacktrack` 负责在 overlay 状态下重设 `clear_hint_deadline`，防止提示残留（`// !Modify: Reset Esc clear hint`）。

## UI 与提示
- 底栏提示位于 `chat_composer.rs:1330+`：
  - `Esc clear` 标签常驻但为淡色；当 `clear_hint_deadline` 存在时追加第二行 `Please Escape again to clear`。
  - `Line` 构造遵循 ratatui Stylize 约定，避免 snapshot 抖动。
- 回退 overlay footer (`chatwidget.rs:2220+`) 更新为 `↑/↓ 选择 · Enter 回退并编辑 · Esc 取消`，与 ModeBar 提示对齐。

## 守卫逻辑
- 仅在 `AppMode::Normal` 且无 modal/popup 打开时拦截 Esc；其他场景继续透传给上游。
- 正在运行任务时（`bottom_pane/mod.rs:210+`）如果状态条可见，Esc 仍优先“中断任务”，不会触发回退。
- ModeBar 打开时直接短路 Esc，避免与回退状态机互相抢占（`mode_bar.rs` 内部处理）。

## 测试与快照
- Snapshots：`tui/src/chatwidget/snapshots/*backtrack*` 覆盖提示行与 overlay 列表。
- 单元测试：
  - `chatwidget/tests.rs` 中新增 case 断言双 Esc 清空与 overlay 回退流程。
  - Deadline 逻辑通过时间注入器（`Instant::now` 包装）进行模拟，确保 1 秒窗口可测。
- 更新流程：修改相关 UI 后运行 `cargo test -p codex-tui` 并使用 `cargo insta accept -p codex-tui` 复核快照。

## 同步指引
- 每次上游调整 Esc 处理链时，优先查看 `app.rs` 的输入分发和 overlay 结构是否变动；移动 `// !Modify:` 区块即可。
- 若 overlay 列表实现被重写，确保 `AppBacktrack::enter_overlay` 仍在入口处重置 `clear_hint_deadline`，否则提示会残留。
- 与 Mode 系统共存：当后续改动 Esc 行为时务必验证 ModeBar/ModePanel 热键未被破坏。
