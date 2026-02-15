# IronClaw ↔ OpenClaw Feature Parity Matrix

This document tracks feature parity between IronClaw (Rust implementation) and OpenClaw (TypeScript reference implementation). Use this to coordinate work across developers.

**Legend:**
- ✅ Implemented
- 🚧 Partial (in progress or incomplete)
- ❌ Not implemented
- 🔮 Planned (in scope but not started)
- 🚫 Out of scope (intentionally skipped)
- ➖ N/A (not applicable to Rust implementation)

---

## 1. Architecture

| Feature | OpenClaw | IronClaw | Notes |
|---------|----------|----------|-------|
| Hub-and-spoke architecture | ✅ | ✅ | Web gateway as central hub |
| WebSocket control plane | ✅ | ✅ | Gateway with WebSocket + SSE |
| Single-user system | ✅ | ✅ | |
| Multi-agent routing | ✅ | ✅ | `AgentRouter` with identity-based routing, workspace isolation per-agent (`src/agent/multi_agent.rs`) |
| Session-based messaging | ✅ | ✅ | Per-sender sessions |
| Loopback-first networking | ✅ | ✅ | HTTP binds to 0.0.0.0 but can be configured |

---

## 2. Gateway System

| Feature | OpenClaw | IronClaw | Notes |
|---------|----------|----------|-------|
| Gateway control plane | ✅ | ✅ | Web gateway with 40+ API endpoints |
| HTTP endpoints for Control UI | ✅ | ✅ | Web dashboard with chat, memory, jobs, logs, extensions |
| Channel connection lifecycle | ✅ | ✅ | ChannelManager + WebSocket tracker |
| Session management/routing | ✅ | ✅ | SessionManager exists |
| Configuration hot-reload | ✅ | 🚧 | Infrastructure in `src/hot_reload.rs` (ConfigWatcher, ReloadEvent), wiring in progress |
| Network modes (loopback/LAN/remote) | ✅ | 🚧 | HTTP only |
| OpenAI-compatible HTTP API | ✅ | ✅ | /v1/chat/completions |
| Canvas hosting | ✅ | 🔮 | Agent-driven UI, planned |
| Gateway lock (PID-based) | ✅ | ✅ | `PidLock` in `src/channels/web/pid_lock.rs` |
| launchd/systemd integration | ✅ | ✅ | Service file generation in `src/cli/service.rs` (systemd + launchd) |
| Bonjour/mDNS discovery | ✅ | 🔮 | Planned |
| Tailscale integration | ✅ | 🔮 | Planned |
| Presence system | ✅ | 🔮 | OpenClaw tracks connected clients (macOS, WebChat, CLI) with 5-min TTL |
| Health check endpoints | ✅ | ✅ | /api/health + /api/gateway/status |
| `doctor` diagnostics | ✅ | ✅ | `ironclaw doctor` CLI command (`src/cli/doctor.rs`) |

---

## 3. Messaging Channels

| Channel | OpenClaw | IronClaw | Priority | Notes |
|---------|----------|----------|----------|-------|
| CLI/REPL | ✅ | ✅ | - | Interactive REPL with rustyline, termimad markdown rendering, crossterm, approval cards |
| HTTP webhook | ✅ | ✅ | - | axum with secret validation |
| WASM channels | ❌ | ✅ | - | IronClaw innovation |
| WhatsApp | ✅ | ❌ | P1 | Baileys (Web) |
| Telegram | ✅ | ✅ | - | WASM channel(MTProto), DM pairing, caption, /start, bot_username |
| Discord | ✅ | ❌ | P2 | discord.js |
| Signal | ✅ | ❌ | P2 | signal-cli |
| Slack | ✅ | ✅ | - | WASM tool |
| iMessage | ✅ | ❌ | P3 | BlueBubbles recommended |
| Feishu/Lark | ✅ | ❌ | P3 | |
| LINE | ✅ | ❌ | P3 | |
| WebChat | ✅ | ✅ | - | Web gateway chat |
| Matrix | ✅ | ❌ | P3 | E2EE support |
| Mattermost | ✅ | ❌ | P3 | |
| Google Chat | ✅ | ❌ | P3 | |
| MS Teams | ✅ | ❌ | P3 | |
| Twitch | ✅ | ❌ | P3 | |
| Voice Call | ✅ | ❌ | P3 | Twilio/Telnyx |
| Nostr | ✅ | ❌ | P3 | |

