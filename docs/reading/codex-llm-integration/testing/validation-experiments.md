# 验证实验与测试方案

本文定义了当前 Codex LLM 集成方案的关键不确定性，并为每个不确定性提供最小可行验证实验（MVP），便于快速验证与迭代。所有实验均可在本地通过 Node ≥ 18 + curl 复现，无需真实 OpenAI Key。

## 实验概览

我们识别出以下 3 个关键不确定性，并针对每个设计了验证实验：

| 编号 | 不确定性 | 核心风险 | 验证方法 |
|------|---------|----------|----------|
| E1 | Chat→Responses 合成语义 | 事件顺序或缺失导致 Codex 无法正常推进 | 文本闭环测试 |
| E2 | 工具调用分片聚合 | arguments 拼接错误或多并发工具污染 | 双工具并发分片测试 |  
| E3 | 错误透传与退避协同 | 缺失 Retry-After 导致退避失效 | 429 头部透传测试 |

## 实验 E1：文本响应闭环验证

### 目标假设

只要代理在 `/v1/responses` 合成输出 `response.output_text.delta` 与 `response.completed`（可选再补 `response.output_item.done(message)`），Codex 就能顺利推进一个回合。

### 风险点

- 若缺少 `response.completed` 或事件顺序异常，Codex 将判定流异常，无法结束回合
- 不正确的事件格式可能导致解析失败

### 实验设计

#### 1. 创建 Mock 上游（输出标准 Chat SSE）

创建文件 `mock/chat-min.ts`：

```typescript
import http from 'node:http';

const sse = (res: http.ServerResponse, line: any) => 
  res.write(`data: ${JSON.stringify(line)}\\n\\n`);

const server = http.createServer((req, res) => {
  if (req.url === '/v1/chat/completions' && req.method === 'POST') {
    console.log('Mock chat request received');
    
    // 设置 SSE 头部
    res.writeHead(200, {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
      'Connection': 'keep-alive'
    });
    
    // 模拟分片文本响应
    sse(res, { 
      id: 'chat_123', 
      choices: [{ delta: { content: 'Hello' } }] 
    });
    
    setTimeout(() => {
      sse(res, { 
        id: 'chat_123', 
        choices: [{ delta: { content: ' world' } }] 
      });
    }, 100);
    
    setTimeout(() => {
      sse(res, { 
        id: 'chat_123', 
        choices: [{ delta: { content: '! 你好世界！' } }] 
      });
    }, 200);
    
    setTimeout(() => {
      sse(res, { 
        id: 'chat_123', 
        choices: [{ finish_reason: 'stop' }] 
      });
      res.end('data: [DONE]\\n\\n');
    }, 300);
    
  } else {
    res.writeHead(404).end('Not Found');
  }
});

server.listen(3100, () => {
  console.log('Mock Chat API server listening on :3100');
  console.log('Test with: curl -N http://localhost:3100/v1/chat/completions -H "content-type: application/json" -d "{\\"model\\":\\"test\\",\\"messages\\":[],\\"stream\\":true}"');
});
```

#### 2. 启动测试环境

```bash
# 终端 1：启动 Mock 上游
cd mock
npx tsx chat-min.ts

# 终端 2：启动 Node 代理（使用桥接模式）
export UPSTREAM_BASE_URL=http://localhost:3100
export UPSTREAM_SUPPORTS_RESPONSES=false
cd proxy-node
npm run dev

# 终端 3：测试调用
curl -N http://localhost:3000/v1/responses \\
  -H 'Content-Type: application/json' \\
  -d '{
    "model": "test-model",
    "instructions": "You are a helpful assistant",
    "input": [{
      "type": "message",
      "role": "user", 
      "content": [{"type": "input_text", "text": "Say hello"}]
    }]
  }'
```

#### 3. 验证标准

