# 安全设计方案

**文档版本**: v1.0  
**最后更新**: 2025-09-11  
**依赖文档**: [02-architecture.md](02-architecture.md), [03-backend-design.md](03-backend-design.md)

## 安全原则

### 1. 本地化優先 (Local-First Security)
- 仅本地回环地址访问（127.0.0.1）
- 随机端口分配，避免端口冲突
- 不提供远程访问能力

### 2. 最小权限原则 (Principle of Least Privilege)
- 严格遵守现有沙箱策略
- 所有文件操作经过审批流程
- 不修改任何 `CODEX_SANDBOX_*` 环境变量

### 3. 防御性设计 (Defense in Depth)
- 多层次认证机制
- 输入验证和输出过滤
- 完整的日志和审计

## 网络安全

### 端口绑定策略
```rust
// 仅绑定本地回环地址
let bind_addr = "127.0.0.1:0"; // 0 表示随机端口
let listener = TcpListener::bind(bind_addr).await?;

// 验证绑定地址
let actual_addr = listener.local_addr()?;
assert!(actual_addr.ip().is_loopback());
```

### 访问令牌机制
```rust
// 生成一次性随机令牌
fn generate_access_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| {
            let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
            chars[rng.gen_range(0..chars.len())] as char
        })
        .collect()
}

// 令牌验证中间件
async fn auth_middleware(
    headers: HeaderMap,
    request: Request<Body>,
    next: Next<Body>,
) -> Response {
    let token = extract_bearer_token(&headers);
    if !verify_access_token(token) {
        return Response::builder()
            .status(401)
            .body("Unauthorized".into())
            .unwrap();
    }
    next.run(request).await
}
```

### CORS 策略
```rust
// 仅允许本地访问
let cors = CorsLayer::new()
    .allow_origin("http://127.0.0.1:*".parse::<HeaderValue>().unwrap())
    .allow_origin("http://localhost:*".parse::<HeaderValue>().unwrap())
    .allow_methods([Method::GET, Method::POST])
    .allow_headers(Any);
```

## 输入验证

### 请求验证
```rust
// 请求大小限制
const MAX_REQUEST_SIZE: usize = 10 * 1024 * 1024; // 10MB

// JSON 验证
#[derive(Deserialize, Validate)]
struct CreateSessionRequest {
    #[validate(length(max = 1000))]
    cwd: Option<String>,
    
    #[validate(regex = "MODEL_NAME_REGEX")]
    model: Option<String>,
}

// 路径验证
fn validate_file_path(path: &str) -> Result<(), SecurityError> {
    // 禁止目录遍历
    if path.contains("..") {
        return Err(SecurityError::PathTraversal);
    }
    
    // 检查绝对路径
    let canonical = std::fs::canonicalize(path)?;
    if !canonical.starts_with("/allowed/project/root") {
        return Err(SecurityError::PathNotAllowed);
    }
    
    Ok(())
}
```

### WebSocket 消息验证
```rust
// 消息大小限制
const MAX_WS_MESSAGE_SIZE: usize = 1024 * 1024; // 1MB

// 消息频率限制  
struct RateLimiter {
    tokens: Arc<Mutex<u32>>,
    last_refill: Arc<Mutex<Instant>>,
}

impl RateLimiter {
    fn check_rate_limit(&self) -> bool {
        let mut tokens = self.tokens.lock().unwrap();
        let mut last_refill = self.last_refill.lock().unwrap();
        
        let now = Instant::now();
        let elapsed = now.duration_since(*last_refill);
        
        // 每秒补充 10 个 token
        let new_tokens = (elapsed.as_secs() * 10) as u32;
        *tokens = (*tokens + new_tokens).min(100); // 最大 100 个
        *last_refill = now;
        
        if *tokens > 0 {
            *tokens -= 1;
            true
        } else {
            false
        }
    }
}
```

## 沙箱集成

### 环境变量保护
```rust
// 检查沙箱环境变量完整性
fn verify_sandbox_integrity() -> Result<(), SecurityError> {
    let sandbox_vars: Vec<_> = std::env::vars()
        .filter(|(key, _)| key.starts_with("CODEX_SANDBOX_"))
        .collect();
    
    // 记录沙箱配置用于审计
    tracing::info!("Sandbox configuration: {:?}", sandbox_vars);
    
    // 验证必需的沙箱变量
    if !std::env::var("CODEX_SANDBOX_ENABLED").is_ok() {
        return Err(SecurityError::SandboxNotConfigured);
    }
    
    Ok(())
}

// 文件操作拦截
async fn intercept_file_operation(
    operation: &FileOperation,
) -> Result<(), SecurityError> {
    // 检查文件访问权限
    if !check_file_permission(&operation.path, &operation.mode) {
        return Err(SecurityError::FileAccessDenied);
    }
    
    // 记录文件操作
    tracing::warn!(
        "File operation requested: {} on {}", 
        operation.mode, 
        operation.path
    );
    
    Ok(())
}
```

## 审批流程安全

### 审批信息安全
```rust
#[derive(Serialize)]
struct ApprovalRequest {
    id: Uuid,
    operation_type: String,
    
    // 敏感信息过滤
    #[serde(serialize_with = "sanitize_file_path")]
    file_path: Option<String>,
    
    #[serde(serialize_with = "sanitize_command")]
    command: Option<String>,
    
    // 风险等级评估
    risk_level: RiskLevel,
}

fn sanitize_file_path<S>(path: &Option<String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match path {
        Some(p) => {
            // 隐藏敏感路径信息
            let sanitized = p.replace(std::env::var("HOME").unwrap_or_default().as_str(), "~");
            serializer.serialize_str(&sanitized)
        }
        None => serializer.serialize_none(),
    }
}
```

