//! Configuration hot-reload task.
//!
//! Listens for [`ReloadEvent`]s from a [`ConfigWatcher`] and reloads
//! configuration from the database, updating the shared [`HotReloadConfig`]
//! so all components see the new values without a restart.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::bootstrap::BootstrapConfig;
use crate::config::Config;
use crate::db::Database;
use crate::hot_reload::{HotReloadConfig, ReloadEvent};

/// Default debounce duration to coalesce rapid reload events.
const DEBOUNCE_DURATION: Duration = Duration::from_millis(500);

/// Spawn a background task that reloads configuration on [`ReloadEvent`]s.
///
/// The task listens on the provided broadcast receiver and, after a 500ms
/// debounce window, reloads `Config` from the database and updates the
/// shared `HotReloadConfig`. This means rapid successive events (e.g.
/// multiple file writes) are collapsed into a single reload.
///
/// # Arguments
///
/// * `rx` - Broadcast receiver for reload events (obtained via
///   [`ConfigWatcher::subscribe`]).
/// * `hot_config` - The shared hot-reload config container to update.
/// * `db` - Database handle used to reload settings via `Config::from_db`.
/// * `user_id` - The user ID passed to `Config::from_db`.
///
/// # Returns
///
/// A `JoinHandle` for the spawned task. The task runs until the broadcast
/// sender is dropped (all senders gone) or the tokio runtime shuts down.
pub fn spawn_config_reload_task(
    rx: broadcast::Receiver<ReloadEvent>,
    hot_config: HotReloadConfig<Config>,
    db: Arc<dyn Database>,
    user_id: String,
) -> JoinHandle<()> {
    tokio::spawn(config_reload_loop(rx, hot_config, db, user_id))
}

