# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo fmt                                                    # Format code
cargo clippy --all --benches --tests --examples --all-features  # Lint (fix warnings before committing)
cargo test                                                   # Run all tests
cargo test test_name                                         # Run a specific test
cargo test safety::sanitizer::tests                          # Run a module's tests
RUST_LOG=ironclaw=debug cargo run                            # Run with debug logging
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
| `Database` | `src/db/mod.rs` | ~60 async methods for all persistence. **Both backends must be updated for new features.** |
| `Channel` | `src/channels/channel.rs` | Input sources (REPL, HTTP, WASM, web gateway) |
| `Tool` | `src/tools/tool.rs` | Executable capabilities (built-in, WASM, MCP) |
| `LlmProvider` | `src/llm/provider.rs` | LLM backends (currently NEAR AI only) |
| `EmbeddingProvider` | `src/workspace/embeddings.rs` | Vector embedding backends |
| `SuccessEvaluator` | `src/evaluation/success.rs` | Job outcome evaluation |

### Startup Sequence (main.rs)

CLI commands: `run` (default), `worker`, `claude-bridge`, `tool`, `config`, `memory`, `mcp`, `pairing`, `status`, `onboard`. Special commands exit early with minimal setup.

For `run`: load config (env > DB > defaults) → create LLM session → connect DB → build safety layer → register tools → init workspace + embeddings → load WASM tools/MCP servers → init channels → create agent → spawn background tasks (self-repair, session pruning, heartbeat, routine engine) → enter message loop.

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

### Safety Layer

All external tool output passes through `SafetyLayer` (sanitizer → validator → policy) before reaching the LLM. Tool outputs are XML-wrapped with sanitization markers. `LeakDetector` scans for secret exfiltration.

### Workspace & Memory

Filesystem-like persistent memory in the database (`memory_documents` + `memory_chunks` tables). Identity files (IDENTITY.md, SOUL.md, AGENTS.md, USER.md) are injected into LLM system prompts. Hybrid search uses FTS + vector via Reciprocal Rank Fusion (PostgreSQL) or FTS5 only (libSQL).

## Code Conventions

- **Error handling**: `thiserror` types in `error.rs`. No `.unwrap()` in production code. Map errors with context: `.map_err(|e| SomeError::Variant { reason: e.to_string() })?`
- **Async**: All I/O is async with tokio. `Arc<T>` for shared state, `RwLock` for concurrent read/write.
- **Imports**: Use `crate::` paths, not `super::`. No `pub use` re-exports unless exposing to downstream consumers.
- **Types**: Prefer strong types (enums, newtypes) over strings.
- **Testing**: Tests live in `mod tests {}` blocks at the bottom of each file. Async tests use `#[tokio::test]`. No mocks — prefer real implementations or stubs.
- **Architecture**: Prefer generic/extensible designs over hardcoded integrations. Ask clarifying questions about abstraction level before implementing.

## Adding New Components

### New Built-in Tool

1. Create `src/tools/builtin/my_tool.rs` implementing the `Tool` trait
2. Add `mod my_tool;` and `pub use` in `src/tools/builtin/mod.rs`
3. Register in `ToolRegistry::register_builtin_tools()` in `registry.rs`

### New WASM Tool

1. Create crate in `tools-src/<name>/`
2. Implement WIT interface (`wit/tool.wit`)
3. Create `<name>.capabilities.json` for permissions, auth, rate limits
4. Build with `cargo build --target wasm32-wasip2 --release`

### New Channel

1. Create `src/channels/my_channel.rs` implementing the `Channel` trait
2. Add config fields in `src/config.rs`
3. Wire up in `main.rs` channel setup section

### New Database Method

1. Add method to `Database` trait in `src/db/mod.rs`
2. Implement in `src/db/postgres.rs` (delegate to Store/Repository)
3. Implement in `src/db/libsql_backend.rs` (native SQL)

## Configuration

Config loads with priority: environment variables > database settings > defaults. Bootstrap config persists to `~/.ironclaw/bootstrap.json`. See `.env.example` for all environment variables. Key ones:

- `DATABASE_BACKEND` — `postgres` (default) or `libsql`
- `NEARAI_SESSION_TOKEN` / `NEARAI_MODEL` / `NEARAI_BASE_URL` — LLM provider (required)
- `GATEWAY_ENABLED` / `GATEWAY_PORT` / `GATEWAY_AUTH_TOKEN` — web UI
- `SANDBOX_ENABLED` — Docker container isolation
- `CLAUDE_CODE_ENABLED` — Claude CLI delegation mode
- `RUST_LOG=ironclaw=debug` or `ironclaw::agent=debug` — targeted logging
