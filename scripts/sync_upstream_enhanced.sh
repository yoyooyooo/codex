#!/usr/bin/env bash
set -euo pipefail

# Enhanced upstream sync script for fork management
# 针对上游使用发布分支策略的增强版本同步脚本

UPSTREAM_URL_DEFAULT="https://github.com/openai/codex.git"
DRY_RUN="false"
FORCE_TAGS="false"

usage() {
  cat <<EOF
Usage: $(basename "$0") <command> [options]

Fork-aware Commands
  smart-sync                智能同步 - 分析上游状态并推荐同步策略  
  fork-release <version>    创建fork版本 (如: rust-v0.31.0-fork.1)
  compare-upstream          详细对比fork与上游的差异状态
  sync-to-main              同步到上游main分支最新状态
  sync-to-tag <tag>         同步到特定上游标签状态，但保持fork特性
  
Analysis Commands  
  upstream-status           显示上游当前状态和发布信息
  fork-status               显示fork当前状态和版本信息
  
Legacy Commands (保留兼容性)
  merge-main                合并上游main分支
  merge-tag <tag>           合并特定标签
  list-tags                 列出上游标签
  
Options
  --dry-run                 预览操作，不实际执行
  --push                    推送结果到远程
  --branch <name>           目标分支 (default: main)
  --upstream-url <url>      上游仓库URL
  -h, --help                显示帮助

Examples
  # 推荐的日常使用
  $(basename "$0") smart-sync --dry-run    # 分析并预览推荐操作
  $(basename "$0") smart-sync --push       # 执行智能同步
  
  # 创建fork版本
  $(basename "$0") fork-release rust-v0.31.0-fork.1 --push
  
  # 分析状态
  $(basename "$0") compare-upstream
EOF
}

# 智能分析上游状态
analyze_upstream_status() {
  echo "🔍 分析上游状态..."
  
  # 获取最新信息
  git fetch upstream --prune >/dev/null 2>&1 || true
  git fetch upstream 'refs/tags/rust-v*:refs/tags/rust-v*' >/dev/null 2>&1 || true
  
  local latest_stable_tag=$(git tag -l 'rust-v[0-9]*\.[0-9]*\.[0-9]*' --sort=-version:refname | head -1)
  local upstream_main=$(git rev-parse upstream/main)
  local current_head=$(git rev-parse HEAD)
  
  echo "📊 上游状态:"
  echo "  最新稳定版: $latest_stable_tag"
  echo "  上游main: ${upstream_main:0:8}"
  echo "  本地HEAD: ${current_head:0:8}"
  
  # 分析关系
  local common_base=$(git merge-base HEAD upstream/main 2>/dev/null || echo "")
  if [[ -n "$common_base" ]]; then
    local ahead=$(git rev-list --count upstream/main..HEAD 2>/dev/null || echo "0") 
    local behind=$(git rev-list --count HEAD..upstream/main 2>/dev/null || echo "0")
    echo "  关系: 领先${ahead}个提交，落后${behind}个提交"
  fi
  
  # 检查标签关系
  if git merge-base --is-ancestor "$latest_stable_tag" HEAD >/dev/null 2>&1; then
    echo "  ✓ 本地包含最新稳定版 $latest_stable_tag"
  else
    echo "  ⚠️  本地不包含最新稳定版 $latest_stable_tag"
  fi
  
  echo ""
}

# 智能同步逻辑
smart_sync() {
  analyze_upstream_status
  
  echo "💡 智能同步建议:"
  
  local latest_stable=$(git tag -l 'rust-v[0-9]*\.[0-9]*\.[0-9]*' --sort=-version:refname | head -1)
  local behind=$(git rev-list --count HEAD..upstream/main 2>/dev/null || echo "0")
  local ahead=$(git rev-list --count upstream/main..HEAD 2>/dev/null || echo "0")
  
  if [[ "$behind" == "0" && "$ahead" -gt "0" ]]; then
    echo "  状态: 你的fork是最新的，且有独有功能"
    echo "  建议: 创建fork版本标签"
    echo "  命令: $(basename "$0") fork-release ${latest_stable}-fork.1"
  elif [[ "$behind" -gt "0" && "$ahead" == "0" ]]; then
    echo "  状态: 你的fork落后于上游"  
    echo "  建议: 同步到上游main分支"
    echo "  命令: $(basename "$0") sync-to-main --push"
  elif [[ "$behind" -gt "0" && "$ahead" -gt "0" ]]; then
    echo "  状态: 你的fork有分叉，需要合并上游更新"
    echo "  建议: 合并上游main分支，保留fork特性"
    echo "  命令: $(basename "$0") sync-to-main --push"
  else
    echo "  状态: fork与上游同步"
    echo "  建议: 无需操作或考虑创建基线标签"
  fi
  
  echo ""
  
  if [[ "$DRY_RUN" == "false" ]]; then
    read -p "是否执行推荐操作? (y/N): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
      echo "执行智能同步..."
      # 这里可以自动执行推荐的操作
      sync_to_main
    fi
  fi
}

