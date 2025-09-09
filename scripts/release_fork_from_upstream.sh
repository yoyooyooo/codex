#!/usr/bin/env bash
set -euo pipefail

# Simple fork release helper anchored at an upstream rust tag.
# Compatible with macOS's Bash 3.2 (no mapfile/readarray; no sort -V).
#
# Flow:
#   1) Ensure upstream tag rust-vX.Y.Z exists locally (fetch if missing)
#   2) Create or reuse local branch release/fork-X.Y.Z at that tag (hard reset)
#   3) Optionally apply local fork commits (range: upstream/main...<main-branch>)
#   4) Update codex-rs/Cargo.toml version to X.Y.Z-fork.N (and Cargo.lock)
#   5) Create annotated tag rust-vX.Y.Z-fork.N
#   6) Optionally push tag to origin (default: true)
#
# Defaults:
#   - upstream remote: upstream
#   - main branch    : main
#   - apply patches  : true (apply commits from upstream/main...<main-branch>)
#   - push tags      : true (to origin)
#
# Usage examples:
#   scripts/release_fork_from_upstream.sh                         # auto choose base (last fork's base or latest upstream)
#   scripts/release_fork_from_upstream.sh --version 0.31.0        # explicit base version
#   scripts/release_fork_from_upstream.sh --tag rust-v0.31.0      # explicit upstream tag
#   scripts/release_fork_from_upstream.sh --main-branch dev       # fork commits from dev
#   scripts/release_fork_from_upstream.sh --dry-run               # preview only

DRY_RUN="false"
UPSTREAM_REMOTE="upstream"
UPSTREAM_BASE="main"   # upstream base branch to diff against
MAIN_BRANCH="main"     # local branch containing fork commits
APPLY_PATCHES="true"
NO_LOCK_UPDATE="false"
NO_FETCH_TAGS="false"
PREFER="ask"   # ask | latest | given
EXPLICIT_VERSION=""
UPSTREAM_TAG=""
EXPLICIT_FORK_N=""
PUSH_TAGS="true"
USER_SET_MAIN="false"
USER_SET_UPSTREAM_BASE="false"
NO_RESET="false"
RESUME_CHERRY="false"
ABORT_INPROGRESS="false"
RESUME="false"

usage() {
  cat <<EOF
Usage: $(basename "$0") [--version X.Y.Z | --tag rust-vX.Y.Z] [options]

Options
  --version X.Y.Z         Upstream numeric version
  --tag    rust-vX.Y.Z    Upstream annotated tag (alternative to --version)
  --fork   N              Explicit fork suffix number (default: auto next)
  --upstream-remote NAME  Upstream remote name (default: upstream)
  --upstream-base  NAME   Upstream base branch to compare against (default: main)
  --main-branch    NAME   Local branch containing fork changes (default: main)
  --no-apply-patches      Do not cherry-pick upstream/main...<main-branch> onto release branch
  --no-lock-update        Skip Cargo.lock update
  --no-fetch-tags         Do not fetch tags (defaults to fetching from upstream and origin)
  --prefer-latest         If given version < latest upstream, auto switch to latest
  --prefer-given          If given version < latest upstream, keep given version
  --push-tags             Push created tag to origin (default: true)
  --no-push-tags          Do not push created tag
  --no-reset              Do not reset existing release branch to upstream tag (for manual resume)
  --resume-cherry         If cherry-pick in progress, try 'git cherry-pick --continue' and skip replay
  --abort-inprogress      If cherry-pick in progress, run 'git cherry-pick --abort' automatically
  --resume                Resume after conflicts: continue cherry-pick if needed, then bump+tag(+push)
  --dry-run               Print plan only, make no changes
  -h, --help              Show this help

Behavior
  - Creates local branch release/fork-X.Y.Z at rust-vX.Y.Z
  - Updates codex-rs/Cargo.toml version to X.Y.Z-fork.N
  - Cherry-picks commits unique to <main-branch> vs upstream/main
  - Creates tag rust-vX.Y.Z-fork.N (pushes to origin unless --no-push-tags)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      shift; EXPLICIT_VERSION="${1:-}" || true ;;
    --tag)
      shift; UPSTREAM_TAG="${1:-}" || true ;;
    --fork)
      shift; EXPLICIT_FORK_N="${1:-}" || true ;;
    --upstream-remote)
      shift; UPSTREAM_REMOTE="${1:-}" || true ;;
    --upstream-base)
      shift; UPSTREAM_BASE="${1:-}" || true; USER_SET_UPSTREAM_BASE="true" ;;
    --main-branch)
      shift; MAIN_BRANCH="${1:-}" || true; USER_SET_MAIN="true" ;;
    --no-apply-patches)
      APPLY_PATCHES="false" ;;
    --no-lock-update)
      NO_LOCK_UPDATE="true" ;;
    --no-fetch-tags)
      NO_FETCH_TAGS="true" ;;
    --prefer-latest)
      PREFER="latest" ;;
    --prefer-given)
      PREFER="given" ;;
    --push-tags)
      PUSH_TAGS="true" ;;
    --no-push-tags)
      PUSH_TAGS="false" ;;
    --no-reset)
      NO_RESET="true" ;;
    --resume-cherry)
      RESUME_CHERRY="true" ;;
    --abort-inprogress)
      ABORT_INPROGRESS="true" ;;
    --resume)
      RESUME="true" ;;
    --dry-run)
      DRY_RUN="true" ;;
    -h|--help)
      usage; exit 0 ;;
    --*)
      echo "Unknown option: $1" >&2; exit 1 ;;
    *)
      echo "Unexpected argument: $1" >&2; usage; exit 1 ;;
  esac
  shift || true
