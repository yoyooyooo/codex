# 自定义 Mode 需求说明

## 1. 背景与愿景
- 自定义 mode 与 prompts 在概念上相关，但实现独立：prompts 继续作为“保存提示”，mode 覆盖 Slash 链路。
- 目标是提供统一的 `/name` 入口：既保留瞬时执行，也支持可组合、可配置的“常驻模式”长期生效。
- 设计需兼容现有 `.codex/prompts`，同时在 TUI、CLI、SDK 间保持一致体验。

## 2. 核心概念
- **模式（Mode）**：通过 `/name` 触发，分为常驻模式（持久叠加上下文）与瞬时模式（执行后立即结束），描述、变量、默认值等由 frontmatter 定义。
- **常驻模式（persistent）**：开启后在会话中持续生效，将正文内容拼接到用户消息前缀，可组合，后续支持互斥/依赖扩展。
- **瞬时模式（instant）**：执行后立即返回结果，不改变常驻模式集合。
- **变量**：模式输入参数，支持默认值、必填、枚举、快捷键等，用于参数面板和校验。
- **缓存策略**：变量可声明为会话共享或模式私有，核心必须负责持久化/清理，前端仅作 UI 同步。

## 3. 模式来源与优先级
- 目录结构：
  - 项目目录：沿当前工作目录向上查找每一层的 `.codex/modes/`。
  - 全局目录：`$CODEX_HOME/modes/`（默认 `~/.codex/modes/`）。
- 模式 ID 由相对路径映射得到：子目录名称全部参与命名，以冒号连接。例如 `.codex/modes/a/b/c.md` 会被注册为 `/a:b:c`；根目录文件 `foo.md` 则对应 `/foo`。
- 模式文件和目录名仅允许 `[A-Za-z0-9_-]` 字符；若包含其它字符（空格、标点、额外冒号等），加载时会被跳过，Slash 与 CLI 也不会单独提示错误。由于模式 ID 通过子目录使用冒号拼接，最终触发 ID 形如 `/a:b:c`，冒号仅用作命名空间分隔。
- 合并顺序：
  - 先沿 `cwd` 的父目录链一路向上收集所有 `.codex/modes/` 目录，追加全局目录 `$CODEX_HOME/modes/`（若存在），形成从“最远”到“最近”的列表。
  - 合并时采用“后写覆盖先写”的规则：先写入优先级最低（全局或离 `cwd` 最远的目录）的模式，再依次写入更靠近 `cwd` 的目录；当遇到同一路径 + 文件名的模式文件时，以距离 `cwd` 最近的定义覆盖前者。最终 Slash 列表也按该优先级排序，离 `cwd` 越近的模式越靠前。
- 冲突与标签：
  - UI 用 `(project:<dirname>)`、`(global)` 标注来源；子目录名称用于命名空间标签。
  - 同一路径（含子目录）下的模式文件名作为唯一 ID：优先级更高的目录覆盖更低优先级的定义；Slash 菜单会展示来源及覆盖关系，便于排查。同名模式必须在 Slash 列表中展示完整命名空间（`/a:b:c`），避免用户误选。
  - 被覆盖的模式会在 Slash 菜单中以淡色提示“被 {winner_scope} 覆盖”，方便开发者排查。
- `.codex/prompts` 独立存在，在 Slash 菜单中以“保存提示”分类展示。

