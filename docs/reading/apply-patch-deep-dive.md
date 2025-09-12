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

## 时序流程（10 步到位）

1. 模型产出工具调用：以 Function/CustomTool 或 `shell` 形式触发 `apply_patch`（可能是 heredoc 形态）。
2. 命令预识别：`core/src/codex.rs::handle_container_exec_with_params()` 调 `codex_apply_patch::maybe_parse_apply_patch_verified(argv, cwd)` 尝试识别为补丁。
3. Bash/heredoc 提取：`extract_apply_patch_from_bash()` 用 Tree‑sitter Bash 查询，支持可选的 `cd <path> &&` 前缀，抓取 heredoc 体与工作目录。
4. 语法解析：`parser.rs::parse_patch()` 解析为 `Vec<Hunk>`（Add/Delete/Update+Move，`@@` 块等），宽松模式自动剥离 heredoc 包裹。
5. 预计算变更：`maybe_parse_apply_patch_verified()` 基于磁盘现状构造 `ApplyPatchAction`：
   - Add/Delete 读取原/新内容；
   - Update 经 `unified_diff_from_chunks()` 生成新内容与 `unified_diff` 预览。
6. 安全评估：`assess_patch_safety()` 依据审批策略与沙箱策略判断 AutoApprove/AskUser/Reject，并检查是否仅触达可写根。
7. 用户审批（可选）：`request_patch_approval()` 展示变更摘要，等待 Approve/Deny/ApproveForSession。
8. 受控执行封装：将命令重写为 `[ <codex_path>, "--codex-run-as-apply-patch", <PATCH> ]`，并选择沙箱（Seatbelt/Landlock/None）。
9. 子进程应用补丁：`arg0` 分发命中隐藏参数 → `codex_apply_patch::apply_patch()` → `apply_hunks_to_files()` 落盘 → `print_summary()` 输出 A/M/D 摘要。
10. 汇总回传：发送 `PatchApplyEnd` 与本回合 `TurnDiff`，并以 `FunctionCallOutput{content, success}` 反馈给模型与 UI。

## 核心类型与函数对照

- 解析建模：
  - `ApplyPatchArgs`：原始补丁文本 + `hunks` + 可选 `workdir`。
  - `Hunk::{AddFile, DeleteFile, UpdateFile}`；`UpdateFileChunk{ change_context, old_lines, new_lines, is_end_of_file }`。
  - `parse_patch()`/`parse_one_hunk()`/`parse_update_file_chunk()`/`extract_apply_patch_from_bash()`。
- 识别与验证：
  - `maybe_parse_apply_patch()`（argv 粗识别）→ `maybe_parse_apply_patch_verified()`（读取磁盘、构造 `ApplyPatchAction`）。
  - `ApplyPatchAction`、`ApplyPatchFileChange::{Add,Delete,Update{unified_diff,move_path,new_content}}`。
- 匹配与替换：
  - `compute_replacements()`：逐块顺序定位替换区间；
  - `seek_sequence::seek_sequence()`：严格匹配 → 忽略尾空白 → 忽略首尾空白 → Unicode 标点/空白归一化匹配；支持 EOF 对齐与“去掉末尾空行哨兵”重试；
  - `apply_replacements()`：按“从后往前”应用替换，避免位置漂移。
- 落盘与展示：
  - `apply_hunks_to_files()`：创建目录、写/删/移；
  - `unified_diff_from_chunks*()`：以 `similar::TextDiff` 生成统一 diff；
  - `print_summary()`：输出 Git 风格摘要。
- 安全与审批：
  - `assess_patch_safety()`、`assess_command_safety()`、`convert_apply_patch_to_protocol()`；
  - `ApplyPatchExec` 与 `InternalApplyPatchInvocation`：决定直接输出/委托到受控 exec。

## 调试与测试建议

