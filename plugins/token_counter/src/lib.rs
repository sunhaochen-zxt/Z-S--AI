//! Token 计数插件（token_counter）。
//!
//! # 职责
//!
//! 在 `postprocess` 阶段估算对话的 token 用量，写入 Context 供监控和裁剪。
//!
//! # 估算算法
//!
//! 粗略估算（无真实 tokenizer）：
//! - 中文字符 ≈ 1.5 tokens/char → 简化为 2 chars ≈ 1 token
//! - 英文单词 ≈ 1.3 tokens/word
//! - 综合：`content.len() / 2` 作为保守估计
//!
//! # 输入
//!
//! - `ctx.messages` — 对话历史
//!
//! # 输出
//!
//! - `ctx.custom["token_counter.estimate"]` — { total, system, conversation }
//! - `ctx.custom["token_counter.warning"]` — 如果超限，写入警告信息

use tracing::{debug, info, warn};

use zsai_core::*;

// ============================================================
// Token 估算
// ============================================================

/// 粗略估算一段文本的 token 数。
fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() { return 0; }
    // 简单启发式：中文字符密度高，用 char 数 / 1.5
    // 英文用字节数 / 4。折中：char 数 / 2（略微高估）
    (text.chars().count() / 2).max(1)
}

/// 估算消息列表的总 token 数。
fn estimate_messages(msgs: &[Message]) -> usize {
    msgs.iter().map(|m| estimate_tokens(&m.content)).sum()
}

// ============================================================
// 插件结构体
// ============================================================

struct TokenCounterPlugin {
    meta: PluginMeta,
    /// 警告阈值
    max_tokens: usize,
}

impl TokenCounterPlugin {
    fn new() -> Self {
        TokenCounterPlugin {
            meta: PluginMeta::new("token_counter", (0, 1, 0), "postprocess", 5)
                .with_capability("health_check"),
            max_tokens: 32768,
        }
    }
}

impl Plugin for TokenCounterPlugin {
    fn metadata(&self) -> PluginMeta { self.meta.clone() }

    fn execute(&self, ctx: &mut Context) -> Result<PluginResult> {
        let msgs = &ctx.messages;
        if msgs.is_empty() { return Ok(PluginResult::r#continue()); }

        // 分开统计 system prompt 和对话
        let system_tokens: usize = msgs.iter()
            .filter(|m| m.role == "system")
            .map(|m| estimate_tokens(&m.content))
            .sum();

        let conv_tokens: usize = msgs.iter()
            .filter(|m| m.role != "system")
            .map(|m| estimate_tokens(&m.content))
            .sum();

        let total = system_tokens + conv_tokens;

        // 写入统计
        ctx.set_custom("token_counter.estimate", &serde_json::json!({
            "total": total,
            "system": system_tokens,
            "conversation": conv_tokens,
            "max": self.max_tokens,
            "usage_percent": if self.max_tokens > 0 {
                (total * 100 / self.max_tokens) as f64
            } else { 0.0 },
        })).ok();

        // 超限警告
        if total > self.max_tokens {
            warn!(total, max = self.max_tokens, percent = total * 100 / self.max_tokens,
                "⚠ Token 用量超限！");
            ctx.set_custom_value("token_counter.warning", serde_json::json!({
                "message": format!("Token 用量超限: {} / {} ({}%)", total, self.max_tokens, total * 100 / self.max_tokens),
                "total": total,
                "max": self.max_tokens,
            }));
        } else {
            debug!(total, max = self.max_tokens, "Token 用量正常");
        }

        Ok(PluginResult::r#continue())
    }

    fn init(&mut self, config: &serde_json::Value) -> Result<()> {
        if let Some(n) = config.get("max_tokens").and_then(|v| v.as_u64()) {
            self.max_tokens = n as usize;
        }
        Ok(())
    }

    fn health_check(&self) -> Result<HealthStatus> {
        Ok(HealthStatus::healthy())
    }
}

// ============================================================
// FFI 导出
// ============================================================

#[no_mangle]
pub extern "C" fn create_plugin() -> Box<dyn Plugin> {
    Box::new(TokenCounterPlugin::new())
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_estimate_chinese() {
        // 10 个中文字符 → 约 5 tokens
        let t = estimate_tokens("你好世界这是一个测试");
        assert!(t >= 3 && t <= 8);
    }

    #[test]
    fn test_estimate_english() {
        let t = estimate_tokens("Hello world this is a test");
        assert!(t > 0);
    }

    #[test]
    fn test_estimate_messages() {
        let msgs = vec![
            Message::system("You are a helpful assistant. "),
            Message::user("你好！"),
            Message::assistant("你好！有什么可以帮助你的？"),
        ];
        let total = estimate_messages(&msgs);
        assert!(total > 0);
    }

    #[test]
    fn test_metadata() {
        let p = TokenCounterPlugin::new();
        assert_eq!(p.metadata().name, "token_counter");
        assert_eq!(p.metadata().stage, "postprocess");
    }

    #[test]
    fn test_execute() {
        let p = TokenCounterPlugin::new();
        let mut ctx = Context::new("test");
        ctx.push_message("system", "You are helpful.");
        ctx.push_message("user", "Hi");
        ctx.push_message("assistant", "Hello!");

        let r = p.execute(&mut ctx);
        assert!(r.is_ok());

        let est: serde_json::Value = ctx.get_custom("token_counter.estimate").unwrap();
        assert!(est["total"].as_u64().unwrap() > 0);
    }
}
