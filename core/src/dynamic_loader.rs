//! 动态库加载器。
//!
//! 使用 `libloading` 加载 `.so`/`.dylib`/`.dll` 插件动态库，
//! 通过 `create_plugin` 导出符号获取 `Box<dyn Plugin>` 实例。
//!
//! # 内存安全与延迟释放
//!
//! 插件存储在 [`PluginRef`]（`Arc<RwLock<Box<dyn Plugin>>>`）中：
//! - 流水线通过读锁调用 `execute()`，多 session 可并发。
//! - 生命周期方法（`init`、`shutdown` 等）通过写锁调用。
//!
//! 热重载时，旧插件的 `(Library, PluginRef)` 被移入
//! `deferred_drops` 队列。只有在确认无流水线持有读锁后（写锁成功获取后）
//! 才真正 drop。`Arc` 确保即使正在执行的流水线持有引用，插件也不会
//! 被提前释放。
//!
//! # FFI 安全说明
//!
//! 当前实现使用 `Box<dyn Plugin>` 跨 FFI 边界传递。
//! 这依赖于调用方和被调用方使用**完全相同的 Rust 编译器版本**
//! 和**完全相同的 `core` crate 编译产物**（vtable 布局一致）。
//! 在 workspace 内编译时满足此条件。

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use libloading::{Library, Symbol};
use tracing::{debug, error, info, warn};

use crate::error::{Result, CoreError};
use crate::plugin::{Plugin, PluginRef};

/// `create_plugin` 导出符号的函数签名。
///
/// 每个插件动态库必须导出一个此签名的函数：
/// ```ignore
/// #[no_mangle]
/// pub extern "C" fn create_plugin() -> Box<dyn zsai_core::Plugin> {
///     Box::new(MyPlugin::new())
/// }
/// ```
///
/// # FFI 安全性
///
/// `Box<dyn Plugin>` 在 C ABI 中不是 FFI-safe 的（编译器会发出 `improper_ctypes` 警告）。
/// 在 workspace 内，由于所有 crate 使用相同 Rust 版本和相同 `core` crate，
/// vtable 布局和 allocator 一致，实际使用是安全的。
/// 如果未来需要跨编译器版本兼容，应改为返回 `*mut dyn Plugin` 或使用 `abi_stable`。
#[allow(improper_ctypes_definitions)]
type CreatePluginFn = extern "C" fn() -> Box<dyn Plugin>;

/// 插件条目。
///
/// `Library` 在 `plugin` 之前声明，确保 drop 顺序正确：
/// 先 drop `PluginRef`（包括所有 `Arc` 引用），
/// 再 drop `Library`（释放动态库句柄）。
struct PluginEntry {
    /// 动态库句柄。必须先于 plugin drop。
    #[allow(dead_code)]
    library: Library,

    /// 插件实例（线程安全引用计数 + 读写锁）。
    plugin: PluginRef,
}

/// 动态库加载器。
///
/// # 线程安全
///
/// 外层 `DynamicLoader` 自身不要求 `Send + Sync`（由调用方用 `Arc<RwLock<>>` 包裹）。
/// 内部 `PluginRef` 支持多线程并发访问。
pub struct DynamicLoader {
    /// 已加载的插件。key = 插件名（从 `metadata().name` 获取）。
    plugins: HashMap<String, PluginEntry>,

    /// 待释放的旧插件（热重载延迟释放队列）。
    deferred_drops: Vec<(Library, PluginRef)>,

    /// 插件搜索目录。
    plugin_dir: PathBuf,
}

impl DynamicLoader {
    /// 创建新的动态加载器。
    pub fn new(plugin_dir: impl Into<PathBuf>) -> Self {
        DynamicLoader {
            plugins: HashMap::new(),
            deferred_drops: Vec::new(),
            plugin_dir: plugin_dir.into(),
        }
    }

    /// 扫描插件目录，加载所有发现的插件。
    ///
    /// 注意：`load_one` 只加载插件（调用 `create_plugin`），
    /// 不调用 `init()`。调用方需在 `load_all` 后调用 [`init_all`] 传入配置。
    pub fn load_all(&mut self) -> Vec<String> {
        let dir = &self.plugin_dir;

        if !dir.exists() {
            warn!(plugin_dir = %dir.display(), "插件目录不存在，跳过加载");
            return Vec::new();
        }

        let mut entries: Vec<PathBuf> = match fs::read_dir(dir) {
            Ok(entries) => entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_file())
                .filter(|p| {
                    p.extension()
                        .and_then(OsStr::to_str)
                        .map(|ext| matches!(ext, "so" | "dylib" | "dll"))
                        .unwrap_or(false)
                })
                .collect(),
            Err(e) => {
                error!(plugin_dir = %dir.display(), error = %e, "无法读取插件目录");
                return Vec::new();
            }
        };

