use codex_modes::IndexMap;
use codex_modes::IndexSet;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use std::collections::HashMap;

use super::PersistentModeState;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::BottomPaneView;
use crate::bottom_pane::CancellationEvent;
use crate::history_cell;
use codex_modes::YamlValue as Value;
use tokio::time::Duration as TokioDuration;
use tokio::time::sleep;

pub(crate) struct ModeBarView {
    title: String,
    defs: Vec<codex_modes::ModeDefinition>,
    enabled: IndexSet<String>,
    enable_order: Vec<String>,
    selected_mode_idx: usize,
    selected_var_idx_by_mode: HashMap<String, usize>,
    var_values: HashMap<String, IndexMap<String, Option<String>>>,
    // Inline edit state
    editing: bool,
    editing_mode_id: Option<String>,
    editing_var_name: Option<String>,
    editing_buffer: String,
    app_event_tx: AppEventSender,
    base_user_instructions: String,
    last_sent: Option<String>,
    complete: bool,
    // debounce generation; only the latest scheduled send will proceed
    debouncer: codex_modes::DebounceGen,
    // expand details view under summary
    expanded_details: bool,
    /// 直接更新摘要行（取代 AppEvent::UpdateModeSummary）
    on_update_summary: std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>,
    /// 直接更新持久状态（取代 AppEvent::UpdatePersistentModeState）
    on_update_persistent_state:
        std::sync::Arc<dyn Fn(super::PersistentModeState) + Send + Sync + 'static>,
}

impl ModeBarView {
    const DEBOUNCE_DELAY_MS: u64 = 200;
    pub(crate) fn new(
        defs: Vec<codex_modes::ModeDefinition>,
        initial_state: PersistentModeState,
        base_user_instructions: String,
        initial_rendered: Option<String>,
        app_event_tx: AppEventSender,
        on_update_summary: std::sync::Arc<dyn Fn(String) + Send + Sync + 'static>,
        on_update_persistent_state: std::sync::Arc<
            dyn Fn(PersistentModeState) + Send + Sync + 'static,
        >,
    ) -> Self {
        let PersistentModeState {
            enabled,
            enable_order,
            var_values,
        } = initial_state.sanitize(&defs);

        // 初始 last_sent 优先使用当前会话已知的完整渲染；缺失时退回仅基线渲染。
        let empty: Vec<codex_modes::EnabledMode> = Vec::new();
        let initial_last = initial_rendered.or_else(|| {
            codex_modes::render_user_instructions(&base_user_instructions, &empty, &defs).ok()
        });

        Self {
            title: "Mode".to_string(),
            defs,
            enabled,
            enable_order,
            selected_mode_idx: 0,
            selected_var_idx_by_mode: HashMap::new(),
            var_values,
            editing: false,
            editing_mode_id: None,
            editing_var_name: None,
            editing_buffer: String::new(),
            app_event_tx,
            base_user_instructions,
            last_sent: initial_last,
            complete: false,
            debouncer: codex_modes::DebounceGen::new(),
            expanded_details: false,
            on_update_summary,
            on_update_persistent_state,
        }
    }

    fn prev_mode(&mut self) {
        if self.defs.is_empty() {
            return;
        }
        self.selected_mode_idx = if self.selected_mode_idx == 0 {
            self.defs.len() - 1
        } else {
            self.selected_mode_idx - 1
        };
    }

    fn next_mode(&mut self) {
        if self.defs.is_empty() {
            return;
        }
        self.selected_mode_idx = (self.selected_mode_idx + 1) % self.defs.len();
    }

    fn prev_var(&mut self) {
        if let Some(def) = self.defs.get(self.selected_mode_idx) {
            let len = def.variables.len();
            if len == 0 {
                return;
            }
            let id = def.id.clone();
            let entry = self.selected_var_idx_by_mode.entry(id).or_insert(0);
            *entry = if *entry == 0 { len - 1 } else { *entry - 1 };
        }
    }

    fn next_var(&mut self) {
        if let Some(def) = self.defs.get(self.selected_mode_idx) {
            let len = def.variables.len();
            if len == 0 {
                return;
            }
            let id = def.id.clone();
            let entry = self.selected_var_idx_by_mode.entry(id).or_insert(0);
            *entry = (*entry + 1) % len;
        }
    }

    fn toggle_selected(&mut self) {
        if self.defs.is_empty() {
            return;
        }
        let idx = self.selected_mode_idx.min(self.defs.len() - 1);
        let id = self.defs[idx].id.clone();
        if self.enabled.contains(&id) {
            self.enabled.swap_remove(&id);
            self.enable_order.retain(|x| x != &id);
        } else {
            self.enabled.insert(id.clone());
            self.enable_order.push(id);
        }
    }

