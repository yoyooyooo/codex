use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use codex_core::config::Config;
use codex_core::config_types::Notifications;
use codex_core::git_info::current_branch_name;
use codex_core::git_info::local_git_branches;
use codex_core::protocol::AgentMessageDeltaEvent;
use codex_core::protocol::AgentMessageEvent;
use codex_core::protocol::AgentReasoningDeltaEvent;
use codex_core::protocol::AgentReasoningEvent;
use codex_core::protocol::AgentReasoningRawContentDeltaEvent;
use codex_core::protocol::AgentReasoningRawContentEvent;
use codex_core::protocol::ApplyPatchApprovalRequestEvent;
use codex_core::protocol::BackgroundEventEvent;
use codex_core::protocol::ErrorEvent;
use codex_core::protocol::Event;
use codex_core::protocol::EventMsg;
use codex_core::protocol::ExecApprovalRequestEvent;
use codex_core::protocol::ExecCommandBeginEvent;
use codex_core::protocol::ExecCommandEndEvent;
use codex_core::protocol::ExitedReviewModeEvent;
use codex_core::protocol::InputItem;
use codex_core::protocol::InputMessageKind;
use codex_core::protocol::ListCustomPromptsResponseEvent;
use codex_core::protocol::McpListToolsResponseEvent;
use codex_core::protocol::McpToolCallBeginEvent;
use codex_core::protocol::McpToolCallEndEvent;
use codex_core::protocol::Op;
use codex_core::protocol::PatchApplyBeginEvent;
use codex_core::protocol::RateLimitSnapshot;
use codex_core::protocol::ReviewRequest;
use codex_core::protocol::StreamErrorEvent;
use codex_core::protocol::TaskCompleteEvent;
use codex_core::protocol::TokenUsage;
use codex_core::protocol::TokenUsageInfo;
use codex_core::protocol::TurnAbortReason;
use codex_core::protocol::TurnDiffEvent;
use codex_core::protocol::UserMessageEvent;
use codex_core::protocol::WebSearchBeginEvent;
use codex_core::protocol::WebSearchEndEvent;
use codex_protocol::ConversationId;
use codex_protocol::parse_command::ParsedCommand;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use rand::Rng;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use ratatui::widgets::WidgetRef;
use tokio::sync::mpsc::UnboundedSender;
use tracing::debug;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::BottomPane;
use crate::bottom_pane::BottomPaneParams;
use crate::bottom_pane::CancellationEvent;
use crate::bottom_pane::InputResult;
use crate::bottom_pane::SelectionAction;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionViewParams;
use crate::bottom_pane::custom_prompt_view::CustomPromptView;
use crate::bottom_pane::popup_consts::standard_popup_hint_line;
use crate::clipboard_paste::paste_image_to_temp_png;
use crate::diff_render::display_path_for;
use crate::exec_cell::CommandOutput;
use crate::exec_cell::ExecCell;
use crate::get_git_diff::get_git_diff;
use crate::history_cell;
use crate::history_cell::AgentMessageCell;
use crate::history_cell::HistoryCell;
use crate::history_cell::McpToolCallCell;
use ratatui::text::Line;
// Patch event rendering has been simplified; use history_cell helpers directly.
use crate::exec_cell::new_active_exec_command;
use crate::markdown::append_markdown;
use crate::slash_command::SlashCommand;
use crate::status::RateLimitSnapshotDisplay;
use crate::text_formatting::truncate_text;
use crate::tui::FrameRequester;
use codex_protocol::plan_tool;
// streaming internals are provided by crate::streaming and crate::markdown_stream
use crate::bottom_pane::ApprovalRequest;
mod interrupts;
use self::interrupts::InterruptManager;
mod agent;
use self::agent::spawn_agent;
use self::agent::spawn_agent_from_existing;
mod session_header;
use self::session_header::SessionHeader;
use crate::addons::AppLifecycleHook;
use crate::addons::ModeUiContext;
use crate::addons::UiViewFactory;
use crate::addons::UiViewKind;
use crate::modes::PersistentModeState;
use crate::streaming::controller::StreamController;
use chrono::Local;
use codex_common::approval_presets::ApprovalPreset;
use codex_common::approval_presets::builtin_approval_presets;
use codex_common::model_presets::ModelPreset;
use codex_common::model_presets::builtin_model_presets;
use codex_core::AuthManager;
use codex_core::ConversationManager;
use codex_core::protocol::AskForApproval;
use codex_core::protocol::SandboxPolicy;
use codex_core::protocol_config_types::ReasoningEffort as ReasoningEffortConfig;
use codex_file_search::FileMatch;
use codex_git_tooling::CreateGhostCommitOptions;
use codex_git_tooling::GhostCommit;
use codex_git_tooling::GitToolingError;
use codex_git_tooling::create_ghost_commit;
use codex_git_tooling::restore_ghost_commit;
use strum::IntoEnumIterator;

const MAX_TRACKED_GHOST_COMMITS: usize = 20;

// Track information about an in-flight exec command.
struct RunningCommand {
    command: Vec<String>,
    parsed_cmd: Vec<ParsedCommand>,
}

const RATE_LIMIT_WARNING_THRESHOLDS: [f64; 3] = [75.0, 90.0, 95.0];

#[derive(Default)]
struct RateLimitWarningState {
    secondary_index: usize,
    primary_index: usize,
}

impl RateLimitWarningState {
    fn take_warnings(
        &mut self,
        secondary_used_percent: Option<f64>,
        primary_used_percent: Option<f64>,
    ) -> Vec<String> {
        let reached_secondary_cap =
            matches!(secondary_used_percent, Some(percent) if percent == 100.0);
        let reached_primary_cap = matches!(primary_used_percent, Some(percent) if percent == 100.0);
        if reached_secondary_cap || reached_primary_cap {
            return Vec::new();
        }

        let mut warnings = Vec::new();

        if let Some(secondary_used_percent) = secondary_used_percent {
            let mut highest_secondary: Option<f64> = None;
            while self.secondary_index < RATE_LIMIT_WARNING_THRESHOLDS.len()
                && secondary_used_percent >= RATE_LIMIT_WARNING_THRESHOLDS[self.secondary_index]
            {
                highest_secondary = Some(RATE_LIMIT_WARNING_THRESHOLDS[self.secondary_index]);
                self.secondary_index += 1;
            }
            if let Some(threshold) = highest_secondary {
                warnings.push(format!(
                    "Heads up, you've used over {threshold:.0}% of your weekly limit. Run /status for a breakdown."
                ));
            }
        }

        if let Some(primary_used_percent) = primary_used_percent {
            let mut highest_primary: Option<f64> = None;
            while self.primary_index < RATE_LIMIT_WARNING_THRESHOLDS.len()
                && primary_used_percent >= RATE_LIMIT_WARNING_THRESHOLDS[self.primary_index]
            {
                highest_primary = Some(RATE_LIMIT_WARNING_THRESHOLDS[self.primary_index]);
                self.primary_index += 1;
            }
            if let Some(threshold) = highest_primary {
                warnings.push(format!(
                    "Heads up, you've used over {threshold:.0}% of your 5h limit. Run /status for a breakdown."
                ));
            }
        }

        warnings
    }
}

/// Common initialization parameters shared by all `ChatWidget` constructors.
pub(crate) struct ChatWidgetInit {
    pub(crate) config: Config,
    pub(crate) frame_requester: FrameRequester,
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) initial_prompt: Option<String>,
    pub(crate) initial_images: Vec<PathBuf>,
    pub(crate) enhanced_keys_supported: bool,
    pub(crate) auth_manager: Arc<AuthManager>,
}

pub(crate) struct ChatWidget {
    app_event_tx: AppEventSender,
    codex_op_tx: UnboundedSender<Op>,
    bottom_pane: BottomPane,
    active_cell: Option<Box<dyn HistoryCell>>,
    config: Config,
    auth_manager: Arc<AuthManager>,
    session_header: SessionHeader,
    initial_user_message: Option<UserMessage>,
    token_info: Option<TokenUsageInfo>,
    rate_limit_snapshot: Option<RateLimitSnapshotDisplay>,
    rate_limit_warnings: RateLimitWarningState,
    // Stream lifecycle controller
    stream_controller: Option<StreamController>,
    running_commands: HashMap<String, RunningCommand>,
    task_complete_pending: bool,
    // Queue of interruptive UI events deferred during an active write cycle
    interrupts: InterruptManager,
    // Accumulates the current reasoning block text to extract a header
    reasoning_buffer: String,
    // Accumulates full reasoning content for transcript-only recording
    full_reasoning_buffer: String,
    conversation_id: Option<ConversationId>,
    frame_requester: FrameRequester,
    // Whether to include the initial welcome banner on session configured
    show_welcome_banner: bool,
    // When resuming an existing session (selected via resume picker), avoid an
    // immediate redraw on SessionConfigured to prevent a gratuitous UI flicker.
    suppress_session_configured_redraw: bool,
    // User messages queued while a turn is in progress
    queued_user_messages: VecDeque<UserMessage>,
    // Pending notification to show when unfocused on next Draw
    pending_notification: Option<Notification>,
    // Simple review mode flag; used to adjust layout and banners.
    is_review_mode: bool,
    // List of ghost commits corresponding to each turn.
    ghost_snapshots: Vec<GhostCommit>,
    ghost_snapshots_disabled: bool,
    // Captured baseline <user_instructions> (without <mode_instructions>)
    base_user_instructions: Option<String>,
    // Current applied full <user_instructions> (for equivalence short‑circuit)
    current_user_instructions: Option<String>,
    // Cached persistent mode enablement state for reopening ModeBar/Panel.
    persistent_mode_state: PersistentModeState,
    // 是否为恢复已有会话（而非全新会话）
    resumed_session: bool,

    // 可选的生命周期钩子（最小插桩）。
    lifecycle_hooks: Vec<Box<dyn AppLifecycleHook>>,
    // 通用视图工厂（优先级高于 mode_ui_factories）。
    ui_view_factories: Vec<Box<dyn UiViewFactory>>,

    // 内部 UI 调度：跨线程将 UI 更新回投到主线程执行
    ui_tasks_tx: std::sync::mpsc::Sender<Box<dyn Fn(&mut ChatWidget) + Send + 'static>>,
    ui_tasks_rx: std::sync::mpsc::Receiver<Box<dyn Fn(&mut ChatWidget) + Send + 'static>>,
}

struct UserMessage {
    text: String,
    image_paths: Vec<PathBuf>,
}

impl From<String> for UserMessage {
    fn from(text: String) -> Self {
        Self {
            text,
            image_paths: Vec::new(),
        }
    }
}

fn create_initial_user_message(text: String, image_paths: Vec<PathBuf>) -> Option<UserMessage> {
    if text.is_empty() && image_paths.is_empty() {
        None
    } else {
        Some(UserMessage { text, image_paths })
    }
}

