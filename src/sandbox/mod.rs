//! Docker execution sandbox for secure command execution.
//!
//! This module provides a complete sandboxing solution for running untrusted commands:
//! - **Container isolation**: Commands run in ephemeral Docker containers
//! - **Network proxy**: All network traffic goes through a validating proxy
//! - **Credential injection**: Secrets are injected by the proxy, never exposed in containers
//! - **Resource limits**: Memory, CPU, and timeout enforcement
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │                           Sandbox System                                     │
//! │                                                                              │
//! │  ┌─────────────────────────────────────────────────────────────────────┐    │
//! │  │                        SandboxManager                                │    │
//! │  │                                                                      │    │
//! │  │  • Coordinates container creation and execution                     │    │
//! │  │  • Manages proxy lifecycle                                          │    │
//! │  │  • Enforces resource limits                                         │    │
//! │  └─────────────────────────────────────────────────────────────────────┘    │
//! │           │                              │                                   │
//! │           ▼                              ▼                                   │
//! │  ┌──────────────────┐          ┌───────────────────┐                        │
//! │  │   Container      │          │   Network Proxy   │                        │
//! │  │   Runner         │          │                   │                        │
//! │  │                  │          │  • Allowlist      │                        │
//! │  │  • Create        │◀────────▶│  • Credentials    │                        │
//! │  │  • Execute       │          │  • Logging        │                        │
//! │  │  • Cleanup       │          │                   │                        │
//! │  └──────────────────┘          └───────────────────┘                        │
//! │           │                              │                                   │
//! │           ▼                              ▼                                   │
//! │  ┌──────────────────┐          ┌───────────────────┐                        │
//! │  │     Docker       │          │     Internet      │                        │
//! │  │                  │          │   (allowed hosts) │                        │
//! │  └──────────────────┘          └───────────────────┘                        │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Sandbox Policies
//!
//! | Policy | Filesystem | Network | Use Case |
//! |--------|------------|---------|----------|
//! | `ReadOnly` | Read workspace | Proxied | Explore code, fetch docs |
//! | `WorkspaceWrite` | Read/write workspace | Proxied | Build software, run tests |
//! | `FullAccess` | Full host | Full | Direct execution (no sandbox) |
//!
//! # Example
//!
//! ```rust,no_run
//! use ironclaw::sandbox::{SandboxManager, SandboxManagerBuilder, SandboxPolicy};
//! use std::collections::HashMap;
//! use std::path::Path;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let manager = SandboxManagerBuilder::new()
//!     .enabled(true)
//!     .policy(SandboxPolicy::WorkspaceWrite)
//!     .build();
//!
//! manager.initialize().await?;
//!
//! let result = manager.execute(
//!     "cargo build --release",
//!     Path::new("/workspace/my-project"),
//!     HashMap::new(),
//! ).await?;
//!
//! println!("Exit code: {}", result.exit_code);
//! println!("Output: {}", result.output);
//!
//! manager.shutdown().await;
//! # Ok(())
//! # }
//! ```
//!
//! # Security Properties
//!
//! - **No credentials in containers**: Environment variables with secrets never enter containers
//! - **Network isolation**: All traffic routes through the proxy (validated domains only)
//! - **Non-root execution**: Containers run as UID 1000
//! - **Read-only root**: Container filesystem is read-only (except workspace mount)
//! - **Capability dropping**: All Linux capabilities dropped, only essential ones added back
//! - **Auto-cleanup**: Containers are removed after execution (--rm + explicit cleanup)
//! - **Timeout enforcement**: Commands are killed after the timeout

pub mod config;
pub mod container;
pub mod error;
pub mod manager;
pub mod proxy;

pub use config::{
    CredentialLocation, CredentialMapping, ResourceLimits, SandboxConfig, SandboxPolicy,
};
pub use container::{ContainerOutput, ContainerRunner, connect_docker};
pub use error::{Result, SandboxError};
pub use manager::{ExecOutput, SandboxManager, SandboxManagerBuilder};
pub use proxy::{
    CredentialResolver, DefaultPolicyDecider, DomainAllowlist, EnvCredentialResolver, HttpProxy,
    NetworkDecision, NetworkPolicyDecider, NetworkProxyBuilder, NetworkRequest,
};

