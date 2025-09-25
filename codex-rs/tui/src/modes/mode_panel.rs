use codex_modes::IndexMap;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;

use super::PersistentModeState;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::BottomPaneView;
use crate::bottom_pane::CancellationEvent;
use crate::history_cell;

/// Minimal模式面板：
/// - 上下移动选择模式
/// - 空格切换启用状态
/// - Enter 应用（覆写 <user_instructions>）
/// - Esc 取消
pub(crate) struct ModePanelView {
    title: String,
    defs: Vec<codex_modes::ModeDefinition>,
    /// 启用集合（以 id 标识）
    enabled: codex_modes::IndexSet<String>,
    /// 启用顺序（用于渲染按启用时间升序）
    enable_order: Vec<String>,
    /// 变量值缓存（沿用自初始状态；面板不编辑变量，仅透传）
    var_values: std::collections::HashMap<String, IndexMap<String, Option<String>>>,
    /// 当前高亮索引
    selected: usize,
    /// 发送事件
    app_event_tx: AppEventSender,
    /// 会话基线 <user_instructions>（不含 <mode_instructions>）
    base_user_instructions: String,
    /// 上次成功发送的完整 <user_instructions> 用于等价检测
    last_sent: Option<String>,
    /// 完成标志
    complete: bool,
    /// 直接更新摘要行（取代 AppEvent::UpdateModeSummary）
    on_update_summary: std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>,
    /// 直接更新持久状态（取代 AppEvent::UpdatePersistentModeState）
    on_update_persistent_state:
        std::sync::Arc<dyn Fn(super::PersistentModeState) + Send + Sync + 'static>,
}

impl ModePanelView {
    pub(crate) fn new(
        title: String,
        defs: Vec<codex_modes::ModeDefinition>,
        initial_state: PersistentModeState,
        base_user_instructions: String,
        initial_current_user_instructions: Option<String>,
        app_event_tx: AppEventSender,
        on_update_summary: std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>,
        on_update_persistent_state: std::sync::Arc<
            dyn Fn(PersistentModeState) + Send + Sync + 'static,
        >,
    ) -> Self {
        let selected = 0;
        let PersistentModeState {
            enabled,
            enable_order,
            var_values,
        } = initial_state.sanitize(&defs);
        Self {
            title,
            defs,
            enabled,
            enable_order,
            var_values,
            selected,
            app_event_tx,
            base_user_instructions,
            last_sent: initial_current_user_instructions,
            complete: false,
            on_update_summary,
            on_update_persistent_state,
        }
    }

    fn toggle_selected(&mut self) {
        if self.defs.is_empty() {
            return;
        }
        let idx = self.selected.min(self.defs.len() - 1);
        let id = self.defs[idx].id.clone();
        if self.enabled.contains(&id) {
            self.enabled.swap_remove(&id);
            self.enable_order.retain(|x| x != &id);
        } else {
            self.enabled.insert(id);
            self.enable_order.push(self.defs[idx].id.clone());
        }
    }

    fn move_up(&mut self) {
        if self.defs.is_empty() {
            return;
        }
        self.selected = if self.selected == 0 {
            self.defs.len() - 1
        } else {
            self.selected - 1
        };
    }

    fn move_down(&mut self) {
        if self.defs.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.defs.len();
    }

