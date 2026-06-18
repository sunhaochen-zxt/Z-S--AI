//! 角色卡插件（character_card）。
//!
//! # 职责
//!
//! 在 `preprocess` 阶段加载角色卡数据：
//! 1. 读取 JSON 角色卡文件（SillyTavern v3 格式或纯 JSON）
//! 2. 将角色数据写入 `ctx.custom["character_card.data"]`
//! 3. 处理 `first_mes`（首次问候语）写入对话历史
//!
//! # 输入
//!
//! - `ctx.custom["character_card.path"]` — 角色卡文件路径（由 server 层注入）
//!   或 `ctx.opaque["character_card.json_bytes"]` — 已读取的 JSON 字节（由 server 注入）
//!
//! # 输出
//!
//! - `ctx.custom["character_card.data"]` — 角色数据（`serde_json::Value` 对象）
//! - `ctx.messages` — 如角色卡包含 `first_mes`，追加一条 assistant 消息
//!
//! # 支持格式
//!
//! - SillyTavern v3：`{"spec": "chara_card_v3", "data": {...}}`
//! - 纯 JSON：`{"name": "...", "personality": "...", ...}`
//!
//! # 配置（config.toml）
//!
//! ```toml
//! [plugins.character_card]
//! default_card_path = "./data/characters/default.json"
//! ```

use std::fs;
use std::path::Path;

use serde_json::Value;
use tracing::{debug, info, warn};

use zsai_core::*;

// ============================================================
// 插件结构体
// ============================================================

struct CharacterCardPlugin {
    meta: PluginMeta,
    /// 默认角色卡路径（从 config.toml 读取）
    default_card_path: String,
}

impl CharacterCardPlugin {
    fn new() -> Self {
        CharacterCardPlugin {
            meta: PluginMeta::new("character_card", (0, 1, 0), "preprocess", 10)
                .with_capability("health_check")
                .with_capability("validate_config"),
            default_card_path: String::new(),
        }
    }

    /// 从文件路径加载角色卡 JSON。
    fn load_from_file(&self, path: &Path) -> Result<Value> {
        let content = fs::read_to_string(path)
            .map_err(|e| CoreError::plugin(
                "character_card",
                format!("无法读取角色卡文件 {}: {}", path.display(), e),
            ))?;

        Self::parse_json(&content)
    }

    /// 从 JSON 字节加载角色卡（预留——用于后续 PNG 导入功能）。
    #[allow(dead_code)]
    fn load_from_bytes(&self, bytes: &[u8]) -> Result<Value> {
        let content = std::str::from_utf8(bytes)
            .map_err(|e| CoreError::plugin(
                "character_card",
                format!("角色卡 JSON 不是有效的 UTF-8: {}", e),
            ))?;

        Self::parse_json(content)
    }

    /// 解析角色卡 JSON，提取 `data` 字段。
    ///
    /// 支持：
    /// - v3 格式：`{"spec": "chara_card_v3", "data": { ... }}`
    /// - 纯 JSON：`{"name": "...", ...}`（直接作为数据）
    fn parse_json(json_str: &str) -> Result<Value> {
        let root: Value = serde_json::from_str(json_str)
            .map_err(|e| CoreError::plugin(
                "character_card",
                format!("角色卡 JSON 解析失败: {}", e),
            ))?;

        let root_obj = root.as_object().ok_or_else(|| {
            CoreError::plugin("character_card", "角色卡 JSON 根元素必须是对象")
        })?;

        // 检测 v3 格式
        let is_v3 = root_obj.get("spec")
            .and_then(|v| v.as_str())
            .map(|s| s.starts_with("chara_card_v"))
            .unwrap_or(false);

        if is_v3 {
            // v3 格式：提取 data 字段
            let data = root_obj.get("data").cloned().unwrap_or(Value::Null);
            if data.is_null() {
                return Err(CoreError::plugin(
                    "character_card",
                    "v3 角色卡缺少 'data' 字段",
                ));
            }
            debug!(spec = %root_obj["spec"].as_str().unwrap_or("?"), "解析 v3 角色卡");
            Ok(data)
        } else {
            // 纯 JSON：直接使用整个对象作为数据
            debug!("解析纯 JSON 角色卡");
            Ok(root)
        }
    }

    /// 处理 first_mes：如果角色卡包含 `first_mes`，追加到对话历史。
    fn handle_first_message(data: &Value, ctx: &mut Context) {
        if let Some(first_mes) = data.get("first_mes").and_then(|v| v.as_str()) {
            if !first_mes.is_empty() {
                ctx.push_message("assistant", first_mes);
                debug!(len = first_mes.len(), "已添加 first_mes 到对话历史");
            }
        }
    }
}

// ============================================================
// Plugin trait 实现
// ============================================================

impl Plugin for CharacterCardPlugin {
    fn metadata(&self) -> PluginMeta {
        self.meta.clone()
    }

