//! API 客户端插件（api_client）。
//!
//! # 职责
//!
//! 在 `api_call` 阶段调用 LLM API（DeepSeek 或 OpenAI 兼容模式）。
//!
//! # 输入
//!
//! - `ctx.custom["prompt_builder.output"]` — system prompt 字符串
//! - `ctx.custom["api_client.config"]` — API 配置（api_type, api_key, base_url, model 等）
//! - `ctx.messages` / `ctx.user_input` — 对话历史和用户输入
//!
//! # 输出
//!
//! - `ctx.ai_response` — AI 回复文本
//! - `ctx.custom["api_client.token_usage"]` — token 用量
//! - `ctx.messages` — 追加 assistant 消息
//!
//! # 配置（config.toml）
//!
//! ```toml
//! [plugins.api_client]
//! api_type = "deepseek"     # deepseek | openai
//! timeout_seconds = 30
//! max_retries = 2
//! reasoning_effort = "medium"
//! thinking_type = ""        # 空 或 "enabled"
//! ```

use std::io::Read;
use std::sync::mpsc;

use serde_json::Value;
use tracing::{debug, info, warn};

use zsai_core::*;

// ============================================================
// 插件结构体
// ============================================================

struct ApiClientPlugin {
    meta: PluginMeta,
    /// 超时时间（秒）
    timeout_seconds: u64,
    /// 最大重试次数
    max_retries: u32,
    /// DeepSeek reasoning_effort 参数
    reasoning_effort: String,
    /// DeepSeek thinking_type 参数
    thinking_type: String,
    /// HTTP 客户端（阻塞模式）
    client: Option<reqwest::blocking::Client>,
}

impl ApiClientPlugin {
    fn new() -> Self {
        ApiClientPlugin {
            meta: PluginMeta::new("api_client", (0, 1, 0), "api_call", 10)
                .with_capability("health_check")
                .with_capability("validate_config"),
            timeout_seconds: 30,
            max_retries: 2,
            reasoning_effort: String::new(),
            thinking_type: String::new(),
            client: None,
        }
    }

    /// 获取或创建 HTTP 客户端。
    fn get_client(&self) -> Result<&reqwest::blocking::Client> {
        self.client.as_ref().ok_or_else(|| {
            CoreError::plugin("api_client", "HTTP 客户端未初始化——init() 未被调用？")
        })
    }

    /// 从 `ctx.custom` 读取 API 配置。
    /// 优先级：ctx.custom["api_client.config"] > config.toml defaults
    fn read_api_config(&self, ctx: &Context) -> ApiConfig {
        if let Some(cfg) = ctx.get_custom_value("api_client.config") {
            ApiConfig {
                api_type: cfg.get("api_type").and_then(|v| v.as_str()).unwrap_or("deepseek").to_string(),
                api_key: cfg.get("api_key").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                base_url: cfg.get("base_url").and_then(|v| v.as_str()).unwrap_or("https://api.deepseek.com").to_string(),
                model: cfg.get("model").and_then(|v| v.as_str()).unwrap_or("deepseek-v4-flash").to_string(),
                stream: cfg.get("stream").and_then(|v| v.as_bool()).unwrap_or(false),
                reasoning_effort: cfg.get("reasoning_effort").and_then(|v| v.as_str()).unwrap_or(&self.reasoning_effort).to_string(),
                thinking_type: cfg.get("thinking_type").and_then(|v| v.as_str()).unwrap_or(&self.thinking_type).to_string(),
            }
        } else {
            ApiConfig {
                api_type: "deepseek".into(),
                api_key: String::new(),
                base_url: "https://api.deepseek.com".into(),
                model: "deepseek-v4-flash".into(),
                stream: false,
                reasoning_effort: self.reasoning_effort.clone(),
                thinking_type: self.thinking_type.clone(),
            }
        }
    }

    /// 从 Context 读取 API key。
    /// 优先级：ctx.custom > 环境变量 DEEPSEEK_API_KEY
    #[allow(dead_code)]
    fn resolve_api_key(&self, ctx: &Context) -> String {
        // 1. 从 ctx.custom["api_client.config"] 中读取
        if let Some(cfg) = ctx.get_custom_value("api_client.config") {
            if let Some(key) = cfg.get("api_key").and_then(|v| v.as_str()) {
                if !key.is_empty() {
                    return key.to_string();
                }
            }
        }

        // 2. 从环境变量读取
        if let Ok(key) = std::env::var("DEEPSEEK_API_KEY") {
            if !key.is_empty() {
                return key;
            }
        }

        String::new()
    }

