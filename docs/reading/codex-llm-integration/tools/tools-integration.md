# 工具系统与集成指南

本文详细说明 Codex 的工具建模、与 Wire API 的映射关系、MCP 工具 Schema 归一化策略，以及工具调用的完整执行生命周期。

## 工具类型概览

Codex 支持多种类型的工具，以满足不同的使用场景：

### Responses API 原生支持的工具类型

| 工具类型 | 描述 | 使用场景 | 示例 |
|---------|------|----------|------|
| `function` | 标准函数调用，带 JSON Schema 参数 | 结构化数据处理、API 调用 | 文件操作、数据库查询 |
| `local_shell` | 本地 Shell 命令执行 | 系统操作、开发工具调用 | `ls`, `git status`, `npm install` |
| `web_search` | Web 搜索功能 | 获取最新信息、研究 | 搜索技术文档、新闻 |
| `custom`/`freeform` | 自由格式工具 | 灵活的文本处理 | 自定义脚本、模板处理 |
| `view_image` | 图片内容分析 | 图像理解、视觉任务 | 截图分析、图表解读 |

### Chat API 兼容性

> **重要**：Chat Completions API 仅支持 `function` 工具。Codex 会自动将 Responses 工具集转换为 Chat 格式，非 function 工具会被过滤掉。

## JSON Schema 规范

Codex 使用限定的 JSON Schema 子集来定义工具参数，确保跨平台兼容性：

### 支持的类型

| Schema 类型 | 说明 | 示例 |
|------------|------|------|
| `string` | 字符串类型 | `{ "type": "string", "description": "文件名" }` |
| `number` | 数值类型（包含 integer） | `{ "type": "number", "minimum": 0 }` |
| `boolean` | 布尔类型 | `{ "type": "boolean", "default": false }` |
| `object` | 对象类型 | `{ "type": "object", "properties": {...} }` |
| `array` | 数组类型 | `{ "type": "array", "items": {...} }` |

### Schema 归一化策略

Codex 实现了 `sanitize_json_schema` 函数来标准化 Schema：

```rust
// 归一化规则示例
fn sanitize_json_schema(schema: JsonValue) -> JsonSchema {
    match schema.get("type") {
        Some("integer") => {
            // integer 归一化为 number
            JsonSchema::Number { /* ... */ }
        },
        None => {
            // 根据其他字段推断类型
            if schema.get("properties").is_some() {
                JsonSchema::Object { /* ... */ }
            } else if schema.get("items").is_some() {
                JsonSchema::Array { /* ... */ }
            } else {
                JsonSchema::String { /* ... */ }
            }
        },
        // ... 其他类型处理
    }
}
```

### 对象类型详细配置

```json
{
  "type": "object",
  "properties": {
    "filename": {
      "type": "string",
      "description": "要操作的文件名"
    },
    "content": {
      "type": "string", 
      "description": "文件内容"
    },
    "options": {
      "type": "object",
      "properties": {
        "backup": {
          "type": "boolean",
          "default": true,
          "description": "是否创建备份"
        }
      },
      "required": ["backup"]
    }
  },
  "required": ["filename", "content"],
  "additionalProperties": false
}
```

### 数组类型配置

```json
{
  "type": "array",
  "items": {
    "type": "object",
    "properties": {
      "name": { "type": "string" },
      "value": { "type": "string" }
    },
    "required": ["name", "value"]
  },
  "minItems": 1,
  "maxItems": 10
}
```

## MCP 工具映射与集成

### MCP 工具注册

MCP (Model Context Protocol) 工具通过全限定名注册：

```rust
// 注册格式：server_name/tool_name
let tool_name = format!("{}/{}", server_name, tool.name);

// 示例
"filesystem/read_file"
"database/query" 
"browser/navigate"
```

### MCP 到 OpenAI 工具转换

```typescript
function mcp_tool_to_openai_tool(mcp_tool: MCPTool): ResponsesApiTool {
  return {
    type: 'function',
    function: {
      name: mcp_tool.name,
      description: mcp_tool.description || '',
      parameters: sanitize_json_schema(mcp_tool.inputSchema)
    }
  };
}
```

### MCP 工具排序

为提高缓存命中率，工具按名称排序：

```rust
// 工具列表按字母序排序
tools.sort_by(|a, b| a.name.cmp(&b.name));
```

## 内置工具定义

### Shell 工具