## 4. 模式文件结构
- 路径：`<name>.md`，允许子目录，目录结构决定模式 ID。
- Frontmatter 建议字段：
  - `kind: persistent | instant`。
  - `description`、`argument_hint`：用于 Slash 列表与参数面板。
  - `display_name`：可选。Slash 列表和事件里展示的友好标题；缺省时回退到模式 ID（去掉前导 `/` 后的字符串）。
  - `variables`: **必须**使用保序结构（推荐 YAML 数组），以确保解析顺序一致。每个变量包含：
    - `name`：变量名，必填。
    - `type`：推荐使用 `text` / `enum` / `boolean` / `number` / `path` 等枚举值；未设置时按约定规则推断（有 `enum` ⇒ `enum`，否则默认为 `text`）。前后端按该类型决定面板控件与 CLI 校验逻辑。
    - `default`：未显式填写时采用的值；若缺省且变量非必填，则解析时跳过该变量。
    - `required`：布尔值，标记在模式启用/触发前必须落到一个有效值；值可来自默认值、历史快照或当前输入，若最终仍为空则阻止模式生效。
    - `enum`：允许值的枚举列表，Slash 参数面板与 CLI 校验都会据此限制选择。
    - `shortcuts`：定义额外的输入别名，例如 `-t`、`ticket=`。匹配规则如下：
      - 仅支持完整匹配，按声明顺序依次尝试；命中后立即消费该参数并绑定对应变量。
      - `-t foo` 或 `ticket=foo` 会被识别为 `ticket` 变量；未命中 `shortcuts` 的部分仍按变量声明顺序消费。
    - `pattern`：可选的正则表达式字符串，用于客户端预校验输入；若 `type` 为 `enum` 则忽略。
    - `inline_edit`：可选布尔值，指示 UI 是否默认使用就地编辑；若未设置则根据 `type`、`enum` 等信息自动选择最合适的交互方式。
    - `mode_scoped`：布尔值，表示变量值是否仅在该模式实例内部缓存。
      - `true`（默认）：变量值随模式关闭由核心清空，其它模式不可见。
      - `false`：变量值写入“会话级共享缓存”，即同一 Codex 会话在模式关闭后仍能复用该值，键为 `{mode_id}:{variable_name}`；更换会话或退出后则统一清空。不同模式之间不会因为变量同名而互相读取。未来如需跨模式共享变量，将设计显式的共享组机制。
  - `default_enabled`：可选布尔值，控制“新会话初始是否自动启用该模式”。默认值为 `false`；只要模式文件存在，就会出现在 Slash 菜单中，与 `default_enabled` 无关。
  - `model`、`reasoning_effort` 等模型配置（可选）。
  - `allowed_tools`：暂不实现；字段保留作为未来扩展。
- 正文：常驻模式正文在启用后拼接到提示前缀；瞬时模式正文作为即时 prompt。
  - 变量替换使用 `{{variable_name}}` 语法，渲染时按照 frontmatter 定义的值替换；缺失必填变量会阻止模式生效。
  - 支持 `!command`、`@path`，复用现有安全策略。

示例：

```markdown
---
kind: persistent
description: "代码评审辅助模式"
display_name: "Design Review"
variables:
  - name: role
    required: true            # 必须有值，否则模式无法启用
    enum: [author, reviewer]
  - name: region
    default: "emea"
    mode_scoped: true
  - name: ticket
    shortcuts:
      - "-t"
      - "ticket="            # 作为命令行粘贴时的键值别名
    pattern: "^[A-Z]+-\d+$"
default_enabled: true
---

- 总是先阅读需求描述
- 按照 {{role}} 视角输出建议
```

上例中 `variables` 为数组，保证声明顺序在所有客户端一致。`role` 因 `required: true` 必须有值；`region` 未设置时使用默认值 `emea`；`ticket` 允许通过 `-t` 或 `ticket=` 快捷方式快速定位，并使用正则做格式校验。

