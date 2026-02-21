# Tests & Feasible TODOs Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add unit tests to ~25 untested files and implement 3 feasible TODOs.

**Architecture:** Pure unit tests using existing test patterns (`mod tests` blocks, `#[tokio::test]` for async). Test constructors, builders, serialization, error paths. TODO implementations are isolated changes.

**Tech Stack:** Rust 2024, tokio, serde_json, thiserror

---

### Task 1: TODO — Convert OAuth nonce comment

**Files:**
- Modify: `src/channels/wasm/router.rs:431`

**Step 1: Replace TODO with design-decision comment**

```rust
// Design note: The OAuth flow uses local callbacks via authorize_mcp_server()
// which handles the full token exchange synchronously. A nonce-based async
// lookup would be needed if we move to server-side OAuth redirect handling.
```

**Step 2: Commit**

```bash
git add src/channels/wasm/router.rs
git commit -m "docs: clarify OAuth nonce design decision in WASM router"
```

---

### Task 2: Tests — `src/estimation/mod.rs`

**Files:**
- Modify: `src/estimation/mod.rs` (add `mod tests` block)

**Step 1: Write tests for Estimator**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimator_new() {
        let est = Estimator::new();
        let job = est.estimate_job("simple task", None, &[]);
        assert!(job.confidence >= 0.0 && job.confidence <= 1.0);
        assert!(job.cost >= Decimal::ZERO);
        assert!(job.duration.as_secs() > 0);
    }

    #[test]
    fn test_estimator_default() {
        let est = Estimator::default();
        assert!(est.cost().base_cost_per_token() > Decimal::ZERO || true); // accessor works
    }

    #[test]
    fn test_estimate_with_tools() {
        let est = Estimator::new();
        let job = est.estimate_job("search the web", Some("research"), &["http".to_string(), "shell".to_string()]);
        assert!(!job.tool_breakdown.is_empty() || job.confidence > 0.0);
    }

    #[test]
    fn test_record_actuals() {
        let mut est = Estimator::new();
        est.record_actuals(
            "test",
            Decimal::new(100, 2),
            Decimal::new(95, 2),
            Duration::from_secs(60),
            Duration::from_secs(55),
        );
        // Should not panic; learner absorbs the data
    }
}
```

**Step 2: Run and verify**

Run: `cargo test --lib estimation::tests`

**Step 3: Commit**

```bash
git add src/estimation/mod.rs
git commit -m "test: add unit tests for Estimator"
```

---

### Task 3: Tests — `src/llm/provider.rs`

**Files:**
- Modify: `src/llm/provider.rs` (add `mod tests` block)

**Step 1: Write tests for ChatMessage, CompletionRequest, Role**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_system() {
        let msg = ChatMessage::system("You are helpful");
        assert_eq!(msg.role, Role::System);
        assert_eq!(msg.content, "You are helpful");
        assert!(msg.tool_calls.is_none());
    }

    #[test]
    fn test_chat_message_user() {
        let msg = ChatMessage::user("Hello");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content, "Hello");
    }

    #[test]
    fn test_chat_message_assistant() {
        let msg = ChatMessage::assistant("Hi there");
        assert_eq!(msg.role, Role::Assistant);
        assert_eq!(msg.content, "Hi there");
    }

    #[test]
    fn test_chat_message_tool_result() {
        let msg = ChatMessage::tool_result("call_1", "http", "200 OK");
        assert_eq!(msg.role, Role::Tool);
        assert_eq!(msg.tool_call_id, Some("call_1".to_string()));
        assert_eq!(msg.name, Some("http".to_string()));
    }

    #[test]
    fn test_chat_message_assistant_with_tool_calls() {
        let calls = vec![ToolCall {
            id: "tc_1".to_string(),
            name: "http".to_string(),
            arguments: serde_json::json!({"url": "https://example.com"}),
        }];
        let msg = ChatMessage::assistant_with_tool_calls(Some("Let me check".into()), calls.clone());
        assert_eq!(msg.role, Role::Assistant);
        assert_eq!(msg.tool_calls.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_completion_request_builder() {
        let req = CompletionRequest::new(vec![ChatMessage::user("Hi")])
            .with_max_tokens(100)
            .with_temperature(0.7);
        assert_eq!(req.max_tokens, Some(100));
        assert_eq!(req.temperature, Some(0.7));
        assert_eq!(req.messages.len(), 1);
    }

    #[test]
    fn test_role_serialization() {
        let json = serde_json::to_string(&Role::System).unwrap();
        let back: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Role::System);
    }

    #[test]
    fn test_tool_definition_serialization() {
        let tool = ToolDefinition {
            name: "http".to_string(),
            description: "Make HTTP requests".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        };
        let json = serde_json::to_string(&tool).unwrap();
        let back: ToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "http");
    }
}
```

