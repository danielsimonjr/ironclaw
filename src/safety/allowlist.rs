//! General-purpose allowlist/blocklist system for access control.
//!
//! Provides a unified mechanism for controlling access based on sender IDs,
//! IP addresses, domains, or arbitrary string identifiers. Used across
//! channels, tools, and security policies.

use std::collections::HashSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Access control mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AccessMode {
    /// Allow all unless explicitly blocked.
    #[default]
    AllowAll,
    /// Block all unless explicitly allowed.
    AllowList,
    /// Allow all with specific blocks.
    BlockList,
}

/// Result of an access check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessDecision {
    /// Access is allowed.
    Allowed,
    /// Access is denied with reason.
    Denied(String),
}

impl AccessDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }
}

/// A single access control rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessRule {
    /// The pattern to match (exact string, glob, or regex).
    pub pattern: String,
    /// Match type for the pattern.
    pub match_type: MatchType,
    /// Optional reason/description for this rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// When this rule was added.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// How to match a pattern against identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MatchType {
    /// Exact string match (case-insensitive).
    #[default]
    Exact,
    /// Prefix match.
    Prefix,
    /// Suffix match.
    Suffix,
    /// Contains substring.
    Contains,
    /// Glob pattern (supports * and ?).
    Glob,
}

impl AccessRule {
    /// Create a new exact-match rule.
    pub fn exact(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            match_type: MatchType::Exact,
            reason: None,
            added_at: Some(chrono::Utc::now()),
        }
    }

    /// Create a new prefix-match rule.
    pub fn prefix(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            match_type: MatchType::Prefix,
            reason: None,
            added_at: Some(chrono::Utc::now()),
        }
    }

    /// Attach a reason to this rule.
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Check if a value matches this rule.
    pub fn matches(&self, value: &str) -> bool {
        let pattern = self.pattern.to_lowercase();
        let value = value.to_lowercase();

        match self.match_type {
            MatchType::Exact => value == pattern,
            MatchType::Prefix => value.starts_with(&pattern),
            MatchType::Suffix => value.ends_with(&pattern),
            MatchType::Contains => value.contains(&pattern),
            MatchType::Glob => glob_match(&pattern, &value),
        }
    }
}

/// Simple glob matching (supports * and ?).
fn glob_match(pattern: &str, value: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let value: Vec<char> = value.chars().collect();
    glob_match_inner(&pattern, &value, 0, 0)
}

fn glob_match_inner(pattern: &[char], value: &[char], pi: usize, vi: usize) -> bool {
    if pi == pattern.len() && vi == value.len() {
        return true;
    }
    if pi == pattern.len() {
        return false;
    }

    match pattern[pi] {
        '*' => {
            // Try matching zero or more characters
            for i in vi..=value.len() {
                if glob_match_inner(pattern, value, pi + 1, i) {
                    return true;
                }
            }
            false
        }
        '?' => {
            if vi < value.len() {
                glob_match_inner(pattern, value, pi + 1, vi + 1)
            } else {
                false
            }
        }
        c => {
            if vi < value.len() && value[vi] == c {
                glob_match_inner(pattern, value, pi + 1, vi + 1)
            } else {
                false
            }
        }
    }
}

/// Thread-safe allowlist/blocklist manager.
///
/// Supports both allow-list and block-list modes. Thread-safe for
/// concurrent reads/writes via `RwLock`.
pub struct AccessControlList {
    mode: RwLock<AccessMode>,
    allow_rules: RwLock<Vec<AccessRule>>,
    block_rules: RwLock<Vec<AccessRule>>,
    /// Cached allowed set for fast exact-match lookups.
    allow_cache: RwLock<HashSet<String>>,
    /// Cached blocked set for fast exact-match lookups.
    block_cache: RwLock<HashSet<String>>,
}

impl AccessControlList {
    /// Create a new access control list with the specified mode.
    pub fn new(mode: AccessMode) -> Self {
        Self {
            mode: RwLock::new(mode),
            allow_rules: RwLock::new(Vec::new()),
            block_rules: RwLock::new(Vec::new()),
            allow_cache: RwLock::new(HashSet::new()),
            block_cache: RwLock::new(HashSet::new()),
        }
    }

    /// Create a new ACL in allow-all mode (default).
    pub fn allow_all() -> Self {
        Self::new(AccessMode::AllowAll)
    }

    /// Create a new ACL in allowlist mode (deny by default).
    pub fn allowlist() -> Self {
        Self::new(AccessMode::AllowList)
    }

    /// Create a new ACL in blocklist mode (allow by default with exceptions).
    pub fn blocklist() -> Self {
        Self::new(AccessMode::BlockList)
    }

    /// Set the access mode.
    pub async fn set_mode(&self, mode: AccessMode) {
        *self.mode.write().await = mode;
    }

    /// Get the current access mode.
    pub async fn mode(&self) -> AccessMode {
        *self.mode.read().await
    }

    /// Add an allow rule.
    pub async fn allow(&self, rule: AccessRule) {
        if rule.match_type == MatchType::Exact {
            self.allow_cache
                .write()
                .await
                .insert(rule.pattern.to_lowercase());
        }
        self.allow_rules.write().await.push(rule);
    }

    /// Add a block rule.
    pub async fn block(&self, rule: AccessRule) {
        if rule.match_type == MatchType::Exact {
            self.block_cache
                .write()
                .await
                .insert(rule.pattern.to_lowercase());
        }
        self.block_rules.write().await.push(rule);
    }

    /// Remove an allow rule by exact pattern.
    pub async fn remove_allow(&self, pattern: &str) {
        let lower = pattern.to_lowercase();
        self.allow_rules
            .write()
            .await
            .retain(|r| r.pattern.to_lowercase() != lower);
        self.allow_cache.write().await.remove(&lower);
    }

