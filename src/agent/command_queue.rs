//! Command queue with lane-based priority and message coalescing.
//!
//! Provides per-session FIFO queues with support for:
//! - Priority lanes (system commands run before user input)
//! - Message coalescing (combine rapid successive messages)
//! - Debounce (delay processing to collect related messages)

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, RwLock};
use uuid::Uuid;

/// Priority lanes for command processing.
///
/// Lower discriminant values indicate higher priority. When dequeuing,
/// the queue returns commands from the highest-priority lane first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CommandLane {
    /// System commands (highest priority) - /help, /version, etc.
    System = 0,
    /// Approval responses - yes/no/always.
    Approval = 1,
    /// Control commands - /undo, /redo, /compact, etc.
    Control = 2,
    /// User input (lowest priority) - regular messages.
    UserInput = 3,
}

impl CommandLane {
    /// All lanes in priority order (highest to lowest).
    const ALL: [CommandLane; 4] = [
        CommandLane::System,
        CommandLane::Approval,
        CommandLane::Control,
        CommandLane::UserInput,
    ];
}

/// A queued command with metadata.
#[derive(Debug, Clone)]
pub struct QueuedCommand {
    /// Unique identifier for this command.
    pub id: Uuid,
    /// Session this command belongs to.
    pub session_id: Uuid,
    /// Priority lane for processing order.
    pub lane: CommandLane,
    /// The command content (user text, slash command, etc.).
    pub content: String,
    /// Channel the command arrived from.
    pub channel: String,
    /// User who sent the command.
    pub user_id: String,
    /// When this command was enqueued.
    pub enqueued_at: Instant,
    /// Arbitrary metadata attached to the command.
    pub metadata: serde_json::Value,
}

impl QueuedCommand {
    /// Create a new queued command with auto-generated ID and current timestamp.
    pub fn new(
        session_id: Uuid,
        lane: CommandLane,
        content: impl Into<String>,
        channel: impl Into<String>,
        user_id: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            lane,
            content: content.into(),
            channel: channel.into(),
            user_id: user_id.into(),
            enqueued_at: Instant::now(),
            metadata: serde_json::Value::Null,
        }
    }

    /// Attach metadata to this command.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Configuration for the command queue.
#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// Maximum commands per session queue.
    pub max_queue_size: usize,
    /// Debounce delay for user input (to coalesce rapid messages).
    pub debounce_ms: u64,
    /// Whether to coalesce consecutive user messages.
    pub coalesce_enabled: bool,
    /// Maximum age of a queued command before it's dropped.
    pub max_age: Duration,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_queue_size: 100,
            debounce_ms: 300,
            coalesce_enabled: true,
            max_age: Duration::from_secs(300),
        }
    }
}

/// Statistics for a session's command queue.
#[derive(Debug, Clone, Serialize)]
pub struct QueueStats {
    /// Total commands enqueued over the lifetime of this session queue.
    pub total_enqueued: u64,
    /// Total commands dequeued over the lifetime of this session queue.
    pub total_dequeued: u64,
    /// Total commands coalesced (merged into another command).
    pub total_coalesced: u64,
    /// Number of pending commands per lane.
    pub pending_by_lane: HashMap<CommandLane, usize>,
    /// Total pending commands across all lanes.
    pub queue_depth: usize,
}

/// Errors that can occur during queue operations.
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    /// The session queue is at capacity.
    #[error("Queue full for session {session_id}: {size}/{max}")]
    QueueFull {
        session_id: Uuid,
        size: usize,
        max: usize,
    },
    /// The command has exceeded its maximum age.
    #[error("Command expired: age {age_secs}s exceeds max {max_secs}s")]
    Expired { age_secs: u64, max_secs: u64 },
}

/// Per-session command queue with lane-based priority.
///
/// Each session gets its own set of lane queues. Commands are dequeued
/// in priority order: System > Approval > Control > UserInput.
pub struct CommandQueue {
    /// Per-session queues, each containing lane-sorted commands.
    queues: Arc<RwLock<HashMap<Uuid, SessionQueue>>>,
    /// Configuration for queue behavior.
    config: QueueConfig,
    /// Notifier for when new commands are enqueued.
    notify: Arc<Notify>,
}

/// Internal per-session queue state.
struct SessionQueue {
    /// Commands organized by lane.
    lanes: HashMap<CommandLane, VecDeque<QueuedCommand>>,
    /// When the last command was enqueued.
    last_enqueue: Instant,
    /// Lifetime count of enqueued commands.
    total_enqueued: u64,
    /// Lifetime count of dequeued commands.
    total_dequeued: u64,
    /// Lifetime count of coalesced commands.
    total_coalesced: u64,
}

