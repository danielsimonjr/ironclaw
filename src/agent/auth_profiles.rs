//! Authentication profiles for multiple authentication strategies per session/channel.
//!
//! Allows the agent to manage different authentication profiles (API keys, OAuth tokens,
//! bearer tokens, session tokens) and associate them with specific channels.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// The type of authentication used by a profile.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuthProfileType {
    /// API key-based authentication (e.g., `Authorization: ApiKey <key>`).
    ApiKey,
    /// OAuth 2.0 token-based authentication.
    OAuth,
    /// Bearer token authentication (e.g., `Authorization: Bearer <token>`).
    Bearer,
    /// Session-based authentication (e.g., cookies or session IDs).
    Session,
    /// Custom authentication strategy identified by a string tag.
    Custom(String),
}

impl std::fmt::Display for AuthProfileType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiKey => write!(f, "ApiKey"),
            Self::OAuth => write!(f, "OAuth"),
            Self::Bearer => write!(f, "Bearer"),
            Self::Session => write!(f, "Session"),
            Self::Custom(tag) => write!(f, "Custom({})", tag),
        }
    }
}

/// An authentication profile containing credentials and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthProfile {
    /// Human-readable name for the profile (e.g., "production-api", "staging-oauth").
    pub name: String,
    /// The authentication strategy this profile uses.
    pub profile_type: AuthProfileType,
    /// Key-value credential store (e.g., "api_key" -> "sk-...", "token" -> "...").
    pub credentials: HashMap<String, String>,
    /// Whether this is the default profile when no specific one is requested.
    pub default: bool,
    /// Which channels this profile applies to (empty means all channels).
    pub channels: Vec<String>,
}

impl AuthProfile {
    /// Create a new authentication profile.
    pub fn new(
        name: impl Into<String>,
        profile_type: AuthProfileType,
        credentials: HashMap<String, String>,
    ) -> Self {
        Self {
            name: name.into(),
            profile_type,
            credentials,
            default: false,
            channels: Vec::new(),
        }
    }

    /// Set this profile as the default.
    pub fn with_default(mut self, default: bool) -> Self {
        self.default = default;
        self
    }

    /// Associate this profile with specific channels.
    pub fn with_channels(mut self, channels: Vec<String>) -> Self {
        self.channels = channels;
        self
    }
}

/// Manages a collection of authentication profiles.
///
/// Thread-safe via `Arc<RwLock<...>>` for concurrent access from
/// multiple channels and sessions.
#[derive(Debug, Clone)]
pub struct AuthProfileManager {
    profiles: Arc<RwLock<Vec<AuthProfile>>>,
}

impl AuthProfileManager {
    /// Create a new empty profile manager.
    pub fn new() -> Self {
        Self {
            profiles: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a new authentication profile.
    ///
    /// If a profile with the same name already exists, it will be replaced.
    /// If the new profile is marked as default, all other profiles of the same
    /// type will have their default flag cleared.
    pub async fn register(&self, profile: AuthProfile) {
        let mut profiles = self.profiles.write().await;

        // If this profile is the default, unset default on other profiles of the same type.
        if profile.default {
            for existing in profiles.iter_mut() {
                if existing.profile_type == profile.profile_type {
                    existing.default = false;
                }
            }
        }

        // Replace existing profile with the same name, or append.
        if let Some(pos) = profiles.iter().position(|p| p.name == profile.name) {
            profiles[pos] = profile;
        } else {
            profiles.push(profile);
        }
    }

    /// Get a profile by name.
    pub async fn get(&self, name: &str) -> Option<AuthProfile> {
        let profiles = self.profiles.read().await;
        profiles.iter().find(|p| p.name == name).cloned()
    }

    /// List all registered profiles.
    pub async fn list(&self) -> Vec<AuthProfile> {
        let profiles = self.profiles.read().await;
        profiles.clone()
    }

    /// Set a profile as the default for its authentication type.
    ///
    /// Returns `true` if the profile was found and updated, `false` otherwise.
    pub async fn set_default(&self, name: &str) -> bool {
        let mut profiles = self.profiles.write().await;

        let target_type = match profiles.iter().find(|p| p.name == name) {
            Some(p) => p.profile_type.clone(),
            None => return false,
        };

        // Clear default on all profiles of the same type, then set the target.
        for profile in profiles.iter_mut() {
            if profile.profile_type == target_type {
                profile.default = profile.name == name;
            }
        }

        true
    }

    /// Get the appropriate profile for a given channel.
    ///
    /// Resolution order:
    /// 1. A profile explicitly associated with the channel.
    /// 2. The default profile (any type).
    /// 3. `None` if no matching profile exists.
    pub async fn get_for_channel(&self, channel: &str) -> Option<AuthProfile> {
        let profiles = self.profiles.read().await;

        // First: look for a profile explicitly bound to this channel.
        if let Some(profile) = profiles
            .iter()
            .find(|p| p.channels.iter().any(|c| c == channel))
        {
            return Some(profile.clone());
        }

        // Second: fall back to the default profile.
        profiles.iter().find(|p| p.default).cloned()
    }
}

impl Default for AuthProfileManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_api_key_profile(name: &str) -> AuthProfile {
        let mut creds = HashMap::new();
        creds.insert("api_key".to_string(), format!("sk-{}", name));
        AuthProfile::new(name, AuthProfileType::ApiKey, creds)
    }

    fn make_oauth_profile(name: &str) -> AuthProfile {
        let mut creds = HashMap::new();
        creds.insert("access_token".to_string(), format!("token-{}", name));
        creds.insert("refresh_token".to_string(), format!("refresh-{}", name));
        AuthProfile::new(name, AuthProfileType::OAuth, creds)
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let manager = AuthProfileManager::new();
        let profile = make_api_key_profile("prod");

        manager.register(profile.clone()).await;

        let retrieved = manager.get("prod").await;
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.name, "prod");
        assert_eq!(retrieved.profile_type, AuthProfileType::ApiKey);
        assert_eq!(retrieved.credentials.get("api_key").unwrap(), "sk-prod");
    }

