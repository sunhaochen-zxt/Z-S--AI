//! 流水线执行引擎。
//!
//! 按 `config.toml` 中 `[stages].order` 定义的顺序执行各阶段，
//! 每个阶段内按插件优先级排序并逐个执行。
//!
//! # 执行流程
//!
//! ```text
//! 创建 Context → 遍历 stages.order
//!   → 收集该阶段插件（按 priority 排序）
//!     → on_stage_enter
//!     → 逐个 execute
//!       → abort? → 跳到 error_stage
//!       → stop_propagation? → 跳过该阶段剩余插件
//!     → on_stage_exit
//!   → 进入下一阶段
//! → 返回 Context
//! ```

use std::collections::HashMap;

use tracing::{debug, error, info, instrument, warn};

use crate::config::StagesConfig;
use crate::context::Context;
use crate::plugin::PluginRef;

/// 流水线执行统计信息。
#[derive(Debug, Default, Clone)]
pub struct PipelineStats {
    /// 执行的阶段总数。
    pub stages_executed: usize,
    /// 执行的插件总数。
    pub plugins_executed: usize,
    /// 跳过的插件数（因 stop_propagation）。
    pub plugins_skipped: usize,
    /// 失败的插件数。
    pub plugins_failed: usize,
    /// 是否因 abort 提前结束。
    pub was_aborted: bool,
    /// 跳转到的错误阶段。
    pub error_stage: Option<String>,
}

