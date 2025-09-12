# 示例与最佳实践

本文提供了完整的工作流示例，按使用场景分类，涵盖从基础构建测试到复杂业务流程的各种场景。

## 基础开发流程

### 1) Rust 项目构建与单测（基础模板）

```yaml
name: build_and_test
description: CI 预置流程
inputs:
  branch: { type: string, default: main }
  release: { type: bool, default: false }
concurrency: 2
steps:
  - id: checkout
    uses: shell
    run: git checkout {{ inputs.branch }}

  - id: deps
    uses: shell
    needs: [checkout]
    run: pnpm i

  - id: unit
    uses: shell
    needs: [deps]
    run: cargo test -p codex-core
    retry: { max_attempts: 2, backoff: 2s }

  - id: fix
    uses: codex
    if: "{{ steps.unit.status == 'failure' && inputs.release }}"
    prompt: |
      单测失败了。请生成补丁修复它，并保证测试通过。
    capture:
      patch: apply_patch

  - id: apply
    uses: apply_patch
    if: "{{ steps.fix.outputs.patch is defined }}"
    patch: "{{ steps.fix.outputs.patch }}"
```

### 2) 版本号校验与发布（人工确认）

```yaml
name: tag_release
description: 校验版本号并人工确认后打 tag
inputs:
  version: { type: string, required: true }
steps:
  - id: check-semver
    uses: shell
    run: |
      if ! echo {{ inputs.version }} | grep -Eq '^v?[0-9]+\.[0-9]+\.[0-9]+$'; then
        echo "invalid version"
        exit 1
      fi

  - id: confirm
    uses: manual
    needs: [check-semver]
    message: "将要创建 tag {{ inputs.version }}，请确认。"

  - id: tag
    uses: shell
    needs: [confirm]
    run:
      - git tag {{ inputs.version }}
      - git push origin {{ inputs.version }}
```

## 低代码与脚手架

### 3) 页面脚手架（render_template + script）
> 详见模板低代码说明：[render_template](./render_template.md)

```yaml
name: scaffold_page
description: 生成页面/路由/权限/测试的基础骨架
inputs:
  app_dir: { type: string, default: "apps/admin" }
  name: { type: string, required: true }
  route: { type: string, required: true }
steps:
  - id: tpl
    uses: render_template
    template: page-basic
    target_dir: {{ inputs.app_dir }}
    params: { name: "{{ inputs.name }}", route: "{{ inputs.route }}" }

  - id: inject
    uses: script
    needs: [tpl]
    entry: ts/inject_route.ts
    args: ["{{ inputs.app_dir }}/src/router.tsx", "{{ inputs.route }}", "{{ inputs.name }}Page"]
```

## 外部集成

### 4) 使用 MCP 工具生成报告

```yaml
name: report
description: 调用 MCP 生成代码分析报告
steps:
  - id: analyze
    uses: mcp
    server: local-analyzer
    tool: project_analyze
    args: { path: "." }
    outputs:
      json: $

  - id: save
    uses: shell
    needs: [analyze]
    run: |
      printf "%s" '{{ steps.analyze.outputs.json }}' > analysis.json
    artifacts:
      - analysis.json
```

## 前端工作流集合

### 5) 前端 CI：安装依赖、类型检查、Lint、单元测试与构建

```yaml
name: fe_ci
description: 前端基础 CI 流程（pnpm + Vite + Vitest/Jest）
inputs:
  dir: { type: string, default: "." }
env:
  CI: "true"
steps:
  - id: install
    uses: shell
    cwd: {{ inputs.dir }}
    run: pnpm i --frozen-lockfile

  - id: typecheck
    uses: shell
    needs: [install]
    cwd: {{ inputs.dir }}
    run: pnpm exec tsc --noEmit

  - id: lint
    uses: shell
    needs: [install]
    cwd: {{ inputs.dir }}
    run: pnpm exec eslint "src/**/*.{ts,tsx,js,jsx}"

  - id: unit
    uses: shell
    needs: [install]
    cwd: {{ inputs.dir }}
    run: |
      if pnpm ls vitest > /dev/null 2>&1; then pnpm exec vitest run --coverage; \
      else pnpm exec jest --ci --coverage; fi
    artifacts:
      - {{ inputs.dir }}/coverage

  - id: build
    uses: shell
    needs: [typecheck, lint, unit]
    cwd: {{ inputs.dir }}
    run: |
      if pnpm ls vite > /dev/null 2>&1; then pnpm exec vite build; \
      else pnpm run build; fi
    artifacts:
      - {{ inputs.dir }}/dist
```