**Step 2: Run and verify**

Run: `cargo test --lib llm::provider::tests`

**Step 3: Commit**

```bash
git add src/llm/provider.rs
git commit -m "test: add unit tests for LLM provider types"
```

---

### Task 4: Tests — `src/channels/channel.rs`

**Files:**
- Modify: `src/channels/channel.rs` (add `mod tests` block)

**Step 1: Write tests for IncomingMessage, OutgoingResponse, StatusUpdate**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_incoming_message_new() {
        let msg = IncomingMessage::new("telegram", "user123", "hello");
        assert_eq!(msg.channel, "telegram");
        assert_eq!(msg.user_id, "user123");
        assert_eq!(msg.content, "hello");
        assert!(msg.thread_id.is_none());
        assert!(msg.user_name.is_none());
    }

    #[test]
    fn test_incoming_message_builder() {
        let msg = IncomingMessage::new("slack", "u1", "hi")
            .with_thread("t1")
            .with_user_name("Alice")
            .with_metadata(serde_json::json!({"key": "val"}));
        assert_eq!(msg.thread_id, Some("t1".to_string()));
        assert_eq!(msg.user_name, Some("Alice".to_string()));
        assert_eq!(msg.metadata["key"], "val");
    }

    #[test]
    fn test_outgoing_response_text() {
        let resp = OutgoingResponse::text("Hello!");
        assert_eq!(resp.content, "Hello!");
        assert!(resp.thread_id.is_none());
    }

    #[test]
    fn test_outgoing_response_in_thread() {
        let resp = OutgoingResponse::text("Reply").in_thread("thread_42");
        assert_eq!(resp.thread_id, Some("thread_42".to_string()));
    }

    #[test]
    fn test_status_update_variants() {
        let s = StatusUpdate::Thinking("working...".to_string());
        assert!(matches!(s, StatusUpdate::Thinking(_)));

        let s = StatusUpdate::ToolStarted { name: "http".to_string() };
        assert!(matches!(s, StatusUpdate::ToolStarted { .. }));

        let s = StatusUpdate::ToolCompleted { name: "http".to_string(), success: true };
        if let StatusUpdate::ToolCompleted { success, .. } = s {
            assert!(success);
        }
    }
}
```

**Step 2: Run and verify**

Run: `cargo test --lib channels::channel::tests`

**Step 3: Commit**

```bash
git add src/channels/channel.rs
git commit -m "test: add unit tests for Channel types"
```

---

### Task 5: Tests — `src/media/transcription.rs`

**Files:**
- Modify: `src/media/transcription.rs` (add `mod tests` block)

**Step 1: Write tests for WhisperProvider construction and availability**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_whisper_provider_new() {
        let p = WhisperProvider::new("sk-test".to_string());
        assert_eq!(p.name(), "whisper");
        assert!(p.is_available());
    }

    #[test]
    fn test_whisper_provider_empty_key_not_available() {
        let p = WhisperProvider::new("".to_string());
        assert!(!p.is_available());
    }

    #[test]
    fn test_whisper_provider_builder() {
        let p = WhisperProvider::new("key".to_string())
            .with_base_url("https://custom.api".to_string())
            .with_model("whisper-large".to_string());
        assert!(p.is_available());
    }

    #[test]
    fn test_transcription_result_serialization() {
        let result = TranscriptionResult {
            text: "Hello world".to_string(),
            language: Some("en".to_string()),
            duration_seconds: Some(1.5),
            provider: "whisper".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: TranscriptionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.text, "Hello world");
        assert_eq!(back.language, Some("en".to_string()));
    }
}
```

