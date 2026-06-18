//! 统一错误类型。
//!
//! 所有内核和插件操作都通过 `CoreError` 返回，
//! 外部调用方（server）根据变体决定 HTTP 状态码和错误响应。

use thiserror::Error;

/// 内核操作可能返回的所有错误。
///
/// 每个变体携带一个人类可读的消息和可选的源错误。
#[derive(Error, Debug)]
pub enum CoreError {
    /// 插件执行错误。
    ///
    /// 单个插件 `execute()` 失败时返回。
    /// 默认不中断流水线，错误信息写入 `Context.custom["error"]`。
    #[error("插件错误 [{plugin}]: {message}")]
    Plugin {
        /// 发生错误的插件名称
        plugin: String,
        /// 错误描述
        message: String,
        /// 底层错误（可选）
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// 流水线执行错误。
    ///
    /// 流水线级别的结构性错误（如阶段配置无效），
    /// 不同于单个插件的运行时错误。
    #[error("流水线错误: {0}")]
    Pipeline(String),

    /// 配置错误。
    ///
    /// config.toml 解析失败、缺少必要字段、
    /// error_stage 不在 order 列表中等情况。
    #[error("配置错误: {0}")]
    Config(String),

    /// 动态加载器错误。
    ///
    /// 加载/卸载动态库时出错：
    /// 文件不存在、不是有效的动态库、缺少 `create_plugin` 导出符号等。
    #[error("动态加载器错误 [{context}]: {message}")]
    DynamicLoader {
        /// 发生错误的上下文（如文件路径）
        context: String,
        /// 错误描述
        message: String,
        /// 底层 libloading 错误
        #[source]
        source: Option<libloading::Error>,
    },

    /// 热加载错误。
    ///
    /// 文件监听、热重载过程中的错误。
    /// 热加载错误通常不致命，会回退到旧插件。
    #[error("热加载错误 [{context}]: {message}")]
    HotReload {
        /// 发生错误的上下文
        context: String,
        /// 错误描述
        message: String,
        /// 底层错误（如 notify::Error）
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl CoreError {
    /// 便捷构造：插件执行错误（无底层源错误）。
    pub fn plugin(plugin: impl Into<String>, message: impl Into<String>) -> Self {
        CoreError::Plugin {
            plugin: plugin.into(),
            message: message.into(),
            source: None,
        }
    }

    /// 便捷构造：配置错误。
    pub fn config(message: impl Into<String>) -> Self {
        CoreError::Config(message.into())
    }

    /// 便捷构造：流水线错误。
    pub fn pipeline(message: impl Into<String>) -> Self {
        CoreError::Pipeline(message.into())
    }

    /// 便捷构造：动态加载器错误（无底层源错误）。
    pub fn dynamic_loader(context: impl Into<String>, message: impl Into<String>) -> Self {
        CoreError::DynamicLoader {
            context: context.into(),
            message: message.into(),
            source: None,
        }
    }

    /// 便捷构造：热加载错误（无底层源错误）。
    pub fn hot_reload(context: impl Into<String>, message: impl Into<String>) -> Self {
        CoreError::HotReload {
            context: context.into(),
            message: message.into(),
            source: None,
        }
    }
}

/// 内核操作的 `Result` 别名。
pub type Result<T> = std::result::Result<T, CoreError>;