    /// 构建 JSON 请求体。
    fn build_request_body(
        &self,
        system_prompt: &str,
        user_input: &str,
        model: &str,
        api_type: &str,
        reasoning_effort: &str,
        thinking_type: &str,
        stream: bool,
    ) -> Value {
        let messages = vec![
            serde_json::json!({"role": "system", "content": system_prompt}),
            serde_json::json!({"role": "user", "content": user_input}),
        ];

        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": stream,
        });

        // DeepSeek 专有参数（OpenAI 兼容模式下跳过）
        if api_type != "openai" {
            if !reasoning_effort.is_empty() {
                body["reasoning_effort"] = serde_json::Value::String(reasoning_effort.to_string());
            }
            if !thinking_type.is_empty() {
                body["extra_body"] = serde_json::json!({
                    "thinking": {"type": thinking_type}
                });
            }
        }

        body
    }

    /// 解析 API 响应，提取 assistant 回复内容。
    fn parse_response(&self, response_body: &Value) -> Result<String> {
        // 检查 API 层错误
        if let Some(err) = response_body.get("error") {
            let msg = err.get("message").and_then(|v| v.as_str()).unwrap_or("(未知错误)");
            return Err(CoreError::plugin("api_client", format!("API 返回错误: {}", msg)));
        }

        // 提取 choices[0].message.content
        let content = response_body
            .get("choices")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(|c| c.as_str());

        match content {
            Some(text) => Ok(text.to_string()),
            None => Err(CoreError::plugin(
                "api_client",
                format!("响应中缺少 choices[0].message.content。响应体: {}",
                    serde_json::to_string_pretty(response_body).unwrap_or_default()),
            )),
        }
    }

    /// 解析 token 用量。
    fn parse_token_usage(&self, response_body: &Value) -> Value {
        response_body.get("usage").cloned().unwrap_or(Value::Null)
    }
}

// ============================================================
// 内部类型
// ============================================================

struct ApiConfig {
    api_type: String,
    api_key: String,
    base_url: String,
    model: String,
    stream: bool,
    reasoning_effort: String,
    thinking_type: String,
}

// ============================================================
// Plugin trait 实现
// ============================================================

impl Plugin for ApiClientPlugin {
    fn metadata(&self) -> PluginMeta {
        self.meta.clone()
    }

    fn execute(&self, ctx: &mut Context) -> Result<PluginResult> {
        info!(session = %ctx.session_id, "调用 LLM API");

        // ---- 1. 读取输入 ----
        let system_prompt = ctx
            .get_custom::<String>("prompt_builder.output")
            .unwrap_or_default();

        let user_input = ctx.user_input.clone().unwrap_or_default();
        if user_input.is_empty() {
            warn!("user_input 为空，跳过 API 调用");
            return Ok(PluginResult::r#continue());
        }

        // ---- 2. 读取配置 ----
        let api_cfg = self.read_api_config(ctx);
        let api_key = if !api_cfg.api_key.is_empty() {
            api_cfg.api_key.clone()
        } else {
            std::env::var("DEEPSEEK_API_KEY").unwrap_or_default()
        };

        if api_key.is_empty() {
            return Err(CoreError::plugin(
                "api_client",
                "API Key 未设置。请在 config.toml 中设置 api_key，或设置环境变量 DEEPSEEK_API_KEY",
            ));
        }

        // ---- 3. 构建请求体 ----
        let request_body = self.build_request_body(
            &system_prompt,
            &user_input,
            &api_cfg.model,
            &api_cfg.api_type,
            &api_cfg.reasoning_effort,
            &api_cfg.thinking_type,
            api_cfg.stream,
        );

        debug!(
            model = %api_cfg.model,
            api_type = %api_cfg.api_type,
            prompt_len = system_prompt.len(),
            "发送 API 请求"
        );

        // ---- 4. 发送 HTTP 请求 ----
        let client = self.get_client()?;
        let url = format!("{}/chat/completions", api_cfg.base_url.trim_end_matches('/'));

        // 检查是否流式模式
        let stream_enabled = api_cfg.stream;
        let has_sender = ctx.has_opaque("stream_parser.sender");

        let mut last_error: Option<CoreError> = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                debug!(attempt, "重试 API 请求");
            }

            let response = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&request_body)
                .send();

