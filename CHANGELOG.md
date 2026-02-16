# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Windows installer and PowerShell install script** ([#13](https://github.com/nearai/ironclaw/pull/13)):
  - PowerShell installer (`deploy/windows/ironclaw-installer.ps1`) with one-liner install: `irm .../ironclaw-installer.ps1 | iex`
  - Architecture detection (x86_64, ARM64 via Windows emulation)
  - Latest version auto-detection via GitHub API with fallback
  - Dual install modes: archive-based (tar.gz) or MSI (`-UseMsi` flag)
  - Configurable install directory (`-InstallDir`), version pinning (`-Version`), PATH opt-out (`-NoPathUpdate`)
  - CARGO_HOME/bin detection with LocalAppData fallback for install location
  - Upgrade handling for existing installations with binary rename
  - WiX v3 MSI source (`deploy/windows/ironclaw.wxs`) for `cargo-wix`/`cargo-dist` builds with per-user install scope and PATH integration
  - GitHub Actions workflow (`windows-installer.yml`) to upload PowerShell script to each release

- **RLM-based large document processing** ([#11](https://github.com/nearai/ironclaw/pull/11)):
  - Implements Recursive Language Model (RLM) techniques from Zhang, Kraska & Khattab (arXiv:2512.24601)
  - `DocumentContext`: external environment holding full document text, pre-computed overlapping chunks, and metadata
  - `RlmOperation`: typed operations (read_slice, read_lines, read_chunk, search, sub_query, batch_sub_query, final_answer) the LLM can request
  - `LargeDocumentProcessor`: iterative processing loop with recursive sub-query support and concurrent batch execution
  - `RlmConfig`: configurable max iterations, recursion depth, chunk sizes, context limits, and LLM parameters
  - `ProcessingStats`: detailed metrics tracking (iterations, sub-queries, tokens, chunks accessed, elapsed time)
  - New `MediaError` variants for recursive processing depth/iteration limits (`src/media/large_doc.rs`)

- **P3 feature parity** — 12 new modules for OpenClaw alignment ([#9](https://github.com/nearai/ironclaw/pull/9)):
  - Google Gemini LLM provider with REST API and function calling (`src/llm/gemini.rs`)
  - AWS Bedrock LLM provider with SigV4 auth and Converse API (`src/llm/bedrock.rs`)
  - Gemini embeddings with text-embedding-004 model (`src/workspace/gemini_embeddings.rs`)
  - Local hash-based BoW embeddings with TF-IDF weighting (`src/workspace/local_embeddings.rs`)
  - Network mode abstraction: Loopback/LAN/Remote (`src/channels/web/network_mode.rs`)
  - Bonjour/mDNS service advertisement (`src/channels/web/mdns.rs`)
  - Tailscale integration with WhoIs identity (`src/channels/web/tailscale.rs`)
  - Agent management REST API (`src/channels/web/agent_management.rs`)
  - OAuth 2.0/2.1 with PKCE S256 support (`src/safety/oauth.rs`)
  - General allowlist/blocklist ACL system (`src/safety/allowlist.rs`)
  - Gmail Pub/Sub webhook handler with deduplication (`src/hooks/gmail_pubsub.rs`)
  - ClawHub registry client with search, download, SHA256 verification (`src/extensions/clawhub.rs`)

- **P2 feature parity** — 15 new modules for OpenClaw alignment ([#8](https://github.com/nearai/ironclaw/pull/8)):
  - Config hot-reload wiring with debounced ConfigWatcher events (`src/agent/config_reload.rs`)
  - Per-session lane-aware command queue with message coalescing (`src/agent/command_queue.rs`)
  - Per-channel metrics with atomic counters (`src/channels/status_tracker.rs`)
  - Block streaming with paragraph/sentence/word splitting (`src/channels/block_streamer.rs`)
  - Delivery retry with exponential backoff and jitter (`src/channels/delivery_retry.rs`)
  - Portable slash command parsing across channels (`src/channels/inline_commands.rs`)
  - Canvas system with A2UI agent-driven UI (`src/channels/web/canvas.rs`)
  - Structured config editor with validation for 9 sections (`src/channels/web/config_editor.rs`)
  - 8 pre-built bundled hooks (profanity filter, rate limit guard, etc.) (`src/hooks/bundled.rs`)
  - Audio transcription handler (`src/hooks/transcribe.rs`)
  - Edge TTS provider with 10 voices and SSML generation (`src/media/edge_tts.rs`)
  - Browser automation tool with session management (`src/tools/builtin/browser.rs`)
  - Vulnerability scanner with 10 default rules (`src/skills/vulnerability_scanner.rs`)
  - Auth/memory/provider/hook/channel/tool plugin lifecycle (`src/extensions/plugin_manager.rs`)
  - Device management CLI (list/add/remove/ping/pair) (`src/cli/nodes.rs`)

- **OpenClaw feature parity** (P0/P1) ([#7](https://github.com/nearai/ironclaw/pull/7), [#6](https://github.com/nearai/ironclaw/pull/6)):
  - Session tools: `session_list`, `session_history`, `session_send`
  - Presence tracking with TTL-based expiry and capacity eviction
  - Log redaction for API keys, Bearer tokens, JWTs, AWS keys, emails
  - Multi-agent routing with identity-based dispatch
  - Per-channel auth profiles
  - Per-group tool policies with allow/deny lists
  - Self-message bypass filter
  - Device pairing with challenge codes
  - Gateway PID lock file management
  - LLM model auto-discovery
  - Multi-provider failover with cooldown and exponential backoff
  - Thinking modes: low/medium/high reasoning depth
  - Sticker-to-image conversion, video metadata extraction, OpenAI TTS
  - Hooks system with lifecycle events, outbound webhooks, HMAC-SHA256 signing
  - Media handling: MIME detection, image processing, PDF extraction, audio transcription, vision
  - Skills system: modular capability bundles with tools, prompts, and policies
  - Elevated mode, safe binaries allowlist, hot-reload
  - New CLI subcommands: `doctor`, `gateway`, `sessions`, `hooks`, `cron`, `logs`, `message`, `channels`, `plugins`, `webhooks`, `skills`, `agents`, `nodes`, `browser`, `completion`, `service`

- **Supermemory-inspired memory features** ([#5](https://github.com/nearai/ironclaw/pull/5)):
  - Memory connections (knowledge graph), spaces (collections), user profiles (personalization)
  - New tools: `memory_connect`, `memory_spaces`, `memory_profile`
  - PostgreSQL migration V9 with 4 new tables; libSQL schema updated
  - New CLI subcommands: `spaces`, `profile`, `connect`

- **Multi-provider LLM setup wizard** ([#4](https://github.com/nearai/ironclaw/pull/4)):
  - Replace NEAR AI-specific auth with multi-provider LLM selection
  - Supports NEAR AI, OpenAI, Anthropic, Ollama, OpenAI-compatible endpoints
  - Adaptive model selection and embeddings defaults based on chosen backend

- libSQL/Turso embedded database backend with full feature parity ([#47](https://github.com/nearai/ironclaw/pull/47))
- OpenAI-compatible HTTP API: `/v1/chat/completions` and `/v1/models` endpoints ([#31](https://github.com/nearai/ironclaw/pull/31))
- GCP deployment scaffolding: Dockerfile, systemd units, setup script ([#40](https://github.com/nearai/ironclaw/pull/40))

### Fixed

- Comprehensive security hardening across 42 findings ([#2](https://github.com/nearai/ironclaw/pull/2)):
  - Unicode normalization to prevent prompt injection bypass
  - JSON recursion depth limit to prevent stack overflow
  - OAuth CSRF protection, OsRng for crypto operations
  - SSRF prevention, WASM tools require approval by default
  - WebSocket rate limiting, DOM-based XSS sanitizer, security headers
- Security hardening across all layers ([#35](https://github.com/nearai/ironclaw/pull/35)):
  - Constant-time token comparison, container capability restrictions
  - HTTP redirect disabling (SSRF), user-scoped job APIs, CORS restrictions
  - Path traversal guard, TOCTOU fixes, DNS rebinding protection
  - Rate limiter on gateway, extension install validation, destructive command blocklist
- Flatten tool messages for NEAR AI cloud-api compatibility ([#41](https://github.com/nearai/ironclaw/pull/41))
- Move debug log truncation to REPL channel with UTF-8 safe truncation ([#65](https://github.com/nearai/ironclaw/pull/65))

### Changed

- Bump MSRV to 1.92 for rig-core 0.30 let_chains compatibility ([#40](https://github.com/nearai/ironclaw/pull/40))
- CI test workflow split to exclude PostgreSQL-dependent integration tests
- Module count increased from 25 to 28 (added `hooks`, `hot_reload`, `media`, `skills`)

## [0.1.3](https://github.com/nearai/ironclaw/compare/v0.1.2...v0.1.3) - 2026-02-12

### Other

- Enabled builds caching during CI/CD
- Disabled npm publishing as the name is already taken

## [0.1.2](https://github.com/nearai/ironclaw/compare/v0.1.1...v0.1.2) - 2026-02-12

### Other

- Added Installation instructions for the pre-built binaries
- Disabled Windows ARM64 builds as auto-updater [provided by cargo-dist] does not support this platform yet and it is not a common platform for us to support

## [0.1.1](https://github.com/nearai/ironclaw/compare/v0.1.0...v0.1.1) - 2026-02-12

### Other

- Renamed the secrets in release-plz.yml to match the configuration
- Make sure that the binaries release CD it kicking in after release-plz

## [0.1.0](https://github.com/nearai/ironclaw/releases/tag/v0.1.0) - 2026-02-12

### Added

- Add multi-provider LLM support via rig-core adapter ([#36](https://github.com/nearai/ironclaw/pull/36))
- Sandbox jobs ([#4](https://github.com/nearai/ironclaw/pull/4))
- Add Google Suite & Telegram WASM tools ([#9](https://github.com/nearai/ironclaw/pull/9))
- Improve CLI ([#5](https://github.com/nearai/ironclaw/pull/5))

### Fixed

- resolve runtime panic in Linux keychain integration ([#32](https://github.com/nearai/ironclaw/pull/32))

### Other

- Skip release-plz on forks
- Upgraded release-plz CD pipeline
- Added CI/CD and release pipelines ([#45](https://github.com/nearai/ironclaw/pull/45))
- DM pairing + Telegram channel improvements ([#17](https://github.com/nearai/ironclaw/pull/17))
- Fixes build, adds missing sse event and correct command ([#11](https://github.com/nearai/ironclaw/pull/11))
- Codex/feature parity pr hook ([#6](https://github.com/nearai/ironclaw/pull/6))
- Add WebSocket gateway and control plane ([#8](https://github.com/nearai/ironclaw/pull/8))
- select bundled Telegram channel and auto-install ([#3](https://github.com/nearai/ironclaw/pull/3))
- Adding skills for reusable work
- Fix MCP tool calls, approval loop, shutdown, and improve web UI
- Add auth mode, fix MCP token handling, and parallelize startup loading
- Merge remote-tracking branch 'origin/main' into ui
- Adding web UI
- Rename `setup` CLI command to `onboard` for compatibility
- Add in-chat extension discovery, auth, and activation system
- Add Telegram typing indicator via WIT on-status callback
- Add proactivity features: memory CLI, session pruning, self-repair notifications, slash commands, status diagnostics, context warnings
- Add hosted MCP server support with OAuth 2.1 and token refresh
- Add interactive setup wizard and persistent settings
- Rebrand to IronClaw with security-first mission
- Fix build_software tool stuck in planning mode loop
- Enable sandbox by default
- Fix Telegram Markdown formatting and clarify tool/memory distinctions
- Simplify Telegram channel config with host-injected tunnel/webhook settings
- Apply Telegram channel learnings to WhatsApp implementation
- Merge remote-tracking branch 'origin/main'
- Docker file for sandbox
- Replace hardcoded intent patterns with job tools
- Fix router test to match intentional job creation patterns
- Add Docker execution sandbox for secure shell command isolation
- Move setup wizard credentials to database storage
- Add interactive setup wizard for first-run configuration
- Add Telegram Bot API channel as WASM module
- Add OpenClaw feature parity tracking matrix
- Add Chat Completions API support and expand REPL debugging
- Implementing channels to be handled in wasm
- Support non interactive mode and model selection
- Implement tool approval, fix tool definition refresh, and wire embeddings
- Tool use
- Wiring more
- Add heartbeat integration, planning phase, and auto-repair
- Login flow
- Extend support for session management
- Adding builder capability
- Load tools at launch
- Fix multiline message rendering in TUI
- Parse NEAR AI alternative response format with output field
- Handle NEAR AI plain text responses
- Disable mouse capture to allow text selection in TUI
- Add verbose logging to debug empty NEAR AI responses
- Improve NEAR AI response parsing for varying response formats
- Show status/thinking messages in chat window, debug empty responses
- Add timeout and logging to NEAR AI provider
- Add status updates to show agent thinking/processing state
- Add CLI subcommands for WASM tool management
- Fix TUI shutdown: send /shutdown message and handle in agent loop
- Remove SimpleCliChannel, add Ctrl+D twice quit, redirect logs to TUI
- Fix TuiChannel integration and enable in main.rs
- Integrate Codex patterns: task scheduler, TUI, sessions, compaction
- Adding LICENSE
- Add README with IronClaw branding
- Add WASM sandbox secure API extension
- Wire database Store into agent loop
- Implementing WASM runtime
- Add workspace integration tests
- Compact memory_tree output format
- Replace memory_list with memory_tree tool
- Simplify workspace to path-based storage, remove legacy code
- Add NEAR AI chat-api as default LLM provider
- Add CLAUDE.md project documentation
- Add workspace and memory system (OpenClaw-inspired)
- Initial implementation of the agent framework
