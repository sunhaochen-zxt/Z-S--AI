//! 流水线上下文（Context）。
//!
//! Context 是插件间唯一的数据传递通道，分为两层：
//!
//! **协议层**（固定字段，极少修改）：
//! - `session_id`、`phase`、`abort`
//! - `user_input`、`ai_response`、`messages`
//!
//! **扩展层**（插件间通信的唯一方式）：
//! - `custom`：可序列化数据（JSON 兼容），用于角色卡、配置、token 统计等。
//! - `opaque`：不可序列化数据（运行时句柄），用于函数指针、channel sender 等。

use std::any::Any;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 一条对话消息。
///
/// 遵循 OpenAI Chat Completions 消息格式。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    /// 角色：`"system"`、`"user"`、`"assistant"` 等。
    pub role: String,
    /// 消息正文。
    pub content: String,
}

impl Message {
    /// 创建一条新消息。
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Message {
            role: role.into(),
            content: content.into(),
        }
    }

    /// 创建一条 system 角色消息。
    pub fn system(content: impl Into<String>) -> Self {
        Message::new("system", content)
    }

    /// 创建一条 user 角色消息。
    pub fn user(content: impl Into<String>) -> Self {
        Message::new("user", content)
    }

    /// 创建一条 assistant 角色消息。
    pub fn assistant(content: impl Into<String>) -> Self {
        Message::new("assistant", content)
    }
}

/// 流水线执行上下文。
///
/// # 协议层 vs 扩展层
///
/// 协议层字段修改需谨慎（影响所有插件），扩展层随时可用。
///
/// # 线程安全
///
/// Context 不实现 `Send` / `Sync`（因为 `opaque` 中包含
/// `Box<dyn Any>`，而 `Any` 不自动实现 `Send`）。
/// Context 在单次流水线执行中由单一线程持有（`&mut Context`），
/// 不同 session 的 Context 相互独立。
#[derive(Default)]
pub struct Context {
    // ============================================================
    // 协议层 —— 尽量不改
    // ============================================================

    /// 会话唯一标识（UUID v7）。
    ///
    /// 由 server 层在创建 Context 时生成。
    /// `history` 插件按此字段隔离存储。
    pub session_id: String,

    /// 当前阶段名称（字符串，非枚举）。
    ///
    /// 由流水线在执行时设置，插件可读取以了解当前所处阶段。
    pub phase: String,

    /// 是否中止流水线。
    ///
    /// 任意插件可设置此字段为 `true`。
    /// 当前阶段执行完毕后，流水线跳转到 `error_stage`。
    pub abort: bool,

    /// 用户本轮输入。
    ///
    /// 由 server 层在创建 Context 时设置。
    pub user_input: Option<String>,

    /// AI 回复。
    ///
    /// 由 `api_client` 插件在 `ApiCall` 阶段填充。
    /// 流式模式下，完整回复在流结束后写入。
    pub ai_response: Option<String>,

    /// 完整对话历史。
    ///
    /// 包含 system prompt、所有历史 user/assistant 消息。
    /// `history` 插件负责加载和保存。
    pub messages: Vec<Message>,

    // ============================================================
    // 扩展层 —— 插件间通信的唯一通道
    // ============================================================

    /// 可序列化数据（JSON 兼容值）。
    ///
    /// key 命名规范：`插件名.字段名`。
    ///
    /// 示例：
    /// - `character_card.data` — 角色卡 JSON
    /// - `api_client.config` — API 配置
    /// - `prompt_builder.output` — 构建好的 system prompt
    /// - `stream_parser.accumulator` — 流式回复累积文本
    /// - `history.save_directory` — 历史存储路径
    pub custom: HashMap<String, Value>,

    /// 不可序列化数据（运行时句柄）。
    ///
    /// key 命名规范：`插件名.字段名`。
    /// 不参与序列化，生命周期绑定到 Context 实例。
    ///
    /// 示例：
    /// - `stream_parser.parse_fn` — SSE 解析函数指针
    /// - `stream_parser.sender` — WebSocket 推送通道
    pub opaque: HashMap<String, Box<dyn Any + Send + Sync>>,
}

impl Context {
    /// 使用给定的 session_id 创建新 Context。
    pub fn new(session_id: impl Into<String>) -> Self {
        Context {
            session_id: session_id.into(),
            phase: String::new(),
            abort: false,
            user_input: None,
            ai_response: None,
            messages: Vec::new(),
            custom: HashMap::new(),
            opaque: HashMap::new(),
        }
    }

