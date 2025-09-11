# 实现指南：构建 Node.js 代理

## 实现概述

本章提供基于 Node.js 的自建代理（Proxy）完整实现方案，目标是：

- 🔄 **协议桥接**：支持 Chat ↔ Responses 双向转换
- 🚀 **流式处理**：完整的 SSE 事件流转发与合成
- 🌐 **多厂商支持**：可扩展至各种上游 LLM 服务
- 🛠️ **生产就绪**：包含错误处理、监控、部署方案

### 实现策略选择

| 实现模式 | 适用场景 | 复杂度 | 推荐度 |
|----------|----------|--------|--------|
| **Chat 直通** | 上游支持 Chat API | 低 | ⭐⭐⭐ 首选 |
| **Responses 直通** | 上游支持 Responses API | 低 | ⭐⭐ 如果可用 |
| **Chat → Responses 桥接** | 需要完整语义支持 | 中 | ⭐⭐ 按需实现 |

## 技术栈与依赖

### 运行时环境

```json
{
  "engines": {
    "node": ">=18.0.0"
  },
  "type": "module"
}
```

**核心特性**：
- **内置 fetch**：Node 18+ 原生支持
- **Web Streams**：流式处理标准 API
- **ES Modules**：现代模块系统

### 项目依赖

```json
{
  "dependencies": {
    "express": "^4.19.2",      // Web 框架
    "cors": "^2.8.5",          // CORS 支持
    "helmet": "^7.1.0",        // 安全头
    "pino": "^8.17.0",         // 高性能日志
    "undici": "^6.6.0"         // 可选：高性能 HTTP 客户端
  },
  "devDependencies": {
    "tsx": "^4.7.0",           // TypeScript 执行器
    "typescript": "^5.4.0",    // TypeScript 编译器
    "@types/express": "^4.17.21",
    "@types/cors": "^2.8.17"
  }
}
```

### 开发工具链

```json
{
  "scripts": {
    "dev": "tsx watch src/server.ts",
    "build": "tsc && npm run copy-assets",
    "start": "node dist/server.js",
    "test": "tsx test/integration.test.ts",
    "copy-assets": "cp -r src/static dist/"
  }
}
```

## 项目结构设计

```
codex-proxy/
├── src/
│   ├── server.ts              # 服务器入口
│   ├── routes/
│   │   ├── chat.ts           # Chat Completions 端点
│   │   ├── responses.ts      # Responses 端点
│   │   └── health.ts         # 健康检查
│   ├── lib/
│   │   ├── sse.ts           # SSE 工具函数
│   │   ├── config.ts        # 配置管理
│   │   ├── logger.ts        # 日志系统
│   │   └── validation.ts    # 请求验证
│   ├── transform/
│   │   ├── chat-to-responses.ts    # Chat → Responses 转换
│   │   ├── responses-to-chat.ts    # Responses → Chat 转换
│   │   └── tools-mapping.ts       # 工具调用映射
│   ├── middleware/
│   │   ├── auth.ts          # 鉴权中间件
│   │   ├── rate-limit.ts    # 限流中间件
│   │   └── error-handler.ts # 错误处理
│   └── types/
│       ├── openai.ts        # OpenAI API 类型
│       └── codex.ts         # Codex 内部类型
├── test/
│   ├── integration/         # 集成测试
│   ├── fixtures/           # 测试数据
│   └── mocks/              # Mock 服务
├── config/
│   ├── development.toml    # 开发配置
│   ├── production.toml     # 生产配置
│   └── local.toml.example  # 配置模板
├── docker/
│   ├── Dockerfile          # 容器镜像
│   └── docker-compose.yml  # 本地开发环境
└── docs/
    ├── api.md              # API 文档
    └── deployment.md       # 部署指南
```

## 核心实现

### 服务器入口

