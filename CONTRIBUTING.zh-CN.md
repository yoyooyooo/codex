# 为本 Fork 贡献

语言： [English](./CONTRIBUTING.md) | [中文（简体）](./CONTRIBUTING.zh-CN.md)

本文档描述了如何在尽量贴近上游（https://github.com/openai/codex） 的前提下，开发与发布本 fork，并保持与上游清晰区分。

目标：上游版本可复现（基于 tag），fork 版本可追溯（-fork.N），流程简洁且适合团队协作。

## 分支与发布模型（推荐）

### 分支角色
- `upstream/main`：上游开发分支（版本号通常为 `0.0.0`）。
- `main`（本仓库）：镜像 `upstream/main`，仅用于同步上游；不合入“发布用版本号提交”。
- `dev`：你的长期开发分支，承载 fork 特性；应定期 `rebase upstream/main` 薄化差异。
- `release/fork-X.Y.Z`：发布分支。
  - 首次在该基线上发布：从上游标签 `rust-vX.Y.Z` 切出并叠加 fork 特性。
  - 复发（`-fork.N+1`）：在已有 `release/fork-X.Y.Z` 的基础上“增量重放”`dev` 的新增独有提交（不再重置到上游 tag）。
- `stable`（可选）：指向最近一次对外发布的 fork 标签，便于固定引用。

### 初始化与远端
```bash
# 添加并验证上游远端
git remote add upstream https://github.com/openai/codex.git  # 已存在则忽略
git fetch upstream --tags

# 让本地 main 镜像上游 main（保持干净基线）
git checkout main
git reset --hard upstream/main
git push origin main --force-with-lease

# 创建或更新 fork/dev（基于上游 main）
git checkout -B dev upstream/main
git push -u origin dev
```

### 日常开发
- 从 `dev` 切出特性分支开发，小步提交，完结后合回 `dev`。
- 需要向上游提 PR 时，务必从 `upstream/main` 切分支，避免将 fork 私有改动带入。

### 同步上游（保持 dev 最新）
```bash
git fetch upstream --tags

# 更新 main 为上游最新
git checkout main
git reset --hard upstream/main
git push origin main --force-with-lease

# 将 dev 变基到最新上游
git checkout dev
git rebase upstream/main
git push -f
```

### 发布自己的版本（脚本化，推荐）
目标：以 `rust-vX.Y.Z` 为基线，叠加 fork 私有改动，在发布分支上修改版本号为 `X.Y.Z-fork.N` 并打标签。

首发（从上游 tag 起点重建）：
```bash
# 自动：从 rust-vX.Y.Z 切出 release/fork-X.Y.Z，重放 upstream/main..dev 的独有提交
scripts/release_fork_from_upstream.sh --version X.Y.Z \
  [--strategy rebase-onto] [--main-branch dev]
```

复发（增量，分支已存在时推荐）：
```bash
# 在 release/fork-X.Y.Z 上，仅重放 dev 相对 release 的新增独有提交
scripts/release_fork_from_upstream.sh --version X.Y.Z \
  --replay-base release [--main-branch dev]
```

冲突处理与收尾：
- 队列式重放遇到冲突：在当前分支解决冲突 → `git add -A` → `git cherry-pick --continue` / `git rebase --continue`
- 仅做收尾（改版本+打标签+推送）：
  ```bash
  scripts/release_fork_from_upstream.sh --resume --prefer-given --version X.Y.Z
  # 可选：--no-push-tags 仅打本地标签；--no-lock-update 跳过锁文件更新；--fork N 固定序号
  ```

常用选项：
- `--replay-base auto|upstream|release`（默认 auto）：
  - auto：存在发布分支走增量，否则走首发
  - upstream：强制首发（从 tag 重建）
  - release：强制增量（要求分支已存在）
- `--strategy cherry-pick|rebase-onto`：首发可选 `rebase-onto`，增量自动回落为 `cherry-pick`
- `--resume` / `--resume-cherry` / `--abort-inprogress` / `--no-reset`
- `--no-apply-patches`：跳过重放，仅做收尾
- `--no-push-tags`：仅本地打标签

### 版本号与锁文件
- 开发线（`main`/`dev`）：保持 `0.0.0`，避免无谓的 `Cargo.lock` 漂移。
- 发布线（`release/fork-X.Y.Z`）：将 `codex-rs/Cargo.toml` 版本改为 `X.Y.Z-fork.N`；
  - 可选更新锁文件：`--no-lock-update` 可跳过；若仅因版本号导致锁文件漂移，通常不必提交。