### Channel Features

| Feature | OpenClaw | IronClaw | Notes |
|---------|----------|----------|-------|
| DM pairing codes | ✅ | ✅ | `ironclaw pairing list/approve`, host APIs |
| Allowlist/blocklist | ✅ | 🚧 | allow_from + pairing store |
| Self-message bypass | ✅ | ✅ | `SelfMessageFilter` in `src/channels/self_message.rs` |
| Mention-based activation | ✅ | ✅ | bot_username + respond_to_all_group_messages |
| Per-group tool policies | ✅ | ✅ | `GroupPolicyManager` in `src/safety/group_policies.rs` |
| Thread isolation | ✅ | ✅ | Separate sessions per thread |
| Per-channel media limits | ✅ | 🚧 | Caption support for media; no size limits |
| Typing indicators | ✅ | 🚧 | REPL shows status; channel-level typing indicator management not implemented |
| Block streaming to channels | ✅ | 🔮 | OpenClaw streams partial text blocks as separate messages with human-like pacing |
| Channel-level retry | ✅ | 🔮 | OpenClaw has per-provider retry with jitter; IronClaw has LLM-level failover only |
| Group activation modes | ✅ | 🚧 | `bot_username` mention detection + `respond_to_all_group_messages` config |

---

## 4. CLI Commands

| Command | OpenClaw | IronClaw | Priority | Notes |
|---------|----------|----------|----------|-------|
| `run` (agent) | ✅ | ✅ | - | Default command |
| `tool install/list/remove` | ✅ | ✅ | - | WASM tools |
| `gateway start/stop/status` | ✅ | ✅ | - | `src/cli/gateway.rs` |
| `onboard` (wizard) | ✅ | ✅ | - | Interactive setup |
| `tui` | ✅ | ➖ | - | IronClaw uses interactive REPL via default `run` command (no separate `tui` subcommand) |
| `config` | ✅ | ✅ | - | Read/write config |
| `channels` | ✅ | ✅ | - | Channel list/status/enable/disable (`src/cli/channels.rs`) |
| `models` | ✅ | 🚧 | - | Model selector via `/model` REPL command; no dedicated CLI subcommand |
| `status` | ✅ | ✅ | - | System status |
| `agents` | ✅ | ✅ | - | Agent identity management (`src/cli/agents.rs`) |
| `sessions` | ✅ | ✅ | - | Session list/prune (`src/cli/sessions.rs`) |
| `memory` | ✅ | ✅ | - | search, read, write, tree, status, spaces, profile, connect |
| `skills` | ✅ | ✅ | - | Skill list/enable/disable/info (`src/cli/skills.rs`) |
| `pairing` | ✅ | ✅ | - | list/approve for channel DM pairing |
| `nodes` | ✅ | 🔮 | P3 | Device management |
| `plugins` | ✅ | ✅ | - | Plugin list/install/remove/info/update (`src/cli/plugins.rs`) |
| `hooks` | ✅ | ✅ | - | Lifecycle hook list/add/remove (`src/cli/hooks.rs`) |
| `cron` | ✅ | ✅ | - | Routine list/enable/disable/history (`src/cli/cron.rs`) |
| `webhooks` | ✅ | ✅ | - | Webhook list/add/remove/test (`src/cli/webhooks.rs`) |
| `message send` | ✅ | ✅ | - | Send to channels (`src/cli/message.rs`) |
| `browser` | ✅ | 🔮 | P3 | Browser automation |
| `sandbox` | ✅ | ✅ | - | WASM sandbox |
| `doctor` | ✅ | ✅ | - | Comprehensive diagnostics (`src/cli/doctor.rs`) |
| `logs` | ✅ | ✅ | - | Log tail/search/job (`src/cli/logs.rs`) |
| `update` | ✅ | ✅ | - | Self-update / version check |
| `completion` | ✅ | ✅ | - | Shell completion generation (`src/cli/completion.rs`) |

---

## 5. Agent System

