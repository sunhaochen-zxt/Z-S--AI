//! 流式解析插件（stream_parser）。
//!
//! # 职责
//!
//! 提供 SSE (Server-Sent Events) 解析纯函数，供 `api_client` 在流式模式下调。
//! 插件自身的 `execute()` 仅做初始化，将解析函数注入 `ctx.opaque`。
//!
//! # 输出
//!
//! - `ctx.opaque["stream_parser.factory"]` — SSE 解析器工厂函数指针
//!   签名：`fn() -> Box<dyn FnMut(&[u8]) -> Vec<String> + Send>`

use tracing::debug;
use zsai_core::*;

// ============================================================
// SSE 解析器
// ============================================================

/// SSE 事件流解析器。
///
/// 维护内部缓冲区，处理跨 chunk 的不完整行。
#[derive(Default)]
pub struct SseParser {
    /// 未完成行的缓冲区
    buffer: Vec<u8>,
}

impl SseParser {
    pub fn new() -> Self {
        SseParser { buffer: Vec::new() }
    }

    /// 喂入一个数据块，返回解析出的所有增量 token。
    ///
    /// 格式：`data: {"choices":[{"delta":{"content":"..."}}]}`
    /// 结束标志：`data: [DONE]`
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<String> {
        let mut tokens = Vec::new();
        self.buffer.extend_from_slice(chunk);

        while let Some(line_end) = self.buffer.iter().position(|&b| b == b'\n') {
            let line = self.buffer.drain(..=line_end).collect::<Vec<_>>();
            let line_str = String::from_utf8_lossy(&line).trim().to_string();

            if line_str.is_empty() {
                continue; // SSE 空行分隔符
            }
            if !line_str.starts_with("data: ") {
                continue;
            }

            let payload = &line_str[6..]; // 去掉 "data: "
            if payload == "[DONE]" {
                debug!("SSE 流结束 ([DONE])");
                continue;
            }

            // 解析 JSON payload
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(payload) {
                if let Some(content) = extract_delta_content(&val) {
                    if !content.is_empty() {
                        tokens.push(content);
                    }
                }
            }
        }

        tokens
    }

    /// 创建一个可在 FFI 边界传递的解析函数指针。
    ///
    /// 由于 `SseParser` 需要可变状态（内部缓冲区），
    /// 此函数返回一个闭包包装的 `Box<dyn FnMut>`。
    /// 实际使用时，`api_client` 在流式循环中维护自己的 `SseParser` 实例。
    pub fn create_parser() -> Box<dyn FnMut(&[u8]) -> Vec<String> + Send> {
        let mut parser = SseParser::new();
        Box::new(move |chunk: &[u8]| parser.feed(chunk))
    }
}

