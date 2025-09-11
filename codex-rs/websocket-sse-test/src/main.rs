use axum::{
    extract::{ws::WebSocket, WebSocketUpgrade, Query},
    response::{Response, Html, sse::{Event, Sse}},
    routing::{get, post},
    Router, Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::{info, warn, error};
use uuid::Uuid;

// 测试消息结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMessage {
    pub id: String,
    pub timestamp: u64,
    pub content: String,
    pub message_type: MessageType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    Ping,
    Echo,
    Broadcast,
    LargeData,
}

// 连接统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStats {
    pub session_id: String,
    pub protocol: String,
    pub connected_at: u64,
    pub messages_sent: u32,
    pub messages_received: u32,
    pub avg_latency_ms: f64,
    pub connection_errors: u32,
}

// 全局状态
#[derive(Debug)]
pub struct AppState {
    pub connections: Arc<Mutex<HashMap<String, ConnectionStats>>>,
    pub broadcast_tx: broadcast::Sender<TestMessage>,
}

impl AppState {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1000);
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            broadcast_tx: tx,
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::init();
    
    let state = Arc::new(AppState::new());
    
    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .route("/sse", get(sse_handler))
        .route("/stats", get(get_stats))
        .route("/test", post(send_test_message))
        .route("/", get(serve_test_page))
        .nest_service("/static", ServeDir::new("static"))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("Failed to bind to port 3000");
    
    info!("WebSocket/SSE 测试服务器启动在 http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}

// WebSocket 处理器
async fn websocket_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>
) -> Response {
    let session_id = params.get("session_id")
        .unwrap_or(&Uuid::new_v4().to_string())
        .clone();
    
    ws.on_upgrade(move |socket| handle_websocket(socket, session_id, state))
}

async fn handle_websocket(mut socket: WebSocket, session_id: String, state: Arc<AppState>) {
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 注册连接
    {
        let mut connections = state.connections.lock().await;
        connections.insert(session_id.clone(), ConnectionStats {
            session_id: session_id.clone(),
            protocol: "websocket".to_string(),
            connected_at: start_time,
            messages_sent: 0,
            messages_received: 0,
            avg_latency_ms: 0.0,
            connection_errors: 0,
        });
    }

    let mut rx = state.broadcast_tx.subscribe();
    let cloned_state = state.clone();
    let cloned_session_id = session_id.clone();
    
    // 广播消息转发任务
    let broadcast_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            let json_msg = serde_json::to_string(&msg).unwrap();
            if socket.send(axum::extract::ws::Message::Text(json_msg)).await.is_err() {
                error!("WebSocket 发送失败，会话: {}", cloned_session_id);
                break;
            }
            
            // 更新统计
            {
                let mut connections = cloned_state.connections.lock().await;
                if let Some(stats) = connections.get_mut(&cloned_session_id) {
                    stats.messages_sent += 1;
                }
            }
        }
    });

    // 处理来自客户端的消息
    while let Some(msg_result) = socket.recv().await {
        match msg_result {
            Ok(axum::extract::ws::Message::Text(text)) => {
                match serde_json::from_str::<TestMessage>(&text) {
                    Ok(test_msg) => {
                        // 计算延迟
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u64;
                        let latency = now - test_msg.timestamp;
                        
                        // 更新统计
                        {
                            let mut connections = state.connections.lock().await;
                            if let Some(stats) = connections.get_mut(&session_id) {
                                stats.messages_received += 1;
                                stats.avg_latency_ms = 
                                    (stats.avg_latency_ms * (stats.messages_received - 1) as f64 + latency as f64) 
                                    / stats.messages_received as f64;
                            }
                        }

                        // 处理不同类型的消息
                        match test_msg.message_type {
                            MessageType::Ping => {
                                let pong = TestMessage {
                                    id: Uuid::new_v4().to_string(),
                                    timestamp: now,
                                    content: format!("pong-{}", test_msg.id),
                                    message_type: MessageType::Echo,
                                };
                                let json_msg = serde_json::to_string(&pong).unwrap();
                                if socket.send(axum::extract::ws::Message::Text(json_msg)).await.is_err() {
                                    error!("WebSocket Pong 发送失败");
                                    break;
                                }
                            },
                            MessageType::Broadcast => {
                                // 广播给所有连接
                                let _ = state.broadcast_tx.send(test_msg);
                            },
                            _ => {
                                info!("收到消息: {:?}", test_msg);
                            }
                        }
                    },
                    Err(e) => {
                        warn!("WebSocket 消息解析失败: {}", e);
                        let mut connections = state.connections.lock().await;
                        if let Some(stats) = connections.get_mut(&session_id) {
                            stats.connection_errors += 1;
                        }
                    }
                }
            },
            Ok(axum::extract::ws::Message::Close(_)) => {
                info!("WebSocket 连接关闭: {}", session_id);
                break;
            },
            Err(e) => {
                error!("WebSocket 错误: {}", e);
                let mut connections = state.connections.lock().await;
                if let Some(stats) = connections.get_mut(&session_id) {
                    stats.connection_errors += 1;
                }
                break;
            },
            _ => {}
        }
    }

    broadcast_task.abort();
    
    // 清理连接记录
    {
        let mut connections = state.connections.lock().await;
        connections.remove(&session_id);
    }
    info!("WebSocket 会话结束: {}", session_id);
}

