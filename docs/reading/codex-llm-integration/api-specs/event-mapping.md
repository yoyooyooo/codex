# 流式事件映射规范

本文定义底层 SSE 事件如何被标准化为 Codex 的统一事件枚举 `ResponseEvent`，以及 Chat 与 Responses 的差异与聚合策略。

## 统一事件类型（ResponseEvent）

Codex 内部使用统一的事件枚举来处理不同来源的流式数据：

| 事件类型 | 描述 | 数据结构 |
|---------|------|----------|
| `Created` | 会话/响应创建 | 响应 ID |
| `OutputTextDelta(String)` | 纯文本增量 | 文本片段 |
| `ReasoningSummaryDelta(String)` | 推理摘要增量 | 推理摘要片段 |
| `ReasoningContentDelta(String)` | 推理正文增量 | 推理内容片段 |
| `OutputItemDone(ResponseItem)` | 完整输出项 | assistant message / function_call / reasoning 等 |
| `Completed { response_id, token_usage }` | 回合结束 | 响应 ID + 可选 token 用量统计 |
| `WebSearchCallBegin { call_id }` | Web 搜索开始 | 调用 ID（辅助 UI 显示） |

**定义位置**: `codex-rs/core/src/client_common.rs`

## Responses API 事件映射

### 映射规则

| Responses SSE 事件 | ResponseEvent | 说明 |
|-------------------|---------------|------|
| `response.created` | `Created` | 响应创建事件 |
| `response.output_text.delta` | `OutputTextDelta` | 助手文本增量 |
| `response.reasoning_summary_text.delta` | `ReasoningSummaryDelta` | 推理摘要增量 |
| `response.reasoning_text.delta` | `ReasoningContentDelta` | 推理内容增量 |
| `response.output_item.done` | `OutputItemDone` | 完整输出项（message/function_call 等） |
| `response.output_item.added`（web_search） | `WebSearchCallBegin` | 检测到 web_search 开始 |
| `response.completed` | `Completed` | 回合完成，包含 ID 和用量统计 |
| `response.failed` | 错误处理 | 组装错误并终止流，支持 retry-after 语义 |

### 示例映射过程

**输入 SSE 事件**:
```json
data: {"type":"response.output_text.delta","delta":"Hello"}
data: {"type":"response.output_text.delta","delta":" world"}
data: {"type":"response.output_item.done","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Hello world"}]}}
data: {"type":"response.completed","id":"resp_123","usage":{"input_tokens":10,"output_tokens":5}}
```

**映射后的 ResponseEvent 序列**:
1. `OutputTextDelta("Hello")`
2. `OutputTextDelta(" world")`  
3. `OutputItemDone(ResponseItem::Message { role: "assistant", content: "Hello world" })`
4. `Completed { response_id: "resp_123", token_usage: TokenUsage { input: 10, output: 5 } }`

### 错误处理映射

**输入错误事件**:
```json
data: {"type":"response.failed","error":{"type":"rate_limit","message":"Too many requests","retry-after":30}}
```

**处理逻辑**:
- 解析 `error` 字段构造错误信息
- 提取 `retry-after` 用于上层重试决策
- 终止事件流并返回错误

**实现位置**: `codex-rs/core/src/client.rs`（`process_sse` 函数）

## Chat Completions 事件映射

### 映射规则

Chat API 的映射更复杂，需要状态累积和完成时机判断：

| Chat SSE 事件 | ResponseEvent | 处理逻辑 |
|---------------|---------------|----------|
| `delta.content` | `OutputTextDelta` | 直接映射文本增量 |
| `delta.tool_calls` | 状态累积 | 按 index 分桶累积 id/name/arguments |
| `finish_reason=tool_calls` | `OutputItemDone(FunctionCall)` + `Completed` | 输出累积的工具调用 |
| `finish_reason=stop` | `OutputItemDone(Message)` + `Completed` | 输出累积的助手消息 |
| `[DONE]` | 结束处理 | 流结束标记 |

### 工具调用状态累积

Chat API 的工具调用以分片形式发送，需要累积处理：

```typescript
type FunctionState = {
  id?: string;
  name?: string; 
  args: string;  // 字符串分片累积
};

// 按 tool_calls[].index 分桶
const functionCalls = new Map<number, FunctionState>();
```

