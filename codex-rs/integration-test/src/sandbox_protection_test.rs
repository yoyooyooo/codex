// 沙箱环境变量保护机制验证

use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use tokio::sync::RwLock;
use codex_core::spawn::{CODEX_SANDBOX_ENV_VAR, CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR};

/// 沙箱环境变量保护验证器
pub struct SandboxEnvProtectionValidator {
    /// 原始环境变量快照
    original_env: HashMap<String, String>,
    /// 关键沙箱环境变量
    critical_sandbox_vars: Vec<String>,
    /// 验证结果
    results: Arc<RwLock<SandboxValidationResult>>,
}

#[derive(Debug, Default, Clone)]
pub struct SandboxValidationResult {
    pub sandbox_vars_protected: bool,
    pub env_modifications_detected: Vec<String>,
    pub unauthorized_changes: Vec<String>,
    pub isolation_breaches: Vec<String>,
    pub concurrent_modifications_safe: bool,
    pub restoration_successful: bool,
}

impl SandboxEnvProtectionValidator {
    pub fn new() -> Self {
        // 保存所有当前环境变量
        let original_env: HashMap<String, String> = env::vars().collect();
        
        // 定义关键沙箱环境变量
        let critical_sandbox_vars = vec![
            CODEX_SANDBOX_ENV_VAR.to_string(),
            CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR.to_string(),
            "CODEX_SANDBOX_READ_ONLY".to_string(),
            "CODEX_SANDBOX_EXEC_POLICY".to_string(),
        ];
        
        Self {
            original_env,
            critical_sandbox_vars,
            results: Arc::new(RwLock::new(SandboxValidationResult::default())),
        }
    }

    /// 测试1: 沙箱环境变量保护机制
    pub async fn test_sandbox_env_protection(&self) -> Result<(), Box<dyn std::error::Error>> {
        // 设置初始沙箱环境变量
        env::set_var(CODEX_SANDBOX_ENV_VAR, "seatbelt");
        env::set_var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR, "1");
        