## 5. Slash 体验与参数采集
- Slash 菜单：合并内置模式、自定义模式、保存提示；展示作用域、描述、参数提示，支持模糊搜索和标签过滤。
- 输入交互：
  1. 输入 `/[name]` 选择模式；带子目录的模式会以 `/a:b:c` 形式出现。常驻模式按 Enter 即启用并写入当前会话模式集合；瞬时模式按 Enter 立即触发执行。
  2. 模式变量调整可打开参数面板（类似 `/approvals`），展示变量、默认值、快捷键。
  3. 支持直接粘贴 `/deploy foo bar`，解析规则如下：
     - 解析器优先按声明顺序扫描 `shortcuts`；命中后立即绑定对应变量并跳过位置消费。
     - 剩余未匹配别名的参数按照变量声明顺序消费。
     - 允许使用引号与反斜杠转义来保留空格，例如 `/deploy "foo bar"`；未加引号的空格会作为参数分隔符。
     - Slash 行被解析为“模式操作”，该行不会自动发送剩余自然语言。若解析后仍有残余文本，前端提示用户确认是否继续发送为新消息。
     - 缺失必填参数时阻止模式生效，并在模式标签上显示 `⚠` 与“缺少参数”提示；可选参数若缺失则保留默认值。
     - 以上解析与校验仅发生在模式触发瞬间；变量一旦写入 Mode 条，后续普通消息不再二次解析，所有调整都通过 Mode 条或参数面板完成。
- 主输入框保持自然语言输入，避免在文本中注入占位符。
- CLI/SDK：
  - CLI 的 `/name` 体验对应 `codex chat --mode /name ...` 形式，使用同一解析器（包括冒号命名空间与 `shortcuts` 匹配）。
  - SDK 调用时，通过 `Op::SetModeState` 同步常驻模式集；触发瞬时模式则复用新的 `Op::TriggerInstantMode`，无需手写 prompt 拼接。
  - 三端共享 `ModeStateChangedEvent` 结构，客户端需在收到事件后刷新本地缓存，以保持模式列表和变量状态一致。

### 5.1 瞬时模式执行链路
- 触发瞬时模式后，前端组装 `InstantModeInvocation`：

  ```rust
  Op::TriggerInstantMode {
      mode_id: String,                            // 例如 "/deploy"
      variables: BTreeMap<String, ModeArg>,
      client_rev: Option<u64>,
  }

  enum ModeArg {
      UseDefault,
      Set(String),
  }
  ```

  - `mode_id` 与 Slash 列表中注册的 ID 完全一致（含 `/` 前缀）。
  - 变量收集规则与常驻模式相同：未在 map 中出现的键表示“保持现有值”；要清空或回退默认值时显式发送 `UseDefault`。
- 核心渲染正文后，将 prompt 封装为一次性的 `InstantModeExecuted` 事件：

  ```rust
  EventMsg::InstantModeExecuted(InstantModeExecuted)

  struct InstantModeExecuted {
      mode_id: String,
      rendered_prompt: String,
      warnings: Vec<String>,
      errors: Vec<ModeError>,
  }
  ```

  - `rendered_prompt` 会被立即注入当前会话的 `<user_message>`，随后照常走提问/回复流程。
  - 若渲染失败，`errors` 会给出变量缺失、`!command` 拒绝等原因，前端需提示用户重试；核心不会写入会话内容。
  - `ModeError` 与常驻模式共享结构，包含模式 ID / 变量名与错误信息，便于 UI 精确标注失败项。
- CLI/SDK 复用同一请求/事件，方便脚本批量触发瞬时模式。

## 6. Mode 条与模式管理
- 在现有输入辅助栏下新增“mode 条”，用于展示和管理当前会话的常驻模式状态：

  ```
  ┌────────────────────────────────────────────────────────────┐
  │ ⏎ send   ⇧⏎ newline   ⌃T transcript   ⌃C quit   …         │ ← 现有辅助栏
  ├────────────────────────────────────────────────────────────┤
  │ Mode: design-review · qa · accessibility                   │ ← 非焦点态（仅标签摘要）
  ├────────────────────────────────────────────────────────────┤
  │ ▸ design-review ⚠ [role=?] [region=emea]                   │ ← 焦点态展开详情，⚠ 表示变量缺失
  │ ○ qa              [target=staging] [retries=2]
  │ ○ accessibility   [level=aa]
  │ ↑↓ 焦点切换   ←→ 在变量标签间移动   ⏎ 编辑/展开   Space 开关模式
  └────────────────────────────────────────────────────────────┘
  ```

  - 非焦点态仅显示“Mode: <启用模式名列表>”，不展示变量，避免干扰输入。
  - 焦点切换到 mode 条后展开详情：`▸` 表示当前焦点模式，`○` 表示其他启用模式。
  - 当模式存在必填变量缺失时，在模式名称旁显示 `⚠`，并将变量标签以高亮形式呈现，提醒用户补齐。
  - 每个模式的变量以标签列出：
    - 已设置值：`[name=value]`。
    - 尚未设置且非必填：`[name=?]`（灰色或淡色，提示可选）。
    - 必填未设置：`[name=!]/[name=?]` 配红色高亮，并在模式标签上显示 `⚠`，提示需要补充。
  - 当变量较多时，使用 `← →` 在同一模式的变量标签间移动，折行时保持焦点标记。

