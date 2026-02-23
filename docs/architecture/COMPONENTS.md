# IronClaw - Component Reference

**Last Updated**: 2026-02-22

---

## Table of Contents

1. [Overview](#overview)
2. [Agent Components](#agent-components)
3. [Channel Components](#channel-components)
4. [Core Components](#core-components)
5. [LLM Components](#llm-components)
6. [Tool Components](#tool-components)
7. [Safety Components](#safety-components)
8. [Media Components](#media-components)
9. [Workspace Components](#workspace-components)
10. [Extension Components](#extension-components)
11. [Hook Components](#hook-components)
12. [Sandbox Components](#sandbox-components)
13. [CLI Components](#cli-components)
14. [Component Dependencies](#component-dependencies)

---

## Overview

IronClaw is a composable, trait-based personal AI assistant built in Rust. Components are organized by domain:

```
┌─────────────────────────────────────────────────────────────┐
│  agent/          │  Agent loop, routing, scheduling (21 files)│
├─────────────────────────────────────────────────────────────┤
│  channels/       │  Input sources: REPL, HTTP, web, WASM (38 files)│
├─────────────────────────────────────────────────────────────┤
│  db/             │  Database abstraction + backends (4 files) │
├─────────────────────────────────────────────────────────────┤
│  llm/            │  LLM providers + failover (14 files)      │
├─────────────────────────────────────────────────────────────┤
│  tools/          │  Tool registry, built-in/WASM/MCP (45 files)│
├─────────────────────────────────────────────────────────────┤
│  safety/         │  Sanitizer, validator, policy (11 files)  │
├─────────────────────────────────────────────────────────────┤
│  media/          │  TTS, vision, PDF, transcription (12 files)│
├─────────────────────────────────────────────────────────────┤
│  workspace/      │  Memory, embeddings, search (9 files)     │
├─────────────────────────────────────────────────────────────┤
│  extensions/     │  Discovery, install, ClawHub (7 files)    │
├─────────────────────────────────────────────────────────────┤
│  hooks/          │  Lifecycle hooks, webhooks (7 files)      │
├─────────────────────────────────────────────────────────────┤
│  sandbox/        │  Docker isolation, network proxy (9 files)│
├─────────────────────────────────────────────────────────────┤
│  cli/            │  CLI subcommands (23 files)               │
└─────────────────────────────────────────────────────────────┘
```

---

## Agent Components

### Agent (`src/agent/agent_loop.rs`)

**Purpose**: Main agent loop that coordinates all components -- receives messages from channels, routes them, dispatches jobs, and returns responses.

**Key Types**:
- `Agent` -- holds config, deps, channels, scheduler, router, session manager, context monitor
- `AgentDeps` -- bundles shared components: `store`, `llm`, `safety`, `tools`, `workspace`, `extension_manager`
- `AgenticLoopResult` -- `Response(String)` or `NeedApproval { pending }`

**Key Methods**:
- `Agent::new()` -- create agent with all dependencies
- `Agent::run()` -- main message loop: receive from channels, parse submission, route, schedule, respond
- `truncate_for_preview()` -- collapse tool output for terminal display

**Dependencies**: `Router`, `Scheduler`, `SessionManager`, `ContextMonitor`, `ChannelManager`, `SubmissionParser`, `ContextCompactor`, `HeartbeatConfig`, `RoutineEngine`, `ConfigWatcher`

---

### Router (`src/agent/router.rs`)

**Purpose**: Routes explicit slash commands (starting with `/`) to the appropriate `MessageIntent`. Natural language classification is delegated to `IntentClassifier`.

**Key Types**:
- `MessageIntent` -- enum: `CreateJob`, `CheckJobStatus`, `CancelJob`, `ListJobs`, `HelpJob`, `Chat`, `Command`, `Unknown`
- `Router` -- holds a configurable command prefix (default `/`)

**Key Methods**:
- `is_command()` -- check if a message starts with the command prefix
- `route_command()` -- parse command into `MessageIntent`
- `with_prefix()` -- set custom command prefix

**Dependencies**: `IncomingMessage`

---

### Scheduler (`src/agent/scheduler.rs`)

**Purpose**: Manages parallel job execution with configurable concurrency limits. Spawns workers for LLM-driven jobs and tracks sub-tasks.

**Key Types**:
- `Scheduler` -- holds config, context_manager, LLM, safety, tools, store, jobs map, subtasks map
- `ScheduledJob` -- `JoinHandle` + `mpsc::Sender<WorkerMessage>`
- `WorkerMessage` -- `Start`, `Stop`, `Ping`

**Key Methods**:
- `schedule(job_id)` -- spawn a worker for a job (enforces `max_parallel_jobs`)
- `cancel(job_id)` -- send Stop message to a running job
- `schedule_subtask()` -- spawn a background sub-task
- `active_job_count()` -- count running jobs

**Dependencies**: `Worker`, `WorkerDeps`, `ContextManager`, `JobContext`, `LlmProvider`, `SafetyLayer`, `ToolRegistry`, `Database`

---

### Session (`src/agent/session.rs`)

**Purpose**: Session and thread model for turn-based agent interactions. Supports undo, interrupt, compaction, and resume.

**Key Types**:
- `Session` -- contains `id`, `user_id`, `active_thread`, `threads` map, `auto_approved_tools`
- `Thread` -- conversation sequence within a session, contains `Turn` entries
- `Turn` -- request/response pair
- `ThreadState` -- `Idle`, `Processing`, `WaitingApproval`
- `PendingApproval` -- tool approval request awaiting user response

**Key Methods**:
- `Session::new(user_id)` -- create session
- `create_thread()` -- add a new thread
- `get_or_create_thread()` -- lazy thread creation
- `switch_thread()` -- change active thread
- `is_tool_auto_approved()` / `auto_approve_tool()` -- per-session tool approval

**Dependencies**: `ChatMessage`

---

### SessionManager (`src/agent/session_manager.rs`)

**Purpose**: Maps external channel thread IDs to internal UUIDs and manages undo state for each thread. Multi-user, multi-thread conversation handling.

**Key Types**:
- `SessionManager` -- holds `sessions`, `thread_map`, `undo_managers` (all behind `RwLock`)
- `ThreadKey` -- `(user_id, channel, external_thread_id)`

**Key Methods**:
- `get_or_create_session(user_id)` -- double-checked locking for session creation
- `resolve_thread(user_id, channel, external_thread_id)` -- resolve or create internal thread
- `get_undo_manager(thread_id)` -- per-thread undo state

**Dependencies**: `Session`, `UndoManager`

---

### SelfRepair (`src/agent/self_repair.rs`)

**Purpose**: Detects and recovers stuck jobs and broken tools. Trait-based for extensibility.

**Key Types**:
- `SelfRepair` (trait) -- `detect_stuck_jobs()`, `repair_stuck_job()`, `detect_broken_tools()`, `repair_broken_tool()`
- `DefaultSelfRepair` -- implementation with configurable `stuck_threshold` and `max_repair_attempts`
- `StuckJob` -- `job_id`, `last_activity`, `stuck_duration`, `repair_attempts`
- `BrokenTool` -- `name`, `failure_count`, `last_error`, `repair_attempts`
- `RepairResult` -- `Success`, `Retry`, `Failed`, `ManualRequired`

**Dependencies**: `ContextManager`, `Database`, `SoftwareBuilder`, `ToolRegistry`

---

### RoutineEngine (`src/agent/routine_engine.rs`)

**Purpose**: Cron-based and event-driven scheduled job execution. Runs two loops: a cron ticker polling the DB and an event matcher called from the agent main loop.

**Key Types**:
- `RoutineEngine` -- holds config, store, LLM, workspace, notify sender, running count, event regex cache

**Key Methods**:
- `refresh_event_cache()` -- reload event trigger regexes from DB
- `check_event_triggers(message)` -- match incoming messages against event patterns
- `spawn_cron_ticker()` -- start the periodic cron polling loop

**Dependencies**: `Routine`, `RoutineConfig`, `Database`, `LlmProvider`, `Workspace`, `OutgoingResponse`

---

### Heartbeat (`src/agent/heartbeat.rs`)

**Purpose**: Proactive periodic execution (default: 30 minutes). Reads HEARTBEAT.md checklist, runs an agent turn, and reports findings only when action is needed.

**Key Types**:
- `HeartbeatConfig` -- `interval`, `enabled`, `max_failures`, `notify_user_id`, `notify_channel`

**Key Methods**:
- `spawn_heartbeat()` -- start the heartbeat background task
- Replies "HEARTBEAT_OK" if nothing needs attention (no notification sent)

**Dependencies**: `LlmProvider`, `Workspace`, `OutgoingResponse`

---

### ContextMonitor (`src/agent/context_monitor.rs`)

**Purpose**: Monitors conversation context size and triggers compaction when approaching the token limit (default: 100K tokens, threshold: 80%).

**Key Types**:
- `ContextMonitor` -- holds `context_limit` and `threshold_ratio`
- `CompactionStrategy` -- `Summarize { keep_recent }`, `Truncate { keep_recent }`, `MoveToWorkspace`

**Key Methods**:
- `estimate_tokens(messages)` -- approximate token count (~1.3 tokens/word)
- `needs_compaction(messages)` -- check if context exceeds threshold
- `with_limit()` / `with_threshold()` -- builder methods

**Dependencies**: `ChatMessage`

---

### Worker (`src/agent/worker.rs`)

**Purpose**: Executes a single job by running the LLM reasoning + tool execution loop.

**Dependencies**: `WorkerDeps`, `LlmProvider`, `ToolRegistry`, `SafetyLayer`, `ContextManager`

---

### Additional Agent Files

| File | Purpose |
|------|---------|
| `compaction.rs` | Context compaction (summarize old turns) |
| `config_reload.rs` | File system watching with broadcast notifications for hot-reload |
| `command_queue.rs` | Queued command processing |
| `submission.rs` | Parse raw messages into structured `Submission` objects |
| `task.rs` | `Task` and `TaskContext` types for sub-task execution |
| `undo.rs` | Undo/redo manager for conversation turns |
| `routine.rs` | `Routine`, `Trigger`, `RoutineAction`, `RoutineRun` data types |
| `session_pruning.rs` | Cleanup of expired/idle sessions |
| `multi_agent.rs` | Multi-agent coordination |
| `auth_profiles.rs` | Per-user authentication profiles |

---

## Channel Components

### Channel Trait (`src/channels/channel.rs`)

**Purpose**: Defines the interface all input sources must implement and the core message types.

**Key Types**:
- `Channel` (trait) -- `name()`, `start()`, `respond()`, `send_status()`
- `IncomingMessage` -- `id`, `channel`, `user_id`, `content`, `thread_id`, `received_at`, `metadata`
- `OutgoingResponse` -- `content`, `thread_id`, `channel`, `user_id`
- `StatusUpdate` -- status/progress notifications
- `MessageStream` -- `Pin<Box<dyn Stream<Item = IncomingMessage> + Send>>`

**Key Methods on IncomingMessage**:
- `new(channel, user_id, content)` -- create message
- `with_thread()`, `with_metadata()`, `with_user_name()` -- builder methods

---

### ChannelManager (`src/channels/manager.rs`)

**Purpose**: Coordinates multiple input channels and merges their message streams into a single unified stream.

**Key Methods**:
- `add(channel)` -- register a channel
- `start_all()` -- start all channels and merge via `stream::select_all`
- `respond(msg, response)` -- route response to the originating channel
- `send_status(msg, status)` -- send status update to a channel

**Dependencies**: `Channel`, `IncomingMessage`, `OutgoingResponse`

---

### ReplChannel (`src/channels/repl.rs`)

**Purpose**: Interactive CLI interface with line editing (rustyline), history, tab-completion, and markdown rendering (termimad).

**Key Features**:
- Slash commands: `/help`, `/quit`, `/debug`, `/undo`, `/redo`, `/clear`, `/compact`, `/new`, `/tools`, `/version`
- Tool approval responses: `yes`, `no`, `always`
- Markdown rendering for responses

**Dependencies**: `rustyline`, `termimad`, `Channel` trait

---

### HttpChannel (`src/channels/http.rs`)

**Purpose**: HTTP webhook channel for receiving messages via POST with rate limiting, webhook secret auth, and request/response pairing.

**Key Types**:
- `HttpChannel` -- config, shared state
- `HttpChannelState` -- message sender, pending responses, webhook secret, rate limit

**Configuration**: `HttpConfig` (port, webhook_secret, user_id)

**Dependencies**: `axum`, `Channel` trait

---

### GatewayChannel (`src/channels/web/mod.rs`)

**Purpose**: Full web gateway for browser-based access. Single-page UI with REST + SSE + WebSocket support.

**Key Sub-modules** (15 files):
- `server.rs` -- Axum router setup, `GatewayState`
- `sse.rs` -- Server-Sent Events manager
- `ws.rs` -- WebSocket bidirectional support
- `auth.rs` -- Token-based authentication
- `types.rs` -- SSE event type definitions
- `canvas.rs` -- Canvas/drawing support
- `config_editor.rs` -- Live config editing UI
- `log_layer.rs` -- Log broadcasting via `LogBroadcaster`
- `mdns.rs` -- mDNS service discovery
- `network_mode.rs` -- Network mode configuration
- `openai_compat.rs` -- OpenAI-compatible API endpoint
- `pid_lock.rs` -- PID file locking
- `presence.rs` -- User presence tracking
- `tailscale.rs` -- Tailscale integration
- `agent_management.rs` -- Multi-agent management API

**Dependencies**: `axum`, `Database`, `Workspace`, `ToolRegistry`, `SessionManager`, `ExtensionManager`

---

### WASM Channel System (`src/channels/wasm/`)

**Purpose**: Pluggable WASM channels loaded at runtime. Each implements the Channel WIT interface. (9 files)

**Key Files**:
- `wrapper.rs` -- `WasmChannelWrapper` adapts WASM components to the `Channel` trait
- `runtime.rs` -- WASM component runtime management
- `host.rs` -- Host function implementations for WASM channels
- `loader.rs` -- Load WASM channel components from disk
- `capabilities.rs` -- Channel capability declarations
- `bundled.rs` -- Pre-built channel registration
- `router.rs` -- WASM channel message routing
- `schema.rs` -- Schema validation
- `error.rs` -- WASM channel error types

---

### Additional Channel Files

| File | Purpose |
|------|---------|
| `block_streamer.rs` | Stream responses in blocks for progressive rendering |
| `inline_commands.rs` | Parse inline commands within messages |
| `self_message.rs` | Agent-to-agent self-messaging |
| `status_tracker.rs` | Track message processing status |
| `webhook_server.rs` | Inbound webhook server for external services |
| `delivery_retry.rs` | Retry failed message deliveries |

---

## Core Components

### Database Trait (`src/db/mod.rs`)

**Purpose**: Backend-agnostic persistence trait (~91 async methods) unifying all storage operations. Two implementations behind feature flags.

**Key Method Groups**:
- **Conversations**: `create_conversation`, `add_conversation_message`, `list_conversation_messages`, `ensure_conversation`
- **Jobs**: `save_job`, `get_job`, `update_job_status`, `mark_job_stuck`, `get_stuck_jobs`
- **Actions**: `save_action`, `get_job_actions`
- **LLM Calls**: `record_llm_call`
- **Sandbox Jobs**: `save_sandbox_job`, `list_sandbox_jobs`, `update_sandbox_job_status`, `cleanup_stale_sandbox_jobs`
- **Routines**: `create_routine`, `list_due_cron_routines`, `list_event_routines`, `update_routine_runtime`
- **Estimation**: `save_estimation_snapshot`, `update_estimation_actuals`
- **Settings**: `get_all_settings`, `set_setting`
- **Workspace/Memory**: `get_document_by_path`, `search_documents`, `save_document`, `save_chunks`

**Backends**:
- `PgBackend` (`src/db/postgres.rs`) -- PostgreSQL via `deadpool-postgres` + `tokio-postgres`
- `LibSqlBackend` (`src/db/libsql_backend.rs`) -- libSQL/Turso for embedded/edge deployment

**Helper**: `connect_from_config()` -- factory function that creates the right backend, runs migrations, returns `Arc<dyn Database>`

---

### Config (`src/config.rs`)

**Purpose**: Hierarchical configuration (env vars > DB settings > defaults). Contains all config structs for every subsystem.

**Key Types**:
- `Config` -- top-level: `database`, `llm`, `embeddings`, `tunnel`, `channels`, `agent`, `safety`, `wasm`, `secrets`, `builder`, `heartbeat`, `routines`, `sandbox`, `claude_code`
- `AgentConfig` -- `max_parallel_jobs`, agent behavior settings
- `LlmConfig` -- `backend`, provider-specific settings
- `SafetyConfig` -- `max_output_length`, safety thresholds
- `GatewayConfig` -- web gateway port, auth
- `RoutineConfig` -- cron interval, max concurrent routines

**Key Methods**:
- `Config::from_db(store, user_id, bootstrap)` -- load from database
- `Config::from_env()` -- load from environment only

---

### Error (`src/error.rs`)

**Purpose**: Centralized error types using `thiserror`. Top-level `Error` enum wraps all domain errors.

**Error Types**: `ConfigError`, `DatabaseError`, `ChannelError`, `LlmError`, `ToolError`, `SafetyError`, `JobError`, `EstimationError`, `EvaluationError`, `RepairError`, `WorkspaceError`, `OrchestratorError`, `WorkerError`, `HookError`, `MediaError`, `SkillsError`

---

## LLM Components

### LlmProvider Trait (`src/llm/provider.rs`)

**Purpose**: Backend-agnostic interface for LLM providers.

**Key Types**:
- `LlmProvider` (trait) -- `model_name()`, `cost_per_token()`, `complete()`, `complete_with_tools()`, `list_models()`, `model_metadata()`
- `ChatMessage` -- `role`, `content`, `tool_call_id`, `name`, `tool_calls`
- `Role` -- `System`, `User`, `Assistant`, `Tool`
- `CompletionRequest` / `CompletionResponse` -- standard chat completion
- `ToolCompletionRequest` / `ToolCompletionResponse` -- completion with tool use
- `ToolDefinition` -- name, description, parameters schema
- `ToolCall` -- `id`, `name`, `arguments`
- `FinishReason` -- `Stop`, `Length`, `ToolUse`, `ContentFilter`, `Unknown`
- `ModelMetadata` -- `id`, `context_length`

---

### Provider Factory (`src/llm/mod.rs`)

**Purpose**: Creates the appropriate LLM provider based on config. Supports 8 backends.

**Function**: `create_llm_provider(config, session)` -- dispatches to backend-specific constructors

**Backends**:

| Provider | File | Description |
|----------|------|-------------|
| NEAR AI (Responses) | `nearai.rs` | Session-based auth via NEAR AI proxy |
| NEAR AI (Chat) | `nearai_chat.rs` | API key auth, Chat Completions API |
| OpenAI | via `rig_adapter.rs` | Direct API, rig-core adapter |
| Anthropic | via `rig_adapter.rs` | Direct API, rig-core adapter |
| Ollama | via `rig_adapter.rs` | Local model inference |
| OpenAI-compatible | via `rig_adapter.rs` | Any OpenAI-compatible endpoint |
| Google Gemini | `gemini.rs` | Direct API with native function calling |
| AWS Bedrock | `bedrock.rs` | SigV4 auth for AWS-managed models |
| OpenRouter | `openrouter.rs` | Multi-model routing service |

---

### FailoverProvider (`src/llm/failover.rs`)

**Purpose**: Automatic failover between LLM providers with exponential backoff cooldowns.

**Key Types**:
- `FailoverProvider` -- implements `LlmProvider`, wraps multiple providers
- `ProviderState` -- tracks `consecutive_failures`, `last_success`, `cooldown_until`, `total_requests`, `total_errors`
- `ProviderEntry` -- named provider in the failover chain

**Behavior**: On failure, applies exponential backoff (cooldown * 2^(failures-1), capped at 5 minutes). Tries next available provider.

---

### Additional LLM Files

| File | Purpose |
|------|---------|
| `reasoning.rs` | `Reasoning` engine: plan, select tools, generate response; `ActionPlan`, `ToolSelection`, `TokenUsage` |
| `rig_adapter.rs` | `RigAdapter` -- adapts rig-core `CompletionClient` to `LlmProvider` trait |
| `session.rs` | `SessionManager` for NEAR AI session-based auth |
| `costs.rs` | Per-model cost tables for token pricing |
| `auto_discovery.rs` | `ModelDiscovery` -- list available models from any provider |
| `thinking.rs` | `ThinkingMode` -- extended thinking / chain-of-thought configuration |

---

## Tool Components

### Tool Trait (`src/tools/tool.rs`)

**Purpose**: Interface for all executable capabilities (built-in, WASM, MCP).

**Key Types**:
- `Tool` (trait) -- `name()`, `description()`, `parameters_schema()`, `execute(params, ctx)`, `estimated_cost()`, `estimated_duration()`, `requires_sanitization()`, `requires_approval()`, `timeout()`
- `ToolDomain` -- `Orchestrator` (safe, in-process) or `Container` (sandboxed)
- `ToolOutput` -- `result` (JSON), `cost`, `duration`, `raw`
- `ToolError` -- `InvalidParameters`, `ExecutionFailed`, `Timeout`, `NotAuthorized`, `RateLimited`, `ExternalService`, `Sandbox`
- `ToolSchema` -- `name`, `description`, `parameters` (JSON Schema)

---

### ToolRegistry (`src/tools/registry.rs`)

**Purpose**: Central registry of available tools with protection against shadowing built-in tools.

**Key Methods**:
- `register(tool)` -- async registration, rejects shadowing of protected names
- `register_sync(tool)` -- sync registration at startup, marks as built-in
- `get(name)` -- look up a tool by name
- `list()` -- list all registered tools
- `register_builtin_tools()` -- phase-based registration of all built-in tools
- `get_tool_definitions()` -- convert all tools to `ToolDefinition` for LLM

**Protected Names**: `echo`, `time`, `json`, `http`, `shell`, `read_file`, `write_file`, `list_dir`, `apply_patch`, `memory_*`, `create_job`, `list_jobs`, `job_status`, `cancel_job`, `build_software`, `tool_*`, `routine_*`

---

### Built-in Tools (`src/tools/builtin/`)

**Purpose**: Rust-implemented tools registered in phases. 16 files.

| Tool | File | Requires Approval |
|------|------|:-:|
| `EchoTool` | `echo.rs` | No |
| `TimeTool` | `time.rs` | No |
| `JsonTool` | `json.rs` | No |
| `HttpTool` | `http.rs` | Yes |
| `ShellTool` | `shell.rs` | Yes |
| `ReadFileTool` | `file.rs` | No |
| `WriteFileTool` | `file.rs` | Yes |
| `ListDirTool` | `file.rs` | No |
| `ApplyPatchTool` | `file.rs` | Yes |
| `MemorySearchTool` | `memory.rs` | No |
| `MemoryWriteTool` | `memory.rs` | No |
| `MemoryReadTool` | `memory.rs` | No |
| `MemoryTreeTool` | `memory.rs` | No |
| `MemoryConnectTool` | `memory.rs` | No |
| `MemorySpacesTool` | `memory.rs` | No |
| `MemoryProfileTool` | `memory.rs` | No |
| `CreateJobTool` | `job.rs` | No |
| `ListJobsTool` | `job.rs` | No |
| `JobStatusTool` | `job.rs` | No |
| `CancelJobTool` | `job.rs` | No |
| `BuildSoftwareTool` | via `builder/` | Yes |
| `ToolSearchTool` | `extension_tools.rs` | No |
| `ToolInstallTool` | `extension_tools.rs` | No |
| `ToolAuthTool` | `extension_tools.rs` | No |
| `ToolActivateTool` | `extension_tools.rs` | No |
| `ToolListTool` | `extension_tools.rs` | No |
| `ToolRemoveTool` | `extension_tools.rs` | No |
| `RoutineCreateTool` | `routine.rs` | No |
| `BrowserTool` | `browser.rs` | Yes |
| `SessionTools` | `session_tools.rs` | No |

Additional domain tools: `ecommerce.rs`, `marketplace.rs`, `restaurant.rs`, `taskrabbit.rs`

---

### WASM Tool System (`src/tools/wasm/`)

**Purpose**: Sandboxed WASM tool runtime with capability-based security. 13 files.

| File | Purpose |
|------|---------|
| `mod.rs` | Module exports and types |
| `runtime.rs` | `WasmToolRuntime` -- component model runtime |
| `wrapper.rs` | `WasmToolWrapper` -- adapts WASM components to `Tool` trait |
| `host.rs` | Host function implementations for WASM tools |
| `loader.rs` | Load WASM components from disk |
| `capabilities.rs` | `Capabilities` -- parsed capability declarations |
| `capabilities_schema.rs` | JSON schema validation for capabilities |
| `credential_injector.rs` | Inject credentials at the sandbox boundary |
| `allowlist.rs` | URL/domain allowlist for WASM HTTP requests |
| `rate_limiter.rs` | Per-tool rate limiting |
| `limits.rs` | `ResourceLimits` -- memory, CPU, timeout |
| `storage.rs` | `WasmToolStore` -- persistent tool metadata |
| `error.rs` | `WasmError`, `WasmStorageError` |

---

### MCP Tool System (`src/tools/mcp/`)

**Purpose**: Model Context Protocol client for connecting to external MCP servers. 6 files.

| File | Purpose |
|------|---------|
| `mod.rs` | Module exports |
| `client.rs` | MCP HTTP client implementation |
| `protocol.rs` | MCP protocol message types |
| `config.rs` | MCP server configuration |
| `session.rs` | MCP session management |
| `auth.rs` | MCP authentication (OAuth, API keys) |

---

### Software Builder (`src/tools/builder/`)

**Purpose**: AI-driven software building tool that generates, tests, and packages WASM tools. 5 files.

| File | Purpose |
|------|---------|
| `mod.rs` | `BuildSoftwareTool`, `LlmSoftwareBuilder`, `BuilderConfig` |
| `core.rs` | Core build logic and `SoftwareBuilder` trait |
| `templates.rs` | Code templates for generated tools |
| `testing.rs` | Automated test execution for built tools |
| `validation.rs` | Build output validation |

---

### Tool Sandbox (`src/tools/sandbox.rs`)

**Purpose**: Tool execution within the Docker sandbox for container-domain tools.

---

## Safety Components

### SafetyLayer (`src/safety/mod.rs`)

**Purpose**: Unified safety layer combining sanitizer, validator, policy, and leak detector. All external tool output passes through this before reaching the LLM.

**Key Methods**:
- `sanitize_tool_output(tool_name, output)` -- full pipeline: size check, leak detection, injection scanning, policy evaluation
- `validate_input(content)` -- validate user input
- `check_policy(content)` -- check against policy rules

**Dependencies**: `Sanitizer`, `Validator`, `Policy`, `LeakDetector`, `SafetyConfig`

---

### Sanitizer (`src/safety/sanitizer.rs`)

**Purpose**: Detects and neutralizes prompt injection attempts in tool outputs.

**Key Techniques**:
- Strip invisible Unicode characters (zero-width spaces, RTL marks, etc.)
- Normalize Unicode confusables/homoglyphs to ASCII
- Decode HTML/XML entity encoding (numeric and named references)
- Aho-Corasick multi-pattern matching for injection patterns
- Regex-based detection for complex patterns

**Key Types**:
- `Sanitizer` -- main sanitizer with compiled patterns
- `SanitizedOutput` -- `content`, `warnings`, `was_modified`
- `InjectionWarning` -- `pattern`, `severity`, `location`, `description`

---

### Validator (`src/safety/validator.rs`)

**Purpose**: Input validation with composable validation results.

**Key Types**:
- `Validator` -- validates inputs against rules
- `ValidationResult` -- `is_valid`, `errors`, `warnings`; supports `merge()` for combining results
- `ValidationError` -- `field`, `message`, `code`

---

### Policy (`src/safety/policy.rs`)

**Purpose**: Regex-based safety policy rules with severity levels and configurable actions.

**Key Types**:
- `Policy` -- collection of `PolicyRule` entries
- `PolicyRule` -- `id`, `description`, `pattern` (regex), `severity`, `action`
- `Severity` -- `Low`, `Medium`, `High`, `Critical`
- `PolicyAction` -- `Allow`, `Warn`, `Block`, `Redact`

---

### LeakDetector (`src/safety/leak_detector.rs`)

**Purpose**: Scans data at sandbox boundaries to prevent secret exfiltration. Uses Aho-Corasick for fast multi-pattern matching plus regex for complex patterns.

**Key Types**:
- `LeakDetector` -- holds compiled patterns and known secrets
- `LeakScanResult` -- scan results with matches
- `LeakMatch` -- `pattern_name`, `location`, `severity`, `action`
- `LeakAction` -- `Block`, `Redact`, `Warn`
- `LeakSeverity` -- `Low`, `Medium`, `High`, `Critical`

**Scan Points**: Before outbound requests (prevent exfiltration) and after responses (prevent exposure)

---

### OAuthFlowManager (`src/safety/oauth.rs`)

**Purpose**: Complete OAuth 2.0/2.1 authorization flow with PKCE support for tool and extension authentication.

**Key Types**:
- `OAuthConfig` -- `client_id`, `client_secret` (SecretString, skipped in serde), `authorize_url`, `token_url`, `redirect_uri`, `scopes`, `use_pkce`
- `OAuthTokens` -- `access_token`, `refresh_token` (both SecretString), `expires_at`
- `OAuthFlowManager` -- manages concurrent OAuth flows

**Security**: All secrets stored as `SecretString` (never serialized), PKCE verifier protected, OsRng for random values

---

### Additional Safety Files

| File | Purpose |
|------|---------|
| `allowlist.rs` | URL/domain allowlists for tool HTTP access |
| `bins_allowlist.rs` | Binary/executable allowlists for shell tool |
| `elevated.rs` | `ElevatedMode` -- session-bound privilege escalation |
| `group_policies.rs` | `GroupPolicyManager` -- per-group ACLs |
| `log_redaction.rs` | `LogRedactor` -- credential redaction in logs |

---

## Media Components

### Media Module (`src/media/mod.rs`)

**Purpose**: Processing for various media types. 12 files.

| Component | File | Purpose |
|-----------|------|---------|
| `TtsProvider` (trait) | `tts.rs` | Text-to-speech synthesis (OpenAI TTS API) |
| `OpenAiTtsProvider` | `tts.rs` | OpenAI TTS implementation with voice/format options |
| `EdgeTtsProvider` | `edge_tts.rs` | Microsoft Edge TTS via WebSocket (free, 300+ voices) |
| `TranscriptionProvider` (trait) | `transcription.rs` | Audio-to-text transcription |
| `VisionProvider` (trait) | `vision.rs` | Image understanding via vision models |
| `ImageProcessor` | `image.rs` | Image resize, format conversion |
| `PdfExtractor` | `pdf.rs` | PDF text extraction |
| `VideoProcessor` | `video.rs` | Video metadata extraction (MP4, WebM, AVI, MOV, MKV) |
| `StickerConverter` | `sticker.rs` | WebP/TGS sticker-to-image conversion |
| `MediaCache` | `cache.rs` | Media file caching |
| `detect_mime_type` | `detection.rs` | MIME type detection and URL validation |
| `LargeDocumentProcessor` | `large_doc.rs` | Recursive Language Model (RLM) processing for large docs |

**Key Types**: `TtsVoice`, `TtsFormat`, `VoiceGender`, `ImageFormat`, `ProcessedImage`, `PdfPage`, `VideoInfo`, `VideoFormat`, `MediaType`, `MediaInfo`, `EdgeVoice`, `RlmConfig`, `RlmOperation`

---

## Workspace Components

### Workspace (`src/workspace/mod.rs`)

**Purpose**: Filesystem-like persistent memory system. Agents create markdown file hierarchies indexed for full-text and semantic search.

**Key Operations**:
- `read(path)` -- read a file
- `write(path, content)` -- create or update
- `append(path, content)` -- append to a file
- `list(dir)` -- list directory contents
- `delete(path)` -- delete a file
- `search(query)` -- hybrid FTS + vector search

**Storage Abstraction**: `WorkspaceStorage` enum -- `Repo(Repository)` for PostgreSQL or `Db(Arc<dyn Database>)` for any backend

---

### EmbeddingProvider Trait (`src/workspace/embeddings.rs`)

**Purpose**: Generate dense vectors for semantic search.

**Trait Methods**: `dimension()`, `model_name()`, `max_input_length()`, `embed(text)`, `embed_batch(texts)`

**Implementations**:

| Provider | File | Description |
|----------|------|-------------|
| `OpenAiEmbeddings` | `embeddings.rs` | text-embedding-3-small (1536 dims) |
| `NearAiEmbeddings` | `embeddings.rs` | NEAR AI embedding proxy |
| `GeminiEmbeddings` | `gemini_embeddings.rs` | Google Gemini embedding API |
| `LocalEmbeddings` | `local_embeddings.rs` | Hash-based local embeddings (no API) |
| `MockEmbeddings` | `embeddings.rs` | Test stub |

---

### Additional Workspace Files

| File | Purpose |
|------|---------|
| `document.rs` | `MemoryDocument`, `MemoryChunk`, `MemoryConnection`, `MemorySpace`, `UserProfile`, `WorkspaceEntry`, `ConnectionType`, `ProfileType` |
| `chunker.rs` | `chunk_document()` -- split documents into chunks with `ChunkConfig` |
| `search.rs` | `SearchConfig`, `SearchResult`, `RankedResult`, `reciprocal_rank_fusion()` -- hybrid FTS + vector via RRF |
| `repository.rs` | PostgreSQL-specific `Repository` (connection pool queries) |
| `batch_embeddings.rs` | `BatchEmbeddingProcessor` -- queue-based batch embedding processing |

---

## Extension Components

### ExtensionManager (`src/extensions/manager.rs`)

**Purpose**: Orchestrates discovering, installing, authenticating, and activating MCP servers and WASM tools at runtime.

---

### ExtensionRegistry (`src/extensions/registry.rs`)

**Purpose**: Built-in registry of known extensions with search capability.

**Key Types**:
- `RegistryEntry` -- `name`, `display_name`, `kind`, `description`, `keywords`, `source`, `auth_hint`
- `ExtensionKind` -- `McpServer`, `WasmTool`, `WasmChannel`
- `ExtensionSource` -- `McpUrl { url }`, `WasmDownload { wasm_url }`, `WasmBundled { path }`
- `AuthHint` -- `None`, `ApiKey`, `OAuth`, `Custom`

---

### Additional Extension Files

| File | Purpose |
|------|---------|
| `discovery.rs` | `OnlineDiscovery` -- discover extensions from online sources |
| `clawhub.rs` | ClawHub marketplace client |
| `plugin_manager.rs` | `PluginManager` -- lifecycle management for plugins: `PluginSnapshot`, `PluginSummary` |
| `plugins.rs` | Plugin data types and configuration |
| `mod.rs` | Module exports, `ExtensionKind` enum |

---

## Hook Components

### HookEngine (`src/hooks/engine.rs`)

**Purpose**: Manages hook registration, ordering by priority, and execution.

**Key Methods**:
- `register(hook)` -- register with duplicate name check
- `unregister(hook_type, name)` -- remove a hook
- `list_hooks()` / `list_hooks_by_type(hook_type)` -- enumerate registered hooks
- `run_before_inbound()` -- execute `BeforeInbound` hooks
- `run_before_outbound()` -- execute `BeforeOutbound` hooks
- `run_before_tool_call()` -- execute `BeforeToolCall` hooks
- `run_transform_response()` -- execute `TransformResponse` hooks

---

### Hook Types (`src/hooks/types.rs`)

**Purpose**: Type definitions for the hook system.

**Key Types**:
- `HookType` -- `BeforeInbound`, `BeforeOutbound`, `BeforeToolCall`, `OnSessionStart`, `OnSessionEnd`, `TransformResponse`, `TranscribeAudio`
- `HookPriority` -- `System` (0), `High` (10), `Normal` (50), `Low` (90)
- `HookSource` -- `Builtin`, `Plugin { name }`, `Workspace`, `User`
- `HookAction` -- Shell command, HTTP request, or inline function
- `HookContext` -- execution context with message, session, metadata
- `HookOutcome` -- `Continue`, `Block`, `Modify`
- Result types: `InboundHookResult`, `OutboundHookResult`, `ToolCallHookResult`, `TransformResponseResult`

---

### Additional Hook Files

| File | Purpose |
|------|---------|
| `bundled.rs` | 8 bundled hooks: `profanity_filter`, `rate_limit_guard`, `sensitive_data_redactor`, etc. |
| `webhooks.rs` | Outbound webhooks with HMAC-SHA256 signatures and retry logic |
| `gmail_pubsub.rs` | Gmail pub/sub handler with watch setup and deduplication |
| `transcribe.rs` | Audio transcription hook integration |

---

## Sandbox Components

### Sandbox System (`src/sandbox/`)

**Purpose**: Docker-based execution sandbox with network proxy for secure command execution. 9 files.

**Key Types**:
- `SandboxManager` -- coordinates container creation, proxy lifecycle, resource limits
- `SandboxManagerBuilder` -- builder pattern for sandbox configuration
- `SandboxPolicy` -- `ReadOnly`, `WorkspaceWrite`, `FullAccess`

**Key Methods**:
- `initialize()` -- prepare Docker environment
- `execute(command, workdir, env)` -- run command in container
- `shutdown()` -- cleanup containers

| File | Purpose |
|------|---------|
| `mod.rs` | Module exports, architecture docs |
| `manager.rs` | `SandboxManager` implementation |
| `container.rs` | Docker container lifecycle |
| `config.rs` | Sandbox configuration types |
| `error.rs` | Sandbox error types |
| `proxy/mod.rs` | Network proxy coordinator |
| `proxy/allowlist.rs` | Domain/URL allowlisting |
| `proxy/policy.rs` | Proxy security policies |
| `proxy/http.rs` | HTTP proxy implementation |

**Security Properties**: No credentials in containers (injected by proxy), all traffic proxied, resource limits enforced (memory, CPU, timeout).

---

## CLI Components

### CLI Module (`src/cli/`)

**Purpose**: CLI subcommands for all features. 23 files, one per command group.

| File | Subcommand | Purpose |
|------|------------|---------|
| `mod.rs` | -- | CLI entry point and argument parsing |
| `agents.rs` | `agents` | Multi-agent management |
| `browser.rs` | `browser` | Browser automation |
| `channels.rs` | `channels` | Channel management |
| `completion.rs` | `completion` | Shell completion generation |
| `config.rs` | `config` | Configuration viewing/editing |
| `cron.rs` | `cron` | Routine/cron management |
| `doctor.rs` | `doctor` | System health checks |
| `gateway.rs` | `gateway` | Web gateway management |
| `hooks.rs` | `hooks` | Hook management |
| `logs.rs` | `logs` | Log viewing |
| `mcp.rs` | `mcp` | MCP server management |
| `memory.rs` | `memory` | Workspace/memory operations |
| `message.rs` | `message` | Send a one-shot message |
| `nodes.rs` | `nodes` | Distributed node management |
| `pairing.rs` | `pairing` | Device pairing |
| `plugins.rs` | `plugins` | Plugin management |
| `service.rs` | `service` | System service management |
| `sessions.rs` | `sessions` | Session management |
| `skills.rs` | `skills` | Skill management |
| `status.rs` | `status` | System status |
| `tool.rs` | `tool` | Tool testing and inspection |
| `webhooks.rs` | `webhooks` | Webhook management |

---

## Component Dependencies

```
┌──────────────────────────────────────────────────────────────┐
│  Agent                                                        │
│    ├── Router ─────────────── IncomingMessage                 │
│    ├── Scheduler ──────────── Worker ──────────────┐          │
│    │     └── ContextManager                        │          │
│    ├── SessionManager ─────── Session              │          │
│    │     └── UndoManager                           │          │
│    ├── ContextMonitor ─────── ChatMessage           │          │
│    ├── SelfRepair ─────────── ContextManager        │          │
│    ├── RoutineEngine ──────── Database, LlmProvider │          │
│    └── Heartbeat ──────────── LlmProvider, Workspace│          │
│                                                     │          │
│  ChannelManager ─── Channel (trait) ────────────────┤          │
│    ├── ReplChannel                                  │          │
│    ├── HttpChannel                                  │          │
│    ├── GatewayChannel ──── Database, Workspace      │          │
│    └── WasmChannelWrapper                           │          │
│                                                     │          │
│  ToolRegistry ──── Tool (trait) ────────────────────┤          │
│    ├── Built-in Tools ──── Database, Workspace      │          │
│    ├── WasmToolWrapper ─── WasmToolRuntime          │          │
│    └── MCP Tools ───────── McpClient                │          │
│                                                     │          │
│  SafetyLayer ───────────────────────────────────────┤          │
│    ├── Sanitizer                                    │          │
│    ├── Validator                                    │          │
│    ├── Policy                                       │          │
│    └── LeakDetector                                 │          │
│                                                     │          │
│  LlmProvider (trait) ───────────────────────────────┘          │
│    ├── NearAiProvider                                          │
│    ├── RigAdapter (OpenAI, Anthropic, Ollama, Compatible)     │
│    ├── GeminiProvider                                          │
│    ├── BedrockProvider                                         │
│    ├── OpenRouterProvider                                      │
│    └── FailoverProvider ── wraps multiple providers            │
│                                                                │
│  Database (trait) ─────────────────────────────────────────── │
│    ├── PgBackend (PostgreSQL)                                  │
│    └── LibSqlBackend (libSQL/Turso)                           │
│                                                                │
│  Workspace ────── Database, EmbeddingProvider                  │
│    └── EmbeddingProvider (trait)                               │
│         ├── OpenAiEmbeddings                                   │
│         ├── GeminiEmbeddings                                   │
│         ├── LocalEmbeddings                                    │
│         └── NearAiEmbeddings                                   │
│                                                                │
│  HookEngine ──── Hook (trait)                                  │
│  ExtensionManager ── ExtensionRegistry, ToolRegistry           │
│  SandboxManager ── Docker, NetworkProxy                        │
└──────────────────────────────────────────────────────────────┘
```

**Shared Dependencies**:
- All components with persistence receive `Arc<dyn Database>` via dependency injection
- `SafetyLayer` is shared across the agent loop and worker via `Arc<SafetyLayer>`
- `ToolRegistry` is shared via `Arc<ToolRegistry>` with `RwLock` for dynamic registration
- `LlmProvider` is shared via `Arc<dyn LlmProvider>`
- `Workspace` is shared via `Arc<Workspace>`
- `Config` is loaded once at startup; `HotReloadConfig<Config>` enables runtime updates