| Feature | OpenClaw | IronClaw | Notes |
|---------|----------|----------|-------|
| Pi agent runtime | ✅ | ➖ | IronClaw uses custom runtime |
| RPC-based execution | ✅ | ✅ | Orchestrator/worker pattern |
| Multi-provider failover | ✅ | ✅ | `FailoverProvider` with exponential backoff (`src/llm/failover.rs`) |
| Per-sender sessions | ✅ | ✅ | |
| Global sessions | ✅ | ✅ | `GlobalSession` in `src/agent/session_pruning.rs` |
| Session pruning | ✅ | ✅ | `SessionPruner` with configurable policy (`src/agent/session_pruning.rs`) |
| Context compaction | ✅ | ✅ | Auto summarization |
| Custom system prompts | ✅ | ✅ | Template variables |
| Skills (modular capabilities) | ✅ | ✅ | `SkillRegistry` with tool bundles, tags, config (`src/skills/registry.rs`) |
| Thinking modes (low/med/high) | ✅ | ✅ | `ThinkingMode` with temperature, max_tokens, planning flags (`src/llm/thinking.rs`) |
| Block-level streaming | ✅ | 🚧 | SSE `StreamChunk` events via gateway |
| Tool-level streaming | ✅ | 🚧 | `ToolStarted`/`ToolCompleted`/`ToolResult` SSE events |
| Plugin tools | ✅ | ✅ | WASM tools |
| Tool policies (allow/deny) | ✅ | ✅ | |
| Exec approvals (`/approve`) | ✅ | ✅ | REPL approval cards with yes/no/always prompts |
| Elevated mode | ✅ | ✅ | `ElevatedMode` with time-limited activation, per-tool bypass (`src/safety/elevated.rs`) |
| Subagent support | ✅ | ✅ | Task framework |
| Auth profiles | ✅ | ✅ | `AuthProfileManager` with per-channel strategies (`src/agent/auth_profiles.rs`) |
| Session tools | ✅ | 🔮 | OpenClaw has session_list, session_history, session_send, session_spawn tools |
| Inline chat commands | ✅ | 🚧 | REPL has /help, /model, /undo, /redo, /clear, /compact, etc.; other channels lack inline command parsing |
| Command queue/lanes | ✅ | 🔮 | OpenClaw has per-session lane-aware FIFO with debounce and message coalescing |
| Presence tracking | ✅ | 🔮 | OpenClaw tracks connected clients with TTL; IronClaw has WebSocket tracker only |

---

## 6. Model & Provider Support

| Provider | OpenClaw | IronClaw | Priority | Notes |
|----------|----------|----------|----------|-------|
| NEAR AI | ✅ | ✅ | - | Primary provider (Responses API + Chat Completions API) |
| Anthropic (Claude) | ✅ | ✅ | - | Direct API via rig-core adapter (`src/llm/mod.rs`) |
| OpenAI | ✅ | ✅ | - | Direct API via rig-core adapter (`src/llm/mod.rs`) |
| AWS Bedrock | ✅ | 🔮 | P3 | Planned |
| Google Gemini | ✅ | 🔮 | P3 | Planned |
| OpenRouter | ✅ | ✅ | - | Via OpenAI-compatible endpoint config |
| Ollama (local) | ✅ | ✅ | - | Direct provider via rig-core adapter (`src/llm/mod.rs`) |
| node-llama-cpp | ✅ | ➖ | - | N/A for Rust |
| llama.cpp (native) | ❌ | 🔮 | P3 | Rust bindings |

### Model Features

| Feature | OpenClaw | IronClaw | Notes |
|---------|----------|----------|-------|
| Auto-discovery | ✅ | ✅ | `ModelDiscovery` for OpenAI, Anthropic, Ollama (`src/llm/auto_discovery.rs`) |
| Failover chains | ✅ | ✅ | `FailoverProvider` with priority ordering (`src/llm/failover.rs`) |
| Cooldown management | ✅ | ✅ | Exponential backoff per-provider in failover (`src/llm/failover.rs`) |
| Per-session model override | ✅ | ✅ | `/model` REPL command |
| Model selection UI | ✅ | ✅ | REPL `/model` command |