    fn render_and_maybe_send(&mut self) {
        use codex_modes::EnabledMode;
        use codex_modes::ModeScope;
        use codex_modes::is_equivalent;
        use codex_modes::render_user_instructions;
        // 构建 enabled 列表（保持 defs 顺序）
        let mut enabled_list: Vec<EnabledMode> = Vec::new();
        for id in &self.enable_order {
            if let Some(def) = self.defs.iter().find(|d| &d.id == id) {
                let mut vars: IndexMap<&str, Option<String>> = IndexMap::new();
                let vv = self.var_values.get(&def.id);
                for v in &def.variables {
                    let val = vv.and_then(|m| m.get(&v.name).cloned()).flatten();
                    vars.insert(v.name.as_str(), val);
                }
                enabled_list.push(EnabledMode {
                    id: &def.id,
                    display_name: def.display_name.as_deref(),
                    scope: match &def.scope {
                        s @ ModeScope::Global => s,
                        s @ ModeScope::Project(_) => s,
                    },
                    variables: vars,
                });
            }
        }
        // 库层校验：必填缺失、基础类型错误等
        let v_errs = codex_modes::validate_enabled(&self.defs, &enabled_list);
        if !v_errs.is_empty() {
            // 仅将 RequiredMissing 聚合为 E3101，保持面板行为（其余值错误主要出现在编辑态）
            let missing: Vec<String> = v_errs
                .iter()
                .filter_map(|e| match e {
                    codex_modes::ValidationError::RequiredMissing { mode_id, var } => {
                        Some(format!("{mode_id}/{var}"))
                    }
                    _ => None,
                })
                .collect();
            if !missing.is_empty() {
                let msg = format!("E3101 RequiredMissing: {}", missing.join(", "));
                let cell = history_cell::new_error_event(msg);
                self.app_event_tx
                    .send(AppEvent::InsertHistoryCell(Box::new(cell)));
                return;
            }
        }
        let rendered =
            match render_user_instructions(&self.base_user_instructions, &enabled_list, &self.defs)
            {
                Ok(s) => s,
                Err(e) => {
                    let msg = codex_modes::format_modes_error(&e);
                    let cell = history_cell::new_error_event(msg);
                    self.app_event_tx
                        .send(AppEvent::InsertHistoryCell(Box::new(cell)));
                    return;
                }
            };
        (self.on_update_persistent_state)(PersistentModeState {
            enabled: self.enabled.clone(),
            enable_order: self.enable_order.clone(),
            var_values: self.var_values.clone(),
        });
        if self
            .last_sent
            .as_ref()
            .map(|s| is_equivalent(s, &rendered))
            .unwrap_or(false)
        {
            return;
        }

        self.app_event_tx.send(AppEvent::CodexOp(
            codex_core::protocol::Op::OverrideTurnContext {
                cwd: None,
                approval_policy: None,
                sandbox_policy: None,
                model: None,
                effort: None,
                summary: None,
                user_instructions: Some(rendered.clone()),
            },
        ));

        // 同步一条简短的反馈信息到历史（库层统一文案）
        let labels_comma = enabled_list
            .iter()
            .map(|e| e.id.trim_start_matches('/'))
            .collect::<Vec<_>>()
            .join(", ");
        let info = history_cell::new_info_event(
            codex_modes::applied_message(enabled_list.len()),
            if labels_comma.is_empty() {
                None
            } else {
                Some(labels_comma)
            },
        );
        self.app_event_tx
            .send(AppEvent::InsertHistoryCell(Box::new(info)));

        self.last_sent = Some(rendered);
        // 更新模式摘要栏（直接回调，库层统一摘要）
        let labels_dotted = codex_modes::enabled_labels(&enabled_list);
        (self.on_update_summary)(labels_dotted);
    }
}

impl BottomPaneView for ModePanelView {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Up, ..
            } => self.move_up(),
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => self.move_down(),
            KeyEvent {
                code: KeyCode::Char(' '),
                ..
            } => {
                self.toggle_selected();
                // 最小去抖：立即尝试发送（等价则跳过）
                self.render_and_maybe_send();
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                // 应用并关闭
                self.render_and_maybe_send();
                self.complete = true;
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                // 取消
                self.complete = true;
            }
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        self.complete = true;
        CancellationEvent::Handled
    }
}

impl crate::render::renderable::Renderable for ModePanelView {
    fn desired_height(&self, _width: u16) -> u16 {
        // 标题1行 + 空行1 + 列表(<=8) + 底部提示1
        let rows = self.defs.len().min(8) as u16;
        1 + 1 + rows + 1
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let title_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        };
        let title = Paragraph::new(Line::from(vec!["▌ ".dim(), self.title.clone().bold()]));
        title.render(title_area, buf);

