use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::BottomPaneView;
use crate::bottom_pane::InputResult;
use crate::modes::PersistentModeState;
use crate::tui::FrameRequester;

/// 在 BottomPane 中挂载的最小扩展接口。
///
/// 目标：
/// - 不改变宿主（Host）默认行为，无扩展时完全无感；
/// - 收敛差异到少量稳定钩子：高度计算、渲染及按键处理；
/// - 为后续把 ModeBar/Panel/摘要等 UI 下沉到扩展提供插桩位。
pub trait BottomPaneAddon {
    /// 额外的高度需求（在宿主基础上追加）。
    fn desired_height_additional(&self, _width: u16) -> u16 {
        0
    }

    /// 在宿主渲染完成后追加渲染（覆盖或叠加由扩展自行决定）。
    fn render_after(&self, _area: Rect, _buf: &mut Buffer) {}

    /// 让扩展优先处理按键；返回 Some 表示已处理并给出结果，None 表示放行宿主。
    fn handle_key_event(&mut self, _key_event: KeyEvent) -> Option<InputResult> {
        None
    }
}

/// 生命周期钩子：用于在会话配置完成等关键时机进行扩展初始化。
pub trait AppLifecycleHook {
    fn on_session_configured(&mut self) {}
}

/// 为 Mode UI 提供的工厂接口：用于按需创建面板/底栏视图。
/// 默认返回 None，宿主将回退到现有内联实现，确保行为不变。
pub trait ModeUiFactory {
    fn make_mode_panel(&self, _ctx: &ModeUiContext) -> Option<Box<dyn BottomPaneView>> {
        None
    }
    fn make_mode_bar(&self, _ctx: &ModeUiContext) -> Option<Box<dyn BottomPaneView>> {
        None
    }
}

/// 将构建视图可能需要的上下文集中传入，避免与宿主产生强耦合。
pub struct ModeUiContext {
    pub app_event_tx: AppEventSender,
    // 预留：供需要主动触发重绘的视图使用（当前默认路径未直接使用）。
    #[allow(dead_code)]
    pub frame_requester: FrameRequester,
    pub cwd: std::path::PathBuf,
    pub codex_home: std::path::PathBuf,
    pub base_user_instructions: String,
    pub current_user_instructions: Option<String>,
    pub persistent_mode_state: PersistentModeState,
    pub on_update_summary: std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>,
    pub on_update_persistent_state:
        std::sync::Arc<dyn Fn(PersistentModeState) + Send + Sync + 'static>,
}

impl ModeUiContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        app_event_tx: AppEventSender,
        frame_requester: FrameRequester,
        cwd: std::path::PathBuf,
        codex_home: std::path::PathBuf,
        base_user_instructions: String,
        current_user_instructions: Option<String>,
        persistent_mode_state: PersistentModeState,
        on_update_summary: std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>,
        on_update_persistent_state: std::sync::Arc<
            dyn Fn(PersistentModeState) + Send + Sync + 'static,
        >,
    ) -> Self {
        Self {
            app_event_tx,
            frame_requester,
            cwd,
            codex_home,
            base_user_instructions,
            current_user_instructions,
            persistent_mode_state,
            on_update_summary,
            on_update_persistent_state,
        }
    }
}

/// 通用视图工厂：统一非模式与模式类视图的构建入口。
pub enum UiViewKind {
    ModePanel,
    ModeBar,
}

pub trait UiViewFactory {
    fn make_view(&self, kind: UiViewKind, ctx: &ModeUiContext) -> Option<Box<dyn BottomPaneView>>;
}

/// 适配器：用已有的 `ModeUiFactory` 实现通用的 `UiViewFactory`。
/// 说明：当前默认路径通过 `ChatWidget::register_mode_ui_factory` 才会使用该适配器，
/// 未注册时不会生效；保留以支持上游/第三方按“旧接口（ModeUiFactory）”接入。
#[allow(dead_code)]
pub struct ModeUiFactoryAdapter {
    inner: Box<dyn ModeUiFactory>,
}

impl ModeUiFactoryAdapter {
    pub fn new(inner: Box<dyn ModeUiFactory>) -> Self {
        Self { inner }
    }
}

impl UiViewFactory for ModeUiFactoryAdapter {
    fn make_view(&self, kind: UiViewKind, ctx: &ModeUiContext) -> Option<Box<dyn BottomPaneView>> {
        match kind {
            UiViewKind::ModePanel => self.inner.make_mode_panel(ctx),
            UiViewKind::ModeBar => self.inner.make_mode_bar(ctx),
        }
    }
}
