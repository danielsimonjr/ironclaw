# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo fmt                                                       # Format code
cargo clippy --all --benches --tests --examples --all-features  # Lint (fix warnings before committing)
cargo test                                                      # Run all tests
cargo test test_name                                            # Run a specific test
cargo test safety::sanitizer::tests                             # Run a module's tests
RUST_LOG=ironclaw=debug cargo run                               # Run with debug logging
```

Feature-gated builds:

```bash
cargo build                                          # PostgreSQL only (default)
cargo build --no-default-features --features libsql  # libSQL only
cargo build --features "postgres,libsql"             # Both backends
```

WASM tool/channel builds:

```bash
cargo build --target wasm32-wasip2 --release  # Build WASM component
```

The build script (`build.rs`) automatically compiles the Telegram channel from `channels-src/telegram/`. If you modify channel sources, rebuild before `cargo build`.

## Architecture Overview

IronClaw is a secure personal AI assistant built as a composable, trait-based system in Rust. The core flow is:

```
Channels (REPL, HTTP, WASM, Web Gateway)
    → ChannelManager (merges all input streams)
    → Agent Loop (submission parsing, session resolution)
    → Router (intent classification)
    → Scheduler (parallel job dispatch)
    → Worker (LLM reasoning + tool execution loop)
    → Safety Layer (sanitize tool outputs)
    → Response back through channel
