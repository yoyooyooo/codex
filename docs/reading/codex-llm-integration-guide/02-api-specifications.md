# API 规范详解

## 概览

Codex 支持两种 Wire API 协议，通过统一的事件抽象层实现协议无关的集成体验：

- **Responses API**：OpenAI 的实验性 API，事件语义丰富，支持推理与多种工具类型
- **Chat Completions API**：标准 OpenAI Chat API，生态兼容性最好，仅支持 function 工具

两种协议最终都会被映射为统一的 `ResponseEvent` 事件流，确保上层业务逻辑的一致性。

## Wire API 协议对比

| 特性 | Responses API | Chat Completions API |
|------|---------------|----------------------|
| **端点** | `POST /v1/responses` | `POST /v1/chat/completions` |
| **生态支持** | OpenAI 实验性 | 工业标准，广泛支持 |
| **事件粒度** | 细粒度（推理/工具/文本） | 粗粒度（内容/工具） |
| **工具类型** | function/shell/web_search/custom | 仅 function |
| **推理支持** | ✅ 原生支持 | ❌ 无推理语义 |
| **消息格式** | `instructions` + `input[]` | `messages[]` |

## Responses API 详细规范

### 请求体结构

```json
{
  "model": "gpt-5",                    // 模型名称
  "instructions": "...",               // 系统指令（内置 prompt.md + 覆盖）
  "input": [                          // 对话历史与上下文
    {
      "type": "message",
      "role": "user",
      "content": [{"type": "input_text", "text": "..."}]
    },
    {
      "type": "function_call_output", 
      "call_id": "tc_1",
      "output": "..."
    }
  ],
  "tools": [...],                     // OpenAI 工具 JSON 定义
  "tool_choice": "auto",              // 工具选择策略
  "parallel_tool_calls": false,       // 是否并行调用工具
  "reasoning": {...},                 // 推理配置（o3/gpt-5 等）
  "text": {"verbosity": "medium"},     // 输出详细程度
  "store": false,                     // 是否存储对话
  "stream": true,                     // 流式输出（必须）
  "prompt_cache_key": "session_123",  // 会话 ID，用于缓存
  "include": ["reasoning.encrypted_content"]  // 可选包含内容
}
```

### 必需 HTTP 头

```http
Authorization: Bearer <token>
Content-Type: application/json
OpenAI-Beta: responses=experimental
Accept: text/event-stream
```

### 可选头（由 Provider 配置注入）

```http
OpenAI-Organization: org-xxx
OpenAI-Project: proj_xxx
conversation_id: <session_id>
X-Custom-Header: <value>  # 通过 http_headers 配置
```

### SSE 事件流

Responses API 的 SSE 事件具有丰富的语义，支持细粒度的状态跟踪：

#### 1. 创建事件
```json
{"type": "response.created", "id": "resp_123"}
```

#### 2. 文本增量
```json
{"type": "response.output_text.delta", "delta": "Hello"}
{"type": "response.output_text.delta", "delta": " world"}
```

#### 3. 推理增量（o3/gpt-5 等模型）
```json
{"type": "response.reasoning_summary_text.delta", "delta": "分析问题..."}
{"type": "response.reasoning_text.delta", "delta": "详细推理过程..."}
```

#### 4. 完整输出项
```json
{
  "type": "response.output_item.done",
  "item": {
    "type": "message",
    "role": "assistant", 
    "content": [{"type": "output_text", "text": "完整回答"}]
  }
}
```

#### 5. 工具调用完成
```json
{
  "type": "response.output_item.done",
  "item": {
    "type": "function_call",
    "name": "apply_patch", 
    "arguments": "{\"patch\": \"...\"}",
    "call_id": "tc_1"
  }
}
```

#### 6. 其他工具类型
```json
// Local Shell 调用
{
  "type": "response.output_item.done", 
  "item": {
    "type": "local_shell_call",
    "command": ["ls", "-la"],
    "call_id": "tc_2"
  }
}

// Web 搜索调用
{
  "type": "response.output_item.done",
  "item": {
    "type": "web_search_call", 
    "query": "Rust async programming",
    "call_id": "tc_3"
  }
}
```