            match response {
                Ok(mut resp) => {
                    let status = resp.status();

                    if !status.is_success() {
                        let response_body: Value = resp.json().unwrap_or(Value::Null);
                        let err_msg = response_body
                            .get("error")
                            .and_then(|e| e.get("message"))
                            .and_then(|m| m.as_str())
                            .unwrap_or("(无错误详情)");

                        last_error = Some(CoreError::plugin(
                            "api_client",
                            format!("HTTP {}: {}", status.as_u16(), err_msg),
                        ));

                        if status.is_client_error() { break; }
                        continue;
                    }

                    // ---- 流式模式 ----
                    if stream_enabled && has_sender {
                        info!("流式模式：开始接收 SSE 事件流");
                        let sender = ctx.get_opaque::<mpsc::Sender<String>>("stream_parser.sender")
                            .cloned()
                            .ok_or_else(|| CoreError::plugin("api_client", "stream_parser.sender 类型不匹配"))?;

                        // SSE 解析逻辑内联于此（与 stream_parser 插件重复，但因插件
                        // 间禁止编译时依赖，此重复是故意的。stream_parser 的 SseParser
                        // 类型可在未来通过 ctx.opaque 注入）。
                        let mut buf = Vec::new();
                        let mut accumulated = String::new();
                        let mut byte_buf = [0u8; 4096];

                        loop {
                            match resp.read(&mut byte_buf) {
                                Ok(0) => break,
                                Ok(n) => buf.extend_from_slice(&byte_buf[..n]),
                                Err(e) => { warn!(error = %e, "SSE 读取错误"); break; }
                            }

                            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                                let line = String::from_utf8_lossy(&buf[..pos]).trim().to_string();
                                buf.drain(..=pos);

                                if line.is_empty() { continue; }
                                if !line.starts_with("data: ") { continue; }

                                let payload = &line[6..];
                                if payload == "[DONE]" { continue; }

                                if let Ok(val) = serde_json::from_str::<Value>(payload) {
                                    if let Some(c) = val.get("choices")
                                        .and_then(|v| v.as_array())
                                        .and_then(|a| a.first())
                                        .and_then(|v| v.get("delta"))
                                        .and_then(|v| v.get("content"))
                                        .and_then(|v| v.as_str())
                                    {
                                        accumulated.push_str(c);
                                        let _ = sender.send(c.to_string());
                                    }
                                }
                            }
                        }

                        info!(len = accumulated.len(), "流式响应完成");
                        ctx.ai_response = Some(accumulated.clone());
                        ctx.push_message("assistant", &accumulated);
                        ctx.set_custom_value("api_client.last_response", serde_json::json!({
                            "stream": true,
                            "content": accumulated,
                        }));
                        return Ok(PluginResult::r#continue());
                    }

                    // ---- 非流式模式 ----
                    let response_body: Value = resp.json().unwrap_or(Value::Null);

                    match self.parse_response(&response_body) {
                        Ok(content) => {
                            info!(content_len = content.len(), "API 响应成功");
                            ctx.ai_response = Some(content.clone());
                            ctx.push_message("assistant", &content);

                            let usage = self.parse_token_usage(&response_body);
                            if !usage.is_null() {
                                ctx.set_custom_value("api_client.token_usage", usage);
                            }
                            ctx.set_custom_value("api_client.last_response", response_body);
                            return Ok(PluginResult::r#continue());
                        }
                        Err(e) => {
                            last_error = Some(e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    last_error = Some(CoreError::plugin(
                        "api_client",
                        format!("网络请求失败: {}", e),
                    ));
                    continue;
                }
            }
        }

        // 所有重试已耗尽
        Err(last_error.unwrap_or_else(|| {
            CoreError::plugin("api_client", "未知错误：所有重试已耗尽")
        }))
    }

    // ---- 可选能力 ----

    fn init(&mut self, config: &Value) -> Result<()> {
        if !config.is_null() {
            if let Some(t) = config.get("timeout_seconds").and_then(|v| v.as_u64()) {
                self.timeout_seconds = t;
            }
            if let Some(r) = config.get("max_retries").and_then(|v| v.as_u64()) {
                self.max_retries = r as u32;
            }
            if let Some(re) = config.get("reasoning_effort").and_then(|v| v.as_str()) {
                self.reasoning_effort = re.to_string();
            }
            if let Some(tt) = config.get("thinking_type").and_then(|v| v.as_str()) {
                self.thinking_type = tt.to_string();
            }
        }

        // 创建 HTTP 客户端
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(self.timeout_seconds))
            .build()
            .map_err(|e| CoreError::plugin(
                "api_client",
                format!("无法创建 HTTP 客户端: {}", e),
            ))?;

        self.client = Some(client);

        info!(
            timeout = self.timeout_seconds,
            max_retries = self.max_retries,
            reasoning_effort = %self.reasoning_effort,
            "api_client 插件初始化完成"
        );
        Ok(())
    }

    fn health_check(&self) -> Result<HealthStatus> {
        match &self.client {
            Some(_) => Ok(HealthStatus::healthy()),
            None => Ok(HealthStatus::unhealthy("HTTP 客户端未初始化")),
        }
    }

    fn validate_config(&self, config: &Value) -> Result<()> {
        if !config.is_null() {
            if let Some(t) = config.get("timeout_seconds").and_then(|v| v.as_u64()) {
                if t == 0 || t > 300 {
                    return Err(CoreError::plugin(
                        "api_client",
                        format!("timeout_seconds 必须在 1..=300 之间，当前值: {}", t),
                    ));
                }
            }
            if let Some(re) = config.get("reasoning_effort").and_then(|v| v.as_str()) {
                if !matches!(re, "low" | "medium" | "high" | "") {
                    return Err(CoreError::plugin(
                        "api_client",
                        format!("reasoning_effort 无效: '{}'，有效值: low/medium/high", re),
                    ));
                }
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
    Box::new(ApiClientPlugin::new())
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
        let plugin = ApiClientPlugin::new();
        let meta = plugin.metadata();
        assert_eq!(meta.name, "api_client");
        assert_eq!(meta.stage, "api_call");
    }

    #[test]
    fn test_build_request_body_deepseek() {
        let plugin = ApiClientPlugin::new();
        let body = plugin.build_request_body(
            "[System]\nTest prompt",
            "Hello",
            "deepseek-v4-flash",
            "deepseek",
            "medium",
            "",
            false,
        );

        assert_eq!(body["model"], "deepseek-v4-flash");
        assert_eq!(body["stream"], false);
        assert_eq!(body["reasoning_effort"], "medium");
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "Hello");
    }

    #[test]
    fn test_build_request_body_openai() {
        let plugin = ApiClientPlugin::new();
        let body = plugin.build_request_body(
            "prompt",
            "Hi",
            "gpt-4",
            "openai",
            "high",  // should be ignored for openai
            "",
            false,
        );

        assert_eq!(body["model"], "gpt-4");
        // OpenAI 模式下不应有 reasoning_effort
        assert!(body.get("reasoning_effort").is_none());
        // OpenAI 模式下不应有 extra_body
        assert!(body.get("extra_body").is_none());
    }

    #[test]
    fn test_build_request_body_with_thinking() {
        let plugin = ApiClientPlugin::new();
        let body = plugin.build_request_body(
            "prompt", "test", "deepseek-v4-pro", "deepseek",
            "high", "enabled", true,
        );

        assert_eq!(body["stream"], true);
        assert_eq!(body["reasoning_effort"], "high");
        assert_eq!(body["extra_body"]["thinking"]["type"], "enabled");
    }

    #[test]
    fn test_parse_response_success() {
        let plugin = ApiClientPlugin::new();
        let response = serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "你好！有什么可以帮助你的？"
                }
            }],
            "usage": {"prompt_tokens": 100, "completion_tokens": 50}
        });

        let content = plugin.parse_response(&response).unwrap();
        assert_eq!(content, "你好！有什么可以帮助你的？");

        let usage = plugin.parse_token_usage(&response);
        assert_eq!(usage["prompt_tokens"], 100);
    }

    #[test]
    fn test_parse_response_api_error() {
        let plugin = ApiClientPlugin::new();
        let response = serde_json::json!({
            "error": {
                "message": "Invalid API key"
            }
        });

        let result = plugin.parse_response(&response);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("Invalid API key"));
    }

    #[test]
    fn test_resolve_api_key_from_env() {
        // 这个测试依赖环境变量，在 CI 中可能不可用
        let plugin = ApiClientPlugin::new();
        let ctx = Context::new("test");
        let key = plugin.resolve_api_key(&ctx);
        // 要么从环境变量获取，要么为空
        // 不做强断言，只确保不 panic
        let _ = key;
    }

    #[test]
    fn test_init_creates_client() {
        let mut plugin = ApiClientPlugin::new();
        let config = serde_json::json!({"timeout_seconds": 10});
        plugin.init(&config).unwrap();
        assert!(plugin.client.is_some());
    }

    #[test]
    fn test_validate_config_rejects_bad_timeout() {
        let plugin = ApiClientPlugin::new();
        let config = serde_json::json!({"timeout_seconds": 0});
        assert!(plugin.validate_config(&config).is_err());

        let config = serde_json::json!({"timeout_seconds": 500});
        assert!(plugin.validate_config(&config).is_err());
    }
}
