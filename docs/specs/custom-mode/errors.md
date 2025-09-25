# 错误类型与编号

编号区间
- E100x：发现/IO/非法 ID
- E120x：引用模式缺失
- E200x：frontmatter 解析
- E210x：声明冲突/非法值（枚举/重复变量）
- E220x：正则相关
- E310x：变量校验失败
- E320x：渲染模板错误

清单（示例）
- E1001 IllegalId：文件或目录名包含非法字符；跳过。
- E1004 Io：目录/文件读取失败；继续扫描其它文件。
- E1201 UnknownMode：启用集合或 Slash 引用的 ID 未在 defs 中出现。
- E2001 Frontmatter：YAML 解析失败。
- E2101 VarDup：变量名重复。
- E2102 EnumInvalid：`enum` 为空或含重复值。
- E2201 Regex：`pattern` 无法编译。
- E3101 RequiredMissing：必填变量缺失。
- E3102 EnumMismatch：不在枚举内。
- E3103 PatternMismatch：不满足正则。
- E3106 BooleanInvalid：布尔值非法（仅 true/false）。
- E3107 NumberInvalid：数字非法（i64/f64 可解析）。
- E3108 PathInvalid：路径非法（非空、无控制字符）。
- E3201 TemplateError：模板替换失败或占位符不闭合。

输出要求
- 结构建议：`{"code":"EXXXX","message":"...","hint":"...","file":"...","id":"/a:b"}`（字段可选）。
 - UI 显示：TUI 在模式标签显示 `⚠` 并在详情列出；无需 CLI 输出。

说明
- E3106/E3107/E3108 为本阶段 TUI 前拦截用的细分错误（布尔/数字/路径），避免与 E3103 PatternMismatch 语义冲突；如上游日后统一编号，可做内聚映射，不影响协议。

继续/跳过规则
- E1004（IO）继续扫描其它目录/文件；
- E1001（IllegalId）跳过该项但不中断流程；
- E1201（UnknownMode）在引用时报告，但合并定义阶段不报错。
