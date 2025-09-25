# TUI 交互 / 样式 / 快照（含常驻模式摘要栏）

快捷键总览（不触发模型请求）
- Alt+B：打开/关闭模式条（ModeBar）。
- Down：当输入历史已在“最新一条”且无法再向下时，按 ↓ 进入模式条（免去 Alt+B）。
- Tab / Shift+Tab：在模式之间切换。
- ← / →：在“当前模式”的变量标签之间切换。
- ⏎：编辑当前变量（枚举：↑/↓ Select；⏎ Apply；Esc Cancel）。
- Space：启用/禁用当前模式（去抖 200ms + 等价短路）。
- d：展开/收起详情。
- Esc：退出模式条。
- Ctrl+U：查看当前生效的 `<user_instructions>`（只读 Pager 覆盖层，包含 `<mode_instructions>`）。

ModePanel（最简选择面板）
- 打开方式：从 ModeBar 的“d 详情/Enter 编辑”旁，使用 Slash 命令或菜单打开“Modes 面板”（内部列表样式）。
- ↑/↓：移动选择；Space：勾选/取消；Enter：应用；Esc：取消。
- 面板不编辑变量值，仅切换启用集合；变量值沿用 ModeBar（或上次应用）中的显式值；必填校验会考虑已设置的变量值。

渲染样式（ratatui Stylize）
- 配色规范：激活项（当前模式/当前变量）`cyan().bold()`；非激活项统一 `.dim()`；缺失仅用 `⚠` 文案，不再使用红色。
- 变量标签：`[name=value]`（已设置）、`[name=?]`（使用默认/可为空）、`[name=!]`（必填缺失，仅作文案提示）。
- 详情视图使用 `Paragraph::new(vec![…].into())`；避免 `.white()`；链式 `.dim().bold()` 等。
- 详情多行换行：使用 `tui/src/wrapping.rs` 的 `word_wrap_line`，首行缩进 `▌ `，后续行缩进两个空格。
 - 详情与提示区分：详情区与底部提示之间绘制一条 dim 分隔线（由 `─` 重复组成），提升可读性。

状态
- `ModeManagerState`：IndexMap 记录变量值（UseDefault/Explicit/PendingUnset）；`focused_mode_idx`、`selected_var_idx`、`expanded`、`editing_var`。
- 会话切换：状态存于会话级；`resume` 时前端重放覆写。

事件与同步
- 不依赖核心事件；前端本地扫描/管理；去抖 150–300ms（当前实现常量 200ms）；`normalize_equiv` 为真时跳过发送（规范化比较：CRLF→LF、逐行去尾空格、空行折叠）。
- 覆写成功后，本地回写 `current_user_instructions`，保证 ModeBar/ModePanel/快捷键的初始态与会话当前态一致。
 - 查看当前 `<user_instructions>`：
  - 数据源优先级：`current_user_instructions` → `base_user_instructions` → `config.user_instructions`。
  - 以 Pager 覆盖层方式展示（顶部标题 `Current <user_instructions>`，支持滚动与跳页）。
- 模式摘要栏的数据源：来自前端最近一次生效的“启用集合 + 变量值”（发送覆写时一并回写），不从核心拉取。
 - 自动启用：会话启动（SessionConfigured）后，自动启用“kind=persistent 且 default_enabled=true 且所有必填变量存在默认值”的集合；静默覆写 `<user_instructions>` 并更新模式摘要栏。

布局与区域
- 底部区域自上而下依次为：
  1) 输入框（Composer）
  2) 辅助栏（发送/快捷键提示）
  3) 常驻模式摘要栏（Persistent Mode Summary Bar，简称“模式摘要栏”）
- 模式摘要栏始终占 1 行；当“展开详情”时，在摘要栏“上方”临时增加 1–3 行详情（不覆盖输入框与辅助栏，只挤占上方的对话区域高度）。
- 常驻规则：仅当“已开启的模式数量 > 0”时显示摘要栏；当无模式开启时隐藏该行。
- 在极小高度时，至少保留摘要 1 行；无法显示详情时，仅显示摘要行。

ASCII 线框图（示意）

摘要态（折叠，仅标签摘要）
```
┌────────────────────────────────────────────────────────────┐
│ [ 输入框 ]                                                │ ← Composer（多行）
├────────────────────────────────────────────────────────────┤
│ ⏎ send   ⇧⏎ newline   ⌃T transcript   ⌃C quit   …         │ ← 辅助栏
├────────────────────────────────────────────────────────────┤
│ Mode: design-review · qa · accessibility                   │ ← 模式摘要栏（常驻一行）
└────────────────────────────────────────────────────────────┘
```

