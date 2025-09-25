# 自定义 Mode 实施规划

本文将自定义 Mode 方案拆解为低风险、可逐项执行的任务清单；执行顺序即阶段顺序，勾选即可追踪完成度。

## 阶段 0：准备与基线
- [ ] 阅读 `codex-rs/core` 与 `codex-rs/tui` 中现有 Slash、prompt、`TurnContext` 相关代码，记录入口文件与核心结构（输出至 `docs/fork-feat/custom-mode-plan.md` 附注）。
- [ ] 列出当前协议枚举与事件（`codex-rs/protocol`），确认是否已有模式相关定义；若无，整理新增枚举的命名草稿。
- [ ] 在 `docs/fork-feat/custom-mode-plan.md` 中整理 `.codex/modes` 目录约束 checklist，包含 ID 命名、优先级、合并规则。
- [ ] 起草 `ModeDefinition` / `ModeVariableDefinition` / `RenderedMode` / `ModeError` 的字段表（中英文对照、类型），供后续代码实现复用。
- [ ] 与团队评审阶段 0 调研输出，确认无遗漏后锁定为实现基线。

## 阶段 1：核心能力（codex-core）
- [ ] 在 `codex-rs/core/src/modes/mod.rs`（新建模块）实现模式扫描器：
  - [ ] 遍历 `$CODEX_HOME/modes` 与工作目录向上链的 `.codex/modes`，使用 `IndexMap` 保序。
  - [ ] 过滤非法 ID（非 `[A-Za-z0-9_-]`），写入 `ModeLoadWarning` 集合。
  - [ ] 为覆盖场景编写单元测试，验证后写覆盖先写规则。
- [ ] 在 `codex-rs/core/src/modes/frontmatter.rs` 新建解析器：
  - [ ] 使用 `serde_yaml` 解析 frontmatter，映射到 `ModeVariableDefinition`。
  - [ ] 实现默认值、`required`、`enum`、`pattern`、`shortcuts`、`mode_scoped` 等字段校验。
  - [ ] 编写单测覆盖必填缺失、非法枚举、正则失败等路径。
- [ ] 在 `codex-rs/core/src/modes/registry.rs` 构建 `ModeRegistry`：
  - [ ] 会话启动 (`Codex::spawn`) 时加载一次，缓存 `Vec<ModeDefinition>`。
  - [ ] 提供 `list()`、`get(id)`、`render(mode_state)` 等接口。
- [ ] 扩展会话上下文（`codex-rs/core/src/session/state.rs`）：
  - [ ] 新增结构保存常驻模式列表、变量缓存（区分 mode_scoped / session_scoped）。
  - [ ] 实现 `apply_mode_state(SetModeState)`，返回 `ModeStateChangedEvent` 所需信息。
- [ ] 在协议层 (`codex-rs/protocol/src/ops.rs` & `events.rs`) 添加：
  - [ ] `Op::ListCustomModes`、`Op::SetModeState`、`Op::TriggerInstantMode`。
  - [ ] 对应 `ListCustomModesResponseEvent`、`ModeStateChangedEvent`、`InstantModeExecuted` 结构。
  - [ ] 为新结构补 `serde` / `ts_rs` 派生。
- [ ] 在核心命令处理（`codex-rs/core/src/runtime/handlers.rs`）实现：
  - [ ] `handle_list_custom_modes`：返回缓存列表。
  - [ ] `handle_set_mode_state`：校验变量、渲染模式正文、生成 `mode_instruction_block`。
  - [ ] `handle_trigger_instant_mode`：执行变量校验与 prompt 渲染，触发即时执行逻辑。
- [ ] 调整 `TurnContext`：保存 `base_user_instructions` 与 `mode_instruction_block`，确保输出拼接顺序正确，并添加回归测试。
- [ ] 安全校验：
  - [ ] 复用 `!command` 白名单与 `@path` sandbox 检查；
  - [ ] 为变量正则注入与 shell 参数传递写单元测试，防止注入风险。
- [ ] 核心测试：
  - [ ] `cargo test -p codex-core` 验证全部新增逻辑；
  - [ ] 如涉及共享 crate，再运行 `just fix`（待最终确认时执行）。

## 阶段 2：TUI 适配（codex-tui）
- [ ] 在 `codex-rs/tui/src/chatwidget.rs` 引入 `ModeManagerState`：
  - [ ] 使用 `IndexMap` 存储启用模式及变量 `StoredVar`（三态：`UseDefault`/`Explicit`/`PendingUnset`）。
  - [ ] 缓存 `focused_mode_idx`、`selected_var_idx`、`expanded` 状态。