- 焦点与操作：
  - 光标在输入框时按 `↓` 进入 mode 条（焦点默认停在最近启用的模式），按 `↑` 返回输入框；离开后恢复为摘要视图。
  - `Space` 快捷开关当前焦点模式；关闭时从发送给核心的 `Op::SetModeState` 列表中移除该模式并清除其 `mode_scoped` 变量缓存，再次按下会将其重新加入。
  - `Enter` 的行为依据变量配置确定：
    1. **面板式表单**（默认）：弹出面板集中编辑变量；支持必填标识、默认值、快捷键提示。
    2. **标签就地编辑**：当变量声明 `inline_edit: true` 或类型为自由文本且未声明枚举时，`Enter` 后光标落入标签，直接输入新值。
    3. 枚举/媒体：就地编辑时弹出下拉或选择列表，通过 `↑ ↓`/快捷键选择。
  - Frontmatter 中的 `default_enabled` 仅影响“新会话是否默认加入常驻集合”，不参与 Mode 条的开关逻辑。
  - `Tab`/`Shift+Tab` 在面板或下拉中切换变量，`Esc` 退出编辑返回 mode 条。

- `mode_scoped` 变量在模式关闭时由核心和前端双向清理；非 `mode_scoped` 变量则保留在同一会话的共享缓存中，以便模式再次启用时复用（切换或结束会话后统一清空）。

## 7. 实现细节：状态管理与事件分发
- **数据来源与同步**：
  - 核心在启动阶段通过 `Op::ListCustomModes` 下发模式定义；TUI 侧可在 `ChatWidget::on_list_custom_modes` 中缓存成 `Vec<ModeDefinition>`。
  - 所有常驻模式开关与变量修改在前端汇总到 `ModeManagerState`，最终组装成一次 `Op::SetModeState`（携带完整 `modes` 列表）发送给核心；不再单独调用旧的 toggle 请求。
  - 前端可在收到 `ModeStateChangedEvent` 后，用 `server_rev` 做版本确认，并更新本地缓存。如果 `errors` 非空，仅局部更新失败的模式状态。
- **前端状态结构**：
  - 在 `chatwidget.rs` 中新增 `ModeManagerState`（维护启用模式、变量值、焦点索引、mode 条展开态等）。
  - `ModeManagerState` 内部使用 `IndexMap<String, StoredVar>` 保存变量，其中 `StoredVar` 表示三态：`UseDefault` / `Explicit(String)` / `PendingUnset`（等待提交 `UseDefault`）。这样可区分“保持现值”“显式空串”“还原默认”。
  - 启用模式列表保持“启用时间”顺序；当接收到 `ModeStateChangedEvent` 时仅在 `changed == true` 且 `enabled_modes` 顺序调整时同步刷新本地序列，保证前后端顺序一致。