done

ensure_clean_repo() {
  if [[ "$DRY_RUN" == "true" ]]; then
    return 0
  fi
  if ! git diff --quiet || ! git diff --cached --quiet || [ -n "$(git ls-files --others --exclude-standard)" ]; then
    echo "ERROR: You have uncommitted or untracked changes." >&2
    exit 1
  fi
}

cherry_pick_in_progress() {
  # Detect ongoing sequencer/cherry-pick state even if worktree is clean
  if git rev-parse -q --verify CHERRY_PICK_HEAD >/dev/null 2>&1; then
    return 0
  fi
  if [ -f .git/CHERRY_PICK_HEAD ] || [ -d .git/sequencer ]; then
    return 0
  fi
  return 1
}

fetch_tags_default() {
  if [[ "$NO_FETCH_TAGS" == "true" ]]; then
    echo "Skipping tags fetch per --no-fetch-tags"
    return 0
  fi
  local remotes=()
  remotes+=("${UPSTREAM_REMOTE}")
  if git remote get-url origin >/dev/null 2>&1; then
    remotes+=("origin")
  fi
  for r in "${remotes[@]}"; do
    echo "Fetching all tags from ${r}..."
    if [[ "$DRY_RUN" == "true" ]]; then
      echo "[dry-run] git fetch ${r} --tags --force --no-recurse-submodules"
    else
      git fetch "${r}" --tags --force --no-recurse-submodules >/dev/null 2>&1 || true
    fi
  done
}

ensure_tag_local() {
  local tag="$1"
  if git rev-parse -q --verify "refs/tags/${tag}" >/dev/null; then
    return 0
  fi
  echo "Fetching tag ${tag} from ${UPSTREAM_REMOTE}..."
  if [[ "$DRY_RUN" == "true" ]]; then
    echo "[dry-run] git fetch ${UPSTREAM_REMOTE} tag ${tag} --no-tags"; return 0
  fi
  git fetch "${UPSTREAM_REMOTE}" tag "${tag}" --no-tags
  if ! git rev-parse -q --verify "refs/tags/${tag}" >/dev/null; then
    echo "ERROR: Tag ${tag} not found locally after fetch."
    exit 1
  fi
}

