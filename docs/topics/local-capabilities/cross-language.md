# 局部特性：跨语言集成（子进程/协议流）从 0 到 1

本文介绍两种跨语言、与主会话解耦的集成方式：

1) 通过 `codex exec --json` 子进程快速获得一次性结果（推荐入门）
2) 通过 `codex proto` 使用标准化 JSON 行协议（适合自建前端/长期维护）

二者都可通过 `-c key=value` 临时覆盖配置，并通过只读沙箱与“无需审批”模式安全执行文本类任务。

## 前置条件
- 已安装并可运行 CLI（见 docs/install.md）。
- 已完成认证：`codex login` 或配置 API key（见 docs/authentication.md）。

---

## 方式一：`codex exec --json`（快速上手）

适合在任何语言里用子进程调用 Codex 完成“一次性文本任务”，例如提示词优化。优点是命令行接口稳定、集成成本低。

关键点：
- 使用 `--json` 将事件以 JSON Lines 打印到 stdout（流式）。
- 使用 `--output-last-message <file>` 将最终结果写入文件，因为 JSON 模式下不会打印 `task_complete` 事件本体。
- 通过 `-c` 传入本次调用专属的覆盖项，严禁工具调用，仅输出文本：
  - `-c base_instructions="...禁止调用任何工具，仅输出结果"`
  - `-c include_plan_tool=false`
  - `-c include_apply_patch_tool=false`
  - `-c include_view_image_tool=false`
  - `-c tools.web_search_request=false`

最小命令示例（直接在终端试跑）：

```bash
echo "把这个提示词精炼并更可执行" | \
codex exec --json \
  --output-last-message /tmp/codex_last.txt \
  -c base_instructions='你是提示词优化器。禁止调用任何工具，只输出优化后的文本，不要解释。' \
  -c include_plan_tool=false \
  -c include_apply_patch_tool=false \
  -c include_view_image_tool=false \
  -c tools.web_search_request=false \
  -

cat /tmp/codex_last.txt
```

Python 子进程示例：

```python
import json, subprocess, tempfile, os

prompt = "把这个提示词精炼并更可执行"
with tempfile.NamedTemporaryFile(delete=False) as f:
    out_file = f.name

cmd = [
    "codex", "exec", "--json",
    "--output-last-message", out_file,
    "-c", "base_instructions=你是提示词优化器。禁止调用任何工具，只输出优化后的文本，不要解释。",
    "-c", "include_plan_tool=false",
    "-c", "include_apply_patch_tool=false",
    "-c", "include_view_image_tool=false",
    "-c", "tools.web_search_request=false",
    "-",
]

proc = subprocess.Popen(
    cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    text=True
)

# 写入提示词
proc.stdin.write(prompt + "\n")
proc.stdin.close()

# 流式读取事件（可选：仅用于日志/诊断）
for line in proc.stdout:
    line = line.strip()
    if not line:
        continue
    try:
        evt = json.loads(line)
        # 这里会包含大多数事件，但不会包含 task_complete 本体
        # 可按需打印或统计
    except json.JSONDecodeError:
        pass

proc.wait()

# 读取最终结果
with open(out_file, "r", encoding="utf-8") as f:
    result = f.read().strip()

os.unlink(out_file)
print(result)
```

注意事项：
- `--json` 模式会先输出两行辅助信息：配置摘要（JSON 对象）和 `{"prompt": "..."}`；可忽略。
- 流事件里抑制了 `agent_message_delta` 等增量，且不打印 `task_complete` 本体；因此请使用 `--output-last-message` 获取最终文本。

---

## 方式二：`codex proto`（标准协议流）

适合自建前端或和其它系统深度集成。通过 stdin/stdout 交换 JSON 行：向 stdin 写入 Submission（带 `id`），从 stdout 读取 Event（包含相同 `id`）。