```

### Key Traits (Extension Points)

| Trait | Location | Purpose |
|-------|----------|---------|
| `Database` | `src/db/mod.rs` | ~75 async methods for all persistence. **Both backends must be updated for new features.** |
| `Channel` | `src/channels/channel.rs` | Input sources (REPL, HTTP, WASM, web gateway) |
| `Tool` | `src/tools/tool.rs` | Executable capabilities (built-in, WASM, MCP) |
| `LlmProvider` | `src/llm/provider.rs` | LLM backends (NEAR AI, OpenAI, Anthropic, Gemini, Bedrock, Ollama, OpenRouter) |
| `EmbeddingProvider` | `src/workspace/embeddings.rs` | Vector embedding backends (OpenAI, Gemini, local hash-based) |
| `SuccessEvaluator` | `src/evaluation/success.rs` | Job outcome evaluation |

### Source Modules

The codebase is organized into 28 public modules (`src/lib.rs`):

| Module | Purpose |
|--------|---------|
| `agent` | Core agent loop, routing, scheduling, session management, worker dispatch, self-repair, heartbeat, routine engine, compaction, undo, multi-agent routing, auth profiles, command queue, config reload |
| `bootstrap` | Initial setup, bootstrap config persistence (`~/.ironclaw/bootstrap.json`) |
| `channels` | Input channel abstraction (REPL, HTTP, WASM, web gateway), channel manager, block streaming, delivery retry, inline commands, status tracking, self-message bypass |
| `cli` | CLI subcommands (tool, mcp, memory, config, pairing, status, doctor, gateway, sessions, hooks, cron, logs, message, channels, plugins, webhooks, skills, agents, nodes, browser, completion, service) |
| `config` | Configuration management (env > DB > defaults) |
| `context` | Job context, mutable state, identity memory injection (IDENTITY.md, SOUL.md, etc.) |
| `db` | Database trait + dual backend (PostgreSQL, libSQL) |
| `error` | Typed error hierarchy via `thiserror` |
| `estimation` | Cost/time prediction with ML-based learner |
| `evaluation` | Job outcome success evaluation and metrics |
| `extensions` | Dynamic tool/MCP server discovery, install, auth, lifecycle management, plugin system, ClawHub registry |
| `history` | Job/session/conversation history persistence and analytics |
| `hooks` | Lifecycle hooks engine (beforeInbound, beforeOutbound, beforeToolCall, onSessionStart, onSessionEnd, transformResponse), 8 bundled hooks, outbound webhooks, Gmail pub/sub, audio transcription |
| `hot_reload` | Dynamic component reloading with file system watching |
| `llm` | LLM provider abstraction, multi-provider support (NEAR AI, OpenAI, Anthropic, Gemini, Bedrock, Ollama, OpenRouter), failover chains, auto-discovery, thinking modes, reasoning, cost tracking |
| `media` | Image processing, PDF extraction, audio transcription, video metadata, vision integration, TTS (OpenAI + Edge), sticker conversion, MIME detection, media caching, large document processing via RLM techniques |
| `orchestrator` | Container job orchestration, internal API (`:50051`), per-job bearer token auth |
| `pairing` | DM approval flow for unknown senders, device pairing with challenge codes |
| `safety` | Safety layer (sanitizer → validator → policy), leak detection, log redaction, OAuth 2.0/2.1, allowlist/blocklist ACLs, group policies, elevated mode, safe binaries allowlist |
| `sandbox` | Docker container management, network proxy with HTTP allowlist |
| `secrets` | Encrypted credential vault, system keychain integration (macOS/Linux), AES-GCM crypto |
| `settings` | Runtime settings storage (separate from config) |
| `setup` | Interactive onboarding wizard (multi-provider LLM selection) |
| `skills` | Modular capability bundles with tools, prompts, and policies; vulnerability scanner |
| `tools` | Tool registry, built-in/WASM/MCP tool execution, software builder |
| `tracing_fmt` | Custom tracing/logging format with terminal truncation |
| `util` | Shared utilities |
| `worker` | Sandboxed worker runtime, Claude Code bridge (`claude` CLI delegation) |
| `workspace` | Filesystem-like persistent memory, hybrid search (FTS + vector), document chunking, connections, spaces, profiles, batch embeddings, citations |

### Startup Sequence (main.rs)

CLI commands: `run` (default), `worker`, `claude-bridge`, `tool`, `config`, `memory`, `mcp`, `pairing`, `status`, `onboard`, `doctor`, `gateway`, `sessions`, `hooks`, `cron`, `logs`, `message`, `channels`, `plugins`, `webhooks`, `skills`, `agents`, `nodes`, `browser`, `completion`, `service`. Special commands exit early with minimal setup.

For `run`: load config (env > DB > defaults) → create LLM session → connect DB → build safety layer → register tools → init workspace + embeddings → load WASM tools/MCP servers → init channels → create agent → spawn background tasks (self-repair, session pruning, heartbeat, routine engine, config reload) → enter message loop.

### Job Execution Models

- **Local**: Worker runs in-process with direct tool access
- **Sandboxed**: Docker container with `ironclaw worker` command, communicates with orchestrator on `:50051` via per-job bearer tokens
- **Claude Code**: Docker container with `ironclaw claude-bridge`, spawns `claude` CLI process

Job state machine: `Pending → InProgress → Completed → Submitted → Accepted` (also `→ Failed`, `→ Stuck → InProgress` for recovery)

### Dual Database Backend

PostgreSQL (default, `postgres` feature) and libSQL/Turso (`libsql` feature). **All new persistence features must implement both backends**: add the method to the `Database` trait in `src/db/mod.rs`, implement in `src/db/postgres.rs` (delegates to Store/Repository), and `src/db/libsql_backend.rs` (native SQL).

Key type mappings for libSQL: `UUID→TEXT`, `TIMESTAMPTZ→TEXT(ISO-8601)`, `JSONB→TEXT`, `VECTOR(1536)→F32_BLOB(1536)`, `tsvector→FTS5`.

Database trait methods by category (~75 total):

| Category | Count | Examples |
|----------|-------|---------|
| Conversations | 12 | `create_conversation`, `add_conversation_message`, `list_conversations_with_preview` |
| Jobs | 5 | `save_job`, `get_job`, `update_job_status`, `mark_job_stuck` |
| Sandbox Jobs | 11 | `save_sandbox_job`, `list_sandbox_jobs`, `update_sandbox_job_status`, `update_sandbox_job_mode` |
| Routines | 10 | `create_routine`, `list_routines`, `list_due_cron_routines`, `update_routine_runtime` |
| Routine Runs | 4 | `create_routine_run`, `complete_routine_run`, `list_routine_runs` |
| Settings | 8 | `get_setting`, `set_setting`, `list_settings`, `get_all_settings` |
| Workspace Docs | 8 | `get_document_by_path`, `update_document`, `list_directory` |
| Workspace Chunks | 4 | `insert_chunk`, `update_chunk_embedding`, `get_chunks_without_embeddings` |
| Workspace Search | 1 | `hybrid_search` |
| Workspace Memory | 4+ | `create_connection`, `create_space`, `set_profile_fact` (connections, spaces, profiles) |
| Tool Failures | 4 | `record_tool_failure`, `get_broken_tools`, `mark_tool_repaired` |
| Actions/Events | 4 | `save_action`, `get_job_actions`, `save_job_event`, `list_job_events` |
| LLM/Estimation | 3 | `record_llm_call`, `save_estimation_snapshot`, `update_estimation_actuals` |
| Migrations | 1 | `run_migrations` |

### Tool System

Three tool types share the same `Tool` trait interface:

- **Built-in** (Rust): `src/tools/builtin/` — register in `ToolRegistry::register_builtin_tools()` in `registry.rs`
- **WASM** (sandboxed): loaded from `~/.ironclaw/tools/`, declare capabilities in JSON, credentials injected by host
- **MCP** (external): HTTP transport, discovered from MCP server protocol

**Critical rule**: Keep tool-specific logic (API endpoints, auth flows, service config) in the tool's `capabilities.json`, not in the main agent codebase.

Tools with `requires_approval() = true` (shell, http, file write/patch, builder) gate execution on user approval.

#### Built-in Tools (registered in phases)

| Phase | Tools | Domain |
|-------|-------|--------|
| `register_builtin_tools()` | `echo`, `time`, `json`, `http` | Orchestrator — always safe |
| `register_dev_tools()` | `shell`, `read_file`, `write_file`, `list_dir`, `apply_patch` | Container — file/shell ops |
| `register_memory_tools(workspace)` | `memory_search`, `memory_write`, `memory_read`, `memory_tree`, `memory_connect`, `memory_spaces`, `memory_profile` | Memory operations |
| `register_job_tools(...)` | `create_job`, `list_jobs`, `job_status`, `cancel_job` | Job management |
| `register_extension_tools(manager)` | `tool_search`, `tool_install`, `tool_auth`, `tool_activate`, `tool_list`, `tool_remove` | Extension lifecycle |
| `register_routine_tools(store, engine)` | `routine_create`, `routine_list`, `routine_update`, `routine_delete`, `routine_history` | Scheduled routines |
| `register_builder_tool(llm, safety, ...)` | `build_software` | LLM-driven software builder |

Additional built-in tools: `session_list`, `session_history`, `session_send` (session management), `browser` (browser automation), `marketplace`, `ecommerce`, `restaurant`, `taskrabbit` (service integrations).

#### WASM Tools (`tools-src/`)

Nine pre-built WASM tools: `gmail`, `google-calendar`, `google-docs`, `google-drive`, `google-sheets`, `google-slides`, `okta`, `slack`, `telegram`. Each is a standalone Rust crate with a `capabilities.json` manifest.

#### WASM Channels (`channels-src/`)

Three pluggable WASM channels: `slack`, `telegram`, `whatsapp`. Each implements the Channel WIT interface (`wit/channel.wit`) and is loaded at runtime.

### Safety Layer

All external tool output passes through `SafetyLayer` (sanitizer → validator → policy) before reaching the LLM. Tool outputs are XML-wrapped with sanitization markers. Additional safety systems:

- **Sanitizer**: Injection pattern detection with HTML entity decoding, invisible character stripping, homoglyph normalization
- **LeakDetector**: Scans for secret exfiltration in requests and responses; URL percent-encoding detection; SHA256/384/512 hex patterns; case-insensitive header scanning
- **LogRedactor**: Regex-based redaction of API keys, Bearer tokens, JWTs, AWS keys, emails, passwords in URLs, Basic auth, database connection strings, GitHub/Slack tokens
- **Policy**: System file access blocking, shell/SQL injection detection, path traversal variant detection (URL-encoded, double-encoded, backslash)
- **GroupPolicyManager**: Per-group tool allow/deny/require-approval policies
- **ElevatedMode**: Session-bound privileged execution with duration clamping [60s, 8h] and audit tracking
- **BinsAllowlist**: Curated POSIX utility allowlist, enforced by default, with LD_PRELOAD/DYLD environment variable validation
- **AccessControlList**: Allowlist/blocklist ACLs with glob matching
- **OAuthFlowManager**: OAuth 2.0/2.1 with PKCE S256 support; `SecretString` for all tokens/secrets; `OsRng` for all security-critical random values
- **VulnerabilityScanner**: Regex-based skill vulnerability scanning with severity levels

### Workspace & Memory

Filesystem-like persistent memory in the database (`memory_documents` + `memory_chunks` tables). Identity files (IDENTITY.md, SOUL.md, AGENTS.md, USER.md) are injected into LLM system prompts. Hybrid search uses FTS + vector via Reciprocal Rank Fusion (PostgreSQL) or FTS5 only (libSQL).

Additional memory features (supermemory-inspired):
- **Connections**: Typed relationships (updates, extends, derives) forming a knowledge graph
- **Spaces**: Named collections for organizing memories by topic/project
- **Profiles**: Auto-maintained static/dynamic fact profiles for personalization
- **Batch embeddings**: Queue-based batch processing with `BatchEmbeddingProcessor`
- **Citations**: `CitedSearchResult` types for search result attribution

### Hooks System

Lifecycle hooks with shell/HTTP/inline/webhook actions (`src/hooks/`):

- Hook types: `beforeInbound`, `beforeOutbound`, `beforeToolCall`, `onSessionStart`, `onSessionEnd`, `transformResponse`, `transcribeAudio`
- 8 bundled hooks: `profanity_filter`, `rate_limit_guard`, `sensitive_data_redactor`, and more
- Outbound webhooks with HMAC-SHA256 signatures and retry
- Gmail pub/sub handler with watch setup and deduplication

### Background Systems

Several background tasks run alongside the main agent loop:

- **Self-repair**: Periodic detection and recovery of stuck jobs and broken tools (`src/agent/self_repair.rs`)
- **Session pruning**: Cleanup of expired/idle sessions with configurable thresholds (`src/agent/session_pruning.rs`)
- **Heartbeat**: Proactive periodic execution driven by `HEARTBEAT.md` checklist (`src/agent/heartbeat.rs`)
- **Routine engine**: Cron-based and event-driven scheduled job execution (`src/agent/routine_engine.rs`)
- **Context monitor**: Token/time/cost tracking for active jobs (`src/agent/context_monitor.rs`)
- **Config reload**: File system watching with broadcast notifications (`src/agent/config_reload.rs`)

## Repository Structure

```
ironclaw/
├── src/                    # Main Rust source (28 modules)
├── tools-src/              # WASM tool crates (9 tools)
│   ├── gmail/
│   ├── google-calendar/
│   ├── google-docs/
│   ├── google-drive/
│   ├── google-sheets/
│   ├── google-slides/
│   ├── okta/
│   ├── slack/
│   └── telegram/
├── channels-src/           # WASM channel crates (3 channels)
│   ├── slack/
│   ├── telegram/
│   └── whatsapp/
├── wit/                    # WASM Interface Definitions
│   ├── tool.wit            # Tool component interface
│   └── channel.wit         # Channel component interface
├── migrations/             # PostgreSQL schema (V1–V9)
├── docs/                   # Additional documentation
│   ├── BUILDING_CHANNELS.md
│   └── TELEGRAM_SETUP.md
├── deploy/                 # Deployment configs (systemd, setup scripts, GCP, Windows installer)
│   ├── cloud-sql-proxy.service
│   ├── ironclaw.service
│   ├── setup.sh
│   ├── env.example
│   └── windows/            # Windows installer files
│       ├── ironclaw-installer.ps1  # PowerShell installer script
│       └── ironclaw.wxs           # WiX MSI source
├── docker/                 # Container images (sandbox.Dockerfile)
├── examples/               # Example code (test_heartbeat.rs)
├── .claude/                # Claude Code custom commands
│   └── commands/
│       ├── add-tool.md
│       ├── add-sse-event.md
│       ├── trace.md
│       └── ship.md
├── .github/workflows/      # CI (test, code_style, release, release-plz, windows-installer)
├── build.rs                # Build script (compiles Telegram channel WASM)
├── Cargo.toml              # Rust 2024 edition, MSRV 1.92
├── Dockerfile              # Main service container
├── Dockerfile.worker       # Worker process container
├── docker-compose.yml      # Local dev setup
├── FEATURE_PARITY.md       # IronClaw ↔ OpenClaw tracking matrix
├── AGENTS.md               # Agent rules and policies
└── CONTRIBUTING.md          # Contributing guidelines
```

## Error Handling Hierarchy

All errors use `thiserror` with a top-level `Error` enum in `src/error.rs` that wraps domain-specific error types:

| Error Type | Domain |
|------------|--------|
| `ConfigError` | Missing env vars, invalid values, parse failures |
| `DatabaseError` | Pool, query, not-found, constraint, migration errors (feature-gated for postgres/libsql) |
| `ChannelError` | Startup, disconnect, send, auth, rate-limit failures |
| `LlmError` | Request, rate-limit, context-length, auth, session errors |
| `ToolError` | Not-found, execution, timeout, sandbox, auth-required errors |
| `SafetyError` | Injection detection, output size, blocked content, policy violations |
| `JobError` | Not-found, invalid transition, stuck, max-jobs-exceeded |
| `EstimationError` | Insufficient data, calculation failures |
| `EvaluationError` | Failed evaluation, missing data |
| `RepairError` | Repair failure, max attempts exceeded, diagnosis failure |
| `WorkspaceError` | Document not-found, search, embedding, chunking errors |
| `OrchestratorError` | Container creation, auth, Docker, timeout errors |
| `WorkerError` | Connection, LLM proxy, secret resolution, missing token |
| `HookError` | Execution failure, timeout, registration errors |
| `MediaError` | Unsupported type, processing failure, size limits, transcription, vision, recursive processing depth/iteration limits |
| `SkillsError` | Not-found, execution failure, invalid definition |

## Code Conventions

- **Error handling**: `thiserror` types in `error.rs`. No `.unwrap()` in production code. Map errors with context: `.map_err(|e| SomeError::Variant { reason: e.to_string() })?`
- **Async**: All I/O is async with tokio. `Arc<T>` for shared state, `RwLock` for concurrent read/write.
- **Imports**: Use `crate::` paths, not `super::`. No `pub use` re-exports unless exposing to downstream consumers.
- **Types**: Prefer strong types (enums, newtypes) over strings.
- **Testing**: Tests live in `mod tests {}` blocks at the bottom of each file. Async tests use `#[tokio::test]`. No mocks — prefer real implementations or stubs. Feature-gated tests use `#[cfg(all(test, feature = "postgres"))]` or `#[cfg(test)]` for feature-independent tests. Environment variable tests in Rust 2024 require `unsafe` blocks for `std::env::set_var`/`remove_var`.
- **Architecture**: Prefer generic/extensible designs over hardcoded integrations. Ask clarifying questions about abstraction level before implementing.
- **Rust edition**: 2024 with MSRV 1.92.