fn extract_base_user_instructions(xml: &str) -> Option<String> {
    let s = xml.trim();
    let open = "<user_instructions>";
    let close = "</user_instructions>";
    let mut inner = if let (Some(a), Some(b)) = (s.find(open), s.rfind(close)) {
        let start = a + open.len();
        if b <= start {
            return None;
        }
        s[start..b].to_string()
    } else {
        s.to_string()
    };
    // Strip <mode_instructions> block if present
    let mi_open = "<mode_instructions>";
    let mi_close = "</mode_instructions>";
    if let (Some(a), Some(b)) = (inner.find(mi_open), inner.rfind(mi_close)) {
        let end = b + mi_close.len();
        // Remove the block, carefully handling surrounding newlines.
        inner.replace_range(a..end, "");
    }
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

impl ChatWidget {
    fn model_description_for(slug: &str) -> Option<&'static str> {
        if slug.starts_with("gpt-5-codex") {
            Some("Optimized for coding tasks with many tools.")
        } else if slug.starts_with("gpt-5") {
            Some("Broad world knowledge with strong general reasoning.")
        } else {
            None
        }
    }
    /// 从内部队列拉取并执行所有待处理的 UI 任务（在 UI 线程调用）。
    pub(crate) fn drain_ui_tasks(&mut self) {
        loop {
            match self.ui_tasks_rx.try_recv() {
                Ok(task) => task(self),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }
        }
    }
    /// 注册一个生命周期钩子。
    #[allow(dead_code)]
    pub(crate) fn register_lifecycle_hook(&mut self, hook: Box<dyn AppLifecycleHook>) {
        self.lifecycle_hooks.push(hook);
    }

    /// 注册一个用于构建 Mode UI 的工厂（自动适配为通用工厂）。
    /// 注：内部统一路由到 `ui_view_factories`，不再单独维护 `mode_ui_factories`。
    #[allow(dead_code)]
    pub(crate) fn register_mode_ui_factory(
        &mut self,
        factory: Box<dyn crate::addons::ModeUiFactory>,
    ) {
        let adapter = crate::addons::ModeUiFactoryAdapter::new(factory);
        self.ui_view_factories.push(Box::new(adapter));
    }

    /// 注册一个通用视图工厂。
    pub(crate) fn register_ui_view_factory(&mut self, factory: Box<dyn UiViewFactory>) {
        self.ui_view_factories.push(factory);
    }

    /// Compute a best-effort baseline <user_instructions> by combining
    /// config.user_instructions with project-level AGENTS.md discovered from
    /// the Git root to cwd. This mirrors core's project_doc discovery so that
    /// TUI features (Ctrl+U, ModeBar/Panel rendering) include project docs
    /// even when the initial <user_instructions> message was not replayed.
    fn compose_fallback_user_instructions_with_project_doc(&self) -> String {
        use std::fs;
        use std::path::PathBuf;

        let mut combined = self.config.user_instructions.clone().unwrap_or_default();

        // Discover project docs: walk up to git root, then collect AGENTS.md
        // from root -> cwd (inclusive).
        let mut dir = self.config.cwd.clone();
        if let Ok(canon) = dir.canonicalize() {
            dir = canon;
        }

        // Build chain upwards and detect git root.
        let mut chain: Vec<PathBuf> = vec![dir.clone()];
        let mut git_root: Option<PathBuf> = None;
        let mut cursor = dir;
        while let Some(parent) = cursor.parent() {
            let git_marker = cursor.join(".git");
            let git_exists = match fs::metadata(&git_marker) {
                Ok(_) => true,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(_) => false,
            };
            if git_exists {
                git_root = Some(cursor.clone());
                break;
            }
            chain.push(parent.to_path_buf());
            cursor = parent.to_path_buf();
        }

        let search_dirs: Vec<PathBuf> = if let Some(root) = git_root {
            let mut dirs: Vec<PathBuf> = Vec::new();
            let mut saw_root = false;
            for p in chain.iter().rev() {
                if !saw_root {
                    if p == &root {
                        saw_root = true;
                    } else {
                        continue;
                    }
                }
                dirs.push(p.clone());
            }
            dirs
        } else {
            vec![self.config.cwd.clone()]
        };

        let mut project_parts: Vec<String> = Vec::new();
        let mut remaining = self.config.project_doc_max_bytes as u64;
        for d in search_dirs {
            if remaining == 0 {
                break;
            }
            let candidate = d.join("AGENTS.md");
            match fs::symlink_metadata(&candidate) {
                Ok(md) if md.is_file() || md.file_type().is_symlink() => {
                    if let Ok(text) = fs::read_to_string(&candidate) {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            let bytes = text.as_bytes();
                            let take = (remaining as usize).min(bytes.len());
                            let slice = &bytes[..take];
                            let s = String::from_utf8_lossy(slice).to_string();
                            project_parts.push(s);
                            remaining = remaining.saturating_sub(take as u64);
                        }
                    }
                }
                _ => {}
            }
        }

        if !project_parts.is_empty() {
            if !combined.is_empty() {
                combined.push_str("\n\n--- project-doc ---\n\n");
            }
            combined.push_str(&project_parts.join("\n\n"));
        }

        combined
    }
    fn flush_answer_stream_with_separator(&mut self) {
        if let Some(mut controller) = self.stream_controller.take()
            && let Some(cell) = controller.finalize()
        {
            self.add_boxed_history(cell);
        }
    }

    // --- Small event handlers ---
    fn on_session_configured(&mut self, event: codex_core::protocol::SessionConfiguredEvent) {
        self.bottom_pane
            .set_history_metadata(event.history_log_id, event.history_entry_count);
        self.conversation_id = Some(event.session_id);
        let initial_messages = event.initial_messages.clone();
        let model_for_header = event.model.clone();
        self.session_header.set_model(&model_for_header);
        self.add_to_history(history_cell::new_session_info(
            &self.config,
            event,
            self.show_welcome_banner,
        ));
        if let Some(messages) = initial_messages {
            self.replay_initial_messages(messages);
        }
        // Ask codex-core to enumerate custom prompts for this session.
        self.submit_op(Op::ListCustomPrompts);
        if let Some(user_message) = self.initial_user_message.take() {
            self.submit_user_message(user_message);
        }
        if !self.suppress_session_configured_redraw {
            self.request_redraw();
        }

        // Auto-apply default-enabled persistent modes at session start (silent)
        // 仅在“非恢复会话”且当前不存在完整的 user_instructions 时执行。
        if !self.resumed_session && self.current_user_instructions.is_none() {
            // Only when we have a baseline <user_instructions> captured or a config fallback.
            let base = self
                .base_user_instructions
                .clone()
                .unwrap_or_else(|| self.compose_fallback_user_instructions_with_project_doc());
            // Build enabled list without emitting history noise; apply only if any.
            use codex_modes::EnabledMode;
            use codex_modes::IndexMap as IMap;
            use codex_modes::ModeKind;
            use codex_modes::render_user_instructions;
            use codex_modes::scan_modes;
            if let Ok(defs) = scan_modes(&self.config.cwd, Some(&self.config.codex_home)) {
                let mut enabled: Vec<EnabledMode> = Vec::new();
                for def in &defs {
                    if def.kind != ModeKind::Persistent || !def.default_enabled {
                        continue;
                    }
                    let mut ok = true;
                    let mut vars: IMap<&str, Option<String>> = IMap::new();
                    for v in &def.variables {
                        if v.required && v.default.is_none() {
                            ok = false;
                            break;
                        }
                        vars.insert(v.name.as_str(), None);
                    }
                    if ok {
                        enabled.push(EnabledMode {
                            id: &def.id,
                            display_name: def.display_name.as_deref(),
                            scope: &def.scope,
                            variables: vars,
                        });
                    }
                }
                if !enabled.is_empty()
                    && let Ok(rendered) = render_user_instructions(&base, &enabled, &defs)
                {
                    let mut state = PersistentModeState::default();
                    for em in &enabled {
                        let id = em.id.to_string();
                        if state.enabled.insert(id.clone()) {
                            state.enable_order.push(id);
                        }
                    }
                    self.persistent_mode_state = state.clone();
                    self.submit_op(Op::OverrideTurnContext {
                        cwd: None,
                        approval_policy: None,
                        sandbox_policy: None,
                        model: None,
                        effort: None,
                        summary: None,
                        user_instructions: Some(rendered),
                    });
                    if let Ok(rendered_now) = render_user_instructions(&base, &enabled, &defs) {
                        self.current_user_instructions = Some(rendered_now);
                    }
                    // Update persistent mode summary silently（直接更新 BottomPane）
                    let labels = codex_modes::enabled_labels(&enabled);
                    let summary = codex_modes::format_mode_summary(&labels);
                    self.set_mode_summary(summary);
                }
            }
        }

        // 尾部调用扩展生命周期钩子（无扩展时无效果）。
        for hook in self.lifecycle_hooks.iter_mut() {
            hook.on_session_configured();
        }
    }

    fn on_agent_message(&mut self, message: String) {
        if self.stream_controller.is_none() {
            self.handle_streaming_delta(message);
        }
        self.flush_answer_stream_with_separator();
        self.handle_stream_finished();
        self.request_redraw();
    }

    fn on_agent_message_delta(&mut self, delta: String) {
        self.handle_streaming_delta(delta);
    }

    fn on_agent_reasoning_delta(&mut self, delta: String) {
        // For reasoning deltas, do not stream to history. Accumulate the
        // current reasoning block and extract the first bold element
        // (between **/**) as the chunk header. Show this header as status.
        self.reasoning_buffer.push_str(&delta);

        if let Some(header) = extract_first_bold(&self.reasoning_buffer) {
            // Update the shimmer header to the extracted reasoning chunk header.
            self.bottom_pane.update_status_header(header);
        } else {
            // Fallback while we don't yet have a bold header: leave existing header as-is.
        }
        self.request_redraw();
    }

    fn on_agent_reasoning_final(&mut self) {
        // At the end of a reasoning block, record transcript-only content.
        self.full_reasoning_buffer.push_str(&self.reasoning_buffer);
        if !self.full_reasoning_buffer.is_empty() {
            let cell = history_cell::new_reasoning_summary_block(
                self.full_reasoning_buffer.clone(),
                &self.config,
            );
            self.add_boxed_history(cell);
        }
        self.reasoning_buffer.clear();
        self.full_reasoning_buffer.clear();
        self.request_redraw();
    }

    fn on_reasoning_section_break(&mut self) {
        // Start a new reasoning block for header extraction and accumulate transcript.
        self.full_reasoning_buffer.push_str(&self.reasoning_buffer);
        self.full_reasoning_buffer.push_str("\n\n");
        self.reasoning_buffer.clear();
    }

    // Raw reasoning uses the same flow as summarized reasoning

    fn on_task_started(&mut self) {
        self.bottom_pane.clear_ctrl_c_quit_hint();
        self.bottom_pane.set_task_running(true);
        self.stream_controller = None;
        self.full_reasoning_buffer.clear();
        self.reasoning_buffer.clear();
        self.request_redraw();
    }

    fn on_task_complete(&mut self, last_agent_message: Option<String>) {
        // If a stream is currently active, finalize it.
        self.flush_answer_stream_with_separator();
        // Mark task stopped and request redraw now that all content is in history.
        self.bottom_pane.set_task_running(false);
        self.running_commands.clear();
        self.request_redraw();

        // If there is a queued user message, send exactly one now to begin the next turn.
        self.maybe_send_next_queued_input();
        // Emit a notification when the turn completes (suppressed if focused).
        self.notify(Notification::AgentTurnComplete {
            response: last_agent_message.unwrap_or_default(),
        });
    }

    pub(crate) fn set_token_info(&mut self, info: Option<TokenUsageInfo>) {
        if info.is_some() {
            self.bottom_pane.set_token_usage(info.clone());
            self.token_info = info;
        }
    }
    fn on_rate_limit_snapshot(&mut self, snapshot: Option<RateLimitSnapshot>) {
        if let Some(snapshot) = snapshot {
            let warnings = self.rate_limit_warnings.take_warnings(
                snapshot
                    .secondary
                    .as_ref()
                    .map(|window| window.used_percent),
                snapshot.primary.as_ref().map(|window| window.used_percent),
            );

            let display = crate::status::rate_limit_snapshot_display(&snapshot, Local::now());
            self.rate_limit_snapshot = Some(display);

            if !warnings.is_empty() {
                for warning in warnings {
                    self.add_to_history(history_cell::new_warning_event(warning));
                }
                self.request_redraw();
            }
        } else {
            self.rate_limit_snapshot = None;
        }
    }
    /// Finalize any active exec as failed and stop/clear running UI state.
    fn finalize_turn(&mut self) {
        // Ensure any spinner is replaced by a red ✗ and flushed into history.
        self.finalize_active_cell_as_failed();
        // Reset running state and clear streaming buffers.
        self.bottom_pane.set_task_running(false);
        self.running_commands.clear();
        self.stream_controller = None;
    }

    fn on_error(&mut self, message: String) {
        self.finalize_turn();
        self.add_to_history(history_cell::new_error_event(message));
        self.request_redraw();

        // After an error ends the turn, try sending the next queued input.
        self.maybe_send_next_queued_input();
    }

    /// Handle a turn aborted due to user interrupt (Esc).
    /// When there are queued user messages, restore them into the composer
    /// separated by newlines rather than auto‑submitting the next one.
    fn on_interrupted_turn(&mut self, reason: TurnAbortReason) {
        // Finalize, log a gentle prompt, and clear running state.
        self.finalize_turn();

        if reason != TurnAbortReason::ReviewEnded {
            self.add_to_history(history_cell::new_error_event(
                "Conversation interrupted - tell the model what to do differently".to_owned(),
            ));
        }

        // If any messages were queued during the task, restore them into the composer.
        if !self.queued_user_messages.is_empty() {
            let combined = self
                .queued_user_messages
                .iter()
                .map(|m| m.text.clone())
                .collect::<Vec<_>>()
                .join("\n");
            self.bottom_pane.set_composer_text(combined);
            // Clear the queue and update the status indicator list.
            self.queued_user_messages.clear();
            self.refresh_queued_user_messages();
        }

        self.request_redraw();
    }

    fn on_plan_update(&mut self, update: plan_tool::UpdatePlanArgs) {
        self.add_to_history(history_cell::new_plan_update(update));
    }

    fn on_exec_approval_request(&mut self, id: String, ev: ExecApprovalRequestEvent) {
        let id2 = id.clone();
        let ev2 = ev.clone();
        self.defer_or_handle(
            |q| q.push_exec_approval(id, ev),
            |s| s.handle_exec_approval_now(id2, ev2),
        );
    }

    fn on_apply_patch_approval_request(&mut self, id: String, ev: ApplyPatchApprovalRequestEvent) {
        let id2 = id.clone();
        let ev2 = ev.clone();
        self.defer_or_handle(
            |q| q.push_apply_patch_approval(id, ev),
            |s| s.handle_apply_patch_approval_now(id2, ev2),
        );
    }

    fn on_exec_command_begin(&mut self, ev: ExecCommandBeginEvent) {
        self.flush_answer_stream_with_separator();
        let ev2 = ev.clone();
        self.defer_or_handle(|q| q.push_exec_begin(ev), |s| s.handle_exec_begin_now(ev2));
    }

    fn on_exec_command_output_delta(
        &mut self,
        _ev: codex_core::protocol::ExecCommandOutputDeltaEvent,
    ) {
        // TODO: Handle streaming exec output if/when implemented
    }

    fn on_patch_apply_begin(&mut self, event: PatchApplyBeginEvent) {
        if event.auto_approved {
            self.add_to_history(history_cell::new_patch_event(
                event.changes,
                &self.config.cwd,
            ));
        } else {
            self.add_to_history(history_cell::new_change_approved_event(
                event.changes,
                &self.config.cwd,
            ));
        }
    }

    fn on_patch_apply_end(&mut self, event: codex_core::protocol::PatchApplyEndEvent) {
        let ev2 = event.clone();
        self.defer_or_handle(
            |q| q.push_patch_end(event),
            |s| s.handle_patch_apply_end_now(ev2),
        );
    }

    fn on_exec_command_end(&mut self, ev: ExecCommandEndEvent) {
        let ev2 = ev.clone();
        self.defer_or_handle(|q| q.push_exec_end(ev), |s| s.handle_exec_end_now(ev2));
    }

    fn on_mcp_tool_call_begin(&mut self, ev: McpToolCallBeginEvent) {
        let ev2 = ev.clone();
        self.defer_or_handle(|q| q.push_mcp_begin(ev), |s| s.handle_mcp_begin_now(ev2));
    }

    fn on_mcp_tool_call_end(&mut self, ev: McpToolCallEndEvent) {
        let ev2 = ev.clone();
        self.defer_or_handle(|q| q.push_mcp_end(ev), |s| s.handle_mcp_end_now(ev2));
    }

    fn on_web_search_begin(&mut self, _ev: WebSearchBeginEvent) {
        self.flush_answer_stream_with_separator();
    }

    fn on_web_search_end(&mut self, ev: WebSearchEndEvent) {
        self.flush_answer_stream_with_separator();
        self.add_to_history(history_cell::new_web_search_call(format!(
            "Searched: {}",
            ev.query
        )));
    }

    fn on_get_history_entry_response(
        &mut self,
        event: codex_core::protocol::GetHistoryEntryResponseEvent,
    ) {
        let codex_core::protocol::GetHistoryEntryResponseEvent {
            offset,
            log_id,
            entry,
        } = event;
        self.bottom_pane
            .on_history_entry_response(log_id, offset, entry.map(|e| e.text));
    }

    fn on_shutdown_complete(&mut self) {
        self.app_event_tx.send(AppEvent::ExitRequest);
    }

    fn on_turn_diff(&mut self, unified_diff: String) {
        debug!("TurnDiffEvent: {unified_diff}");
    }

    fn on_background_event(&mut self, message: String) {
        debug!("BackgroundEvent: {message}");
    }

    fn on_stream_error(&mut self, message: String) {
        // Show stream errors in the transcript so users see retry/backoff info.
        self.add_to_history(history_cell::new_stream_error_event(message));
        self.request_redraw();
    }

    /// Periodic tick to commit at most one queued line to history with a small delay,
    /// animating the output.
    pub(crate) fn on_commit_tick(&mut self) {
        if let Some(controller) = self.stream_controller.as_mut() {
            let (cell, is_idle) = controller.on_commit_tick();
            if let Some(cell) = cell {
                self.bottom_pane.set_task_running(false);
                self.add_boxed_history(cell);
            }
            if is_idle {
                self.app_event_tx.send(AppEvent::StopCommitAnimation);
            }
        }
    }

    fn flush_interrupt_queue(&mut self) {
        let mut mgr = std::mem::take(&mut self.interrupts);
        mgr.flush_all(self);
        self.interrupts = mgr;
    }

    #[inline]
    fn defer_or_handle(
        &mut self,
        push: impl FnOnce(&mut InterruptManager),
        handle: impl FnOnce(&mut Self),
    ) {
        // Preserve deterministic FIFO across queued interrupts: once anything
        // is queued due to an active write cycle, continue queueing until the
        // queue is flushed to avoid reordering (e.g., ExecEnd before ExecBegin).
        if self.stream_controller.is_some() || !self.interrupts.is_empty() {
            push(&mut self.interrupts);
        } else {
            handle(self);
        }
    }

    fn handle_stream_finished(&mut self) {
        if self.task_complete_pending {
            self.bottom_pane.set_task_running(false);
            self.task_complete_pending = false;
        }
        // A completed stream indicates non-exec content was just inserted.
        self.flush_interrupt_queue();
    }

    #[inline]
    fn handle_streaming_delta(&mut self, delta: String) {
        // Before streaming agent content, flush any active exec cell group.
        self.flush_active_cell();

        if self.stream_controller.is_none() {
            self.stream_controller = Some(StreamController::new(self.config.clone(), None));
        }
        if let Some(controller) = self.stream_controller.as_mut()
            && controller.push(&delta)
        {
            self.app_event_tx.send(AppEvent::StartCommitAnimation);
        }
        self.request_redraw();
    }

    pub(crate) fn handle_exec_end_now(&mut self, ev: ExecCommandEndEvent) {
        let running = self.running_commands.remove(&ev.call_id);
        let (command, parsed) = match running {
            Some(rc) => (rc.command, rc.parsed_cmd),
            None => (vec![ev.call_id.clone()], Vec::new()),
        };

        let needs_new = self
            .active_cell
            .as_ref()
            .map(|cell| cell.as_any().downcast_ref::<ExecCell>().is_none())
            .unwrap_or(true);
        if needs_new {
            self.flush_active_cell();
            self.active_cell = Some(Box::new(new_active_exec_command(
                ev.call_id.clone(),
                command,
                parsed,
            )));
        }

        if let Some(cell) = self
            .active_cell
            .as_mut()
            .and_then(|c| c.as_any_mut().downcast_mut::<ExecCell>())
        {
            cell.complete_call(
                &ev.call_id,
                CommandOutput {
                    exit_code: ev.exit_code,
                    stdout: ev.stdout.clone(),
                    stderr: ev.stderr.clone(),
                    formatted_output: ev.formatted_output.clone(),
                },
                ev.duration,
            );
            if cell.should_flush() {
                self.flush_active_cell();
            }
        }
    }

    pub(crate) fn handle_patch_apply_end_now(
        &mut self,
        event: codex_core::protocol::PatchApplyEndEvent,
    ) {
        // If the patch was successful, just let the "Edited" block stand.
        // Otherwise, add a failure block.
        if !event.success {
            self.add_to_history(history_cell::new_patch_apply_failure(event.stderr));
        }
    }

    pub(crate) fn handle_exec_approval_now(&mut self, id: String, ev: ExecApprovalRequestEvent) {
        self.flush_answer_stream_with_separator();
        // Emit the proposed command into history (like proposed patches)
        self.add_to_history(history_cell::new_proposed_command(&ev.command));
        let command = shlex::try_join(ev.command.iter().map(std::string::String::as_str))
            .unwrap_or_else(|_| ev.command.join(" "));
        self.notify(Notification::ExecApprovalRequested { command });

        let request = ApprovalRequest::Exec {
            id,
            command: ev.command,
            reason: ev.reason,
        };
        self.bottom_pane.push_approval_request(request);
        self.request_redraw();
    }

    pub(crate) fn handle_apply_patch_approval_now(
        &mut self,
        id: String,
        ev: ApplyPatchApprovalRequestEvent,
    ) {
        self.flush_answer_stream_with_separator();
        self.add_to_history(history_cell::new_patch_event(
            ev.changes.clone(),
            &self.config.cwd,
        ));

        let request = ApprovalRequest::ApplyPatch {
            id,
            reason: ev.reason,
            cwd: self.config.cwd.clone(),
            changes: ev.changes.clone(),
        };
        self.bottom_pane.push_approval_request(request);
        self.request_redraw();
        self.notify(Notification::EditApprovalRequested {
            cwd: self.config.cwd.clone(),
            changes: ev.changes.keys().cloned().collect(),
        });
    }

    pub(crate) fn handle_exec_begin_now(&mut self, ev: ExecCommandBeginEvent) {
        // Ensure the status indicator is visible while the command runs.
        self.running_commands.insert(
            ev.call_id.clone(),
            RunningCommand {
                command: ev.command.clone(),
                parsed_cmd: ev.parsed_cmd.clone(),
            },
        );
        if let Some(cell) = self
            .active_cell
            .as_mut()
            .and_then(|c| c.as_any_mut().downcast_mut::<ExecCell>())
            && let Some(new_exec) = cell.with_added_call(
                ev.call_id.clone(),
                ev.command.clone(),
                ev.parsed_cmd.clone(),
            )
        {
            *cell = new_exec;
        } else {
            self.flush_active_cell();

            self.active_cell = Some(Box::new(new_active_exec_command(
                ev.call_id.clone(),
                ev.command.clone(),
                ev.parsed_cmd,
            )));
        }

        self.request_redraw();
    }

    pub(crate) fn handle_mcp_begin_now(&mut self, ev: McpToolCallBeginEvent) {
        self.flush_answer_stream_with_separator();
        self.flush_active_cell();
        self.active_cell = Some(Box::new(history_cell::new_active_mcp_tool_call(
            ev.call_id,
            ev.invocation,
        )));
        self.request_redraw();
    }
    pub(crate) fn handle_mcp_end_now(&mut self, ev: McpToolCallEndEvent) {
        self.flush_answer_stream_with_separator();

        let McpToolCallEndEvent {
            call_id,
            invocation,
            duration,
            result,
        } = ev;

        let extra_cell = match self
            .active_cell
            .as_mut()
            .and_then(|cell| cell.as_any_mut().downcast_mut::<McpToolCallCell>())
        {
            Some(cell) if cell.call_id() == call_id => cell.complete(duration, result),
            _ => {
                self.flush_active_cell();
                let mut cell = history_cell::new_active_mcp_tool_call(call_id, invocation);
                let extra_cell = cell.complete(duration, result);
                self.active_cell = Some(Box::new(cell));
                extra_cell
            }
        };

        self.flush_active_cell();
        if let Some(extra) = extra_cell {
            self.add_boxed_history(extra);
        }
    }

    fn layout_areas(&self, area: Rect) -> [Rect; 3] {
        let bottom_min = self.bottom_pane.desired_height(area.width).min(area.height);
        let remaining = area.height.saturating_sub(bottom_min);

        let active_desired = self
            .active_cell
            .as_ref()
            .map_or(0, |c| c.desired_height(area.width) + 1);
        let active_height = active_desired.min(remaining);
        // Note: no header area; remaining is not used beyond computing active height.

        let header_height = 0u16;

        Layout::vertical([
            Constraint::Length(header_height),
            Constraint::Length(active_height),
            Constraint::Min(bottom_min),
        ])
        .areas(area)
    }

    pub(crate) fn new(
        common: ChatWidgetInit,
        conversation_manager: Arc<ConversationManager>,
    ) -> Self {
        let ChatWidgetInit {
            config,
            frame_requester,
            app_event_tx,
            initial_prompt,
            initial_images,
            enhanced_keys_supported,
            auth_manager,
        } = common;
        let mut rng = rand::rng();
        let placeholder = EXAMPLE_PROMPTS[rng.random_range(0..EXAMPLE_PROMPTS.len())].to_string();
        let codex_op_tx = spawn_agent(config.clone(), app_event_tx.clone(), conversation_manager);
        let (ui_tasks_tx, ui_tasks_rx) = std::sync::mpsc::channel();

        let mut s = Self {
            app_event_tx: app_event_tx.clone(),
            frame_requester: frame_requester.clone(),
            codex_op_tx,
            bottom_pane: BottomPane::new(BottomPaneParams {
                frame_requester,
                app_event_tx,
                has_input_focus: true,
                enhanced_keys_supported,
                placeholder_text: placeholder,
                disable_paste_burst: config.disable_paste_burst,
            }),
            active_cell: None,
            config: config.clone(),
            auth_manager,
            session_header: SessionHeader::new(config.model),
            initial_user_message: create_initial_user_message(
                initial_prompt.unwrap_or_default(),
                initial_images,
            ),
            token_info: None,
            rate_limit_snapshot: None,
            rate_limit_warnings: RateLimitWarningState::default(),
            stream_controller: None,
            running_commands: HashMap::new(),
            task_complete_pending: false,
            interrupts: InterruptManager::new(),
            reasoning_buffer: String::new(),
            full_reasoning_buffer: String::new(),
            conversation_id: None,
            queued_user_messages: VecDeque::new(),
            show_welcome_banner: true,
            suppress_session_configured_redraw: false,
            pending_notification: None,
            is_review_mode: false,
            ghost_snapshots: Vec::new(),
            ghost_snapshots_disabled: true,
            base_user_instructions: None,
            current_user_instructions: None,
            persistent_mode_state: PersistentModeState::default(),
            resumed_session: false,
            lifecycle_hooks: Vec::new(),
            ui_view_factories: Vec::new(),
            ui_tasks_tx,
            ui_tasks_rx,
        };
        // 注册默认的 Modes 视图工厂，确保 Host 仅通过扩展创建模式 UI。
        s.register_ui_view_factory(Box::new(crate::modes::ModesUiDefaultFactory::new()));
        s
    }

    /// Create a ChatWidget attached to an existing conversation (e.g., a fork).
    pub(crate) fn new_from_existing(
        common: ChatWidgetInit,
        conversation: std::sync::Arc<codex_core::CodexConversation>,
        session_configured: codex_core::protocol::SessionConfiguredEvent,
    ) -> Self {
        let ChatWidgetInit {
            config,
            frame_requester,
            app_event_tx,
            initial_prompt,
            initial_images,
            enhanced_keys_supported,
            auth_manager,
        } = common;
        let mut rng = rand::rng();
        let placeholder = EXAMPLE_PROMPTS[rng.random_range(0..EXAMPLE_PROMPTS.len())].to_string();

        let codex_op_tx =
            spawn_agent_from_existing(conversation, session_configured, app_event_tx.clone());
        let (ui_tasks_tx, ui_tasks_rx) = std::sync::mpsc::channel();

        let mut s = Self {
            app_event_tx: app_event_tx.clone(),
            frame_requester: frame_requester.clone(),
            codex_op_tx,
            bottom_pane: BottomPane::new(BottomPaneParams {
                frame_requester,
                app_event_tx,
                has_input_focus: true,
                enhanced_keys_supported,
                placeholder_text: placeholder,
                disable_paste_burst: config.disable_paste_burst,
            }),
            active_cell: None,
            config: config.clone(),
            auth_manager,
            session_header: SessionHeader::new(config.model),
            initial_user_message: create_initial_user_message(
                initial_prompt.unwrap_or_default(),
                initial_images,
            ),
            token_info: None,
            rate_limit_snapshot: None,
            rate_limit_warnings: RateLimitWarningState::default(),
            stream_controller: None,
            running_commands: HashMap::new(),
            task_complete_pending: false,
            interrupts: InterruptManager::new(),
            reasoning_buffer: String::new(),
            full_reasoning_buffer: String::new(),
            conversation_id: None,
            queued_user_messages: VecDeque::new(),
            show_welcome_banner: true,
            suppress_session_configured_redraw: true,
            pending_notification: None,
            is_review_mode: false,
            ghost_snapshots: Vec::new(),
            ghost_snapshots_disabled: true,
            base_user_instructions: None,
            current_user_instructions: None,
            persistent_mode_state: PersistentModeState::default(),
            resumed_session: true,
            lifecycle_hooks: Vec::new(),
            ui_view_factories: Vec::new(),
            ui_tasks_tx,
            ui_tasks_rx,
        };
        s.register_ui_view_factory(Box::new(crate::modes::ModesUiDefaultFactory::new()));
        s
    }

    pub fn desired_height(&self, width: u16) -> u16 {
        self.bottom_pane.desired_height(width)
            + self
                .active_cell
                .as_ref()
                .map_or(0, |c| c.desired_height(width) + 1)
    }

    pub(crate) fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event {
            // 保留上下方向键用于输入历史导航（原有行为）；不再用 Down 打开 ModeBar。
            // Hotkey: apply modes override (Ctrl+P). Note: Ctrl+M is Enter in many terminals.
            KeyEvent {
                code: KeyCode::Char('p'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            }
            // Alternative hotkey: Alt+M.
            | KeyEvent {
                code: KeyCode::Char('m'),
                modifiers: KeyModifiers::ALT,
                kind: KeyEventKind::Press,
                ..
            } => {
                let base = self
                    .base_user_instructions
                    .clone()
                    .unwrap_or_else(|| self.compose_fallback_user_instructions_with_project_doc());
                if let Err(e) = self.apply_modes_override_from_disk(&base) {
                    tracing::error!("apply modes override failed: {e:#}");
                    self.add_to_history(history_cell::new_error_event(format!(
                        "Failed to apply modes: {e}"
                    )));
                }
                return;
            }
            // 打开模式面板（Alt+P）
            KeyEvent {
                code: KeyCode::Char('p'),
                modifiers: KeyModifiers::ALT,
                kind: KeyEventKind::Press,
                ..
            } => {
                self.open_modes_panel();
                return;
            }
            // 查看当前 <user_instructions>（Ctrl+U）
            KeyEvent {
                code: KeyCode::Char('u'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            } => {
                let text = self
                    .current_user_instructions
                    .clone()
                    .or_else(|| self.base_user_instructions.clone())
                    .unwrap_or_else(|| self.compose_fallback_user_instructions_with_project_doc());
                self.app_event_tx
                    .send(crate::app_event::AppEvent::ShowUserInstructions(text));
                return;
            }
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: crossterm::event::KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            } => {
                self.on_ctrl_c();
                return;
            }
            KeyEvent {
                code: KeyCode::Char('v'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            } => {
                if let Ok((path, info)) = paste_image_to_temp_png() {
                    self.attach_image(path, info.width, info.height, info.encoded_format.label());
                }
                return;
            }
            other if other.kind == KeyEventKind::Press => {
                self.bottom_pane.clear_ctrl_c_quit_hint();
            }
            _ => {}
        }

        match key_event {
            // Alt+B 打开模式条（ModeBar）进行就地编辑
            KeyEvent {
                code: KeyCode::Char('b'),
                modifiers: KeyModifiers::ALT,
                kind: KeyEventKind::Press,
                ..
            } => {
                if self.bottom_pane.is_normal_backtrack_mode() {
                    self.open_mode_bar();
                }
            }
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::ALT,
                kind: KeyEventKind::Press,
                ..
            } if !self.queued_user_messages.is_empty() => {
                // Prefer the most recently queued item.
                if let Some(user_message) = self.queued_user_messages.pop_back() {
                    self.bottom_pane.set_composer_text(user_message.text);
                    self.refresh_queued_user_messages();
                    self.request_redraw();
                }
            }
            _ => {
                match self.bottom_pane.handle_key_event(key_event) {
                    InputResult::Submitted(text) => {
                        // If a task is running, queue the user input to be sent after the turn completes.
                        let user_message = UserMessage {
                            text,
                            image_paths: self.bottom_pane.take_recent_submission_images(),
                        };
                        if self.bottom_pane.is_task_running() {
                            self.queued_user_messages.push_back(user_message);
                            self.refresh_queued_user_messages();
                        } else {
                            self.submit_user_message(user_message);
                        }
                    }
                    InputResult::Command(cmd) => {
                        self.dispatch_command(cmd);
                    }
                    InputResult::None => {}
                }
            }
        }
    }

    pub(crate) fn attach_image(
        &mut self,
        path: PathBuf,
        width: u32,
        height: u32,
        format_label: &str,
    ) {
        tracing::info!(
            "attach_image path={path:?} width={width} height={height} format={format_label}",
        );
        self.bottom_pane
            .attach_image(path, width, height, format_label);
        self.request_redraw();
    }

    /// Scan modes from disk and override <user_instructions> with a rendered block.
    fn apply_modes_override_from_disk(&mut self, base: &str) -> anyhow::Result<()> {
        use codex_modes::EnabledMode;
        use codex_modes::IndexMap as IMap;
        use codex_modes::ModeKind;
        use codex_modes::render_user_instructions;
        use codex_modes::scan_modes;
        let defs = scan_modes(&self.config.cwd, Some(&self.config.codex_home))?;
        if defs.is_empty() {
            self.add_to_history(history_cell::new_error_event(
                "No modes found under .codex/modes or $CODEX_HOME/modes".to_string(),
            ));
            return Ok(());
        }
        // Build enabled list: only persistent modes with all required vars having defaults.
        let mut enabled: Vec<EnabledMode> = Vec::new();
        for def in &defs {
            if def.kind != ModeKind::Persistent {
                continue;
            }
            if !def.default_enabled {
                continue;
            }
            let mut ok = true;
            let mut vars: IMap<&str, Option<String>> = IMap::new();
            for v in &def.variables {
                if v.required && v.default.is_none() {
                    ok = false;
                    break;
                }
                vars.insert(v.name.as_str(), None); // None => UseDefault
            }
            if ok {
                enabled.push(EnabledMode {
                    id: &def.id,
                    display_name: def.display_name.as_deref(),
                    scope: &def.scope,
                    variables: vars,
                });
            }
        }
        if enabled.is_empty() {
            self.add_to_history(history_cell::new_error_event(
                "No default-enabled persistent modes with satisfiable defaults to apply"
                    .to_string(),
            ));
            return Ok(());
        }
        let mut state = PersistentModeState::default();
        for em in &enabled {
            let id = em.id.to_string();
            if state.enabled.insert(id.clone()) {
                state.enable_order.push(id);
            }
        }
        self.persistent_mode_state = state.clone();
        let rendered = render_user_instructions(base, &enabled, &defs)?;
        self.submit_op(Op::OverrideTurnContext {
            cwd: None,
            approval_policy: None,
            sandbox_policy: None,
            model: None,
            effort: None,
            summary: None,
            user_instructions: Some(rendered),
        });
        // Track current applied content for equivalence checks
        let rendered_now = render_user_instructions(base, &enabled, &defs)?;
        self.current_user_instructions = Some(rendered_now);
        // 提示与摘要：改用库层统一文案
        let labels = codex_modes::enabled_labels(&enabled);
        let message = codex_modes::applied_message(enabled.len());
        self.add_to_history(history_cell::new_info_event(
            message,
            if labels.is_empty() {
                None
            } else {
                Some(labels.clone())
            },
        ));
        let summary = codex_modes::format_mode_summary(&labels);
        self.set_mode_summary(summary);
        Ok(())
    }

    /// 打开模式面板（扫描与交互）。
    fn open_modes_panel(&mut self) {
        // 优先：通用工厂
        if !self.ui_view_factories.is_empty() {
            let base = self
                .base_user_instructions
                .clone()
                .unwrap_or_else(|| self.compose_fallback_user_instructions_with_project_doc());
            let ui_tx1 = self.ui_tasks_tx.clone();
            let ui_tx2 = self.ui_tasks_tx.clone();
            let ctx = ModeUiContext::new(
                self.app_event_tx.clone(),
                self.frame_requester.clone(),
                self.config.cwd.clone(),
                self.config.codex_home.clone(),
                base,
                self.current_user_instructions.clone(),
                self.persistent_mode_state.clone(),
                std::sync::Arc::new(move |labels: String| {
                    let payload = labels;
                    let _ = ui_tx1.send(Box::new(move |cw| {
                        let t = payload.trim();
                        if t.is_empty() {
                            cw.set_mode_summary(None);
                        } else {
                            cw.set_mode_summary(Some(format!("Mode: {t}")));
                        }
                    }));
                }),
                std::sync::Arc::new(move |state: PersistentModeState| {
                    let st = state;
                    let _ = ui_tx2.send(Box::new(move |cw| {
                        cw.set_persistent_mode_state(st.clone());
                    }));
                }),
            );
            for f in &self.ui_view_factories {
                if let Some(view) = f.make_view(UiViewKind::ModePanel, &ctx) {
                    self.bottom_pane.show_custom_view(view);
                    self.request_redraw();
                    return;
                }
            }
        }
        // 未提供任何工厂时给出提示信息，避免静默失败（理论上不会触发，默认工厂已注册）。
        self.add_to_history(history_cell::new_error_event(
            "No UI view factory registered (ModePanel)".to_string(),
        ));
    }

    /// 打开底栏 ModeBar（摘要交互：左右选择、空格启用/禁用、Esc 退出）。
    pub(crate) fn open_mode_bar(&mut self) {
        // 优先：通用工厂
        if !self.ui_view_factories.is_empty() {
            let base = self
                .base_user_instructions
                .clone()
                .unwrap_or_else(|| self.compose_fallback_user_instructions_with_project_doc());
            let ui_tx1 = self.ui_tasks_tx.clone();
            let ui_tx2 = self.ui_tasks_tx.clone();
            let ctx = ModeUiContext::new(
                self.app_event_tx.clone(),
                self.frame_requester.clone(),
                self.config.cwd.clone(),
                self.config.codex_home.clone(),
                base,
                self.current_user_instructions.clone(),
                self.persistent_mode_state.clone(),
                std::sync::Arc::new(move |labels: String| {
                    let payload = labels;
                    let _ = ui_tx1.send(Box::new(move |cw| {
                        let t = payload.trim();
                        if t.is_empty() {
                            cw.set_mode_summary(None);
                        } else {
                            cw.set_mode_summary(Some(format!("Mode: {t}")));
                        }
                    }));
                }),
                std::sync::Arc::new(move |state: PersistentModeState| {
                    let st = state;
                    let _ = ui_tx2.send(Box::new(move |cw| {
                        cw.set_persistent_mode_state(st.clone());
                    }));
                }),
            );
            for f in &self.ui_view_factories {
                if let Some(view) = f.make_view(UiViewKind::ModeBar, &ctx) {
                    self.bottom_pane.show_custom_view(view);
                    self.request_redraw();
                    return;
                }
            }
        }
        // 未提供任何工厂时给出提示信息，避免静默失败（理论上不会触发，默认工厂已注册）。
        self.add_to_history(history_cell::new_error_event(
            "No UI view factory registered (ModeBar)".to_string(),
        ));
    }

    fn dispatch_command(&mut self, cmd: SlashCommand) {
        if !cmd.available_during_task() && self.bottom_pane.is_task_running() {
            let message = format!(
                "'/{}' is disabled while a task is in progress.",
                cmd.command()
            );
            self.add_to_history(history_cell::new_error_event(message));
            self.request_redraw();
            return;
        }
        match cmd {
            SlashCommand::New => {
                self.app_event_tx.send(AppEvent::NewSession);
            }
            SlashCommand::Init => {
                const INIT_PROMPT: &str = include_str!("../prompt_for_init_command.md");
                self.submit_text_message(INIT_PROMPT.to_string());
            }
            SlashCommand::Compact => {
                self.clear_token_usage();
                self.app_event_tx.send(AppEvent::CodexOp(Op::Compact));
            }
            SlashCommand::Review => {
                self.open_review_popup();
            }
            SlashCommand::Model => {
                self.open_model_popup();
            }
            SlashCommand::Approvals => {
                self.open_approvals_popup();
            }
            SlashCommand::Quit => {
                self.app_event_tx.send(AppEvent::ExitRequest);
            }
            SlashCommand::Logout => {
                if let Err(e) = codex_core::auth::logout(&self.config.codex_home) {
                    tracing::error!("failed to logout: {e}");
                }
                self.app_event_tx.send(AppEvent::ExitRequest);
            }
            SlashCommand::Undo => {
                self.undo_last_snapshot();
            }
            SlashCommand::Diff => {
                self.add_diff_in_progress();
                let tx = self.app_event_tx.clone();
                tokio::spawn(async move {
                    let text = match get_git_diff().await {
                        Ok((is_git_repo, diff_text)) => {
                            if is_git_repo {
                                diff_text
                            } else {
                                "`/diff` — _not inside a git repository_".to_string()
                            }
                        }
                        Err(e) => format!("Failed to compute diff: {e}"),
                    };
                    tx.send(AppEvent::DiffResult(text));
                });
            }
            SlashCommand::Mention => {
                self.insert_str("@");
            }
            SlashCommand::Status => {
                self.add_status_output();
            }
            SlashCommand::Mcp => {
                self.add_mcp_output();
            }
            #[cfg(debug_assertions)]
            SlashCommand::TestApproval => {
                use codex_core::protocol::EventMsg;
                use std::collections::HashMap;

                use codex_core::protocol::ApplyPatchApprovalRequestEvent;
                use codex_core::protocol::FileChange;

                self.app_event_tx.send(AppEvent::CodexEvent(Event {
                    id: "1".to_string(),
                    // msg: EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
                    //     call_id: "1".to_string(),
                    //     command: vec!["git".into(), "apply".into()],
                    //     cwd: self.config.cwd.clone(),
                    //     reason: Some("test".to_string()),
                    // }),
                    msg: EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
                        call_id: "1".to_string(),
                        changes: HashMap::from([
                            (
                                PathBuf::from("/tmp/test.txt"),
                                FileChange::Add {
                                    content: "test".to_string(),
                                },
                            ),
                            (
                                PathBuf::from("/tmp/test2.txt"),
                                FileChange::Update {
                                    unified_diff: "+test\n-test2".to_string(),
                                    move_path: None,
                                },
                            ),
                        ]),
                        reason: None,
                        grant_root: Some(PathBuf::from("/tmp")),
                    }),
                }));
            }
        }
    }

    pub(crate) fn handle_paste(&mut self, text: String) {
        self.bottom_pane.handle_paste(text);
    }

    // Returns true if caller should skip rendering this frame (a future frame is scheduled).
    pub(crate) fn handle_paste_burst_tick(&mut self, frame_requester: FrameRequester) -> bool {
        if self.bottom_pane.flush_paste_burst_if_due() {
            // A paste just flushed; request an immediate redraw and skip this frame.
            self.request_redraw();
            true
        } else if self.bottom_pane.is_in_paste_burst() {
            // While capturing a burst, schedule a follow-up tick and skip this frame
            // to avoid redundant renders between ticks.
            frame_requester.schedule_frame_in(
                crate::bottom_pane::ChatComposer::recommended_paste_flush_delay(),
            );
            true
        } else {
            false
        }
    }

    fn flush_active_cell(&mut self) {
        if let Some(active) = self.active_cell.take() {
            self.app_event_tx.send(AppEvent::InsertHistoryCell(active));
        }
    }

    fn add_to_history(&mut self, cell: impl HistoryCell + 'static) {
        self.add_boxed_history(Box::new(cell));
    }

    fn add_boxed_history(&mut self, cell: Box<dyn HistoryCell>) {
        if !cell.display_lines(u16::MAX).is_empty() {
            // Only break exec grouping if the cell renders visible lines.
            self.flush_active_cell();
        }
        self.app_event_tx.send(AppEvent::InsertHistoryCell(cell));
    }

    fn submit_user_message(&mut self, user_message: UserMessage) {
        let UserMessage { text, image_paths } = user_message;
        if text.is_empty() && image_paths.is_empty() {
            return;
        }

        self.capture_ghost_snapshot();

        let mut items: Vec<InputItem> = Vec::new();

        if !text.is_empty() {
            items.push(InputItem::Text { text: text.clone() });
        }

        for path in image_paths {
            items.push(InputItem::LocalImage { path });
        }

        self.codex_op_tx
            .send(Op::UserInput { items })
            .unwrap_or_else(|e| {
                tracing::error!("failed to send message: {e}");
            });

        // Persist the text to cross-session message history.
        if !text.is_empty() {
            self.codex_op_tx
                .send(Op::AddToHistory { text: text.clone() })
                .unwrap_or_else(|e| {
                    tracing::error!("failed to send AddHistory op: {e}");
                });
        }

        // Only show the text portion in conversation history.
        if !text.is_empty() {
            self.add_to_history(history_cell::new_user_prompt(text));
        }
    }

    fn capture_ghost_snapshot(&mut self) {
        if self.ghost_snapshots_disabled {
            return;
        }

        let options = CreateGhostCommitOptions::new(&self.config.cwd);
        match create_ghost_commit(&options) {
            Ok(commit) => {
                self.ghost_snapshots.push(commit);
                if self.ghost_snapshots.len() > MAX_TRACKED_GHOST_COMMITS {
                    self.ghost_snapshots.remove(0);
                }
            }
            Err(err) => {
                self.ghost_snapshots_disabled = true;
                let (message, hint) = match &err {
                    GitToolingError::NotAGitRepository { .. } => (
                        "Snapshots disabled: current directory is not a Git repository."
                            .to_string(),
                        None,
                    ),
                    _ => (
                        format!("Snapshots disabled after error: {err}"),
                        Some(
                            "Restart Codex after resolving the issue to re-enable snapshots."
                                .to_string(),
                        ),
                    ),
                };
                self.add_info_message(message, hint);
                tracing::warn!("failed to create ghost snapshot: {err}");
            }
        }
    }

    fn undo_last_snapshot(&mut self) {
        let Some(commit) = self.ghost_snapshots.pop() else {
            self.add_info_message("No snapshot available to undo.".to_string(), None);
            return;
        };

        if let Err(err) = restore_ghost_commit(&self.config.cwd, &commit) {
            self.add_error_message(format!("Failed to restore snapshot: {err}"));
            self.ghost_snapshots.push(commit);
            return;
        }

        let short_id: String = commit.id().chars().take(8).collect();
        self.add_info_message(format!("Restored workspace to snapshot {short_id}"), None);
    }

    /// Replay a subset of initial events into the UI to seed the transcript when
    /// resuming an existing session. This approximates the live event flow and
    /// is intentionally conservative: only safe-to-replay items are rendered to
    /// avoid triggering side effects. Event ids are passed as `None` to
    /// distinguish replayed events from live ones.
    fn replay_initial_messages(&mut self, events: Vec<EventMsg>) {
        for msg in events {
            if matches!(msg, EventMsg::SessionConfigured(_)) {
                continue;
            }
            // `id: None` indicates a synthetic/fake id coming from replay.
            self.dispatch_event_msg(None, msg, true);
        }
    }

    pub(crate) fn handle_codex_event(&mut self, event: Event) {
        let Event { id, msg } = event;
        self.dispatch_event_msg(Some(id), msg, false);
    }

    /// Dispatch a protocol `EventMsg` to the appropriate handler.
    ///
    /// `id` is `Some` for live events and `None` for replayed events from
    /// `replay_initial_messages()`. Callers should treat `None` as a "fake" id
    /// that must not be used to correlate follow-up actions.
    fn dispatch_event_msg(&mut self, id: Option<String>, msg: EventMsg, from_replay: bool) {
        match msg {
            EventMsg::AgentMessageDelta(_)
            | EventMsg::AgentReasoningDelta(_)
            | EventMsg::ExecCommandOutputDelta(_) => {}
            _ => {
                tracing::trace!("handle_codex_event: {:?}", msg);
            }
        }

        match msg {
            EventMsg::SessionConfigured(e) => self.on_session_configured(e),
            EventMsg::AgentMessage(AgentMessageEvent { message }) => self.on_agent_message(message),
            EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta }) => {
                self.on_agent_message_delta(delta)
            }
            EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent { delta })
            | EventMsg::AgentReasoningRawContentDelta(AgentReasoningRawContentDeltaEvent {
                delta,
            }) => self.on_agent_reasoning_delta(delta),
            EventMsg::AgentReasoning(AgentReasoningEvent { .. }) => self.on_agent_reasoning_final(),
            EventMsg::AgentReasoningRawContent(AgentReasoningRawContentEvent { text }) => {
                self.on_agent_reasoning_delta(text);
                self.on_agent_reasoning_final()
            }
            EventMsg::AgentReasoningSectionBreak(_) => self.on_reasoning_section_break(),
            EventMsg::TaskStarted(_) => self.on_task_started(),
            EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message }) => {
                self.on_task_complete(last_agent_message)
            }
            EventMsg::TokenCount(ev) => self.set_token_info(ev.info),
            EventMsg::Error(ErrorEvent { message }) => self.on_error(message),
            EventMsg::TurnAborted(ev) => match ev.reason {
                TurnAbortReason::Interrupted => {
                    self.on_interrupted_turn(ev.reason);
                }
                TurnAbortReason::Replaced => {
                    self.on_error("Turn aborted: replaced by a new task".to_owned())
                }
                TurnAbortReason::ReviewEnded => {
                    self.on_interrupted_turn(ev.reason);
                }
            },
            EventMsg::PlanUpdate(update) => self.on_plan_update(update),
            EventMsg::ExecApprovalRequest(ev) => {
                // For replayed events, synthesize an empty id (these should not occur).
                self.on_exec_approval_request(id.unwrap_or_default(), ev)
            }
            EventMsg::ApplyPatchApprovalRequest(ev) => {
                self.on_apply_patch_approval_request(id.unwrap_or_default(), ev)
            }
            EventMsg::ExecCommandBegin(ev) => self.on_exec_command_begin(ev),
            EventMsg::ExecCommandOutputDelta(delta) => self.on_exec_command_output_delta(delta),
            EventMsg::PatchApplyBegin(ev) => self.on_patch_apply_begin(ev),
            EventMsg::PatchApplyEnd(ev) => self.on_patch_apply_end(ev),
            EventMsg::ExecCommandEnd(ev) => self.on_exec_command_end(ev),
            EventMsg::McpToolCallBegin(ev) => self.on_mcp_tool_call_begin(ev),
            EventMsg::McpToolCallEnd(ev) => self.on_mcp_tool_call_end(ev),
            EventMsg::WebSearchBegin(ev) => self.on_web_search_begin(ev),
            EventMsg::WebSearchEnd(ev) => self.on_web_search_end(ev),
            EventMsg::GetHistoryEntryResponse(ev) => self.on_get_history_entry_response(ev),
            EventMsg::McpListToolsResponse(ev) => self.on_list_mcp_tools(ev),
            EventMsg::ListCustomPromptsResponse(ev) => self.on_list_custom_prompts(ev),
            EventMsg::ShutdownComplete => self.on_shutdown_complete(),
            EventMsg::TurnDiff(TurnDiffEvent { unified_diff }) => self.on_turn_diff(unified_diff),
            EventMsg::BackgroundEvent(BackgroundEventEvent { message }) => {
                self.on_background_event(message)
            }
            EventMsg::StreamError(StreamErrorEvent { message }) => self.on_stream_error(message),
            EventMsg::UserMessage(ev) => {
                if from_replay {
                    self.on_user_message_event(ev);
                }
            }
            EventMsg::ConversationPath(ev) => {
                self.app_event_tx
                    .send(crate::app_event::AppEvent::ConversationHistory(ev));
            }
            EventMsg::EnteredReviewMode(review_request) => {
                self.on_entered_review_mode(review_request)
            }
            EventMsg::ExitedReviewMode(review) => self.on_exited_review_mode(review),
            EventMsg::ViewImageToolCall(_ev) => {}
        }
    }

    fn on_entered_review_mode(&mut self, review: ReviewRequest) {
        // Enter review mode and emit a concise banner
        self.is_review_mode = true;
        let banner = format!(">> Code review started: {} <<", review.user_facing_hint);
        self.add_to_history(history_cell::new_review_status_line(banner));
        self.request_redraw();
    }

    fn on_exited_review_mode(&mut self, review: ExitedReviewModeEvent) {
        // Leave review mode; if output is present, flush pending stream + show results.
        if let Some(output) = review.review_output {
            self.flush_answer_stream_with_separator();
            self.flush_interrupt_queue();
            self.flush_active_cell();

            if output.findings.is_empty() {
                let explanation = output.overall_explanation.trim().to_string();
                if explanation.is_empty() {
                    tracing::error!("Reviewer failed to output a response.");
                    self.add_to_history(history_cell::new_error_event(
                        "Reviewer failed to output a response.".to_owned(),
                    ));
                } else {
                    // Show explanation when there are no structured findings.
                    let mut rendered: Vec<ratatui::text::Line<'static>> = vec!["".into()];
                    append_markdown(&explanation, None, &mut rendered, &self.config);
                    let body_cell = AgentMessageCell::new(rendered, false);
                    self.app_event_tx
                        .send(AppEvent::InsertHistoryCell(Box::new(body_cell)));
                }
            } else {
                let message_text =
                    codex_core::review_format::format_review_findings_block(&output.findings, None);
                let mut message_lines: Vec<ratatui::text::Line<'static>> = Vec::new();
                append_markdown(&message_text, None, &mut message_lines, &self.config);
                let body_cell = AgentMessageCell::new(message_lines, true);
                self.app_event_tx
                    .send(AppEvent::InsertHistoryCell(Box::new(body_cell)));
            }
        }

        self.is_review_mode = false;
        // Append a finishing banner at the end of this turn.
        self.add_to_history(history_cell::new_review_status_line(
            "<< Code review finished >>".to_string(),
        ));
        self.request_redraw();
    }

    fn on_user_message_event(&mut self, event: UserMessageEvent) {
        match event.kind {
            Some(InputMessageKind::EnvironmentContext)
            | Some(InputMessageKind::UserInstructions) => {
                // Capture baseline <user_instructions> once, but do not render.
                if matches!(event.kind, Some(InputMessageKind::UserInstructions))
                    && self.base_user_instructions.is_none()
                {
                    self.base_user_instructions = extract_base_user_instructions(&event.message);
                }
            }
            Some(InputMessageKind::Plain) | None => {
                let message = event.message.trim();
                if !message.is_empty() {
                    self.add_to_history(history_cell::new_user_prompt(message.to_string()));
                }
            }
        }
    }

    fn request_redraw(&mut self) {
        self.frame_requester.schedule_frame();
    }

    fn notify(&mut self, notification: Notification) {
        if !notification.allowed_for(&self.config.tui_notifications) {
            return;
        }
        self.pending_notification = Some(notification);
        self.request_redraw();
    }

    pub(crate) fn maybe_post_pending_notification(&mut self, tui: &mut crate::tui::Tui) {
        if let Some(notif) = self.pending_notification.take() {
            tui.notify(notif.display());
        }
    }

    /// Mark the active cell as failed (✗) and flush it into history.
    fn finalize_active_cell_as_failed(&mut self) {
        if let Some(mut cell) = self.active_cell.take() {
            // Insert finalized cell into history and keep grouping consistent.
            if let Some(exec) = cell.as_any_mut().downcast_mut::<ExecCell>() {
                exec.mark_failed();
            } else if let Some(tool) = cell.as_any_mut().downcast_mut::<McpToolCallCell>() {
                tool.mark_failed();
            }
            self.add_boxed_history(cell);
        }
    }

    // If idle and there are queued inputs, submit exactly one to start the next turn.
    fn maybe_send_next_queued_input(&mut self) {
        if self.bottom_pane.is_task_running() {
            return;
        }
        if let Some(user_message) = self.queued_user_messages.pop_front() {
            self.submit_user_message(user_message);
        }
        // Update the list to reflect the remaining queued messages (if any).
        self.refresh_queued_user_messages();
    }

    /// Rebuild and update the queued user messages from the current queue.
    fn refresh_queued_user_messages(&mut self) {
        let messages: Vec<String> = self
            .queued_user_messages
            .iter()
            .map(|m| m.text.clone())
            .collect();
        self.bottom_pane.set_queued_user_messages(messages);
    }

    pub(crate) fn add_diff_in_progress(&mut self) {
        self.request_redraw();
    }

    pub(crate) fn on_diff_complete(&mut self) {
        self.request_redraw();
    }

    pub(crate) fn add_status_output(&mut self) {
        let default_usage = TokenUsage::default();
        let (total_usage, context_usage) = if let Some(ti) = &self.token_info {
            (&ti.total_token_usage, Some(&ti.last_token_usage))
        } else {
            (&default_usage, Some(&default_usage))
        };
        self.add_to_history(crate::status::new_status_output(
            &self.config,
            total_usage,
            context_usage,
            &self.conversation_id,
            self.rate_limit_snapshot.as_ref(),
        ));
    }

    /// Open a popup to choose the model (stage 1). After selecting a model,
    /// a second popup is shown to choose the reasoning effort.
    pub(crate) fn open_model_popup(&mut self) {
        let current_model = self.config.model.clone();
        let auth_mode = self.auth_manager.auth().map(|auth| auth.mode);
        let presets: Vec<ModelPreset> = builtin_model_presets(auth_mode);

        let mut grouped: Vec<(&str, Vec<ModelPreset>)> = Vec::new();
        for preset in presets.into_iter() {
            if let Some((_, entries)) = grouped.iter_mut().find(|(model, _)| *model == preset.model)
            {
                entries.push(preset);
            } else {
                grouped.push((preset.model, vec![preset]));
            }
        }

        let mut items: Vec<SelectionItem> = Vec::new();
        for (model_slug, entries) in grouped.into_iter() {
            let name = model_slug.to_string();
            let description = Self::model_description_for(model_slug)
                .map(std::string::ToString::to_string)
                .or_else(|| {
                    entries
                        .iter()
                        .find(|preset| !preset.description.is_empty())
                        .map(|preset| preset.description.to_string())
                })
                .or_else(|| entries.first().map(|preset| preset.description.to_string()));
            let is_current = model_slug == current_model;
            let model_slug_string = model_slug.to_string();
            let presets_for_model = entries.clone();
            let actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
                tx.send(AppEvent::OpenReasoningPopup {
                    model: model_slug_string.clone(),
                    presets: presets_for_model.clone(),
                });
            })];
            items.push(SelectionItem {
                name,
                description,
                is_current,
                actions,
                dismiss_on_select: false,
                ..Default::default()
            });
        }

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Select Model and Effort".to_string()),
            subtitle: Some("Switch the model for this and future Codex CLI sessions".to_string()),
            footer_hint: Some("Press enter to select reasoning effort, or esc to dismiss.".into()),
            items,
            ..Default::default()
        });
    }

    /// Open a popup to choose the reasoning effort (stage 2) for the given model.
    pub(crate) fn open_reasoning_popup(&mut self, model_slug: String, presets: Vec<ModelPreset>) {
        let default_effort = ReasoningEffortConfig::default();

        let has_none_choice = presets.iter().any(|preset| preset.effort.is_none());
        struct EffortChoice {
            stored: Option<ReasoningEffortConfig>,
            display: ReasoningEffortConfig,
        }
        let mut choices: Vec<EffortChoice> = Vec::new();
        for effort in ReasoningEffortConfig::iter() {
            if presets.iter().any(|preset| preset.effort == Some(effort)) {
                choices.push(EffortChoice {
                    stored: Some(effort),
                    display: effort,
                });
            }
            if has_none_choice && default_effort == effort {
                choices.push(EffortChoice {
                    stored: None,
                    display: effort,
                });
            }
        }
        if choices.is_empty() {
            choices.push(EffortChoice {
                stored: Some(default_effort),
                display: default_effort,
            });
        }

        let default_choice: Option<ReasoningEffortConfig> = if has_none_choice {
            None
        } else if choices
            .iter()
            .any(|choice| choice.stored == Some(default_effort))
        {
            Some(default_effort)
        } else {
            choices
                .iter()
                .find_map(|choice| choice.stored)
                .or(Some(default_effort))
        };

        let is_current_model = self.config.model == model_slug;
        let highlight_choice = if is_current_model {
            self.config.model_reasoning_effort
        } else {
            default_choice
        };

        let mut items: Vec<SelectionItem> = Vec::new();
        for choice in choices.iter() {
            let effort = choice.display;
            let mut effort_label = effort.to_string();
            if let Some(first) = effort_label.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            if choice.stored == default_choice {
                effort_label.push_str(" (default)");
            }

            let description = presets
                .iter()
                .find(|preset| preset.effort == choice.stored && !preset.description.is_empty())
                .map(|preset| preset.description.to_string())
                .or_else(|| {
                    presets
                        .iter()
                        .find(|preset| preset.effort == choice.stored)
                        .map(|preset| preset.description.to_string())
                });

            let model_for_action = model_slug.clone();
            let effort_for_action = choice.stored;
            let actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
                tx.send(AppEvent::CodexOp(Op::OverrideTurnContext {
                    cwd: None,
                    approval_policy: None,
                    sandbox_policy: None,
                    model: Some(model_for_action.clone()),
                    effort: Some(effort_for_action),
                    summary: None,
                    user_instructions: None,
                }));
                tx.send(AppEvent::UpdateModel(model_for_action.clone()));
                tx.send(AppEvent::UpdateReasoningEffort(effort_for_action));
                tx.send(AppEvent::PersistModelSelection {
                    model: model_for_action.clone(),
                    effort: effort_for_action,
                });
                tracing::info!(
                    "Selected model: {}, Selected effort: {}",
                    model_for_action,
                    effort_for_action
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "default".to_string())
                );
            })];

            items.push(SelectionItem {
                name: effort_label,
                description,
                is_current: is_current_model && choice.stored == highlight_choice,
                actions,
                dismiss_on_select: true,
                ..Default::default()
            });
        }

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Select Reasoning Level".to_string()),
            subtitle: Some(format!("Reasoning for model {model_slug}")),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            ..Default::default()
        });
    }

    /// Open a popup to choose the approvals mode (ask for approval policy + sandbox policy).
    pub(crate) fn open_approvals_popup(&mut self) {
        let current_approval = self.config.approval_policy;
        let current_sandbox = self.config.sandbox_policy.clone();
        let mut items: Vec<SelectionItem> = Vec::new();
        let presets: Vec<ApprovalPreset> = builtin_approval_presets();
        for preset in presets.into_iter() {
            let is_current =
                current_approval == preset.approval && current_sandbox == preset.sandbox;
            let approval = preset.approval;
            let sandbox = preset.sandbox.clone();
            let name = preset.label.to_string();
            let description = Some(preset.description.to_string());
            let actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
                tx.send(AppEvent::CodexOp(Op::OverrideTurnContext {
                    cwd: None,
                    approval_policy: Some(approval),
                    sandbox_policy: Some(sandbox.clone()),
                    model: None,
                    effort: None,
                    summary: None,
                    user_instructions: None,
                }));
                tx.send(AppEvent::UpdateAskForApprovalPolicy(approval));
                tx.send(AppEvent::UpdateSandboxPolicy(sandbox.clone()));
            })];
            items.push(SelectionItem {
                name,
                description,
                is_current,
                actions,
                dismiss_on_select: true,
                search_value: None,
                display_shortcut: None,
            });
        }

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Select Approval Mode".to_string()),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            ..Default::default()
        });
    }

    // !Modify: Deduplicated config setter helpers after merge
    /// Set the approval policy in the widget's config copy.
    pub(crate) fn set_approval_policy(&mut self, policy: AskForApproval) {
        self.config.approval_policy = policy;
    }

    /// Set the sandbox policy in the widget's config copy.
    pub(crate) fn set_sandbox_policy(&mut self, policy: SandboxPolicy) {
        self.config.sandbox_policy = policy;
    }

    /// Set the reasoning effort in the widget's config copy.
    pub(crate) fn set_reasoning_effort(&mut self, effort: Option<ReasoningEffortConfig>) {
        self.config.model_reasoning_effort = effort;
    }

    /// Set the model in the widget's config copy.
    pub(crate) fn set_model(&mut self, model: &str) {
        self.session_header.set_model(model);
        self.config.model = model.to_string();
    }

    pub(crate) fn add_info_message(&mut self, message: String, hint: Option<String>) {
        self.add_to_history(history_cell::new_info_event(message, hint));
        self.request_redraw();
    }

    pub(crate) fn add_error_message(&mut self, message: String) {
        self.add_to_history(history_cell::new_error_event(message));
        self.request_redraw();
    }

    pub(crate) fn add_mcp_output(&mut self) {
        if self.config.mcp_servers.is_empty() {
            self.add_to_history(history_cell::empty_mcp_output());
        } else {
            self.submit_op(Op::ListMcpTools);
        }
    }

    /// Forward file-search results to the bottom pane.
    pub(crate) fn apply_file_search_result(&mut self, query: String, matches: Vec<FileMatch>) {
        self.bottom_pane.on_file_search_result(query, matches);
    }

    /// Handle Ctrl-C key press.
    fn on_ctrl_c(&mut self) {
        if self.bottom_pane.on_ctrl_c() == CancellationEvent::Handled {
            return;
        }

        if self.bottom_pane.is_task_running() {
            self.bottom_pane.show_ctrl_c_quit_hint();
            self.submit_op(Op::Interrupt);
            return;
        }

        self.submit_op(Op::Shutdown);
    }

    pub(crate) fn composer_is_empty(&self) -> bool {
        self.bottom_pane.composer_is_empty()
    }

    /// True when the UI is in the regular composer state with no running task,
    /// no modal overlay (e.g. approvals or status indicator), and no composer popups.
    /// In this state Esc-Esc backtracking is enabled.
    pub(crate) fn is_normal_backtrack_mode(&self) -> bool {
        self.bottom_pane.is_normal_backtrack_mode()
    }

    pub(crate) fn insert_str(&mut self, text: &str) {
        self.bottom_pane.insert_str(text);
    }

    /// Replace the composer content with the provided text and reset cursor.
    pub(crate) fn set_composer_text(&mut self, text: String) {
        self.bottom_pane.set_composer_text(text);
    }

    pub(crate) fn clear_composer_text(&mut self) {
        self.bottom_pane.set_composer_text(String::new());
    }

    // !Modify: Backtrack picker params (searchable, localized hints)
    pub(crate) fn open_backtrack_picker(&mut self, items: Vec<SelectionItem>) {
        use crate::bottom_pane::SelectionViewParams;
        let params = SelectionViewParams {
            title: Some("Backtrack to User Messages".to_string()),
            subtitle: Some(
                "Only list user messages; selecting one will revert to that node (dropping subsequent context)"
                    .to_string(),
            ),
            footer_hint: Some(Line::from("↑/↓ 选择 · Enter 回退并编辑 · Esc 取消")),
            items,
            is_searchable: true,
            search_placeholder: Some("输入关键字过滤".to_string()),
            header: Box::new(()),
        };
        self.bottom_pane.show_selection_view(params);
    }

    pub(crate) fn show_esc_backtrack_hint(&mut self) {
        self.bottom_pane.show_esc_backtrack_hint();
    }

    pub(crate) fn clear_esc_backtrack_hint(&mut self) {
        self.bottom_pane.clear_esc_backtrack_hint();
    }

    pub(crate) fn show_esc_clear_hint_for(&mut self, dur: std::time::Duration) {
        self.bottom_pane.show_esc_clear_hint_for(dur);
    }

    pub(crate) fn clear_esc_clear_hint(&mut self) {
        self.bottom_pane.clear_esc_clear_hint();
    }
    /// Forward an `Op` directly to codex.
    pub(crate) fn submit_op(&self, op: Op) {
        // Record outbound operation for session replay fidelity.
        crate::session_log::log_outbound_op(&op);
        if let Err(e) = self.codex_op_tx.send(op) {
            tracing::error!("failed to submit op: {e}");
        }
    }

    fn on_list_mcp_tools(&mut self, ev: McpListToolsResponseEvent) {
        self.add_to_history(history_cell::new_mcp_tools_output(&self.config, ev.tools));
    }

    fn on_list_custom_prompts(&mut self, ev: ListCustomPromptsResponseEvent) {
        let len = ev.custom_prompts.len();
        debug!("received {len} custom prompts");
        // Forward to bottom pane so the slash popup can show them now.
        self.bottom_pane.set_custom_prompts(ev.custom_prompts);
    }

    pub(crate) fn open_review_popup(&mut self) {
        let mut items: Vec<SelectionItem> = Vec::new();

        items.push(SelectionItem {
            name: "Review uncommitted changes".to_string(),
            description: None,
            is_current: false,
            actions: vec![Box::new(
                move |tx: &AppEventSender| {
                    tx.send(AppEvent::CodexOp(Op::Review {
                        review_request: ReviewRequest {
                            prompt: "Review the current code changes (staged, unstaged, and untracked files) and provide prioritized findings.".to_string(),
                            user_facing_hint: "current changes".to_string(),
                        },
                    }));
                },
            )],
            dismiss_on_select: true,
            search_value: None,
            display_shortcut: None,
        });

        items.push(SelectionItem {
            name: "Review a commit".to_string(),
            description: None,
            is_current: false,
            actions: vec![Box::new({
                let cwd = self.config.cwd.clone();
                move |tx| {
                    tx.send(AppEvent::OpenReviewCommitPicker(cwd.clone()));
                }
            })],
            dismiss_on_select: false,
            search_value: None,
            display_shortcut: None,
        });

        items.push(SelectionItem {
            name: "Review against a base branch".to_string(),
            description: None,
            is_current: false,
            actions: vec![Box::new({
                let cwd = self.config.cwd.clone();
                move |tx| {
                    tx.send(AppEvent::OpenReviewBranchPicker(cwd.clone()));
                }
            })],
            dismiss_on_select: false,
            search_value: None,
            display_shortcut: None,
        });

        items.push(SelectionItem {
            name: "Custom review instructions".to_string(),
            description: None,
            is_current: false,
            actions: vec![Box::new(move |tx| {
                tx.send(AppEvent::OpenReviewCustomPrompt);
            })],
            dismiss_on_select: false,
            search_value: None,
            display_shortcut: None,
        });

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Select a review preset".to_string()),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            ..Default::default()
        });
    }

    pub(crate) async fn show_review_branch_picker(&mut self, cwd: &Path) {
        let branches = local_git_branches(cwd).await;
        let current_branch = current_branch_name(cwd)
            .await
            .unwrap_or_else(|| "(detached HEAD)".to_string());
        let mut items: Vec<SelectionItem> = Vec::with_capacity(branches.len());

        for option in branches {
            let branch = option.clone();
            items.push(SelectionItem {
                name: format!("{current_branch} -> {branch}"),
                description: None,
                is_current: false,
                actions: vec![Box::new(move |tx3: &AppEventSender| {
                    tx3.send(AppEvent::CodexOp(Op::Review {
                        review_request: ReviewRequest {
                            prompt: format!(
                                "Review the code changes against the base branch '{branch}'. Start by finding the merge diff between the current branch and {branch}'s upstream e.g. (`git merge-base HEAD \"$(git rev-parse --abbrev-ref \"{branch}@{{upstream}}\")\"`), then run `git diff` against that SHA to see what changes we would merge into the {branch} branch. Provide prioritized, actionable findings."
                            ),
                            user_facing_hint: format!("changes against '{branch}'"),
                        },
                    }));
                })],
                dismiss_on_select: true,
                search_value: Some(option),
                display_shortcut: None,
            });
        }

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Select a base branch".to_string()),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            is_searchable: true,
            search_placeholder: Some("Type to search branches".to_string()),
            ..Default::default()
        });
    }

    pub(crate) async fn show_review_commit_picker(&mut self, cwd: &Path) {
        let commits = codex_core::git_info::recent_commits(cwd, 100).await;

        let mut items: Vec<SelectionItem> = Vec::with_capacity(commits.len());
        for entry in commits {
            let subject = entry.subject.clone();
            let sha = entry.sha.clone();
            let short = sha.chars().take(7).collect::<String>();
            let search_val = format!("{subject} {sha}");

            items.push(SelectionItem {
                name: subject.clone(),
                description: None,
                is_current: false,
                actions: vec![Box::new(move |tx3: &AppEventSender| {
                    let hint = format!("commit {short}");
                    let prompt = format!(
                        "Review the code changes introduced by commit {sha} (\"{subject}\"). Provide prioritized, actionable findings."
                    );
                    tx3.send(AppEvent::CodexOp(Op::Review {
                        review_request: ReviewRequest {
                            prompt,
                            user_facing_hint: hint,
                        },
                    }));
                })],
                dismiss_on_select: true,
                search_value: Some(search_val),
                display_shortcut: None,
            });
        }

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Select a commit to review".to_string()),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            is_searchable: true,
            search_placeholder: Some("Type to search commits".to_string()),
            ..Default::default()
        });
    }

    pub(crate) fn show_review_custom_prompt(&mut self) {
        let tx = self.app_event_tx.clone();
        let view = CustomPromptView::new(
            "Custom review instructions".to_string(),
            "Type instructions and press Enter".to_string(),
            None,
            Box::new(move |prompt: String| {
                let trimmed = prompt.trim().to_string();
                if trimmed.is_empty() {
                    return;
                }
                tx.send(AppEvent::CodexOp(Op::Review {
                    review_request: ReviewRequest {
                        prompt: trimmed.clone(),
                        user_facing_hint: trimmed,
                    },
                }));
            }),
        );
        self.bottom_pane.show_view(Box::new(view));
    }

    /// Programmatically submit a user text message as if typed in the
    /// composer. The text will be added to conversation history and sent to
    /// the agent.
    pub(crate) fn submit_text_message(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        self.submit_user_message(text.into());
    }

    pub(crate) fn set_current_user_instructions(&mut self, s: String) {
        self.current_user_instructions = Some(s);
    }

    pub(crate) fn persistent_mode_state(&self) -> PersistentModeState {
        self.persistent_mode_state.clone()
    }

    pub(crate) fn set_persistent_mode_state(&mut self, state: PersistentModeState) {
        self.persistent_mode_state = state;
    }

    pub(crate) fn token_usage(&self) -> TokenUsage {
        self.token_info
            .as_ref()
            .map(|ti| ti.total_token_usage.clone())
            .unwrap_or_default()
    }

    pub(crate) fn set_mode_summary(&mut self, s: Option<String>) {
        self.bottom_pane.set_mode_summary(s);
    }

    pub(crate) fn conversation_id(&self) -> Option<ConversationId> {
        self.conversation_id
    }

    /// Return a reference to the widget's current config (includes any
    /// runtime overrides applied via TUI, e.g., model or approval policy).
    pub(crate) fn config_ref(&self) -> &Config {
        &self.config
    }

    pub(crate) fn clear_token_usage(&mut self) {
        self.token_info = None;
        self.bottom_pane.set_token_usage(None);
    }

    pub fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        let [_, _, bottom_pane_area] = self.layout_areas(area);
        self.bottom_pane.cursor_pos(bottom_pane_area)
    }
}