/// The inner reload loop, extracted for testability.
async fn config_reload_loop(
    mut rx: broadcast::Receiver<ReloadEvent>,
    hot_config: HotReloadConfig<Config>,
    db: Arc<dyn Database>,
    user_id: String,
) {
    tracing::info!("Config hot-reload task started");

    loop {
        // Wait for the first event.
        let first_event = match rx.recv().await {
            Ok(event) => event,
            Err(broadcast::error::RecvError::Closed) => {
                tracing::info!("Config reload channel closed, stopping hot-reload task");
                return;
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(
                    "Config reload receiver lagged by {} events, performing reload",
                    n
                );
                ReloadEvent::DatabaseChanged
            }
        };

        tracing::debug!("Config reload event received: {:?}", first_event);

        // Debounce: wait a short period and drain any additional events
        // that arrive within the window to avoid redundant reloads.
        tokio::time::sleep(DEBOUNCE_DURATION).await;
        let mut coalesced_count: u64 = 0;
        loop {
            match rx.try_recv() {
                Ok(_) => {
                    coalesced_count += 1;
                }
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(broadcast::error::TryRecvError::Closed) => {
                    tracing::info!(
                        "Config reload channel closed during debounce, stopping hot-reload task"
                    );
                    return;
                }
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    tracing::warn!(
                        "Config reload receiver lagged by {} events during debounce",
                        n
                    );
                    coalesced_count += n;
                    break;
                }
            }
        }

        if coalesced_count > 0 {
            tracing::debug!(
                "Coalesced {} additional reload events during debounce window",
                coalesced_count
            );
        }

        // Reload configuration from the database.
        let old_generation = hot_config.generation();
        let bootstrap = BootstrapConfig::load();

        match Config::from_db(db.as_ref(), &user_id, &bootstrap).await {
            Ok(new_config) => {
                hot_config.update(new_config).await;
                let new_generation = hot_config.generation();
                tracing::info!(
                    "Configuration reloaded successfully (generation {} -> {})",
                    old_generation,
                    new_generation
                );
            }
            Err(e) => {
                tracing::error!("Failed to reload configuration: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use std::time::Duration;

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    use crate::agent::BrokenTool;
    use crate::agent::routine::{Routine, RoutineRun, RunStatus};
    use crate::context::{ActionRecord, JobContext, JobState};
    use crate::error::{DatabaseError, WorkspaceError};
    use crate::history::{
        ConversationMessage, ConversationSummary, JobEventRecord, LlmCallRecord, SandboxJobRecord,
        SandboxJobSummary, SettingRow,
    };
    use crate::workspace::{
        MemoryChunk, MemoryConnection, MemoryDocument, MemorySpace, ProfileType, SearchConfig,
        SearchResult, UserProfile, WorkspaceEntry,
    };

    /// Create a test config with minimal required env vars.
    ///
    /// Sets `DATABASE_URL` to a dummy postgres value so that
    /// `Config::from_env()` and `Config::from_db()` don't fail
    /// due to missing required configuration.
    async fn test_config() -> Config {
        // Safety: test-only; potential races are acceptable in test contexts.
        unsafe {
            std::env::set_var("DATABASE_URL", "postgres://localhost:5432/ironclaw_test");
        }
        Config::from_env().await.unwrap()
    }

    /// Generates a full Database trait implementation for a test stub struct.
    ///
    /// All methods return Ok with empty/default values, except `get_all_settings`
    /// which is overridable. By default it returns an empty HashMap.
    macro_rules! impl_stub_database {
        ($name:ident) => {
            #[async_trait]
            impl Database for $name {
                async fn run_migrations(&self) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn create_conversation(
                    &self,
                    _channel: &str,
                    _user_id: &str,
                    _thread_id: Option<&str>,
                ) -> Result<Uuid, DatabaseError> {
                    Ok(Uuid::new_v4())
                }
                async fn touch_conversation(&self, _id: Uuid) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn add_conversation_message(
                    &self,
                    _conversation_id: Uuid,
                    _role: &str,
                    _content: &str,
                ) -> Result<Uuid, DatabaseError> {
                    Ok(Uuid::new_v4())
                }
                async fn ensure_conversation(
                    &self,
                    _id: Uuid,
                    _channel: &str,
                    _user_id: &str,
                    _thread_id: Option<&str>,
                ) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn list_conversations_with_preview(
                    &self,
                    _user_id: &str,
                    _channel: &str,
                    _limit: i64,
                ) -> Result<Vec<ConversationSummary>, DatabaseError> {
                    Ok(vec![])
                }
                async fn get_or_create_assistant_conversation(
                    &self,
                    _user_id: &str,
                    _channel: &str,
                ) -> Result<Uuid, DatabaseError> {
                    Ok(Uuid::new_v4())
                }
                async fn create_conversation_with_metadata(
                    &self,
                    _channel: &str,
                    _user_id: &str,
                    _metadata: &serde_json::Value,
                ) -> Result<Uuid, DatabaseError> {
                    Ok(Uuid::new_v4())
                }
                async fn list_conversation_messages_paginated(
                    &self,
                    _conversation_id: Uuid,
                    _before: Option<DateTime<Utc>>,
                    _limit: i64,
                ) -> Result<(Vec<ConversationMessage>, bool), DatabaseError> {
                    Ok((vec![], false))
                }
                async fn update_conversation_metadata_field(
                    &self,
                    _id: Uuid,
                    _key: &str,
                    _value: &serde_json::Value,
                ) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn get_conversation_metadata(
                    &self,
                    _id: Uuid,
                ) -> Result<Option<serde_json::Value>, DatabaseError> {
                    Ok(None)
                }
                async fn list_conversation_messages(
                    &self,
                    _conversation_id: Uuid,
                ) -> Result<Vec<ConversationMessage>, DatabaseError> {
                    Ok(vec![])
                }
                async fn conversation_belongs_to_user(
                    &self,
                    _conversation_id: Uuid,
                    _user_id: &str,
                ) -> Result<bool, DatabaseError> {
                    Ok(true)
                }
                async fn save_job(&self, _ctx: &JobContext) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn get_job(&self, _id: Uuid) -> Result<Option<JobContext>, DatabaseError> {
                    Ok(None)
                }
                async fn update_job_status(
                    &self,
                    _id: Uuid,
                    _status: JobState,
                    _failure_reason: Option<&str>,
                ) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn mark_job_stuck(&self, _id: Uuid) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn get_stuck_jobs(&self) -> Result<Vec<Uuid>, DatabaseError> {
                    Ok(vec![])
                }
                async fn save_action(
                    &self,
                    _job_id: Uuid,
                    _action: &ActionRecord,
                ) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn get_job_actions(
                    &self,
                    _job_id: Uuid,
                ) -> Result<Vec<ActionRecord>, DatabaseError> {
                    Ok(vec![])
                }
                async fn record_llm_call(
                    &self,
                    _record: &LlmCallRecord<'_>,
                ) -> Result<Uuid, DatabaseError> {
                    Ok(Uuid::new_v4())
                }
                async fn save_estimation_snapshot(
                    &self,
                    _job_id: Uuid,
                    _category: &str,
                    _tool_names: &[String],
                    _estimated_cost: Decimal,
                    _estimated_time_secs: i32,
                    _estimated_value: Decimal,
                ) -> Result<Uuid, DatabaseError> {
                    Ok(Uuid::new_v4())
                }
                async fn update_estimation_actuals(
                    &self,
                    _id: Uuid,
                    _actual_cost: Decimal,
                    _actual_time_secs: i32,
                    _actual_value: Option<Decimal>,
                ) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn save_sandbox_job(
                    &self,
                    _job: &SandboxJobRecord,
                ) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn get_sandbox_job(
                    &self,
                    _id: Uuid,
                ) -> Result<Option<SandboxJobRecord>, DatabaseError> {
                    Ok(None)
                }
                async fn list_sandbox_jobs(&self) -> Result<Vec<SandboxJobRecord>, DatabaseError> {
                    Ok(vec![])
                }
                async fn update_sandbox_job_status(
                    &self,
                    _id: Uuid,
                    _status: &str,
                    _success: Option<bool>,
                    _message: Option<&str>,
                    _started_at: Option<DateTime<Utc>>,
                    _completed_at: Option<DateTime<Utc>>,
                ) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn cleanup_stale_sandbox_jobs(&self) -> Result<u64, DatabaseError> {
                    Ok(0)
                }
                async fn sandbox_job_summary(&self) -> Result<SandboxJobSummary, DatabaseError> {
                    Ok(SandboxJobSummary::default())
                }
                async fn list_sandbox_jobs_for_user(
                    &self,
                    _user_id: &str,
                ) -> Result<Vec<SandboxJobRecord>, DatabaseError> {
                    Ok(vec![])
                }
                async fn sandbox_job_summary_for_user(
                    &self,
                    _user_id: &str,
                ) -> Result<SandboxJobSummary, DatabaseError> {
                    Ok(SandboxJobSummary::default())
                }
                async fn sandbox_job_belongs_to_user(
                    &self,
                    _job_id: Uuid,
                    _user_id: &str,
                ) -> Result<bool, DatabaseError> {
                    Ok(false)
                }
                async fn update_sandbox_job_mode(
                    &self,
                    _id: Uuid,
                    _mode: &str,
                ) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn get_sandbox_job_mode(
                    &self,
                    _id: Uuid,
                ) -> Result<Option<String>, DatabaseError> {
                    Ok(None)
                }
                async fn save_job_event(
                    &self,
                    _job_id: Uuid,
                    _event_type: &str,
                    _data: &serde_json::Value,
                ) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn list_job_events(
                    &self,
                    _job_id: Uuid,
                ) -> Result<Vec<JobEventRecord>, DatabaseError> {
                    Ok(vec![])
                }
                async fn create_routine(&self, _routine: &Routine) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn get_routine(&self, _id: Uuid) -> Result<Option<Routine>, DatabaseError> {
                    Ok(None)
                }
                async fn get_routine_by_name(
                    &self,
                    _user_id: &str,
                    _name: &str,
                ) -> Result<Option<Routine>, DatabaseError> {
                    Ok(None)
                }
                async fn list_routines(
                    &self,
                    _user_id: &str,
                ) -> Result<Vec<Routine>, DatabaseError> {
                    Ok(vec![])
                }
                async fn list_event_routines(&self) -> Result<Vec<Routine>, DatabaseError> {
                    Ok(vec![])
                }
                async fn list_due_cron_routines(&self) -> Result<Vec<Routine>, DatabaseError> {
                    Ok(vec![])
                }
                async fn update_routine(&self, _routine: &Routine) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn update_routine_runtime(
                    &self,
                    _id: Uuid,
                    _last_run_at: DateTime<Utc>,
                    _next_fire_at: Option<DateTime<Utc>>,
                    _run_count: u64,
                    _consecutive_failures: u32,
                    _state: &serde_json::Value,
                ) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn delete_routine(&self, _id: Uuid) -> Result<bool, DatabaseError> {
                    Ok(false)
                }
                async fn create_routine_run(&self, _run: &RoutineRun) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn complete_routine_run(
                    &self,
                    _id: Uuid,
                    _status: RunStatus,
                    _result_summary: Option<&str>,
                    _tokens_used: Option<i32>,
                ) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn list_routine_runs(
                    &self,
                    _routine_id: Uuid,
                    _limit: i64,
                ) -> Result<Vec<RoutineRun>, DatabaseError> {
                    Ok(vec![])
                }
                async fn count_running_routine_runs(
                    &self,
                    _routine_id: Uuid,
                ) -> Result<i64, DatabaseError> {
                    Ok(0)
                }
                async fn record_tool_failure(
                    &self,
                    _tool_name: &str,
                    _error_message: &str,
                ) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn get_broken_tools(
                    &self,
                    _threshold: i32,
                ) -> Result<Vec<BrokenTool>, DatabaseError> {
                    Ok(vec![])
                }
                async fn mark_tool_repaired(&self, _tool_name: &str) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn increment_repair_attempts(
                    &self,
                    _tool_name: &str,
                ) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn get_setting(
                    &self,
                    _user_id: &str,
                    _key: &str,
                ) -> Result<Option<serde_json::Value>, DatabaseError> {
                    Ok(None)
                }
                async fn get_setting_full(
                    &self,
                    _user_id: &str,
                    _key: &str,
                ) -> Result<Option<SettingRow>, DatabaseError> {
                    Ok(None)
                }
                async fn set_setting(
                    &self,
                    _user_id: &str,
                    _key: &str,
                    _value: &serde_json::Value,
                ) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn delete_setting(
                    &self,
                    _user_id: &str,
                    _key: &str,
                ) -> Result<bool, DatabaseError> {
                    Ok(false)
                }
                async fn list_settings(
                    &self,
                    _user_id: &str,
                ) -> Result<Vec<SettingRow>, DatabaseError> {
                    Ok(vec![])
                }
                async fn get_all_settings(
                    &self,
                    _user_id: &str,
                ) -> Result<HashMap<String, serde_json::Value>, DatabaseError> {
                    Ok(HashMap::new())
                }
                async fn set_all_settings(
                    &self,
                    _user_id: &str,
                    _settings: &HashMap<String, serde_json::Value>,
                ) -> Result<(), DatabaseError> {
                    Ok(())
                }
                async fn has_settings(&self, _user_id: &str) -> Result<bool, DatabaseError> {
                    Ok(false)
                }
                async fn get_document_by_path(
                    &self,
                    _user_id: &str,
                    _agent_id: Option<Uuid>,
                    _path: &str,
                ) -> Result<MemoryDocument, WorkspaceError> {
                    Err(WorkspaceError::DocumentNotFound {
                        doc_type: "stub".into(),
                        user_id: "stub".into(),
                    })
                }
                async fn get_document_by_id(
                    &self,
                    _id: Uuid,
                ) -> Result<MemoryDocument, WorkspaceError> {
                    Err(WorkspaceError::DocumentNotFound {
                        doc_type: "stub".into(),
                        user_id: "stub".into(),
                    })
                }
                async fn get_or_create_document_by_path(
                    &self,
                    _user_id: &str,
                    _agent_id: Option<Uuid>,
                    _path: &str,
                ) -> Result<MemoryDocument, WorkspaceError> {
                    Err(WorkspaceError::DocumentNotFound {
                        doc_type: "stub".into(),
                        user_id: "stub".into(),
                    })
                }
                async fn update_document(
                    &self,
                    _id: Uuid,
                    _content: &str,
                ) -> Result<(), WorkspaceError> {
                    Ok(())
                }
                async fn delete_document_by_path(
                    &self,
                    _user_id: &str,
                    _agent_id: Option<Uuid>,
                    _path: &str,
                ) -> Result<(), WorkspaceError> {
                    Ok(())
                }
                async fn list_directory(
                    &self,
                    _user_id: &str,
                    _agent_id: Option<Uuid>,
                    _directory: &str,
                ) -> Result<Vec<WorkspaceEntry>, WorkspaceError> {
                    Ok(vec![])
                }
                async fn list_all_paths(
                    &self,
                    _user_id: &str,
                    _agent_id: Option<Uuid>,
                ) -> Result<Vec<String>, WorkspaceError> {
                    Ok(vec![])
                }
                async fn list_documents(
                    &self,
                    _user_id: &str,
                    _agent_id: Option<Uuid>,
                ) -> Result<Vec<MemoryDocument>, WorkspaceError> {
                    Ok(vec![])
                }
                async fn delete_chunks(&self, _document_id: Uuid) -> Result<(), WorkspaceError> {
                    Ok(())
                }
                async fn insert_chunk(
                    &self,
                    _document_id: Uuid,
                    _chunk_index: i32,
                    _content: &str,
                    _embedding: Option<&[f32]>,
                ) -> Result<Uuid, WorkspaceError> {
                    Ok(Uuid::new_v4())
                }
                async fn update_chunk_embedding(
                    &self,
                    _chunk_id: Uuid,
                    _embedding: &[f32],
                ) -> Result<(), WorkspaceError> {
                    Ok(())
                }
                async fn get_chunks_without_embeddings(
                    &self,
                    _user_id: &str,
                    _agent_id: Option<Uuid>,
                    _limit: usize,
                ) -> Result<Vec<MemoryChunk>, WorkspaceError> {
                    Ok(vec![])
                }
                async fn hybrid_search(
                    &self,
                    _user_id: &str,
                    _agent_id: Option<Uuid>,
                    _query: &str,
                    _embedding: Option<&[f32]>,
                    _config: &SearchConfig,
                ) -> Result<Vec<SearchResult>, WorkspaceError> {
                    Ok(vec![])
                }
                async fn create_connection(
                    &self,
                    _connection: &MemoryConnection,
                ) -> Result<(), WorkspaceError> {
                    Ok(())
                }
                async fn get_connections(
                    &self,
                    _document_id: Uuid,
                ) -> Result<Vec<MemoryConnection>, WorkspaceError> {
                    Ok(vec![])
                }
                async fn delete_connection(&self, _id: Uuid) -> Result<(), WorkspaceError> {
                    Ok(())
                }
                async fn create_space(&self, _space: &MemorySpace) -> Result<(), WorkspaceError> {
                    Ok(())
                }
                async fn list_spaces(
                    &self,
                    _user_id: &str,
                ) -> Result<Vec<MemorySpace>, WorkspaceError> {
                    Ok(vec![])
                }
                async fn get_space_by_name(
                    &self,
                    _user_id: &str,
                    _name: &str,
                ) -> Result<Option<MemorySpace>, WorkspaceError> {
                    Ok(None)
                }
                async fn add_to_space(
                    &self,
                    _space_id: Uuid,
                    _document_id: Uuid,
                ) -> Result<(), WorkspaceError> {
                    Ok(())
                }
                async fn remove_from_space(
                    &self,
                    _space_id: Uuid,
                    _document_id: Uuid,
                ) -> Result<(), WorkspaceError> {
                    Ok(())
                }
                async fn list_space_documents(
                    &self,
                    _space_id: Uuid,
                ) -> Result<Vec<MemoryDocument>, WorkspaceError> {
                    Ok(vec![])
                }
                async fn delete_space(&self, _id: Uuid) -> Result<(), WorkspaceError> {
                    Ok(())
                }
                async fn upsert_profile(
                    &self,
                    _profile: &UserProfile,
                ) -> Result<(), WorkspaceError> {
                    Ok(())
                }
                async fn get_profile(
                    &self,
                    _user_id: &str,
                ) -> Result<Vec<UserProfile>, WorkspaceError> {
                    Ok(vec![])
                }
                async fn get_profile_by_type(
                    &self,
                    _user_id: &str,
                    _profile_type: ProfileType,
                ) -> Result<Vec<UserProfile>, WorkspaceError> {
                    Ok(vec![])
                }
                async fn delete_profile_entry(
                    &self,
                    _user_id: &str,
                    _key: &str,
                ) -> Result<(), WorkspaceError> {
                    Ok(())
                }
                async fn record_document_access(
                    &self,
                    _document_id: Uuid,
                ) -> Result<(), WorkspaceError> {
                    Ok(())
                }
                async fn update_document_metadata(
                    &self,
                    _document_id: Uuid,
                    _metadata: &serde_json::Value,
                ) -> Result<(), WorkspaceError> {
                    Ok(())
                }
            }
        };
    }

    /// Minimal stub database that returns empty settings.
    struct StubDatabase;
    impl_stub_database!(StubDatabase);

    #[tokio::test]
    async fn test_reload_on_file_changed_event() {
        let (tx, rx) = broadcast::channel::<ReloadEvent>(16);
        let initial_config = test_config().await;
        let hot_config = HotReloadConfig::new(initial_config);
        let db: Arc<dyn Database> = Arc::new(StubDatabase);

        assert_eq!(hot_config.generation(), 0);

        let handle = spawn_config_reload_task(rx, hot_config.clone(), db, "test_user".to_string());

        // Send a file-changed event.
        tx.send(ReloadEvent::FileChanged {
            path: "/tmp/test.toml".into(),
        })
        .unwrap();

        // Wait for the debounce window + processing time.
        tokio::time::sleep(Duration::from_millis(800)).await;

        assert_eq!(
            hot_config.generation(),
            1,
            "Config generation should have incremented after reload"
        );

        // Clean up.
        drop(tx);
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
    }

    #[tokio::test]
    async fn test_reload_on_database_changed_event() {
        let (tx, rx) = broadcast::channel::<ReloadEvent>(16);
        let initial_config = test_config().await;
        let hot_config = HotReloadConfig::new(initial_config);
        let db: Arc<dyn Database> = Arc::new(StubDatabase);

        let handle = spawn_config_reload_task(rx, hot_config.clone(), db, "test_user".to_string());

        tx.send(ReloadEvent::DatabaseChanged).unwrap();
        tokio::time::sleep(Duration::from_millis(800)).await;

        assert_eq!(hot_config.generation(), 1);

        drop(tx);
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
    }

    #[tokio::test]
    async fn test_reload_on_env_changed_event() {
        let (tx, rx) = broadcast::channel::<ReloadEvent>(16);
        let initial_config = test_config().await;
        let hot_config = HotReloadConfig::new(initial_config);
        let db: Arc<dyn Database> = Arc::new(StubDatabase);

        let handle = spawn_config_reload_task(rx, hot_config.clone(), db, "test_user".to_string());

        tx.send(ReloadEvent::EnvChanged).unwrap();
        tokio::time::sleep(Duration::from_millis(800)).await;

        assert_eq!(hot_config.generation(), 1);

        drop(tx);
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
    }

    #[tokio::test]
    async fn test_debounce_coalesces_rapid_events() {
        let (tx, rx) = broadcast::channel::<ReloadEvent>(16);
        let initial_config = test_config().await;
        let hot_config = HotReloadConfig::new(initial_config);
        let db: Arc<dyn Database> = Arc::new(StubDatabase);

        let handle = spawn_config_reload_task(rx, hot_config.clone(), db, "test_user".to_string());

        // Send multiple events in rapid succession.
        for i in 0..5 {
            tx.send(ReloadEvent::FileChanged {
                path: format!("/tmp/config_{}.toml", i).into(),
            })
            .unwrap();
        }

        // Wait for debounce + processing.
        tokio::time::sleep(Duration::from_millis(800)).await;

        // All events should be coalesced into a single reload.
        assert_eq!(
            hot_config.generation(),
            1,
            "Rapid events should be coalesced into a single reload"
        );

        drop(tx);
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
    }

    #[tokio::test]
    async fn test_separate_reloads_after_debounce_window() {
        let (tx, rx) = broadcast::channel::<ReloadEvent>(16);
        let initial_config = test_config().await;
        let hot_config = HotReloadConfig::new(initial_config);
        let db: Arc<dyn Database> = Arc::new(StubDatabase);

        let handle = spawn_config_reload_task(rx, hot_config.clone(), db, "test_user".to_string());

        // First event.
        tx.send(ReloadEvent::DatabaseChanged).unwrap();
        tokio::time::sleep(Duration::from_millis(800)).await;
        assert_eq!(hot_config.generation(), 1);

        // Second event after debounce window has passed.
        tx.send(ReloadEvent::EnvChanged).unwrap();
        tokio::time::sleep(Duration::from_millis(800)).await;
        assert_eq!(hot_config.generation(), 2);

        drop(tx);
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
    }

    #[tokio::test]
    async fn test_task_stops_when_sender_dropped() {
        let (tx, rx) = broadcast::channel::<ReloadEvent>(16);
        let initial_config = test_config().await;
        let hot_config = HotReloadConfig::new(initial_config);
        let db: Arc<dyn Database> = Arc::new(StubDatabase);

        let handle = spawn_config_reload_task(rx, hot_config.clone(), db, "test_user".to_string());

        // Drop the sender; the task should exit.
        drop(tx);

        let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
        assert!(
            result.is_ok(),
            "Task should have stopped after sender was dropped"
        );
    }

    #[tokio::test]
    async fn test_hot_config_readable_during_reload() {
        let (tx, rx) = broadcast::channel::<ReloadEvent>(16);
        let initial_config = test_config().await;
        let initial_agent_name = initial_config.agent.name.clone();
        let hot_config = HotReloadConfig::new(initial_config);
        let db: Arc<dyn Database> = Arc::new(StubDatabase);

        let handle = spawn_config_reload_task(rx, hot_config.clone(), db, "test_user".to_string());

        // Config should be readable before any events.
        let config = hot_config.get().await;
        assert_eq!(config.agent.name, initial_agent_name);

        // Trigger reload and verify config is still accessible.
        tx.send(ReloadEvent::DatabaseChanged).unwrap();
        tokio::time::sleep(Duration::from_millis(800)).await;

        let config = hot_config.get().await;
        // The stub DB returns empty settings, so from_db will use defaults/env.
        // The key thing is that we get a valid config back.
        assert!(!config.agent.name.is_empty());

        drop(tx);
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
    }

    #[tokio::test]
    async fn test_reload_survives_db_error() {
        /// A database stub that always fails on get_all_settings.
        struct FailingDatabase;

        // Use the macro for the base implementation, then override get_all_settings.
        // Unfortunately macros can't selectively override, so we implement manually
        // for the one method that differs. We re-use the macro approach by implementing
        // a wrapper. Instead, let's just implement the full trait with the override.
        #[async_trait]
        impl Database for FailingDatabase {
            async fn run_migrations(&self) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn create_conversation(
                &self,
                _channel: &str,
                _user_id: &str,
                _thread_id: Option<&str>,
            ) -> Result<Uuid, DatabaseError> {
                Ok(Uuid::new_v4())
            }
            async fn touch_conversation(&self, _id: Uuid) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn add_conversation_message(
                &self,
                _conversation_id: Uuid,
                _role: &str,
                _content: &str,
            ) -> Result<Uuid, DatabaseError> {
                Ok(Uuid::new_v4())
            }
            async fn ensure_conversation(
                &self,
                _id: Uuid,
                _channel: &str,
                _user_id: &str,
                _thread_id: Option<&str>,
            ) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn list_conversations_with_preview(
                &self,
                _user_id: &str,
                _channel: &str,
                _limit: i64,
            ) -> Result<Vec<ConversationSummary>, DatabaseError> {
                Ok(vec![])
            }
            async fn get_or_create_assistant_conversation(
                &self,
                _user_id: &str,
                _channel: &str,
            ) -> Result<Uuid, DatabaseError> {
                Ok(Uuid::new_v4())
            }
            async fn create_conversation_with_metadata(
                &self,
                _channel: &str,
                _user_id: &str,
                _metadata: &serde_json::Value,
            ) -> Result<Uuid, DatabaseError> {
                Ok(Uuid::new_v4())
            }
            async fn list_conversation_messages_paginated(
                &self,
                _conversation_id: Uuid,
                _before: Option<DateTime<Utc>>,
                _limit: i64,
            ) -> Result<(Vec<ConversationMessage>, bool), DatabaseError> {
                Ok((vec![], false))
            }
            async fn update_conversation_metadata_field(
                &self,
                _id: Uuid,
                _key: &str,
                _value: &serde_json::Value,
            ) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn get_conversation_metadata(
                &self,
                _id: Uuid,
            ) -> Result<Option<serde_json::Value>, DatabaseError> {
                Ok(None)
            }
            async fn list_conversation_messages(
                &self,
                _conversation_id: Uuid,
            ) -> Result<Vec<ConversationMessage>, DatabaseError> {
                Ok(vec![])
            }
            async fn conversation_belongs_to_user(
                &self,
                _conversation_id: Uuid,
                _user_id: &str,
            ) -> Result<bool, DatabaseError> {
                Ok(true)
            }
            async fn save_job(&self, _ctx: &JobContext) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn get_job(&self, _id: Uuid) -> Result<Option<JobContext>, DatabaseError> {
                Ok(None)
            }
            async fn update_job_status(
                &self,
                _id: Uuid,
                _status: JobState,
                _failure_reason: Option<&str>,
            ) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn mark_job_stuck(&self, _id: Uuid) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn get_stuck_jobs(&self) -> Result<Vec<Uuid>, DatabaseError> {
                Ok(vec![])
            }
            async fn save_action(
                &self,
                _job_id: Uuid,
                _action: &ActionRecord,
            ) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn get_job_actions(
                &self,
                _job_id: Uuid,
            ) -> Result<Vec<ActionRecord>, DatabaseError> {
                Ok(vec![])
            }
            async fn record_llm_call(
                &self,
                _record: &LlmCallRecord<'_>,
            ) -> Result<Uuid, DatabaseError> {
                Ok(Uuid::new_v4())
            }
            async fn save_estimation_snapshot(
                &self,
                _job_id: Uuid,
                _category: &str,
                _tool_names: &[String],
                _estimated_cost: Decimal,
                _estimated_time_secs: i32,
                _estimated_value: Decimal,
            ) -> Result<Uuid, DatabaseError> {
                Ok(Uuid::new_v4())
            }
            async fn update_estimation_actuals(
                &self,
                _id: Uuid,
                _actual_cost: Decimal,
                _actual_time_secs: i32,
                _actual_value: Option<Decimal>,
            ) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn save_sandbox_job(&self, _job: &SandboxJobRecord) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn get_sandbox_job(
                &self,
                _id: Uuid,
            ) -> Result<Option<SandboxJobRecord>, DatabaseError> {
                Ok(None)
            }
            async fn list_sandbox_jobs(&self) -> Result<Vec<SandboxJobRecord>, DatabaseError> {
                Ok(vec![])
            }
            async fn update_sandbox_job_status(
                &self,
                _id: Uuid,
                _status: &str,
                _success: Option<bool>,
                _message: Option<&str>,
                _started_at: Option<DateTime<Utc>>,
                _completed_at: Option<DateTime<Utc>>,
            ) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn cleanup_stale_sandbox_jobs(&self) -> Result<u64, DatabaseError> {
                Ok(0)
            }
            async fn sandbox_job_summary(&self) -> Result<SandboxJobSummary, DatabaseError> {
                Ok(SandboxJobSummary::default())
            }
            async fn list_sandbox_jobs_for_user(
                &self,
                _user_id: &str,
            ) -> Result<Vec<SandboxJobRecord>, DatabaseError> {
                Ok(vec![])
            }
            async fn sandbox_job_summary_for_user(
                &self,
                _user_id: &str,
            ) -> Result<SandboxJobSummary, DatabaseError> {
                Ok(SandboxJobSummary::default())
            }
            async fn sandbox_job_belongs_to_user(
                &self,
                _job_id: Uuid,
                _user_id: &str,
            ) -> Result<bool, DatabaseError> {
                Ok(false)
            }
            async fn update_sandbox_job_mode(
                &self,
                _id: Uuid,
                _mode: &str,
            ) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn get_sandbox_job_mode(
                &self,
                _id: Uuid,
            ) -> Result<Option<String>, DatabaseError> {
                Ok(None)
            }
            async fn save_job_event(
                &self,
                _job_id: Uuid,
                _event_type: &str,
                _data: &serde_json::Value,
            ) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn list_job_events(
                &self,
                _job_id: Uuid,
            ) -> Result<Vec<JobEventRecord>, DatabaseError> {
                Ok(vec![])
            }
            async fn create_routine(&self, _routine: &Routine) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn get_routine(&self, _id: Uuid) -> Result<Option<Routine>, DatabaseError> {
                Ok(None)
            }
            async fn get_routine_by_name(
                &self,
                _user_id: &str,
                _name: &str,
            ) -> Result<Option<Routine>, DatabaseError> {
                Ok(None)
            }
            async fn list_routines(&self, _user_id: &str) -> Result<Vec<Routine>, DatabaseError> {
                Ok(vec![])
            }
            async fn list_event_routines(&self) -> Result<Vec<Routine>, DatabaseError> {
                Ok(vec![])
            }
            async fn list_due_cron_routines(&self) -> Result<Vec<Routine>, DatabaseError> {
                Ok(vec![])
            }
            async fn update_routine(&self, _routine: &Routine) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn update_routine_runtime(
                &self,
                _id: Uuid,
                _last_run_at: DateTime<Utc>,
                _next_fire_at: Option<DateTime<Utc>>,
                _run_count: u64,
                _consecutive_failures: u32,
                _state: &serde_json::Value,
            ) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn delete_routine(&self, _id: Uuid) -> Result<bool, DatabaseError> {
                Ok(false)
            }
            async fn create_routine_run(&self, _run: &RoutineRun) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn complete_routine_run(
                &self,
                _id: Uuid,
                _status: RunStatus,
                _result_summary: Option<&str>,
                _tokens_used: Option<i32>,
            ) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn list_routine_runs(
                &self,
                _routine_id: Uuid,
                _limit: i64,
            ) -> Result<Vec<RoutineRun>, DatabaseError> {
                Ok(vec![])
            }
            async fn count_running_routine_runs(
                &self,
                _routine_id: Uuid,
            ) -> Result<i64, DatabaseError> {
                Ok(0)
            }
            async fn record_tool_failure(
                &self,
                _tool_name: &str,
                _error_message: &str,
            ) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn get_broken_tools(
                &self,
                _threshold: i32,
            ) -> Result<Vec<BrokenTool>, DatabaseError> {
                Ok(vec![])
            }
            async fn mark_tool_repaired(&self, _tool_name: &str) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn increment_repair_attempts(
                &self,
                _tool_name: &str,
            ) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn get_setting(
                &self,
                _user_id: &str,
                _key: &str,
            ) -> Result<Option<serde_json::Value>, DatabaseError> {
                Ok(None)
            }
            async fn get_setting_full(
                &self,
                _user_id: &str,
                _key: &str,
            ) -> Result<Option<SettingRow>, DatabaseError> {
                Ok(None)
            }
            async fn set_setting(
                &self,
                _user_id: &str,
                _key: &str,
                _value: &serde_json::Value,
            ) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn delete_setting(
                &self,
                _user_id: &str,
                _key: &str,
            ) -> Result<bool, DatabaseError> {
                Ok(false)
            }
            async fn list_settings(
                &self,
                _user_id: &str,
            ) -> Result<Vec<SettingRow>, DatabaseError> {
                Ok(vec![])
            }
            // This is the method that differs -- returns an error.
            async fn get_all_settings(
                &self,
                _user_id: &str,
            ) -> Result<HashMap<String, serde_json::Value>, DatabaseError> {
                Err(DatabaseError::Query("simulated DB failure".to_string()))
            }
            async fn set_all_settings(
                &self,
                _user_id: &str,
                _settings: &HashMap<String, serde_json::Value>,
            ) -> Result<(), DatabaseError> {
                Ok(())
            }
            async fn has_settings(&self, _user_id: &str) -> Result<bool, DatabaseError> {
                Ok(false)
            }
            async fn get_document_by_path(
                &self,
                _user_id: &str,
                _agent_id: Option<Uuid>,
                _path: &str,
            ) -> Result<MemoryDocument, WorkspaceError> {
                Err(WorkspaceError::DocumentNotFound {
                    doc_type: "stub".into(),
                    user_id: "stub".into(),
                })
            }
            async fn get_document_by_id(
                &self,
                _id: Uuid,
            ) -> Result<MemoryDocument, WorkspaceError> {
                Err(WorkspaceError::DocumentNotFound {
                    doc_type: "stub".into(),
                    user_id: "stub".into(),
                })
            }
            async fn get_or_create_document_by_path(
                &self,
                _user_id: &str,
                _agent_id: Option<Uuid>,
                _path: &str,
            ) -> Result<MemoryDocument, WorkspaceError> {
                Err(WorkspaceError::DocumentNotFound {
                    doc_type: "stub".into(),
                    user_id: "stub".into(),
                })
            }
            async fn update_document(
                &self,
                _id: Uuid,
                _content: &str,
            ) -> Result<(), WorkspaceError> {
                Ok(())
            }
            async fn delete_document_by_path(
                &self,
                _user_id: &str,
                _agent_id: Option<Uuid>,
                _path: &str,
            ) -> Result<(), WorkspaceError> {
                Ok(())
            }
            async fn list_directory(
                &self,
                _user_id: &str,
                _agent_id: Option<Uuid>,
                _directory: &str,
            ) -> Result<Vec<WorkspaceEntry>, WorkspaceError> {
                Ok(vec![])
            }
            async fn list_all_paths(
                &self,
                _user_id: &str,
                _agent_id: Option<Uuid>,
            ) -> Result<Vec<String>, WorkspaceError> {
                Ok(vec![])
            }
            async fn list_documents(
                &self,
                _user_id: &str,
                _agent_id: Option<Uuid>,
            ) -> Result<Vec<MemoryDocument>, WorkspaceError> {
                Ok(vec![])
            }
            async fn delete_chunks(&self, _document_id: Uuid) -> Result<(), WorkspaceError> {
                Ok(())
            }
            async fn insert_chunk(
                &self,
                _document_id: Uuid,
                _chunk_index: i32,
                _content: &str,
                _embedding: Option<&[f32]>,
            ) -> Result<Uuid, WorkspaceError> {
                Ok(Uuid::new_v4())
            }
            async fn update_chunk_embedding(
                &self,
                _chunk_id: Uuid,
                _embedding: &[f32],
            ) -> Result<(), WorkspaceError> {
                Ok(())
            }
            async fn get_chunks_without_embeddings(
                &self,
                _user_id: &str,
                _agent_id: Option<Uuid>,
                _limit: usize,
            ) -> Result<Vec<MemoryChunk>, WorkspaceError> {
                Ok(vec![])
            }
            async fn hybrid_search(
                &self,
                _user_id: &str,
                _agent_id: Option<Uuid>,
                _query: &str,
                _embedding: Option<&[f32]>,
                _config: &SearchConfig,
            ) -> Result<Vec<SearchResult>, WorkspaceError> {
                Ok(vec![])
            }
            async fn create_connection(
                &self,
                _connection: &MemoryConnection,
            ) -> Result<(), WorkspaceError> {
                Ok(())
            }
            async fn get_connections(
                &self,
                _document_id: Uuid,
            ) -> Result<Vec<MemoryConnection>, WorkspaceError> {
                Ok(vec![])
            }
            async fn delete_connection(&self, _id: Uuid) -> Result<(), WorkspaceError> {
                Ok(())
            }
            async fn create_space(&self, _space: &MemorySpace) -> Result<(), WorkspaceError> {
                Ok(())
            }
            async fn list_spaces(
                &self,
                _user_id: &str,
            ) -> Result<Vec<MemorySpace>, WorkspaceError> {
                Ok(vec![])
            }
            async fn get_space_by_name(
                &self,
                _user_id: &str,
                _name: &str,
            ) -> Result<Option<MemorySpace>, WorkspaceError> {
                Ok(None)
            }
            async fn add_to_space(
                &self,
                _space_id: Uuid,
                _document_id: Uuid,
            ) -> Result<(), WorkspaceError> {
                Ok(())
            }
            async fn remove_from_space(
                &self,
                _space_id: Uuid,
                _document_id: Uuid,
            ) -> Result<(), WorkspaceError> {
                Ok(())
            }
            async fn list_space_documents(
                &self,
                _space_id: Uuid,
            ) -> Result<Vec<MemoryDocument>, WorkspaceError> {
                Ok(vec![])
            }
            async fn delete_space(&self, _id: Uuid) -> Result<(), WorkspaceError> {
                Ok(())
            }
            async fn upsert_profile(&self, _profile: &UserProfile) -> Result<(), WorkspaceError> {
                Ok(())
            }
            async fn get_profile(
                &self,
                _user_id: &str,
            ) -> Result<Vec<UserProfile>, WorkspaceError> {
                Ok(vec![])
            }
            async fn get_profile_by_type(
                &self,
                _user_id: &str,
                _profile_type: ProfileType,
            ) -> Result<Vec<UserProfile>, WorkspaceError> {
                Ok(vec![])
            }
            async fn delete_profile_entry(
                &self,
                _user_id: &str,
                _key: &str,
            ) -> Result<(), WorkspaceError> {
                Ok(())
            }
            async fn record_document_access(
                &self,
                _document_id: Uuid,
            ) -> Result<(), WorkspaceError> {
                Ok(())
            }
            async fn update_document_metadata(
                &self,
                _document_id: Uuid,
                _metadata: &serde_json::Value,
            ) -> Result<(), WorkspaceError> {
                Ok(())
            }
        }

        let (tx, rx) = broadcast::channel::<ReloadEvent>(16);
        let initial_config = test_config().await;
        let hot_config = HotReloadConfig::new(initial_config);
        let db: Arc<dyn Database> = Arc::new(FailingDatabase);

        let handle = spawn_config_reload_task(rx, hot_config.clone(), db, "test_user".to_string());

        // Trigger a reload that will hit the DB error path.
        tx.send(ReloadEvent::DatabaseChanged).unwrap();
        tokio::time::sleep(Duration::from_millis(800)).await;

        // Config::from_db handles get_all_settings failure gracefully by falling
        // back to defaults, so the reload should still succeed (generation increments).
        // The key assertion is that the task survives and continues running.
        assert!(
            !handle.is_finished(),
            "Task should survive DB errors and continue running"
        );

        drop(tx);
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
    }
}
