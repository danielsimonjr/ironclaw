//! Plugin system extensions.
//!
//! Extends the basic extension system with plugin categories:
//! - Auth plugins: Custom authentication strategies
//! - Memory plugins: Alternative memory/storage backends
//! - Hook plugins: Lifecycle hook implementations
//! - Provider plugins: Additional LLM providers
//! - HTTP path plugins: Custom HTTP routes registered by plugins

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Types of plugins that can be registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    /// Authentication strategy plugin.
    Auth,
    /// Memory/storage backend plugin.
    Memory,
    /// Lifecycle hook plugin.
    Hook,
    /// LLM provider plugin.
    Provider,
    /// HTTP route plugin.
    HttpRoute,
    /// Channel plugin.
    Channel,
    /// Tool plugin.
    Tool,
}

impl std::fmt::Display for PluginType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auth => write!(f, "auth"),
            Self::Memory => write!(f, "memory"),
            Self::Hook => write!(f, "hook"),
            Self::Provider => write!(f, "provider"),
            Self::HttpRoute => write!(f, "http_route"),
            Self::Channel => write!(f, "channel"),
            Self::Tool => write!(f, "tool"),
        }
    }
}

/// A registered plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    /// Unique plugin identifier.
    pub name: String,
    /// Plugin type.
    pub plugin_type: PluginType,
    /// Human-readable description.
    pub description: String,
    /// Version string.
    pub version: String,
    /// Whether the plugin is enabled.
    pub enabled: bool,
    /// Plugin configuration.
    pub config: HashMap<String, serde_json::Value>,
    /// HTTP routes registered by this plugin.
    pub routes: Vec<PluginRoute>,
}

/// An HTTP route registered by a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRoute {
    /// HTTP path (e.g., "/plugins/my-plugin/api").
    pub path: String,
    /// HTTP method (GET, POST, PUT, DELETE).
    pub method: String,
    /// Description of what this route does.
    pub description: String,
}

/// Status of a plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginStatus {
    Active,
    Disabled,
    Error,
    Loading,
}

/// Auth profile for multi-auth strategy support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthProfile {
    /// Profile name.
    pub name: String,
    /// Authentication type.
    pub auth_type: String,
    /// Whether this is the default profile.
    pub is_default: bool,
    /// Profile-specific configuration.
    pub config: HashMap<String, serde_json::Value>,
}

/// Registry for managing plugins.
pub struct PluginRegistry {
    plugins: Arc<RwLock<HashMap<String, Plugin>>>,
    auth_profiles: Arc<RwLock<Vec<AuthProfile>>>,
}

impl PluginRegistry {
    /// Create a new plugin registry.
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            auth_profiles: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a plugin.
    pub async fn register(&self, plugin: Plugin) {
        self.plugins
            .write()
            .await
            .insert(plugin.name.clone(), plugin);
    }

    /// Unregister a plugin.
    pub async fn unregister(&self, name: &str) -> bool {
        self.plugins.write().await.remove(name).is_some()
    }

    /// Get a plugin by name.
    pub async fn get(&self, name: &str) -> Option<Plugin> {
        self.plugins.read().await.get(name).cloned()
    }

    /// List all plugins.
    pub async fn list(&self) -> Vec<Plugin> {
        self.plugins.read().await.values().cloned().collect()
    }

    /// List plugins by type.
    pub async fn list_by_type(&self, plugin_type: PluginType) -> Vec<Plugin> {
        self.plugins
            .read()
            .await
            .values()
            .filter(|p| p.plugin_type == plugin_type)
            .cloned()
            .collect()
    }

    /// Get all HTTP routes from active plugins.
    pub async fn all_routes(&self) -> Vec<(String, PluginRoute)> {
        self.plugins
            .read()
            .await
            .values()
            .filter(|p| p.enabled)
            .flat_map(|p| {
                p.routes
                    .iter()
                    .map(|r| (p.name.clone(), r.clone()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Register an auth profile.
    pub async fn register_auth_profile(&self, profile: AuthProfile) {
        let mut profiles = self.auth_profiles.write().await;
        // If this is marked as default, unmark others
        if profile.is_default {
            for p in profiles.iter_mut() {
                p.is_default = false;
            }
        }
        profiles.push(profile);
    }

    /// Get the active auth profile.
    pub async fn active_auth_profile(&self) -> Option<AuthProfile> {
        let profiles = self.auth_profiles.read().await;
        profiles
            .iter()
            .find(|p| p.is_default)
            .or(profiles.first())
            .cloned()
    }

    /// List all auth profiles.
    pub async fn list_auth_profiles(&self) -> Vec<AuthProfile> {
        self.auth_profiles.read().await.clone()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_plugin() -> Plugin {
        Plugin {
            name: "test-plugin".to_string(),
            plugin_type: PluginType::Tool,
            description: "A test plugin".to_string(),
            version: "1.0.0".to_string(),
            enabled: true,
            config: HashMap::new(),
            routes: vec![PluginRoute {
                path: "/plugins/test/api".to_string(),
                method: "GET".to_string(),
                description: "Test endpoint".to_string(),
            }],
        }
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let registry = PluginRegistry::new();
        registry.register(test_plugin()).await;

        let plugin = registry.get("test-plugin").await;
        assert!(plugin.is_some());
        assert_eq!(plugin.unwrap().name, "test-plugin");
    }

    #[tokio::test]
    async fn test_list_by_type() {
        let registry = PluginRegistry::new();
        registry.register(test_plugin()).await;

        let tools = registry.list_by_type(PluginType::Tool).await;
        assert_eq!(tools.len(), 1);

        let hooks = registry.list_by_type(PluginType::Hook).await;
        assert_eq!(hooks.len(), 0);
    }

    #[tokio::test]
    async fn test_routes() {
        let registry = PluginRegistry::new();
        registry.register(test_plugin()).await;

        let routes = registry.all_routes().await;
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].1.path, "/plugins/test/api");
    }

    #[tokio::test]
    async fn test_auth_profiles() {
        let registry = PluginRegistry::new();

        registry
            .register_auth_profile(AuthProfile {
                name: "default".to_string(),
                auth_type: "bearer".to_string(),
                is_default: true,
                config: HashMap::new(),
            })
            .await;

        let active = registry.active_auth_profile().await;
        assert!(active.is_some());
        assert_eq!(active.unwrap().name, "default");
    }
}