**必须出现的事件序列**：
```text
data: {"type":"response.output_text.delta","delta":"Hello"}
data: {"type":"response.output_text.delta","delta":" world"}  
data: {"type":"response.output_text.delta","delta":"! 你好世界！"}
data: {"type":"response.output_item.done","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Hello world! 你好世界！"}]}}
data: {"type":"response.completed","id":"chat_123"}
```

**失败指标**：
- 缺少 `response.completed` 事件
- 事件顺序错误（completed 出现在 delta 之前）
- JSON 格式错误或字段缺失
- 连接超时或异常断开

#### 4. 扩展测试

测试边界情况：

```typescript
// 空响应测试
sse(res, { 
  id: 'chat_empty', 
  choices: [{ finish_reason: 'stop' }] 
});

// 超长响应测试  
const longText = 'A'.repeat(10000);
sse(res, { 
  id: 'chat_long',
  choices: [{ delta: { content: longText } }] 
});
```

## 实验 E2：工具调用分片聚合验证

### 目标假设

对 Chat 流的 `tool_calls` 分片，按 `index` 聚合 `id/name/arguments` 字段，在 `finish_reason=tool_calls` 时输出一个或多个 `response.output_item.done`（`type=function_call`）即可满足 Codex 对工具回合的期望。

### 风险点

- `arguments` 为字符串分片，拼接顺序错误将导致非合法 JSON
- 多并发工具（`index` 0/1/2...）未分桶聚合会相互污染
- 工具调用 ID 丢失或重复

### 实验设计

#### 1. 创建复杂工具调用 Mock

创建文件 `mock/chat-tools.ts`：

```typescript
import http from 'node:http';

const sse = (res: http.ServerResponse, line: any) => 
  res.write(`data: ${JSON.stringify(line)}\\n\\n`);

const server = http.createServer((req, res) => {
  if (req.url === '/v1/chat/completions' && req.method === 'POST') {
    console.log('Mock tools request received');
    
    res.writeHead(200, {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache', 
      'Connection': 'keep-alive'
    });
    
    // 工具 0：apply_patch 分片
    sse(res, { 
      id: 'tool_test', 
      choices: [{ 
        delta: { 
          tool_calls: [{ 
            index: 0, 
            id: 'call_apply_123', 
            function: { 
              name: 'apply_patch', 
              arguments: '{"files":[{"path":"test.js","content":"console.log(' 
            } 
          }] 
        } 
      }] 
    });
    
    // 工具 1：shell 调用开始
    setTimeout(() => {
      sse(res, { 
        id: 'tool_test', 
        choices: [{ 
          delta: { 
            tool_calls: [{ 
              index: 1, 
              id: 'call_shell_456', 
              function: { 
                name: 'local_shell',
                arguments: '{"command":"ls -la'
              } 
            }] 
          } 
        }] 
      });
    }, 50);
    
    // 工具 0：继续分片 
    setTimeout(() => {
      sse(res, { 
        id: 'tool_test', 
        choices: [{ 
          delta: { 
            tool_calls: [{ 
              index: 0, 
              function: { 
                arguments: '\\'Hello World\\')"}]}' 
              } 
            }] 
          } 
        }] 
      });
    }, 100);
    
    // 工具 1：完成参数
    setTimeout(() => {
      sse(res, { 
        id: 'tool_test', 
        choices: [{ 
          delta: { 
            tool_calls: [{ 
              index: 1, 
              function: { 
                arguments: ' /tmp"}'
              } 
            }] 
          } 
        }] 
      });
    }, 150);
    
    // 工具调用完成
    setTimeout(() => {
      sse(res, { 
        id: 'tool_test', 
        choices: [{ finish_reason: 'tool_calls' }] 
      });
      res.end('data: [DONE]\\n\\n');
    }, 200);
    
  } else {
    res.writeHead(404).end('Not Found');
  }
});

server.listen(3101, () => {
  console.log('Mock Tools API server listening on :3101');
});
```

#### 2. 运行并验证

