//! 热加载管理器。
//!
//! 使用 `notify` crate 监听插件目录，当检测到动态库文件变化时自动重载对应插件。
//!
//! # 线程模型
//!
//! 热加载管理器在独立线程中运行文件监听循环。
//! 与主线程通过 `Arc<RwLock<DynamicLoader>>` 共享加载器。
//!
//! - 文件变化 → 尝试获取写锁 → 执行重载 → 释放写锁。
//! - 写锁获取失败（有流水线正在执行）→ 等待，不阻塞。
//! - 重载失败 → 记录错误日志，该插件当前不可用（旧版本在 `unload` 中已卸下）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{debug, error, info, warn};

use crate::config::HotReloadConfig;
use crate::dynamic_loader::DynamicLoader;
use crate::error::{Result, CoreError};

/// 热加载管理器。
///
/// 创建后调用 `start()` 开始监听，调用 `stop()` 停止。
/// Drop 时自动停止。
pub struct HotReloadManager {
    /// 共享的动态加载器。
    loader: Arc<RwLock<DynamicLoader>>,

    /// 文件监听器（None 表示已停止）。
    watcher: Option<RecommendedWatcher>,

    /// 插件目录路径。
    plugin_dir: PathBuf,

    /// 防抖延迟。
    debounce_duration: Duration,

    /// 运行标志。
    running: Arc<AtomicBool>,
}

impl HotReloadManager {
    /// 创建新的热加载管理器。
    ///
    /// # 参数
    ///
    /// - `loader`：共享的动态加载器（已在外部加载了初始插件集）。
    /// - `config`：热加载配置段。
    pub fn new(
        loader: Arc<RwLock<DynamicLoader>>,
        config: &HotReloadConfig,
    ) -> Self {
        HotReloadManager {
            loader,
            watcher: None,
            plugin_dir: PathBuf::from(&config.plugin_dir),
            debounce_duration: Duration::from_millis(config.delay_ms),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 启动热加载监听。
    ///
    /// 在独立线程中运行文件监听循环。
    /// 如果 `HotReloadConfig::enabled` 为 `false`，此方法无操作。
    ///
    /// # 错误
    ///
    /// 仅在无法创建文件监听器时返回错误。
    /// 如果插件目录不存在，会自动创建。
    pub fn start(&mut self) -> Result<()> {
        let plugin_dir = self.plugin_dir.clone();

        // 确保插件目录存在
        if !plugin_dir.exists() {
            std::fs::create_dir_all(&plugin_dir).map_err(|e| {
                CoreError::hot_reload(
                    plugin_dir.display().to_string(),
                    format!("无法创建插件目录: {}", e),
                )
            })?;
            info!(plugin_dir = %plugin_dir.display(), "创建插件目录");
        }

        let loader = Arc::clone(&self.loader);
        let debounce = self.debounce_duration;
        let running = Arc::clone(&self.running);

        running.store(true, Ordering::SeqCst);

        // 创建文件监听器
        let mut watcher = RecommendedWatcher::new(
            move |event: notify::Result<Event>| {
                match event {
                    Ok(event) => {
                        handle_event(
                            &loader,
                            &event,
                            debounce,
                            &running,
                        );
                    }
                    Err(e) => {
                        error!(error = %e, "文件监听错误");
                    }
                }
            },
            Config::default(),
        )
        .map_err(|e| {
            CoreError::hot_reload(
                plugin_dir.display().to_string(),
                format!("无法创建文件监听器: {}", e),
            )
        })?;

        // 开始监听（非递归，只监听顶层目录）
        watcher
            .watch(&plugin_dir, RecursiveMode::NonRecursive)
            .map_err(|e| {
                CoreError::hot_reload(
                    plugin_dir.display().to_string(),
                    format!("无法开始监听目录: {}", e),
                )
            })?;

        self.watcher = Some(watcher);

        info!(
            plugin_dir = %plugin_dir.display(),
            debounce_ms = debounce.as_millis(),
            "热加载监听已启动"
        );

        Ok(())
    }

    /// 停止热加载监听。
    ///
    /// 关闭文件监听器，等待运行标志清零。
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        self.watcher = None; // Drop watcher → 停止监听
        info!("热加载监听已停止");
    }

    /// 检查是否正在运行。
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Drop for HotReloadManager {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 处理文件系统事件。
///
/// 将实际重载操作派发到独立线程，避免阻塞 notify 的 watcher 线程。
/// `notify` 已确保事件来自被监听的目录，因此无需额外路径校验。
fn handle_event(
    loader: &Arc<RwLock<DynamicLoader>>,
    event: &Event,
    debounce: Duration,
    running: &Arc<AtomicBool>,
) {
    // 只关心 Create 和 Modify 事件
    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) => {}
        _ => return,
    }

    for path in &event.paths {
        // 过滤：只处理动态库文件
        if !is_plugin_file(path) {
            continue;
        }

        // 过滤：忽略临时文件
        if is_temp_file(path) {
            continue;
        }

        // 提取插件名
        let plugin_name = extract_plugin_name(path);
        let path_buf = path.clone();
        let loader = Arc::clone(loader);
        let running = Arc::clone(running);

        debug!(
            path = %path.display(),
            plugin = %plugin_name,
            "检测到插件文件变化"
        );

        // 在独立线程中执行重载，避免阻塞 watcher 线程
        thread::spawn(move || {
            // 防抖：等待文件写入完成
            if running.load(Ordering::SeqCst) {
                thread::sleep(debounce);
            }

            // 再次检查文件是否存在（可能已被删除）
            if !path_buf.exists() {
                warn!(
                    path = %path_buf.display(),
                    "文件在防抖期间消失，跳过"
                );
                return;
            }

            // 获取写锁并执行重载
            match loader.write() {
                Ok(mut dyn_loader) => {
                    match dyn_loader.reload(&plugin_name, &path_buf) {
                        Ok(()) => {
                            info!(
                                plugin = %plugin_name,
                                path = %path_buf.display(),
                                "热重载成功"
                            );
                        }
                        Err(e) => {
                            error!(
                                plugin = %plugin_name,
                                path = %path_buf.display(),
                                error = %e,
                                "热重载失败，该插件当前不可用（旧版本已卸载）"
                            );
                        }
                    }

                    // 重载后清理延迟释放队列
                    dyn_loader.cleanup_deferred();
                }
                Err(e) => {
                    error!(
                        error = %e,
                        "无法获取 DynamicLoader 写锁（RwLock 中毒）"
                    );
                }
            }
        });
    }
}

/// 检查文件是否为动态库（按扩展名）。
fn is_plugin_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| matches!(ext, "so" | "dylib" | "dll"))
        .unwrap_or(false)
}