---

## 7. Media Handling

| Feature | OpenClaw | IronClaw | Priority | Notes |
|---------|----------|----------|----------|-------|
| Image processing | ✅ | ✅ | - | `ImageProcessor` with dimension detection, format parsing (`src/media/image.rs`) |
| Audio transcription | ✅ | ✅ | - | `WhisperProvider` via OpenAI API (`src/media/transcription.rs`) |
| Video support | ✅ | ✅ | - | `VideoProcessor` with MP4/WebM/AVI/MOV/MKV metadata extraction (`src/media/video.rs`) |
| PDF parsing | ✅ | ✅ | - | `PdfExtractor` with BT/ET text stream extraction (`src/media/pdf.rs`) |
| MIME detection | ✅ | ✅ | - | `detect_mime_type` with magic byte detection (`src/media/detection.rs`) |
| Media caching | ✅ | ✅ | - | `MediaCache` with TTL, LRU eviction, size limits (`src/media/cache.rs`) |
| Vision model integration | ✅ | ✅ | - | `OpenAiVisionProvider` for GPT-4V/Claude vision (`src/media/vision.rs`) |
| TTS (Edge TTS) | ✅ | 🔮 | P3 | Planned |
| TTS (OpenAI) | ✅ | ✅ | - | `OpenAiTtsProvider` with voice/format options (`src/media/tts.rs`) |
| Sticker-to-image | ✅ | ✅ | - | `StickerConverter` for WebP/TGS/animated WebP (`src/media/sticker.rs`) |

---

## 8. Plugin & Extension System

| Feature | OpenClaw | IronClaw | Notes |
|---------|----------|----------|-------|
| Dynamic loading | ✅ | ✅ | WASM modules |
| Manifest validation | ✅ | ✅ | WASM metadata |
| HTTP path registration | ✅ | 🚧 | `PluginRoute` framework in `src/extensions/plugins.rs` |
| Workspace-relative install | ✅ | ✅ | ~/.ironclaw/tools/ |
| Channel plugins | ✅ | ✅ | WASM channels |
| Auth plugins | ✅ | 🔮 | Planned |
| Memory plugins | ✅ | 🔮 | Custom backends, planned |
| Tool plugins | ✅ | ✅ | WASM tools |
| Hook plugins | ✅ | 🚧 | HookEngine framework exists (`src/hooks/engine.rs`) |
| Provider plugins | ✅ | 🔮 | Planned |
| Plugin CLI (`install`, `list`) | ✅ | ✅ | `tool` + `plugins` subcommands |
| ClawHub registry | ✅ | 🔮 | Discovery, planned |

---

## 9. Configuration System

| Feature | OpenClaw | IronClaw | Notes |
|---------|----------|----------|-------|
| Primary config file | ✅ `~/.openclaw/openclaw.json` | ✅ `.env` | Different formats |
| JSON5 support | ✅ | ✅ | `json5` crate integrated in Cargo.toml |
| YAML alternative | ✅ | ✅ | `serde_yaml` crate integrated in Cargo.toml |
| Environment variable interpolation | ✅ | ✅ | `${VAR}` |
| Config validation/schema | ✅ | ✅ | Type-safe Config struct |
| Hot-reload | ✅ | 🚧 | `ConfigWatcher` infrastructure in `src/hot_reload.rs` |
| Legacy migration | ✅ | ➖ | |
| State directory | ✅ `~/.openclaw-state/` | ✅ `~/.ironclaw/` | |
| Credentials directory | ✅ | ✅ | Session files |

---

## 10. Memory & Knowledge System

