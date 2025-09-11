# 2B 业务团队的高收益工作流（优先清单）

面向以业务为核心的 B2B 前端团队，以下工作流比通用的前端 CI/可视化更“贴业务”：它们围绕接口契约、权限矩阵、核心业务流程、数据质量与集成稳定性构建质量门禁与自愈闭环。

## 1) API 契约守护（OpenAPI/GraphQL）
- 价值：避免后端改动导致前端类型/校验失真，尽早发现“破坏性变更”。
- 做法：拉取当前环境的 API schema，与基线对比；若存在 breaking diff，触发 Codex 生成最小补丁（类型/客户端/校验更新），随后跑 typecheck/test 验证。
- 触发：PR 前、合入前、日常巡检。

YAML 片段（示意）：
```yaml
name: api_contract_guard
inputs:
  spec_url: { type: string, required: true }
  baseline: { type: string, default: ".codex/baselines/openapi.json" }
steps:
  - id: fetch
    uses: shell
    run: curl -s {{ inputs.spec_url }} -o current.json

  - id: diff
    uses: shell
    needs: [fetch]
    run: |
      npx @redocly/openapi-cli lint current.json || true
      npx oasdiff {{ inputs.baseline }} current.json > diff.json || true
    artifacts: [diff.json]

  - id: analyze
    uses: shell
    needs: [diff]
    run: node scripts/check_breaking_diff.js diff.json  # 返回非零即视为破坏性

  - id: codex_fix
    uses: codex
    if: "{{ steps.analyze.status == 'failure' }}"
    prompt: |
      根据 openapi diff.json 的破坏性变更，更新 TS 类型、API 客户端与表单校验，生成最小补丁。
    capture: { patch: apply_patch }

  - id: apply
    uses: apply_patch
    if: "{{ steps.codex_fix.outputs.patch is defined }}"
    patch: "{{ steps.codex_fix.outputs.patch }}"

  - id: verify
    uses: shell
    needs: [apply]
    run: pnpm exec tsc --noEmit && pnpm test -r
```

## 2) 核心业务流程回归（领域旅程）
- 价值：与“视觉回归”相比，B2B 更需要“规则与状态机”正确。
- 做法：用一组 HTTP 调用/脚本串起典型旅程（如：创建客户→下单→折扣→审批→开票），对关键不变量断言（金额、状态、权限）。

YAML 片段（示意，以 curl 为例）：
```yaml
name: journey_order_to_cash
env:
  BASE: http://dev.api.internal
steps:
  - id: create_customer
    uses: shell
    run: |
      curl -s "$BASE/customers" -H 'Content-Type: application/json' \
        -d '{"name":"ACME"}' | tee .out1.json
    capture: { stdout_json_pointer: "/id" }
    outputs: { customer_id: value }

  - id: create_order
    uses: shell
    needs: [create_customer]
    run: |
      curl -s "$BASE/orders" -H 'Content-Type: application/json' \
        -d '{"customerId":"{{ steps.create_customer.outputs.customer_id }}","amount":1000}' | tee .out2.json
    capture: { stdout_json_pointer: "/id" }
    outputs: { order_id: value }

  - id: approve
    uses: shell
    needs: [create_order]
    run: curl -s -X POST "$BASE/orders/{{ steps.create_order.outputs.order_id }}/approve" -o /dev/null

  - id: assert
    uses: shell
    needs: [approve]
    run: node scripts/assert_invoice.js {{ steps.create_order.outputs.order_id }}
```

## 3) 权限矩阵（RBAC）校验
- 价值：防“越权/错权”；对 B2B 至关重要。
- 做法：维护 `permission_matrix.yaml`，遍历用户/角色/资源/动作，运行 Playwright/脚本验证 200/403、UI 禁用/隐藏。

YAML 片段：
```yaml
name: rbac_matrix
steps:
  - id: check
    uses: shell
    run: pnpm exec ts-node scripts/check_rbac.ts permission_matrix.yaml
```

## 4) 数据基准与匿名化/脱敏演练
- 价值：保证联调/演示环境数据质量与合规。
- 做法：导入基准数据 → 跑脱敏脚本 → 报告覆盖率/异常记录。