## Adding New Components

### New Built-in Tool

1. Create `src/tools/builtin/my_tool.rs` implementing the `Tool` trait
2. Add `mod my_tool;` and `pub use` in `src/tools/builtin/mod.rs`
3. Register in the appropriate phase in `ToolRegistry` (`registry.rs`) — choose the phase matching the tool's domain (orchestrator, container, memory, job, extension, routine, or builder)

### New WASM Tool

1. Create crate in `tools-src/<name>/`
2. Implement WIT interface (`wit/tool.wit`)
3. Create `<name>.capabilities.json` for permissions, auth, rate limits
4. Build with `cargo build --target wasm32-wasip2 --release`

### New Channel

1. Create `src/channels/my_channel.rs` implementing the `Channel` trait
2. Add config fields in `src/config.rs`
3. Wire up in `main.rs` channel setup section

### New WASM Channel

1. Create crate in `channels-src/<name>/`
2. Implement WIT interface (`wit/channel.wit`)
3. Create `<name>.capabilities.json` for permissions
4. Build with `cargo build --target wasm32-wasip2 --release`

### New Database Method

1. Add method to `Database` trait in `src/db/mod.rs`
2. Implement in `src/db/postgres.rs` (delegate to Store/Repository)
3. Implement in `src/db/libsql_backend.rs` (native SQL)
4. Add migration if schema changes needed: `migrations/V<N>__description.sql` (PostgreSQL) and update `src/db/libsql_migrations.rs` (libSQL)

