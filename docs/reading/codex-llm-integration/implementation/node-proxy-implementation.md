# Node.js 代理实现方案

本文提供基于 Node.js 的 Codex LLM 代理（Proxy）完整实现方案，目标是：

- 直接兼容 Codex 的两类 Wire API：Chat Completions 与 Responses
- 支持流式（SSE）转发与必要的事件合成
- 可逐步扩展到多厂商上游（OpenAI、Azure、Ollama、其他 LLM）

> 推荐优先实现 Chat 端点（生态最广），如需 Responses 语义再补充；或通过 Provider 配置让 Codex 以 Chat 模式对接你的代理。

## 运行时环境与依赖

### 基础要求
- **Node.js** ≥ 18（内置 `fetch` 与 Web Streams API）
- **核心依赖**: `express`、`cors`
- **可选优化**: `undici`（更佳性能的 fetch）、`pino`（日志）、`rate-limiter-flexible`（限流）

### 环境配置约定

```bash
# 基础配置
PORT=3000                                    # 服务端口
UPSTREAM_BASE_URL=https://api.openai.com/v1  # 上游基础 URL
UPSTREAM_API_KEY=your_api_key                # 上游 API Key
UPSTREAM_SUPPORTS_RESPONSES=false            # 上游是否原生支持 /responses

# Azure 专用
AZURE_API_VERSION=2025-04-01-preview         # Azure API 版本

# 安全配置
ALLOW_ORIGINS=*                              # CORS 白名单（逗号分隔）

# 生产环境扩展
RATE_LIMIT_MAX=100                           # 每分钟请求限制
LOG_LEVEL=info                               # 日志级别
ENABLE_TLS=true                              # 启用 HTTPS
```

## 项目结构

```
proxy-node/
├── src/
│   ├── server.ts              # 服务器启动与路由配置
│   ├── handlers/
│   │   ├── chat.ts           # /v1/chat/completions 处理器
│   │   └── responses.ts      # /v1/responses 处理器
│   ├── utils/
│   │   ├── sse.ts           # SSE 工具函数
│   │   ├── auth.ts          # 认证中间件
│   │   └── logger.ts        # 日志工具
│   └── transform/
│       ├── chat-to-responses.ts  # Chat → Responses 事件转换
│       └── responses-to-chat.ts  # Responses → Chat 请求映射
├── package.json
├── tsconfig.json
└── Dockerfile
```

## 核心组件实现

### SSE 工具函数（utils/sse.ts）

```typescript
import type { Response } from 'express';

/**
 * 设置 SSE 响应头
 */
export function setupSSE(res: Response): void {
  res.setHeader('Content-Type', 'text/event-stream');
  res.setHeader('Cache-Control', 'no-cache');
  res.setHeader('Connection', 'keep-alive');
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Headers', 'Cache-Control');
}

/**
 * 发送 SSE 数据事件
 */
export function sseWrite(res: Response, data: unknown): void {
  res.write(`data: ${JSON.stringify(data)}\\n\\n`);
}

/**
 * 发送 SSE 注释（用于心跳）
 */
export function sseComment(res: Response, comment: string): void {
  res.write(`: ${comment}\\n\\n`);
}

/**
 * 发送流结束标记
 */
export function sseDone(res: Response): void {
  res.write('data: [DONE]\\n\\n');
}

/**
 * 设置心跳定时器
 */
export function setupHeartbeat(res: Response, interval = 30000): NodeJS.Timer {
  return setInterval(() => {
    sseComment(res, 'heartbeat');
  }, interval);
}
```

### 服务器启动（server.ts）