    /// Remove a block rule by exact pattern.
    pub async fn remove_block(&self, pattern: &str) {
        let lower = pattern.to_lowercase();
        self.block_rules
            .write()
            .await
            .retain(|r| r.pattern.to_lowercase() != lower);
        self.block_cache.write().await.remove(&lower);
    }

    /// Check if a value is allowed by the access control list.
    pub async fn check(&self, value: &str) -> AccessDecision {
        let mode = *self.mode.read().await;
        let lower = value.to_lowercase();

        // Always check blocklist first (block takes priority)
        if self.block_cache.read().await.contains(&lower) {
            return AccessDecision::Denied("Blocked by exact match".to_string());
        }
        for rule in self.block_rules.read().await.iter() {
            if rule.match_type != MatchType::Exact && rule.matches(value) {
                return AccessDecision::Denied(
                    rule.reason
                        .clone()
                        .unwrap_or_else(|| format!("Blocked by rule: {}", rule.pattern)),
                );
            }
        }

        match mode {
            AccessMode::AllowAll => AccessDecision::Allowed,
            AccessMode::BlockList => AccessDecision::Allowed, // Not blocked = allowed
            AccessMode::AllowList => {
                // Must be explicitly allowed
                if self.allow_cache.read().await.contains(&lower) {
                    return AccessDecision::Allowed;
                }
                for rule in self.allow_rules.read().await.iter() {
                    if rule.matches(value) {
                        return AccessDecision::Allowed;
                    }
                }
                AccessDecision::Denied("Not on allowlist".to_string())
            }
        }
    }

    /// Get all allow rules.
    pub async fn allow_rules(&self) -> Vec<AccessRule> {
        self.allow_rules.read().await.clone()
    }

    /// Get all block rules.
    pub async fn block_rules(&self) -> Vec<AccessRule> {
        self.block_rules.read().await.clone()
    }
}

impl Default for AccessControlList {
    fn default() -> Self {
        Self::allow_all()
    }
}

/// Convenience type for a shared access control list.
pub type SharedAccessControl = Arc<AccessControlList>;

/// Create a new shared access control list.
pub fn shared_acl(mode: AccessMode) -> SharedAccessControl {
    Arc::new(AccessControlList::new(mode))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_allow_all_mode() {
        let acl = AccessControlList::allow_all();
        assert!(acl.check("anyone").await.is_allowed());
    }

    #[tokio::test]
    async fn test_allowlist_mode() {
        let acl = AccessControlList::allowlist();
        assert!(!acl.check("unknown").await.is_allowed());

        acl.allow(AccessRule::exact("allowed@example.com")).await;
        assert!(acl.check("allowed@example.com").await.is_allowed());
        assert!(!acl.check("other@example.com").await.is_allowed());
    }

    #[tokio::test]
    async fn test_blocklist_mode() {
        let acl = AccessControlList::blocklist();
        assert!(acl.check("anyone").await.is_allowed());

        acl.block(AccessRule::exact("blocked@example.com")).await;
        assert!(!acl.check("blocked@example.com").await.is_allowed());
        assert!(acl.check("other@example.com").await.is_allowed());
    }

    #[tokio::test]
    async fn test_block_takes_priority() {
        let acl = AccessControlList::allowlist();
        acl.allow(AccessRule::exact("user@example.com")).await;
        acl.block(AccessRule::exact("user@example.com")).await;
        assert!(!acl.check("user@example.com").await.is_allowed());
    }

    #[tokio::test]
    async fn test_prefix_match() {
        let acl = AccessControlList::blocklist();
        acl.block(AccessRule::prefix("spam_")).await;
        assert!(!acl.check("spam_user123").await.is_allowed());
        assert!(acl.check("normal_user").await.is_allowed());
    }

    #[tokio::test]
    async fn test_glob_match() {
        let acl = AccessControlList::allowlist();
        acl.allow(AccessRule {
            pattern: "*@example.com".to_string(),
            match_type: MatchType::Glob,
            reason: None,
            added_at: None,
        })
        .await;

        assert!(acl.check("user@example.com").await.is_allowed());
        assert!(acl.check("admin@example.com").await.is_allowed());
        assert!(!acl.check("user@other.com").await.is_allowed());
    }

    #[tokio::test]
    async fn test_case_insensitive() {
        let acl = AccessControlList::allowlist();
        acl.allow(AccessRule::exact("User@Example.COM")).await;
        assert!(acl.check("user@example.com").await.is_allowed());
        assert!(acl.check("USER@EXAMPLE.COM").await.is_allowed());
    }

    #[tokio::test]
    async fn test_remove_rules() {
        let acl = AccessControlList::allowlist();
        acl.allow(AccessRule::exact("user@example.com")).await;
        assert!(acl.check("user@example.com").await.is_allowed());

        acl.remove_allow("user@example.com").await;
        assert!(!acl.check("user@example.com").await.is_allowed());
    }

    #[test]
    fn test_glob_match_fn() {
        assert!(glob_match("hello*", "hello world"));
        assert!(glob_match("*world", "hello world"));
        assert!(glob_match("hello*world", "hello beautiful world"));
        assert!(glob_match("h?llo", "hello"));
        assert!(!glob_match("h?llo", "heello"));
        assert!(glob_match("*", "anything"));
        assert!(glob_match("", ""));
        assert!(!glob_match("hello", "world"));
    }

    #[tokio::test]
    async fn test_set_mode() {
        let acl = AccessControlList::allow_all();
        assert_eq!(acl.mode().await, AccessMode::AllowAll);

        acl.set_mode(AccessMode::AllowList).await;
        assert_eq!(acl.mode().await, AccessMode::AllowList);
    }
}
