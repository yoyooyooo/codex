# Codex Provider 配置指南

本文详细说明如何在 `~/.codex/config.toml` 中配置 LLM Provider，以便无需修改代码即可接入不同的上游服务或自建代理。

## 配置层次与优先级

配置的生效顺序（从高到低）：

1. **命令行专用标志**：如 `--model o3`
2. **通用配置覆盖**：如 `--config model_providers.xxx.wire_api="chat"`  
3. **配置文件**：`~/.codex/config.toml`

## 基本配置结构

### 顶层配置

```toml
model = "gpt-4o"                # 默认使用的模型
model_provider = "openai"       # 默认使用的 Provider ID
```

### Provider 定义

每个 Provider 需要在 `model_providers` 表中定义：

```toml
[model_providers.<provider_id>]
name = "Display Name"
base_url = "https://api.example.com/v1"
# ... 其他配置项
```

## Provider 配置字段详解

### 必需字段

| 字段 | 类型 | 说明 | 示例 |
|------|------|------|------|
| `name` | String | Provider 友好显示名称 | `"OpenAI"` |
| `base_url` | String | API 基础 URL，Codex 会自动追加路径 | `"https://api.openai.com/v1"` |

### 认证字段

| 字段 | 类型 | 说明 | 示例 |
|------|------|------|------|
| `env_key` | String | 环境变量名，用于获取 API Key | `"OPENAI_API_KEY"` |
| `env_key_instructions` | String | 当 API Key 缺失时的提示信息 | `"Please set your OpenAI API key"` |
| `requires_openai_auth` | Boolean | 是否需要 OpenAI 认证，默认 false | `false` |

### 协议与格式字段  

| 字段 | 类型 | 说明 | 可选值 |
|------|------|------|--------|
| `wire_api` | String | 使用的 Wire API 协议，默认 "chat" | `"chat"` / `"responses"` |

### HTTP 配置字段

| 字段 | 类型 | 说明 | 示例 |
|------|------|------|------|
| `query_params` | Table | 附加到 URL 的查询参数 | `{ api-version = "2025-04-01" }` |
| `http_headers` | Table | 静态 HTTP 头部 | `{ "X-Feature" = "enabled" }` |
| `env_http_headers` | Table | 从环境变量读取的头部 | `{ "X-Org-ID" = "ORG_ID_ENV" }` |

### 重试与超时字段

| 字段 | 类型 | 说明 | 默认值 |
|------|------|------|--------|
| `request_max_retries` | Integer | 请求失败重试次数上限 | `4` |
| `stream_max_retries` | Integer | 流断开重连次数上限 | `5` |  
| `stream_idle_timeout_ms` | Integer | 流空闲超时时间（毫秒） | `300000` |

## 常见配置示例

### OpenAI 官方 API

```toml
model = "gpt-4o"
model_provider = "openai"

[model_providers.openai]
name = "OpenAI Official API"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
env_key_instructions = "Get your API key from https://platform.openai.com/api-keys"
wire_api = "chat"
request_max_retries = 4
stream_max_retries = 5
stream_idle_timeout_ms = 300000
```

### OpenAI Responses API

```toml
[model_providers.openai-responses]
name = "OpenAI Responses API"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
wire_api = "responses"
http_headers = { "OpenAI-Beta" = "responses=experimental" }
```

### Azure OpenAI

```toml
[model_providers.azure]
name = "Azure OpenAI"
base_url = "https://YOUR_RESOURCE.openai.azure.com/openai"
env_key = "AZURE_OPENAI_API_KEY"
wire_api = "chat"
query_params = { api-version = "2025-04-01-preview" }
http_headers = { "api-key" = "{{env.AZURE_OPENAI_API_KEY}}" }
```

### Ollama 本地服务

```toml
[model_providers.ollama]
name = "Ollama Local"
base_url = "http://localhost:11434/v1"
wire_api = "chat"
# Ollama 不需要 API Key
request_max_retries = 2
stream_idle_timeout_ms = 120000
```

### 自建代理服务

