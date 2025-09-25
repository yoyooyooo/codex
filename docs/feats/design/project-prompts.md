# 项目级 Prompts

## 背景与范围
- upstream 仅识别 `$CODEX_HOME/prompts/`。Fork 引入“项目优先”的检索链，以便不同仓库持有自己的模板。
- 核心实现位于 `codex-rs/core/src/custom_prompts.rs`，UI/CLI 共用，协议层维持不变。
- 所有新增逻辑保持 `// !Modify:` 标记（主入口在 `codex-rs/core/src/codex.rs:1480+`）。

## 目录检索算法
1. `discover_custom_prompts` 调用 `discover_prompts_in`/`discover_prompts_in_excluding`：
   - 输入目录：`cwd` → `cwd` 父级们（含 Git worktree root） → `$CODEX_HOME/prompts/`。
   - 每层使用 `std::fs::read_dir` 扫描，仅保留 `.md` 文件并按文件名排序。
   - 利用 `HashSet<String>` 过滤内置命令名（由调用方传入）。
2. `codex.rs` 中的 `Op::ListCustomPrompts` 分支按“靠近 cwd 的覆盖远端”的顺序合并：
   - 初始列表：全局目录（若存在）。
   - 迭代 `cwd` 父链，自上而下覆盖同名项。
   - 结果写入 `EventMsg::ListCustomPromptsResponse.custom_prompts`，由 TUI/CLI Slash 菜单消费。

## 数据结构
- `CustomPrompt`（`codex_protocol::custom_prompts`）包含 `name`、`path`、`content`。
- Slash 菜单在 UI 层根据 `path` 推导 scope label：
  - 如果路径位于 `$CODEX_HOME/prompts/` ⇒ `global`。
  - 否则取最近的父目录名作为 `project:<dirname>`。
- CLI 侧 `prompts/list` MCP 事件沿用同一结构。

## 容错策略
- 文件读取失败、非 UTF-8 内容、`.md` 以外扩展名统一跳过（`discover_prompts_in` 内部 `continue`）。
- 非法文件名（例如含空格/冒号）仍会出现在 Slash 名称中，上层约束通过“仅 `[A-Za-z0-9_-]`”文档要求解决；若未来需要自动过滤，可在 `discover_prompts_in` 增加校验。
- 缓存策略：目前无 watcher，新增/修改文件需重启会话；避免在 TUI 循环中引入额外 IO。

## 最小差异准则
- 核心只新增 `custom_prompts.rs` 助手函数与 `Op::ListCustomPrompts` 的目录遍历逻辑；其余代码不触碰。
- 仓库根不再存放 `.codex/prompts/**` 示例，由 `docs/fork-feats/project-prompts.md` 提供说明，避免与 upstream 文档冲突。
- 若 upstream 引入同类特性，可比较 `discover_prompts_in` 与 upstream 实现做对齐，保留 fork 特有的覆盖顺序。

## 同步与回归检查
- 每次同步 upstream 时：
  1. 检查 `codex-rs/core/src/codex.rs` 中 `Op::ListCustomPrompts` 是否有结构调整。
  2. 确认 `codex_protocol` 的 `ListCustomPromptsResponseEvent` 未发生签名变化。
  3. 重新运行 `cargo test -p codex-core`（若存在）与 `cargo test -p codex-tui`，确保 Slash 菜单依旧读取多目录。
- 若后续要支持“实时刷新”，可在现有实现上挂钩文件 watcher；设计时需考虑 sandbox（`CODEX_SANDBOX_*`）限制。

## 手动验证脚本
- 在临时目录执行：
  ```bash
  mkdir -p ~/.codex/prompts
  echo '全局' > ~/.codex/prompts/review.md
  mkdir -p /tmp/project/.codex/prompts
  echo '项目' > /tmp/project/.codex/prompts/review.md
  cd /tmp/project
  jcodex  # 或 TUI，输入 /review
  ```
  预期：Slash 菜单显示项目版本，并在 tooltip 中标注 `project:project`。
- 删除项目文件后重新启动会话，应回退到全局版本。
