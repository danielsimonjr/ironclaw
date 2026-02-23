//! Error types for the Docker execution sandbox.

use std::time::Duration;

/// Errors that can occur in the sandbox system.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// Docker daemon is not available or not running.
    #[error("Docker not available: {reason}")]
    DockerNotAvailable { reason: String },

    /// Failed to create container.
    #[error("Container creation failed: {reason}")]
    ContainerCreationFailed { reason: String },

    /// Failed to start container.
    #[error("Container start failed: {reason}")]
    ContainerStartFailed { reason: String },

    /// Command execution failed inside container.
    #[error("Execution failed: {reason}")]
    ExecutionFailed { reason: String },

    /// Command timed out.
    #[error("Command timed out after {0:?}")]
    Timeout(Duration),

    /// Container resource limit exceeded.
    #[error("Resource limit exceeded: {resource} limit of {limit}")]
    ResourceLimitExceeded { resource: String, limit: String },

    /// Network proxy error.
    #[error("Proxy error: {reason}")]
    ProxyError { reason: String },

    /// Network request blocked by policy.
    #[error("Network request blocked: {reason}")]
    NetworkBlocked { reason: String },

    /// Credential injection failed.
    #[error("Credential injection failed for {domain}: {reason}")]
    CredentialInjectionFailed { domain: String, reason: String },

    /// Docker API error.
    #[error("Docker API error: {0}")]
    Docker(#[from] bollard::errors::Error),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Configuration error.
    #[error("Configuration error: {reason}")]
    Config { reason: String },
}

/// Result type for sandbox operations.
pub type Result<T> = std::result::Result<T, SandboxError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_not_available_display() {
        let err = SandboxError::DockerNotAvailable {
            reason: "daemon not running".to_string(),
        };
        assert!(err.to_string().contains("daemon not running"));
        assert!(err.to_string().contains("Docker not available"));
    }

    #[test]
    fn test_container_creation_failed_display() {
        let err = SandboxError::ContainerCreationFailed {
            reason: "image not found".to_string(),
        };
        assert!(err.to_string().contains("image not found"));
    }

    #[test]
    fn test_container_start_failed_display() {
        let err = SandboxError::ContainerStartFailed {
            reason: "port conflict".to_string(),
        };
        assert!(err.to_string().contains("port conflict"));
    }

    #[test]
    fn test_execution_failed_display() {
        let err = SandboxError::ExecutionFailed {
            reason: "exit code 1".to_string(),
        };
        assert!(err.to_string().contains("exit code 1"));
    }

    #[test]
    fn test_timeout_display() {
        let err = SandboxError::Timeout(Duration::from_secs(30));
        let msg = err.to_string();
        assert!(msg.contains("timed out"));
        assert!(msg.contains("30"));
    }

    #[test]
    fn test_resource_limit_exceeded_display() {
        let err = SandboxError::ResourceLimitExceeded {
            resource: "memory".to_string(),
            limit: "512MB".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("memory"));
        assert!(msg.contains("512MB"));
    }

    #[test]
    fn test_proxy_error_display() {
        let err = SandboxError::ProxyError {
            reason: "upstream timeout".to_string(),
        };
        assert!(err.to_string().contains("upstream timeout"));
    }

    #[test]
    fn test_network_blocked_display() {
        let err = SandboxError::NetworkBlocked {
            reason: "domain not in allowlist".to_string(),
        };
        assert!(err.to_string().contains("domain not in allowlist"));
    }

    #[test]
    fn test_credential_injection_failed_display() {
        let err = SandboxError::CredentialInjectionFailed {
            domain: "api.example.com".to_string(),
            reason: "secret not found".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("api.example.com"));
        assert!(msg.contains("secret not found"));
    }

    #[test]
    fn test_config_error_display() {
        let err = SandboxError::Config {
            reason: "missing image name".to_string(),
        };
        assert!(err.to_string().contains("missing image name"));
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err = SandboxError::from(io_err);
        assert!(err.to_string().contains("access denied"));
    }

    #[test]
    fn test_debug_is_implemented() {
        let err = SandboxError::Timeout(Duration::from_secs(5));
        let debug = format!("{:?}", err);
        assert!(debug.contains("Timeout"));
    }
}
