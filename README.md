<p align="center">
  <img src="ironclaw.png" alt="IronClaw" width="200"/>
</p>

<h1 align="center">IronClaw</h1>

<p align="center">
  <strong>Your secure personal AI assistant, always on your side</strong>
</p>

<p align="center">
  <a href="#philosophy">Philosophy</a> •
  <a href="#features">Features</a> •
  <a href="#installation">Installation</a> •
  <a href="#configuration">Configuration</a> •
  <a href="#security">Security</a> •
  <a href="#architecture">Architecture</a>
</p>

---

## Philosophy

IronClaw is built on a simple principle: **your AI assistant should work for you, not against you**.

In a world where AI systems are increasingly opaque about data handling and aligned with corporate interests, IronClaw takes a different approach:

- **Your data stays yours** - All information is stored locally, encrypted, and never leaves your control
- **Transparency by design** - Open source, auditable, no hidden telemetry or data harvesting
- **Self-expanding capabilities** - Build new tools on the fly without waiting for vendor updates
- **Defense in depth** - Multiple security layers protect against prompt injection and data exfiltration

IronClaw is the AI assistant you can actually trust with your personal and professional life.

## Features

### Security First

- **WASM Sandbox** - Untrusted tools run in isolated WebAssembly containers with capability-based permissions
- **Credential Protection** - Secrets are never exposed to tools; injected at the host boundary with leak detection
- **Prompt Injection Defense** - Pattern detection, Unicode normalization, content sanitization, and policy enforcement
- **Endpoint Allowlisting** - HTTP requests only to explicitly approved hosts and paths
- **Log Redaction** - Automatic redaction of API keys, Bearer tokens, JWTs, and credentials from log output
- **SSRF Prevention** - DNS rebinding protection, private IP rejection, redirect blocking
- **Elevated Mode** - Time-limited privileged execution with full audit tracking

### Always Available

- **Multi-channel** - REPL, HTTP webhooks, WASM channels (Telegram, Slack), and web gateway
- **Docker Sandbox** - Isolated container execution with per-job tokens and orchestrator/worker pattern
- **Web Gateway** - Browser UI with real-time SSE/WebSocket streaming, canvas (A2UI), config editor
- **Routines** - Cron schedules, event triggers, webhook handlers for background automation
- **Hooks System** - Lifecycle hooks (beforeInbound, beforeOutbound, beforeToolCall, etc.) with 8 bundled hooks
- **Heartbeat System** - Proactive background execution for monitoring and maintenance tasks
- **Parallel Jobs** - Handle multiple requests concurrently with isolated contexts
- **Self-repair** - Automatic detection and recovery of stuck operations

### Multi-Provider LLM Support

- **NEAR AI** - Session-based auth with Responses API + Chat Completions API
- **OpenAI** - Direct API integration via rig-core adapter
- **Anthropic (Claude)** - Direct API integration via rig-core adapter
- **Google Gemini** - REST API with function calling support
- **AWS Bedrock** - SigV4 authentication with Converse API
- **Ollama** - Local inference, no account needed
- **OpenRouter** - Unified API gateway for 200+ models from multiple providers
- **OpenAI-compatible** - vLLM, LiteLLM, Together, and more
- **Failover Chains** - Priority-based multi-provider failover with cooldown and exponential backoff
- **Auto-discovery** - Automatic model listing for OpenAI, Anthropic, Ollama, and OpenRouter
- **Thinking Modes** - Low/medium/high reasoning depth with configurable temperature and token limits

### Self-Expanding

- **Dynamic Tool Building** - Describe what you need, and IronClaw builds it as a WASM tool
- **MCP Protocol** - Connect to Model Context Protocol servers for additional capabilities
- **Plugin Architecture** - Drop in new WASM tools and channels without restarting
- **ClawHub Registry** - Search, download, and verify extensions from ClawHub with SHA256 integrity checks

### Persistent Memory

- **Hybrid Search** - Full-text + vector search using Reciprocal Rank Fusion
- **Workspace Filesystem** - Flexible path-based storage for notes, logs, and context
- **Identity Files** - Maintain consistent personality and preferences across sessions
- **Knowledge Graph** - Typed connections (updates, extends, derives) between memories
- **Spaces** - Named collections for organizing memories by topic or project
- **User Profiles** - Auto-maintained fact profiles for personalization
- **Multiple Embedding Backends** - OpenAI, Google Gemini, and local hash-based BoW embeddings

