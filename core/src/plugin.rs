//! 插件系统核心 trait。
//!
//! # 设计原则
//!
//! 1. `metadata()` + `execute()` 是两个 **必须实现** 的方法。
//! 2. 其余方法全部有默认实现（空操作或默认值），新增方法不影响旧插件。
//! 3. Plugin trait 必须保持 **对象安全**（object-safe）：
//!    - 所有方法不能有泛型参数。
//!    - 不能返回 `Self`（除 `Box<Self>` 外，但 trait 方法不使用）。
//!    - 不能有 `impl Trait` 参数或返回值。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde_json::Value;

use crate::context::Context;
use crate::error::Result;

/// 插件引用类型：线程安全引用计数 + 读写锁。
///
/// 所有动态加载的插件都被包裹在此类型中：
/// - `Arc`：多 session 可共享同一插件实例。
/// - `RwLock`：`execute()` 通过读锁并发调用，
///   `init()`/`shutdown()` 等通过写锁独占调用。
/// - `Box<dyn Plugin>`：匹配 `create_plugin()` 的返回类型。
pub type PluginRef = Arc<RwLock<Box<dyn Plugin>>>;

/// 插件元信息。
///
/// 在插件动态库加载时立即调用 `metadata()` 获取，
/// 内核据此确定插件的身份、所属阶段和优先级。
#[derive(Debug, Clone)]
pub struct PluginMeta {
    /// 插件唯一标识。
    ///
    /// 与 `config.toml` 中 `[plugins.xxx]` 的 key 对应。
    /// 例如 `"character_card"`、`"api_client"`。
    pub name: String,

    /// 语义化版本 `(主版本, 次版本, 补丁版本)`。
    ///
    /// - 主版本变更：Plugin trait 签名变化，二进制不兼容，需重启。
    /// - 次版本变更：新增可选 trait 方法（有默认实现），热加载兼容。
    /// - 补丁版本变更：实现细节修改，热加载兼容。
    pub version: (u16, u16, u16),

    /// 插件所属阶段名称。
    ///
    /// 由插件自己声明，如 `"preprocess"`、`"api_call"`。
    /// 内核在加载时校验此阶段是否在 `config.toml` 的
    /// `[stages].order` 列表中，不在则跳过加载。
    pub stage: String,

    /// 阶段内优先级（值越小越先执行）。
    pub priority: i32,

    /// 插件声明实现了哪些可选能力。
    ///
    /// **内核不在运行时查询此字段**（所有可选方法都有默认实现，
    /// 直接调用即可）。它的用途是：
    /// - `/health` 端点筛选实现了 `health_check` 的插件。
    /// - 监控/调试工具展示插件能力矩阵。
    /// - 开发者阅读代码时快速了解插件提供的接口。
    pub capabilities: Vec<String>,
}

impl PluginMeta {
    /// 创建新的 PluginMeta。
    ///
    /// `capabilities` 初始为空，插件可在构造后追加。
    pub fn new(
        name: impl Into<String>,
        version: (u16, u16, u16),
        stage: impl Into<String>,
        priority: i32,
    ) -> Self {
        PluginMeta {
            name: name.into(),
            version,
            stage: stage.into(),
            priority,
            capabilities: Vec::new(),
        }
    }

    /// 添加一个能力声明。
    pub fn with_capability(mut self, cap: impl Into<String>) -> Self {
        self.capabilities.push(cap.into());
        self
    }

    /// 检查是否声明了某能力。
    pub fn has_capability(&self, cap: &str) -> bool {
        self.capabilities.iter().any(|c| c == cap)
    }
}

/// 插件 `execute()` 的返回值。
///
/// 控制流水线中本阶段后续插件的执行。
#[derive(Debug, Clone)]
pub struct PluginResult {
    /// 是否停止传播。
    ///
    /// `true` 表示跳过当前阶段剩余的插件，
    /// 直接进入下一阶段（或 abort 目标阶段）。
    /// `false`（默认）表示继续执行当前阶段的下一个插件。
    pub stop_propagation: bool,
}

impl PluginResult {
    /// 创建"继续传播"的结果（默认）。
    pub fn r#continue() -> Self {
        PluginResult {
            stop_propagation: false,
        }
    }

    /// 创建"停止传播"的结果。
    pub fn stop() -> Self {
        PluginResult {
            stop_propagation: true,
        }
    }
}

impl Default for PluginResult {
    fn default() -> Self {
        PluginResult::r#continue()
    }
}

/// 插件健康状态。
#[derive(Debug, Clone)]
pub struct HealthStatus {
    /// 是否健康。
    pub healthy: bool,
    /// 状态描述（如 "OK"、"数据库连接失败"）。
    pub message: String,
}

