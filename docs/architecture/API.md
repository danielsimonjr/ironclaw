# IronClaw API Reference

Complete reference for IronClaw's public API surfaces: trait interfaces, REST endpoints, CLI commands, configuration, and error types.

---

## Table of Contents

- [Trait APIs](#trait-apis)
  - [Database](#database-trait)
  - [Channel](#channel-trait)
  - [Tool](#tool-trait)
  - [LlmProvider](#llmprovider-trait)
  - [EmbeddingProvider](#embeddingprovider-trait)
  - [TtsProvider](#ttsprovider-trait)
  - [TranscriptionProvider](#transcriptionprovider-trait)
  - [VisionProvider](#visionprovider-trait)
  - [SuccessEvaluator](#successevaluator-trait)
- [Web Gateway REST API](#web-gateway-rest-api)
- [CLI Commands](#cli-commands)
- [Configuration](#configuration)
- [Error Types](#error-types)

---

## Trait APIs

### Database Trait

**Location:** `src/db/mod.rs`

Backend-agnostic persistence trait combining all storage operations. Two implementations exist behind feature flags: `PgBackend` (PostgreSQL, default) and `LibSqlBackend` (libSQL/Turso).

```rust
#[async_trait]
pub trait Database: Send + Sync { ... }
```

#### Migrations

### run_migrations
**Signature:** `async fn run_migrations(&self) -> Result<(), DatabaseError>`
**Description:** Run schema migrations for this backend.

#### Conversations

### create_conversation
**Signature:** `async fn create_conversation(&self, channel: &str, user_id: &str, thread_id: Option<&str>) -> Result<Uuid, DatabaseError>`
**Description:** Create a new conversation and return its ID.

### touch_conversation
**Signature:** `async fn touch_conversation(&self, id: Uuid) -> Result<(), DatabaseError>`
**Description:** Update conversation last activity timestamp.

### add_conversation_message
**Signature:** `async fn add_conversation_message(&self, conversation_id: Uuid, role: &str, content: &str) -> Result<Uuid, DatabaseError>`
**Description:** Add a message to a conversation. Returns the message ID.

### ensure_conversation
**Signature:** `async fn ensure_conversation(&self, id: Uuid, channel: &str, user_id: &str, thread_id: Option<&str>) -> Result<(), DatabaseError>`
**Description:** Ensure a conversation row exists (upsert).

### list_conversations_with_preview
**Signature:** `async fn list_conversations_with_preview(&self, user_id: &str, channel: &str, limit: i64) -> Result<Vec<ConversationSummary>, DatabaseError>`
**Description:** List conversations with a title preview for a user on a channel.

### get_or_create_assistant_conversation
**Signature:** `async fn get_or_create_assistant_conversation(&self, user_id: &str, channel: &str) -> Result<Uuid, DatabaseError>`
**Description:** Get or create the singleton assistant conversation.

### create_conversation_with_metadata
**Signature:** `async fn create_conversation_with_metadata(&self, channel: &str, user_id: &str, metadata: &serde_json::Value) -> Result<Uuid, DatabaseError>`
**Description:** Create a conversation with specific metadata.

### list_conversation_messages_paginated
**Signature:** `async fn list_conversation_messages_paginated(&self, conversation_id: Uuid, before: Option<DateTime<Utc>>, limit: i64) -> Result<(Vec<ConversationMessage>, bool), DatabaseError>`
**Description:** Load messages with cursor-based pagination. Returns messages and a `has_more` flag.

### update_conversation_metadata_field
**Signature:** `async fn update_conversation_metadata_field(&self, id: Uuid, key: &str, value: &serde_json::Value) -> Result<(), DatabaseError>`
**Description:** Merge a single key into conversation metadata.

### get_conversation_metadata
**Signature:** `async fn get_conversation_metadata(&self, id: Uuid) -> Result<Option<serde_json::Value>, DatabaseError>`
**Description:** Read conversation metadata.

### list_conversation_messages
**Signature:** `async fn list_conversation_messages(&self, conversation_id: Uuid) -> Result<Vec<ConversationMessage>, DatabaseError>`
**Description:** Load all messages for a conversation.

### conversation_belongs_to_user
**Signature:** `async fn conversation_belongs_to_user(&self, conversation_id: Uuid, user_id: &str) -> Result<bool, DatabaseError>`
**Description:** Check if a conversation belongs to a specific user.

#### Jobs

### save_job
**Signature:** `async fn save_job(&self, ctx: &JobContext) -> Result<(), DatabaseError>`
**Description:** Save a job context to the database.

### get_job
**Signature:** `async fn get_job(&self, id: Uuid) -> Result<Option<JobContext>, DatabaseError>`
**Description:** Get a job by ID.

### update_job_status
**Signature:** `async fn update_job_status(&self, id: Uuid, status: JobState, failure_reason: Option<&str>) -> Result<(), DatabaseError>`
**Description:** Update job status with an optional failure reason.

### mark_job_stuck
**Signature:** `async fn mark_job_stuck(&self, id: Uuid) -> Result<(), DatabaseError>`
**Description:** Mark a job as stuck.

### get_stuck_jobs
**Signature:** `async fn get_stuck_jobs(&self) -> Result<Vec<Uuid>, DatabaseError>`
**Description:** Get all stuck job IDs.

#### Actions

### save_action
**Signature:** `async fn save_action(&self, job_id: Uuid, action: &ActionRecord) -> Result<(), DatabaseError>`
**Description:** Save a job action record.

### get_job_actions
**Signature:** `async fn get_job_actions(&self, job_id: Uuid) -> Result<Vec<ActionRecord>, DatabaseError>`
**Description:** Get all actions for a job.

#### LLM Calls

### record_llm_call
**Signature:** `async fn record_llm_call(&self, record: &LlmCallRecord<'_>) -> Result<Uuid, DatabaseError>`
**Description:** Record an LLM call for cost tracking and auditing.

#### Estimation Snapshots

### save_estimation_snapshot
**Signature:** `async fn save_estimation_snapshot(&self, job_id: Uuid, category: &str, tool_names: &[String], estimated_cost: Decimal, estimated_time_secs: i32, estimated_value: Decimal) -> Result<Uuid, DatabaseError>`
**Description:** Save an estimation snapshot before job execution.

### update_estimation_actuals
**Signature:** `async fn update_estimation_actuals(&self, id: Uuid, actual_cost: Decimal, actual_time_secs: i32, actual_value: Option<Decimal>) -> Result<(), DatabaseError>`
**Description:** Update estimation snapshot with actual values after execution.

#### Sandbox Jobs

### save_sandbox_job
**Signature:** `async fn save_sandbox_job(&self, job: &SandboxJobRecord) -> Result<(), DatabaseError>`
**Description:** Insert a new sandbox job record.

### get_sandbox_job
**Signature:** `async fn get_sandbox_job(&self, id: Uuid) -> Result<Option<SandboxJobRecord>, DatabaseError>`
**Description:** Get a sandbox job by ID.

### list_sandbox_jobs
**Signature:** `async fn list_sandbox_jobs(&self) -> Result<Vec<SandboxJobRecord>, DatabaseError>`
**Description:** List all sandbox jobs, most recent first.

### update_sandbox_job_status
**Signature:** `async fn update_sandbox_job_status(&self, id: Uuid, status: &str, success: Option<bool>, message: Option<&str>, started_at: Option<DateTime<Utc>>, completed_at: Option<DateTime<Utc>>) -> Result<(), DatabaseError>`
**Description:** Update sandbox job status with optional timestamps and result.

### cleanup_stale_sandbox_jobs
**Signature:** `async fn cleanup_stale_sandbox_jobs(&self) -> Result<u64, DatabaseError>`
**Description:** Mark stale sandbox jobs as interrupted. Returns count of affected jobs.

### sandbox_job_summary
**Signature:** `async fn sandbox_job_summary(&self) -> Result<SandboxJobSummary, DatabaseError>`
**Description:** Get aggregate sandbox job summary (counts by status).

### list_sandbox_jobs_for_user
**Signature:** `async fn list_sandbox_jobs_for_user(&self, user_id: &str) -> Result<Vec<SandboxJobRecord>, DatabaseError>`
**Description:** List sandbox jobs for a specific user, most recent first.

### sandbox_job_summary_for_user
**Signature:** `async fn sandbox_job_summary_for_user(&self, user_id: &str) -> Result<SandboxJobSummary, DatabaseError>`
**Description:** Get sandbox job summary for a specific user.

### sandbox_job_belongs_to_user
**Signature:** `async fn sandbox_job_belongs_to_user(&self, job_id: Uuid, user_id: &str) -> Result<bool, DatabaseError>`
**Description:** Check if a sandbox job belongs to a specific user.

### update_sandbox_job_mode
**Signature:** `async fn update_sandbox_job_mode(&self, id: Uuid, mode: &str) -> Result<(), DatabaseError>`
**Description:** Update sandbox job execution mode (worker or claude_code).

### get_sandbox_job_mode
**Signature:** `async fn get_sandbox_job_mode(&self, id: Uuid) -> Result<Option<String>, DatabaseError>`
**Description:** Get sandbox job execution mode.

#### Job Events

### save_job_event
**Signature:** `async fn save_job_event(&self, job_id: Uuid, event_type: &str, data: &serde_json::Value) -> Result<(), DatabaseError>`
**Description:** Persist a job event for history replay.

### list_job_events
**Signature:** `async fn list_job_events(&self, job_id: Uuid) -> Result<Vec<JobEventRecord>, DatabaseError>`
**Description:** Load all events for a job.

#### Routines

### create_routine
**Signature:** `async fn create_routine(&self, routine: &Routine) -> Result<(), DatabaseError>`
**Description:** Create a new routine.

### get_routine
**Signature:** `async fn get_routine(&self, id: Uuid) -> Result<Option<Routine>, DatabaseError>`
**Description:** Get a routine by ID.

### get_routine_by_name
**Signature:** `async fn get_routine_by_name(&self, user_id: &str, name: &str) -> Result<Option<Routine>, DatabaseError>`
**Description:** Get a routine by user_id and name.

### list_routines
**Signature:** `async fn list_routines(&self, user_id: &str) -> Result<Vec<Routine>, DatabaseError>`
**Description:** List all routines for a user.

### list_event_routines
**Signature:** `async fn list_event_routines(&self) -> Result<Vec<Routine>, DatabaseError>`
**Description:** List all enabled event-triggered routines.

### list_due_cron_routines
**Signature:** `async fn list_due_cron_routines(&self) -> Result<Vec<Routine>, DatabaseError>`
**Description:** List cron routines that are due to fire.

### update_routine
**Signature:** `async fn update_routine(&self, routine: &Routine) -> Result<(), DatabaseError>`
**Description:** Update a routine's configuration.

### update_routine_runtime
**Signature:** `async fn update_routine_runtime(&self, id: Uuid, last_run_at: DateTime<Utc>, next_fire_at: Option<DateTime<Utc>>, run_count: u64, consecutive_failures: u32, state: &serde_json::Value) -> Result<(), DatabaseError>`
**Description:** Update runtime state after a routine fires.

### delete_routine
**Signature:** `async fn delete_routine(&self, id: Uuid) -> Result<bool, DatabaseError>`
**Description:** Delete a routine. Returns whether it existed.

#### Routine Runs

### create_routine_run
**Signature:** `async fn create_routine_run(&self, run: &RoutineRun) -> Result<(), DatabaseError>`
**Description:** Record a routine run starting.

### complete_routine_run
**Signature:** `async fn complete_routine_run(&self, id: Uuid, status: RunStatus, result_summary: Option<&str>, tokens_used: Option<i32>) -> Result<(), DatabaseError>`
**Description:** Complete a routine run with status and optional summary.

### list_routine_runs
**Signature:** `async fn list_routine_runs(&self, routine_id: Uuid, limit: i64) -> Result<Vec<RoutineRun>, DatabaseError>`
**Description:** List recent runs for a routine.

### count_running_routine_runs
**Signature:** `async fn count_running_routine_runs(&self, routine_id: Uuid) -> Result<i64, DatabaseError>`
**Description:** Count currently running runs for a routine (concurrency guard).

#### Tool Failures

### record_tool_failure
**Signature:** `async fn record_tool_failure(&self, tool_name: &str, error_message: &str) -> Result<(), DatabaseError>`
**Description:** Record a tool failure (upsert, increments failure count).

### get_broken_tools
**Signature:** `async fn get_broken_tools(&self, threshold: i32) -> Result<Vec<BrokenTool>, DatabaseError>`
**Description:** Get tools exceeding the failure threshold.

### mark_tool_repaired
**Signature:** `async fn mark_tool_repaired(&self, tool_name: &str) -> Result<(), DatabaseError>`
**Description:** Mark a tool as repaired (resets failure count).

### increment_repair_attempts
**Signature:** `async fn increment_repair_attempts(&self, tool_name: &str) -> Result<(), DatabaseError>`
**Description:** Increment the repair attempt counter for a tool.

#### Settings

### get_setting
**Signature:** `async fn get_setting(&self, user_id: &str, key: &str) -> Result<Option<serde_json::Value>, DatabaseError>`
**Description:** Get a single setting value.

### get_setting_full
**Signature:** `async fn get_setting_full(&self, user_id: &str, key: &str) -> Result<Option<SettingRow>, DatabaseError>`
**Description:** Get a single setting with full metadata (key, value, updated_at).

### set_setting
**Signature:** `async fn set_setting(&self, user_id: &str, key: &str, value: &serde_json::Value) -> Result<(), DatabaseError>`
**Description:** Set a single setting (upsert).

### delete_setting
**Signature:** `async fn delete_setting(&self, user_id: &str, key: &str) -> Result<bool, DatabaseError>`
**Description:** Delete a single setting. Returns whether it existed.

### list_settings
**Signature:** `async fn list_settings(&self, user_id: &str) -> Result<Vec<SettingRow>, DatabaseError>`
**Description:** List all settings for a user with metadata.

### get_all_settings
**Signature:** `async fn get_all_settings(&self, user_id: &str) -> Result<HashMap<String, serde_json::Value>, DatabaseError>`
**Description:** Get all settings as a flat key-value map.

### set_all_settings
**Signature:** `async fn set_all_settings(&self, user_id: &str, settings: &HashMap<String, serde_json::Value>) -> Result<(), DatabaseError>`
**Description:** Bulk-write settings atomically.

### has_settings
**Signature:** `async fn has_settings(&self, user_id: &str) -> Result<bool, DatabaseError>`
**Description:** Check if any settings exist for a user.

#### Workspace: Documents

### get_document_by_path
**Signature:** `async fn get_document_by_path(&self, user_id: &str, agent_id: Option<Uuid>, path: &str) -> Result<MemoryDocument, WorkspaceError>`
**Description:** Get a document by its filesystem-like path.

### get_document_by_id
**Signature:** `async fn get_document_by_id(&self, id: Uuid) -> Result<MemoryDocument, WorkspaceError>`
**Description:** Get a document by its UUID.

### get_or_create_document_by_path
**Signature:** `async fn get_or_create_document_by_path(&self, user_id: &str, agent_id: Option<Uuid>, path: &str) -> Result<MemoryDocument, WorkspaceError>`
**Description:** Get or create a document by path (upsert).

### update_document
**Signature:** `async fn update_document(&self, id: Uuid, content: &str) -> Result<(), WorkspaceError>`
**Description:** Update a document's content.

### delete_document_by_path
**Signature:** `async fn delete_document_by_path(&self, user_id: &str, agent_id: Option<Uuid>, path: &str) -> Result<(), WorkspaceError>`
**Description:** Delete a document by path.

### list_directory
**Signature:** `async fn list_directory(&self, user_id: &str, agent_id: Option<Uuid>, directory: &str) -> Result<Vec<WorkspaceEntry>, WorkspaceError>`
**Description:** List files and directories in a directory path.

### list_all_paths
**Signature:** `async fn list_all_paths(&self, user_id: &str, agent_id: Option<Uuid>) -> Result<Vec<String>, WorkspaceError>`
**Description:** List all file paths in the workspace.

### list_documents
**Signature:** `async fn list_documents(&self, user_id: &str, agent_id: Option<Uuid>) -> Result<Vec<MemoryDocument>, WorkspaceError>`
**Description:** List all documents for a user.

#### Workspace: Chunks

### delete_chunks
**Signature:** `async fn delete_chunks(&self, document_id: Uuid) -> Result<(), WorkspaceError>`
**Description:** Delete all chunks for a document.

### insert_chunk
**Signature:** `async fn insert_chunk(&self, document_id: Uuid, chunk_index: i32, content: &str, embedding: Option<&[f32]>) -> Result<Uuid, WorkspaceError>`
**Description:** Insert a chunk with optional embedding vector.

### update_chunk_embedding
**Signature:** `async fn update_chunk_embedding(&self, chunk_id: Uuid, embedding: &[f32]) -> Result<(), WorkspaceError>`
**Description:** Update a chunk's embedding vector (for backfilling).

### get_chunks_without_embeddings
**Signature:** `async fn get_chunks_without_embeddings(&self, user_id: &str, agent_id: Option<Uuid>, limit: usize) -> Result<Vec<MemoryChunk>, WorkspaceError>`
**Description:** Get chunks without embeddings for batch backfilling.

#### Workspace: Search

### hybrid_search
**Signature:** `async fn hybrid_search(&self, user_id: &str, agent_id: Option<Uuid>, query: &str, embedding: Option<&[f32]>, config: &SearchConfig) -> Result<Vec<SearchResult>, WorkspaceError>`
**Description:** Perform hybrid search combining full-text search and vector similarity via Reciprocal Rank Fusion.

#### Workspace: Connections

### create_connection
**Signature:** `async fn create_connection(&self, connection: &MemoryConnection) -> Result<(), WorkspaceError>`
**Description:** Create a typed relationship between two memory documents.

### get_connections
**Signature:** `async fn get_connections(&self, document_id: Uuid) -> Result<Vec<MemoryConnection>, WorkspaceError>`
**Description:** Get all connections for a document (both as source and target).

### delete_connection
**Signature:** `async fn delete_connection(&self, id: Uuid) -> Result<(), WorkspaceError>`
**Description:** Delete a connection by ID.

#### Workspace: Spaces

### create_space
**Signature:** `async fn create_space(&self, space: &MemorySpace) -> Result<(), WorkspaceError>`
**Description:** Create a new named memory space for organizing documents.

### list_spaces
**Signature:** `async fn list_spaces(&self, user_id: &str) -> Result<Vec<MemorySpace>, WorkspaceError>`
**Description:** List all spaces for a user.

### get_space_by_name
**Signature:** `async fn get_space_by_name(&self, user_id: &str, name: &str) -> Result<Option<MemorySpace>, WorkspaceError>`
**Description:** Get a space by name.

### add_to_space
**Signature:** `async fn add_to_space(&self, space_id: Uuid, document_id: Uuid) -> Result<(), WorkspaceError>`
**Description:** Add a document to a space.

### remove_from_space
**Signature:** `async fn remove_from_space(&self, space_id: Uuid, document_id: Uuid) -> Result<(), WorkspaceError>`
**Description:** Remove a document from a space.

### list_space_documents
**Signature:** `async fn list_space_documents(&self, space_id: Uuid) -> Result<Vec<MemoryDocument>, WorkspaceError>`
**Description:** List all documents in a space.

### delete_space
**Signature:** `async fn delete_space(&self, id: Uuid) -> Result<(), WorkspaceError>`
**Description:** Delete a space and its memberships.

#### Workspace: User Profiles

### upsert_profile
**Signature:** `async fn upsert_profile(&self, profile: &UserProfile) -> Result<(), WorkspaceError>`
**Description:** Upsert a user profile entry (static or dynamic fact).

### get_profile
**Signature:** `async fn get_profile(&self, user_id: &str) -> Result<Vec<UserProfile>, WorkspaceError>`
**Description:** Get all profile entries for a user.

### get_profile_by_type
**Signature:** `async fn get_profile_by_type(&self, user_id: &str, profile_type: ProfileType) -> Result<Vec<UserProfile>, WorkspaceError>`
**Description:** Get profile entries of a specific type (Static or Dynamic).

### delete_profile_entry
**Signature:** `async fn delete_profile_entry(&self, user_id: &str, key: &str) -> Result<(), WorkspaceError>`
**Description:** Delete a profile entry by key.

#### Workspace: Document Metadata

### record_document_access
**Signature:** `async fn record_document_access(&self, document_id: Uuid) -> Result<(), WorkspaceError>`
**Description:** Record an access to a document (increments access_count, updates last_accessed_at).

### update_document_metadata
**Signature:** `async fn update_document_metadata(&self, document_id: Uuid, metadata: &serde_json::Value) -> Result<(), WorkspaceError>`
**Description:** Update document metadata fields (importance, tags, source_url, event_date).

---

### Channel Trait

**Location:** `src/channels/channel.rs`

Input sources that receive messages and deliver responses. Implementations: REPL, HTTP, WASM channels (Slack, Telegram, WhatsApp), Web Gateway.

```rust
#[async_trait]
pub trait Channel: Send + Sync { ... }
```

### name
**Signature:** `fn name(&self) -> &str`
**Description:** Get the channel name (e.g., "cli", "slack", "telegram", "http", "gateway").

### start
**Signature:** `async fn start(&self) -> Result<MessageStream, ChannelError>`
**Description:** Start listening for messages. Returns a stream of `IncomingMessage`. The channel handles reconnection and error recovery internally.

### respond
**Signature:** `async fn respond(&self, msg: &IncomingMessage, response: OutgoingResponse) -> Result<(), ChannelError>`
**Description:** Send a response back to the user in the context of the original message.

### send_status
**Signature:** `async fn send_status(&self, status: StatusUpdate, metadata: &serde_json::Value) -> Result<(), ChannelError>`
**Description:** Send a status update (thinking, tool execution, etc.). Default implementation is a no-op.

### broadcast
**Signature:** `async fn broadcast(&self, user_id: &str, response: OutgoingResponse) -> Result<(), ChannelError>`
**Description:** Send a proactive message without a prior incoming message. Used for alerts and heartbeat notifications. Default is a no-op.

### health_check
**Signature:** `async fn health_check(&self) -> Result<(), ChannelError>`
**Description:** Check if the channel is healthy and connected.

### shutdown
**Signature:** `async fn shutdown(&self) -> Result<(), ChannelError>`
**Description:** Gracefully shut down the channel. Default is a no-op.

**Associated Types:**

| Type | Definition |
|------|-----------|
| `IncomingMessage` | Struct with `id: Uuid`, `channel: String`, `user_id: String`, `user_name: Option<String>`, `content: String`, `thread_id: Option<String>`, `received_at: DateTime<Utc>`, `metadata: serde_json::Value` |
| `OutgoingResponse` | Struct with `content: String`, `thread_id: Option<String>`, `metadata: serde_json::Value` |
| `StatusUpdate` | Enum: `Thinking`, `ToolStarted`, `ToolCompleted`, `ToolResult`, `StreamChunk`, `Status`, `JobStarted`, `ApprovalNeeded`, `AuthRequired`, `AuthCompleted` |
| `MessageStream` | `Pin<Box<dyn Stream<Item = IncomingMessage> + Send>>` |

---

### Tool Trait

**Location:** `src/tools/tool.rs`

Executable capabilities the agent can invoke. Three implementations: built-in (Rust), WASM (sandboxed), MCP (external HTTP).

```rust
#[async_trait]
pub trait Tool: Send + Sync { ... }
```

### name
**Signature:** `fn name(&self) -> &str`
**Description:** Get the tool name.

### description
**Signature:** `fn description(&self) -> &str`
**Description:** Get a human-readable description of what the tool does.

### parameters_schema
**Signature:** `fn parameters_schema(&self) -> serde_json::Value`
**Description:** Get the JSON Schema for the tool's parameters.

### execute
**Signature:** `async fn execute(&self, params: serde_json::Value, ctx: &JobContext) -> Result<ToolOutput, ToolError>`
**Description:** Execute the tool with the given parameters in a job context.

### estimated_cost
**Signature:** `fn estimated_cost(&self, params: &serde_json::Value) -> Option<Decimal>`
**Description:** Estimate the monetary cost of running this tool. Default returns `None`.

### estimated_duration
**Signature:** `fn estimated_duration(&self, params: &serde_json::Value) -> Option<Duration>`
**Description:** Estimate how long this tool will take. Default returns `None`.

### requires_sanitization
**Signature:** `fn requires_sanitization(&self) -> bool`
**Description:** Whether this tool's output needs sanitization (true for external services). Default: `true`.

### requires_approval
**Signature:** `fn requires_approval(&self) -> bool`
**Description:** Whether this tool requires explicit user approval before execution. Default: `false`.

### execution_timeout
**Signature:** `fn execution_timeout(&self) -> Duration`
**Description:** Maximum execution time before the caller kills it. Default: 60 seconds.

### domain
**Signature:** `fn domain(&self) -> ToolDomain`
**Description:** Where this tool should execute: `Orchestrator` (main process) or `Container` (Docker). Default: `Orchestrator`.

### schema
**Signature:** `fn schema(&self) -> ToolSchema`
**Description:** Get the tool schema for LLM function calling. Default builds from `name()`, `description()`, and `parameters_schema()`.

**Associated Types:**

| Type | Definition |
|------|-----------|
| `ToolOutput` | Struct with `result: serde_json::Value`, `cost: Option<Decimal>`, `duration: Duration`, `raw: Option<String>` |
| `ToolError` | Enum: `InvalidParameters`, `ExecutionFailed`, `Timeout`, `NotAuthorized`, `RateLimited`, `ExternalService`, `Sandbox` |
| `ToolDomain` | Enum: `Orchestrator`, `Container` |
| `ToolSchema` | Struct with `name: String`, `description: String`, `parameters: serde_json::Value` |

---

### LlmProvider Trait

**Location:** `src/llm/provider.rs`

LLM backends for chat completion and tool use. Implementations: NEAR AI, OpenAI, Anthropic, Gemini, Bedrock, Ollama, OpenRouter.

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync { ... }
```

### model_name
**Signature:** `fn model_name(&self) -> &str`
**Description:** Get the configured model name.

### cost_per_token
**Signature:** `fn cost_per_token(&self) -> (Decimal, Decimal)`
**Description:** Get cost per token as (input_cost, output_cost).

### complete
**Signature:** `async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError>`
**Description:** Complete a chat conversation (no tool use).

### complete_with_tools
**Signature:** `async fn complete_with_tools(&self, request: ToolCompletionRequest) -> Result<ToolCompletionResponse, LlmError>`
**Description:** Complete with tool use support. The model may return tool calls.

### list_models
**Signature:** `async fn list_models(&self) -> Result<Vec<String>, LlmError>`
**Description:** List available models from the provider. Default returns empty list.

### model_metadata
**Signature:** `async fn model_metadata(&self) -> Result<ModelMetadata, LlmError>`
**Description:** Fetch metadata for the current model (context length, etc.).

### active_model_name
**Signature:** `fn active_model_name(&self) -> String`
**Description:** Get the currently active model name (may differ from `model_name()` after `set_model()`).

### set_model
**Signature:** `fn set_model(&self, model: &str) -> Result<(), LlmError>`
**Description:** Switch the active model at runtime. Not all providers support this.

### seed_response_chain
**Signature:** `fn seed_response_chain(&self, thread_id: &str, response_id: String)`
**Description:** Seed a response chain for a thread (e.g., restoring from DB for NEAR AI response chaining).

### get_response_chain_id
**Signature:** `fn get_response_chain_id(&self, thread_id: &str) -> Option<String>`
**Description:** Get the last response chain ID for a thread.

### calculate_cost
**Signature:** `fn calculate_cost(&self, input_tokens: u32, output_tokens: u32) -> Decimal`
**Description:** Calculate cost for a completion from token counts.

**Associated Types:**

| Type | Definition |
|------|-----------|
| `CompletionRequest` | `messages: Vec<ChatMessage>`, `max_tokens: Option<u32>`, `temperature: Option<f32>`, `stop_sequences: Option<Vec<String>>`, `metadata: HashMap<String, String>` |
| `CompletionResponse` | `content: String`, `input_tokens: u32`, `output_tokens: u32`, `finish_reason: FinishReason`, `response_id: Option<String>` |
| `ToolCompletionRequest` | `messages`, `tools: Vec<ToolDefinition>`, `max_tokens`, `temperature`, `tool_choice: Option<String>`, `metadata` |
| `ToolCompletionResponse` | `content: Option<String>`, `tool_calls: Vec<ToolCall>`, `input_tokens`, `output_tokens`, `finish_reason`, `response_id` |
| `ChatMessage` | `role: Role`, `content: String`, `tool_call_id: Option<String>`, `name: Option<String>`, `tool_calls: Option<Vec<ToolCall>>` |
| `Role` | Enum: `System`, `User`, `Assistant`, `Tool` |
| `FinishReason` | Enum: `Stop`, `Length`, `ToolUse`, `ContentFilter`, `Unknown` |

---

### EmbeddingProvider Trait

**Location:** `src/workspace/embeddings.rs`

Vector embedding backends for semantic search. Implementations: `OpenAiEmbeddings`, `NearAiEmbeddings`, `MockEmbeddings`.

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync { ... }
```

### dimension
**Signature:** `fn dimension(&self) -> usize`
**Description:** Get the embedding vector dimension (e.g., 1536 for text-embedding-3-small).

### model_name
**Signature:** `fn model_name(&self) -> &str`
**Description:** Get the model name.

### max_input_length
**Signature:** `fn max_input_length(&self) -> usize`
**Description:** Maximum input length in characters.

### embed
**Signature:** `async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError>`
**Description:** Generate an embedding for a single text.

### embed_batch
**Signature:** `async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError>`
**Description:** Generate embeddings for multiple texts in a single batch. Default calls `embed()` sequentially.

---

### TtsProvider Trait

**Location:** `src/media/tts.rs`

Text-to-speech providers. Implementation: `OpenAiTtsProvider` (also supports Edge TTS via WebSocket).

```rust
#[async_trait]
pub trait TtsProvider: Send + Sync { ... }
```

### synthesize
**Signature:** `async fn synthesize(&self, text: &str, voice: &TtsVoice, format: TtsFormat) -> Result<Vec<u8>, MediaError>`
**Description:** Synthesize speech from text. Returns raw audio bytes in the requested format.

### name
**Signature:** `fn name(&self) -> &str`
**Description:** Get the provider name.

### is_available
**Signature:** `fn is_available(&self) -> bool`
**Description:** Check if the provider is available and configured.

### available_voices
**Signature:** `fn available_voices(&self) -> Vec<TtsVoice>`
**Description:** List available voices for this provider.

**Associated Types:**

| Type | Definition |
|------|-----------|
| `TtsFormat` | Enum: `Mp3`, `Wav`, `Ogg`, `Opus` |
| `TtsVoice` | Struct with `name: String`, `language: String`, `gender: VoiceGender` |
| `VoiceGender` | Enum: `Male`, `Female`, `Neutral` |

---

### TranscriptionProvider Trait

**Location:** `src/media/transcription.rs`

Audio transcription providers. Implementation: `WhisperProvider` (OpenAI Whisper API).

```rust
#[async_trait]
pub trait TranscriptionProvider: Send + Sync { ... }
```

### transcribe
**Signature:** `async fn transcribe(&self, data: &[u8], mime_type: &str, language: Option<&str>) -> Result<TranscriptionResult, MediaError>`
**Description:** Transcribe audio data. Accepts raw audio bytes with MIME type and optional language hint.

### name
**Signature:** `fn name(&self) -> &str`
**Description:** Get the provider name.

### is_available
**Signature:** `fn is_available(&self) -> bool`
**Description:** Check if the provider is available and configured.

**Associated Types:**

| Type | Definition |
|------|-----------|
| `TranscriptionResult` | Struct with `text: String`, `language: Option<String>`, `duration_seconds: Option<f64>`, `provider: String` |

---

### VisionProvider Trait

**Location:** `src/media/vision.rs`

Vision model providers for image understanding. Implementation: `OpenAiVisionProvider`.

```rust
#[async_trait]
pub trait VisionProvider: Send + Sync { ... }
```

### analyze
**Signature:** `async fn analyze(&self, request: VisionRequest) -> Result<VisionResponse, MediaError>`
**Description:** Analyze an image with a vision model.

### name
**Signature:** `fn name(&self) -> &str`
**Description:** Get the provider name.

### is_available
**Signature:** `fn is_available(&self) -> bool`
**Description:** Check if the provider supports vision.

**Associated Types:**

| Type | Definition |
|------|-----------|
| `VisionRequest` | Struct with `image: ImageSource`, `prompt: String`, `detail: Option<String>`, `max_tokens: Option<u32>` |
| `ImageSource` | Enum: `Base64 { data, media_type }`, `Url { url }` |
| `VisionResponse` | Struct with `content: String`, `input_tokens: Option<u32>`, `output_tokens: Option<u32>`, `provider: String` |

---

### SuccessEvaluator Trait

**Location:** `src/evaluation/success.rs`

Job outcome evaluation. Implementations: `RuleBasedEvaluator` (action success rate, failure counts), `LlmEvaluator` (LLM-based nuanced evaluation).

```rust
#[async_trait]
pub trait SuccessEvaluator: Send + Sync { ... }
```

### evaluate
**Signature:** `async fn evaluate(&self, job: &JobContext, actions: &[ActionRecord], output: Option<&str>) -> Result<EvaluationResult, EvaluationError>`
**Description:** Evaluate whether a job was completed successfully based on its context, actions taken, and output.

**Associated Types:**

| Type | Definition |
|------|-----------|
| `EvaluationResult` | Struct with `success: bool`, `confidence: f64`, `reasoning: String`, `issues: Vec<String>`, `suggestions: Vec<String>`, `quality_score: u32` |

---

## Web Gateway REST API

The web gateway is an Axum HTTP server providing a REST API for the IronClaw web UI. All protected endpoints require a `Bearer` token in the `Authorization` header. The gateway binds to localhost by default.

**Base URL:** `http://localhost:{port}`

### Authentication

All endpoints except `/api/health` and static files require authentication via:
```
Authorization: Bearer <token>
```

### Chat

#### POST /api/chat/send
Send a message to the agent.

**Request:**
```json
{ "content": "string", "thread_id": "string (optional)" }
```
**Response (202):**
```json
{ "message_id": "uuid", "status": "accepted" }
```
**Rate limited:** 30 messages per 60 seconds.

#### POST /api/chat/approval
Approve or deny a pending tool execution.

**Request:**
```json
{ "request_id": "uuid", "action": "approve|always|deny", "thread_id": "string (optional)" }
```
**Response (202):**
```json
{ "message_id": "uuid", "status": "accepted" }
```

#### POST /api/chat/auth-token
Submit an auth token for an extension (bypasses message pipeline, never touches LLM).

**Request:**
```json
{ "extension_name": "string", "token": "string" }
```
**Response:**
```json
{ "success": true, "message": "string" }
```

#### POST /api/chat/auth-cancel
Cancel an in-progress auth flow.

**Request:**
```json
{ "extension_name": "string" }
```

#### GET /api/chat/events
SSE endpoint for real-time events. Returns a `text/event-stream` with events: `response`, `thinking`, `tool_started`, `tool_completed`, `tool_result`, `stream_chunk`, `status`, `job_started`, `approval_needed`, `auth_required`, `auth_completed`, `error`, `heartbeat`, `job_message`, `job_tool_use`, `job_tool_result`, `job_status`, `job_result`, `channel_status`, `config_changed`, `canvas_created`, `canvas_updated`, `canvas_deleted`.

#### GET /api/chat/ws
WebSocket endpoint for bidirectional real-time communication. Requires `Origin` header from localhost. Client sends `WsClientMessage` (message, approval, auth_token, auth_cancel, ping), server sends `WsServerMessage` (event, pong, error).

#### GET /api/chat/history
Get conversation history for a thread.

**Query params:** `thread_id` (optional UUID), `limit` (optional, default 50), `before` (optional ISO8601 cursor).
**Response:**
```json
{
  "thread_id": "uuid",
  "turns": [{ "turn_number": 0, "user_input": "string", "response": "string|null", "state": "string", "started_at": "iso8601", "completed_at": "iso8601|null", "tool_calls": [] }],
  "has_more": false,
  "oldest_timestamp": "iso8601|null"
}
```

#### GET /api/chat/threads
List all conversation threads.

**Response:**
```json
{
  "assistant_thread": { "id": "uuid", "state": "string", "turn_count": 0, "created_at": "iso8601", "updated_at": "iso8601", "title": "string|null", "thread_type": "string|null" },
  "threads": [...],
  "active_thread": "uuid|null"
}
```

#### POST /api/chat/thread/new
Create a new conversation thread.

**Response:**
```json
{ "id": "uuid", "state": "Idle", "turn_count": 0, "created_at": "iso8601", "updated_at": "iso8601", "thread_type": "thread" }
```

### Memory

#### GET /api/memory/tree
Get the full memory document tree.

**Response:**
```json
{ "entries": [{ "path": "string", "is_dir": true }] }
```

#### GET /api/memory/list
List entries in a directory.

**Query params:** `path` (optional, default root).
**Response:**
```json
{ "path": "string", "entries": [{ "name": "string", "path": "string", "is_dir": false, "updated_at": "iso8601|null" }] }
```

#### GET /api/memory/read
Read a memory document.

**Query params:** `path` (required).
**Response:**
```json
{ "path": "string", "content": "string", "updated_at": "iso8601|null" }
```

#### POST /api/memory/write
Write a memory document.

**Request:**
```json
{ "path": "string", "content": "string" }
```
**Response:**
```json
{ "path": "string", "status": "written" }
```

#### POST /api/memory/search
Search memory documents using hybrid FTS + vector search.

**Request:**
```json
{ "query": "string", "limit": 10 }
```
**Response:**
```json
{ "results": [{ "path": "string", "content": "string", "score": 0.95 }] }
```

### Jobs

#### GET /api/jobs
List sandbox jobs for the authenticated user (most recent first).

**Response:**
```json
{ "jobs": [{ "id": "uuid", "title": "string", "state": "string", "user_id": "string", "created_at": "iso8601", "started_at": "iso8601|null" }] }
```

#### GET /api/jobs/summary
Get aggregate job counts by status.

**Response:**
```json
{ "total": 0, "pending": 0, "in_progress": 0, "completed": 0, "failed": 0, "stuck": 0 }
```

#### GET /api/jobs/{id}
Get job details including transitions and browse URL.

**Response:**
```json
{ "id": "uuid", "title": "string", "description": "string", "state": "string", "user_id": "string", "created_at": "iso8601", "started_at": "iso8601|null", "completed_at": "iso8601|null", "elapsed_secs": 0, "project_dir": "string|null", "browse_url": "string|null", "job_mode": "string|null", "transitions": [] }
```

#### POST /api/jobs/{id}/cancel
Cancel a running or creating job.

#### POST /api/jobs/{id}/restart
Restart a failed or interrupted job (creates a new job with the same task).

#### POST /api/jobs/{id}/prompt
Submit a follow-up prompt to a running Claude Code sandbox job.

**Request:**
```json
{ "content": "string", "done": false }
```

#### GET /api/jobs/{id}/events
Load persisted job events for history replay.

#### GET /api/jobs/{id}/files/list
List files in a sandbox job's project directory.

**Query params:** `path` (optional subdirectory).

#### GET /api/jobs/{id}/files/read
Read a file from a sandbox job's project directory.

**Query params:** `path` (required).

### Logs

#### GET /api/logs/events
SSE endpoint for real-time log streaming. Replays recent history on connection.

### Extensions

#### GET /api/extensions
List installed extensions.

**Response:**
```json
{ "extensions": [{ "name": "string", "kind": "string", "description": "string|null", "url": "string|null", "authenticated": true, "active": true, "tools": ["tool1"] }] }
```

#### GET /api/extensions/tools
List all registered tools.

**Response:**
```json
{ "tools": [{ "name": "string", "description": "string" }] }
```

#### POST /api/extensions/install
Install an extension.

**Request:**
```json
{ "name": "string", "url": "string (optional)", "kind": "mcp_server|wasm_tool|wasm_channel (optional)" }
```

#### POST /api/extensions/{name}/activate
Activate an installed extension (loads its tools). May trigger auth flow.

#### POST /api/extensions/{name}/remove
Remove an installed extension.

### Routines

#### GET /api/routines
List routines for the authenticated user.

#### GET /api/routines/summary
Get routine summary (total, enabled, disabled, failing, runs_today).

#### GET /api/routines/{id}
Get routine details with recent runs.

#### POST /api/routines/{id}/trigger
Manually trigger a routine.

#### POST /api/routines/{id}/toggle
Toggle routine enabled/disabled state.

**Request (optional):**
```json
{ "enabled": true }
```

#### DELETE /api/routines/{id}
Delete a routine.

#### GET /api/routines/{id}/runs
List recent runs for a routine (up to 50).

### Settings

#### GET /api/settings
List all settings for the authenticated user.

#### GET /api/settings/export
Export all settings as a flat key-value map.

#### POST /api/settings/import
Import settings from a flat key-value map (bulk write).

**Request:**
```json
{ "settings": { "key": "value" } }
```

#### GET /api/settings/{key}
Get a single setting by key.

#### PUT /api/settings/{key}
Set a single setting value.

**Request:**
```json
{ "value": "any JSON value" }
```

#### DELETE /api/settings/{key}
Delete a single setting.

### Gateway

#### GET /api/gateway/status
Get gateway connection statistics.

**Response:**
```json
{ "sse_connections": 0, "ws_connections": 0, "total_connections": 0 }
```

#### GET /api/health
Health check (no auth required).

**Response:**
```json
{ "status": "healthy", "channel": "gateway" }
```

### OpenAI-Compatible API

#### POST /v1/chat/completions
OpenAI-compatible chat completions endpoint (proxies to configured LLM provider).

#### GET /v1/models
List available models.

---

## CLI Commands

All commands are invoked via `ironclaw <command>`. The default command is `run`.

| Command | Description |
|---------|-------------|
| `run` | Start the agent (default). Loads config, connects DB, initializes all channels, enters message loop. |
| `worker` | Run as a sandboxed worker inside a Docker container. Communicates with orchestrator. |
| `claude-bridge` | Run as a Claude Code bridge inside a Docker container. Spawns `claude` CLI process. |
| `tool` | Execute a single tool by name with JSON parameters. |
| `config` | View or modify configuration values. |
| `memory` | Manage workspace memory (read, write, search, list). |
| `mcp` | Manage MCP server connections. |
| `pairing` | Generate and manage pairing codes for remote access. |
| `status` | Show agent status (connections, jobs, health). |
| `onboard` | Run the interactive onboarding wizard. |
| `doctor` | Diagnose configuration and connectivity issues. |
| `gateway` | Start the web gateway server only (without the full agent). |
| `sessions` | List and manage active sessions. |
| `hooks` | List, test, and manage lifecycle hooks. |
| `cron` | List and manage cron routines. |
| `logs` | View and search agent logs. |
| `message` | Send a single message to the agent and print the response. |
| `channels` | List and manage active channels. |
| `plugins` | List and manage plugins (deprecated, use `extensions`). |
| `webhooks` | List and manage outbound webhooks. |
| `skills` | List and manage agent skills. |
| `agents` | List and manage sub-agents. |
| `nodes` | List and manage cluster nodes. |
| `browser` | Launch the web gateway and open in default browser. |
| `completion` | Generate shell completion scripts. |
| `service` | Install/uninstall as a system service. |

Use `ironclaw --help` or `ironclaw <command> --help` for detailed usage.

Use `cargo run -- --no-db` to bypass the `DATABASE_URL` requirement on startup (useful for CLI-only commands).

---

## Configuration

**Location:** `src/config.rs`

Config loads with priority: **environment variables > database settings > defaults**. Bootstrap config persists to `~/.ironclaw/bootstrap.json`.

### Config Struct

```rust
pub struct Config {
    pub database: DatabaseConfig,
    pub llm: LlmConfig,
    pub embeddings: EmbeddingsConfig,
    pub tunnel: TunnelConfig,
    pub channels: ChannelsConfig,
    pub agent: AgentConfig,
    pub safety: SafetyConfig,
    pub wasm: WasmConfig,
    pub secrets: SecretsConfig,
    pub builder: BuilderModeConfig,
    pub heartbeat: HeartbeatConfig,
    pub routines: RoutineConfig,
    pub sandbox: SandboxModeConfig,
    pub claude_code: ClaudeCodeConfig,
}
```

### Loading Methods

- `Config::from_db(store, user_id, bootstrap)` -- Load from env vars + database settings (primary method after DB is connected).
- `Config::from_env()` -- Load from env vars only (early startup, CLI commands without DB).

### DatabaseConfig

| Field | Type | Env Var | Description |
|-------|------|---------|-------------|
| `backend` | `DatabaseBackend` | `DATABASE_BACKEND` | `postgres` (default) or `libsql` |
| `url` | `SecretString` | `DATABASE_URL` | PostgreSQL connection string |
| `pool_size` | `usize` | (bootstrap) | Connection pool size |
| `libsql_path` | `Option<PathBuf>` | `LIBSQL_PATH` | Local libSQL database file path |
| `libsql_url` | `Option<String>` | `LIBSQL_URL` | Turso cloud URL for remote sync |
| `libsql_auth_token` | `Option<SecretString>` | `LIBSQL_AUTH_TOKEN` | Turso auth token |

### TunnelConfig

| Field | Type | Env Var | Description |
|-------|------|---------|-------------|
| `public_url` | `Option<String>` | `TUNNEL_URL` | Public URL from tunnel provider (must be HTTPS) |

### Key Environment Variables

| Variable | Description |
|----------|-------------|
| `DATABASE_URL` | PostgreSQL connection string (required for postgres backend) |
| `DATABASE_BACKEND` | Database backend: `postgres`, `libsql`, `turso`, `sqlite` |
| `LLM_BACKEND` | LLM provider: `nearai`, `openai`, `anthropic`, `gemini`, `bedrock`, `ollama`, `openrouter` |
| `OPENAI_API_KEY` | OpenAI API key |
| `ANTHROPIC_API_KEY` | Anthropic API key |
| `TUNNEL_URL` | Public HTTPS URL for webhooks |
| `RUST_LOG` | Log level (e.g., `ironclaw=debug`) |

See `deploy/env.example` for the complete list of environment variables.

---

## Error Types

**Location:** `src/error.rs`

All error types use `thiserror` for Display/Error derivation.

### Error (Top-level)

Wraps all domain-specific error types via `#[from]`.

| Variant | Source |
|---------|--------|
| `Config` | `ConfigError` |
| `Database` | `DatabaseError` |
| `Channel` | `ChannelError` |
| `Llm` | `LlmError` |
| `Tool` | `ToolError` |
| `Safety` | `SafetyError` |
| `Job` | `JobError` |
| `Estimation` | `EstimationError` |
| `Evaluation` | `EvaluationError` |
| `Repair` | `RepairError` |
| `Workspace` | `WorkspaceError` |
| `Orchestrator` | `OrchestratorError` |
| `Worker` | `WorkerError` |
| `Hook` | `HookError` |
| `Media` | `MediaError` |
| `Skills` | `SkillsError` |

### ConfigError

| Variant | Fields | Description |
|---------|--------|-------------|
| `MissingEnvVar` | `String` | Missing required environment variable |
| `MissingRequired` | `key: String, hint: String` | Missing required configuration with hint |
| `InvalidValue` | `key: String, message: String` | Invalid configuration value |
| `ParseError` | `String` | Failed to parse configuration |
| `Io` | `std::io::Error` | IO error |

### DatabaseError

| Variant | Fields | Description |
|---------|--------|-------------|
| `Pool` | `String` | Connection pool error |
| `Query` | `String` | Query execution failed |
| `NotFound` | `entity: String, id: String` | Entity not found |
| `Constraint` | `String` | Constraint violation |
| `Migration` | `String` | Migration failed |
| `Serialization` | `String` | Serialization error |
| `Postgres` | `tokio_postgres::Error` | PostgreSQL error (feature-gated) |
| `PoolBuild` | `deadpool_postgres::BuildError` | Pool build error (feature-gated) |
| `PoolRuntime` | `deadpool_postgres::PoolError` | Pool runtime error (feature-gated) |
| `LibSql` | `libsql::Error` | libSQL error (feature-gated) |

### ChannelError

| Variant | Fields | Description |
|---------|--------|-------------|
| `StartupFailed` | `name, reason` | Channel failed to start |
| `Disconnected` | `name, reason` | Channel disconnected |
| `SendFailed` | `name, reason` | Failed to send response |
| `InvalidMessage` | `String` | Invalid message format |
| `AuthFailed` | `name, reason` | Authentication failed |
| `RateLimited` | `name` | Rate limited |
| `Http` | `String` | HTTP error |
| `HealthCheckFailed` | `name` | Health check failed |

### LlmError

| Variant | Fields | Description |
|---------|--------|-------------|
| `RequestFailed` | `provider, reason` | Provider request failed |
| `RateLimited` | `provider, retry_after` | Provider rate limited |
| `InvalidResponse` | `provider, reason` | Invalid response from provider |
| `ContextLengthExceeded` | `used, limit` | Context length exceeded |
| `ModelNotAvailable` | `provider, model` | Model not available |
| `AuthFailed` | `provider` | Authentication failed |
| `SessionExpired` | `provider` | Session expired |
| `SessionRenewalFailed` | `provider, reason` | Session renewal failed |
| `Http` | `reqwest::Error` | HTTP error |
| `Json` | `serde_json::Error` | JSON error |
| `Io` | `std::io::Error` | IO error |

### ToolError (error.rs)

| Variant | Fields | Description |
|---------|--------|-------------|
| `NotFound` | `name` | Tool not found |
| `ExecutionFailed` | `name, reason` | Tool execution failed |
| `Timeout` | `name, timeout` | Tool timed out |
| `InvalidParameters` | `name, reason` | Invalid parameters |
| `Disabled` | `name, reason` | Tool is disabled |
| `Sandbox` | `name, reason` | Sandbox error |
| `AuthRequired` | `name` | Tool requires authentication |
| `BuilderFailed` | `String` | Tool builder failed |

### SafetyError

| Variant | Fields | Description |
|---------|--------|-------------|
| `InjectionDetected` | `pattern` | Potential prompt injection detected |
| `OutputTooLarge` | `length, max` | Output exceeded maximum length |
| `BlockedContent` | `pattern` | Blocked content pattern detected |
| `ValidationFailed` | `reason` | Validation failed |
| `PolicyViolation` | `rule` | Policy violation |

### JobError

| Variant | Fields | Description |
|---------|--------|-------------|
| `NotFound` | `id` | Job not found |
| `InvalidTransition` | `id, state, target` | Invalid state transition |
| `Failed` | `id, reason` | Job failed |
| `Stuck` | `id, duration` | Job stuck |
| `MaxJobsExceeded` | `max` | Maximum parallel jobs exceeded |
| `ContextError` | `id, reason` | Job context error |

### EstimationError

| Variant | Fields | Description |
|---------|--------|-------------|
| `InsufficientData` | `needed, have` | Not enough samples for estimation |
| `CalculationFailed` | `reason` | Estimation calculation failed |
| `InvalidParameters` | `reason` | Invalid estimation parameters |

### EvaluationError

| Variant | Fields | Description |
|---------|--------|-------------|
| `Failed` | `job_id, reason` | Evaluation failed for job |
| `MissingData` | `field` | Missing required evaluation data |
| `InvalidCriteria` | `reason` | Invalid evaluation criteria |

### RepairError

| Variant | Fields | Description |
|---------|--------|-------------|
| `Failed` | `target_type, target_id, reason` | Repair failed |
| `MaxAttemptsExceeded` | `target_type, target_id, max` | Maximum repair attempts exceeded |
| `DiagnosisFailed` | `target_type, target_id, reason` | Cannot diagnose issue |

### WorkspaceError

| Variant | Fields | Description |
|---------|--------|-------------|
| `DocumentNotFound` | `doc_type, user_id` | Document not found |
| `SearchFailed` | `reason` | Search failed |
| `EmbeddingFailed` | `reason` | Embedding generation failed |
| `ChunkingFailed` | `reason` | Document chunking failed |
| `InvalidDocType` | `doc_type` | Invalid document type |
| `NotInitialized` | `user_id` | Workspace not initialized |
| `HeartbeatError` | `reason` | Heartbeat error |

### OrchestratorError

| Variant | Fields | Description |
|---------|--------|-------------|
| `ContainerCreationFailed` | `job_id, reason` | Container creation failed |
| `ContainerNotFound` | `job_id` | Container not found |
| `InvalidContainerState` | `job_id, state` | Unexpected container state |
| `AuthFailed` | `reason` | Worker authentication failed |
| `ApiError` | `reason` | Internal API error |
| `Docker` | `reason` | Docker error |
| `ContainerTimeout` | `job_id` | Job timed out in container |

### WorkerError

| Variant | Fields | Description |
|---------|--------|-------------|
| `ConnectionFailed` | `url, reason` | Failed to connect to orchestrator |
| `LlmProxyFailed` | `reason` | LLM proxy request failed |
| `SecretResolveFailed` | `secret_name, reason` | Secret resolution failed |
| `OrchestratorRejected` | `job_id, reason` | Orchestrator returned error |
| `ExecutionFailed` | `reason` | Worker execution failed |
| `MissingToken` | | Missing IRONCLAW_WORKER_TOKEN |

### HookError

| Variant | Fields | Description |
|---------|--------|-------------|
| `ExecutionFailed` | `name, reason` | Hook execution failed |
| `Timeout` | `name, timeout_ms` | Hook timed out |
| `RegistrationFailed` | `reason` | Hook registration failed |

### MediaError

| Variant | Fields | Description |
|---------|--------|-------------|
| `UnsupportedType` | `mime_type` | Unsupported media type |
| `ProcessingFailed` | `reason` | Media processing failed |
| `TooLarge` | `size, max` | Media file too large |
| `DownloadFailed` | `reason` | Media download failed |
| `TranscriptionFailed` | `reason` | Transcription failed |
| `VisionFailed` | `reason` | Vision processing failed |
| `RecursiveProcessingFailed` | `reason` | Recursive processing failed |
| `MaxDepthExceeded` | `max_depth` | Exceeded max recursion depth |
| `MaxIterationsExceeded` | `max_iterations` | Exceeded max iterations |
| `ChunkOutOfRange` | `index, total` | Chunk index out of range |

### SkillsError

| Variant | Fields | Description |
|---------|--------|-------------|
| `NotFound` | `name` | Skill not found |
| `ExecutionFailed` | `name, reason` | Skill execution failed |
| `InvalidDefinition` | `reason` | Invalid skill definition |

### EmbeddingError

**Location:** `src/workspace/embeddings.rs`

| Variant | Fields | Description |
|---------|--------|-------------|
| `HttpError` | `String` | HTTP request failed |
| `InvalidResponse` | `String` | Invalid response |
| `RateLimited` | `retry_after: Option<Duration>` | Rate limited |
| `AuthFailed` | | Authentication failed |
| `TextTooLong` | `length, max` | Text exceeds maximum length |

### ToolError (tool.rs)

**Location:** `src/tools/tool.rs` (distinct from `error.rs` ToolError)

| Variant | Fields | Description |
|---------|--------|-------------|
| `InvalidParameters` | `String` | Invalid parameters |
| `ExecutionFailed` | `String` | Execution failed |
| `Timeout` | `Duration` | Timeout |
| `NotAuthorized` | `String` | Not authorized |
| `RateLimited` | `Option<Duration>` | Rate limited |
| `ExternalService` | `String` | External service error |
| `Sandbox` | `String` | Sandbox error |