```typescript
import express from 'express';
import cors from 'cors';
import { chatHandler } from './handlers/chat';
import { responsesHandler } from './handlers/responses';
import { authMiddleware } from './utils/auth';
import { logger } from './utils/logger';

const app = express();

// 中间件配置
app.use(express.json({ limit: '10mb' }));
app.use(cors({
  origin: (origin, callback) => {
    const allowedOrigins = process.env.ALLOW_ORIGINS?.split(',').map(s => s.trim()) ?? ['*'];
    if (!origin || allowedOrigins.includes('*') || allowedOrigins.includes(origin)) {
      return callback(null, true);
    }
    callback(new Error('CORS: Origin not allowed'));
  }
}));

// 可选：认证中间件
if (process.env.REQUIRE_AUTH === 'true') {
  app.use('/v1', authMiddleware);
}

// 路由配置
app.post('/v1/chat/completions', chatHandler);
app.post('/v1/responses', responsesHandler);

// 健康检查
app.get('/health', (req, res) => {
  res.json({ status: 'healthy', timestamp: new Date().toISOString() });
});

// 启动服务器
const port = Number(process.env.PORT || 3000);
app.listen(port, () => {
  logger.info(`Codex proxy server listening on port ${port}`);
  logger.info(`Upstream: ${process.env.UPSTREAM_BASE_URL}`);
  logger.info(`Supports Responses API: ${process.env.UPSTREAM_SUPPORTS_RESPONSES === 'true'}`);
});

// 优雅关闭
process.on('SIGTERM', () => {
  logger.info('SIGTERM received, shutting down gracefully');
  process.exit(0);
});
```

### Chat Completions 处理器（handlers/chat.ts）

```typescript
import type { Request, Response } from 'express';
import { setupSSE, setupHeartbeat } from '../utils/sse';
import { logger } from '../utils/logger';

export async function chatHandler(req: Request, res: Response): Promise<void> {
  const startTime = Date.now();
  let heartbeatTimer: NodeJS.Timer | null = null;
  
  try {
    const baseUrl = process.env.UPSTREAM_BASE_URL!;
    const apiKey = process.env.UPSTREAM_API_KEY;
    const azureApiVersion = process.env.AZURE_API_VERSION;

    // 构建上游 URL
    const url = new URL(baseUrl + '/chat/completions');
    if (baseUrl.includes('.openai.azure.com/') && azureApiVersion) {
      url.searchParams.set('api-version', azureApiVersion);
    }

    // 强制流式响应
    const payload = { ...req.body, stream: true };

    // 构建请求头
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      'Accept': 'text/event-stream',
    };
    
    if (apiKey) {
      headers['Authorization'] = `Bearer ${apiKey}`;
    }
    
    // 透传特定头部
    ['OpenAI-Organization', 'OpenAI-Project'].forEach(header => {
      const value = req.headers[header.toLowerCase()];
      if (value && typeof value === 'string') {
        headers[header] = value;
      }
    });

    logger.info('Forwarding chat request', { 
      model: payload.model,
      messageCount: payload.messages?.length,
      hasTools: Boolean(payload.tools?.length)
    });

    // 发起上游请求
    const upstream = await fetch(url, {
      method: 'POST',
      headers,
      body: JSON.stringify(payload)
    });

    if (!upstream.ok) {
      // 透传错误状态和头部
      const errorText = await upstream.text();
      upstream.headers.forEach((value, key) => {
        if (['retry-after', 'x-ratelimit-remaining'].includes(key.toLowerCase())) {
          res.setHeader(key, value);
        }
      });
      
      logger.error('Upstream error', { 
        status: upstream.status,
        statusText: upstream.statusText,
        error: errorText.substring(0, 200)
      });
      
      return res.status(upstream.status).json(JSON.parse(errorText));
    }

    // 设置 SSE 响应
    setupSSE(res);
    heartbeatTimer = setupHeartbeat(res);
    
    // 流式转发
    const reader = upstream.body!.getReader();
    const decoder = new TextDecoder();
    
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        
        // 直接转发字节流（保持 SSE 格式）
        res.write(value);
      }
    } catch (error) {
      logger.error('Stream processing error', { error: error.message });
    }

    logger.info('Chat request completed', { 
      duration: Date.now() - startTime 
    });

  } catch (error) {
    logger.error('Chat handler error', { error: error.message, stack: error.stack });
    if (!res.headersSent) {
      res.status(500).json({ 
        error: { 
          type: 'proxy_error', 
          message: 'Internal proxy error' 
        }
      });
    }
  } finally {
    if (heartbeatTimer) {
      clearInterval(heartbeatTimer);
    }
    res.end();
  }
}
```

### Responses API 处理器（handlers/responses.ts）

