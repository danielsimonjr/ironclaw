# Test Coverage Analysis

**Date**: 2026-02-17
**Scope**: All source files under `src/` and `tests/`

## Executive Summary

IronClaw has **1,056 unit test functions** across **180 tested files** (out of 247 total `.rs` files in `src/`), plus **53 integration tests** in 5 files under `tests/`. The safety, agent, and channel modules are well-tested. However, the **database layer** (5,757+ LOC, 90 async methods) has **zero tests**, representing the single largest coverage gap. Configuration parsing (1,515 LOC), history/analytics, and several built-in tools also lack tests.

| Metric | Value |
|--------|-------|
| Total `.rs` files in `src/` | 247 |
| Files with tests | 180 (73%) |
| Files without tests | 67 (27%) |
| Total unit test functions | ~1,056 |
| Total integration test functions | 53 |
| Total assertions | ~3,855 |

## Current Coverage by Module

### Well-Tested (A-tier)

| Module | Files Tested | Test Count | Notes |
|--------|-------------|------------|-------|
| `safety` | 11/11 (100%) | 109 | Best coverage in the project — leak detection, redaction, policies all tested |
| `agent` | 20/21 (95%) | 240 | Command queue (44 tests), session mgmt, routing, scheduling |
| `channels` | 29/37 (78%) | 423 | WASM routing, web gateway, inline commands (73 tests), block streaming |
| `tools` | 32/45 (71%) | 241 | Browser (39), MCP client, WASM hosting, extensions |
| `media` | 9/12 (75%) | 137 | Large doc processing (32), Edge TTS (30), video metadata (20) |
| `skills` | 2/3 (67%) | 61 | Vulnerability scanner alone has 56 tests |

### Moderately Tested (B-tier)

| Module | Files Tested | Test Count | Notes |
|--------|-------------|------------|-------|
| `llm` | 12/14 (86%) | 91 | Missing `provider.rs` trait and `mod.rs` |
| `hooks` | 6/7 (86%) | 63 | Missing `mod.rs` |
| `extensions` | 6/7 (86%) | 59 | Missing `mod.rs` |
| `workspace` | 8/9 (89%) | 48 | Missing `repository.rs` |
| `sandbox` | 7/9 (78%) | 25 | Sparse per-file coverage |
| `pairing` | 2/3 (67%) | 27 | Missing `mod.rs` |

### Undertested (C-tier)

| Module | Files Tested | Test Count | Notes |
|--------|-------------|------------|-------|
| `cli` | 7/23 (30%) | 63 | 16 command files untested |
| `evaluation` | 2/3 (67%) | 5 | Minimal assertions |
| `estimation` | 4/5 (80%) | 10 | Learner module untested |
| `context` | 3/4 (75%) | 16 | State machine, manager untested |

### Not Tested (D-tier)

| Module | Files | LOC | Notes |
|--------|-------|-----|-------|
| `db` | 0/4 | ~4,700 | **Critical**: trait (619), postgres (724), libsql (3,080), migrations |
| `history` | 0/3 | ~2,000 | store.rs (1,767), analytics.rs (226), mod.rs |
| `config` | 0/1 | 1,515 | 18 enum variants, 9 sub-configs, cascade logic |

---

## Priority 1: Database Layer (0% coverage, ~5,757 LOC)

**Risk**: The database layer is the persistence backbone of the entire system. 90 async methods across PostgreSQL and libSQL backends have zero tests. Bugs in type conversions, query logic, or feature parity between backends go completely undetected.

### What to test

**1a. libSQL type conversion helpers** (`src/db/libsql_backend.rs`)
26+ helper functions convert between SQL types and Rust types. These are pure functions and can be unit-tested trivially:
- `parse_timestamp()` — ISO-8601 string → `DateTime<Utc>`
- `get_text()`, `get_opt_text()` — row column extraction
- `row_to_memory_document()` — full row deserialization
- `row_to_routine_libsql()` — routine construction from row data

**1b. Hybrid search / Reciprocal Rank Fusion** (`libsql_backend.rs:2417`)
The RRF algorithm merges FTS and vector search results. This is a non-trivial ranking algorithm where bugs silently return wrong results. Test with known document sets and verify ranking order.

**1c. Integration tests using testcontainers**
`testcontainers-modules` with PostgreSQL support is already in `Cargo.toml` dev-dependencies but is **never used**. Stand up a containerized PostgreSQL in tests to validate:
- CRUD round-trips for all major entity types (conversations, jobs, routines, settings)
- Migration execution (V1–V9) on a fresh database
- Constraint enforcement (unique keys, foreign keys)
- Concurrent access patterns (settings upsert, job state transitions)