```bash
# 启动工具 Mock
export UPSTREAM_BASE_URL=http://localhost:3101
export UPSTREAM_SUPPORTS_RESPONSES=false
npx tsx mock/chat-tools.ts

# 在另一终端启动代理
npm run dev

# 测试工具调用
curl -N http://localhost:3000/v1/responses \\
  -H 'Content-Type: application/json' \\
  -d '{
    "model": "test",
    "instructions": "You have access to file and shell tools",
    "input": [{
      "type": "message", 
      "role": "user",
      "content": [{"type": "input_text", "text": "修改文件并查看目录"}]
    }],
    "tools": [
      {"type": "function", "function": {"name": "apply_patch", "description": "Apply code patch"}},
      {"type": "local_shell", "name": "local_shell", "description": "Execute shell command"}
    ]
  }' | jq .
```

#### 3. 验证标准

**期望的输出序列**：
```json
{"type":"response.output_item.done","item":{"type":"function_call","name":"apply_patch","arguments":"{\\"files\\":[{\\"path\\":\\"test.js\\",\\"content\\":\\"console.log('Hello World')\\"}]}","call_id":"call_apply_123"}}

{"type":"response.output_item.done","item":{"type":"function_call","name":"local_shell","arguments":"{\\"command\\":\\"ls -la /tmp\\"}","call_id":"call_shell_456"}}

{"type":"response.completed","id":"tool_test"}
```

**验证检查点**：
- [ ] 两个工具调用都正确输出
- [ ] `arguments` 字段为合法 JSON
- [ ] `call_id` 正确对应
- [ ] 没有交叉污染（工具 0 的 arguments 不包含工具 1 的内容）
- [ ] JSON 解析成功：`echo '{"files":[{"path":"test.js","content":"console.log('\''Hello World'\'')"}]}' | jq .`

#### 4. 压力测试

创建包含 5 个并发工具的测试，验证高并发场景下的分片聚合：

```typescript
// 模拟 5 个工具并发，每个工具参数分 3-5 片
const tools = [
  { index: 0, name: 'read_file', args_parts: ['{"path":"', '/home/user/', 'config.json"}'] },
  { index: 1, name: 'write_file', args_parts: ['{"path":"/tmp/', 'output.txt","content":"', 'Hello World"}'] },
  { index: 2, name: 'shell', args_parts: ['{"command":"find /tmp -name ', '\\"*.log\\" -type f"}'] },
  { index: 3, name: 'web_search', args_parts: ['{"query":"Node.js ', 'best practices 2025"}'] },
  { index: 4, name: 'apply_patch', args_parts: ['{"files":[{"path":"src/', 'main.js","content":"// Updated code"}]}'] }
];
```

## 实验 E3：错误透传与退避协同验证

### 目标假设  

代理对 429/5xx 错误应原样透传状态码与 `Retry-After` 头；Codex 会读取并按该头退避重试。若未透传，该回退将退化为指数退避，影响体验。

### 风险点

- 当前最小实现只转发响应体文本，未传递上游头
- 导致 Codex 无法利用上游建议的退避时间
- 错误恢复机制失效

### 实验设计

#### 1. 创建限流错误 Mock

创建文件 `mock/chat-429.ts`：