#### 7. 完成事件
```json
{
  "type": "response.completed",
  "id": "resp_123",
  "usage": {
    "input_tokens": 150,
    "output_tokens": 89,
    "total_tokens": 239,
    "input_tokens_details": {"cached_tokens": 50},
    "output_tokens_details": {"reasoning_tokens": 20}
  }
}
```

#### 8. 错误事件
```json
{
  "type": "response.failed",
  "error": {
    "type": "rate_limit",
    "message": "Too many requests", 
    "retry-after": 2
  }
}
```

## Chat Completions API 详细规范

### 请求体结构

```json
{
  "model": "gpt-4o",                  // 使用 ModelFamily.slug
  "messages": [                      // 标准 OpenAI 消息格式
    {"role": "system", "content": "..."},
    {"role": "user", "content": "..."},
    {"role": "assistant", "content": "..."},
    {
      "role": "tool", 
      "tool_call_id": "tc_1",
      "content": "工具执行结果"
    }
  ],
  "tools": [                         // 仅 function 工具
    {
      "type": "function",
      "function": {
        "name": "apply_patch",
        "description": "...", 
        "parameters": {"type": "object", ...}
      }
    }
  ],
  "stream": true                     // 流式输出（必须）
}
```

### SSE 事件流

Chat API 的事件结构相对简单，通过增量 delta 实现流式输出：

#### 1. 文本增量
```json
{
  "id": "chatcmpl-123",
  "choices": [{
    "delta": {"content": "Hello"},
    "index": 0
  }]
}
```

#### 2. 工具调用增量
```json
// 工具开始
{
  "id": "chatcmpl-123", 
  "choices": [{
    "delta": {
      "tool_calls": [{
        "index": 0,
        "id": "tc_1",
        "function": {"name": "apply_patch", "arguments": ""}
      }]
    }
  }]
}

// 参数分片
{
  "id": "chatcmpl-123",
  "choices": [{
    "delta": {
      "tool_calls": [{
        "index": 0,
        "function": {"arguments": "{\"patch\":"}
      }]
    }
  }]
}

{
  "id": "chatcmpl-123", 
  "choices": [{
    "delta": {
      "tool_calls": [{
        "index": 0,
        "function": {"arguments": "\"content\"}"}
      }]
    }
  }]
}
```

#### 3. 完成信号
```json
// 工具调用完成
{
  "id": "chatcmpl-123",
  "choices": [{"finish_reason": "tool_calls"}]
}

// 文本回答完成  
{
  "id": "chatcmpl-123",
  "choices": [{"finish_reason": "stop"}]
}

// 流结束
{"data": "[DONE]"}
```

## 统一事件映射 (ResponseEvent)

Codex 通过 `ResponseEvent` 枚举统一两种协议的事件语义：

```rust
pub enum ResponseEvent {
    Created,                           // 会话创建
    OutputTextDelta(String),          // 文本增量
    ReasoningSummaryDelta(String),    // 推理摘要增量  
    ReasoningContentDelta(String),    // 推理内容增量
    OutputItemDone(ResponseItem),     // 完整输出项
    Completed {                       // 回合完成
        response_id: String,
        token_usage: Option<TokenUsage>
    },
    WebSearchCallBegin {              // Web 搜索开始
        call_id: String
    }
}
```

### Responses API → ResponseEvent 映射

| Responses 事件 | ResponseEvent | 说明 |
|----------------|---------------|------|
| `response.created` | `Created` | 直接映射 |
| `response.output_text.delta` | `OutputTextDelta` | 直接映射 |
| `response.reasoning_summary_text.delta` | `ReasoningSummaryDelta` | 直接映射 |
| `response.reasoning_text.delta` | `ReasoningContentDelta` | 直接映射 |
| `response.output_item.done` | `OutputItemDone` | 反序列化 ResponseItem |
| `response.completed` | `Completed` | 提取 usage 信息 |
| `response.output_item.added` (web_search) | `WebSearchCallBegin` | 检测工具类型 |

### Chat API → ResponseEvent 映射

| Chat 事件 | ResponseEvent | 处理逻辑 |
|-----------|---------------|----------|
| `delta.content` | `OutputTextDelta` | 直接映射 |
| `delta.tool_calls[]` | 累积状态 | 按 index 分桶聚合 |
| `finish_reason=tool_calls` | `OutputItemDone` | 输出聚合的工具调用 |
| `finish_reason=stop` | `OutputItemDone` + `Completed` | 输出 message + 完成信号 |
| `[DONE]` | `Completed` | 兜底完成信号 |

