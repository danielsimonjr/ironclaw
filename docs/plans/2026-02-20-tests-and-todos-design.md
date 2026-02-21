# Tests & Feasible TODOs Design

Date: 2026-02-20

## Scope

3 TODO implementations + unit tests for ~25 untested files.

## TODO Implementations

### 1. OAuth nonce lookup (`channels/wasm/router.rs:431`)
Convert TODO to explanatory doc comment. The OAuth flow already uses local callbacks via `authorize_mcp_server()` which handles the full flow synchronously. The nonce-based lookup is a future iteration that isn't needed with the current design.

### 2. WIT description/schema extraction (`tools/wasm/runtime.rs:256,269`)
Parse the WAT/WIT component type information to extract tool description and JSON schema instead of returning hardcoded defaults. Use wasmtime's component type introspection API to read exported function signatures and doc annotations.

### 3. Edge TTS WebSocket (`media/edge_tts.rs:212`)
Implement the WebSocket connection to Microsoft Edge TTS endpoint using tokio-tungstenite. The protocol: connect to `wss://speech.platform.bing.com/consumer/speech/synthesize/readaloud/edge/v1`, send SSML payload, receive binary audio chunks. The existing `build_ssml()` and voice selection logic is already implemented.

## Test Strategy

### Pure function files (9)
- `estimation/mod.rs` — EMA calculation, cost estimation helpers
- `cli/mod.rs` — CLI arg parsing
- `llm/mod.rs` — Provider construction helpers
- `channels/wasm/error.rs` — Error type conversions
- `channels/wasm/mod.rs` — WASM channel routing
- `channels/mod.rs` — Channel type helpers
- `extensions/mod.rs` — Extension manager helpers
- `agent/mod.rs` — Agent construction
- `lib.rs` — Module re-exports (smoke test)

### Async files (10)
- `llm/provider.rs` — Default trait method behavior, message formatting
- `channels/channel.rs` — Channel trait default implementations
- `media/transcription.rs` — Transcription config, error paths
- `media/vision.rs` — Vision config, error paths
- `channels/repl.rs` — Input parsing, command detection
- `sandbox/mod.rs` — Config construction, availability checks
- `channels/manager.rs` — Manager construction, channel registration
- `channels/webhook_server.rs` — Webhook config, signature verification helpers
- `tools/wasm/mod.rs` — WASM tool loading config
- `tools/builtin/restaurant.rs` — Input parsing, parameter validation

### Infra-light files (6)
- `tools/builtin/routine.rs` — Cron expression parsing, routine parameter validation
- `cli/doctor.rs` — Health check item construction, status formatting
- `channels/web/mod.rs` — Web gateway config, route matching
- `cli/gateway.rs` — Gateway command parsing
- `cli/hooks.rs` — Hook command parsing
- `cli/cron.rs` — Cron command parsing

### Skipped (infra-heavy, ~20 files)
- `history/store.rs`, `workspace/repository.rs`, `db/postgres.rs`, `db/mod.rs`, `db/libsql_migrations.rs` — require live database
- `main.rs` — integration test territory
- `error.rs` — error types tested transitively

## Out of Scope
- Sandbox execution TODOs (user decision)
- Stub service tools (ecommerce, marketplace, restaurant, taskrabbit API implementations)
- Database integration tests
- main.rs tests