# Return latest upstream stable version X.Y.Z (max by X,Y,Z)
latest_upstream_stable_version() {
  git tag --list 'rust-v*' \
    | sed -E 's/^rust-v([0-9]+)\.([0-9]+)\.([0-9]+)$/\1 \2 \3/' \
    | grep -E '^[0-9]+ [0-9]+ [0-9]+$' \
    | sort -n -k1,1 -k2,2 -k3,3 \
    | tail -n1 \
    | awk '{print $1"."$2"."$3}'
}

# Return base version X.Y.Z from latest rust-vX.Y.Z-fork.N (max by X,Y,Z,N)
last_fork_base_version() {
  git tag --list 'rust-v*-fork.*' \
    | sed -E 's/^rust-v([0-9]+)\.([0-9]+)\.([0-9]+)-fork\.([0-9]+)$/\1 \2 \3 \4/' \
    | grep -E '^[0-9]+ [0-9]+ [0-9]+ [0-9]+$' \
    | sort -n -k1,1 -k2,2 -k3,3 -k4,4 \
    | tail -n1 \
    | awk '{print $1"."$2"."$3}'
}

version_lt() {
  # returns true if $1 < $2 (semver compare X.Y.Z)
  local a1 a2 a3 b1 b2 b3
  IFS=. read -r a1 a2 a3 <<EOF
$1
EOF
  IFS=. read -r b1 b2 b3 <<EOF
$2
EOF
  a1=${a1:-0}; a2=${a2:-0}; a3=${a3:-0}
  b1=${b1:-0}; b2=${b2:-0}; b3=${b3:-0}
  if (( a1 < b1 )); then return 0; fi
  if (( a1 > b1 )); then return 1; fi
  if (( a2 < b2 )); then return 0; fi
  if (( a2 > b2 )); then return 1; fi
  if (( a3 < b3 )); then return 0; fi
  return 1
}

compute_next_fork_n() {
  local base_ver="$1"
  local last_n
  last_n=$(git tag --list "rust-v${base_ver}-fork.*" \
              | sed -E 's/.*-fork\.([0-9]+)$/\1/' \
              | sort -n | tail -n1)
  if [[ -z "$last_n" ]]; then
    echo 1
  else
    echo $((last_n+1))
  fi
}

update_version_and_lock() {
  local full_ver="$1" # X.Y.Z-fork.N
  echo "Updating version in codex-rs/Cargo.toml -> ${full_ver}"
  if [[ "$DRY_RUN" == "true" ]]; then
    echo "[dry-run] perl -i -pe 's/^version = \".*\"/version = \"${full_ver}\"/' codex-rs/Cargo.toml"
  else
    perl -i -pe "s/^version = \".*\"/version = \"${full_ver}\"/" codex-rs/Cargo.toml
  fi

  if [[ "$NO_LOCK_UPDATE" == "true" ]]; then
    echo "Skipping Cargo.lock update (per --no-lock-update)"
  else
    echo "Updating Cargo.lock to reflect version change..."
    if [[ "$DRY_RUN" == "true" ]]; then
      echo "[dry-run] (cd codex-rs && cargo update --workspace)"
    else
      (cd codex-rs && cargo update --workspace)
    fi
  fi

  if [[ "$DRY_RUN" == "true" ]]; then
    echo "[dry-run] git add codex-rs/Cargo.toml codex-rs/Cargo.lock"
    echo "[dry-run] git commit -m 'chore: bump version to ${full_ver} for fork release'"
  else
    git add codex-rs/Cargo.toml codex-rs/Cargo.lock 2>/dev/null || git add codex-rs/Cargo.toml || true
    git commit -m "chore: bump version to ${full_ver} for fork release"
  fi
}

