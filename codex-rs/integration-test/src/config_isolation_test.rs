// 配置文件共享和隔离策略验证

use codex_core::config::{Config, ConfigOverrides, find_codex_home};
use codex_common::CliConfigOverrides;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use tempfile::TempDir;

/// 配置系统隔离和共享验证器
pub struct ConfigIsolationValidator {
    /// 测试用临时目录
    temp_dir: TempDir,
    /// 原始环境变量快照
    original_env: HashMap<String, String>,
    /// 验证结果
    results: Arc<RwLock<ConfigValidationResult>>,
}

#[derive(Debug, Default, Clone)]
pub struct ConfigValidationResult {
    pub config_loading_isolated: bool,
    pub shared_config_consistent: bool,
    pub override_isolation_works: bool,
    pub file_access_conflicts: Vec<String>,
    pub config_corruption_detected: bool,
    pub concurrent_access_safe: bool,
}

impl ConfigIsolationValidator {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        
        // 保存原始环境变量
        let original_env: HashMap<String, String> = env::vars().collect();
        
        Ok(Self {
            temp_dir,
            original_env,
            results: Arc::new(RwLock::new(ConfigValidationResult::default())),
        })
    }

    /// 测试1: 配置加载隔离性
    pub async fn test_config_loading_isolation(&self) -> Result<(), Box<dyn std::error::Error>> {
        // 创建独立的配置环境
        let config_dir = self.temp_dir.path().join("test_config");
        std::fs::create_dir_all(&config_dir)?;
        
        // 设置测试用 CODEX 主目录
        env::set_var("CODEX_HOME", config_dir.to_str().unwrap());
        
        // 创建测试配置文件
        let config_content = r#"
[model]
name = "test-model"
provider = "test-provider"

[sandbox]
mode = "strict"

[tui]
show_reasoning = true
"#;
        std::fs::write(config_dir.join("config.toml"), config_content)?;
        
        // 测试 TUI 配置加载
        let tui_overrides = CliConfigOverrides::default();
        let tui_config = Config::load_with_cli_overrides(&tui_overrides)?;
        
        // 测试 Web 配置加载（模拟不同的覆盖项）
        let mut web_overrides = CliConfigOverrides::default();
        web_overrides.raw_overrides.push("model.name=web-model".to_string());
        let web_config = Config::load_with_cli_overrides(&web_overrides)?;
        
        // 验证配置隔离
        let tui_model = &tui_config.model;
        let web_model = &web_config.model;
        
        let isolation_works = tui_model != web_model && 
                             tui_model == "test-model" && 
                             web_model == "web-model";
        
        {
            let mut results = self.results.write().await;
            results.config_loading_isolated = isolation_works;
            results.shared_config_consistent = tui_config.sandbox_policy == web_config.sandbox_policy;
        }
        
        Ok(())
    }

    /// 测试2: 并发配置访问安全性
    pub async fn test_concurrent_config_access(&self) -> Result<(), Box<dyn std::error::Error>> {
        let num_concurrent_loads = 10;
        let mut handles = Vec::new();
        
        for i in 0..num_concurrent_loads {
            let results_clone = Arc::clone(&self.results);
            let temp_dir = self.temp_dir.path().to_path_buf();
            
            let handle = tokio::spawn(async move {
                let mut overrides = CliConfigOverrides::default();
                overrides.raw_overrides.push(format!("model.name=concurrent-model-{}", i));
                
                match Config::load_with_cli_overrides(&overrides) {
                    Ok(config) => {
                        // 验证配置内容正确性
                        config.model.contains(&format!("concurrent-model-{}", i))
                    }
                    Err(_) => false,
                }
            });
            
            handles.push(handle);
        }
        
        let results: Vec<_> = futures::future::join_all(handles).await;
        let successful_loads = results.iter()
            .filter(|r| r.is_ok() && r.as_ref().unwrap())
            .count();
        
        {
            let mut validation_results = self.results.write().await;
            validation_results.concurrent_access_safe = successful_loads == num_concurrent_loads;
        }
        
        Ok(())
    }

    /// 测试3: 配置文件锁定和竞争条件
    pub async fn test_config_file_locking(&self) -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::atomic::{AtomicBool, Ordering};
        use tokio::fs::OpenOptions;
        use tokio::io::AsyncWriteExt;
        
        let config_file = self.temp_dir.path().join("test_concurrent.toml");
        let corruption_detected = Arc::new(AtomicBool::new(false));
        let mut handles = Vec::new();
        
        // 并发写入测试
        for i in 0..5 {
            let config_file = config_file.clone();
            let corruption_detected = Arc::clone(&corruption_detected);
            
            let handle = tokio::spawn(async move {
                let content = format!("# Config modification {}\ntest_value = {}\n", i, i);
                
                match OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&config_file)
                    .await
                {
                    Ok(mut file) => {
                        if let Err(_) = file.write_all(content.as_bytes()).await {
                            corruption_detected.store(true, Ordering::Relaxed);
                        }
                    }
                    Err(_) => {
                        corruption_detected.store(true, Ordering::Relaxed);
                    }
                }
            });
            
            handles.push(handle);
        }
        
        futures::future::join_all(handles).await;
        
        {
            let mut results = self.results.write().await;
            results.config_corruption_detected = corruption_detected.load(Ordering::Relaxed);
        }
        
        Ok(())
    }

    /// 测试4: 环境变量覆盖隔离
    pub async fn test_environment_override_isolation(&self) -> Result<(), Box<dyn std::error::Error>> {
        // 保存当前环境
        let original_model = env::var("CODEX_MODEL").ok();
        
        // TUI 设置环境变量
        env::set_var("CODEX_MODEL", "tui-model");
        let tui_config = Config::load_with_cli_overrides(&CliConfigOverrides::default())?;
        
        // Web 设置不同的环境变量
        env::set_var("CODEX_MODEL", "web-model");
        let web_config = Config::load_with_cli_overrides(&CliConfigOverrides::default())?;
        
        // 验证隔离效果
        let override_isolation_works = tui_config.model != web_config.model;
        
        {
            let mut results = self.results.write().await;
            results.override_isolation_works = override_isolation_works;
        }
        
        // 恢复原始环境
        if let Some(original) = original_model {
            env::set_var("CODEX_MODEL", original);
        } else {
            env::remove_var("CODEX_MODEL");
        }
        
        Ok(())
    }

    pub async fn get_results(&self) -> ConfigValidationResult {
        self.results.read().await.clone()
    }
}