    // ============================================================
    // custom 层便捷方法
    // ============================================================

    /// 向 `custom` 插入一个可序列化的值。
    ///
    /// key 应遵循 `插件名.字段名` 规范。
    ///
    /// # 错误
    ///
    /// 当 `value` 无法序列化为 JSON 时返回 `Err`。
    /// 对于大多数实现了 `Serialize` 的标准类型，这不会发生。
    pub fn set_custom<V: Serialize>(
        &mut self,
        key: impl Into<String>,
        value: &V,
    ) -> Result<(), serde_json::Error> {
        let val = serde_json::to_value(value)?;
        self.custom.insert(key.into(), val);
        Ok(())
    }

    /// 向 `custom` 直接插入一个 `serde_json::Value`（跳过序列化步骤）。
    ///
    /// 用于已有 `serde_json::Value` 的场景，避免二次序列化。
    pub fn set_custom_value(
        &mut self,
        key: impl Into<String>,
        value: Value,
    ) {
        self.custom.insert(key.into(), value);
    }

    /// 从 `custom` 读取并反序列化一个值。
    ///
    /// 返回 `None` 如果 key 不存在或反序列化失败。
    /// 内部会克隆 `Value`，如需高频访问请使用 [`get_custom_value`] 获取引用。
    pub fn get_custom<V: for<'de> Deserialize<'de>>(
        &self,
        key: &str,
    ) -> Option<V> {
        self.custom
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// 从 `custom` 读取原始 `serde_json::Value` 引用（零拷贝）。
    ///
    /// 比 [`get_custom`] 更高效，适合只需读取部分字段或不确定类型的场景。
    pub fn get_custom_value(&self, key: &str) -> Option<&Value> {
        self.custom.get(key)
    }

    /// 从 `custom` 读取字符串值（零拷贝，直接引用 JSON 内部的 `&str`）。
    ///
    /// 比 `get_custom::<String>()` 高效（避免克隆和堆分配）。
    /// 返回 `None` 如果 key 不存在或值不是字符串。
    pub fn get_custom_str(&self, key: &str) -> Option<&str> {
        self.custom.get(key).and_then(|v| v.as_str())
    }

    /// 从 `custom` 读取数值（`i64`，零拷贝）。
    pub fn get_custom_i64(&self, key: &str) -> Option<i64> {
        self.custom.get(key).and_then(|v| v.as_i64())
    }

    /// 从 `custom` 读取布尔值（零拷贝）。
    pub fn get_custom_bool(&self, key: &str) -> Option<bool> {
        self.custom.get(key).and_then(|v| v.as_bool())
    }

    /// 检查 `custom` 中是否存在某 key。
    pub fn has_custom(&self, key: &str) -> bool {
        self.custom.contains_key(key)
    }

    /// 从 `custom` 中删除一个 key，返回旧值（如果存在）。
    pub fn remove_custom(&mut self, key: &str) -> Option<Value> {
        self.custom.remove(key)
    }

    // ============================================================
    // opaque 层便捷方法
    // ============================================================

    /// 向 `opaque` 插入一个不可序列化的运行时句柄。
    ///
    /// key 应遵循 `插件名.字段名` 规范。
    /// 值必须满足 `'static + Any + Send + Sync`。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// ctx.set_opaque("stream_parser.sender", tx);
    /// ctx.set_opaque("stream_parser.parse_fn", parse_sse_chunk as fn(&[u8]) -> Vec<String>);
    /// ```
    pub fn set_opaque<T: 'static + Any + Send + Sync>(
        &mut self,
        key: impl Into<String>,
        value: T,
    ) {
        self.opaque.insert(key.into(), Box::new(value));
    }