    fn render_and_maybe_send(&mut self) {
        use codex_modes::EnabledMode;
        use codex_modes::ModeScope;
        // 构建按启用顺序的 enabled 列表
        let mut enabled_list: Vec<EnabledMode> = Vec::new();
        let mut missing_required: Vec<String> = Vec::new();
        for id in &self.enable_order {
            if let Some(def) = self.defs.iter().find(|d| &d.id == id) {
                let mut vars: IndexMap<&str, Option<String>> = IndexMap::new();
                let vv = self.var_values.entry(def.id.clone()).or_default();
                for v in &def.variables {
                    let key = v.name.as_str();
                    let val = vv.get(key).cloned().unwrap_or(None);
                    vars.insert(key, val);
                }
                // 必填缺失拦截：显式为空且无默认
                for v in &def.variables {
                    let key = v.name.as_str();
                    let explicit = vars.get(key).and_then(std::clone::Clone::clone);
                    let has_default = v.default.is_some();
                    if v.required && explicit.is_none() && !has_default {
                        missing_required.push(format!("{}/{}", def.id, v.name));
                    }
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
        if !missing_required.is_empty() {
            let msg = format!("E3101 RequiredMissing: {}", missing_required.join(", "));
            let cell = history_cell::new_error_event(msg);
            self.app_event_tx
                .send(AppEvent::InsertHistoryCell(Box::new(cell)));
            return;
        }
        let rendered = match codex_modes::render_user_instructions(
            &self.base_user_instructions,
            &enabled_list,
            &self.defs,
        ) {
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
            .map(|s| codex_modes::is_equivalent(s, &rendered))
            .unwrap_or(false)
        {
            return;
        }

        // Debounce: schedule sending the latest rendered content only.
        let r#gen = self.debouncer.next();
        let tx = self.app_event_tx.clone();
        let on_update_summary = self.on_update_summary.clone();
        let labels = enabled_list
            .iter()
            .map(|e| e.id.trim_start_matches('/'))
            .collect::<Vec<_>>()
            .join(", ");
        // 提前记录 last_sent，避免等价内容重复发送造成多条提示
        self.last_sent = Some(rendered.clone());
        let rendered_cloned = rendered;
        let debouncer = self.debouncer.clone();
        tokio::spawn(async move {
            sleep(TokioDuration::from_millis(Self::DEBOUNCE_DELAY_MS)).await;
            if !debouncer.is_latest(r#gen) {
                return; // superseded by a newer change
            }
            {
                tx.send(AppEvent::CodexOp(
                    codex_core::protocol::Op::OverrideTurnContext {
                        cwd: None,
                        approval_policy: None,
                        sandbox_policy: None,
                        model: None,
                        effort: None,
                        summary: None,
                        user_instructions: Some(rendered_cloned),
                    },
                ));
                let count = labels
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .count();
                let message = codex_modes::applied_message(count);
                let info = history_cell::new_info_event(
                    message,
                    if count == 0 {
                        None
                    } else {
                        Some(labels.clone())
                    },
                );
                tx.send(AppEvent::InsertHistoryCell(Box::new(info)));
                // 更新模式摘要栏内容
                let s = labels
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ");
                (on_update_summary)(s);
            }
        });
    }
}

impl BottomPaneView for ModeBarView {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        // 编辑态优先处理
        if self.editing {
            match key_event {
                KeyEvent {
                    code: KeyCode::Esc, ..
                } => {
                    self.editing = false;
                    self.editing_mode_id = None;
                    self.editing_var_name = None;
                    self.editing_buffer.clear();
                    return;
                }
                // 在枚举编辑态下，↑/↓ 循环选择
                KeyEvent {
                    code: KeyCode::Up, ..
                } => {
                    if let (Some(mode_id), Some(var_name)) = (
                        self.editing_mode_id.as_ref(),
                        self.editing_var_name.as_ref(),
                    ) && let Some(def) = self.defs.iter().find(|d| &d.id == mode_id)
                        && let Some(vdef) = def.variables.iter().find(|v| &v.name == var_name)
                        && let Some(options) = &vdef.r#enum
                        && !options.is_empty()
                    {
                        let mut idx = options
                            .iter()
                            .position(|o| o == &self.editing_buffer)
                            .unwrap_or(0);
                        idx = if idx == 0 { options.len() - 1 } else { idx - 1 };
                        self.editing_buffer = options[idx].clone();
                        return;
                    }
                }
                KeyEvent {
                    code: KeyCode::Down,
                    ..
                } => {
                    if let (Some(mode_id), Some(var_name)) = (
                        self.editing_mode_id.as_ref(),
                        self.editing_var_name.as_ref(),
                    ) && let Some(def) = self.defs.iter().find(|d| &d.id == mode_id)
                        && let Some(vdef) = def.variables.iter().find(|v| &v.name == var_name)
                        && let Some(options) = &vdef.r#enum
                        && !options.is_empty()
                    {
                        let mut idx = options
                            .iter()
                            .position(|o| o == &self.editing_buffer)
                            .unwrap_or(0);
                        idx = (idx + 1) % options.len();
                        self.editing_buffer = options[idx].clone();
                        return;
                    }
                }
                KeyEvent {
                    code: KeyCode::Enter,
                    modifiers: KeyModifiers::NONE,
                    ..
                } => {
                    if let (Some(mode_id), Some(var_name)) =
                        (self.editing_mode_id.clone(), self.editing_var_name.clone())
                    {
                        // 库层校验
                        if let Some(def) = self.defs.iter().find(|d| d.id == mode_id)
                            && let Some(vdef) = def.variables.iter().find(|v| v.name == var_name)
                        {
                            if let Some(err) =
                                codex_modes::validate_var_value(&def.id, vdef, &self.editing_buffer)
                                && let Some(msg) = codex_modes::format_validation_error(&err)
                            {
                                let cell = history_cell::new_error_event(msg);
                                self.app_event_tx
                                    .send(AppEvent::InsertHistoryCell(Box::new(cell)));
                                return;
                            }
                            let vv = self.var_values.entry(mode_id).or_default();
                            // 存储规范化：非枚举值 trim；布尔值统一小写 true/false
                            if vdef.r#enum.is_some() {
                                if self.editing_buffer.is_empty() {
                                    vv.insert(var_name, None);
                                } else {
                                    vv.insert(var_name, Some(self.editing_buffer.clone()));
                                }
                            } else {
                                let trimmed = self.editing_buffer.trim();
                                if trimmed.is_empty() {
                                    vv.insert(var_name, None);
                                } else if matches!(
                                    vdef.var_type,
                                    Some(codex_modes::VarType::Boolean)
                                ) {
                                    vv.insert(var_name, Some(trimmed.to_lowercase()));
                                } else {
                                    vv.insert(var_name, Some(trimmed.to_string()));
                                }
                            }
                        } else {
                            // 找不到 def/vdef 时回退到“是否空值”的旧逻辑
                            let vv = self.var_values.entry(mode_id).or_default();
                            if self.editing_buffer.is_empty() {
                                vv.insert(var_name, None);
                            } else {
                                vv.insert(var_name, Some(self.editing_buffer.clone()));
                            }
                        }
                    }
                    self.editing = false;
                    self.editing_mode_id = None;
                    self.editing_var_name = None;
                    self.editing_buffer.clear();
                    self.render_and_maybe_send();
                    return;
                }
                KeyEvent {
                    code: KeyCode::Backspace,
                    ..
                } => {
                    self.editing_buffer.pop();
                    return;
                }
                KeyEvent {
                    code: KeyCode::Char(c),
                    modifiers: KeyModifiers::NONE,
                    ..
                } => {
                    self.editing_buffer.push(c);
                    return;
                }
                _ => {}
            }
        }
        match key_event {
            // 非编辑态：按上返回输入框（退出 ModeBar）。编辑态由上面的分支捕获↑用于枚举选择。
            KeyEvent {
                code: KeyCode::Up, ..
            } => {
                self.complete = true;
            }
            // 模式切换：Tab/BackTab
            KeyEvent {
                code: KeyCode::Tab, ..
            } => self.next_mode(),
            KeyEvent {
                code: KeyCode::BackTab,
                ..
            } => self.prev_mode(),
            // 变量切换：Left/Right
            KeyEvent {
                code: KeyCode::Left,
                ..
            } => self.prev_var(),
            KeyEvent {
                code: KeyCode::Right,
                ..
            } => self.next_var(),
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.complete = true;
            }
            KeyEvent {
                code: KeyCode::Char(' '),
                ..
            } => {
                self.toggle_selected();
                self.render_and_maybe_send();
            }
            // 详情展开/收起
            KeyEvent {
                code: KeyCode::Char('d'),
                ..
            } => {
                self.expanded_details = !self.expanded_details;
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                // 进入就地编辑
                if let Some(def) = self.defs.get(self.selected_mode_idx) {
                    if def.variables.is_empty() {
                        return;
                    }
                    let var_idx = *self
                        .selected_var_idx_by_mode
                        .entry(def.id.clone())
                        .or_insert(0);
                    let var_def = &def.variables[var_idx];
                    let vv = self.var_values.entry(def.id.clone()).or_default();
                    let cur = vv.get(&var_def.name).cloned().flatten().unwrap_or_default();
                    self.editing = true;
                    self.editing_mode_id = Some(def.id.clone());
                    self.editing_var_name = Some(var_def.name.clone());
                    // 若为枚举，进入编辑时预填默认或第一项；否则保留当前值
                    if let Some(options) = &var_def.r#enum {
                        if !cur.is_empty() {
                            self.editing_buffer = cur;
                        } else if let Some(Value::String(s)) = &var_def.default {
                            self.editing_buffer = s.clone();
                        } else if let Some(first) = options.first() {
                            self.editing_buffer = first.clone();
                        } else {
                            self.editing_buffer.clear();
                        }
                    } else {
                        self.editing_buffer = cur;
                    }
                }
            }
            // 编辑态的 ↑/↓ 已在前置分支处理，这里无需重复
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

impl crate::render::renderable::Renderable for ModeBarView {
    fn desired_height(&self, width: u16) -> u16 {
        if !self.expanded_details {
            // 1 行顶部分隔 + 1 行摘要 + 1 行变量 + 1 行内部虚线 + 1 行提示
            return 5;
        }
        // 详情展开：
        // 1 行顶部分隔 + 1 行摘要 + scope/vars/body 的 wrap 行数 + 1 行分隔 + 1 行底部提示
        let mut total: u16 = 2; // top-sep + summary
        if let Some(def) = self.defs.get(self.selected_mode_idx) {
            use crate::wrapping::RtOptions;
            use crate::wrapping::word_wrap_line;
            let wrap_w = width.max(1) as usize;

            // scope 行
            let title = def
                .display_name
                .clone()
                .unwrap_or_else(|| def.id.trim_start_matches('/').to_string());
            let scope_label = match &def.scope {
                codex_modes::ModeScope::Global => "global".to_string(),
                codex_modes::ModeScope::Project(d) => format!("project:{d}"),
            };
            let scope_line = Line::from(format!("{title} — scope: {scope_label}"));
            let opts = RtOptions::new(wrap_w)
                .initial_indent("▌ ".into())
                .subsequent_indent("  ".into());
            total = total.saturating_add(word_wrap_line(&scope_line, opts).len() as u16);

            // vars 行
            let vv = self.var_values.get(&def.id);
            let mut kvs: Vec<String> = Vec::new();
            for v in &def.variables {
                if let Some(Some(val)) = vv.and_then(|m| m.get(&v.name)) {
                    kvs.push(format!("{}={}", v.name, val));
                } else if v.default.is_some() {
                    kvs.push(format!("{}=(default)", v.name));
                }
            }
            let vars_text = if kvs.is_empty() {
                "vars: (none)".to_string()
            } else {
                format!("vars: {}", kvs.join(", "))
            };
            let vars_line = Line::from(vars_text);
            let opts = RtOptions::new(wrap_w)
                .initial_indent("▌ ".into())
                .subsequent_indent("  ".into());
            total = total.saturating_add(word_wrap_line(&vars_line, opts).len() as u16);

            // body 行
            let body_line = Line::from(def.body.replace('\n', " "));
            let opts = RtOptions::new(wrap_w)
                .initial_indent("▌ ".into())
                .subsequent_indent("  ".into());
            total = total.saturating_add(word_wrap_line(&body_line, opts).len() as u16);
        }
        // 额外为分隔线与底部提示各预留一行
        total = total.saturating_add(2);
        total.max(2)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        // 顶部分隔线
        {
            use ratatui::text::Span as RtSpan;
            use ratatui::widgets::Paragraph;
            Paragraph::new(Line::from(vec![
                RtSpan::from("─".repeat(area.width as usize)).dim(),
            ]))
            .render(
                Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: 1,
                },
                buf,
            );
        }
        // 构造摘要行（spans）：激活统一颜色，非激活灰色
        use ratatui::text::Span;
        let mut spans: Vec<Span> = Vec::new();
        spans.push(Span::from(format!("{}: ", self.title)));
        for (i, def) in self.defs.iter().enumerate() {
            if i > 0 {
                spans.push(" · ".into());
            }
            let checked = if self.enabled.contains(&def.id) {
                "[x]"
            } else {
                "[ ]"
            };
            spans.push(Span::from(checked));
            spans.push(" ".into());
            let mut name = def
                .display_name
                .as_deref()
                .unwrap_or_else(|| def.id.trim_start_matches('/'))
                .to_string();
            let mut missing = false;
            if self.enabled.contains(&def.id) {
                let vv = self.var_values.get(&def.id);
                for v in &def.variables {
                    let explicit = vv.and_then(|m| m.get(&v.name).cloned()).flatten();
                    let has_default = v.default.is_some();
                    if v.required && explicit.is_none() && !has_default {
                        missing = true;
                        break;
                    }
                }
            }
            if missing {
                name.push_str(" ⚠");
            }
            let mut s = Span::from(name);
            if i == self.selected_mode_idx {
                s = s.cyan().bold();
            } else {
                s = s.dim();
            }
            spans.push(s);
        }
        Paragraph::new(Line::from(spans)).render(
            Rect {
                x: area.x,
                y: area.y.saturating_add(1),
                width: area.width,
                height: 1,
            },
            buf,
        );
        if !self.expanded_details {
            // 第二行：变量标签 + hint（spans 渲染，当前变量高亮）
            use ratatui::text::Span;
            let mut spans: Vec<Span> = Vec::new();
            if let Some(def) = self.defs.get(self.selected_mode_idx) {
                let cur_idx = *self.selected_var_idx_by_mode.get(&def.id).unwrap_or(&0);
                let vv = self.var_values.get(&def.id);
                for (i, var) in def.variables.iter().enumerate() {
                    let explicit = vv.and_then(|m| m.get(&var.name).cloned()).flatten();
                    // 默认值字符串（用于标签直观展示）
                    let default_str: Option<String> = var.default.as_ref().and_then(|d| match d {
                        Value::Null => None,
                        Value::Bool(b) => Some(b.to_string()),
                        Value::Number(n) => Some(n.to_string()),
                        Value::String(s) => Some(s.clone()),
                        _ => None,
                    });
                    let text = if self.editing
                        && self.editing_mode_id.as_deref() == Some(def.id.as_str())
                        && self.editing_var_name.as_deref() == Some(var.name.as_str())
                    {
                        format!("[{}={}_]", var.name, self.editing_buffer)
                    } else if let Some(val) = explicit.clone() {
                        format!("[{}={}]", var.name, val)
                    } else if var.required && var.default.is_none() {
                        format!("[{}=!]", var.name)
                    } else if let Some(dv) = default_str {
                        // 方案 B：使用默认值直接展示
                        format!("[{}={}]", var.name, dv)
                    } else {
                        format!("[{}=?]", var.name)
                    };
                    let mut s = Span::from(text);
                    if i == cur_idx {
                        s = s.cyan().bold();
                    } else {
                        s = s.dim();
                    }
                    spans.push(s);
                    if i + 1 < def.variables.len() {
                        spans.push(Span::from(" "));
                    }
                }
            }
            // 渲染变量行（单独一行）
            Paragraph::new(Line::from(spans)).render(
                Rect {
                    x: area.x,
                    y: area.y + 2,
                    width: area.width,
                    height: 1,
                },
                buf,
            );
            // 内部分隔（更醒目的双线，变量与提示之间）
            {
                use ratatui::text::Span as RtSpan;
                use ratatui::widgets::Paragraph;
                let heavy = "═".repeat(area.width as usize);
                Paragraph::new(Line::from(vec![RtSpan::from(heavy).dim()])).render(
                    Rect {
                        x: area.x,
                        y: area.y + 3,
                        width: area.width,
                        height: 1,
                    },
                    buf,
                );
            }
            let hint = if self.editing {
                // 若是枚举编辑，提供上下选择提示
                let mut enum_editing = false;
                if let (Some(mode_id), Some(var_name)) = (
                    self.editing_mode_id.as_ref(),
                    self.editing_var_name.as_ref(),
                ) && let Some(def) = self.defs.iter().find(|d| &d.id == mode_id)
                    && let Some(vdef) = def.variables.iter().find(|v| &v.name == var_name)
                    && vdef.r#enum.is_some()
                {
                    enum_editing = true;
                }
                if enum_editing {
                    "↑↓ Select  ⏎ Apply  Esc Cancel".dim().to_string()
                } else {
                    "⏎ Apply  Esc Cancel".dim().to_string()
                }
            } else {
                "d Details  Tab Modes  ←→ Vars  ⏎ Edit  Space Toggle  Esc Exit"
                    .dim()
                    .to_string()
            };
            use ratatui::text::Span as RtSpan;
            Paragraph::new(Line::from(vec![RtSpan::from(hint)])).render(
                Rect {
                    x: area.x,
                    y: area.y + 4,
                    width: area.width,
                    height: 1,
                },
                buf,
            );
        } else {
            // 详情模式：scope/vars/body 使用多行 wrap
            if let Some(def) = self.defs.get(self.selected_mode_idx) {
                use crate::wrapping::RtOptions;
                use crate::wrapping::word_wrap_line;
                let wrap_w = area.width.max(1) as usize;
                // y = 0: top-sep；y = 1: summary；内容自 y=2 起
                let mut y = area.y + 2;
                // 最后一行留给提示，倒数第二行用于分隔
                let bottom_hint_y = area.y + area.height - 1;
                let separator_y = bottom_hint_y.saturating_sub(1);

                // scope 段
                let title = def
                    .display_name
                    .clone()
                    .unwrap_or_else(|| def.id.trim_start_matches('/').to_string());
                let scope_label = match &def.scope {
                    codex_modes::ModeScope::Global => "global".to_string(),
                    codex_modes::ModeScope::Project(d) => format!("project:{d}"),
                };
                let scope_line = Line::from(format!("{title} — scope: {scope_label}"));
                let opts = RtOptions::new(wrap_w)
                    .initial_indent("▌ ".into())
                    .subsequent_indent("  ".into());
                for l in word_wrap_line(&scope_line, opts) {
                    if y >= separator_y {
                        break;
                    }
                    Paragraph::new(l).render(
                        Rect {
                            x: area.x,
                            y,
                            width: area.width,
                            height: 1,
                        },
                        buf,
                    );
                    y = y.saturating_add(1);
                }

                // vars 段
                let vv = self.var_values.get(&def.id);
                let mut kvs: Vec<String> = Vec::new();
                for v in &def.variables {
                    if let Some(Some(val)) = vv.and_then(|m| m.get(&v.name)) {
                        kvs.push(format!("{}={}", v.name, val));
                    } else if v.default.is_some() {
                        kvs.push(format!("{}=(default)", v.name));
                    }
                }
                let vars_text = if kvs.is_empty() {
                    "vars: (none)".to_string()
                } else {
                    format!("vars: {}", kvs.join(", "))
                };
                let vars_line = Line::from(vars_text);
                let opts = RtOptions::new(wrap_w)
                    .initial_indent("▌ ".into())
                    .subsequent_indent("  ".into());
                for l in word_wrap_line(&vars_line, opts) {
                    if y >= separator_y {
                        break;
                    }
                    Paragraph::new(l).render(
                        Rect {
                            x: area.x,
                            y,
                            width: area.width,
                            height: 1,
                        },
                        buf,
                    );
                    y = y.saturating_add(1);
                }

                // body 段（dim）
                let body_line = Line::from(def.body.replace('\n', " ").dim());
                let opts = RtOptions::new(wrap_w)
                    .initial_indent("▌ ".into())
                    .subsequent_indent("  ".into());
                for l in word_wrap_line(&body_line, opts) {
                    if y >= separator_y {
                        break;
                    }
                    Paragraph::new(l).render(
                        Rect {
                            x: area.x,
                            y,
                            width: area.width,
                            height: 1,
                        },
                        buf,
                    );
                    y = y.saturating_add(1);
                }
                // 分隔线（倒数第二行），增强与提示行的区分
                use ratatui::text::Span;
                if separator_y > area.y {
                    let sep_text = "─".repeat(area.width as usize);
                    let sep = Span::from(sep_text).dim();
                    Paragraph::new(Line::from(vec![sep])).render(
                        Rect {
                            x: area.x,
                            y: separator_y,
                            width: area.width,
                            height: 1,
                        },
                        buf,
                    );
                }
                // 在底部保留一行英文提示（详情模式下提供 d 关闭提示）
                let hint = "   d Hide details  Tab Modes  ←→ Vars  ⏎ Edit  Space Toggle  Esc Exit"
                    .dim()
                    .to_string();
                let line = Line::from(vec![Span::from(hint)]);
                Paragraph::new(line).render(
                    Rect {
                        x: area.x,
                        y: bottom_hint_y,
                        width: area.width,
                        height: 1,
                    },
                    buf,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_event::AppEvent;
    use crate::app_event_sender::AppEventSender;
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
                variables: vec![codex_modes::ModeVariableDefinition {
                    name: "target".into(),
                    var_type: None,
                    required: false,
                    default: None,
                    r#enum: None,
                    shortcuts: None,
                    pattern: None,
                    inline_edit: None,
                    mode_scoped: None,
                }],
                scope: codex_modes::ModeScope::Project("demo".into()),
                path: PathBuf::new(),
                body: "Review with target={{target}}".into(),
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
                body: "QA level={{level}}".into(),
            },
        ]
    }

    fn render_lines(view: &ModeBarView, width: u16) -> String {
        use crate::render::renderable::Renderable;
        let height = Renderable::desired_height(view, width);
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
    fn renders_summary_and_vars() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let defs = mk_defs();
        let mut state = PersistentModeState::default();
        if state.enabled.insert("/review".to_string()) {
            state.enable_order.push("/review".to_string());
        }
        let view = ModeBarView::new(
            defs,
            state,
            "base".to_string(),
            None,
            tx,
            std::sync::Arc::new(|_s: String| {}),
            std::sync::Arc::new(|_st: PersistentModeState| {}),
        );
        assert_snapshot!("modebar_summary_and_vars", render_lines(&view, 64));
    }

    #[test]
    fn shows_inline_edit_buffer() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let defs = mk_defs();
        let mut state = PersistentModeState::default();
        if state.enabled.insert("/review".to_string()) {
            state.enable_order.push("/review".to_string());
        }
        let mut view = ModeBarView::new(
            defs,
            state,
            "base".to_string(),
            None,
            tx,
            std::sync::Arc::new(|_s: String| {}),
            std::sync::Arc::new(|_st: PersistentModeState| {}),
        );
        // Enter editing for /review:target
        view.editing = true;
        view.editing_mode_id = Some("/review".to_string());
        view.editing_var_name = Some("target".to_string());
        view.editing_buffer = "staging".to_string();
        assert_snapshot!("modebar_inline_edit", render_lines(&view, 64));
    }

    #[test]
    fn renders_expanded_details_wrapped() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut defs = mk_defs();
        // Make body long to force wrapping
        if let Some(first) = defs.get_mut(0) {
            first.body = "This is a very long instruction body that should wrap across multiple lines when rendered in the expanded details view.".into();
        }
        let mut state = PersistentModeState::default();
        if state.enabled.insert("/review".to_string()) {
            state.enable_order.push("/review".to_string());
        }
        let mut view = ModeBarView::new(
            defs,
            state,
            "base".to_string(),
            None,
            tx,
            std::sync::Arc::new(|_s: String| {}),
            std::sync::Arc::new(|_st: PersistentModeState| {}),
        );
        view.expanded_details = true;
        assert_snapshot!("modebar_expanded_details_wrap", render_lines(&view, 48));
    }

    #[test]
    fn renders_summary_with_missing_required_flag() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let defs = mk_defs();
        let mut state = PersistentModeState::default();
        if state.enabled.insert("/qa".to_string()) {
            state.enable_order.push("/qa".to_string());
        }
        let view = ModeBarView::new(
            defs,
            state,
            "base".to_string(),
            None,
            tx,
            std::sync::Arc::new(|_s: String| {}),
            std::sync::Arc::new(|_st: PersistentModeState| {}),
        );
        assert_snapshot!("modebar_summary_missing_required", render_lines(&view, 64));
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
                        for ch in sp.content.as_ref().chars() {
                            if ch.is_control() {
                                use std::fmt::Write as _;
                                let _ = write!(s, "\\u{:04x}", ch as u32);
                            } else {
                                s.push(ch);
                            }
                        }
                    }
                    s.push('\n');
                }
                out.push(s);
            }
        }
        out
    }

    #[test]
    fn emits_e3102_on_enum_mismatch() {
        let (tx_raw, mut rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let defs = vec![codex_modes::ModeDefinition {
            id: "/enum".into(),
            display_name: Some("enum".into()),
            description: None,
            argument_hint: None,
            kind: codex_modes::ModeKind::Persistent,
            default_enabled: true,
            variables: vec![codex_modes::ModeVariableDefinition {
                name: "choice".into(),
                var_type: Some(codex_modes::VarType::Enum),
                required: false,
                default: None,
                r#enum: Some(vec!["a".into(), "b".into()]),
                shortcuts: None,
                pattern: None,
                inline_edit: None,
                mode_scoped: None,
            }],
            scope: codex_modes::ModeScope::Project("demo".into()),
            path: PathBuf::new(),
            body: "Body".into(),
        }];
        let mut state = PersistentModeState::default();
        if state.enabled.insert("/enum".to_string()) {
            state.enable_order.push("/enum".to_string());
        }
        let mut view = ModeBarView::new(
            defs,
            state,
            "base".to_string(),
            None,
            tx,
            std::sync::Arc::new(|_s: String| {}),
            std::sync::Arc::new(|_st: PersistentModeState| {}),
        );
        view.editing = true;
        view.editing_mode_id = Some("/enum".to_string());
        view.editing_var_name = Some("choice".to_string());
        view.editing_buffer = "c".to_string();
        view.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let msgs = drain_history_strings(&mut rx);
        let last = msgs.last().cloned().unwrap_or_default();
        assert_snapshot!("modebar_error_enum_mismatch", last);
    }

    #[test]
    fn emits_e3106_on_boolean_invalid() {
        let (tx_raw, mut rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let defs = vec![codex_modes::ModeDefinition {
            id: "/b".into(),
            display_name: Some("b".into()),
            description: None,
            argument_hint: None,
            kind: codex_modes::ModeKind::Persistent,
            default_enabled: true,
            variables: vec![codex_modes::ModeVariableDefinition {
                name: "flag".into(),
                var_type: Some(codex_modes::VarType::Boolean),
                required: false,
                default: None,
                r#enum: None,
                shortcuts: None,
                pattern: None,
                inline_edit: None,
                mode_scoped: None,
            }],
            scope: codex_modes::ModeScope::Project("demo".into()),
            path: PathBuf::new(),
            body: "Body".into(),
        }];
        let mut state = PersistentModeState::default();
        if state.enabled.insert("/b".to_string()) {
            state.enable_order.push("/b".to_string());
        }
        let mut view = ModeBarView::new(
            defs,
            state,
            "base".to_string(),
            None,
            tx,
            std::sync::Arc::new(|_s: String| {}),
            std::sync::Arc::new(|_st: PersistentModeState| {}),
        );
        view.editing = true;
        view.editing_mode_id = Some("/b".to_string());
        view.editing_var_name = Some("flag".to_string());
        view.editing_buffer = "notbool".to_string();
        view.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let msgs = drain_history_strings(&mut rx);
        let last = msgs.last().cloned().unwrap_or_default();
        assert_snapshot!("modebar_error_boolean_invalid", last);
    }

    #[test]
    fn emits_e3107_on_number_invalid() {
        let (tx_raw, mut rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let defs = vec![codex_modes::ModeDefinition {
            id: "/n".into(),
            display_name: Some("n".into()),
            description: None,
            argument_hint: None,
            kind: codex_modes::ModeKind::Persistent,
            default_enabled: true,
            variables: vec![codex_modes::ModeVariableDefinition {
                name: "num".into(),
                var_type: Some(codex_modes::VarType::Number),
                required: false,
                default: None,
                r#enum: None,
                shortcuts: None,
                pattern: None,
                inline_edit: None,
                mode_scoped: None,
            }],
            scope: codex_modes::ModeScope::Project("demo".into()),
            path: PathBuf::new(),
            body: "Body".into(),
        }];
        let mut state = PersistentModeState::default();
        if state.enabled.insert("/n".to_string()) {
            state.enable_order.push("/n".to_string());
        }
        let mut view = ModeBarView::new(
            defs,
            state,
            "base".to_string(),
            None,
            tx,
            std::sync::Arc::new(|_s: String| {}),
            std::sync::Arc::new(|_st: PersistentModeState| {}),
        );
        view.editing = true;
        view.editing_mode_id = Some("/n".to_string());
        view.editing_var_name = Some("num".to_string());
        view.editing_buffer = "abc".to_string();
        view.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let msgs = drain_history_strings(&mut rx);
        let last = msgs.last().cloned().unwrap_or_default();
        assert_snapshot!("modebar_error_number_invalid", last);
    }

    #[test]
    fn emits_e3108_on_path_invalid() {
        let (tx_raw, mut rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let defs = vec![codex_modes::ModeDefinition {
            id: "/p".into(),
            display_name: Some("p".into()),
            description: None,
            argument_hint: None,
            kind: codex_modes::ModeKind::Persistent,
            default_enabled: true,
            variables: vec![codex_modes::ModeVariableDefinition {
                name: "path".into(),
                var_type: Some(codex_modes::VarType::Path),
                required: false,
                default: None,
                r#enum: None,
                shortcuts: None,
                pattern: None,
                inline_edit: None,
                mode_scoped: None,
            }],
            scope: codex_modes::ModeScope::Project("demo".into()),
            path: PathBuf::new(),
            body: "Body".into(),
        }];
        let mut state = PersistentModeState::default();
        if state.enabled.insert("/p".to_string()) {
            state.enable_order.push("/p".to_string());
        }
        let mut view = ModeBarView::new(
            defs,
            state,
            "base".to_string(),
            None,
            tx,
            std::sync::Arc::new(|_s: String| {}),
            std::sync::Arc::new(|_st: PersistentModeState| {}),
        );
        view.editing = true;
        view.editing_mode_id = Some("/p".to_string());
        view.editing_var_name = Some("path".to_string());
        view.editing_buffer = "\u{0001}bad".to_string();
        view.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let msgs = drain_history_strings(&mut rx);
        let last = msgs.last().cloned().unwrap_or_default();
        assert_snapshot!("modebar_error_path_invalid", last);
    }
}