/// 执行完整流水线。
///
/// # 参数
///
/// - `ctx`：流水线上下文（`&mut`，插件按序读写）。
/// - `stages`：阶段配置（order + error_stage + 额外参数）。
/// - `stage_plugins`：预分组的插件集合，key 为阶段名，
///   value 为已按 priority 排序的 [`PluginRef`] 列表。
///
/// # 返回
///
/// 返回执行统计信息。即使部分插件失败，也会继续执行后续插件，
/// 除非设置了 `ctx.abort`。
///
/// # 并发安全
///
/// 同一 session 的流水线调用是串行的（server 层保证）。
/// 不同 session 可以并行调用，各自持有独立的 Context 和
/// 从 `stage_plugins` 的 `Arc` 中克隆的独立引用。
#[instrument(skip(ctx, stage_plugins), fields(session = %ctx.session_id))]
pub fn run_pipeline(
    ctx: &mut Context,
    stages: &StagesConfig,
    stage_plugins: &HashMap<String, Vec<PluginRef>>,
) -> PipelineStats {
    let mut stats = PipelineStats::default();

    info!(
        order = ?stages.order,
        error_stage = %stages.error_stage,
        total_stages = stages.order.len(),
        "流水线开始执行"
    );

    for stage_name in &stages.order {
        // 检查 abort：如果已设置，尝试跳转到 error_stage
        if ctx.abort {
            stats.was_aborted = true;
            stats.error_stage = Some(stages.error_stage.clone());
            // 如果当前已经在 error_stage 中，正常执行
            if stage_name != &stages.error_stage {

                // 找到 error_stage 在 order 中的位置
                if let Some(error_idx) = stages.order.iter().position(|s| s == &stages.error_stage) {
                    let current_idx = stages.order.iter().position(|s| s == stage_name).unwrap_or(0);
                    if error_idx < current_idx {
                        // error_stage 在当前阶段之前，已经执行过了，直接返回
                        debug!(
                            error_stage = %stages.error_stage,
                            "abort: error_stage 已执行过，跳过剩余阶段"
                        );
                        break;
                    }
                }

                debug!(
                    error_stage = %stages.error_stage,
                    current_stage = %stage_name,
                    "abort: 跳过中间阶段，等待到达 error_stage"
                );
                continue;
            }
        }

        ctx.phase = stage_name.clone();
        stats.stages_executed += 1;

        // 获取该阶段的插件列表
        let plugins = match stage_plugins.get(stage_name) {
            Some(p) => p,
            None => {
                debug!(stage = %stage_name, "该阶段无插件，跳过");
                continue;
            }
        };

        if plugins.is_empty() {
            debug!(stage = %stage_name, "该阶段插件列表为空，跳过");
            continue;
        }

        debug!(
            stage = %stage_name,
            plugin_count = plugins.len(),
            "进入阶段"
        );

        // 调用阶段进入钩子
        for plugin_arc in plugins {
            if let Ok(plugin) = plugin_arc.read() {
                if let Err(e) = plugin.on_stage_enter(stage_name) {
                    warn!(
                        stage = %stage_name,
                        plugin = %plugin.metadata().name,
                        error = %e,
                        "on_stage_enter 失败"
                    );
                }
            }
        }

        // 按优先级顺序执行插件
        let mut skip_remaining = false;

        for (idx, plugin_arc) in plugins.iter().enumerate() {
            if skip_remaining {
                stats.plugins_skipped += 1;
                debug!(
                    stage = %stage_name,
                    plugin_index = idx,
                    "因 stop_propagation 跳过"
                );
                continue;
            }

            // 获取读锁
            let plugin = match plugin_arc.read() {
                Ok(p) => p,
                Err(e) => {
                    error!(
                        stage = %stage_name,
                        plugin_index = idx,
                        error = %e,
                        "无法获取插件读锁（RwLock 中毒）"
                    );
                    stats.plugins_failed += 1;
                    continue;
                }
            };

            let plugin_name = plugin.metadata().name;

            debug!(
                stage = %stage_name,
                plugin = %plugin_name,
                priority = plugin.metadata().priority,
                "执行插件"
            );

            // 执行插件
            match plugin.execute(ctx) {
                Ok(result) => {
                    stats.plugins_executed += 1;

                    if result.stop_propagation {
                        debug!(
                            stage = %stage_name,
                            plugin = %plugin_name,
                            "插件请求 stop_propagation，跳过本阶段剩余插件"
                        );
                        skip_remaining = true;
                    }

                    if ctx.abort {
                        info!(
                            stage = %stage_name,
                            plugin = %plugin_name,
                            "插件设置了 ctx.abort"
                        );
                        // 不立即跳出，让 abort 检查在阶段循环顶部处理
                        skip_remaining = true;
                    }
                }
                Err(e) => {
                    stats.plugins_failed += 1;
                    warn!(
                        stage = %stage_name,
                        plugin = %plugin_name,
                        error = %e,
                        "插件执行失败"
                    );

                    // 记录错误到 Context（不覆盖已有错误）
                    if ctx.get_custom_value("error").is_none() {
                        ctx.set_custom_value(
                            "error",
                            serde_json::json!({
                                "plugin": plugin_name,
                                "stage": stage_name,
                                "message": e.to_string(),
                            }),
                        );
                    }
                }
            }
        }

        // 调用阶段退出钩子
        for plugin_arc in plugins {
            if let Ok(plugin) = plugin_arc.read() {
                if let Err(e) = plugin.on_stage_exit(stage_name) {
                    warn!(
                        stage = %stage_name,
                        plugin = %plugin.metadata().name,
                        error = %e,
                        "on_stage_exit 失败"
                    );
                }
            }
        }

        debug!(
            stage = %stage_name,
            executed = stats.plugins_executed,
            skipped = stats.plugins_skipped,
            failed = stats.plugins_failed,
            "阶段完成"
        );

        // 再次检查 abort（在阶段退出后）
        if ctx.abort && stage_name == &stages.error_stage {
            info!("error_stage 完成，流水线因 abort 提前结束");
            break;
        }
    }

    info!(
        stages_executed = stats.stages_executed,
        plugins_executed = stats.plugins_executed,
        plugins_skipped = stats.plugins_skipped,
        plugins_failed = stats.plugins_failed,
        was_aborted = stats.was_aborted,
        "流水线执行完成"
    );

    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, RwLock};
    use crate::context::Context;
    use crate::error::{CoreError, Result};
    use crate::plugin::{Plugin, PluginMeta, PluginResult};

    /// 测试用插件：原样返回，什么也不做。
    struct NoopPlugin {
        meta: PluginMeta,
    }

    impl NoopPlugin {
        fn new(name: &str, stage: &str, priority: i32) -> Self {
            NoopPlugin {
                meta: PluginMeta::new(name, (1, 0, 0), stage, priority),
            }
        }
    }

    impl Plugin for NoopPlugin {
        fn metadata(&self) -> PluginMeta { self.meta.clone() }
        fn execute(&self, _ctx: &mut Context) -> Result<PluginResult> {
            Ok(PluginResult::r#continue())
        }
    }

    /// 测试用插件：执行时设置 abort。
    struct AbortPlugin {
        meta: PluginMeta,
    }

    impl Plugin for AbortPlugin {
        fn metadata(&self) -> PluginMeta { self.meta.clone() }
        fn execute(&self, ctx: &mut Context) -> Result<PluginResult> {
            ctx.abort = true;
            Ok(PluginResult::r#continue())
        }
    }

    /// 测试用插件：追加消息。
    struct AppendPlugin {
        meta: PluginMeta,
        text: String,
    }

    impl Plugin for AppendPlugin {
        fn metadata(&self) -> PluginMeta { self.meta.clone() }
        fn execute(&self, ctx: &mut Context) -> Result<PluginResult> {
            ctx.push_message("assistant", &self.text);
            Ok(PluginResult::r#continue())
        }
    }

    fn make_plugin_arc(plugin: impl Plugin + 'static) -> PluginRef {
        Arc::new(RwLock::new(Box::new(plugin)))
    }

    fn make_stages(order: Vec<&str>) -> StagesConfig {
        let order: Vec<String> = order.into_iter().map(String::from).collect();
        let error_stage = order.last().cloned().unwrap_or_default();
        StagesConfig {
            order,
            error_stage,
            extra: HashMap::new(),
        }
    }

    #[test]
    fn test_empty_pipeline() {
        let mut ctx = Context::new("test");
        let stages = make_stages(vec!["preprocess"]);
        let plugins: HashMap<String, Vec<PluginRef>> = HashMap::new();

        let stats = run_pipeline(&mut ctx, &stages, &plugins);
        assert_eq!(stats.stages_executed, 1);
        assert_eq!(stats.plugins_executed, 0);
    }

    #[test]
    fn test_single_plugin_execution() {
        let mut ctx = Context::new("test");
        let stages = make_stages(vec!["api_call"]);
        let mut plugins: HashMap<String, Vec<PluginRef>> = HashMap::new();
        plugins.insert(
            "api_call".into(),
            vec![
                make_plugin_arc(NoopPlugin::new("test_plugin", "api_call", 10)),
            ],
        );

        let stats = run_pipeline(&mut ctx, &stages, &plugins);
        assert_eq!(stats.plugins_executed, 1);
        assert_eq!(stats.plugins_failed, 0);
    }

    #[test]
    fn test_priority_ordering() {
        let mut ctx = Context::new("test");
        let stages = make_stages(vec!["api_call"]);
        let mut plugins: HashMap<String, Vec<PluginRef>> = HashMap::new();
        plugins.insert(
            "api_call".into(),
            vec![
                make_plugin_arc(AppendPlugin {
                    meta: PluginMeta::new("first", (1, 0, 0), "api_call", 1),
                    text: "first".into(),
                }),
                make_plugin_arc(AppendPlugin {
                    meta: PluginMeta::new("second", (1, 0, 0), "api_call", 2),
                    text: "second".into(),
                }),
            ],
        );

        let stats = run_pipeline(&mut ctx, &stages, &plugins);
        assert_eq!(stats.plugins_executed, 2);
        assert_eq!(ctx.messages[0].content, "first");
        assert_eq!(ctx.messages[1].content, "second");
    }

    #[test]
    fn test_abort_jumps_to_error_stage() {
        let mut ctx = Context::new("test");
        let stages = make_stages(vec!["preprocess", "api_call", "postprocess"]);
        let stages_clone = stages.clone();
        let error_stage = stages_clone.error_stage.clone();

        let mut plugins: HashMap<String, Vec<PluginRef>> = HashMap::new();
        plugins.insert(
            "preprocess".into(),
            vec![
                make_plugin_arc(AbortPlugin {
                    meta: PluginMeta::new("aborter", (1, 0, 0), "preprocess", 1),
                }),
            ],
        );
        plugins.insert(
            "api_call".into(),
            vec![
                make_plugin_arc(AppendPlugin {
                    meta: PluginMeta::new("should_not_run", (1, 0, 0), "api_call", 1),
                    text: "SHOULD NOT APPEAR".into(),
                }),
            ],
        );
        plugins.insert(
            "postprocess".into(),
            vec![
                make_plugin_arc(AppendPlugin {
                    meta: PluginMeta::new("cleanup", (1, 0, 0), "postprocess", 1),
                    text: "cleanup".into(),
                }),
            ],
        );

        // 需要把 error_stage 从克隆中取出
        drop(stages_clone);
        let stats = run_pipeline(&mut ctx, &stages, &plugins);

        assert!(stats.was_aborted);
        assert_eq!(stats.error_stage, Some(error_stage));
        // "should_not_run" 不应该被执行
        assert!(ctx.messages.iter().all(|m| m.content != "SHOULD NOT APPEAR"));
        // cleanup 应该被执行
        assert!(ctx.messages.iter().any(|m| m.content == "cleanup"));
    }

    #[test]
    fn test_stop_propagation() {
        // 使用一个返回 stop_propagation 的插件
        struct StopPlugin {
            meta: PluginMeta,
        }
        impl Plugin for StopPlugin {
            fn metadata(&self) -> PluginMeta { self.meta.clone() }
            fn execute(&self, _ctx: &mut Context) -> Result<PluginResult> {
                Ok(PluginResult::stop())
            }
        }

        let mut ctx = Context::new("test");
        let stages = make_stages(vec!["api_call"]);
        let mut plugins: HashMap<String, Vec<PluginRef>> = HashMap::new();
        plugins.insert(
            "api_call".into(),
            vec![
                make_plugin_arc(StopPlugin {
                    meta: PluginMeta::new("stopper", (1, 0, 0), "api_call", 1),
                }),
                make_plugin_arc(NoopPlugin::new("should_be_skipped", "api_call", 2)),
            ],
        );

        let stats = run_pipeline(&mut ctx, &stages, &plugins);
        assert_eq!(stats.plugins_executed, 1);
        assert_eq!(stats.plugins_skipped, 1);
    }

    #[test]
    fn test_abort_after_error_stage_does_not_loop() {
        // error_stage 之后的阶段不应再跳转
        let stages = make_stages(vec!["preprocess", "postprocess", "cleanup"]);
        let error_stage = "postprocess".to_string();

        let mut plugins: HashMap<String, Vec<PluginRef>> = HashMap::new();
        // preprocess 中 abort
        struct AbortInPreprocess { meta: PluginMeta }
        impl Plugin for AbortInPreprocess {
            fn metadata(&self) -> PluginMeta { self.meta.clone() }
            fn execute(&self, ctx: &mut Context) -> Result<PluginResult> {
                ctx.abort = true;
                Ok(PluginResult::r#continue())
            }
        }
        plugins.insert("preprocess".into(), vec![
            make_plugin_arc(AbortInPreprocess {
                meta: PluginMeta::new("aborter", (1,0,0), "preprocess", 1),
            }),
        ]);

        // postprocess (error_stage)
        plugins.insert("postprocess".into(), vec![
            make_plugin_arc(AppendPlugin {
                meta: PluginMeta::new("cleanup", (1,0,0), "postprocess", 1),
                text: "cleaned".into(),
            }),
        ]);

        // cleanup (after error_stage) — 不应该被执行
        plugins.insert("cleanup".into(), vec![
            make_plugin_arc(AppendPlugin {
                meta: PluginMeta::new("after_cleanup", (1,0,0), "cleanup", 1),
                text: "SHOULD_NOT_RUN".into(),
            }),
        ]);

        let mut stages_clone = stages.clone();
        stages_clone.error_stage = error_stage.clone();
        let mut ctx = Context::new("test");
        let stats = run_pipeline(&mut ctx, &stages_clone, &plugins);

        assert!(stats.was_aborted);
        assert!(ctx.messages.iter().any(|m| m.content == "cleaned"));
        assert!(!ctx.messages.iter().any(|m| m.content == "SHOULD_NOT_RUN"));
    }

    #[test]
    fn test_error_stage_before_abort_point_is_skipped() {
        // error_stage 在 abort 点之前 → 已执行过，直接结束
        let stages = make_stages(vec!["preprocess", "api_call", "postprocess"]);
        let mut stages_with_early_error = stages.clone();
        stages_with_early_error.error_stage = "preprocess".to_string();

        struct AbortInApiCall { meta: PluginMeta }
        impl Plugin for AbortInApiCall {
            fn metadata(&self) -> PluginMeta { self.meta.clone() }
            fn execute(&self, ctx: &mut Context) -> Result<PluginResult> {
                ctx.abort = true;
                Ok(PluginResult::r#continue())
            }
        }

        let mut plugins: HashMap<String, Vec<PluginRef>> = HashMap::new();
        plugins.insert("api_call".into(), vec![
            make_plugin_arc(AbortInApiCall {
                meta: PluginMeta::new("aborter", (1,0,0), "api_call", 1),
            }),
        ]);
        plugins.insert("postprocess".into(), vec![
            make_plugin_arc(AppendPlugin {
                meta: PluginMeta::new("should_not_run", (1,0,0), "postprocess", 1),
                text: "NO".into(),
            }),
        ]);

        let mut ctx = Context::new("test");
        let stats = run_pipeline(&mut ctx, &stages_with_early_error, &plugins);

        assert!(stats.was_aborted);
        // postprocess 不应该执行（error_stage=preprocess 在 abort 点之前）
        assert!(!ctx.messages.iter().any(|m| m.content == "NO"));
    }

    #[test]
    fn test_plugin_failure_does_not_stop_pipeline() {
        struct FailingPlugin { meta: PluginMeta }
        impl Plugin for FailingPlugin {
            fn metadata(&self) -> PluginMeta { self.meta.clone() }
            fn execute(&self, _ctx: &mut Context) -> Result<PluginResult> {
                Err(CoreError::plugin("failing", "intentional failure"))
            }
        }

        let stages = make_stages(vec!["api_call"]);
        let mut plugins: HashMap<String, Vec<PluginRef>> = HashMap::new();
        plugins.insert("api_call".into(), vec![
            make_plugin_arc(FailingPlugin {
                meta: PluginMeta::new("failer", (1,0,0), "api_call", 1),
            }),
            make_plugin_arc(NoopPlugin::new("runner", "api_call", 2)),
        ]);

        let mut ctx = Context::new("test");
        let stats = run_pipeline(&mut ctx, &stages, &plugins);

        assert_eq!(stats.plugins_failed, 1);
        assert_eq!(stats.plugins_executed, 1); // NoopPlugin 仍被执行
        assert!(ctx.get_custom_value("error").is_some());
    }

    #[test]
    fn test_multiple_plugins_same_priority() {
        // 同优先级按 HashMap 迭代顺序（不保证），但都执行
        let stages = make_stages(vec!["api_call"]);
        let mut plugins: HashMap<String, Vec<PluginRef>> = HashMap::new();
        plugins.insert("api_call".into(), vec![
            make_plugin_arc(AppendPlugin {
                meta: PluginMeta::new("p1", (1,0,0), "api_call", 5),
                text: "first".into(),
            }),
            make_plugin_arc(AppendPlugin {
                meta: PluginMeta::new("p2", (1,0,0), "api_call", 5),
                text: "second".into(),
            }),
        ]);

        let mut ctx = Context::new("test");
        let stats = run_pipeline(&mut ctx, &stages, &plugins);

        assert_eq!(stats.plugins_executed, 2);
        assert_eq!(ctx.message_count(), 2);
    }
}