```typescript
// src/server.ts
import express from 'express';
import cors from 'cors';
import helmet from 'helmet';
import pino from 'pino';

import { loadConfig } from './lib/config.js';
import { createLogger } from './lib/logger.js';
import { authMiddleware } from './middleware/auth.js';
import { rateLimitMiddleware } from './middleware/rate-limit.js';
import { errorHandler } from './middleware/error-handler.js';

import chatRoutes from './routes/chat.js';
import responsesRoutes from './routes/responses.js';
import healthRoutes from './routes/health.js';

async function createServer() {
  const config = await loadConfig();
  const logger = createLogger(config.log);
  const app = express();

  // 安全和基础中间件
  app.use(helmet());
  app.use(cors({
    origin: config.cors.allowedOrigins,
    credentials: true
  }));
  
  // 请求解析
  app.use(express.json({ 
    limit: config.server.maxRequestSize 
  }));

  // 自定义中间件
  app.use(authMiddleware(config.auth));
  app.use(rateLimitMiddleware(config.rateLimit));

  // 路由注册
  app.use('/v1/chat', chatRoutes);
  app.use('/v1/responses', responsesRoutes);
  app.use('/health', healthRoutes);

  // 错误处理（必须在最后）
  app.use(errorHandler(logger));

  return { app, logger, config };
}

async function main() {
  const { app, logger, config } = await createServer();
  
  const server = app.listen(config.server.port, () => {
    logger.info(
      `Codex Proxy listening on port ${config.server.port}`,
      { 
        environment: config.environment,
        upstreamUrl: config.upstream.baseUrl
      }
    );
  });

  // 优雅关闭
  process.on('SIGTERM', () => {
    logger.info('SIGTERM received, shutting down gracefully');
    server.close(() => {
      logger.info('Server closed');
      process.exit(0);
    });
  });
}

main().catch(console.error);
```

### 配置管理

```typescript
// src/lib/config.ts
export interface ProxyConfig {
  environment: 'development' | 'production' | 'test';
  server: {
    port: number;
    maxRequestSize: string;
    timeout: number;
  };
  upstream: {
    baseUrl: string;
    apiKey?: string;
    supportsResponses: boolean;
    timeout: number;
    retries: number;
  };
  cors: {
    allowedOrigins: string[];
  };
  rateLimit: {
    windowMs: number;
    maxRequests: number;
  };
  auth: {
    enabled: boolean;
    apiKeys: string[];
  };
  log: {
    level: string;
    format: 'json' | 'pretty';
  };
}

export async function loadConfig(): Promise<ProxyConfig> {
  const environment = process.env.NODE_ENV || 'development';
  
  // 基础配置
  const config: ProxyConfig = {
    environment: environment as ProxyConfig['environment'],
    server: {
      port: parseInt(process.env.PORT || '3000'),
      maxRequestSize: process.env.MAX_REQUEST_SIZE || '10mb',
      timeout: parseInt(process.env.SERVER_TIMEOUT || '300000'),
    },
    upstream: {
      baseUrl: process.env.UPSTREAM_BASE_URL || 'https://api.openai.com/v1',
      apiKey: process.env.UPSTREAM_API_KEY,
      supportsResponses: process.env.UPSTREAM_SUPPORTS_RESPONSES === 'true',
      timeout: parseInt(process.env.UPSTREAM_TIMEOUT || '300000'),
      retries: parseInt(process.env.UPSTREAM_RETRIES || '3'),
    },
    cors: {
      allowedOrigins: process.env.CORS_ORIGINS?.split(',') || ['*'],
    },
    rateLimit: {
      windowMs: parseInt(process.env.RATE_LIMIT_WINDOW || '60000'),
      maxRequests: parseInt(process.env.RATE_LIMIT_MAX || '100'),
    },
    auth: {
      enabled: process.env.AUTH_ENABLED === 'true',
      apiKeys: process.env.API_KEYS?.split(',') || [],
    },
    log: {
      level: process.env.LOG_LEVEL || 'info',
      format: (process.env.LOG_FORMAT as 'json' | 'pretty') || 'json',
    },
  };

  // 配置验证
  validateConfig(config);
  
  return config;
}

function validateConfig(config: ProxyConfig): void {
  if (!config.upstream.baseUrl) {
    throw new Error('UPSTREAM_BASE_URL is required');
  }
  
  if (config.auth.enabled && config.auth.apiKeys.length === 0) {
    throw new Error('API_KEYS required when AUTH_ENABLED=true');
  }
  
  // 更多验证逻辑...
}
```

### SSE 工具库