```toml
[model_providers.my-proxy]
name = "My Custom Proxy"
base_url = "https://my-proxy.example.com/v1"
env_key = "MY_PROXY_API_KEY"
env_key_instructions = "Contact admin for your proxy API key"
wire_api = "chat"  # 或 "responses"

# 自定义头部
http_headers = { 
  "X-Client" = "codex",
  "X-Version" = "1.0"
}

# 从环境变量读取的头部
env_http_headers = {
  "X-User-ID" = "USER_ID_ENV",
  "X-Org-ID" = "ORG_ID_ENV"
}

# 针对自建服务优化的重试策略
request_max_retries = 3
stream_max_retries = 3
stream_idle_timeout_ms = 180000
```

### 多 Provider 配置示例

```toml
# 默认使用 OpenAI
model = "gpt-4o"
model_provider = "openai"

[model_providers.openai]
name = "OpenAI"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
wire_api = "chat"

[model_providers.azure]
name = "Azure OpenAI"
base_url = "https://my-resource.openai.azure.com/openai"
env_key = "AZURE_OPENAI_API_KEY"
wire_api = "chat"
query_params = { api-version = "2025-04-01-preview" }

[model_providers.ollama]
name = "Ollama"  
base_url = "http://localhost:11434/v1"
wire_api = "chat"

[model_providers.my-proxy]
name = "Custom Proxy"
base_url = "https://api.my-proxy.com/v1"
env_key = "MY_PROXY_KEY"
wire_api = "responses"
```

## 环境变量快捷覆盖

Codex 支持通过环境变量快速覆盖内置 Provider 的配置：

| 环境变量 | 影响的 Provider | 说明 |
|----------|----------------|------|
| `OPENAI_BASE_URL` | `openai` | 覆盖 OpenAI 的 base_url |
| `CODEX_OSS_BASE_URL` | `oss` | 覆盖开源模型的 base_url（默认 `http://localhost:11434/v1`） |

### 使用示例

```bash
# 将 OpenAI 请求代理到本地服务
export OPENAI_BASE_URL=http://localhost:8080/v1
codex "你好"

# 使用不同的 Ollama 地址
export CODEX_OSS_BASE_URL=http://192.168.1.100:11434/v1  
codex --model llama3 "Hello"
```

## Profile 多环境配置

Codex 支持通过 Profile 管理多套配置，便于在不同环境间切换：

```toml
# 默认配置
model = "gpt-4o"
model_provider = "openai"

# 开发环境 Profile
[profiles.dev]
model = "gpt-3.5-turbo"
model_provider = "openai"

# 测试环境 Profile  
[profiles.test]
model = "llama3"
model_provider = "ollama"

# 生产环境 Profile
[profiles.prod]
model = "gpt-4o"
model_provider = "azure"
```

### 使用 Profile

```bash
# 使用默认配置
codex "写一个 Hello World"

# 使用开发环境配置
codex --profile dev "写一个 Hello World"

# 使用测试环境配置
codex --profile test "写一个 Hello World"
```

## 配置验证与调试

### 检查配置状态

```bash
# 显示当前配置
codex --config-status

# 显示特定 Provider 配置
codex --show-provider openai

# 测试 Provider 连通性
codex --test-provider openai
```

### 常见配置错误

#### API Key 缺失
```
Error: Missing API key for provider 'openai'
Set environment variable: OPENAI_API_KEY
```

**解决方案**：
```bash
export OPENAI_API_KEY=your_api_key_here
```

#### Base URL 格式错误
```
Error: Invalid base_url format: 'https://api.openai.com/v1/'  
Remove trailing slash from base_url
```

**解决方案**：
```toml
# 错误
base_url = "https://api.openai.com/v1/"

# 正确  
base_url = "https://api.openai.com/v1"
```

#### 不支持的 Wire API
```
Error: Provider 'some-provider' does not support wire_api: 'responses'
Try wire_api: 'chat' instead
```

**解决方案**：
```toml
# 改为通用性更好的 chat API
wire_api = "chat"
```

## 高级配置技巧

### 条件性头部设置

