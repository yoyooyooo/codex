# 配置指南

## 配置系统概述

Codex 采用**分层配置**与**配置驱动**的设计理念，通过 `~/.codex/config.toml` 实现无代码接入各种 LLM 提供商。

### 配置优先级（高 → 低）

```bash
1. 命令行专用旗标     # --model o3
2. 通用配置覆盖       # --config model_providers.xxx.wire_api="chat"  
3. ~/.codex/config.toml  # 持久化配置文件
```

### 配置哲学
- **提供商抽象**：通过配置定义，而非硬编码实现
- **协议灵活性**：支持 Responses 与 Chat API 自由切换
- **环境适配**：开发/生产环境的差异化配置
- **安全优先**：API Key 通过环境变量注入

## 核心配置结构

### 顶层配置键

```toml
# ~/.codex/config.toml

# 全局默认设置
model = "gpt-5"                    # 默认模型
model_provider = "openai"          # 默认提供商

# 多环境支持（可选）
[profiles.development] 
model = "gpt-4o"
model_provider = "openai-dev"

[profiles.production]
model = "claude-3-opus"  
model_provider = "anthropic"
```

### Provider 定义模板

```toml
[model_providers.<provider_id>]
name = "Display Name"              # 友好显示名称
base_url = "https://api.example.com/v1"  # 基础 URL
env_key = "API_KEY_ENV_VAR"       # API Key 环境变量名
env_key_instructions = "..."       # 缺失时的获取指引
wire_api = "chat"                  # "chat" | "responses"  
query_params = { key = "value" }   # URL 查询参数
http_headers = { header = "value" } # 静态 HTTP 头
env_http_headers = { header = "ENV_VAR" } # 从环境读取的头
request_max_retries = 4           # 请求重试上限
stream_max_retries = 5            # 流重连上限
stream_idle_timeout_ms = 300000   # 流空闲超时（毫秒）
requires_openai_auth = false      # 是否需要 OpenAI 鉴权
```

## Provider 配置详解

### base_url 规则

**端点拼接规则**：
- Responses API：`POST {base_url}/responses`
- Chat API：`POST {base_url}/chat/completions`

**常见模式**：
```toml
# OpenAI 官方
base_url = "https://api.openai.com/v1"

# Azure OpenAI  
base_url = "https://YOUR_RESOURCE.openai.azure.com/openai"

# 本地 Ollama
base_url = "http://localhost:11434/v1"

# 自建代理
base_url = "https://your-proxy.example.com/v1"
```

### 鉴权配置

#### 1. API Key 鉴权
```toml
[model_providers.example]
env_key = "EXAMPLE_API_KEY"
env_key_instructions = "从 https://example.com/api-keys 获取 API Key"
```

使用方式：
```bash
export EXAMPLE_API_KEY="sk-your-key-here"
codex chat "Hello world"
```

#### 2. 无鉴权模式
```toml
[model_providers.local]
base_url = "http://localhost:11434/v1"
# 不设置 env_key，表示无需鉴权
```

#### 3. ChatGPT 登录态复用
```toml
[model_providers.openai-browser]
base_url = "https://api.openai.com/v1"
requires_openai_auth = true  # 使用浏览器登录态
```

### wire_api 选择策略

| wire_api 值 | 适用场景 | 优缺点 |
|-------------|----------|--------|
| **"chat"** | 标准兼容，大多数提供商 | ✅ 兼容性好 ❌ 功能受限 |
| **"responses"** | OpenAI 官方，需要完整语义 | ✅ 功能完整 ❌ 支持范围小 |

**选择建议**：
- 新接入提供商：优先选择 `"chat"`
- OpenAI 官方且需要推理：选择 `"responses"`
- 自建代理：根据实现能力选择

### 网络参数优化

```toml
[model_providers.production]
name = "Production OpenAI"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
wire_api = "responses"

# 生产环境优化
request_max_retries = 6           # 更多重试次数
stream_max_retries = 8           # 更多流重连  
stream_idle_timeout_ms = 600000  # 更长超时（10分钟）

# Azure 必需参数
query_params = { "api-version" = "2025-04-01-preview" }

# 自定义头
http_headers = { "X-Environment" = "production" }
env_http_headers = { "OpenAI-Organization" = "OPENAI_ORG_ID" }
```