impl Default for HealthStatus {
    fn default() -> Self {
        HealthStatus {
            healthy: true,
            message: "OK".into(),
        }
    }
}

impl HealthStatus {
    /// 创建健康状态。
    pub fn healthy() -> Self {
        HealthStatus::default()
    }

    /// 创建不健康状态。
    pub fn unhealthy(message: impl Into<String>) -> Self {
        HealthStatus {
            healthy: false,
            message: message.into(),
        }
    }
}

/// 插件 trait。
///
/// # 必须实现（2 个方法）
///
/// - `metadata()` — 返回插件元信息。
/// - `execute()` — 核心逻辑，读写 Context。
///
/// # 可选能力（9 个方法，全部有默认实现）
///
/// - `init()` / `shutdown()` — 生命周期管理。
/// - `health_check()` — 健康检查。
/// - `metrics()` — 暴露指标。
/// - `validate_config()` — 校验配置。
/// - `on_stage_enter()` / `on_stage_exit()` — 阶段钩子。
/// - `before_reload()` / `after_reload()` — 热重载钩子。
///
/// # 对象安全约束
///
/// 此 trait 必须作为 `dyn Plugin` 使用（通过动态库加载）。
/// 因此所有方法：
/// - 不能有泛型参数。
/// - 不能返回或接收 `Self`（`execute` 接收 `&self`，这是允许的）。
/// - 不能使用 `impl Trait`。
///
/// # 线程安全
///
/// `Send + Sync` 约束确保插件可以在多线程环境中使用。
/// 在动态加载器中，插件被包装在 `Arc<RwLock<dyn Plugin>>` 中：
/// - `execute()` 通过读锁（`RwLock::read`）调用，支持多 session 并发。
/// - `init()` / `shutdown()` 等通过写锁（`RwLock::write`）调用。
pub trait Plugin: Send + Sync {
    // ============================================================
    // 必须实现
    // ============================================================

    /// 返回插件元信息。
    ///
    /// 在动态库加载时调用一次。
    /// 返回值决定了插件的身份、阶段归属和执行优先级。
    fn metadata(&self) -> PluginMeta;

    /// 核心执行逻辑。
    ///
    /// 在流水线中调用，读写 Context。
    /// 当前阶段和流水线状态可通过 `ctx.phase` 和 `ctx.abort` 读取。
    fn execute(&self, ctx: &mut Context) -> Result<PluginResult>;

    // ============================================================
    // 可选能力（带默认实现）
    // ============================================================

    /// 初始化插件。
    ///
    /// 接收 `config.toml` 中 `[plugins.插件名]` 段的配置。
    /// 在首次加载和热重载时调用。
    fn init(&mut self, _config: &Value) -> Result<()> {
        Ok(())
    }

    /// 关闭插件。
    ///
    /// 保存状态、释放资源。
    /// 在卸载和热重载前调用。
    /// 状态应序列化到 `ctx.custom["插件名.state"]` 或磁盘。
    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }

    /// 健康检查。
    ///
    /// 返回插件是否正常运行。
    /// 由 `/health` 端点调用。
    fn health_check(&self) -> Result<HealthStatus> {
        Ok(HealthStatus::default())
    }

    /// 暴露指标。
    ///
    /// 返回键值对形式的监控指标。
    /// 由监控系统定期采集。
    fn metrics(&self) -> HashMap<String, f64> {
        HashMap::new()
    }

    /// 校验配置。
    ///
    /// 在 `init()` 之前调用，验证 `config.toml` 中
    /// `[plugins.插件名]` 段的配置是否合法。
    fn validate_config(&self, _config: &Value) -> Result<()> {
        Ok(())
    }

    /// 阶段进入钩子。
    ///
    /// 在流水线进入插件所属阶段时调用（在 `execute()` 之前）。
    /// 可用于日志、计时等横切关注点。
    fn on_stage_enter(&self, _stage: &str) -> Result<()> {
        Ok(())
    }

    /// 阶段退出钩子。
    ///
    /// 在流水线离开插件所属阶段时调用（在阶段所有插件执行完毕后）。
    fn on_stage_exit(&self, _stage: &str) -> Result<()> {
        Ok(())
    }

    /// 热重载前钩子。
    ///
    /// 在热重载卸载旧插件实例之前调用。
    /// 此时插件应保存所有必要状态。
    fn before_reload(&mut self) -> Result<()> {
        Ok(())
    }

    /// 热重载后钩子。
    ///
    /// 在热重载加载新插件实例并调用 `init()` 之后调用。
    /// 此时插件应恢复之前保存的状态。
    fn after_reload(&mut self) -> Result<()> {
        Ok(())
    }
}
