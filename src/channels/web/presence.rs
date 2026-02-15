//! Presence tracking for connected gateway clients.
//!
//! Tracks connected clients (WebChat, CLI, nodes) with TTL-based expiry.
//! Each client registers with an instance ID and periodically sends heartbeats
//! to stay active. Stale entries are automatically eligible for cleanup.
//!
//! ```text
//! Client connects  ──► register(entry)
//! Client heartbeat ──► heartbeat(instance_id)
//! Client disconnects ► remove(instance_id)
//! Periodic cleanup  ──► cleanup_stale()
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Default TTL in seconds before an entry is considered stale.
const DEFAULT_TTL_SECS: u64 = 300;

/// Default maximum number of tracked entries.
const DEFAULT_MAX_ENTRIES: usize = 200;

/// Information about a connected client instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceEntry {
    /// Unique instance identifier (stable per connection).
    pub instance_id: String,
    /// Client type (e.g., "webchat", "cli", "node", "gateway").
    pub client_type: String,
    /// Hostname or display name.
    pub display_name: Option<String>,
    /// IP address if known.
    pub ip_address: Option<String>,
    /// Client version string.
    pub version: Option<String>,
    /// When this entry was first registered.
    pub connected_at: DateTime<Utc>,
    /// When this entry was last refreshed (heartbeat).
    pub last_seen: DateTime<Utc>,
}

/// Manages presence tracking for connected clients.
///
/// Thread-safe via `RwLock`. Entries that have not been refreshed within
/// `ttl_secs` seconds are considered stale and excluded from active listings.
pub struct PresenceTracker {
    entries: Arc<RwLock<HashMap<String, PresenceEntry>>>,
    /// TTL in seconds before an entry is considered stale (default: 300 = 5 minutes).
    ttl_secs: u64,
    /// Maximum number of entries to track (default: 200).
    max_entries: usize,
}

impl PresenceTracker {
    /// Create a new tracker with default settings (300s TTL, 200 max entries).
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            ttl_secs: DEFAULT_TTL_SECS,
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }

    /// Builder: set the TTL in seconds.
    pub fn with_ttl(mut self, ttl_secs: u64) -> Self {
        self.ttl_secs = ttl_secs;
        self
    }

    /// Builder: set the maximum number of tracked entries.
    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }

    /// Add or update a presence entry.
    ///
    /// If an entry with the same `instance_id` already exists, it is replaced.
    /// If the tracker is at capacity and the entry is new, the oldest entry
    /// (by `last_seen`) is evicted to make room.
    pub async fn register(&self, entry: PresenceEntry) {
        let mut entries = self.entries.write().await;

        // If the entry already exists, just update it in place.
        if entries.contains_key(&entry.instance_id) {
            entries.insert(entry.instance_id.clone(), entry);
            return;
        }

        // Evict the oldest entry if at capacity.
        if entries.len() >= self.max_entries {
            let oldest_id = entries
                .iter()
                .min_by_key(|(_, e)| e.last_seen)
                .map(|(id, _)| id.clone());

            if let Some(id) = oldest_id {
                entries.remove(&id);
            }
        }

        entries.insert(entry.instance_id.clone(), entry);
    }

    /// Refresh the `last_seen` timestamp for an existing entry.
    ///
    /// Returns `true` if the entry was found and updated, `false` if not found.
    pub async fn heartbeat(&self, instance_id: &str) -> bool {
        let mut entries = self.entries.write().await;
        match entries.get_mut(instance_id) {
            Some(entry) => {
                entry.last_seen = Utc::now();
                true
            }
            None => false,
        }
    }

    /// Explicitly remove an entry by instance ID.
    ///
    /// Returns the removed entry, or `None` if it was not found.
    pub async fn remove(&self, instance_id: &str) -> Option<PresenceEntry> {
        let mut entries = self.entries.write().await;
        entries.remove(instance_id)
    }

    /// Get a single entry by instance ID, if it exists and is not expired.
    pub async fn get(&self, instance_id: &str) -> Option<PresenceEntry> {
        let entries = self.entries.read().await;
        entries.get(instance_id).and_then(|entry| {
            if self.is_active(entry) {
                Some(entry.clone())
            } else {
                None
            }
        })
    }

    /// List all non-expired entries, sorted by `last_seen` descending (most recent first).
    pub async fn list_active(&self) -> Vec<PresenceEntry> {
        let entries = self.entries.read().await;
        let mut active: Vec<PresenceEntry> = entries
            .values()
            .filter(|e| self.is_active(e))
            .cloned()
            .collect();
        active.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        active
    }

    /// Remove all expired entries from the tracker.
    ///
    /// Returns the number of entries removed.
    pub async fn cleanup_stale(&self) -> usize {
        let mut entries = self.entries.write().await;
        let before = entries.len();
        entries.retain(|_, e| self.is_active(e));
        before - entries.len()
    }

    /// Count the number of active (non-expired) entries.
    pub async fn count(&self) -> usize {
        let entries = self.entries.read().await;
        entries.values().filter(|e| self.is_active(e)).count()
    }

    /// Check whether an entry is still within the TTL window.
    fn is_active(&self, entry: &PresenceEntry) -> bool {
        let elapsed = Utc::now()
            .signed_duration_since(entry.last_seen)
            .num_seconds();
        // Treat negative durations (clock skew) as active.
        elapsed >= 0 && (elapsed as u64) < self.ttl_secs
    }
}

