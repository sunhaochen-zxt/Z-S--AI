//! 配置文件解析。
//!
//! 读取并校验 `config.toml`，提供结构化的配置访问。
//! 所有配置段对应 refactor-plan.md v2 中定义的格式。

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Deserializer};

use crate::error::{Result, CoreError};

// ============================================================
// 顶层配置
// ============================================================

/// 完整配置结构。
///
/// 从 `config.toml` 反序列化。
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    /// 热加载配置。
    #[serde(default)]
    pub hot_reload: HotReloadConfig,

    /// 阶段配置。
    pub stages: StagesConfig,

    /// 会话默认配置。
    #[serde(default)]
    pub session: SessionConfig,

    /// API 默认配置。
    #[serde(default)]
    pub api: ApiConfig,

    /// 历史记录默认配置。
    #[serde(default)]
    pub history: HistoryConfig,

    /// 各插件的独立配置。
    ///
    /// key 为插件名，value 为该插件的配置（任意 JSON 对象）。
    /// 内核不解析 value 的具体内容，原样传给插件的 `init()`。
    #[serde(default)]
    pub plugins: HashMap<String, toml::Value>,
}

impl AppConfig {
    /// 从文件加载配置。
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref()).map_err(|e| {
            CoreError::config(format!(
                "无法读取配置文件 {}: {}",
                path.as_ref().display(),
                e
            ))
        })?;

        let config: AppConfig = toml::from_str(&content).map_err(|e| {
            CoreError::config(format!("配置文件解析失败: {}", e))
        })?;

        config.validate()?;
        Ok(config)
    }

    /// 校验配置的完整性。
    ///
    /// 检查：
    /// - `order` 列表不能为空。
    /// - `order` 列表中不能有重复的阶段名。
    /// - `error_stage` 必须存在于 `order` 列表中。
    /// - 插件目录路径存在（如果热加载启用）。
    pub fn validate(&self) -> Result<()> {
        // 校验 order 非空
        if self.stages.order.is_empty() {
            return Err(CoreError::config(
                "[stages].order 不能为空，至少需要一个阶段"
            ));
        }

        // 校验 order 中无重复项
        {
            let mut seen = std::collections::HashSet::new();
            for stage in &self.stages.order {
                if !seen.insert(stage) {
                    return Err(CoreError::config(format!(
                        "[stages].order 中存在重复的阶段名: \"{}\"。\
                         \n每个阶段名只能出现一次。",
                        stage
                    )));
                }
            }
        }

        // 校验 error_stage 存在于 order 中
        if !self.stages.order.contains(&self.stages.error_stage) {
            return Err(CoreError::config(format!(
                "[stages].error_stage = \"{}\" 不在 [stages].order 列表中。\
                 \norder = {:?}\
                 \n请确保 error_stage 的值与 order 中的某个阶段名一致。",
                self.stages.error_stage, self.stages.order
            )));
        }

        // 校验 hot_reload.plugin_dir 存在（如果热加载启用）
        if self.hot_reload.enabled {
            let dir = Path::new(&self.hot_reload.plugin_dir);
            if !dir.exists() {
                return Err(CoreError::config(format!(
                    "热加载已启用但插件目录不存在: {}。\
                     \n请先编译插件（cargo build --workspace）或创建该目录。",
                    dir.display()
                )));
            }
            if !dir.is_dir() {
                return Err(CoreError::config(format!(
                    "plugin_dir 不是目录: {}",
                    dir.display()
                )));
            }
        }

        Ok(())
    }

    /// 获取指定插件的配置，转为 serde_json::Value。
    ///
    /// 返回 `None` 如果该插件没有专属配置段。
    pub fn get_plugin_config(&self, plugin_name: &str) -> Option<serde_json::Value> {
        self.plugins.get(plugin_name).map(|v| {
            // toml::Value → serde_json::Value
            let json_str = serde_json::to_string(v)
                .unwrap_or_else(|_| "null".to_string());
            serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null)
        })
    }
}

// ============================================================
// 各配置段
// ============================================================

/// 热加载配置。
#[derive(Debug, Clone, Deserialize)]
pub struct HotReloadConfig {
    /// 是否启用热加载。
    #[serde(default = "default_hot_reload_enabled")]
    pub enabled: bool,

    /// 插件动态库目录。
    #[serde(default = "default_plugin_dir")]
    pub plugin_dir: String,

    /// 防抖延迟（毫秒）。
    #[serde(default = "default_delay_ms")]
    pub delay_ms: u64,
}

impl Default for HotReloadConfig {
    fn default() -> Self {
        HotReloadConfig {
            enabled: true,
            plugin_dir: default_plugin_dir(),
            delay_ms: default_delay_ms(),
        }
    }
}

fn default_hot_reload_enabled() -> bool { true }
fn default_plugin_dir() -> String { "./target/debug".to_string() }
fn default_delay_ms() -> u64 { 100 }

/// 阶段配置。
///
/// 这是整个流水线的核心配置。
#[derive(Debug, Clone, Deserialize)]
pub struct StagesConfig {
    /// 阶段执行顺序。
    ///
    /// 例如 `["preprocess", "validate", "build_prompt", "api_call", "postprocess"]`。
    #[serde(deserialize_with = "deserialize_non_empty_vec")]
    pub order: Vec<String>,

    /// abort 时跳转的目标阶段。
    ///
    /// 必须在 `order` 中存在（`AppConfig::validate()` 会校验）。
    /// 通常为 `"postprocess"` 或类似的后处理阶段。
    #[serde(default = "default_error_stage")]
    pub error_stage: String,

