# Provider 配置规范（config.toml）

本文说明如何在 `~/.codex/config.toml` 中新增/覆盖 LLM Provider，以便无改码接入不同的上游服务或自建代理。并包含常见示例与进阶网络参数。

## 配置入口与优先级

生效顺序（高→低）：

1. 命令行专用旗标（如 `--model o3`）
2. 通用 `-c/--config` 覆盖（如 `--config model_providers.xxx.wire_api="chat"`）
3. `~/.codex/config.toml`

## 选择模型与 Provider

顶层常用键：

```toml
model = "gpt-5"         # 或你的模型
model_provider = "openai"  # 从下方的 model_providers 中选择
```

可结合 `profiles` 管理多套默认值，使用 `--profile` 选择（示例见仓库 `docs/config.md`）。

## model_providers.<id> 字段

每个 Provider 块的字段含义：

```toml
[model_providers.your-proxy]
name = "Display Name"                       # 友好名称
base_url = "https://host:port/v1"           # 基础 URL（Codex 会追加路径）
env_key = "YOUR_API_KEY_ENV"                # 可选；从环境变量注入 Bearer Token
env_key_instructions = "how to get key"     # 可选；缺失时的提示文本
wire_api = "chat"                            # "chat" | "responses"；缺省为 "chat"
query_params = { api-version = "2025-04-01" } # 可选；拼接到 URL 的查询参数
http_headers = { "X-Feature" = "on" }       # 可选；静态附加头
env_http_headers = { "X-Flag" = "FLAG_ENV" } # 可选；从环境读取的头（空值则忽略）
request_max_retries = 4                      # 可选；请求级重试上限
stream_max_retries = 5                       # 可选；流重连上限
stream_idle_timeout_ms = 300000              # 可选；流空闲超时（毫秒）
requires_openai_auth = false                 # 可选；默认为 false
```

说明：

- `base_url`：
  - Responses：最终请求 `POST {base_url}/responses`。
  - Chat：最终请求 `POST {base_url}/chat/completions`。
- `env_key`：若设置但未提供环境变量，会报缺失错误；若未设置、且使用 ChatGPT 登录态，则由 Codex 注入。
- `query_params`：按键值对拼接到请求 URL（Azure 需要 `api-version`）。
- `http_headers` / `env_http_headers`：为该 Provider 所有请求附加额外头；后者从环境变量读取，空值跳过。

## 常见示例

### OpenAI（Chat Completions）

```toml
model = "gpt-4o"
model_provider = "openai-chat"

[model_providers.openai-chat]
name = "OpenAI using Chat Completions"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
wire_api = "chat"
```

### Azure（Chat Completions，必须 `api-version`）

```toml
[model_providers.azure]
name = "Azure"
base_url = "https://YOUR_PROJECT.openai.azure.com/openai"
env_key = "AZURE_OPENAI_API_KEY"  # 或你使用的 OPENAI_API_KEY
wire_api = "chat"
query_params = { api-version = "2025-04-01-preview" }
```

### Ollama（本地）

```toml
[model_providers.ollama]
name = "Ollama"
base_url = "http://localhost:11434/v1"
wire_api = "chat"
```

### 任意第三方（自建代理）

```toml
[model_providers.your-proxy]
name = "Your Proxy"
base_url = "https://api.your-proxy.com/v1"
env_key = "YOUR_PROXY_API_KEY"
wire_api = "responses"   # 或 "chat"
http_headers = { "X-Feature" = "on" }
env_http_headers = { "X-Flag" = "YOUR_FLAG_ENV" }
```

## 环境变量快捷覆盖

- 覆盖内置 OpenAI `base_url`：`OPENAI_BASE_URL`（便于指向代理/Mock/Azure 风格）
- 覆盖内置 OSS 提供方 `base_url`：`CODEX_OSS_BASE_URL`（默认 `http://localhost:11434/v1`）

> 提示：如改动通用/核心 Provider，可同时考虑 `request_max_retries / stream_*`，避免弱网下交互不稳。

## 进阶网络参数

- `request_max_retries`：请求失败的重试次数上限（默认 4）。
- `stream_max_retries`：流断开的重连次数上限（默认 5，具体重连策略由上层管理）。
- `stream_idle_timeout_ms`：流空闲超时（默认 300_000ms）。

## 与模型家族的关系

`model` 会映射到 `ModelFamily`（`core/src/model_family.rs`）：

- 是否支持 `reasoning` 参数与 `text.verbosity`（如 gpt‑5/o3）
- 是否偏好/需要 `local_shell` 工具或 `apply_patch` 的特定形态

在切换 Provider 的同时，建议一并确认 `model` 与 `family` 的适配性。

