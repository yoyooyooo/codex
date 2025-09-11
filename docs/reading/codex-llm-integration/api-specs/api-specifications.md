# API 规范与协议定义

本文定义 Codex 与上游 LLM（或自建代理）之间的请求/响应契约，覆盖两类 Wire API（Responses、Chat Completions）、SSE 事件语义、鉴权与错误处理，并给出最小自建代理实现建议。

> 说明：文中"自建代理"仅指你实现的 HTTP 兼容层，用于对接任意 LLM 服务端或多厂商聚合；不代表特定项目名或产品。

## 支持的 Wire API

Codex 通过 `model_provider_info.wire_api` 选择具体协议：

### Responses API（OpenAI `/v1/responses`）
- **特点**: 事件语义丰富（reasoning、output_item、web_search、custom 等）
- **请求体**: 字段与 Chat 不同，包含 `instructions`/`input`/`tools` 等
- **适用场景**: 需要细粒度控制和丰富事件类型的场景

### Chat Completions API（OpenAI `/v1/chat/completions`）  
- **特点**: 生态兼容性最好；工具仅支持 function 形式；消息为 `messages[]`
- **请求体**: 标准 OpenAI Chat 格式
- **适用场景**: 大部分 LLM 服务的标准接口

两者最终都会被标准化成统一事件流 `ResponseEvent`（见[事件映射文档](./event-mapping.md)）。

## 请求体规范

### Responses API 请求格式

**端点**: `POST {base_url}/responses`

**必备字段**：
```json
{
  "model": "gpt-5",                    // 模型名称
  "instructions": "system prompt",     // 系统指令
  "input": [],                         // ResponseItem[] 对话历史
  "tools": [],                         // OpenAI 工具 JSON
  "tool_choice": "auto",               // 工具选择策略
  "parallel_tool_calls": false,       // 是否允许并行工具调用
  "reasoning": {},                     // 可选，推理配置
  "store": false,                      // 是否存储
  "stream": true,                      // 要求流式响应
  "include": ["reasoning.encrypted_content"], // 可选包含内容
  "prompt_cache_key": "session_id",    // 会话缓存键
  "text": {                           // 仅 GPT-5 家族支持
    "verbosity": "medium"             // low/medium/high
  }
}
```

**常见 HTTP 头**：
```http
Authorization: Bearer <token>
OpenAI-Beta: responses=experimental
Content-Type: application/json
conversation_id: <session_id>
```

### Chat Completions 请求格式

**端点**: `POST {base_url}/chat/completions`

**必备字段**：
```json
{
  "model": "gpt-4o",                  // 使用 ModelFamily.slug
  "messages": [                       // 标准 OpenAI chat 结构
    {"role": "system", "content": "..."},
    {"role": "user", "content": "..."},
    {"role": "assistant", "content": "..."}
  ],
  "stream": true,                     // 流式响应
  "tools": [                          // 仅 function 工具
    {
      "type": "function",
      "function": {
        "name": "tool_name",
        "description": "...",
        "parameters": {...}           // JSON Schema
      }
    }
  ]
}
```

## 流式响应规范（SSE）

### Responses API 事件格式

服务端每条以 `data: <json>\n\n` 推送：

```text
data: {"type":"response.output_text.delta","delta":"Hello"}
data: {"type":"response.output_text.delta","delta":" world"}
data: {"type":"response.output_item.done","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Hello world"}]}}
data: {"type":"response.completed","id":"resp_123","usage":{"input_tokens":10,"output_tokens":5}}
```

**核心事件类型**：
- `response.output_text.delta`: 文本增量
- `response.reasoning_text.delta`: 推理内容增量（可选）
- `response.reasoning_summary_text.delta`: 推理摘要增量（可选）
- `response.output_item.done`: 完整输出项（message/function_call 等）
- `response.created`: 响应创建（可选）
- `response.completed`: **必须**，回合结束信号，可带 `usage`
- `response.failed`: 错误事件，包含 `error` 信息

### Chat Completions 事件格式

```text
data: {"id":"chat_123","choices":[{"delta":{"content":"Hello"}}]}
data: {"id":"chat_123","choices":[{"delta":{"content":" world"}}]}
data: {"id":"chat_123","choices":[{"delta":{"tool_calls":[{"id":"call_1","function":{"name":"fn","arguments":"{\"key\":"}}]}}]}
data: {"id":"chat_123","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"value\"}"}}]}}]}
data: {"id":"chat_123","choices":[{"finish_reason":"tool_calls"}]}
data: [DONE]
```

**语义说明**：
- `delta.content`: 助手文本增量
- `delta.tool_calls`: 工具调用增量（支持分片 arguments）
- `finish_reason=tool_calls`: 工具调用完成
- `finish_reason=stop`: 普通对话完成
- `[DONE]`: 流结束标记

## 鉴权与 HTTP 头

