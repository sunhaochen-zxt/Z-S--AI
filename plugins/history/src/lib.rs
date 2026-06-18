//! 历史记录插件（history）。
//!
//! # 职责
//!
//! 在 `postprocess` 阶段保存对话历史，支持按 session 隔离存储。
//!
//! # 输入
//!
//! - `ctx.messages` — 对话历史
//! - `ctx.session_id` — 会话标识
//! - `ctx.custom["history.save_directory"]` — 存储目录（可选，默认 `./data/history`）
//! - `ctx.custom["history.max_tokens"]` — token 上限（可选，默认 32768）
//!
//! # 输出
//!
//! - `ctx.custom["history.path"]` — 保存的文件路径
//!
//! # 配置（config.toml）
//!
//! ```toml
//! [plugins.history]
//! save_directory = "./data/history"
//! max_tokens = 32768
//! save_on_message = true
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tracing::{debug, info, warn};

use zsai_core::*;

// ============================================================
// 存储格式
// ============================================================

/// 对话历史的磁盘存储格式。
#[derive(serde::Serialize, serde::Deserialize)]
struct HistoryFile {
    /// 会话 ID
    session_id: String,
    /// 最后一次更新的时间戳
    updated_at: String,
    /// 消息数量
    message_count: usize,
    /// 消息列表
    messages: Vec<Message>,
}

impl HistoryFile {
    fn from_context(ctx: &Context) -> Self {
        HistoryFile {
            session_id: ctx.session_id.clone(),
            updated_at: chrono_now(),
            message_count: ctx.messages.len(),
            messages: ctx.messages.clone(),
        }
    }

    #[allow(dead_code)]
    fn to_messages(&self) -> Vec<Message> {
        self.messages.clone()
    }
}

/// 简易时间戳生成（不依赖 chrono crate）。
fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // 简单的 RFC3339 格式
    let minutes = (secs / 60) % 60;
    let hours = (secs / 3600) % 24;
    let days_since_epoch = secs / 86400;
    // 粗略的日期计算（1970-01-01 + days）
    let year = 1970 + (days_since_epoch / 365) as i64;
    let day_of_year = (days_since_epoch % 365) as u32;
    let month = (day_of_year / 30 + 1).min(12);
    let day = (day_of_year % 30 + 1).min(31);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, secs % 60
    )
}

// ============================================================
// 插件结构体
// ============================================================

struct HistoryPlugin {
    meta: PluginMeta,
    /// 历史记录存储目录
    save_directory: String,
    /// 上下文 token 上限（超限自动裁剪）
    max_tokens: usize,
    /// 是否每条消息后自动保存
    save_on_message: bool,
}

impl HistoryPlugin {
    fn new() -> Self {
        HistoryPlugin {
            meta: PluginMeta::new("history", (0, 1, 0), "postprocess", 10)
                .with_capability("health_check")
                .with_capability("validate_config"),
            save_directory: "./data/history".to_string(),
            max_tokens: 32768,
            save_on_message: true,
        }
    }

    /// 获取 session 对应的历史文件路径。
    fn history_path(&self, session_id: &str) -> PathBuf {
        let dir = Path::new(&self.save_directory);
        dir.join(format!("{}.json", session_id))
    }

    /// 从磁盘加载对话历史（预留——preprocess 阶段使用）。
    #[allow(dead_code)]
    fn load_history(&self, session_id: &str) -> Option<Vec<Message>> {
        let path = self.history_path(session_id);
        if !path.exists() {
            debug!(session = %session_id, "未找到历史记录文件");
            return None;
        }

        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<HistoryFile>(&content) {
                Ok(history) => {
                    info!(
                        session = %session_id,
                        messages = history.message_count,
                        "加载对话历史"
                    );
                    Some(history.to_messages())
                }
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "历史文件解析失败");
                    None
                }
            },
            Err(e) => {
                warn!(path = %path.display(), error = %e, "无法读取历史文件");
                None
            }
        }
    }

    /// 保存对话历史到磁盘。
    fn save_history(&self, ctx: &Context) {
        let path = self.history_path(&ctx.session_id);

        // 确保目录存在
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                if let Err(e) = fs::create_dir_all(parent) {
                    warn!(dir = %parent.display(), error = %e, "无法创建历史存储目录");
                    return;
                }
            }
        }

        let history = HistoryFile::from_context(ctx);

        match serde_json::to_string_pretty(&history) {
            Ok(json) => {
                match fs::write(&path, &json) {
                    Ok(()) => {
                        debug!(
                            path = %path.display(),
                            messages = history.message_count,
                            "对话历史已保存"
                        );
                    }
                    Err(e) => {
                        warn!(path = %path.display(), error = %e, "无法写入历史文件");
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "历史记录序列化失败");
            }
        }
    }

    /// 裁剪上下文：保留 system prompt + 最近 N 条消息。
    ///
    /// 粗略估计：中文约 1.5 字符/token，英文约 4 字符/token。
    fn trim_context(&self, messages: &mut Vec<Message>) {
        if messages.is_empty() {
            return;
        }

        // 粗略估算当前 token 数
        let estimate_tokens = |msgs: &[Message]| -> usize {
            msgs.iter()
                .map(|m| m.content.len() / 2) // 粗略: 2 字符 ≈ 1 token
                .sum()
        };

        let total = estimate_tokens(messages);
        if total <= self.max_tokens {
            return;
        }

        debug!(total_est = total, max = self.max_tokens, "触发上下文裁剪");

        // 保留第一条（通常是 system prompt）
        let system_msg = messages.first().cloned();

        // 从尾部保留消息直到接近限制
        let mut kept: Vec<Message> = Vec::new();
        let mut current_tokens = 0;

        // 从尾部向前保留
        for msg in messages.iter().rev() {
            let msg_tokens = msg.content.len() / 2;
            if current_tokens + msg_tokens > self.max_tokens && !kept.is_empty() {
                break;
            }
            current_tokens += msg_tokens;
            kept.push(msg.clone());
        }

        kept.reverse();

        // 确保第一条是 system prompt
        if let Some(sys) = system_msg {
            if kept.first().map(|m| &m.role) != Some(&"system".to_string()) {
                kept.insert(0, sys);
            }
        }

        let removed = messages.len() - kept.len();
        *messages = kept;

        info!(removed, kept = messages.len(), "上下文裁剪完成");
    }
}

