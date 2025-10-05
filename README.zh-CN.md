<h1 align="center">Codex CLI（Fork）</h1>

<p align="center">语言：<a href="README.md">English</a> | <a href="README.zh-CN.md">简体中文</a></p>

<p align="center">
  <a href="https://github.com/openai/codex/releases/tag/rust-v0.30.0">
    <img alt="Upstream" src="https://img.shields.io/badge/upstream-0.30.0-blue" />
  </a>
  &nbsp;
  <a href="https://github.com/openai/codex/releases">
    <img alt="Upstream Releases" src="https://img.shields.io/badge/upstream-releases-555" />
  </a>
  &nbsp;
  <a href="https://github.com/openai/codex">
    <img alt="Upstream Repo" src="https://img.shields.io/badge/source-openai%2Fcodex-555" />
  </a>
</p>

本仓库在以下上游项目的基础上维护升级：
https://github.com/openai/codex

关于产品文档、安装与通用用法，请参考上游 README 与文档。本 Fork 专注于少量增强特性与独立的同步/发版流程，同时尽量与上游保持一致。

> 上游基线：**0.30.0** — 基于上游 tag
> [`rust-v0.30.0`](https://github.com/openai/codex/releases/tag/rust-v0.30.0)

## 本 Fork 的特性

- 自定义 Mode：Slash `/name` 支持常驻/瞬时模式，按项目优先级合并 `.codex/modes/`（使用说明 [docs/fork-feats/custom-mode.md](docs/fork-feats/custom-mode.md)，设计细节 [docs/feats/design/custom-mode.md](docs/feats/design/custom-mode.md)）
- TUI：Esc 按键交互优化（更快清空 / 快速回退）— 详见下文「[TUI — Esc](#tui--esc清空输入或回退)」，使用说明见 [docs/fork-feats/tui-esc.md](docs/fork-feats/tui-esc.md)，设计细节见 [docs/feats/design/tui-esc.md](docs/feats/design/tui-esc.md)
- 项目级 Prompts：会话启动时自下而上合并 `.codex/prompts/` 目录，就近覆盖全局条目 — 使用说明 [docs/fork-feats/project-prompts.md](docs/fork-feats/project-prompts.md)，设计细节 [docs/feats/design/project-prompts.md](docs/feats/design/project-prompts.md)

### TUI — Esc：清空输入或回退

在 TUI 的输入框区域，Esc 会根据上下文自适应：

- 当输入框有内容：按一次 Esc 会在底部出现弱化的 `Esc clear` 标签，并额外出现一行“Please Escape again to clear”（持续 1 秒）。在该 1 秒内再次按 Esc 即清空输入框；如果 1 秒内未再按，提示会自动消失。
- 当输入框为空：按 Esc 进入回退预备态；再次按 Esc 打开 “Backtrack to User Messages” 选择器，列出历史用户消息（从近到远）。用 ↑/↓ 选择并按 Enter 确认回退；系统会从选定节点分叉对话、裁剪可见转录，并将该消息预填回输入框以便编辑与重发。转录视图（Ctrl+T）也保持可用，在其中 Esc 可向更早的用户消息步进，Enter 确认。

交互原理、防御路径详见 [docs/fork-feats/tui-esc.md](docs/fork-feats/tui-esc.md)，底层状态机与同步提示见 [docs/feats/design/tui-esc.md](docs/feats/design/tui-esc.md)。

### 自定义 Mode：常驻/瞬时 Slash 模式

Slash 会自当前目录向上收集 `.codex/modes/`，最后合并 `$CODEX_HOME/modes/`，并以距离 `cwd` 最近的定义覆盖远端定义。每个 Markdown 模式文件通过 frontmatter 描述 `persistent` / `instant` 类型与变量清单，TUI 在本地解析、渲染 ModeBar/ModePanel，并通过 `Op::OverrideTurnContext` 注入 `<mode_instructions>`，核心协议保持不变。更多发现规则、变量语义与测试建议见 [docs/fork-feats/custom-mode.md](docs/fork-feats/custom-mode.md)。

### 项目级 Prompts：就近覆盖全局模板

Codex 会从当前工作目录一路向上收集 `.codex/prompts/` 目录，并与 `$CODEX_HOME/prompts/` 合并，靠近项目的定义优先生效。Slash 菜单展示来源标签，便于识别覆盖关系。使用方法见 [docs/fork-feats/project-prompts.md](docs/fork-feats/project-prompts.md)，合并算法与排障守则见 [docs/feats/design/project-prompts.md](docs/feats/design/project-prompts.md)。

## 安装与 CLI 名称（Fork）

- npm 包名：`@jojoyo/codex`
- 全局二进制：`jcodex`（与上游 `codex` 区分）
  - 安装：`npm i -g @jojoyo/codex`
  - 运行：`jcodex`

## 同步与发版流程

- 版本号：以上游版本为基础追加 “-fork.N” 后缀，例如 `0.21.0-fork.1`。
- Tag：使用 `rust-v<version>` 触发发版（包含 fork 后缀，例如 `rust-v0.21.0-fork.1`）。
- 发布：仅 `-alpha/-beta/-rc` 视为预发布；`-fork.*` 作为正式发布。
- 同步工具：`scripts/sync_upstream.sh` 脚本，以及一个可手动/定时的 GitHub Action，用于从上游拉取 PR 进行同步。
- Tag 选择：默认仅同步稳定标签（`rust-vX.Y.Z`）。可用 `--include-pre` 包含预发布（`-alpha/-beta/-rc`），或用 `--pre-only` 仅同步预发布。

开始使用本 Fork：

- 从本 Fork 的 GitHub Releases 页面下载构建版本。
- 查看 CONTRIBUTING.md 了解本 Fork 的贡献与发布流程。

许可证：

与上游一致（Apache-2.0，见 LICENSE）。
