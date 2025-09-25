# Fork Feature Docs

目标：为每个 Fork 专属特性同时编写用户文档与设计文档，保持与上游最小差异的同时，确保未来同步/扩展有据可依。

## 收集信息
1. 读取最新 commit/PR 中涉及的功能改动，确认行为范围。
2. 梳理交互、配置、路径、入口快捷键，识别是否影响 CLI、TUI、SDK 或核心协议。
3. 标记所有 `// !Modify:` 注入点与关键文件，注意上游同步时的潜在冲突区。

## 输出结构
- 用户文档（`docs/fork-feats/<feature>.md`）
  - 用语面向最终使用者，强调场景、步骤、提示、常见问题。
  - 保持轻量示例（代码块/截图路径即可），禁止泄露实现细节。
  - 结尾指向设计文档。
- 设计文档（`docs/feats/design/<feature>.md`）
  - 面向维护者，详述数据流、状态机、关键函数、`// !Modify:` 钩子位置。
  - 说明与 upstream 的差异、同步时的检查项、测试与快照要求。
  - 若涉及多 crate，分段描述（core/tui/cli/...）。

## 更新 README
- 在 `README.md` 与 `README.zh-CN.md` 的 Fork 特性列表里新增该特性。
- 每个特性条目链接用户文档 + 设计文档，保持简洁概述。

## 质量检查
- 全文使用 ASCII 与项目既有术语。
- 确认新文档路径已加入 git 并无拼写错误。
- `git status` 检查，确保未误动上游文件。

## 交付清单
- `docs/fork-feats/<feature>.md`
- `docs/feats/design/<feature>.md`
- 更新过的 `README.md`、`README.zh-CN.md`

完成后在总结中说明未跑测试（如属文档改动），并列出后续可能的验证建议。
