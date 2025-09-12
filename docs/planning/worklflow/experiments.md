# Top3 不确定性与最小实验（MVP 验证）

本页记录当前方案中影响成败的三类关键不确定性，并给出“最小可行实验（POC）”设计，用于快速验证与去风险。完成这些 POC 后，再进入全面实现与文档固化。

## A. Codex 步骤输出可控性（补丁/JSON 稳定抽取）
- 假设
  - 在不同事件序列（工具调用先后、插话信息、失败重试）下，仍能稳定抽取“最终补丁”或“结构化文本/JSON”作为下游输入。
- 最小实验
  - 输入：3 份离线事件日志样本（jsonl），覆盖：
    1) 有 apply_patch 审批并成功的回合（应抽到 patch）
    2) 仅文本答案无补丁的回合（应抽到 text）
    3) 申请补丁但被拒/中断的回合（应返回无 patch 且状态失败/中断）
  - 方法：实现极小“事件归并器”脚本（TS/Rust 皆可）→ 读取 jsonl → 输出 `{status, text?, patch?}`。
    - 归并规则：按事件 id/类型与时序收敛，选“最后完成”的 agent 消息；apply_patch 以“本轮最终批准且成功应用的 unified diff”为准。
  - 观测点：
    - 插入无关事件或扰乱顺序不改变最终抽取结果
    - 中间思考/工具事件不会误判为最终结果
  - 成功标准：
    - 3 个样本分别得到预期结果与正确 `status`
    - 对样本做无害扰动（插入/重排）后结果不变
  - 预计成本与风险：0.5–1 天；样本不全 → 结合现有 exec 的 --json 日志导出片段 + 手写边界样本

## B. 模板渲染 + 多文件 Diff 预览可用性（无需 AST 先覆盖 80%）
（实现与交互细节见：[render_template](./render_template.md)）
- 假设
  - 使用通用模板引擎（推荐 Jinja 家族）+ 多文件渲染 + 统一 diff 预览 + 手动确认，能覆盖 80% 的“脚手架/批量修改”场景，且心智负担低。
- 最小实验
  - 输入：最小代码模板（scaffold-page）：组件/路由/测试各 1 文件；变量：`name`、`route`、`permissions`。
  - 方法：实现 `render_template` 雏形：
    1) 读取模板 → 渲染到临时目录
    2) 使用 `git diff --no-index` 对比“目标目录 VS 渲染产物”
    3) TUI/CLI 打印人类可读 diff；确认后写入（或拒绝退出）
    4) 覆盖两类修改：新增文件、对既有文件的追加插入
  - 观测点：
    - diff 是否清晰可读；失败/冲突时反馈是否清楚
    - 修改变量后重放是否得到稳定一致的结果
  - 成功标准：
    - 完成一次 3 文件脚手架生成，diff 可读，非模板作者也能确认
    - 对已有文件的追加不破坏原内容，多次运行可复现
  - 预计成本与风险：1–2 天；UX 不足 → 先不做 AST，仅追求“可读 + 可控”，AST 留待下一轮

## C. 团队共享与可信分发（pack/unpack/来源与校验）
- 假设
  - 通过轻量打包（tar + manifest）即可让工作流/模板在项目间低摩擦共享，并提供来源与完整性提示。
- 最小实验
  - 输入：一个样例目录（`.codex/workflows/hello.yaml` 与 `.codex/templates/code/scaffold-page/*`）。
  - 方法：
    1) `pack`：将 `.codex/{workflows,templates,registry.yml}` 打包为 tar，生成 `manifest.json`（条目清单、sha256、来源 repo/url、版本）
    2) `unpack`：在另一个项目解包到 `.codex/shared`，合并 registry；`list` 时展示来源与校验状态
  - 观测点：
    - 同名冲突是否友好提示/合并
    - TUI/CLI 是否能清晰显示来源与校验结果
    - 篡改包后能被识别并拒绝/警示
  - 成功标准：
    - 在 B 项目成功列出/运行 A 项目的 workflow 与模板；显示来源与完整性
    - 人为篡改后 unpack 拒绝或显著标红提示
  - 预计成本与风险：1–2 天；签名体系过重 → MVP 先做 sha256 完整性 + 来源显示，签名放后续

## 并行推进与优先顺序
- A 与 B 可并行（互不依赖）；完成后再做 C（需具备 1–2 个可共享条目以便验证）。
- 建议顺序：A (0.5–1d) + B (1–2d) → C (1–2d)。

## POC 产物清单（交付标准）
- A：`tools/event-reducer.(ts|rs)` + 3 份事件样本（jsonl）+ 运行示例与期望输出
- B：`tools/render_template` 命令雏形 + `templates/scaffold-page` + 两轮渲染的 diff 截图/文本
- C：`tools/workflow-pack` / `tools/workflow-unpack` + manifest 样例 + 被篡改的失败用例

完成以上 POC 后，再将相应设计与实现细节沉淀回方案文档，并进入工程化落地。