事件与提交的 JSON 结构（关键字段）
- Submission：
  - 形如 `{ "id": "1", "op": { "type": "user_input", "items": [ ... ] } }`
  - `items` 示例（输入文本）：`{ "type": "text", "text": "你好" }`
- Event：
  - 形如 `{ "id": "1", "msg": { "type": "agent_message", ... } }`
  - 结束标志：`{ "id": "1", "msg": { "type": "task_complete", "last_agent_message": "..." } }`

最小交互流程
1) 启动：`codex proto`（stdin 必须是管道，不可为 TTY）
2) 读取第一行：`session_configured`（包含模型、会话等信息）
3) 发送一次 `user_input` Submission（自定义 `id`）
4) 读取同 `id` 的事件流，直到 `task_complete`，取得 `last_agent_message`

简易 Node.js 示例：

```js
import { spawn } from 'node:child_process';

const p = spawn('codex', ['proto', 
  '-c', 'base_instructions=你是提示词优化器。禁止调用任何工具，只输出优化后的文本，不要解释。',
  '-c', 'include_plan_tool=false',
  '-c', 'include_apply_patch_tool=false',
  '-c', 'include_view_image_tool=false',
  '-c', 'tools.web_search_request=false',
], { stdio: ['pipe', 'pipe', 'inherit'] });

let buffer = '';
let done = false;
const id = '1';

p.stdout.setEncoding('utf8');
p.stdout.on('data', (chunk) => {
  buffer += chunk;
  let idx;
  while ((idx = buffer.indexOf('\n')) >= 0) {
    const line = buffer.slice(0, idx).trim();
    buffer = buffer.slice(idx + 1);
    if (!line) continue;
    const evt = JSON.parse(line);
    if (evt.msg?.type === 'task_complete' && evt.id === id) {
      console.log(evt.msg.last_agent_message || '');
      done = true;
      p.stdin.end();
      p.kill();
      break;
    }
  }
});

// 发送一次性输入
const submission = {
  id,
  op: {
    type: 'user_input',
    items: [{ type: 'text', text: '把这个提示词精炼并更可执行' }],
  },
};
p.stdin.write(JSON.stringify(submission) + '\n');
```

类型安全（推荐）
- 为前端/服务端生成 TypeScript 类型：
  - 在工作区根目录运行：
    - `cargo run -p codex-protocol-ts -- -o ./gen`
  - 在你的项目中引用 `./gen` 下的类型，避免手写 JSON 结构错误。

## 协议原理（proto）

传输层
- 载体：stdin/stdout 上的 JSON Lines（每行一个完整 JSON，对应一条 Submission 或 Event）。
- 方向：
  - 输入（到 Codex）：Submission（提交）
  - 输出（从 Codex）：Event（事件）

报文结构（关键字段，snake_case）
- Submission（SQ）：
  - `id: string`：客户端自定义的唯一 id，用于关联
  - `op`：操作枚举，如 `user_input`、`override_turn_context`、`interrupt` 等
    - `user_input.items: InputItem[]` 典型仅用 `{ type: "text", text: "..." }`
    - `override_turn_context` 更新后续 turn 的默认上下文（如 `model`、`sandbox_policy`、`approval_policy`）
    - `interrupt` 中断当前任务
- Event（EQ）：
  - `id: string`：与 Submission 的 id 一致
  - `msg.type`：事件类型（如 `session_configured`、`task_started`、`agent_message[_delta]`、`stream_error`、`task_complete` 等）
  - 完成标志：`task_complete` 携带 `last_agent_message: Option<string>`

生命周期与并发
- 启动后 Codex 先输出一条 `session_configured`（合成，说明会话/模型等上下文）。
- 对每个 `user_input`，通常会看到：`task_started` → 若干 `user_message`/`agent_message`/`agent_message_delta`/`agent_reasoning*`/`exec_*`/`patch_*` → `task_complete`。
- 支持多条 Submission；事件可能交错出现，但依靠 `id` 可准确归属；`interrupt` 会导致当前任务终止并发出 `turn_aborted`。

