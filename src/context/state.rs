//! Job state machine.

use std::time::Duration;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// State of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    /// Job is waiting to be started.
    Pending,
    /// Job is currently being worked on.
    InProgress,
    /// Job work is complete, awaiting submission.
    Completed,
    /// Job has been submitted for review.
    Submitted,
    /// Job was accepted/paid.
    Accepted,
    /// Job failed and cannot be completed.
    Failed,
    /// Job is stuck and needs repair.
    Stuck,
    /// Job was cancelled.
    Cancelled,
}

impl JobState {
    /// Check if this state allows transitioning to another state.
    pub fn can_transition_to(&self, target: JobState) -> bool {
        use JobState::*;

        matches!(
            (self, target),
            // From Pending
            (Pending, InProgress) | (Pending, Cancelled) |
            // From InProgress
            (InProgress, Completed) | (InProgress, Failed) |
            (InProgress, Stuck) | (InProgress, Cancelled) |
            // From Completed
            (Completed, Submitted) | (Completed, Failed) |
            // From Submitted
            (Submitted, Accepted) | (Submitted, Failed) |
            // From Stuck (can recover or fail)
            (Stuck, InProgress) | (Stuck, Failed) | (Stuck, Cancelled)
        )
    }

    /// Check if this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Accepted | Self::Failed | Self::Cancelled)
    }

    /// Check if the job is active (not terminal).
    pub fn is_active(&self) -> bool {
        !self.is_terminal()
    }
}

impl std::fmt::Display for JobState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Submitted => "submitted",
            Self::Accepted => "accepted",
            Self::Failed => "failed",
            Self::Stuck => "stuck",
            Self::Cancelled => "cancelled",
        };
        write!(f, "{}", s)
    }
}

/// A state transition event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    /// Previous state.
    pub from: JobState,
    /// New state.
    pub to: JobState,
    /// When the transition occurred.
    pub timestamp: DateTime<Utc>,
    /// Reason for the transition.
    pub reason: Option<String>,
}

/// Context for a running job.
#[derive(Debug, Clone, Serialize)]
pub struct JobContext {
    /// Unique job ID.
    pub job_id: Uuid,
    /// Current state.
    pub state: JobState,
    /// User ID that owns this job (for workspace scoping).
    pub user_id: String,
    /// Conversation ID if linked to a conversation.
    pub conversation_id: Option<Uuid>,
    /// Job title.
    pub title: String,
    /// Job description.
    pub description: String,
    /// Job category.
    pub category: Option<String>,
    /// Budget amount (if from marketplace).
    pub budget: Option<Decimal>,
    /// Budget token (e.g., "NEAR", "USD").
    pub budget_token: Option<String>,
    /// Our bid amount.
    pub bid_amount: Option<Decimal>,
    /// Estimated cost to complete.
    pub estimated_cost: Option<Decimal>,
    /// Estimated time to complete.
    pub estimated_duration: Option<Duration>,
    /// Actual cost so far.
    pub actual_cost: Decimal,
    /// Total tokens consumed by LLM calls in this job.
    pub total_tokens_used: u64,
    /// Maximum tokens allowed per job (0 = unlimited).
    pub max_tokens: u64,
    /// When the job was created.
    pub created_at: DateTime<Utc>,
    /// When the job was started.
    pub started_at: Option<DateTime<Utc>>,
    /// When the job was completed.
    pub completed_at: Option<DateTime<Utc>>,
    /// Number of repair attempts.
    pub repair_attempts: u32,
    /// State transition history.
    pub transitions: Vec<StateTransition>,
    /// Metadata.
    pub metadata: serde_json::Value,
}