- 单元/集成测试：
  - 解析与落盘测试集中在 `apply-patch/src/lib.rs` 与 `apply-patch/src/parser.rs` 的 `#[test]` 中（Add/Delete/Update/Move、EOF 插入、首尾行替换、Unicode dash 等）；
  - 运行：`cargo test -p codex-apply-patch`。
- 行为观察：
  - 成功时 stdout 固定以 `Success. Updated the following files:` 开头，并按 A/M/D 枚举文件；
  - 解析/匹配/IO 失败会写入 stderr 并返回非 0，消息含定界符/行号/文件路径，便于快速定位；
  - 通过 `core` 路径执行时，还会看到 `PatchApplyBegin/End` 与整回合 `TurnDiff` 事件。
- 常见排错：
  - heredoc 被当作字面量：宽松模式会剥离 `<<'EOF' ... EOF`，但需保证首尾标记匹配；
  - 命中 EOF 的替换：若 `old_lines`/`new_lines` 末尾包含空字符串（表示终止换行），匹配失败时会自动“去尾重试”；
  - 空补丁或 Update 无块：解析期即报错；
  - 写路径不在可写根：根据审批策略会 Ask/Reject。

## 实操：直接用 Codex 主程序体验 apply_patch

以下示例展示如何直接用 Codex 主程序二进制调用隐藏参数 `--codex-run-as-apply-patch` 来应用补丁（无需进入会话）。

### 准备

- 构建：
  - 开发构建：`cargo build -p codex-cli`
  - 发布构建：`cargo build -p codex-cli --release`
- 二进制位置：
  - 开发：`codex-rs/target/debug/codex`
  - 发布：`codex-rs/target/release/codex`

建议在一个临时目录演示（相对路径以当前工作目录为准）。

### 示例 1：新增文件（macOS/Linux）

```bash
mkdir -p /tmp/codex-patch-demo && cd /tmp/codex-patch-demo

PATCH="$(cat <<'EOF'
*** Begin Patch
*** Add File: demo.txt
+Hello
+Codex
*** End Patch
EOF
)"

# 任选一种方式运行（发布构建或 cargo 直接运行）
<repo>/codex-rs/target/release/codex --codex-run-as-apply-patch "$PATCH"
# 或者：
cargo run -p codex-cli -- --codex-run-as-apply-patch "$PATCH"

cat demo.txt  # 应看到两行：Hello / Codex
```

预期 stdout：

```
Success. Updated the following files:
A /tmp/codex-patch-demo/demo.txt
```

### 示例 1（Windows/PowerShell）

```powershell
New-Item -ItemType Directory -Force -Path $env:TEMP\codex-patch-demo | Out-Null
Set-Location $env:TEMP\codex-patch-demo

$patch = @'
*** Begin Patch
*** Add File: demo.txt
+Hello
+Codex
*** End Patch
'@

<repo>\codex-rs\target\release\codex.exe --codex-run-as-apply-patch $patch
Get-Content .\demo.txt
```

### 示例 2：修改并重命名文件（macOS/Linux）

```bash
# 基于上一步已生成 demo.txt

PATCH2="$(cat <<'EOF'
*** Begin Patch
*** Update File: demo.txt
*** Move to: demo-renamed.txt
@@
-Hello
+Hello, Codex!
*** End Patch
EOF
)"

<repo>/codex-rs/target/release/codex --codex-run-as-apply-patch "$PATCH2"

test ! -f demo.txt && echo "old removed ok"
cat demo-renamed.txt
```

### 示例 2（Windows/PowerShell）

```powershell
$patch2 = @'
*** Begin Patch
*** Update File: demo.txt
*** Move to: demo-renamed.txt
@@
-Hello
+Hello, Codex!
*** End Patch
'@

<repo>\codex-rs\target\release\codex.exe --codex-run-as-apply-patch $patch2
Test-Path .\demo.txt   # 应为 False
Get-Content .\demo-renamed.txt
```

