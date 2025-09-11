# 团队协作优先事项与落地细节

本文从"高收益优先顺序 + 可操作落地细节"的角度，指导前端大团队（30+ 成员）采用 workflow 能力，形成统一的质量门禁与自愈闭环。

## 高收益优先级（建议推进顺序）

### 第一阶段：基础质量门禁（必须，ROI 最高）

#### 1) 统一 PR Gate
- **内容**：类型检查（tsc）、ESLint、单元测试（Vitest/Jest）、构建（Vite/Next/CRA 等），可选烟测（Playwright）与 Storybook 构建
- **价值**：把主干质量拉齐，减少"能过本地但过不了 CI"的反复
- **落地**：`fe_pr_gate` 工作流（见 examples.md）
- **推广策略**：
  - 先在 1-2 个核心项目试点
  - 建立 pre-push hook 调用 quick 模式
  - 收集问题反馈并优化配置

#### 2) 自愈式失败修复（强烈推荐）
- **内容**：当 Lint/TS/构建失败时，触发 `codex` 步骤生成最小补丁，并 `apply_patch` 后复测
- **价值**：显著减少反复提交/沟通成本；在"确定性关口"兜底，风险可控
- **落地**：`fe_autofix`（见 examples.md）
- **风险控制**：
  - 在 feature 分支运行，避免直接修改主干
  - 设置审批策略，重要修复需要人工确认
  - 建立回退机制和问题报告流程

### 第二阶段：质量提升与优化（推荐）

#### 3) 包体积与性能预算
- **内容**：Bundle Budget（size/总依赖/重复依赖）与 Lighthouse CI（性能/可访问性/最佳实践），超标即失败或进入人工确认
- **价值**：提前控制回归；把"体验问题"前置到开发环节
- **落地**：`fe_bundle_budget`、`fe_lhci`
- **阈值设定**：
  - Bundle size: 300KB (gzip)
  - Lighthouse 性能分: 90+
  - 可访问性分: 95+

#### 4) 视觉回归与可访问性
- **内容**：Storybook/Playwright 的截图对比（visual diff）与 axe-core 可访问性检查
- **价值**：减少 UI 非预期改动与可访问性债务
- **落地**：`fe_visual_regression`、`fe_a11y`
- **实施注意**：
  - 建立基准截图管理流程
  - 设置合理的视觉差异阈值
  - 培训团队理解可访问性检查规则

### 第三阶段：维护与治理（按需）

#### 5) 依赖升级与安全巡检（每月执行）
- **内容**：`pnpm up` + `tsc/test` 校验；若失败由 `codex` 生成补丁；附带 `pnpm audit` 或 SCA 工具输出
- **价值**：降低集中升级成本，持续健康
- **落地**：`fe_dep_upgrade`（examples 已含）
- **执行节奏**：
  - 每月第一周执行依赖升级检查
  - 安全漏洞发现立即响应
  - 主要版本升级单独规划

#### 6) Monorepo 深度集成
- **内容**：Nx/Turborepo 等针对性优化，自动按变更集选择子包运行
- **价值**：避免全量执行，缩短反馈环
- **落地**：通过 `inputs.dir` 批量触发；后续可加"变更感知"脚本
- **优化方向**：
  - 基于 git diff 的智能包选择
  - 缓存策略优化
  - 并发执行控制

## 关键实施细节

### 环境与工具标准化
- **版本固定**：
  - Node/PNPM 版本固定：推荐 `volta` 或 `.node-version`，在工作流首步校验并打印版本
  - 锁文件：本地与 CI 一律 `pnpm i --frozen-lockfile`
  - 工具版本：ESLint、TypeScript、测试框架版本统一管理

### 稳定性保障
- **端口与 E2E 稳定性**：
  - 使用随机可用端口：`PORT=$(node -e 'require("net").createServer().listen(0,()=>{console.log(this.address().port);process.exit(0)})')` 并传给 dev server
  - Playwright `webServer` 推荐在配置中管理，或在步骤里手动 `wait-on http://localhost:$PORT` 再跑测试