```typescript
// src/lib/sse.ts
import { Response } from 'express';

export class SSEWriter {
  private response: Response;
  private isConnected = true;

  constructor(response: Response) {
    this.response = response;
    this.setupSSE();
    this.setupErrorHandlers();
  }

  private setupSSE(): void {
    this.response.writeHead(200, {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache', 
      'Connection': 'keep-alive',
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Headers': 'Cache-Control'
    });
  }

  private setupErrorHandlers(): void {
    this.response.on('close', () => {
      this.isConnected = false;
    });

    this.response.on('error', (err) => {
      console.error('SSE connection error:', err);
      this.isConnected = false;
    });
  }

  writeData(data: any): boolean {
    if (!this.isConnected) return false;
    
    try {
      this.response.write(`data: ${JSON.stringify(data)}\n\n`);
      return true;
    } catch (err) {
      console.error('Failed to write SSE data:', err);
      this.isConnected = false;
      return false;
    }
  }

  writeComment(comment: string): boolean {
    if (!this.isConnected) return false;
    
    try {
      this.response.write(`: ${comment}\n\n`);
      return true;
    } catch (err) {
      this.isConnected = false;
      return false;
    }
  }

  writeDone(): boolean {
    if (!this.isConnected) return false;
    
    try {
      this.response.write('data: [DONE]\n\n');
      return true;
    } catch (err) {
      this.isConnected = false;
      return false;
    }
  }

  end(): void {
    if (this.isConnected) {
      this.response.end();
      this.isConnected = false;
    }
  }

  get connected(): boolean {
    return this.isConnected;
  }
}

// 心跳保持连接
export class SSEHeartbeat {
  private timer?: NodeJS.Timeout;
  private writer: SSEWriter;

  constructor(writer: SSEWriter, intervalMs: number = 30000) {
    this.writer = writer;
    this.start(intervalMs);
  }

  private start(intervalMs: number): void {
    this.timer = setInterval(() => {
      if (this.writer.connected) {
        this.writer.writeComment('heartbeat');
      } else {
        this.stop();
      }
    }, intervalMs);
  }

  stop(): void {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = undefined;
    }
  }
}
```

### Chat Completions 端点

```typescript
// src/routes/chat.ts
import { Router, Request, Response } from 'express';
import { ProxyConfig } from '../lib/config.js';
import { SSEWriter, SSEHeartbeat } from '../lib/sse.js';
import { Logger } from 'pino';

const router = Router();

interface ChatCompletionsRequest {
  model: string;
  messages: any[];
  stream?: boolean;
  tools?: any[];
  [key: string]: any;
}

router.post('/completions', async (req: Request, res: Response) => {
  const config = req.app.get('config') as ProxyConfig;
  const logger = req.app.get('logger') as Logger;
  
  try {
    const requestBody = req.body as ChatCompletionsRequest;
    
    // 强制启用流式
    requestBody.stream = true;
    
    // 构建上游请求
    const upstreamUrl = new URL('/chat/completions', config.upstream.baseUrl);
    
    // Azure 特殊处理
    if (isAzureEndpoint(config.upstream.baseUrl)) {
      upstreamUrl.searchParams.set('api-version', '2025-04-01-preview');
    }

    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      'Accept': 'text/event-stream',
    };

    // 鉴权头
    if (config.upstream.apiKey) {
      headers['Authorization'] = `Bearer ${config.upstream.apiKey}`;
    }

    // 发起上游请求
    const upstreamResponse = await fetch(upstreamUrl, {
      method: 'POST',
      headers,
      body: JSON.stringify(requestBody),
      signal: AbortSignal.timeout(config.upstream.timeout)
    });

    // 错误处理
    if (!upstreamResponse.ok) {
      await handleUpstreamError(upstreamResponse, res, logger);
      return;
    }

    // 流式转发
    await streamChatResponse(upstreamResponse, res, logger);
    
  } catch (error) {
    logger.error({ error: error.message }, 'Chat endpoint error');
    res.status(500).json({
      error: {
        message: 'Internal server error',
        type: 'server_error'
      }
    });
  }
});

async function streamChatResponse(
  upstreamResponse: globalThis.Response,
  res: Response,
  logger: Logger
): Promise<void> {
  const writer = new SSEWriter(res);
  const heartbeat = new SSEHeartbeat(writer);
  
  try {
    const reader = upstreamResponse.body!.getReader();
    const decoder = new TextDecoder();
    let buffer = '';

    while (writer.connected) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      
      // 处理完整的 SSE 块
      let newlineIndex;
      while ((newlineIndex = buffer.indexOf('\n\n')) !== -1) {
        const chunk = buffer.slice(0, newlineIndex).trim();
        buffer = buffer.slice(newlineIndex + 2);
        
        if (chunk.startsWith('data: ')) {
          const data = chunk.slice(6);
          
          if (data === '[DONE]') {
            writer.writeDone();
            break;
          }
          
          try {
            const parsed = JSON.parse(data);
            writer.writeData(parsed);
          } catch (err) {
            logger.warn({ data }, 'Failed to parse SSE data');
          }
        }
      }
    }
  } catch (error) {
    logger.error({ error: error.message }, 'Stream processing error');
  } finally {
    heartbeat.stop();
    writer.end();
  }
}

async function handleUpstreamError(
  upstreamResponse: globalThis.Response,
  res: Response,
  logger: Logger
): Promise<void> {
  const statusCode = upstreamResponse.status;
  const statusText = upstreamResponse.statusText;
  
  // 透传重要头
  const retryAfter = upstreamResponse.headers.get('retry-after');
  if (retryAfter) {
    res.set('Retry-After', retryAfter);
  }

  try {
    const errorBody = await upstreamResponse.text();
    logger.warn(
      { statusCode, statusText, errorBody },
      'Upstream error response'
    );
    
    res.status(statusCode).json(
      errorBody ? JSON.parse(errorBody) : {
        error: {
          message: statusText,
          type: 'upstream_error'
        }
      }
    );
  } catch (err) {
    res.status(statusCode).json({
      error: {
        message: statusText,
        type: 'upstream_error'
      }
    });
  }
}

function isAzureEndpoint(baseUrl: string): boolean {
  return baseUrl.includes('.openai.azure.com');
}

export default router;
```

