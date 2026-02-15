//! Channel status tracking for monitoring connected channels.
//!
//! Tracks per-channel metrics including message counts, error counts,
//! connection status, and throughput. Thread-safe via `RwLock` and atomics.
//!
//! ```text
//! Channel connects    --> register_channel(name)
//! Message received    --> record_message(name)
//! Error occurred      --> record_error(name, reason)
//! Status changed      --> set_status(name, status)
//! Dashboard queries   --> get_all_statuses(), message_throughput()
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::channels::web::types::ChannelStatusInfo;

/// Status of a channel connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelStatus {
    Connected,
    Disconnected,
    Error(String),
}

impl std::fmt::Display for ChannelStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelStatus::Connected => write!(f, "connected"),
            ChannelStatus::Disconnected => write!(f, "disconnected"),
            ChannelStatus::Error(msg) => write!(f, "error: {}", msg),
        }
    }
}

/// Per-channel metrics tracked by the status tracker.
pub struct ChannelMetrics {
    pub status: RwLock<ChannelStatus>,
    pub connected_since: RwLock<Option<DateTime<Utc>>>,
    pub message_count: AtomicU64,
    pub last_message_at: RwLock<Option<DateTime<Utc>>>,
    pub error_count: AtomicU64,
    pub last_error: RwLock<Option<String>>,
}

impl ChannelMetrics {
    fn new() -> Self {
        Self {
            status: RwLock::new(ChannelStatus::Disconnected),
            connected_since: RwLock::new(None),
            message_count: AtomicU64::new(0),
            last_message_at: RwLock::new(None),
            error_count: AtomicU64::new(0),
            last_error: RwLock::new(None),
        }
    }
}

/// Tracks status and metrics for all registered channels.
///
/// Thread-safe: uses `Arc<RwLock<>>` for the channel map and per-field
/// locks/atomics within each `ChannelMetrics`.
pub struct ChannelStatusTracker {
    channels: Arc<RwLock<HashMap<String, Arc<ChannelMetrics>>>>,
    started_at: Instant,
}

