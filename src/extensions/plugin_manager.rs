//! Plugin lifecycle manager.
//!
//! Manages registration, activation, and lifecycle of plugins across
//! all plugin types (auth, memory, provider, hook, channel, tool, http_route).

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::RwLock;

use crate::extensions::plugins::{Plugin, PluginRoute, PluginStatus, PluginType};

/// Error type for plugin operations.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("Plugin not found: {0}")]
    NotFound(String),
    #[error("Plugin already registered: {0}")]
    AlreadyRegistered(String),
    #[error("Plugin activation failed: {reason}")]
    ActivationFailed { name: String, reason: String },
    #[error("Plugin type mismatch: expected {expected}, got {got}")]
    TypeMismatch { expected: String, got: String },
    #[error("Plugin validation failed: {0}")]
    ValidationFailed(String),
    #[error("Plugin dependency not met: {plugin} requires {dependency}")]
    DependencyNotMet { plugin: String, dependency: String },
}

/// Manages all registered plugins.
pub struct PluginManager {
    plugins: Arc<RwLock<HashMap<String, PluginEntry>>>,
    max_plugins: usize,
}

/// Internal plugin entry with status tracking.
#[derive(Debug, Clone)]
struct PluginEntry {
    plugin: Plugin,
    status: PluginStatus,
    activated_at: Option<chrono::DateTime<chrono::Utc>>,
    error: Option<String>,
    load_order: usize,
}

/// Summary of registered plugins.
#[derive(Debug, Clone, Serialize)]
pub struct PluginSummary {
    pub total: usize,
    pub active: usize,
    pub disabled: usize,
    pub error: usize,
    pub by_type: HashMap<String, usize>,
}

/// Snapshot of a plugin's state.
#[derive(Debug, Clone, Serialize)]
pub struct PluginSnapshot {
    pub name: String,
    pub plugin_type: PluginType,
    pub description: String,
    pub version: String,
    pub status: PluginStatus,
    pub enabled: bool,
    pub activated_at: Option<String>,
    pub error: Option<String>,
    pub routes: Vec<PluginRoute>,
}

