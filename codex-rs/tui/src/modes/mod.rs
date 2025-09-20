mod factory;
mod mode_bar;
mod mode_panel;
mod state;

pub(crate) use factory::ModesUiDefaultFactory;
pub(crate) use mode_bar::ModeBarView;
pub(crate) use mode_panel::ModePanelView;
pub(crate) use state::PersistentModeState;

// 等价规范化已迁移到 codex-modes::normalize_equiv。