impl ChannelStatusTracker {
    /// Create a new tracker. Records the current instant as the start time.
    pub fn new() -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            started_at: Instant::now(),
        }
    }

    /// Register a new channel. If the channel already exists, this is a no-op.
    pub async fn register_channel(&self, name: &str) {
        let mut channels = self.channels.write().await;
        channels
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(ChannelMetrics::new()));
    }

    /// Record that a message was received on the named channel.
    ///
    /// Increments the message count and updates `last_message_at`.
    /// No-op if the channel is not registered.
    pub async fn record_message(&self, name: &str) {
        let channels = self.channels.read().await;
        if let Some(metrics) = channels.get(name) {
            metrics.message_count.fetch_add(1, Ordering::Relaxed);
            let mut last = metrics.last_message_at.write().await;
            *last = Some(Utc::now());
        }
    }

    /// Record an error on the named channel.
    ///
    /// Increments the error count and stores the error reason.
    /// No-op if the channel is not registered.
    pub async fn record_error(&self, name: &str, reason: &str) {
        let channels = self.channels.read().await;
        if let Some(metrics) = channels.get(name) {
            metrics.error_count.fetch_add(1, Ordering::Relaxed);
            let mut last_err = metrics.last_error.write().await;
            *last_err = Some(reason.to_string());
        }
    }

    /// Update the connection status of a channel.
    ///
    /// When transitioning to `Connected`, sets `connected_since` to now.
    /// When transitioning away from `Connected`, clears `connected_since`.
    /// No-op if the channel is not registered.
    pub async fn set_status(&self, name: &str, status: ChannelStatus) {
        let channels = self.channels.read().await;
        if let Some(metrics) = channels.get(name) {
            let is_connecting = status == ChannelStatus::Connected;
            {
                let mut s = metrics.status.write().await;
                *s = status;
            }
            {
                let mut cs = metrics.connected_since.write().await;
                if is_connecting {
                    if cs.is_none() {
                        *cs = Some(Utc::now());
                    }
                } else {
                    *cs = None;
                }
            }
        }
    }

    /// Get the status information for all registered channels.
    pub async fn get_all_statuses(&self) -> Vec<ChannelStatusInfo> {
        let channels = self.channels.read().await;
        let mut result = Vec::with_capacity(channels.len());

        for (name, metrics) in channels.iter() {
            let status = metrics.status.read().await;
            let connected_since = metrics.connected_since.read().await;
            let last_message_at = metrics.last_message_at.read().await;
            let last_error = metrics.last_error.read().await;

            let status_str = match &*status {
                ChannelStatus::Connected => "connected".to_string(),
                ChannelStatus::Disconnected => "disconnected".to_string(),
                ChannelStatus::Error(_) => "error".to_string(),
            };

            let metadata = match (&*status, &*last_error) {
                (ChannelStatus::Error(msg), _) => {
                    serde_json::json!({"error": msg})
                }
                (_, Some(err)) => {
                    serde_json::json!({"last_error": err})
                }
                _ => serde_json::Value::Object(serde_json::Map::new()),
            };

            result.push(ChannelStatusInfo {
                name: name.clone(),
                status: status_str,
                connected_since: connected_since.map(|dt| dt.to_rfc3339()),
                message_count: metrics.message_count.load(Ordering::Relaxed),
                last_message_at: last_message_at.map(|dt| dt.to_rfc3339()),
                error_count: metrics.error_count.load(Ordering::Relaxed),
                metadata,
            });
        }

        // Sort by name for deterministic output.
        result.sort_by(|a, b| a.name.cmp(&b.name));
        result
    }

    /// Get uptime in seconds since the tracker was created.
    pub fn uptime(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Calculate overall message throughput (messages per minute) across all channels.
    ///
    /// Uses total message count divided by uptime. Returns 0.0 if uptime is zero.
    pub async fn message_throughput(&self) -> f64 {
        let elapsed_secs = self.started_at.elapsed().as_secs_f64();
        if elapsed_secs < 1.0 {
            return 0.0;
        }

        let channels = self.channels.read().await;
        let total_messages: u64 = channels
            .values()
            .map(|m| m.message_count.load(Ordering::Relaxed))
            .sum();

        (total_messages as f64 / elapsed_secs) * 60.0
    }

    /// Get the total number of messages across all channels.
    pub async fn total_messages(&self) -> u64 {
        let channels = self.channels.read().await;
        channels
            .values()
            .map(|m| m.message_count.load(Ordering::Relaxed))
            .sum()
    }

    /// Get the number of registered channels.
    pub async fn channel_count(&self) -> usize {
        let channels = self.channels.read().await;
        channels.len()
    }

    /// Get the number of connected channels.
    pub async fn connected_count(&self) -> usize {
        let channels = self.channels.read().await;
        let mut count = 0;
        for metrics in channels.values() {
            let status = metrics.status.read().await;
            if *status == ChannelStatus::Connected {
                count += 1;
            }
        }
        count
    }

    /// Get status for a single channel by name.
    pub async fn get_channel_status(&self, name: &str) -> Option<ChannelStatusInfo> {
        let channels = self.channels.read().await;
        let metrics = channels.get(name)?;

        let status = metrics.status.read().await;
        let connected_since = metrics.connected_since.read().await;
        let last_message_at = metrics.last_message_at.read().await;
        let last_error = metrics.last_error.read().await;

        let status_str = match &*status {
            ChannelStatus::Connected => "connected".to_string(),
            ChannelStatus::Disconnected => "disconnected".to_string(),
            ChannelStatus::Error(_) => "error".to_string(),
        };

        let metadata = match (&*status, &*last_error) {
            (ChannelStatus::Error(msg), _) => {
                serde_json::json!({"error": msg})
            }
            (_, Some(err)) => {
                serde_json::json!({"last_error": err})
            }
            _ => serde_json::Value::Object(serde_json::Map::new()),
        };

        Some(ChannelStatusInfo {
            name: name.to_string(),
            status: status_str,
            connected_since: connected_since.map(|dt| dt.to_rfc3339()),
            message_count: metrics.message_count.load(Ordering::Relaxed),
            last_message_at: last_message_at.map(|dt| dt.to_rfc3339()),
            error_count: metrics.error_count.load(Ordering::Relaxed),
            metadata,
        })
    }
}