impl PluginManager {
    /// Create a new plugin manager with default limits.
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            max_plugins: 100,
        }
    }

    /// Set the maximum number of plugins allowed.
    pub fn with_max_plugins(mut self, max: usize) -> Self {
        self.max_plugins = max;
        self
    }

    /// Register a new plugin.
    pub async fn register(&self, plugin: Plugin) -> Result<(), PluginError> {
        let mut plugins = self.plugins.write().await;
        if plugins.contains_key(&plugin.name) {
            return Err(PluginError::AlreadyRegistered(plugin.name.clone()));
        }
        if plugins.len() >= self.max_plugins {
            return Err(PluginError::ValidationFailed(format!(
                "Maximum plugin limit ({}) reached",
                self.max_plugins
            )));
        }
        let load_order = plugins.len();
        plugins.insert(
            plugin.name.clone(),
            PluginEntry {
                status: if plugin.enabled {
                    PluginStatus::Active
                } else {
                    PluginStatus::Disabled
                },
                plugin,
                activated_at: None,
                error: None,
                load_order,
            },
        );
        Ok(())
    }

    /// Unregister a plugin by name, returning it if found.
    pub async fn unregister(&self, name: &str) -> Result<Plugin, PluginError> {
        self.plugins
            .write()
            .await
            .remove(name)
            .map(|e| e.plugin)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))
    }

    /// Activate a plugin by name.
    pub async fn activate(&self, name: &str) -> Result<(), PluginError> {
        let mut plugins = self.plugins.write().await;
        let entry = plugins
            .get_mut(name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;
        entry.status = PluginStatus::Active;
        entry.plugin.enabled = true;
        entry.activated_at = Some(chrono::Utc::now());
        entry.error = None;
        Ok(())
    }

    /// Deactivate a plugin by name.
    pub async fn deactivate(&self, name: &str) -> Result<(), PluginError> {
        let mut plugins = self.plugins.write().await;
        let entry = plugins
            .get_mut(name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;
        entry.status = PluginStatus::Disabled;
        entry.plugin.enabled = false;
        Ok(())
    }

    /// Set error state on a plugin.
    pub async fn set_error(&self, name: &str, error: String) -> Result<(), PluginError> {
        let mut plugins = self.plugins.write().await;
        let entry = plugins
            .get_mut(name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;
        entry.status = PluginStatus::Error;
        entry.error = Some(error);
        Ok(())
    }

    /// Get a snapshot of a plugin by name.
    pub async fn get(&self, name: &str) -> Option<PluginSnapshot> {
        self.plugins.read().await.get(name).map(|e| e.snapshot())
    }

    /// List all plugins, ordered by registration order.
    pub async fn list(&self) -> Vec<PluginSnapshot> {
        let plugins = self.plugins.read().await;
        let mut entries: Vec<_> = plugins.values().collect();
        entries.sort_by_key(|e| e.load_order);
        entries.iter().map(|e| e.snapshot()).collect()
    }

    /// List plugins filtered by type.
    pub async fn list_by_type(&self, plugin_type: PluginType) -> Vec<PluginSnapshot> {
        self.plugins
            .read()
            .await
            .values()
            .filter(|e| e.plugin.plugin_type == plugin_type)
            .map(|e| e.snapshot())
            .collect()
    }

    /// Get summary statistics of all registered plugins.
    pub async fn summary(&self) -> PluginSummary {
        let plugins = self.plugins.read().await;
        let mut by_type: HashMap<String, usize> = HashMap::new();
        let mut active = 0;
        let mut disabled = 0;
        let mut error = 0;

        for entry in plugins.values() {
            *by_type
                .entry(entry.plugin.plugin_type.to_string())
                .or_default() += 1;
            match entry.status {
                PluginStatus::Active => active += 1,
                PluginStatus::Disabled => disabled += 1,
                PluginStatus::Error => error += 1,
                PluginStatus::Loading => {}
            }
        }

        PluginSummary {
            total: plugins.len(),
            active,
            disabled,
            error,
            by_type,
        }
    }

    /// Update the configuration of a plugin.
    pub async fn update_config(
        &self,
        name: &str,
        config: HashMap<String, serde_json::Value>,
    ) -> Result<(), PluginError> {
        let mut plugins = self.plugins.write().await;
        let entry = plugins
            .get_mut(name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;
        entry.plugin.config = config;
        Ok(())
    }
}

impl PluginEntry {
    fn snapshot(&self) -> PluginSnapshot {
        PluginSnapshot {
            name: self.plugin.name.clone(),
            plugin_type: self.plugin.plugin_type,
            description: self.plugin.description.clone(),
            version: self.plugin.version.clone(),
            status: self.status,
            enabled: self.plugin.enabled,
            activated_at: self.activated_at.map(|t| t.to_rfc3339()),
            error: self.error.clone(),
            routes: self.plugin.routes.clone(),
        }
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::plugins::PluginRoute;

    fn make_plugin(name: &str, plugin_type: PluginType, enabled: bool) -> Plugin {
        Plugin {
            name: name.to_string(),
            plugin_type,
            description: format!("{name} plugin"),
            version: "1.0.0".to_string(),
            enabled,
            config: HashMap::new(),
            routes: vec![],
        }
    }

    fn make_plugin_with_routes(name: &str, routes: Vec<PluginRoute>) -> Plugin {
        Plugin {
            name: name.to_string(),
            plugin_type: PluginType::HttpRoute,
            description: format!("{name} plugin"),
            version: "1.0.0".to_string(),
            enabled: true,
            config: HashMap::new(),
            routes,
        }
    }

    fn auth_plugin(name: &str) -> Plugin {
        make_plugin(name, PluginType::Auth, true)
    }

    fn memory_plugin(name: &str) -> Plugin {
        make_plugin(name, PluginType::Memory, true)
    }

    fn provider_plugin(name: &str) -> Plugin {
        make_plugin(name, PluginType::Provider, true)
    }

    // --- Registration tests ---

    #[tokio::test]
    async fn test_register_and_list() {
        let mgr = PluginManager::new();
        mgr.register(auth_plugin("auth-basic")).await.unwrap();
        mgr.register(memory_plugin("mem-redis")).await.unwrap();

        let list = mgr.list().await;
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "auth-basic");
        assert_eq!(list[1].name, "mem-redis");
    }

    #[tokio::test]
    async fn test_register_duplicate_rejected() {
        let mgr = PluginManager::new();
        mgr.register(auth_plugin("auth-basic")).await.unwrap();

        let err = mgr.register(auth_plugin("auth-basic")).await.unwrap_err();
        assert!(matches!(err, PluginError::AlreadyRegistered(ref n) if n == "auth-basic"));
        assert!(err.to_string().contains("auth-basic"));
    }

    #[tokio::test]
    async fn test_register_enabled_plugin_is_active() {
        let mgr = PluginManager::new();
        mgr.register(make_plugin("p1", PluginType::Auth, true))
            .await
            .unwrap();

        let snap = mgr.get("p1").await.unwrap();
        assert_eq!(snap.status, PluginStatus::Active);
        assert!(snap.enabled);
    }

    #[tokio::test]
    async fn test_register_disabled_plugin_is_disabled() {
        let mgr = PluginManager::new();
        mgr.register(make_plugin("p1", PluginType::Auth, false))
            .await
            .unwrap();

        let snap = mgr.get("p1").await.unwrap();
        assert_eq!(snap.status, PluginStatus::Disabled);
        assert!(!snap.enabled);
    }

    // --- Unregister tests ---

    #[tokio::test]
    async fn test_unregister_plugin() {
        let mgr = PluginManager::new();
        mgr.register(auth_plugin("auth-basic")).await.unwrap();

        let removed = mgr.unregister("auth-basic").await.unwrap();
        assert_eq!(removed.name, "auth-basic");

        let list = mgr.list().await;
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_unregister_not_found() {
        let mgr = PluginManager::new();

        let err = mgr.unregister("nonexistent").await.unwrap_err();
        assert!(matches!(err, PluginError::NotFound(ref n) if n == "nonexistent"));
    }

    // --- Activate / Deactivate tests ---

    #[tokio::test]
    async fn test_activate_plugin() {
        let mgr = PluginManager::new();
        mgr.register(make_plugin("p1", PluginType::Memory, false))
            .await
            .unwrap();

        assert_eq!(mgr.get("p1").await.unwrap().status, PluginStatus::Disabled);

        mgr.activate("p1").await.unwrap();
        let snap = mgr.get("p1").await.unwrap();
        assert_eq!(snap.status, PluginStatus::Active);
        assert!(snap.enabled);
        assert!(snap.activated_at.is_some());
    }

    #[tokio::test]
    async fn test_deactivate_plugin() {
        let mgr = PluginManager::new();
        mgr.register(make_plugin("p1", PluginType::Memory, true))
            .await
            .unwrap();

        mgr.deactivate("p1").await.unwrap();
        let snap = mgr.get("p1").await.unwrap();
        assert_eq!(snap.status, PluginStatus::Disabled);
        assert!(!snap.enabled);
    }

    #[tokio::test]
    async fn test_activate_not_found() {
        let mgr = PluginManager::new();

        let err = mgr.activate("ghost").await.unwrap_err();
        assert!(matches!(err, PluginError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_deactivate_not_found() {
        let mgr = PluginManager::new();

        let err = mgr.deactivate("ghost").await.unwrap_err();
        assert!(matches!(err, PluginError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_activate_clears_error() {
        let mgr = PluginManager::new();
        mgr.register(auth_plugin("p1")).await.unwrap();
        mgr.set_error("p1", "something broke".to_string())
            .await
            .unwrap();
        assert_eq!(mgr.get("p1").await.unwrap().status, PluginStatus::Error);

        mgr.activate("p1").await.unwrap();
        let snap = mgr.get("p1").await.unwrap();
        assert_eq!(snap.status, PluginStatus::Active);
        assert!(snap.error.is_none());
    }

    // --- Error state tests ---

    #[tokio::test]
    async fn test_set_error_state() {
        let mgr = PluginManager::new();
        mgr.register(auth_plugin("p1")).await.unwrap();

        mgr.set_error("p1", "connection timeout".to_string())
            .await
            .unwrap();

        let snap = mgr.get("p1").await.unwrap();
        assert_eq!(snap.status, PluginStatus::Error);
        assert_eq!(snap.error.as_deref(), Some("connection timeout"));
    }

    #[tokio::test]
    async fn test_set_error_not_found() {
        let mgr = PluginManager::new();

        let err = mgr
            .set_error("ghost", "oops".to_string())
            .await
            .unwrap_err();
        assert!(matches!(err, PluginError::NotFound(_)));
    }

    // --- Get tests ---

    #[tokio::test]
    async fn test_get_by_name() {
        let mgr = PluginManager::new();
        mgr.register(provider_plugin("llm-openai")).await.unwrap();

        let snap = mgr.get("llm-openai").await;
        assert!(snap.is_some());
        let snap = snap.unwrap();
        assert_eq!(snap.name, "llm-openai");
        assert_eq!(snap.plugin_type, PluginType::Provider);
        assert_eq!(snap.version, "1.0.0");
    }

    #[tokio::test]
    async fn test_get_not_found_returns_none() {
        let mgr = PluginManager::new();

        assert!(mgr.get("nonexistent").await.is_none());
    }

    // --- List by type tests ---

    #[tokio::test]
    async fn test_list_by_type() {
        let mgr = PluginManager::new();
        mgr.register(auth_plugin("auth-1")).await.unwrap();
        mgr.register(auth_plugin("auth-2")).await.unwrap();
        mgr.register(memory_plugin("mem-1")).await.unwrap();
        mgr.register(provider_plugin("prov-1")).await.unwrap();

        let auths = mgr.list_by_type(PluginType::Auth).await;
        assert_eq!(auths.len(), 2);

        let mems = mgr.list_by_type(PluginType::Memory).await;
        assert_eq!(mems.len(), 1);

        let hooks = mgr.list_by_type(PluginType::Hook).await;
        assert!(hooks.is_empty());
    }

    #[tokio::test]
    async fn test_list_by_type_multiple_types() {
        let mgr = PluginManager::new();
        mgr.register(auth_plugin("a")).await.unwrap();
        mgr.register(memory_plugin("b")).await.unwrap();
        mgr.register(provider_plugin("c")).await.unwrap();
        mgr.register(make_plugin("d", PluginType::Hook, true))
            .await
            .unwrap();
        mgr.register(make_plugin("e", PluginType::Channel, true))
            .await
            .unwrap();
        mgr.register(make_plugin("f", PluginType::Tool, true))
            .await
            .unwrap();
        mgr.register(make_plugin("g", PluginType::HttpRoute, true))
            .await
            .unwrap();

        assert_eq!(mgr.list_by_type(PluginType::Auth).await.len(), 1);
        assert_eq!(mgr.list_by_type(PluginType::Memory).await.len(), 1);
        assert_eq!(mgr.list_by_type(PluginType::Provider).await.len(), 1);
        assert_eq!(mgr.list_by_type(PluginType::Hook).await.len(), 1);
        assert_eq!(mgr.list_by_type(PluginType::Channel).await.len(), 1);
        assert_eq!(mgr.list_by_type(PluginType::Tool).await.len(), 1);
        assert_eq!(mgr.list_by_type(PluginType::HttpRoute).await.len(), 1);
    }

    // --- Summary tests ---

    #[tokio::test]
    async fn test_summary_statistics() {
        let mgr = PluginManager::new();
        mgr.register(auth_plugin("auth-1")).await.unwrap();
        mgr.register(make_plugin("auth-2", PluginType::Auth, false))
            .await
            .unwrap();
        mgr.register(memory_plugin("mem-1")).await.unwrap();
        mgr.register(provider_plugin("prov-1")).await.unwrap();

        // Set one to error state.
        mgr.set_error("prov-1", "bad key".to_string())
            .await
            .unwrap();

        let summary = mgr.summary().await;
        assert_eq!(summary.total, 4);
        assert_eq!(summary.active, 2); // auth-1, mem-1
        assert_eq!(summary.disabled, 1); // auth-2
        assert_eq!(summary.error, 1); // prov-1
        assert_eq!(summary.by_type.get("auth"), Some(&2));
        assert_eq!(summary.by_type.get("memory"), Some(&1));
        assert_eq!(summary.by_type.get("provider"), Some(&1));
    }

    #[tokio::test]
    async fn test_summary_empty_manager() {
        let mgr = PluginManager::new();
        let summary = mgr.summary().await;
        assert_eq!(summary.total, 0);
        assert_eq!(summary.active, 0);
        assert_eq!(summary.disabled, 0);
        assert_eq!(summary.error, 0);
        assert!(summary.by_type.is_empty());
    }

    // --- Max plugins limit tests ---

    #[tokio::test]
    async fn test_max_plugins_limit() {
        let mgr = PluginManager::new().with_max_plugins(2);
        mgr.register(auth_plugin("p1")).await.unwrap();
        mgr.register(auth_plugin("p2")).await.unwrap();

        let err = mgr.register(auth_plugin("p3")).await.unwrap_err();
        assert!(matches!(err, PluginError::ValidationFailed(_)));
        assert!(err.to_string().contains("Maximum plugin limit"));
    }

    #[tokio::test]
    async fn test_max_plugins_limit_after_unregister() {
        let mgr = PluginManager::new().with_max_plugins(2);
        mgr.register(auth_plugin("p1")).await.unwrap();
        mgr.register(auth_plugin("p2")).await.unwrap();

        // At capacity, cannot add.
        assert!(mgr.register(auth_plugin("p3")).await.is_err());

        // Remove one, then can add again.
        mgr.unregister("p1").await.unwrap();
        mgr.register(auth_plugin("p3")).await.unwrap();
        assert_eq!(mgr.list().await.len(), 2);
    }

    // --- Update config tests ---

    #[tokio::test]
    async fn test_update_config() {
        let mgr = PluginManager::new();
        mgr.register(auth_plugin("auth-basic")).await.unwrap();

        let mut new_config = HashMap::new();
        new_config.insert(
            "token_url".to_string(),
            serde_json::Value::String("https://auth.example.com".to_string()),
        );
        new_config.insert(
            "max_retries".to_string(),
            serde_json::Value::Number(serde_json::Number::from(3)),
        );

        mgr.update_config("auth-basic", new_config).await.unwrap();

        // Re-read through unregister to inspect the underlying Plugin.
        let plugin = mgr.unregister("auth-basic").await.unwrap();
        assert_eq!(
            plugin.config.get("token_url").and_then(|v| v.as_str()),
            Some("https://auth.example.com")
        );
        assert_eq!(
            plugin.config.get("max_retries").and_then(|v| v.as_i64()),
            Some(3)
        );
    }

    #[tokio::test]
    async fn test_update_config_not_found() {
        let mgr = PluginManager::new();

        let err = mgr
            .update_config("ghost", HashMap::new())
            .await
            .unwrap_err();
        assert!(matches!(err, PluginError::NotFound(_)));
    }

    // --- Load order preservation tests ---

    #[tokio::test]
    async fn test_load_order_preserved() {
        let mgr = PluginManager::new();
        let names = ["alpha", "beta", "gamma", "delta"];
        for name in &names {
            mgr.register(auth_plugin(name)).await.unwrap();
        }

        let list = mgr.list().await;
        let listed_names: Vec<&str> = list.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(listed_names, names);
    }

    // --- Default manager tests ---

    #[tokio::test]
    async fn test_default_manager() {
        let mgr = PluginManager::default();
        assert!(mgr.list().await.is_empty());

        // Default should allow up to 100 plugins.
        mgr.register(auth_plugin("p1")).await.unwrap();
        assert_eq!(mgr.list().await.len(), 1);
    }

    // --- Snapshot serialization tests ---

    #[tokio::test]
    async fn test_snapshot_serialization() {
        let mgr = PluginManager::new();
        let mut plugin = auth_plugin("auth-oauth");
        plugin.routes = vec![PluginRoute {
            path: "/plugins/auth-oauth/callback".to_string(),
            method: "GET".to_string(),
            description: "OAuth callback endpoint".to_string(),
        }];
        mgr.register(plugin).await.unwrap();

        let snap = mgr.get("auth-oauth").await.unwrap();
        let json = serde_json::to_value(&snap).unwrap();

        assert_eq!(json["name"], "auth-oauth");
        assert_eq!(json["plugin_type"], "auth");
        assert_eq!(json["status"], "active");
        assert_eq!(json["enabled"], true);
        assert!(json["activated_at"].is_null());
        assert!(json["error"].is_null());
        assert_eq!(json["routes"][0]["path"], "/plugins/auth-oauth/callback");
    }

    #[tokio::test]
    async fn test_snapshot_with_activation_timestamp() {
        let mgr = PluginManager::new();
        mgr.register(make_plugin("p1", PluginType::Auth, false))
            .await
            .unwrap();
        mgr.activate("p1").await.unwrap();

        let snap = mgr.get("p1").await.unwrap();
        assert!(snap.activated_at.is_some());

        // Verify the timestamp is valid RFC 3339.
        let ts = snap.activated_at.unwrap();
        assert!(chrono::DateTime::parse_from_rfc3339(&ts).is_ok());
    }

    #[tokio::test]
    async fn test_summary_serialization() {
        let mgr = PluginManager::new();
        mgr.register(auth_plugin("a")).await.unwrap();
        mgr.register(memory_plugin("b")).await.unwrap();

        let summary = mgr.summary().await;
        let json = serde_json::to_value(&summary).unwrap();

        assert_eq!(json["total"], 2);
        assert_eq!(json["active"], 2);
        assert_eq!(json["disabled"], 0);
        assert_eq!(json["error"], 0);
        assert!(json["by_type"].is_object());
    }

    // --- Error variant display tests ---

    #[test]
    fn test_error_display_not_found() {
        let err = PluginError::NotFound("missing-plugin".to_string());
        assert_eq!(err.to_string(), "Plugin not found: missing-plugin");
    }

    #[test]
    fn test_error_display_already_registered() {
        let err = PluginError::AlreadyRegistered("dup".to_string());
        assert_eq!(err.to_string(), "Plugin already registered: dup");
    }

    #[test]
    fn test_error_display_activation_failed() {
        let err = PluginError::ActivationFailed {
            name: "my-plugin".to_string(),
            reason: "timeout".to_string(),
        };
        assert_eq!(err.to_string(), "Plugin activation failed: timeout");
    }

    #[test]
    fn test_error_display_type_mismatch() {
        let err = PluginError::TypeMismatch {
            expected: "auth".to_string(),
            got: "memory".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Plugin type mismatch: expected auth, got memory"
        );
    }

    #[test]
    fn test_error_display_validation_failed() {
        let err = PluginError::ValidationFailed("bad config".to_string());
        assert_eq!(err.to_string(), "Plugin validation failed: bad config");
    }

    #[test]
    fn test_error_display_dependency_not_met() {
        let err = PluginError::DependencyNotMet {
            plugin: "auth-oauth".to_string(),
            dependency: "http-server".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Plugin dependency not met: auth-oauth requires http-server"
        );
    }

    // --- Plugin routes in snapshot ---

    #[tokio::test]
    async fn test_plugin_with_routes_in_snapshot() {
        let mgr = PluginManager::new();
        let routes = vec![
            PluginRoute {
                path: "/api/v1/health".to_string(),
                method: "GET".to_string(),
                description: "Health check".to_string(),
            },
            PluginRoute {
                path: "/api/v1/data".to_string(),
                method: "POST".to_string(),
                description: "Submit data".to_string(),
            },
        ];
        mgr.register(make_plugin_with_routes("http-api", routes))
            .await
            .unwrap();

        let snap = mgr.get("http-api").await.unwrap();
        assert_eq!(snap.routes.len(), 2);
        assert_eq!(snap.routes[0].path, "/api/v1/health");
        assert_eq!(snap.routes[1].method, "POST");
    }
}
