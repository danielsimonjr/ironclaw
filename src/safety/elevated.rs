//! Elevated mode for privileged execution.
//!
//! When enabled, elevated mode bypasses certain tool approval requirements
//! while maintaining safety guardrails. It requires explicit user opt-in
//! and is tracked for audit purposes.
//!
//! # Security
//!
//! - Elevated mode is bound to a specific session ID (A-2).
//! - Duration must be > 0 (minimum 60s) to prevent permanent elevation (A-3).
//! - Activation requires both a user ID and session ID for audit tracking.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Minimum allowed duration for elevated mode (60 seconds).
const MIN_DURATION_SECS: u64 = 60;
/// Maximum allowed duration for elevated mode (8 hours).
const MAX_DURATION_SECS: u64 = 28800;

/// Elevated mode state for a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElevatedMode {
    /// Whether elevated mode is currently active.
    enabled: bool,
    /// When elevated mode was activated.
    activated_at: Option<DateTime<Utc>>,
    /// Who activated it.
    activated_by: Option<String>,
    /// Session this elevation is bound to (A-2).
    session_id: Option<String>,
    /// How long elevated mode lasts (in seconds). Must be >= MIN_DURATION_SECS (A-3).
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
            session_id: None,
            duration_secs: 3600, // 1 hour default
            always_approve: vec![
                "build_software".to_string(), // Always require approval for builds
            ],
        }
    }

    /// Activate elevated mode bound to a specific session (A-2).
    ///
    /// Both `user_id` and `session_id` are required. The elevation is
    /// scoped to the given session — `is_active_for_session()` must be
    /// used to check eligibility.
    pub fn activate(&mut self, user_id: &str, session_id: &str) {
        self.enabled = true;
        self.activated_at = Some(Utc::now());
        self.activated_by = Some(user_id.to_string());
        self.session_id = Some(session_id.to_string());
        tracing::warn!(
            user = user_id,
            session = session_id,
            duration = self.duration_secs,
            "Elevated mode activated"
        );
    }

    /// Deactivate elevated mode.
    pub fn deactivate(&mut self) {
        let session = self.session_id.take();
        self.enabled = false;
        self.activated_at = None;
        self.activated_by = None;
        tracing::info!(session = ?session, "Elevated mode deactivated");
    }

    /// Check if elevated mode is currently active (respects duration).
    pub fn is_active(&self) -> bool {
        if !self.enabled {
            return false;
        }

        // Duration == 0 is no longer allowed (A-3); treat as expired
        if self.duration_secs == 0 {
            return false;
        }

        if let Some(activated_at) = self.activated_at {
            let elapsed = Utc::now().signed_duration_since(activated_at).num_seconds() as u64;
            elapsed < self.duration_secs
        } else {
            false
        }
    }

    /// Check if elevated mode is active for a specific session (A-2).
    ///
    /// Returns `false` if:
    /// - Elevated mode is not active
    /// - The session ID does not match
    pub fn is_active_for_session(&self, session_id: &str) -> bool {
        self.is_active() && self.session_id.as_deref().is_some_and(|s| s == session_id)
    }

    /// Check if a tool should bypass approval in elevated mode for a session.
    pub fn should_bypass_approval(&self, tool_name: &str) -> bool {
        if !self.is_active() {
            return false;
        }

        // Some tools always require approval even in elevated mode
        !self.always_approve.iter().any(|t| t == tool_name)
    }

    /// Check if a tool should bypass approval for a specific session (A-2).
    pub fn should_bypass_approval_for_session(&self, tool_name: &str, session_id: &str) -> bool {
        if !self.is_active_for_session(session_id) {
            return false;
        }

        !self.always_approve.iter().any(|t| t == tool_name)
    }

    /// Set the duration for elevated mode (A-3).
    ///
    /// Clamps to `[MIN_DURATION_SECS, MAX_DURATION_SECS]`. A value of 0
    /// is rejected and clamped to `MIN_DURATION_SECS`.
    pub fn set_duration(&mut self, secs: u64) {
        self.duration_secs = secs.clamp(MIN_DURATION_SECS, MAX_DURATION_SECS);
        if secs != self.duration_secs {
            tracing::warn!(
                requested = secs,
                actual = self.duration_secs,
                "Elevated mode duration clamped to safe range"
            );
        }
    }

    /// Add a tool to the always-approve list.
    pub fn add_always_approve(&mut self, tool_name: String) {
        if !self.always_approve.contains(&tool_name) {
            self.always_approve.push(tool_name);
        }
    }

    /// Get the session ID this elevation is bound to.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
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
        mode.activate("user1", "session-abc");
        assert!(mode.is_active());
        assert!(mode.should_bypass_approval("shell"));
        assert_eq!(mode.session_id(), Some("session-abc"));

        mode.deactivate();
        assert!(!mode.is_active());
        assert!(mode.session_id().is_none());
    }

    #[test]
    fn test_session_binding() {
        let mut mode = ElevatedMode::new();
        mode.activate("user1", "session-abc");

        // Active for the correct session
        assert!(mode.is_active_for_session("session-abc"));
        // Not active for a different session (A-2)
        assert!(!mode.is_active_for_session("session-xyz"));

        assert!(mode.should_bypass_approval_for_session("shell", "session-abc"));
        assert!(!mode.should_bypass_approval_for_session("shell", "session-xyz"));
    }

    #[test]
    fn test_always_approve_tools() {
        let mut mode = ElevatedMode::new();
        mode.activate("user1", "session-abc");

        // build_software is in always_approve by default
        assert!(!mode.should_bypass_approval("build_software"));
        // shell is not in always_approve
        assert!(mode.should_bypass_approval("shell"));
    }

    #[test]
    fn test_zero_duration_not_allowed() {
        let mut mode = ElevatedMode::new();
        // A-3: duration == 0 is clamped to MIN_DURATION_SECS
        mode.set_duration(0);
        assert_eq!(mode.duration_secs, MIN_DURATION_SECS);

        mode.activate("user1", "session-abc");
        // Should still be active since clamped to MIN_DURATION_SECS
        assert!(mode.is_active());
    }

    #[test]
    fn test_duration_clamping() {
        let mut mode = ElevatedMode::new();
        // Below minimum
        mode.set_duration(10);
        assert_eq!(mode.duration_secs, MIN_DURATION_SECS);

        // Above maximum
        mode.set_duration(999999);
        assert_eq!(mode.duration_secs, MAX_DURATION_SECS);

        // Valid value
        mode.set_duration(1800);
        assert_eq!(mode.duration_secs, 1800);
    }

    #[test]
    fn test_guard() {
        let guard = ElevatedModeGuard::new();
        assert!(!guard.is_active());

        guard.activate();
        assert!(guard.is_active());

        guard.deactivate();
        assert!(!guard.is_active());
    }
}