### 通用鉴权
- `Authorization: Bearer <token>`: 来自 `env_key` 环境变量或 ChatGPT 登录态
- 可选组织/项目头: `OpenAI-Organization`、`OpenAI-Project`（通过 `env_http_headers` 注入）

### Responses API 特有头
- `OpenAI-Beta: responses=experimental`: 启用实验性 Responses API
- `conversation_id`/`session_id`: 会话标识

### 自定义头支持
- 通过 Provider 配置的 `http_headers`、`env_http_headers` 可注入额外头部

## 错误处理与重试

### HTTP 错误
- **4xx/5xx**: 建议返回结构化 JSON 错误体（至少包含 `message`/`type`）
- **429**: 应附带 `Retry-After` 头，Codex 会遵循该头进行退避
- **5xx**: Codex 会按 Provider 配置的重试策略进行重试

### 流式错误
- **Responses API**: 可发送 `response.failed` 事件，包含 `error` 字段
- **连接异常**: 流空闲超时、意外断开等，Codex 会根据配置重连或报错

### 重试机制
- **请求级重试**: `request_max_retries` 控制
- **流重连**: `stream_max_retries` 控制  
- **退避策略**: 遵循 `Retry-After` 或使用指数退避

## 自建代理最小实现指南

### Chat 兼容代理（推荐优先实现）

**优势**: 生态兼容性最好，大部分 LLM 服务都支持

**实现要点**:
```javascript
// 路由: POST /chat/completions
app.post('/chat/completions', async (req, res) => {
  // 1. 强制启用流式
  const payload = { ...req.body, stream: true };
  
  // 2. 转发到上游
  const upstream = await fetch(UPSTREAM_URL, {
    method: 'POST',
    headers: { 'Authorization': `Bearer ${API_KEY}` },
    body: JSON.stringify(payload)
  });
  
  // 3. 设置 SSE 头
  res.setHeader('Content-Type', 'text/event-stream');
  res.setHeader('Cache-Control', 'no-cache');
  res.setHeader('Connection', 'keep-alive');
  
  // 4. 转发 SSE 流
  const reader = upstream.body.getReader();
  // ... 流处理逻辑
});
```

### Responses 兼容代理（高级功能）

**优势**: 支持丰富的事件类型和工具语义

**实现要点**:
```javascript
// 路由: POST /responses  
app.post('/responses', async (req, res) => {
  if (UPSTREAM_SUPPORTS_RESPONSES) {
    // 直通模式: 原样转发到上游 /responses
    return forwardToUpstream(req, res, '/responses');
  } else {
    // 桥接模式: 转换为 Chat 请求，再合成 Responses 事件
    const chatPayload = convertResponsesToChat(req.body);
    const chatStream = await fetchChatStream(chatPayload);
    await synthesizeResponsesEvents(chatStream, res);
  }
});
```

### 事件合成示例（Chat → Responses）

```javascript
// 将 Chat 流事件合成为 Responses 事件
async function synthesizeResponsesEvents(chatStream, res) {
  let assistantText = '';
  const toolCalls = new Map();
  
  for await (const event of chatStream) {
    const delta = event.choices?.[0]?.delta;
    
    if (delta?.content) {
      assistantText += delta.content;
      sseWrite(res, {
        type: 'response.output_text.delta',
        delta: delta.content
      });
    }
    
    if (delta?.tool_calls) {
      // 处理工具调用分片...
    }
    
    const finish = event.choices?.[0]?.finish_reason;
    if (finish === 'stop') {
      // 输出最终消息
      sseWrite(res, {
        type: 'response.output_item.done',
        item: {
          type: 'message',
          role: 'assistant', 
          content: [{ type: 'output_text', text: assistantText }]
        }
      });
      
      // 完成信号
      sseWrite(res, { type: 'response.completed', id: responseId });
      return;
    }
  }
}
```

## 兼容性建议

### 工具支持差异
- **Chat**: 仅支持 function 工具
- **Responses**: 支持 function/local_shell/web_search/custom/view_image 等
- **建议**: 桥接时将非 function 工具统一映射为 function 形式

### 消息格式转换
- **Responses → Chat**: 将 `instructions`+`input` 转换为 `messages` 数组
- **Chat → Responses**: 将增量事件合成为完整的 output_item

### 错误透传
- 保持 HTTP 状态码和重要头部（如 `Retry-After`）的透传
- 提供结构化的错误信息便于调试

## 相关文档

- [事件映射规范](./event-mapping.md) - SSE 事件到 ResponseEvent 的详细映射规则
- [架构概览](../architecture/architecture-overview.md) - 系统整体架构和设计理念
- [配置指南](../configuration/configuration-guide.md) - Provider 配置详细说明
- [Node 实现方案](../implementation/node-proxy-implementation.md) - 基于 Node.js 的完整实现示例