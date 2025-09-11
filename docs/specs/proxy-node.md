# Node 自建代理实现方案（Chat / Responses / SSE）

本文给出基于 Node 的最小可用代理（Proxy）实现方案与实践建议，目标是：

- 直接兼容 Codex 的两类 Wire API：Chat Completions 与 Responses。
- 支持流式（SSE）转发与必要的事件合成。
- 可逐步扩展到多厂商上游（OpenAI、Azure、Ollama、其他 LLM）。

> 推荐先实现 Chat 端点（生态最广），如需 Responses 语义再补充；或通过 Provider 配置让 Codex 以 Chat 模式对接你的代理。

## 运行时与依赖

- Node.js ≥ 18（内置 `fetch` 与 Web Streams）
- 推荐：`express`、`cors`
- 可选：`undici`（更佳性能的 fetch）、`pino`（日志）、`rate-limiter-flexible`（限流）

## 配置约定（环境变量）

- `PORT`：服务端口（默认 3000）
- `UPSTREAM_BASE_URL`：上游基础 URL（如 `https://api.openai.com/v1`/`https://YOUR_PROJECT.openai.azure.com/openai`/`http://localhost:11434/v1`）
- `UPSTREAM_API_KEY`：上游 API Key（如需）
- `UPSTREAM_SUPPORTS_RESPONSES`：`true/false`，上游是否原生支持 `/responses`
- `AZURE_API_VERSION`：若走 Azure Chat，需要 `api-version`
- `ALLOW_ORIGINS`：逗号分隔的 CORS 白名单

生产加强：`RATE_LIMIT_*`、`LOG_LEVEL`、`ENABLE_TLS` 等。

## 目录结构建议

```
proxy-node/
  src/
    server.ts           # 启动/路由
    chat.ts             # /v1/chat/completions（直通或桥接）
    responses.ts        # /v1/responses（直通或合成）
    sse.ts              # SSE 工具函数（写入/心跳/错误）
    transform/
      chat-to-responses.ts  # 将 Chat 流合成为 Responses 事件
      responses-to-chat.ts  # 将 Responses 请求映射为 Chat 请求（如需）
  package.json
  tsconfig.json
  Dockerfile
```

> 若不使用 TS，可改为 `.js` 并删除类型标注。

## 通用：SSE 工具函数（sse.ts）

```ts
// src/sse.ts
import type { Response } from 'express';

export function setupSSE(res: Response) {
  res.setHeader('Content-Type', 'text/event-stream');
  res.setHeader('Cache-Control', 'no-cache');
  res.setHeader('Connection', 'keep-alive');
}

export function sseWrite(res: Response, obj: unknown) {
  res.write(`data: ${JSON.stringify(obj)}\n\n`);
}

export function sseComment(res: Response, text: string) {
  res.write(`: ${text}\n\n`);
}

export function sseDone(res: Response) {
  res.write('data: [DONE]\n\n');
}
```

## 服务器入口（server.ts）

```ts
// src/server.ts
import express from 'express';
import cors from 'cors';
import chatHandler from './chat';
import responsesHandler from './responses';

const app = express();
app.use(express.json({ limit: '2mb' }));
app.use(cors({
  origin: (origin, cb) => {
    const allow = process.env.ALLOW_ORIGINS?.split(',').map(s => s.trim()).filter(Boolean) ?? ['*'];
    if (!origin || allow.includes('*') || allow.includes(origin)) return cb(null, true);
    cb(new Error('Not allowed by CORS'));
  },
}));

app.post('/v1/chat/completions', chatHandler);
app.post('/v1/responses', responsesHandler);

const port = Number(process.env.PORT || 3000);
app.listen(port, () => console.log(`proxy listening on :${port}`));
```

## Chat 端点（chat.ts）

最小实现：将请求直通至上游 `/chat/completions` 并原样转发 SSE；适用于上游是 OpenAI/Azure/Ollama（均为 Chat 风格）。

