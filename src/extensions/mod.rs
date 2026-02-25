//! Unified extension system for discovering, installing, authenticating, and activating
//! MCP servers and WASM tools through conversational agent interactions.
//!
//! Extensions are the user-facing abstraction over MCP servers and WASM tools. The agent
//! can search a built-in registry (or discover online), install, authenticate, and activate
//! extensions at runtime without CLI commands.
//!
//! ```text
//!  User: "add notion"
//!    -> tool_search("notion")      -> finds MCP server in registry
//!    -> tool_install("notion")     -> saves config to mcp-servers.json
//!    -> tool_auth("notion")        -> OAuth 2.1 flow, returns URL
//!    -> tool_activate("notion")    -> connects, registers tools
//! ```

pub mod clawhub;
pub mod discovery;
pub mod manager;
pub mod plugin_manager;
pub mod plugins;
pub mod registry;

pub use discovery::OnlineDiscovery;
pub use manager::ExtensionManager;
pub use plugin_manager::{PluginError, PluginManager, PluginSnapshot, PluginSummary};
pub use registry::ExtensionRegistry;

use serde::{Deserialize, Serialize};

/// The kind of extension, determining how it's installed, authenticated, and activated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionKind {
    /// Hosted MCP server, HTTP transport, OAuth 2.1 auth.
    McpServer,
    /// Sandboxed WASM module, file-based, capabilities auth.
    WasmTool,
    /// WASM channel module (future: dynamic activation, currently needs restart).
    WasmChannel,
}

impl std::fmt::Display for ExtensionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtensionKind::McpServer => write!(f, "mcp_server"),
            ExtensionKind::WasmTool => write!(f, "wasm_tool"),
            ExtensionKind::WasmChannel => write!(f, "wasm_channel"),
        }
    }
}

/// A registry entry describing a known or discovered extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// Unique identifier (e.g., "notion", "weather", "telegram").
    pub name: String,
    /// Human-readable name (e.g., "Notion", "Weather Tool").
    pub display_name: String,
    /// What kind of extension this is.
    pub kind: ExtensionKind,
    /// Short description of what this extension does.
    pub description: String,
    /// Search keywords beyond the name.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Where to get this extension.
    pub source: ExtensionSource,
    /// How authentication works.
    pub auth_hint: AuthHint,
}

/// Where the extension binary or server lives.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExtensionSource {
    /// URL to a hosted MCP server.
    McpUrl { url: String },
    /// Downloadable WASM binary.
    WasmDownload {
        wasm_url: String,
        #[serde(default)]
        capabilities_url: Option<String>,
    },
    /// Build from source repository.
    WasmBuildable {
        repo_url: String,
        #[serde(default)]
        build_dir: Option<String>,
    },
    /// Discovered online (not yet validated for a specific source type).
    Discovered { url: String },
}

/// Hint about what authentication method is needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthHint {
    /// MCP server supports Dynamic Client Registration (zero-config OAuth).
    Dcr,
    /// MCP server needs a pre-configured OAuth client_id.
    OAuthPreConfigured {
        /// URL where the user can create an OAuth app.
        setup_url: String,
    },
    /// WASM tool has auth defined in its capabilities.json file.
    CapabilitiesAuth,
    /// No authentication needed.
    None,
}

/// Where a search result came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultSource {
    /// From the built-in curated registry.
    Registry,
    /// From online discovery (validated).
    Discovered,
}

/// Result of searching for extensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// The registry entry.
    #[serde(flatten)]
    pub entry: RegistryEntry,
    /// Where this result came from.
    pub source: ResultSource,
    /// Whether the endpoint was validated (for discovered entries).
    #[serde(default)]
    pub validated: bool,
}

/// Result of installing an extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResult {
    pub name: String,
    pub kind: ExtensionKind,
    pub message: String,
}

/// Result of authenticating an extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResult {
    pub name: String,
    pub kind: ExtensionKind,
    /// OAuth URL to open (for OAuth flows).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_url: Option<String>,
    /// Whether using local or remote callback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_type: Option<String>,
    /// Instructions for manual token entry (for WASM tools).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// URL for manual token setup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup_url: Option<String>,
    /// Whether the tool is waiting for a token from the user.
    #[serde(default)]
    pub awaiting_token: bool,
    /// Current auth status.
    pub status: String,
}

/// Result of activating an extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivateResult {
    pub name: String,
    pub kind: ExtensionKind,
    /// Names of tools that were loaded/registered.
    pub tools_loaded: Vec<String>,
    pub message: String,
}

/// An installed extension with its current status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledExtension {
    pub name: String,
    pub kind: ExtensionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Server or source URL (e.g. MCP server endpoint).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub authenticated: bool,
    pub active: bool,
    /// Tool names if active.
    #[serde(default)]
    pub tools: Vec<String>,
}

/// Error type for extension operations.
#[derive(Debug, thiserror::Error)]
pub enum ExtensionError {
    #[error("Extension not found: {0}")]
    NotFound(String),

    #[error("Extension already installed: {0}")]
    AlreadyInstalled(String),