```typescript
import type { Request, Response } from 'express';
import { setupSSE, setupHeartbeat } from '../utils/sse';
import { chatStreamToResponses } from '../transform/chat-to-responses';
import { responsesToChat } from '../transform/responses-to-chat';
import { logger } from '../utils/logger';

export async function responsesHandler(req: Request, res: Response): Promise<void> {
  const startTime = Date.now();
  let heartbeatTimer: NodeJS.Timer | null = null;

  try {
    const baseUrl = process.env.UPSTREAM_BASE_URL!;
    const apiKey = process.env.UPSTREAM_API_KEY;
    const supportsResponses = process.env.UPSTREAM_SUPPORTS_RESPONSES === 'true';

    setupSSE(res);
    heartbeatTimer = setupHeartbeat(res);

    if (supportsResponses) {
      // 直通模式：上游原生支持 Responses API
      await forwardResponsesDirectly(req, res, baseUrl, apiKey);
    } else {
      // 桥接模式：转换为 Chat 请求，再合成 Responses 事件
      await bridgeViaChatAPI(req, res, baseUrl, apiKey);
    }

    logger.info('Responses request completed', {
      mode: supportsResponses ? 'direct' : 'bridged',
      duration: Date.now() - startTime
    });

  } catch (error) {
    logger.error('Responses handler error', { error: error.message, stack: error.stack });
    if (!res.headersSent) {
      res.status(500).json({
        error: {
          type: 'proxy_error',
          message: 'Internal proxy error'
        }
      });
    }
  } finally {
    if (heartbeatTimer) {
      clearInterval(heartbeatTimer);
    }
    res.end();
  }
}

/**
 * 直通模式：原样转发到上游 /responses
 */
async function forwardResponsesDirectly(
  req: Request, 
  res: Response, 
  baseUrl: string, 
  apiKey?: string
): Promise<void> {
  const url = new URL(baseUrl + '/responses');
  
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    'Accept': 'text/event-stream',
    'OpenAI-Beta': 'responses=experimental',
  };
  
  if (apiKey) {
    headers['Authorization'] = `Bearer ${apiKey}`;
  }

  const upstream = await fetch(url, {
    method: 'POST',
    headers,
    body: JSON.stringify(req.body)
  });

  if (!upstream.ok) {
    const errorText = await upstream.text();
    upstream.headers.forEach((value, key) => {
      if (['retry-after'].includes(key.toLowerCase())) {
        res.setHeader(key, value);
      }
    });
    return res.status(upstream.status).json(JSON.parse(errorText));
  }

  // 原样转发 SSE 流
  const reader = upstream.body!.getReader();
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    res.write(value);
  }
}

/**
 * 桥接模式：通过 Chat API 实现 Responses 语义
 */
async function bridgeViaChatAPI(
  req: Request, 
  res: Response, 
  baseUrl: string, 
  apiKey?: string
): Promise<void> {
  // 1. 转换 Responses 请求为 Chat 格式
  const chatPayload = responsesToChat(req.body);
  
  // 2. 发起 Chat 请求
  const url = new URL(baseUrl + '/chat/completions');
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    'Accept': 'text/event-stream',
  };
  
  if (apiKey) {
    headers['Authorization'] = `Bearer ${apiKey}`;
  }

  const upstream = await fetch(url, {
    method: 'POST',
    headers,
    body: JSON.stringify(chatPayload)
  });

  if (!upstream.ok) {
    const errorText = await upstream.text();
    return res.status(upstream.status).json(JSON.parse(errorText));
  }

  // 3. 将 Chat 流转换为 Responses 事件
  await chatStreamToResponses(upstream, res);
}
```

### Chat → Responses 事件转换（transform/chat-to-responses.ts）