impl Default for ChannelStatusTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_tracker_has_zero_channels() {
        let tracker = ChannelStatusTracker::new();
        assert_eq!(tracker.channel_count().await, 0);
    }

    #[tokio::test]
    async fn test_default_trait() {
        let tracker = ChannelStatusTracker::default();
        assert_eq!(tracker.channel_count().await, 0);
    }

    #[tokio::test]
    async fn test_register_channel() {
        let tracker = ChannelStatusTracker::new();
        tracker.register_channel("repl").await;
        assert_eq!(tracker.channel_count().await, 1);
    }

    #[tokio::test]
    async fn test_register_duplicate_is_noop() {
        let tracker = ChannelStatusTracker::new();
        tracker.register_channel("repl").await;
        tracker.register_channel("repl").await;
        assert_eq!(tracker.channel_count().await, 1);
    }

    #[tokio::test]
    async fn test_register_multiple_channels() {
        let tracker = ChannelStatusTracker::new();
        tracker.register_channel("repl").await;
        tracker.register_channel("gateway").await;
        tracker.register_channel("telegram").await;
        assert_eq!(tracker.channel_count().await, 3);
    }

    #[tokio::test]
    async fn test_initial_status_is_disconnected() {
        let tracker = ChannelStatusTracker::new();
        tracker.register_channel("repl").await;

        let status = tracker.get_channel_status("repl").await.unwrap();
        assert_eq!(status.status, "disconnected");
        assert!(status.connected_since.is_none());
    }

    #[tokio::test]
    async fn test_set_status_connected() {
        let tracker = ChannelStatusTracker::new();
        tracker.register_channel("repl").await;
        tracker.set_status("repl", ChannelStatus::Connected).await;

        let status = tracker.get_channel_status("repl").await.unwrap();
        assert_eq!(status.status, "connected");
        assert!(status.connected_since.is_some());
    }

    #[tokio::test]
    async fn test_set_status_error() {
        let tracker = ChannelStatusTracker::new();
        tracker.register_channel("telegram").await;
        tracker
            .set_status(
                "telegram",
                ChannelStatus::Error("connection refused".to_string()),
            )
            .await;

        let status = tracker.get_channel_status("telegram").await.unwrap();
        assert_eq!(status.status, "error");
        assert_eq!(status.metadata["error"], "connection refused");
    }

    #[tokio::test]
    async fn test_connected_since_cleared_on_disconnect() {
        let tracker = ChannelStatusTracker::new();
        tracker.register_channel("repl").await;

        tracker.set_status("repl", ChannelStatus::Connected).await;
        let status = tracker.get_channel_status("repl").await.unwrap();
        assert!(status.connected_since.is_some());

        tracker
            .set_status("repl", ChannelStatus::Disconnected)
            .await;
        let status = tracker.get_channel_status("repl").await.unwrap();
        assert!(status.connected_since.is_none());
        assert_eq!(status.status, "disconnected");
    }

    #[tokio::test]
    async fn test_record_message_increments_count() {
        let tracker = ChannelStatusTracker::new();
        tracker.register_channel("repl").await;

        tracker.record_message("repl").await;
        tracker.record_message("repl").await;
        tracker.record_message("repl").await;

        let status = tracker.get_channel_status("repl").await.unwrap();
        assert_eq!(status.message_count, 3);
        assert!(status.last_message_at.is_some());
    }

    #[tokio::test]
    async fn test_record_message_unregistered_channel_is_noop() {
        let tracker = ChannelStatusTracker::new();
        // Should not panic.
        tracker.record_message("nonexistent").await;
    }

    #[tokio::test]
    async fn test_record_error_increments_count() {
        let tracker = ChannelStatusTracker::new();
        tracker.register_channel("gateway").await;

        tracker.record_error("gateway", "timeout").await;
        tracker.record_error("gateway", "rate limited").await;

        let status = tracker.get_channel_status("gateway").await.unwrap();
        assert_eq!(status.error_count, 2);
        assert_eq!(status.metadata["last_error"], "rate limited");
    }

    #[tokio::test]
    async fn test_record_error_unregistered_channel_is_noop() {
        let tracker = ChannelStatusTracker::new();
        // Should not panic.
        tracker.record_error("nonexistent", "something broke").await;
    }

    #[tokio::test]
    async fn test_set_status_unregistered_channel_is_noop() {
        let tracker = ChannelStatusTracker::new();
        // Should not panic.
        tracker
            .set_status("nonexistent", ChannelStatus::Connected)
            .await;
    }

    #[tokio::test]
    async fn test_get_channel_status_nonexistent_returns_none() {
        let tracker = ChannelStatusTracker::new();
        assert!(tracker.get_channel_status("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_get_all_statuses_empty() {
        let tracker = ChannelStatusTracker::new();
        let statuses = tracker.get_all_statuses().await;
        assert!(statuses.is_empty());
    }

    #[tokio::test]
    async fn test_get_all_statuses_sorted_by_name() {
        let tracker = ChannelStatusTracker::new();
        tracker.register_channel("telegram").await;
        tracker.register_channel("gateway").await;
        tracker.register_channel("repl").await;

        let statuses = tracker.get_all_statuses().await;
        assert_eq!(statuses.len(), 3);
        assert_eq!(statuses[0].name, "gateway");
        assert_eq!(statuses[1].name, "repl");
        assert_eq!(statuses[2].name, "telegram");
    }

    #[tokio::test]
    async fn test_total_messages_across_channels() {
        let tracker = ChannelStatusTracker::new();
        tracker.register_channel("repl").await;
        tracker.register_channel("gateway").await;

        tracker.record_message("repl").await;
        tracker.record_message("repl").await;
        tracker.record_message("gateway").await;

        assert_eq!(tracker.total_messages().await, 3);
    }

    #[tokio::test]
    async fn test_connected_count() {
        let tracker = ChannelStatusTracker::new();
        tracker.register_channel("repl").await;
        tracker.register_channel("gateway").await;
        tracker.register_channel("telegram").await;

        tracker.set_status("repl", ChannelStatus::Connected).await;
        tracker
            .set_status("gateway", ChannelStatus::Connected)
            .await;
        // telegram stays disconnected

        assert_eq!(tracker.connected_count().await, 2);
    }

    #[tokio::test]
    async fn test_uptime_is_non_negative() {
        let tracker = ChannelStatusTracker::new();
        // Uptime should be at least 0 (it was just created).
        assert!(tracker.uptime() < 5);
    }

    #[tokio::test]
    async fn test_message_throughput_zero_when_no_messages() {
        let tracker = ChannelStatusTracker::new();
        tracker.register_channel("repl").await;
        // With no messages, throughput should be 0.
        let throughput = tracker.message_throughput().await;
        assert!((throughput - 0.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_channel_status_display() {
        assert_eq!(ChannelStatus::Connected.to_string(), "connected");
        assert_eq!(ChannelStatus::Disconnected.to_string(), "disconnected");
        assert_eq!(
            ChannelStatus::Error("timeout".to_string()).to_string(),
            "error: timeout"
        );
    }

    #[tokio::test]
    async fn test_connected_since_preserved_on_repeated_connect() {
        let tracker = ChannelStatusTracker::new();
        tracker.register_channel("repl").await;

        tracker.set_status("repl", ChannelStatus::Connected).await;
        let first = tracker.get_channel_status("repl").await.unwrap();
        let first_connected = first.connected_since.clone().unwrap();

        // Setting connected again should not change connected_since.
        tracker.set_status("repl", ChannelStatus::Connected).await;
        let second = tracker.get_channel_status("repl").await.unwrap();
        assert_eq!(second.connected_since.unwrap(), first_connected);
    }

    #[tokio::test]
    async fn test_error_metadata_in_status() {
        let tracker = ChannelStatusTracker::new();
        tracker.register_channel("slack").await;

        // Record an error then check metadata when status is not error
        tracker.record_error("slack", "rate limited").await;
        let status = tracker.get_channel_status("slack").await.unwrap();
        // Status is still disconnected, but last_error shows in metadata
        assert_eq!(status.status, "disconnected");
        assert_eq!(status.metadata["last_error"], "rate limited");

        // Now set error status
        tracker
            .set_status("slack", ChannelStatus::Error("connection lost".to_string()))
            .await;
        let status = tracker.get_channel_status("slack").await.unwrap();
        assert_eq!(status.status, "error");
        assert_eq!(status.metadata["error"], "connection lost");
    }

    #[tokio::test]
    async fn test_channel_status_info_serialization() {
        let info = ChannelStatusInfo {
            name: "repl".to_string(),
            status: "connected".to_string(),
            connected_since: Some("2026-01-01T00:00:00+00:00".to_string()),
            message_count: 42,
            last_message_at: Some("2026-01-01T01:00:00+00:00".to_string()),
            error_count: 0,
            metadata: serde_json::json!({}),
        };

        let json = serde_json::to_string(&info).expect("serialization should succeed");
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(parsed["name"], "repl");
        assert_eq!(parsed["status"], "connected");
        assert_eq!(parsed["message_count"], 42);
    }

    #[tokio::test]
    async fn test_full_lifecycle() {
        let tracker = ChannelStatusTracker::new();

        // Register
        tracker.register_channel("telegram").await;
        assert_eq!(tracker.channel_count().await, 1);
        assert_eq!(tracker.connected_count().await, 0);

        // Connect
        tracker
            .set_status("telegram", ChannelStatus::Connected)
            .await;
        assert_eq!(tracker.connected_count().await, 1);

        // Send messages
        tracker.record_message("telegram").await;
        tracker.record_message("telegram").await;
        assert_eq!(tracker.total_messages().await, 2);

        // Error
        tracker.record_error("telegram", "api error").await;

        // Disconnect
        tracker
            .set_status("telegram", ChannelStatus::Disconnected)
            .await;
        assert_eq!(tracker.connected_count().await, 0);

        // Verify final state
        let status = tracker.get_channel_status("telegram").await.unwrap();
        assert_eq!(status.status, "disconnected");
        assert_eq!(status.message_count, 2);
        assert_eq!(status.error_count, 1);
        assert!(status.connected_since.is_none());
    }
}
