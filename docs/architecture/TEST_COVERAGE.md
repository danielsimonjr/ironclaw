# Test Coverage Analysis

**Generated**: 2026-02-23

## Summary

| Metric | Count |
|--------|-------|
| Total Tests | 2,147 |
| Source Files with Tests | 208 |
| Test Modules | 23 |
| Pre-existing Failures | 11 (Windows-specific) |

All tests are inline unit tests using Rust's `#[cfg(test)] mod tests {}` convention at the bottom of each source file. Async tests use `#[tokio::test]`. There are no separate test files or directories.

---

## Test Organization

### Conventions

- **Inline tests**: All tests live in `mod tests {}` blocks at the bottom of the source file they test
- **Async**: I/O-bound tests use `#[tokio::test]`
- **No mocks**: Prefer real implementations or lightweight stubs
- **Feature gates**: PostgreSQL-dependent tests use `#[cfg(all(test, feature = "postgres"))]`; feature-independent tests use `#[cfg(test)]`
- **Environment variables**: Rust 2024 edition requires `unsafe` blocks for `std::env::set_var` / `remove_var` in tests
- **Strong types**: Tests validate enum variants and newtypes rather than raw strings

### Test Pyramid

```
              /\
             /  \
            / DB  \  (Database: 20 tests, requires PostgreSQL/libSQL)
           /______\
          /        \
         / Integration\ (Agent, Worker, Orchestrator: 276 tests)
        /______________\
       /                \
      /    Unit Tests    \ (Channels, Tools, Safety, LLM, etc.: 1,851 tests)
     /____________________\
```

---

## Test Categories by Module

| Module | Tests | Files | Description |
|--------|------:|------:|-------------|
| channels | 477 | 35 | REPL, HTTP, WASM, web gateway, SSE, PID lock, webhooks |
| tools | 348 | 40 | Built-in tools, WASM tools, MCP, tool registry |
| agent | 240 | 20 | Agent loop, routing, scheduling, session mgmt, self-repair, heartbeat |
| media | 150 | 11 | Image, PDF, audio, video, TTS processing |
| safety | 129 | 11 | Sanitizer, validator, policy, leak detection, ACLs, OAuth |
| llm | 101 | 13 | Provider trait, failover, cost tracking, 7 backends |
| (root) | 108 | 7 | Top-level modules (config, error, lib, util) |
| cli | 89 | 10 | CLI commands, completion, output formatting |
| hooks | 63 | 6 | Lifecycle hooks, bundled hooks, webhooks |
| skills | 61 | 2 | Skill definition and execution |
| workspace | 60 | 8 | Memory, hybrid search, embeddings |
| extensions | 89 | 7 | Discovery, install, ClawHub, types |
| sandbox | 48 | 9 | Docker sandbox, network proxy, error types |
| context | 35 | 3 | Token/time/cost tracking |
| estimation | 32 | 5 | Job estimation |
| pairing | 27 | 2 | Device pairing |
| worker | 23 | 4 | Worker execution loop |
| secrets | 21 | 4 | Secret storage and retrieval |
| db | 20 | 1 | Database trait and migrations |
| orchestrator | 13 | 3 | Job orchestration |
| setup | 10 | 3 | First-run setup |
| history | 30 | 2 | Analytics structs, store data types |
| evaluation | 5 | 2 | Success evaluation |
| **Total** | **2,147** | **208** | |

---

## Source-to-Test Mapping

### channels (472 tests, 34 files)

The largest test module. Covers all input channel implementations and the web gateway.

| Source File | Has Tests |
|-------------|-----------|
| `src/channels/channel.rs` | Yes |
| `src/channels/repl.rs` | Yes |
| `src/channels/http.rs` | Yes |
| `src/channels/wasm_channel.rs` | Yes |
| `src/channels/webhook_server.rs` | Yes |
| `src/channels/web/*.rs` | Yes (multiple files) |

### tools (348 tests, 40 files)

Covers the tool trait, registry, and built-in tool implementations.

