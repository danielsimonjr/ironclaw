//! Per-group tool policies for channel-based access control.
//!
//! Allows administrators to define which tools are allowed, denied, or require
//! approval for specific groups within a channel. This enables fine-grained
//! control over tool access based on organizational structure.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// The result of checking whether a tool is allowed for a group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolPermission {
    /// The tool is allowed without restrictions.
    Allowed,
    /// The tool is explicitly denied.
    Denied,
    /// The tool requires explicit approval before execution.
    RequiresApproval,
}

/// A policy that controls which tools a specific group can use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupToolPolicy {
    /// Unique identifier for the group this policy applies to.
    pub group_id: String,
    /// The channel this policy is scoped to (e.g., "telegram", "slack").
    pub channel: String,
    /// Tools explicitly allowed for this group. An empty list means no
    /// explicit allowlist (all tools allowed unless denied).
    pub allowed_tools: Vec<String>,
    /// Tools explicitly denied for this group.
    pub denied_tools: Vec<String>,
    /// Tools that require manual approval before execution for this group.
    pub require_approval_tools: Vec<String>,
    /// Whether this policy is currently active.
    pub enabled: bool,
}

impl GroupToolPolicy {
    /// Create a new enabled group tool policy.
    pub fn new(group_id: impl Into<String>, channel: impl Into<String>) -> Self {
        Self {
            group_id: group_id.into(),
            channel: channel.into(),
            allowed_tools: Vec::new(),
            denied_tools: Vec::new(),
            require_approval_tools: Vec::new(),
            enabled: true,
        }
    }

    /// Set the allowed tools for this policy.
    pub fn with_allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_tools = tools;
        self
    }

    /// Set the denied tools for this policy.
    pub fn with_denied_tools(mut self, tools: Vec<String>) -> Self {
        self.denied_tools = tools;
        self
    }

    /// Set the tools requiring approval for this policy.
    pub fn with_require_approval_tools(mut self, tools: Vec<String>) -> Self {
        self.require_approval_tools = tools;
        self
    }

    /// Set whether this policy is enabled.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Manages per-group tool policies.
///
/// Thread-safe via `Arc<RwLock<...>>` for concurrent access from
/// multiple channels and sessions.
#[derive(Debug, Clone)]
pub struct GroupPolicyManager {
    policies: Arc<RwLock<Vec<GroupToolPolicy>>>,
}

