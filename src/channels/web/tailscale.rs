//! Tailscale integration for the web gateway.
//!
//! Detects Tailscale presence, retrieves device identity, and verifies
//! incoming connections against Tailscale-authenticated identities.
//! Uses the local Tailscale API socket at `/var/run/tailscale/tailscaled.sock`
//! (Linux) or the HTTP API on macOS.

use serde::{Deserialize, Serialize};

/// Tailscale integration manager.
pub struct TailscaleIntegration {
    client: reqwest::Client,
    api_base: String,
}

/// Tailscale device status information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailscaleStatus {
    /// Whether Tailscale is running and connected.
    pub connected: bool,
    /// The device's Tailscale hostname.
    pub hostname: Option<String>,
    /// The device's Tailscale IP (v4).
    pub tailscale_ip: Option<String>,
    /// The tailnet name (e.g., "user@example.com").
    pub tailnet: Option<String>,
    /// Whether this node is online.
    pub online: bool,
}

/// Tailscale identity of a connecting peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailscaleIdentity {
    /// Tailscale node ID.
    pub node_id: String,
    /// User login (email).
    pub user_login: String,
    /// Display name.
    pub display_name: String,
    /// Tailscale IP addresses.
    pub addresses: Vec<String>,
    /// Whether the connection is authenticated via Tailscale.
    pub authenticated: bool,
}

/// Raw status response from the Tailscale local API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TailscaleStatusResponse {
    #[serde(rename = "Self")]
    self_node: Option<TailscaleNode>,
    #[allow(dead_code)]
    tailnet_name: Option<String>,
    #[allow(dead_code)]
    peer: Option<std::collections::HashMap<String, TailscalePeer>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TailscaleNode {
    host_name: Option<String>,
    #[serde(rename = "TailscaleIPs")]
    tailscale_ips: Option<Vec<String>>,
    online: Option<bool>,
    #[allow(dead_code)]
    user_id: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
struct TailscalePeer {
    host_name: Option<String>,
    #[serde(rename = "TailscaleIPs")]
    tailscale_ips: Option<Vec<String>>,
    online: Option<bool>,
    user_id: Option<u64>,
}

/// WhoIs response from the Tailscale local API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TailscaleWhoIsResponse {
    node: Option<TailscaleWhoIsNode>,
    user_profile: Option<TailscaleUserProfile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TailscaleWhoIsNode {
    #[serde(rename = "ID")]
    id: Option<String>,
    #[serde(rename = "TailscaleIPs")]
    tailscale_ips: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TailscaleUserProfile {
    login_name: Option<String>,
    display_name: Option<String>,
}

impl TailscaleIntegration {
    /// Create a new Tailscale integration using the local API.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            api_base: "http://127.0.0.1:41112".to_string(), // Tailscale local API
        }
    }

    /// Create with a custom API base URL.
    pub fn with_api_base(api_base: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_base: api_base.into(),
        }
    }

    /// Check if Tailscale is available on this machine.
    pub async fn is_available(&self) -> bool {
        self.get_status().await.is_ok()
    }

    /// Get the current Tailscale status.
    pub async fn get_status(&self) -> Result<TailscaleStatus, String> {
        let url = format!("{}/localapi/v0/status", self.api_base);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to Tailscale: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Tailscale API error: {}", response.status()));
        }

        let status: TailscaleStatusResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Tailscale status: {}", e))?;

        let self_node = status.self_node;
        let connected = self_node.is_some();
        let (hostname, tailscale_ip, online) = match self_node {
            Some(node) => (
                node.host_name,
                node.tailscale_ips.and_then(|ips| ips.into_iter().next()),
                node.online.unwrap_or(false),
            ),
            None => (None, None, false),
        };

        Ok(TailscaleStatus {
            connected,
            hostname,
            tailscale_ip,
            tailnet: status.tailnet_name,
            online,
        })
    }

    /// Identify a peer by their IP address using Tailscale's WhoIs API.
    ///
    /// This verifies that a connection from a given IP is actually
    /// authenticated through Tailscale and returns their identity.
    pub async fn identify_peer(&self, remote_addr: &str) -> Result<TailscaleIdentity, String> {
        let url = format!("{}/localapi/v0/whois?addr={}", self.api_base, remote_addr);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to query Tailscale WhoIs: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Tailscale WhoIs failed for {}: {}",
                remote_addr,
                response.status()
            ));
        }

        let whois: TailscaleWhoIsResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse WhoIs response: {}", e))?;

        let node = whois.node.ok_or("No node in WhoIs response")?;
        let profile = whois
            .user_profile
            .ok_or("No user profile in WhoIs response")?;

        Ok(TailscaleIdentity {
            node_id: node.id.unwrap_or_default(),
            user_login: profile.login_name.unwrap_or_default(),
            display_name: profile.display_name.unwrap_or_default(),
            addresses: node.tailscale_ips.unwrap_or_default(),
            authenticated: true,
        })
    }
}

impl Default for TailscaleIntegration {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tailscale_status_default() {
        let status = TailscaleStatus {
            connected: false,
            hostname: None,
            tailscale_ip: None,
            tailnet: None,
            online: false,
        };
        assert!(!status.connected);
        assert!(!status.online);
    }

    #[test]
    fn test_tailscale_identity_serialization() {
        let identity = TailscaleIdentity {
            node_id: "n12345".to_string(),
            user_login: "user@example.com".to_string(),
            display_name: "Test User".to_string(),
            addresses: vec!["100.64.0.1".to_string()],
            authenticated: true,
        };
        let json = serde_json::to_string(&identity).unwrap();
        assert!(json.contains("user@example.com"));
        assert!(json.contains("100.64.0.1"));
    }

    #[test]
    fn test_default_api_base() {
        let ts = TailscaleIntegration::new();
        assert_eq!(ts.api_base, "http://127.0.0.1:41112");
    }

    #[test]
    fn test_custom_api_base() {
        let ts = TailscaleIntegration::with_api_base("http://localhost:9999");
        assert_eq!(ts.api_base, "http://localhost:9999");
    }
}