```toml
[model_providers.conditional]
name = "Conditional Headers"
base_url = "https://api.example.com/v1"
env_key = "API_KEY"

# 仅在环境变量存在时设置头部
env_http_headers = {
  "X-Organization" = "ORG_ID",      # 如果 ORG_ID 为空，此头部不会发送
  "X-Project" = "PROJECT_ID"        # 如果 PROJECT_ID 为空，此头部不会发送  
}
```

### 性能优化配置

```toml
[model_providers.optimized]
name = "Performance Optimized"
base_url = "https://fast-api.example.com/v1"
env_key = "API_KEY"

# 针对快速 API 的优化配置
request_max_retries = 2           # 减少重试次数
stream_max_retries = 3            # 减少流重试次数  
stream_idle_timeout_ms = 60000    # 更短的超时时间
```

### 调试配置

```toml
[model_providers.debug]
name = "Debug Provider"
base_url = "http://localhost:8080/v1"
env_key = "DEBUG_API_KEY"

# 调试友好的配置
request_max_retries = 1           # 不重试，便于观察错误
stream_max_retries = 1
stream_idle_timeout_ms = 30000    # 较短超时，快速失败

# 添加调试头部
http_headers = {
  "X-Debug" = "true",
  "X-Source" = "codex"
}
```

## 与模型家族的关系

Codex 会根据 `model` 字段推导出 `ModelFamily`，影响以下行为：

### 支持的功能特性

| ModelFamily | reasoning 支持 | text.verbosity 支持 | local_shell 偏好 |
|-------------|---------------|-------------------|------------------|
| GPT-5 | ✅ | ✅ | ✅ |
| O3 | ✅ | ❌ | ✅ |
| GPT-4 | ❌ | ❌ | ✅ |
| Claude | ❌ | ❌ | ✅ |

### 工具配置影响

```rust
// ModelFamily 影响工具的默认配置
match model_family {
    ModelFamily::Gpt5 => {
        // GPT-5 偏好使用 local_shell 工具
        config.prefer_local_shell = true;
        config.reasoning_enabled = true;
    },
    ModelFamily::O3 => {
        // O3 支持推理但不支持 text verbosity
        config.reasoning_enabled = true;
        config.text_verbosity_enabled = false;
    },
    // ...
}
```

## 配置最佳实践

### 1. 分环境管理

```toml
# 使用 Profile 管理不同环境
[profiles.dev]
model_provider = "ollama"  # 开发时使用本地模型

[profiles.prod]  
model_provider = "openai"  # 生产时使用云服务
```

### 2. 安全配置

```bash
# 使用环境变量存储敏感信息，不要写在配置文件中
export OPENAI_API_KEY=sk-...
export AZURE_OPENAI_API_KEY=...

# 权限控制
chmod 600 ~/.codex/config.toml
```

### 3. 性能调优

```toml
# 针对不同网络环境调整超时和重试
[model_providers.fast-network]
stream_idle_timeout_ms = 60000    # 快速网络：短超时

[model_providers.slow-network]
stream_idle_timeout_ms = 600000   # 慢速网络：长超时
request_max_retries = 6           # 更多重试
```

### 4. 多 Provider 故障转移

虽然单个调用只使用一个 Provider，但可以配置多个备选：

```toml
model_provider = "primary"

[model_providers.primary]
name = "Primary Service"
base_url = "https://api.primary.com/v1"
env_key = "PRIMARY_API_KEY"

[model_providers.backup]
name = "Backup Service"  
base_url = "https://api.backup.com/v1"
env_key = "BACKUP_API_KEY"
```

```bash
# 手动切换到备用服务
codex --config model_provider=backup "你好"
```

## 相关文档

- [API 规范](../api-specs/api-specifications.md) - 了解不同 Wire API 的详细规格
- [架构概览](../architecture/architecture-overview.md) - Provider 在整体架构中的位置
- [Node 实现方案](../implementation/node-proxy-implementation.md) - 自建 Provider 的完整实现示例
- [测试验证](../testing/validation-experiments.md) - 如何验证 Provider 配置的正确性