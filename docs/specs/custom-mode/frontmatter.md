# Frontmatter 规范（YAML）

总则
- frontmatter 位于 Markdown 顶部 `---` 包围的 YAML；正文紧随其后。
- `variables` 使用数组以保证声明顺序。

字段
- kind: `persistent | instant`（默认 `persistent`；本阶段仅实现 persistent）
- display_name: string（可选）
- description: string（可选）
- argument_hint: string（可选）
- default_enabled: bool（可选；会话创建时若变量校验通过则自动启用）
- variables: VariableDef[]（可空数组）

VariableDef
- name: string（必填，`[A-Za-z0-9_-]+`）
- type: `text|enum|boolean|number|path`（可选；若存在 `enum` 字段，则强制为 `enum`；否则默认为 `text`）
- default: string|number|boolean（可选）
- required: bool（可选，默认 false）
- enum: string[]（可选；非空且唯一）
- shortcuts: string[]（可选；短旗标如 `-t` 或键值前缀如 `ticket=`）
- pattern: string（可选；Rust regex；对 `enum` 忽略）
- inline_edit: bool（可选）
- mode_scoped: bool（可选；默认 false）

校验
- 变量名唯一（E2101）；`enum` 非空且元素唯一（E2102）。
- `pattern` 可编译（E2201）；校验使用整串匹配。
- `required=true` 的变量在启用前必须具有有效值（默认/历史/当前输入）。

Slash 参数消费顺序
1) 按变量声明顺序匹配 `shortcuts`（完整匹配）；
2) 剩余位置参数按变量声明顺序消费；
3) 类型校验：`enum` 严格匹配；`boolean` 接受 true/false（大小写不敏感）；`number` 解析为 f64；`path` 原样保留。

错误
- 未知模式：E1201；变量缺失/不匹配：E310x；模板渲染异常：E3201。

示例见 rendering.md。

完整示例
```yaml
---
kind: persistent
display_name: Review
description: Review code with ticket context
variables:
  - name: ticket
    type: text
    required: true
    shortcuts: ["-t", "ticket="]
    pattern: "^[A-Z]+-[0-9]+$"
  - name: severity
    enum: [low, medium, high]
    required: true
  - name: strict
    type: boolean
    default: true
    required: false
    mode_scoped: true
---
Ensure code references ticket {{ticket}}; severity={{severity}}.
```

类型与推断
- 若存在 `enum` 字段，则 `type` 强制为 `enum`（忽略手动 `type`）。
- 无 `type`、无 `enum` 时，默认 `type=text`。
- `boolean`/`number` 类型用于解析与 UI 控件选择；渲染时转为字符串。

保序硬约束
- `variables` 必须为 YAML 数组以保证顺序；若使用映射（无序），应报错提示并拒绝加载（建议 E2103 BadVariablesType）。

错误示例
```yaml
---
kind: persistent
variables:
  severity:  # 映射而非数组（无序）
    enum: [low, medium, high]
  ticket:
    required: true
---
```
期望：加载失败并给出错误码（如 E2103 BadVariablesType），提示“variables 必须为有序数组”。

短参消费规则示例
- 声明：`ticket` 定义了 `shortcuts: ["-t", "ticket="]`
- 输入：`/review -t 123 foo`
  - 命中 `-t`，绑定 `ticket=123`
  - 剩余位置参数 `foo` 按声明顺序尝试消费（若无匹配可留空或报错，取决于是否 `required`）
