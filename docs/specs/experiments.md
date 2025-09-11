# Top 3 不确定性与最小验证实验

本文识别当前方案的 3 个关键不确定性，并为每个给出最小可行实验（MVP），便于快速验证与迭代。所有实验均可在本地通过 Node ≥ 18 + curl 复现，无需真实 OpenAI Key。

---

## 不确定性 1：Chat→Responses 合成语义是否足以驱动 Codex 正常推进

- 假设：只要代理在 `/v1/responses` 合成输出 `response.output_text.delta` 与 `response.completed`（可选再补 `response.output_item.done(message)`），Codex 就能顺利推进一个回合。
- 风险：若缺少 `response.completed` 或事件顺序异常，Codex 将判定流异常，无法结束回合。

### 最小实验 E1：文本闭环

1) 启动 Mock 上游（输出标准 Chat SSE）

```ts
// mock/chat-min.ts
import http from 'node:http';
const sse = (res: http.ServerResponse, line: any) => res.write(`data: ${JSON.stringify(line)}\n\n`);
http.createServer((req, res) => {
  if (req.url === '/v1/chat/completions' && req.method === 'POST') {
    res.writeHead(200, {'Content-Type':'text/event-stream','Cache-Control':'no-cache','Connection':'keep-alive'});
    sse(res, { id: 'c1', choices: [{ delta: { content: 'Hello' } }] });
    sse(res, { id: 'c1', choices: [{ delta: { content: ' world' } }] });
    sse(res, { id: 'c1', choices: [{ finish_reason: 'stop' }] });
    res.end('data: [DONE]\n\n');
  } else { res.writeHead(404).end(); }
}).listen(3100, () => console.log('mock chat on :3100'));
```

2) 启动 Node 代理（使用 docs/specs/proxy-node.md 的 `/v1/responses` 桥接实现，`UPSTREAM_BASE_URL=http://localhost:3100`）

```bash
# 环境
export UPSTREAM_BASE_URL=http://localhost:3100
export UPSTREAM_SUPPORTS_RESPONSES=false
# 运行两个进程（或分别开两个终端）
node mock/chat-min.ts
node dist/server.js # 或 tsx src/server.ts
```

3) 调用并观测合成的 Responses 事件

```bash
curl -N http://localhost:3000/v1/responses -H 'content-type: application/json' -d '{"model":"dummy","instructions":"system","input":[]}'
```

4) 预期：能看到以下关键事件顺序
- `response.output_text.delta`（多条）
- `response.output_item.done`（message，可选）
- `response.completed`（必须）

---

## 不确定性 2：工具调用（tool_calls）分片聚合与多并发是否正确

- 假设：对 Chat 流的 `tool_calls` 分片，按 `index` 聚合 `id/name/arguments` 字段，在 `finish_reason=tool_calls` 时输出一个或多个 `response.output_item.done`（`type=function_call`）即可满足 Codex 对工具回合的期望。
- 风险：
  - `arguments` 为字符串分片，拼接顺序错误将导致非合法 JSON。
  - 多并发工具（`index` 0/1/2…）未分桶聚合会相互污染。

### 最小实验 E2：双工具并发 + 分片 arguments

1) 启动 Mock 上游（在一次回合内并发两个工具，并对 arguments 分片）