- [ ] 订阅事件与初始化：
  - [ ] 在 `ChatWidget::on_list_custom_modes` 缓存模式定义。
  - [ ] 在 `handle_event` 中处理 `ModeStateChangedEvent`，同步 `server_rev` 与错误列表。
- [ ] 渲染层更新：
  - [ ] 扩展底部区域渲染摘要模式条（未聚焦）与详情模式条（聚焦）。
  - [ ] 使用 `Stylize` 应用 `⚠` 高亮、灰色可选标签、红色必填缺失。
  - [ ] 更新/新增 snapshot 覆盖摘要、详情、错误态。
- [ ] 焦点与键盘交互（`codex-rs/tui/src/chat_composer.rs`）：
  - [ ] 扩充 `ComposerFocus` 枚举，引入 `ModeBar`。
  - [ ] 处理 `Down` 进入 mode 条、`Up` 返回输入框、`Left/Right` 切换变量、`Space` 切换启用、`Enter` 进入编辑、`Esc` 退出。
  - [ ] 为交互编写 behaviour 测试或 snapshot。
- [ ] 变量编辑：
  - [ ] 新建 `codex-rs/tui/src/modes/mode_panel.rs`（或同目录模块）实现表单面板，支持 Tab/Shift+Tab、必填提示。
  - [ ] 实现枚举选择，复用 `selection_popup_common.rs`。
  - [ ] 支持 `inline_edit: true` 的就地编辑，使用 textarea 缓冲。
- [ ] 错误提示：
  - [ ] 在 Mode 条下方渲染错误文本（例如“变量未填写：role, region”）。
  - [ ] 确保关闭模式后清空 `mode_scoped` 缓存。
- [ ] 运行 `cargo test -p codex-tui` 并处理 snapshot 更新（如需，使用 `cargo insta accept -p codex-tui`）。

## 阶段 3：CLI / SDK / TypeScript
- [ ] 在 `codex-rs/protocol` 中为新结构添加 `ts_rs` 导出（若未在阶段 1 完成）。
- [ ] `codex-cli` Slash 菜单：
  - [ ] 扩展模式列表显示来源标签、覆盖提示。
  - [ ] 实现 `/mode` 命令启用/关闭常驻模式（调用 `Op::SetModeState`）。
  - [ ] 实现瞬时模式触发命令（`codex mode trigger /name --var value`）。
- [ ] CLI 参数校验：
  - [ ] 基于模式定义校验必填/枚举/正则，返回清晰错误信息。
- [ ] SDK（TypeScript）更新：
  - [ ] 暴露 `listCustomModes()`、`setModeState()`、`triggerInstantMode()` API。
  - [ ] 补充事件回调（mode 状态变更），并编写最小示例。 
- [ ] 为 CLI / SDK 新功能编写端到端测试或集成脚本。

## 阶段 4：文档与迁移
- [ ] 更新 `docs/prompts.md`、`docs/getting-started.md`，新增自定义模式章节、目录结构说明、变量示例。
- [ ] 在 `docs/fork-feat/custom-mode.md` 末尾补充“实现进度”章节链接至本规划文档。
- [ ] 编写迁移脚本草稿（保存在 `scripts/migrate_commands_to_modes.rs` 或 TS 脚本）：
  - [ ] 读取旧 `.codex/commands`，生成新的模式 frontmatter 模板。
  - [ ] 输出迁移指南与示例。
- [ ] 准备发布说明：在 `CHANGELOG.md` 草拟条目，概述新特性、升级事项、兼容性提醒。

## 阶段 5：验收与质量保障
- [ ] 编写手动验收脚本（存入本文件附录），覆盖常驻/瞬时启用、变量缺失提示、模式覆盖提醒、会话缓存复用。
- [ ] 执行 `cargo test -p codex-core`、`cargo test -p codex-tui`，确认通过；根据需要征求是否运行 `cargo test --all-features`。
- [ ] 记录已知限制：模式热更新缺失、会话持久化策略待定、auto-submit 暂不支持；添加到发布说明与 backlog。
- [ ] 评估新增日志与指标需求，若需要，创建后续 issue。

## 后续扩展（待规划）
- [ ] 模式热更新（文件系统监听或手动刷新）。
- [ ] 常驻模式互斥/依赖关系配置。
- [ ] 跨会话持久化策略（本地 store）。
- [ ] auto-submit 支持与安全确认 UX。
- [ ] 模式授权与共享机制（团队级别）。
