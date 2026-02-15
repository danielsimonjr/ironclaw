//! Channel-level message delivery retry with exponential backoff.
//!
//! Wraps channel send operations with configurable retry behavior,
//! including exponential backoff with jitter to handle transient failures.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Configuration for delivery retry behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Base delay for exponential backoff (milliseconds).
    pub base_delay_ms: u64,
    /// Maximum delay cap (milliseconds).
    pub max_delay_ms: u64,
    /// Jitter factor (0.0 to 1.0) - randomness added to delay.
    pub jitter_factor: f64,
    /// Whether to enable retry for this channel.
    pub enabled: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 500,
            max_delay_ms: 30_000,
            jitter_factor: 0.25,
            enabled: true,
        }
    }
}

/// Outcome of a delivery attempt.
#[derive(Debug, Clone)]
pub enum DeliveryOutcome {
    /// Message delivered successfully.
    Delivered {
        /// Number of attempts made (1 = first try succeeded).
        attempts: u32,
    },
    /// All retries exhausted, delivery failed.
    Failed {
        /// Total number of attempts made.
        attempts: u32,
        /// Error message from the last failed attempt.
        last_error: String,
    },
    /// Retry is disabled, first attempt failed.
    NotRetried {
        /// Error message from the single attempt.
        error: String,
    },
}

/// Tracks delivery metrics per channel.
#[derive(Debug)]
pub struct DeliveryMetrics {
    /// Total delivery attempts across all operations.
    pub total_attempts: AtomicU64,
    /// Number of successful deliveries.
    pub successful: AtomicU64,
    /// Number of failed deliveries (all retries exhausted).
    pub failed: AtomicU64,
    /// Number of deliveries that required at least one retry.
    pub retried: AtomicU64,
    /// Cumulative retry delay in milliseconds.
    pub total_retry_delay_ms: AtomicU64,
}

impl DeliveryMetrics {
    /// Create a new metrics tracker with all counters at zero.
    pub fn new() -> Self {
        Self {
            total_attempts: AtomicU64::new(0),
            successful: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            retried: AtomicU64::new(0),
            total_retry_delay_ms: AtomicU64::new(0),
        }
    }

    /// Take a point-in-time snapshot of the metrics for serialization.
    pub fn snapshot(&self) -> DeliverySnapshot {
        let total_attempts = self.total_attempts.load(Ordering::Relaxed);
        let successful = self.successful.load(Ordering::Relaxed);
        let failed = self.failed.load(Ordering::Relaxed);
        let retried = self.retried.load(Ordering::Relaxed);
        let total_retry_delay_ms = self.total_retry_delay_ms.load(Ordering::Relaxed);

        let total_deliveries = successful + failed;
        let success_rate = if total_deliveries > 0 {
            successful as f64 / total_deliveries as f64
        } else {
            0.0
        };

        let avg_retry_delay_ms = if retried > 0 {
            total_retry_delay_ms as f64 / retried as f64
        } else {
            0.0
        };

        DeliverySnapshot {
            total_attempts,
            successful,
            failed,
            retried,
            avg_retry_delay_ms,
            success_rate,
        }
    }
}

impl Default for DeliveryMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of delivery metrics for serialization.
#[derive(Debug, Clone, Serialize)]
pub struct DeliverySnapshot {
    /// Total delivery attempts across all operations.
    pub total_attempts: u64,
    /// Number of successful deliveries.
    pub successful: u64,
    /// Number of failed deliveries (all retries exhausted).
    pub failed: u64,
    /// Number of deliveries that required at least one retry.
    pub retried: u64,
    /// Average retry delay in milliseconds (for deliveries that were retried).
    pub avg_retry_delay_ms: f64,
    /// Success rate as a fraction (0.0 to 1.0).
    pub success_rate: f64,
}

/// Per-channel delivery retry manager.
///
/// Manages retry configuration and metrics for each channel independently.
/// Provides exponential backoff with jitter for transient failure recovery.
pub struct DeliveryRetryManager {
    configs: Arc<RwLock<HashMap<String, RetryConfig>>>,
    metrics: Arc<RwLock<HashMap<String, Arc<DeliveryMetrics>>>>,
    default_config: RetryConfig,
}