- npm 包版本：由 GitHub Actions 的 `codex-cli/scripts/stage_release.sh` 在临时目录注入，无需在仓库内修改 `codex-cli/package.json`。

### 隔离构建（可选，避免污染工作区）
```bash
# 在独立目录检出同一仓库工作树，专用于构建发布
git worktree add ../codex-fork-X.Y.Z release/fork-X.Y.Z
# 在该目录内进行构建/测试，不影响主工作区的锁文件与临时产物
```

### 自检清单
- 我是否站在“发布提交/标签”上：
  ```bash
  git tag --points-at HEAD  # 显示 rust-vX.Y.Z(-fork.N) 说明在发布点
  ```
- `main` 是否镜像上游：
  ```bash
  git checkout main && git rev-parse --abbrev-ref --symbolic-full-name @{u}  # 应为 origin/main
  git merge-base --is-ancestor main upstream/main && echo OK || echo NEED_SYNC
  ```
- 发布迁移是否对齐：
  ```bash
  git range-diff upstream/main..dev rust-vX.Y.Z..release/fork-X.Y.Z
  ```

### 常见误区
- 不要从上游“Release x.y.z”那个提交向开发线合并或 cherry-pick；保持开发线始终以 `upstream/main` 为基线。
- 不要把发布分支的“版本号提交”合回 `dev`/`main`。
- 仅因版本号变化导致的 `Cargo.lock` 漂移，不要单独发 PR；除非你改变了依赖关系。

## 分支模型

- `main`：镜像 `upstream/main`。
- `dev`：fork 的开发主线。
- `release/fork-X.Y.Z`：发布分支，首发自上游 tag，复发走增量重放。
- `sync/...`：同步上游的临时分支；以 PR 合并回 `main`。

## 版本与标签

- 版本号：在上游版本后追加 `-fork.N`（示例：`0.21.0-fork.1`）。
- 标签：发布由 `rust-v<version>` 触发，标签需与 `codex-rs/Cargo.toml` 完全一致（示例：`rust-v0.21.0-fork.1`）。

为什么这样做？

- 一眼可见“基于哪个上游版本 + 第几次 fork 增量”；
- GitHub Actions 的发布流水线以 `rust-v*` 标签触发，天然兼容。

## 跟进上游（推荐：基于标签）

### **最佳实践：同步上游稳定标签**

经验证明，上游使用发布分支策略，`main`分支包含未发布的开发代码，而稳定标签才是真正的发布版本。因此推荐**仅同步上游的稳定标签**。

#### **智能同步流程（推荐）**

使用现有同步脚本：

```bash
# 列出上游最新稳定标签
scripts/sync_upstream.sh list-tags --limit 10

# 将上游 main/tag 合入本地分支（默认创建 sync/... 分支并推送）
scripts/sync_upstream.sh merge-main --branch main --push
scripts/sync_upstream.sh merge-tag rust-v0.31.0 --branch main --push
```

#### **手动基于标签同步**

```bash
# 1. 查看上游最新稳定标签
scripts/sync_upstream.sh list-tags --limit 5

# 2. 基于稳定标签重建（清理历史，推荐）
git tag backup-$(date +%Y%m%d) HEAD                    # 备份当前状态
git checkout -b sync-to-v0.31.0 rust-v0.31.0          # 基于标签创建分支
git cherry-pick <你的fork特性提交>                      # 应用fork特性
git checkout main && git reset --hard sync-to-v0.31.0  # 更新main分支
git branch -D sync-to-v0.31.0                          # 清理临时分支
```

#### **为什么不建议同步main分支**

- ❌ 上游`main`包含未发布的实验性代码
- ❌ 可能引入不稳定的功能或bug
- ❌ 版本语义不清晰
- ✅ 稳定标签经过测试，版本明确
- ✅ 更容易管理和追溯问题

## Release Notes（去重）

- 在同一上游基线（X.Y.Z）下，采用“增量重放”后，`rust-vX.Y.Z-fork.(N-1)..rust-vX.Y.Z-fork.N` 自然只包含新增提交。
- 若需要在 CI 中生成说明，可调用 `scripts/gen_release_notes.sh rust-vX.Y.Z-fork.N RELEASE_NOTES.md` 并将其作为 `body_path` 传给发布步骤；
  该脚本会在存在 `git-cliff` 时使用其模板，否则回退为 `git log`。建议结合“增量基线”生成，避免重复条目。

