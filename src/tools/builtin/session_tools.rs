//! Session management tools for inter-session messaging.
//!
//! These tools allow the LLM to perform cross-session operations:
//! - List active sessions with filtering
//! - Retrieve conversation history from a session
//! - Send a message to another session (requires approval)

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::agent::SessionManager;
use crate::agent::session::Session;
use crate::context::JobContext;
use crate::db::Database;
use crate::tools::tool::{Tool, ToolError, ToolOutput};

/// Summary of a session returned by the list tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionInfo {
    id: String,
    user_id: String,
    thread_count: usize,
    created_at: String,
    last_active_at: String,
}

/// A message entry returned by the history tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MessageEntry {
    role: String,
    content: String,
    timestamp: String,
}

// ---------------------------------------------------------------------------
// SessionListTool
// ---------------------------------------------------------------------------

/// Lists active sessions with optional filtering.
///
/// Returns a JSON array of session summaries including id, user_id,
/// thread_count, created_at, and last_active_at.
pub struct SessionListTool {
    session_manager: Arc<SessionManager>,
}

impl SessionListTool {
    /// Create a new `SessionListTool`.
    pub fn new(session_manager: Arc<SessionManager>) -> Self {
        Self { session_manager }
    }
}

#[async_trait]
impl Tool for SessionListTool {
    fn name(&self) -> &str {
        "session_list"
    }

    fn description(&self) -> &str {
        "List active sessions. Optionally filter by kind and limit the number of results. \
         Returns session IDs, user IDs, thread counts, and timestamps."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "kind": {
                    "type": "string",
                    "description": "Optional filter by session kind: 'main', 'group', 'cron', 'hook'",
                    "enum": ["main", "group", "cron", "hook"]
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of sessions to return (default: 20)",
                    "minimum": 1,
                    "maximum": 100
                }
            }
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let _kind = params.get("kind").and_then(|v| v.as_str());
        let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

        // Access the sessions from the session manager.
        // The SessionManager exposes sessions via an RwLock; we read-lock to
        // enumerate them without blocking writers for long.
        let sessions_lock: tokio::sync::RwLockReadGuard<'_, HashMap<String, Arc<Mutex<Session>>>> =
            self.session_manager.sessions.read().await;

        let mut infos: Vec<SessionInfo> = Vec::new();
        for (_user_id, session_arc) in sessions_lock.iter() {
            if let Ok(sess) = session_arc.try_lock() {
                // If a kind filter is provided, check session metadata for a
                // "kind" field. Sessions without a matching kind are skipped.
                if let Some(kind_filter) = _kind {
                    let session_kind = sess
                        .metadata
                        .get("kind")
                        .and_then(|v: &serde_json::Value| v.as_str())
                        .unwrap_or("main");
                    if session_kind != kind_filter {
                        continue;
                    }
                }

                infos.push(SessionInfo {
                    id: sess.id.to_string(),
                    user_id: sess.user_id.clone(),
                    thread_count: sess.threads.len(),
                    created_at: sess.created_at.to_rfc3339(),
                    last_active_at: sess.last_active_at.to_rfc3339(),
                });

                if infos.len() >= limit {
                    break;
                }
            }
        }