```json
{
  "type": "local_shell",
  "name": "local_shell", 
  "description": "Execute shell commands locally",
  "parameters": {
    "type": "object",
    "properties": {
      "command": {
        "type": "string",
        "description": "Shell command to execute"
      },
      "request_permission_elevation": {
        "type": "boolean",
        "description": "Request permission for elevated operations",
        "default": false
      }
    },
    "required": ["command"],
    "additionalProperties": false
  }
}
```

### Apply Patch 工具

```json
{
  "type": "function",
  "function": {
    "name": "apply_patch",
    "description": "Apply code changes to files",
    "parameters": {
      "type": "object", 
      "properties": {
        "files": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "path": { "type": "string" },
              "content": { "type": "string" }
            },
            "required": ["path", "content"]
          }
        }
      },
      "required": ["files"]
    }
  }
}
```

### View Image 工具

```json
{
  "type": "view_image",
  "name": "view_image",
  "description": "View and analyze images",
  "parameters": {
    "type": "object",
    "properties": {
      "path": {
        "type": "string",
        "description": "Path to the image file"
      }
    },
    "required": ["path"],
    "additionalProperties": false
  }
}
```

### Web Search 工具

```json
{
  "type": "web_search",
  "name": "web_search",
  "description": "Search the web for information",
  "parameters": {
    "type": "object",
    "properties": {
      "query": {
        "type": "string", 
        "description": "Search query"
      },
      "num_results": {
        "type": "number",
        "default": 5,
        "description": "Number of results to return"
      }
    },
    "required": ["query"],
    "additionalProperties": false
  }
}
```

## 工具调用执行生命周期

### 1. 工具调用发起（模型侧）

#### Chat API 流程

```text
1. 模型发送工具调用增量：
   data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_123","function":{"name":"read_file","arguments":"{\\"path\\":"}}]}}]}

2. 继续发送参数分片：  
   data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\\"/tmp/file.txt\\"}"}}]}}]}

3. 发送完成信号：
   data: {"choices":[{"finish_reason":"tool_calls"}]}
```

#### Responses API 流程

```text
1. 模型直接发送完整工具调用：
   data: {"type":"response.output_item.done","item":{"type":"function_call","name":"read_file","arguments":"{\\"path\\": \\"/tmp/file.txt\\"}","call_id":"call_123"}}

2. 发送完成信号：
   data: {"type":"response.completed","id":"resp_456"}
```

### 2. 工具调用解析（Codex 侧）

#### 参数解析与验证

```rust
fn parse_tool_call(call: &ToolCall) -> Result<ExecutableCall, Error> {
    // 1. 查找工具定义
    let tool_def = find_tool_by_name(&call.name)?;
    
    // 2. 解析 JSON 参数
    let args: serde_json::Value = serde_json::from_str(&call.arguments)?;
    
    // 3. 验证参数 Schema
    validate_against_schema(&args, &tool_def.parameters)?;
    
    // 4. 创建可执行调用
    Ok(ExecutableCall {
        id: call.id.clone(),
        name: call.name.clone(),
        args,
    })
}
```

#### 工具分发

```rust
async fn execute_tool_call(call: ExecutableCall) -> ToolResult {
    match call.name.as_str() {
        "local_shell" => execute_shell_command(&call.args).await,
        "apply_patch" => execute_apply_patch(&call.args).await,
        "view_image" => execute_view_image(&call.args).await,
        name if name.contains('/') => {
            // MCP 工具调用
            execute_mcp_tool(name, &call.args).await
        },
        _ => Err(ToolError::UnknownTool(call.name.clone()))
    }
}
```

### 3. 工具执行（实际操作）

#### Shell 命令执行

```rust
async fn execute_shell_command(args: &Value) -> ToolResult {
    let command = args["command"].as_str().ok_or(ToolError::InvalidArgs)?;
    let request_elevation = args.get("request_permission_elevation")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    
    // 权限检查
    if requires_elevation(command) && !request_elevation {
        return Err(ToolError::PermissionDenied);
    }
    
    // 执行命令
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .await?;
    
    Ok(ToolResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code(),
    })
}
```

#### MCP 工具调用

```rust
async fn execute_mcp_tool(name: &str, args: &Value) -> ToolResult {
    let (server_name, tool_name) = name.split_once('/').unwrap();
    
    // 查找 MCP 服务器连接
    let server = mcp_manager.get_server(server_name)?;
    
    // 发送工具调用请求
    let response = server.call_tool(CallToolRequest {
        name: tool_name.to_string(),
        arguments: Some(args.clone()),
    }).await?;
    
    Ok(ToolResult::from_mcp_response(response))
}
```

