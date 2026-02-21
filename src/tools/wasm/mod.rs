//! WASM sandbox for untrusted tool execution.
//!
//! This module provides Wasmtime-based sandboxed execution for tools,
//! following patterns from NEAR blockchain and modern WASM best practices:
//!
//! - **Compile once, instantiate fresh**: Tools are validated and compiled
//!   at registration time. Each execution creates a fresh instance.
//!
//! - **Fuel metering**: CPU usage is limited via Wasmtime's fuel system.
//!
//! - **Memory limits**: Memory growth is bounded via ResourceLimiter.
//!
//! - **Extended host API (V2)**: log, time, workspace, HTTP, tool invoke, secrets
//!
//! - **Capability-based security**: Features are opt-in via Capabilities.
//!
//! # Architecture (V2)
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │                              WASM Tool Execution                             │
//! │                                                                              │
//! │   WASM Tool ──▶ Host Function ──▶ Allowlist ──▶ Credential ──▶ Execute     │
//! │   (untrusted)   (boundary)        Validator     Injector       Request      │
//! │                                                                    │        │
//! │                                                                    ▼        │
//! │                              ◀────── Leak Detector ◀────── Response        │
//! │                          (sanitized, no secrets)                            │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Security Constraints
//!
//! | Threat | Mitigation |
//! |--------|------------|
//! | CPU exhaustion | Fuel metering |
//! | Memory exhaustion | ResourceLimiter, 10MB default |
//! | Infinite loops | Epoch interruption + tokio timeout |
//! | Filesystem access | No WASI FS, only host workspace_read |
//! | Network access | Allowlisted endpoints only |
//! | Credential exposure | Injection at host boundary only |
//! | Secret exfiltration | Leak detector scans all outputs |
//! | Log spam | Max 1000 entries, 4KB per message |
//! | Path traversal | Validate paths (no `..`, no `/` prefix) |
//! | Trap recovery | Discard instance, never reuse |
//! | Side channels | Fresh instance per execution |
//! | Rate abuse | Per-tool rate limiting |
//! | WASM tampering | BLAKE3 hash verification on load |
//! | Direct tool access | Tool aliasing (indirection layer) |
//!
//! # Example
//!
//! ```ignore
//! use ironclaw::tools::wasm::{WasmToolRuntime, WasmRuntimeConfig, WasmToolWrapper};
//! use ironclaw::tools::wasm::Capabilities;
//! use std::sync::Arc;
//!
//! // Create runtime
//! let runtime = Arc::new(WasmToolRuntime::new(WasmRuntimeConfig::default())?);
//!
//! // Prepare a tool from WASM bytes
//! let wasm_bytes = std::fs::read("my_tool.wasm")?;
//! let prepared = runtime.prepare("my_tool", &wasm_bytes, None).await?;
//!
//! // Create wrapper with HTTP capability
//! let capabilities = Capabilities::none()
//!     .with_http(HttpCapability::new(vec![
//!         EndpointPattern::host("api.openai.com").with_path_prefix("/v1/"),
//!     ]));
//! let tool = WasmToolWrapper::new(runtime, prepared, capabilities);
//!
//! // Execute (implements Tool trait)
//! let output = tool.execute(serde_json::json!({"input": "test"}), &ctx).await?;
//! ```

mod allowlist;
mod capabilities;
mod capabilities_schema;
mod credential_injector;
mod error;
mod host;
mod limits;
mod loader;
mod rate_limiter;
mod runtime;
mod storage;
mod wrapper;

// Core types
pub use error::{TrapCode, TrapInfo, WasmError};
pub use host::{HostState, LogEntry, LogLevel};
pub use limits::{
    DEFAULT_FUEL_LIMIT, DEFAULT_MEMORY_LIMIT, DEFAULT_TIMEOUT, FuelConfig, ResourceLimits,
    WasmResourceLimiter,
};
pub use runtime::{PreparedModule, WasmRuntimeConfig, WasmToolRuntime};
pub use wrapper::WasmToolWrapper;

// Capabilities (V2)
pub use capabilities::{
    Capabilities, EndpointPattern, HttpCapability, RateLimitConfig, SecretsCapability,
    ToolInvokeCapability, WorkspaceCapability, WorkspaceReader,
};

// Security components (V2)
pub use allowlist::{AllowlistResult, AllowlistValidator, DenyReason};
pub use credential_injector::{CredentialInjector, InjectedCredentials, InjectionError};
pub use rate_limiter::{LimitType, RateLimitError, RateLimitResult, RateLimiter};