### Responses 端点实现

```typescript
// src/routes/responses.ts
import { Router, Request, Response } from 'express';
import { SSEWriter, SSEHeartbeat } from '../lib/sse.js';
import { chatToResponsesTransform } from '../transform/chat-to-responses.js';

const router = Router();

router.post('/', async (req: Request, res: Response) => {
  const config = req.app.get('config');
  const logger = req.app.get('logger');

  try {
    if (config.upstream.supportsResponses) {
      // 直通模式：上游原生支持 Responses API
      await handleDirectResponsesMode(req, res, config, logger);
    } else {
      // 桥接模式：Chat → Responses 转换
      await handleBridgeMode(req, res, config, logger);
    }
  } catch (error) {
    logger.error({ error: error.message }, 'Responses endpoint error');
    res.status(500).json({
      error: {
        message: 'Internal server error',
        type: 'server_error'
      }
    });
  }
});

// 直通模式：上游支持 Responses
async function handleDirectResponsesMode(
  req: Request,
  res: Response,
  config: any,
  logger: any
): Promise<void> {
  const upstreamUrl = new URL('/responses', config.upstream.baseUrl);
  
  const headers = {
    'Content-Type': 'application/json',
    'Accept': 'text/event-stream',
    'OpenAI-Beta': 'responses=experimental'
  };

  if (config.upstream.apiKey) {
    headers['Authorization'] = `Bearer ${config.upstream.apiKey}`;
  }

  const upstreamResponse = await fetch(upstreamUrl, {
    method: 'POST',
    headers,
    body: JSON.stringify(req.body),
    signal: AbortSignal.timeout(config.upstream.timeout)
  });

  if (!upstreamResponse.ok) {
    await handleUpstreamError(upstreamResponse, res, logger);
    return;
  }

  // 直接转发 SSE 流
  const writer = new SSEWriter(res);
  const heartbeat = new SSEHeartbeat(writer);

  try {
    const reader = upstreamResponse.body!.getReader();
    
    while (writer.connected) {
      const { done, value } = await reader.read();
      if (done) break;
      
      res.write(Buffer.from(value));
    }
  } finally {
    heartbeat.stop();
    writer.end();
  }
}

// 桥接模式：Chat → Responses 转换
async function handleBridgeMode(
  req: Request,
  res: Response,
  config: any,
  logger: any
): Promise<void> {
  // 1. 将 Responses 请求转换为 Chat 请求
  const chatRequest = responsesToChatRequest(req.body);
  
  // 2. 调用上游 Chat API
  const upstreamUrl = new URL('/chat/completions', config.upstream.baseUrl);
  
  const headers = {
    'Content-Type': 'application/json',
    'Accept': 'text/event-stream'
  };

  if (config.upstream.apiKey) {
    headers['Authorization'] = `Bearer ${config.upstream.apiKey}`;
  }

  const upstreamResponse = await fetch(upstreamUrl, {
    method: 'POST',
    headers,
    body: JSON.stringify(chatRequest),
    signal: AbortSignal.timeout(config.upstream.timeout)
  });

  if (!upstreamResponse.ok) {
    await handleUpstreamError(upstreamResponse, res, logger);
    return;
  }

  // 3. 将 Chat 流转换为 Responses 事件
  const writer = new SSEWriter(res);
  const heartbeat = new SSEHeartbeat(writer);

  try {
    await chatToResponsesTransform(upstreamResponse, writer, logger);
  } finally {
    heartbeat.stop();
    writer.end();
  }
}

// 简化版 Responses → Chat 转换
function responsesToChatRequest(responsesBody: any): any {
  const { model, instructions, input, tools } = responsesBody;

  // 构建 messages 数组
  const messages: any[] = [];
  
  // 系统指令
  if (instructions) {
    messages.push({ role: 'system', content: instructions });
  }

  // 处理输入历史（简化版，仅处理文本消息）
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
      }
      // TODO: 处理工具调用输出等复杂情况
    }
  }

  return {
    model,
    messages,
    stream: true,
    tools: convertTooChatTools(tools)  // 转换工具格式
  };
}

function extractTextContent(content: any[]): string {
  if (!Array.isArray(content)) return '';
  
  return content
    .filter(c => c?.type === 'input_text' || c?.type === 'output_text')
    .map(c => c.text || '')
    .join('');
}

function convertTooChatTools(responsesTools: any[]): any[] {
  if (!Array.isArray(responsesTools)) return [];
  
  // 只保留 function 类型工具
  return responsesTools
    .filter(tool => tool?.type === 'function')
    .map(tool => ({
      type: 'function',
      function: {
        name: tool.name,
        description: tool.description,
        parameters: tool.parameters
      }
    }));
}

export default router;
```