#[cfg(test)]
pub(crate) fn show_review_commit_picker_with_entries(
    widget: &mut ChatWidget,
    entries: Vec<codex_core::git_info::CommitLogEntry>,
) {
    let mut items: Vec<SelectionItem> = Vec::with_capacity(entries.len());
    for entry in entries {
        let subject = entry.subject.clone();
        let sha = entry.sha.clone();
        let short = sha.chars().take(7).collect::<String>();
        let search_val = format!("{subject} {sha}");

        items.push(SelectionItem {
            name: subject.clone(),
            description: None,
            is_current: false,
            actions: vec![Box::new(move |tx3: &AppEventSender| {
                let hint = format!("commit {short}");
                let prompt = format!(
                    "Review the code changes introduced by commit {sha} (\"{subject}\"). Provide prioritized, actionable findings."
                );
                tx3.send(AppEvent::CodexOp(Op::Review {
                    review_request: ReviewRequest {
                        prompt,
                        user_facing_hint: hint,
                    },
                }));
            })],
            dismiss_on_select: true,
            search_value: Some(search_val),
            display_shortcut: None,
        });
    }

    widget.bottom_pane.show_selection_view(SelectionViewParams {
        title: Some("Select a commit to review".to_string()),
        footer_hint: Some(standard_popup_hint_line()),
        items,
        is_searchable: true,
        search_placeholder: Some("Type to search commits".to_string()),
        ..Default::default()
    });
}