// ============================================================
// Plugin trait 实现
// ============================================================

impl Plugin for HistoryPlugin {
    fn metadata(&self) -> PluginMeta {
        self.meta.clone()
    }

    fn execute(&self, ctx: &mut Context) -> Result<PluginResult> {
        // 1. 上下文裁剪
        self.trim_context(&mut ctx.messages);

        // 2. 保存历史
        if self.save_on_message {
            self.save_history(ctx);
        }

        Ok(PluginResult::r#continue())
    }

    // ---- 可选能力 ----

    fn init(&mut self, config: &Value) -> Result<()> {
        if !config.is_null() {
            if let Some(dir) = config.get("save_directory").and_then(|v| v.as_str()) {
                self.save_directory = dir.to_string();
            }
            if let Some(n) = config.get("max_tokens").and_then(|v| v.as_u64()) {
                self.max_tokens = n as usize;
            }
            if let Some(b) = config.get("save_on_message").and_then(|v| v.as_bool()) {
                self.save_on_message = b;
            }
        }
        info!(dir = %self.save_directory, max_tokens = self.max_tokens, "history 插件初始化完成");
        Ok(())
    }

    fn health_check(&self) -> Result<HealthStatus> {
        let dir = Path::new(&self.save_directory);
        if dir.exists() || fs::create_dir_all(dir).is_ok() {
            Ok(HealthStatus::healthy())
        } else {
            Ok(HealthStatus::unhealthy(format!("无法创建历史目录: {}", self.save_directory)))
        }
    }

    fn validate_config(&self, config: &Value) -> Result<()> {
        if let Some(dir) = config.get("save_directory").and_then(|v| v.as_str()) {
            if dir.is_empty() {
                return Err(CoreError::plugin("history", "save_directory 不能为空"));
            }
        }
        Ok(())
    }
}

// ============================================================
// FFI 导出
// ============================================================

#[no_mangle]
pub extern "C" fn create_plugin() -> Box<dyn Plugin> {
    Box::new(HistoryPlugin::new())
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use zsai_core::{Context, Plugin};

    #[test]
    fn test_metadata() {
        let plugin = HistoryPlugin::new();
        let meta = plugin.metadata();
        assert_eq!(meta.name, "history");
        assert_eq!(meta.stage, "postprocess");
    }

    #[test]
    fn test_history_path() {
        let plugin = HistoryPlugin::new();
        let path = plugin.history_path("test-session");
        assert!(path.to_string_lossy().contains("test-session"));
        assert!(path.to_string_lossy().ends_with(".json"));
    }

    #[test]
    fn test_save_and_load() {
        let plugin = HistoryPlugin {
            save_directory: "/tmp/zsai-test-history".to_string(),
            ..HistoryPlugin::new()
        };

        let mut ctx = Context::new("test-save-load");
        ctx.push_message("system", "You are helpful.");
        ctx.push_message("user", "Hello");
        ctx.push_message("assistant", "Hi there!");

        // Save
        plugin.save_history(&ctx);

        // Load
        let loaded = plugin.load_history("test-save-load");
        assert!(loaded.is_some());
        let msgs = loaded.unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[2].content, "Hi there!");

        // Clean up
        let _ = fs::remove_file(plugin.history_path("test-save-load"));
    }

    #[test]
    fn test_load_nonexistent() {
        let plugin = HistoryPlugin::new();
        let result = plugin.load_history("nonexistent-session-12345");
        assert!(result.is_none());
    }

    #[test]
    fn test_init_reads_config() {
        let mut plugin = HistoryPlugin::new();
        let config = serde_json::json!({
            "save_directory": "/tmp/custom-history",
            "max_tokens": 16000
        });
        plugin.init(&config).unwrap();
        assert_eq!(plugin.save_directory, "/tmp/custom-history");
        assert_eq!(plugin.max_tokens, 16000);
    }
}