    #[tokio::test]
    async fn test_get_nonexistent_returns_none() {
        let manager = AuthProfileManager::new();
        assert!(manager.get("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_register_replaces_existing() {
        let manager = AuthProfileManager::new();

        let profile1 = make_api_key_profile("prod");
        manager.register(profile1).await;

        let mut creds = HashMap::new();
        creds.insert("api_key".to_string(), "sk-updated".to_string());
        let profile2 = AuthProfile::new("prod", AuthProfileType::ApiKey, creds);
        manager.register(profile2).await;

        let profiles = manager.list().await;
        assert_eq!(profiles.len(), 1);
        assert_eq!(
            profiles[0].credentials.get("api_key").unwrap(),
            "sk-updated"
        );
    }

    #[tokio::test]
    async fn test_list_profiles() {
        let manager = AuthProfileManager::new();
        manager.register(make_api_key_profile("prod")).await;
        manager.register(make_oauth_profile("staging")).await;

        let profiles = manager.list().await;
        assert_eq!(profiles.len(), 2);
    }

    #[tokio::test]
    async fn test_set_default() {
        let manager = AuthProfileManager::new();

        let p1 = make_api_key_profile("key-a").with_default(true);
        let p2 = make_api_key_profile("key-b");

        manager.register(p1).await;
        manager.register(p2).await;

        // key-a should be default
        let profiles = manager.list().await;
        assert!(profiles.iter().find(|p| p.name == "key-a").unwrap().default);
        assert!(!profiles.iter().find(|p| p.name == "key-b").unwrap().default);

        // Switch default to key-b
        let result = manager.set_default("key-b").await;
        assert!(result);

        let profiles = manager.list().await;
        assert!(!profiles.iter().find(|p| p.name == "key-a").unwrap().default);
        assert!(profiles.iter().find(|p| p.name == "key-b").unwrap().default);
    }

    #[tokio::test]
    async fn test_set_default_nonexistent_returns_false() {
        let manager = AuthProfileManager::new();
        assert!(!manager.set_default("ghost").await);
    }

    #[tokio::test]
    async fn test_default_only_affects_same_type() {
        let manager = AuthProfileManager::new();

        let api = make_api_key_profile("api-default").with_default(true);
        let oauth = make_oauth_profile("oauth-default").with_default(true);

        manager.register(api).await;
        manager.register(oauth).await;

        // Both should remain default since they are different types.
        let profiles = manager.list().await;
        assert!(
            profiles
                .iter()
                .find(|p| p.name == "api-default")
                .unwrap()
                .default
        );
        assert!(
            profiles
                .iter()
                .find(|p| p.name == "oauth-default")
                .unwrap()
                .default
        );
    }

    #[tokio::test]
    async fn test_get_for_channel_explicit_binding() {
        let manager = AuthProfileManager::new();

        let telegram_profile =
            make_api_key_profile("telegram-key").with_channels(vec!["telegram".to_string()]);
        let default_profile = make_oauth_profile("default-oauth").with_default(true);

        manager.register(telegram_profile).await;
        manager.register(default_profile).await;

        // Telegram channel should get the explicitly bound profile.
        let result = manager.get_for_channel("telegram").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "telegram-key");
    }

    #[tokio::test]
    async fn test_get_for_channel_falls_back_to_default() {
        let manager = AuthProfileManager::new();

        let telegram_profile =
            make_api_key_profile("telegram-key").with_channels(vec!["telegram".to_string()]);
        let default_profile = make_oauth_profile("default-oauth").with_default(true);

        manager.register(telegram_profile).await;
        manager.register(default_profile).await;

        // Slack has no explicit binding, should fall back to default.
        let result = manager.get_for_channel("slack").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "default-oauth");
    }

    #[tokio::test]
    async fn test_get_for_channel_no_match() {
        let manager = AuthProfileManager::new();

        let telegram_profile =
            make_api_key_profile("telegram-key").with_channels(vec!["telegram".to_string()]);
        manager.register(telegram_profile).await;

        // No default, no explicit binding for slack.
        let result = manager.get_for_channel("slack").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_custom_profile_type() {
        let manager = AuthProfileManager::new();

        let mut creds = HashMap::new();
        creds.insert("token".to_string(), "custom-token".to_string());
        let profile = AuthProfile::new(
            "custom-auth",
            AuthProfileType::Custom("HMAC-SHA256".to_string()),
            creds,
        );

        manager.register(profile).await;

        let retrieved = manager.get("custom-auth").await.unwrap();
        assert_eq!(
            retrieved.profile_type,
            AuthProfileType::Custom("HMAC-SHA256".to_string())
        );
    }

    #[tokio::test]
    async fn test_profile_display() {
        assert_eq!(AuthProfileType::ApiKey.to_string(), "ApiKey");
        assert_eq!(AuthProfileType::OAuth.to_string(), "OAuth");
        assert_eq!(AuthProfileType::Bearer.to_string(), "Bearer");
        assert_eq!(AuthProfileType::Session.to_string(), "Session");
        assert_eq!(
            AuthProfileType::Custom("JWT".to_string()).to_string(),
            "Custom(JWT)"
        );
    }
}
