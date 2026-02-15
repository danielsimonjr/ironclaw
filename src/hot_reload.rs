//! Configuration hot-reload support.
//!
//! Watches configuration files for changes and triggers reload callbacks.
//! This is a standalone module used by the agent to monitor config changes.

use std::path::PathBuf;
use std::sync::Arc;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{RwLock, broadcast};

/// Events emitted when configuration changes.
#[derive(Debug, Clone)]
pub enum ReloadEvent {
    /// A configuration file was modified.
    FileChanged { path: PathBuf },
    /// Configuration was reloaded from the database.
    DatabaseChanged,
    /// Environment variables changed (manual trigger).
    EnvChanged,
}

/// Configuration watcher that monitors files for changes.
pub struct ConfigWatcher {
    /// The paths being watched.
    watched_paths: Vec<PathBuf>,
    /// Broadcast sender for reload notifications.
    tx: broadcast::Sender<ReloadEvent>,
    /// The underlying file watcher (kept alive).
    _watcher: Option<RecommendedWatcher>,
}

impl ConfigWatcher {
    /// Create a new config watcher.
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(16);
        Self {
            watched_paths: Vec::new(),
            tx,
            _watcher: None,
        }
    }

    /// Start watching a file or directory for changes.
    pub fn watch(&mut self, path: PathBuf) -> Result<(), notify::Error> {
        let tx = self.tx.clone();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                match event.kind {
                    EventKind::Modify(_) | EventKind::Create(_) => {
                        for path in &event.paths {
                            let _ = tx.send(ReloadEvent::FileChanged { path: path.clone() });
                        }
                    }
                    _ => {}
                }
            }
        })?;

        watcher.watch(&path, RecursiveMode::NonRecursive)?;
        self.watched_paths.push(path);
        self._watcher = Some(watcher);

        Ok(())
    }

    /// Subscribe to reload events.
    pub fn subscribe(&self) -> broadcast::Receiver<ReloadEvent> {
        self.tx.subscribe()
    }

    /// Manually trigger a reload.
    pub fn trigger_reload(&self, event: ReloadEvent) {
        let _ = self.tx.send(event);
    }

    /// Get the list of watched paths.
    pub fn watched_paths(&self) -> &[PathBuf] {
        &self.watched_paths
    }
}

impl Default for ConfigWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Hot-reloadable configuration container.
pub struct HotReloadConfig<T: Clone + Send + Sync> {
    config: Arc<RwLock<T>>,
    generation: Arc<std::sync::atomic::AtomicU64>,
}

impl<T: Clone + Send + Sync> HotReloadConfig<T> {
    /// Create a new hot-reloadable config.
    pub fn new(initial: T) -> Self {
        Self {
            config: Arc::new(RwLock::new(initial)),
            generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Get the current configuration.
    pub async fn get(&self) -> T {
        self.config.read().await.clone()
    }

    /// Update the configuration.
    pub async fn update(&self, new_config: T) {
        *self.config.write().await = new_config;
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// Get the configuration generation (incremented on each update).
    pub fn generation(&self) -> u64 {
        self.generation.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl<T: Clone + Send + Sync> Clone for HotReloadConfig<T> {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            generation: Arc::clone(&self.generation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hot_reload_config() {
        let config = HotReloadConfig::new("initial".to_string());

        assert_eq!(config.get().await, "initial");
        assert_eq!(config.generation(), 0);

        config.update("updated".to_string()).await;
        assert_eq!(config.get().await, "updated");
        assert_eq!(config.generation(), 1);
    }

    #[test]
    fn test_config_watcher_creation() {
        let watcher = ConfigWatcher::new();
        assert!(watcher.watched_paths().is_empty());
    }

    #[test]
    fn test_manual_trigger() {
        let watcher = ConfigWatcher::new();
        let mut rx = watcher.subscribe();

        watcher.trigger_reload(ReloadEvent::EnvChanged);

        let result = rx.try_recv();
        assert!(result.is_ok());
    }
}
