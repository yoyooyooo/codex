# Codex LLM 集成完全指南

> Codex（Rust CLI）与 LLM 提供商统一集成系统的技术规范与实现指南

## 系统概述

本指南基于 Codex 原始技术规范文档整理而成，系统性地介绍了 Codex 如何与各种 LLM 提供商（OpenAI、Azure、Ollama、自建代理等）进行集成的完整技术体系。

**核心特性**：
- 🔄 **协议抽象**：支持 Responses API 与 Chat Completions API 双向转换
- 🚀 **实时流式**：完整的 SSE 事件流处理与映射
- 🛠️ **丰富工具**：集成 function、shell、MCP、web_search 等工具体系
- ⚙️ **配置驱动**：通过 config.toml 无代码接入新提供商
- 🏗️ **可扩展性**：模块化设计，易于扩展与定制

## 文档导航

按照**学习路径**组织，从概念理解到实践实现：

### 1. [架构概览](./01-architecture-overview.md)
- 📋 系统整体架构与设计原则
- 🔄 完整调用链路：用户输入 → LLM 流式响应  
- 🎯 关键模块与扩展点
- 💡 设计哲学与技术决策

### 2. [API 规范](./02-api-specifications.md)
- 🌐 Wire API 详解：Responses API vs Chat Completions API
- 📡 SSE 事件流规范与映射规则
- 🔐 鉴权、错误处理与重试策略
- 🔄 协议转换与兼容性处理

### 3. [配置指南](./03-configuration-guide.md)
- ⚙️ Provider 配置完全参考（config.toml）
- 🎛️ 模型选择与家族特性
- 🌍 环境变量与参数覆盖
- 📝 常见配置示例与最佳实践

### 4. [工具集成](./04-tools-integration.md)
- 🛠️ 工具类型与执行生命周期
- 🔧 Function、Local Shell、MCP 工具详解
- 📝 JSON Schema 子集与归一化
- 🔄 Chat ↔ Responses 工具映射

### 5. [实现指南](./05-implementation-guide.md)
- 🏗️ Node.js 代理实现方案
- 💻 最小可用实现与进阶优化
- 🌊 Chat → Responses 事件合成
- 🚀 部署与运维建议

### 6. [测试验证](./06-testing-validation.md)
- 🧪 Top 3 关键不确定性点
- ✅ 最小验证实验（MVP）
- 🔍 本地测试环境搭建
- 📊 验证标准与判定规则

## 快速开始

### 适用场景
- 🔌 需要集成/替换上游 LLM 提供方的工程师
- 🛠️ 需要理解工具调用映射规则的开发者  
- 🔧 需要调试流式响应与事件聚合的技术人员
- 🏗️ 需要构建自定义 LLM 代理的架构师

### 学习建议
1. **新手**：按顺序阅读，重点关注架构概览和配置指南
2. **有经验者**：可直接查阅 API 规范和实现指南  
3. **架构师**：重点关注架构概览、工具集成、测试验证

## 技术栈

**核心技术**：
- **语言**：Rust（Codex CLI）、TypeScript/Node.js（代理实现）
- **协议**：HTTP/SSE、OpenAI Responses API、Chat Completions API
- **工具**：MCP（Model Context Protocol）、JSON Schema
- **配置**：TOML 配置驱动

## 原始文档映射

本指南基于以下原始 specs 文档整理：

| 原始文档 | 对应章节 | 主要内容 |
|---------|---------|---------|
| `overview.md` | 架构概览 | 系统总体流程与模块 |
| `requests-and-proxy.md` | API 规范 | Wire API 与代理规范 |
| `stream-events.md` | API 规范 | SSE 事件映射 |
| `provider-config.md` | 配置指南 | Provider 配置规范 |
| `tools-and-integration.md` | 工具集成 | 工具体系与集成 |
| `proxy-node.md` | 实现指南 | Node 代理实现方案 |
| `experiments.md` | 测试验证 | 验证实验与测试 |

---

## 更新记录

- **v1.0** (2025-01): 基于原始 specs 文档整理完成
- 内容来源：`docs/specs/` 目录下的8个技术规范文档
- 整理原则：保持技术准确性，优化学习路径，增强实用性

## 贡献指南

本指南基于 Codex 项目的原始技术规范整理而成，如需更新或补充：
1. 参考原始 `docs/specs/` 目录下的最新文档
2. 遵循现有的章节组织结构
3. 保持技术内容的准确性和一致性