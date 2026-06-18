//! 提示词构建插件（prompt_builder）。
//!
//! # 职责
//!
//! 在 `build_prompt` 阶段从角色卡数据和对话历史组装 system prompt。
//! 纯函数，无网络调用，无副作用。
//!
//! # 输入
//!
//! - `ctx.custom["character_card.data"]` — 角色数据（由 character_card 插件写入）
//! - `ctx.messages` — 对话历史
//!
//! # 输出
//!
//! - `ctx.custom["prompt_builder.output"]` — 构建好的 system prompt 字符串
//!
//! # Prompt 格式
//!
//! ```text
//! [System]
//! (system_prompt 字段或默认规则)
//!
//! [Character]
//! Name: (name)
//!
//! [Description]
//! (description)
//!
//! [Personality]
//! (personality)
//!
//! [Scenario]
//! (scenario)
//!
//! [Example Dialogues]
//! (example_dialogue)
//!
//! [Creator Notes]
//! (creator_notes)
//!
//! [Conversation History]
//! (messages 序列化为 User:/Assistant: 格式)
//! ```

use serde_json::Value;
use tracing::{debug, info, warn};

use zsai_core::*;

// ============================================================
// 插件结构体
// ============================================================

struct PromptBuilderPlugin {
    meta: PluginMeta,
}

impl PromptBuilderPlugin {
    fn new() -> Self {
        PromptBuilderPlugin {
            meta: PluginMeta::new("prompt_builder", (0, 1, 0), "build_prompt", 10)
                .with_capability("health_check"),
        }
    }

    /// 从角色卡 JSON 构建 system prompt 字符串。
    ///
    /// 纯函数：给定角色数据和消息历史，返回格式化的 prompt。
    fn build_prompt(card: &Value, messages: &[Message]) -> String {
        let mut prompt = String::with_capacity(4096);

        // ---- [System] ----
        prompt.push_str("[System]\n\n");
        if let Some(sys) = card.get("system_prompt").and_then(|v| v.as_str()) {
            if !sys.is_empty() {
                prompt.push_str(sys);
                prompt.push_str("\n\n");
            }
        }
        // 默认行为规则
        prompt.push_str("Write the next reply as the character.\n");
        prompt.push_str("Remain fully in character.\n");
        prompt.push_str("Maintain personality, background, and goals consistency.\n");
        prompt.push_str("Use conversation history naturally.\n");
        prompt.push_str("Respect the user's autonomy.\n");
        prompt.push_str("Never determine the user's dialogue, thoughts, or actions.\n");
        prompt.push_str("Advance the scene logically and coherently.\n");
        prompt.push_str("Third-person perspective recommended for multi-character clarity.\n\n");

        // ---- [Character] ----
        let name = card.get("name").and_then(|v| v.as_str()).unwrap_or("Unknown");
        prompt.push_str("[Character]\n");
        prompt.push_str(&format!("Name: {}\n\n", name));

        // ---- [Description] ----
        if let Some(desc) = card.get("description").and_then(|v| v.as_str()) {
            if !desc.is_empty() {
                prompt.push_str("[Description]\n");
                prompt.push_str(desc);
                prompt.push_str("\n\n");
            }
        }

        // ---- [Personality] ----
        if let Some(pers) = card.get("personality").and_then(|v| v.as_str()) {
            if !pers.is_empty() {
                prompt.push_str("[Personality]\n");
                prompt.push_str(pers);
                prompt.push_str("\n\n");
            }
        }

        // ---- [Scenario] ----
        if let Some(scene) = card.get("scenario").and_then(|v| v.as_str()) {
            if !scene.is_empty() {
                prompt.push_str("[Scenario]\n");
                prompt.push_str(scene);
                prompt.push_str("\n\n");
            }
        }

        // ---- [Post History Instructions] ----
        if let Some(phi) = card.get("post_history_instructions").and_then(|v| v.as_str()) {
            if !phi.is_empty() {
                prompt.push_str("[Post History Instructions]\n");
                prompt.push_str(phi);
                prompt.push_str("\n\n");
            }
        }

        // ---- [Example Dialogues] ----
        if let Some(ex) = card.get("example_dialogue").and_then(|v| v.as_str()) {
            if !ex.is_empty() {
                prompt.push_str("[Example Dialogues]\n");
                prompt.push_str(ex);
                prompt.push_str("\n\n");
            }
        }

        // ---- [Creator Notes] ----
        if let Some(notes) = card.get("creator_notes").and_then(|v| v.as_str()) {
            if !notes.is_empty() {
                prompt.push_str("[Creator Notes]\n");
                prompt.push_str(notes);
                prompt.push_str("\n\n");
            }
        }

        // ---- [Conversation History] ----
        prompt.push_str("[Conversation History]\n");
        for msg in messages {
            match msg.role.as_str() {
                "user" => {
                    prompt.push_str(&format!("User: {}\n", msg.content));
                }
                "assistant" => {
                    prompt.push_str(&format!("Assistant: {}\n", msg.content));
                }
                "system" => {
                    // system 消息是元数据，不写入对话历史区
                    // （除非它是 first_mes 之类的实际内容）
                    if msg.content.len() < 200
                        && !msg.content.starts_with("[test_stub")
                    {
                        prompt.push_str(&format!("Assistant: {}\n", msg.content));
                    }
                }
                _ => {}
            }
        }

        debug!(prompt_len = prompt.len(), name = name, "system prompt 构建完成");
        prompt
    }
}