    fn execute(&self, ctx: &mut Context) -> Result<PluginResult> {
        info!(session = %ctx.session_id, "加载角色卡");

        // 确定角色卡来源：路径 > JSON 字节
        let card_path = ctx.get_custom::<String>("character_card.path");

        let data = if let Some(path) = card_path {
            info!(path = %path, "从文件加载角色卡");
            let data = self.load_from_file(Path::new(&path))?;
            data
        } else {
            // 尝试从默认路径加载
            if !self.default_card_path.is_empty() {
                let default_path = Path::new(&self.default_card_path);
                if default_path.exists() {
                    info!(path = %self.default_card_path, "使用默认角色卡");
                    let data = self.load_from_file(default_path)?;
                    data
                } else {
                    warn!("未指定角色卡路径，且默认角色卡不存在，跳过");
                    return Ok(PluginResult::r#continue());
                }
            } else {
                warn!("未指定角色卡路径，跳过加载");
                return Ok(PluginResult::r#continue());
            }
        };

        // 验证必要字段
        let name = data.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("(未命名)");
        info!(character_name = name, "角色卡加载成功");

        // 写入 Context
        ctx.set_custom_value("character_card.data", data.clone());

        // 处理 first_mes
        Self::handle_first_message(&data, ctx);

        Ok(PluginResult::r#continue())
    }

    // ---- 可选能力 ----

    fn init(&mut self, config: &Value) -> Result<()> {
        if !config.is_null() {
            if let Some(path) = config.get("default_card_path").and_then(|v| v.as_str()) {
                self.default_card_path = path.to_string();
                info!(path = %self.default_card_path, "默认角色卡路径已设置");
            }
        }
        Ok(())
    }

    fn health_check(&self) -> Result<HealthStatus> {
        let ok = self.default_card_path.is_empty()
            || Path::new(&self.default_card_path).exists();
        if ok {
            Ok(HealthStatus::healthy())
        } else {
            Ok(HealthStatus::unhealthy(format!(
                "默认角色卡文件不存在: {}",
                self.default_card_path
            )))
        }
    }

    fn validate_config(&self, config: &Value) -> Result<()> {
        if let Some(path) = config.get("default_card_path").and_then(|v| v.as_str()) {
            if !path.is_empty() && !Path::new(path).exists() {
                // 不阻止启动，仅返回错误信息（可能后续才创建文件）
                warn!(path = %path, "默认角色卡文件不存在，将在首次使用时创建");
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
    Box::new(CharacterCardPlugin::new())
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use zsai_core::{Context, Plugin};

    /// 测试用 v3 角色卡 JSON
    const V3_CARD: &str = r#"{
        "spec": "chara_card_v3",
        "spec_version": "3.0",
        "data": {
            "name": "李白",
            "description": "唐朝伟大诗人",
            "personality": "豪放、浪漫、富有想象力",
            "scenario": "酒肆之中，月色皎洁",
            "first_mes": "哈哈哈！来得正好！",
            "example_dialogue": "User: 李白兄\nAssistant: 哈哈！",
            "system_prompt": "你扮演李白。始终保持豪放气质。",
            "creator_notes": "测试用角色卡"
        }
    }"#;

    /// 测试用纯 JSON 角色卡
    const PLAIN_CARD: &str = r#"{
        "name": "Luna",
        "description": "友善的 AI 助手",
        "personality": "友善、耐心",
        "scenario": "聊天界面",
        "first_mes": "你好呀！很高兴见到你！"
    }"#;

    #[test]
    fn test_metadata() {
        let plugin = CharacterCardPlugin::new();
        let meta = plugin.metadata();
        assert_eq!(meta.name, "character_card");
        assert_eq!(meta.stage, "preprocess");
        assert!(meta.capabilities.contains(&"health_check".to_string()));
    }

    #[test]
    fn test_parse_v3_card() {
        let data = CharacterCardPlugin::parse_json(V3_CARD).unwrap();
        assert_eq!(data["name"], "李白");
        assert_eq!(data["personality"], "豪放、浪漫、富有想象力");
        assert_eq!(data["first_mes"], "哈哈哈！来得正好！");
        assert_eq!(data["creator_notes"], "测试用角色卡");
    }

    #[test]
    fn test_parse_plain_card() {
        let data = CharacterCardPlugin::parse_json(PLAIN_CARD).unwrap();
        assert_eq!(data["name"], "Luna");
        assert_eq!(data["personality"], "友善、耐心");
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = CharacterCardPlugin::parse_json("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_non_object() {
        let result = CharacterCardPlugin::parse_json("[]");
        assert!(result.is_err());
    }

    #[test]
    fn test_first_message_added() {
        let mut ctx = Context::new("test");
        let data: Value = serde_json::from_str(PLAIN_CARD).unwrap();
        CharacterCardPlugin::handle_first_message(&data, &mut ctx);

        assert_eq!(ctx.message_count(), 1);
        assert_eq!(ctx.messages[0].role, "assistant");
        assert_eq!(ctx.messages[0].content, "你好呀！很高兴见到你！");
    }

    #[test]
    fn test_no_first_message() {
        let mut ctx = Context::new("test");
        let data = serde_json::json!({"name": "test"});
        CharacterCardPlugin::handle_first_message(&data, &mut ctx);
        assert_eq!(ctx.message_count(), 0);
    }

    #[test]
    fn test_init_reads_default_path() {
        let mut plugin = CharacterCardPlugin::new();
        let config = serde_json::json!({"default_card_path": "/tmp/test.json"});
        plugin.init(&config).unwrap();
        assert_eq!(plugin.default_card_path, "/tmp/test.json");
    }

    #[test]
    fn test_init_empty_config() {
        let mut plugin = CharacterCardPlugin::new();
        plugin.init(&serde_json::Value::Null).unwrap();
        assert_eq!(plugin.default_card_path, "");
    }
}
