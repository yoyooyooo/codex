# API 接口规范

**文档版本**: v1.0  
**最后更新**: 2025-09-11  
**依赖文档**: [03-backend-design.md](03-backend-design.md)  

## REST API 端点

### 会话管理

#### 创建会话
```
POST /api/sessions
Content-Type: application/json
Authorization: Bearer {access_token}

Request Body:
{
  "cwd": "/path/to/project",
  "model": "claude-3-sonnet",
  "oss": false,
  "web_search": true,
  "config_profile": "default"
}

Response (201):
{
  "session_id": "550e8400-e29b-41d4-a716-446655440000",
  "ws_url": "ws://127.0.0.1:8080/api/sessions/550e8400-e29b-41d4-a716-446655440000/events",
  "http_url": "http://127.0.0.1:8080/api/sessions/550e8400-e29b-41d4-a716-446655440000"
}
```

#### 提交到会话
```
POST /api/sessions/{session_id}/submit
Content-Type: application/json

Request Body:
{
  "type": "user_message",
  "content": "请帮我实现一个排序函数",
  "metadata": {}
}

Response (200):
{
  "status": "submitted",
  "timestamp": 1642694400000
}
```

#### 应用补丁
```
POST /api/sessions/{session_id}/apply_patch

Response (200):
{
  "status": "applied", 
  "timestamp": 1642694400000
}
```

### WebSocket 协议

#### 连接
```
GET /api/sessions/{session_id}/events
Upgrade: websocket
Authorization: Bearer {access_token}
```

#### 消息格式
```typescript
// 服务端 -> 客户端
interface WebSocketMessage {
  type: 'event' | 'heartbeat' | 'connection_ack' | 'error';
  id?: number;
  timestamp: number;
  payload: Event | HeartbeatData | ConnectionAck | ErrorData;
}

// 客户端 -> 服务端  
interface ClientMessage {
  type: 'submission' | 'heartbeat';
  id: string;
  timestamp: number;
  payload: Submission | HeartbeatData;
}
```

### 认证端点
```
POST /api/login
GET /api/login/status  
POST /api/logout
```

### 健康检查
```
GET /api/health

Response:
{
  "status": "healthy",
  "timestamp": 1642694400000,
  "session_count": 3,
  "uptime": 3600
}
```

## 错误代码

| 状态码 | 错误类型 | 描述 |
|-------|---------|------|
| 400 | InvalidRequest | 请求格式错误 |
| 401 | Unauthorized | 访问令牌无效 |
| 404 | SessionNotFound | 会话不存在 |
| 429 | TooManyConnections | 连接数超限 |
| 500 | InternalError | 服务器内部错误 |

---
**变更记录**：
- v1.0 (2025-09-11): API 接口规范文档