| Feature | OpenClaw | IronClaw | Notes |
|---------|----------|----------|-------|
| Vector memory | ✅ | ✅ | pgvector |
| Session-based memory | ✅ | ✅ | |
| Hybrid search (BM25 + vector) | ✅ | ✅ | RRF algorithm |
| OpenAI embeddings | ✅ | ✅ | |
| Gemini embeddings | ✅ | 🔮 | Planned |
| Local embeddings | ✅ | 🔮 | Planned |
| SQLite-vec backend | ✅ | ➖ | IronClaw uses PostgreSQL + libSQL |
| LanceDB backend | ✅ | 🔮 | Planned |
| QMD backend | ✅ | 🔮 | Planned |
| Atomic reindexing | ✅ | ✅ | |
| Embeddings batching | ✅ | ✅ | `BatchEmbeddingProcessor` in `src/workspace/batch_embeddings.rs` |
| Citation support | ✅ | ✅ | `Citation` and `CitedSearchResult` types |
| Memory CLI commands | ✅ | ✅ | search, read, write, tree, status, spaces, profile, connect (`src/cli/memory.rs`) |
| Flexible path structure | ✅ | ✅ | Filesystem-like API |
| Identity files (AGENTS.md, etc.) | ✅ | ✅ | |
| Daily logs | ✅ | ✅ | |
| Heartbeat checklist | ✅ | ✅ | HEARTBEAT.md |

---

## 11. Mobile Apps

| Feature | OpenClaw | IronClaw | Priority | Notes |
|---------|----------|----------|----------|-------|
| iOS app (SwiftUI) | ✅ | 🚫 | - | Out of scope initially |
| Android app (Kotlin) | ✅ | 🚫 | - | Out of scope initially |
| Gateway WebSocket client | ✅ | 🚫 | - | |
| Camera/photo access | ✅ | 🚫 | - | |
| Voice input | ✅ | 🚫 | - | |
| Push-to-talk | ✅ | 🚫 | - | |
| Location sharing | ✅ | 🚫 | - | |
| Node pairing | ✅ | 🚫 | - | |

### Owner: _Unassigned_ (if ever prioritized)

---

## 12. macOS App

| Feature | OpenClaw | IronClaw | Priority | Notes |
|---------|----------|----------|----------|-------|
| SwiftUI native app | ✅ | 🚫 | - | Out of scope |
| Menu bar presence | ✅ | 🚫 | - | |
| Bundled gateway | ✅ | 🚫 | - | |
| Canvas hosting | ✅ | 🚫 | - | |
| Voice wake | ✅ | 🚫 | - | |
| Exec approval dialogs | ✅ | ✅ | - | REPL approval cards |
| iMessage integration | ✅ | 🚫 | - | |

### Owner: _Unassigned_ (if ever prioritized)

---

## 13. Web Interface

| Feature | OpenClaw | IronClaw | Priority | Notes |
|---------|----------|----------|----------|-------|
| Control UI Dashboard | ✅ | ✅ | - | Web gateway with chat, memory, jobs, logs, extensions |
| Channel status view | ✅ | 🚧 | P2 | Gateway status widget, full channel view pending |
| Agent management | ✅ | 🚧 | - | CLI agent management done; web UI pending |
| Model selection | ✅ | ✅ | - | REPL `/model` command |
| Config editing | ✅ | 🔮 | P3 | Web UI planned |
| Debug/logs viewer | ✅ | ✅ | - | Real-time log streaming with level/target filters |
| WebChat interface | ✅ | ✅ | - | Web gateway chat with SSE/WebSocket |
| Canvas system (A2UI) | ✅ | 🔮 | P3 | Agent-driven UI, planned |

---

## 14. Automation

| Feature | OpenClaw | IronClaw | Priority | Notes |
|---------|----------|----------|----------|-------|
| Cron jobs | ✅ | ✅ | - | Routines with cron trigger |
| Timezone support | ✅ | ✅ | - | Via cron expressions |
| One-shot/recurring jobs | ✅ | ✅ | - | Manual + cron triggers |
| `beforeInbound` hook | ✅ | ✅ | - | `HookEngine::run_before_inbound` (`src/hooks/engine.rs`) |
| `beforeOutbound` hook | ✅ | ✅ | - | `HookEngine::run_before_outbound` |
| `beforeToolCall` hook | ✅ | ✅ | - | `HookEngine::run_before_tool_call` |
| `onMessage` hook | ✅ | ✅ | - | Routines with event trigger |
| `onSessionStart` hook | ✅ | ✅ | - | `HookEngine::run_on_session_start` |
| `onSessionEnd` hook | ✅ | ✅ | - | `HookEngine::run_on_session_end` |
| `transcribeAudio` hook | ✅ | 🚧 | P3 | HookType registered, handler pending |
| `transformResponse` hook | ✅ | ✅ | - | `HookEngine::run_transform_response` |
| Bundled hooks | ✅ | 🚧 | P2 | Framework exists, expanding library |
| Plugin hooks | ✅ | 🚧 | P3 | HookEngine + plugin framework exists |
| Workspace hooks | ✅ | ✅ | - | `HookSource::Workspace` with `HookAction` support |
| Outbound webhooks | ✅ | ✅ | - | `WebhookManager` with HMAC signing, retry (`src/hooks/webhooks.rs`) |
| Heartbeat system | ✅ | ✅ | - | Periodic execution |
| Gmail pub/sub | ✅ | 🔮 | P3 | Planned |

