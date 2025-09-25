# 渲染与 <mode_instructions> 规范

输出结构
- 无常驻模式：仅包裹基线文本。
- 存在常驻模式：在基线后追加 `<mode_instructions>`，每个模式一个区块，启用时间升序。

示例
```
<user_instructions>

{base_user_instructions.trim()}

<mode_instructions>
### Mode: {display_name}
- scope: {scope_label}
- variables: foo=bar, active=True

{rendered_body}

</mode_instructions>

</user_instructions>
```

规则
- 变量替换按 frontmatter 执行；默认/显式值均参与；变量顺序为声明顺序。
- 模式顺序：启用时间升序；列表/渲染顺序一致。
- 等价检测：全文相同则不发送覆写（is_equivalent）。
- 去抖：本地 150–300ms 聚合变更后再尝试覆写。

等价检测规范（规范化步骤）
- 统一换行为 LF（\n）。
- 去除每行尾随空格；收敛连续空行为最多 1 行。
- 固定块间空行：
  - `<user_instructions>` 头尾各 1 空行；
  - 基线与 `<mode_instructions>` 之间 1 空行；
  - 各模式块之间 1 空行；块内标题与 `variables` 行之间不加额外空行。
- 变量序列化：`key=value`，按变量声明顺序，使用英文逗号加单空格分隔。
- 比较策略：对规范化后的完整文本进行字节级比较，相同则视为等价，跳过发送。

模板异常与定位
- 渲染失败返回 `E3201 TemplateError`，建议包含：`file`（源文件路径）、`id`（模式 ID）、`message`（错误信息）、`snippet`（可选，出错的模板片段）。
- 若变量缺失导致渲染失败，应在上游校验阶段（E310x）提前拦截，避免进入模板引擎。

错误
- 未知模式：E1201；模板错误：E3201；变量校验失败：E310x（前端阻止发送）。