### New SSE Event (Web Gateway)

1. Define event type in `src/channels/web/types.rs`
2. Add SSE serialization in `src/channels/web/sse.rs`
3. Emit from the appropriate agent/worker code
4. Handle in the web frontend (`src/channels/web/static/app.js`)

### New Hook

1. Add hook type to `HookType` enum in `src/hooks/types.rs`
2. Add execution method in `src/hooks/engine.rs`
3. For bundled hooks, add to `src/hooks/bundled.rs`
4. Wire invocation from the appropriate agent/channel code

### New LLM Provider

1. Create `src/llm/my_provider.rs` implementing the `LlmProvider` trait
2. Add variant to `LlmBackend` enum in `src/config.rs`
3. Add env vars for configuration (API key, model, base URL)
4. Wire up in `src/llm/mod.rs` provider construction
5. Optionally add to `src/llm/auto_discovery.rs` for model listing
6. Add to `src/llm/failover.rs` for multi-provider failover support

## Configuration

Config loads with priority: environment variables > database settings > defaults. Bootstrap config persists to `~/.ironclaw/bootstrap.json`. See `.env.example` for all environment variables. Key ones:

- `DATABASE_URL` / `DATABASE_BACKEND` — Connection string and backend (`postgres` default, or `libsql`)
- `DATABASE_POOL_SIZE` — Connection pool size (default 10)
- `LLM_BACKEND` — LLM provider: `nearai` (default), `openai`, `anthropic`, `ollama`, `openai_compatible`, `gemini`, `bedrock`, `openrouter`
- `NEARAI_SESSION_TOKEN` / `NEARAI_MODEL` / `NEARAI_BASE_URL` / `NEARAI_AUTH_URL` — NEAR AI provider (when `LLM_BACKEND=nearai`)
- `OPENAI_API_KEY` / `OPENAI_MODEL` — OpenAI provider (when `LLM_BACKEND=openai`)
- `ANTHROPIC_API_KEY` / `ANTHROPIC_MODEL` — Anthropic provider (when `LLM_BACKEND=anthropic`)
- `OLLAMA_BASE_URL` / `OLLAMA_MODEL` — Ollama provider (when `LLM_BACKEND=ollama`)
- `LLM_BASE_URL` / `LLM_API_KEY` / `LLM_MODEL` — OpenAI-compatible provider (when `LLM_BACKEND=openai_compatible`)
- `GEMINI_API_KEY` / `GEMINI_MODEL` — Google Gemini provider (when `LLM_BACKEND=gemini`)
- `AWS_REGION` / `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` / `BEDROCK_MODEL` — AWS Bedrock provider (when `LLM_BACKEND=bedrock`)
- `OPENROUTER_API_KEY` / `OPENROUTER_MODEL` / `OPENROUTER_BASE_URL` / `OPENROUTER_REFERER` — OpenRouter provider (when `LLM_BACKEND=openrouter`)
- `GATEWAY_ENABLED` / `GATEWAY_PORT` / `GATEWAY_AUTH_TOKEN` — Web UI gateway
- `SANDBOX_ENABLED` — Docker container isolation
- `CLAUDE_CODE_ENABLED` — Claude CLI delegation mode
- `HEARTBEAT_ENABLED` / `HEARTBEAT_INTERVAL_SECS` / `HEARTBEAT_NOTIFY_CHANNEL` — Proactive periodic execution
- `SELF_REPAIR_CHECK_INTERVAL_SECS` / `SELF_REPAIR_MAX_ATTEMPTS` — Self-repair loop
- `AGENT_MAX_PARALLEL_JOBS` / `AGENT_JOB_TIMEOUT_SECS` / `AGENT_STUCK_THRESHOLD_SECS` — Job execution limits
- `AGENT_USE_PLANNING` — Enable planning phase before tool execution (default true)
- `SAFETY_MAX_OUTPUT_LENGTH` / `SAFETY_INJECTION_CHECK_ENABLED` — Safety layer settings
- `RUST_LOG=ironclaw=debug` or `ironclaw::agent=debug` — Targeted logging