### Chat → Responses 事件转换

```typescript
// src/transform/chat-to-responses.ts
import { SSEWriter } from '../lib/sse.js';

interface FunctionCallState {
  id?: string;
  name?: string;
  arguments: string;
}

export async function chatToResponsesTransform(
  upstreamResponse: globalThis.Response,
  writer: SSEWriter,
  logger: any
): Promise<void> {
  const reader = upstreamResponse.body!.getReader();
  const decoder = new TextDecoder();
  
  let buffer = '';
  let responseId = '';
  let assistantText = '';
  
  // 工具调用状态（支持并发调用）
  const functionCalls = new Map<number, FunctionCallState>();

  try {
    while (writer.connected) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });

      let newlineIndex;
      while ((newlineIndex = buffer.indexOf('\n\n')) !== -1) {
        const chunk = buffer.slice(0, newlineIndex).trim();
        buffer = buffer.slice(newlineIndex + 2);

        if (!chunk.startsWith('data: ')) continue;
        
        const data = chunk.slice(6).trim();
        
        if (data === '[DONE]') {
          // 兜底完成事件
          writer.writeData({
            type: 'response.completed',
            id: responseId || generateResponseId()
          });
          return;
        }

        try {
          const json = JSON.parse(data);
          responseId = json?.id || responseId;
          
          const choice = json?.choices?.[0];
          if (!choice) continue;

          const delta = choice.delta;
          const finishReason = choice.finish_reason;

          // 处理文本增量
          if (delta?.content) {
            assistantText += delta.content;
            writer.writeData({
              type: 'response.output_text.delta',
              delta: delta.content
            });
          }

          // 处理工具调用增量
          if (delta?.tool_calls) {
            procesToolCallselta(delta.tool_calls, functionCalls);
          }

          // 处理完成语义
          if (finishReason === 'tool_calls') {
            // 输出所有聚合的工具调用
            for (const [, state] of functionCalls) {
              writer.writeData({
                type: 'response.output_item.done',
                item: {
                  type: 'function_call',
                  name: state.name || '',
                  arguments: state.arguments,
                  call_id: state.id || generateCallId()
                }
              });
            }

            // 完成事件
            writer.writeData({
              type: 'response.completed',
              id: responseId
            });
            return;
          }

          if (finishReason === 'stop') {
            // 输出最终的 assistant 消息
            if (assistantText) {
              writer.writeData({
                type: 'response.output_item.done',
                item: {
                  type: 'message',
                  role: 'assistant',
                  content: [
                    {
                      type: 'output_text',
                      text: assistantText
                    }
                  ]
                }
              });
            }

            // 完成事件
            writer.writeData({
              type: 'response.completed',
              id: responseId
            });
            return;
          }
        } catch (err) {
          logger.warn({ data, error: err.message }, 'Failed to parse Chat SSE data');
        }
      }
    }
  } catch (error) {
    logger.error({ error: error.message }, 'Chat to Responses transform error');
    
    // 发送错误事件
    writer.writeData({
      type: 'response.failed',
      error: {
        message: 'Transform failed',
        type: 'transform_error'
      }
    });
  }
}

function procesToolCallselta(
  toolCalls: any[],
  functionCalls: Map<number, FunctionCallState>
): void {
  for (const toolCall of toolCalls) {
    const index = typeof toolCall.index === 'number' ? toolCall.index : 0;
    const state = functionCalls.get(index) || { arguments: '' };

    // 更新状态
    if (toolCall.id) {
      state.id = toolCall.id;
    }
    
    if (toolCall.function?.name) {
      state.name = toolCall.function.name;
    }
    
    if (toolCall.function?.arguments) {
      state.arguments += toolCall.function.arguments; // 拼接分片
    }

    functionCalls.set(index, state);
  }
}

function generateResponseId(): string {
  return `resp_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
}