impl GroupPolicyManager {
    /// Create a new empty policy manager.
    pub fn new() -> Self {
        Self {
            policies: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add or replace a policy for a group/channel combination.
    ///
    /// If a policy for the same `(group_id, channel)` pair already exists,
    /// it will be replaced.
    pub async fn add_policy(&self, policy: GroupToolPolicy) {
        let mut policies = self.policies.write().await;

        if let Some(pos) = policies
            .iter()
            .position(|p| p.group_id == policy.group_id && p.channel == policy.channel)
        {
            policies[pos] = policy;
        } else {
            policies.push(policy);
        }
    }

    /// Remove a policy by group ID and channel.
    ///
    /// Returns `true` if a policy was found and removed, `false` otherwise.
    pub async fn remove_policy(&self, group_id: &str, channel: &str) -> bool {
        let mut policies = self.policies.write().await;
        let len_before = policies.len();
        policies.retain(|p| !(p.group_id == group_id && p.channel == channel));
        policies.len() < len_before
    }

    /// Get the policy for a specific group and channel.
    pub async fn get_policy(&self, group_id: &str, channel: &str) -> Option<GroupToolPolicy> {
        let policies = self.policies.read().await;
        policies
            .iter()
            .find(|p| p.group_id == group_id && p.channel == channel)
            .cloned()
    }

    /// Check whether a tool is allowed for a given group and channel.
    ///
    /// Resolution logic:
    /// 1. If no policy exists for the group/channel, the tool is `Allowed`.
    /// 2. If the policy is disabled, the tool is `Allowed`.
    /// 3. If the tool is in `denied_tools`, it is `Denied`.
    /// 4. If the tool is in `require_approval_tools`, it `RequiresApproval`.
    /// 5. If `allowed_tools` is non-empty and the tool is NOT in it, it is `Denied`.
    /// 6. Otherwise, the tool is `Allowed`.
    pub async fn check_tool_allowed(
        &self,
        group_id: &str,
        channel: &str,
        tool_name: &str,
    ) -> ToolPermission {
        let policies = self.policies.read().await;

        let policy = match policies
            .iter()
            .find(|p| p.group_id == group_id && p.channel == channel)
        {
            Some(p) => p,
            None => return ToolPermission::Allowed,
        };

        if !policy.enabled {
            return ToolPermission::Allowed;
        }

        // Deny list takes highest priority.
        if policy.denied_tools.iter().any(|t| t == tool_name) {
            return ToolPermission::Denied;
        }

        // Approval requirement comes next.
        if policy.require_approval_tools.iter().any(|t| t == tool_name) {
            return ToolPermission::RequiresApproval;
        }

        // If an allowlist is specified, the tool must be in it.
        if !policy.allowed_tools.is_empty() && !policy.allowed_tools.iter().any(|t| t == tool_name)
        {
            return ToolPermission::Denied;
        }

        ToolPermission::Allowed
    }

    /// List all policies, optionally filtered by channel.
    pub async fn list_policies(&self, channel: Option<&str>) -> Vec<GroupToolPolicy> {
        let policies = self.policies.read().await;
        match channel {
            Some(ch) => policies
                .iter()
                .filter(|p| p.channel == ch)
                .cloned()
                .collect(),
            None => policies.clone(),
        }
    }
}

impl Default for GroupPolicyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_and_get_policy() {
        let manager = GroupPolicyManager::new();
        let policy = GroupToolPolicy::new("admins", "slack")
            .with_allowed_tools(vec!["shell".to_string(), "http".to_string()]);

        manager.add_policy(policy).await;

        let retrieved = manager.get_policy("admins", "slack").await;
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.group_id, "admins");
        assert_eq!(retrieved.channel, "slack");
        assert_eq!(retrieved.allowed_tools.len(), 2);
    }

    #[tokio::test]
    async fn test_get_nonexistent_policy() {
        let manager = GroupPolicyManager::new();
        assert!(manager.get_policy("ghost", "slack").await.is_none());
    }