```ts
// src/chat.ts
import type { Request, Response } from 'express';
import { setupSSE } from './sse';

export default async function chatHandler(req: Request, res: Response) {
  const base = process.env.UPSTREAM_BASE_URL!; // e.g. https://api.openai.com/v1
  const apiKey = process.env.UPSTREAM_API_KEY;
  const azureApiVersion = process.env.AZURE_API_VERSION;

  const url = new URL(base + '/chat/completions');
  if (base.includes('.openai.azure.com/') && azureApiVersion) {
    url.searchParams.set('api-version', azureApiVersion);
  }

  // 强制流式
  const payload = { ...req.body, stream: true };

  const headers: Record<string, string> = {
    'content-type': 'application/json',
    'accept': 'text/event-stream',
  };
  if (apiKey) headers['authorization'] = `Bearer ${apiKey}`;

  const upstream = await fetch(url, { method: 'POST', headers, body: JSON.stringify(payload) });

  if (!upstream.ok) {
    const text = await upstream.text();
    return res.status(upstream.status).type('application/json').send(text);
  }

  setupSSE(res);
  const reader = upstream.body!.getReader();
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      res.write(Buffer.from(value)); // 直接转发字节（包含 data: ...\n\n）
    }
  } catch (e) {
    // 客户端断开/上游终止
  } finally {
    res.end();
  }
}
```

> 说明：上游如非 OpenAI，也只需保证输出是标准 Chat SSE（`data: {...}\n\n` + 末尾 `data: [DONE]`）。

## Responses 端点（responses.ts）

两种模式：

1) 上游原生支持 `/responses`：直接转发（最省力）。
2) 上游不支持：将 Responses 请求“桥接”为 Chat 请求；再把 Chat 流“合成为 Responses 事件”。

建议优先按 1）部署；若必须支持 2），可先实现“文本最小闭环”（只处理 assistant 文本，忽略工具/推理），再逐步补齐工具与推理事件。

```ts
// src/responses.ts
import type { Request, Response } from 'express';
import { setupSSE, sseWrite } from './sse';
import { chatStreamToResponses } from './transform/chat-to-responses';

export default async function responsesHandler(req: Request, res: Response) {
  const base = process.env.UPSTREAM_BASE_URL!;
  const apiKey = process.env.UPSTREAM_API_KEY;
  const supportsResponses = process.env.UPSTREAM_SUPPORTS_RESPONSES === 'true';

  if (supportsResponses) {
    // 直通模式
    const url = new URL(base + '/responses');
    const headers: Record<string, string> = {
      'content-type': 'application/json',
      'accept': 'text/event-stream',
      'openai-beta': 'responses=experimental',
    };
    if (apiKey) headers['authorization'] = `Bearer ${apiKey}`;

    const upstream = await fetch(url, { method: 'POST', headers, body: JSON.stringify(req.body) });
    if (!upstream.ok) {
      const text = await upstream.text();
      return res.status(upstream.status).type('application/json').send(text);
    }

    // 原样转发 SSE
    setupSSE(res);
    const reader = upstream.body!.getReader();
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        res.write(Buffer.from(value));
      }
    } catch {}
    return res.end();
  }

  // 桥接模式：Responses → Chat 请求，并合成 Responses 事件
  // 最小改造：仅处理文本消息；如需完整语义，参照映射规范补齐工具/推理等。
  const { model, instructions, input, tools } = req.body ?? {};

  // 将 Responses 的 instructions+input 映射为 Chat messages（最小版，仅文本）
  const messages: any[] = [];
  if (instructions) messages.push({ role: 'system', content: instructions });
  if (Array.isArray(input)) {
    for (const item of input) {
      if (item?.type === 'message' && Array.isArray(item.content)) {
        let text = '';
        for (const c of item.content) {
          if (c?.type === 'input_text' || c?.type === 'output_text') text += c.text || '';
        }
        if (text) messages.push({ role: item.role ?? 'user', content: text });
      }
      // TODO: function/local_shell/custom/web_search 等映射，可按需补齐
    }
  }

  const payload = {
    model,
    messages,
    stream: true,
    // 可按需把 tools（若均为 function）转换为 Chat 工具；此处省略
  };

  const url = new URL(base + '/chat/completions');
  const headers: Record<string, string> = {
    'content-type': 'application/json',
    'accept': 'text/event-stream',
  };
  if (apiKey) headers['authorization'] = `Bearer ${apiKey}`;

  const upstream = await fetch(url, { method: 'POST', headers, body: JSON.stringify(payload) });
  if (!upstream.ok) {
    const text = await upstream.text();
    return res.status(upstream.status).type('application/json').send(text);
  }

  setupSSE(res);
  await chatStreamToResponses(upstream, res);
  res.end();
}
```

