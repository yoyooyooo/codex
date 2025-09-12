# 模板低代码（render_template/script）MVP 说明

本文阐述基于模板的低代码能力：以通用模板引擎渲染多文件改动，提供统一 diff 预览，在确认后一次性写盘；配合轻量脚本（script）完成锚点式插入与小幅改写。目标是确定性、可审、低心智，而非重型脚手架或 AST 改写。

## 是什么

- 模板渲染：使用 Jinja 家族（推荐 minijinja）渲染文件内容与文件名/路径。
- 上下文注入：将 `inputs/env/vars/steps.*` 暴露为模板变量，支持条件、循环与常用过滤器。
- 安全预览：先渲染到临时目录，使用 `git diff --no-index` 生成“多文件统一 diff”，用户确认后写盘。
- 幂等可回放：同一参数多次执行结果一致；对既有文件的“追加/插入”不破坏原内容。
- 可审与追溯：diff、渲染清单与日志落在 `runs/<ts>/artifacts`，支持审计与回滚。

## 核心能力

- 渲染能力：
  - 支持模板化路径和文件名，例如：`src/pages/{{ name | kebab_case }}.tsx`。
  - 支持 include/import 宏与片段复用（Jinja 语义）。
- 预览与确认：
  - 新增/修改/删除均在统一 diff 里展示；CLI/TUI 侧提供“接受/驳回”。
  - 驳回不写盘、无副作用；接受后一次性落地。
- 幂等策略：
  - 仅在明确的锚点或占位段内做插入/替换；多次执行不重复插入。
  - 变更尽量保持最小化，便于 code review。
- 产物管理：
  - 保存 `render_template.diff`、文件清单与校验信息；便于 PR 讨论与回退。

## 典型执行流程

1) 收集参数：从 workflow `inputs`（或 TUI 表单）获得 `name/route/...`。
2) 预渲染：构造上下文 → 渲染到临时目录。
3) 生成预览：对比“目标目录 vs 临时目录”，输出统一 diff（新增/修改/删除）。
4) 人工确认：TUI/CLI 展示摘要与详情，用户“接受/驳回”。
5) 应用与校验：接受后写盘；可选触发格式化/构建/单测等确定性校验步骤。

## 目录与模板规范（建议）

- 位置：`.codex/templates/<bundle>/<template-name>/...`
- 片段：公共片段放在 `partials/` 或 `_partials/`，通过 include/import 复用。
- 忽略：`_partials/`、`README.md` 等不会写入目标目录。
- 命名过滤器：建议提供 `kebab_case`、`snake_case`、`pascal_case` 等，便于统一命名。

## 与 script 的协作

- 适用：当需要在既有文件中做小幅、幂等的插入/替换时（如在路由聚合文件中追加一行）。
- 锚点约定：在目标文件预留锚点（如 `// @codex:routes`），脚本按照锚点进行插入，避免 AST 复杂度。
- 运行方式：`script` 支持 `sh/node/ts-node`，stdout/stderr/退出码纳入统一事件流与审批/沙箱策略。

## 适用场景

- 新建骨架：页面/组件/路由/测试/文档一把梭，统一风格与目录。
- 批量对齐：eslint/tsconfig/pnpm-workspace 等工程化文件标准化。
- 局部改造：给现有文件追加片段（env、路由表、注册表），或替换占位段。
- 组织复用：将团队最佳实践封装为模板包，跨仓分发复用（结合 pack/unpack）。

## 限制与风险

- 非 AST：不擅长对复杂既有代码做“语义级重构”；遇到复杂改造需用 codemod/AST 或 LLM patch 兜底。
- 冲突处理：若与未提交改动冲突，预览会展示差异，但不会做自动三方合并，需要人工判断。
- 二进制/大文件：二进制文件仅做新增/替换，diff 预览有限。

## MVP 验收标准

- 可用性：以“页面脚手架”为例，完成一次从渲染→预览→写盘→构建绿灯的闭环。
- 可读性：diff 清晰可审；非模板作者也能判断是否接受。
- 幂等性：同一参数重复运行输出一致；对已有文件“追加插入”可多次执行且无重复。
- 可回滚：生成 `render_template.diff` 及清单，便于代码评审与回退。

## 快速上手示例

- 模板目录：`.codex/templates/page-basic/`
  - `src/pages/{{ name | kebab_case }}.tsx`
  - `src/routes.ts`（包含锚点 `// @codex:routes` 供插入）
  - `tests/{{ name | kebab_case }}.test.ts`

- 工作流片段：

```yaml
name: scaffold_page
description: 生成页面/路由/测试的基础骨架
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

## 后续路线（展望）

- 模板 lint/校验：为模板提供 schema 校验与必填参数检查。
- 更友好的 diff 视图：在 TUI 中提供分组与高亮，长文件折叠。
- 可插拔过滤器：允许项目注册自定义命名过滤器（如 i18n key 生成）。
- 与 pack/unpack 打通：模板随 manifest 分发并进行 sha256 校验与来源标注。