```typescript
import type { Response } from 'express';
import { sseWrite } from '../utils/sse';

type FunctionState = {
  id?: string;
  name?: string;
  arguments: string;
};

/**
 * 将 Chat 流式响应转换为 Responses 格式事件
 */
export async function chatStreamToResponses(
  upstream: globalThis.Response, 
  res: Response
): Promise<void> {
  let responseId = '';
  let buffer = '';
  let assistantText = '';
  
  // 工具调用状态（支持并行调用）
  const functionCalls = new Map<number, FunctionState>();

  const reader = upstream.body!.getReader();
  const decoder = new TextDecoder();

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    
    buffer += decoder.decode(value, { stream: true });
    
    // 处理 SSE 数据行
    let lineEndIndex;
    while ((lineEndIndex = buffer.indexOf('\\n\\n')) !== -1) {
      const chunk = buffer.slice(0, lineEndIndex).trim();
      buffer = buffer.slice(lineEndIndex + 2);
      
      if (!chunk.startsWith('data:')) continue;
      
      const data = chunk.slice(5).trim();
      
      // 处理流结束标记
      if (data === '[DONE]') {
        // 兜底 completed 事件（如果之前没有发送）
        sseWrite(res, { 
          type: 'response.completed', 
          id: responseId || generateId()
        });
        return;
      }

      try {
        const event = JSON.parse(data);
        responseId = event.id || responseId;
        
        const choice = event.choices?.[0];
        if (!choice) continue;
        
        const delta = choice.delta;
        
        // 处理文本增量
        if (delta?.content) {
          assistantText += delta.content;
          sseWrite(res, {
            type: 'response.output_text.delta',
            delta: delta.content
          });
        }
        
        // 处理工具调用增量
        if (delta?.tool_calls && Array.isArray(delta.tool_calls)) {
          for (const toolCall of delta.tool_calls) {
            const index = typeof toolCall.index === 'number' ? toolCall.index : 0;
            const state = functionCalls.get(index) || { arguments: '' };
            
            if (toolCall.id) state.id = toolCall.id;
            if (toolCall.function?.name) state.name = toolCall.function.name;
            if (toolCall.function?.arguments) state.arguments += toolCall.function.arguments;
            
            functionCalls.set(index, state);
          }
        }
        
        // 处理完成状态
        const finishReason = choice.finish_reason;
        
        if (finishReason === 'tool_calls') {
          // 输出所有工具调用
          for (const [, state] of functionCalls) {
            sseWrite(res, {
              type: 'response.output_item.done',
              item: {
                type: 'function_call',
                name: state.name || '',
                arguments: state.arguments || '{}',
                call_id: state.id || generateId()
              }
            });
          }
          functionCalls.clear();
          
          // 完成当前回合
          sseWrite(res, {
            type: 'response.completed',
            id: responseId || generateId()
          });
          return;
        }
        
        if (finishReason === 'stop') {
          // 输出最终助手消息
          if (assistantText.trim()) {
            sseWrite(res, {
              type: 'response.output_item.done',
              item: {
                type: 'message',
                role: 'assistant',
                content: [{ 
                  type: 'output_text', 
                  text: assistantText 
                }]
              }
            });
          }
          
          // 完成当前回合
          sseWrite(res, {
            type: 'response.completed',
            id: responseId || generateId()
          });
          return;
        }
        
      } catch (parseError) {
        // 忽略 JSON 解析错误，继续处理后续数据
        continue;
      }
    }
  }

  // 异常结束：发送失败事件
  sseWrite(res, {
    type: 'response.failed',
    error: {
      type: 'stream_error',
      message: 'Stream ended unexpectedly without completion signal'
    }
  });
}

function generateId(): string {
  return `resp_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
}
```

### Responses → Chat 请求转换（transform/responses-to-chat.ts）

```typescript
/**
 * 将 Responses API 请求转换为 Chat Completions 格式
 */