// SSE 处理器
async fn sse_handler(
    Query(params): Query<HashMap<String, String>>,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>
) -> Sse<impl futures::Stream<Item = Result<Event, axum::Error>>> {
    let session_id = params.get("session_id")
        .unwrap_or(&Uuid::new_v4().to_string())
        .clone();
    
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 注册连接
    {
        let mut connections = state.connections.lock().await;
        connections.insert(session_id.clone(), ConnectionStats {
            session_id: session_id.clone(),
            protocol: "sse".to_string(),
            connected_at: start_time,
            messages_sent: 0,
            messages_received: 0,
            avg_latency_ms: 0.0,
            connection_errors: 0,
        });
    }

    let mut rx = state.broadcast_tx.subscribe();
    let cloned_state = state.clone();
    let cloned_session_id = session_id.clone();
    
    let stream = async_stream::stream! {
        // 发送初始连接确认
        let init_msg = TestMessage {
            id: Uuid::new_v4().to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            content: format!("SSE 连接建立: {}", cloned_session_id),
            message_type: MessageType::Echo,
        };
        
        if let Ok(json_data) = serde_json::to_string(&init_msg) {
            yield Ok(Event::default().data(json_data));
        }

        // 监听广播消息
        while let Ok(msg) = rx.recv().await {
            match serde_json::to_string(&msg) {
                Ok(json_data) => {
                    // 更新统计
                    {
                        let mut connections = cloned_state.connections.lock().await;
                        if let Some(stats) = connections.get_mut(&cloned_session_id) {
                            stats.messages_sent += 1;
                        }
                    }
                    yield Ok(Event::default().data(json_data));
                },
                Err(e) => {
                    error!("SSE 消息序列化失败: {}", e);
                    let mut connections = cloned_state.connections.lock().await;
                    if let Some(stats) = connections.get_mut(&cloned_session_id) {
                        stats.connection_errors += 1;
                    }
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("keep-alive")
    )
}

// 获取统计信息
async fn get_stats(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>
) -> Json<HashMap<String, ConnectionStats>> {
    let connections = state.connections.lock().await;
    Json(connections.clone())
}

// 发送测试消息
#[derive(Deserialize)]
struct TestRequest {
    message_type: MessageType,
    content: String,
}

async fn send_test_message(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<TestRequest>
) -> Json<serde_json::Value> {
    let test_msg = TestMessage {
        id: Uuid::new_v4().to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
        content: req.content,
        message_type: req.message_type,
    };
    
    match state.broadcast_tx.send(test_msg.clone()) {
        Ok(_) => Json(serde_json::json!({
            "status": "success",
            "message_id": test_msg.id
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "error": e.to_string()
        }))
    }
}

// 服务测试页面
async fn serve_test_page() -> Html<&'static str> {
    Html(include_str!("../static/test.html"))
}