impl DeliveryRetryManager {
    /// Create a new manager with default retry configuration.
    pub fn new() -> Self {
        Self {
            configs: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(HashMap::new())),
            default_config: RetryConfig::default(),
        }
    }

    /// Create a new manager with a custom default retry configuration.
    pub fn with_default_config(config: RetryConfig) -> Self {
        Self {
            configs: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(HashMap::new())),
            default_config: config,
        }
    }

    /// Set retry config for a specific channel.
    pub async fn set_channel_config(&self, channel: &str, config: RetryConfig) {
        self.configs
            .write()
            .await
            .insert(channel.to_string(), config);
    }

    /// Get retry config for a channel (falls back to default).
    pub async fn get_config(&self, channel: &str) -> RetryConfig {
        self.configs
            .read()
            .await
            .get(channel)
            .cloned()
            .unwrap_or_else(|| self.default_config.clone())
    }

    /// Calculate delay for a given attempt number with jitter.
    ///
    /// Uses exponential backoff: `base_delay * 2^attempt`, capped at `max_delay`,
    /// with additive jitter of up to `jitter_factor * computed_delay`.
    pub fn calculate_delay(config: &RetryConfig, attempt: u32) -> Duration {
        // Exponential backoff: base * 2^attempt
        let exp_delay_ms = config
            .base_delay_ms
            .saturating_mul(1u64.checked_shl(attempt).unwrap_or(u64::MAX));
        let capped_delay_ms = exp_delay_ms.min(config.max_delay_ms);

        // Apply jitter: add random value in [0, jitter_factor * delay]
        let jitter_range = (capped_delay_ms as f64 * config.jitter_factor) as u64;
        let jitter = if jitter_range > 0 {
            // Use simple pseudo-random based on attempt and current time for jitter.
            // This avoids pulling in rand just for jitter in production code,
            // while still providing decorrelation across retries.
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos() as u64;
            seed % (jitter_range + 1)
        } else {
            0
        };

        Duration::from_millis(capped_delay_ms.saturating_add(jitter))
    }

    /// Get or create metrics for a channel.
    async fn ensure_metrics(&self, channel: &str) -> Arc<DeliveryMetrics> {
        // Try read-only first
        {
            let metrics = self.metrics.read().await;
            if let Some(m) = metrics.get(channel) {
                return Arc::clone(m);
            }
        }

        // Create new metrics entry
        let mut metrics = self.metrics.write().await;
        Arc::clone(
            metrics
                .entry(channel.to_string())
                .or_insert_with(|| Arc::new(DeliveryMetrics::new())),
        )
    }

    /// Execute a delivery operation with retry.
    ///
    /// The `operation` closure is called for each attempt and should return
    /// `Ok(())` on success or `Err(String)` on failure. On failure, the manager
    /// will retry according to the channel's retry configuration with exponential
    /// backoff and jitter between attempts.
    pub async fn deliver_with_retry<F, Fut>(&self, channel: &str, operation: F) -> DeliveryOutcome
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        let config = self.get_config(channel).await;
        let metrics = self.ensure_metrics(channel).await;

        // First attempt
        metrics.total_attempts.fetch_add(1, Ordering::Relaxed);
        match operation().await {
            Ok(()) => {
                metrics.successful.fetch_add(1, Ordering::Relaxed);
                return DeliveryOutcome::Delivered { attempts: 1 };
            }
            Err(e) => {
                if !config.enabled || config.max_retries == 0 {
                    metrics.failed.fetch_add(1, Ordering::Relaxed);
                    return if config.enabled {
                        // Enabled but zero retries configured
                        DeliveryOutcome::Failed {
                            attempts: 1,
                            last_error: e,
                        }
                    } else {
                        DeliveryOutcome::NotRetried { error: e }
                    };
                }

                tracing::debug!(
                    channel = channel,
                    error = %e,
                    "Delivery attempt 1 failed, will retry"
                );

                let mut last_error = e;
                let mut total_delay_ms: u64 = 0;

                // Retry loop
                for attempt in 0..config.max_retries {
                    let delay = Self::calculate_delay(&config, attempt);
                    total_delay_ms = total_delay_ms.saturating_add(delay.as_millis() as u64);
                    tokio::time::sleep(delay).await;

                    metrics.total_attempts.fetch_add(1, Ordering::Relaxed);
                    match operation().await {
                        Ok(()) => {
                            metrics.successful.fetch_add(1, Ordering::Relaxed);
                            metrics.retried.fetch_add(1, Ordering::Relaxed);
                            metrics
                                .total_retry_delay_ms
                                .fetch_add(total_delay_ms, Ordering::Relaxed);

                            tracing::debug!(
                                channel = channel,
                                attempts = attempt + 2,
                                "Delivery succeeded after retry"
                            );

                            return DeliveryOutcome::Delivered {
                                attempts: attempt + 2,
                            };
                        }
                        Err(e) => {
                            tracing::debug!(
                                channel = channel,
                                attempt = attempt + 2,
                                error = %e,
                                "Delivery retry failed"
                            );
                            last_error = e;
                        }
                    }
                }

                // All retries exhausted
                metrics.failed.fetch_add(1, Ordering::Relaxed);
                metrics.retried.fetch_add(1, Ordering::Relaxed);
                metrics
                    .total_retry_delay_ms
                    .fetch_add(total_delay_ms, Ordering::Relaxed);

                tracing::warn!(
                    channel = channel,
                    attempts = config.max_retries + 1,
                    last_error = %last_error,
                    "Delivery failed after all retries"
                );

                DeliveryOutcome::Failed {
                    attempts: config.max_retries + 1,
                    last_error,
                }
            }
        }
    }

    /// Get metrics snapshot for a channel.
    pub async fn get_metrics(&self, channel: &str) -> Option<DeliverySnapshot> {
        self.metrics.read().await.get(channel).map(|m| m.snapshot())
    }

    /// Get metrics snapshots for all channels.
    pub async fn get_all_metrics(&self) -> HashMap<String, DeliverySnapshot> {
        self.metrics
            .read()
            .await
            .iter()
            .map(|(name, m)| (name.clone(), m.snapshot()))
            .collect()
    }

    /// Reset metrics for a channel.
    pub async fn reset_metrics(&self, channel: &str) {
        let mut metrics = self.metrics.write().await;
        metrics.insert(channel.to_string(), Arc::new(DeliveryMetrics::new()));
    }
}

