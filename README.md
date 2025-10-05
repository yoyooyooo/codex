<h1 align="center">Codex CLI (Fork)</h1>

<p align="center">Languages: <a href="README.md">English</a> | <a href="README.zh-CN.md">中文</a></p>

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

This repository is a maintained fork of the upstream project at:
https://github.com/openai/codex

For product docs, installation, and general usage, please refer to the
upstream README and documentation. This fork focuses on a small set of
exclusive features and a separate sync/release workflow while staying close to upstream.

> Upstream baseline: **0.30.0** - based on upstream tag
> [`rust-v0.30.0`](https://github.com/openai/codex/releases/tag/rust-v0.30.0)

## Fork-Specific Features

- Custom modes: persistent/instant `/name` workflows with project-scoped discovery ([docs/fork-feats/custom-mode.md](docs/fork-feats/custom-mode.md) · design: [docs/feats/design/custom-mode.md](docs/feats/design/custom-mode.md))
- TUI: Esc behavior optimized for fast editing/backtracking ([docs/fork-feats/tui-esc.md](docs/fork-feats/tui-esc.md) · design: [docs/feats/design/tui-esc.md](docs/feats/design/tui-esc.md) · see details in [TUI - Esc](#tui--esc-clear-input-or-backtrack))
- Project prompts: per-project `.codex/prompts/` directories override global entries ([docs/fork-feats/project-prompts.md](docs/fork-feats/project-prompts.md) · design: [docs/feats/design/project-prompts.md](docs/feats/design/project-prompts.md))

### TUI - Esc: clear input or backtrack

In the TUI composer, Esc adapts to context:

- When the composer has text: press Esc once to show a one-second window to clear. The footer adds a subtle `Esc clear` indicator and a second line "Please Escape again to clear"; press Esc again within 1s to clear. If you don't, the hint hides automatically.
- When the composer is empty: press Esc to prime backtrack; press Esc again to open "Backtrack to User Messages" and pick an earlier user message to fork from (Up/Down, Enter). The transcript overlay (`Ctrl+T`) remains available and continues to support Esc-to-step, Enter-to-confirm.

See [docs/fork-feats/tui-esc.md](docs/fork-feats/tui-esc.md) for usage notes and [docs/feats/design/tui-esc.md](docs/feats/design/tui-esc.md) for guard-rail and implementation details.

### Custom modes: persistent and instant `/name`

Slash commands now discover Markdown definitions from `.codex/modes/` across the project tree and `$CODEX_HOME/modes/`, merge them by proximity, and expose both persistent (session-scoped) and instant (one-shot) modes with typed variables. Rendering and guard logic stay in the client while the core keeps the upstream protocol. Review [docs/fork-feats/custom-mode.md](docs/fork-feats/custom-mode.md) for user guidance and [docs/feats/design/custom-mode.md](docs/feats/design/custom-mode.md) for discovery rules, UI flows, and testing checklists.

### Project prompts: project-first overrides

Codex now walks up from the current working directory to locate `.codex/prompts/` folders, merges them with the global `$CODEX_HOME/prompts/`, and prefers the closest definitions. This enables per-project prompt kits while keeping upstream defaults intact. Check [docs/fork-feats/project-prompts.md](docs/fork-feats/project-prompts.md) for usage and [docs/feats/design/project-prompts.md](docs/feats/design/project-prompts.md) for merge implementation details and troubleshooting guidance.

## Install & CLI Name (Fork)

- npm package: `@jojoyo/codex`
- global binary: `jcodex` (renamed to avoid conflicts with the upstream `codex`)
  - install: `npm i -g @jojoyo/codex`
  - run: `jcodex`

If you prefer upstream packaging and naming, upstream Codex can be installed via:

<p align="left"><code>npm i -g @openai/codex</code><br />or <code>brew install codex</code></p>

See upstream docs below for more details.

## Sync and Release Workflow

- Versioning: releases follow upstream versions with a "-fork.N" suffix, e.g. "0.21.0-fork.1".
- Tags: publishing is triggered by tags named "rust-v<version>", including fork suffixes (e.g., "rust-v0.21.0-fork.1").
- Release pipeline: only "-alpha/-beta/-rc" are marked as pre-release; "-fork.*" are published as normal releases.
- Upstream sync tooling: local helper script at "scripts/sync_upstream.sh" and a scheduled/hand-run GitHub Action to open PRs that sync from upstream.
- Tag selection for sync: by default only stable tags ("rust-vX.Y.Z"). Use `--include-pre` to include pre-release tags (e.g., `-alpha/-beta/-rc`) or `--pre-only` for pre-releases only.

Getting started with this fork:

- Download builds from this fork's GitHub Releases page.
- See CONTRIBUTING.md for the fork-specific contribution and release guide.

### Configuration

Codex CLI supports a rich set of configuration options, with preferences stored in `~/.codex/config.toml`. For full configuration options, see [Configuration](./docs/config.md).

---

### Docs & FAQ

- [**Getting started**](./docs/getting-started.md)
  - [CLI usage](./docs/getting-started.md#cli-usage)
  - [Running with a prompt as input](./docs/getting-started.md#running-with-a-prompt-as-input)
  - [Example prompts](./docs/getting-started.md#example-prompts)
  - [Memory with AGENTS.md](./docs/getting-started.md#memory-with-agentsmd)
  - [Configuration](./docs/config.md)
- [**Sandbox & approvals**](./docs/sandbox.md)
- [**Authentication**](./docs/authentication.md)
  - [Auth methods](./docs/authentication.md#forcing-a-specific-auth-method-advanced)
  - [Login on a "Headless" machine](./docs/authentication.md#connecting-on-a-headless-machine)
- **Automating Codex**
  - [GitHub Action](https://github.com/openai/codex-action)
  - [TypeScript SDK](./sdk/typescript/README.md)
  - [Non-interactive mode (`codex exec`)](./docs/exec.md)
- [**Advanced**](./docs/advanced.md)
  - [Tracing / verbose logging](./docs/advanced.md#tracing--verbose-logging)
  - [Model Context Protocol (MCP)](./docs/advanced.md#model-context-protocol-mcp)
- [**Zero data retention (ZDR)**](./docs/zdr.md)
- [**Contributing**](./docs/contributing.md)
- [**Install & build**](./docs/install.md)
  - [System Requirements](./docs/install.md#system-requirements)
  - [DotSlash](./docs/install.md#dotslash)
  - [Build from source](./docs/install.md#build-from-source)
- [**FAQ**](./docs/faq.md)
- [**Open source fund**](./docs/open-source-fund.md)

---

## License

This repository is licensed under the [Apache-2.0 License](LICENSE).