export function responsesToChat(responsesBody: any): any {
  const { model, instructions, input, tools } = responsesBody;

  // 构建 messages 数组
  const messages: any[] = [];

  // 添加系统指令
  if (instructions) {
    messages.push({
      role: 'system',
      content: instructions
    });
  }

  // 转换 input 数组为 messages
  if (Array.isArray(input)) {
    for (const item of input) {
      if (item?.type === 'message') {
        const content = extractTextContent(item.content);
        if (content) {
          messages.push({
            role: item.role || 'user',
            content
          });
        }
      } else if (item?.type === 'function_call') {
        // 工具调用消息
        messages.push({
          role: 'assistant',
          tool_calls: [{
            id: item.call_id,
            type: 'function',
            function: {
              name: item.name,
              arguments: item.arguments
            }
          }]
        });
      } else if (item?.type === 'function_call_output') {
        // 工具输出消息
        messages.push({
          role: 'tool',
          tool_call_id: item.call_id,
          content: item.output || ''
        });
      }
      // 其他类型（local_shell, web_search, custom 等）在桥接模式下暂时忽略
    }
  }

  // 转换工具定义（仅保留 function 类型）
  const chatTools = [];
  if (Array.isArray(tools)) {
    for (const tool of tools) {
      if (tool.type === 'function') {
        chatTools.push({
          type: 'function',
          function: {
            name: tool.function.name,
            description: tool.function.description,
            parameters: tool.function.parameters
          }
        });
      }
    }
  }

  // 构建 Chat API 请求
  const chatPayload: any = {
    model,
    messages,
    stream: true
  };

  if (chatTools.length > 0) {
    chatPayload.tools = chatTools;
  }

  // 透传其他兼容参数
  ['temperature', 'max_tokens', 'top_p'].forEach(param => {
    if (responsesBody[param] !== undefined) {
      chatPayload[param] = responsesBody[param];
    }
  });

  return chatPayload;
}

/**
 * 从 content 数组中提取文本内容
 */
function extractTextContent(contentArray: any[]): string {
  if (!Array.isArray(contentArray)) return '';
  
  let text = '';
  for (const item of contentArray) {
    if (item?.type === 'input_text' || item?.type === 'output_text') {
      text += item.text || '';
    }
  }
  return text;
}
```

## 构建与部署

### package.json 配置

```json
{
  "name": "codex-proxy-node",
  "version": "1.0.0",
  "type": "module",
  "private": true,
  "scripts": {
    "dev": "tsx watch src/server.ts",
    "build": "tsc && cp package.json dist/",
    "start": "node dist/server.js",
    "test": "vitest",
    "lint": "eslint src/",
    "type-check": "tsc --noEmit"
  },
  "dependencies": {
    "cors": "^2.8.5",
    "express": "^4.19.2",
    "pino": "^8.19.0"
  },
  "devDependencies": {
    "@types/cors": "^2.8.17",
    "@types/express": "^4.17.21",
    "@types/node": "^20.11.0",
    "tsx": "^4.7.0",
    "typescript": "^5.4.0",
    "vitest": "^1.2.0",
    "eslint": "^8.56.0"
  }
}
```

### TypeScript 配置

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ES2020", 
    "moduleResolution": "Bundler",
    "outDir": "dist",
    "rootDir": "src",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "declaration": false,
    "removeComments": true,
    "emitDecoratorMetadata": true,
    "experimentalDecorators": true
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist", "**/*.test.ts"]
}
```

### Docker 配置

```dockerfile
# 构建阶段
FROM node:20-alpine AS builder
WORKDIR /app

# 启用 pnpm
RUN corepack enable && corepack prepare pnpm@latest --activate

# 安装依赖
COPY package.json pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile

# 构建应用
COPY . .
RUN pnpm build

# 运行阶段  
FROM node:20-alpine AS runtime
WORKDIR /app

# 非 root 用户
RUN addgroup -g 1001 -S nodejs && adduser -S nodejs -u 1001
USER nodejs

# 复制构建产物
COPY --from=builder --chown=nodejs:nodejs /app/dist ./dist
COPY --from=builder --chown=nodejs:nodejs /app/node_modules ./node_modules

# 环境变量
ENV NODE_ENV=production
ENV PORT=3000

# 健康检查
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \\
  CMD node -e "http.get('http://localhost:3000/health', (res) => { process.exit(res.statusCode === 200 ? 0 : 1) })"

EXPOSE 3000
CMD ["node", "dist/server.js"]
```

## 生产环境考虑

### 安全加固
```typescript
// 请求大小限制
app.use(express.json({ limit: '2mb' }));

// 请求率限制
import rateLimit from 'express-rate-limit';
const limiter = rateLimit({
  windowMs: 1 * 60 * 1000, // 1分钟
  max: 100, // 每分钟最多100个请求
  message: 'Too many requests from this IP'
});
app.use('/v1', limiter);

// API Key 验证
function authMiddleware(req: Request, res: Response, next: NextFunction) {
  const apiKey = req.headers.authorization?.replace('Bearer ', '');
  if (!isValidApiKey(apiKey)) {
    return res.status(401).json({ error: 'Invalid API key' });
  }
  next();
}
```