impl Drop for ConfigIsolationValidator {
    fn drop(&mut self) {
        // 恢复原始环境变量
        env::vars().for_each(|(key, _)| env::remove_var(&key));
        for (key, value) in &self.original_env {
            env::set_var(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_config_isolation_comprehensive() {
        let validator = ConfigIsolationValidator::new().unwrap();
        
        // 运行所有配置隔离测试
        validator.test_config_loading_isolation().await.unwrap();
        validator.test_concurrent_config_access().await.unwrap();
        validator.test_config_file_locking().await.unwrap();
        validator.test_environment_override_isolation().await.unwrap();
        
        let results = validator.get_results().await;
        
        // 验证结果
        assert!(results.config_loading_isolated, "配置加载隔离失败");
        assert!(results.shared_config_consistent, "共享配置不一致");
        assert!(results.override_isolation_works, "环境变量覆盖隔离失败");
        assert!(results.concurrent_access_safe, "并发配置访问不安全");
        assert!(!results.config_corruption_detected, "检测到配置文件腐化");
        
        println!("✅ 配置隔离和共享策略验证通过");
        println!("   - 配置加载隔离: {}", results.config_loading_isolated);
        println!("   - 共享配置一致: {}", results.shared_config_consistent);
        println!("   - 覆盖隔离有效: {}", results.override_isolation_works);
        println!("   - 并发访问安全: {}", results.concurrent_access_safe);
        println!("   - 文件访问冲突: {:?}", results.file_access_conflicts);
    }
}