### Media Handling

- **Image Processing** - Dimension detection, format parsing
- **Audio Transcription** - Whisper API integration
- **Video Metadata** - MP4/WebM/AVI/MOV/MKV extraction
- **PDF Parsing** - Text stream extraction
- **Text-to-Speech** - OpenAI TTS and Edge TTS with 10 voices
- **Vision Models** - GPT-4V/Claude vision integration
- **Sticker Conversion** - WebP/TGS/animated WebP support
- **Media Caching** - TTL-based LRU cache with size limits
- **Large Document Processing** - RLM-based recursive processing for documents exceeding LLM context windows, with structured operations (slicing, chunking, search, recursive sub-queries)

## Installation

### Prerequisites

- Rust 1.92+
- PostgreSQL 15+ with [pgvector](https://github.com/pgvector/pgvector) extension, **or** libSQL/Turso (embedded, no server needed)
- LLM provider account or API key (configured via setup wizard; supports NEAR AI, OpenAI, Anthropic, Google Gemini, AWS Bedrock, Ollama, OpenRouter, and OpenAI-compatible endpoints)

## Download or Build

Visit [Releases page](https://github.com/danielsimonjr/ironclaw/releases/) to see the latest updates.

<details>
  <summary>Install via Windows Installer (Windows)</summary>

Download the [Windows Installer (.msi)](https://github.com/danielsimonjr/ironclaw/releases/latest/download/ironclaw-x86_64-pc-windows-msvc.msi) and run it. The MSI installs per-user to `LocalAppData\Programs\IronClaw` and adds `ironclaw` to your PATH automatically.

</details>

<details>
  <summary>Install via PowerShell script (Windows)</summary>

```sh
irm https://github.com/danielsimonjr/ironclaw/releases/latest/download/ironclaw-installer.ps1 | iex
```

The PowerShell installer supports additional options when saved and run as a script:

```powershell
.\ironclaw-installer.ps1                          # Install latest to default location
.\ironclaw-installer.ps1 -Version 0.1.3           # Install specific version
.\ironclaw-installer.ps1 -InstallDir "C:\tools"   # Custom install directory
.\ironclaw-installer.ps1 -NoPathUpdate             # Skip PATH modification
.\ironclaw-installer.ps1 -UseMsi                   # Use MSI installer instead
```

</details>

<details>
  <summary>Install via shell script (macOS, Linux, Windows/WSL)</summary>

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/danielsimonjr/ironclaw/releases/latest/download/ironclaw-installer.sh | sh
```
</details>

<details>
  <summary>Compile the source code (Cargo on Windows, Linux, macOS)</summary>

Install it with `cargo`, just make sure you have [Rust](https://rustup.rs) installed on your computer.

```bash
# Clone the repository
git clone https://github.com/danielsimonjr/ironclaw.git
cd ironclaw

# Build (PostgreSQL backend, default)
cargo build --release

# Or build with libSQL backend (no PostgreSQL needed)
cargo build --release --no-default-features --features libsql

# Run tests
cargo test
```

For **full release** (after modifying channel sources), run `./scripts/build-all.sh` to rebuild channels first.

</details>

### Database Setup

**PostgreSQL (default):**

```bash
# Create database
createdb ironclaw

# Enable pgvector
psql ironclaw -c "CREATE EXTENSION IF NOT EXISTS vector;"
```

**libSQL/Turso (embedded alternative):**

No setup required. The database is created automatically at `~/.ironclaw/ironclaw.db`. Configure with:

```bash
DATABASE_BACKEND=libsql
```

## Configuration

Run the setup wizard to configure IronClaw:

```bash
ironclaw onboard
```

The wizard handles database connection, LLM provider selection (NEAR AI, OpenAI,
Anthropic, Google Gemini, AWS Bedrock, Ollama, OpenRouter, or OpenAI-compatible), and secrets
encryption (using your system keychain). All settings are saved to
`~/.ironclaw/settings.json`.

### Environment Variables

Key configuration options (see `.env.example` for the full list):

| Variable | Description |
|----------|-------------|
| `DATABASE_BACKEND` | `postgres` (default) or `libsql` |
| `DATABASE_URL` | PostgreSQL connection string |
| `LLM_BACKEND` | `nearai`, `openai`, `anthropic`, `ollama`, `openai_compatible`, `gemini`, `bedrock`, `openrouter` |
| `GATEWAY_ENABLED` | Enable web UI gateway |
| `SANDBOX_ENABLED` | Enable Docker container isolation |
| `HEARTBEAT_ENABLED` | Enable proactive background execution |

## Security

IronClaw implements defense in depth to protect your data and prevent misuse.

### WASM Sandbox

All untrusted tools run in isolated WebAssembly containers:

- **Capability-based permissions** - Explicit opt-in for HTTP, secrets, tool invocation
- **Endpoint allowlisting** - HTTP requests only to approved hosts/paths
- **Credential injection** - Secrets injected at host boundary, never exposed to WASM code
- **Leak detection** - Scans requests and responses for secret exfiltration attempts
- **Rate limiting** - Per-tool request limits to prevent abuse
- **Resource limits** - Memory, CPU, and execution time constraints

```
WASM ──► Allowlist ──► Leak Scan ──► Credential ──► Execute ──► Leak Scan ──► WASM
         Validator     (request)     Injector       Request     (response)
```

### Prompt Injection Defense

External content passes through multiple security layers:

- Unicode normalization (zero-width chars, homoglyph detection)
- Pattern-based detection of injection attempts
- Content sanitization and escaping with recursion depth limits
- Policy rules with severity levels (Block/Warn/Review/Sanitize)
- Tool output wrapping for safe LLM context injection

### Data Protection

- All data stored locally in your PostgreSQL or libSQL database
- Secrets encrypted with AES-256-GCM
- System keychain integration (macOS Keychain, Linux GNOME Keyring/KWallet)
- No telemetry, analytics, or data sharing
- Full audit log of all tool executions
- Log redaction of sensitive data (API keys, tokens, credentials)
- Constant-time token comparison to prevent timing attacks

## Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│                          Channels                                  │
│  ┌──────┐  ┌──────┐  ┌─────────────┐  ┌─────────────┐            │
│  │ REPL │  │ HTTP │  │WASM Channels│  │ Web Gateway │            │
│  └──┬───┘  └──┬───┘  └──────┬──────┘  │ (SSE + WS) │            │
│     │         │              │         └──────┬──────┘            │
│     └─────────┴──────────────┴────────────────┘                   │
│                              │                                     │
│                    ┌─────────▼─────────┐                          │
│                    │    Agent Loop     │  Intent routing           │
│                    └────┬─────────┬────┘                          │
│                         │         │                                │
│              ┌──────────▼───┐  ┌──▼──────────────┐               │
│              │  Scheduler   │  │ Routines Engine  │               │
│              │(parallel jobs)│  │(cron, event, wh) │               │
│              └──────┬───────┘  └────────┬─────────┘               │
│                     │                   │                          │
│       ┌─────────────┼───────────────────┘                         │
│       │             │                                              │
│   ┌───▼────┐   ┌────▼────────────────┐                           │
│   │ Local  │   │    Orchestrator     │                           │
│   │Workers │   │  ┌───────────────┐  │                           │
│   │(in-proc)│   │  │ Docker Sandbox│  │                           │
│   └───┬────┘   │  │   Containers  │  │                           │
│       │        │  │ ┌───────────┐ │  │                           │
│       │        │  │ │Worker / CC│ │  │                           │
│       │        │  │ └───────────┘ │  │                           │
│       │        │  └───────────────┘  │                           │
│       │        └─────────┬───────────┘                           │
│       └──────────────────┤                                        │
│                          │                                        │
│              ┌───────────▼──────────┐                             │
│              │    Tool Registry     │                             │
│              │  Built-in, MCP, WASM │                             │
│              └──────────────────────┘                             │
└────────────────────────────────────────────────────────────────────┘
```

### Core Components

| Component | Purpose |
|-----------|---------|
| **Agent Loop** | Main message handling, multi-agent routing, and job coordination |
| **Router** | Classifies user intent (command, query, task) |
| **Scheduler** | Manages parallel job execution with priorities |
| **Worker** | Executes jobs with LLM reasoning and tool calls |
| **Orchestrator** | Container lifecycle, LLM proxying, per-job auth |
| **Web Gateway** | Browser UI with chat, memory, jobs, logs, extensions, routines, canvas, config editor |
| **Routines Engine** | Scheduled (cron) and reactive (event, webhook) background tasks |
| **Hooks Engine** | Lifecycle hooks with shell/HTTP/inline/webhook actions |
| **Workspace** | Persistent memory with hybrid search, connections, spaces, and profiles |
| **Safety Layer** | Prompt injection defense, leak detection, log redaction, and content sanitization |
| **Skills** | Modular capability bundles with tools, prompts, and policies |
| **Media** | Image, audio, video, PDF, TTS, vision, sticker, and large document processing |

### Dual Database Backend

IronClaw supports two database backends via compile-time feature flags:

| Backend | Feature | Best For |
|---------|---------|----------|
| **PostgreSQL** | `postgres` (default) | Production deployments, full vector search |
| **libSQL/Turso** | `libsql` | Embedded/edge deployment, zero-config setup |

## Usage

```bash
# First-time setup (configures database, auth, etc.)
ironclaw onboard

# Start interactive REPL
cargo run

# With debug logging
RUST_LOG=ironclaw=debug cargo run

# System diagnostics
ironclaw doctor

# Manage gateway
ironclaw gateway start
ironclaw gateway status
```

### CLI Commands

| Command | Purpose |
|---------|---------|
| `run` | Start interactive REPL (default) |
| `onboard` | Interactive setup wizard |
| `doctor` | System diagnostics |
| `config` | Read/write configuration |
| `status` | System status overview |
| `memory` | Search, read, write, tree, spaces, profile, connect |
| `tool` | WASM tool management |
| `mcp` | MCP server management |
| `gateway` | Web gateway start/stop/status |
| `sessions` | Session list/prune |
| `hooks` | Lifecycle hook management |
| `cron` | Routine list/enable/disable/history |
| `channels` | Channel list/status/enable/disable |
| `plugins` | Plugin list/install/remove |
| `agents` | Agent identity management |
| `pairing` | DM pairing approval |
| `logs` | Log tail/search/filter |
| `message` | Send messages to channels |
| `webhooks` | Webhook list/add/remove/test |
| `skills` | Skill list/enable/disable |
| `nodes` | Device management |
| `browser` | Browser automation |
| `completion` | Shell completion generation |
| `service` | systemd/launchd service file generation |

## Development

```bash
# Format code
cargo fmt

# Lint
cargo clippy --all --benches --tests --examples --all-features

# Run tests
createdb ironclaw_test
cargo test

# Run specific test
cargo test test_name

# Build with libSQL instead of PostgreSQL
cargo build --no-default-features --features libsql
```

- **Telegram channel**: See [docs/TELEGRAM_SETUP.md](docs/TELEGRAM_SETUP.md) for setup and DM pairing.
- **Building channels**: See [docs/BUILDING_CHANNELS.md](docs/BUILDING_CHANNELS.md) for WASM channel development.
- **Changing channel sources**: Run `./channels-src/telegram/build.sh` before `cargo build` so the updated WASM is bundled.
- **Feature parity**: See [FEATURE_PARITY.md](FEATURE_PARITY.md) for the complete IronClaw vs OpenClaw tracking matrix.

## OpenClaw Heritage

IronClaw is a Rust reimplementation inspired by [OpenClaw](https://github.com/openclaw/openclaw). See [FEATURE_PARITY.md](FEATURE_PARITY.md) for the complete tracking matrix.

Key differences:

- **Rust vs TypeScript** - Native performance, memory safety, single binary
- **WASM sandbox vs Docker** - Lightweight, capability-based security
- **Dual database backend** - PostgreSQL for production, libSQL/Turso for embedded/edge
- **Security-first design** - Multiple defense layers, credential protection, log redaction
- **Multi-provider LLM** - 8+ providers with failover, auto-discovery, and thinking modes

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.
