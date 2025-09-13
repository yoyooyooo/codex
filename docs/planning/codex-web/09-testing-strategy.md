# 测试策略文档

**文档版本**: v1.0  
**最后更新**: 2025-09-13  
**依赖文档**: [03-backend-design.md](03-backend-design.md), [04-frontend-design.md](04-frontend-design.md), [06-api-specification.md](06-api-specification.md), [07-security-design.md](07-security-design.md), [08-development-workflow.md](08-development-workflow.md)  
**后续文档**: [10-deployment.md](10-deployment.md)

## 目录
- [测试目标与范围](#测试目标与范围)
- [测试分层](#测试分层)
- [后端测试策略（Rust）](#后端测试策略rust)
- [前端测试策略（Web UI）](#前端测试策略web-ui)
- [契约测试与类型一致性](#契约测试与类型一致性)
- [端到端测试（E2E）](#端到端测试e2e)
- [性能与稳定性](#性能与稳定性)
- [安全与合规测试](#安全与合规测试)
- [覆盖率与质量门槛](#覆盖率与质量门槛)
- [CI 集成与基线](#ci-集成与基线)

## 测试目标与范围

- 确认 Web 端与 TUI 的核心能力等价（事件流、审批、补丁、历史/恢复）。
- 确认进程内复用的语义一致性（协议/Event/Submission 不变）。
- 确认本地安全边界不被突破（仅回环地址、Token、CORS/CSRF）。

## 测试分层

1) 单元测试（Unit）
- 针对纯函数与小型组件：事件缓存、聚合器、UI 纯渲染等。

2) 集成测试（Integration）
- 组件协同验证：会话注册表 ↔ 会话 ↔ WS 处理 ↔ 协议序列化。

3) 端到端测试（E2E）
- 从浏览器视角覆盖主要用户路径：会话创建、输入输出、审批、补丁应用、历史恢复。

## 后端测试策略（Rust）

- 单元测试
  - EventCache：顺序/去重/窗口裁剪；重连 `since_event_id` 取回。
  - Session：连接上限/订阅管理/心跳广播。
  - 错误类型与映射：`WebError` → HTTP 状态码。

- 集成测试
  - Axum 路由：`/api/sessions`、`/api/health`、`/api/sessions/{id}/apply_patch` 等返回语义。
  - WebSocket：连接 → 发送 Submission → 收到 Event（含乱序扰动样本）。
  - 历史接口：基于 `RolloutRecorder` 的列表与恢复入口（可用 test double）。

- 快照测试（可选）
  - 使用 `insta` 快照事件序列与归并输出，确保 UI 端可预测。

- 执行命令示例
```
cargo test -p codex-web
```

> 注：遵守现有跳过策略；不修改任何与 `CODEX_SANDBOX_*` 相关的逻辑与判断。

## 前端测试策略（Web UI）

- 单元/组件测试
  - 组件渲染：消息列表、diff 面板、审批对话、状态栏。
  - 状态更新：事件流 reducer 与去重逻辑。
  - 工具函数：ANSI → HTML、代码高亮块。

- 合成测试（集成）
  - 使用本地假 WS 服务或 mock 层回放事件样本；验证 UI 聚合一致。

- 执行命令示例（建议）
```
pnpm -F codex-web-ui test
```

## 契约测试与类型一致性

- 类型生成即契约：
  - 执行 `cargo run -p codex-cli -- generate-ts -o apps/codex-web-ui/src/protocol`。
  - 前端构建若类型不匹配即失败，作为契约门槛。

- JSON 负载校验：
  - 后端端到端集成测试中验证关键字段存在/语义一致。

## 端到端测试（E2E）

- 建议工具：Playwright 或 Cypress。
- 主要用例：
  - 会话创建 → 连接 WS → 发送用户消息 → 流式事件显示。
  - AskForApproval → 批准/拒绝 → 继续流转。
  - 生成补丁 → UI 预览 → 一键应用 → 结果事件与文件系统校验。
  - 历史页加载 → 选择条目 → 恢复进入会话。
  - 掉线与重连：断开网络/刷新页面，带 `last_event_id` 恢复。

- 执行命令示例（建议）
```
# 后端（dev-proxy）与前端（dev）分别启动后，再运行：
pnpm -F codex-web-ui test:e2e
```

## 性能与稳定性

- WS 压测：高频事件（stdout/stderr）场景下 95P 延迟 < 200ms。
- 长时间运行：4h+ 无内存泄漏（监控连接数/事件缓存）。
- 补丁一致性：预览 ≡ 实际落盘；失败无脏写（见 12-风险分析）。

## 安全与合规测试

- 本地限定：仅 `127.0.0.1` 监听验证；跨源请求/无 Token 请求被拒。
- 审批一致性：敏感操作未经审批不得执行；日志审计完整。
- 令牌管理：过期/错误 Token 行为正确；无信息泄露。

## 覆盖率与质量门槛

- 后端：核心模块单元/集成测试覆盖率 > 80%。
- 前端：关键组件/核心流程覆盖；E2E 覆盖主要用户路径。
- PR 门槛：新增/变更核心逻辑需附带相应测试。

## CI 集成与基线

- 任务建议：
  - 构建与类型生成（协议）
  - 后端 `cargo test -p codex-web`
  - 前端 `pnpm -F codex-web-ui test`
  - 可选 E2E（冒烟场景）

---
**变更记录**：
- v1.0 (2025-09-13): 初版骨架，覆盖分层策略与执行命令