稳健性与错误
- JSON 解析错误写到 stderr（非协议事件）；协议事件只走 stdout。
- 执行错误通过 `error` 事件返回（`id` 与提交一致）。
- provider 流中断时，系统按退避策略自动重试，并通过 `stream_error` 报告；重试耗尽则以错误结束。

源码参考
- 协议定义：`codex-rs/protocol/src/protocol.rs`
- 入口实现：`codex-rs/cli/src/proto.rs`（stdin 读 Submission / stdout 写 Event）
- 事件产出：`codex-rs/core/src/codex.rs`（任务编排与各类事件）

---

## 一行命令快速验证（JSONL）

最短查看事件类型（依赖 jq）：

```bash
echo 'hi' | codex exec --json - | jq -rc 'select(.msg) | .msg.type'
```

仅提取完整的 Agent 文本（不含增量）：

```bash
echo '把这句改写更简洁' | codex exec --json - | jq -rc 'select(.msg.type=="agent_message") | .msg.message'
```

获取最终完整输出（exec 的 JSON 模式不打印 task_complete）：

```bash
tmp=$(mktemp); echo '把这句改写更简洁' | codex exec --json --output-last-message "$tmp" - >/dev/null; cat "$tmp"; rm "$tmp"
```

zsh 进程替换一行直出：

```bash
echo '把这句改写更简洁' | codex exec --json --output-last-message >(cat) - >/dev/null
```

协议流直接取 last_agent_message：

```bash
printf '%s\n' '{"id":"1","op":{"type":"user_input","items":[{"type":"text","text":"把这句改写更简洁"}]}}' \
  | codex proto \
  | jq -rc 'select(.msg.type=="task_complete") | .msg.last_agent_message'
```

可选：为“只输出文本、禁用工具”附加覆盖项（按需拼到命令后）：

```bash
-c 'base_instructions=禁止调用任何工具，只输出最终文本。' \
-c include_plan_tool=false \
-c include_apply_patch_tool=false \
-c include_view_image_tool=false \
-c tools.web_search_request=false
```

---

## 安全与沙箱建议
- 文本类局部任务：`approval_policy=never` + `sandbox=read-only` 更安全；确需写入临时文件时，可用 `workspace-write`。
- 同时在系统提示中明确“禁止调用任何工具，仅输出文本”。
- 若需网络访问或文件写入，请评估最小权限，并仅在稳定场景下放宽限制。

## 常见问题
- exec JSON 模式没有 `task_complete` 行：这是设计使然，请使用 `--output-last-message` 获取最终结果。
- `-c` 键名：工具开关为 `include_plan_tool`、`include_apply_patch_tool`、`include_view_image_tool`；网页搜索为 `tools.web_search_request`。
- 输入图片：在协议里可用 `{ "type": "image", "image_url": "data:...base64" }` 或 `{ "type": "local_image", "path": "/path/to.png" }`（后者会被自动转换）。

---

## 内部实现一瞥（便于深入集成）
- `codex exec --json`：
  - 入口：`codex-rs/exec/src/lib.rs::run_main`。
  - JSON 输出：`event_processor_with_json_output.rs` 抑制增量，打印配置摘要/提示与事件；`TaskComplete` 触发 `InitiateShutdown`；`--output-last-message` 由 `handle_last_message` 写入文件。
  - 会话与事件：内部同样通过 `ConversationManager` 新建会话、提交 `Op::UserInput`，消费事件直到完成。
- `codex proto`：
  - 入口：`codex-rs/cli/src/proto.rs`。
  - stdin 读入 `Submission`（`id` + `op`），stdout 输出 `Event`（带相同 `id`）。`EventMsg`/`Submission`/`InputItem` 等结构见 `codex-rs/protocol/src/protocol.rs`。
  - 首行会输出 `session_configured` 事件以提供上下文信息。