impl JobContext {
    /// Create a new job context.
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self::with_user("default", title, description)
    }

    /// Create a new job context with a specific user ID.
    pub fn with_user(
        user_id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            job_id: Uuid::new_v4(),
            state: JobState::Pending,
            user_id: user_id.into(),
            conversation_id: None,
            title: title.into(),
            description: description.into(),
            category: None,
            budget: None,
            budget_token: None,
            bid_amount: None,
            estimated_cost: None,
            estimated_duration: None,
            actual_cost: Decimal::ZERO,
            total_tokens_used: 0,
            max_tokens: 0,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            repair_attempts: 0,
            transitions: Vec::new(),
            metadata: serde_json::Value::Null,
        }
    }

    /// Transition to a new state.
    pub fn transition_to(
        &mut self,
        new_state: JobState,
        reason: Option<String>,
    ) -> Result<(), String> {
        if !self.state.can_transition_to(new_state) {
            return Err(format!(
                "Cannot transition from {} to {}",
                self.state, new_state
            ));
        }

        let transition = StateTransition {
            from: self.state,
            to: new_state,
            timestamp: Utc::now(),
            reason,
        };

        self.transitions.push(transition);

        // Cap transition history to prevent unbounded memory growth
        const MAX_TRANSITIONS: usize = 200;
        if self.transitions.len() > MAX_TRANSITIONS {
            let drain_count = self.transitions.len() - MAX_TRANSITIONS;
            self.transitions.drain(..drain_count);
        }

        self.state = new_state;

        // Update timestamps
        match new_state {
            JobState::InProgress if self.started_at.is_none() => {
                self.started_at = Some(Utc::now());
            }
            JobState::Completed | JobState::Accepted | JobState::Failed | JobState::Cancelled => {
                self.completed_at = Some(Utc::now());
            }
            _ => {}
        }

        Ok(())
    }

    /// Add to the actual cost.
    pub fn add_cost(&mut self, cost: Decimal) {
        self.actual_cost += cost;
    }

    /// Record token usage from an LLM call. Returns an error string if the
    /// token budget has been exceeded after this addition.
    pub fn add_tokens(&mut self, tokens: u64) -> Result<(), String> {
        self.total_tokens_used += tokens;
        if self.max_tokens > 0 && self.total_tokens_used > self.max_tokens {
            Err(format!(
                "Token budget exceeded: used {} of {} allowed tokens",
                self.total_tokens_used, self.max_tokens
            ))
        } else {
            Ok(())
        }
    }

    /// Check whether the monetary budget has been exceeded.
    pub fn budget_exceeded(&self) -> bool {
        if let Some(ref budget) = self.budget {
            self.actual_cost > *budget
        } else {
            false
        }
    }

    /// Get the duration since the job started.
    pub fn elapsed(&self) -> Option<Duration> {
        self.started_at.map(|start| {
            let end = self.completed_at.unwrap_or_else(Utc::now);
            let duration = end.signed_duration_since(start);
            Duration::from_secs(duration.num_seconds().max(0) as u64)
        })
    }

    /// Mark the job as stuck.
    pub fn mark_stuck(&mut self, reason: impl Into<String>) -> Result<(), String> {
        self.transition_to(JobState::Stuck, Some(reason.into()))
    }

    /// Attempt to recover from stuck state.
    pub fn attempt_recovery(&mut self) -> Result<(), String> {
        if self.state != JobState::Stuck {
            return Err("Job is not stuck".to_string());
        }
        self.repair_attempts += 1;
        self.transition_to(JobState::InProgress, Some("Recovery attempt".to_string()))
    }
}