```typescript
import http from 'node:http';

let requestCount = 0;

const server = http.createServer((req, res) => {
  requestCount++;
  console.log(`Request #${requestCount} received`);
  
  if (req.url === '/v1/chat/completions' && req.method === 'POST') {
    
    if (requestCount <= 2) {
      // 前两次请求返回 429 
      console.log('Returning 429 with Retry-After');
      res.statusCode = 429;
      res.setHeader('Content-Type', 'application/json');
      res.setHeader('Retry-After', '3'); // 3 秒后重试
      res.setHeader('X-RateLimit-Remaining', '0');
      res.setHeader('X-RateLimit-Reset', Math.floor(Date.now() / 1000) + 60);
      
      return res.end(JSON.stringify({
        error: {
          type: 'rate_limit_exceeded',
          message: 'Too many requests, please retry after 3 seconds',
          code: 'rate_limit_exceeded'
        }
      }));
      
    } else if (requestCount === 3) {
      // 第三次请求返回 500
      console.log('Returning 500 with Retry-After');
      res.statusCode = 500;
      res.setHeader('Content-Type', 'application/json');
      res.setHeader('Retry-After', '5'); // 5 秒后重试
      
      return res.end(JSON.stringify({
        error: {
          type: 'server_error', 
          message: 'Internal server error, retry after 5 seconds',
          code: 'internal_error'
        }
      }));
      
    } else {
      // 第四次及以后：成功响应
      console.log('Returning successful response');
      res.writeHead(200, {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        'Connection': 'keep-alive'
      });
      
      res.write(`data: ${JSON.stringify({ 
        id: 'success_123', 
        choices: [{ delta: { content: 'Request succeeded after retries!' } }] 
      })}\\n\\n`);
      
      setTimeout(() => {
        res.write(`data: ${JSON.stringify({ 
          id: 'success_123', 
          choices: [{ finish_reason: 'stop' }] 
        })}\\n\\n`);
        res.end('data: [DONE]\\n\\n');
      }, 100);
    }
    
  } else {
    res.writeHead(404).end('Not Found');
  }
});

server.listen(3102, () => {
  console.log('Mock Error API server listening on :3102');
  console.log('Will return 429, 500, then success on subsequent requests');
});
```

#### 2. 代理错误透传实现

确保代理正确透传错误头：

```typescript
// 在 chat.ts 和 responses.ts 的错误处理中添加
if (!upstream.ok) {
  // 透传重要的错误头
  const headersToForward = [
    'retry-after',
    'x-ratelimit-remaining', 
    'x-ratelimit-reset',
    'x-ratelimit-limit'
  ];
  
  headersToForward.forEach(header => {
    const value = upstream.headers.get(header);
    if (value) {
      res.setHeader(header, value);
    }
  });
  
  const errorText = await upstream.text();
  return res.status(upstream.status).json(JSON.parse(errorText));
}
```

#### 3. 验证测试

```bash
# 启动错误 Mock
export UPSTREAM_BASE_URL=http://localhost:3102
npx tsx mock/chat-429.ts

# 启动代理
npm run dev

# 测试错误透传（观察响应头）
curl -i http://localhost:3000/v1/chat/completions \\
  -H 'Content-Type: application/json' \\
  -d '{
    "model": "test",
    "messages": [{"role": "user", "content": "测试请求"}],
    "stream": true
  }'

# 应该看到类似输出：
# HTTP/1.1 429 Too Many Requests
# Retry-After: 3
# X-RateLimit-Remaining: 0
# Content-Type: application/json
```

#### 4. 自动化验证脚本

创建 `test/error-handling.ts`：

```typescript
async function testErrorTransparency() {
  const testCases = [
    { expectedStatus: 429, expectedHeaders: ['retry-after'] },
    { expectedStatus: 500, expectedHeaders: ['retry-after'] }, 
    { expectedStatus: 200, expectedHeaders: [] }
  ];
  
  for (let i = 0; i < testCases.length; i++) {
    const testCase = testCases[i];
    console.log(`Test ${i + 1}: Expecting status ${testCase.expectedStatus}`);
    
    const response = await fetch('http://localhost:3000/v1/chat/completions', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        model: 'test',
        messages: [{ role: 'user', content: 'test' }],
        stream: true
      })
    });
    
    console.log(`Got status: ${response.status}`);
    
    // 验证状态码
    if (response.status !== testCase.expectedStatus) {
      throw new Error(`Expected ${testCase.expectedStatus}, got ${response.status}`);
    }
    
    // 验证头部透传
    for (const header of testCase.expectedHeaders) {
      const value = response.headers.get(header);
      if (!value) {
        throw new Error(`Missing expected header: ${header}`);
      }
      console.log(`✓ Header ${header}: ${value}`);
    }
    
    if (response.ok) {
      // 成功响应，读取流内容
      const reader = response.body?.getReader();
      if (reader) {
        const { value } = await reader.read();
        const text = new TextDecoder().decode(value);
        console.log(`Response preview: ${text.substring(0, 100)}...`);
      }
    }
    
    console.log(`Test ${i + 1}: PASSED\\n`);
  }
  
  console.log('All error handling tests passed!');
}