| Source File | Has Tests |
|-------------|-----------|
| `src/tools/tool.rs` | Yes |
| `src/tools/registry.rs` | Yes |
| `src/tools/builtin/echo.rs` | Yes |
| `src/tools/builtin/time.rs` | Yes |
| `src/tools/builtin/restaurant.rs` | Yes |
| `src/tools/builtin/marketplace.rs` | Yes |
| `src/tools/builtin/taskrabbit.rs` | Yes |
| `src/tools/builtin/ecommerce.rs` | Yes |
| `src/tools/builtin/*.rs` | Yes (most files) |
| `src/tools/builtin/routine.rs` | **No** (654 lines, requires Database/RoutineEngine) |

### agent (240 tests, 20 files)

Covers the core agent loop, routing, scheduling, and background tasks.

| Source File | Has Tests |
|-------------|-----------|
| `src/agent/*.rs` | Yes (20 files) |

### media (150 tests, 11 files)

Covers media processing: image, PDF, audio, video, and TTS.

| Source File | Has Tests |
|-------------|-----------|
| `src/media/*.rs` | Yes (11 files) |

### safety (129 tests, 11 files)

Covers the full safety pipeline: sanitizer, validator, policy, leak detection, OAuth.

| Source File | Has Tests |
|-------------|-----------|
| `src/safety/sanitizer.rs` | Yes |
| `src/safety/validator.rs` | Yes |
| `src/safety/policy.rs` | Yes |
| `src/safety/leak_detector.rs` | Yes |
| `src/safety/oauth.rs` | Yes |

### llm (101 tests, 13 files)

Covers the LLM provider trait, failover, and individual backends.

| Source File | Has Tests |
|-------------|-----------|
| `src/llm/provider.rs` | Yes |
| `src/llm/failover.rs` | Yes |
| `src/llm/mod.rs` | Yes |

### workspace (60 tests, 8 files)

Covers memory, hybrid search, and embeddings.

| Source File | Has Tests |
|-------------|-----------|
| `src/workspace/*.rs` | Yes (8 files) |
| `src/workspace/repository.rs` | **No** (910 lines) |

### db (20 tests, 1 file)

Covers the `Database` trait. Both backend implementations lack direct tests.

| Source File | Has Tests |
|-------------|-----------|
| `src/db/mod.rs` | Yes |
| `src/db/postgres.rs` | **No** (724 lines) |
| `src/db/libsql_migrations.rs` | **No** (625 lines) |

### hooks (63 tests, 6 files)

| Source File | Has Tests |
|-------------|-----------|
| `src/hooks/*.rs` | Yes (6 files) |

### cli (89 tests, 10 files)

| Source File | Has Tests |
|-------------|-----------|
| `src/cli/mod.rs` | Yes |
| `src/cli/completion.rs` | Yes |
| `src/cli/*.rs` | Yes (8 other files) |
| `src/cli/hooks.rs` | **No** (234 lines) |
| `src/cli/cron.rs` | **No** (231 lines) |
| `src/cli/gateway.rs` | **No** (212 lines) |
| `src/cli/status.rs` | **No** (200 lines) |
| `src/cli/logs.rs` | **No** (193 lines) |

### Other Modules

| Module | Tests | Files | Notes |
|--------|------:|------:|-------|
| skills | 61 | 2 | Skill definition and execution |
| extensions | 89 | 7 | Discovery, install, ClawHub, types |
| sandbox | 48 | 9 | Docker sandbox, network proxy, error types |
| context | 35 | 3 | Token/time/cost tracking |
| estimation | 32 | 5 | Job estimation |
| pairing | 27 | 2 | Device pairing (7 Windows failures) |
| worker | 23 | 4 | Worker execution loop |
| secrets | 21 | 4 | Secret storage |
| orchestrator | 13 | 3 | Job orchestration |
| setup | 10 | 3 | First-run setup |
| history | 30 | 2 | Analytics structs, store data types |
| evaluation | 5 | 2 | Success evaluation |