## Chat → Responses 事件合成（最小版）

根据《流式事件映射》，将 Chat 流的 `delta.content` 合成为 Responses 的 `response.output_text.delta`，在 τέλος 处输出 `response.completed`。

```ts
// src/transform/chat-to-responses.ts
import type { Response } from 'express';
import { sseWrite } from '../sse';

export async function chatStreamToResponses(upstream: globalThis.Response, res: Response) {
  let responseId = '';
  let buffer = '';

  const reader = upstream.body!.getReader();
  const decoder = new TextDecoder();
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });

    let idx;
    while ((idx = buffer.indexOf('\n\n')) !== -1) {
      const chunk = buffer.slice(0, idx).trim();
      buffer = buffer.slice(idx + 2);
      if (!chunk.startsWith('data:')) continue;
      const data = chunk.slice(5).trim();
      if (data === '[DONE]') {
        sseWrite(res, { type: 'response.completed', id: responseId || '' });
        return;
      }
      try {
        const json = JSON.parse(data);
        // OpenAI/兼容：choices[].delta.content
        const delta = json?.choices?.[0]?.delta;
        if (delta?.content) {
          sseWrite(res, { type: 'response.output_text.delta', delta: delta.content });
        }
        // 也可在此处理 tool_calls 增量，合成为 response.output_item.done(FunctionCall)
        responseId = json?.id || responseId;
      } catch {
        // ignore
      }
    }
  }
  // 异常结束：无 completed，输出 failed 或结束
  sseWrite(res, { type: 'response.failed', error: { message: 'stream ended without [DONE]' } });
}
```

> 进阶：
> - 解析 `tool_calls` 分片并在 finish_reason=tool_calls 时输出 `response.output_item.done`（FunctionCall）；
> - 聚合 assistant 文本为最终 `response.output_item.done(message)`；
> - 透传 token 用量（若上游有 usage），合成为 `response.completed.usage`。

## Chat → Responses 工具调用合成（进阶版）

下面在最小版基础上，加入对 `tool_calls` 的分片累计与“完成时机”的合成；同时在 `stop` 时输出一次合成的 assistant 文本消息：

```ts
// src/transform/chat-to-responses.ts（进阶版，覆盖最小版实现）
import type { Response } from 'express';
import { sseWrite } from '../sse';

type FnState = { id?: string; name?: string; args: string };

export async function chatStreamToResponses(upstream: globalThis.Response, res: Response) {
  let responseId = '';
  let buffer = '';
  let assistantText = '';
  // 以 index 作为 key，可同时支持多个并行 tool_call
  const fnCalls = new Map<number, FnState>();

  const reader = upstream.body!.getReader();
  const decoder = new TextDecoder();
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });

    let idx;
    while ((idx = buffer.indexOf('\n\n')) !== -1) {
      const chunk = buffer.slice(0, idx).trim();
      buffer = buffer.slice(idx + 2);
      if (!chunk.startsWith('data:')) continue;
      const data = chunk.slice(5).trim();
      if (data === '[DONE]') {
        // 若未显式完成，给出兜底 completed
        sseWrite(res, { type: 'response.completed', id: responseId || '' });
        return;
      }
      try {
        const json = JSON.parse(data);
        responseId = json?.id || responseId;
        const choice = json?.choices?.[0];
        const delta = choice?.delta;

        // 文本增量 → response.output_text.delta
        if (delta?.content) {
          assistantText += delta.content;
          sseWrite(res, { type: 'response.output_text.delta', delta: delta.content });
        }

        // 工具调用增量：按 index 累计 id/name/arguments
        const toolCalls = delta?.tool_calls;
        if (Array.isArray(toolCalls)) {
          for (const tc of toolCalls) {
            const i = typeof tc.index === 'number' ? tc.index : 0;
            const st = fnCalls.get(i) ?? { args: '' };
            if (tc.id) st.id = tc.id;
            const fn = tc.function;
            if (fn?.name) st.name = fn.name;
            if (fn?.arguments) st.args += fn.arguments; // 注意是字符串分片
            fnCalls.set(i, st);
          }
        }

        // 完成语义：tool_calls 或 stop
        const finish = choice?.finish_reason;
        if (finish === 'tool_calls') {
          // 在 Responses 语义里合成 FunctionCall 输出项
          for (const [, st] of fnCalls) {
            sseWrite(res, {
              type: 'response.output_item.done',
              item: {
                type: 'function_call',
                name: st.name || '',
                arguments: st.args || '',
                call_id: st.id || '',
              },
            });
          }
          fnCalls.clear();

          // turn 结束
          sseWrite(res, { type: 'response.completed', id: responseId || '' });
          return;
        }

        if (finish === 'stop') {
          // 合成最终 assistant message 为一个输出项
          if (assistantText) {
            sseWrite(res, {
              type: 'response.output_item.done',
              item: {
                type: 'message',
                role: 'assistant',
                content: [{ type: 'output_text', text: assistantText }],
              },
            });
          }
          sseWrite(res, { type: 'response.completed', id: responseId || '' });
          return;
        }
      } catch {
        // ignore
      }
    }
  }

  // 非预期终止：无 completed
  sseWrite(res, { type: 'response.failed', error: { message: 'stream ended without finish_reason/[DONE]' } });
}
```

