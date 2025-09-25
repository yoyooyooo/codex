# 自定义 Mode

自定义 Mode 让 `/name` Slash 命令既可以一次执行，也可以作为“常驻模式”持续影响对话。它的设计目标是：在不同项目之间复用规范化的对话约束，并保持与上游 CLI 的兼容行为。

## 适用场景
- 为团队准备统一的代码审查、需求评审、翻译等操作手册。
- 在长会话里叠加多个约束，比如同时启用 `design-review` 和 `security-check`。
- 为一次性的脚本任务准备临时模式，执行后立即结束。

## 如何定义
1. 在项目或全局目录建立 `.codex/modes/`，可按子目录分组：
   ```text
   .codex/modes/
     ├── design/
     │   └── review.md   # /design:review
     └── qa.md          # /qa
   ```
2. 新建 Markdown 文件，使用 YAML frontmatter 描述：
   ```markdown
   ---
   kind: persistent      # 或 instant
   display_name: Design Review
   description: 审查 design diff
   variables:
     - name: ticket
       required: true
       shortcuts: ["-t", "ticket="]
     - name: focus
       enum: ["ui", "logic", "copy"]
   ---
   审查流程：
   1. 阅读 diff …
   ```
3. 保存后重新启动 Codex 会话，在输入框输入 `/` 即可看到新模式。

## Slash 菜单行为
- Codex 会从当前目录一路向上搜集 `.codex/modes/` 并合并 `$CODEX_HOME/modes/`；离当前目录最近的定义优先生效。
- Slash 列表中会显示来源标签，例如 `(project:repo-name)` 或 `(global)`，方便识别覆盖关系。
- `persistent` 模式启用后会显示在底栏的 `Mode: …` 摘要中；`instant` 模式执行完毕立即结束。

## TUI 操作速查
- `Alt+B` 或在历史末尾按 `↓` 打开 ModeBar。
- `Space` 启用/禁用模式，`Enter` 编辑变量，`Tab`/`Shift+Tab` 切换模式卡片，`Esc` 退出。
- 轻量选择面板（ModePanel）通过 `/` ⇒ `Tab` 打开，`Space` 勾选模式，`Enter` 应用。

## CLI/脚本使用
- CLI 同步使用 Slash 语义：在终端输入 `/design:review ticket=FOO` 可直接触发瞬时模式。
- 若需要在自动化脚本中复用模式内容，可直接读取 `.codex/modes/*.md` 文件并拼接到自定义请求。

## 常见问题
- **模式看不到？** 确认文件扩展名为 `.md` 且保存为 UTF-8，重新开启会话。
- **变量提示缺失？** 检查 frontmatter 中是否标记 `required: true`，并通过 ModeBar 填写。
- **想共享给所有项目？** 将文件放入 `~/.codex/modes/`，所有仓库都会加载。

想了解解析细节与同步策略，请阅读对应的设计文档：`docs/feats/design/custom-mode.md`。
