# apply_patch 全链路原理梳理

本文面向开发者，梳理 Codex 在一次 `apply_patch` 调用中，从“模型产生工具调用”到“本机安全落盘并反馈结果”的完整链路与关键实现位置，帮助你在需要时快速定位与扩展。

## 总览

- 工具形态：`apply_patch` 既可作为“函数工具（function tool）”，也可作为“自由格式（custom/freeform）工具”。哪种形态由模型族与配置决定（`openai_tools.rs`）。
- 触发方式：
  - 模型直接发起 `FunctionCall{name: "apply_patch", arguments: ...}`；
  - 或通过 `shell`/`local_shell` 工具执行形如 `apply_patch "*** Begin Patch ..."` 的命令；
  - Responses API 下亦可用 `CustomToolCall{name: "apply_patch", input: <PATCH>}`（freeform）。
- 安全治理：所有补丁写盘前走统一的“补丁安全评估 + 审批 + 沙箱”流程（`core/src/safety.rs`、`core/src/apply_patch.rs`）。
- 实施方式：最终以“受控子进程”调用同一可执行体的 `apply_patch` 子功能（通过 arg0 trick 或 `--codex-run-as-apply-patch` 的隐藏参数），由 `codex-apply-patch` crate 解析补丁、落盘、输出摘要。
- 结果回注：结果作为 `FunctionCallOutput{content, success}` 回注入下一回合输入，UI 同时生成 `PatchApplyBegin/End` 与汇总 `TurnDiff` 事件。

## 工具暴露与模型侧调用

- 工具清单构建：`core/src/openai_tools.rs:get_openai_tools()` 汇总当前回合可用工具：
  - 依据配置/模型族决定是否提供 `apply_patch`，以及提供“函数工具”还是“自由格式”工具：
    - 函数工具：`create_apply_patch_json_tool()`，参数为 `{ input: string }`（整段补丁文本）。
    - 自由格式（custom/freeform）：`create_apply_patch_freeform_tool()`，约定 `format.syntax` 与 `definition`（Lark 语法式的补丁语言）。
  - Chat Completions 只支持 function 工具；自由格式仅在 Responses API 下使用，工具转换逻辑见 `create_tools_json_for_chat_completions_api()`。
- 工具描述：各工具包含简短 `description`，在 Responses/Chat 载荷中以 `tools` 字段声明，由模型原生的 function-calling 机制决定是否、何时调用。

## Codex 接收与分发（ResponseItem → 处理）

入口位于 `core/src/codex.rs`：

- 回合驱动：`run_turn()` → `try_run_turn()` 读取流事件并聚合为 `ResponseItem`。
- 分发处理：
  - `FunctionCall{name:"apply_patch"}` → `handle_function_call()` 的 `"apply_patch"` 分支；
  - `CustomToolCall{name:"apply_patch"}`（Responses 自由格式）→ `handle_custom_tool_call()`；
  - `LocalShellCall`/`FunctionCall{name:"shell"}` 且命令为 `apply_patch ...` → 走统一的执行路径 `handle_container_exec_with_params()`，其中首先“识别并前置处理补丁”。

### 识别与预解析补丁

- 统一在 `handle_container_exec_with_params()` 首行调用：
  - `codex_apply_patch::maybe_parse_apply_patch_verified(argv, cwd)`：
    - 支持两类 argv：
      1) `apply_patch "<PATCH>"`
      2) `bash -lc "apply_patch <<'EOF' ... EOF"`（lenient 模式自动剥离 heredoc）
    - 返回 `MaybeApplyPatchVerified`：
      - `Body(ApplyPatchAction)`：提取到补丁，且读取了目标文件原始内容，构造了“将发生哪些文件变更”的结构化结果；
      - `CorrectnessError(...)`：补丁语法/一致性错误；
      - `ShellParseError(...)` 或 `NotApplyPatch`：非补丁调用或无法判定。

若识别为补丁（`Body`），会：
- 生成用于 UI 的“文件变更摘要”`HashMap<PathBuf, FileChange>`（`convert_apply_patch_to_protocol()`），并在开始时发出 `PatchApplyBegin` 事件；
- 将该摘要喂给 `TurnDiffTracker::on_patch_begin()` 记录基线，用于回合结束后计算聚合统一 diff。

## 安全评估与审批（在真正落盘前）

位置：`core/src/apply_patch.rs` + `core/src/safety.rs`

流程：
1) `assess_patch_safety(action, approval_policy, sandbox_policy, cwd)`：
   - 空补丁直接拒绝。
   - 若补丁写入路径全部落在可写根（`SandboxPolicy::WorkspaceWrite` 的 `writable_roots`）内，且平台支持沙箱（macOS Seatbelt / Linux Landlock），则可“自动批准 + 沙箱执行”。
   - DangerFullAccess 或 `AskForApproval::OnFailure` 等策略会放宽到“自动批准（可无沙箱）”或“失败后再询问”。
   - 其他情况进入 `AskUser`：弹审批对话（包含文件变更摘要），用户 Deny/Approve/ApproveForSession。

2) 判定结果：返回 `InternalApplyPatchInvocation`：
   - `DelegateToExec(ApplyPatchExec{ action, user_explicitly_approved_this_action })`：进入受控执行（下一节）。
   - 或直接构造 `FunctionCallOutput{ success:false, content:"patch rejected..." }` 返回模型（例如被拒绝）。