---

## 15. Security Features

| Feature | OpenClaw | IronClaw | Notes |
|---------|----------|----------|-------|
| Gateway token auth | ✅ | ✅ | Bearer token auth on web gateway |
| Device pairing | ✅ | ✅ | `DevicePairingManager` with challenge codes (`src/pairing/device.rs`) |
| Tailscale identity | ✅ | 🔮 | Planned |
| OAuth flows | ✅ | 🚧 | NEAR AI OAuth + extension OAuth 2.1 |
| DM pairing verification | ✅ | ✅ | ironclaw pairing approve, host APIs |
| Allowlist/blocklist | ✅ | 🚧 | allow_from + pairing store |
| Per-group tool policies | ✅ | ✅ | `GroupPolicyManager` with allow/deny/require-approval (`src/safety/group_policies.rs`) |
| Exec approvals | ✅ | ✅ | REPL approval cards with yes/no/always |
| TLS 1.3 minimum | ✅ | ✅ | reqwest rustls |
| SSRF protection | ✅ | ✅ | WASM allowlist |
| Loopback-first | ✅ | 🚧 | HTTP binds 0.0.0.0 |
| Docker sandbox | ✅ | ✅ | Orchestrator/worker containers |
| WASM sandbox | ❌ | ✅ | IronClaw innovation |
| Tool policies | ✅ | ✅ | |
| Elevated mode | ✅ | ✅ | Time-limited activation, per-tool bypass (`src/safety/elevated.rs`) |
| Safe bins allowlist | ✅ | ✅ | Curated POSIX + dev tool whitelist (`src/safety/bins_allowlist.rs`) |
| LD*/DYLD* validation | ✅ | ✅ | `validate_env_vars()` detects dangerous env vars (`src/safety/bins_allowlist.rs`) |
| Path traversal prevention | ✅ | ✅ | |
| Webhook signature verification | ✅ | ✅ | HMAC-SHA256 in outbound webhooks |
| Media URL validation | ✅ | ✅ | `validate_media_url()` in `src/media/detection.rs` |
| Prompt injection defense | ✅ | ✅ | Pattern detection, sanitization |
| Leak detection | ✅ | ✅ | Secret exfiltration |
| Log redaction | ✅ | 🚧 | Field-level `[REDACTED]` in Debug impls for Config, Secrets, OAuth tokens; no systematic log output redaction |
| Skill vulnerability scanning | ✅ | 🔮 | OpenClaw scans skill code for vulnerabilities; planned |

---

## 16. Development & Build System

| Feature | OpenClaw | IronClaw | Notes |
|---------|----------|----------|-------|
| Primary language | TypeScript | Rust | Different ecosystems |
| Build tool | tsdown | cargo | |
| Type checking | TypeScript/tsgo | rustc | |
| Linting | Oxlint | clippy | |
| Formatting | Oxfmt | rustfmt | |
| Package manager | pnpm | cargo | |
| Test framework | Vitest | built-in | |
| Coverage | V8 | tarpaulin/llvm-cov | |
| CI/CD | GitHub Actions | GitHub Actions | |
| Pre-commit hooks | prek | - | Consider adding |

---

## Implementation Priorities

