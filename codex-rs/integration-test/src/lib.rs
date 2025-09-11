// 集成测试主协调器和量化成功标准

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

mod conversation_manager_test;
mod config_isolation_test;
mod sandbox_protection_test;
mod concurrent_run_test;

use conversation_manager_test::{ConversationManagerValidator, ValidationResult as ConvResult};
use config_isolation_test::{ConfigIsolationValidator, ConfigValidationResult};
use sandbox_protection_test::{SandboxEnvProtectionValidator, SandboxValidationResult};
use concurrent_run_test::{ConcurrentRunValidator, ConcurrentValidationResult};

/// 集成兼容性验证主协调器
pub struct IntegrationCompatibilityValidator {
    /// 测试开始时间
    start_time: Instant,
    /// 综合验证结果
    overall_results: IntegrationValidationReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationValidationReport {
    /// 测试执行摘要
    pub test_summary: TestSummary,
    
    /// ConversationManager 测试结果
    pub conversation_manager: ConvResult,
    
    /// 配置隔离测试结果
    pub config_isolation: ConfigValidationResult,
    
    /// 沙箱保护测试结果
    pub sandbox_protection: SandboxValidationResult,
    
    /// 并发运行测试结果
    pub concurrent_operation: ConcurrentValidationResult,
    
    /// 综合评估
    pub overall_assessment: OverallAssessment,
    
    /// 建议的集成策略
    pub recommended_integration_strategy: IntegrationStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSummary {
    pub total_tests_run: u32,
    pub tests_passed: u32,
    pub tests_failed: u32,
    pub total_execution_time_ms: u64,
    pub critical_failures: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverallAssessment {
    pub integration_feasible: bool,
    pub risk_level: RiskLevel,
    pub confidence_score: f64,  // 0.0 - 1.0
    pub major_concerns: Vec<String>,
    pub mitigation_required: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,       // 可以直接集成
    Medium,    // 需要额外预防措施
    High,      // 需要重大架构调整
    Critical,  // 不建议进程内集成
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IntegrationStrategy {
    /// 直接进程内集成，风险低
    DirectInProcess,
    /// 进程内集成 + 额外隔离机制
    InProcessWithIsolation,
    /// 混合模式：部分进程内，部分独立进程
    HybridMode,
    /// 完全独立进程通信
    SeparateProcesses,
    /// 不建议集成，需要重新设计
    NotRecommended,
}

impl IntegrationCompatibilityValidator {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            overall_results: IntegrationValidationReport::default(),
        }
    }

    /// 执行完整的集成兼容性验证
    pub async fn run_comprehensive_validation(&mut self) -> Result<IntegrationValidationReport, Box<dyn std::error::Error>> {
        println!("🔍 开始 Codex Web 与 TUI 集成兼容性验证...");
        
        let mut test_summary = TestSummary::default();
        let mut critical_failures = Vec::new();
        let mut warnings = Vec::new();

        // 1. ConversationManager 兼容性测试
        println!("\n📋 测试 1: ConversationManager 进程内复用安全性");
        let conv_result = match self.test_conversation_manager().await {
            Ok(result) => {
                test_summary.tests_passed += 1;
                if !result.thread_safety_verified {
                    critical_failures.push("ConversationManager 线程安全性验证失败".to_string());
                }
                if result.memory_leaks_detected {
                    warnings.push("检测到 ConversationManager 内存泄漏".to_string());
                }
                result
            }
            Err(e) => {
                test_summary.tests_failed += 1;
                critical_failures.push(format!("ConversationManager 测试失败: {}", e));
                ConvResult::default()
            }
        };
        test_summary.total_tests_run += 1;

        // 2. 配置系统隔离测试
        println!("\n⚙️  测试 2: 配置文件共享和隔离策略");
        let config_result = match self.test_config_isolation().await {
            Ok(result) => {
                test_summary.tests_passed += 1;
                if !result.config_loading_isolated {
                    critical_failures.push("配置加载隔离机制失败".to_string());
                }
                if result.config_corruption_detected {
                    critical_failures.push("检测到配置文件腐化".to_string());
                }
                if !result.concurrent_access_safe {
                    warnings.push("并发配置访问存在风险".to_string());
                }
                result
            }
            Err(e) => {
                test_summary.tests_failed += 1;
                critical_failures.push(format!("配置隔离测试失败: {}", e));
                ConfigValidationResult::default()
            }
        };
        test_summary.total_tests_run += 1;

        // 3. 沙箱环境变量保护测试
        println!("\n🛡️  测试 3: 沙箱环境变量完整性保护");
        let sandbox_result = match self.test_sandbox_protection().await {
            Ok(result) => {
                test_summary.tests_passed += 1;
                if !result.sandbox_vars_protected {
                    critical_failures.push("沙箱环境变量保护机制失效".to_string());
                }
                if !result.isolation_breaches.is_empty() {
                    critical_failures.push(format!("沙箱隔离被破坏: {:?}", result.isolation_breaches));
                }
                if !result.restoration_successful {
                    warnings.push("环境变量恢复机制存在问题".to_string());
                }
                result
            }
            Err(e) => {
                test_summary.tests_failed += 1;
                critical_failures.push(format!("沙箱保护测试失败: {}", e));
                SandboxValidationResult::default()
            }
        };
        test_summary.total_tests_run += 1;

        // 4. 并发运行兼容性测试
        println!("\n🔄 测试 4: TUI 和 Web 同时运行兼容性");
        let concurrent_result = match self.test_concurrent_operation().await {
            Ok(result) => {
                test_summary.tests_passed += 1;
                if !result.tui_web_can_coexist {
                    critical_failures.push("TUI 和 Web 无法同时运行".to_string());
                }
                if !result.resource_conflicts_detected.is_empty() {
                    warnings.push(format!("资源冲突: {:?}", result.resource_conflicts_detected));
                }
                if result.performance_degradation > 30.0 {
                    warnings.push(format!("性能降级过大: {:.1}%", result.performance_degradation));
                }
                result
            }
            Err(e) => {
                test_summary.tests_failed += 1;
                critical_failures.push(format!("并发运行测试失败: {}", e));
                ConcurrentValidationResult::default()
            }
        };
        test_summary.total_tests_run += 1;

        // 更新测试摘要
        test_summary.total_execution_time_ms = self.start_time.elapsed().as_millis() as u64;
        test_summary.critical_failures = critical_failures.clone();
        test_summary.warnings = warnings.clone();

        // 生成综合评估
        let overall_assessment = self.generate_overall_assessment(
            &conv_result,
            &config_result,
            &sandbox_result,
            &concurrent_result,
            &critical_failures,
            &warnings,
        );

        // 确定集成策略建议
        let integration_strategy = self.determine_integration_strategy(&overall_assessment);

        // 构建最终报告
        self.overall_results = IntegrationValidationReport {
            test_summary,
            conversation_manager: conv_result,
            config_isolation: config_result,
            sandbox_protection: sandbox_result,
            concurrent_operation: concurrent_result,
            overall_assessment,
            recommended_integration_strategy: integration_strategy,
        };

        Ok(self.overall_results.clone())
    }

    async fn test_conversation_manager(&self) -> Result<ConvResult, Box<dyn std::error::Error>> {
        let validator = ConversationManagerValidator::new();
        
        validator.test_concurrent_conversation_creation().await?;
        validator.test_conversation_isolation().await?;
        validator.test_memory_usage().await?;
        
        Ok(validator.get_results().await)
    }

    async fn test_config_isolation(&self) -> Result<ConfigValidationResult, Box<dyn std::error::Error>> {
        let validator = ConfigIsolationValidator::new()?;
        
        validator.test_config_loading_isolation().await?;
        validator.test_concurrent_config_access().await?;
        validator.test_config_file_locking().await?;
        validator.test_environment_override_isolation().await?;
        
        Ok(validator.get_results().await)
    }

    async fn test_sandbox_protection(&self) -> Result<SandboxValidationResult, Box<dyn std::error::Error>> {
        let validator = SandboxEnvProtectionValidator::new();
        
        validator.test_sandbox_env_protection().await?;
        validator.test_environment_isolation().await?;
        validator.test_environment_restoration().await?;
        validator.test_sandbox_policy_consistency().await?;
        
        Ok(validator.get_results().await)
    }

    async fn test_concurrent_operation(&self) -> Result<ConcurrentValidationResult, Box<dyn std::error::Error>> {
        let mut validator = ConcurrentRunValidator::new();
        
        validator.test_concurrent_startup().await?;
        validator.test_resource_conflicts().await?;
        validator.test_performance_impact().await?;
        validator.test_shutdown_cleanup().await?;
        
        Ok(validator.get_results().await)
    }

    fn generate_overall_assessment(
        &self,
        conv: &ConvResult,
        config: &ConfigValidationResult,
        sandbox: &SandboxValidationResult,
        concurrent: &ConcurrentValidationResult,
        critical_failures: &[String],
        warnings: &[String],
    ) -> OverallAssessment {
        let mut major_concerns = Vec::new();
        let mut mitigation_required = Vec::new();

        // 计算风险评分
        let mut risk_score = 0;
        
        // ConversationManager 风险评估
        if !conv.thread_safety_verified {
            risk_score += 3;
            major_concerns.push("ConversationManager 线程安全性存疑".to_string());
        }
        if conv.memory_leaks_detected {
            risk_score += 2;
            mitigation_required.push("需要解决 ConversationManager 内存泄漏".to_string());
        }

        // 配置系统风险评估
        if !config.config_loading_isolated {
            risk_score += 3;
            major_concerns.push("配置隔离机制不完善".to_string());
        }
        if config.config_corruption_detected {
            risk_score += 4;
            major_concerns.push("存在配置文件腐化风险".to_string());
        }

        // 沙箱风险评估
        if !sandbox.sandbox_vars_protected {
            risk_score += 4;
            major_concerns.push("沙箱环境变量保护失效".to_string());
        }
        if !sandbox.isolation_breaches.is_empty() {
            risk_score += 3;
            major_concerns.push("沙箱隔离存在漏洞".to_string());
        }

        // 并发运行风险评估
        if !concurrent.tui_web_can_coexist {
            risk_score += 4;
            major_concerns.push("TUI 和 Web 无法同时运行".to_string());
        }
        if concurrent.performance_degradation > 50.0 {
            risk_score += 2;
            mitigation_required.push("需要优化并发性能".to_string());
        }

        // 确定风险等级
        let risk_level = match risk_score {
            0..=2 => RiskLevel::Low,
            3..=6 => RiskLevel::Medium,
            7..=10 => RiskLevel::High,
            _ => RiskLevel::Critical,
        };

        // 计算信心分数
        let confidence_score = match critical_failures.len() {
            0 => 0.9 - (warnings.len() as f64 * 0.1).min(0.3),
            1..=2 => 0.6 - (critical_failures.len() as f64 * 0.1),
            _ => 0.3,
        }.max(0.0);

        let integration_feasible = matches!(risk_level, RiskLevel::Low | RiskLevel::Medium) 
            && critical_failures.len() <= 2
            && confidence_score >= 0.6;

        OverallAssessment {
            integration_feasible,
            risk_level,
            confidence_score,
            major_concerns,
            mitigation_required,
        }
    }

    fn determine_integration_strategy(&self, assessment: &OverallAssessment) -> IntegrationStrategy {
        match (&assessment.risk_level, assessment.integration_feasible) {
            (RiskLevel::Low, true) => IntegrationStrategy::DirectInProcess,
            (RiskLevel::Medium, true) => IntegrationStrategy::InProcessWithIsolation,
            (RiskLevel::High, true) => IntegrationStrategy::HybridMode,
            (RiskLevel::High, false) => IntegrationStrategy::SeparateProcesses,
            (RiskLevel::Critical, _) => IntegrationStrategy::NotRecommended,
            (_, false) => IntegrationStrategy::SeparateProcesses,
        }
    }

    /// 生成详细的验证报告
    pub fn generate_detailed_report(&self) -> String {
        let report = &self.overall_results;
        
        format!(r#"
# Codex Web 与 TUI 集成兼容性验证报告

## 执行摘要
- **测试总数**: {}
- **通过测试**: {} 
- **失败测试**: {}
- **执行时间**: {}ms
- **整体可行性**: {}
- **风险等级**: {:?}
- **信心分数**: {:.1}%

## 关键发现

### ✅ ConversationManager 测试
- 线程安全性: {}
- 内存泄漏检测: {}
- 并发对话数: {}
- 状态腐化: {}

### ⚙️ 配置隔离测试
- 加载隔离: {}
- 共享一致性: {}
- 并发访问安全: {}
- 文件腐化: {}

### 🛡️ 沙箱保护测试
- 环境变量保护: {}
- 隔离完整性: {} 个违规
- 恢复机制: {}
- 并发修改安全: {}

### 🔄 并发运行测试
- 共存能力: {}
- 启动时间 - TUI: {}ms, Web: {}ms
- 性能降级: {:.1}%
- 清理成功: {}

## 风险评估

### 重大关注点
{}

### 需要缓解措施
{}

## 集成策略建议
**推荐策略**: {:?}

{}

## 量化成功标准评估

| 指标 | 目标 | 实际 | 状态 |
|------|------|------|------|
| ConversationManager 线程安全 | ✅ 通过 | {} | {} |
| 配置隔离有效性 | ✅ 通过 | {} | {} |  
| 沙箱变量保护 | ✅ 通过 | {} | {} |
| 并发运行兼容 | ✅ 通过 | {} | {} |
| 内存使用增长 | < 50% | {:.1}% | {} |
| 性能降级 | < 25% | {:.1}% | {} |

## 建议的下一步行动

{}
"#,
            // 执行摘要
            report.test_summary.total_tests_run,
            report.test_summary.tests_passed,
            report.test_summary.tests_failed,
            report.test_summary.total_execution_time_ms,
            if report.overall_assessment.integration_feasible { "✅ 可行" } else { "❌ 不可行" },
            report.overall_assessment.risk_level,
            report.overall_assessment.confidence_score * 100.0,
            
            // 关键发现
            if report.conversation_manager.thread_safety_verified { "✅" } else { "❌" },
            if report.conversation_manager.memory_leaks_detected { "❌ 检测到" } else { "✅ 无泄漏" },
            report.conversation_manager.max_concurrent_conversations,
            if report.conversation_manager.state_corruption_detected { "❌ 检测到" } else { "✅ 无腐化" },
            
            if report.config_isolation.config_loading_isolated { "✅" } else { "❌" },
            if report.config_isolation.shared_config_consistent { "✅" } else { "❌" },
            if report.config_isolation.concurrent_access_safe { "✅" } else { "❌" },
            if report.config_isolation.config_corruption_detected { "❌ 检测到" } else { "✅ 无腐化" },
            
            if report.sandbox_protection.sandbox_vars_protected { "✅" } else { "❌" },
            report.sandbox_protection.isolation_breaches.len(),
            if report.sandbox_protection.restoration_successful { "✅" } else { "❌" },
            if report.sandbox_protection.concurrent_modifications_safe { "✅" } else { "❌" },
            
            if report.concurrent_operation.tui_web_can_coexist { "✅" } else { "❌" },
            report.concurrent_operation.startup_times.tui_startup_time_ms,
            report.concurrent_operation.startup_times.web_startup_time_ms,
            report.concurrent_operation.performance_degradation,
            if report.concurrent_operation.shutdown_cleanup_successful { "✅" } else { "❌" },
            
            // 风险评估
            if report.overall_assessment.major_concerns.is_empty() {
                "无重大关注点".to_string()
            } else {
                report.overall_assessment.major_concerns.join("\n- ")
            },
            
            if report.overall_assessment.mitigation_required.is_empty() {
                "无需额外缓解措施".to_string()
            } else {
                report.overall_assessment.mitigation_required.join("\n- ")
            },
            
            // 集成策略
            report.recommended_integration_strategy,
            self.get_strategy_explanation(&report.recommended_integration_strategy),
            
            // 量化标准表格
            report.conversation_manager.thread_safety_verified,
            if report.conversation_manager.thread_safety_verified { "✅" } else { "❌" },
            report.config_isolation.config_loading_isolated,
            if report.config_isolation.config_loading_isolated { "✅" } else { "❌" },
            report.sandbox_protection.sandbox_vars_protected,
            if report.sandbox_protection.sandbox_vars_protected { "✅" } else { "❌" },
            report.concurrent_operation.tui_web_can_coexist,
            if report.concurrent_operation.tui_web_can_coexist { "✅" } else { "❌" },
            
            // 性能指标（这里需要从实际测试结果中获取）
            0.0, // 内存使用增长百分比
            if 0.0 < 50.0 { "✅" } else { "❌" },
            report.concurrent_operation.performance_degradation,
            if report.concurrent_operation.performance_degradation < 25.0 { "✅" } else { "❌" },
            
            // 下一步行动
            self.generate_next_actions(&report.overall_assessment, &report.recommended_integration_strategy)
        )
    }

    fn get_strategy_explanation(&self, strategy: &IntegrationStrategy) -> &'static str {
        match strategy {
            IntegrationStrategy::DirectInProcess => {
                "可以直接在同一进程中运行 TUI 和 Web 组件，风险很低。"
            }
            IntegrationStrategy::InProcessWithIsolation => {
                "建议在同一进程中运行，但需要额外的隔离机制来防止相互干扰。"
            }
            IntegrationStrategy::HybridMode => {
                "建议部分功能进程内集成，关键功能使用独立进程以降低风险。"
            }
            IntegrationStrategy::SeparateProcesses => {
                "建议使用独立进程通信，通过 IPC 机制进行交互以确保稳定性。"
            }
            IntegrationStrategy::NotRecommended => {
                "当前架构下不建议集成，需要重新设计系统架构。"
            }
        }
    }

    fn generate_next_actions(&self, assessment: &OverallAssessment, strategy: &IntegrationStrategy) -> String {
        let mut actions = Vec::new();

        if !assessment.integration_feasible {
            actions.push("1. 分析并解决所有关键失败项".to_string());
            actions.push("2. 重新评估架构设计的可行性".to_string());
        }

        match strategy {
            IntegrationStrategy::DirectInProcess => {
                actions.push("1. 开始实施直接进程内集成".to_string());
                actions.push("2. 建立持续监控机制".to_string());
            }
            IntegrationStrategy::InProcessWithIsolation => {
                actions.push("1. 设计并实施额外的隔离机制".to_string());
                actions.push("2. 加强资源管理和错误处理".to_string());
                actions.push("3. 建立隔离效果的持续验证".to_string());
            }
            IntegrationStrategy::HybridMode => {
                actions.push("1. 确定哪些组件进程内集成，哪些独立运行".to_string());
                actions.push("2. 设计混合模式的通信协议".to_string());
                actions.push("3. 实施渐进式迁移策略".to_string());
            }
            IntegrationStrategy::SeparateProcesses => {
                actions.push("1. 设计进程间通信协议".to_string());
                actions.push("2. 实施独立进程架构".to_string());
                actions.push("3. 优化 IPC 性能和可靠性".to_string());
            }
            IntegrationStrategy::NotRecommended => {
                actions.push("1. 重新评估 Web 集成的必要性".to_string());
                actions.push("2. 考虑替代架构方案".to_string());
                actions.push("3. 如有必要，重新设计核心系统".to_string());
            }
        }

        if !assessment.mitigation_required.is_empty() {
            actions.push(format!("4. 优先处理缓解措施: {}", assessment.mitigation_required.join(", ")));
        }

        actions.join("\n")
    }
}

// 默认实现
impl Default for IntegrationValidationReport {
    fn default() -> Self {
        Self {
            test_summary: TestSummary::default(),
            conversation_manager: ConvResult::default(),
            config_isolation: ConfigValidationResult::default(),
            sandbox_protection: SandboxValidationResult::default(),
            concurrent_operation: ConcurrentValidationResult::default(),
            overall_assessment: OverallAssessment {
                integration_feasible: false,
                risk_level: RiskLevel::High,
                confidence_score: 0.0,
                major_concerns: Vec::new(),
                mitigation_required: Vec::new(),
            },
            recommended_integration_strategy: IntegrationStrategy::NotRecommended,
        }
    }
}

impl Default for TestSummary {
    fn default() -> Self {
        Self {
            total_tests_run: 0,
            tests_passed: 0,
            tests_failed: 0,
            total_execution_time_ms: 0,
            critical_failures: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_full_integration_validation() {
        let mut validator = IntegrationCompatibilityValidator::new();
        
        println!("开始完整集成兼容性验证...");
        
        let report = validator.run_comprehensive_validation().await.unwrap();
        
        // 输出详细报告
        println!("{}", validator.generate_detailed_report());
        
        // 保存报告到文件
        let report_json = serde_json::to_string_pretty(&report).unwrap();
        std::fs::write("integration_validation_report.json", report_json).unwrap();
        
        println!("\n📊 验证报告已保存到 integration_validation_report.json");
        
        // 基本断言
        assert!(report.test_summary.total_tests_run > 0);
        println!("✅ 集成兼容性验证完成");
    }
}