// Storage (V2)
#[cfg(feature = "libsql")]
pub use storage::LibSqlWasmToolStore;
#[cfg(feature = "postgres")]
pub use storage::PostgresWasmToolStore;
pub use storage::{
    StoreToolParams, StoredCapabilities, StoredWasmTool, StoredWasmToolWithBinary, ToolStatus,
    TrustLevel, WasmStorageError, WasmToolStore, compute_binary_hash, verify_binary_integrity,
};

// Loader
pub use loader::{
    DiscoveredTool, LoadResults, WasmLoadError, WasmToolLoader, discover_dev_tools, discover_tools,
    load_dev_tools,
};

// Capabilities schema (for parsing *.capabilities.json files)
pub use capabilities_schema::{
    AuthCapabilitySchema, CapabilitiesFile, OAuthConfigSchema, RateLimitSchema,
    ValidationEndpointSchema,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_fuel_limit() {
        assert!(DEFAULT_FUEL_LIMIT > 0);
    }

    #[test]
    fn test_default_memory_limit() {
        assert!(DEFAULT_MEMORY_LIMIT > 0);
    }

    #[test]
    fn test_default_timeout() {
        assert!(DEFAULT_TIMEOUT > std::time::Duration::ZERO);
    }

    #[test]
    fn test_wasm_runtime_config_default() {
        let config = WasmRuntimeConfig::default();
        // Should have sensible defaults
        assert!(config.default_limits.fuel > 0);
        assert!(config.default_limits.memory_bytes > 0);
        assert!(config.cache_compiled);
    }

    #[test]
    fn test_capabilities_none() {
        let caps = Capabilities::none();
        assert!(caps.http.is_none());
        assert!(caps.workspace_read.is_none());
        assert!(caps.secrets.is_none());
        assert!(caps.tool_invoke.is_none());
    }

    #[test]
    fn test_endpoint_pattern_host() {
        let pattern = EndpointPattern::host("api.example.com");
        assert_eq!(pattern.host, "api.example.com");
    }

    #[test]
    fn test_endpoint_pattern_with_path_prefix() {
        let pattern = EndpointPattern::host("api.example.com").with_path_prefix("/v1/");
        assert_eq!(pattern.host, "api.example.com");
        assert_eq!(pattern.path_prefix.as_deref(), Some("/v1/"));
    }

    #[test]
    fn test_http_capability_new() {
        let cap = HttpCapability::new(vec![
            EndpointPattern::host("api.openai.com"),
        ]);
        assert_eq!(cap.allowlist.len(), 1);
    }

    #[test]
    fn test_capabilities_with_http() {
        let caps = Capabilities::none().with_http(HttpCapability::new(vec![
            EndpointPattern::host("example.com"),
        ]));
        assert!(caps.http.is_some());
        assert_eq!(caps.http.unwrap().allowlist.len(), 1);
    }

    #[test]
    fn test_log_level_variants() {
        // Verify log levels can be constructed
        let _debug = LogLevel::Debug;
        let _info = LogLevel::Info;
        let _warn = LogLevel::Warn;
        let _error = LogLevel::Error;
    }

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert!(limits.fuel > 0);
        assert!(limits.memory_bytes > 0);
    }

    #[test]
    fn test_allowlist_validator_creation() {
        let validator = AllowlistValidator::new(vec![
            EndpointPattern::host("api.example.com"),
        ]);
        // Should not panic
        drop(validator);
    }

    #[test]
    fn test_tool_status_variants() {
        let _active = ToolStatus::Active;
        let _disabled = ToolStatus::Disabled;
        let _quarantined = ToolStatus::Quarantined;
    }

    #[test]
    fn test_trust_level_variants() {
        let _system = TrustLevel::System;
        let _verified = TrustLevel::Verified;
        let _user = TrustLevel::User;
    }

    #[test]
    fn test_compute_binary_hash() {
        let hash1 = compute_binary_hash(b"hello");
        let hash2 = compute_binary_hash(b"hello");
        let hash3 = compute_binary_hash(b"world");
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert!(!hash1.is_empty());
    }

    #[test]
    fn test_verify_binary_integrity() {
        let data = b"test data";
        let hash = compute_binary_hash(data);
        assert!(verify_binary_integrity(data, &hash));
        assert!(!verify_binary_integrity(b"other data", &hash));
    }
}
