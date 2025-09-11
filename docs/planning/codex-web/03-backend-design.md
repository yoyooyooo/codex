# 后端详细设计

**文档版本**: v1.0  
**最后更新**: 2025-09-11  
**依赖文档**: [02-architecture.md](02-architecture.md)  
**后续文档**: [06-api-specification.md](06-api-specification.md), [07-security-design.md](07-security-design.md)

## 目录
- [Crate 结构设计](#crate-结构设计)
- [核心数据结构](#核心数据结构)
- [服务架构](#服务架构)
- [会话管理系统](#会话管理系统)
- [事件处理机制](#事件处理机制)
- [API 路由实现](#api-路由实现)
- [WebSocket 处理](#websocket-处理)
- [错误处理系统](#错误处理系统)
- [资源管理策略](#资源管理策略)
- [配置和启动](#配置和启动)
- [集成现有组件](#集成现有组件)
- [性能优化设计](#性能优化设计)

## Crate 结构设计

### 项目结构
```
codex-rs/web/
├── Cargo.toml
├── src/
│   ├── lib.rs                 # Crate 入口
│   ├── server.rs              # Web 服务器
│   ├── session/               # 会话管理模块
│   │   ├── mod.rs
│   │   ├── manager.rs         # SessionRegistry
│   │   ├── session.rs         # Session 结构
│   │   └── cleanup.rs         # 资源清理
│   ├── handlers/              # HTTP 处理器
│   │   ├── mod.rs
│   │   ├── api.rs             # REST API
│   │   ├── websocket.rs       # WebSocket 处理
│   │   └── static_files.rs    # 静态文件服务
│   ├── events/                # 事件处理
│   │   ├── mod.rs
│   │   ├── broadcaster.rs     # 事件广播
│   │   └── cache.rs           # 事件缓存
│   ├── config.rs              # Web 专用配置
│   ├── error.rs               # 错误定义
│   ├── auth.rs                # 认证中间件
│   └── utils.rs               # 工具函数
├── tests/                     # 集成测试
└── examples/                  # 示例代码
```

### Cargo.toml 配置
```toml
[package]
name = "codex-web"
version = "0.1.0"
edition = "2021"
description = "Codex Web Server Implementation"

[dependencies]
# Web 框架核心
axum = { version = "0.7", features = ["ws", "macros"] }
tokio = { version = "1.0", features = [
    "full", "rt-multi-thread", "macros", "signal"
] }
tokio-stream = "0.1"
tower = "0.4"
tower-http = { version = "0.5", features = [
    "cors", "trace", "compression", "fs"
] }

# 序列化和 JSON
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# 异步和并发
futures = "0.3"
parking_lot = "0.12"
dashmap = "5.5"
tokio-util = "0.7"

# 错误处理和日志
anyhow = "1.0"
thiserror = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# 工具类
uuid = { version = "1.0", features = ["v4", "serde"] }
rand = "0.8"
include_dir = { version = "0.7", optional = true }

# HTTP 客户端（用于健康检查等）
reqwest = { version = "0.11", features = ["json"] }

# 复用现有能力
codex-core = { path = "../core" }
codex-common = { path = "../common" }
codex-protocol = { path = "../protocol" }
codex-exec = { path = "../exec" }

[dev-dependencies]
tokio-test = "0.4"
tempfile = "3.0"
criterion = "0.5"

[features]
default = ["embed-assets"]
embed-assets = ["include_dir"]
dev-mode = []

[[bench]]
name = "session_management"
harness = false
```

## 核心数据结构

### 会话数据结构
```rust
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, broadcast, mpsc};
use uuid::Uuid;
use codex_core::{Conversation, ConversationManager};
use codex_protocol::{Event, Submission};

/// 会话 ID 类型定义
#[derive(Debug, Clone, Hash, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionId(Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// WebSocket 连接信息
#[derive(Debug)]
pub struct WebSocketConnection {
    pub id: Uuid,
    pub sender: mpsc::UnboundedSender<WebSocketMessage>,
    pub connected_at: Instant,
    pub last_pong: Arc<parking_lot::Mutex<Instant>>,
}

/// 会话状态
#[derive(Debug)]
pub struct Session {
    /// 会话唯一标识
    pub id: SessionId,
    
    /// 底层对话实例
    conversation: Arc<Conversation>,
    
    /// WebSocket 连接池
    connections: Arc<RwLock<Vec<WebSocketConnection>>>,
    
    /// 事件缓存（用于重连恢复）
    event_cache: Arc<parking_lot::Mutex<EventCache>>,
    
    /// 会话配置
    config: SessionConfig,
    
    /// 创建时间
    created_at: Instant,
    
    /// 最后活跃时间
    last_activity: Arc<parking_lot::Mutex<Instant>>,
    
    /// 关闭通知通道
    shutdown_tx: Option<broadcast::Sender<()>>,
}

/// 事件缓存结构
#[derive(Debug)]
pub struct EventCache {
    events: std::collections::VecDeque<(u64, Event)>,
    next_id: u64,
    max_size: usize,
}

impl EventCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            events: std::collections::VecDeque::new(),
            next_id: 0,
            max_size,
        }
    }
    
    pub fn push_event(&mut self, event: Event) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        
        self.events.push_back((id, event));
        
        // 保持缓存大小限制
        while self.events.len() > self.max_size {
            self.events.pop_front();
        }
        
        id
    }
    
    pub fn get_events_since(&self, since_id: u64) -> Vec<(u64, Event)> {
        self.events
            .iter()
            .filter(|(id, _)| *id > since_id)
            .cloned()
            .collect()
    }
}

/// 会话配置
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SessionConfig {
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub oss: Option<bool>,
    pub web_search: Option<bool>,
    pub config_profile: Option<String>,
    pub timeout_minutes: Option<u64>,
}
```

### 会话注册表
```rust
use dashmap::DashMap;
use std::sync::Arc;
use tokio::time::{interval, Duration};

/// 全局会话注册表
#[derive(Debug)]
pub struct SessionRegistry {
    /// 活跃会话映射
    sessions: DashMap<SessionId, Arc<Session>>,
    
    /// 对话管理器
    conversation_manager: Arc<ConversationManager>,
    
    /// 配置
    config: WebServerConfig,
    
    /// 清理任务句柄
    cleanup_task: Option<tokio::task::JoinHandle<()>>,
}

impl SessionRegistry {
    pub fn new(
        conversation_manager: Arc<ConversationManager>,
        config: WebServerConfig,
    ) -> Arc<Self> {
        let registry = Arc::new(Self {
            sessions: DashMap::new(),
            conversation_manager,
            config,
            cleanup_task: None,
        });
        
        // 启动清理任务
        let weak_registry = Arc::downgrade(&registry);
        let cleanup_interval = Duration::from_secs(60); // 每分钟清理一次
        
        let cleanup_task = tokio::spawn(async move {
            let mut interval = interval(cleanup_interval);
            
            loop {
                interval.tick().await;
                
                if let Some(registry) = weak_registry.upgrade() {
                    registry.cleanup_inactive_sessions().await;
                } else {
                    break; // Registry 已被释放，退出清理任务
                }
            }
        });
        
        // 注意：实际实现中需要存储 cleanup_task
        registry
    }
    
    /// 创建新会话
    pub async fn create_session(
        &self,
        config: SessionConfig,
    ) -> Result<Arc<Session>, WebError> {
        let session_id = SessionId::new();
        
        // 应用配置覆盖
        let cli_overrides = self.config_to_cli_overrides(&config);
        
        // 创建对话实例
        let conversation = self.conversation_manager
            .new_conversation(cli_overrides)
            .await
            .map_err(WebError::ConversationError)?;
        
        // 创建关闭通知通道
        let (shutdown_tx, _) = broadcast::channel(1);
        
        let session = Arc::new(Session {
            id: session_id.clone(),
            conversation: Arc::new(conversation),
            connections: Arc::new(RwLock::new(Vec::new())),
            event_cache: Arc::new(parking_lot::Mutex::new(
                EventCache::new(1000) // 最多缓存 1000 个事件
            )),
            config,
            created_at: Instant::now(),
            last_activity: Arc::new(parking_lot::Mutex::new(Instant::now())),
            shutdown_tx: Some(shutdown_tx),
        });
        
        // 启动事件监听任务
        self.start_event_listener(&session).await?;
        
        // 注册会话
        self.sessions.insert(session_id, session.clone());
        
        Ok(session)
    }
    
    /// 获取会话
    pub fn get_session(&self, session_id: &SessionId) -> Option<Arc<Session>> {
        self.sessions.get(session_id).map(|entry| entry.clone())
    }
    
    /// 清理非活跃会话
    async fn cleanup_inactive_sessions(&self) {
        let timeout = Duration::from_secs(
            self.config.session_timeout_seconds.unwrap_or(3600) // 默认 1 小时
        );
        let now = Instant::now();
        
        let inactive_sessions: Vec<SessionId> = self.sessions
            .iter()
            .filter_map(|entry| {
                let session = entry.value();
                let last_activity = *session.last_activity.lock();
                
                if now.duration_since(last_activity) > timeout {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect();
        
        for session_id in inactive_sessions {
            if let Some((_, session)) = self.sessions.remove(&session_id) {
                tracing::info!("清理非活跃会话: {:?}", session_id);
                // 发送关闭信号
                if let Some(shutdown_tx) = &session.shutdown_tx {
                    let _ = shutdown_tx.send(());
                }
            }
        }
    }
    
    /// 启动事件监听任务
    async fn start_event_listener(
        &self,
        session: &Arc<Session>,
    ) -> Result<(), WebError> {
        let session_weak = Arc::downgrade(session);
        let conversation = session.conversation.clone();
        
        tokio::spawn(async move {
            loop {
                // 检查会话是否仍然存在
                let session = match session_weak.upgrade() {
                    Some(s) => s,
                    None => break, // 会话已释放，退出循环
                };
                
                // 监听下一个事件
                match conversation.next_event().await {
                    Ok(event) => {
                        session.handle_event(event).await;
                    }
                    Err(e) => {
                        tracing::error!("事件监听错误: {:?}", e);
                        break;
                    }
                }
            }
        });
        
        Ok(())
    }
    
    /// 配置转换为 CLI 覆盖
    fn config_to_cli_overrides(&self, config: &SessionConfig) -> CliConfigOverrides {
        CliConfigOverrides {
            cwd: config.cwd.clone(),
            model: config.model.clone(),
            oss: config.oss,
            web_search: config.web_search,
            profile: config.config_profile.clone(),
            ..Default::default()
        }
    }
}
```

## 服务架构

### 主服务器结构
```rust
use axum::{
    Router, Extension, 
    routing::{get, post},
    middleware,
};
use tower_http::{
    cors::{CorsLayer, Any},
    trace::TraceLayer,
    compression::CompressionLayer,
};
use std::net::SocketAddr;

/// Web 服务器配置
#[derive(Debug, Clone)]
pub struct WebServerConfig {
    pub host: String,
    pub port: Option<u16>,
    pub access_token: String,
    pub static_dir: Option<String>,
    pub dev_proxy_url: Option<String>,
    pub session_timeout_seconds: Option<u64>,
    pub max_connections_per_session: usize,
    pub event_cache_size: usize,
}

impl Default for WebServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: None, // 随机端口
            access_token: generate_access_token(),
            static_dir: None,
            dev_proxy_url: None,
            session_timeout_seconds: Some(3600), // 1 小时
            max_connections_per_session: 10,
            event_cache_size: 1000,
        }
    }
}

/// Web 服务器主结构
pub struct WebServer {
    config: WebServerConfig,
    session_registry: Arc<SessionRegistry>,
    conversation_manager: Arc<ConversationManager>,
}

impl WebServer {
    pub fn new(
        config: WebServerConfig,
        conversation_manager: Arc<ConversationManager>,
    ) -> Self {
        let session_registry = SessionRegistry::new(
            conversation_manager.clone(),
            config.clone(),
        );
        
        Self {
            config,
            session_registry,
            conversation_manager,
        }
    }
    
    /// 构建路由
    fn build_router(&self) -> Router {
        let api_routes = Router::new()
            .route("/sessions", post(create_session))
            .route("/sessions/:id/events", get(handle_websocket))
            .route("/sessions/:id/submit", post(submit_to_session))
            .route("/sessions/:id/apply_patch", post(apply_patch))
            .route("/sessions/:id/history", get(get_session_history))
            .route("/login", post(handle_login))
            .route("/login/status", get(get_login_status))
            .route("/logout", post(handle_logout))
            .route("/health", get(health_check));
        
        let mut app = Router::new()
            .nest("/api", api_routes)
            .layer(Extension(self.session_registry.clone()))
            .layer(Extension(self.config.clone()))
            .layer(middleware::from_fn(auth_middleware))
            .layer(TraceLayer::new_for_http())
            .layer(CompressionLayer::new());
        
        // 添加 CORS（仅本地）
        if self.config.host == "127.0.0.1" || self.config.host == "localhost" {
            app = app.layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any)
            );
        }
        
        // 添加静态文件服务
        app = self.add_static_file_service(app);
        
        app
    }
    
    /// 添加静态文件服务
    fn add_static_file_service(&self, router: Router) -> Router {
        if let Some(static_dir) = &self.config.static_dir {
            // 开发模式：从文件系统服务
            router.fallback_service(
                tower::service_fn(|req| {
                    tower_http::services::ServeDir::new(static_dir)
                        .call(req)
                })
            )
        } else if cfg!(feature = "embed-assets") {
            // 生产模式：嵌入资源
            router.fallback_service(
                tower::service_fn(|req| {
                    serve_embedded_assets(req)
                })
            )
        } else {
            // 代理模式（开发时）
            if let Some(proxy_url) = &self.config.dev_proxy_url {
                router.fallback_service(
                    tower::service_fn(|req| {
                        proxy_to_dev_server(req, proxy_url.clone())
                    })
                )
            } else {
                router
            }
        }
    }
    
    /// 启动服务器
    pub async fn start(self) -> Result<SocketAddr, WebError> {
        let router = self.build_router();
        
        // 绑定地址
        let addr = if let Some(port) = self.config.port {
            format!("{}:{}", self.config.host, port)
        } else {
            format!("{}:0", self.config.host) // 随机端口
        };
        
        let listener = tokio::net::TcpListener::bind(&addr).await
            .map_err(|e| WebError::ServerStartup(format!("无法绑定地址 {}: {}", addr, e)))?;
        
        let actual_addr = listener.local_addr()
            .map_err(|e| WebError::ServerStartup(format!("无法获取本地地址: {}", e)))?;
        
        tracing::info!("Web 服务器启动在: http://{}", actual_addr);
        tracing::info!("访问令牌: {}", self.config.access_token);
        
        // 启动服务器
        axum::serve(listener, router).await
            .map_err(|e| WebError::ServerStartup(format!("服务器启动失败: {}", e)))?;
        
        Ok(actual_addr)
    }
}

/// 生成访问令牌
fn generate_access_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| {
            let idx = rng.gen_range(0..62);
            match idx {
                0..=9 => (b'0' + idx) as char,
                10..=35 => (b'A' + idx - 10) as char,
                36..=61 => (b'a' + idx - 36) as char,
                _ => unreachable!(),
            }
        })
        .collect()
}
```

## 会话管理系统

### 会话实现详情
```rust
impl Session {
    /// 处理事件
    pub async fn handle_event(&self, event: Event) {
        // 更新活跃时间
        *self.last_activity.lock() = Instant::now();
        
        // 添加到事件缓存
        let event_id = self.event_cache.lock().push_event(event.clone());
        
        // 广播给所有连接
        self.broadcast_to_connections(WebSocketMessage::Event {
            id: event_id,
            event,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }).await;
    }
    
    /// 处理提交
    pub async fn handle_submission(
        &self,
        submission: Submission,
    ) -> Result<(), WebError> {
        // 更新活跃时间
        *self.last_activity.lock() = Instant::now();
        
        // 提交给对话管理器
        self.conversation
            .handle_submission(submission)
            .await
            .map_err(WebError::ConversationError)?;
        
        Ok(())
    }
    
    /// 添加 WebSocket 连接
    pub async fn add_connection(
        &self,
        connection: WebSocketConnection,
        since_event_id: Option<u64>,
    ) -> Result<(), WebError> {
        let mut connections = self.connections.write().await;
        
        // 检查连接数限制
        if connections.len() >= 10 { // 配置化
            return Err(WebError::TooManyConnections);
        }
        
        // 发送历史事件（如果需要）
        if let Some(since_id) = since_event_id {
            let cached_events = self.event_cache.lock().get_events_since(since_id);
            for (id, event) in cached_events {
                let message = WebSocketMessage::Event {
                    id,
                    event,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                };
                let _ = connection.sender.send(message);
            }
        }
        
        connections.push(connection);
        Ok(())
    }
    
    /// 移除 WebSocket 连接
    pub async fn remove_connection(&self, connection_id: Uuid) {
        let mut connections = self.connections.write().await;
        connections.retain(|conn| conn.id != connection_id);
    }
    
    /// 广播消息给所有连接
    async fn broadcast_to_connections(&self, message: WebSocketMessage) {
        let connections = self.connections.read().await;
        let mut failed_connections = Vec::new();
        
        for (idx, connection) in connections.iter().enumerate() {
            if let Err(_) = connection.sender.send(message.clone()) {
                failed_connections.push(idx);
            }
        }
        
        // 清理失败的连接（需要异步处理以避免死锁）
        if !failed_connections.is_empty() {
            let connections_clone = self.connections.clone();
            tokio::spawn(async move {
                let mut connections = connections_clone.write().await;
                // 从后往前删除，避免索引偏移
                for &idx in failed_connections.iter().rev() {
                    if idx < connections.len() {
                        connections.remove(idx);
                    }
                }
            });
        }
    }
    
    /// 应用补丁
    pub async fn apply_current_patch(&self) -> Result<(), WebError> {
        // 调用现有的补丁应用逻辑
        codex_exec::apply_patch::apply_current_patch(
            &self.conversation
        ).await
        .map_err(WebError::PatchApplicationFailed)?;
        
        Ok(())
    }
    
    /// 获取会话统计信息
    pub async fn get_stats(&self) -> SessionStats {
        let connections = self.connections.read().await;
        let event_count = self.event_cache.lock().events.len();
        
        SessionStats {
            session_id: self.id.clone(),
            connection_count: connections.len(),
            event_count,
            created_at: self.created_at,
            last_activity: *self.last_activity.lock(),
            uptime: self.created_at.elapsed(),
        }
    }
}

/// 会话统计信息
#[derive(Debug, serde::Serialize)]
pub struct SessionStats {
    pub session_id: SessionId,
    pub connection_count: usize,
    pub event_count: usize,
    pub created_at: Instant,
    pub last_activity: Instant,
    pub uptime: Duration,
}
```

## 事件处理机制

### WebSocket 消息定义
```rust
/// WebSocket 消息类型
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum WebSocketMessage {
    /// 服务端事件
    Event {
        id: u64,
        event: Event,
        timestamp: i64,
    },
    
    /// 客户端提交
    Submission {
        id: String,
        submission: Submission,
        timestamp: i64,
    },
    
    /// 心跳消息
    Heartbeat {
        timestamp: i64,
    },
    
    /// 错误消息
    Error {
        code: String,
        message: String,
        details: Option<serde_json::Value>,
    },
    
    /// 连接确认
    ConnectionAck {
        session_id: SessionId,
        server_time: i64,
    },
}
```

### 事件广播器
```rust
/// 事件广播器
#[derive(Debug)]
pub struct EventBroadcaster {
    /// 订阅者列表
    subscribers: Arc<RwLock<Vec<broadcast::Sender<Event>>>>,
    
    /// 事件计数器
    event_counter: Arc<AtomicU64>,
}

impl EventBroadcaster {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(Vec::new())),
            event_counter: Arc::new(AtomicU64::new(0)),
        }
    }
    
    /// 订阅事件
    pub async fn subscribe(&self) -> broadcast::Receiver<Event> {
        let (tx, rx) = broadcast::channel(1000);
        let mut subscribers = self.subscribers.write().await;
        subscribers.push(tx);
        rx
    }
    
    /// 广播事件
    pub async fn broadcast(&self, event: Event) {
        let event_id = self.event_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let subscribers = self.subscribers.read().await;
        let mut failed_subscribers = Vec::new();
        
        for (idx, subscriber) in subscribers.iter().enumerate() {
            if let Err(_) = subscriber.send(event.clone()) {
                failed_subscribers.push(idx);
            }
        }
        
        // 异步清理失败的订阅者
        if !failed_subscribers.is_empty() {
            let subscribers_clone = self.subscribers.clone();
            tokio::spawn(async move {
                let mut subscribers = subscribers_clone.write().await;
                for &idx in failed_subscribers.iter().rev() {
                    if idx < subscribers.len() {
                        subscribers.remove(idx);
                    }
                }
            });
        }
    }
    
    /// 获取订阅者数量
    pub async fn subscriber_count(&self) -> usize {
        self.subscribers.read().await.len()
    }
}
```

## API 路由实现

### 主要 API 处理器
```rust
use axum::{
    extract::{Path, Query, WebSocketUpgrade, Extension},
    response::{Json, Response},
    http::StatusCode,
};

/// 创建会话
pub async fn create_session(
    Extension(registry): Extension<Arc<SessionRegistry>>,
    Extension(config): Extension<WebServerConfig>,
    Json(session_config): Json<SessionConfig>,
) -> Result<Json<CreateSessionResponse>, WebError> {
    let session = registry.create_session(session_config).await?;
    
    let response = CreateSessionResponse {
        session_id: session.id.clone(),
        ws_url: format!("ws://{}:{}/api/sessions/{}/events", 
                       config.host, 
                       config.port.unwrap_or(8080),  // 实际应该用真实端口
                       session.id.0),
        http_url: format!("http://{}:{}/api/sessions/{}", 
                         config.host,
                         config.port.unwrap_or(8080),
                         session.id.0),
    };
    
    Ok(Json(response))
}

#[derive(serde::Serialize)]
pub struct CreateSessionResponse {
    pub session_id: SessionId,
    pub ws_url: String,
    pub http_url: String,
}

/// WebSocket 处理
pub async fn handle_websocket(
    ws: WebSocketUpgrade,
    Extension(registry): Extension<Arc<SessionRegistry>>,
    Path(session_id): Path<SessionId>,
    Query(params): Query<WebSocketParams>,
) -> Result<Response, WebError> {
    let session = registry
        .get_session(&session_id)
        .ok_or(WebError::SessionNotFound(session_id.clone()))?;
    
    Ok(ws.on_upgrade(move |socket| {
        handle_websocket_connection(socket, session, params.since_event_id)
    }))
}

#[derive(serde::Deserialize)]
pub struct WebSocketParams {
    pub since_event_id: Option<u64>,
}

/// 提交到会话
pub async fn submit_to_session(
    Extension(registry): Extension<Arc<SessionRegistry>>,
    Path(session_id): Path<SessionId>,
    Json(submission): Json<Submission>,
) -> Result<Json<SubmitResponse>, WebError> {
    let session = registry
        .get_session(&session_id)
        .ok_or(WebError::SessionNotFound(session_id))?;
    
    session.handle_submission(submission).await?;
    
    Ok(Json(SubmitResponse {
        status: "submitted".to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
    }))
}

#[derive(serde::Serialize)]
pub struct SubmitResponse {
    pub status: String,
    pub timestamp: i64,
}

/// 应用补丁
pub async fn apply_patch(
    Extension(registry): Extension<Arc<SessionRegistry>>,
    Path(session_id): Path<SessionId>,
) -> Result<Json<ApplyPatchResponse>, WebError> {
    let session = registry
        .get_session(&session_id)
        .ok_or(WebError::SessionNotFound(session_id))?;
    
    session.apply_current_patch().await?;
    
    Ok(Json(ApplyPatchResponse {
        status: "applied".to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
    }))
}

#[derive(serde::Serialize)]
pub struct ApplyPatchResponse {
    pub status: String,
    pub timestamp: i64,
}

/// 健康检查
pub async fn health_check(
    Extension(registry): Extension<Arc<SessionRegistry>>,
) -> Json<HealthResponse> {
    let session_count = registry.sessions.len();
    
    Json(HealthResponse {
        status: "healthy".to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        session_count,
        uptime: std::process::id(), // 简化实现
    })
}

#[derive(serde::Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: i64,
    pub session_count: usize,
    pub uptime: u32,
}
```

## WebSocket 处理

### WebSocket 连接处理
```rust
use axum::extract::ws::{WebSocket, Message};
use futures::{SinkExt, StreamExt};
use tokio::time::{interval, Duration};

/// 处理 WebSocket 连接
pub async fn handle_websocket_connection(
    socket: WebSocket,
    session: Arc<Session>,
    since_event_id: Option<u64>,
) {
    let (mut sender, mut receiver) = socket.split();
    let connection_id = Uuid::new_v4();
    
    // 创建消息通道
    let (tx, mut rx) = mpsc::unbounded_channel::<WebSocketMessage>();
    
    // 创建连接对象
    let connection = WebSocketConnection {
        id: connection_id,
        sender: tx.clone(),
        connected_at: Instant::now(),
        last_pong: Arc::new(parking_lot::Mutex::new(Instant::now())),
    };
    
    // 添加到会话
    if let Err(e) = session.add_connection(connection, since_event_id).await {
        tracing::error!("添加 WebSocket 连接失败: {:?}", e);
        return;
    }
    
    // 发送连接确认
    let ack_message = WebSocketMessage::ConnectionAck {
        session_id: session.id.clone(),
        server_time: chrono::Utc::now().timestamp_millis(),
    };
    let _ = tx.send(ack_message);
    
    // 启动心跳任务
    let heartbeat_tx = tx.clone();
    let heartbeat_task = tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(30));
        
        loop {
            interval.tick().await;
            let heartbeat = WebSocketMessage::Heartbeat {
                timestamp: chrono::Utc::now().timestamp_millis(),
            };
            
            if heartbeat_tx.send(heartbeat).is_err() {
                break; // 连接已关闭
            }
        }
    });
    
    // 消息发送任务
    let send_task = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            let text = match serde_json::to_string(&message) {
                Ok(text) => text,
                Err(e) => {
                    tracing::error!("序列化 WebSocket 消息失败: {:?}", e);
                    continue;
                }
            };
            
            if sender.send(Message::Text(text)).await.is_err() {
                break; // 发送失败，连接已断开
            }
        }
    });
    
    // 消息接收任务
    let receive_task = {
        let session = session.clone();
        let tx = tx.clone();
        
        tokio::spawn(async move {
            while let Some(msg) = receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Err(e) = handle_websocket_message(&session, text).await {
                            tracing::error!("处理 WebSocket 消息失败: {:?}", e);
                            let error_msg = WebSocketMessage::Error {
                                code: "message_processing_error".to_string(),
                                message: e.to_string(),
                                details: None,
                            };
                            let _ = tx.send(error_msg);
                        }
                    }
                    Ok(Message::Pong(_)) => {
                        // 更新最后 pong 时间
                        // *connection.last_pong.lock() = Instant::now();
                    }
                    Ok(Message::Close(_)) => {
                        tracing::info!("WebSocket 连接关闭: {}", connection_id);
                        break;
                    }
                    Err(e) => {
                        tracing::error!("WebSocket 错误: {:?}", e);
                        break;
                    }
                    _ => {
                        // 忽略其他消息类型
                    }
                }
            }
        })
    };
    
    // 等待任务完成
    tokio::select! {
        _ = send_task => {},
        _ = receive_task => {},
        _ = heartbeat_task => {},
    }
    
    // 清理连接
    session.remove_connection(connection_id).await;
    tracing::info!("WebSocket 连接 {} 已清理", connection_id);
}

/// 处理 WebSocket 消息
async fn handle_websocket_message(
    session: &Session,
    message: String,
) -> Result<(), WebError> {
    let ws_message: WebSocketMessage = serde_json::from_str(&message)
        .map_err(|e| WebError::InvalidMessage(format!("JSON 解析错误: {}", e)))?;
    
    match ws_message {
        WebSocketMessage::Submission { submission, .. } => {
            session.handle_submission(submission).await?;
        }
        WebSocketMessage::Heartbeat { .. } => {
            // 心跳消息，无需处理
        }
        _ => {
            return Err(WebError::InvalidMessage(
                "客户端不应发送此类型消息".to_string()
            ));
        }
    }
    
    Ok(())
}
```

## 错误处理系统

### 错误类型定义
```rust
use axum::{
    response::{IntoResponse, Response},
    http::StatusCode,
    Json,
};

/// Web 服务错误类型
#[derive(Debug, thiserror::Error)]
pub enum WebError {
    #[error("会话未找到: {0:?}")]
    SessionNotFound(SessionId),
    
    #[error("连接数过多")]
    TooManyConnections,
    
    #[error("认证失败: {0}")]
    AuthenticationFailed(String),
    
    #[error("无效的消息: {0}")]
    InvalidMessage(String),
    
    #[error("服务器启动失败: {0}")]
    ServerStartup(String),
    
    #[error("对话管理错误: {0}")]
    ConversationError(#[from] codex_core::Error),
    
    #[error("补丁应用失败: {0}")]
    PatchApplicationFailed(#[from] codex_exec::Error),
    
    #[error("配置错误: {0}")]
    ConfigError(String),
    
    #[error("内部服务器错误: {0}")]
    Internal(String),
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        let (status, error_message) = match &self {
            WebError::SessionNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            WebError::TooManyConnections => (StatusCode::TOO_MANY_REQUESTS, self.to_string()),
            WebError::AuthenticationFailed(_) => (StatusCode::UNAUTHORIZED, self.to_string()),
            WebError::InvalidMessage(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            WebError::ConfigError(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "内部服务器错误".to_string()),
        };
        
        let body = Json(ErrorResponse {
            error: error_message,
            code: format!("{:?}", self),
            timestamp: chrono::Utc::now().timestamp_millis(),
        });
        
        (status, body).into_response()
    }
}

#[derive(serde::Serialize)]
struct ErrorResponse {
    error: String,
    code: String,
    timestamp: i64,
}
```

## 资源管理策略

### 内存管理
```rust
/// 资源监控器
#[derive(Debug)]
pub struct ResourceMonitor {
    max_memory_mb: usize,
    max_sessions: usize,
    max_connections_per_session: usize,
}

impl ResourceMonitor {
    pub fn new(config: &WebServerConfig) -> Self {
        Self {
            max_memory_mb: 512, // 默认 512MB 限制
            max_sessions: 100,  // 最多 100 个会话
            max_connections_per_session: config.max_connections_per_session,
        }
    }
    
    /// 检查资源使用情况
    pub async fn check_resources(&self, registry: &SessionRegistry) -> ResourceStatus {
        let session_count = registry.sessions.len();
        let total_connections: usize = registry.sessions
            .iter()
            .map(|entry| {
                // 这里需要异步处理，实际实现需要调整
                0 // entry.value().connections.read().await.len()
            })
            .sum();
        
        ResourceStatus {
            session_count,
            total_connections,
            memory_usage_mb: self.get_memory_usage(),
            limits_exceeded: session_count > self.max_sessions,
        }
    }
    
    fn get_memory_usage(&self) -> usize {
        // 简化的内存使用估算
        // 实际实现可能需要更复杂的内存监控
        std::process::id() as usize % 100 // 占位实现
    }
}

#[derive(Debug)]
pub struct ResourceStatus {
    pub session_count: usize,
    pub total_connections: usize,
    pub memory_usage_mb: usize,
    pub limits_exceeded: bool,
}

/// 优雅关闭处理
pub struct GracefulShutdown {
    shutdown_tx: broadcast::Sender<()>,
}

impl GracefulShutdown {
    pub fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self { shutdown_tx }
    }
    
    pub async fn shutdown(&self, registry: &SessionRegistry) {
        tracing::info!("开始优雅关闭...");
        
        // 通知所有组件关闭
        let _ = self.shutdown_tx.send(());
        
        // 等待活跃会话完成
        let mut attempts = 0;
        while !registry.sessions.is_empty() && attempts < 30 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            attempts += 1;
            
            if attempts % 5 == 0 {
                tracing::info!("等待 {} 个会话关闭...", registry.sessions.len());
            }
        }
        
        // 强制清理剩余会话
        registry.sessions.clear();
        
        tracing::info!("优雅关闭完成");
    }
    
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }
}
```

## 集成现有组件

### 与 codex-core 集成
```rust
use codex_core::{ConversationManager, Config, CliConfigOverrides};

/// Web 服务集成层
pub struct CodexWebIntegration {
    config: Config,
    conversation_manager: Arc<ConversationManager>,
}

impl CodexWebIntegration {
    /// 从现有配置初始化
    pub async fn from_config(
        base_config: Config,
        web_overrides: WebConfigOverrides,
    ) -> Result<Self, WebError> {
        // 应用 Web 特定的配置覆盖
        let mut config = base_config;
        
        // 这里可以修改配置，但要保持与现有系统的兼容性
        if let Some(log_level) = web_overrides.log_level {
            // 调整日志级别等
        }
        
        let conversation_manager = ConversationManager::new(config.clone())
            .await
            .map_err(WebError::ConversationError)?;
        
        Ok(Self {
            config,
            conversation_manager: Arc::new(conversation_manager),
        })
    }
    
    /// 获取对话管理器
    pub fn conversation_manager(&self) -> Arc<ConversationManager> {
        self.conversation_manager.clone()
    }
    
    /// 检查沙箱状态
    pub fn check_sandbox_compliance(&self) -> Result<(), WebError> {
        // 确保我们没有违反任何沙箱约束
        let sandbox_vars: Vec<_> = std::env::vars()
            .filter(|(key, _)| key.starts_with("CODEX_SANDBOX_"))
            .collect();
        
        tracing::debug!("检查到 {} 个沙箱环境变量", sandbox_vars.len());
        
        // 这里可以添加更多的沙箱合规性检查
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct WebConfigOverrides {
    pub log_level: Option<String>,
    pub max_sessions: Option<usize>,
    pub session_timeout: Option<Duration>,
}
```

## 性能优化设计

### 连接池和缓存
```rust
/// 连接池管理器
#[derive(Debug)]
pub struct ConnectionPoolManager {
    pools: DashMap<SessionId, ConnectionPool>,
    max_pools: usize,
}

#[derive(Debug)]
struct ConnectionPool {
    connections: Vec<WebSocketConnection>,
    last_used: Instant,
}

impl ConnectionPoolManager {
    pub fn new(max_pools: usize) -> Self {
        Self {
            pools: DashMap::new(),
            max_pools,
        }
    }
    
    /// 优化连接分配
    pub fn allocate_connection(&self, session_id: &SessionId) -> Option<usize> {
        // 实现连接池逻辑
        None // 占位
    }
}

/// 性能监控
pub struct PerformanceMonitor {
    request_latencies: Arc<parking_lot::Mutex<Vec<Duration>>>,
    websocket_message_counts: Arc<AtomicUsize>,
    active_sessions: Arc<AtomicUsize>,
}

impl PerformanceMonitor {
    pub fn new() -> Self {
        Self {
            request_latencies: Arc::new(parking_lot::Mutex::new(Vec::new())),
            websocket_message_counts: Arc::new(AtomicUsize::new(0)),
            active_sessions: Arc::new(AtomicUsize::new(0)),
        }
    }
    
    pub fn record_request_latency(&self, latency: Duration) {
        let mut latencies = self.request_latencies.lock();
        latencies.push(latency);
        
        // 保持最近 1000 个记录
        if latencies.len() > 1000 {
            latencies.drain(0..100);
        }
    }
    
    pub fn get_stats(&self) -> PerformanceStats {
        let latencies = self.request_latencies.lock();
        let avg_latency = if !latencies.is_empty() {
            latencies.iter().sum::<Duration>() / latencies.len() as u32
        } else {
            Duration::ZERO
        };
        
        PerformanceStats {
            avg_request_latency: avg_latency,
            total_websocket_messages: self.websocket_message_counts.load(std::sync::atomic::Ordering::Relaxed),
            active_sessions: self.active_sessions.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct PerformanceStats {
    pub avg_request_latency: Duration,
    pub total_websocket_messages: usize,
    pub active_sessions: usize,
}
```

---

**变更记录**：
- v1.0 (2025-09-11): 初始版本，详细的后端设计文档