> 如需进一步对齐 Responses 的完整语义，可在合成层加入：
> - `response.reasoning_text.delta`/`response.reasoning_summary_text.delta` 的映射（若上游可提供推理 token/内容）。
> - `custom_tool_call`、`local_shell_call`、`web_search_call` 等特定类型的输出项。

## pnpm 脚本与 TypeScript 工程示例

`package.json`：

```json
{
  "name": "codex-proxy-node",
  "version": "0.1.0",
  "type": "module",
  "private": true,
  "scripts": {
    "dev": "tsx watch src/server.ts",
    "build": "tsc -p tsconfig.json",
    "start": "node dist/server.js"
  },
  "dependencies": {
    "cors": "^2.8.5",
    "express": "^4.19.2"
  },
  "devDependencies": {
    "tsx": "^4.7.0",
    "typescript": "^5.4.0"
  }
}
```

`tsconfig.json`：

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ES2020",
    "moduleResolution": "Bundler",
    "outDir": "dist",
    "strict": true,
    "skipLibCheck": true,
    "esModuleInterop": true
  },
  "include": ["src"]
}
```

## Dockerfile 示例（多阶段，pnpm 构建）

```dockerfile
# syntax=docker/dockerfile:1
FROM node:20-slim AS build
WORKDIR /app

# 安装 pnpm（Node 18+ 自带 corepack）
RUN corepack enable && corepack prepare pnpm@latest --activate

COPY package.json pnpm-lock.yaml ./
RUN pnpm i --frozen-lockfile

COPY tsconfig.json ./
COPY src ./src
RUN pnpm build

FROM node:20-slim
WORKDIR /app
ENV NODE_ENV=production
COPY --from=build /app/node_modules ./node_modules
COPY --from=build /app/dist ./dist
EXPOSE 3000
CMD ["node", "dist/server.js"]
```

> 若使用 `npm` 或 `yarn`，将安装与构建命令替换为对应工具即可。

## 错误与重试建议

- 非流式错误：直接返回 HTTP 状态码与 JSON 错误体（至少 `message`）。
- 流式错误：Responses 合成时可发送 `response.failed` 后关闭连接。
- 透传 `Retry-After`：对 429/5xx 设置该头，Codex 会遵循或使用指数退避。

## 心跳与断线

- 建议每 15–30 秒输出一次注释心跳（`: keepalive`）以防中间设备断流。
- 客户端断开（`req.aborted`/写入抛错）时中止上游 fetch 并回收资源。

## 安全与配额

- 校验 `Authorization`（若需代理层鉴权）与来源域名（CORS）。
- 限制模型白名单、请求体大小、并发数与速率（IP/Key 维度）。
- 日志脱敏（切勿打印完整 Prompt/Key），为排障保留请求 ID 与上游 `x-request-id`。

## 部署建议

- Docker/容器：使用 Node 18+ 基础镜像，开启 `NODE_OPTIONS=--max_old_space_size=...`。
- 进程管理：`pm2`/`systemd`/K8s，配合 readiness 探针监控。
- 观测性：接入结构化日志与基础指标（QPS/错误率/平均耗时/上游 4xx/5xx 比例）。

## 与 Codex 集成（config.toml）

最小 Chat 对接：

```toml
model = "your-model"
model_provider = "your-proxy"