- **焦点切换与按键分发**：
  - 在 `chat_composer.rs` 的 `handle_key_event` 中扩展 `ComposerFocus` 枚举，包含 `Input`、`CommandPalette`、`ModeBar` 等状态。
  - `KeyCode::Down`：当当前焦点为输入框且没有打开其它弹窗时，切换到 `ModeBar`（设置 `ModeManagerState::focused = true` 并展开详情）。
  - `KeyCode::Up`：从 mode 条返回输入框，同时收起详情视图。
  - `KeyCode::Left/Right`：若焦点在 mode 条，移动 `selected_var_idx`；当跨越变量边界时更新 `ScrollState`。
  - `KeyCode::Enter` / `Char(' ')`：调用 `ModeManagerState` 方法执行开关或进入编辑；在进入表单式面板时，可以像 `ApprovalsModal` 那样通过 `AppEvent::OpenPopup` 创建新的 overlay。
- **变量编辑流程**：
  - 表单式面板：可新建 `mode_panel.rs`，结构参考 `codex-rs/tui/src/bottom_pane/approvals.rs`，使用 `ModeVariableEditor` 结构管理字段、校验、`pretty_assertions` 测试。
  - 标签就地编辑：在 `ModeManagerState` 中维护 `editing_var: Option<EditingState>`，类似 `chat_composer` 目前处理文本占位符的逻辑；通过 `textarea` 或自定义缓冲区接收输入。
  - 枚举/媒体选择：复用 `selection_popup_common.rs` 渲染下拉列表，监听 `KeyCode::Up/Down/Enter` 完成选择，选定后更新变量值并退出编辑态。
- **渲染**：
  - 在 `chatwidget::render` 中，根据 `ModeManagerState::focused` 决定绘制摘要或详情。利用 `Line::from`、`Span::styled` 与 `Stylize` 扩展着色（灰色/红色标签）。
  - 详情态多行布局可用 `Paragraph::new(vec![Line::from(...)])`，与现有底部辅助栏绘制方式保持一致。
- **状态持久化**：
  - 会话内持久化：模式集合与变量值可存入 `ChatWidget` 的 session state，随会话切换恢复。
  - 长期复用：如需跨会话记忆，可复用 `session_configured` 保存模型选择的逻辑（见 `chatwidget::new_from_existing`），或后续考虑写入本地 store。

## 8. 模式提示拼接与用户指令更新
- **基线指令缓存**：核心在 `Codex::spawn` 时通过 `get_user_instructions` 计算出基线内容（包含用户配置与 AGENTS.md，见 `codex-rs/core/src/project_doc.rs`）。新的模式系统在 `TurnContext` 中额外记录：
  - `base_user_instructions`：未包含模式的原始文本。
  - `mode_instruction_block`：根据当前启用模式动态生成。
- **最终串的拼接**：

  ```text
  <user_instructions>

  {base_user_instructions.trim()}

  <mode_instructions>
  ### Mode: {display_name_1}
  - scope: {scope_label}
  - variables: foo=bar, active=True

  {rendered_instructions_1}

  ### Mode: {display_name_2}
  ...
  </mode_instructions>

  </user_instructions>
  ```

  - 若无常驻模式启用，省略 `<mode_instructions>` 块，仅保留基线文本。
  - `rendered_instructions` 为模式正文替换变量后的结果；变量替换按 frontmatter 定义执行（缺省值和用户输入都会应用）。
  - 每次模式/变量变化都会重新生成整个 `<mode_instructions>`，不会追加历史条目。
  - 多模式输出顺序固定为“启用时间”升序：最早启用的排在前面；若同一请求内同时启用多个新模式，则按 `Op::SetModeState.modes` 中的顺序稳定输出，确保渲染结果可复现，便于快照与 diff。