        let initial_sandbox_value = env::var(CODEX_SANDBOX_ENV_VAR).unwrap();
        let initial_network_disabled = env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).unwrap();
        
        // 模拟 Web 组件尝试修改关键环境变量
        self.simulate_unauthorized_modifications().await;
        
        // 验证关键环境变量是否受到保护
        let sandbox_value_after = env::var(CODEX_SANDBOX_ENV_VAR).unwrap_or_default();
        let network_disabled_after = env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).unwrap_or_default();
        
        let protection_effective = sandbox_value_after == initial_sandbox_value &&
                                 network_disabled_after == initial_network_disabled;
        
        {
            let mut results = self.results.write().await;
            results.sandbox_vars_protected = protection_effective;
            
            if !protection_effective {
                results.unauthorized_changes.push(format!(
                    "CODEX_SANDBOX: {} -> {}", 
                    initial_sandbox_value, 
                    sandbox_value_after
                ));
                results.unauthorized_changes.push(format!(
                    "CODEX_SANDBOX_NETWORK_DISABLED: {} -> {}", 
                    initial_network_disabled, 
                    network_disabled_after
                ));
            }
        }
        
        Ok(())
    }

    /// 模拟未授权的环境变量修改
    async fn simulate_unauthorized_modifications(&self) {
        // 模拟恶意或错误的环境变量修改
        env::set_var(CODEX_SANDBOX_ENV_VAR, "compromised");
        env::set_var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR, "0");  // 尝试启用网络
        env::set_var("CODEX_SANDBOX_READ_ONLY", "false");  // 尝试禁用只读保护
    }

    /// 测试2: 环境变量隔离验证
    pub async fn test_environment_isolation(&self) -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        
        let modification_counter = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        
        // 并发修改环境变量
        for i in 0..5 {
            let counter = Arc::clone(&modification_counter);
            let critical_vars = self.critical_sandbox_vars.clone();
            
            let handle = tokio::spawn(async move {
                // 尝试修改关键环境变量
                for var in &critical_vars {
                    env::set_var(var, format!("modified_by_task_{}", i));
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            });
            
            handles.push(handle);
        }
        
        futures::future::join_all(handles).await;
        
        // 验证环境变量的最终状态
        let mut isolation_breaches = Vec::new();
        for var in &self.critical_sandbox_vars {
            if let Ok(value) = env::var(var) {
                if value.contains("modified_by_task_") {
                    isolation_breaches.push(format!("{}={}", var, value));
                }
            }
        }
        
        {
            let mut results = self.results.write().await;
            results.isolation_breaches = isolation_breaches;
            results.concurrent_modifications_safe = results.isolation_breaches.is_empty();
        }
        
        Ok(())
    }

    /// 测试3: 环境变量恢复机制
    pub async fn test_environment_restoration(&self) -> Result<(), Box<dyn std::error::Error>> {
        // 备份当前状态
        let backup_env: HashMap<String, String> = env::vars().collect();
        
        // 进行大量环境变量修改
        for i in 0..100 {
            env::set_var(&format!("TEMP_VAR_{}", i), &format!("temp_value_{}", i));
        }
        
        // 修改关键沙箱变量
        for var in &self.critical_sandbox_vars {
            env::set_var(var, "temporarily_modified");
        }
        
        // 执行恢复操作
        self.restore_environment().await;
        
        // 验证恢复效果
        let mut restoration_successful = true;
        
        // 检查临时变量是否被清理
        for i in 0..100 {
            if env::var(&format!("TEMP_VAR_{}", i)).is_ok() {
                restoration_successful = false;
                break;
            }
        }
        
        // 检查关键变量是否恢复
        for var in &self.critical_sandbox_vars {
            if let Some(original_value) = self.original_env.get(var) {
                if env::var(var).unwrap_or_default() != *original_value {
                    restoration_successful = false;
                    break;
                }
            } else if env::var(var).is_ok() {
                restoration_successful = false;
                break;
            }
        }
        
        {
            let mut results = self.results.write().await;
            results.restoration_successful = restoration_successful;
        }
        
        Ok(())
    }

    /// 环境恢复实现
    async fn restore_environment(&self) {
        // 清除所有当前环境变量
        let current_vars: Vec<String> = env::vars().map(|(k, _)| k).collect();
        for var in current_vars {
            env::remove_var(&var);
        }
        
        // 恢复原始环境变量
        for (key, value) in &self.original_env {
            env::set_var(key, value);
        }
    }

    /// 测试4: 沙箱策略一致性验证
    pub async fn test_sandbox_policy_consistency(&self) -> Result<(), Box<dyn std::error::Error>> {
        use codex_core::protocol::SandboxPolicy;
        
        // 创建标准沙箱策略
        let policy = SandboxPolicy::default();
        
        // 验证环境变量与策略的一致性
        let env_sandbox_type = env::var(CODEX_SANDBOX_ENV_VAR).unwrap_or_default();
        let env_network_disabled = env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR)
            .map(|v| v == "1")
            .unwrap_or(false);
        
        let policy_network_disabled = !policy.has_full_network_access();
        
        let consistency_check = env_network_disabled == policy_network_disabled;
        
        {
            let mut results = self.results.write().await;
            if !consistency_check {
                results.env_modifications_detected.push(format!(
                    "网络策略不一致: 环境变量={}，策略={}",
                    env_network_disabled,
                    policy_network_disabled
                ));
            }
        }
        
        Ok(())
    }

    pub async fn get_results(&self) -> SandboxValidationResult {
        self.results.read().await.clone()
    }
}

impl Drop for SandboxEnvProtectionValidator {
    fn drop(&mut self) {
        // 确保测试后环境变量被正确恢复
        let current_vars: Vec<String> = env::vars().map(|(k, _)| k).collect();
        for var in current_vars {
            env::remove_var(&var);
        }
        
        for (key, value) in &self.original_env {
            env::set_var(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_sandbox_environment_protection() {
        let validator = SandboxEnvProtectionValidator::new();
        
        // 运行所有沙箱环境保护测试
        validator.test_sandbox_env_protection().await.unwrap();
        validator.test_environment_isolation().await.unwrap();
        validator.test_environment_restoration().await.unwrap();
        validator.test_sandbox_policy_consistency().await.unwrap();
        
        let results = validator.get_results().await;
        
        // 验证结果
        assert!(results.sandbox_vars_protected, "沙箱环境变量未受保护");
        assert!(results.concurrent_modifications_safe, "并发环境变量修改不安全");
        assert!(results.restoration_successful, "环境变量恢复失败");
        assert!(results.isolation_breaches.is_empty(), "检测到隔离破坏: {:?}", results.isolation_breaches);
        
        println!("✅ 沙箱环境变量保护机制验证通过");
        println!("   - 沙箱变量受保护: {}", results.sandbox_vars_protected);
        println!("   - 并发修改安全: {}", results.concurrent_modifications_safe);
        println!("   - 环境恢复成功: {}", results.restoration_successful);
        println!("   - 未授权更改: {:?}", results.unauthorized_changes);
        println!("   - 隔离破坏: {:?}", results.isolation_breaches);
        
        // 确保测试结束后环境清洁
        assert_eq!(results.isolation_breaches.len(), 0);
        assert_eq!(results.unauthorized_changes.len(), 0);
    }
}