---

## Coverage Gaps

Files without tests, sorted by priority (size and importance):

### Critical (Core Infrastructure)

| File | Lines | Why It Matters |
|------|------:|----------------|
| `src/main.rs` | 1,421 | Application entry point and startup sequence |
| `src/workspace/repository.rs` | 910 | Workspace persistence layer |
| `src/db/postgres.rs` | 724 | PostgreSQL backend implementation |
| `src/db/libsql_migrations.rs` | 625 | libSQL migration definitions |

### High (Feature Modules)

| File | Lines | Why It Matters |
|------|------:|----------------|
| `src/tools/builtin/routine.rs` | 654 | Routine/cron tool (requires Database/RoutineEngine) |
| `src/cli/hooks.rs` | 234 | CLI hooks subcommand |
| `src/cli/cron.rs` | 231 | CLI cron subcommand |
| `src/cli/gateway.rs` | 212 | CLI gateway subcommand |
| `src/cli/status.rs` | 200 | CLI status subcommand |
| `src/cli/logs.rs` | 193 | CLI logs subcommand |

### Medium (CLI Subcommands)

| File | Lines | Why It Matters |
|------|------:|----------------|
| `src/cli/sessions.rs` | 151 | CLI sessions subcommand |
| `src/cli/message.rs` | 97 | CLI message subcommand |
| `src/cli/skills.rs` | 93 | CLI skills subcommand |

---

## Known Test Issues

### Pre-existing Failures (11 tests, all Windows-specific)

These tests fail on Windows due to platform-specific behavior and pass on Linux/macOS:

| Module | Failures | Root Cause |
|--------|:--------:|------------|
| `pairing::store::tests` | 7 | Windows file locking prevents concurrent file access |
| `channels::web::pid_lock::tests` | 2 | Windows process detection differs from Unix |
| `cli::pairing::tests` | 2 | Depends on pairing store (same file locking issue) |

These failures do not indicate code defects. They reflect platform differences in file locking semantics.

### Resolved Issues

- **Clap short flag conflicts** (fixed 2026-02-24): Three duplicate `-c`, `-l`, `-t` short flags across CLI subcommands caused `clap_complete::generate()` to panic with debug assertions. Fixed by assigning explicit short flags: `message --channel` → `-C`, `sessions --channel` → `-C`, `logs --level` → `-L`, `message --to` → `-T`.

---

## Running Tests

```bash
# Run all tests
cargo test

# Run all tests with output
cargo test -- --nocapture

# Run a specific test by name
cargo test test_name

# Run all tests in a module
cargo test safety::sanitizer::tests

# Run only library tests (skip integration tests)
cargo test --lib

# Run with debug logging
RUST_LOG=ironclaw=debug cargo test

# Run with PostgreSQL feature only
cargo test --features postgres

# Run with libSQL feature only
cargo test --no-default-features --features libsql

# Run with both database backends
cargo test --features "postgres,libsql"

# Format and lint before testing
cargo fmt && cargo clippy --all --benches --tests --examples --all-features
```

---

## Test Conventions Reference

From `CLAUDE.md`:

1. **Location**: Tests live in `mod tests {}` blocks at the bottom of each source file
2. **Async**: Use `#[tokio::test]` for async tests
3. **No mocks**: Prefer real implementations or stubs
4. **Feature gates**: Use `#[cfg(all(test, feature = "postgres"))]` for PostgreSQL-only tests; `#[cfg(test)]` for feature-independent tests
5. **Environment variables**: Rust 2024 edition requires `unsafe` blocks for `std::env::set_var` and `remove_var` in tests
6. **Error handling**: No `.unwrap()` in production code; tests may use `.unwrap()` for brevity
7. **Imports**: Use `crate::` paths, not `super::` (except within test modules where `super::*` is conventional)

---

**Document Version**: 1.2
**Last Updated**: 2026-02-24
**Maintained By**: Daniel Simon Jr.