impl SessionQueue {
    /// Create a new empty session queue.
    fn new() -> Self {
        let mut lanes = HashMap::new();
        for lane in CommandLane::ALL {
            lanes.insert(lane, VecDeque::new());
        }
        Self {
            lanes,
            last_enqueue: Instant::now(),
            total_enqueued: 0,
            total_dequeued: 0,
            total_coalesced: 0,
        }
    }

    /// Total number of pending commands across all lanes.
    fn total_pending(&self) -> usize {
        self.lanes.values().map(|q| q.len()).sum()
    }

    /// Dequeue the highest-priority command.
    fn dequeue(&mut self) -> Option<QueuedCommand> {
        for lane in CommandLane::ALL {
            if let Some(queue) = self.lanes.get_mut(&lane)
                && let Some(cmd) = queue.pop_front()
            {
                self.total_dequeued += 1;
                return Some(cmd);
            }
        }
        None
    }

    /// Peek at the highest-priority command without removing it.
    fn peek(&self) -> Option<&QueuedCommand> {
        for lane in CommandLane::ALL {
            if let Some(queue) = self.lanes.get(&lane)
                && let Some(cmd) = queue.front()
            {
                return Some(cmd);
            }
        }
        None
    }

    /// Drain all commands, returning them in priority order.
    fn drain(&mut self) -> Vec<QueuedCommand> {
        let mut result = Vec::with_capacity(self.total_pending());
        for lane in CommandLane::ALL {
            if let Some(queue) = self.lanes.get_mut(&lane) {
                let count = queue.len() as u64;
                result.extend(queue.drain(..));
                self.total_dequeued += count;
            }
        }
        result
    }

    /// Remove expired commands from all lanes, returning the count removed.
    fn remove_expired(&mut self, max_age: Duration) -> usize {
        let now = Instant::now();
        let mut removed = 0;
        for queue in self.lanes.values_mut() {
            let before = queue.len();
            queue.retain(|cmd| now.duration_since(cmd.enqueued_at) < max_age);
            removed += before - queue.len();
        }
        removed
    }
}

impl CommandQueue {
    /// Create a new command queue with the given configuration.
    pub fn new(config: QueueConfig) -> Self {
        Self {
            queues: Arc::new(RwLock::new(HashMap::new())),
            config,
            notify: Arc::new(Notify::new()),
        }
    }

    /// Enqueue a command into the appropriate session and lane queue.
    ///
    /// Returns `QueueError::QueueFull` if the session queue is at capacity.
    pub async fn enqueue(&self, cmd: QueuedCommand) -> Result<(), QueueError> {
        let mut queues = self.queues.write().await;
        let session_queue = queues
            .entry(cmd.session_id)
            .or_insert_with(SessionQueue::new);

        // Check capacity
        let current_size = session_queue.total_pending();
        if current_size >= self.config.max_queue_size {
            return Err(QueueError::QueueFull {
                session_id: cmd.session_id,
                size: current_size,
                max: self.config.max_queue_size,
            });
        }

        let lane = cmd.lane;
        session_queue
            .lanes
            .entry(lane)
            .or_insert_with(VecDeque::new)
            .push_back(cmd);
        session_queue.total_enqueued += 1;
        session_queue.last_enqueue = Instant::now();

        // Drop the write lock before notifying to avoid holding it
        // while waiters wake up.
        drop(queues);

        self.notify.notify_waiters();
        Ok(())
    }

    /// Dequeue the highest-priority command for a session.
    ///
    /// Returns `None` if the session queue is empty or does not exist.
    pub async fn dequeue(&self, session_id: Uuid) -> Option<QueuedCommand> {
        let mut queues = self.queues.write().await;
        queues.get_mut(&session_id).and_then(|sq| sq.dequeue())
    }

    /// Peek at the highest-priority command for a session without removing it.
    pub async fn peek(&self, session_id: Uuid) -> Option<QueuedCommand> {
        let queues = self.queues.read().await;
        queues.get(&session_id).and_then(|sq| sq.peek()).cloned()
    }