YAML 片段：
```yaml
name: seed_and_mask
steps:
  - id: seed
    uses: shell
    run: pnpm exec ts-node scripts/seed.ts ./fixtures
  - id: mask
    uses: shell
    needs: [seed]
    run: pnpm exec ts-node scripts/mask_pii.ts
  - id: report
    uses: shell
    needs: [mask]
    run: node scripts/report_masking.js > masking_report.md
    artifacts: [masking_report.md]
```

## 5) CSV 导入校验器（业务规则）
- 价值：B2B 常见“批量导入”场景，规则复杂易出错。
- 做法：对典型 CSV 跑导入 Dry‑run，输出逐行错误与建议修复；必要时触发 Codex 生成纠错脚本或文档。

YAML 片段：
```yaml
name: csv_import_validator
inputs:
  file: { type: string, required: true }
steps:
  - id: dryrun
    uses: shell
    run: pnpm exec ts-node scripts/import_dryrun.ts {{ inputs.file }} > import_errors.csv || true
    artifacts: [import_errors.csv]
  - id: assert
    uses: shell
    needs: [dryrun]
    run: node -e "require('fs').statSync('import_errors.csv').size===0||process.exit(1)"
```

## 6) 集成桩与离线开发（Mock Harness）
- 价值：B2B 集成多系统（ERP/支付/CRM），本地需稳定桩服务快速联调。
- 做法：起 mock（如 Prism/WireMock）并用业务旅程回归用例验证契约一致性。

YAML 片段：
```yaml
name: mock_harness
steps:
  - id: start
    uses: shell
    run: npx prism mock openapi.yml & echo $! > .prism.pid && sleep 1
  - id: probe
    uses: shell
    needs: [start]
    run: curl -s http://localhost:4010/health
  - id: stop
    uses: shell
    needs: [probe]
    run: kill $(cat .prism.pid) || true
```

## 7) 特性开关发布闸门（Feature Flag Gate）
- 价值：B2B 常靠灰度/租户级开关发布；需要自动化验证关键指标。
- 做法：开启开关 → 跑烟测 → 不通过则自动关闭并报告。

YAML 片段：
```yaml
name: ff_gate
inputs:
  flag: { type: string, required: true }
steps:
  - id: enable
    uses: shell
    run: node scripts/ff_toggle.js {{ inputs.flag }} on
  - id: smoke
    uses: shell
    needs: [enable]
    run: pnpm exec playwright test --grep=@smoke
  - id: disable_on_fail
    uses: shell
    if: "{{ steps.smoke.status == 'failure' }}"
    run: node scripts/ff_toggle.js {{ inputs.flag }} off
```

## 8) 关键接口 SLA 烟测
- 价值：把“业务可用性”放在第一优先级；通过简单时延阈值挡住明显回归。

YAML 片段：
```yaml
name: sla_smoke
env:
  BASE: https://dev.api.internal
steps:
  - id: ping
    uses: shell
    run: |
      for p in /auth/login /orders /customers; do
        t=$(curl -o /dev/null -s -w '%{time_total}' "$BASE$p") || exit 1
        echo "$p: $t"; python - "$t" <<'PY' || exit 1
import sys
print(float(sys.argv[1]) < 1.0)
PY
      done
```

## 9) 设计系统/组件库迁移（Codemod + 验收）
- 价值：B2B 项目长周期运行，组件升级与弃用频繁。
- 做法：扫描使用点 → 运行 codemod → Codex 生成补丁补齐边缘用法 → typecheck/test 验收。

## 10) i18n 完备性与死链检查
- 价值：B2B 多租户与多区域常见；缺失翻译与无效路由影响体验。
- 做法：扫描翻译 key 覆盖率、冗余 key，校验关键路由/菜单可达性。

---

与 Codex 的契合点：
- 以“确定性编排”固化业务旅程与集成自检；用 `codex` 步骤做跨仓库/跨模块的补丁生成与解释文档输出；
- 全程复用审批/沙箱，适合本地与 CI，产物可审计。