### 4. 结果返回（回传给模型）

#### 构造 Tool 消息

```rust
fn create_tool_message(call_id: &str, result: &ToolResult) -> Message {
    Message {
        role: "tool".to_string(),
        content: format_tool_result(result),
        tool_call_id: Some(call_id.to_string()),
        // 其他字段...
    }
}

fn format_tool_result(result: &ToolResult) -> String {
    match result {
        ToolResult::Success { output } => output.clone(),
        ToolResult::Error { error, .. } => {
            format!("Error: {}", error)
        },
        ToolResult::Shell { stdout, stderr, exit_code } => {
            let mut output = String::new();
            if !stdout.is_empty() {
                output.push_str(&format!("stdout:\n{}\n", stdout));
            }
            if !stderr.is_empty() {
                output.push_str(&format!("stderr:\n{}\n", stderr));
            }
            output.push_str(&format!("exit_code: {}", exit_code));
            output
        }
    }
}
```

#### 发送到下一轮对话

```rust
// 在对话历史中添加工具结果
conversation.messages.push(create_tool_message(&call_id, &result));

// 继续与模型对话
let next_response = client.send_chat_request(&conversation).await?;
```

## Wire API 工具差异处理

### Chat 工具转换

```rust
fn create_tools_json_for_chat_completions_api(tools: &[Tool]) -> Vec<ChatTool> {
    tools.iter()
        .filter_map(|tool| {
            // 只保留 function 类型的工具
            match tool.tool_type {
                ToolType::Function => Some(ChatTool {
                    r#type: "function".to_string(),
                    function: tool.function.clone(),
                }),
                _ => None, // 过滤掉其他类型的工具
            }
        })
        .collect()
}
```

### Responses 工具保持

```rust
fn create_tools_json_for_responses_api(tools: &[Tool]) -> Vec<ResponsesTool> {
    // Responses API 支持所有工具类型
    tools.iter()
        .map(|tool| ResponsesTool::from(tool))
        .collect()
}
```

## 工具配置管理

### 配置选项

```rust
pub struct ToolsConfig {
    pub include_plan_tool: bool,
    pub include_apply_patch_tool: bool,
    pub tools_web_search_request: bool,
    pub include_view_image_tool: bool,
    pub use_experimental_streamable_shell_tool: bool,
    pub approval_policy: ApprovalPolicy,
    pub sandbox_policy: SandboxPolicy,
}
```

### 动态工具集生成

```rust
pub fn get_openai_tools(config: &ToolsConfig, model_family: &ModelFamily) -> Vec<Tool> {
    let mut tools = Vec::new();
    
    // 内置工具
    if config.include_plan_tool {
        tools.push(create_plan_tool());
    }
    
    if config.include_apply_patch_tool {
        tools.push(create_apply_patch_tool(model_family));
    }
    
    if model_family.supports_local_shell() {
        tools.push(create_shell_tool(config));
    }
    
    if config.tools_web_search_request {
        tools.push(create_web_search_tool());
    }
    
    if config.include_view_image_tool {
        tools.push(create_view_image_tool());
    }
    
    // MCP 工具
    tools.extend(get_mcp_tools());
    
    // 按名称排序以提高缓存命中率
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    
    tools
}
```

## 安全与权限控制

### 权限策略

```rust
pub enum ApprovalPolicy {
    Auto,        // 自动执行所有工具
    Interactive, // 需要用户确认
    Disabled,    // 禁用工具调用
}

pub enum SandboxPolicy {
    None,        // 无沙箱
    ReadOnly,    // 只读操作
    Restricted,  // 受限操作
}
```

### Shell 命令安全检查

```rust
fn is_safe_command(command: &str) -> bool {
    let dangerous_commands = [
        "rm -rf", "sudo rm", "mkfs", "dd", ":(){ :|:& };:", // 危险命令
        "curl", "wget", "nc", "telnet",                     // 网络命令
        "chmod 777", "chown root",                          // 权限命令
    ];
    
    !dangerous_commands.iter().any(|&dangerous| command.contains(dangerous))
}

fn requires_approval(command: &str, policy: &ApprovalPolicy) -> bool {
    match policy {
        ApprovalPolicy::Auto => false,
        ApprovalPolicy::Interactive => !is_safe_command(command),
        ApprovalPolicy::Disabled => true,
    }
}
```