    /// Coalesce consecutive user-input messages for a session.
    ///
    /// When coalescing is enabled, this drains all `UserInput` lane commands
    /// and joins their content with newlines into a single command. Non-user-input
    /// lanes are left untouched.
    ///
    /// Returns `None` if there are no user-input commands to coalesce.
    pub async fn coalesce(&self, session_id: Uuid) -> Option<QueuedCommand> {
        if !self.config.coalesce_enabled {
            return self.dequeue(session_id).await;
        }

        let mut queues = self.queues.write().await;
        let session_queue = queues.get_mut(&session_id)?;

        let user_queue = session_queue.lanes.get_mut(&CommandLane::UserInput)?;
        if user_queue.is_empty() {
            return None;
        }

        // Drain all user-input commands
        let commands: Vec<QueuedCommand> = user_queue.drain(..).collect();
        let count = commands.len();

        if count == 0 {
            return None;
        }

        // Use the first command as the base
        let first = &commands[0];
        let combined_content = commands
            .iter()
            .map(|c| c.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let coalesced = QueuedCommand {
            id: first.id,
            session_id: first.session_id,
            lane: CommandLane::UserInput,
            content: combined_content,
            channel: first.channel.clone(),
            user_id: first.user_id.clone(),
            enqueued_at: first.enqueued_at,
            metadata: first.metadata.clone(),
        };

        // Track coalesced count (the merged-away messages)
        if count > 1 {
            session_queue.total_coalesced += (count - 1) as u64;
        }
        session_queue.total_dequeued += 1;

        Some(coalesced)
    }

    /// Drain all commands for a session, returning them in priority order.
    pub async fn drain_session(&self, session_id: Uuid) -> Vec<QueuedCommand> {
        let mut queues = self.queues.write().await;
        queues
            .get_mut(&session_id)
            .map(|sq| sq.drain())
            .unwrap_or_default()
    }

    /// Get the number of pending commands for a session.
    pub async fn len(&self, session_id: Uuid) -> usize {
        let queues = self.queues.read().await;
        queues
            .get(&session_id)
            .map(|sq| sq.total_pending())
            .unwrap_or(0)
    }

    /// Check if a session's queue is empty.
    pub async fn is_empty(&self, session_id: Uuid) -> bool {
        self.len(session_id).await == 0
    }

    /// Clear all commands for a session.
    pub async fn clear_session(&self, session_id: Uuid) {
        let mut queues = self.queues.write().await;
        if let Some(sq) = queues.get_mut(&session_id) {
            for queue in sq.lanes.values_mut() {
                queue.clear();
            }
        }
    }

    /// Remove commands older than `max_age` from all sessions.
    ///
    /// Also removes session queues that are completely empty after expiration.
    pub async fn clear_expired(&self) {
        let mut queues = self.queues.write().await;
        let mut empty_sessions = Vec::new();

        for (session_id, sq) in queues.iter_mut() {
            sq.remove_expired(self.config.max_age);
            if sq.total_pending() == 0 {
                empty_sessions.push(*session_id);
            }
        }

        for session_id in empty_sessions {
            queues.remove(&session_id);
        }
    }

    /// Get statistics for a session's queue.
    ///
    /// Returns `None` if the session has no queue.
    pub async fn stats(&self, session_id: Uuid) -> Option<QueueStats> {
        let queues = self.queues.read().await;
        let sq = queues.get(&session_id)?;

        let mut pending_by_lane = HashMap::new();
        for (lane, queue) in &sq.lanes {
            if !queue.is_empty() {
                pending_by_lane.insert(*lane, queue.len());
            }
        }

        Some(QueueStats {
            total_enqueued: sq.total_enqueued,
            total_dequeued: sq.total_dequeued,
            total_coalesced: sq.total_coalesced,
            pending_by_lane,
            queue_depth: sq.total_pending(),
        })
    }

    /// Wait until a command is enqueued.
    ///
    /// This is useful for implementing a blocking dequeue loop:
    /// ```ignore
    /// loop {
    ///     queue.wait_for_command().await;
    ///     if let Some(cmd) = queue.dequeue(session_id).await {
    ///         // process cmd
    ///     }
    /// }
    /// ```
    pub async fn wait_for_command(&self) {
        self.notify.notified().await;
    }
}

/// Classify a message's content into the appropriate command lane.
///
/// Uses simple heuristics based on content patterns:
/// - System commands: `/help`, `/version`, `/tools`, `/ping`, `/model`, `/status`
/// - Approval responses: `yes`, `y`, `no`, `n`, `always`
/// - Control commands: any message starting with `/`
/// - User input: everything else
pub fn classify_lane(content: &str) -> CommandLane {
    let trimmed = content.trim().to_lowercase();
    if matches!(
        trimmed.as_str(),
        "/help" | "/version" | "/tools" | "/ping" | "/model" | "/status"
    ) {
        CommandLane::System
    } else if matches!(trimmed.as_str(), "yes" | "y" | "no" | "n" | "always") {
        CommandLane::Approval
    } else if trimmed.starts_with('/') {
        CommandLane::Control
    } else {
        CommandLane::UserInput
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a command for testing.
    fn make_cmd(session_id: Uuid, lane: CommandLane, content: &str) -> QueuedCommand {
        QueuedCommand::new(session_id, lane, content, "test", "user1")
    }

    /// Helper to create a command with a specific enqueue time.
    fn make_cmd_at(
        session_id: Uuid,
        lane: CommandLane,
        content: &str,
        enqueued_at: Instant,
    ) -> QueuedCommand {
        let mut cmd = make_cmd(session_id, lane, content);
        cmd.enqueued_at = enqueued_at;
        cmd
    }

    // --- Lane classification tests ---

    #[test]
    fn test_classify_system_commands() {
        assert_eq!(classify_lane("/help"), CommandLane::System);
        assert_eq!(classify_lane("/version"), CommandLane::System);
        assert_eq!(classify_lane("/tools"), CommandLane::System);
        assert_eq!(classify_lane("/ping"), CommandLane::System);
        assert_eq!(classify_lane("/model"), CommandLane::System);
        assert_eq!(classify_lane("/status"), CommandLane::System);
    }

    #[test]
    fn test_classify_system_commands_case_insensitive() {
        assert_eq!(classify_lane("/HELP"), CommandLane::System);
        assert_eq!(classify_lane("/Version"), CommandLane::System);
        assert_eq!(classify_lane("  /ping  "), CommandLane::System);
    }

    #[test]
    fn test_classify_approval_responses() {
        assert_eq!(classify_lane("yes"), CommandLane::Approval);
        assert_eq!(classify_lane("y"), CommandLane::Approval);
        assert_eq!(classify_lane("no"), CommandLane::Approval);
        assert_eq!(classify_lane("n"), CommandLane::Approval);
        assert_eq!(classify_lane("always"), CommandLane::Approval);
    }

    #[test]
    fn test_classify_approval_case_insensitive() {
        assert_eq!(classify_lane("YES"), CommandLane::Approval);
        assert_eq!(classify_lane("No"), CommandLane::Approval);
        assert_eq!(classify_lane("  Always  "), CommandLane::Approval);
    }

    #[test]
    fn test_classify_control_commands() {
        assert_eq!(classify_lane("/undo"), CommandLane::Control);
        assert_eq!(classify_lane("/redo"), CommandLane::Control);
        assert_eq!(classify_lane("/compact"), CommandLane::Control);
        assert_eq!(classify_lane("/clear"), CommandLane::Control);
        assert_eq!(classify_lane("/heartbeat"), CommandLane::Control);
        assert_eq!(classify_lane("/unknown_command"), CommandLane::Control);
    }

    #[test]
    fn test_classify_user_input() {
        assert_eq!(classify_lane("Hello, world!"), CommandLane::UserInput);
        assert_eq!(classify_lane("Build me a website"), CommandLane::UserInput);
        assert_eq!(classify_lane(""), CommandLane::UserInput);
        assert_eq!(classify_lane("  some text  "), CommandLane::UserInput);
    }

    #[test]
    fn test_classify_edge_cases() {
        // "yesterday" starts with "yes" but should not match as approval
        assert_eq!(classify_lane("yesterday"), CommandLane::UserInput);
        // "nothing" starts with "no" but should not match
        assert_eq!(classify_lane("nothing"), CommandLane::UserInput);
        // A slash in the middle is not a command
        assert_eq!(classify_lane("a/b"), CommandLane::UserInput);
    }

    // --- CommandLane ordering tests ---

    #[test]
    fn test_lane_ordering() {
        assert!(CommandLane::System < CommandLane::Approval);
        assert!(CommandLane::Approval < CommandLane::Control);
        assert!(CommandLane::Control < CommandLane::UserInput);
    }

    // --- QueuedCommand tests ---

    #[test]
    fn test_queued_command_new() {
        let session_id = Uuid::new_v4();
        let cmd = QueuedCommand::new(session_id, CommandLane::UserInput, "hello", "repl", "user1");
        assert_eq!(cmd.session_id, session_id);
        assert_eq!(cmd.lane, CommandLane::UserInput);
        assert_eq!(cmd.content, "hello");
        assert_eq!(cmd.channel, "repl");
        assert_eq!(cmd.user_id, "user1");
        assert_eq!(cmd.metadata, serde_json::Value::Null);
    }

    #[test]
    fn test_queued_command_with_metadata() {
        let cmd = QueuedCommand::new(Uuid::new_v4(), CommandLane::System, "/help", "web", "user1")
            .with_metadata(serde_json::json!({"source": "gateway"}));

        assert_eq!(cmd.metadata["source"], "gateway");
    }

    // --- Enqueue and dequeue tests ---

    #[tokio::test]
    async fn test_enqueue_and_dequeue_single() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_id = Uuid::new_v4();
        let cmd = make_cmd(session_id, CommandLane::UserInput, "hello");

        queue.enqueue(cmd).await.unwrap();
        assert_eq!(queue.len(session_id).await, 1);

        let dequeued = queue.dequeue(session_id).await.unwrap();
        assert_eq!(dequeued.content, "hello");
        assert!(queue.is_empty(session_id).await);
    }

    #[tokio::test]
    async fn test_dequeue_respects_priority_order() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_id = Uuid::new_v4();

        // Enqueue in reverse priority order
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "user msg"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::Control, "/undo"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::System, "/help"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::Approval, "yes"))
            .await
            .unwrap();

        // Should dequeue in priority order
        assert_eq!(queue.dequeue(session_id).await.unwrap().content, "/help");
        assert_eq!(queue.dequeue(session_id).await.unwrap().content, "yes");
        assert_eq!(queue.dequeue(session_id).await.unwrap().content, "/undo");
        assert_eq!(queue.dequeue(session_id).await.unwrap().content, "user msg");
        assert!(queue.dequeue(session_id).await.is_none());
    }

    #[tokio::test]
    async fn test_dequeue_fifo_within_same_lane() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_id = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "first"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "second"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "third"))
            .await
            .unwrap();

        assert_eq!(queue.dequeue(session_id).await.unwrap().content, "first");
        assert_eq!(queue.dequeue(session_id).await.unwrap().content, "second");
        assert_eq!(queue.dequeue(session_id).await.unwrap().content, "third");
    }

    #[tokio::test]
    async fn test_dequeue_empty_queue() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_id = Uuid::new_v4();

        assert!(queue.dequeue(session_id).await.is_none());
    }

    #[tokio::test]
    async fn test_dequeue_nonexistent_session() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_id = Uuid::new_v4();

        // Never enqueued anything for this session
        assert!(queue.dequeue(session_id).await.is_none());
        assert!(queue.is_empty(session_id).await);
        assert_eq!(queue.len(session_id).await, 0);
    }

    // --- Peek tests ---

    #[tokio::test]
    async fn test_peek_does_not_remove() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_id = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "hello"))
            .await
            .unwrap();

        let peeked = queue.peek(session_id).await.unwrap();
        assert_eq!(peeked.content, "hello");

        // Still there after peek
        assert_eq!(queue.len(session_id).await, 1);
        let dequeued = queue.dequeue(session_id).await.unwrap();
        assert_eq!(dequeued.content, "hello");
    }

    #[tokio::test]
    async fn test_peek_returns_highest_priority() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_id = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "user msg"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::Approval, "yes"))
            .await
            .unwrap();

        let peeked = queue.peek(session_id).await.unwrap();
        assert_eq!(peeked.content, "yes");
        assert_eq!(peeked.lane, CommandLane::Approval);
    }

    #[tokio::test]
    async fn test_peek_empty_queue() {
        let queue = CommandQueue::new(QueueConfig::default());
        assert!(queue.peek(Uuid::new_v4()).await.is_none());
    }

    // --- Coalescing tests ---

    #[tokio::test]
    async fn test_coalesce_multiple_user_messages() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_id = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "line 1"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "line 2"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "line 3"))
            .await
            .unwrap();

        let coalesced = queue.coalesce(session_id).await.unwrap();
        assert_eq!(coalesced.content, "line 1\nline 2\nline 3");
        assert_eq!(coalesced.lane, CommandLane::UserInput);

        // User input lane should be empty now
        assert!(queue.coalesce(session_id).await.is_none());
    }

    #[tokio::test]
    async fn test_coalesce_single_message() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_id = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "only one"))
            .await
            .unwrap();

        let coalesced = queue.coalesce(session_id).await.unwrap();
        assert_eq!(coalesced.content, "only one");
    }

    #[tokio::test]
    async fn test_coalesce_does_not_touch_other_lanes() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_id = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_id, CommandLane::System, "/help"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "msg 1"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "msg 2"))
            .await
            .unwrap();

        // Coalesce only affects UserInput
        let coalesced = queue.coalesce(session_id).await.unwrap();
        assert_eq!(coalesced.content, "msg 1\nmsg 2");

        // System command is still there
        let system_cmd = queue.dequeue(session_id).await.unwrap();
        assert_eq!(system_cmd.content, "/help");
        assert_eq!(system_cmd.lane, CommandLane::System);
    }

    #[tokio::test]
    async fn test_coalesce_disabled() {
        let config = QueueConfig {
            coalesce_enabled: false,
            ..QueueConfig::default()
        };
        let queue = CommandQueue::new(config);
        let session_id = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "msg 1"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "msg 2"))
            .await
            .unwrap();

        // When coalescing is disabled, coalesce falls back to dequeue
        let cmd = queue.coalesce(session_id).await.unwrap();
        assert_eq!(cmd.content, "msg 1");

        let cmd = queue.coalesce(session_id).await.unwrap();
        assert_eq!(cmd.content, "msg 2");
    }

    #[tokio::test]
    async fn test_coalesce_empty_user_lane() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_id = Uuid::new_v4();

        // Only a system command, no user input
        queue
            .enqueue(make_cmd(session_id, CommandLane::System, "/help"))
            .await
            .unwrap();

        assert!(queue.coalesce(session_id).await.is_none());
    }

    #[tokio::test]
    async fn test_coalesce_tracks_stats() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_id = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "a"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "b"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "c"))
            .await
            .unwrap();

        let _coalesced = queue.coalesce(session_id).await.unwrap();

        let stats = queue.stats(session_id).await.unwrap();
        assert_eq!(stats.total_enqueued, 3);
        assert_eq!(stats.total_dequeued, 1);
        assert_eq!(stats.total_coalesced, 2); // 3 messages merged into 1 = 2 coalesced
    }

    // --- Queue full tests ---

    #[tokio::test]
    async fn test_queue_full_rejection() {
        let config = QueueConfig {
            max_queue_size: 2,
            ..QueueConfig::default()
        };
        let queue = CommandQueue::new(config);
        let session_id = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "1"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "2"))
            .await
            .unwrap();

        let result = queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "3"))
            .await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(
            err,
            QueueError::QueueFull {
                size: 2,
                max: 2,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn test_queue_full_across_lanes() {
        let config = QueueConfig {
            max_queue_size: 3,
            ..QueueConfig::default()
        };
        let queue = CommandQueue::new(config);
        let session_id = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_id, CommandLane::System, "/help"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::Approval, "yes"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "msg"))
            .await
            .unwrap();

        // Queue is full across all lanes
        let result = queue
            .enqueue(make_cmd(session_id, CommandLane::Control, "/undo"))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_queue_accepts_after_dequeue() {
        let config = QueueConfig {
            max_queue_size: 1,
            ..QueueConfig::default()
        };
        let queue = CommandQueue::new(config);
        let session_id = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "1"))
            .await
            .unwrap();

        // Full
        assert!(
            queue
                .enqueue(make_cmd(session_id, CommandLane::UserInput, "2"))
                .await
                .is_err()
        );

        // Dequeue frees a slot
        queue.dequeue(session_id).await.unwrap();

        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "2"))
            .await
            .unwrap();
    }

    // --- Drain tests ---

    #[tokio::test]
    async fn test_drain_session() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_id = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "msg 1"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::System, "/help"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "msg 2"))
            .await
            .unwrap();

        let drained = queue.drain_session(session_id).await;
        assert_eq!(drained.len(), 3);

        // Should be in priority order
        assert_eq!(drained[0].lane, CommandLane::System);
        assert_eq!(drained[1].lane, CommandLane::UserInput);
        assert_eq!(drained[2].lane, CommandLane::UserInput);

        assert!(queue.is_empty(session_id).await);
    }

    #[tokio::test]
    async fn test_drain_empty_session() {
        let queue = CommandQueue::new(QueueConfig::default());
        let drained = queue.drain_session(Uuid::new_v4()).await;
        assert!(drained.is_empty());
    }

    // --- Clear session tests ---

    #[tokio::test]
    async fn test_clear_session() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_id = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "msg"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::System, "/help"))
            .await
            .unwrap();

        assert_eq!(queue.len(session_id).await, 2);

        queue.clear_session(session_id).await;

        assert!(queue.is_empty(session_id).await);
        assert!(queue.dequeue(session_id).await.is_none());
    }

    // --- Expiry tests ---

    #[tokio::test]
    async fn test_clear_expired_removes_old_commands() {
        let config = QueueConfig {
            max_age: Duration::from_millis(50),
            ..QueueConfig::default()
        };
        let queue = CommandQueue::new(config);
        let session_id = Uuid::new_v4();

        // Enqueue a command with an old timestamp
        let old_cmd = make_cmd_at(
            session_id,
            CommandLane::UserInput,
            "old",
            Instant::now() - Duration::from_millis(100),
        );
        queue.enqueue(old_cmd).await.unwrap();

        // Enqueue a fresh command
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "fresh"))
            .await
            .unwrap();

        assert_eq!(queue.len(session_id).await, 2);

        queue.clear_expired().await;

        assert_eq!(queue.len(session_id).await, 1);
        let remaining = queue.dequeue(session_id).await.unwrap();
        assert_eq!(remaining.content, "fresh");
    }

    #[tokio::test]
    async fn test_clear_expired_removes_empty_sessions() {
        let config = QueueConfig {
            max_age: Duration::from_millis(10),
            ..QueueConfig::default()
        };
        let queue = CommandQueue::new(config);
        let session_id = Uuid::new_v4();

        let old_cmd = make_cmd_at(
            session_id,
            CommandLane::UserInput,
            "old",
            Instant::now() - Duration::from_millis(50),
        );
        queue.enqueue(old_cmd).await.unwrap();

        queue.clear_expired().await;

        // Session queue should have been removed entirely
        assert!(queue.stats(session_id).await.is_none());
    }

    // --- Stats tests ---

    #[tokio::test]
    async fn test_stats_tracking() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_id = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "1"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::System, "/help"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "2"))
            .await
            .unwrap();

        let stats = queue.stats(session_id).await.unwrap();
        assert_eq!(stats.total_enqueued, 3);
        assert_eq!(stats.total_dequeued, 0);
        assert_eq!(stats.total_coalesced, 0);
        assert_eq!(stats.queue_depth, 3);
        assert_eq!(stats.pending_by_lane[&CommandLane::UserInput], 2);
        assert_eq!(stats.pending_by_lane[&CommandLane::System], 1);

        // Dequeue one
        queue.dequeue(session_id).await.unwrap();
        let stats = queue.stats(session_id).await.unwrap();
        assert_eq!(stats.total_dequeued, 1);
        assert_eq!(stats.queue_depth, 2);
    }

    #[tokio::test]
    async fn test_stats_nonexistent_session() {
        let queue = CommandQueue::new(QueueConfig::default());
        assert!(queue.stats(Uuid::new_v4()).await.is_none());
    }

    // --- Multi-session isolation tests ---

    #[tokio::test]
    async fn test_multi_session_isolation() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_a = Uuid::new_v4();
        let session_b = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_a, CommandLane::UserInput, "msg A"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_b, CommandLane::UserInput, "msg B"))
            .await
            .unwrap();

        assert_eq!(queue.len(session_a).await, 1);
        assert_eq!(queue.len(session_b).await, 1);

        let cmd_a = queue.dequeue(session_a).await.unwrap();
        assert_eq!(cmd_a.content, "msg A");

        let cmd_b = queue.dequeue(session_b).await.unwrap();
        assert_eq!(cmd_b.content, "msg B");
    }

    #[tokio::test]
    async fn test_clear_session_does_not_affect_others() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_a = Uuid::new_v4();
        let session_b = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_a, CommandLane::UserInput, "msg A"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_b, CommandLane::UserInput, "msg B"))
            .await
            .unwrap();

        queue.clear_session(session_a).await;

        assert!(queue.is_empty(session_a).await);
        assert!(!queue.is_empty(session_b).await);
    }

    #[tokio::test]
    async fn test_queue_full_per_session() {
        let config = QueueConfig {
            max_queue_size: 1,
            ..QueueConfig::default()
        };
        let queue = CommandQueue::new(config);
        let session_a = Uuid::new_v4();
        let session_b = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_a, CommandLane::UserInput, "a"))
            .await
            .unwrap();

        // Session A is full but session B should still accept
        assert!(
            queue
                .enqueue(make_cmd(session_a, CommandLane::UserInput, "a2"))
                .await
                .is_err()
        );
        queue
            .enqueue(make_cmd(session_b, CommandLane::UserInput, "b"))
            .await
            .unwrap();
    }

    // --- Concurrent access tests ---

    #[tokio::test]
    async fn test_concurrent_enqueue() {
        let queue = Arc::new(CommandQueue::new(QueueConfig::default()));
        let session_id = Uuid::new_v4();

        let mut handles = Vec::new();
        for i in 0..20 {
            let q = Arc::clone(&queue);
            let sid = session_id;
            handles.push(tokio::spawn(async move {
                q.enqueue(make_cmd(sid, CommandLane::UserInput, &format!("msg {}", i)))
                    .await
                    .unwrap();
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(queue.len(session_id).await, 20);
    }

    #[tokio::test]
    async fn test_concurrent_enqueue_and_dequeue() {
        let queue = Arc::new(CommandQueue::new(QueueConfig::default()));
        let session_id = Uuid::new_v4();

        // Pre-fill with some commands
        for i in 0..10 {
            queue
                .enqueue(make_cmd(
                    session_id,
                    CommandLane::UserInput,
                    &format!("pre {}", i),
                ))
                .await
                .unwrap();
        }

        let q_enqueue = Arc::clone(&queue);
        let q_dequeue = Arc::clone(&queue);
        let sid = session_id;

        let enqueue_handle = tokio::spawn(async move {
            for i in 0..10 {
                q_enqueue
                    .enqueue(make_cmd(sid, CommandLane::UserInput, &format!("new {}", i)))
                    .await
                    .unwrap();
            }
        });

        let dequeue_handle = tokio::spawn(async move {
            let mut dequeued = 0;
            for _ in 0..20 {
                if q_dequeue.dequeue(sid).await.is_some() {
                    dequeued += 1;
                }
                tokio::task::yield_now().await;
            }
            dequeued
        });

        enqueue_handle.await.unwrap();
        let dequeued = dequeue_handle.await.unwrap();

        // All items should be accounted for
        let remaining = queue.len(session_id).await;
        assert_eq!(dequeued + remaining, 20);
    }

    // --- Notify / wait_for_command tests ---

    #[tokio::test]
    async fn test_wait_for_command_wakes_on_enqueue() {
        let queue = Arc::new(CommandQueue::new(QueueConfig::default()));
        let session_id = Uuid::new_v4();

        let q_wait = Arc::clone(&queue);
        let q_enqueue = Arc::clone(&queue);

        let wait_handle = tokio::spawn(async move {
            q_wait.wait_for_command().await;
            q_wait.dequeue(session_id).await
        });

        // Give the waiter a moment to start waiting
        tokio::time::sleep(Duration::from_millis(10)).await;

        q_enqueue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "wake up"))
            .await
            .unwrap();

        let result = tokio::time::timeout(Duration::from_secs(1), wait_handle)
            .await
            .expect("timed out waiting for command")
            .expect("task panicked");

        assert_eq!(result.unwrap().content, "wake up");
    }

    // --- QueueConfig default tests ---

    #[test]
    fn test_queue_config_default() {
        let config = QueueConfig::default();
        assert_eq!(config.max_queue_size, 100);
        assert_eq!(config.debounce_ms, 300);
        assert!(config.coalesce_enabled);
        assert_eq!(config.max_age, Duration::from_secs(300));
    }

    // --- QueueError display tests ---

    #[test]
    fn test_queue_error_display() {
        let err = QueueError::QueueFull {
            session_id: Uuid::nil(),
            size: 100,
            max: 100,
        };
        let msg = err.to_string();
        assert!(msg.contains("Queue full"));
        assert!(msg.contains("100/100"));

        let err = QueueError::Expired {
            age_secs: 400,
            max_secs: 300,
        };
        let msg = err.to_string();
        assert!(msg.contains("expired"));
        assert!(msg.contains("400"));
        assert!(msg.contains("300"));
    }

    // --- Mixed lane operations ---

    #[tokio::test]
    async fn test_interleaved_enqueue_dequeue_priority() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_id = Uuid::new_v4();

        // Enqueue user input
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "user 1"))
            .await
            .unwrap();

        // Dequeue user input (only thing available)
        let cmd = queue.dequeue(session_id).await.unwrap();
        assert_eq!(cmd.content, "user 1");

        // Enqueue user input then system
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "user 2"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::System, "/ping"))
            .await
            .unwrap();

        // System should come first even though enqueued second
        let cmd = queue.dequeue(session_id).await.unwrap();
        assert_eq!(cmd.content, "/ping");
        assert_eq!(cmd.lane, CommandLane::System);

        let cmd = queue.dequeue(session_id).await.unwrap();
        assert_eq!(cmd.content, "user 2");
    }

    #[tokio::test]
    async fn test_drain_preserves_priority_ordering() {
        let queue = CommandQueue::new(QueueConfig::default());
        let session_id = Uuid::new_v4();

        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "u1"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::Control, "/undo"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::Approval, "yes"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::System, "/help"))
            .await
            .unwrap();
        queue
            .enqueue(make_cmd(session_id, CommandLane::UserInput, "u2"))
            .await
            .unwrap();

        let drained = queue.drain_session(session_id).await;
        assert_eq!(drained.len(), 5);
        assert_eq!(drained[0].lane, CommandLane::System);
        assert_eq!(drained[1].lane, CommandLane::Approval);
        assert_eq!(drained[2].lane, CommandLane::Control);
        assert_eq!(drained[3].lane, CommandLane::UserInput);
        assert_eq!(drained[4].lane, CommandLane::UserInput);
        // FIFO within UserInput lane
        assert_eq!(drained[3].content, "u1");
        assert_eq!(drained[4].content, "u2");
    }
}
