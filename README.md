<h1 align="center">Codex CLI (Fork)</h1>

<p align="center">Languages: <a href="README.md">English</a> | <a href="README.zh-CN.md">Chinese (zh-CN)</a></p>

This repository is a maintained fork of the upstream project at:
https://github.com/openai/codex

For product docs, installation, and general usage, please refer to the
upstream README and documentation. This fork focuses on a small set of
exclusive features and a separate sync/release workflow while staying close to upstream.

## Fork-Specific Features

- TUI: double-press Esc opens the user prompt node selector for faster input routing.

## Install & CLI Name (Fork)

- npm package: `codeu`
- global binary: `ycodex` (renamed to avoid conflicts with the upstream `codex`)
  - install: `npm i -g codeu`
  - run: `ycodex`

## Sync and Release Workflow

- Versioning: releases follow upstream versions with a "-fork.N" suffix, e.g. "0.21.0-fork.1".
- Tags: publishing is triggered by tags named "rust-v<version>", including fork suffixes (e.g., "rust-v0.21.0-fork.1").
- Release pipeline: only "-alpha/-beta/-rc" are marked as pre-release; "-fork.*" are published as normal releases.
- Upstream sync tooling: local helper script at "scripts/sync_upstream.sh" and a scheduled/hand-run GitHub Action to open PRs that sync from upstream.
- Tag selection for sync: by default only stable tags ("rust-vX.Y.Z"). Use `--include-pre` to include pre-release tags (e.g., `-alpha/-beta/-rc`) or `--pre-only` for pre-releases only.

Getting started with this fork:

- Download builds from this fork's GitHub Releases page.
- See CONTRIBUTING.md for the fork-specific contribution and release guide.

License:

Apache-2.0, same as upstream (see LICENSE).
