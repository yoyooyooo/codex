#!/usr/bin/env bash
set -euo pipefail

# Enhanced release helper for fork versions.
# - Integrates with our tag-based sync workflow
# - Handles Cargo.lock updates properly
# - Manages baseline tags
# - Compatible with GitHub Actions release pipeline
#
# Usage examples:
#   scripts/release_fork.sh                       # auto-detect baseline, compute next -fork.N
#   scripts/release_fork.sh --baseline 0.31.0     # override baseline
#   scripts/release_fork.sh --version 0.31.0-fork.2  # fully explicit
#   scripts/release_fork.sh --dry-run             # preview only

DRY_RUN="false"
BRANCH="main"
BASELINE=""
EXPLICIT_VERSION=""
PUSH="true"
CREATE_BASELINE_TAG="true"

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options
  --baseline X.Y.Z      Baseline upstream version (numeric). Auto-detected if not specified.
  --version  V          Full version to release (e.g. 0.31.0-fork.2). Overrides --baseline.
  --branch   NAME       Branch to release from (default: main)
  --no-push             Don't push tags and commits automatically
  --no-baseline-tag     Don't create baseline tag
  --dry-run             Preview the computed version and actions without making changes
  -h, --help            Show this help

Enhanced Features (vs original script):
  - Properly updates both Cargo.toml and Cargo.lock
  - Creates baseline tags for tracking upstream versions
  - Compatible with our tag-based sync workflow
  - Handles version validation for GitHub Actions

Examples:
  # Standard release workflow
  scripts/release_fork.sh --dry-run    # Preview
  scripts/release_fork.sh              # Execute

  # After syncing to new upstream version
  scripts/release_fork.sh --baseline 0.32.0
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --baseline)
      shift; BASELINE="${1:-}" || true ;;
    --version)
      shift; EXPLICIT_VERSION="${1:-}" || true ;;
    --branch)
      shift; BRANCH="${1:-}" || true ;;
    --no-push)
      PUSH="false" ;;
    --no-baseline-tag)
      CREATE_BASELINE_TAG="false" ;;
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

# Helper: read current Cargo.toml version numeric part
read_numeric_version_from_cargo() {
  local ver
  ver=$(grep -m1 '^version' codex-rs/Cargo.toml | sed -E 's/version *= *"([^"]+)".*/\1/')
  # Extract numeric x.y.z part
  echo "$ver" | sed -E 's/^([0-9]+\.[0-9]+\.[0-9]+).*$/\1/'
}

# Helper: compute next fork number given baseline X.Y.Z
next_fork_version() {
  local base="$1"
  local last_n
  last_n=$(git tag --list "rust-v${base}-fork.*" \
              | sed -E 's/.*-fork\.([0-9]+)$/\1/' \
              | sort -n | tail -n1)
  if [[ -z "$last_n" ]]; then
    echo "${base}-fork.1"
  else
    echo "${base}-fork.$((last_n+1))"
  fi
}

# Helper: detect a sensible baseline X.Y.Z (enhanced version)
detect_baseline() {
  # Method 1: Look for base-rust-v* tags (most reliable)
  local from_base_tag
  from_base_tag=$(git tag --list 'base-rust-v*' \
                    | sed -E 's/^base-rust-v(.*)$/\1/' \
                    | grep -E '^[0-9]+\.[0-9]+\.[0-9]+$' \
                    | sort -V | tail -n1)
  if [[ -n "$from_base_tag" ]]; then
    echo "$from_base_tag"; return 0
  fi

  # Method 2: Look for the newest rust-v* stable tag that HEAD contains
  local available_tags
  mapfile -t available_tags < <(git tag --list 'rust-v*' \
                                | sed -E 's/^rust-v(.*)$/\1/' \
                                | grep -E '^[0-9]+\.[0-9]+\.[0-9]+$' \
                                | sort -V)
  
  local latest_contained=""
  for tag in "${available_tags[@]}"; do
    if git merge-base --is-ancestor "rust-v${tag}" HEAD >/dev/null 2>&1; then
      latest_contained="$tag"
    fi
  done
  
  if [[ -n "$latest_contained" ]]; then
    echo "$latest_contained"; return 0
  fi

  # Method 3: Fallback to Cargo.toml
  read_numeric_version_from_cargo
}

# Environment checks
ensure_clean_repo() {
  if [[ "$DRY_RUN" == "true" ]]; then
    echo "[dry-run] Skipping clean repo check"
    return 0
  fi
  
  if ! git diff --quiet || ! git diff --cached --quiet || [ -n "$(git ls-files --others --exclude-standard)" ]; then
    echo "ERROR: You have uncommitted or untracked changes." >&2
    exit 1
  fi
}