**分片示例**:
```json
// 第一片
{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"search","arguments":"{\"query\":"}}]}}

// 第二片  
{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"hello world\"}"}}]}}

// 完成
{"finish_reason":"tool_calls"}
```

**累积结果**:
```json
{
  "0": {
    "id": "call_1",
    "name": "search", 
    "args": "{\"query\":\"hello world\"}"
  }
}
```

### 完成时机处理

| finish_reason | 处理逻辑 |
|---------------|----------|
| `tool_calls` | 1. 为每个累积的工具调用生成 `OutputItemDone(FunctionCall)` <br> 2. 发送 `Completed` 事件 <br> 3. 清空累积状态 |
| `stop` | 1. 为累积的助手文本生成 `OutputItemDone(Message)` <br> 2. 发送 `Completed` 事件 |
| 其他 | 按错误或异常情况处理 |

**实现位置**: `codex-rs/core/src/chat_completions.rs`（`process_chat_sse` 函数）

## 聚合模式（Chat 专用）

为减少 UI 噪声，Chat 流支持可选的"聚合模式"：

### 标准模式 vs 聚合模式

| 模式 | 事件输出 | 适用场景 |
|------|----------|----------|
| 标准模式 | 实时输出所有增量事件 | 需要实时反馈的交互场景 |
| 聚合模式 | 仅在回合结束时输出完整结果 | 减少 UI 更新频率的场景 |

### 聚合策略

**抑制的事件**:
- `OutputTextDelta` - token 级别的文本增量

**保留的事件**:  
- `OutputItemDone` - 完整的输出项
- `Completed` - 回合完成事件
- 错误事件

**配置控制**:
```rust
// 根据配置选择模式
let use_aggregation = !config.show_raw_agent_reasoning;
if use_aggregation {
    stream.aggregate().process()
} else {
    stream.process()
}
```

## Token 用量映射

### Responses API 用量映射

**输入格式**:
```json
{
  "type": "response.completed",
  "usage": {
    "input_tokens": 100,
    "output_tokens": 50,
    "total_tokens": 150,
    "input_tokens_details": {
      "cached_tokens": 20
    },
    "output_tokens_details": {
      "reasoning_tokens": 30
    }
  }
}
```

**映射结果**:
```rust
TokenUsage {
    input_tokens: 100,
    output_tokens: 50,
    total_tokens: 150,
    cached_input_tokens: Some(20),
    reasoning_output_tokens: Some(30),
}
```

### Chat API 用量映射

Chat API 通常不在流式响应中提供用量统计，因此：
- `Completed` 事件的 `token_usage` 字段为 `None`
- Codex 可正常处理无用量统计的场景

## 可靠性保障

### 超时处理
- **流空闲超时**: 由 `stream_idle_timeout_ms` 配置控制
- **检测机制**: 在指定时间内未收到任何事件
- **处理策略**: 终止流并报告超时错误

### 异常终止处理
- **缺失 completed 事件**: 流结束但未收到完成信号
- **连接中断**: 网络异常或服务端断开连接  
- **处理策略**: 根据 Provider 配置决定是否重试

### 重试机制
- **请求级重试**: `request_max_retries` 控制完整请求的重试次数
- **流重连**: `stream_max_retries` 控制流断开后的重连次数
- **退避策略**: 遵循 `Retry-After` 头或使用指数退避

**实现参考**: 
- 集成测试 `stream_no_completed.rs`
- 错误处理逻辑在 `core/src/client.rs`

## 扩展性考虑

### 添加新事件类型
1. 在 `ResponseEvent` 枚举中添加新变体
2. 在相应的映射函数中添加处理逻辑  
3. 更新上层事件处理器

### 自定义映射逻辑
- 继承现有的映射接口
- 实现自定义的事件转换器
- 通过 Provider 配置选择映射策略

## 相关文档

- [API 规范](./api-specifications.md) - 详细的请求/响应格式定义
- [架构概览](../architecture/architecture-overview.md) - 事件处理在整体架构中的位置  
- [工具集成](../tools/tools-integration.md) - 工具调用相关的事件处理
- [实现方案](../implementation/node-proxy-implementation.md) - 实际的事件映射实现示例