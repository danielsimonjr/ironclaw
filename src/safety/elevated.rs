//! Elevated mode for privileged execution.
//!
//! When enabled, elevated mode bypasses certain tool approval requirements
//! while maintaining safety guardrails. It requires explicit user opt-in
//! and is tracked for audit purposes.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Elevated mode state for a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElevatedMode {
    /// Whether elevated mode is currently active.
    enabled: bool,
    /// When elevated mode was activated.
    activated_at: Option<DateTime<Utc>>,
    /// Who activated it.
    activated_by: Option<String>,
    /// How long elevated mode lasts (in seconds). 0 = until deactivated.
    duration_secs: u64,
    /// Tools that are always gated even in elevated mode.
    always_approve: Vec<String>,
}

impl ElevatedMode {
    /// Create a new elevated mode (disabled by default).
    pub fn new() -> Self {
        Self {
            enabled: false,
            activated_at: None,
            activated_by: None,
            duration_secs: 3600, // 1 hour default
            always_approve: vec![
                "build_software".to_string(), // Always require approval for builds
            ],
        }
    }

    /// Activate elevated mode.
    pub fn activate(&mut self, user_id: &str) {
        self.enabled = true;
        self.activated_at = Some(Utc::now());
        self.activated_by = Some(user_id.to_string());
        tracing::warn!(
            user = user_id,
            duration = self.duration_secs,
            "Elevated mode activated"
        );
    }

    /// Deactivate elevated mode.
    pub fn deactivate(&mut self) {
        self.enabled = false;
        self.activated_at = None;
        self.activated_by = None;
        tracing::info!("Elevated mode deactivated");
    }

    /// Check if elevated mode is currently active (respects duration).
    pub fn is_active(&self) -> bool {
        if !self.enabled {
            return false;
        }

        if self.duration_secs == 0 {
            return true; // No expiry
        }

        if let Some(activated_at) = self.activated_at {
            let elapsed = Utc::now().signed_duration_since(activated_at).num_seconds() as u64;
            elapsed < self.duration_secs
        } else {
            false
        }
    }

    /// Check if a tool should bypass approval in elevated mode.
    pub fn should_bypass_approval(&self, tool_name: &str) -> bool {
        if !self.is_active() {
            return false;
        }

        // Some tools always require approval even in elevated mode
        !self.always_approve.iter().any(|t| t == tool_name)
    }

    /// Set the duration for elevated mode.
    pub fn set_duration(&mut self, secs: u64) {
        self.duration_secs = secs;
    }

    /// Add a tool to the always-approve list.
    pub fn add_always_approve(&mut self, tool_name: String) {
        if !self.always_approve.contains(&tool_name) {
            self.always_approve.push(tool_name);
        }
    }
}

impl Default for ElevatedMode {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe wrapper for elevated mode.
pub struct ElevatedModeGuard {
    active: Arc<AtomicBool>,
}

impl ElevatedModeGuard {
    pub fn new() -> Self {
        Self {
            active: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn activate(&self) {
        self.active.store(true, Ordering::SeqCst);
    }

    pub fn deactivate(&self) {
        self.active.store(false, Ordering::SeqCst);
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }
}

impl Default for ElevatedModeGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elevated_mode_default_inactive() {
        let mode = ElevatedMode::new();
        assert!(!mode.is_active());
        assert!(!mode.should_bypass_approval("shell"));
    }

    #[test]
    fn test_activate_deactivate() {
        let mut mode = ElevatedMode::new();
        mode.activate("user1");
        assert!(mode.is_active());
        assert!(mode.should_bypass_approval("shell"));

        mode.deactivate();
        assert!(!mode.is_active());
    }

    #[test]
    fn test_always_approve_tools() {
        let mut mode = ElevatedMode::new();
        mode.activate("user1");

        // build_software is in always_approve by default
        assert!(!mode.should_bypass_approval("build_software"));
        // shell is not in always_approve
        assert!(mode.should_bypass_approval("shell"));
    }

    #[test]
    fn test_expired_elevated_mode() {
        let mut mode = ElevatedMode::new();
        mode.set_duration(0); // No expiry
        mode.activate("user1");
        assert!(mode.is_active());
    }
}
