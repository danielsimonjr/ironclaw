//! Media caching for processed files.
//!
//! Caches downloaded and processed media to avoid redundant downloads
//! and processing.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

/// Cached media entry.
#[derive(Clone)]
struct CacheEntry {
    data: Vec<u8>,
    mime_type: String,
    inserted_at: Instant,
    access_count: u64,
}

/// In-memory media cache with TTL and size limits.
pub struct MediaCache {
    entries: Arc<RwLock<HashMap<String, CacheEntry>>>,
    max_entries: usize,
    max_total_bytes: usize,
    ttl: Duration,
}

impl MediaCache {
    /// Create a new media cache.
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries: 100,
            max_total_bytes: 100 * 1024 * 1024, // 100MB
            ttl: Duration::from_secs(3600),     // 1 hour
        }
    }

    /// Set maximum number of cached entries.
    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }

    /// Set maximum total cache size in bytes.
    pub fn with_max_bytes(mut self, max: usize) -> Self {
        self.max_total_bytes = max;
        self
    }

    /// Set time-to-live for cache entries.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Get a cached entry.
    pub async fn get(&self, key: &str) -> Option<(Vec<u8>, String)> {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(key) {
            if entry.inserted_at.elapsed() < self.ttl {
                entry.access_count += 1;
                return Some((entry.data.clone(), entry.mime_type.clone()));
            } else {
                entries.remove(key);
            }
        }
        None
    }

    /// Insert an entry into the cache.
    pub async fn insert(&self, key: String, data: Vec<u8>, mime_type: String) {
        let mut entries = self.entries.write().await;

        // Evict expired entries
        let now = Instant::now();
        entries.retain(|_, v| now.duration_since(v.inserted_at) < self.ttl);

        // Check size limits
        if entries.len() >= self.max_entries {
            // Evict least recently used
            if let Some(lru_key) = entries
                .iter()
                .min_by_key(|(_, v)| v.access_count)
                .map(|(k, _)| k.clone())
            {
                entries.remove(&lru_key);
            }
        }

        let total_bytes: usize = entries.values().map(|e| e.data.len()).sum();
        if total_bytes + data.len() > self.max_total_bytes {
            // Evict oldest entries until we have space
            let mut sorted: Vec<_> = entries
                .iter()
                .map(|(k, v)| (k.clone(), v.inserted_at))
                .collect();
            sorted.sort_by_key(|(_, t)| *t);

            let mut freed = 0;
            for (key, _) in &sorted {
                if total_bytes + data.len() - freed <= self.max_total_bytes {
                    break;
                }
                if let Some(entry) = entries.remove(key) {
                    freed += entry.data.len();
                }
            }
        }

        entries.insert(
            key,
            CacheEntry {
                data,
                mime_type,
                inserted_at: Instant::now(),
                access_count: 0,
            },
        );
    }

    /// Remove a specific entry.
    pub async fn remove(&self, key: &str) -> bool {
        self.entries.write().await.remove(key).is_some()
    }

    /// Clear all cached entries.
    pub async fn clear(&self) {
        self.entries.write().await.clear();
    }

    /// Get the number of cached entries.
    pub async fn len(&self) -> usize {
        self.entries.read().await.len()
    }

    /// Check if the cache is empty.
    pub async fn is_empty(&self) -> bool {
        self.entries.read().await.is_empty()
    }

    /// Get total cached bytes.
    pub async fn total_bytes(&self) -> usize {
        self.entries
            .read()
            .await
            .values()
            .map(|e| e.data.len())
            .sum()
    }
}

impl Default for MediaCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_insert_and_get() {
        let cache = MediaCache::new();
        cache
            .insert("key1".to_string(), vec![1, 2, 3], "image/png".to_string())
            .await;

        let result = cache.get("key1").await;
        assert!(result.is_some());
        let (data, mime) = result.unwrap();
        assert_eq!(data, vec![1, 2, 3]);
        assert_eq!(mime, "image/png");
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache = MediaCache::new();
        assert!(cache.get("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_cache_remove() {
        let cache = MediaCache::new();
        cache
            .insert("key1".to_string(), vec![1], "text/plain".to_string())
            .await;
        assert!(cache.remove("key1").await);
        assert!(cache.get("key1").await.is_none());
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let cache = MediaCache::new();
        cache
            .insert("a".to_string(), vec![1], "t".to_string())
            .await;
        cache
            .insert("b".to_string(), vec![2], "t".to_string())
            .await;
        assert_eq!(cache.len().await, 2);

        cache.clear().await;
        assert!(cache.is_empty().await);
    }

    #[tokio::test]
    async fn test_cache_eviction_by_count() {
        let cache = MediaCache::new().with_max_entries(2);
        cache
            .insert("a".to_string(), vec![1], "t".to_string())
            .await;
        cache
            .insert("b".to_string(), vec![2], "t".to_string())
            .await;
        cache
            .insert("c".to_string(), vec![3], "t".to_string())
            .await;

        // Should have evicted one entry
        assert!(cache.len().await <= 2);
    }
}