impl Default for DeliveryRetryManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU32;

    use super::*;

    // ── Successful first attempt (no retry needed) ──

    #[tokio::test]
    async fn test_successful_first_attempt() {
        let manager = DeliveryRetryManager::new();
        let outcome = manager
            .deliver_with_retry("test", || async { Ok(()) })
            .await;

        match outcome {
            DeliveryOutcome::Delivered { attempts } => assert_eq!(attempts, 1),
            other => panic!("Expected Delivered, got {:?}", other),
        }
    }

    // ── Retry on transient failure then success ──

    #[tokio::test]
    async fn test_retry_then_success() {
        let manager = DeliveryRetryManager::new();
        manager
            .set_channel_config(
                "test",
                RetryConfig {
                    max_retries: 3,
                    base_delay_ms: 1, // minimal delay for tests
                    max_delay_ms: 10,
                    jitter_factor: 0.0,
                    enabled: true,
                },
            )
            .await;

        let call_count = Arc::new(AtomicU32::new(0));
        let cc = Arc::clone(&call_count);
        let outcome = manager
            .deliver_with_retry("test", || {
                let cc = Arc::clone(&cc);
                async move {
                    let n = cc.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        Err("transient error".to_string())
                    } else {
                        Ok(())
                    }
                }
            })
            .await;

        match outcome {
            DeliveryOutcome::Delivered { attempts } => assert_eq!(attempts, 3),
            other => panic!("Expected Delivered after retries, got {:?}", other),
        }
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    // ── All retries exhausted ──

    #[tokio::test]
    async fn test_all_retries_exhausted() {
        let manager = DeliveryRetryManager::new();
        manager
            .set_channel_config(
                "test",
                RetryConfig {
                    max_retries: 2,
                    base_delay_ms: 1,
                    max_delay_ms: 10,
                    jitter_factor: 0.0,
                    enabled: true,
                },
            )
            .await;

        let call_count = Arc::new(AtomicU32::new(0));
        let cc = Arc::clone(&call_count);
        let outcome = manager
            .deliver_with_retry("test", || {
                let cc = Arc::clone(&cc);
                async move {
                    cc.fetch_add(1, Ordering::SeqCst);
                    Err("persistent error".to_string())
                }
            })
            .await;

        match outcome {
            DeliveryOutcome::Failed {
                attempts,
                last_error,
            } => {
                assert_eq!(attempts, 3); // 1 initial + 2 retries
                assert_eq!(last_error, "persistent error");
            }
            other => panic!("Expected Failed, got {:?}", other),
        }
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    // ── Retry disabled returns NotRetried ──

    #[tokio::test]
    async fn test_retry_disabled() {
        let manager = DeliveryRetryManager::new();
        manager
            .set_channel_config(
                "test",
                RetryConfig {
                    enabled: false,
                    ..RetryConfig::default()
                },
            )
            .await;

        let outcome = manager
            .deliver_with_retry("test", || async { Err("some error".to_string()) })
            .await;

        match outcome {
            DeliveryOutcome::NotRetried { error } => {
                assert_eq!(error, "some error");
            }
            other => panic!("Expected NotRetried, got {:?}", other),
        }
    }

    // ── Exponential backoff calculation ──

    #[test]
    fn test_exponential_backoff_calculation() {
        let config = RetryConfig {
            base_delay_ms: 100,
            max_delay_ms: 10_000,
            jitter_factor: 0.0,
            ..RetryConfig::default()
        };

        let d0 = DeliveryRetryManager::calculate_delay(&config, 0);
        let d1 = DeliveryRetryManager::calculate_delay(&config, 1);
        let d2 = DeliveryRetryManager::calculate_delay(&config, 2);
        let d3 = DeliveryRetryManager::calculate_delay(&config, 3);

        assert_eq!(d0, Duration::from_millis(100)); // 100 * 2^0 = 100
        assert_eq!(d1, Duration::from_millis(200)); // 100 * 2^1 = 200
        assert_eq!(d2, Duration::from_millis(400)); // 100 * 2^2 = 400
        assert_eq!(d3, Duration::from_millis(800)); // 100 * 2^3 = 800
    }

    // ── Jitter within expected range ──

    #[test]
    fn test_jitter_within_expected_range() {
        let config = RetryConfig {
            base_delay_ms: 1000,
            max_delay_ms: 60_000,
            jitter_factor: 0.5,
            ..RetryConfig::default()
        };

        // Run multiple times to check jitter is within bounds
        for _ in 0..100 {
            let delay = DeliveryRetryManager::calculate_delay(&config, 0);
            let delay_ms = delay.as_millis() as u64;

            // Base delay is 1000ms, jitter can add up to 500ms (0.5 * 1000)
            assert!(delay_ms >= 1000, "Delay {}ms should be >= 1000ms", delay_ms);
            assert!(delay_ms <= 1500, "Delay {}ms should be <= 1500ms", delay_ms);
        }
    }

    // ── Max delay cap ──

    #[test]
    fn test_max_delay_cap() {
        let config = RetryConfig {
            base_delay_ms: 1000,
            max_delay_ms: 5000,
            jitter_factor: 0.0,
            ..RetryConfig::default()
        };

        // attempt=10: 1000 * 2^10 = 1,024,000 which far exceeds max
        let delay = DeliveryRetryManager::calculate_delay(&config, 10);
        assert_eq!(delay, Duration::from_millis(5000));
    }

    // ── Jitter respects max delay cap ──

    #[test]
    fn test_jitter_with_max_delay_cap() {
        let config = RetryConfig {
            base_delay_ms: 1000,
            max_delay_ms: 5000,
            jitter_factor: 0.25,
            ..RetryConfig::default()
        };

        // At high attempt, base delay is capped at 5000ms
        // jitter adds up to 0.25 * 5000 = 1250ms
        for _ in 0..50 {
            let delay = DeliveryRetryManager::calculate_delay(&config, 10);
            let delay_ms = delay.as_millis() as u64;
            assert!(delay_ms >= 5000, "Delay {}ms should be >= 5000ms", delay_ms);
            assert!(delay_ms <= 6250, "Delay {}ms should be <= 6250ms", delay_ms);
        }
    }

    // ── Metrics tracking: successful delivery ──

    #[tokio::test]
    async fn test_metrics_successful_delivery() {
        let manager = DeliveryRetryManager::new();
        let _ = manager
            .deliver_with_retry("test_ch", || async { Ok(()) })
            .await;

        let snap = manager.get_metrics("test_ch").await.unwrap();
        assert_eq!(snap.total_attempts, 1);
        assert_eq!(snap.successful, 1);
        assert_eq!(snap.failed, 0);
        assert_eq!(snap.retried, 0);
        assert!((snap.success_rate - 1.0).abs() < f64::EPSILON);
    }

    // ── Metrics tracking: failed delivery ──

    #[tokio::test]
    async fn test_metrics_failed_delivery() {
        let manager = DeliveryRetryManager::new();
        manager
            .set_channel_config(
                "test_ch",
                RetryConfig {
                    max_retries: 2,
                    base_delay_ms: 1,
                    max_delay_ms: 5,
                    jitter_factor: 0.0,
                    enabled: true,
                },
            )
            .await;

        let _ = manager
            .deliver_with_retry("test_ch", || async { Err("fail".to_string()) })
            .await;

        let snap = manager.get_metrics("test_ch").await.unwrap();
        assert_eq!(snap.total_attempts, 3); // 1 initial + 2 retries
        assert_eq!(snap.successful, 0);
        assert_eq!(snap.failed, 1);
        assert_eq!(snap.retried, 1);
        assert!((snap.success_rate - 0.0).abs() < f64::EPSILON);
    }

    // ── Metrics tracking: retried then succeeded ──

    #[tokio::test]
    async fn test_metrics_retried_success() {
        let manager = DeliveryRetryManager::new();
        manager
            .set_channel_config(
                "test_ch",
                RetryConfig {
                    max_retries: 3,
                    base_delay_ms: 1,
                    max_delay_ms: 5,
                    jitter_factor: 0.0,
                    enabled: true,
                },
            )
            .await;

        let counter = Arc::new(AtomicU32::new(0));
        let c = Arc::clone(&counter);
        let _ = manager
            .deliver_with_retry("test_ch", || {
                let c = Arc::clone(&c);
                async move {
                    if c.fetch_add(1, Ordering::SeqCst) == 0 {
                        Err("first fail".to_string())
                    } else {
                        Ok(())
                    }
                }
            })
            .await;

        let snap = manager.get_metrics("test_ch").await.unwrap();
        assert_eq!(snap.total_attempts, 2);
        assert_eq!(snap.successful, 1);
        assert_eq!(snap.failed, 0);
        assert_eq!(snap.retried, 1);
        assert!((snap.success_rate - 1.0).abs() < f64::EPSILON);
    }

    // ── Per-channel config ──

    #[tokio::test]
    async fn test_per_channel_config() {
        let manager = DeliveryRetryManager::new();

        let slack_config = RetryConfig {
            max_retries: 5,
            base_delay_ms: 200,
            ..RetryConfig::default()
        };
        manager
            .set_channel_config("slack", slack_config.clone())
            .await;

        let telegram_config = RetryConfig {
            max_retries: 10,
            base_delay_ms: 100,
            ..RetryConfig::default()
        };
        manager
            .set_channel_config("telegram", telegram_config.clone())
            .await;

        let slack = manager.get_config("slack").await;
        assert_eq!(slack.max_retries, 5);
        assert_eq!(slack.base_delay_ms, 200);

        let telegram = manager.get_config("telegram").await;
        assert_eq!(telegram.max_retries, 10);
        assert_eq!(telegram.base_delay_ms, 100);
    }

    // ── Default config fallback ──

    #[tokio::test]
    async fn test_default_config_fallback() {
        let custom_default = RetryConfig {
            max_retries: 7,
            base_delay_ms: 250,
            max_delay_ms: 15_000,
            jitter_factor: 0.1,
            enabled: true,
        };
        let manager = DeliveryRetryManager::with_default_config(custom_default);

        // No channel-specific config set, should fall back to custom default
        let config = manager.get_config("unknown_channel").await;
        assert_eq!(config.max_retries, 7);
        assert_eq!(config.base_delay_ms, 250);
        assert_eq!(config.max_delay_ms, 15_000);
        assert!((config.jitter_factor - 0.1).abs() < f64::EPSILON);
    }

    // ── Reset metrics ──

    #[tokio::test]
    async fn test_reset_metrics() {
        let manager = DeliveryRetryManager::new();

        // Generate some metrics
        let _ = manager
            .deliver_with_retry("test_ch", || async { Ok(()) })
            .await;
        let snap = manager.get_metrics("test_ch").await.unwrap();
        assert_eq!(snap.successful, 1);

        // Reset
        manager.reset_metrics("test_ch").await;
        let snap = manager.get_metrics("test_ch").await.unwrap();
        assert_eq!(snap.successful, 0);
        assert_eq!(snap.total_attempts, 0);
        assert_eq!(snap.failed, 0);
        assert_eq!(snap.retried, 0);
    }

    // ── Concurrent delivery ──

    #[tokio::test]
    async fn test_concurrent_delivery() {
        let manager = Arc::new(DeliveryRetryManager::new());

        let mut handles = Vec::new();
        for i in 0..10 {
            let m = Arc::clone(&manager);
            handles.push(tokio::spawn(async move {
                m.deliver_with_retry("concurrent", move || async move {
                    if i % 3 == 0 {
                        Err("intermittent".to_string())
                    } else {
                        Ok(())
                    }
                })
                .await
            }));
        }

        for handle in handles {
            let _ = handle.await.unwrap();
        }

        let snap = manager.get_metrics("concurrent").await.unwrap();
        // All 10 deliveries should have been attempted
        assert!(snap.total_attempts >= 10);
    }

    // ── Zero retries config ──

    #[tokio::test]
    async fn test_zero_retries_config() {
        let manager = DeliveryRetryManager::new();
        manager
            .set_channel_config(
                "test",
                RetryConfig {
                    max_retries: 0,
                    enabled: true,
                    ..RetryConfig::default()
                },
            )
            .await;

        let call_count = Arc::new(AtomicU32::new(0));
        let cc = Arc::clone(&call_count);
        let outcome = manager
            .deliver_with_retry("test", || {
                let cc = Arc::clone(&cc);
                async move {
                    cc.fetch_add(1, Ordering::SeqCst);
                    Err("error".to_string())
                }
            })
            .await;

        // Should only attempt once, no retries
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
        match outcome {
            DeliveryOutcome::Failed {
                attempts,
                last_error,
            } => {
                assert_eq!(attempts, 1);
                assert_eq!(last_error, "error");
            }
            other => panic!("Expected Failed with 1 attempt, got {:?}", other),
        }
    }

    // ── Get all metrics across channels ──

    #[tokio::test]
    async fn test_get_all_metrics() {
        let manager = DeliveryRetryManager::new();

        let _ = manager
            .deliver_with_retry("slack", || async { Ok(()) })
            .await;
        let _ = manager
            .deliver_with_retry("telegram", || async { Ok(()) })
            .await;
        let _ = manager
            .deliver_with_retry("http", || async { Err("fail".to_string()) })
            .await;

        let all = manager.get_all_metrics().await;
        assert_eq!(all.len(), 3);
        assert!(all.contains_key("slack"));
        assert!(all.contains_key("telegram"));
        assert!(all.contains_key("http"));

        assert_eq!(all["slack"].successful, 1);
        assert_eq!(all["telegram"].successful, 1);
    }

    // ── Get metrics for nonexistent channel returns None ──

    #[tokio::test]
    async fn test_get_metrics_nonexistent_channel() {
        let manager = DeliveryRetryManager::new();
        assert!(manager.get_metrics("nonexistent").await.is_none());
    }

    // ── DeliveryMetrics snapshot accuracy ──

    #[test]
    fn test_delivery_metrics_snapshot() {
        let metrics = DeliveryMetrics::new();
        metrics.total_attempts.store(10, Ordering::Relaxed);
        metrics.successful.store(7, Ordering::Relaxed);
        metrics.failed.store(3, Ordering::Relaxed);
        metrics.retried.store(4, Ordering::Relaxed);
        metrics.total_retry_delay_ms.store(2000, Ordering::Relaxed);

        let snap = metrics.snapshot();
        assert_eq!(snap.total_attempts, 10);
        assert_eq!(snap.successful, 7);
        assert_eq!(snap.failed, 3);
        assert_eq!(snap.retried, 4);
        assert!((snap.avg_retry_delay_ms - 500.0).abs() < f64::EPSILON);
        assert!((snap.success_rate - 0.7).abs() < f64::EPSILON);
    }

    // ── DeliveryMetrics snapshot with zero deliveries ──

    #[test]
    fn test_delivery_metrics_snapshot_empty() {
        let metrics = DeliveryMetrics::new();
        let snap = metrics.snapshot();

        assert_eq!(snap.total_attempts, 0);
        assert_eq!(snap.successful, 0);
        assert_eq!(snap.failed, 0);
        assert_eq!(snap.retried, 0);
        assert!((snap.avg_retry_delay_ms - 0.0).abs() < f64::EPSILON);
        assert!((snap.success_rate - 0.0).abs() < f64::EPSILON);
    }

    // ── RetryConfig default values ──

    #[test]
    fn test_retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_ms, 500);
        assert_eq!(config.max_delay_ms, 30_000);
        assert!((config.jitter_factor - 0.25).abs() < f64::EPSILON);
        assert!(config.enabled);
    }

    // ── RetryConfig serialization roundtrip ──

    #[test]
    fn test_retry_config_serde_roundtrip() {
        let config = RetryConfig {
            max_retries: 5,
            base_delay_ms: 1000,
            max_delay_ms: 60_000,
            jitter_factor: 0.3,
            enabled: true,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: RetryConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.max_retries, 5);
        assert_eq!(deserialized.base_delay_ms, 1000);
        assert_eq!(deserialized.max_delay_ms, 60_000);
        assert!((deserialized.jitter_factor - 0.3).abs() < f64::EPSILON);
        assert!(deserialized.enabled);
    }

    // ── Backoff with zero base delay ──

    #[test]
    fn test_backoff_zero_base_delay() {
        let config = RetryConfig {
            base_delay_ms: 0,
            max_delay_ms: 10_000,
            jitter_factor: 0.0,
            ..RetryConfig::default()
        };

        let delay = DeliveryRetryManager::calculate_delay(&config, 0);
        assert_eq!(delay, Duration::from_millis(0));
    }

    // ── Multiple deliveries accumulate metrics ──

    #[tokio::test]
    async fn test_metrics_accumulation() {
        let manager = DeliveryRetryManager::new();

        // 3 successful deliveries
        for _ in 0..3 {
            let _ = manager.deliver_with_retry("acc", || async { Ok(()) }).await;
        }

        let snap = manager.get_metrics("acc").await.unwrap();
        assert_eq!(snap.total_attempts, 3);
        assert_eq!(snap.successful, 3);
        assert_eq!(snap.failed, 0);
    }

    // ── DeliveryRetryManager Default trait impl ──

    #[tokio::test]
    async fn test_manager_default_trait() {
        let manager = DeliveryRetryManager::default();
        let config = manager.get_config("any").await;
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_ms, 500);
    }

    // ── Retry succeeds on last attempt ──

    #[tokio::test]
    async fn test_retry_succeeds_on_last_attempt() {
        let manager = DeliveryRetryManager::new();
        manager
            .set_channel_config(
                "test",
                RetryConfig {
                    max_retries: 3,
                    base_delay_ms: 1,
                    max_delay_ms: 5,
                    jitter_factor: 0.0,
                    enabled: true,
                },
            )
            .await;

        let call_count = Arc::new(AtomicU32::new(0));
        let cc = Arc::clone(&call_count);
        let outcome = manager
            .deliver_with_retry("test", || {
                let cc = Arc::clone(&cc);
                async move {
                    let n = cc.fetch_add(1, Ordering::SeqCst);
                    // Succeed only on 4th attempt (index 3)
                    if n < 3 {
                        Err(format!("attempt {} failed", n + 1))
                    } else {
                        Ok(())
                    }
                }
            })
            .await;

        match outcome {
            DeliveryOutcome::Delivered { attempts } => {
                assert_eq!(attempts, 4); // 1 initial + 3 retries
            }
            other => panic!("Expected Delivered on last attempt, got {:?}", other),
        }
    }

    // ── Metrics track retry delay ──

    #[tokio::test]
    async fn test_metrics_track_retry_delay() {
        let manager = DeliveryRetryManager::new();
        manager
            .set_channel_config(
                "test",
                RetryConfig {
                    max_retries: 2,
                    base_delay_ms: 10,
                    max_delay_ms: 100,
                    jitter_factor: 0.0,
                    enabled: true,
                },
            )
            .await;

        let counter = Arc::new(AtomicU32::new(0));
        let c = Arc::clone(&counter);
        let _ = manager
            .deliver_with_retry("test", || {
                let c = Arc::clone(&c);
                async move {
                    if c.fetch_add(1, Ordering::SeqCst) == 0 {
                        Err("fail".to_string())
                    } else {
                        Ok(())
                    }
                }
            })
            .await;

        let snap = manager.get_metrics("test").await.unwrap();
        assert_eq!(snap.retried, 1);
        // The delay should be at least 10ms (the base delay for attempt 0)
        assert!(
            snap.avg_retry_delay_ms >= 10.0,
            "avg retry delay {}ms should be >= 10.0ms",
            snap.avg_retry_delay_ms
        );
    }

    // ── Separate metrics per channel ──

    #[tokio::test]
    async fn test_separate_metrics_per_channel() {
        let manager = DeliveryRetryManager::new();

        // Succeed on channel A
        let _ = manager
            .deliver_with_retry("channel_a", || async { Ok(()) })
            .await;

        // Fail on channel B (with retries disabled for speed)
        manager
            .set_channel_config(
                "channel_b",
                RetryConfig {
                    max_retries: 0,
                    enabled: true,
                    ..RetryConfig::default()
                },
            )
            .await;
        let _ = manager
            .deliver_with_retry("channel_b", || async { Err("fail".to_string()) })
            .await;

        let snap_a = manager.get_metrics("channel_a").await.unwrap();
        let snap_b = manager.get_metrics("channel_b").await.unwrap();

        assert_eq!(snap_a.successful, 1);
        assert_eq!(snap_a.failed, 0);
        assert_eq!(snap_b.successful, 0);
        assert_eq!(snap_b.failed, 1);
    }

    // ── High jitter factor ──

    #[test]
    fn test_high_jitter_factor() {
        let config = RetryConfig {
            base_delay_ms: 1000,
            max_delay_ms: 60_000,
            jitter_factor: 1.0,
            ..RetryConfig::default()
        };

        for _ in 0..100 {
            let delay = DeliveryRetryManager::calculate_delay(&config, 0);
            let delay_ms = delay.as_millis() as u64;
            // With jitter_factor=1.0, delay should be in [1000, 2000]
            assert!(delay_ms >= 1000, "Delay {}ms should be >= 1000ms", delay_ms);
            assert!(delay_ms <= 2000, "Delay {}ms should be <= 2000ms", delay_ms);
        }
    }

    // ── Zero jitter factor produces exact delay ──

    #[test]
    fn test_zero_jitter_produces_exact_delay() {
        let config = RetryConfig {
            base_delay_ms: 500,
            max_delay_ms: 60_000,
            jitter_factor: 0.0,
            ..RetryConfig::default()
        };

        // Without jitter, delay should be exactly base * 2^attempt
        for attempt in 0..5 {
            let delay = DeliveryRetryManager::calculate_delay(&config, attempt);
            let expected = 500u64 * (1u64 << attempt);
            assert_eq!(
                delay,
                Duration::from_millis(expected),
                "Attempt {}: expected {}ms, got {:?}",
                attempt,
                expected,
                delay
            );
        }
    }

    // ── Overflow safety for very high attempt numbers ──

    #[test]
    fn test_overflow_safety_high_attempts() {
        let config = RetryConfig {
            base_delay_ms: 1000,
            max_delay_ms: 30_000,
            jitter_factor: 0.0,
            ..RetryConfig::default()
        };

        // Very large attempt number should not panic
        let delay = DeliveryRetryManager::calculate_delay(&config, 100);
        // Should be capped at max_delay
        assert_eq!(delay, Duration::from_millis(30_000));
    }
}
