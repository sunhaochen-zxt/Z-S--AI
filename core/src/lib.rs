//! # ZS-AI 内核（zsai-core）
//!
//! 微内核+插件化架构的核心 crate。不实现任何业务逻辑，
//! 只负责配置驱动下的插件编排和 Context 传递。
//!
//! 注意：crate 名为 `zsai-core`（避免与标准库 `core` 冲突），
//! 导入时使用 `use zsai_core::...`。
//!
//! ## 模块概览
//!
//! | 模块 | 职责 |
//! |------|------|
//! | [`context`] | 流水线上下文（协议层 + 扩展层） |
//! | [`plugin`] | Plugin trait 定义（2 必须 + 9 可选） |
//! | [`config`] | config.toml 解析与校验 |
//! | [`pipeline`] | 流水线编排引擎 |
//! | [`dynamic_loader`] | 动态库加载器（libloading） |
//! | [`hot_reload`] | 热加载管理器（notify） |
//! | [`error`] | 统一错误类型 |
//!
//! ## 快速开始
//!
//! ```ignore
//! use zsai_core::{AppConfig, Context, DynamicLoader, run_pipeline};
//!
//! // 1. 读取配置
//! let config = AppConfig::load("config.toml")?;
//!
//! // 2. 加载插件
//! let loader = Arc::new(RwLock::new(DynamicLoader::new(&config.hot_reload.plugin_dir)));
//! loader.write().unwrap().load_all();
//!
//! // 3. 初始化插件（需要将 config.plugins HashMap<String, toml::Value> 转为 HashMap<String, serde_json::Value>）
//! use std::collections::HashMap;
//! let plugin_configs: HashMap<String, serde_json::Value> = config.plugins.iter().map(|(k, v)| {
//!     let json_str = serde_json::to_string(v).unwrap_or_default();
//!     (k.clone(), serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null))
//! }).collect();
//! loader.write().unwrap().init_all(&plugin_configs);
//!
//! // 4. 创建 Context
//! let mut ctx = Context::new("session-1");
//! ctx.user_input = Some("你好！".into());
//!
//! // 5. 执行流水线
//! let stage_map = loader.read().unwrap().build_stage_map(&config.stages.order);
//! let stats = run_pipeline(&mut ctx, &config.stages, &stage_map);
//!
//! // 6. 返回结果
//! println!("AI: {}", ctx.ai_response.unwrap_or_default());
//! ```

pub mod config;
pub mod context;
pub mod dynamic_loader;
pub mod error;
pub mod hot_reload;
pub mod pipeline;
pub mod plugin;

// 最常用的类型在 crate 根部重新导出
pub use config::AppConfig;
pub use context::{Context, Message};
pub use dynamic_loader::DynamicLoader;
pub use error::{CoreError, Result};
pub use pipeline::{run_pipeline, PipelineStats};
pub use plugin::{
    HealthStatus, Plugin, PluginMeta, PluginRef, PluginResult,
};