    #[error("Extension not installed: {0}")]
    NotInstalled(String),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Activation failed: {0}")]
    ActivationFailed(String),

    #[error("Installation failed: {0}")]
    InstallFailed(String),

    #[error("Discovery failed: {0}")]
    DiscoveryFailed(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Download failed: {0}")]
    DownloadFailed(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Channels require restart to activate")]
    ChannelNeedsRestart,

    #[error("{0}")]
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // --- ExtensionKind ---

    #[test]
    fn extension_kind_display() {
        assert_eq!(ExtensionKind::McpServer.to_string(), "mcp_server");
        assert_eq!(ExtensionKind::WasmTool.to_string(), "wasm_tool");
        assert_eq!(ExtensionKind::WasmChannel.to_string(), "wasm_channel");
    }

    #[test]
    fn extension_kind_serde_roundtrip() {
        for kind in [
            ExtensionKind::McpServer,
            ExtensionKind::WasmTool,
            ExtensionKind::WasmChannel,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            let back: ExtensionKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn extension_kind_clone_copy_eq() {
        let a = ExtensionKind::McpServer;
        let b = a; // Copy
        let c = a.clone(); // Clone
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn extension_kind_hash() {
        let mut set = HashSet::new();
        set.insert(ExtensionKind::McpServer);
        set.insert(ExtensionKind::WasmTool);
        set.insert(ExtensionKind::McpServer); // duplicate
        assert_eq!(set.len(), 2);
    }

    // --- ExtensionSource ---

    #[test]
    fn extension_source_mcp_url_serde() {
        let src = ExtensionSource::McpUrl {
            url: "https://example.com".into(),
        };
        let json = serde_json::to_value(&src).unwrap();
        assert_eq!(json["type"], "mcp_url");
        assert_eq!(json["url"], "https://example.com");
        let back: ExtensionSource = serde_json::from_value(json).unwrap();
        assert!(matches!(back, ExtensionSource::McpUrl { url } if url == "https://example.com"));
    }

    #[test]
    fn extension_source_wasm_download_serde() {
        let src = ExtensionSource::WasmDownload {
            wasm_url: "https://example.com/tool.wasm".into(),
            capabilities_url: Some("https://example.com/cap.json".into()),
        };
        let json = serde_json::to_value(&src).unwrap();
        assert_eq!(json["type"], "wasm_download");
        let back: ExtensionSource = serde_json::from_value(json).unwrap();
        assert!(matches!(back, ExtensionSource::WasmDownload { capabilities_url: Some(_), .. }));
    }

    #[test]
    fn extension_source_wasm_download_optional_caps() {
        let src = ExtensionSource::WasmDownload {
            wasm_url: "https://example.com/tool.wasm".into(),
            capabilities_url: None,
        };
        let json = serde_json::to_value(&src).unwrap();
        let back: ExtensionSource = serde_json::from_value(json).unwrap();
        assert!(matches!(back, ExtensionSource::WasmDownload { capabilities_url: None, .. }));
    }

    #[test]
    fn extension_source_wasm_buildable_serde() {
        let src = ExtensionSource::WasmBuildable {
            repo_url: "https://github.com/foo/bar".into(),
            build_dir: Some("tools/my_tool".into()),
        };
        let json = serde_json::to_value(&src).unwrap();
        assert_eq!(json["type"], "wasm_buildable");
        let back: ExtensionSource = serde_json::from_value(json).unwrap();
        assert!(matches!(back, ExtensionSource::WasmBuildable { build_dir: Some(_), .. }));
    }

    #[test]
    fn extension_source_discovered_serde() {
        let src = ExtensionSource::Discovered {
            url: "https://found.example.com".into(),
        };
        let json = serde_json::to_value(&src).unwrap();
        assert_eq!(json["type"], "discovered");
        let _back: ExtensionSource = serde_json::from_value(json).unwrap();
    }

    // --- AuthHint ---

    #[test]
    fn auth_hint_dcr_serde() {
        let hint = AuthHint::Dcr;
        let json = serde_json::to_value(&hint).unwrap();
        assert_eq!(json["type"], "dcr");
        let _back: AuthHint = serde_json::from_value(json).unwrap();
    }

    #[test]
    fn auth_hint_oauth_preconfigured_serde() {
        let hint = AuthHint::OAuthPreConfigured {
            setup_url: "https://dev.example.com".into(),
        };
        let json = serde_json::to_value(&hint).unwrap();
        assert_eq!(json["type"], "o_auth_pre_configured");
        let _back: AuthHint = serde_json::from_value(json).unwrap();
    }

    #[test]
    fn auth_hint_capabilities_auth_serde() {
        let hint = AuthHint::CapabilitiesAuth;
        let json = serde_json::to_value(&hint).unwrap();
        assert_eq!(json["type"], "capabilities_auth");
        let _back: AuthHint = serde_json::from_value(json).unwrap();
    }

    #[test]
    fn auth_hint_none_serde() {
        let hint = AuthHint::None;
        let json = serde_json::to_value(&hint).unwrap();
        assert_eq!(json["type"], "none");
        let _back: AuthHint = serde_json::from_value(json).unwrap();
    }

    // --- ResultSource ---

    #[test]
    fn result_source_eq_and_serde() {
        assert_eq!(ResultSource::Registry, ResultSource::Registry);
        assert_ne!(ResultSource::Registry, ResultSource::Discovered);

        for src in [ResultSource::Registry, ResultSource::Discovered] {
            let json = serde_json::to_string(&src).unwrap();
            let back: ResultSource = serde_json::from_str(&json).unwrap();
            assert_eq!(src, back);
        }
    }

    // --- RegistryEntry ---

    fn sample_entry() -> RegistryEntry {
        RegistryEntry {
            name: "notion".into(),
            display_name: "Notion".into(),
            kind: ExtensionKind::McpServer,
            description: "Notion integration".into(),
            keywords: vec!["notes".into(), "wiki".into()],
            source: ExtensionSource::McpUrl {
                url: "https://mcp.notion.so".into(),
            },
            auth_hint: AuthHint::Dcr,
        }
    }

    #[test]
    fn registry_entry_field_access() {
        let entry = sample_entry();
        assert_eq!(entry.name, "notion");
        assert_eq!(entry.display_name, "Notion");
        assert_eq!(entry.kind, ExtensionKind::McpServer);
        assert_eq!(entry.keywords.len(), 2);
    }

    #[test]
    fn registry_entry_serde_roundtrip() {
        let entry = sample_entry();
        let json = serde_json::to_string(&entry).unwrap();
        let back: RegistryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "notion");
        assert_eq!(back.kind, ExtensionKind::McpServer);
    }

    // --- ExtensionError ---

    #[test]
    fn extension_error_display() {
        assert_eq!(
            ExtensionError::NotFound("x".into()).to_string(),
            "Extension not found: x"
        );
        assert_eq!(
            ExtensionError::AlreadyInstalled("x".into()).to_string(),
            "Extension already installed: x"
        );
        assert_eq!(
            ExtensionError::NotInstalled("x".into()).to_string(),
            "Extension not installed: x"
        );
        assert_eq!(
            ExtensionError::AuthFailed("bad".into()).to_string(),
            "Authentication failed: bad"
        );
        assert_eq!(
            ExtensionError::ActivationFailed("err".into()).to_string(),
            "Activation failed: err"
        );
        assert_eq!(
            ExtensionError::InstallFailed("err".into()).to_string(),
            "Installation failed: err"
        );
        assert_eq!(
            ExtensionError::DiscoveryFailed("err".into()).to_string(),
            "Discovery failed: err"
        );
        assert_eq!(
            ExtensionError::InvalidUrl("bad".into()).to_string(),
            "Invalid URL: bad"
        );
        assert_eq!(
            ExtensionError::DownloadFailed("err".into()).to_string(),
            "Download failed: err"
        );
        assert_eq!(
            ExtensionError::Config("err".into()).to_string(),
            "Config error: err"
        );
        assert_eq!(
            ExtensionError::ChannelNeedsRestart.to_string(),
            "Channels require restart to activate"
        );
        assert_eq!(ExtensionError::Other("misc".into()).to_string(), "misc");
    }

    #[test]
    fn extension_error_is_debug() {
        let err = ExtensionError::NotFound("test".into());
        let debug = format!("{:?}", err);
        assert!(debug.contains("NotFound"));
    }

    // --- Other structs ---

    #[test]
    fn install_result_fields() {
        let r = InstallResult {
            name: "slack".into(),
            kind: ExtensionKind::WasmTool,
            message: "Installed".into(),
        };
        assert_eq!(r.name, "slack");
        assert_eq!(r.kind, ExtensionKind::WasmTool);
    }

    #[test]
    fn auth_result_serde_roundtrip() {
        let r = AuthResult {
            name: "notion".into(),
            kind: ExtensionKind::McpServer,
            auth_url: Some("https://auth.example.com".into()),
            callback_type: Some("local".into()),
            instructions: None,
            setup_url: None,
            awaiting_token: false,
            status: "pending".into(),
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: AuthResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "notion");
        assert_eq!(back.status, "pending");
    }

    #[test]
    fn activate_result_fields() {
        let r = ActivateResult {
            name: "notion".into(),
            kind: ExtensionKind::McpServer,
            tools_loaded: vec!["search".into(), "create_page".into()],
            message: "OK".into(),
        };
        assert_eq!(r.tools_loaded.len(), 2);
    }

    #[test]
    fn installed_extension_fields() {
        let ext = InstalledExtension {
            name: "telegram".into(),
            kind: ExtensionKind::WasmChannel,
            description: Some("Telegram channel".into()),
            url: None,
            authenticated: true,
            active: false,
            tools: vec![],
        };
        assert!(ext.authenticated);
        assert!(!ext.active);
        assert_eq!(ext.kind, ExtensionKind::WasmChannel);
    }

    #[test]
    fn search_result_fields() {
        let sr = SearchResult {
            entry: sample_entry(),
            source: ResultSource::Registry,
            validated: true,
        };
        assert_eq!(sr.entry.name, "notion");
        assert_eq!(sr.source, ResultSource::Registry);
        assert!(sr.validated);
    }
}
