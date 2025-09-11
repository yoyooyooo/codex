# Codex LLM 集成指南

欢迎使用 Codex LLM 集成文档！本指南提供了完整的 Codex 与 LLM 服务集成方案，包括架构设计、API 规范、实现方案、配置指南、工具集成和验证测试。

## 📚 文档结构

```
docs/learning/codex-llm-integration/
├── 📖 README.md                    # 本文件 - 总览和导航
├── 🏗️  architecture/               # 架构设计
│   └── architecture-overview.md    # 系统架构总览
├── 📡 api-specs/                   # API 规范
│   ├── api-specifications.md       # API 协议定义  
│   └── event-mapping.md           # 事件映射规范
├── ⚙️  implementation/             # 实现方案
│   └── node-proxy-implementation.md # Node.js 代理完整实现
├── 🔧 configuration/               # 配置指南
│   └── configuration-guide.md     # Provider 配置详解
├── 🛠️  tools/                      # 工具集成
│   └── tools-integration.md       # 工具系统集成指南
└── 🧪 testing/                    # 测试验证
    └── validation-experiments.md   # 验证实验与测试方案
```

## 🚀 快速开始

### 第一次使用？

1. **了解架构** → [架构总览](./architecture/architecture-overview.md)
2. **选择协议** → [API 规范](./api-specs/api-specifications.md)  
3. **配置 Provider** → [配置指南](./configuration/configuration-guide.md)
4. **验证集成** → [测试验证](./testing/validation-experiments.md)

### 要实现自建代理？

1. **理解 API 规范** → [API 协议定义](./api-specs/api-specifications.md)
2. **参考完整实现** → [Node.js 实现方案](./implementation/node-proxy-implementation.md)
3. **了解事件映射** → [事件映射规范](./api-specs/event-mapping.md)
4. **运行验证实验** → [验证实验](./testing/validation-experiments.md)

### 要集成工具系统？

