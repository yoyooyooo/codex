# WebSocket vs SSE 兼容性测试工具

这是一个专门为验证 WebSocket 和 Server-Sent Events (SSE) 在不同网络环境下兼容性的最小化测试工具。

## 快速开始

### 1. 启动测试服务器

```bash
# 安装依赖并启动 Rust 服务器
cargo run

# 服务器将在 http://localhost:3000 启动
```

### 2. 交互式测试

在浏览器中打开 http://localhost:3000，使用可视化界面进行测试：

- **连接测试**: 分别建立 WebSocket 和 SSE 连接
- **性能测试**: 对比两种协议的延迟和吞吐量
- **稳定性测试**: 测试连接中断和重连机制
- **环境切换**: 模拟不同网络环境（代理、防火墙等）

### 3. 自动化测试

```bash
# 安装 Node.js 依赖
npm install

# 运行自动化兼容性测试
npm test

# 指定服务器地址
npm run test:proxy
```

## 测试场景

### 网络环境
- ✅ **直连环境**: 基础功能验证
- ⚠️  **HTTP代理**: 企业环境兼容性
- ⚠️  **HTTPS代理**: 加密代理环境 
- ❌ **限制性防火墙**: 严格网络策略
- 🐌 **慢速网络**: 网络质量影响

### 测试指标
- 🔗 连接建立成功率
- ⏱️  连接建立时间
- 📨 消息传输延迟
- 🔄 自动重连成功率
- ❌ 错误和异常处理

## 核心功能

### Rust 服务器端
- 同时支持 WebSocket 和 SSE 协议
- 实时连接状态监控和统计
- 消息延迟测量
- 自动重连机制测试

### JavaScript 客户端
- 统一的协议抽象接口
- 实时性能指标显示
- 自动化批量测试
- 结果对比和分析

### 自动化测试脚本
- 多环境批量测试
- 代理服务器兼容性验证
- 详细的测试报告生成
- 技术选择建议

## 使用案例

### 场景 1: 基础兼容性验证
```bash
# 启动服务器
cargo run

# 在浏览器中测试基础功能
open http://localhost:3000
```

### 场景 2: 企业网络环境测试
```bash
# 配置企业代理后运行
HTTP_PROXY=http://proxy.company.com:8080 cargo run

# 运行自动化测试
node test-runner.js http://localhost:3000
```

### 场景 3: 持续集成测试
```bash
# 后台启动服务器
cargo run &
SERVER_PID=$!

# 运行测试套件
npm test

# 清理
kill $SERVER_PID
```

## 预期结果与决策标准

### ✅ WebSocket 优先方案
**触发条件:**
- WebSocket 连接成功率 > 85%
- 平均消息延迟 < 200ms
- 重连成功率 > 90%

**架构决策:** 仅实现 WebSocket，简化架构

### ⚠️  混合降级方案
**触发条件:**
- WebSocket 连接成功率 70-85%
- SSE 连接成功率 > 85%
- 存在明显的网络环境差异

**架构决策:** 实现 WebSocket + SSE 自动降级机制

### 🔄 SSE 优先方案
**触发条件:**
- WebSocket 连接成功率 < 70%
- SSE 连接成功率 > 85%

**架构决策:** 优先使用 SSE，考虑 WebSocket 作为可选增强

### ❌ 重新评估方案
**触发条件:**
- 两种协议连接成功率都 < 70%
- 存在严重的兼容性问题

**架构决策:** 考虑其他实时通信方案或优化网络配置

## 技术架构

### 服务器端 (Rust)
- **框架**: Axum + Tokio
- **WebSocket**: 原生 WebSocket 支持
- **SSE**: HTTP 流式响应
- **统计**: 实时连接和性能监控

### 客户端 (JavaScript)
- **WebSocket**: 原生 WebSocket API
- **SSE**: EventSource API
- **测试框架**: 自定义测试套件
- **UI**: 原生 HTML/CSS/JavaScript

### 测试工具 (Node.js)
- **WebSocket 客户端**: ws 库
- **SSE 客户端**: eventsource 库
- **代理支持**: http-proxy-agent, https-proxy-agent
- **报告生成**: 结构化 JSON 输出

## 实验价值

1. **技术风险评估**: 量化不同协议在实际环境中的兼容性风险
2. **架构复杂度权衡**: 基于数据决定是否需要实现降级机制
3. **性能基准建立**: 为后续优化提供基线数据
4. **企业部署信心**: 验证方案在目标环境下的可行性

这个最小验证实验能够在最短时间内为 Codex Web 项目的实时通信技术选择提供可靠的决策依据。