### 审批决策验证
```rust
#[derive(Deserialize, Validate)]
struct ApprovalDecision {
    approval_id: Uuid,
    
    #[validate(custom = "validate_decision")]
    decision: ApprovalChoice,
    
    #[validate(length(max = 1000))]
    reason: Option<String>,
    
    // 防止重放攻击
    timestamp: i64,
    nonce: String,
}

fn validate_decision(choice: &ApprovalChoice) -> ValidationResult {
    match choice {
        ApprovalChoice::Approve => {
            // 高风险操作需要额外确认
            ValidationResult::Ok(())
        }
        ApprovalChoice::Reject => ValidationResult::Ok(()),
        ApprovalChoice::Modify { command } => {
            // 验证修改后的命令安全性
            validate_command_safety(command)
        }
    }
}
```

## 日志和审计

### 安全事件日志
```rust
#[derive(Debug, Serialize)]
struct SecurityEvent {
    timestamp: DateTime<Utc>,
    event_type: SecurityEventType,
    severity: Severity,
    source_ip: IpAddr,
    session_id: Option<SessionId>,
    details: serde_json::Value,
}

#[derive(Debug, Serialize)]
enum SecurityEventType {
    AuthenticationFailed,
    UnauthorizedAccess,
    SuspiciousActivity,
    FileAccessDenied,
    RateLimitExceeded,
    SandboxViolation,
}

// 安全事件记录
fn log_security_event(event: SecurityEvent) {
    // 结构化日志记录
    tracing::warn!(
        security_event = true,
        event_type = ?event.event_type,
        severity = ?event.severity,
        source_ip = %event.source_ip,
        session_id = ?event.session_id,
        details = %serde_json::to_string(&event.details).unwrap_or_default(),
        "{:?} security event detected",
        event.event_type
    );
    
    // 高严重级事件发送告警
    if matches!(event.severity, Severity::High | Severity::Critical) {
        send_security_alert(event);
    }
}
```

### 操作审计日志
```rust
#[derive(Debug, Serialize)]
struct AuditLog {
    timestamp: DateTime<Utc>,
    session_id: SessionId,
    user_action: String,
    operation_type: String,
    target_resource: String,
    result: OperationResult,
    duration_ms: u64,
}

// 审计日志记录
async fn log_operation<T, F, Fut>(
    session_id: SessionId,
    operation: &str,
    target: &str,
    func: F,
) -> Result<T, Box<dyn std::error::Error>>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, Box<dyn std::error::Error>>>,
{
    let start = Instant::now();
    let result = func().await;
    let duration = start.elapsed();
    
    let audit_log = AuditLog {
        timestamp: Utc::now(),
        session_id,
        user_action: operation.to_string(),
        operation_type: classify_operation(operation),
        target_resource: target.to_string(),
        result: match &result {
            Ok(_) => OperationResult::Success,
            Err(e) => OperationResult::Failed(e.to_string()),
        },
        duration_ms: duration.as_millis() as u64,
    };
    
    tracing::info!(
        audit = true,
        session_id = %session_id,
        operation = %operation,
        result = ?audit_log.result,
        duration_ms = audit_log.duration_ms,
        "Operation completed"
    );
    
    result
}
```

## 安全监控

### 实时威胁检测
```rust
struct ThreatDetector {
    failed_auth_attempts: DashMap<IpAddr, u32>,
    rate_limiters: DashMap<SessionId, RateLimiter>,
    suspicious_patterns: Vec<SuspiciousPattern>,
}

impl ThreatDetector {
    async fn analyze_request(&self, request: &HttpRequest) -> ThreatLevel {
        let mut threat_score = 0;
        
        // 检查频繁失败认证
        let ip = request.remote_addr().ip();
        if let Some(failures) = self.failed_auth_attempts.get(&ip) {
            if *failures > 5 {
                threat_score += 50;
            }
        }
        
        // 检查异常请求模式
        for pattern in &self.suspicious_patterns {
            if pattern.matches(request) {
                threat_score += pattern.severity;
            }
        }
        
        // 根据威胁分数返回级别
        match threat_score {
            0..=20 => ThreatLevel::Low,
            21..=50 => ThreatLevel::Medium,
            51..=80 => ThreatLevel::High,
            _ => ThreatLevel::Critical,
        }
    }
}
```

## 安全配置

### 安全策略配置
```rust
#[derive(Debug, Clone, Deserialize)]
struct SecurityConfig {
    // 网络安全
    max_connections_per_ip: usize,
    request_timeout_seconds: u64,
    max_request_size_bytes: usize,
    
    // 认证安全
    token_expiry_minutes: u64,
    max_failed_auth_attempts: u32,
    auth_lockout_duration_minutes: u64,
    
    // 文件安全
    allowed_file_extensions: Vec<String>,
    max_file_size_bytes: usize,
    quarantine_suspicious_files: bool,
    
    // 审计配置
    audit_log_retention_days: u32,
    security_log_level: String,
    alert_on_high_risk_operations: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            max_connections_per_ip: 10,
            request_timeout_seconds: 30,
            max_request_size_bytes: 10 * 1024 * 1024,
            
            token_expiry_minutes: 60,
            max_failed_auth_attempts: 5,
            auth_lockout_duration_minutes: 15,
            
            allowed_file_extensions: vec![
                "rs".to_string(), "py".to_string(), "js".to_string(),
                "ts".to_string(), "md".to_string(), "json".to_string(),
            ],
            max_file_size_bytes: 50 * 1024 * 1024,
            quarantine_suspicious_files: true,
            
            audit_log_retention_days: 90,
            security_log_level: "INFO".to_string(),
            alert_on_high_risk_operations: true,
        }
    }
}
```

---
**变更记录**：
- v1.0 (2025-09-11): 初始版本，完整的安全设计方案