### **传统方式（仍可用）**

1) 本地脚本（手动）

使用 `scripts/sync_upstream.sh`（需要工作区干净）：

- 列出上游最新标签：

```
scripts/sync_upstream.sh list-tags --limit 10
```

- 合并上游 main：

```
scripts/sync_upstream.sh merge-main --branch main --push
```

- 合并指定 tag：

```
scripts/sync_upstream.sh merge-tag rust-v0.21.0 --branch main --push
```

选项：`--rebase` 使用变基；`--no-branch` 直接在目标分支操作；`--upstream-url` 覆盖默认上游；`--push` 推送产生的分支到 origin。
再补充：`--dry-run` 可进行“预览模式”，打印将要创建/切换的分支、要进行的合并/变基、基线 tag、推送动作等，
但不会对本地仓库做任何改动。dry-run 会进行一次轻量网络连通性探测，若检测到网络受限会明确提示，之后使用本地已有的 tag 视图（不执行 fetch）。
如需拉取最新标签，请去掉 `--dry-run` 运行；若 fetch 失败，脚本会提示并继续使用本地视图。

另有：`--force-tags` 用于在抓取上游时强制更新本地 tag 与上游对齐（等价于 `git fetch upstream --tags --prune --force`）。
仅在你明确希望覆盖本地与上游“同名但指向不同”的标签时使用。

基线标记（推荐）：

- 在合并上游标签（如 `rust-v0.21.0`）时，可加 `--tag-baseline`，在合并结果上创建
  `base-rust-v0.21.0` 这样的“基线 tag”，方便追踪当前代码所基于的上游版本。
- 搭配 `--push-tags` 可将该基线 tag 推送到 origin。
- 注意：基线 tag 的前缀为 `base-`，不会触发发布工作流（发布触发仅匹配 `rust-v*`）。

查询当前基线：

```
scripts/sync_upstream.sh current-baseline
```
优先输出最近的基线 tag（如 `base-rust-v0.21.0`）；若不存在，则回退到日志中最近一次
“Merge upstream rust-v...” 的提交来推断。

自动连续合并上游标签：

```
scripts/sync_upstream.sh merge-series --branch main --push --push-tags --limit 10
```

行为说明：

- 自动识别当前基线（`base-rust-v*`），依次合并其后的上游 `rust-v*` 标签（默认仅稳定版本）；每合并
  成功一个标签，会在结果上打一个新的基线 tag，便于中断后续接续。
- 若合并出现冲突，脚本会在该标签处停止；请手动解决冲突并提交，然后再次运行同一命令，脚本会
  从下一个标签继续。
- 当未检测到基线时，可用 `--from <rust-vX.Y.Z>` 指定起始基线；也可用 `--to <rust-vX.Y.Z>` 指定
  终止的上游标签。
 - 可加 `--dry-run` 先预览完整的合并计划，确保无误后再正式执行。
 - 标签选择：
   - 默认仅合并稳定版本（匹配 `rust-vX.Y.Z`）
   - `--include-pre`：包含预发布标签（如 `-alpha/-beta/-rc`）
   - `--pre-only`：仅包含预发布标签

在全新 fork（还没有任何基线 tag）时初始化一次基线：

```
scripts/sync_upstream.sh init-baseline rust-v0.21.0 --push-tags
```

该命令会在当前 `HEAD` 上创建 `base-rust-v0.21.0`，用来标记“本地当前代码基于上游的 rust-v0.21.0”。
之后再运行 `merge-series` 时即可自动识别出这个基线并从其后的标签开始连续合并。

2) GitHub Action（自动/手动）

工作流：`.github/workflows/upstream-sync.yml`

- 定时（每日）：选择上游最新 `rust-v*` 标签，创建 `sync/...` 分支并自动开 PR 到 `main`。
- 手动触发：在 Actions 中选择 `upstream-sync`，可指定：
  - `upstream_repo`（可选，默认 `openai/codex`）
  - `ref`（可选；如 `upstream/main` 或 `rust-v0.21.0`）
  - `mode`（`merge` 或 `rebase`，默认 `merge`）

## 发布 fork 版本

### **完整发布流程（经过实践验证）**

基于我们的实际操作经验，推荐以下完整流程：

#### **1. 同步上游稳定版本**