1. **了解工具类型** → [工具集成指南](./tools/tools-integration.md)
2. **理解执行生命周期** → [工具系统详解](./tools/tools-integration.md#工具调用执行生命周期)
3. **配置安全策略** → [安全控制](./tools/tools-integration.md#安全与权限控制)

## 🎯 使用场景

### 场景一：接入现有 LLM 服务

**适用于**：使用 OpenAI、Azure、Claude 等现有服务

**推荐路径**：
1. [配置指南](./configuration/configuration-guide.md) - 设置 Provider
2. [API 规范](./api-specs/api-specifications.md) - 了解协议差异
3. [测试验证](./testing/validation-experiments.md) - 验证集成

**配置示例**：
```toml
model = "gpt-4o"
model_provider = "openai"

[model_providers.openai]
name = "OpenAI"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
wire_api = "chat"
```

### 场景二：构建自建代理服务

**适用于**：需要自建 LLM 代理，聚合多个上游服务

**推荐路径**：
1. [架构总览](./architecture/architecture-overview.md) - 理解整体设计
2. [Node.js 实现](./implementation/node-proxy-implementation.md) - 参考完整实现
3. [事件映射](./api-specs/event-mapping.md) - 理解事件转换
4. [验证实验](./testing/validation-experiments.md) - 验证实现正确性

**技术栈**：Node.js + Express + TypeScript

### 场景三：本地模型部署

**适用于**：使用 Ollama、LM Studio 等本地模型服务

**推荐路径**：
1. [配置指南](./configuration/configuration-guide.md) - Ollama 配置示例
2. [API 规范](./api-specs/api-specifications.md) - Chat API 兼容性

**配置示例**：
```toml
model = "llama3"
model_provider = "ollama"

[model_providers.ollama]
name = "Ollama Local"
base_url = "http://localhost:11434/v1"
wire_api = "chat"
```

### 场景四：企业级集成

**适用于**：企业环境，需要安全控制、监控、多环境管理

**推荐路径**：
1. [架构总览](./architecture/architecture-overview.md) - 可观测性设计
2. [配置指南](./configuration/configuration-guide.md) - Profile 多环境管理
3. [工具集成](./tools/tools-integration.md) - 安全与权限控制
4. [Node.js 实现](./implementation/node-proxy-implementation.md) - 生产环境部署

## 🔄 协议支持

### Chat Completions API（推荐）

- ✅ **兼容性最佳**：支持 OpenAI、Azure、Ollama 等主流服务
- ✅ **生态丰富**：大部分工具和库都支持
- ❌ **功能受限**：仅支持 function 工具，无推理事件

**适用场景**：快速集成、兼容性要求高

### Responses API（高级）

- ✅ **功能丰富**：支持推理、多种工具类型、细粒度事件
- ✅ **扩展性强**：便于自定义工具和事件类型
- ❌ **支持有限**：主要是 OpenAI 原生支持

**适用场景**：需要高级功能、自建代理场景

## 🛠️ 核心组件

### 1. 事件系统

统一的事件抽象，将不同协议的 SSE 事件标准化：

```rust
pub enum ResponseEvent {
    Created,
    OutputTextDelta(String),
    ReasoningDelta(String), 
    OutputItemDone(ResponseItem),
    Completed { response_id: String, token_usage: Option<TokenUsage> },
    WebSearchCallBegin { call_id: String },
}
```

**详细了解** → [事件映射规范](./api-specs/event-mapping.md)

### 2. 工具系统

支持多种工具类型的统一调用框架：

- **function**: 标准函数调用
- **local_shell**: 本地命令执行
- **web_search**: 网络搜索
- **view_image**: 图像分析
- **MCP 工具**: 通过 MCP 协议集成的第三方工具

**详细了解** → [工具集成指南](./tools/tools-integration.md)

### 3. Provider 系统

可插拔的 LLM 服务提供商抽象：

```toml
[model_providers.my-service]
name = "My Service"
base_url = "https://api.my-service.com/v1"
wire_api = "chat"  # 或 "responses"
env_key = "MY_SERVICE_API_KEY"
```

**详细了解** → [配置指南](./configuration/configuration-guide.md)

## 🧪 验证与测试

### 核心验证实验

我们设计了 3 个关键实验来验证集成方案的可靠性：

| 实验 | 目标 | 风险点 |
|------|------|--------|
| **E1** | Chat→Responses 事件合成 | 事件顺序错误导致流程中断 |
| **E2** | 工具调用分片聚合 | 参数拼接错误或并发污染 |
| **E3** | 错误透传与退避协同 | 缺失重试头导致退避失效 |

### 运行验证

```bash
# 克隆仓库并安装依赖
git clone <repository>
cd codex-llm-integration

# 运行完整验证套件
./test/full-integration.sh

# 单独运行某个实验
npx tsx test/experiment-e1.ts
```

**详细了解** → [验证实验](./testing/validation-experiments.md)

## 📈 性能特性

### 延迟优化
- **首字节时间** < 500ms
- **流式响应** 实时输出，无缓冲延迟
- **连接复用** 减少建连开销

### 并发支持
- **多路复用** 单进程支持 1000+ 并发连接
- **背压控制** 防止内存溢出
- **优雅降级** 负载过高时的处理策略

### 可靠性保障
- **自动重试** 请求级和流级重试机制
- **断路器** 防止级联故障
- **健康检查** 上游服务状态监控

## 🔒 安全考虑

### 输入验证
- **Schema 验证** 严格的 JSON Schema 参数验证
- **长度限制** 防止 DoS 攻击
- **注入防护** SQL/命令注入防护

### 权限控制
```rust
pub enum ApprovalPolicy {
    Auto,        // 自动执行
    Interactive, // 需要确认
    Disabled,    // 禁用工具
}
```

### 数据安全
- **敏感信息脱敏** 日志和监控中的敏感数据处理
- **传输加密** HTTPS/WSS 传输
- **访问控制** 基于角色的权限管理

## 🤝 贡献指南

### 文档改进

发现文档问题或有改进建议：

1. 在相应的 `.md` 文件中直接编辑
2. 遵循现有的文档结构和风格
3. 确保代码示例可以正常运行
4. 提交 Pull Request

### 新功能请求

需要新功能或有疑问：

1. 查看[现有文档](./README.md)确认未覆盖
2. 在 Issues 中描述需求场景
3. 参考现有结构提供设计建议

## 📞 获取帮助

### 常见问题

**Q: Chat API 和 Responses API 如何选择？**
A: 优先选择 Chat API（兼容性更好），需要高级功能时选择 Responses API。参考 [API 规范](./api-specs/api-specifications.md)。

**Q: 如何处理工具调用失败？**
A: 参考 [工具集成指南](./tools/tools-integration.md#错误处理与调试) 中的错误处理策略。

**Q: 自建代理的最小实现是什么？**
A: 参考 [Node.js 实现方案](./implementation/node-proxy-implementation.md) 中的最小实现示例。

### 更多资源

- **架构问题** → [架构总览](./architecture/architecture-overview.md)
- **配置问题** → [配置指南](./configuration/configuration-guide.md)  
- **实现问题** → [Node.js 实现](./implementation/node-proxy-implementation.md)
- **测试问题** → [验证实验](./testing/validation-experiments.md)

---

🎉 **祝你使用愉快！** 如有任何问题，请参考对应的详细文档或提交 Issue。