## 常见 Provider 配置示例

### 1. OpenAI 官方

#### 标准 Chat API
```toml
[model_providers.openai-chat]
name = "OpenAI (Chat Completions)"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
wire_api = "chat"
request_max_retries = 4
```

#### 完整 Responses API  
```toml
[model_providers.openai-responses]
name = "OpenAI (Responses API)"
base_url = "https://api.openai.com/v1"  
env_key = "OPENAI_API_KEY"
wire_api = "responses"
env_http_headers = { "OpenAI-Organization" = "OPENAI_ORG_ID" }
```

### 2. Azure OpenAI

```toml
[model_providers.azure]
name = "Azure OpenAI"
base_url = "https://YOUR_RESOURCE.openai.azure.com/openai"
env_key = "AZURE_OPENAI_API_KEY"
wire_api = "chat"
query_params = { "api-version" = "2025-04-01-preview" }
request_max_retries = 6
stream_idle_timeout_ms = 480000
```

### 3. 本地 Ollama

```toml
[model_providers.ollama]
name = "Local Ollama"
base_url = "http://localhost:11434/v1"
wire_api = "chat"
# 无需 env_key，本地服务
request_max_retries = 2
stream_idle_timeout_ms = 180000
```

### 4. Anthropic Claude

```toml
[model_providers.anthropic]
name = "Anthropic Claude"  
base_url = "https://api.anthropic.com/v1"
env_key = "ANTHROPIC_API_KEY"
wire_api = "chat"
http_headers = { "anthropic-version" = "2025-01-01" }
```

### 5. 自建代理

```toml
[model_providers.custom-proxy]
name = "Custom Proxy"
base_url = "https://your-proxy.example.com/v1"  
env_key = "CUSTOM_PROXY_KEY"
wire_api = "responses"  # 支持完整语义

# 自定义配置
http_headers = { 
    "X-Proxy-Version" = "v2",
    "X-Feature-Set" = "full"  
}
env_http_headers = {
    "X-User-ID" = "USER_ID_ENV",
    "X-Org-ID" = "ORG_ID_ENV"
}

# 针对代理的网络优化
request_max_retries = 8
stream_max_retries = 10  
stream_idle_timeout_ms = 900000  # 15分钟
```

## 模型与模型家族

### ModelFamily 映射

Codex 会将 `model` 自动映射到 `ModelFamily`，影响可用特性：

```rust
// 模型家族特性示例
pub enum ModelFamily {
    Gpt5 {
        reasoning: bool,          // 支持推理模式
        text_verbosity: bool,     // 支持详细程度控制
    },
    GptO3 {
        reasoning: bool,
        local_shell_preference: bool,  // 偏好 local_shell 工具
    },
    Claude3 {
        apply_patch_format: PatchFormat, // apply_patch 工具格式
    },
    // ...
}
```

### 模型特性配置

```toml
# 配置不同模型的特性
model = "gpt-5"  # 自动映射到 Gpt5 家族

[model_providers.openai-gpt5]
name = "GPT-5 with Reasoning"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
wire_api = "responses"  # 推理功能需要 responses API
```

## 环境变量快捷覆盖

### 内置环境变量支持

```bash
# 覆盖内置 OpenAI base_url（便于代理/测试）
export OPENAI_BASE_URL="https://your-proxy.example.com/v1"

# 覆盖内置 OSS 提供商（Ollama 等）
export CODEX_OSS_BASE_URL="http://localhost:8080/v1"
```

### 动态配置覆盖

```bash
# 临时切换模型
codex --model claude-3-opus chat "Hello"

# 临时覆盖 Provider 配置  
codex --config model_providers.openai.wire_api="chat" chat "Hello"

# 组合使用
codex --model gpt-4o --config model_providers.openai.base_url="https://proxy.example.com/v1" chat "Hello"
```

## Profile 多环境管理

### Profile 定义

