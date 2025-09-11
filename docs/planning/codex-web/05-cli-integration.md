# CLI 集成设计

**文档版本**: v1.0  
**最后更新**: 2025-09-11  
**依赖文档**: [02-architecture.md](02-architecture.md), [03-backend-design.md](03-backend-design.md)

## 命令行集成方案

### 新增子命令
```rust
// codex-rs/cli/src/main.rs
#[derive(Parser)]
pub enum Subcommand {
    // 现有子命令...
    
    /// 启动 Web 服务器
    #[command(name = "web")]
    Web(WebCommand),
}

#[derive(Parser)]
pub struct WebCommand {
    /// 服务器端口
    #[arg(long, short = 'p')]
    pub port: Option<u16>,
    
    /// 绑定地址
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
    
    /// 不自动打开浏览器
    #[arg(long)]
    pub no_open: bool,
    
    /// 静态文件目录
    #[arg(long)]
    pub static_dir: Option<String>,
    
    /// 开发代理 URL
    #[arg(long)]
    pub dev_proxy: Option<String>,
    
    // 复用现有覆盖项
    #[command(flatten)]
    pub overrides: CliConfigOverrides,
}
```

### 快捷开关
```rust
// 在顶层 CLI 结构中添加 --web 选项
#[derive(Parser)]
#[command(name = "codex")]
pub struct MultitoolCli {
    #[command(subcommand)]
    pub subcommand: Option<Subcommand>,
    
    /// 启动 Web 服务器（等同于 codex web）
    #[arg(long, global = true)]
    pub web: bool,
    
    #[command(flatten)]
    pub overrides: CliConfigOverrides,
}
```

### 启动流程
```rust
// 主命令处理
pub async fn run_cli() -> Result<(), Box<dyn std::error::Error>> {
    let cli = MultitoolCli::parse();
    
    // 处理 --web 快捷开关
    if cli.web {
        return run_web_server(WebCommand {
            port: None,
            host: "127.0.0.1".to_string(),
            no_open: false,
            static_dir: None,
            dev_proxy: None,
            overrides: cli.overrides,
        }).await;
    }
    
    match cli.subcommand {
        Some(Subcommand::Web(web_cmd)) => {
            run_web_server(web_cmd).await
        }
        // 其他子命令...
    }
}

// Web 服务器启动
async fn run_web_server(cmd: WebCommand) -> Result<(), Box<dyn std::error::Error>> {
    // 1. 加载配置
    let config = Config::load_with_cli_overrides(&cmd.overrides)?;
    
    // 2. 初始化 Web 服务
    let web_config = WebServerConfig {
        host: cmd.host,
        port: cmd.port,
        static_dir: cmd.static_dir,
        dev_proxy_url: cmd.dev_proxy,
        ..Default::default()
    };
    
    // 3. 启动服务器
    let server = codex_web::WebServer::new(web_config, config).await?;
    let addr = server.start().await?;
    
    // 4. 打开浏览器
    if !cmd.no_open {
        open_browser(&format!("http://{}", addr))?;
    }
    
    println!("🌐 Codex Web 服务在 http://{} 运行", addr);
    
    Ok(())
}
```

### 打开浏览器实现
```rust
fn open_browser(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(url).spawn()?;
    
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open").arg(url).spawn()?;
    
    #[cfg(target_os = "windows")]
    std::process::Command::new("cmd")
        .args(["/c", "start", url])
        .spawn()?;
    
    Ok(())
}
```

### 配置透传
```rust
// 确保所有现有配置都能透传到 Web 服务
impl WebCommand {
    pub fn to_cli_overrides(&self) -> CliConfigOverrides {
        self.overrides.clone()
    }
    
    pub fn effective_config(&self) -> Result<Config, ConfigError> {
        Config::load_with_cli_overrides(&self.overrides)
    }
}
```

---
**变更记录**：
- v1.0 (2025-09-11): CLI 集成设计文档