# 局部特性：进程内调用（Rust）从 0 到 1

本文演示如何在同一进程内直接调用 Codex 的模型能力，用于局部特性（如提示词优化、标题生成等），不影响主聊天会话。示例基于仓库内 `codex-rs` 工作区的公共 API（`codex-core`）。

## 前置条件
- 已能编译本仓库（见 docs/install.md）。
- 已完成认证（任选其一）：
  - 交互式登录：`codex login`
  - 或在 `~/.codex/config.toml` 配置 API key（见 docs/authentication.md）。

## 设计要点
- 使用 `ConversationManager` 新建独立会话，完全隔离历史，不污染主对话。
- 使用 `ConfigOverrides` 精准控制本次调用：
  - 设为非交互批准：`AskForApproval::Never`
  - 安全沙箱：推荐 `SandboxMode::ReadOnly`（只读）；确需写入再用 `WorkspaceWrite`。
  - 禁用非必须工具：`include_plan_tool=false`、`include_apply_patch_tool=false`、`include_view_image_tool=false`、`tools_web_search_request=false`
  - 自定义系统提示词（完全替换）：`base_instructions = "你是提示词优化器…禁止调用任何工具，只输出最终文本"`
- 提交一次 `Op::UserInput`，消费事件直到 `TaskComplete`，读取 `last_agent_message` 作为结果。

## 最小可用示例

在工作区内的任意二进制 crate 中增加如下代码（或直接在现有可执行里调用）：

```rust
use codex_core::{
    AuthManager, ConversationManager, NewConversation,
    config::{Config, ConfigOverrides},
    protocol::{EventMsg, InputItem, Op, TaskCompleteEvent, AskForApproval},
};
use codex_protocol::config_types::SandboxMode;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1) 构造强类型 overrides（优先级最高）
    let overrides = ConfigOverrides {
        // 可选：覆盖使用的模型与提供方
        // model: Some("gpt-4o".to_string()),
        // model_provider: Some("openai".to_string()),

        approval_policy: Some(AskForApproval::Never),
        sandbox_mode: Some(SandboxMode::ReadOnly),
        include_plan_tool: Some(false),
        include_apply_patch_tool: Some(false),
        include_view_image_tool: Some(false),
        tools_web_search_request: Some(false),
        base_instructions: Some(
            "你是提示词优化器。要求：\n\
             - 禁止调用任何工具，只输出优化后的提示词文本；\n\
             - 不要解释、不加前后缀；\n\
             - 语言保持与输入一致。"
                .to_string(),
        ),
        ..Default::default()
    };

    // 2) 无 `-c` 形式的 CLI overrides，这里传空（如需可从命令行收集后传入）
    let config = Config::load_with_cli_overrides(vec![], overrides)?;

    // 3) 新建独立会话
    let cm = ConversationManager::new(AuthManager::shared(
        config.codex_home.clone(),
        config.preferred_auth_method,
        config.responses_originator_header.clone(),
    ));
    let NewConversation { conversation, .. } = cm.new_conversation(config).await?;

    // 4) 发送一次性输入（示例：优化提示词）
    let prompt = "帮我把这个提示词更简洁高效：用要点列出改进点".to_string();
    let items = vec![InputItem::Text { text: prompt }];
    let submit_id = conversation.submit(Op::UserInput { items }).await?;

    // 5) 消费事件，直到 TaskComplete，取 last_agent_message
    let mut final_text: Option<String> = None;
    while let Ok(event) = conversation.next_event().await {
        match event.msg {
            EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message })
                if event.id == submit_id =>
            {
                final_text = last_agent_message;
                break;
            }
            EventMsg::Error(e) if event.id == submit_id => {
                anyhow::bail!(e.message);
            }
            _ => {}
        }
    }

    println!("{}", final_text.unwrap_or_else(|| "<无输出>".to_string()));
    Ok(())
}
```

> 提示：`base_instructions` 为“完全替换”模式。若要在默认系统提示词基础上追加，自行读取 `AGENTS.md` 拼接后再传入。

## 依赖与构建
- 上述代码位于本工作区内时，可直接依赖内部 crate（无需发布到 crates.io）。
- 如需在外部项目试验，建议先在本工作区增加你的二进制 crate，或通过 path 依赖引用本仓库的 `codex-core`（注意版本一致性）。

## 常见问题与建议
- 为什么不“彻底禁用”工具注入？目前 `shell` 工具属于核心能力，不提供硬开关。强烈建议在 `base_instructions` 明确“禁止调用任何工具”，并结合只读沙箱。
- 如何切换模型/提供方？可以在 `ConfigOverrides` 中设置 `model` 与 `model_provider`，或使用 `config.toml` 的 profile（见 docs/config.md）。
- 如何保留结果但不打印过程？事件循环里只处理 `TaskComplete` 与 `Error`，忽略其他事件即可。

---

## 内部实现一瞥（便于深入集成）
- Turn 执行：`codex-rs/core/src/codex.rs` 中 `run_turn` / `drain_to_completed` 负责将 `Prompt` 序列化为请求、消费流式响应、映射为事件。
- Prompt 构造：`codex-rs/core/src/client_common.rs::Prompt`，`get_full_instructions()` 将 `base_instructions`（或默认系统提示）与必要补充（apply_patch 说明）拼接。
- 工具注入：`codex-rs/core/src/openai_tools.rs::ToolsConfig` 根据模型族/批准/沙箱等决定是否添加 `shell`/`apply_patch`/`view_image`/`web_search`。
- 客户端与重试：`codex-rs/core/src/client.rs::ModelClient` 负责 Responses API 请求、SSE 处理与重试策略。
- 任务/提交：`Op::UserInput`、`EventMsg::*` 等协议在 `codex-rs/protocol/src/protocol.rs` 定义；提交队列与事件队列的编排见 `codex-rs/core/src/codex.rs`。
