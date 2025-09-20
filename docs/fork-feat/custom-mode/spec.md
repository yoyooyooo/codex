# 自定义 Mode 规格（A 案：前端合成 + 覆写）

版本：0.1.0（proposal）

目标
- 最小化与上游的差异：不新增 `Op`/`EventMsg`，仅使用现有 `Op::OverrideTurnContext` 的 `user_instructions` 可选字段完成覆写（当前 fork 已具备）。
- 提供可配置、可组合的常驻模式，使其以 `<mode_instructions>` 形式拼接到 `<user_instructions>` 中，影响后续回合。

非目标
- 不引入服务端真源/模式事件（如 `ListCustomModes`、`SetModeState`）。
- 不处理多客户端一致性与模式热更新广播。
- 不提供 JS/TS SDK；仅 Rust（TUI）。

职责划分
- 前端（TUI，Rust）：扫描 `.codex/modes` 与 `$CODEX_HOME/modes`，解析 frontmatter，管理启用集合与变量，渲染 `<user_instructions>`，去抖与等价检测后调用 `OverrideTurnContext`。
- 核心（codex-core）：接收覆写后用 `ConversationHistory::replace` 重建首条 `<user_instructions>` 消息；`TurnContext` 保留 `base_user_instructions` 作为渲染基线；其余请求/推理逻辑不变。

兼容性
- 与 `.codex/prompts` 并存；未启用模式时不生成 `<mode_instructions>`，行为与现状一致。

相关文档
- 文件发现/优先级：files.md
- Frontmatter 与校验：frontmatter.md
- 渲染算法与规范：rendering.md
- 前端共享库（Rust）API：frontend-rust-api.md
- TUI：ui-tui.md
- 错误与安全：errors.md、security.md
- 测试与规划：testplan.md、plan.md

上游适配层 / 迁移策略
- 定义前端库 `ModeEngine` 作为唯一入口，提供：
  - `load_defs(data_source)` 加载模式定义；
  - `enable/disable/assign_var/validate` 管理状态；
  - `render(base_user_instructions)` 生成最终 `<user_instructions>`；
  - `normalize_equiv(prev, next)` 规范化等价检测；
  - `DataSource` trait：`LocalFsDataSource`（当前）与 `ServerEventDataSource`（未来上游事件）可互换。
- TUI 仅依赖 `ModeEngine`，对数据源无感；未来接入上游的 `ListCustomModes`/`ModeStateChangedEvent` 时，仅替换 `DataSource`，避免界面与调用点发生 diff。

上游不变式（最小差异）
- 未启用模式时，生成的 `<user_instructions>` 必与上游现状完全等价。
- 不新增协议变体/事件；只使用 `Op::OverrideTurnContext { user_instructions }`。
- 不向核心会话落地模式状态；前端会话级缓存，`resume` 通过重放覆写恢复。
- 渲染输出稳定可快照；等价时不发送覆写，避免无意义的历史污染。
  - UI 侧通过 `codex-modes::normalize_equiv` 与 `Debouncer` 统一实现“等价短路 + 去抖”。

非目标（本阶段）
- 不提供独立 CLI 子命令；全部操作通过 TUI 完成，且仅作用于当前会话。