        // Sort by last_active_at descending (most recent first).
        infos.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));

        let result = serde_json::json!({
            "sessions": infos,
            "count": infos.len(),
        });

        Ok(ToolOutput::success(result, start.elapsed()))
    }

    fn requires_sanitization(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// SessionHistoryTool
// ---------------------------------------------------------------------------

/// Retrieves conversation history from a specific session.
///
/// Looks up the session by its ID, finds the active thread, and returns
/// the most recent messages as a JSON array of `{role, content, timestamp}`.
pub struct SessionHistoryTool {
    session_manager: Arc<SessionManager>,
    db: Arc<dyn Database>,
}

impl SessionHistoryTool {
    /// Create a new `SessionHistoryTool`.
    pub fn new(session_manager: Arc<SessionManager>, db: Arc<dyn Database>) -> Self {
        Self {
            session_manager,
            db,
        }
    }
}

#[async_trait]
impl Tool for SessionHistoryTool {
    fn name(&self) -> &str {
        "session_history"
    }

    fn description(&self) -> &str {
        "Retrieve conversation history from a session. Returns messages with role, content, \
         and timestamp. Use session_list to find valid session IDs first."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The UUID of the session to retrieve history from"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of messages to return (default: 50)",
                    "minimum": 1,
                    "maximum": 200
                }
            },
            "required": ["session_id"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let session_id_str = params
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("missing 'session_id' parameter".into()))?;

        let session_id = uuid::Uuid::parse_str(session_id_str).map_err(|_| {
            ToolError::InvalidParameters(format!("invalid session ID format: {}", session_id_str))
        })?;

        let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

        // Find the session by iterating the session manager's sessions.
        let sessions_lock: tokio::sync::RwLockReadGuard<'_, HashMap<String, Arc<Mutex<Session>>>> =
            self.session_manager.sessions.read().await;

        let mut target_session = None;
        for (_user_id, session_arc) in sessions_lock.iter() {
            if let Ok(sess) = session_arc.try_lock()
                && sess.id == session_id
            {
                // Verify the requesting user owns this session.
                if sess.user_id != ctx.user_id {
                    let result = serde_json::json!({
                        "error": "Access denied: session belongs to another user"
                    });
                    return Ok(ToolOutput::success(result, start.elapsed()));
                }

                // If the session has a conversation_id in metadata, use DB
                // history; otherwise fall back to in-memory thread turns.
                let conversation_id = sess
                    .metadata
                    .get("conversation_id")
                    .and_then(|v: &serde_json::Value| v.as_str())
                    .and_then(|s| uuid::Uuid::parse_str(s).ok());

                // Collect in-memory messages from the active thread.
                let mut entries: Vec<MessageEntry> = Vec::new();
                if let Some(thread) = sess.active_thread() {
                    for turn in &thread.turns {
                        entries.push(MessageEntry {
                            role: "user".to_string(),
                            content: turn.user_input.clone(),
                            timestamp: turn.started_at.to_rfc3339(),
                        });
                        if let Some(ref response) = turn.response {
                            let ts = turn.completed_at.unwrap_or(turn.started_at).to_rfc3339();
                            entries.push(MessageEntry {
                                role: "assistant".to_string(),
                                content: response.clone(),
                                timestamp: ts,
                            });
                        }
                    }
                }

                target_session = Some((entries, conversation_id));
                break;
            }
        }
        drop(sessions_lock);

        let (mut entries, conversation_id) = match target_session {
            Some(data) => data,
            None => {
                let result = serde_json::json!({
                    "error": format!("Session {} not found", session_id)
                });
                return Ok(ToolOutput::success(result, start.elapsed()));
            }
        };

        // If a conversation ID is available and the in-memory turns are empty,
        // try to load persisted messages from the database.
        if entries.is_empty()
            && let Some(conv_id) = conversation_id
        {
            match self.db.list_conversation_messages(conv_id).await {
                Ok(messages) => {
                    for msg in messages {
                        entries.push(MessageEntry {
                            role: msg.role.clone(),
                            content: msg.content.clone(),
                            timestamp: msg.created_at.to_rfc3339(),
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        conversation_id = %conv_id,
                        "Failed to load conversation messages: {}",
                        e
                    );
                }
            }
        }

        // Apply limit (take the most recent messages).
        if entries.len() > limit {
            entries = entries.split_off(entries.len() - limit);
        }

        let result = serde_json::json!({
            "session_id": session_id_str,
            "messages": entries,
            "count": entries.len(),
        });

        Ok(ToolOutput::success(result, start.elapsed()))
    }

    fn requires_sanitization(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// SessionSendTool
// ---------------------------------------------------------------------------

/// Sends a message to another session's active thread.
///
/// This tool requires user approval before execution because it performs
/// a cross-session write operation.
pub struct SessionSendTool {
    session_manager: Arc<SessionManager>,
    db: Arc<dyn Database>,
}

impl SessionSendTool {
    /// Create a new `SessionSendTool`.
    pub fn new(session_manager: Arc<SessionManager>, db: Arc<dyn Database>) -> Self {
        Self {
            session_manager,
            db,
        }
    }
}

#[async_trait]
impl Tool for SessionSendTool {
    fn name(&self) -> &str {
        "session_send"
    }

    fn description(&self) -> &str {
        "Send a message to another session. The message is injected into the target session's \
         active thread as a new user turn. Requires approval because it writes to another session."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The UUID of the target session to send the message to"
                },
                "content": {
                    "type": "string",
                    "description": "The message content to send"
                }
            },
            "required": ["session_id", "content"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let session_id_str = params
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("missing 'session_id' parameter".into()))?;

        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("missing 'content' parameter".into()))?;

        if content.is_empty() {
            return Err(ToolError::InvalidParameters(
                "content must not be empty".into(),
            ));
        }

        let session_id = uuid::Uuid::parse_str(session_id_str).map_err(|_| {
            ToolError::InvalidParameters(format!("invalid session ID format: {}", session_id_str))
        })?;

        // Look up the target session.
        let sessions_lock: tokio::sync::RwLockReadGuard<'_, HashMap<String, Arc<Mutex<Session>>>> =
            self.session_manager.sessions.read().await;

        let mut target_session_arc: Option<Arc<Mutex<Session>>> = None;
        for (_user_id, session_arc) in sessions_lock.iter() {
            if let Ok(sess) = session_arc.try_lock()
                && sess.id == session_id
            {
                // Verify the requesting user owns the target session.
                if sess.user_id != ctx.user_id {
                    let result = serde_json::json!({
                        "error": "Access denied: session belongs to another user"
                    });
                    return Ok(ToolOutput::success(result, start.elapsed()));
                }
                target_session_arc = Some(session_arc.clone());
                break;
            }
        }
        drop(sessions_lock);

        let target_session_arc = match target_session_arc {
            Some(arc) => arc,
            None => {
                let result = serde_json::json!({
                    "error": format!("Session {} not found", session_id)
                });
                return Ok(ToolOutput::success(result, start.elapsed()));
            }
        };

        // Inject the message into the target session's active thread.
        let message_id = uuid::Uuid::new_v4();
        let mut sess = target_session_arc.lock().await;
        let thread = sess.get_or_create_thread();
        thread.start_turn(content);

        // If the session has a conversation_id, also persist to the database.
        let conversation_id = sess
            .metadata
            .get("conversation_id")
            .and_then(|v: &serde_json::Value| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok());
        drop(sess);

        if let Some(conv_id) = conversation_id {
            let db = self.db.clone();
            let content_owned = content.to_string();
            tokio::spawn(async move {
                if let Err(e) = db
                    .add_conversation_message(conv_id, "user", &content_owned)
                    .await
                {
                    tracing::warn!(
                        conversation_id = %conv_id,
                        "Failed to persist cross-session message: {}",
                        e
                    );
                }
            });
        }

        let result = serde_json::json!({
            "message_id": message_id.to_string(),
            "session_id": session_id_str,
            "status": "sent",
            "message": format!("Message sent to session {}", session_id)
        });

        Ok(ToolOutput::success(result, start.elapsed()))
    }

    fn requires_approval(&self) -> bool {
        true
    }

    fn requires_sanitization(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a SessionManager with one session pre-populated.
    async fn setup_session_manager() -> (Arc<SessionManager>, uuid::Uuid) {
        let manager = Arc::new(SessionManager::new());

        // Resolve a thread to create a session for "test-user".
        let (session_arc, _thread_id): (Arc<Mutex<Session>>, uuid::Uuid) =
            manager.resolve_thread("test-user", "cli", None).await;

        let session_id = {
            let sess = session_arc.lock().await;
            sess.id
        };

        (manager, session_id)
    }

    // -----------------------------------------------------------------------
    // SessionListTool tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_session_list_returns_sessions() {
        let (manager, _session_id) = setup_session_manager().await;
        let tool = SessionListTool::new(manager);

        let ctx = JobContext::default();
        let result = tool.execute(serde_json::json!({}), &ctx).await.unwrap();

        let sessions = result.result.get("sessions").unwrap().as_array().unwrap();
        assert!(!sessions.is_empty());
        assert_eq!(
            result.result.get("count").unwrap().as_u64().unwrap(),
            sessions.len() as u64
        );
    }

    #[tokio::test]
    async fn test_session_list_respects_limit() {
        let manager = Arc::new(SessionManager::new());

        // Create multiple sessions.
        let _: (Arc<Mutex<Session>>, uuid::Uuid) =
            manager.resolve_thread("user-a", "cli", None).await;
        let _: (Arc<Mutex<Session>>, uuid::Uuid) =
            manager.resolve_thread("user-b", "cli", None).await;
        let _: (Arc<Mutex<Session>>, uuid::Uuid) =
            manager.resolve_thread("user-c", "cli", None).await;

        let tool = SessionListTool::new(manager);
        let ctx = JobContext::default();

        let result = tool
            .execute(serde_json::json!({"limit": 2}), &ctx)
            .await
            .unwrap();

        let sessions = result.result.get("sessions").unwrap().as_array().unwrap();
        assert!(sessions.len() <= 2);
    }

    #[tokio::test]
    async fn test_session_list_empty() {
        let manager = Arc::new(SessionManager::new());
        let tool = SessionListTool::new(manager);

        let ctx = JobContext::default();
        let result = tool.execute(serde_json::json!({}), &ctx).await.unwrap();

        let sessions = result.result.get("sessions").unwrap().as_array().unwrap();
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_session_list_kind_filter_no_match() {
        let (manager, _session_id) = setup_session_manager().await;
        let tool = SessionListTool::new(manager);

        let ctx = JobContext::default();
        // Sessions default to "main" kind when metadata has no "kind" field.
        // Filtering by "cron" should exclude them.
        let result = tool
            .execute(serde_json::json!({"kind": "cron"}), &ctx)
            .await
            .unwrap();

        let sessions = result.result.get("sessions").unwrap().as_array().unwrap();
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_session_list_kind_filter_match() {
        let (manager, _session_id) = setup_session_manager().await;
        let tool = SessionListTool::new(manager);

        let ctx = JobContext::default();
        // Sessions without explicit kind in metadata default to "main".
        let result = tool
            .execute(serde_json::json!({"kind": "main"}), &ctx)
            .await
            .unwrap();

        let sessions = result.result.get("sessions").unwrap().as_array().unwrap();
        assert!(!sessions.is_empty());
    }

    #[tokio::test]
    async fn test_session_list_schema() {
        let manager = Arc::new(SessionManager::new());
        let tool = SessionListTool::new(manager);

        assert_eq!(tool.name(), "session_list");
        assert!(!tool.description().is_empty());
        assert!(!tool.requires_approval());
        assert!(!tool.requires_sanitization());

        let schema = tool.parameters_schema();
        let props = schema.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("kind"));
        assert!(props.contains_key("limit"));
    }

    // -----------------------------------------------------------------------
    // SessionHistoryTool tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_session_history_missing_session_id() {
        let manager = Arc::new(SessionManager::new());
        let db = create_stub_db();
        let tool = SessionHistoryTool::new(manager, db);

        let ctx = JobContext::default();
        let result = tool.execute(serde_json::json!({}), &ctx).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidParameters(_) => {}
            other => panic!("Expected InvalidParameters, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_session_history_invalid_uuid() {
        let manager = Arc::new(SessionManager::new());
        let db = create_stub_db();
        let tool = SessionHistoryTool::new(manager, db);

        let ctx = JobContext::default();
        let result = tool
            .execute(serde_json::json!({"session_id": "not-a-uuid"}), &ctx)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_history_not_found() {
        let manager = Arc::new(SessionManager::new());
        let db = create_stub_db();
        let tool = SessionHistoryTool::new(manager, db);

        let ctx = JobContext::default();
        let fake_id = uuid::Uuid::new_v4().to_string();
        let result = tool
            .execute(serde_json::json!({"session_id": fake_id}), &ctx)
            .await
            .unwrap();

        assert!(result.result.get("error").is_some());
    }

    #[tokio::test]
    async fn test_session_history_returns_messages() {
        let manager = Arc::new(SessionManager::new());
        let db = create_stub_db();

        // Create a session with some turns.
        let (session_arc, _thread_id): (Arc<Mutex<Session>>, uuid::Uuid) =
            manager.resolve_thread("default", "cli", None).await;

        let session_id = {
            let mut sess = session_arc.lock().await;
            let thread = sess.get_or_create_thread();
            thread.start_turn("Hello there");
            thread.complete_turn("Hi! How can I help?");
            thread.start_turn("What is 2+2?");
            thread.complete_turn("4");
            sess.id
        };

        let tool = SessionHistoryTool::new(manager, db);
        let ctx = JobContext::default();

        let result = tool
            .execute(
                serde_json::json!({"session_id": session_id.to_string()}),
                &ctx,
            )
            .await
            .unwrap();

        let messages = result.result.get("messages").unwrap().as_array().unwrap();
        assert_eq!(messages.len(), 4); // 2 turns * 2 messages each
        assert_eq!(result.result.get("count").unwrap().as_u64().unwrap(), 4);
    }

    #[tokio::test]
    async fn test_session_history_respects_limit() {
        let manager = Arc::new(SessionManager::new());
        let db = create_stub_db();

        let (session_arc, _thread_id): (Arc<Mutex<Session>>, uuid::Uuid) =
            manager.resolve_thread("default", "cli", None).await;

        let session_id = {
            let mut sess = session_arc.lock().await;
            let thread = sess.get_or_create_thread();
            for i in 0..10 {
                thread.start_turn(format!("msg-{}", i));
                thread.complete_turn(format!("resp-{}", i));
            }
            sess.id
        };

        let tool = SessionHistoryTool::new(manager, db);
        let ctx = JobContext::default();

        let result = tool
            .execute(
                serde_json::json!({"session_id": session_id.to_string(), "limit": 5}),
                &ctx,
            )
            .await
            .unwrap();

        let messages = result.result.get("messages").unwrap().as_array().unwrap();
        assert_eq!(messages.len(), 5);
    }

    #[tokio::test]
    async fn test_session_history_access_denied() {
        let manager = Arc::new(SessionManager::new());
        let db = create_stub_db();

        // Create a session for "other-user".
        let (session_arc, _): (Arc<Mutex<Session>>, uuid::Uuid) =
            manager.resolve_thread("other-user", "cli", None).await;
        let session_id = {
            let sess = session_arc.lock().await;
            sess.id
        };

        let tool = SessionHistoryTool::new(manager, db);

        // Request as "default" user (different from "other-user").
        let ctx = JobContext::default();
        let result = tool
            .execute(
                serde_json::json!({"session_id": session_id.to_string()}),
                &ctx,
            )
            .await
            .unwrap();

        let error = result.result.get("error").unwrap().as_str().unwrap();
        assert!(error.contains("Access denied"));
    }

    #[tokio::test]
    async fn test_session_history_schema() {
        let manager = Arc::new(SessionManager::new());
        let db = create_stub_db();
        let tool = SessionHistoryTool::new(manager, db);

        assert_eq!(tool.name(), "session_history");
        assert!(!tool.requires_approval());

        let schema = tool.parameters_schema();
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&serde_json::json!("session_id")));
    }

    // -----------------------------------------------------------------------
    // SessionSendTool tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_session_send_missing_params() {
        let manager = Arc::new(SessionManager::new());
        let db = create_stub_db();
        let tool = SessionSendTool::new(manager, db);

        let ctx = JobContext::default();

        // Missing session_id
        let result = tool
            .execute(serde_json::json!({"content": "hello"}), &ctx)
            .await;
        assert!(result.is_err());

        // Missing content
        let result = tool
            .execute(
                serde_json::json!({"session_id": uuid::Uuid::new_v4().to_string()}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_send_empty_content() {
        let manager = Arc::new(SessionManager::new());
        let db = create_stub_db();
        let tool = SessionSendTool::new(manager, db);

        let ctx = JobContext::default();
        let result = tool
            .execute(
                serde_json::json!({
                    "session_id": uuid::Uuid::new_v4().to_string(),
                    "content": ""
                }),
                &ctx,
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_send_not_found() {
        let manager = Arc::new(SessionManager::new());
        let db = create_stub_db();
        let tool = SessionSendTool::new(manager, db);

        let ctx = JobContext::default();
        let result = tool
            .execute(
                serde_json::json!({
                    "session_id": uuid::Uuid::new_v4().to_string(),
                    "content": "hello"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.result.get("error").is_some());
    }

    #[tokio::test]
    async fn test_session_send_success() {
        let manager = Arc::new(SessionManager::new());
        let db = create_stub_db();

        // Create a session for "default" user.
        let (session_arc, _): (Arc<Mutex<Session>>, uuid::Uuid) =
            manager.resolve_thread("default", "cli", None).await;
        let session_id = {
            let sess = session_arc.lock().await;
            sess.id
        };

        let tool = SessionSendTool::new(manager.clone(), db);
        let ctx = JobContext::default();

        let result = tool
            .execute(
                serde_json::json!({
                    "session_id": session_id.to_string(),
                    "content": "Cross-session greeting!"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert_eq!(
            result.result.get("status").unwrap().as_str().unwrap(),
            "sent"
        );
        assert!(result.result.get("message_id").is_some());

        // Verify the message was injected into the session's thread.
        let sess = session_arc.lock().await;
        let thread = sess.active_thread().unwrap();
        let last_turn = thread.last_turn().unwrap();
        assert_eq!(last_turn.user_input, "Cross-session greeting!");
    }

    #[tokio::test]
    async fn test_session_send_access_denied() {
        let manager = Arc::new(SessionManager::new());
        let db = create_stub_db();

        // Create a session for "other-user".
        let (session_arc, _): (Arc<Mutex<Session>>, uuid::Uuid) =
            manager.resolve_thread("other-user", "cli", None).await;
        let session_id = {
            let sess = session_arc.lock().await;
            sess.id
        };

        let tool = SessionSendTool::new(manager, db);
        let ctx = JobContext::default(); // user_id = "default"

        let result = tool
            .execute(
                serde_json::json!({
                    "session_id": session_id.to_string(),
                    "content": "Trying to send to someone else"
                }),
                &ctx,
            )
            .await
            .unwrap();

        let error = result.result.get("error").unwrap().as_str().unwrap();
        assert!(error.contains("Access denied"));
    }

    #[tokio::test]
    async fn test_session_send_requires_approval() {
        let manager = Arc::new(SessionManager::new());
        let db = create_stub_db();
        let tool = SessionSendTool::new(manager, db);

        assert!(tool.requires_approval());
    }

    #[tokio::test]
    async fn test_session_send_schema() {
        let manager = Arc::new(SessionManager::new());
        let db = create_stub_db();
        let tool = SessionSendTool::new(manager, db);

        assert_eq!(tool.name(), "session_send");

        let schema = tool.parameters_schema();
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&serde_json::json!("session_id")));
        assert!(required.contains(&serde_json::json!("content")));
    }

    // -----------------------------------------------------------------------
    // Stub Database for tests
    // -----------------------------------------------------------------------

    /// Create a minimal stub database that satisfies `Arc<dyn Database>`.
    ///
    /// The session tools only call `list_conversation_messages` and
    /// `add_conversation_message` on the database, so the stub only needs
    /// those to return reasonable defaults. All other methods are unreachable
    /// in these tests and simply panic if called.
    fn create_stub_db() -> Arc<dyn Database> {
        Arc::new(StubDatabase)
    }

    struct StubDatabase;

    #[async_trait]
    impl Database for StubDatabase {
        async fn run_migrations(&self) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn create_conversation(
            &self,
            _channel: &str,
            _user_id: &str,
            _thread_id: Option<&str>,
        ) -> Result<uuid::Uuid, crate::error::DatabaseError> {
            Ok(uuid::Uuid::new_v4())
        }

        async fn touch_conversation(
            &self,
            _id: uuid::Uuid,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn add_conversation_message(
            &self,
            _conversation_id: uuid::Uuid,
            _role: &str,
            _content: &str,
        ) -> Result<uuid::Uuid, crate::error::DatabaseError> {
            Ok(uuid::Uuid::new_v4())
        }

        async fn ensure_conversation(
            &self,
            _id: uuid::Uuid,
            _channel: &str,
            _user_id: &str,
            _thread_id: Option<&str>,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn list_conversations_with_preview(
            &self,
            _user_id: &str,
            _channel: &str,
            _limit: i64,
        ) -> Result<Vec<crate::history::ConversationSummary>, crate::error::DatabaseError> {
            Ok(vec![])
        }

        async fn get_or_create_assistant_conversation(
            &self,
            _user_id: &str,
            _channel: &str,
        ) -> Result<uuid::Uuid, crate::error::DatabaseError> {
            Ok(uuid::Uuid::new_v4())
        }

        async fn create_conversation_with_metadata(
            &self,
            _channel: &str,
            _user_id: &str,
            _metadata: &serde_json::Value,
        ) -> Result<uuid::Uuid, crate::error::DatabaseError> {
            Ok(uuid::Uuid::new_v4())
        }

        async fn list_conversation_messages_paginated(
            &self,
            _conversation_id: uuid::Uuid,
            _before: Option<chrono::DateTime<chrono::Utc>>,
            _limit: i64,
        ) -> Result<(Vec<crate::history::ConversationMessage>, bool), crate::error::DatabaseError>
        {
            Ok((vec![], false))
        }

        async fn update_conversation_metadata_field(
            &self,
            _id: uuid::Uuid,
            _key: &str,
            _value: &serde_json::Value,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn get_conversation_metadata(
            &self,
            _id: uuid::Uuid,
        ) -> Result<Option<serde_json::Value>, crate::error::DatabaseError> {
            Ok(None)
        }

        async fn list_conversation_messages(
            &self,
            _conversation_id: uuid::Uuid,
        ) -> Result<Vec<crate::history::ConversationMessage>, crate::error::DatabaseError> {
            Ok(vec![])
        }

        async fn conversation_belongs_to_user(
            &self,
            _conversation_id: uuid::Uuid,
            _user_id: &str,
        ) -> Result<bool, crate::error::DatabaseError> {
            Ok(true)
        }

        async fn save_job(&self, _ctx: &JobContext) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn get_job(
            &self,
            _id: uuid::Uuid,
        ) -> Result<Option<JobContext>, crate::error::DatabaseError> {
            Ok(None)
        }

        async fn update_job_status(
            &self,
            _id: uuid::Uuid,
            _status: crate::context::JobState,
            _failure_reason: Option<&str>,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn mark_job_stuck(&self, _id: uuid::Uuid) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn get_stuck_jobs(&self) -> Result<Vec<uuid::Uuid>, crate::error::DatabaseError> {
            Ok(vec![])
        }

        async fn save_action(
            &self,
            _job_id: uuid::Uuid,
            _action: &crate::context::ActionRecord,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn get_job_actions(
            &self,
            _job_id: uuid::Uuid,
        ) -> Result<Vec<crate::context::ActionRecord>, crate::error::DatabaseError> {
            Ok(vec![])
        }

        async fn record_llm_call(
            &self,
            _record: &crate::history::LlmCallRecord<'_>,
        ) -> Result<uuid::Uuid, crate::error::DatabaseError> {
            Ok(uuid::Uuid::new_v4())
        }

        async fn save_estimation_snapshot(
            &self,
            _job_id: uuid::Uuid,
            _category: &str,
            _tool_names: &[String],
            _estimated_cost: rust_decimal::Decimal,
            _estimated_time_secs: i32,
            _estimated_value: rust_decimal::Decimal,
        ) -> Result<uuid::Uuid, crate::error::DatabaseError> {
            Ok(uuid::Uuid::new_v4())
        }

        async fn update_estimation_actuals(
            &self,
            _id: uuid::Uuid,
            _actual_cost: rust_decimal::Decimal,
            _actual_time_secs: i32,
            _actual_value: Option<rust_decimal::Decimal>,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn save_sandbox_job(
            &self,
            _job: &crate::history::SandboxJobRecord,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn get_sandbox_job(
            &self,
            _id: uuid::Uuid,
        ) -> Result<Option<crate::history::SandboxJobRecord>, crate::error::DatabaseError> {
            Ok(None)
        }

        async fn list_sandbox_jobs(
            &self,
        ) -> Result<Vec<crate::history::SandboxJobRecord>, crate::error::DatabaseError> {
            Ok(vec![])
        }

        async fn update_sandbox_job_status(
            &self,
            _id: uuid::Uuid,
            _status: &str,
            _success: Option<bool>,
            _message: Option<&str>,
            _started_at: Option<chrono::DateTime<chrono::Utc>>,
            _completed_at: Option<chrono::DateTime<chrono::Utc>>,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn cleanup_stale_sandbox_jobs(&self) -> Result<u64, crate::error::DatabaseError> {
            Ok(0)
        }

        async fn sandbox_job_summary(
            &self,
        ) -> Result<crate::history::SandboxJobSummary, crate::error::DatabaseError> {
            Ok(crate::history::SandboxJobSummary::default())
        }

        async fn list_sandbox_jobs_for_user(
            &self,
            _user_id: &str,
        ) -> Result<Vec<crate::history::SandboxJobRecord>, crate::error::DatabaseError> {
            Ok(vec![])
        }

        async fn sandbox_job_summary_for_user(
            &self,
            _user_id: &str,
        ) -> Result<crate::history::SandboxJobSummary, crate::error::DatabaseError> {
            Ok(crate::history::SandboxJobSummary::default())
        }

        async fn sandbox_job_belongs_to_user(
            &self,
            _job_id: uuid::Uuid,
            _user_id: &str,
        ) -> Result<bool, crate::error::DatabaseError> {
            Ok(false)
        }

        async fn update_sandbox_job_mode(
            &self,
            _id: uuid::Uuid,
            _mode: &str,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn get_sandbox_job_mode(
            &self,
            _id: uuid::Uuid,
        ) -> Result<Option<String>, crate::error::DatabaseError> {
            Ok(None)
        }

        async fn save_job_event(
            &self,
            _job_id: uuid::Uuid,
            _event_type: &str,
            _data: &serde_json::Value,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn list_job_events(
            &self,
            _job_id: uuid::Uuid,
        ) -> Result<Vec<crate::history::JobEventRecord>, crate::error::DatabaseError> {
            Ok(vec![])
        }

        async fn create_routine(
            &self,
            _routine: &crate::agent::routine::Routine,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn get_routine(
            &self,
            _id: uuid::Uuid,
        ) -> Result<Option<crate::agent::routine::Routine>, crate::error::DatabaseError> {
            Ok(None)
        }

        async fn get_routine_by_name(
            &self,
            _user_id: &str,
            _name: &str,
        ) -> Result<Option<crate::agent::routine::Routine>, crate::error::DatabaseError> {
            Ok(None)
        }

        async fn list_routines(
            &self,
            _user_id: &str,
        ) -> Result<Vec<crate::agent::routine::Routine>, crate::error::DatabaseError> {
            Ok(vec![])
        }

        async fn list_event_routines(
            &self,
        ) -> Result<Vec<crate::agent::routine::Routine>, crate::error::DatabaseError> {
            Ok(vec![])
        }

        async fn list_due_cron_routines(
            &self,
        ) -> Result<Vec<crate::agent::routine::Routine>, crate::error::DatabaseError> {
            Ok(vec![])
        }

        async fn update_routine(
            &self,
            _routine: &crate::agent::routine::Routine,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn update_routine_runtime(
            &self,
            _id: uuid::Uuid,
            _last_run_at: chrono::DateTime<chrono::Utc>,
            _next_fire_at: Option<chrono::DateTime<chrono::Utc>>,
            _run_count: u64,
            _consecutive_failures: u32,
            _state: &serde_json::Value,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn delete_routine(
            &self,
            _id: uuid::Uuid,
        ) -> Result<bool, crate::error::DatabaseError> {
            Ok(false)
        }

        async fn create_routine_run(
            &self,
            _run: &crate::agent::routine::RoutineRun,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn complete_routine_run(
            &self,
            _id: uuid::Uuid,
            _status: crate::agent::routine::RunStatus,
            _result_summary: Option<&str>,
            _tokens_used: Option<i32>,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn list_routine_runs(
            &self,
            _routine_id: uuid::Uuid,
            _limit: i64,
        ) -> Result<Vec<crate::agent::routine::RoutineRun>, crate::error::DatabaseError> {
            Ok(vec![])
        }

        async fn count_running_routine_runs(
            &self,
            _routine_id: uuid::Uuid,
        ) -> Result<i64, crate::error::DatabaseError> {
            Ok(0)
        }

        async fn record_tool_failure(
            &self,
            _tool_name: &str,
            _error_message: &str,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn get_broken_tools(
            &self,
            _threshold: i32,
        ) -> Result<Vec<crate::agent::BrokenTool>, crate::error::DatabaseError> {
            Ok(vec![])
        }

        async fn mark_tool_repaired(
            &self,
            _tool_name: &str,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn increment_repair_attempts(
            &self,
            _tool_name: &str,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn get_setting(
            &self,
            _user_id: &str,
            _key: &str,
        ) -> Result<Option<serde_json::Value>, crate::error::DatabaseError> {
            Ok(None)
        }

        async fn get_setting_full(
            &self,
            _user_id: &str,
            _key: &str,
        ) -> Result<Option<crate::history::SettingRow>, crate::error::DatabaseError> {
            Ok(None)
        }

        async fn set_setting(
            &self,
            _user_id: &str,
            _key: &str,
            _value: &serde_json::Value,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn delete_setting(
            &self,
            _user_id: &str,
            _key: &str,
        ) -> Result<bool, crate::error::DatabaseError> {
            Ok(false)
        }

        async fn list_settings(
            &self,
            _user_id: &str,
        ) -> Result<Vec<crate::history::SettingRow>, crate::error::DatabaseError> {
            Ok(vec![])
        }

        async fn get_all_settings(
            &self,
            _user_id: &str,
        ) -> Result<std::collections::HashMap<String, serde_json::Value>, crate::error::DatabaseError>
        {
            Ok(std::collections::HashMap::new())
        }

        async fn set_all_settings(
            &self,
            _user_id: &str,
            _settings: &std::collections::HashMap<String, serde_json::Value>,
        ) -> Result<(), crate::error::DatabaseError> {
            Ok(())
        }

        async fn has_settings(&self, _user_id: &str) -> Result<bool, crate::error::DatabaseError> {
            Ok(false)
        }

        async fn get_document_by_path(
            &self,
            _user_id: &str,
            _agent_id: Option<uuid::Uuid>,
            _path: &str,
        ) -> Result<crate::workspace::MemoryDocument, crate::error::WorkspaceError> {
            Err(crate::error::WorkspaceError::DocumentNotFound {
                doc_type: "stub".into(),
                user_id: "stub".into(),
            })
        }

        async fn get_document_by_id(
            &self,
            _id: uuid::Uuid,
        ) -> Result<crate::workspace::MemoryDocument, crate::error::WorkspaceError> {
            Err(crate::error::WorkspaceError::DocumentNotFound {
                doc_type: "stub".into(),
                user_id: "stub".into(),
            })
        }

        async fn get_or_create_document_by_path(
            &self,
            _user_id: &str,
            _agent_id: Option<uuid::Uuid>,
            _path: &str,
        ) -> Result<crate::workspace::MemoryDocument, crate::error::WorkspaceError> {
            Err(crate::error::WorkspaceError::DocumentNotFound {
                doc_type: "stub".into(),
                user_id: "stub".into(),
            })
        }

        async fn update_document(
            &self,
            _id: uuid::Uuid,
            _content: &str,
        ) -> Result<(), crate::error::WorkspaceError> {
            Ok(())
        }

        async fn delete_document_by_path(
            &self,
            _user_id: &str,
            _agent_id: Option<uuid::Uuid>,
            _path: &str,
        ) -> Result<(), crate::error::WorkspaceError> {
            Ok(())
        }

        async fn list_directory(
            &self,
            _user_id: &str,
            _agent_id: Option<uuid::Uuid>,
            _directory: &str,
        ) -> Result<Vec<crate::workspace::WorkspaceEntry>, crate::error::WorkspaceError> {
            Ok(vec![])
        }

        async fn list_all_paths(
            &self,
            _user_id: &str,
            _agent_id: Option<uuid::Uuid>,
        ) -> Result<Vec<String>, crate::error::WorkspaceError> {
            Ok(vec![])
        }

        async fn list_documents(
            &self,
            _user_id: &str,
            _agent_id: Option<uuid::Uuid>,
        ) -> Result<Vec<crate::workspace::MemoryDocument>, crate::error::WorkspaceError> {
            Ok(vec![])
        }

        async fn delete_chunks(
            &self,
            _document_id: uuid::Uuid,
        ) -> Result<(), crate::error::WorkspaceError> {
            Ok(())
        }

        async fn insert_chunk(
            &self,
            _document_id: uuid::Uuid,
            _chunk_index: i32,
            _content: &str,
            _embedding: Option<&[f32]>,
        ) -> Result<uuid::Uuid, crate::error::WorkspaceError> {
            Ok(uuid::Uuid::new_v4())
        }

        async fn update_chunk_embedding(
            &self,
            _chunk_id: uuid::Uuid,
            _embedding: &[f32],
        ) -> Result<(), crate::error::WorkspaceError> {
            Ok(())
        }

        async fn get_chunks_without_embeddings(
            &self,
            _user_id: &str,
            _agent_id: Option<uuid::Uuid>,
            _limit: usize,
        ) -> Result<Vec<crate::workspace::MemoryChunk>, crate::error::WorkspaceError> {
            Ok(vec![])
        }

        async fn hybrid_search(
            &self,
            _user_id: &str,
            _agent_id: Option<uuid::Uuid>,
            _query: &str,
            _embedding: Option<&[f32]>,
            _config: &crate::workspace::SearchConfig,
        ) -> Result<Vec<crate::workspace::SearchResult>, crate::error::WorkspaceError> {
            Ok(vec![])
        }

        async fn create_connection(
            &self,
            _connection: &crate::workspace::MemoryConnection,
        ) -> Result<(), crate::error::WorkspaceError> {
            Ok(())
        }

        async fn get_connections(
            &self,
            _document_id: uuid::Uuid,
        ) -> Result<Vec<crate::workspace::MemoryConnection>, crate::error::WorkspaceError> {
            Ok(vec![])
        }

        async fn delete_connection(
            &self,
            _id: uuid::Uuid,
        ) -> Result<(), crate::error::WorkspaceError> {
            Ok(())
        }

        async fn create_space(
            &self,
            _space: &crate::workspace::MemorySpace,
        ) -> Result<(), crate::error::WorkspaceError> {
            Ok(())
        }

        async fn list_spaces(
            &self,
            _user_id: &str,
        ) -> Result<Vec<crate::workspace::MemorySpace>, crate::error::WorkspaceError> {
            Ok(vec![])
        }

        async fn get_space_by_name(
            &self,
            _user_id: &str,
            _name: &str,
        ) -> Result<Option<crate::workspace::MemorySpace>, crate::error::WorkspaceError> {
            Ok(None)
        }

        async fn add_to_space(
            &self,
            _space_id: uuid::Uuid,
            _document_id: uuid::Uuid,
        ) -> Result<(), crate::error::WorkspaceError> {
            Ok(())
        }

        async fn remove_from_space(
            &self,
            _space_id: uuid::Uuid,
            _document_id: uuid::Uuid,
        ) -> Result<(), crate::error::WorkspaceError> {
            Ok(())
        }

        async fn list_space_documents(
            &self,
            _space_id: uuid::Uuid,
        ) -> Result<Vec<crate::workspace::MemoryDocument>, crate::error::WorkspaceError> {
            Ok(vec![])
        }

        async fn delete_space(&self, _id: uuid::Uuid) -> Result<(), crate::error::WorkspaceError> {
            Ok(())
        }

        async fn upsert_profile(
            &self,
            _profile: &crate::workspace::UserProfile,
        ) -> Result<(), crate::error::WorkspaceError> {
            Ok(())
        }

        async fn get_profile(
            &self,
            _user_id: &str,
        ) -> Result<Vec<crate::workspace::UserProfile>, crate::error::WorkspaceError> {
            Ok(vec![])
        }

        async fn get_profile_by_type(
            &self,
            _user_id: &str,
            _profile_type: crate::workspace::ProfileType,
        ) -> Result<Vec<crate::workspace::UserProfile>, crate::error::WorkspaceError> {
            Ok(vec![])
        }

        async fn delete_profile_entry(
            &self,
            _user_id: &str,
            _key: &str,
        ) -> Result<(), crate::error::WorkspaceError> {
            Ok(())
        }

        async fn record_document_access(
            &self,
            _document_id: uuid::Uuid,
        ) -> Result<(), crate::error::WorkspaceError> {
            Ok(())
        }

        async fn update_document_metadata(
            &self,
            _document_id: uuid::Uuid,
            _metadata: &serde_json::Value,
        ) -> Result<(), crate::error::WorkspaceError> {
            Ok(())
        }
    }
}