    /// 各阶段的配置参数（可选）。
    ///
    /// 如 `[stages.api_call] timeout_ms = 30000`。
    /// key 为阶段名，value 为任意配置。
    #[serde(default, flatten)]
    pub extra: HashMap<String, toml::Value>,
}

fn default_error_stage() -> String {
    "postprocess".to_string()
}

fn deserialize_non_empty_vec<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let vec = Vec::<String>::deserialize(deserializer)?;
    if vec.is_empty() {
        Err(serde::de::Error::custom("[stages].order 不能为空数组"))
    } else {
        Ok(vec)
    }
}

/// 会话默认配置。
#[derive(Debug, Clone, Deserialize)]
pub struct SessionConfig {
    /// 默认角色卡路径。
    #[serde(default = "default_character")]
    pub default_character: String,
}

impl Default for SessionConfig {
    fn default() -> Self {
        SessionConfig {
            default_character: default_character(),
        }
    }
}

fn default_character() -> String {
    "./data/characters/default.json".to_string()
}

/// API 默认配置。
#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    /// API 类型：`"deepseek"` 或 `"openai"`。
    #[serde(default = "default_api_type")]
    pub api_type: String,

    /// API 密钥。空字符串时从环境变量读取。
    #[serde(default)]
    pub api_key: String,

    /// API 基础 URL（不含 `/chat/completions` 路径）。
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// 默认模型名称。
    #[serde(default = "default_model")]
    pub model: String,

    /// 是否默认启用流式。
    #[serde(default)]
    pub stream: bool,
}

impl Default for ApiConfig {
    fn default() -> Self {
        ApiConfig {
            api_type: default_api_type(),
            api_key: String::new(),
            base_url: default_base_url(),
            model: default_model(),
            stream: false,
        }
    }
}

fn default_api_type() -> String { "deepseek".to_string() }
fn default_base_url() -> String { "https://api.deepseek.com".to_string() }
fn default_model() -> String { "deepseek-v4-flash".to_string() }

/// 历史记录默认配置。
#[derive(Debug, Clone, Deserialize)]
pub struct HistoryConfig {
    /// 历史记录存储目录。
    #[serde(default = "default_history_dir")]
    pub save_directory: String,

    /// 上下文 token 上限（超过后自动裁剪）。
    #[serde(default = "default_max_tokens")]
    pub max_context_tokens: usize,

    /// 是否在每条消息后自动保存。
    #[serde(default = "default_save_on_message")]
    pub save_on_every_message: bool,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        HistoryConfig {
            save_directory: default_history_dir(),
            max_context_tokens: default_max_tokens(),
            save_on_every_message: default_save_on_message(),
        }
    }
}

fn default_history_dir() -> String { "./data/history".to_string() }
fn default_max_tokens() -> usize { 32768 }
fn default_save_on_message() -> bool { true }

#[cfg(test)]
mod tests {
    use super::*;

    /// 构建一个最小有效的 config.toml 内容用于测试。
    fn minimal_config() -> String {
        format!(
            r#"
[hot_reload]
enabled = false
plugin_dir = "{}"

[stages]
order = ["preprocess", "build_prompt", "api_call", "postprocess"]

[stages.preprocess]
[stages.build_prompt]
[stages.api_call]
[stages.postprocess]
"#,
            std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .display()
        )
    }

    #[test]
    fn test_parse_minimal_config() {
        let config: AppConfig = toml::from_str(&minimal_config()).unwrap();
        assert_eq!(config.stages.order.len(), 4);
        assert_eq!(config.stages.error_stage, "postprocess"); // 默认值
        assert!(!config.hot_reload.enabled);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_parse_full_config() {
        let cwd = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        let toml_str = format!(
            r#"
[hot_reload]
enabled = false
plugin_dir = "{}"
delay_ms = 200

[stages]
order = ["preprocess", "api_call", "postprocess"]
error_stage = "postprocess"

[session]
default_character = "./chars/default.json"

[api]
api_type = "openai"
api_key = ""
base_url = "https://api.openai.com"
model = "gpt-3.5-turbo"
stream = true

[history]
save_directory = "./my_history"
max_context_tokens = 16000
save_on_every_message = false

[plugins.character_card]
default_card_path = "./chars/default.json"

[plugins.api_client]
timeout_seconds = 60
"#,
            cwd.display()
        );
        let config: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.stages.order, vec!["preprocess", "api_call", "postprocess"]);
        assert_eq!(config.api.api_type, "openai");
        assert_eq!(config.history.max_context_tokens, 16000);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_empty_order_rejected() {
        let toml_str = r#"
[stages]
order = []
"#;
        let result: std::result::Result<AppConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_error_stage_not_in_order() {
        let toml_str = r#"
[stages]
order = ["preprocess", "api_call"]
error_stage = "postprocess"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let result = config.validate();
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("error_stage"));
        assert!(err_msg.contains("postprocess"));
    }

    #[test]
    fn test_get_plugin_config() {
        let toml_str = r#"
[stages]
order = ["api_call"]

[plugins.api_client]
timeout_seconds = 30
model = "custom-model"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let plugin_cfg = config.get_plugin_config("api_client");
        assert!(plugin_cfg.is_some());
        let cfg = plugin_cfg.unwrap();
        assert_eq!(cfg["timeout_seconds"], 30);
        assert_eq!(cfg["model"], "custom-model");
    }

    #[test]
    fn test_get_plugin_config_nonexistent() {
        let toml_str = r#"
[stages]
order = ["api_call"]
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert!(config.get_plugin_config("nonexistent").is_none());
    }
}