        let mut y = area.y.saturating_add(1);
        // 空行
        Paragraph::new(Line::from("")).render(
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
            buf,
        );
        y = y.saturating_add(1);

        let list_height = area.height.saturating_sub(3); // 标题+空行+底部提示
        let visible = self.defs.len().min(list_height as usize);
        let start = self
            .selected
            .saturating_sub(self.selected.min(visible.saturating_sub(1)));
        let end = (start + visible).min(self.defs.len());

        for (i, def) in self.defs[start..end].iter().enumerate() {
            let idx = start + i;
            let selected = idx == self.selected;
            let marker = if selected { '>' } else { ' ' };
            let checked = if self.enabled.contains(&def.id) {
                "[x]"
            } else {
                "[ ]"
            };
            let name = def
                .display_name
                .as_deref()
                .unwrap_or_else(|| def.id.trim_start_matches('/'))
                .to_string();
            let line = format!("{marker} {checked} {name}");
            let para = Paragraph::new(Line::from(line));
            para.render(
                Rect {
                    x: area.x,
                    y,
                    width: area.width,
                    height: 1,
                },
                buf,
            );
            y = y.saturating_add(1);
            if y >= area.y + area.height.saturating_sub(1) {
                break;
            }
        }

        // 底部提示（中文对齐底栏提示风格）
        let hint = Paragraph::new("Space 开关  ⏎ 应用  Esc 关闭".to_string().dim());
        hint.render(
            Rect {
                x: area.x,
                y: area.y + area.height - 1,
                width: area.width,
                height: 1,
            },
            buf,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_event::AppEvent;
    use crate::app_event_sender::AppEventSender;
    use crate::render::renderable::Renderable;
    use insta::assert_snapshot;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use std::path::PathBuf;
    use tokio::sync::mpsc::unbounded_channel;

    fn mk_defs() -> Vec<codex_modes::ModeDefinition> {
        vec![
            codex_modes::ModeDefinition {
                id: "/review".into(),
                display_name: Some("review".into()),
                description: None,
                argument_hint: None,
                kind: codex_modes::ModeKind::Persistent,
                default_enabled: true,
                variables: vec![],
                scope: codex_modes::ModeScope::Project("demo".into()),
                path: PathBuf::new(),
                body: "Review".into(),
            },
            codex_modes::ModeDefinition {
                id: "/qa".into(),
                display_name: Some("qa".into()),
                description: None,
                argument_hint: None,
                kind: codex_modes::ModeKind::Persistent,
                default_enabled: false,
                variables: vec![codex_modes::ModeVariableDefinition {
                    name: "level".into(),
                    var_type: None,
                    required: true,
                    default: None,
                    r#enum: None,
                    shortcuts: None,
                    pattern: None,
                    inline_edit: None,
                    mode_scoped: None,
                }],
                scope: codex_modes::ModeScope::Project("demo".into()),
                path: PathBuf::new(),
                body: "QA".into(),
            },
        ]
    }

    fn render_lines(view: &ModePanelView, width: u16, height: u16) -> String {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);
        let lines: Vec<String> = (0..area.height)
            .map(|row| {
                let mut line = String::new();
                for col in 0..area.width {
                    let symbol = buf[(area.x + col, area.y + row)].symbol();
                    if symbol.is_empty() {
                        line.push(' ');
                    } else {
                        line.push_str(symbol);
                    }
                }
                line
            })
            .collect();
        lines.join("\n")
    }