/// Default allowlist getter (re-export for convenience).
pub fn default_allowlist() -> Vec<String> {
    config::default_allowlist()
}

/// Default credential mappings getter (re-export for convenience).
pub fn default_credential_mappings() -> Vec<CredentialMapping> {
    config::default_credential_mappings()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_allowlist_not_empty() {
        let list = default_allowlist();
        assert!(!list.is_empty());
    }

    #[test]
    fn test_default_allowlist_contains_key_domains() {
        let list = default_allowlist();
        assert!(list.contains(&"crates.io".to_string()));
        assert!(list.contains(&"github.com".to_string()));
        assert!(list.contains(&"api.openai.com".to_string()));
        assert!(list.contains(&"api.near.ai".to_string()));
    }

    #[test]
    fn test_default_credential_mappings_not_empty() {
        let mappings = default_credential_mappings();
        assert!(!mappings.is_empty());
    }

    #[test]
    fn test_default_credential_mappings_domains() {
        let mappings = default_credential_mappings();
        let domains: Vec<&str> = mappings.iter().map(|m| m.domain.as_str()).collect();
        assert!(domains.contains(&"api.openai.com"));
        assert!(domains.contains(&"api.anthropic.com"));
        assert!(domains.contains(&"api.near.ai"));
    }

    #[test]
    fn test_credential_mapping_default() {
        let mapping = CredentialMapping::default();
        assert!(mapping.domain.is_empty());
        assert!(mapping.secret_name.is_empty());
        assert!(matches!(mapping.location, CredentialLocation::AuthorizationBearer));
    }

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.policy, SandboxPolicy::ReadOnly);
        assert_eq!(config.timeout, std::time::Duration::from_secs(120));
        assert_eq!(config.memory_limit_mb, 2048);
        assert_eq!(config.cpu_shares, 1024);
        assert!(config.auto_pull_image);
        assert_eq!(config.proxy_port, 0);
        assert!(!config.network_allowlist.is_empty());
    }

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.memory_bytes, 2 * 1024 * 1024 * 1024);
        assert_eq!(limits.cpu_shares, 1024);
        assert_eq!(limits.timeout, std::time::Duration::from_secs(120));
        assert_eq!(limits.max_output_bytes, 64 * 1024);
    }

    #[test]
    fn test_sandbox_policy_from_str_aliases() {
        use std::str::FromStr;
        assert_eq!(SandboxPolicy::from_str("ro").unwrap(), SandboxPolicy::ReadOnly);
        assert_eq!(SandboxPolicy::from_str("rw").unwrap(), SandboxPolicy::WorkspaceWrite);
        assert_eq!(SandboxPolicy::from_str("full").unwrap(), SandboxPolicy::FullAccess);
        assert_eq!(SandboxPolicy::from_str("READONLY").unwrap(), SandboxPolicy::ReadOnly);
        assert_eq!(SandboxPolicy::from_str("read_only").unwrap(), SandboxPolicy::ReadOnly);
        assert_eq!(SandboxPolicy::from_str("workspacewrite").unwrap(), SandboxPolicy::WorkspaceWrite);
        assert_eq!(SandboxPolicy::from_str("fullaccess").unwrap(), SandboxPolicy::FullAccess);
    }

    #[test]
    fn test_sandbox_policy_none_is_ambiguous() {
        use std::str::FromStr;
        let err = SandboxPolicy::from_str("none").unwrap_err();
        assert!(err.contains("ambiguous"));
    }

    #[test]
    fn test_sandbox_policy_invalid() {
        use std::str::FromStr;
        assert!(SandboxPolicy::from_str("garbage").is_err());
    }

    #[test]
    fn test_anthropic_credential_uses_header() {
        let mappings = default_credential_mappings();
        let anthropic = mappings.iter().find(|m| m.domain == "api.anthropic.com").unwrap();
        assert!(matches!(anthropic.location, CredentialLocation::Header(ref h) if h == "x-api-key"));
    }
}
