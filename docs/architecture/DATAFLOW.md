# IronClaw - Data Flow Documentation

**Last Updated**: 2026-02-22

---

## Table of Contents

1. [Overview](#overview)
2. [Message Processing Flow](#message-processing-flow)
3. [Tool Execution Flow](#tool-execution-flow)
4. [LLM Request Flow](#llm-request-flow)
5. [Safety Pipeline Flow](#safety-pipeline-flow)
6. [Channel I/O Flow](#channel-io-flow)
7. [Job Lifecycle](#job-lifecycle)
8. [Startup Sequence](#startup-sequence)
9. [WASM Tool/Channel Flow](#wasm-toolchannel-flow)
10. [Workspace/Memory Flow](#workspacememory-flow)
11. [Hook Execution Flow](#hook-execution-flow)

---

## Overview

Data flows through IronClaw in a layered, event-driven architecture. Messages enter through Channels, pass through the Agent loop for parsing and routing, are dispatched to Workers for LLM reasoning and tool execution, and flow back out through the same channel. All tool outputs pass through the Safety layer before reaching the LLM.

```
+-------------------------------------------------------------------+
|  Channels (REPL, HTTP, WebSocket, WASM, Web Gateway)              |
+---------------------------------+---------------------------------+
                                  |
                                  v
+---------------------------------+---------------------------------+
|  ChannelManager (merges all input streams via select_all)         |
+---------------------------------+---------------------------------+
                                  |
                                  v
+---------------------------------+---------------------------------+
|  Agent Loop                                                       |
|  +------------------+  +------------------+  +-----------------+  |
|  | SubmissionParser |->| SessionManager   |->| Router          |  |
|  +------------------+  +------------------+  +-----------------+  |
+---------------------------------+---------------------------------+
                                  |
                                  v
+---------------------------------+---------------------------------+
|  Scheduler (parallel job dispatch via tokio tasks)                |
+---------------------------------+---------------------------------+
                                  |
                                  v
+---------------------------------+---------------------------------+
|  Worker (LLM reasoning + tool execution loop)                     |
|  +-------------+  +-------------+  +----------------------------+ |
|  | Reasoning   |->| ToolRegistry|->| SafetyLayer                | |
|  | (LLM calls) |  | (execute)   |  | (sanitize -> validate ->   | |
|  |             |  |             |  |  policy -> leak detection)  | |
|  +-------------+  +-------------+  +----------------------------+ |
+---------------------------------+---------------------------------+
                                  |
                                  v
+---------------------------------+---------------------------------+
|  Persistence Layer                                                |
|  +----------+  +-----------+  +--------------------------------+  |
|  | Database |  | Workspace |  | SecretsStore                   |  |
|  | (PG/     |  | (memory   |  | (encrypted credential storage) |  |
|  |  libSQL) |  |  + search)|  |                                |  |
|  +----------+  +-----------+  +--------------------------------+  |
+-------------------------------------------------------------------+
```

---

## Message Processing Flow

The core request path from channel input to agent response.

```mermaid
sequenceDiagram
    participant Ch as Channel (REPL/HTTP/WS/WASM)
    participant CM as ChannelManager
    participant AL as Agent Loop
    participant SP as SubmissionParser
    participant SM as SessionManager
    participant R as Router
    participant S as Scheduler
    participant W as Worker

    Ch->>CM: IncomingMessage {id, channel, user_id, content, thread_id}
    CM->>AL: Merged stream via select_all()
    AL->>SP: parse(content)
    SP-->>AL: Submission (UserInput | SystemCommand | Undo | Redo | ...)

    alt SystemCommand / Control
        AL-->>Ch: Direct response (help, version, tools, etc.)
    else UserInput
        AL->>SM: resolve_thread(user_id, channel, thread_id)
        SM-->>AL: (Session, thread_id)
        AL->>AL: Check auth mode / pending approval
        AL->>R: route_command(message) or classify intent
        R-->>AL: MessageIntent (Chat | CreateJob | Command | ...)

        alt Chat / simple intent
            AL->>AL: run_agentic_loop(session, thread, content)
            AL->>W: LLM reasoning + tool loop
            W-->>AL: AgenticLoopResult::Response(text)
        else CreateJob
            AL->>S: schedule(job_id)
            S->>W: spawn worker task
            W-->>S: completion
        end

        AL->>CM: respond(msg, OutgoingResponse)
        CM->>Ch: deliver response
    end
```

### Submission Types

`SubmissionParser::parse()` converts raw text into typed submissions:

| Submission | Trigger | Behavior |
|------------|---------|----------|
| `UserInput` | Any non-command text | Enters agentic loop |
| `SystemCommand` | `/help`, `/tools`, `/model`, `/debug`, `/ping` | Direct response, no LLM |
| `Undo` / `Redo` | `/undo`, `/redo` | Roll back / replay turns |
| `Interrupt` | `/interrupt`, `/stop` | Cancel current turn |
| `Compact` | `/compact` | Summarize old turns to save context |
| `Heartbeat` | `/heartbeat` | Trigger heartbeat check |
| `Quit` | `/quit`, `/exit` | Shutdown signal |
| `NewThread` | `/thread new`, `/new` | Create fresh conversation thread |
| `SwitchThread` | `/thread <uuid>` | Switch to existing thread |
| `ApprovalResponse` | `y`, `n`, `a` (yes/no/always) | Tool approval gate |

### Session Resolution

`SessionManager` maps external identifiers to internal UUIDs:

```
ThreadKey { user_id, channel, external_thread_id }
    -> lookup thread_map (RwLock<HashMap>)
    -> if found: return (Session, thread_id)
    -> if not: create Session + Thread, store mapping, return
```

Sessions contain threads, which contain turns (request/response pairs). Each thread maintains its own conversation history and undo stack.

---

## Tool Execution Flow

The Worker runs an iterative LLM reasoning loop. Each iteration may produce tool calls that need execution and safety processing.

```mermaid
sequenceDiagram
    participant W as Worker / Reasoning
    participant LLM as LlmProvider
    participant TR as ToolRegistry
    participant AG as Approval Gate
    participant T as Tool.execute()
    participant SL as SafetyLayer
    participant DB as Database

    W->>LLM: chat(messages, tool_definitions)
    LLM-->>W: RespondResult (text + tool_calls[])

    loop For each tool_call
        W->>TR: get(tool_name)
        TR-->>W: Arc<dyn Tool>

        alt requires_approval() == true
            W->>AG: Check session auto_approved_tools
            alt Auto-approved or user approves
                AG-->>W: Approved
            else User denies
                AG-->>W: Denied
                W-->>W: Skip tool, add denial to messages
            end
        end

        W->>T: execute(params, JobContext)
        T-->>W: ToolOutput {result, cost, duration}

        W->>SL: sanitize_tool_output(tool_name, output)
        SL-->>W: SanitizedOutput {content, warnings, was_modified}

        W->>W: Append tool_result to messages
        Note over W: Record action in JobContext

        opt Database available
            W->>DB: log_llm_call(), log_action()
        end
    end

    W->>LLM: chat(messages + tool_results)
    Note over W,LLM: Loop continues until LLM responds without tool calls
```

### Tool Types

All three tool types implement the same `Tool` trait:

| Type | Location | Sandbox | Examples |
|------|----------|---------|----------|
| **Built-in** (Rust) | `src/tools/builtin/` | In-process | shell, read_file, write_file, memory_search |
| **WASM** (sandboxed) | `~/.ironclaw/tools/` | Fuel-limited WASM | gmail, slack, google-calendar |
| **MCP** (external) | HTTP transport | External process | Any MCP-compatible server |

### Approval Gate

Tools with `requires_approval() = true` (shell, http, write_file, apply_patch, build_software) are gated:

1. Check if tool is in `session.auto_approved_tools` -- if so, skip prompt
2. Send `StatusUpdate::ApprovalNeeded` to channel
3. Thread state transitions to `WaitingForApproval`
4. Next user input is parsed as `ApprovalResponse` (y/n/a)
5. "Always approve" (`a`) adds tool to session's `auto_approved_tools` set

---

## LLM Request Flow

The LLM subsystem supports 8 backends with automatic failover.

```mermaid
sequenceDiagram
    participant W as Worker / Reasoning
    participant FP as FailoverProvider
    participant P1 as Primary Provider
    participant P2 as Fallback Provider
    participant API as External API

    W->>FP: chat(CompletionRequest) or complete_with_tools(ToolCompletionRequest)

    FP->>FP: Select available provider (priority + cooldown check)

    FP->>P1: Forward request
    P1->>API: Provider-specific HTTP call

    alt Success
        API-->>P1: Response (streaming or batch)
        P1-->>FP: CompletionResponse {content, usage, cost}
        FP->>FP: record_success() -- reset cooldown
        FP-->>W: Response
    else Error (rate limit, timeout, API error)
        P1-->>FP: LlmError
        FP->>FP: record_failure() -- exponential backoff cooldown
        FP->>P2: Retry with next available provider
        P2->>API: Provider-specific HTTP call
        API-->>P2: Response
        P2-->>FP: CompletionResponse
        FP-->>W: Response
    end
```

### Provider Selection

```
For each request:
    1. Sort providers by priority (lower = higher priority)
    2. Filter out providers in cooldown (cooldown_until > now)
    3. Try first available provider
    4. On failure: cooldown = base_cooldown * 2^(consecutive_failures - 1), capped at 5min
    5. Try next provider
    6. After max_retries (default 3), return error
```

### Supported Backends

| Backend | Auth | Transport |
|---------|------|-----------|
| NEAR AI (Responses API) | Session-based | HTTP |
| NEAR AI (Chat Completions) | API key | HTTP |
| OpenAI | API key | rig-core adapter |
| Anthropic | API key | rig-core adapter |
| Google Gemini | API key | Custom HTTP |
| AWS Bedrock | SigV4 | Custom HTTP |
| Ollama | None (local) | rig-core adapter |
| OpenRouter | API key | Custom HTTP |

---

## Safety Pipeline Flow

All external tool output passes through the `SafetyLayer` before reaching the LLM context.

```mermaid
flowchart TD
    A[Tool Output raw string] --> B{Length > max_output_length?}
    B -->|Yes| C[Truncate with warning]
    B -->|No| D[LeakDetector.scan_and_clean]

    D -->|Secrets found| E{Severity}
    E -->|Redactable| F[Redact secrets, set was_modified=true]
    E -->|Critical| G[Block entire output]
    D -->|Clean| H[Sanitizer.sanitize]

    F --> H
    H --> I[Strip invisible Unicode chars]
    I --> J[Normalize confusables/homoglyphs]
    J --> K[Decode HTML entities]
    K --> L[Detect injection patterns via Aho-Corasick + regex]
    L --> M{Injection detected?}

    M -->|Yes, High severity| N[Wrap in XML safety markers + warnings]
    M -->|Yes, Low severity| O[Log warning, pass through]
    M -->|No| P[Pass through]

    N --> Q[Validator.validate]
    O --> Q
    P --> Q

    Q --> R[Policy.check - ACL enforcement]
    R -->|PolicyAction::Block| S[Block output]
    R -->|PolicyAction::Warn| T[Attach warnings]
    R -->|PolicyAction::Allow| U[SanitizedOutput to LLM context]

    C --> U
    G --> V[Blocked output placeholder to LLM context]
    S --> V
    T --> U
```

### Safety Components

| Component | File | Purpose |
|-----------|------|---------|
| `Sanitizer` | `safety/sanitizer.rs` | Prompt injection detection via pattern matching (Aho-Corasick), Unicode normalization, HTML entity decoding |
| `Validator` | `safety/validator.rs` | Content validation rules |
| `Policy` | `safety/policy.rs` | ACL enforcement, block/warn/allow actions |
| `LeakDetector` | `safety/leak_detector.rs` | Secret exfiltration scanning and redaction |
| `LogRedactor` | `safety/log_redaction.rs` | Credential redaction in logs |
| `OAuthFlowManager` | `safety/oauth.rs` | OAuth 2.0/2.1 + PKCE flow management |
| `GroupPolicyManager` | `safety/group_policies.rs` | Per-group ACL policies |

---

## Channel I/O Flow

Channels are pluggable input/output adapters implementing the `Channel` trait.

```mermaid
flowchart LR
    subgraph Channels
        REPL[REPL - stdin/stdout]
        HTTP[HTTP - REST API]
        GW[Web Gateway - SSE + REST]
        WASM[WASM Channels - Telegram/Slack/WhatsApp]
        WH[Webhook Server - inbound webhooks]
    end

    subgraph ChannelManager
        SA[select_all - merged MessageStream]
    end

    subgraph Agent
        AL[Agent Loop]
    end

    REPL -->|MessageStream| SA
    HTTP -->|MessageStream| SA
    GW -->|MessageStream| SA
    WASM -->|MessageStream| SA
    WH -->|MessageStream| SA

    SA -->|IncomingMessage| AL

    AL -->|OutgoingResponse| CM{ChannelManager.respond}
    CM -->|route by msg.channel| REPL
    CM -->|route by msg.channel| HTTP
    CM -->|route by msg.channel| GW
    CM -->|route by msg.channel| WASM

    AL -->|StatusUpdate| SS{ChannelManager.send_status}
    SS -->|Thinking, ToolStarted, StreamChunk| GW
    SS -->|ApprovalNeeded| REPL
```

### Channel Trait

```rust
trait Channel: Send + Sync {
    fn name(&self) -> &str;
    async fn start(&self) -> Result<MessageStream, ChannelError>;
    async fn respond(&self, msg: &IncomingMessage, response: OutgoingResponse) -> Result<()>;
    async fn send_status(&self, status: StatusUpdate, metadata: &Value) -> Result<()>;
    async fn broadcast(&self, user_id: &str, response: OutgoingResponse) -> Result<()>;
    async fn health_check(&self) -> Result<()>;
    async fn shutdown(&self) -> Result<()>;
}
```

### Message Types

| Type | Direction | Fields |
|------|-----------|--------|
| `IncomingMessage` | Channel -> Agent | id, channel, user_id, user_name, content, thread_id, received_at, metadata |
| `OutgoingResponse` | Agent -> Channel | content, thread_id, metadata |
| `StatusUpdate` | Agent -> Channel | Thinking, ToolStarted, ToolCompleted, ToolResult, StreamChunk, ApprovalNeeded, AuthRequired, JobStarted |

---

## Job Lifecycle

Jobs follow a state machine through their execution lifecycle.

```mermaid
stateDiagram-v2
    [*] --> Pending: Job created (CreateJob intent or API)

    Pending --> InProgress: Scheduler.schedule() acquires slot

    InProgress --> Completed: Worker finishes successfully
    InProgress --> Failed: Unrecoverable error / timeout
    InProgress --> Stuck: No progress for stuck_threshold

    Stuck --> InProgress: SelfRepair.repair_stuck_job()
    Stuck --> Failed: Max repair attempts exceeded

    Completed --> Submitted: Agent sends result to user
    Submitted --> Accepted: User acknowledges result

    Failed --> [*]: Terminal state

    note right of InProgress
        Worker execution loop:
        1. Build context (system prompt + history)
        2. LLM reasoning call
        3. Execute tool calls (with safety)
        4. Append results to context
        5. Repeat until LLM responds without tools
    end note

    note right of Stuck
        SelfRepair checks every repair_check_interval:
        - detect_stuck_jobs(): jobs in InProgress
          with no progress > stuck_threshold
        - detect_broken_tools(): tools with
          consecutive failures
    end note
```

### Execution Models

| Model | Environment | Communication | Tools |
|-------|-------------|---------------|-------|
| **Local** | In-process worker | Direct function calls | All registered tools |
| **Sandboxed** | Docker container with `ironclaw worker` | HTTP to orchestrator on `:50051` | Container-safe only (shell, read_file, write_file, list_dir, apply_patch) |
| **Claude Code** | Docker container with `ironclaw claude-bridge` | HTTP to orchestrator, spawns `claude` CLI | Claude's built-in tools |

### Sandboxed Worker Communication

```
Docker Container                         Host (Orchestrator)
+-----------------------------+          +-------------------------+
| ironclaw worker             |          | OrchestratorApi :50051  |
|   ProxyLlmProvider ---------|--HTTP--->| /worker/{id}/llm/complete
|   WorkerHttpClient ---------|--HTTP--->| /worker/{id}/status     |
|                             |          | /worker/{id}/events     |
|   ToolRegistry (container)  |          | /worker/{id}/job        |
|   SafetyLayer               |          | TokenStore (per-job JWT)|
+-----------------------------+          +-------------------------+
```

---

## Startup Sequence

The full agent startup from `main.rs`.

```mermaid
flowchart TD
    A[CLI Parse - clap::Parser] --> B{Command type?}

    B -->|Special commands| C[Execute and exit early]
    C --> C1[tool, config, mcp, memory, pairing, status, worker, claude-bridge, doctor, onboard, ...]

    B -->|Run default| D[Load .env + bootstrap config]
    D --> E[Config::from_env]
    E --> F[Create session manager + authenticate]
    F --> G[Initialize tracing with WebLogLayer]
    G --> H[Create CLI channel + HTTP channel]

    H --> I{--no-db?}
    I -->|Yes| J[Skip DB]
    I -->|No| K[Connect DB - PG or libSQL]
    K --> L[Run migrations]
    L --> M[Migrate disk config to DB]
    M --> N[Config::from_db - reload with DB settings]

    N --> O[Create LLM provider]
    O --> P[Create SafetyLayer]
    P --> Q[Create ToolRegistry + register built-in tools]

    Q --> R[Create embeddings provider]
    R --> S[Create Workspace + register memory tools]
    S --> T[Register builder tool if enabled]
    T --> U[Create SecretsStore with master key]

    U --> V[Load WASM tools + MCP servers concurrently]
    V --> V1[WasmToolLoader.load_from_dir]
    V --> V2[McpClient.connect for each server]

    V1 --> W[Create WASM channel runtime]
    V2 --> W
    W --> X[Initialize WASM channels - Telegram/Slack/WhatsApp]
    X --> Y[Create Web Gateway if enabled]

    Y --> Z[Build AgentDeps + ChannelManager]
    Z --> AA[Create Agent]

    AA --> AB[Agent.run - enter message loop]
    AB --> AC[Spawn background tasks]
    AC --> AC1[Self-repair task]
    AC --> AC2[Session pruning task]
    AC --> AC3[Heartbeat task]
    AC --> AC4[Routine engine + cron ticker]
    AC --> AC5[Config reload watcher]

    AC1 --> AD[Main message loop - select on ctrl_c + message_stream]
    AC2 --> AD
    AC3 --> AD
    AC4 --> AD
    AC5 --> AD
```

### Configuration Priority

```
Environment variables (highest)
    > Database settings table (per-agent)
    > Bootstrap config (~/.ironclaw/bootstrap.json)
    > Compiled defaults (lowest)
```

---

## WASM Tool/Channel Flow

WASM components run in sandboxed wasmtime instances with fuel-limited execution.

```mermaid
sequenceDiagram
    participant L as Loader
    participant E as wasmtime::Engine
    participant C as wasmtime::Component
    participant LK as Linker
    participant S as Store (with fuel)
    participant W as WASM Component
    participant H as Host Functions

    Note over L: Tool loading (startup)
    L->>L: Read .wasm file + capabilities.json
    L->>E: Engine::new(wasmtime::Config with fuel)
    L->>C: Component::from_binary(engine, wasm_bytes)
    L->>LK: Linker::new(engine)
    LK->>LK: Bind host functions (HTTP, secrets, logging)
    L->>S: Store::new(engine, host_state)
    S->>S: set_fuel(resource_limits.max_fuel)

    Note over L: Tool execution (runtime)
    L->>W: call execute(params_json)
    W->>H: host_http_request(url, method, headers, body)
    H-->>W: HTTP response
    W->>H: host_get_secret(key)
    H-->>W: Decrypted credential value
    W-->>L: ToolOutput JSON
    L->>L: Check remaining fuel

    Note over L: Fuel exhaustion = ToolError::Timeout
```

### WASM Channel Lifecycle

```
1. WasmChannelLoader scans channels-src/ build artifacts
2. For each .wasm + capabilities.json:
   a. Create WasmChannelRuntime with shared Engine
   b. Bind WIT interface (wit/channel.wit) host functions
   c. Call channel.start() -> MessageStream
   d. Register HTTP endpoints in WasmChannelRouter (axum)
3. Router mounts /channels/{name}/webhook for inbound webhooks
4. Messages flow through SharedWasmChannel -> ChannelManager
```

---

## Workspace/Memory Flow

The Workspace provides persistent, searchable memory using a filesystem-like API backed by the database.

### Write Path

```mermaid
sequenceDiagram
    participant T as Tool (memory_write)
    participant WS as Workspace
    participant CH as Chunker
    participant EM as EmbeddingProvider
    participant DB as Database

    T->>WS: write(user_id, path, content)
    WS->>DB: get_or_create_document_by_path(user_id, agent_id, path)
    DB-->>WS: MemoryDocument {id, path, ...}

    WS->>DB: update_document_content(doc_id, content)

    WS->>CH: chunk_document(content, ChunkConfig)
    CH-->>WS: Vec<(chunk_text, offset, size)>

    WS->>DB: delete_chunks_for_document(doc_id)

    loop For each chunk
        opt Embeddings enabled
            WS->>EM: embed(chunk_text)
            EM-->>WS: Vec<f32> (1536-dim or 3072-dim)
        end
        WS->>DB: insert_chunk(doc_id, chunk_text, embedding, offset)
    end

    WS-->>T: Ok(())
```

### Read Path (Hybrid Search)

```mermaid
sequenceDiagram
    participant T as Tool (memory_search)
    participant WS as Workspace
    participant EM as EmbeddingProvider
    participant DB as Database

    T->>WS: search(user_id, query, SearchConfig)

    par Full-Text Search
        WS->>DB: search_fts(query, limit)
        DB-->>WS: Vec<SearchResult> with FTS scores
    and Vector Search (if embeddings available)
        WS->>EM: embed(query)
        EM-->>WS: query_embedding Vec<f32>
        WS->>DB: search_vector(query_embedding, limit)
        DB-->>WS: Vec<SearchResult> with cosine similarity
    end

    WS->>WS: reciprocal_rank_fusion(fts_results, vector_results)
    Note over WS: RRF score = sum(1 / (k + rank)) across both result sets

    WS-->>T: Vec<RankedResult> sorted by fused score
```

### Storage Details by Backend

| Feature | PostgreSQL | libSQL |
|---------|-----------|--------|
| Full-text search | `tsvector` + `ts_rank` | FTS5 virtual tables |
| Vector search | `pgvector` extension (`VECTOR(1536)`) | `F32_BLOB(1536)` with cosine distance |
| UUID storage | Native `UUID` type | `TEXT` (string representation) |
| Timestamps | `TIMESTAMPTZ` | `TEXT` (ISO-8601) |
| JSON | `JSONB` | `TEXT` (serialized) |

### Memory Features

- **Documents**: Markdown files in a hierarchical path structure
- **Chunks**: Documents split into overlapping chunks for granular search
- **Connections**: Typed relationships between documents (updates, extends, derives) forming a knowledge graph
- **Spaces**: Named collections for organizing memories by topic/project
- **Profiles**: Auto-maintained fact profiles (static/dynamic) for personalization
- **Identity files**: IDENTITY.md, SOUL.md, AGENTS.md, USER.md injected into LLM system prompts

---

## Hook Execution Flow

Lifecycle hooks intercept events at various points in the processing pipeline.

```mermaid
sequenceDiagram
    participant Trigger as Event Source
    participant HE as HookEngine
    participant H as Hook (sorted by priority)
    participant A as Action (Shell/HTTP/Inline/Webhook)

    Trigger->>HE: run_before_inbound(content, sender, ctx)
    Note over HE: Or: run_before_outbound, run_before_tool_call,<br/>run_on_session_start, run_transform_response, etc.

    HE->>HE: Lookup hooks for HookType, filter enabled

    loop For each enabled hook (sorted by priority)
        HE->>H: Check filter conditions (sender, channel, pattern)

        alt Filter matches
            HE->>A: Execute action

            alt Shell action
                A->>A: tokio::process::Command with timeout
                A-->>HE: stdout as result
            else HTTP action
                A->>A: reqwest POST/GET to URL
                A-->>HE: response body as result
            else Inline action
                A->>A: Evaluate Rust closure
                A-->>HE: HookOutcome
            else Webhook action
                A->>A: POST with HMAC-SHA256 signature + retry
                A-->>HE: delivery status
            end

            alt HookOutcome::Block
                HE-->>Trigger: Blocked (with reason)
                Note over HE: Short-circuit, skip remaining hooks
            else HookOutcome::Modify
                HE->>HE: Replace content with modified version
                Note over HE: Continue to next hook with modified content
            else HookOutcome::Continue
                Note over HE: Continue to next hook
            end
        else Filter does not match
            Note over HE: Skip hook
        end
    end

    HE-->>Trigger: Final result (allow/block + modified content)
```

### Hook Types

| Hook Type | Trigger Point | Can Modify | Can Block |
|-----------|---------------|------------|-----------|
| `beforeInbound` | Before message enters agent loop | Content | Yes |
| `beforeOutbound` | Before response sent to channel | Content | Yes |
| `beforeToolCall` | Before tool execution | Parameters | Yes |
| `onSessionStart` | New session created | No | No |
| `onSessionEnd` | Session closed | No | No |
| `transformResponse` | After LLM response, before delivery | Content | No |
| `transcribeAudio` | Audio input received | Transcription | No |

### Bundled Hooks

8 built-in hooks: `profanity_filter`, `rate_limit_guard`, `sensitive_data_redactor`, `audit_logger`, `input_validator`, `response_formatter`, `cost_tracker`, `notification_forwarder`.

### Outbound Webhooks

Outbound webhook actions sign payloads with HMAC-SHA256 and include automatic retry with exponential backoff. The payload includes the event type, content, timestamp, and hook metadata.