// ============================================================
// Plugin trait 实现
// ============================================================

impl Plugin for PromptBuilderPlugin {
    fn metadata(&self) -> PluginMeta {
        self.meta.clone()
    }

    fn execute(&self, ctx: &mut Context) -> Result<PluginResult> {
        info!(session = %ctx.session_id, "构建 system prompt");

        // 从 Context 读取角色卡数据
        let card_data = match ctx.get_custom_value("character_card.data") {
            Some(v) => v.clone(),
            None => {
                warn!("未找到角色卡数据 (character_card.data)，使用空角色卡");
                serde_json::json!({})
            }
        };

        let prompt = Self::build_prompt(&card_data, &ctx.messages);

        // 写入输出
        ctx.set_custom("prompt_builder.output", &prompt)
            .map_err(|e| CoreError::plugin(
                "prompt_builder",
                format!("写入 prompt_builder.output 失败: {}", e),
            ))?;

        // 将 system prompt 作为第一条消息插入（如果 messages 中没有 system 消息）
        let has_system = ctx.messages.iter().any(|m| m.role == "system" && m.content.len() > 200);
        if !has_system {
            ctx.messages.insert(0, Message::system(&prompt));
        }

        info!(prompt_len = prompt.len(), "system prompt 已写入 Context");
        Ok(PluginResult::r#continue())
    }

    // ---- 可选能力 ----

    fn health_check(&self) -> Result<HealthStatus> {
        Ok(HealthStatus::healthy())
    }
}

// ============================================================
// FFI 导出
// ============================================================

#[no_mangle]
pub extern "C" fn create_plugin() -> Box<dyn Plugin> {
    Box::new(PromptBuilderPlugin::new())
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use zsai_core::{Context, Plugin};

    /// 构建一个测试用角色卡数据
    fn test_card() -> Value {
        serde_json::json!({
            "name": "李白",
            "description": "唐朝伟大诗人",
            "personality": "豪放、浪漫",
            "scenario": "酒肆之中，月色皎洁",
            "first_mes": "哈哈哈！来得正好！",
            "example_dialogue": "User: 你好\nAssistant: 哈哈！",
            "system_prompt": "你扮演李白。保持豪放气质。",
            "creator_notes": "测试用"
        })
    }

    #[test]
    fn test_metadata() {
        let plugin = PromptBuilderPlugin::new();
        let meta = plugin.metadata();
        assert_eq!(meta.name, "prompt_builder");
        assert_eq!(meta.stage, "build_prompt");
    }

    #[test]
    fn test_build_prompt_contains_sections() {
        let card = test_card();
        let messages = vec![
            Message::assistant("哈哈哈！来得正好！"),
            Message::user("李白兄好！"),
        ];
        let prompt = PromptBuilderPlugin::build_prompt(&card, &messages);

        assert!(prompt.contains("[System]"));
        assert!(prompt.contains("[Character]"));
        assert!(prompt.contains("Name: 李白"));
        assert!(prompt.contains("[Description]"));
        assert!(prompt.contains("唐朝伟大诗人"));
        assert!(prompt.contains("[Personality]"));
        assert!(prompt.contains("[Scenario]"));
        assert!(prompt.contains("[Example Dialogues]"));
        assert!(prompt.contains("[Creator Notes]"));
        assert!(prompt.contains("[Conversation History]"));
        assert!(prompt.contains("User: 李白兄好！"));
        assert!(prompt.contains("Assistant: 哈哈哈！来得正好！"));
    }

    #[test]
    fn test_build_prompt_with_empty_card() {
        let card = serde_json::json!({"name": "test"});
        let messages = vec![];
        let prompt = PromptBuilderPlugin::build_prompt(&card, &messages);

        assert!(prompt.contains("[System]"));
        assert!(prompt.contains("Name: test"));
        // 没有 description/personality 时不包含对应段
        assert!(!prompt.contains("[Description]"));
    }

    #[test]
    fn test_execute_writes_prompt() {
        let plugin = PromptBuilderPlugin::new();
        let mut ctx = Context::new("test");
        ctx.set_custom_value("character_card.data", test_card());
        ctx.push_message("user", "你好");

        let result = plugin.execute(&mut ctx);
        assert!(result.is_ok());

        let prompt: String = ctx.get_custom("prompt_builder.output").unwrap();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("[System]"));
        assert!(prompt.contains("李白"));
    }

    #[test]
    fn test_execute_without_card_uses_empty() {
        let plugin = PromptBuilderPlugin::new();
        let mut ctx = Context::new("test");

        let result = plugin.execute(&mut ctx);
        assert!(result.is_ok());

        let prompt: String = ctx.get_custom("prompt_builder.output").unwrap();
        assert!(prompt.contains("Name: Unknown"));
    }

    #[test]
    fn test_prompt_contains_default_rules() {
        let card = test_card();
        let prompt = PromptBuilderPlugin::build_prompt(&card, &[]);

        assert!(prompt.contains("Write the next reply as the character."));
        assert!(prompt.contains("Remain fully in character."));
        assert!(prompt.contains("Never determine the user's dialogue"));
    }
}