### Chat 工具调用聚合算法

```rust
// 工具调用状态管理
struct FunctionCallState {
    id: Option<String>,
    name: Option<String>, 
    arguments: String,  // 累积拼接分片
}

// 按 index 分桶聚合
let mut function_calls: HashMap<u32, FunctionCallState> = HashMap::new();

// 处理增量
for tool_call in delta.tool_calls {
    let index = tool_call.index.unwrap_or(0);
    let state = function_calls.entry(index).or_default();
    
    if let Some(id) = tool_call.id {
        state.id = Some(id);
    }
    if let Some(name) = tool_call.function.name {
        state.name = Some(name);
    }
    if let Some(args) = tool_call.function.arguments {
        state.arguments.push_str(&args);  // 拼接分片
    }
}

// finish_reason=tool_calls 时输出
for (_, state) in function_calls {
    emit(ResponseEvent::OutputItemDone(ResponseItem::FunctionCall {
        call_id: state.id.unwrap_or_default(),
        name: state.name.unwrap_or_default(), 
        arguments: state.arguments,
    }));
}
```

## 鉴权与 HTTP 配置

### 鉴权机制
1. **API Key 鉴权**：`Authorization: Bearer <token>`
   - 来源：`env_key` 环境变量或 ChatGPT 登录态
   - 自动注入：Codex 根据 Provider 配置自动处理

2. **组织/项目头**：
   ```http
   OpenAI-Organization: org-xxx
   OpenAI-Project: proj_xxx
   ```

### 查询参数
```http
# Azure 必须的 API 版本
POST /chat/completions?api-version=2025-04-01-preview

# 自定义参数（通过 query_params 配置）
POST /v1/responses?custom_param=value
```

### 自定义头注入
```toml
# 静态头
[model_providers.custom]
http_headers = { "X-Feature" = "enabled", "X-Version" = "v1" }

# 环境变量头  
env_http_headers = { "X-API-Key" = "CUSTOM_API_KEY_ENV" }
```

## 错误处理与重试

### HTTP 错误码处理

| 状态码 | 处理策略 | 说明 |
|--------|----------|------|
| **429** | 按 `Retry-After` 退避 | 限流，遵循服务端建议 |
| **401/403** | 立即失败，不重试 | 鉴权问题 |
| **5xx** | 指数退避重试 | 服务端暂时不可用 |
| **其他 4xx** | 立即失败 | 客户端请求错误 |

### 重试策略配置
```toml
[model_providers.example]
request_max_retries = 4        # 请求级重试上限
stream_max_retries = 5         # 流重连上限  
stream_idle_timeout_ms = 300000 # 流空闲超时（5分钟）
```

### 错误体格式
```json
{
  "error": {
    "type": "rate_limit",
    "message": "Too many requests",
    "code": "rate_limit_exceeded"
  }
}
```

### Responses API 流内错误
```json
{
  "type": "response.failed",
  "error": {
    "type": "server_error", 
    "message": "Internal server error",
    "retry-after": 5  // 建议退避时间（秒）
  }
}
```

## 可靠性保障

### 流式处理可靠性
1. **空闲超时**：防止连接僵死
2. **心跳机制**：保持连接活跃
3. **断线重连**：支持流级别重试
4. **完成检测**：确保收到 `completed` 或 `[DONE]`

### 网络层优化
1. **连接复用**：HTTP/2 连接池
2. **压缩支持**：gzip/brotli 压缩
3. **超时配置**：连接/读取/写入超时
4. **代理支持**：HTTP/SOCKS 代理

### 监控与调试
1. **请求追踪**：每个请求的唯一 ID
2. **性能指标**：延迟/吞吐/错误率
3. **调试日志**：请求/响应详情（脱敏）

---

## 下一步
- **[配置指南](./03-configuration-guide.md)**：学习如何配置不同的 Provider
- **[工具集成](./04-tools-integration.md)**：深入了解工具调用机制
- **[实现指南](./05-implementation-guide.md)**：构建自定义代理实现

这套 API 规范为 Codex 提供了强大而灵活的 LLM 集成能力，支持从标准 OpenAI API 到自建代理的各种集成场景。