### MCP 工具权限

```rust
fn validate_mcp_tool_access(tool_name: &str, args: &Value) -> Result<(), Error> {
    // 检查工具白名单
    if !is_tool_whitelisted(tool_name) {
        return Err(Error::ToolNotAllowed(tool_name.to_string()));
    }
    
    // 检查参数安全性
    validate_tool_arguments(tool_name, args)?;
    
    Ok(())
}
```

## 错误处理与调试

### 工具调用错误类型

```rust
pub enum ToolError {
    UnknownTool(String),
    InvalidArguments(String),
    ExecutionFailed(String),
    PermissionDenied,
    Timeout,
    NetworkError(String),
    MCPError(String),
}
```

### 错误消息格式化

```rust
fn format_tool_error(error: &ToolError, call_id: &str) -> Message {
    let error_message = match error {
        ToolError::UnknownTool(name) => {
            format!("Unknown tool: {}. Available tools: [list]", name)
        },
        ToolError::InvalidArguments(details) => {
            format!("Invalid arguments: {}", details)
        },
        ToolError::ExecutionFailed(details) => {
            format!("Execution failed: {}", details)
        },
        ToolError::PermissionDenied => {
            "Permission denied. Use request_permission_elevation=true if needed".to_string()
        },
        _ => format!("Tool execution error: {:?}", error),
    };
    
    Message {
        role: "tool".to_string(),
        content: error_message,
        tool_call_id: Some(call_id.to_string()),
        // ...
    }
}
```

### 调试日志

```rust
fn log_tool_execution(call: &ToolCall, result: &Result<ToolResult, ToolError>) {
    match result {
        Ok(result) => {
            log::info!("Tool executed successfully: {} -> {:?}", call.name, result);
        },
        Err(error) => {
            log::error!("Tool execution failed: {} -> {:?}", call.name, error);
        }
    }
}
```

## 最佳实践

### 1. 工具设计原则

- **单一职责**：每个工具只做一件事，做好一件事
- **幂等性**：相同输入应该产生相同输出
- **错误处理**：提供清晰的错误信息和恢复建议
- **参数验证**：严格验证输入参数，防止注入攻击

### 2. Schema 设计建议

```json
{
  "type": "object",
  "properties": {
    "required_param": {
      "type": "string",
      "description": "清晰描述参数用途和格式要求"
    },
    "optional_param": {
      "type": "number",
      "default": 10,
      "minimum": 1,
      "maximum": 100,
      "description": "提供默认值和取值范围"
    }
  },
  "required": ["required_param"],
  "additionalProperties": false  // 禁止额外属性
}
```

### 3. 性能优化

```rust
// 工具缓存
static TOOL_CACHE: Lazy<LruCache<String, Arc<Tool>>> = Lazy::new(|| {
    LruCache::new(NonZeroUsize::new(100).unwrap())
});

// 异步并行执行
async fn execute_tools_parallel(calls: Vec<ToolCall>) -> Vec<ToolResult> {
    let futures = calls.into_iter().map(execute_tool_call);
    futures::future::join_all(futures).await
}
```

### 4. 监控与度量

```rust
// 工具执行统计
struct ToolMetrics {
    total_calls: AtomicU64,
    success_calls: AtomicU64, 
    error_calls: AtomicU64,
    avg_duration: AtomicU64,
}

fn record_tool_metrics(name: &str, duration: Duration, result: &Result<ToolResult, ToolError>) {
    TOOL_METRICS.with_label_values(&[name]).total_calls.inc();
    
    match result {
        Ok(_) => TOOL_METRICS.with_label_values(&[name]).success_calls.inc(),
        Err(_) => TOOL_METRICS.with_label_values(&[name]).error_calls.inc(),
    }
    
    TOOL_METRICS.with_label_values(&[name]).avg_duration.set(duration.as_millis() as u64);
}
```

## 相关文档

- [API 规范](../api-specs/api-specifications.md) - 工具调用在不同 API 中的表示方式
- [事件映射](../api-specs/event-mapping.md) - 工具调用事件的处理和映射
- [架构概览](../architecture/architecture-overview.md) - 工具系统在整体架构中的位置
- [实现方案](../implementation/node-proxy-implementation.md) - 工具调用的具体实现示例