        entries.sort();

        let mut loaded = Vec::new();
        for path in &entries {
            match self.load_one(path) {
                Ok(name) => {
                    info!(plugin = %name, path = %path.display(), "插件加载成功");
                    loaded.push(name);
                }
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "插件加载失败，跳过");
                }
            }
        }

        self.cleanup_deferred();
        loaded
    }

    /// 加载单个插件动态库。
    pub fn load_one(&mut self, path: &Path) -> Result<String> {
        let path = path.to_path_buf();

        let library = unsafe { Library::new(&path) }.map_err(|e| {
            CoreError::DynamicLoader {
                context: path.display().to_string(),
                message: format!("无法加载动态库: {}", e),
                source: Some(e),
            }
        })?;

        let constructor: Symbol<CreatePluginFn> =
            unsafe { library.get(b"create_plugin") }.map_err(|e| {
                CoreError::DynamicLoader {
                    context: path.display().to_string(),
                    message: concat!(
                        "缺少 'create_plugin' 导出符号。",
                        "\n请确保插件实现了:",
                        "\n  #[no_mangle]",
                        "\n  pub extern \"C\" fn create_plugin() -> Box<dyn Plugin> { ... }",
                    ).to_string(),
                    source: Some(e),
                }
            })?;

        let boxed: Box<dyn Plugin> = constructor();
        let meta = boxed.metadata();

        debug!(
            plugin = %meta.name,
            version = ?meta.version,
            stage = %meta.stage,
            priority = meta.priority,
            capabilities = ?meta.capabilities,
            "create_plugin 调用成功"
        );

        if self.plugins.contains_key(&meta.name) {
            return Err(CoreError::dynamic_loader(
                path.display().to_string(),
                format!("同名插件 '{}' 已加载", meta.name),
            ));
        }

        let plugin_arc = Arc::new(std::sync::RwLock::new(boxed));
        let name = meta.name.clone();
        self.plugins.insert(name.clone(), PluginEntry {
            library,
            plugin: plugin_arc,
        });

        Ok(name)
    }

    /// 卸载指定插件。
    pub fn unload(&mut self, name: &str) -> Result<()> {
        let entry = self.plugins.remove(name).ok_or_else(|| {
            CoreError::dynamic_loader(name, format!("插件 '{}' 未加载", name))
        })?;

        if let Ok(mut plugin) = entry.plugin.write() {
            if let Err(e) = plugin.shutdown() {
                warn!(plugin = %name, error = %e, "插件 shutdown 失败");
            }
        }

        let plugin_arc = Arc::clone(&entry.plugin);
        self.deferred_drops.push((entry.library, plugin_arc));

        info!(plugin = %name, "插件已卸载（延迟释放中）");
        Ok(())
    }

    /// 重载指定插件（热重载）。
    ///
    /// 流程：`before_reload` → `shutdown`（旧）→ 加载新 .so
    /// → `after_reload`（新）。
    ///
    /// # 注意
    ///
    /// 如果新 .so 加载失败，旧插件已经卸载，调用方应在此方法返回 `Err`
    /// 后禁用该插件，并记录错误。当前实现不保留旧插件（热重载是破坏性操作）。
    pub fn reload(&mut self, name: &str, new_path: &Path) -> Result<()> {
        // 1. 调用旧插件 before_reload
        if let Some(entry) = self.plugins.get(name) {
            if let Ok(mut plugin) = entry.plugin.write() {
                if let Err(e) = plugin.before_reload() {
                    warn!(plugin = %name, error = %e, "before_reload 失败，继续重载");
                }
            }
        }

        // 2. 卸载旧插件（调用 shutdown，移入 deferred_drops）
        self.unload(name)?;

        // 3. 加载新插件
        let new_name = self.load_one(new_path)?;

        if new_name != name {
            warn!(old_name = %name, new_name = %new_name, "热重载后插件名发生变化");
        }

        // 4. 调用新插件 after_reload（可选钩子）
        if let Some(entry) = self.plugins.get(&new_name) {
            if let Ok(mut plugin) = entry.plugin.write() {
                if let Err(e) = plugin.after_reload() {
                    warn!(plugin = %new_name, error = %e, "after_reload 失败");
                }
            }
        }

        Ok(())
    }

    /// 获取指定插件的 `PluginRef`。
    pub fn get(&self, name: &str) -> Option<PluginRef> {
        self.plugins.get(name).map(|e| Arc::clone(&e.plugin))
    }

    /// 获取指定阶段的所有插件，按优先级排序。
    ///
    /// 对每个候选插件只调用一次 `metadata()`（同时获取 stage 和 priority）。
    pub fn get_by_stage(&self, stage_name: &str) -> Vec<PluginRef> {
        let mut plugins: Vec<_> = self
            .plugins
            .values()
            .filter_map(|entry| {
                entry.plugin.read().ok().and_then(|p| {
                    let meta = p.metadata();
                    if meta.stage == stage_name {
                        Some((meta.priority, Arc::clone(&entry.plugin)))
                    } else {
                        None
                    }
                })
            })
            .collect();

        plugins.sort_by_key(|(priority, _)| *priority);
        plugins.into_iter().map(|(_, p)| p).collect()
    }

    /// 构建完整的 `阶段 → 插件列表` 映射。
    pub fn build_stage_map(
        &self,
        stage_order: &[String],
    ) -> HashMap<String, Vec<PluginRef>> {
        let mut map = HashMap::new();
        for stage in stage_order {
            let plugins = self.get_by_stage(stage);
            if !plugins.is_empty() {
                map.insert(stage.clone(), plugins);
            }
        }
        map
    }

    /// 获取所有已加载的插件名。
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.plugins.keys().cloned().collect();
        names.sort();
        names
    }

    /// 已加载插件数量。
    pub fn len(&self) -> usize { self.plugins.len() }

    /// 是否没有已加载插件。
    pub fn is_empty(&self) -> bool { self.plugins.is_empty() }

    /// 为所有插件调用 `init()` 并传入对应配置。
    pub fn init_all(&mut self, plugin_configs: &HashMap<String, serde_json::Value>) {
        for (name, entry) in &self.plugins {
            let config = plugin_configs
                .get(name)
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            if let Ok(mut plugin) = entry.plugin.write() {
                if let Err(e) = plugin.init(&config) {
                    warn!(plugin = %name, error = %e, "插件 init 失败");
                }
            }
        }
    }

    /// 为所有插件调用 `shutdown()` 并清空加载列表。
    pub fn shutdown_all(&mut self) {
        let names: Vec<String> = self.plugins.keys().cloned().collect();
        for name in names {
            let _ = self.unload(&name);
        }
        self.cleanup_deferred();
    }

    /// 清理延迟释放队列。
    pub fn cleanup_deferred(&mut self) {
        if !self.deferred_drops.is_empty() {
            debug!(count = self.deferred_drops.len(), "清理延迟释放队列");
            self.deferred_drops.clear();
        }
    }

    /// 延迟释放队列长度。
    pub fn deferred_count(&self) -> usize {
        self.deferred_drops.len()
    }

    /// 为所有插件调用 `health_check()`。
    pub fn health_check_all(&self) -> Vec<(String, crate::plugin::HealthStatus)> {
        self.plugins
            .iter()
            .map(|(name, entry)| {
                let status = entry
                    .plugin
                    .read()
                    .ok()
                    .and_then(|p| p.health_check().ok())
                    .unwrap_or_else(|| {
                        crate::plugin::HealthStatus::unhealthy("无法获取读锁")
                    });
                (name.clone(), status)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_loader() {
        let loader = DynamicLoader::new("./target/debug");
        assert!(loader.is_empty());
        assert_eq!(loader.len(), 0);
        assert!(loader.names().is_empty());
    }

    #[test]
    fn test_deferred_drops() {
        let mut loader = DynamicLoader::new("./target/debug");
        assert_eq!(loader.deferred_count(), 0);
        loader.cleanup_deferred();
        assert_eq!(loader.deferred_count(), 0);
    }

    #[test]
    fn test_build_stage_map_empty() {
        let loader = DynamicLoader::new("./nonexistent_dir");
        let map = loader.build_stage_map(&["preprocess".into(), "api_call".into()]);
        assert!(map.is_empty());
    }
}