- **在核心中更新指令**：
  - 当前端需要开关模式或提交变量修改时，都通过一次新的 `Op::SetModeState` 将完整意图发给核心；核心处理后以 `ModeStateChangedEvent` 反馈最终状态。
  1. 前端发送新的 `Op::SetModeState`：

     ```rust
     Op::SetModeState {
         modes: Vec<PersistentModeTarget>,
         client_rev: Option<u64>,
     }

     struct PersistentModeTarget {
         mode_id: String,                            // 使用完整模式 ID（例如 "/a:b:c"）
         enabled: bool,                              // 当条目保留在列表中时应为 true；要禁用模式请移除该条目
         variables: BTreeMap<String, ModeVariableInput>,
     }

     enum ModeVariableInput {
         UseDefault,                                 // 恢复 frontmatter 定义的默认值 / 未设置状态
         Set(String),                                // 显式指定变量值
     }
     ```

  - `modes` 携带前端认知下的“全量目标状态”。要禁用模式，就从列表中移除对应项；保留条目意味着该模式目标状态为启用，同时可以附带变量变更（约定此时 `enabled` 字段为 `true`）。
  - 未包含在 `variables` 中的键视为“不变”；要清空或回落到默认值须显式发送 `UseDefault`。
  - 建议前端维护本地变量快照：编辑时更新快照并发送完整的 `variables` map。要删除某个值，发送 `UseDefault`；要保持现值，重复发送 `Set(existing_value)` 即可。
  - `client_rev` 为可选的前端递增版本号，用于防止重试或乱序；因为当前设计不支持多客户端并发，一个会话内只需比较“最新成功的版本号”即可。
  2. 核心的 `ModeInstructionManager` 根据 `client_rev` 先行判重（旧版本直接拒绝并返回错误），随后 diff 与合并内部缓存，调用 `render_mode_instructions()` 生成 `<mode_instructions>` 字符串。
  3. 若渲染成功，引擎将组合 `base_user_instructions` 与新块，写入 `TurnContext.user_instructions` 并使用 `ConversationHistory::replace` 更新首条 `<user_instructions>` 消息，同时记录新的 `server_rev`（若 `client_rev` 存在则沿用，否则核心内部自增一个版本号）。
  4. 若渲染失败（例如缺失必填变量、模板语法错误等），核心保持上一份有效状态不变，仅返回错误事件帮助前端提示用户修正。
- **避免重复发送**：
  - 核心缓存最近一次成功渲染的 `<mode_instructions>` 与 `server_rev`；若新渲染结果与缓存完全一致，即便 `client_rev` 变化，仍可直接返回 `changed = false` 的事件，避免重复写入与消息风暴。
- **同步回前端**：

  ```rust
  EventMsg::ModeStateChanged(ModeStateChangedEvent)

  struct ModeStateChangedEvent {
      enabled_modes: Vec<RenderedMode>,
      combined_user_instructions: Option<String>,
      server_rev: u64,
      changed: bool,
      errors: Vec<ModeError>,
  }

      struct RenderedMode {
          mode_id: String,
          display_name: String,
          scope: ModeScope,
          variables: Vec<ModeVariableState>,
          rendered_instructions: Option<String>,
          warnings: Vec<String>,
      }
  ```

  - `RenderedMode` 中的 `variables` 应携带 `value: Option<String>`、`is_valid`、`source: VariableValueSource` 等字段，供前端渲染标签与状态。
  - `rendered_instructions` 保存当前模式正文渲染后的文本；若渲染失败则返回 `None`。瞬时模式仍通过 `InstantModeExecuted.rendered_prompt` 提供一次性 prompt。
  - `combined_user_instructions` 仅在内容发生变更时返回；若 `changed=false`，可返回 `None` 以减少 payload。
  - `errors` / `warnings` 用于提示截断、变量被覆盖等问题；`errors` 描述部分失败，例如变量缺失或渲染出错。
  - 核心将 `enabled_modes` 视为“当前已生效模式”的唯一真源：缺席的模式应在前端立即显示为“未启用”。

  > 当前版本不支持运行时“模式热更新”。即会话启动时读取的模式内容在整个会话内保持不变，只有下一次启动会话才会重新加载模式文件。

