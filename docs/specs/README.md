# Specs 索引

- [概览](./overview.md)：高层流程、模块与扩展点
- [请求与自建代理规范](./requests-and-proxy.md)：Wire API（Responses/Chat）、SSE、错误、最小代理实现
- [Node 代理实现方案](./proxy-node.md)：基于 Node/Express 的最小可用实现与进阶实践
- [不确定性最小实验](./experiments.md)：Top3 风险点与可执行的本地验证用例
- [Provider 配置规范](./provider-config.md)：`config.toml` Provider 字段与示例
- [工具体系与集成](./tools-and-integration.md)：工具 JSON、MCP 归一化、执行生命周期
- [流式事件映射](./stream-events.md)：SSE → `ResponseEvent` 映射与聚合规则

> 注：文档以“可替换/可扩展”为设计目标，彼此尽量独立，互相引用仅用于导航。