## 受控执行：如何真正“应用补丁”

即使是通过 `shell` 调用的 `apply_patch`，Codex 也不会直接调用用户 PATH 中的某个二进制，而是把执行收口到当前 Codex 可执行体内部的“子命令”，确保行为一致且可沙箱。

关键点：

- arg0 trick（`codex-rs/arg0`）：
  - 在进程启动时，临时把一个指向当前可执行体的 `apply_patch` 符号链接（或 Windows 批处理）放入 PATH 最前，做到“像独立命令一样可被调用”。
  - 同时支持隐藏参数 `--codex-run-as-apply-patch`（常量 `CODEX_APPLY_PATCH_ARG1`），允许直接以当前可执行体 + 隐藏参数 + 补丁文本的方式调用。
  - `arg0_dispatch_or_else()` 根据 argv0/argv1 判定：若以 `apply_patch` 名称启动或带隐藏参数，则直接跳转到 `codex_apply_patch::main()/apply_patch()`，绕过 CLI 主逻辑。

- Codex 构造执行命令（`core/src/codex.rs`）：
  - 若识别到补丁并通过审批：将命令重写为：
    - `[ <path_to_codex>, "--codex-run-as-apply-patch", <PATCH> ]`
  - 并按安全判定设置 `SandboxType`（Seatbelt/Landlock/None），通过 `process_exec_tool_call()` 以受控子进程方式执行。
  - 为 `apply_patch` 关闭流式 stdout 事件，仅在结束时汇总（`stdout_stream: None`）。

- `codex-apply-patch` 真正落盘（`apply-patch/src/lib.rs`）：
  - 解析补丁：`parser.rs` 以 Lark 语法为准（Begin/End Patch，Add/Delete/Update/Move，`@@` 变更块，`*** End of File`），并提供 lenient 模式（自动剥离 heredoc 包装）。
  - Update 计算：`compute_replacements()` 结合 `seek_sequence` 做定位/模糊匹配（含常见标点归一），生成替换列表后写回。
  - 写盘与摘要：创建目录、写文件/删除/重命名，最后打印“成功摘要”（A/M/D + 路径）到 stdout，错误写入 stderr 并返回非 0。

## 事件与结果汇总

- 开始事件：`PatchApplyBegin{ auto_approved, changes }`（在“执行前”根据解析结果构造）。
- 结束事件：`PatchApplyEnd{ success, stdout, stderr }`（来自受控子进程的真实输出）。
- 汇总 diff：`TurnDiffTracker::get_unified_diff()` 对本回合内所有补丁的基线与当前磁盘状态计算统一 diff，并以单条 `TurnDiff` 事件输出（按 Git 风格 headers，文本/二进制自动区分，重命名跟踪）。
- 返回模型：`FunctionCallOutput{ content, success }` 回注到下一回合输入；自由格式路径会被转换为 `CustomToolCallOutput` 保持协议一致。

## 与其他路径的协作关系

- shell 路径：若模型以 `shell` 工具调用 `apply_patch`，Codex 会优先“识别补丁 → 预解析 → 审批 → 受控落盘”，而非把 `apply_patch` 当作普通命令直接跑。这保证了统一的安全策略与 UI 反馈。
- Chat vs Responses：
  - Chat Completions 仅 function 工具；Responses 既支持 function，也支持 custom（freeform）。不论哪种，最终都会流向上述受控执行流程。

## 常见问题与边界

- 相对路径解析：`maybe_parse_apply_patch_verified()` 会结合 `cwd` 解析相对路径，确保写入位置明确；审核时也使用解析后的绝对路径集合判定是否落在可写根内。
- 空补丁/格式错误：直接以结构化失败反馈给模型，鼓励重采样。
- 跨文件移动：Update hunk 支持 `*** Move to:`，执行层会相应地写入新路径并删除旧路径；TurnDiff 能正确显示 rename。
- 无沙箱环境：若平台缺乏沙箱且策略要求“可信才无沙箱”，会降级为“询问用户审批”。如果配置了 DangerFullAccess，则可直接无沙箱执行。

## 关键代码导航

- 工具构建与声明：`core/src/openai_tools.rs`（`create_apply_patch_*` / MCP 工具转化）
- 回合驱动与分发：`core/src/codex.rs`（`run_turn`/`handle_*`/`handle_container_exec_with_params`）
- 审批与沙箱策略：`core/src/apply_patch.rs`、`core/src/safety.rs`、`core/src/exec.rs`、`core/src/landlock.rs`、`core/src/seatbelt.rs`
- 补丁解析与落盘：`apply-patch/src/parser.rs`、`apply-patch/src/lib.rs`
- 汇总 diff：`core/src/turn_diff_tracker.rs`
- arg0 trick：`arg0/src/lib.rs`

## 小结

`apply_patch` 的设计目标是“强约束、强治理、强可视化”：
- 强约束：以专用补丁语法 + 解析器兜底，避免纯文本 diff 的脆弱性；
- 强治理：统一的审批与沙箱策略把风险扼杀在执行前；
- 强可视化：开始/结束事件与统一 diff 让补丁影响一目了然；

同时保持对模型侧的最大兼容：function/custon/shell 多入口一致落盘，开发者不必关心入口细节即可获得一致的安全与体验保障。