# 同步到main分支
sync_to_main() {
  echo "🔄 同步到上游main分支..."
  
  if [[ "$DRY_RUN" == "true" ]]; then
    echo "[dry-run] git pull upstream main --rebase"
    return 0
  fi
  
  # 检查工作树是否干净
  if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "❌ 工作树不干净，请先提交或暂存更改"
    exit 1
  fi
  
  # 执行rebase
  git pull upstream main --rebase || {
    echo "❌ rebase失败，可能有冲突需要解决"
    exit 1
  }
  
  echo "✅ 同步完成"
}

# 创建fork版本
fork_release() {
  local version="$1"
  
  if [[ -z "$version" ]]; then
    echo "❌ 需要指定版本号，如: rust-v0.31.0-fork.1"
    exit 1
  fi
  
  echo "🏷️  创建fork版本: $version"
  
  if [[ "$DRY_RUN" == "true" ]]; then
    echo "[dry-run] git tag -a \"$version\" -m \"Fork version $version with custom enhancements\""
    return 0
  fi
  
  # 检查标签是否已存在
  if git rev-parse --verify "refs/tags/$version" >/dev/null 2>&1; then
    echo "❌ 标签 $version 已存在"
    exit 1
  fi
  
  # 创建标签
  git tag -a "$version" -m "Fork version $version with custom enhancements"
  echo "✅ 创建标签: $version"
  
  # 可选推送
  if [[ "${PUSH:-false}" == "true" ]]; then
    git push origin "$version"
    echo "✅ 推送标签到远程"
  fi
}

# 对比上游状态
compare_upstream() {
  echo "📊 详细对比fork与上游状态"
  echo "================================"
  
  analyze_upstream_status
  
  echo "🔍 提交差异分析:"
  local common_base=$(git merge-base HEAD upstream/main)
  
  echo ""
  echo "从共同祖先以来的提交:"
  echo "  共同祖先: $(git log -1 --format='%h %s' "$common_base")"
  
  echo ""
  echo "上游新增提交:"
  git log --oneline "$common_base"..upstream/main | sed 's/^/  /'
  
  echo ""
  echo "fork独有提交:" 
  git log --oneline "$common_base"..HEAD | sed 's/^/  /'
  
  echo ""
  echo "🏷️  标签状态:"
  local latest_stable=$(git tag -l 'rust-v[0-9]*\.[0-9]*\.[0-9]*' --sort=-version:refname | head -1)
  local latest_fork=$(git tag -l 'rust-v*-fork.*' --sort=-version:refname | head -1 || echo "无")
  echo "  最新上游稳定版: $latest_stable"
  echo "  最新fork版本: $latest_fork"
}

# 主函数
main() {
  local cmd="${1:-}"
  
  if [[ $# -eq 0 ]]; then
    usage; exit 1
  fi
  
  shift || true
  
  # 解析全局选项
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --dry-run)
        DRY_RUN="true" ;;
      --push)
        PUSH="true" ;;
      --branch)
        shift; BRANCH="${1:-main}" ;;
      --upstream-url)
        shift; UPSTREAM_URL="${1:-$UPSTREAM_URL_DEFAULT}" ;;
      -h|--help)
        usage; exit 0 ;;
      --*)
        echo "未知选项: $1" >&2; exit 1 ;;
      *)
        # 这是位置参数，重新处理
        set -- "$1" "$@"
        break ;;
    esac
    shift || true
  done
  
  case "$cmd" in
    smart-sync)
      smart_sync ;;
    fork-release)
      fork_release "${1:-}" ;;
    compare-upstream)
      compare_upstream ;;
    sync-to-main)
      sync_to_main ;;
    upstream-status)
      analyze_upstream_status ;;
    fork-status)
      echo "🔍 Fork状态分析功能待实现" ;;
    *)
      echo "未知命令: $cmd" >&2
      echo "使用 --help 查看可用命令" >&2
      exit 1 ;;
  esac
}

main "$@"