/// 检查文件是否为临时文件。
fn is_temp_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| {
            name.ends_with('~')
                || name.ends_with(".tmp")
                || name.starts_with('#')
                || name.starts_with(".#")
        })
        .unwrap_or(false)
}

/// 从动态库文件路径提取插件名。
///
/// 例如：
/// - `./target/debug/libcharacter_card.so` → `"character_card"`
/// - `./target/debug/api_client.dll` → `"api_client"`
fn extract_plugin_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    // 去掉 `lib` 前缀（Linux/macOS 惯例）
    if let Some(name) = stem.strip_prefix("lib") {
        name.to_string()
    } else {
        stem.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_plugin_file() {
        assert!(is_plugin_file(Path::new("libtest.so")));
        assert!(is_plugin_file(Path::new("test.dylib")));
        assert!(is_plugin_file(Path::new("test.dll")));
        assert!(!is_plugin_file(Path::new("test.txt")));
        assert!(!is_plugin_file(Path::new("test")));
        assert!(!is_plugin_file(Path::new("test.rmeta")));
        assert!(!is_plugin_file(Path::new("test.d")));
    }

    #[test]
    fn test_is_temp_file() {
        assert!(is_temp_file(Path::new("test~")));
        assert!(is_temp_file(Path::new("test.tmp")));
        assert!(is_temp_file(Path::new("#test#")));
        assert!(is_temp_file(Path::new(".#test")));
        assert!(!is_temp_file(Path::new("libtest.so")));
        assert!(!is_temp_file(Path::new("test.dylib")));
    }

    #[test]
    fn test_extract_plugin_name_linux() {
        assert_eq!(
            extract_plugin_name(Path::new("./target/debug/libcharacter_card.so")),
            "character_card"
        );
        assert_eq!(
            extract_plugin_name(Path::new("libapi_client.so")),
            "api_client"
        );
    }

    #[test]
    fn test_extract_plugin_name_windows() {
        assert_eq!(
            extract_plugin_name(Path::new("./target/debug/character_card.dll")),
            "character_card"
        );
    }

    #[test]
    fn test_extract_plugin_name_macos() {
        assert_eq!(
            extract_plugin_name(Path::new("libprompt_builder.dylib")),
            "prompt_builder"
        );
    }
}