```toml
# 默认配置
model = "gpt-4o"
model_provider = "openai"

# 开发环境
[profiles.dev]
model = "gpt-4o-mini" 
model_provider = "openai-dev"

[model_providers.openai-dev]
name = "OpenAI Development"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_DEV_API_KEY"
wire_api = "chat"
request_max_retries = 2

# 生产环境  
[profiles.prod]
model = "gpt-5"
model_provider = "openai-prod"

[model_providers.openai-prod]
name = "OpenAI Production"  
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_PROD_API_KEY" 
wire_api = "responses"
request_max_retries = 6
stream_idle_timeout_ms = 600000

# 本地测试
[profiles.local]
model = "llama2"
model_provider = "ollama"
```

### Profile 使用

```bash
# 使用开发环境配置
codex --profile dev chat "Hello"

# 使用生产环境配置  
codex --profile prod chat "Hello"

# 使用本地测试环境
codex --profile local chat "Hello"
```

## 最佳实践

### 1. 安全最佳实践

```bash
# ✅ 正确：API Key 通过环境变量
export OPENAI_API_KEY="sk-..."
export ANTHROPIC_API_KEY="sk-ant-..."

# ❌ 错误：不要在配置文件中写死 API Key
[model_providers.bad]
api_key = "sk-hardcoded-key"  # 这样做是错误的！
```

### 2. 网络优化建议

```toml
# 针对不同网络环境的优化
[model_providers.fast-network]  
request_max_retries = 2
stream_idle_timeout_ms = 120000

[model_providers.slow-network]
request_max_retries = 8  
stream_idle_timeout_ms = 600000
stream_max_retries = 10
```

### 3. 开发调试配置

```toml
[model_providers.debug]
name = "Debug Mode"
base_url = "https://api.openai.com/v1"  
env_key = "OPENAI_API_KEY"
wire_api = "chat"

# 调试优化：更少重试，更快失败
request_max_retries = 1
stream_idle_timeout_ms = 30000

# 调试头
http_headers = { "X-Debug" = "true" }
```

### 4. 多区域容错

```toml
# 主区域  
[model_providers.openai-primary]
name = "OpenAI US"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"

# 备用区域  
[model_providers.openai-fallback]
name = "OpenAI EU"  
base_url = "https://api.openai.eu/v1"
env_key = "OPENAI_EU_API_KEY"
```

## 故障排查

### 常见配置问题

#### 1. API Key 问题
```bash
# 检查环境变量
echo $OPENAI_API_KEY

# 检查配置
codex config show model_providers.openai.env_key
```

#### 2. 网络连接问题  
```bash
# 测试连通性
curl -i https://api.openai.com/v1/models \
  -H "Authorization: Bearer $OPENAI_API_KEY"

# 检查代理设置
echo $HTTP_PROXY $HTTPS_PROXY
```

#### 3. 协议兼容性问题
```toml
# 如果 responses API 不工作，降级到 chat
[model_providers.openai-safe]
wire_api = "chat"  # 兼容性更好
```

### 调试配置

```toml
# 调试模式配置
[model_providers.debug]
name = "Debug Provider"
base_url = "http://localhost:3000/v1"  # 本地 mock 服务
wire_api = "chat"
request_max_retries = 0  # 不重试，快速失败
stream_idle_timeout_ms = 10000  # 10秒超时

# 详细日志头
http_headers = { 
    "X-Debug" = "verbose",
    "X-Trace-ID" = "debug-session-123"
}
```

## 配置验证

### 验证配置正确性

```bash
# 检查当前配置
codex config show

# 验证特定 Provider
codex config validate model_providers.openai

# 测试连接
codex --model gpt-4o --dry-run chat "test"
```

### 配置模板生成

```bash
# 生成新 Provider 配置模板
codex config generate-provider custom-provider

# 生成完整配置模板  
codex config generate-full > ~/.codex/config.toml.template
```

---

## 下一步
- **[工具集成](./04-tools-integration.md)**：了解工具系统配置
- **[实现指南](./05-implementation-guide.md)**：构建自定义 Provider
- **[测试验证](./06-testing-validation.md)**：验证配置正确性

通过合理的配置管理，Codex 能够无缝集成各种 LLM 提供商，提供一致且可靠的使用体验。