apply_fork_patches() {
  local base_ref="$1"   # e.g. upstream/main
  local head_ref="$2"   # e.g. main or dev
  local target_branch
  if [[ "$DRY_RUN" == "true" ]]; then
    target_branch="${BRANCH:-$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo release/fork)}"
  else
    target_branch="$(git rev-parse --abbrev-ref HEAD)"
  fi
  echo "Applying fork commits from ${base_ref}..${head_ref} onto ${target_branch}"

  # Ensure we have the upstream base branch ref available (skip actual fetch in dry-run)
  if [[ "$DRY_RUN" == "true" ]]; then
    echo "[dry-run] git fetch ${UPSTREAM_REMOTE} ${UPSTREAM_BASE} --no-tags (ensure ${base_ref} exists)"
  else
    git fetch "${UPSTREAM_REMOTE}" "${UPSTREAM_BASE}" --no-tags >/dev/null 2>&1 || true
  fi

  # Use symmetric difference with --cherry-pick to ignore commits already
  # present upstream by patch-id, and pick only commits unique to head_ref.
  local commits
  commits=$(git rev-list --reverse --right-only --cherry-pick "${base_ref}...${head_ref}" 2>/dev/null || true)

  if [[ -z "${commits}" ]]; then
    echo "No fork commits to apply (range ${base_ref}..${head_ref} is empty)"
    return 0
  fi

  # Preview list of commits (short hash + subject) in order
  echo "Commit list to cherry-pick (oldest first):"
  local count=0
  while IFS= read -r c; do
    [[ -z "$c" ]] && continue
    count=$((count+1))
    echo "  $(git show -s --format='%h %s' "$c")"
  done <<< "${commits}"
  echo "Total: ${count} commit(s)"

  if [[ "$DRY_RUN" == "true" ]]; then
    echo "[dry-run] Would cherry-pick the above commit list in order"
    return 0
  fi

  # Queue all commits in a single cherry-pick command so that
  # users can simply run `git cherry-pick --continue` to proceed
  # through the remaining commits after resolving conflicts.
  echo "Starting queued cherry-pick of ${count} commit(s)"
  # Transform newline-separated list into space-separated args
  # shellcheck disable=SC2086
  git cherry-pick -x ${commits}
}

# --- Main ---

ensure_clean_repo
fetch_tags_default

# Handle in-progress cherry-pick before any branch switching
if cherry_pick_in_progress; then
  echo "检测到正在进行的 cherry-pick 操作。"
  if [[ "$RESUME" == "true" || "$RESUME_CHERRY" == "true" ]]; then
    echo "尝试执行: git cherry-pick --continue"
    if [[ "$DRY_RUN" == "true" ]]; then
      echo "[dry-run] git cherry-pick --continue"
      if [[ "$RESUME" == "true" ]]; then
        echo "[dry-run] 准备执行收尾（改版本/打标签/推送）"
      else
        echo "[dry-run] 跳过提交重放与分支切换"
      fi
    else
      if ! git cherry-pick --continue; then
        echo "❌ 无法继续 cherry-pick。请解决冲突后再次运行 --resume（或 --resume-cherry）或手动继续。" >&2
        exit 2
      fi
    fi
    if cherry_pick_in_progress; then
      echo "仍有未完成的 cherry-pick。请继续解决冲突后再次运行 --resume。"
      exit 2
    fi
  elif [[ "$ABORT_INPROGRESS" == "true" ]]; then
    echo "自动中止进行中的 cherry-pick: git cherry-pick --abort"
    if [[ "$DRY_RUN" == "true" ]]; then
      echo "[dry-run] git cherry-pick --abort"
    else
      git cherry-pick --abort || git cherry-pick --quit || true
    fi
  else
    echo "⚠️ 存在进行中的 cherry-pick。请先处理："
    echo "   - 继续：git add -A && git cherry-pick --continue"
    echo "   - 放弃：git cherry-pick --abort  (或 git cherry-pick --quit)"
    echo "   - 或重试本脚本并传入 --abort-inprogress 自动中止"
    exit 1
  fi
fi