### 6) 自动修复：当 Lint/格式化/类型检查失败时由 Codex 生成补丁

```yaml
name: fe_autofix
description: Lint/Prettier/TS 失败时尝试由 Codex 自动修复
inputs:
  dir: { type: string, default: "." }
steps:
  - id: check
    uses: shell
    cwd: {{ inputs.dir }}
    run: |
      set -e
      pnpm i --frozen-lockfile
      pnpm exec prettier -c .
      pnpm exec eslint "src/**/*.{ts,tsx,js,jsx}"
      pnpm exec tsc --noEmit

  - id: fix
    uses: codex
    if: "{{ steps.check.status == 'failure' }}"
    prompt: |
      这是一个前端项目（可能是 React + Vite）。
      请根据 lint/格式化/类型检查错误生成最小补丁进行修复：
      - 优先修复 ESLint 规则与 TypeScript 类型错误；
      - 二选一时，保持项目既有的编码风格与导入路径习惯；
      - 不引入额外依赖；
      - 变更保持最小化，并确保构建与测试能够通过。
    capture:
      patch: apply_patch

  - id: apply
    uses: apply_patch
    if: "{{ steps.fix.outputs.patch is defined }}"
    patch: "{{ steps.fix.outputs.patch }}"
```

### 7) 端到端测试（Playwright）

```yaml
name: fe_e2e_playwright
description: 使用 Playwright 跑 E2E，webServer 由 playwright.config.ts 管理
inputs:
  dir: { type: string, default: "." }
steps:
  - id: install
    uses: shell
    cwd: {{ inputs.dir }}
    run: pnpm i --frozen-lockfile

  - id: browsers
    uses: shell
    needs: [install]
    cwd: {{ inputs.dir }}
    run: pnpm exec playwright install --with-deps

  - id: e2e
    uses: shell
    needs: [browsers]
    cwd: {{ inputs.dir }}
    timeout: 10m
    run: pnpm exec playwright test --reporter=list
    artifacts:
      - {{ inputs.dir }}/playwright-report
```

### 8) 性能与可访问性审计（Lighthouse CI）

```yaml
name: fe_lhci
description: 构建后用 Lighthouse CI 做性能/可访问性评估
inputs:
  dir: { type: string, default: "." }
steps:
  - id: build
    uses: shell
    cwd: {{ inputs.dir }}
    run: pnpm run build

  - id: lhci
    uses: shell
    needs: [build]
    cwd: {{ inputs.dir }}
    run: npx @lhci/cli autorun
    artifacts:
      - {{ inputs.dir }}/.lighthouseci
```

### 9) Storybook 构建与测试

```yaml
name: fe_storybook
description: 构建 Storybook 并运行测试
inputs:
  dir: { type: string, default: "." }
steps:
  - id: build-sb
    uses: shell
    cwd: {{ inputs.dir }}
    run: pnpm exec storybook build --output-dir storybook-static
    artifacts:
      - {{ inputs.dir }}/storybook-static

  - id: test-sb
    uses: shell
    needs: [build-sb]
    cwd: {{ inputs.dir }}
    run: |
      if pnpm ls @storybook/test-runner > /dev/null 2>&1; then pnpm exec test-storybook --watch=false; \
      else echo "skip test-storybook: @storybook/test-runner not installed"; fi
```

### 10) 依赖升级并自动修复类型/编译问题

```yaml
name: fe_dep_upgrade
description: 升级依赖、运行检查、失败时由 Codex 生成修复补丁
inputs:
  dir: { type: string, default: "." }
steps:
  - id: upgrade
    uses: shell
    cwd: {{ inputs.dir }}
    run: |
      pnpm i --frozen-lockfile
      pnpm up -L --latest

  - id: verify
    uses: shell
    needs: [upgrade]
    cwd: {{ inputs.dir }}
    run: |
      pnpm exec tsc --noEmit
      pnpm test -r || true

  - id: codex-fix
    uses: codex
    if: "{{ steps.verify.status == 'failure' }}"
    prompt: |
      依赖升级后出现类型/构建/测试错误，请生成最小修复补丁：
      - 不降级依赖；
      - 优先修复类型声明与 API 变更适配；
      - 确保 `pnpm exec tsc --noEmit` 与主要测试通过。
    capture:
      patch: apply_patch

  - id: apply
    uses: apply_patch
    if: "{{ steps.codex-fix.outputs.patch is defined }}"
    patch: "{{ steps.codex-fix.outputs.patch }}"
```

## 质量门禁与报告

### 11) PR Gate（整合示例，适合团队推广）