- **变量校验失败的用户提示**：

  - 当渲染失败时，核心不写入新的 `<mode_instructions>`，并在 `errors` 中按模式聚合缺失变量。
  - 前端 Mode 条可突出显示失败项，例如：

    ```text
    Mode: design-review · qa
    ├─ design-review ⚠ 变量未填写：role, region
    └─ qa            ✓
    提示：按 Enter 填写缺失变量，或 Space 关闭模式
    ```

  - 若用户后续补全变量并重新提交 `Op::SetModeState`，核心在完成渲染后会携带 `changed = true` + 空的 `errors`，让前端清除警示。

## 9. 模式生命周期
- 会话启动时读取 `.codex/modes`；凡是声明 `default_enabled: true` 且变量校验通过的模式都会在新会话中自动加入常驻集合，未声明或为 `false` 的模式默认保持关闭但仍可在 Slash 中选择。
- 触发 `/mode-name` → Enter：
  - 若模式为常驻且尚未启用，则启用并显示变量面板（必要时）以便用户确认参数。
  - 若模式为常驻且已启用，再次触发可：
    - 打开变量面板供调整；
    - 或执行模式关闭（Slash 菜单提供“关闭模式”操作，相当于从 `Op::SetModeState.modes` 列表移除该模式）。
  - 若模式为瞬时，解析参数后立即发送 `Op::TriggerInstantMode`。
- Mode 条负责显示组合、变量状态、错误提示，并支持快捷开关。
- 记录模式启用历史与变量快照，供会话恢复或模板复用（存储策略待定：本地文件或会话内存）。
  - 当前阶段仅将渲染后的 `<mode_instructions>` 写入 `<user_instructions>`，`ModeManagerState` 及变量快照不会持久化到会话文件；`codex resume` 只能恢复模型上下文，前端需视情况重新启用模式并补录变量。

## 10. 协议与前端适配
- 核心协议：
  - `Op::ListCustomModes` / `ListCustomModesResponseEvent { modes: Vec<ModeDefinition> }`。
  - `Op::SetModeState` / `EventMsg::ModeStateChanged(ModeStateChangedEvent)`。
  - `Op::TriggerInstantMode` / `EventMsg::InstantModeExecuted(InstantModeExecuted)`。
- `ModeDefinition` 元数据需包含模式类型、作用域标签、frontmatter 字段、正文或路径，供前端构建 Slash 列表与变量面板。
- CLI、TUI、SDK 共用数据模型；TypeScript 绑定通过 `ts_rs` 导出结构体（`RenderedMode`、`ModeVariableState`、`ModeError` 等）。
- 列表刷新：会话启动时调用 `Op::ListCustomModes`。当前阶段不监听文件变更，如需重新加载模式，需在新会话中启动或由用户主动触发刷新。

## 11. 安全与权限
- `!command` 暂按现有白名单判断，不引入新的 `allowed_tools` 配置；文档保留该字段作为未来扩展，当前阶段所有模式调用 `!command` 时须遵循全局策略。
- `@path` 访问需遵守 sandbox 限制，越权时返回明确错误。
- 变量展开时避免命令注入：向 shell 传参保持分离或进行必要转义。
- 若未来支持 `auto-submit`，需在 Slash 菜单和 mode 条中显式标识并提示确认。

## 12. 兼容与迁移
- `.codex/prompts` 功能保持不变；在 Slash 菜单中继续以“保存提示”分类展示。
- 可提供脚本将旧版 `.codex/commands` 转换为 `.codex/modes`（包含 frontmatter 调整），帮助用户迁移。
- 更新文档：在 `docs/prompts.md`、`docs/getting-started.md` 等补充 mode 使用说明。

## 13. 验收要点
- 功能验证：frontmatter 解析、模式发现与合并、变量校验、常驻模式启用/关闭、瞬时模式执行、`!/@` 权限检查。
- UI 验收：Slash 菜单、mode 条、参数面板、错误提示；更新相关 TUI snapshot。
- 协议测试：`Op::ListCustomModes`、`Op::SetModeState`、`Op::TriggerInstantMode` 的 end-to-end 流程。
- 发布说明：说明新目录结构、mode 条交互与兼容策略。
