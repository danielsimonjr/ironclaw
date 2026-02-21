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

Use `cargo run -- --no-db` to bypass the `DATABASE_URL` requirement on startup (useful for CLI-only commands).

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

The codebase has 28 public modules (`src/lib.rs`), grouped by domain:

- **Core**: `agent` (loop, routing, scheduling, session mgmt, self-repair, heartbeat), `config`, `context`, `error`, `worker`, `orchestrator`
- **I/O**: `channels` (REPL, HTTP, WASM, web gateway), `llm` (7 providers, failover, cost tracking), `media` (image, PDF, audio, video, TTS)
- **Persistence**: `db` (dual PostgreSQL/libSQL), `workspace` (memory, hybrid search, embeddings), `history`, `settings`, `secrets`
- **Safety**: `safety` (sanitizer → validator → policy, leak detection, ACLs, OAuth), `sandbox` (Docker, network proxy)
- **Extensions**: `tools` (registry, built-in/WASM/MCP), `extensions` (discovery, install, ClawHub), `hooks` (lifecycle events, webhooks), `skills`
- **Support**: `cli`, `bootstrap`, `setup`, `pairing`, `estimation`, `evaluation`, `hot_reload`, `tracing_fmt`, `util`

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

### Tool System

Three tool types share the same `Tool` trait interface:

- **Built-in** (Rust): `src/tools/builtin/` — register in `ToolRegistry::register_builtin_tools()` in `registry.rs`
- **WASM** (sandboxed): loaded from `~/.ironclaw/tools/`, declare capabilities in JSON, credentials injected by host
- **MCP** (external): HTTP transport, discovered from MCP server protocol

**Critical rule**: Keep tool-specific logic (API endpoints, auth flows, service config) in the tool's `capabilities.json`, not in the main agent codebase.

Tools with `requires_approval() = true` (shell, http, file write/patch, builder) gate execution on user approval.

Built-in tools are registered in phases in `ToolRegistry` (`registry.rs`) — see `register_builtin_tools()`, `register_dev_tools()`, `register_memory_tools()`, etc.

#### WASM Tools (`tools-src/`)

Nine pre-built WASM tools: `gmail`, `google-calendar`, `google-docs`, `google-drive`, `google-sheets`, `google-slides`, `okta`, `slack`, `telegram`. Each is a standalone Rust crate with a `capabilities.json` manifest.

#### WASM Channels (`channels-src/`)

Three pluggable WASM channels: `slack`, `telegram`, `whatsapp`. Each implements the Channel WIT interface (`wit/channel.wit`) and is loaded at runtime.

### Safety Layer

All external tool output passes through `SafetyLayer` (sanitizer → validator → policy) before reaching the LLM. Tool outputs are XML-wrapped with sanitization markers. Key subsystems: `LeakDetector` (secret exfiltration scanning), `LogRedactor` (credential redaction), `OAuthFlowManager` (OAuth 2.0/2.1 + PKCE), `GroupPolicyManager` (per-group ACLs), `ElevatedMode` (session-bound privilege escalation). See `src/safety/` for full details.

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

Config loads with priority: environment variables > database settings > defaults. Bootstrap config persists to `~/.ironclaw/bootstrap.json`. See `deploy/env.example` for all environment variables. Key: `DATABASE_URL`, `LLM_BACKEND`, `RUST_LOG=ironclaw=debug`.

## CI / Release

- **test.yml** — Runs `cargo test` on PR and push (excludes PostgreSQL-dependent integration tests)
- **code_style.yml** — Runs `cargo fmt --check` and `cargo clippy` on PR and push
- **release.yml** — Builds release binaries for all platforms (macOS, Linux, Windows) via `cargo-dist`
- **release-plz.yml** — Automated version bumping and changelog generation
- **windows-installer.yml** — Uploads PowerShell installer script (`ironclaw-installer.ps1`) to GitHub Releases on publish

Target platforms: `aarch64-apple-darwin`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`.

