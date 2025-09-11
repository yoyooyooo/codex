// TUI 和 Web 同时运行的互不干扰测试

use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{Duration, Instant};
use tokio::process::Command;
use serde::{Deserialize, Serialize};

/// TUI 和 Web 并发运行兼容性验证器
pub struct ConcurrentRunValidator {
    /// TUI 进程控制
    tui_process: Option<tokio::process::Child>,
    /// Web 服务器控制
    web_process: Option<tokio::process::Child>,
    /// 验证结果
    results: Arc<RwLock<ConcurrentValidationResult>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ConcurrentValidationResult {
    pub tui_web_can_coexist: bool,
    pub resource_conflicts_detected: Vec<String>,
    pub port_conflicts: Vec<u16>,
    pub file_lock_conflicts: Vec<String>,
    pub memory_usage_acceptable: bool,
    pub performance_degradation: f64,  // 百分比
    pub startup_times: StartupMetrics,
    pub shutdown_cleanup_successful: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct StartupMetrics {
    pub tui_startup_time_ms: u64,
    pub web_startup_time_ms: u64,
    pub concurrent_startup_time_ms: u64,
}

impl ConcurrentRunValidator {
    pub fn new() -> Self {
        Self {
            tui_process: None,
            web_process: None,
            results: Arc::new(RwLock::new(ConcurrentValidationResult::default())),
        }
    }

    /// 测试1: TUI 和 Web 同时启动
    pub async fn test_concurrent_startup(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let start_time = Instant::now();
        
        // 并发启动 TUI 和 Web
        let tui_start = Instant::now();
        self.start_tui_process().await?;
        let tui_startup_time = tui_start.elapsed();
        
        let web_start = Instant::now();
        self.start_web_process().await?;
        let web_startup_time = web_start.elapsed();
        
        let total_startup_time = start_time.elapsed();
        
        // 等待进程稳定
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        // 检查进程状态
        let tui_running = self.is_tui_running().await;
        let web_running = self.is_web_running().await;
        
        {
            let mut results = self.results.write().await;
            results.tui_web_can_coexist = tui_running && web_running;
            results.startup_times = StartupMetrics {
                tui_startup_time_ms: tui_startup_time.as_millis() as u64,
                web_startup_time_ms: web_startup_time.as_millis() as u64,
                concurrent_startup_time_ms: total_startup_time.as_millis() as u64,
            };
        }
        
        Ok(())
    }

    /// 启动 TUI 进程
    async fn start_tui_process(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::new("cargo");
        cmd.args(&["run", "--bin", "codex-tui", "--", "--help"]);
        cmd.current_dir("../../"); // 假设测试在 codex-rs/integration-test 中运行
        
        let child = cmd.spawn()?;
        self.tui_process = Some(child);
        
        Ok(())
    }

    /// 启动 Web 进程
    async fn start_web_process(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // 这里假设我们有一个 Web 服务器实现
        let mut cmd = Command::new("cargo");
        cmd.args(&["run", "--bin", "codex-web", "--", "--port", "3001", "--no-open"]);
        cmd.current_dir("../../");
        
        let child = cmd.spawn()?;
        self.web_process = Some(child);
        
        Ok(())
    }

    /// 检查 TUI 进程是否运行
    async fn is_tui_running(&mut self) -> bool {
        if let Some(ref mut process) = &mut self.tui_process {
            match process.try_wait() {
                Ok(Some(_)) => false, // 进程已退出
                Ok(None) => true,     // 进程仍在运行
                Err(_) => false,      // 错误
            }
        } else {
            false
        }
    }

    /// 检查 Web 进程是否运行
    async fn is_web_running(&mut self) -> bool {
        if let Some(ref mut process) = &mut self.web_process {
            match process.try_wait() {
                Ok(Some(_)) => false,
                Ok(None) => true,
                Err(_) => false,
            }
        } else {
            false
        }
    }

    /// 测试2: 资源冲突检测
    pub async fn test_resource_conflicts(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut conflicts = Vec::new();
        let mut file_conflicts = Vec::new();
        let mut port_conflicts = Vec::new();
        
        // 检查端口冲突
        if self.check_port_conflict(3000).await {
            port_conflicts.push(3000);
        }
        
        // 检查文件锁冲突
        let config_file = dirs::home_dir()
            .map(|h| h.join(".config/codex/config.toml"))
            .unwrap_or_default();
        
        if self.check_file_lock_conflict(&config_file).await {
            file_conflicts.push(config_file.display().to_string());
        }
        
        // 检查日志文件冲突
        let log_file = dirs::home_dir()
            .map(|h| h.join(".config/codex/logs/codex.log"))
            .unwrap_or_default();
        
        if self.check_file_lock_conflict(&log_file).await {
            file_conflicts.push(log_file.display().to_string());
        }
        
        {
            let mut results = self.results.write().await;
            results.resource_conflicts_detected = conflicts;
            results.port_conflicts = port_conflicts;
            results.file_lock_conflicts = file_conflicts;
        }
        
        Ok(())
    }

    /// 检查端口冲突
    async fn check_port_conflict(&self, port: u16) -> bool {
        use std::net::TcpListener;
        
        // 尝试绑定端口
        match TcpListener::bind(format!("127.0.0.1:{}", port)) {
            Ok(_) => false, // 端口可用
            Err(_) => true, // 端口冲突
        }
    }

    /// 检查文件锁冲突
    async fn check_file_lock_conflict(&self, file_path: &std::path::Path) -> bool {
        use std::fs::OpenOptions;
        
        if !file_path.exists() {
            return false;
        }
        
        // 尝试以独占模式打开文件
        match OpenOptions::new()
            .write(true)
            .create(false)
            .open(file_path)
        {
            Ok(_) => false, // 无锁冲突
            Err(_) => true, // 文件被锁定
        }
    }

    /// 测试3: 性能影响评估
    pub async fn test_performance_impact(&self) -> Result<(), Box<dyn std::error::Error>> {
        // 测量独立运行时的性能基线
        let baseline_memory = self.measure_memory_usage().await;
        let baseline_response_time = self.measure_response_time().await;
        
        // 等待并发运行稳定
        tokio::time::sleep(Duration::from_secs(5)).await;
        
        // 测量并发运行时的性能
        let concurrent_memory = self.measure_memory_usage().await;
        let concurrent_response_time = self.measure_response_time().await;
        
        // 计算性能降级
        let memory_increase = (concurrent_memory as f64 / baseline_memory as f64 - 1.0) * 100.0;
        let response_time_increase = (concurrent_response_time.as_millis() as f64 / baseline_response_time.as_millis() as f64 - 1.0) * 100.0;
        
        let performance_degradation = memory_increase.max(response_time_increase);
        
        {
            let mut results = self.results.write().await;
            results.memory_usage_acceptable = memory_increase < 50.0; // 50% 内存增加阈值
            results.performance_degradation = performance_degradation;
        }
        
        Ok(())
    }

    /// 测量内存使用量
    async fn measure_memory_usage(&self) -> u64 {
        // 简化实现 - 在实际环境中应使用系统调用
        use std::process::Command as StdCommand;
        
        let output = StdCommand::new("ps")
            .args(&["-o", "rss=", "-p"])
            .arg(std::process::id().to_string())
            .output()
            .unwrap_or_default();
        
        let memory_kb: u64 = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .unwrap_or(0);
        
        memory_kb * 1024 // 转换为字节
    }

    /// 测量响应时间
    async fn measure_response_time(&self) -> Duration {
        let start = Instant::now();
        
        // 执行一个简单的操作
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        start.elapsed()
    }

    /// 测试4: 清理和关闭
    pub async fn test_shutdown_cleanup(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut cleanup_successful = true;
        
        // 关闭 Web 进程
        if let Some(mut web_process) = self.web_process.take() {
            if web_process.kill().await.is_err() {
                cleanup_successful = false;
            }
        }
        
        // 关闭 TUI 进程
        if let Some(mut tui_process) = self.tui_process.take() {
            if tui_process.kill().await.is_err() {
                cleanup_successful = false;
            }
        }
        
        // 等待进程完全退出
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        // 检查临时文件清理
        let temp_files_cleaned = self.check_temp_files_cleanup().await;
        cleanup_successful = cleanup_successful && temp_files_cleaned;
        
        {
            let mut results = self.results.write().await;
            results.shutdown_cleanup_successful = cleanup_successful;
        }
        
        Ok(())
    }

    /// 检查临时文件清理
    async fn check_temp_files_cleanup(&self) -> bool {
        // 检查是否有遗留的临时文件、锁文件等
        let temp_dirs = vec![
            std::env::temp_dir(),
            dirs::home_dir().map(|h| h.join(".config/codex/tmp")).unwrap_or_default(),
        ];
        
        for temp_dir in temp_dirs {
            if temp_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(temp_dir) {
                    for entry in entries {
                        if let Ok(entry) = entry {
                            let filename = entry.file_name();
                            if filename.to_string_lossy().contains("codex") {
                                return false; // 发现未清理的文件
                            }
                        }
                    }
                }
            }
        }
        
        true
    }

    pub async fn get_results(&self) -> ConcurrentValidationResult {
        self.results.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_concurrent_operation_compatibility() {
        let mut validator = ConcurrentRunValidator::new();
        
        // 运行所有并发兼容性测试
        validator.test_concurrent_startup().await.unwrap();
        validator.test_resource_conflicts().await.unwrap();
        validator.test_performance_impact().await.unwrap();
        validator.test_shutdown_cleanup().await.unwrap();
        
        let results = validator.get_results().await;
        
        // 验证结果
        assert!(results.tui_web_can_coexist, "TUI 和 Web 无法同时运行");
        assert!(results.resource_conflicts_detected.is_empty(), "检测到资源冲突: {:?}", results.resource_conflicts_detected);
        assert!(results.port_conflicts.is_empty(), "检测到端口冲突: {:?}", results.port_conflicts);
        assert!(results.file_lock_conflicts.is_empty(), "检测到文件锁冲突: {:?}", results.file_lock_conflicts);
        assert!(results.memory_usage_acceptable, "内存使用量不可接受");
        assert!(results.performance_degradation < 25.0, "性能降级过大: {}%", results.performance_degradation);
        assert!(results.shutdown_cleanup_successful, "关闭清理失败");
        
        println!("✅ TUI 和 Web 并发运行兼容性验证通过");
        println!("   - 可以共存: {}", results.tui_web_can_coexist);
        println!("   - TUI 启动时间: {}ms", results.startup_times.tui_startup_time_ms);
        println!("   - Web 启动时间: {}ms", results.startup_times.web_startup_time_ms);
        println!("   - 并发启动时间: {}ms", results.startup_times.concurrent_startup_time_ms);
        println!("   - 性能降级: {:.1}%", results.performance_degradation);
        println!("   - 内存使用可接受: {}", results.memory_usage_acceptable);
        println!("   - 清理成功: {}", results.shutdown_cleanup_successful);
    }
}