testErrorTransparency().catch(console.error);
```

## 综合验证流程

### 完整测试套件

创建 `test/full-integration.sh`：

```bash
#!/bin/bash
set -e

echo "=== Codex LLM Integration Full Test Suite ==="

# 清理之前的进程
pkill -f "mock/" || true
pkill -f "proxy-node" || true
sleep 2

# 测试 E1: 文本闭环
echo "\\n📝 Running E1: Text Response Loop Test"
npx tsx mock/chat-min.ts &
MOCK_PID=$!
sleep 2

export UPSTREAM_BASE_URL=http://localhost:3100
export UPSTREAM_SUPPORTS_RESPONSES=false
npm run dev &
PROXY_PID=$!
sleep 3

# 测试基本文本响应
echo "Testing basic text response..."
curl -s -N http://localhost:3000/v1/responses \\
  -H 'Content-Type: application/json' \\
  -d '{"model":"test","instructions":"system","input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}]}' \\
  > /tmp/e1_output.txt

# 验证输出
if grep -q "response.output_text.delta" /tmp/e1_output.txt && \\
   grep -q "response.completed" /tmp/e1_output.txt; then
  echo "✅ E1 PASSED: Text response loop working"
else
  echo "❌ E1 FAILED: Missing required events"
  cat /tmp/e1_output.txt
  exit 1
fi

kill $MOCK_PID $PROXY_PID
sleep 2

# 测试 E2: 工具调用
echo "\\n🔧 Running E2: Tool Calls Test" 
npx tsx mock/chat-tools.ts &
MOCK_PID=$!
sleep 2

export UPSTREAM_BASE_URL=http://localhost:3101
npm run dev &
PROXY_PID=$!
sleep 3

curl -s -N http://localhost:3000/v1/responses \\
  -H 'Content-Type: application/json' \\
  -d '{"model":"test","instructions":"system","input":[],"tools":[{"type":"function","function":{"name":"apply_patch"}},{"type":"local_shell","name":"local_shell"}]}' \\
  > /tmp/e2_output.txt

# 验证工具调用输出
if grep -q '"type":"function_call"' /tmp/e2_output.txt && \\
   grep -q '"name":"apply_patch"' /tmp/e2_output.txt && \\
   grep -q '"name":"local_shell"' /tmp/e2_output.txt; then
  echo "✅ E2 PASSED: Tool calls working"
else
  echo "❌ E2 FAILED: Tool call aggregation failed"
  cat /tmp/e2_output.txt  
  exit 1
fi

kill $MOCK_PID $PROXY_PID
sleep 2

# 测试 E3: 错误处理
echo "\\n⚠️  Running E3: Error Handling Test"
npx tsx mock/chat-429.ts &
MOCK_PID=$!
sleep 2

export UPSTREAM_BASE_URL=http://localhost:3102
npm run dev &
PROXY_PID=$!
sleep 3

# 第一次请求应该返回 429 with Retry-After
response=$(curl -s -i http://localhost:3000/v1/chat/completions \\
  -H 'Content-Type: application/json' \\
  -d '{"model":"test","messages":[],"stream":true}')

if echo "$response" | grep -q "HTTP/1.1 429" && \\
   echo "$response" | grep -q "Retry-After: 3"; then
  echo "✅ E3 PASSED: Error headers transparently forwarded"
else
  echo "❌ E3 FAILED: Missing Retry-After header"
  echo "$response"
  exit 1
