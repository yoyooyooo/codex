标题：一条指令全自动发布当前 fork（从上游 rust 标签衍生）

用途
- 我说“发布一个新版本”（或给出可选参数）时，你需全自动完成：分析上游与本地状态 → 选定基线 → 创建/切换发布分支 → 增量重放独有提交 → 更新 `codex-rs` 版本 → 生成并推送注解 tag。

默认行为（可覆盖）
- 上游 remote：`upstream`；上游基准分支：`main`。
- 来源分支：默认 `main`；若 `main` 无独有提交而 `dev` 有，则自动改用 `dev`。
- 重放策略：`cherry-pick`（稳妥，默认）；当以 upstream 为基线时可选 `rebase-onto`。
- 重放基线：`auto`（默认）。若 `release/fork-X.Y.Z` 已存在 → 使用它做增量基线；否则以 `upstream_tag` 为基线。
- 版本：自动选择基线版本并计算 `fork.N`（现有最大 N + 1）。
- 推送：默认推送 tag 到 `origin`；分支可选推送（默认不推）。

可以传入的可选参数
- `base_version`: 显式上游版本号 `X.Y.Z`（否则自动：上次 fork 的基线，否则最新上游稳定版本）。
- `upstream_tag`: 显式上游标签 `rust-vX.Y.Z`（与 `base_version` 二选一）。
- `main_branch`: 来源分支（默认 `main`）。
- `replay_strategy`: `cherry-pick` | `rebase-onto`（默认 `cherry-pick`）。
- `replay_base`: `auto` | `upstream` | `release`（默认 `auto`）。
- `push_tags`: `true|false`（默认 `true`）。
- `push_branch`: `true|false`（默认 `false`）。
- `dry_run`: `true|false`（默认 `false`）。

执行准则（必须遵守）
- 严格“最小上游差异”：仅重放相对上游的独有提交，禁止重写上游历史；
- 若工作树不干净，先失败并输出处理指引（或在显式 `dry_run=false` + `force=true` 时自动中止并提示）；
- 每个关键步骤打印“计划与结果”摘要；失败时输出修复建议并停止。

自动发布步骤（实现细则）
1) 预检查
   - 确认工作树干净：`git diff --quiet && git diff --cached --quiet && git ls-files --others --exclude-standard` 为空。
   - 确认/推断上游 remote：优先 `upstream`；若无则尝试创建只读上游或提示用户配置；随后 `git fetch --tags`。
2) 选择上游基线版本
   - 若提供 `upstream_tag` 或 `base_version`，直接使用；否则：
     - 若存在历史 fork 标签 `rust-v*-fork.*`，取其中 `X.Y.Z` 最大者作为基线；
     - 否则取“最新上游稳定版本”（`git tag --list 'rust-v*'` 后按 X.Y.Z 数值排序取最大）。
   - 设 `X_Y_Z` 与 `upstream_tag=rust-vX.Y.Z`；确保该 tag 本地存在（必要时 `git fetch upstream tag ...`）。
3) 决定发布分支与基线
   - 目标分支：`release/fork-X.Y.Z`。
   - 当 `replay_base=auto`：若目标分支已存在 → `release` 基线；否则 → `upstream` 基线。
   - `upstream` 基线：从 `upstream_tag` 新建/重置目标分支；`release` 基线：切换到已存在分支，保持现有提交不变。
   - 若选择了 `rebase-onto` 且基线为 `release`，降级为 `cherry-pick`。
4) 选择来源分支
   - `main_branch` 优先使用参数；否则默认 `main`；当 `main` 相对上游无独有提交且 `dev` 有，则自动改用 `dev`。
5) 生成需重放的提交清单
   - 基线参照 `range_base`：`upstream` → `upstream/main`；`release` → `release/fork-X.Y.Z` 最新提交。
   - 使用 `git rev-list --reverse --right-only --cherry-pick "${range_base}...${main_branch}"` 产生按时间排序的独有提交清单。
6) 应用独有提交（增量重放）
   - 默认 `cherry-pick`：逐个应用，遇冲突停止并报告冲突文件列表与解决建议（谨慎尝试仅锁文件的自动解决，否则交由人工解决后 `git cherry-pick --continue`）。
   - 可选 `rebase-onto`（仅 `upstream` 基线时）：`git rebase --onto <upstream_tag> <upstream/main> <main_branch>`，遇冲突处理同上。
   - 支持中断/恢复：自动检测 `.git/sequencer`、`CHERRY_PICK_HEAD`、`rebase-apply|merge` 来判断恢复点。
7) 版本与锁文件更新
   - 计算 `fork.N`：遍历 `git tag --list 'rust-vX.Y.Z-fork.*'` 得到最大 N 后加 1；如参数显式给定则使用。
   - 更新 `codex-rs/Cargo.toml` 的 `version = "X.Y.Z-fork.N"`（工作空间版本）；随后 `(cd codex-rs && cargo update --workspace)`。
   - 提交一次版本变更：`git add codex-rs/Cargo.toml codex-rs/Cargo.lock && git commit -m "chore: bump version to X.Y.Z-fork.N for fork release"`。
8) 打标签
   - 创建注解标签：`git tag -a rust-vX.Y.Z-fork.N -m "fork release based on rust-vX.Y.Z; source=${main_branch}; strategy=${replay_strategy}; base=${replay_base}"`。
9) 推送
   - 若 `push_tags=true`：`git push origin rust-vX.Y.Z-fork.N`；若 `push_branch=true`：`git push -u origin release/fork-X.Y.Z`。
10) 最小验证
   - 运行 `cargo test -p codex-tui`；如改动 core/protocol/common，则再跑 `cargo test --all-features`；
   - 构建关键二进制（`codex`, `codex-responses-api-proxy`），输出产物摘要。

一次性指令示例
- “发布一个新版本”：按默认策略自动完成全部步骤，基线优先选择“上次 fork 的基线”，否则“最新上游稳定版”；来源分支自动判定 `main|dev`；打 tag 并推送。
- “发布：基线 0.40.0，从 dev，重放策略 cherry-pick，不推分支”：
  - `base_version=0.40.0 main_branch=dev replay_strategy=cherry-pick push_branch=false`

安全回滚
- 任一步骤失败后：打印失败点与建议；若已创建分支但未打 tag，可提示 `git switch -` 返回；已创建 tag 未推送时，可 `git tag -d` 撤销。

产出
- 分支：`release/fork-X.Y.Z`
- 标签：`rust-vX.Y.Z-fork.N`
- 版本文件：`codex-rs/Cargo.toml` 与 `codex-rs/Cargo.lock` 已更新
- 基本测试与构建通过（如有要求）
