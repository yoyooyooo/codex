# 安全与边界

执行与权限
- 渲染结果仅作为 `<user_instructions>` 文本供模型使用；前端不执行命令。
- 任何 `!command`/`@path` 的实际执行继续受核心白名单与 Sandbox/Seatbelt 约束；本方案不放宽限制。

注入防护
- 变量值仅用于文本替换，不直接拼接到 shell 命令行。
- Slash 参数解析与就地编辑均保持参数分离；对关键字段建议配合 `pattern` 严格限制。

持久化与兼容
- 模式状态与变量值仅在前端会话内缓存；不写入核心会话文件；`resume` 需前端重放覆写。
- 与 `.codex/prompts` 并存；未启用模式时不注入 `<mode_instructions>`。