ensure_correct_branch() {
  local current_branch
  current_branch=$(git symbolic-ref --short -q HEAD 2>/dev/null || true)
  if [[ "$current_branch" != "$BRANCH" ]]; then
    echo "ERROR: Current branch is '$current_branch'. Switch to '$BRANCH' before releasing." >&2
    exit 1
  fi
}

# Main logic
main() {
  ensure_clean_repo
  ensure_correct_branch

  # Compute target version
  if [[ -z "$EXPLICIT_VERSION" ]]; then
    if [[ -z "$BASELINE" ]]; then
      BASELINE="$(detect_baseline)"
      echo "Auto-detected baseline: $BASELINE"
    fi
    if ! [[ "$BASELINE" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
      echo "ERROR: Could not determine a valid baseline (got '${BASELINE}'). Use --baseline X.Y.Z or --version X.Y.Z-fork.N" >&2
      exit 1
    fi
    TARGET_VERSION="$(next_fork_version "$BASELINE")"
  else
    TARGET_VERSION="$EXPLICIT_VERSION"
  fi

  echo "🚀 Release plan:"
  echo "  Branch         : ${BRANCH}"
  echo "  Target version : ${TARGET_VERSION}"
  echo "  Tag            : rust-v${TARGET_VERSION}"
  echo "  Baseline tag   : $([ "$CREATE_BASELINE_TAG" == "true" ] && echo "base-rust-v${BASELINE}" || echo "none")"
  echo "  Push           : $PUSH"

  if [[ "$DRY_RUN" == "true" ]]; then
    echo ""
    echo "[dry-run] Actions that would be performed:"
    echo "  1. Update codex-rs/Cargo.toml version to $TARGET_VERSION"
    echo "  2. Update codex-rs/Cargo.lock to match new version"
    echo "  3. Commit version changes"
    if [[ "$CREATE_BASELINE_TAG" == "true" && -n "$BASELINE" ]]; then
      echo "  4. Create baseline tag base-rust-v${BASELINE}"
    fi
    echo "  5. Create release tag rust-v${TARGET_VERSION}"
    if [[ "$PUSH" == "true" ]]; then
      echo "  6. Push commits and tags to origin"
    fi
    exit 0
  fi

  # Step 1: Update Cargo.toml version
  echo "📝 Updating version in Cargo.toml..."
  perl -i -pe "s/^version = \".*\"/version = \"$TARGET_VERSION\"/" codex-rs/Cargo.toml
  
  # Step 2: Update Cargo.lock
  echo "🔄 Updating Cargo.lock..."
  (cd codex-rs && cargo update --workspace)
  
  # Step 3: Commit changes
  echo "💾 Committing version changes..."
  git add codex-rs/Cargo.toml codex-rs/Cargo.lock
  git commit -m "chore: bump version to $TARGET_VERSION for release"
  
  # Step 4: Check baseline tag exists (create if requested)
  if [[ "$CREATE_BASELINE_TAG" == "true" && -n "$BASELINE" ]]; then
    local baseline_tag="base-rust-v${BASELINE}"
    if ! git tag | grep -q "^${baseline_tag}$"; then
      echo "⚠️  Baseline tag missing: $baseline_tag"
      echo "💡 Consider running: scripts/sync_upstream.sh init-baseline rust-v${BASELINE} --push-tags"
      echo "🏷️  Creating baseline tag: $baseline_tag"
      git tag -a "$baseline_tag" -m "Baseline: $BASELINE"
    else
      echo "ℹ️  Baseline tag $baseline_tag already exists"
    fi
  fi
  
  # Step 5: Create release tag
  echo "🏷️  Creating release tag: rust-v${TARGET_VERSION}"
  git tag -a "rust-v${TARGET_VERSION}" -m "Fork version $TARGET_VERSION"
  
  # Step 6: Push if requested
  if [[ "$PUSH" == "true" ]]; then
    echo "🚀 Pushing to origin..."
    git push origin "$BRANCH"
    git push origin "rust-v${TARGET_VERSION}"
    if [[ "$CREATE_BASELINE_TAG" == "true" && -n "$BASELINE" ]]; then
      git push origin "base-rust-v${BASELINE}" 2>/dev/null || true
    fi
  fi
  
  echo ""
  echo "✅ Release completed successfully!"
  echo "📦 Release tag: rust-v${TARGET_VERSION}"
  if [[ "$PUSH" == "true" ]]; then
    echo "🔄 GitHub Actions should start the release workflow shortly"
    echo "🌐 Check: https://github.com/$(git remote get-url origin | sed 's/.*github.com[:/]\([^/]*\/[^/]*\)\.git.*/\1/')/actions"
  else
    echo "⚠️  Remember to push manually: git push origin $BRANCH rust-v${TARGET_VERSION}"
  fi
}

main "$@"