impl Default for PresenceTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::Duration;

    /// Helper to create a test entry with the given instance ID and client type.
    fn make_entry(instance_id: &str, client_type: &str) -> PresenceEntry {
        let now = Utc::now();
        PresenceEntry {
            instance_id: instance_id.to_string(),
            client_type: client_type.to_string(),
            display_name: None,
            ip_address: None,
            version: None,
            connected_at: now,
            last_seen: now,
        }
    }

    /// Helper to create a test entry with a specific `last_seen` timestamp.
    fn make_entry_with_last_seen(
        instance_id: &str,
        client_type: &str,
        last_seen: DateTime<Utc>,
    ) -> PresenceEntry {
        PresenceEntry {
            instance_id: instance_id.to_string(),
            client_type: client_type.to_string(),
            display_name: None,
            ip_address: None,
            version: None,
            connected_at: last_seen,
            last_seen,
        }
    }

    #[tokio::test]
    async fn test_new_defaults() {
        let tracker = PresenceTracker::new();
        assert_eq!(tracker.ttl_secs, 300);
        assert_eq!(tracker.max_entries, 200);
        assert_eq!(tracker.count().await, 0);
    }

    #[tokio::test]
    async fn test_default_trait() {
        let tracker = PresenceTracker::default();
        assert_eq!(tracker.ttl_secs, 300);
        assert_eq!(tracker.max_entries, 200);
    }

    #[tokio::test]
    async fn test_builder_methods() {
        let tracker = PresenceTracker::new().with_ttl(60).with_max_entries(10);
        assert_eq!(tracker.ttl_secs, 60);
        assert_eq!(tracker.max_entries, 10);
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let tracker = PresenceTracker::new();
        let entry = make_entry("inst-1", "webchat");

        tracker.register(entry.clone()).await;

        let retrieved = tracker.get("inst-1").await;
        assert!(retrieved.is_some());
        let retrieved = retrieved.expect("entry should exist");
        assert_eq!(retrieved.instance_id, "inst-1");
        assert_eq!(retrieved.client_type, "webchat");
    }

    #[tokio::test]
    async fn test_get_nonexistent_returns_none() {
        let tracker = PresenceTracker::new();
        assert!(tracker.get("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_register_updates_existing() {
        let tracker = PresenceTracker::new();

        let entry1 = PresenceEntry {
            display_name: Some("Alice".to_string()),
            ..make_entry("inst-1", "webchat")
        };
        tracker.register(entry1).await;

        let entry2 = PresenceEntry {
            display_name: Some("Alice (updated)".to_string()),
            ..make_entry("inst-1", "webchat")
        };
        tracker.register(entry2).await;

        assert_eq!(tracker.count().await, 1);
        let retrieved = tracker.get("inst-1").await.expect("entry should exist");
        assert_eq!(retrieved.display_name.as_deref(), Some("Alice (updated)"));
    }

    #[tokio::test]
    async fn test_register_evicts_oldest_at_capacity() {
        let tracker = PresenceTracker::new().with_max_entries(2);

        let old = make_entry_with_last_seen("old", "cli", Utc::now() - Duration::seconds(10));
        let recent = make_entry("recent", "webchat");

        tracker.register(old).await;
        tracker.register(recent).await;

        // Now at capacity (2). Registering a third should evict "old".
        let newest = make_entry("newest", "node");
        tracker.register(newest).await;

        assert_eq!(tracker.count().await, 2);
        assert!(tracker.get("old").await.is_none());
        assert!(tracker.get("recent").await.is_some());
        assert!(tracker.get("newest").await.is_some());
    }

    #[tokio::test]
    async fn test_heartbeat_updates_last_seen() {
        let tracker = PresenceTracker::new();

        let entry =
            make_entry_with_last_seen("inst-1", "webchat", Utc::now() - Duration::seconds(5));
        let original_last_seen = entry.last_seen;
        tracker.register(entry).await;

        let result = tracker.heartbeat("inst-1").await;
        assert!(result);

        let updated = tracker.get("inst-1").await.expect("entry should exist");
        assert!(updated.last_seen > original_last_seen);
    }

    #[tokio::test]
    async fn test_heartbeat_unknown_returns_false() {
        let tracker = PresenceTracker::new();
        assert!(!tracker.heartbeat("nonexistent").await);
    }

    #[tokio::test]
    async fn test_remove_existing() {
        let tracker = PresenceTracker::new();
        tracker.register(make_entry("inst-1", "webchat")).await;

        let removed = tracker.remove("inst-1").await;
        assert!(removed.is_some());
        assert_eq!(removed.expect("should be Some").instance_id, "inst-1");
        assert_eq!(tracker.count().await, 0);
    }

    #[tokio::test]
    async fn test_remove_nonexistent_returns_none() {
        let tracker = PresenceTracker::new();
        assert!(tracker.remove("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_list_active_excludes_expired() {
        let tracker = PresenceTracker::new().with_ttl(10);

        // Active entry
        tracker.register(make_entry("active", "webchat")).await;

        // Expired entry (last_seen 20 seconds ago, TTL is 10)
        let expired =
            make_entry_with_last_seen("expired", "cli", Utc::now() - Duration::seconds(20));
        tracker.register(expired).await;

        let active = tracker.list_active().await;
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].instance_id, "active");
    }

    #[tokio::test]
    async fn test_list_active_sorted_by_last_seen_desc() {
        let tracker = PresenceTracker::new();

        let older = make_entry_with_last_seen("older", "cli", Utc::now() - Duration::seconds(5));
        let newer = make_entry("newer", "webchat");

        tracker.register(older).await;
        tracker.register(newer).await;

        let active = tracker.list_active().await;
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].instance_id, "newer");
        assert_eq!(active[1].instance_id, "older");
    }

    #[tokio::test]
    async fn test_list_active_empty() {
        let tracker = PresenceTracker::new();
        let active = tracker.list_active().await;
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn test_cleanup_stale_removes_expired() {
        let tracker = PresenceTracker::new().with_ttl(10);

        // Two active entries
        tracker.register(make_entry("active-1", "webchat")).await;
        tracker.register(make_entry("active-2", "cli")).await;

        // Two expired entries
        let expired1 =
            make_entry_with_last_seen("expired-1", "node", Utc::now() - Duration::seconds(20));
        let expired2 =
            make_entry_with_last_seen("expired-2", "gateway", Utc::now() - Duration::seconds(30));
        tracker.register(expired1).await;
        tracker.register(expired2).await;

        let removed = tracker.cleanup_stale().await;
        assert_eq!(removed, 2);
        assert_eq!(tracker.count().await, 2);
        assert!(tracker.get("active-1").await.is_some());
        assert!(tracker.get("active-2").await.is_some());
    }

    #[tokio::test]
    async fn test_cleanup_stale_nothing_to_remove() {
        let tracker = PresenceTracker::new();
        tracker.register(make_entry("inst-1", "webchat")).await;

        let removed = tracker.cleanup_stale().await;
        assert_eq!(removed, 0);
        assert_eq!(tracker.count().await, 1);
    }

    #[tokio::test]
    async fn test_count_excludes_expired() {
        let tracker = PresenceTracker::new().with_ttl(10);

        tracker.register(make_entry("active", "webchat")).await;

        let expired =
            make_entry_with_last_seen("expired", "cli", Utc::now() - Duration::seconds(20));
        tracker.register(expired).await;

        // Raw map has 2 entries, but count() should only report active ones.
        assert_eq!(tracker.count().await, 1);
    }

    #[tokio::test]
    async fn test_get_expired_returns_none() {
        let tracker = PresenceTracker::new().with_ttl(10);

        let expired =
            make_entry_with_last_seen("expired", "cli", Utc::now() - Duration::seconds(20));
        tracker.register(expired).await;

        assert!(tracker.get("expired").await.is_none());
    }

    #[tokio::test]
    async fn test_entry_with_all_optional_fields() {
        let tracker = PresenceTracker::new();
        let now = Utc::now();

        let entry = PresenceEntry {
            instance_id: "full".to_string(),
            client_type: "webchat".to_string(),
            display_name: Some("My Laptop".to_string()),
            ip_address: Some("192.168.1.100".to_string()),
            version: Some("1.2.3".to_string()),
            connected_at: now,
            last_seen: now,
        };

        tracker.register(entry).await;
        let retrieved = tracker.get("full").await.expect("entry should exist");
        assert_eq!(retrieved.display_name.as_deref(), Some("My Laptop"));
        assert_eq!(retrieved.ip_address.as_deref(), Some("192.168.1.100"));
        assert_eq!(retrieved.version.as_deref(), Some("1.2.3"));
    }

    #[tokio::test]
    async fn test_entry_serialization_roundtrip() {
        let now = Utc::now();
        let entry = PresenceEntry {
            instance_id: "ser-test".to_string(),
            client_type: "node".to_string(),
            display_name: Some("Test Node".to_string()),
            ip_address: None,
            version: Some("0.1.0".to_string()),
            connected_at: now,
            last_seen: now,
        };

        let json = serde_json::to_string(&entry).expect("serialization should succeed");
        let deserialized: PresenceEntry =
            serde_json::from_str(&json).expect("deserialization should succeed");

        assert_eq!(deserialized.instance_id, entry.instance_id);
        assert_eq!(deserialized.client_type, entry.client_type);
        assert_eq!(deserialized.display_name, entry.display_name);
        assert_eq!(deserialized.version, entry.version);
    }

    #[tokio::test]
    async fn test_multiple_client_types() {
        let tracker = PresenceTracker::new();

        tracker.register(make_entry("web-1", "webchat")).await;
        tracker.register(make_entry("cli-1", "cli")).await;
        tracker.register(make_entry("node-1", "node")).await;
        tracker.register(make_entry("gw-1", "gateway")).await;

        assert_eq!(tracker.count().await, 4);

        let active = tracker.list_active().await;
        let types: Vec<&str> = active.iter().map(|e| e.client_type.as_str()).collect();
        assert!(types.contains(&"webchat"));
        assert!(types.contains(&"cli"));
        assert!(types.contains(&"node"));
        assert!(types.contains(&"gateway"));
    }

    #[tokio::test]
    async fn test_zero_ttl_expires_immediately() {
        let tracker = PresenceTracker::new().with_ttl(0);

        tracker.register(make_entry("inst-1", "webchat")).await;

        // With TTL of 0, the entry should be expired immediately.
        assert_eq!(tracker.count().await, 0);
        assert!(tracker.get("inst-1").await.is_none());
    }

    #[tokio::test]
    async fn test_max_entries_one() {
        let tracker = PresenceTracker::new().with_max_entries(1);

        tracker.register(make_entry("first", "webchat")).await;
        tracker.register(make_entry("second", "cli")).await;

        // Only the most recently registered should remain.
        assert_eq!(tracker.count().await, 1);
        assert!(tracker.get("first").await.is_none());
        assert!(tracker.get("second").await.is_some());
    }
}