    /// 从 `opaque` 读取并向下转型为一个引用。
    ///
    /// 返回 `None` 如果 key 不存在或类型不匹配。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// if let Some(sender) = ctx.get_opaque::<UnboundedSender<String>>("stream_parser.sender") {
    ///     sender.send("hello".into()).ok();
    /// }
    /// ```
    pub fn get_opaque<T: 'static + Any>(
        &self,
        key: &str,
    ) -> Option<&T> {
        self.opaque
            .get(key)
            .and_then(|boxed| boxed.downcast_ref::<T>())
    }

    /// 检查 `opaque` 中是否存在某 key。
    pub fn has_opaque(&self, key: &str) -> bool {
        self.opaque.contains_key(key)
    }

    /// 从 `opaque` 中删除一个 key，返回旧值（如果存在）。
    pub fn remove_opaque(
        &mut self,
        key: &str,
    ) -> Option<Box<dyn Any + Send + Sync>> {
        self.opaque.remove(key)
    }

    // ============================================================
    // 错误处理
    // ============================================================

    /// 记录错误信息到 `custom["error"]` 并设置 `abort = true`。
    ///
    /// 这是一个便捷方法，供插件在遇到不可恢复的错误时使用。
    /// 使用 [`set_custom_value`] 直接插入 JSON，避免序列化开销。
    pub fn abort_with_error(&mut self, message: impl Into<String>) {
        self.set_custom_value("error", serde_json::json!({
            "message": message.into(),
            "plugin": self.phase.clone(),
        }));
        self.abort = true;
    }

    /// 获取已记录的错误信息。
    pub fn get_error(&self) -> Option<String> {
        self.get_custom::<serde_json::Value>("error")
            .and_then(|v| v.get("message").cloned())
            .and_then(|v| v.as_str().map(String::from))
    }

    // ============================================================
    // 消息操作
    // ============================================================

    /// 追加一条消息到对话历史。
    pub fn push_message(&mut self, role: impl Into<String>, content: impl Into<String>) {
        self.messages.push(Message::new(role, content));
    }

    /// 清空对话历史。
    pub fn clear_messages(&mut self) {
        self.messages.clear();
    }

    /// 获取对话历史中的消息数量。
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }
}

// 手动实现 Debug 以避免 `opaque` 字段的输出噪音
impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("session_id", &self.session_id)
            .field("phase", &self.phase)
            .field("abort", &self.abort)
            .field("user_input", &self.user_input)
            .field("ai_response", &self.ai_response)
            .field("messages", &format!("[{} messages]", self.messages.len()))
            .field("custom", &self.custom)
            .field("opaque", &format!("[{} opaque handles]", self.opaque.len()))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_new() {
        let ctx = Context::new("session-1");
        assert_eq!(ctx.session_id, "session-1");
        assert!(!ctx.abort);
        assert!(ctx.user_input.is_none());
        assert!(ctx.ai_response.is_none());
        assert!(ctx.messages.is_empty());
        assert!(ctx.custom.is_empty());
        assert!(ctx.opaque.is_empty());
    }

    #[test]
    fn test_custom_set_get() {
        let mut ctx = Context::new("test");
        let data = serde_json::json!({"name": "李白", "personality": "豪放"});
        ctx.set_custom("character_card.data", &data).unwrap();

        let retrieved: serde_json::Value = ctx.get_custom("character_card.data").unwrap();
        assert_eq!(retrieved["name"], "李白");
    }

    #[test]
    fn test_custom_missing_key() {
        let ctx = Context::new("test");
        let result: Option<serde_json::Value> = ctx.get_custom("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_opaque_set_get() {
        let mut ctx = Context::new("test");
        let data: i32 = 42;
        ctx.set_opaque("test.value", data);

        let retrieved: &i32 = ctx.get_opaque("test.value").unwrap();
        assert_eq!(*retrieved, 42);
    }

    #[test]
    fn test_opaque_type_mismatch() {
        let mut ctx = Context::new("test");
        ctx.set_opaque("test.value", 42i32);

        // 尝试以错误类型读取
        let result: Option<&String> = ctx.get_opaque("test.value");
        assert!(result.is_none());
    }

    #[test]
    fn test_abort_with_error() {
        let mut ctx = Context::new("test");
        ctx.abort_with_error("something went wrong");

        assert!(ctx.abort);
        let error: serde_json::Value = ctx.get_custom("error").unwrap();
        assert_eq!(error["message"], "something went wrong");
    }

    #[test]
    fn test_push_message() {
        let mut ctx = Context::new("test");
        ctx.push_message("system", "You are a helpful assistant.");
        ctx.push_message("user", "Hello!");

        assert_eq!(ctx.message_count(), 2);
        assert_eq!(ctx.messages[0].role, "system");
        assert_eq!(ctx.messages[1].content, "Hello!");
    }

    #[test]
    fn test_opaque_function_pointer() {
        let mut ctx = Context::new("test");
        fn dummy_parse(_data: &[u8]) -> Vec<String> { vec![] }
        ctx.set_opaque("stream_parser.parse_fn", dummy_parse as fn(&[u8]) -> Vec<String>);

        let retrieved: Option<&fn(&[u8]) -> Vec<String>> = ctx.get_opaque("stream_parser.parse_fn");
        assert!(retrieved.is_some());
    }
}
