//! 测试桩插件（test_stub）。
//!
//! 这是 ZS-AI 的第一个插件，核心目的是 **验证插件系统的加载/执行链路**：
//!
//! 1. 动态库编译为 `.so` → 被 `DynamicLoader` 发现和加载
//! 2. `create_plugin()` 导出函数被调用 → 返回 `Box<dyn Plugin>`
//! 3. `metadata()` 返回正确的阶段归属和优先级
//! 4. `execute()` 在流水线中被调用 → 写入 Context
//!
//! # 阶段归属
//!
//! 此插件在 `postprocess` 阶段执行，用于验证：
//! - `custom` 和 `opaque` 读写
//! - Context 最终状态
//! - 健康检查和指标收集
//!
//! # 配置（config.toml）
//!
//! ```toml
//! [plugins.test_stub]
//! log_level = "debug"
//! echo_message = "Hello from test_stub!"
//! ```

use std::collections::HashMap;

use serde_json::Value;
use tracing::{debug, info};

use zsai_core::*;

// ============================================================
// 插件结构体
// ============================================================

struct TestStubPlugin {
    /// 插件元信息（在构造时设置）
    meta: PluginMeta,
    /// 日志级别（从 config.toml 读取）
    log_level: String,
    /// 回显消息（从 config.toml 读取，写入 ctx.custom）
    echo_message: String,
}

impl TestStubPlugin {
    fn new() -> Self {
        let meta = PluginMeta::new("test_stub", (0, 1, 0), "postprocess", 100)
            .with_capability("health_check")
            .with_capability("metrics")
            .with_capability("validate_config");

        TestStubPlugin {
            meta,
            log_level: "info".to_string(),
            echo_message: "Hello from test_stub!".to_string(),
        }
    }
}

// ============================================================
// Plugin trait 实现
// ============================================================

impl Plugin for TestStubPlugin {
    fn metadata(&self) -> PluginMeta {
        self.meta.clone()
    }

    fn execute(&self, ctx: &mut Context) -> Result<PluginResult> {
        info!(
            stage = %ctx.phase,
            plugin = "test_stub",
            messages = ctx.message_count(),
            "test_stub 插件执行中"
        );

        // 演示 custom 层读写（先 clone 避免借用冲突）
        let phase = ctx.phase.clone();
        ctx.set_custom("test_stub.stage", &phase)
            .map_err(|e| CoreError::plugin("test_stub", format!("set_custom 失败: {}", e)))?;

        ctx.set_custom("test_stub.echo", &self.echo_message)
            .map_err(|e| CoreError::plugin("test_stub", format!("set_custom 失败: {}", e)))?;

        // 演示 opaque 层读写（使用 i32 作为示例）
        let counter: i32 = ctx.get_opaque("test_stub.counter").copied().unwrap_or(0);
        ctx.set_opaque("test_stub.counter", counter + 1);

        debug!(
            counter = counter + 1,
            echo = %self.echo_message,
            "test_stub 状态更新"
        );

        // 追加一条系统消息到对话历史
        ctx.push_message("system", format!("[test_stub v{}.{}.{}] 流水线活跃，阶段: {}",
            self.meta.version.0,
            self.meta.version.1,
            self.meta.version.2,
            ctx.phase,
        ));

        Ok(PluginResult::r#continue())
    }

    // ---- 可选能力 ----

    fn init(&mut self, config: &Value) -> Result<()> {
        if !config.is_null() {
            if let Some(level) = config.get("log_level").and_then(|v| v.as_str()) {
                self.log_level = level.to_string();
            }
            if let Some(msg) = config.get("echo_message").and_then(|v| v.as_str()) {
                self.echo_message = msg.to_string();
            }
        }
        info!(
            log_level = %self.log_level,
            echo = %self.echo_message,
            "test_stub 插件初始化完成"
        );
        Ok(())
    }

    fn health_check(&self) -> Result<HealthStatus> {
        Ok(HealthStatus::healthy())
    }

    fn metrics(&self) -> HashMap<String, f64> {
        let mut m = HashMap::new();
        m.insert("test_stub.counter".to_string(), 0.0);
        m
    }

    fn validate_config(&self, config: &Value) -> Result<()> {
        if !config.is_null() {
            if let Some(level) = config.get("log_level") {
                let s = level.as_str().unwrap_or("");
                if !matches!(s, "trace" | "debug" | "info" | "warn" | "error") {
                    return Err(CoreError::plugin(
                        "test_stub",
                        format!("无效的 log_level: '{}'，有效值: trace/debug/info/warn/error", s),
                    ));
                }
            }
        }
        Ok(())
    }
}

// ============================================================
// FFI 导出（插件系统入口）
// ============================================================

/// 插件构造函数。
///
/// 由 `DynamicLoader` 通过 `libloading` 调用。
/// 返回的 `Box<dyn Plugin>` 被包裹在 `Arc<RwLock<>>` 中供多线程使用。
#[no_mangle]
pub extern "C" fn create_plugin() -> Box<dyn Plugin> {
    Box::new(TestStubPlugin::new())
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use zsai_core::{Context, Plugin, PluginMeta};

    #[test]
    fn test_create_plugin_returns_valid_instance() {
        let plugin = create_plugin();
        let meta = plugin.metadata();
        assert_eq!(meta.name, "test_stub");
        assert!(meta.capabilities.contains(&"health_check".to_string()));
    }

    #[test]
    fn test_execute_writes_to_context() {
        let plugin = TestStubPlugin::new();
        let mut ctx = Context::new("test-session");
        ctx.phase = "postprocess".to_string();

        let result = plugin.execute(&mut ctx);
        assert!(result.is_ok());

        // 检查 custom 层写入
        let echo: String = ctx.get_custom("test_stub.echo").unwrap();
        assert_eq!(echo, "Hello from test_stub!");

        // 检查 opaque 层写入
        let counter: &i32 = ctx.get_opaque("test_stub.counter").unwrap();
        assert_eq!(*counter, 1);

        // 检查消息追加
        assert!(ctx.message_count() >= 1);
        assert!(ctx.messages.iter().any(|m| m.role == "system"));
    }

    #[test]
    fn test_init_reads_config() {
        let mut plugin = TestStubPlugin::new();
        let config = serde_json::json!({
            "log_level": "warn",
            "echo_message": "configured message"
        });

        plugin.init(&config).unwrap();
        assert_eq!(plugin.echo_message, "configured message");
        assert_eq!(plugin.log_level, "warn");
    }

    #[test]
    fn test_validate_config_rejects_invalid_log_level() {
        let plugin = TestStubPlugin::new();
        let config = serde_json::json!({"log_level": "invalid"});

        let result = plugin.validate_config(&config);
        assert!(result.is_err());
    }
}