```yaml
name: fe_pr_gate
description: 前端 PR 质量门禁（本地/CI 通用）
inputs:
  dir: { type: string, default: "." }
  quick: { type: bool, default: false }
steps:
  - id: env
    uses: shell
    cwd: {{ inputs.dir }}
    run: |
      node -v && pnpm -v
      pnpm i --frozen-lockfile

  - id: typecheck
    uses: shell
    needs: [env]
    cwd: {{ inputs.dir }}
    run: pnpm exec tsc --noEmit

  - id: lint
    uses: shell
    needs: [env]
    cwd: {{ inputs.dir }}
    run: pnpm exec eslint "src/**/*.{ts,tsx,js,jsx}"

  - id: unit
    uses: shell
    needs: [env]
    cwd: {{ inputs.dir }}
    run: pnpm test -r --reporter=default --coverage
    artifacts:
      - {{ inputs.dir }}/coverage

  - id: build
    uses: shell
    needs: [typecheck, lint, unit]
    cwd: {{ inputs.dir }}
    run: |
      if pnpm ls vite > /dev/null 2>&1; then pnpm exec vite build; else pnpm run build; fi
    artifacts:
      - {{ inputs.dir }}/dist

  - id: e2e-smoke
    uses: shell
    if: "{{ not inputs.quick }}"
    needs: [build]
    cwd: {{ inputs.dir }}
    timeout: 10m
    run: |
      pnpm exec playwright install --with-deps
      pnpm exec playwright test --project=chromium --grep=@smoke --reporter=list
    artifacts:
      - {{ inputs.dir }}/playwright-report

> 提示：结合 `fe_autofix` 与模板脚手架（见 [render_template](./render_template.md)）可在质量门禁以外加速新功能骨架搭建。
```

### 12) 包体积预算与报告

```yaml
name: fe_bundle_budget
description: 生成 bundle 报告并校验体积阈值
inputs:
  dir: { type: string, default: "." }
  max_kb: { type: number, default: 300 }
steps:
  - id: build
    uses: shell
    cwd: {{ inputs.dir }}
    run: pnpm run build
  - id: analyze
    uses: shell
    needs: [build]
    cwd: {{ inputs.dir }}
    run: |
      pnpm dlx source-map-explorer "dist/**/*.js" --html report.html || true
      SIZE=$(node -e "const fs=require('fs');let s=0;fs.readdirSync('dist').forEach(f=>{if(f.endsWith('.js'))s+=fs.statSync('dist/'+f).size});console.log(Math.ceil(s/1024))")
      echo "BUNDLE_KB=$SIZE"
      test "$SIZE" -le {{ inputs.max_kb }} || { echo "Bundle size ${SIZE}KB exceeds budget {{ inputs.max_kb }}KB"; exit 1; }
    artifacts:
      - {{ inputs.dir }}/report.html
      - {{ inputs.dir }}/dist
```

### 13) 可访问性与视觉回归（Playwright）

```yaml
name: fe_quality_ui
description: axe 可访问性检查 + 视觉回归（Playwright 截图对比）
inputs:
  dir: { type: string, default: "." }
steps:
  - id: setup
    uses: shell
    cwd: {{ inputs.dir }}
    run: pnpm i --frozen-lockfile && pnpm exec playwright install --with-deps
  - id: a11y
    uses: shell
    needs: [setup]
    cwd: {{ inputs.dir }}
    run: pnpm exec playwright test --grep=@a11y --reporter=list
    artifacts:
      - {{ inputs.dir }}/playwright-report
  - id: visual
    uses: shell
    needs: [setup]
    cwd: {{ inputs.dir }}
    run: pnpm exec playwright test --grep=@visual --reporter=list
    artifacts:
      - {{ inputs.dir }}/playwright-report
```

## 最佳实践总结

### 工作流设计原则
- **确定性优先**：核心逻辑用 shell/script 步骤实现，LLM 仅在需要时介入
- **失败快速**：在早期步骤做基础检查，避免无效的后续执行
- **产物透明**：重要输出均写入 artifacts，便于审查和调试
- **可重复执行**：支持幂等操作，多次运行结果一致

### LLM 步骤使用建议
- **明确输出格式**：优先要求输出 patch 或严格校验的 JSON
- **低温度设置**：使用低温度（如可）和固定 seed（如可）提高稳定性
- **不盲目重试**：失败时转入确定性校验步骤，而非盲目重试

### 模板与脚本组织
- **模板层次化**：将通用模板放在 `.codex/templates/shared`，项目特定模板放在对应目录
- **脚本复用**：常用脚本抽象为独立模块，通过参数化支持不同场景
- **版本管理**：通过 `registry.yml` 记录模板版本和来源信息