function generateCallId(): string {
  return `tc_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
}
```

## 高级特性

### 错误处理与重试

```typescript
// src/lib/retry.ts
export interface RetryOptions {
  maxAttempts: number;
  baseDelayMs: number;
  maxDelayMs: number;
  backoffMultiplier: number;
}

export class RetryHandler {
  constructor(private options: RetryOptions) {}

  async execute<T>(
    operation: () => Promise<T>,
    shouldRetry: (error: any) => boolean = this.defaultShouldRetry
  ): Promise<T> {
    let lastError: any;

    for (let attempt = 1; attempt <= this.options.maxAttempts; attempt++) {
      try {
        return await operation();
      } catch (error) {
        lastError = error;

        if (attempt === this.options.maxAttempts || !shouldRetry(error)) {
          throw error;
        }

        const delay = this.calculateDelay(attempt);
        await this.delay(delay);
      }
    }

    throw lastError;
  }

  private calculateDelay(attempt: number): number {
    const exponentialDelay = this.options.baseDelayMs * 
      Math.pow(this.options.backoffMultiplier, attempt - 1);
    
    return Math.min(exponentialDelay, this.options.maxDelayMs);
  }

  private defaultShouldRetry(error: any): boolean {
    // 重试 5xx 错误和网络错误
    if (error.status >= 500) return true;
    if (error.code === 'ECONNRESET' || error.code === 'ETIMEDOUT') return true;
    
    // 429 限流也重试
    if (error.status === 429) return true;
    
    return false;
  }

  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

// 使用示例
const retryHandler = new RetryHandler({
  maxAttempts: 3,
  baseDelayMs: 1000,
  maxDelayMs: 10000,
  backoffMultiplier: 2
});

await retryHandler.execute(async () => {
  return fetch(upstreamUrl, requestOptions);
});
```

### 监控与日志

```typescript
// src/lib/metrics.ts
export class MetricsCollector {
  private counters = new Map<string, number>();
  private histograms = new Map<string, number[]>();

  incrementCounter(name: string, value: number = 1): void {
    const current = this.counters.get(name) || 0;
    this.counters.set(name, current + value);
  }

  recordDuration(name: string, duration: number): void {
    const values = this.histograms.get(name) || [];
    values.push(duration);
    this.histograms.set(name, values);
  }

  getSnapshot(): MetricsSnapshot {
    const snapshot: MetricsSnapshot = {
      counters: Object.fromEntries(this.counters),
      histograms: {}
    };

    for (const [name, values] of this.histograms) {
      snapshot.histograms[name] = {
        count: values.length,
        sum: values.reduce((a, b) => a + b, 0),
        min: Math.min(...values),
        max: Math.max(...values),
        avg: values.reduce((a, b) => a + b, 0) / values.length,
        p95: this.percentile(values, 0.95),
        p99: this.percentile(values, 0.99)
      };
    }

    return snapshot;
  }

  private percentile(values: number[], p: number): number {
    const sorted = [...values].sort((a, b) => a - b);
    const index = Math.ceil(sorted.length * p) - 1;
    return sorted[index];
  }
}

// 中间件集成
export function metricsMiddleware(metrics: MetricsCollector) {
  return (req: Request, res: Response, next: Function) => {
    const startTime = Date.now();

    res.on('finish', () => {
      const duration = Date.now() - startTime;
      
      metrics.incrementCounter('http_requests_total');
      metrics.incrementCounter(`http_requests_${res.statusCode}`);
      metrics.recordDuration('http_request_duration_ms', duration);
    });

    next();
  };
}
```

### 健康检查

```typescript
// src/routes/health.ts
import { Router } from 'express';

const router = Router();

router.get('/', async (req, res) => {
  const config = req.app.get('config');
  
  try {
    // 检查上游连接
    const upstreamHealth = await checkUpstreamHealth(config.upstream);
    
    // 检查系统资源
    const systemHealth = checkSystemHealth();
    
    const health = {
      status: 'healthy',
      timestamp: new Date().toISOString(),
      version: process.env.npm_package_version || 'unknown',
      uptime: process.uptime(),
      upstream: upstreamHealth,
      system: systemHealth
    };

    res.json(health);
  } catch (error) {
    res.status(503).json({
      status: 'unhealthy',
      timestamp: new Date().toISOString(),
      error: error.message
    });
  }
});

router.get('/ready', async (req, res) => {
  // 就绪探针：检查服务是否准备好接受请求
  try {
    const config = req.app.get('config');
    await checkUpstreamHealth(config.upstream);
    res.status(200).json({ status: 'ready' });
  } catch (error) {
    res.status(503).json({ status: 'not ready', error: error.message });
  }
});

router.get('/live', (req, res) => {
  // 存活探针：检查服务是否还活着
  res.status(200).json({ status: 'alive' });
});

async function checkUpstreamHealth(upstreamConfig: any): Promise<any> {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), 5000);

  try {
    const response = await fetch(
      new URL('/models', upstreamConfig.baseUrl),
      {
        method: 'GET',
        headers: upstreamConfig.apiKey ? {
          'Authorization': `Bearer ${upstreamConfig.apiKey}`
        } : {},
        signal: controller.signal
      }
    );

    return {
      status: response.ok ? 'healthy' : 'degraded',
      statusCode: response.status,
      responseTime: Date.now() - Date.now() // 简化版
    };
  } finally {
    clearTimeout(timeoutId);
  }
}

function checkSystemHealth() {
  const memUsage = process.memoryUsage();
  
  return {
    memory: {
      used: Math.round(memUsage.heapUsed / 1024 / 1024),
      total: Math.round(memUsage.heapTotal / 1024 / 1024),
      external: Math.round(memUsage.external / 1024 / 1024)
    },
    cpu: process.cpuUsage(),
    eventLoop: {
      // 简化的事件循环延迟检测
      lag: 0 // 实际实现需要更复杂的检测
    }
  };
}

export default router;
```

## 部署与运维

### Docker 化部署

```dockerfile
# Dockerfile
FROM node:20-alpine AS builder

WORKDIR /app

# 安装依赖
COPY package*.json ./
RUN npm ci --only=production

# 构建应用
COPY . .
RUN npm run build

# 生产镜像
FROM node:20-alpine AS runtime

# 非 root 用户
RUN addgroup -g 1001 -S nodejs && \
    adduser -S proxy -u 1001

WORKDIR /app

# 复制构建产物
COPY --from=builder --chown=proxy:nodejs /app/node_modules ./node_modules
COPY --from=builder --chown=proxy:nodejs /app/dist ./dist
COPY --from=builder --chown=proxy:nodejs /app/package.json ./

USER proxy

EXPOSE 3000

# 健康检查
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD node -e "fetch('http://localhost:3000/health').then(r=>r.ok?process.exit(0):process.exit(1))"

CMD ["node", "dist/server.js"]
```

### Docker Compose 开发环境

```yaml
# docker-compose.yml
version: '3.8'

services:
  codex-proxy:
    build: .
    ports:
      - "3000:3000"
    environment:
      - NODE_ENV=development
      - LOG_LEVEL=debug
      - UPSTREAM_BASE_URL=http://mock-llm:8000/v1
      - UPSTREAM_SUPPORTS_RESPONSES=false
    depends_on:
      - mock-llm
    volumes:
      - ./config:/app/config:ro

  mock-llm:
    image: mock-server:latest
    ports:
      - "8000:8000"
    volumes:
      - ./test/fixtures:/app/fixtures:ro

  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"

  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9090:9090"
    volumes:
      - ./monitoring/prometheus.yml:/etc/prometheus/prometheus.yml:ro
```

### Kubernetes 部署

```yaml
# k8s/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: codex-proxy
spec:
  replicas: 3
  selector:
    matchLabels:
      app: codex-proxy
  template:
    metadata:
      labels:
        app: codex-proxy
    spec:
      containers:
      - name: proxy
        image: codex-proxy:latest
        ports:
        - containerPort: 3000
        env:
        - name: NODE_ENV
          value: "production"
        - name: UPSTREAM_BASE_URL
          valueFrom:
            secretKeyRef:
              name: codex-config
              key: upstream-url
        - name: UPSTREAM_API_KEY
          valueFrom:
            secretKeyRef:
              name: codex-config
              key: api-key
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health/live
            port: 3000
          initialDelaySeconds: 10
          periodSeconds: 30
        readinessProbe:
          httpGet:
            path: /health/ready
            port: 3000
          initialDelaySeconds: 5
          periodSeconds: 10

---
apiVersion: v1
kind: Service
metadata:
  name: codex-proxy-service
spec:
  selector:
    app: codex-proxy
  ports:
  - protocol: TCP
    port: 80
    targetPort: 3000
  type: LoadBalancer
```

### 监控配置

```yaml
# monitoring/prometheus.yml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'codex-proxy'
    static_configs:
      - targets: ['codex-proxy:3000']
    metrics_path: '/metrics'
    scrape_interval: 5s

  - job_name: 'node-exporter'
    static_configs:
      - targets: ['node-exporter:9100']
```

## 性能优化

### 连接池优化

```typescript
// src/lib/http-client.ts
import { Agent } from 'undici';

export class OptimizedHttpClient {
  private agent: Agent;

  constructor() {
    this.agent = new Agent({
      keepAliveTimeout: 30000,
      keepAliveMaxTimeout: 600000,
      maxRedirections: 3,
      connect: {
        timeout: 10000,
        keepAlive: true
      }
    });
  }

  async fetch(url: string | URL, options: any = {}): Promise<Response> {
    return fetch(url, {
      ...options,
      dispatcher: this.agent
    });
  }

  destroy(): void {
    this.agent.close();
  }
}
```

### 缓存策略

```typescript
// src/lib/cache.ts
export interface CacheEntry<T> {
  value: T;
  expiresAt: number;
}

export class MemoryCache<T> {
  private cache = new Map<string, CacheEntry<T>>();
  private maxSize: number;

  constructor(maxSize: number = 1000) {
    this.maxSize = maxSize;
  }

  set(key: string, value: T, ttlMs: number): void {
    // LRU 清理
    if (this.cache.size >= this.maxSize) {
      const firstKey = this.cache.keys().next().value;
      this.cache.delete(firstKey);
    }

    this.cache.set(key, {
      value,
      expiresAt: Date.now() + ttlMs
    });
  }

  get(key: string): T | null {
    const entry = this.cache.get(key);
    if (!entry) return null;

    if (entry.expiresAt < Date.now()) {
      this.cache.delete(key);
      return null;
    }

    return entry.value;
  }

  clear(): void {
    this.cache.clear();
  }
}

// 模型列表缓存示例
const modelCache = new MemoryCache<any[]>();

export async function getCachedModels(upstreamUrl: string): Promise<any[]> {
  const cacheKey = `models:${upstreamUrl}`;
  
  let models = modelCache.get(cacheKey);
  if (models) return models;

  // 从上游获取
  const response = await fetch(new URL('/models', upstreamUrl));
  models = await response.json();
  
  // 缓存 5 分钟
  modelCache.set(cacheKey, models, 5 * 60 * 1000);
  
  return models;
}
```

---

## 下一步
- **[测试验证](./06-testing-validation.md)**：验证代理实现的正确性
- **[工具集成](./04-tools-integration.md)**：为代理添加工具支持
- **[配置指南](./03-configuration-guide.md)**：优化 Codex 配置

这份实现指南提供了构建生产级 Node.js 代理的完整方案，涵盖了从基础功能到高级特性的各个方面。通过这个实现，你可以为 Codex 构建可靠、高性能的 LLM 代理服务。