impl WidgetRef for &ChatWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let [_, active_cell_area, bottom_pane_area] = self.layout_areas(area);
        (&self.bottom_pane).render(bottom_pane_area, buf);
        if !active_cell_area.is_empty()
            && let Some(cell) = &self.active_cell
        {
            let mut area = active_cell_area;
            area.y = area.y.saturating_add(1);
            area.height = area.height.saturating_sub(1);
            if let Some(exec) = cell.as_any().downcast_ref::<ExecCell>() {
                exec.render_ref(area, buf);
            } else if let Some(tool) = cell.as_any().downcast_ref::<McpToolCallCell>() {
                tool.render_ref(area, buf);
            }
        }
    }
}

enum Notification {
    AgentTurnComplete { response: String },
    ExecApprovalRequested { command: String },
    EditApprovalRequested { cwd: PathBuf, changes: Vec<PathBuf> },
}

impl Notification {
    fn display(&self) -> String {
        match self {
            Notification::AgentTurnComplete { response } => {
                Notification::agent_turn_preview(response)
                    .unwrap_or_else(|| "Agent turn complete".to_string())
            }
            Notification::ExecApprovalRequested { command } => {
                format!("Approval requested: {}", truncate_text(command, 30))
            }
            Notification::EditApprovalRequested { cwd, changes } => {
                format!(
                    "Codex wants to edit {}",
                    if changes.len() == 1 {
                        #[allow(clippy::unwrap_used)]
                        display_path_for(changes.first().unwrap(), cwd)
                    } else {
                        format!("{} files", changes.len())
                    }
                )
            }
        }
    }