```ts
// mock/chat-tools.ts
import http from 'node:http';
const sse = (res: http.ServerResponse, line: any) => res.write(`data: ${JSON.stringify(line)}\n\n`);
http.createServer((req, res) => {
  if (req.url === '/v1/chat/completions' && req.method === 'POST') {
    res.writeHead(200, {'Content-Type':'text/event-stream','Cache-Control':'no-cache','Connection':'keep-alive'});
    // 工具 0 分片
    sse(res, { id: 'c2', choices: [{ delta: { tool_calls: [{ index: 0, id: 'tc_0', function: { name: 'apply_patch', arguments: '{"input":' } }] } }] });
    sse(res, { id: 'c2', choices: [{ delta: { tool_calls: [{ index: 0, function: { arguments: '"patch_a"}' } }] } }] });
    // 工具 1 分片
    sse(res, { id: 'c2', choices: [{ delta: { tool_calls: [{ index: 1, id: 'tc_1', function: { name: 'shell', arguments: '{"command":["echo"' } }] } }] });
    sse(res, { id: 'c2', choices: [{ delta: { tool_calls: [{ index: 1, function: { arguments: ',"hi"]}' } }] } }] });
    // 完成两个工具
    sse(res, { id: 'c2', choices: [{ finish_reason: 'tool_calls' }] });
    res.end('data: [DONE]\n\n');
  } else { res.writeHead(404).end(); }
}).listen(3101, () => console.log('mock tools on :3101'));
```

2) 代理与调用

```bash
export UPSTREAM_BASE_URL=http://localhost:3101
export UPSTREAM_SUPPORTS_RESPONSES=false
node mock/chat-tools.ts
node dist/server.js

# 观察输出中必须包含两个 function_call 输出项
curl -N http://localhost:3000/v1/responses -H 'content-type: application/json' -d '{"model":"dummy","instructions":"sys","input":[]}' | sed -n 's/^data: //p'
```

3) 判定标准：
- 至少两条 `type=response.output_item.done`，且 `item.type=function_call`
- 每条分别包含：
  - `call_id=tc_0`/`tc_1`
  - `name=apply_patch`/`shell`
  - `arguments` 为完整合法 JSON：`{"input":"patch_a"}`、`{"command":["echo","hi"]}`

---

## 不确定性 3：错误/限流在代理层的透传与 Codex 的退避协同

- 假设：代理对 429/5xx 错误应原样透传状态码与 `Retry-After` 头；Codex 会读取并按该头退避重试。若未透传，该回退将退化为指数退避，影响体验。
- 风险：当前最小实现只转发响应体文本，未传递上游头；导致 Codex 无法利用上游建议的退避时间。

### 最小实验 E3：429 + Retry-After 透传

1) Mock 上游返回 429 + 头

```ts
// mock/chat-429.ts
import http from 'node:http';
http.createServer((req, res) => {
  if (req.url === '/v1/chat/completions' && req.method === 'POST') {
    res.statusCode = 429;
    res.setHeader('content-type', 'application/json');
    res.setHeader('retry-after', '2');
    return res.end(JSON.stringify({ error: { type: 'rate_limit', message: 'Too many requests' } }));
  } else { res.writeHead(404).end(); }
}).listen(3102, () => console.log('mock 429 on :3102'));
```

2) 在代理中补充错误头透传（仅本实验所需，关键片段）

```ts
// 伪代码：在 chat.ts / responses.ts 错误分支加入
if (!upstream.ok) {
  upstream.headers.forEach((v, k) => res.setHeader(k, v)); // 关键：透传 Retry-After
  const text = await upstream.text();
  return res.status(upstream.status).type('application/json').send(text);
}
```

3) 运行与校验

```bash
export UPSTREAM_BASE_URL=http://localhost:3102
node mock/chat-429.ts
node dist/server.js

# -i 打印响应头，应包含 Retry-After: 2
curl -i http://localhost:3000/v1/chat/completions -H 'content-type: application/json' -d '{"model":"m","messages":[],"stream":true}'
```

4) 判定标准：
- HTTP 429 返回且包含 `Retry-After: 2`
- Codex 端（如改用该代理）可据此严格退避（无需更改 Codex 代码，已支持读取该头）

---

## 备注与后续

- 如计划支持完整 Responses 语义（reasoning/custom/local_shell/web_search/usage），可在 E1/E2 的 Mock 基础上逐项增加事件，按《流式事件映射》校验。
- 以上 Mock 服务均为最小可用，建议在真实上游前先本地自测通过，再接入外部 LLM 服务与 Codex CLI。