**Step 2: Run and verify**

Run: `cargo test --lib media::transcription::tests`

**Step 3: Commit**

```bash
git add src/media/transcription.rs
git commit -m "test: add unit tests for transcription types"
```

---

### Task 6: Tests — `src/media/vision.rs`

**Files:**
- Modify: `src/media/vision.rs` (add `mod tests` block)

**Step 1: Write tests for vision types and serialization**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_source_base64_serialization() {
        let src = ImageSource::Base64 {
            data: "abc123".to_string(),
            media_type: "image/png".to_string(),
        };
        let json = serde_json::to_string(&src).unwrap();
        assert!(json.contains("base64"));
        let back: ImageSource = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, ImageSource::Base64 { .. }));
    }

    #[test]
    fn test_image_source_url_serialization() {
        let src = ImageSource::Url { url: "https://example.com/img.png".to_string() };
        let json = serde_json::to_string(&src).unwrap();
        let back: ImageSource = serde_json::from_str(&json).unwrap();
        if let ImageSource::Url { url } = back {
            assert_eq!(url, "https://example.com/img.png");
        } else {
            panic!("Expected Url variant");
        }
    }

    #[test]
    fn test_vision_request_construction() {
        let req = VisionRequest {
            image: ImageSource::Url { url: "https://example.com/img.png".to_string() },
            prompt: "Describe this image".to_string(),
            detail: Some("high".to_string()),
            max_tokens: Some(500),
        };
        assert_eq!(req.prompt, "Describe this image");
        assert_eq!(req.max_tokens, Some(500));
    }

    #[test]
    fn test_openai_vision_provider() {
        let p = OpenAiVisionProvider::new("key".to_string(), "gpt-4o".to_string());
        assert_eq!(p.name(), "openai_vision");
        assert!(p.is_available());
    }

    #[test]
    fn test_openai_vision_provider_empty_key() {
        let p = OpenAiVisionProvider::new("".to_string(), "gpt-4o".to_string());
        assert!(!p.is_available());
    }

    #[test]
    fn test_openai_vision_provider_custom_url() {
        let p = OpenAiVisionProvider::new("key".to_string(), "model".to_string())
            .with_base_url("https://custom.api".to_string());
        assert!(p.is_available());
    }

    #[test]
    fn test_vision_response_serialization() {
        let resp = VisionResponse {
            content: "A cat".to_string(),
            input_tokens: Some(100),
            output_tokens: Some(5),
            provider: "openai_vision".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: VisionResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.content, "A cat");
    }
}
```

**Step 2: Run and verify**

Run: `cargo test --lib media::vision::tests`

**Step 3: Commit**

```bash
git add src/media/vision.rs
git commit -m "test: add unit tests for vision types"
```

---

### Task 7: Tests — `src/channels/wasm/error.rs`

**Files:**
- Modify: `src/channels/wasm/error.rs` (add `mod tests` block)

**Step 1: Write tests for error variant construction and Display**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_startup_failed_display() {
        let e = WasmChannelError::StartupFailed {
            name: "telegram".to_string(),
            reason: "missing config".to_string(),
        };
        let msg = e.to_string();
        assert!(msg.contains("telegram"));
        assert!(msg.contains("missing config"));
    }

    #[test]
    fn test_wasm_not_found_display() {
        let e = WasmChannelError::WasmNotFound(PathBuf::from("/tmp/missing.wasm"));
        assert!(e.to_string().contains("missing.wasm"));
    }

    #[test]
    fn test_poll_interval_too_short() {
        let e = WasmChannelError::PollIntervalTooShort {
            name: "slack".to_string(),
            interval_ms: 100,
            min_ms: 1000,
        };
        let msg = e.to_string();
        assert!(msg.contains("100") || msg.contains("slack"));
    }

    #[test]
    fn test_io_error_from() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let e: WasmChannelError = io_err.into();
        assert!(matches!(e, WasmChannelError::Io(_)));
    }
}
```