# Determine upstream version and tag
if [[ -n "$UPSTREAM_TAG" ]]; then
  if [[ ! "$UPSTREAM_TAG" =~ ^rust-v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "ERROR: --tag must be of form rust-vX.Y.Z" >&2
    exit 1
  fi
  X_Y_Z="${UPSTREAM_TAG#rust-v}"
else
  if [[ -z "$EXPLICIT_VERSION" ]]; then
    # Default: use base version from latest fork tag if available; otherwise latest upstream stable
    DEFAULT_FROM_FORK="$(last_fork_base_version || true)"
    if [[ -n "$DEFAULT_FROM_FORK" ]]; then
      X_Y_Z="$DEFAULT_FROM_FORK"
      echo "未提供 --version/--tag，默认使用上次 fork 基线版本：${X_Y_Z}"
    else
      LATEST_FALLBACK="$(latest_upstream_stable_version || true)"
      if [[ -n "$LATEST_FALLBACK" ]]; then
        X_Y_Z="$LATEST_FALLBACK"
        echo "未提供 --version/--tag，且没有历史 fork 标签，默认使用上游最新稳定版本：${X_Y_Z}"
      else
        echo "ERROR: 无法确定默认版本（未找到任何 rust-vX.Y.Z 或 rust-vX.Y.Z-fork.* 标签）。请使用 --version 或 --tag 指定。" >&2
        exit 1
      fi
    fi
  else
    if [[ ! "$EXPLICIT_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
      echo "ERROR: --version must be numeric X.Y.Z (got '${EXPLICIT_VERSION}')" >&2
      exit 1
    fi
    X_Y_Z="$EXPLICIT_VERSION"
  fi
  UPSTREAM_TAG="rust-v${X_Y_Z}"
fi

ensure_tag_local "$UPSTREAM_TAG"

# If given version is older than the latest upstream stable version, ask whether to switch
LATEST_UPSTREAM="$(latest_upstream_stable_version || true)"
if [[ -n "$LATEST_UPSTREAM" ]] && version_lt "$X_Y_Z" "$LATEST_UPSTREAM"; then
  echo "检测到上游存在更新版本: rust-v${LATEST_UPSTREAM} (你提供的是 rust-v${X_Y_Z})"
  if [[ "$PREFER" == "latest" ]]; then
    echo "按 --prefer-latest 选项，切换到 rust-v${LATEST_UPSTREAM} 并准备发布其 fork 版本。"
    X_Y_Z="$LATEST_UPSTREAM"
    UPSTREAM_TAG="rust-v${X_Y_Z}"
  elif [[ "$PREFER" == "given" ]]; then
    echo "按 --prefer-given 选项，继续使用 rust-v${X_Y_Z}。"
  else
    if [[ "$DRY_RUN" == "true" ]]; then
      echo "[dry-run] 询问：是否切换为最新上游版本 rust-v${LATEST_UPSTREAM} 并以其为基线发布? [y/N]"
    else
      read -r -p "是否切换为最新上游版本 rust-v${LATEST_UPSTREAM} 并以其为基线发布? [y/N] " ans
      case "${ans:-}" in
        y|Y)
          X_Y_Z="$LATEST_UPSTREAM"
          UPSTREAM_TAG="rust-v${X_Y_Z}"
          ;;
        *)
          echo "继续使用 rust-v${X_Y_Z}。" ;;
      esac
    fi
  fi
fi

ensure_tag_local "$UPSTREAM_TAG"  # ensure tag present if we switched

BRANCH="release/fork-${X_Y_Z}"

# Auto-detect a better source branch if user didn't specify and main has no unique commits
if [[ "$RESUME" != "true" && "$APPLY_PATCHES" == "true" && "$USER_SET_MAIN" == "false" ]]; then
  base_ref_autodetect="${UPSTREAM_REMOTE}/${UPSTREAM_BASE}"
  # Count unique commits for main
  cnt_main=$(git rev-list --right-only --cherry-pick "${base_ref_autodetect}...main" 2>/dev/null | wc -l | tr -d ' ' || true)
  # If dev exists, count unique commits for dev
  if git rev-parse -q --verify refs/heads/dev >/dev/null; then
    cnt_dev=$(git rev-list --right-only --cherry-pick "${base_ref_autodetect}...dev" 2>/dev/null | wc -l | tr -d ' ' || true)
  else
    cnt_dev=0
  fi
  if [[ "${cnt_main:-0}" -eq 0 && "${cnt_dev:-0}" -gt 0 ]]; then
    echo "检测到 dev 分支相对 ${base_ref_autodetect} 存在独有提交 (${cnt_dev} 个)，自动改用 dev 作为来源（可用 --main-branch 覆盖）"
    MAIN_BRANCH="dev"
  fi
fi

# Determine fork.N
if [[ -n "$EXPLICIT_FORK_N" ]]; then
  if [[ ! "$EXPLICIT_FORK_N" =~ ^[0-9]+$ ]]; then
    echo "ERROR: --fork must be a positive integer" >&2
    exit 1
  fi
  FORK_N="$EXPLICIT_FORK_N"
else
  FORK_N="$(compute_next_fork_n "$X_Y_Z")"
fi

FULL_VERSION="${X_Y_Z}-fork.${FORK_N}"
RELEASE_TAG="rust-v${FULL_VERSION}"

echo "Plan:"
echo "  Upstream tag  : ${UPSTREAM_TAG}"
echo "  Release branch: ${BRANCH} (from upstream tag)"
if [[ "$RESUME" == "true" ]]; then
  echo "  Mode          : resume (no branch switch, no replay)"
  echo "  Apply patches : false"
else
  echo "  Apply patches : ${APPLY_PATCHES} (range: ${UPSTREAM_REMOTE}/${UPSTREAM_BASE}..${MAIN_BRANCH})"
fi
echo "  Cargo version : ${FULL_VERSION}"
echo "  Release tag   : ${RELEASE_TAG}"
echo "  Push tags     : ${PUSH_TAGS} -> origin"

if [[ "$RESUME" != "true" ]]; then
  BRANCH_EXISTS="false"
  if git rev-parse -q --verify "refs/heads/${BRANCH}" >/dev/null; then
    BRANCH_EXISTS="true"
  fi

  if [[ "$DRY_RUN" == "true" ]]; then
    if [[ "$BRANCH_EXISTS" == "true" ]]; then
      echo "[dry-run] git switch ${BRANCH}"
      if [[ "$NO_RESET" == "true" ]]; then
        echo "[dry-run] (no reset due to --no-reset)"
      else
        echo "[dry-run] git reset --hard ${UPSTREAM_TAG}   # reuse existing branch, reset to upstream tag"
      fi
    else
      echo "[dry-run] git switch -c ${BRANCH} ${UPSTREAM_TAG}"
    fi
  else
    if [[ "$BRANCH_EXISTS" == "true" ]]; then
      git switch "${BRANCH}"
      if [[ "$NO_RESET" != "true" ]]; then
        git reset --hard "${UPSTREAM_TAG}"
      fi
    else
      git switch -c "${BRANCH}" "${UPSTREAM_TAG}"
    fi
  fi
else
  echo "Resume 模式：跳过分支切换/重置，直接进行收尾步骤。"
fi

if [[ "$RESUME" != "true" && "$APPLY_PATCHES" == "true" ]]; then
  apply_fork_patches "${UPSTREAM_REMOTE}/${UPSTREAM_BASE}" "${MAIN_BRANCH}"
else
  echo "Skipping patch application per --no-apply-patches"
fi

update_version_and_lock "$FULL_VERSION"

echo "Creating annotated tag ${RELEASE_TAG}"
if [[ "$DRY_RUN" == "true" ]]; then
  echo "[dry-run] git tag -a ${RELEASE_TAG} -m 'Release fork ${FULL_VERSION}'"
else
  git tag -a "${RELEASE_TAG}" -m "Release fork ${FULL_VERSION}"
fi

if [[ "$PUSH_TAGS" == "true" ]]; then
  if [[ "$DRY_RUN" == "true" ]]; then
    echo "[dry-run] git push origin ${RELEASE_TAG}"
  else
    echo "Pushing tag to origin: ${RELEASE_TAG}"
    git push origin "${RELEASE_TAG}"
  fi
else
  echo "Skipping tag push per option (--no-push-tags)."
fi

echo "Done. Local branch '${BRANCH}' is ready at $(git rev-parse --short HEAD)."
echo "Tag created: ${RELEASE_TAG}."