    #[test]
    fn modepanel_list_basic() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let defs = mk_defs();
        let mut state = PersistentModeState::default();
        if state.enabled.insert("/review".to_string()) {
            state.enable_order.push("/review".to_string());
        }
        let panel = ModePanelView::new(
            "Modes".to_string(),
            defs,
            state,
            "base".to_string(),
            None,
            tx,
            std::sync::Arc::new(|_s: String| {}),
            std::sync::Arc::new(|_st: PersistentModeState| {}),
        );
        // Height: title1 + blank1 + list(2) + hint1 = 5
        assert_snapshot!("modepanel_list_basic", render_lines(&panel, 40, 5));
    }

    fn drain_history_strings(
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
    ) -> Vec<String> {
        let mut out = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            if let AppEvent::InsertHistoryCell(cell) = ev {
                let lines = cell.display_lines(80);
                let mut s = String::new();
                for l in lines {
                    for sp in l.spans {
                        s.push_str(sp.content.as_ref());
                    }
                    s.push('\n');
                }
                out.push(s);
            }
        }
        out
    }

    fn drain_override_user_instructions(
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
    ) -> Vec<String> {
        let mut out = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            if let AppEvent::CodexOp(codex_core::protocol::Op::OverrideTurnContext {
                user_instructions,
                ..
            }) = ev
                && let Some(s) = user_instructions
            {
                out.push(s);
            }
        }
        out
    }

    #[test]
    fn modepanel_emits_e3101_required_missing() {
        let (tx_raw, mut rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let defs = mk_defs();
        // Enable /qa which has a required var without default
        let mut state = PersistentModeState::default();
        if state.enabled.insert("/qa".to_string()) {
            state.enable_order.push("/qa".to_string());
        }
        let mut panel = ModePanelView::new(
            "Modes".to_string(),
            defs,
            state,
            "base".to_string(),
            None,
            tx,
            std::sync::Arc::new(|_s: String| {}),
            std::sync::Arc::new(|_st: PersistentModeState| {}),
        );
        // 按 Enter 尝试应用，应触发 E3101 而不发送 override
        panel.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let msgs = drain_history_strings(&mut rx);
        let last = msgs.last().cloned().unwrap_or_default();
        assert_snapshot!("modepanel_error_required_missing", last);
    }

    #[test]
    fn modepanel_uses_var_values_and_renders() {
        let (tx_raw, mut rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let defs = mk_defs();
        // Enable /qa 并提供必填变量 level 的显式值
        let mut state = PersistentModeState::default();
        if state.enabled.insert("/qa".to_string()) {
            state.enable_order.push("/qa".to_string());
        }
        use codex_modes::IndexMap as IMap;
        let mut vars: IMap<String, Option<String>> = IMap::new();
        vars.insert("level".to_string(), Some("High".to_string()));
        state.var_values.insert("/qa".to_string(), vars);

        let mut panel = ModePanelView::new(
            "Modes".to_string(),
            defs,
            state,
            "base".to_string(),
            None,
            tx,
            std::sync::Arc::new(|_s: String| {}),
            std::sync::Arc::new(|_st: PersistentModeState| {}),
        );
        // 应用：不应触发 E3101，且渲染包含 level=High
        panel.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // 单次遍历收集错误与覆写
        let mut errs_acc: Vec<String> = Vec::new();
        let mut overrides: Vec<String> = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            match ev {
                AppEvent::InsertHistoryCell(cell) => {
                    let mut s = String::new();
                    for l in cell.display_lines(80) {
                        for sp in l.spans {
                            s.push_str(sp.content.as_ref());
                        }
                        s.push('\n');
                    }
                    errs_acc.push(s);
                }
                AppEvent::CodexOp(codex_core::protocol::Op::OverrideTurnContext {
                    user_instructions,
                    ..
                }) => {
                    if let Some(s) = user_instructions {
                        overrides.push(s);
                    }
                }
                _ => {}
            }
        }
        let has_e3101 = errs_acc.iter().any(|s| s.contains("E3101"));
        assert!(
            !has_e3101,
            "unexpected E3101 after providing var value: {errs_acc:?}"
        );
        let ok = overrides.iter().any(|s| s.contains("level=High"));
        assert!(
            ok,
            "expected rendered user_instructions to contain level=High; got: {overrides:?}"
        );
    }
}
