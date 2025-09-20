# TUI 扩展接入指南（UiViewFactory / BottomPaneAddon / 生命周期钩子）

适用范围
- 本指南面向 TUI 内部扩展与上游对接，说明如何通过最小插桩接入自定义 UI 视图（如 ModeBar/ModePanel 及其它视图）。
- 目前扩展接口为 crate 内部使用（`pub(crate)`），默认不对第三方 crate 暴露；如需公开插件化接口，可在上游收敛后对外发布适配层。

核心扩展点
- `UiViewFactory`：通用视图工厂，按 `UiViewKind`（`ModeBar`/`ModePanel`）构建底部视图。
- `ModeUiFactory`：面向“模式”场景的兼容工厂；通过 `ModeUiFactoryAdapter` 适配为 `UiViewFactory`。
- `BottomPaneAddon`：在 `BottomPane` 中追加高度/渲染/按键优先处理的最小插桩位。
- `AppLifecycleHook`：会话配置完成（`on_session_configured`）等生命周期节点的回调。

上下文对象（构建视图时注入）
- `ModeUiContext`：提供 `app_event_tx`、`frame_requester`（预留）、`cwd/codex_home`、`base/current user_instructions`、`PersistentModeState`，以及两个回投 UI 的闭包：
  - `on_update_summary: Fn(String)`：更新底部模式摘要文案（已统一格式化为 `Mode: ...`）。
  - `on_update_persistent_state: Fn(PersistentModeState)`：更新持久启用状态（用于 reopening）。

最简示例：自定义 `UiViewFactory`
```rust
use codex_tui::addons::{UiViewFactory, UiViewKind, ModeUiContext};
use codex_tui::bottom_pane::BottomPaneView;

struct MyUiFactory;

impl UiViewFactory for MyUiFactory {
    fn make_view(&self, kind: UiViewKind, ctx: &ModeUiContext) -> Option<Box<dyn BottomPaneView>> {
        match kind {
            UiViewKind::ModeBar => {
                // 根据 ctx 构建一个实现了 BottomPaneView 的视图
                Some(Box::new(MyModeBar::new(ctx.base_user_instructions.clone())))
            }
            UiViewKind::ModePanel => None,
        }
    }
}

// 在 ChatWidget 初始化后（crate 内部）注册：
// chat_widget.register_ui_view_factory(Box::new(MyUiFactory));
```

兼容示例：沿用 `ModeUiFactory`（内部经适配器统一到 `UiViewFactory` 队列；不再单独维护 `mode_ui_factories` 列表）
```rust
use codex_tui::addons::{ModeUiFactory, ModeUiContext};
use codex_tui::bottom_pane::BottomPaneView;

struct MyModeUi;

impl ModeUiFactory for MyModeUi {
    fn make_mode_bar(&self, ctx: &ModeUiContext) -> Option<Box<dyn BottomPaneView>> {
        Some(Box::new(MyModeBar::new(ctx.base_user_instructions.clone())))
    }
}

// 通过适配器注入通用工厂队列：
// chat_widget.register_mode_ui_factory(Box::new(MyModeUi));
```

BottomPane 插桩：`BottomPaneAddon`
```rust
use codex_tui::addons::BottomPaneAddon;
use ratatui::{buffer::Buffer, layout::Rect};
use crossterm::event::KeyEvent;
use codex_tui::bottom_pane::InputResult;

struct MyAddon;

impl BottomPaneAddon for MyAddon {
    fn desired_height_additional(&self, _width: u16) -> u16 { 1 }
    fn render_after(&self, area: Rect, buf: &mut Buffer) {
        // 在宿主渲染完成后追加一行提示
        let _ = (area, buf);
    }
    fn handle_key_event(&mut self, _key: KeyEvent) -> Option<InputResult> { None }
}

// 在 BottomPane 构造后（crate 内部）挂载：
// bottom_pane.register_addon(Box::new(MyAddon));
```

注意事项
- 扩展默认“不改变宿主行为”：未注册时 Host 完全等价；注册后也应遵循同样的键位/快照输出约束。
- UI 更新需在主线程进行：通过 `on_update_*` 闭包（内部使用 UI 任务队列）回投到主线程，避免跨线程直接操作 TUI 状态。
- 风格与换行：遵循 `tui/styles.md` 与 `tui/src/wrapping.rs` 约定（`Stylize` 链式、`word_wrap_lines` 等）。
- 错误处理：将扫描/渲染/校验错误映射为历史区的 info/error cell；库层错误码参考 `errors.md`。

与上游的兼容策略
- 宿主仅保留极少量插点（高度/渲染/键盘、生命周期回调、工厂注册），其它代码全部在扩展或 `codex-modes` 内；
- 当上游 churn 时，优先在扩展与工厂中适配；Host 改动保持最小化以降低冲突成本。