fi

kill $MOCK_PID $PROXY_PID

echo "\\n🎉 All integration tests passed!"
echo "The Codex LLM integration is ready for production use."

# 清理临时文件
rm -f /tmp/e1_output.txt /tmp/e2_output.txt
```

### 持续测试

```bash
# 给脚本执行权限
chmod +x test/full-integration.sh

# 运行完整测试
./test/full-integration.sh

# 设置 CI/CD 钩子
echo "./test/full-integration.sh" >> .git/hooks/pre-push
```

## 性能基准测试

### 延迟测试

```typescript
async function benchmarkLatency() {
  const testCases = [
    { name: 'Short Response', tokens: 10 },
    { name: 'Medium Response', tokens: 100 },
    { name: 'Long Response', tokens: 1000 },
  ];
  
  for (const testCase of testCases) {
    const start = Date.now();
    
    const response = await fetch('http://localhost:3000/v1/responses', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        model: 'test',
        instructions: `Generate exactly ${testCase.tokens} tokens`,
        input: [{ type: 'message', role: 'user', content: [{ type: 'input_text', text: 'test' }] }]
      })
    });
    
    const reader = response.body?.getReader();
    let firstByteTime = 0;
    let lastByteTime = 0;
    
    while (reader) {
      const { done, value } = await reader.read();
      if (done) break;
      
      const now = Date.now();
      if (firstByteTime === 0) firstByteTime = now;
      lastByteTime = now;
    }
    
    console.log(`${testCase.name}:`);
    console.log(`  Time to first byte: ${firstByteTime - start}ms`);  
    console.log(`  Total time: ${lastByteTime - start}ms`);
    console.log(`  Tokens/sec: ${testCase.tokens / (lastByteTime - start) * 1000}\\n`);
  }
}
```

### 并发测试

```bash
# 并发请求测试
echo "Testing concurrent requests..."
for i in {1..10}; do
  curl -s http://localhost:3000/v1/responses \\
    -H 'Content-Type: application/json' \\
    -d '{"model":"test","instructions":"concurrent test","input":[]}' \\
    > /tmp/concurrent_$i.txt &
done

wait
echo "All concurrent requests completed"
```

## 生产就绪检查清单

### 功能验证
- [ ] E1: 文本响应闭环正常
- [ ] E2: 工具调用分片聚合正确  
- [ ] E3: 错误头部透传有效
- [ ] 支持 Chat 和 Responses 两种 API
- [ ] SSE 事件格式正确
- [ ] 工具并发调用无污染

### 性能验证
- [ ] 首字节延迟 < 500ms
- [ ] 支持 100+ 并发连接
- [ ] 内存使用稳定（无泄漏）
- [ ] CPU 使用合理（< 80%）

### 稳定性验证  
- [ ] 24 小时稳定运行
- [ ] 网络异常自动重连
- [ ] 优雅处理客户端断开
- [ ] 正确的错误恢复机制

### 安全验证
- [ ] 输入参数验证
- [ ] 请求大小限制
- [ ] 速率限制有效
- [ ] 敏感信息正确脱敏

### 可观测性
- [ ] 结构化日志输出
- [ ] 关键指标监控
- [ ] 健康检查端点
- [ ] 错误告警机制

### 配置验证
- [ ] 环境变量正确解析
- [ ] 配置热重载支持
- [ ] 多环境配置隔离
- [ ] 配置验证与默认值

通过以上验证实验和检查清单，可以确保 Codex LLM 集成方案的可靠性和生产就绪状态。

## 相关文档

- [API 规范](../api-specs/api-specifications.md) - 理解验证中涉及的 API 格式
- [事件映射](../api-specs/event-mapping.md) - 理解事件转换的正确性验证
- [Node 实现](../implementation/node-proxy-implementation.md) - 参考完整的代理实现
- [配置指南](../configuration/configuration-guide.md) - 了解生产环境的配置最佳实践