- **并发控制**：
  - 高并发项目将 workflow `concurrency` 控制在 CPU 核心数或以下
  - 避免资源竞争和端口冲突

### 质量门禁配置
- **统一阈值**：
  - jest/vitest 覆盖率阈值在 config 中固定
  - Lighthouse、包体积设定明确阈值与趋势文件
  - 断言方式：直接用 `shell` 步骤执行校验脚本，超标 `exit 1`；或在 workflow 中新增 `manual` 步骤进行人工放行
- **失败策略**：
  - 补丁若未通过测试，保留在 `runs` 产物中；不要自动回滚文件树，以免掩盖问题
  - 推荐在 feature 分支上运行修复，避免污染主干

### LLM 步骤优化策略
- **收敛控制**：
  - 明确输出：尽量输出 patch 或严格 JSON；文本需紧跟确定性校验步骤
  - 稳定性：设置低温度（如可用）与固定 seed（如可用）；默认不重试或仅 1 次重试
- **最佳实践**：
  - LLM 步骤仅用于"兜底修复"，不作为主要逻辑
  - 提供明确的上下文和约束条件
  - 设置合理的超时时间

### 报告与产物管理
- **报告聚合**：
  - 在 `runs/<ts>/artifacts/index.html` 汇总链接：coverage、LHCI、bundle 报告、Playwright 报告与步骤日志
  - CI 场景下可解析 `--json` 输出，在 PR 留言中贴出链接与摘要
- **缓存与性能**：
  - `pnpm fetch` 预取依赖；CI 缓存 `~/.pnpm-store`；构建产物按分支隔离
  - 设置合理的产物清理策略，避免磁盘空间问题

### 安全与权限控制
- **密钥与敏感信息**：
  - 通过 `env` 注入，严禁在日志中回显；默认保留 `approval_policy` 限制敏感命令
  - CI 下使用密文变量，本地开发使用 `.env.local`
- **审批策略**：
  - 对包含 `apply_patch` 的工作流设置合适的审批级别
  - 敏感操作（发布、数据库操作）必须有 `manual` gate

### 用户体验优化
- **快速本地体验**：
  - 提供 `quick=true` 输入参数跳过重型步骤（LHCI/visual diff），用于开发机自检
  - 在 TUI 侧提供 `/workflow run fe_pr_gate quick=true` 一键体验
- **错误诊断**：
  - 提供清晰的错误信息和修复建议
  - 建立常见问题的 troubleshooting 指南

## 团队模板（新增示例）

### PR Gate（前端）
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
```

### 包体积预算（Bundle Budget）
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

### 可访问性与视觉回归
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

## 共享与分发（组织级）
- 共享目录：推荐在公司级 catalog 仓库维护共享的 `.codex/workflows` 与 `.codex/templates`，各项目通过 git submodule/subtree 引入到 `.codex/shared`，并合并 `registry.yml`。
- 轻量打包：通过 `codex workflow pack/unpack` 在项目间分发；`pack` 生成 tar 与 `manifest.json`（条目列表、sha256、来源、版本），`unpack` 进行完整性校验并标注来源。
- 安全与治理：
  - 强敏感操作（发布、清数据）必须有 `manual` gate。
  - 模板变更走评审；`registry.yml` 中记录 semver 与变更摘要，便于同步升级。
```

## 组织与治理建议
- 建统一模板库：在公司级 repo 提供“版本化的 workflow 模板”，项目按需复制或同步升级；模板变更需走评审。
- 建“质量阈值基线”：覆盖率、包体积、LHCI 分数、E2E 烟测范围，每季度回顾调整。
- Push/PR Hook：预置 `pre-push` 调用 `fe_pr_gate` 的 quick 版本，减少 CI 红灯。
- 报表沉淀：周报/看板统计 `runs` 目录的关键指标（通过率、失败原因 TopN、平均用时）。

---

与 codex 的契合点：保留“确定性编排”骨架，局部由 `codex` 与 `mcp` 提供智能与外部能力，并通过标准化的质量关口把关，确保规模化协作下的质量与效率。
