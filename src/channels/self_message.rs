//! Self-message bypass for channel messaging.
//!
//! When the agent sends a message to a channel (e.g., Telegram, Slack),
//! the channel may echo the message back as an incoming message. This
//! module detects and filters out those self-messages to avoid infinite
//! loops and unnecessary processing.

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::RwLock;

/// Manages self-message detection and filtering.
///
/// Tracks the agent's own identities across channels so that messages
/// from the agent itself can be recognized and skipped during pairing
/// and routing.
pub struct SelfMessageFilter {
    /// Set of known self-identifiers per channel (channel_name:sender_id).
    self_ids: Arc<RwLock<HashSet<String>>>,
    /// Whether self-message bypass is enabled.
    enabled: bool,
}

impl SelfMessageFilter {
    /// Create a new self-message filter.
    pub fn new(enabled: bool) -> Self {
        Self {
            self_ids: Arc::new(RwLock::new(HashSet::new())),
            enabled,
        }
    }

    /// Register a self-identity for a channel.
    ///
    /// # Arguments
    /// * `channel` - Channel name (e.g., "telegram", "slack")
    /// * `sender_id` - The agent's sender ID on that channel
    pub async fn register_self_id(&self, channel: &str, sender_id: &str) {
        let key = format!("{}:{}", channel, sender_id);
        self.self_ids.write().await.insert(key);
        tracing::debug!(channel, sender_id, "Registered self-identity for channel");
    }

    /// Remove a self-identity.
    pub async fn unregister_self_id(&self, channel: &str, sender_id: &str) {
        let key = format!("{}:{}", channel, sender_id);
        self.self_ids.write().await.remove(&key);
    }

    /// Check if a message is from the agent itself.
    ///
    /// Returns `true` if the message should be skipped (it's a self-message).
    pub async fn is_self_message(&self, channel: &str, sender_id: &str) -> bool {
        if !self.enabled {
            return false;
        }
        let key = format!("{}:{}", channel, sender_id);
        self.self_ids.read().await.contains(&key)
    }

    /// Check if bypass is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// List all registered self-identities.
    pub async fn list_self_ids(&self) -> Vec<String> {
        self.self_ids.read().await.iter().cloned().collect()
    }
}

impl Default for SelfMessageFilter {
    fn default() -> Self {
        Self::new(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_detect() {
        let filter = SelfMessageFilter::new(true);
        filter.register_self_id("telegram", "bot123").await;

        assert!(filter.is_self_message("telegram", "bot123").await);
        assert!(!filter.is_self_message("telegram", "user456").await);
        assert!(!filter.is_self_message("slack", "bot123").await);
    }

    #[tokio::test]
    async fn test_disabled_filter() {
        let filter = SelfMessageFilter::new(false);
        filter.register_self_id("telegram", "bot123").await;

        // Should not detect as self-message when disabled
        assert!(!filter.is_self_message("telegram", "bot123").await);
    }

    #[tokio::test]
    async fn test_unregister() {
        let filter = SelfMessageFilter::new(true);
        filter.register_self_id("slack", "botid").await;
        assert!(filter.is_self_message("slack", "botid").await);

        filter.unregister_self_id("slack", "botid").await;
        assert!(!filter.is_self_message("slack", "botid").await);
    }

    #[tokio::test]
    async fn test_list_self_ids() {
        let filter = SelfMessageFilter::new(true);
        filter.register_self_id("telegram", "bot1").await;
        filter.register_self_id("slack", "bot2").await;

        let ids = filter.list_self_ids().await;
        assert_eq!(ids.len(), 2);
    }
}
