# 自定义 Mode（Custom Modes）

## 背景与约束
- Fork 需要在 Slash 链路中引入“常驻模式（persistent）”与“瞬时模式（instant）”的概念，同时维持 upstream 协议不变，避免破坏 userspace。
- 变更主旨：**客户端负责全部发现/解析/渲染，核心沿用现有 `Op::OverrideTurnContext` 入口**。任何时候都可以回退到上游实现，只需移除 `// !Modify:` 标记段落。
- 代码范围：核心逻辑落在 `codex-rs/modes/`（新 crate）和 `codex-rs/tui/src/modes/**`，少量挂钩位于 `chatwidget.rs`、`app.rs`、`bottom_pane/mod.rs`。

## 目录结构
- `codex-rs/modes/`：纯 Rust 库，负责扫描 `.codex/modes/**`、解析 frontmatter、渲染 `<mode_instructions>`，并提供 `normalize_equiv` 去抖工具。
- `codex-rs/tui/src/modes/`：UI 组件，包括 ModeBar、ModePanel、变量编辑器、状态摘要及测试快照。
- `docs/fork-feat/custom-mode/`：详细规格与测试计划；本文档聚焦落地实现。

## 模式发现流程
1. `scan_modes(cwd, codex_home)` (`codex-rs/modes/src/lib.rs:120` 起) 计算目录链：
   - 调用 `project_chain_from_repo_root_to` 找到 Git 根，生成自根→`cwd` 的路径序列。
   - 对每个路径拼接 `.codex/modes/`，过滤不存在目录。
   - 追加 `$CODEX_HOME/modes/`（若存在）。
2. 逐目录递归 `walkdir`：
   - 仅保留 `.md` 文件 (`id_from_rel_path` 会验证路径段只含 `[A-Za-z0-9_-]`)。
   - Frontmatter 解析使用 `serde_yaml`；缺省 `kind` 回退为 `persistent`。
   - 检查变量重名、正则合法性，否则抛出 `ModesError::VarDup`/`ModesError::Regex`。
   - 以 Slash ID 为 key 写入 `IndexMap`，后写覆盖先写，完成“就近覆盖”。
3. 返回 `ModeDefinition` 列表，包含 scope（`project:<dirname>` 或 `global`）、body、变量定义等。

## 变量与渲染
- `ModeVariableDefinition` 支持 `type`、`default`、`required`、`enum`、`shortcuts`、`pattern`、`inline_edit`、`mode_scoped`。
- `render_user_instructions` 将启用模式（`EnabledMode` 集合）与 base `<user_instructions>` 拼接，在 `<mode_instructions>` 块中写入：
  ```
  ### Mode: {display}
  - scope: project:foo
  - variables: ticket=123, focus=ui

  正文模板（变量替换后）
  ```
- 占位符替换使用 `{{var}}`。缺失值时优先读取变量缓存，其次 frontmatter 默认，最后空字符串。
- `normalize_equiv`（同文件尾部）把 CRLF→LF、去除多余空行，供快照对比和去抖处理。

## TUI 状态与交互
- `PersistentModeState`（`codex-rs/tui/src/modes/state.rs`）负责持久模式的启用序列、变量缓存及快照清理；`sanitize` 会在每次重新加载定义时剔除过期模式/变量。
- `ModesUiDefaultFactory`（`modes/factory.rs`）集中构建 `ModeBarView`、`ModePanelView` 与状态摘要，方便在测试中注入替身。
- `mode_bar.rs`：主交互容器。
  - `ModeBarView::handle_key_event` 在 `ChatWidget` 的 `// !Modify: ModeBar hotkey` 标记处挂载，仅在正常输入态响应。
  - footer 提示行文本位于 `mode_bar.rs:720` 附近，snapshot 前缀为 `modebar_`。
- `mode_panel.rs`：轻量选择器。
  - `ModePanelView::apply` 会批量更新 `PersistentModeState` 并触发回调，让 `ChatWidget` 重新渲染。
- `chatwidget.rs` 注入点：
  - `handle_slash_result` 内部新增 `process_modes_slash` 分支，用于区分 `/mode` 与 upstream Slash；标记为 `// !Modify: Slash modes`。
  - `build_footer` 增加 Mode 摘要行，复用 `PersistentModeState` 中的启用顺序。

## 核心集成点
- `ChatWidget::prepare_submission` 在构造 `AgentInput` 时调用 `mode_store.render_user_instructions(...)`，并将结果放入 `Op::OverrideTurnContext`。
- `bottom_pane/mod.rs` 在任务运行期间禁用 ModeBar，避免工作流状态机被干扰。
- `app.rs`/`app_backtrack.rs` 中的 Esc 逻辑不直接依赖模式，但在 `// !Modify:` 注记处会在 ModeBar 打开时短路 Esc，以防状态冲突。

## 错误处理与 Telemetry
- 所有模式错误编码在 `ModesError` 中，遵循 `E1xxx` (`load`)、`E2xxx` (`frontmatter`) 分类。
- TUI 将错误渲染成红色提示行，并记录 tracing：`error!(mode_id, code, message)`。
- CLI 路径透传 `ListModesResponse.errors`，提示用户修复文件。

## 最小差异与同步注意事项
- 所有 `// !Modify:` 标记保持在函数最外层，便于上游重排时整体移动。
- 与 upstream 差异集中在：
  - 新增 crate `codex-modes`（`Cargo.toml` 中 `path = "modes"`）。
  - `tui/src/chatwidget.rs` 内的 Slash 分支、ModeBar 状态、footer 摘要。
  - `tui/src/bottom_pane/mod.rs` 与 Esc 行为的守卫。
- 同步时务必查看 `docs/fork-feat/custom-mode/min-upstream-diff-architecture.md` 的 Checklist，并运行 `cargo test -p codex-tui` 确认 `modes__*.snap` 未意外漂移。

## 验证流程
- **层级覆盖**：准备 `~/.codex/modes/review.md` 与 `<repo>/.codex/modes/review.md`，确认 Slash 列表展示“覆盖”提示（淡色文案 & scope 标签）。
- **变量守卫**：定义 `required: true` 的变量，启用模式后提交消息，确认被拦截并提示补全。
- **渲染等价**：修改模式模板后运行 `cargo test -p codex-modes`（若有），再运行 `cargo test -p codex-tui`，确保 snapshot 经过 `normalize_equiv` 去噪。
- **CLI 一致性**：在 CLI 里执行 `/mode var=value`，验证与 TUI 行为一致（常驻模式透传到下一次请求）。
