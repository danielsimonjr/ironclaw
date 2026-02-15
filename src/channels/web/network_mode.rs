//! Network mode configuration for the web gateway.
//!
//! Controls how the gateway binds and what security policies apply based
//! on the intended network exposure level.

use serde::{Deserialize, Serialize};

/// Network exposure mode for the gateway.
///
/// Each mode implies different binding addresses, CORS policies,
/// and security posture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    /// Loopback only (127.0.0.1) — default, most secure.
    /// Only processes on the same machine can connect.
    #[default]
    Loopback,

    /// LAN mode (0.0.0.0 or specific interface) — accessible from local network.
    /// Requires authentication token. CORS allows local network origins.
    Lan,

    /// Remote mode — accessible from the internet.
    /// Requires authentication token + TLS. Strictest CORS policy.
    /// Should be used with a reverse proxy or Tailscale.
    Remote,
}

impl NetworkMode {
    /// Get the default bind address for this mode.
    pub fn default_host(&self) -> &str {
        match self {
            Self::Loopback => "127.0.0.1",
            Self::Lan => "0.0.0.0",
            Self::Remote => "0.0.0.0",
        }
    }

    /// Check if TLS should be enforced.
    pub fn requires_tls(&self) -> bool {
        matches!(self, Self::Remote)
    }

    /// Check if auth token is required (always true except loopback).
    pub fn requires_auth(&self) -> bool {
        !matches!(self, Self::Loopback)
    }

    /// Get CORS allowed origins for this mode.
    pub fn cors_origins(&self) -> Vec<String> {
        match self {
            Self::Loopback => vec![
                "http://localhost".to_string(),
                "http://127.0.0.1".to_string(),
                "http://[::1]".to_string(),
            ],
            Self::Lan => vec![
                "http://localhost".to_string(),
                "http://127.0.0.1".to_string(),
                // LAN mode allows any private IP origin
                "http://10.*".to_string(),
                "http://172.16.*".to_string(),
                "http://192.168.*".to_string(),
            ],
            Self::Remote => {
                // Remote mode: origins should be explicitly configured
                Vec::new()
            }
        }
    }

    /// Validate a bind address against this network mode.
    pub fn validate_host(&self, host: &str) -> Result<(), String> {
        match self {
            Self::Loopback => {
                if host != "127.0.0.1" && host != "::1" && host != "localhost" {
                    return Err(format!(
                        "Loopback mode requires binding to 127.0.0.1, ::1, or localhost, got '{}'",
                        host
                    ));
                }
            }
            Self::Lan | Self::Remote => {
                // Any bind address is valid for LAN and remote
            }
        }
        Ok(())
    }
}

impl std::str::FromStr for NetworkMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "loopback" | "local" | "localhost" => Ok(Self::Loopback),
            "lan" | "network" | "local_network" => Ok(Self::Lan),
            "remote" | "public" | "internet" => Ok(Self::Remote),
            _ => Err(format!(
                "Invalid network mode '{}', expected: loopback, lan, or remote",
                s
            )),
        }
    }
}

impl std::fmt::Display for NetworkMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loopback => write!(f, "loopback"),
            Self::Lan => write!(f, "lan"),
            Self::Remote => write!(f, "remote"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_loopback() {
        assert_eq!(NetworkMode::default(), NetworkMode::Loopback);
    }

    #[test]
    fn test_default_hosts() {
        assert_eq!(NetworkMode::Loopback.default_host(), "127.0.0.1");
        assert_eq!(NetworkMode::Lan.default_host(), "0.0.0.0");
        assert_eq!(NetworkMode::Remote.default_host(), "0.0.0.0");
    }

    #[test]
    fn test_requires_tls() {
        assert!(!NetworkMode::Loopback.requires_tls());
        assert!(!NetworkMode::Lan.requires_tls());
        assert!(NetworkMode::Remote.requires_tls());
    }

    #[test]
    fn test_requires_auth() {
        assert!(!NetworkMode::Loopback.requires_auth());
        assert!(NetworkMode::Lan.requires_auth());
        assert!(NetworkMode::Remote.requires_auth());
    }

    #[test]
    fn test_validate_host() {
        assert!(NetworkMode::Loopback.validate_host("127.0.0.1").is_ok());
        assert!(NetworkMode::Loopback.validate_host("0.0.0.0").is_err());
        assert!(NetworkMode::Lan.validate_host("0.0.0.0").is_ok());
        assert!(NetworkMode::Remote.validate_host("0.0.0.0").is_ok());
    }

    #[test]
    fn test_parse() {
        assert_eq!(
            "loopback".parse::<NetworkMode>().unwrap(),
            NetworkMode::Loopback
        );
        assert_eq!("lan".parse::<NetworkMode>().unwrap(), NetworkMode::Lan);
        assert_eq!(
            "remote".parse::<NetworkMode>().unwrap(),
            NetworkMode::Remote
        );
        assert_eq!(
            "local".parse::<NetworkMode>().unwrap(),
            NetworkMode::Loopback
        );
        assert!("invalid".parse::<NetworkMode>().is_err());
    }

    #[test]
    fn test_display() {
        assert_eq!(NetworkMode::Loopback.to_string(), "loopback");
        assert_eq!(NetworkMode::Lan.to_string(), "lan");
        assert_eq!(NetworkMode::Remote.to_string(), "remote");
    }

    #[test]
    fn test_cors_origins() {
        assert!(!NetworkMode::Loopback.cors_origins().is_empty());
        assert!(!NetworkMode::Lan.cors_origins().is_empty());
        assert!(NetworkMode::Remote.cors_origins().is_empty());
    }
}