**1d. Feature parity tests**
Create a shared test suite that runs against both backends to catch divergences in:
- Timestamp precision and timezone handling
- NULL vs empty string semantics
- JSON serialization/deserialization round-trips
- Vector search scoring consistency

### Estimated test count needed: 60–80 tests

---

## Priority 2: Configuration System (0% coverage, 1,515 LOC)

**Risk**: Configuration drives the entire runtime. Incorrect defaults, broken parsing, or cascade priority bugs can cause silent misconfigurations.

### What to test

**2a. Enum parsing** (`FromStr` implementations)
18 enum variants across `LlmBackend`, `DatabaseBackend`, `NearAiApiMode`, etc. Each needs valid input, invalid input, and case-sensitivity tests:
```
LlmBackend::from_str("openai") → Ok(OpenAi)
LlmBackend::from_str("OPENAI") → Ok(OpenAi)  // or error?
LlmBackend::from_str("invalid") → Err(...)
```

**2b. Sub-config resolution**
9 sub-configs each have a `resolve()` method that reads environment variables and applies defaults:
- `LlmConfig::resolve()` — Correct backend loaded based on `LLM_BACKEND`; inactive backends are `None`
- `DatabaseConfig::resolve()` — Pool size parsing, libSQL path defaults
- `ChannelsConfig::resolve()` — HTTP only when port/host set
- `SafetyConfig::resolve()` — `max_output_length` with default
- `WasmConfig::resolve()` — Memory/timeout/fuel limits
- `SecretsConfig::resolve()` — Env var > keychain > None fallback; min key length 32

**2c. Validation logic**
- `TUNNEL_URL` must start with `https://`
- `LIBSQL_AUTH_TOKEN` required when `LIBSQL_URL` is set
- `DATABASE_POOL_SIZE` must be a positive integer
- `SECRETS_MASTER_KEY` must be ≥ 32 bytes

**2d. Helper functions**
- `optional_env()` — empty string treated as `None`
- `parse_optional_env::<T>()` — type parsing with error mapping

### Estimated test count needed: 40–50 tests

---

## Priority 3: Context State Machine (0% coverage on state.rs, manager.rs)

**Risk**: Job state transitions are the core workflow control. Invalid transitions can leave jobs in impossible states.

### What to test

**3a. State transition validation** (`src/context/state.rs`)
The `JobState` enum has 8 states with 13 valid transitions. Test every valid transition and key invalid transitions:

Valid:
- Pending → InProgress, Pending → Cancelled
- InProgress → Completed, InProgress → Failed, InProgress → Stuck
- Completed → Submitted, Submitted → Accepted
- Stuck → InProgress (recovery)

Invalid (should fail):
- Pending → Completed (skips InProgress)
- Accepted → anything (terminal state)
- Failed → anything (terminal state)
- Cancelled → anything (terminal state)

**3b. Terminal state detection**
- `is_terminal()` returns `true` for Accepted, Failed, Cancelled
- `is_active()` returns `true` for all non-terminal states

**3c. Context manager** (`src/context/manager.rs`)
- `max_jobs` limit enforcement
- TOCTOU-safe concurrent job creation
- Terminal jobs not counted toward active limit

### Estimated test count needed: 25–30 tests

---

## Priority 4: History & Analytics (0% coverage, ~2,000 LOC)

**Risk**: Job/conversation history and estimation accuracy calculations contain floating-point arithmetic and aggregation logic that can silently produce wrong results.

### What to test

**4a. Analytics calculations** (`src/history/analytics.rs`, 226 LOC)
- Success rate: 0 jobs → 0.0, 5/10 → 0.5, 0/10 → 0.0
- Division-by-zero guards in cost_error_rate and time_error_rate
- NULL aggregate handling (no completed jobs → avg_duration = 0.0)
- Decimal-to-f64 conversion precision

**4b. Estimation learner** (`src/estimation/learner.rs`)
- Exponential moving average correctness: `new = old * (1 - α) + actual * α`
- Zero-estimate guards (estimated_cost = 0 → ratio = 1.0)
- Sample count incrementing
- Confidence thresholds based on sample count

**4c. Store round-trips** (requires database fixture)
- Conversation message ordering (created_at ASC)
- Pagination cursor edge cases (empty results, single message)
- Settings upsert (ON CONFLICT)
- Tool failure counter increment on duplicates

### Estimated test count needed: 30–40 tests

---

## Priority 5: Memory Tools (4 schema tests only, 1,162 LOC)

**Risk**: The memory tool system is security-critical — identity files (IDENTITY.md, SOUL.md, AGENTS.md, USER.md) must reject unauthorized writes. Currently only 4 schema validation tests exist; no execution logic is tested.