impl Default for JobContext {
    fn default() -> Self {
        Self::with_user("default", "Untitled", "No description")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_transitions() {
        assert!(JobState::Pending.can_transition_to(JobState::InProgress));
        assert!(JobState::InProgress.can_transition_to(JobState::Completed));
        assert!(!JobState::Completed.can_transition_to(JobState::Pending));
        assert!(!JobState::Accepted.can_transition_to(JobState::InProgress));
    }

    #[test]
    fn test_terminal_states() {
        assert!(JobState::Accepted.is_terminal());
        assert!(JobState::Failed.is_terminal());
        assert!(JobState::Cancelled.is_terminal());
        assert!(!JobState::InProgress.is_terminal());
    }

    #[test]
    fn test_job_context_transitions() {
        let mut ctx = JobContext::new("Test", "Test job");
        assert_eq!(ctx.state, JobState::Pending);

        ctx.transition_to(JobState::InProgress, None).unwrap();
        assert_eq!(ctx.state, JobState::InProgress);
        assert!(ctx.started_at.is_some());

        ctx.transition_to(JobState::Completed, Some("Done".to_string()))
            .unwrap();
        assert_eq!(ctx.state, JobState::Completed);
    }

    #[test]
    fn test_transition_history_capped() {
        let mut ctx = JobContext::new("Test", "Transition cap test");
        // Cycle through Pending -> InProgress -> Stuck -> InProgress -> Stuck ...
        ctx.transition_to(JobState::InProgress, None).unwrap();
        for i in 0..250 {
            ctx.mark_stuck(format!("stuck {}", i)).unwrap();
            ctx.attempt_recovery().unwrap();
        }
        // 1 initial + 250*2 = 501 transitions, should be capped at 200
        assert!(
            ctx.transitions.len() <= 200,
            "transitions should be capped at 200, got {}",
            ctx.transitions.len()
        );
    }

    #[test]
    fn test_add_tokens_enforces_budget() {
        let mut ctx = JobContext::new("Test", "Budget test");
        ctx.max_tokens = 1000;
        assert!(ctx.add_tokens(500).is_ok());
        assert_eq!(ctx.total_tokens_used, 500);
        assert!(ctx.add_tokens(600).is_err());
        assert_eq!(ctx.total_tokens_used, 1100); // tokens still recorded
    }

    #[test]
    fn test_add_tokens_unlimited() {
        let mut ctx = JobContext::new("Test", "No budget");
        // max_tokens = 0 means unlimited
        assert!(ctx.add_tokens(1_000_000).is_ok());
    }

    #[test]
    fn test_budget_exceeded() {
        let mut ctx = JobContext::new("Test", "Money test");
        ctx.budget = Some(Decimal::new(100, 0)); // $100
        assert!(!ctx.budget_exceeded());
        ctx.add_cost(Decimal::new(50, 0));
        assert!(!ctx.budget_exceeded());
        ctx.add_cost(Decimal::new(60, 0));
        assert!(ctx.budget_exceeded());
    }

    #[test]
    fn test_budget_exceeded_none() {
        let ctx = JobContext::new("Test", "No budget");
        assert!(!ctx.budget_exceeded()); // No budget = never exceeded
    }

    #[test]
    fn test_stuck_recovery() {
        let mut ctx = JobContext::new("Test", "Test job");
        ctx.transition_to(JobState::InProgress, None).unwrap();
        ctx.mark_stuck("Timed out").unwrap();
        assert_eq!(ctx.state, JobState::Stuck);

        ctx.attempt_recovery().unwrap();
        assert_eq!(ctx.state, JobState::InProgress);
        assert_eq!(ctx.repair_attempts, 1);
    }

    // ==================== Comprehensive state transition matrix ====================

    #[test]
    fn test_all_valid_transitions() {
        use JobState::*;

        let valid = [
            (Pending, InProgress),
            (Pending, Cancelled),
            (InProgress, Completed),
            (InProgress, Failed),
            (InProgress, Stuck),
            (InProgress, Cancelled),
            (Completed, Submitted),
            (Completed, Failed),
            (Submitted, Accepted),
            (Submitted, Failed),
            (Stuck, InProgress),
            (Stuck, Failed),
            (Stuck, Cancelled),
        ];

        for (from, to) in &valid {
            assert!(
                from.can_transition_to(*to),
                "Expected valid transition: {:?} -> {:?}",
                from,
                to
            );
        }
    }

    #[test]
    fn test_invalid_transitions_from_pending() {
        use JobState::*;
        let invalid_targets = [Completed, Submitted, Accepted, Failed, Stuck, Pending];
        for target in &invalid_targets {
            assert!(
                !Pending.can_transition_to(*target),
                "Pending -> {:?} should be invalid",
                target
            );
        }
    }

    #[test]
    fn test_invalid_transitions_from_terminal_states() {
        use JobState::*;
        let terminal = [Accepted, Failed, Cancelled];
        let all_states = [
            Pending, InProgress, Completed, Submitted, Accepted, Failed, Stuck, Cancelled,
        ];

        for from in &terminal {
            for to in &all_states {
                assert!(
                    !from.can_transition_to(*to),
                    "Terminal state {:?} -> {:?} should be invalid",
                    from,
                    to
                );
            }
        }
    }

    #[test]
    fn test_invalid_skip_transitions() {
        use JobState::*;
        // Cannot skip InProgress
        assert!(!Pending.can_transition_to(Completed));
        // Cannot go backwards
        assert!(!Completed.can_transition_to(InProgress));
        assert!(!Submitted.can_transition_to(Completed));
        // Cannot skip Submitted
        assert!(!Completed.can_transition_to(Accepted));
    }

    // ==================== Terminal/active state tests ====================

    #[test]
    fn test_all_terminal_states() {
        assert!(JobState::Accepted.is_terminal());
        assert!(JobState::Failed.is_terminal());
        assert!(JobState::Cancelled.is_terminal());
    }

    #[test]
    fn test_all_active_states() {
        assert!(JobState::Pending.is_active());
        assert!(JobState::InProgress.is_active());
        assert!(JobState::Completed.is_active());
        assert!(JobState::Submitted.is_active());
        assert!(JobState::Stuck.is_active());
    }

    #[test]
    fn test_terminal_and_active_are_complementary() {
        let all_states = [
            JobState::Pending,
            JobState::InProgress,
            JobState::Completed,
            JobState::Submitted,
            JobState::Accepted,
            JobState::Failed,
            JobState::Stuck,
            JobState::Cancelled,
        ];

        for state in &all_states {
            assert_ne!(
                state.is_terminal(),
                state.is_active(),
                "is_terminal and is_active must be complementary for {:?}",
                state
            );
        }
    }

    // ==================== Display roundtrip ====================

    #[test]
    fn test_job_state_display() {
        assert_eq!(JobState::Pending.to_string(), "pending");
        assert_eq!(JobState::InProgress.to_string(), "in_progress");
        assert_eq!(JobState::Completed.to_string(), "completed");
        assert_eq!(JobState::Submitted.to_string(), "submitted");
        assert_eq!(JobState::Accepted.to_string(), "accepted");
        assert_eq!(JobState::Failed.to_string(), "failed");
        assert_eq!(JobState::Stuck.to_string(), "stuck");
        assert_eq!(JobState::Cancelled.to_string(), "cancelled");
    }

    // ==================== JobContext transition validation ====================

    #[test]
    fn test_transition_to_invalid_rejected() {
        let mut ctx = JobContext::new("Test", "Test job");
        // Pending -> Completed should fail
        let result = ctx.transition_to(JobState::Completed, None);
        assert!(result.is_err());
        assert_eq!(ctx.state, JobState::Pending); // state unchanged
    }

    #[test]
    fn test_full_happy_path() {
        let mut ctx = JobContext::new("Test", "Happy path");
        ctx.transition_to(JobState::InProgress, None).unwrap();
        assert!(ctx.started_at.is_some());

        ctx.transition_to(JobState::Completed, None).unwrap();
        assert!(ctx.completed_at.is_some());

        ctx.transition_to(JobState::Submitted, None).unwrap();
        ctx.transition_to(JobState::Accepted, None).unwrap();
        assert!(ctx.state.is_terminal());
    }

    #[test]
    fn test_completed_at_set_on_terminal_states() {
        let mut ctx = JobContext::new("Test", "Terminal timestamps");
        ctx.transition_to(JobState::InProgress, None).unwrap();
        assert!(ctx.completed_at.is_none());

        ctx.transition_to(JobState::Failed, Some("error".to_string()))
            .unwrap();
        assert!(ctx.completed_at.is_some());
    }

    #[test]
    fn test_started_at_not_overwritten_on_recovery() {
        let mut ctx = JobContext::new("Test", "Recovery timestamp");
        ctx.transition_to(JobState::InProgress, None).unwrap();
        let first_start = ctx.started_at;

        ctx.mark_stuck("stuck").unwrap();
        ctx.attempt_recovery().unwrap();
        // started_at should not be overwritten
        assert_eq!(ctx.started_at, first_start);
    }

    #[test]
    fn test_attempt_recovery_not_stuck() {
        let mut ctx = JobContext::new("Test", "Not stuck");
        ctx.transition_to(JobState::InProgress, None).unwrap();
        let result = ctx.attempt_recovery();
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_recoveries_increment_repair_attempts() {
        let mut ctx = JobContext::new("Test", "Multi recovery");
        ctx.transition_to(JobState::InProgress, None).unwrap();
        for i in 1..=5 {
            ctx.mark_stuck(format!("stuck {i}")).unwrap();
            ctx.attempt_recovery().unwrap();
            assert_eq!(ctx.repair_attempts, i);
        }
    }

    #[test]
    fn test_elapsed_none_before_start() {
        let ctx = JobContext::new("Test", "No start");
        assert!(ctx.elapsed().is_none());
    }

    #[test]
    fn test_elapsed_some_after_start() {
        let mut ctx = JobContext::new("Test", "Elapsed test");
        ctx.transition_to(JobState::InProgress, None).unwrap();
        let elapsed = ctx.elapsed();
        assert!(elapsed.is_some());
        // Should be very small since we just started
        assert!(elapsed.unwrap() < Duration::from_secs(5));
    }

    #[test]
    fn test_transition_reason_recorded() {
        let mut ctx = JobContext::new("Test", "Reason test");
        ctx.transition_to(JobState::InProgress, Some("Starting work".to_string()))
            .unwrap();
        assert_eq!(ctx.transitions.len(), 1);
        assert_eq!(ctx.transitions[0].reason, Some("Starting work".to_string()));
        assert_eq!(ctx.transitions[0].from, JobState::Pending);
        assert_eq!(ctx.transitions[0].to, JobState::InProgress);
    }

    #[test]
    fn test_new_context_defaults() {
        let ctx = JobContext::new("Title", "Description");
        assert_eq!(ctx.state, JobState::Pending);
        assert_eq!(ctx.user_id, "default");
        assert_eq!(ctx.title, "Title");
        assert_eq!(ctx.description, "Description");
        assert!(ctx.started_at.is_none());
        assert!(ctx.completed_at.is_none());
        assert_eq!(ctx.repair_attempts, 0);
        assert!(ctx.transitions.is_empty());
        assert_eq!(ctx.actual_cost, Decimal::ZERO);
        assert_eq!(ctx.total_tokens_used, 0);
        assert_eq!(ctx.max_tokens, 0);
    }

    #[test]
    fn test_with_user() {
        let ctx = JobContext::with_user("alice", "Title", "Desc");
        assert_eq!(ctx.user_id, "alice");
    }
}