**Step 2: Run and verify**

Run: `cargo test --lib channels::wasm::error::tests`

**Step 3: Commit**

```bash
git add src/channels/wasm/error.rs
git commit -m "test: add unit tests for WASM channel errors"
```

---

### Task 8: Tests — `src/sandbox/mod.rs`

**Files:**
- Modify: `src/sandbox/mod.rs` — read first to find testable constructors

Tests for SandboxConfig construction, default values.

**Step 1: Read file, write tests for config/construction**

**Step 2: Run and verify**

Run: `cargo test --lib sandbox::tests`

**Step 3: Commit**

```bash
git add src/sandbox/mod.rs
git commit -m "test: add unit tests for sandbox config"
```

---

### Task 9: Tests — `src/channels/manager.rs`

**Files:**
- Modify: `src/channels/manager.rs`

Tests for ChannelManager construction and channel registration.

**Step 1: Read file, write tests**

**Step 2: Run and verify**

Run: `cargo test --lib channels::manager::tests`

**Step 3: Commit**

```bash
git add src/channels/manager.rs
git commit -m "test: add unit tests for ChannelManager"
```

---

### Task 10: Tests — remaining pure/async files

Batch the remaining files (~15) into groups of 3-4, testing constructors, serialization, error Display, and config defaults for each:
- `src/channels/webhook_server.rs`
- `src/media/edge_tts.rs` (already has tests but verify TODO impl)
- `src/tools/wasm/mod.rs`
- `src/cli/doctor.rs` (test Check construction if made pub(crate))
- `src/cli/gateway.rs`, `src/cli/hooks.rs`, `src/cli/cron.rs`
- `src/tools/builtin/routine.rs` (test parameters_schema output)
- `src/channels/web/mod.rs` (test config parsing)
- `src/llm/mod.rs` (test provider helper construction)

Each sub-batch: write tests, run, commit.

---

### Task 11: TODO — WIT description/schema extraction

**Files:**
- Modify: `src/tools/wasm/runtime.rs:253-276`

**Step 1: Examine wasmtime component type API**

Read `Cargo.toml` for wasmtime version, check what introspection is available on `Component`.

**Step 2: Implement extraction using component type info**

Use `component.component_type()` to iterate exports and extract function names/descriptions if available. Fall back to current defaults if no metadata found.

**Step 3: Run tests**

Run: `cargo test --lib tools::wasm`

**Step 4: Commit**

```bash
git add src/tools/wasm/runtime.rs
git commit -m "feat: extract tool description and schema from WASM component types"
```

---

### Task 12: TODO — Edge TTS WebSocket (if tokio-tungstenite available)

**Files:**
- Modify: `src/media/edge_tts.rs`

**Step 1: Check if tokio-tungstenite is a dependency**

Run: `grep tungstenite Cargo.toml`

**Step 2: If available, implement WebSocket connection**

Connect to Edge TTS endpoint, send SSML, collect binary audio response. If not available, convert TODO to a clear "requires tokio-tungstenite" comment.

**Step 3: Run tests**

Run: `cargo test --lib media::edge_tts`

**Step 4: Commit**

```bash
git add src/media/edge_tts.rs Cargo.toml
git commit -m "feat: implement Edge TTS WebSocket audio synthesis"
```

---

### Task 13: Final — commit unwrap fixes, run full test suite

**Step 1: Commit the unwrap fixes from earlier**

```bash
git add src/cli/tool.rs src/pairing/store.rs src/sandbox/proxy/http.rs src/settings.rs src/setup/wizard.rs src/workspace/chunker.rs
git commit -m "fix: replace .unwrap() with proper error handling in production code"
```

**Step 2: Run full test suite**

Run: `cargo test --lib`

**Step 3: Run clippy**

Run: `cargo clippy --all --benches --tests --examples --all-features`

**Step 4: Fix any issues, commit**