### What to test

**5a. Identity file protection** (security-critical)
Two validation paths exist for blocking writes to protected files. Both must be tested:
- Exact match: path == "IDENTITY.md"
- Case-insensitive: path.to_lowercase() == "identity.md"
- All 4 protected files: IDENTITY.md, SOUL.md, AGENTS.md, USER.md

**5b. Search limit clamping**
- Input 0 → default to 5
- Input 25 → clamp to 20
- Input 10 → pass through

**5c. Tree depth limits**
- Input 0 → default to 1
- Input 15 → clamp to 10

**5d. Connection type validation**
- Valid: "updates", "extends", "derives"
- Invalid: "unknown_type" → error

**5e. Space operations**
- Create, list, add document, remove document, delete space
- Operations on non-existent spaces

### Estimated test count needed: 25–30 tests

---

## Priority 6: Routine Tools (0 tests, 655 LOC)

### What to test

- Cron expression validation (valid 6-field, invalid formats)
- Event trigger regex validation (valid/invalid patterns)
- Next-fire computation accuracy
- Trigger type dispatch (cron, event, webhook, manual)
- Pagination limit clamping (0 → 10, 100 → 50)
- Cooldown default (300 sec)

### Estimated test count needed: 15–20 tests

---

## Priority 7: Channel Infrastructure

### What to test

**7a. ChannelManager** (`src/channels/manager.rs`, 177 LOC)
- `start_all()` with 0, 1, multiple channels
- Partial startup failure resilience (some channels fail, others succeed)
- `respond()` to found/not-found channels
- `broadcast_all()` result collection
- `shutdown_all()` with per-channel errors

**7b. REPL** (`src/channels/repl.rs`, 560 LOC)
- Tab completion prefix matching
- JSON parameter truncation (>120 chars per value, >300 chars total)
- Slash command dispatch (/debug, /quit, /help)

**7c. Channel types** (`src/channels/channel.rs`, 206 LOC)
- IncomingMessage builder chains
- OutgoingResponse thread targeting
- StatusUpdate enum variant coverage

### Estimated test count needed: 20–25 tests

---

## Lower Priority Items

### CLI Commands (16/23 files untested)
Most CLI commands are thin wrappers around internal APIs. The underlying logic is tested through the modules they call. Testing CLI commands has lower ROI unless there's CLI-specific error handling or formatting logic.

### Stub Tools (ecommerce, marketplace, restaurant, taskrabbit)
These are placeholder implementations with TODO comments. Skip until real API integrations are added.

### Simple Built-in Tools (echo.rs, time.rs)
Low LOC and low complexity. `time.rs` (135 LOC) has four operations worth testing (now, parse, diff, unknown). `echo.rs` (55 LOC) is trivially simple.

---

## Testing Infrastructure Gaps

### 1. No Code Coverage Measurement
There is no coverage tool configured (tarpaulin, llvm-cov, codecov). Adding coverage reporting to CI would provide ongoing visibility.

**Recommendation**: Add `cargo-llvm-cov` to CI with a coverage threshold.

### 2. Testcontainers Unused
`testcontainers-modules` with PostgreSQL support is in `Cargo.toml` dev-dependencies but never used. This is the intended solution for database testing.

**Recommendation**: Create a shared test helper that spins up a containerized PostgreSQL for database integration tests.

### 3. No libSQL Integration Tests
All 5 existing integration tests are PostgreSQL-only. The libSQL backend (3,080 LOC) has zero integration test coverage.

**Recommendation**: Create parallel integration test suites that run against both backends.

### 4. No Benchmarks
The CLAUDE.md mentions `--benches` in clippy flags, but no benchmark tests exist. Performance-sensitive paths (hybrid search, RRF ranking, media processing) would benefit from benchmarks.

---

## Summary: Recommended Test Implementation Order

| Phase | Target | Est. Tests | Rationale |
|-------|--------|------------|-----------|
| 1 | DB type conversion helpers + hybrid search | 30 | Pure functions, highest risk, no infrastructure needed |
| 2 | Config enum parsing + validation | 25 | Pure functions, foundational correctness |
| 3 | Context state machine + manager | 25 | Core workflow control, no DB needed |
| 4 | Memory tool identity file protection | 15 | Security-critical |
| 5 | DB integration tests (testcontainers) | 40 | Requires infrastructure setup |
| 6 | Analytics + estimation learner | 20 | Floating-point correctness |
| 7 | Routine tools + channel manager | 20 | Business logic validation |
| 8 | Coverage tooling in CI | — | Ongoing visibility |
| **Total** | | **~175** | |