/// 从 SSE JSON payload 中提取 `choices[0].delta.content`。
fn extract_delta_content(val: &serde_json::Value) -> Option<String> {
    val.get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|choice| choice.get("delta"))
        .and_then(|delta| delta.get("content"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
}

// ============================================================
// 插件结构体
// ============================================================

struct StreamParserPlugin {
    meta: PluginMeta,
}

impl StreamParserPlugin {
    fn new() -> Self {
        StreamParserPlugin {
            meta: PluginMeta::new("stream_parser", (0, 1, 0), "api_call", 20)
                .with_capability("health_check"),
        }
    }
}

impl Plugin for StreamParserPlugin {
    fn metadata(&self) -> PluginMeta {
        self.meta.clone()
    }

    fn execute(&self, ctx: &mut Context) -> Result<PluginResult> {
        // 将 SseParser::create_parser 作为函数指针注入 opaque
        // api_client 在流式模式下会读取并调用它
        ctx.set_opaque(
            "stream_parser.factory",
            SseParser::create_parser as fn() -> Box<dyn FnMut(&[u8]) -> Vec<String> + Send>,
        );

        debug!("SSE 解析器工厂已注入 opaque");
        Ok(PluginResult::r#continue())
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
    Box::new(StreamParserPlugin::new())
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_delta_content() {
        let json = serde_json::json!({
            "choices": [{
                "delta": {"content": "你好"}
            }]
        });
        assert_eq!(extract_delta_content(&json), Some("你好".into()));
    }

    #[test]
    fn test_extract_delta_empty() {
        let json = serde_json::json!({
            "choices": [{"delta": {}}]
        });
        assert_eq!(extract_delta_content(&json), None);
    }

    #[test]
    fn test_parser_single_line() {
        let mut parser = SseParser::new();
        let chunk = b"data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n";
        let tokens = parser.feed(chunk);
        assert_eq!(tokens, vec!["Hello"]);
    }

    #[test]
    fn test_parser_multi_line() {
        let mut parser = SseParser::new();
        let chunk = b"data: {\"choices\":[{\"delta\":{\"content\":\"A\"}}]}\ndata: {\"choices\":[{\"delta\":{\"content\":\"B\"}}]}\n";
        let tokens = parser.feed(chunk);
        assert_eq!(tokens, vec!["A", "B"]);
    }

    #[test]
    fn test_parser_done_signal() {
        let mut parser = SseParser::new();
        let chunk = b"data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\ndata: [DONE]\n";
        let tokens = parser.feed(chunk);
        assert_eq!(tokens, vec!["ok"]);
    }

    #[test]
    fn test_parser_partial_line() {
        let mut parser = SseParser::new();
        // 第一块：不完整的行
        let tokens1 = parser.feed(b"data: {\"choices\":[{\"delta\":{\"content\":\"Hel");
        assert!(tokens1.is_empty());

        // 第二块：完整行
        let tokens2 = parser.feed(b"lo\"}}]}\n");
        assert_eq!(tokens2, vec!["Hello"]);
    }

    #[test]
    fn test_parser_ignores_empty_and_comments() {
        let mut parser = SseParser::new();
        let chunk = b"\ndata: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\n";
        let tokens = parser.feed(chunk);
        assert_eq!(tokens, vec!["ok"]);
    }

    #[test]
    fn test_metadata() {
        let plugin = StreamParserPlugin::new();
        let meta = plugin.metadata();
        assert_eq!(meta.name, "stream_parser");
        assert_eq!(meta.stage, "api_call");
    }

    #[test]
    fn test_parser_empty_input() {
        let mut parser = SseParser::new();
        let tokens = parser.feed(b"");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_parser_non_data_lines() {
        let mut parser = SseParser::new();
        let chunk = b":heartbeat\n\n";
        let tokens = parser.feed(chunk);
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_parser_malformed_json() {
        let mut parser = SseParser::new();
        let chunk = b"data: {invalid json}\n";
        let tokens = parser.feed(chunk);
        assert!(tokens.is_empty()); // 不应 panic
    }

    #[test]
    fn test_parser_unicode_content() {
        let mut parser = SseParser::new();
        let chunk = "data: {\"choices\":[{\"delta\":{\"content\":\"你好世界🌍\"}}]}\n".as_bytes();
        let tokens = parser.feed(chunk);
        assert_eq!(tokens, vec!["你好世界🌍"]);
    }

    #[test]
    fn test_parser_multiple_done_signals() {
        let mut parser = SseParser::new();
        let chunk = b"data: [DONE]\ndata: [DONE]\ndata: {\"choices\":[{\"delta\":{\"content\":\"after\"}}]}\n";
        let tokens = parser.feed(chunk);
        // [DONE] 被跳过，"after" 正常解析
        assert_eq!(tokens, vec!["after"]);
    }

    #[test]
    fn test_parser_content_in_choices_array() {
        let mut parser = SseParser::new();
        // choices 数组为空
        let chunk = b"data: {\"choices\":[]}\n";
        let tokens = parser.feed(chunk);
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_parser_delta_without_content() {
        let mut parser = SseParser::new();
        // delta 有 role 但没有 content（DeepSeek 在结束前可能发这个）
        let chunk = b"data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n";
        let tokens = parser.feed(chunk);
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_parser_large_chunk_many_lines() {
        let mut parser = SseParser::new();
        let mut input = Vec::new();
        for i in 0..100 {
            input.extend_from_slice(
                format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"{}\"}}}}]}}\n", i).as_bytes()
            );
        }
        let tokens = parser.feed(&input);
        assert_eq!(tokens.len(), 100);
    }
}
