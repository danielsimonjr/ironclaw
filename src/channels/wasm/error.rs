//! Error types for WASM channels.

use std::path::PathBuf;

/// Error during WASM channel operations.
#[derive(Debug, thiserror::Error)]
pub enum WasmChannelError {
    #[error("Channel {name} failed to start: {reason}")]
    StartupFailed { name: String, reason: String },

    #[error("Channel {name} callback failed: {reason}")]
    CallbackFailed { name: String, reason: String },

    #[error("Channel {name} WASM execution trapped: {reason}")]
    Trapped { name: String, reason: String },

    #[error("Channel {name} callback '{callback}' timed out")]
    Timeout { name: String, callback: String },

    #[error("Channel {name} execution panicked: {reason}")]
    ExecutionPanicked { name: String, reason: String },

    #[error("Channel {name} emit rate limited")]
    EmitRateLimited { name: String },

    #[error("Channel {name} HTTP path not allowed: {path}")]
    PathNotAllowed { name: String, path: String },

    #[error("Channel {name} polling interval too short: {interval_ms}ms (minimum: {min_ms}ms)")]
    PollIntervalTooShort {
        name: String,
        interval_ms: u32,
        min_ms: u32,
    },

    #[error("Channel {name} workspace path escape attempt: {path}")]
    WorkspaceEscape { name: String, path: String },

    #[error("Channel {name} exhausted fuel limit ({limit})")]
    FuelExhausted { name: String, limit: u64 },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("WASM file not found: {0}")]
    WasmNotFound(PathBuf),

    #[error("Capabilities file not found: {0}")]
    CapabilitiesNotFound(PathBuf),

    #[error("Invalid capabilities JSON: {0}")]
    InvalidCapabilities(String),

    #[error("WASM compilation error: {0}")]
    Compilation(String),

    #[error("WASM instantiation error: {0}")]
    Instantiation(String),

    #[error("Invalid channel name: {0}")]
    InvalidName(String),

    #[error("Channel {name} not found")]
    NotFound { name: String },

    #[error("Channel module missing export: {0}")]
    MissingExport(String),

    #[error("Invalid response from WASM: {0}")]
    InvalidResponse(String),

    #[error("Runtime not initialized")]
    RuntimeNotInitialized,

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Webhook registration failed for channel {name}: {reason}")]
    WebhookRegistration { name: String, reason: String },

    #[error("HTTP request error: {0}")]
    HttpRequest(String),
}

impl From<crate::tools::wasm::WasmError> for WasmChannelError {
    fn from(err: crate::tools::wasm::WasmError) -> Self {
        WasmChannelError::Compilation(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_startup_failed_display() {
        let e = WasmChannelError::StartupFailed {
            name: "telegram".into(),
            reason: "no config".into(),
        };
        assert_eq!(e.to_string(), "Channel telegram failed to start: no config");
    }

    #[test]
    fn test_wasm_not_found_display() {
        let e = WasmChannelError::WasmNotFound(PathBuf::from("/tmp/missing.wasm"));
        assert!(e.to_string().contains("missing.wasm"));
    }

    #[test]
    fn test_poll_interval_too_short_display() {
        let e = WasmChannelError::PollIntervalTooShort {
            name: "slack".into(),
            interval_ms: 100,
            min_ms: 500,
        };
        let s = e.to_string();
        assert!(s.contains("100ms"));
        assert!(s.contains("500ms"));
        assert!(s.contains("slack"));
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file gone");
        let e: WasmChannelError = io_err.into();
        assert!(matches!(e, WasmChannelError::Io(_)));
        assert!(e.to_string().contains("file gone"));
    }

    #[test]
    fn test_emit_rate_limited() {
        let e = WasmChannelError::EmitRateLimited {
            name: "test".into(),
        };
        assert!(e.to_string().contains("rate limited"));
    }
}