### 示例 3：在文件末尾追加（命中 EOF）

```bash
PATCH3="$(cat <<'EOF'
*** Begin Patch
*** Update File: demo-renamed.txt
@@
+Appended line
*** End of File
*** End Patch
EOF
)"

<repo>/codex-rs/target/release/codex --codex-run-as-apply-patch "$PATCH3"
tail -n +1 demo-renamed.txt
```

### 补充说明

- 参数与 stdin：有参时整个补丁必须作为“单个参数”传入；无参时从 stdin 读完整补丁。
- heredoc/Here-String：
  - Bash/Zsh 建议用单引号 heredoc 再 `$(...)` 包装为一个参数；
  - PowerShell 建议用 Here-String（`@'... '@`）存入变量后传参。
- 与会话模式差异：`--codex-run-as-apply-patch` 直达执行体，不走会话内的审批/沙箱/事件流。若要体验完整治理链路，请在会话中通过 shell 工具执行 `apply_patch <<'EOF' ... EOF`。

## 格式对比：apply_patch vs git diff 与同类工具

### 概览

- apply_patch：为 LLM/自动化场景定制的精简补丁语言，强调可读、可审计、易解析、对上下文匹配容错。
- git diff/unified diff：业界通用标准（Git/patch 工具链），信息更丰富（行号/偏移、权限、重命名检测、二进制等），生态兼容性最佳。
- 其他同类：JSON Patch（结构化 JSON 文档）、ed 脚本、Mercurial patch 等，各有适用面。

### 语法对比（新增文件示例）

- apply_patch：

```
*** Begin Patch
*** Add File: demo.txt
+Hello
+Codex
*** End Patch
```

- git diff：

```
diff --git a/demo.txt b/demo.txt
new file mode 100644
index 0000000..aaaaaaaa
--- /dev/null
+++ b/demo.txt
@@ -0,0 +1,2 @@
+Hello
+Codex
```

主要差异：apply_patch 用“动作 + 行前缀”表达，无行号/偏移；git diff 含文件头/索引/模式与 hunk 行号。

### 定位/匹配策略

- apply_patch：不依赖行号/偏移；逐级宽容匹配（精确 → 忽略行尾空白 → 忽略首尾空白 → Unicode 标点/空白归一化），支持 `*** End of File`。更稳健于源文件存在微差异的场景（LLM 产出友好）。
- git diff/patch：依赖 hunk 行号与上下文，具备 fuzz（上下文错位容忍）。更接近“可复现的版本控制语义”，生成时对 LLM 更苛刻。

### 文件级元数据

- apply_patch：动作显式（Add/Delete/Update），Update 可 `*** Move to:` 表示重命名；不携带权限/模式等元数据；推荐相对路径，Codex 会做安全校验。
- git diff：提供文件模式、权限、重命名/复制检测、子模块、symlink 等丰富元信息。

### 二进制与编码

- apply_patch：面向 UTF‑8 文本；不支持 Git binary patch/二进制增量。
- git diff：支持二进制补丁、行尾规范化、.gitattributes 等。

### 安全与执行

- apply_patch：与 Codex 安全流深度集成（解析预演 → 路径安全评估 → 沙箱/审批 → 受控执行 → 变更摘要/统一 diff）。
- git diff：工具链本身不包含审批/沙箱，可结合 CI/Hook 实现治理。

### 生态与可移植

- apply_patch：专为 Codex 设计，需通过 Codex 的实现应用；适合代理/自动化回路内演进。应用后仍可用 Git 工作流提交/审阅。
- git diff：行业通用，最适合跨团队/平台/工具传播与审阅。

### 何时选用

- 选 apply_patch：LLM 自动修复/重构、交互式代理、需要稳健定位和统一治理（审批/沙箱/可视化）的场景。
- 选 git diff：对外交换补丁、走标准代码评审与 CI 流程、需要权限/模式/二进制/重命名检测等高级特性。