### 监控与日志
```typescript
import pino from 'pino';

const logger = pino({
  level: process.env.LOG_LEVEL || 'info',
  formatters: {
    level: (label) => ({ level: label }),
  },
  timestamp: pino.stdTimeFunctions.isoTime,
  redact: ['req.headers.authorization', 'body.messages'] // 敏感信息脱敏
});
```

### 错误处理
```typescript
// 全局错误处理
app.use((error: Error, req: Request, res: Response, next: NextFunction) => {
  logger.error('Unhandled error', { error: error.message, stack: error.stack });
  
  if (!res.headersSent) {
    res.status(500).json({
      error: {
        type: 'internal_error',
        message: 'An internal server error occurred'
      }
    });
  }
});
```

## 与 Codex 集成

### 配置文件示例

```toml
# ~/.codex/config.toml

model = "gpt-4o"
model_provider = "node-proxy"

[model_providers.node-proxy]
name = "Node.js Proxy"
base_url = "http://localhost:3000/v1"
env_key = "NODE_PROXY_API_KEY"
wire_api = "chat"                    # 或 "responses"
request_max_retries = 3
stream_max_retries = 5
stream_idle_timeout_ms = 300000

# 可选：添加自定义头部
http_headers = { "X-Client-Version" = "1.0" }
env_http_headers = { "X-User-ID" = "USER_ID_ENV" }
```

### 启动与测试

```bash
# 设置环境变量
export UPSTREAM_BASE_URL=https://api.openai.com/v1
export UPSTREAM_API_KEY=your_openai_key
export NODE_PROXY_API_KEY=your_proxy_key

# 启动代理服务
npm run dev

# 测试 Chat API
curl -X POST http://localhost:3000/v1/chat/completions \\
  -H "Content-Type: application/json" \\
  -H "Authorization: Bearer your_proxy_key" \\
  -d '{
    "model": "gpt-4o",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": true
  }'

# 测试 Responses API  
curl -X POST http://localhost:3000/v1/responses \\
  -H "Content-Type: application/json" \\
  -H "Authorization: Bearer your_proxy_key" \\
  -d '{
    "model": "gpt-4o",
    "instructions": "You are a helpful assistant.",
    "input": [{"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Hello!"}]}]
  }'

# 配置 Codex 使用代理
codex --config model_provider=node-proxy "帮我写一个Hello World程序"
```

## 扩展功能

### 多上游支持
```typescript
const upstreamConfigs = {
  openai: { baseUrl: 'https://api.openai.com/v1', keyEnv: 'OPENAI_API_KEY' },
  azure: { baseUrl: process.env.AZURE_ENDPOINT, keyEnv: 'AZURE_API_KEY' },
  ollama: { baseUrl: 'http://localhost:11434/v1' }
};

function selectUpstream(model: string) {
  if (model.includes('gpt')) return upstreamConfigs.openai;
  if (model.includes('llama')) return upstreamConfigs.ollama;
  return upstreamConfigs.openai;
}
```

### 请求缓存
```typescript
import NodeCache from 'node-cache';
const cache = new NodeCache({ stdTTL: 300 }); // 5分钟缓存

function getCacheKey(body: any): string {
  return crypto.createHash('sha256').update(JSON.stringify(body)).digest('hex');
}
```

### 指标采集
```typescript
let requestCount = 0;
let errorCount = 0;

app.get('/metrics', (req, res) => {
  res.json({
    requests_total: requestCount,
    errors_total: errorCount,
    uptime: process.uptime()
  });
});
```

## 相关文档

- [API 规范](../api-specs/api-specifications.md) - 详细的 API 协议定义
- [事件映射](../api-specs/event-mapping.md) - 事件转换的具体规则
- [配置指南](../configuration/configuration-guide.md) - Codex 侧的 Provider 配置
- [测试验证](../testing/validation-experiments.md) - 完整的测试验证方案