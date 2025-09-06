# Codex CLI（Fork）

<p align="left">语言：<a href="README.md">English</a> | <a href="README.zh-CN.md">中文（简体）</a></p>

本仓库是上游项目的维护型分叉（fork）：
https://github.com/openai/codex

产品文档、安装方式和通用使用说明请以上游仓库为准。本 fork 专注于在尽量贴近上游的同时，提供更适合 fork 场景的“同步与发布”工作流改造，以及少量定制功能。

## 专属特性（本 Fork）

- TUI：双击 Esc 可打开“用户提问节点选择器”，更快选择提问路由（最近新增）。

## 安装与命令名（Fork）

- npm 包名：`@jojoyo/codex`
- 全局命令：`jcodex`（为避免与上游 `codex` 冲突而重命名）
  - 安装：`npm i -g @jojoyo/codex`
  - 运行：`jcodex`

## 分支与发布流程（概览）

- 版本号：遵循上游版本，在其后追加 `-fork.N` 后缀（如 `0.21.0-fork.1`）。
- 标签（tag）：发布由 `rust-v<version>` 触发，支持 `-fork.*` 后缀（如 `rust-v0.21.0-fork.1`）。
- 发布：仅 `-alpha/-beta/-rc` 标记为预发布（Pre-release）；`-fork.*` 作为正式 Release 发布。
- 同步上游：
  - 本地脚本 `scripts/sync_upstream.sh`，用于手动合并 `upstream/main` 或指定上游 tag；
  - GitHub Action `.github/workflows/upstream-sync.yml`，支持定时与手动触发，自动开 PR。
- 标签选择（同步时）：默认仅合并稳定版本（匹配 `rust-vX.Y.Z`）；需要包含预发布时使用 `--include-pre`，仅预发布使用 `--pre-only`。

## 快速开始（本 Fork）

- 直接从本仓库的 Releases 下载并使用；
- 贡献与发布流程请查看下方“贡献指南”。

## 贡献指南

- 英文版：参见 [CONTRIBUTING.md](./CONTRIBUTING.md)
- 中文版：参见 [CONTRIBUTING.zh-CN.md](./CONTRIBUTING.zh-CN.md)

## 许可证

沿用上游 Apache-2.0 许可证（见 [LICENSE](./LICENSE)）。