### P0 - Core (Complete)
- ✅ REPL channel with approval cards
- ✅ HTTP webhook channel
- ✅ DM pairing (ironclaw pairing list/approve, host APIs)
- ✅ WASM tool sandbox
- ✅ Workspace/memory with hybrid search
- ✅ Prompt injection defense
- ✅ Heartbeat system
- ✅ Session management + pruning
- ✅ Context compaction
- ✅ Model selection
- ✅ Gateway control plane + WebSocket
- ✅ Web Control UI (chat, memory, jobs, logs, extensions, routines)
- ✅ WebChat channel (web gateway)
- ✅ Slack channel (WASM tool)
- ✅ Telegram channel (WASM tool, MTProto)
- ✅ Docker sandbox (orchestrator/worker)
- ✅ Cron job scheduling (routines)
- ✅ CLI subcommands (onboard, config, status, memory, doctor, sessions, hooks, cron, logs, message, channels, plugins, webhooks, skills, agents, gateway, completion, update)
- ✅ Gateway token auth
- ✅ Multi-provider failover with cooldown
- ✅ Hooks system (beforeInbound, beforeOutbound, beforeToolCall, onSessionStart, onSessionEnd, transformResponse)
- ✅ Outbound webhooks with HMAC signing
- ✅ Media handling (image, PDF, audio, video, vision, sticker, TTS, caching)
- ✅ Skills system (SkillRegistry with tool bundles)
- ✅ Thinking modes (Low/Medium/High)
- ✅ Security (elevated mode, safe bins, LD/DYLD validation, media URL validation, group tool policies)
- ✅ Multi-agent routing
- ✅ Auth profiles
- ✅ Device pairing
- ✅ LLM auto-discovery
- ✅ Self-message bypass
- ✅ Gateway PID lock + launchd/systemd integration
- ✅ JSON5/YAML config format support
- ✅ Embeddings batching + citation support
- ✅ Direct provider support (Anthropic, OpenAI, Ollama, OpenAI-compatible/OpenRouter)

### P1 - High Priority (Remaining)
- ❌ WhatsApp channel

### P2 - Medium Priority (Remaining)
- 🚧 Configuration hot-reload (wiring to running agent)
- 🚧 Full channel status view in web UI
- 🔮 Canvas hosting (agent-driven UI)

### P2 - Medium Priority (Newly Identified)
- 🔮 Session tools (session_list, session_history, session_send, session_spawn)
- 🔮 Presence system (connected client tracking with TTL)
- 🔮 Command queue / lane system (per-session message coalescing)
- 🚧 Inline chat commands in non-REPL channels
- 🚧 Log redaction (systematic sensitive data removal from log output)
- 🔮 Block streaming to channels (partial text as separate messages)
- 🔮 Channel-level message delivery retry with backoff

### P3 - Lower Priority (Remaining)
- ❌ Messaging channels (Discord, Signal, Matrix, iMessage, etc.)
- 🔮 AWS Bedrock provider
- 🔮 Google Gemini provider
- 🔮 Gemini/local embeddings
- 🔮 Browser automation
- 🔮 Tailscale integration
- 🔮 Bonjour/mDNS discovery
- 🔮 Edge TTS
- 🔮 Gmail pub/sub
- 🔮 Skill vulnerability scanning
- 🔮 Usage tracking from provider APIs

---

## How to Contribute

1. **Claim a section**: Edit this file and add your name/handle to the "Owner" field
2. **Create a tracking issue**: Link to GitHub issue for the feature area
3. **Update status**: Change ❌ to 🚧 when starting, ✅ when complete
4. **Add notes**: Document any design decisions or deviations

### Coordination

- Each major section should have one owner to avoid conflicts
- Owners can delegate sub-features to others
- Update this file as part of your PR

---

## Deviations from OpenClaw

IronClaw intentionally differs from OpenClaw in these ways:

1. **Rust vs TypeScript**: Native performance, memory safety, single binary distribution
2. **WASM sandbox vs Docker**: Lighter weight, faster startup, capability-based security
3. **PostgreSQL vs SQLite**: Better suited for production deployments (also supports libSQL/Turso)
4. **NEAR AI focus**: Primary provider with session-based auth
5. **No mobile/desktop apps**: Focus on server-side and CLI initially
6. **WASM channels**: Novel extension mechanism not in OpenClaw
7. **Dual database backend**: Both PostgreSQL and libSQL/Turso supported via trait abstraction

These are intentional architectural choices, not gaps to be filled.