## CI / Release

- **test.yml** — Runs `cargo test` on PR and push (excludes PostgreSQL-dependent integration tests)
- **code_style.yml** — Runs `cargo fmt --check` and `cargo clippy` on PR and push
- **release.yml** — Builds release binaries for all platforms (macOS, Linux, Windows) via `cargo-dist`
- **release-plz.yml** — Automated version bumping and changelog generation
- **windows-installer.yml** — Uploads PowerShell installer script (`ironclaw-installer.ps1`) to GitHub Releases on publish

Target platforms: `aarch64-apple-darwin`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`.

### Test Coverage

~1,840 unit tests across ~190 tested files (out of 247 total `.rs` files), plus 53 integration tests. Key coverage areas:

| Module | Coverage | Notes |
|--------|----------|-------|
| `safety` | 100% files | Leak detection, redaction, policies |
| `agent` | 95% files | Command queue, session mgmt, routing |
| `channels` | 78% files | WASM routing, web gateway, inline commands |
| `tools` | 71% files | Browser, MCP client, WASM hosting, memory tools identity protection |
| `config` | Tested | Enum parsing, validation, env var helpers |
| `db` | Partial | libSQL type conversion helpers, pure functions |
| `context` | Tested | Full state machine matrix, transition validation |
| `estimation` | Tested | EMA correctness, zero-estimate guards, confidence |
| `workspace` | Tested | RRF algorithm, search config, normalization |

See `TEST_COVERAGE_ANALYSIS.md` for the full analysis and remaining gaps.

### Windows Installer

The `deploy/windows/` directory contains Windows-specific installation tooling:

- **`ironclaw-installer.ps1`** — PowerShell installer script supporting:
  - One-liner install: `irm https://github.com/danielsimonjr/ironclaw/releases/latest/download/ironclaw-installer.ps1 | iex`
  - Architecture detection (x86_64, ARM64 via emulation)
  - Latest version auto-detection via GitHub API
  - Dual install modes: archive-based (tar.gz) or MSI (`-UseMsi`)
  - Custom install directory (`-InstallDir`), version pinning (`-Version`), PATH opt-out (`-NoPathUpdate`)
  - CARGO_HOME/bin detection with LocalAppData fallback
  - Upgrade handling for existing installations
- **`ironclaw.wxs`** — WiX v3 source for MSI builds via `cargo-wix`/`cargo-dist`, per-user install scope with PATH integration. GUIDs must match `[package.metadata.wix]` in `Cargo.toml`.