[model_providers.your-proxy]
name = "Your Node Proxy"
base_url = "https://your-proxy.example.com/v1"
env_key = "YOUR_PROXY_KEY"          # 若需要代理层鉴权
wire_api = "chat"                    # 推荐先用 chat 路线
request_max_retries = 4
stream_max_retries = 5
stream_idle_timeout_ms = 300000
```

若已实现 `/responses`：

```toml
[model_providers.your-proxy-resp]
name = "Your Node Proxy (Responses)"
base_url = "https://your-proxy.example.com/v1"
env_key = "YOUR_PROXY_KEY"
wire_api = "responses"
```

---

附：完整工具与推理事件的合成规则，请参考《请求与自建代理规范》《流式事件映射》。

## 工具语义合成对照表（Chat → Responses）

以下为常见上游 Chat 语义到 Responses 事件的建议映射，用于桥接模式（上游不支持 `/responses` 时）：

- Assistant 文本
  - Chat: `choices[].delta.content`
  - Responses 合成：实时发 `response.output_text.delta`；在 `finish_reason=stop` 时再发一次 `response.output_item.done`（`type=message`，`role=assistant`，`content:[{type:output_text,text:...}]`）与 `response.completed`

- 函数工具调用（function tool）
  - Chat: `choices[].delta.tool_calls[].function.{name,arguments}`（分片字符串）、`tool_calls[].id`、并以 `finish_reason=tool_calls` 表示完成
  - Responses 合成：在完成时机输出若干 `response.output_item.done`（`type=function_call`，携带 `name/arguments/call_id`），随后 `response.completed`

- 自定义工具（custom/freeform）
  - Chat: 仍表现为 function 工具（name 为工具名）
  - Responses 合成：建议仍使用 `function_call` 形态（无需强行转 `custom_tool_call`），由下游（Codex）按照工具名/Schema 执行

- 本地 Shell / Web 搜索
  - Chat: 不支持 `local_shell_call`/`web_search_call` 类型；通常以 function 工具形式出现（如 `name="shell"` 或 `name="web_search"`）
  - Responses 合成：统一映射为 `function_call`；不要伪造 `local_shell_call`/`web_search_call` 专有类型

- 推理内容（reasoning）
  - Chat: 标准流中无显式 `reasoning` 增量字段
  - Responses 合成：默认不合成 `response.reasoning_*` 事件；如上游自定义扩展了 reasoning 字段，可按《流式事件映射》扩展映射

- 用量统计（usage）
  - Chat: SSE 通常不携带 usage
  - Responses 合成：`response.completed` 可不含 `usage`（Codex 能兼容）。若上游以尾包提供 usage，可在 completed 事件中附带 `usage` 字段

- 创建/失败事件
  - Chat: 无 `response.created`/`response.failed` 直接等价
  - Responses 合成：可省略 `response.created`；错误用 HTTP 状态 + JSON 错误体返回；流中断可发 `response.failed`

## 上游事件 → 合成事件差异清单与处理建议

- Chat 仅有 function 工具；Responses 多类型工具
  - 差异：Chat 不存在 `local_shell_call`/`web_search_call`/`custom_tool_call` 的显式类型
  - 建议：桥接时一律合成为 `function_call`，由 Codex 的工具系统根据名称与 Schema 处理具体行为

- `arguments` 为字符串分片
  - 差异：Chat 将 `function.arguments` 作为字符串分片多次增量发送
  - 建议：按 `tool_calls[].index` 分桶并保序拼接；完成时机（`finish_reason=tool_calls`）再输出完整 `arguments`

- 文本与工具的完成语义不同
  - 差异：文本以 `finish_reason=stop`；工具以 `finish_reason=tool_calls`
  - 建议：分别在对应 finish 时机输出 `response.output_item.done` 并随后 `response.completed`

- Usage 缺失
  - 差异：多数 Chat 上游不提供 usage
  - 建议：`response.completed` 无 usage 也可；如上游另行提供，可补充映射

- 错误与退避
  - 差异：退避时间通常由头 `Retry-After` 指示
  - 建议：代理原样透传上游状态码与重要头；Codex 会读取并据此退避

- 事件时序与心跳
  - 差异：SSE 中间网络设备可能因静默超时断流
  - 建议：每 15–30 秒输出注释心跳（`: keepalive`）；对端断开应及时中止上游请求