```bash
# 分析上游状态
./scripts/sync_upstream_enhanced.sh compare-upstream

# 基于最新稳定标签同步（如 rust-v0.31.0）
git tag backup-$(date +%Y%m%d-%H%M) HEAD
git checkout -b sync-to-v0.31.0 rust-v0.31.0
git cherry-pick <你的fork特性提交1> <你的fork特性提交2>  # 应用所有fork特性
git checkout main && git reset --hard sync-to-v0.31.0
git branch -D sync-to-v0.31.0
```

#### **2. 创建基线和fork版本**

```bash
# 创建基线标签
scripts/sync_upstream.sh init-baseline rust-v0.31.0 --push-tags

# 更新版本号匹配发布标签
# 编辑 codex-rs/Cargo.toml: version = "0.31.0-fork.1"
cd codex-rs && cargo update --workspace
cd .. && git add codex-rs/Cargo.toml codex-rs/Cargo.lock
git commit -m "chore: bump version to 0.31.0-fork.1 to match release tag"

# 创建fork版本标签
git tag rust-v0.31.0-fork.1 -m "Fork version based on rust-v0.31.0 with custom enhancements"
```

#### **3. 推送和发布**

```bash
# 推送所有更改
git push origin main --force-with-lease
git push origin rust-v0.31.0-fork.1
git push origin base-rust-v0.31.0  # 基线标签
```

#### **4. GitHub Actions自动构建**

推送标签后，GitHub Actions会自动：
- ✅ 验证标签格式和版本一致性
- ✅ 多平台构建（Linux, macOS, Windows）
- ✅ 创建GitHub Release
- ✅ 生成发布说明

#### **5. 常见问题和解决方案**

**版本不匹配错误**：
```bash
# 如果CI报错 "Tag X.Y.Z-fork.N ≠ Cargo.toml X.Y.Z"
# 更新Cargo.toml版本号匹配标签
sed -i 's/version = ".*"/version = "0.31.0-fork.1"/' codex-rs/Cargo.toml
cd codex-rs && cargo update --workspace
git add codex-rs/Cargo.toml codex-rs/Cargo.lock && git commit -m "fix: version alignment"

# 重新创建标签
git tag -d rust-v0.31.0-fork.1
git tag rust-v0.31.0-fork.1 -m "Fork version 0.31.0-fork.1"
git push origin rust-v0.31.0-fork.1 --force
```

**GitHub Release权限问题**：
- 确保fork的Settings → Actions → General → Workflow permissions设为"Read and write permissions"
- 如遇到dotslash错误，可临时禁用该步骤（见`.github/workflows/rust-release.yml`注释）

### **自动化方式（可选）**

方式一（推荐，一键）：

```
scripts/release_fork.sh --dry-run               # 预览：基线与即将发布的版本号
scripts/release_fork.sh                         # 从 main 发布；内部调用 codex-rs/scripts/create_github_release.sh
scripts/release_fork.sh --baseline 0.30.0       # 覆盖基线（可选）
scripts/release_fork.sh --version 0.30.0-fork.2 # 完全显式指定版本
```

方式二（手动等价流程）：

- 确认 `main` 已合入目标上游 tag 或 `upstream/main`，并完成冲突解决与测试。
- 更新 `codex-rs/Cargo.toml` 的版本为 `X.Y.Z-fork.N`。
- 打标签并推送：`rust-vX.Y.Z-fork.N`。

随后会自动触发 `rust-release` 工作流：

- 校验标签格式并比对 `Cargo.toml` 版本；
- 多平台构建产物并上传；
- 创建 GitHub Release（`-fork.*` 视为正式版本；仅 `-alpha/-beta/-rc` 标记为 Pre-release）；
- 生成 npm 打包产物（如需发布到 npm，请手动执行脚本并确保权限）。

可选：发布到 npm（需要权限）：

```
VERSION=0.21.0-fork.1
# 本 fork（默认仓库：yoyooyooo/codex）：
./scripts/publish_to_npm.py "$VERSION"
# 若需从上游 Release 获取：
./scripts/publish_to_npm.py "$VERSION" --repo openai/codex
```

## 编码、CI 与文档约定

- 尽量保持与上游一致，避免无关重构；
- 遵循上游构建与测试说明（见 `docs/`）；
- 根目录 `README.md` 要求 ASCII-only（CI 检查），中文内容请放在 `README.zh-CN.md`；
- 如果你要在 README 中添加 ToC，请使用 `<!-- Begin ToC -->`/`<!-- End ToC -->` 标记，便于 CI 校验。

## 问题反馈

如有疑问或建议，欢迎在本 fork 提 Issue 或直接提交 PR 改进本文档与脚本。