    #[tokio::test]
    async fn test_add_replaces_existing() {
        let manager = GroupPolicyManager::new();

        let policy1 =
            GroupToolPolicy::new("admins", "slack").with_allowed_tools(vec!["shell".to_string()]);
        manager.add_policy(policy1).await;

        let policy2 = GroupToolPolicy::new("admins", "slack")
            .with_allowed_tools(vec!["http".to_string(), "echo".to_string()]);
        manager.add_policy(policy2).await;

        let policies = manager.list_policies(None).await;
        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].allowed_tools.len(), 2);
        assert!(policies[0].allowed_tools.contains(&"http".to_string()));
    }

    #[tokio::test]
    async fn test_remove_policy() {
        let manager = GroupPolicyManager::new();
        let policy = GroupToolPolicy::new("admins", "slack");
        manager.add_policy(policy).await;

        assert!(manager.remove_policy("admins", "slack").await);
        assert!(manager.get_policy("admins", "slack").await.is_none());
    }

    #[tokio::test]
    async fn test_remove_nonexistent_returns_false() {
        let manager = GroupPolicyManager::new();
        assert!(!manager.remove_policy("ghost", "slack").await);
    }

    #[tokio::test]
    async fn test_check_tool_no_policy_allows() {
        let manager = GroupPolicyManager::new();
        let result = manager.check_tool_allowed("users", "slack", "shell").await;
        assert_eq!(result, ToolPermission::Allowed);
    }

    #[tokio::test]
    async fn test_check_tool_disabled_policy_allows() {
        let manager = GroupPolicyManager::new();
        let policy = GroupToolPolicy::new("users", "slack")
            .with_denied_tools(vec!["shell".to_string()])
            .with_enabled(false);
        manager.add_policy(policy).await;

        let result = manager.check_tool_allowed("users", "slack", "shell").await;
        assert_eq!(result, ToolPermission::Allowed);
    }

    #[tokio::test]
    async fn test_check_tool_denied() {
        let manager = GroupPolicyManager::new();
        let policy = GroupToolPolicy::new("interns", "slack")
            .with_denied_tools(vec!["shell".to_string(), "write_file".to_string()]);
        manager.add_policy(policy).await;

        assert_eq!(
            manager
                .check_tool_allowed("interns", "slack", "shell")
                .await,
            ToolPermission::Denied
        );
        assert_eq!(
            manager.check_tool_allowed("interns", "slack", "echo").await,
            ToolPermission::Allowed
        );
    }

    #[tokio::test]
    async fn test_check_tool_requires_approval() {
        let manager = GroupPolicyManager::new();
        let policy = GroupToolPolicy::new("devs", "telegram")
            .with_require_approval_tools(vec!["shell".to_string(), "http".to_string()]);
        manager.add_policy(policy).await;

        assert_eq!(
            manager
                .check_tool_allowed("devs", "telegram", "shell")
                .await,
            ToolPermission::RequiresApproval
        );
        assert_eq!(
            manager.check_tool_allowed("devs", "telegram", "echo").await,
            ToolPermission::Allowed
        );
    }

    #[tokio::test]
    async fn test_deny_takes_priority_over_approval() {
        let manager = GroupPolicyManager::new();
        let policy = GroupToolPolicy::new("mixed", "slack")
            .with_denied_tools(vec!["shell".to_string()])
            .with_require_approval_tools(vec!["shell".to_string()]);
        manager.add_policy(policy).await;

        // Denied takes precedence over requires_approval.
        assert_eq!(
            manager.check_tool_allowed("mixed", "slack", "shell").await,
            ToolPermission::Denied
        );
    }

    #[tokio::test]
    async fn test_allowlist_denies_unlisted_tools() {
        let manager = GroupPolicyManager::new();
        let policy = GroupToolPolicy::new("restricted", "slack")
            .with_allowed_tools(vec!["echo".to_string(), "time".to_string()]);
        manager.add_policy(policy).await;

        assert_eq!(
            manager
                .check_tool_allowed("restricted", "slack", "echo")
                .await,
            ToolPermission::Allowed
        );
        assert_eq!(
            manager
                .check_tool_allowed("restricted", "slack", "shell")
                .await,
            ToolPermission::Denied
        );
    }

    #[tokio::test]
    async fn test_list_policies_all() {
        let manager = GroupPolicyManager::new();
        manager
            .add_policy(GroupToolPolicy::new("admins", "slack"))
            .await;
        manager
            .add_policy(GroupToolPolicy::new("admins", "telegram"))
            .await;
        manager
            .add_policy(GroupToolPolicy::new("users", "slack"))
            .await;

        let all = manager.list_policies(None).await;
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn test_list_policies_filtered_by_channel() {
        let manager = GroupPolicyManager::new();
        manager
            .add_policy(GroupToolPolicy::new("admins", "slack"))
            .await;
        manager
            .add_policy(GroupToolPolicy::new("admins", "telegram"))
            .await;
        manager
            .add_policy(GroupToolPolicy::new("users", "slack"))
            .await;

        let slack_policies = manager.list_policies(Some("slack")).await;
        assert_eq!(slack_policies.len(), 2);
        assert!(slack_policies.iter().all(|p| p.channel == "slack"));

        let telegram_policies = manager.list_policies(Some("telegram")).await;
        assert_eq!(telegram_policies.len(), 1);
    }

    #[tokio::test]
    async fn test_different_channels_independent() {
        let manager = GroupPolicyManager::new();

        let slack_policy =
            GroupToolPolicy::new("admins", "slack").with_denied_tools(vec!["shell".to_string()]);
        let telegram_policy = GroupToolPolicy::new("admins", "telegram")
            .with_allowed_tools(vec!["shell".to_string()]);

        manager.add_policy(slack_policy).await;
        manager.add_policy(telegram_policy).await;

        // Same group, same tool, different channels, different results.
        assert_eq!(
            manager.check_tool_allowed("admins", "slack", "shell").await,
            ToolPermission::Denied
        );
        assert_eq!(
            manager
                .check_tool_allowed("admins", "telegram", "shell")
                .await,
            ToolPermission::Allowed
        );
    }
}
