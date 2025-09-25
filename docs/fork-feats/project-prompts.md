# 项目级 Prompts

项目级 Prompts 允许你在不同仓库放置专属模板，同时保留全局默认。

## 目录规则
- Codex 启动会话时会从当前目录一路向上查找 `.codex/prompts/`。
- 最后追加全局目录 `$CODEX_HOME/prompts/`（默认为 `~/.codex/prompts/`）。
- 同名文件由“距离当前目录最近”的版本覆盖远端或全局版本。

示例结构：
```text
~/projects/foo/.codex/prompts/review.md   # /review（项目定义）
~/.codex/prompts/review.md               # /review（全局后备）
```
在 `foo` 仓库中打开 Codex 时，Slash `/review` 会使用项目定义；切换到其他仓库则落回全局版本。

## 创建步骤
1. 新建 `.codex/prompts/your-prompt.md`。
2. 文件名（不含 `.md`）即 Slash 名称，例如 `deploy.md` ⇒ `/deploy`。
3. 填写 Markdown 内容；保存后重新启动 Codex 会话即可生效。

## 使用方式
- 在输入框中输入 `/` 打开 Slash 菜单，输入名称或按 `Tab` 自动补全。
- 选择后按 `Enter` 提交整个 Markdown 文本；若想编辑，可在提交前先复制内容到输入框。

## 常见问题
- 新增或修改 Prompt 后需要开启新会话才能被发现。
- 非 `.md` 文件或不合法文件名会被忽略。
- 与内置命令（如 `/init`）同名的 Prompt 会自动跳过，避免冲突。

更深入的合并顺序、代码位置与容错策略，请参考 `docs/feats/design/project-prompts.md`。
