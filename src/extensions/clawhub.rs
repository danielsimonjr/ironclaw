//! ClawHub registry client for discovering and installing extensions.
//!
//! ClawHub is the central registry for IronClaw/OpenClaw tools, channels,
//! and plugins. This module provides the client for searching, fetching,
//! and installing packages from the registry.

use serde::{Deserialize, Serialize};

/// ClawHub registry client.
pub struct ClawHubClient {
    client: reqwest::Client,
    base_url: String,
}

/// A package in the ClawHub registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryPackage {
    /// Package name (unique identifier).
    pub name: String,
    /// Display name.
    pub display_name: String,
    /// Short description.
    pub description: String,
    /// Package version (semver).
    pub version: String,
    /// Package type.
    pub package_type: PackageType,
    /// Author or organization.
    pub author: String,
    /// License (SPDX identifier).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Homepage URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// Repository URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// Keywords for search.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Download count.
    #[serde(default)]
    pub downloads: u64,
    /// Star count.
    #[serde(default)]
    pub stars: u64,
    /// Whether this package is verified/official.
    #[serde(default)]
    pub verified: bool,
    /// When this version was published.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    /// Download URL for the WASM binary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
    /// SHA256 hash of the WASM binary for integrity verification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// Type of package in the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageType {
    /// WASM tool.
    Tool,
    /// WASM channel.
    Channel,
    /// Plugin (tool + hooks + config).
    Plugin,
    /// Skill bundle.
    Skill,
    /// MCP server configuration.
    Mcp,
}

impl std::fmt::Display for PackageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tool => write!(f, "tool"),
            Self::Channel => write!(f, "channel"),
            Self::Plugin => write!(f, "plugin"),
            Self::Skill => write!(f, "skill"),
            Self::Mcp => write!(f, "mcp"),
        }
    }
}

/// Search results from the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    /// Matching packages.
    pub packages: Vec<RegistryPackage>,
    /// Total number of matches.
    pub total: u64,
    /// Current page.
    pub page: u32,
    /// Results per page.
    pub per_page: u32,
}

/// Detailed package information including all versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageDetail {
    /// The latest version info.
    #[serde(flatten)]
    pub latest: RegistryPackage,
    /// All available versions.
    pub versions: Vec<VersionInfo>,
    /// README content (markdown).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readme: Option<String>,
    /// Capabilities manifest (from capabilities.json).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<serde_json::Value>,
}

/// Version information for a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub published_at: Option<String>,
    pub download_url: Option<String>,
    pub sha256: Option<String>,
}

impl ClawHubClient {
    /// Create a new ClawHub client with the default registry URL.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://registry.clawhub.dev/api/v1".to_string(),
        }
    }

    /// Create with a custom registry URL.
    pub fn with_url(url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: url.into(),
        }
    }

    /// Search for packages in the registry.
    pub async fn search(
        &self,
        query: &str,
        package_type: Option<PackageType>,
        page: Option<u32>,
    ) -> Result<SearchResults, String> {
        let mut url = format!(
            "{}/packages?q={}",
            self.base_url,
            urlencoding::encode(query)
        );

        if let Some(pkg_type) = package_type {
            url.push_str(&format!("&type={}", pkg_type));
        }
        if let Some(page) = page {
            url.push_str(&format!("&page={}", page));
        }

        let response = self
            .client
            .get(&url)
            .header(
                "User-Agent",
                format!("ironclaw/{}", env!("CARGO_PKG_VERSION")),
            )
            .send()
            .await
            .map_err(|e| format!("Registry search failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Registry error: {}", error_text));
        }

        response
            .json()
            .await
            .map_err(|e| format!("Failed to parse search results: {}", e))
    }

    /// Get detailed information about a package.
    pub async fn get_package(&self, name: &str) -> Result<PackageDetail, String> {
        let url = format!("{}/packages/{}", self.base_url, urlencoding::encode(name));

        let response = self
            .client
            .get(&url)
            .header(
                "User-Agent",
                format!("ironclaw/{}", env!("CARGO_PKG_VERSION")),
            )
            .send()
            .await
            .map_err(|e| format!("Failed to fetch package: {}", e))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(format!("Package '{}' not found", name));
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Registry error: {}", error_text));
        }

        response
            .json()
            .await
            .map_err(|e| format!("Failed to parse package info: {}", e))
    }

    /// Download a package binary (WASM).
    pub async fn download(
        &self,
        name: &str,
        version: Option<&str>,
    ) -> Result<DownloadedPackage, String> {
        // First get the package info to find the download URL
        let detail = self.get_package(name).await?;

        let version_info = if let Some(ver) = version {
            detail
                .versions
                .iter()
                .find(|v| v.version == ver)
                .ok_or_else(|| format!("Version '{}' not found for package '{}'", ver, name))?
                .clone()
        } else {
            VersionInfo {
                version: detail.latest.version.clone(),
                published_at: detail.latest.published_at.clone(),
                download_url: detail.latest.download_url.clone(),
                sha256: detail.latest.sha256.clone(),
            }
        };

        let download_url = version_info
            .download_url
            .ok_or_else(|| format!("No download URL for package '{}'", name))?;

        let response = self
            .client
            .get(&download_url)
            .header(
                "User-Agent",
                format!("ironclaw/{}", env!("CARGO_PKG_VERSION")),
            )
            .send()
            .await
            .map_err(|e| format!("Download failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Download failed with status: {}",
                response.status()
            ));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read download: {}", e))?;

        // Verify integrity if SHA256 is provided
        if let Some(expected_hash) = &version_info.sha256 {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let actual_hash = hex::encode(hasher.finalize());

            if actual_hash != *expected_hash {
                return Err(format!(
                    "Integrity check failed: expected {}, got {}",
                    expected_hash, actual_hash
                ));
            }
        }

        Ok(DownloadedPackage {
            name: name.to_string(),
            version: version_info.version,
            data: bytes.to_vec(),
            package_type: detail.latest.package_type,
            capabilities: detail.capabilities,
        })
    }

    /// List featured/popular packages.
    pub async fn featured(&self) -> Result<Vec<RegistryPackage>, String> {
        let url = format!("{}/packages/featured", self.base_url);

        let response = self
            .client
            .get(&url)
            .header(
                "User-Agent",
                format!("ironclaw/{}", env!("CARGO_PKG_VERSION")),
            )
            .send()
            .await
            .map_err(|e| format!("Failed to fetch featured packages: {}", e))?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        response
            .json()
            .await
            .map_err(|e| format!("Failed to parse featured packages: {}", e))
    }

    /// Check if the registry is reachable.
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/health", self.base_url);
        self.client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}

