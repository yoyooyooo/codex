// 集成测试执行器 - 可独立运行验证实验

use codex_integration_test::IntegrationCompatibilityValidator;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 设置环境
    env::set_var("RUST_LOG", "info");
    
    println!("🚀 Codex Web 与 TUI 集成兼容性验证实验");
    println!("==========================================");
    
    let mut validator = IntegrationCompatibilityValidator::new();
    
    // 执行完整验证
    match validator.run_comprehensive_validation().await {
        Ok(report) => {
            // 输出详细报告
            println!("{}", validator.generate_detailed_report());
            
            // 保存 JSON 报告
            let report_json = serde_json::to_string_pretty(&report)?;
            std::fs::write("integration_validation_report.json", &report_json)?;
            
            // 保存 Markdown 报告
            std::fs::write("integration_validation_report.md", validator.generate_detailed_report())?;
            
            println!("\n📊 报告已保存:");
            println!("   - JSON: integration_validation_report.json");
            println!("   - Markdown: integration_validation_report.md");
            
            // 返回适当的退出代码
            if report.overall_assessment.integration_feasible {
                println!("\n✅ 集成验证通过 - 可以继续实施 Web 集成");
                std::process::exit(0);
            } else {
                println!("\n⚠️  集成验证发现重大问题 - 建议修复后重新验证");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("❌ 验证执行失败: {}", e);
            std::process::exit(2);
        }
    }
}