    fn type_name(&self) -> &str {
        match self {
            Notification::AgentTurnComplete { .. } => "agent-turn-complete",
            Notification::ExecApprovalRequested { .. }
            | Notification::EditApprovalRequested { .. } => "approval-requested",
        }
    }

    fn allowed_for(&self, settings: &Notifications) -> bool {
        match settings {
            Notifications::Enabled(enabled) => *enabled,
            Notifications::Custom(allowed) => allowed.iter().any(|a| a == self.type_name()),
        }
    }

    fn agent_turn_preview(response: &str) -> Option<String> {
        let mut normalized = String::new();
        for part in response.split_whitespace() {
            if !normalized.is_empty() {
                normalized.push(' ');
            }
            normalized.push_str(part);
        }
        let trimmed = normalized.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(truncate_text(trimmed, AGENT_NOTIFICATION_PREVIEW_GRAPHEMES))
        }
    }
}

const AGENT_NOTIFICATION_PREVIEW_GRAPHEMES: usize = 200;

const EXAMPLE_PROMPTS: [&str; 6] = [
    "Explain this codebase",
    "Summarize recent commits",
    "Implement {feature}",
    "Find and fix a bug in @filename",
    "Write tests for @filename",
    "Improve documentation in @filename",
];

// Extract the first bold (Markdown) element in the form **...** from `s`.
// Returns the inner text if found; otherwise `None`.
fn extract_first_bold(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'*' {
            let start = i + 2;
            let mut j = start;
            while j + 1 < bytes.len() {
                if bytes[j] == b'*' && bytes[j + 1] == b'*' {
                    // Found closing **
                    let inner = &s[start..j];
                    let trimmed = inner.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    } else {
                        return None;
                    }
                }
                j += 1;
            }
            // No closing; stop searching (wait for more deltas)
            return None;
        }
        i += 1;
    }
    None
}

#[cfg(test)]
pub(crate) mod tests;
