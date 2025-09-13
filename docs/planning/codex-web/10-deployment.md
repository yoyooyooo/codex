# 部署与打包方案

**文档版本**: v1.0  
**最后更新**: 2025-09-13  
**依赖文档**: [02-architecture.md](02-architecture.md), [03-backend-design.md](03-backend-design.md), [04-frontend-design.md](04-frontend-design.md), [05-cli-integration.md](05-cli-integration.md), [06-api-specification.md](06-api-specification.md), [07-security-design.md](07-security-design.md)  
**后续文档**: [11-implementation-roadmap.md](11-implementation-roadmap.md), [13-acceptance-criteria.md](13-acceptance-criteria.md)

## 目录
- [打包目标](#打包目标)
- [产物形态](#产物形态)
- [前端构建](#前端构建)
- [后端构建](#后端构建)
- [静态资源交付策略](#静态资源交付策略)
- [运行与启动](#运行与启动)
- [安全加固](#安全加固)
- [发布流程建议](#发布流程建议)
- [验证清单](#验证清单)

## 打包目标

- 单机本地运行，默认仅 `127.0.0.1` 可访问。
- `codex` 二进制提供 `web` 子命令，开箱可用。
- 不引入远程多用户部署；不修改 `CODEX_SANDBOX_*` 行为。

## 产物形态

1) 二进制：`codex`（包含 `web` 子命令）
2) 可选内嵌静态资源：启用 `embed-assets` feature 将前端产物嵌入 `codex-web` 中
3) 可选外部静态资源目录：通过 `--static-dir` 指向前端 `dist/`

## 前端构建

```
pnpm -F codex-web-ui install
pnpm -F codex-web-ui build
# 产物位于 apps/codex-web-ui/dist/
```

- 构建前建议执行协议类型生成：
```
cargo run -p codex-cli -- generate-ts -o apps/codex-web-ui/src/protocol
```

## 后端构建

### 带内嵌资源（推荐生产）
```
cargo build -p codex-web --release --features embed-assets
```
- 优点：单文件部署，避免前端资源缺失。
- 运行时无需 `--static-dir`。

### 外部目录（开发/调试）
```
cargo build -p codex-web --release
codex web --static-dir ./apps/codex-web-ui/dist
```
- 优点：替换前端资产无需重编后端。

## 静态资源交付策略

- 优先使用内嵌（`embed-assets`）；如包体过大或需频繁替换，使用外部目录。
- 开发期使用 `--dev-proxy`，由 Vite 处理前端与热更新。

## 运行与启动

### 通过 CLI 启动（建议入口）
```
codex web --host 127.0.0.1 --port 0
# 如需外部静态目录：
codex web --static-dir ./apps/codex-web-ui/dist --host 127.0.0.1 --port 0
```
- 默认随机端口、自动打开浏览器（可用 `--no-open` 关闭）。
- 日志写入现有 `codex_home` 日志目录（建议区分 `codex-web.log`）。

### 健康检查与自检
- `GET /api/health`：状态、会话数、时间戳。
- 首次加载页面确认前端资源正确加载（无 404/跨域错误）。

## 安全加固

- 仅绑定回环地址（默认 `127.0.0.1`），谨慎暴露 `--host`。
- 启动生成一次性 `access_token`，通过 `Authorization: Bearer` 校验。
- CORS 收紧到本地源；设置 `X-Frame-Options: DENY`，合理配置 CSP。
- 遵守现有沙箱与审批策略；不新增也不修改 `CODEX_SANDBOX_*` 逻辑。

## 发布流程建议

1) 构建矩阵（CI）
- 前端：`build` + 产物校验（哈希/大小）
- 后端：`cargo build -p codex-web --release [--features embed-assets]`
- 测试：`cargo test -p codex-web`、`pnpm -F codex-web-ui test`、（可选）E2E 冒烟

2) 产物打包
- 将 `codex` 可执行文件与必要的 LICENSE/README 一并打包
- 若使用外部静态资源，附带 `dist/` 并在文档中说明 `--static-dir` 用法

3) 版本标识
- 后端与前端版本通过 CI 注入（例如构建时间/commit hash），便于问题追踪

## 验证清单

- 能运行 `codex web` 并自动打开浏览器
- 能创建会话、连接事件流、提交交互、预览与应用补丁
- 历史列表与恢复可用
- Token 校验有效，跨源请求被拒
- （可选）内嵌/外部静态资源两种模式均可用

---
**变更记录**：
- v1.0 (2025-09-13): 初版骨架，覆盖构建/运行/安全与发布流程