焦点态展开详情（含变量与提示 + 分隔线）
```
┌────────────────────────────────────────────────────────────┐
│ [ 输入框 ]                                                │
├────────────────────────────────────────────────────────────┤
│ ⏎ send   ⇧⏎ newline   ⌃T transcript   ⌃C quit   …         │
├────────────────────────────────────────────────────────────┤
│ Mode: design-review · qa · accessibility                   │ ← 标签摘要（常驻）
│ ▌ design-review ⚠ [role=?] [region=emea]                   │ ← 展开详情（在摘要行“上方”增行）
│ ▸ design-review ⚠ [role=?] [region=emea]                   │ ← 当前焦点、⚠ 表示变量缺失
│ ○ qa              [target=staging] [retries=2]             │
│ ○ accessibility   [level=aa]                               │
│ ──────────────────────────────────────────────────────────── │ ← 分隔线（dim）
│ ←→ Vars   ⏎ Edit   ↑↓ Select(enum)   Space Toggle   Esc Exit │ ← 键位提示
└────────────────────────────────────────────────────────────┘
```

展开详情下的模式正文（滚动区）
```
design-review ⚠
  scope: project:vibe
  description: Review code with ticket context
  vars:
    • role     [?] required  shortcuts: -r, role=
    • region   [emea]
  body preview:
    Ensure code references ticket {{ticket}}; role={{role}}.
```

就地编辑（inline）
```
┌──────────────────────────────┐
│ design-review • role: [____] │ ← type=text, required
│ Enter Apply   Esc Cancel      │
└──────────────────────────────┘
```

表单面板（popup）
```
┌────────────────────────────────────┐
│  Edit design-review                │
│  role       [_____________] (必填) │
│  region     ( ) emea  (*) apac     │
│                                    │
│  [Cancel]               [ Apply ]  │
└────────────────────────────────────┘
```

快照与稳定性
- 覆盖摘要/详情/错误三态；顺序稳定（模式启用时间升序、变量声明顺序）。
- 所有文本 wrap 统一使用 `tui/src/wrapping.rs` 辅助（word_wrap_lines / word_wrap_line）。详情首行缩进为 `▌ `，后续缩进为两个空格。
- 行内变量列表序列化为 `key=value`，以逗号+单空格分隔；不因 UI 宽度变化而改变顺序。
- 空白规范：避免尾随空格；空行不超过 1 行；换行统一 LF。

错误提示与编号（TUI 前拦截）
- 必填缺失：`E3101 RequiredMissing: /id/var`
- 枚举不匹配：`E3102 EnumMismatch: name=value (allowed: a|b|c)`
- 布尔非法：`E3106 BooleanInvalid: name=value`（仅 true/false）
- 数字非法：`E3107 NumberInvalid: name=value`（i64/f64 可解析）
- 路径非法：`E3108 PathInvalid: name=value`（非空、无控制字符）
- 模板渲染失败：`E3201 TemplateError: …`（由渲染函数返回，TUI 转为历史提示）

模式摘要栏（Persistent Mode Summary Bar）
- 位置：固定在辅助栏之上 1 行；当“已开启模式数量 > 0”时常驻显示。
- 内容：`Mode: name1 · name2 · …`；当前焦点加粗；非当前 `.dim()`。
- 变量预览：摘要行默认不显示变量；按 `d` 展开后，在摘要行之上显示 1–3 行详情（scope/vars/body preview）；收起后仅保留摘要行。
- 高度：摘要态固定 1 行；详情态 = 1（摘要）+ wrap(scope) + wrap(vars) + wrap(body)；不足高度时优先保留摘要行。
 - 隐藏规则：关闭所有模式或收到空字符串 `UpdateModeSummary("")` 时，App 层折叠为 `None`，UI 隐藏“分隔线+摘要行”。

底部提示文案（统一英文）
- 摘要栏提示（非编辑）：`d Details  Tab Modes  ←→ Vars  ⏎ Edit  Space Toggle  Esc Exit`
- 详情展开状态：`d Hide details  Tab Modes  ←→ Vars  ⏎ Edit  Space Toggle  Esc Exit`
- 编辑态（枚举）：`↑↓ Select  ⏎ Apply  Esc Cancel`
- 编辑态（普通）：`⏎ Apply  Esc Cancel`
- 提示行呈现时，上方会绘制一条 dim 分隔线用于与详情内容区隔。
实现与插桩（最小差异准则）
- 扩展点：以 `BottomPaneAddon` 实现 ModeBar/Panel/摘要行的渲染与按键处理；宿主仅注入 3 个调用点（height/render/keys）；
- 事件面：优先移除 `UpdateModeSummary`/`UpdatePersistentModeState`，扩展直接调用 `BottomPane.set_mode_summary(..)` 与扩展内状态；`OpenModeBar` 可在短期保留，后续下沉到扩展按键；
- 生命周期：在 `on_session_configured` 通过 `AppLifecycleHook` 同步当前渲染与状态；
- 快照边界：仅 `tui/src/modes/**` 新增快照，非模式快照保持不变；
- 等价与去抖：通过 `codex-modes::normalize_equiv` 与 `codex-modes::Debouncer` 归口，两个视图共用；
- 自动启用：仅在“非恢复会话”且所有必填变量可从默认值满足时静默启用；渲染后不产生历史噪声。