impl Default for ClawHubClient {
    fn default() -> Self {
        Self::new()
    }
}

/// A downloaded package ready for installation.
#[derive(Debug)]
pub struct DownloadedPackage {
    /// Package name.
    pub name: String,
    /// Version string.
    pub version: String,
    /// Binary data (WASM component).
    pub data: Vec<u8>,
    /// Package type.
    pub package_type: PackageType,
    /// Capabilities manifest.
    pub capabilities: Option<serde_json::Value>,
}

impl DownloadedPackage {
    /// Get the target installation path.
    pub fn install_path(&self) -> std::path::PathBuf {
        let dir = match self.package_type {
            PackageType::Tool | PackageType::Plugin => "tools",
            PackageType::Channel => "channels",
            PackageType::Skill => "skills",
            PackageType::Mcp => "mcp",
        };

        let base = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".ironclaw")
            .join(dir);

        base.join(format!("{}.wasm", self.name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_type_display() {
        assert_eq!(PackageType::Tool.to_string(), "tool");
        assert_eq!(PackageType::Channel.to_string(), "channel");
        assert_eq!(PackageType::Plugin.to_string(), "plugin");
    }

    #[test]
    fn test_registry_package_serialization() {
        let pkg = RegistryPackage {
            name: "gmail".to_string(),
            display_name: "Gmail Tool".to_string(),
            description: "Gmail integration".to_string(),
            version: "1.0.0".to_string(),
            package_type: PackageType::Tool,
            author: "ironclaw".to_string(),
            license: Some("MIT".to_string()),
            homepage: None,
            repository: None,
            keywords: vec!["email".to_string(), "gmail".to_string()],
            downloads: 1000,
            stars: 50,
            verified: true,
            published_at: None,
            download_url: None,
            sha256: None,
        };

        let json = serde_json::to_string(&pkg).unwrap();
        assert!(json.contains("gmail"));
        assert!(json.contains("\"verified\":true"));
    }

    #[test]
    fn test_install_path() {
        let pkg = DownloadedPackage {
            name: "test-tool".to_string(),
            version: "1.0.0".to_string(),
            data: Vec::new(),
            package_type: PackageType::Tool,
            capabilities: None,
        };

        let path = pkg.install_path();
        assert!(path.to_string_lossy().contains("tools"));
        assert!(path.to_string_lossy().contains("test-tool.wasm"));
    }

    #[test]
    fn test_default_client() {
        let client = ClawHubClient::new();
        assert!(client.base_url.contains("clawhub"));
    }

    #[test]
    fn test_custom_url() {
        let client = ClawHubClient::with_url("https://my-registry.example.com/api/v1");
        assert_eq!(client.base_url, "https://my-registry.example.com/api/v1");
    }
}
