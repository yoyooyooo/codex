use crate::addons::ModeUiContext;
use crate::addons::UiViewFactory;
use crate::addons::UiViewKind;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::history_cell;
use crate::modes::ModeBarView;
use crate::modes::ModePanelView;

pub struct ModesUiDefaultFactory;

impl ModesUiDefaultFactory {
    pub fn new() -> Self {
        Self
    }

    fn send_error(tx: &AppEventSender, msg: String) {
        let cell = history_cell::new_error_event(msg);
        tx.send(AppEvent::InsertHistoryCell(Box::new(cell)));
    }
}

impl UiViewFactory for ModesUiDefaultFactory {
    fn make_view(
        &self,
        kind: UiViewKind,
        ctx: &ModeUiContext,
    ) -> Option<Box<dyn crate::bottom_pane::BottomPaneView>> {
        use codex_modes::scan_modes;
        let defs = match scan_modes(&ctx.cwd, Some(&ctx.codex_home)) {
            Ok(d) => d,
            Err(e) => {
                Self::send_error(&ctx.app_event_tx, format!("Failed to scan modes: {e}"));
                return None;
            }
        };
        if defs.is_empty() {
            Self::send_error(
                &ctx.app_event_tx,
                "No modes found under .codex/modes or $CODEX_HOME/modes".to_string(),
            );
            return None;
        }
        match kind {
            UiViewKind::ModePanel => {
                let panel = ModePanelView::new(
                    "Modes".to_string(),
                    defs,
                    ctx.persistent_mode_state.clone(),
                    ctx.base_user_instructions.clone(),
                    ctx.current_user_instructions.clone(),
                    ctx.app_event_tx.clone(),
                    ctx.on_update_summary.clone(),
                    ctx.on_update_persistent_state.clone(),
                );
                Some(Box::new(panel))
            }
            UiViewKind::ModeBar => {
                let bar = ModeBarView::new(
                    defs,
                    ctx.persistent_mode_state.clone(),
                    ctx.base_user_instructions.clone(),
                    ctx.current_user_instructions.clone(),
                    ctx.app_event_tx.clone(),
                    ctx.on_update_summary.clone(),
                    ctx.on_update_persistent_state.clone(),
                );
                Some(Box::new(bar))
            }
        }
    }
}
