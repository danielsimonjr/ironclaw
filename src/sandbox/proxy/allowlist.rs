//! Domain allowlist for the network proxy.
//!
//! Validates that HTTP requests only go to allowed domains.
//! Supports exact matches and wildcard patterns.

use std::fmt;
use std::net::IpAddr;

/// Check if an IP address is in a private or reserved range.
///
/// Blocks SSRF to localhost, link-local, private RFC 1918 ranges, and
/// cloud metadata services (Finding 5).
fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()                         // 127.0.0.0/8
            || v4.is_private()                       // 10/8, 172.16/12, 192.168/16
            || v4.is_link_local()                    // 169.254.0.0/16
            || v4.is_unspecified()                    // 0.0.0.0
            || v4.is_broadcast()                     // 255.255.255.255
            || v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64 // 100.64/10 (CGNAT)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()                         // ::1
            || v6.is_unspecified() // ::
        }
    }
}

/// Pattern for matching allowed domains.
#[derive(Debug, Clone)]
pub struct DomainPattern {
    /// The domain pattern (e.g., "api.example.com" or "*.example.com").
    pattern: String,
    /// Whether this is a wildcard pattern.
    is_wildcard: bool,
    /// The base domain for wildcard matching.
    base_domain: String,
}

impl DomainPattern {
    /// Create a new domain pattern.
    pub fn new(pattern: &str) -> Self {
        let is_wildcard = pattern.starts_with("*.");
        let base_domain = if is_wildcard {
            pattern[2..].to_lowercase()
        } else {
            pattern.to_lowercase()
        };

        Self {
            pattern: pattern.to_string(),
            is_wildcard,
            base_domain,
        }
    }

    /// Check if a host matches this pattern.
    pub fn matches(&self, host: &str) -> bool {
        let host_lower = host.to_lowercase();

        if self.is_wildcard {
            // *.example.com matches foo.example.com, bar.baz.example.com, example.com
            host_lower == self.base_domain
                || host_lower.ends_with(&format!(".{}", self.base_domain))
        } else {
            host_lower == self.base_domain
        }
    }

    /// Get the pattern string.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }
}

impl fmt::Display for DomainPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.pattern)
    }
}

/// Result of domain validation.
#[derive(Debug, Clone)]
pub enum DomainValidationResult {
    /// Domain is allowed.
    Allowed,
    /// Domain is denied with a reason.
    Denied(String),
}

impl DomainValidationResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, DomainValidationResult::Allowed)
    }
}

/// Validates domains against an allowlist.
#[derive(Debug, Clone)]
pub struct DomainAllowlist {
    patterns: Vec<DomainPattern>,
}

impl DomainAllowlist {
    /// Create a new allowlist from domain strings.
    pub fn new(domains: &[String]) -> Self {
        Self {
            patterns: domains.iter().map(|d| DomainPattern::new(d)).collect(),
        }
    }

    /// Create an empty allowlist (denies everything).
    pub fn empty() -> Self {
        Self { patterns: vec![] }
    }

    /// Add a domain pattern to the allowlist.
    pub fn add(&mut self, pattern: &str) {
        self.patterns.push(DomainPattern::new(pattern));
    }

    /// Check if a domain is allowed.
    ///
    /// Rejects raw IP addresses (both IPv4 and IPv6) to prevent SSRF bypasses
    /// where an attacker uses an IP literal instead of a domain name (Finding 5).
    pub fn is_allowed(&self, host: &str) -> DomainValidationResult {
        if self.patterns.is_empty() {
            return DomainValidationResult::Denied("empty allowlist".to_string());
        }

        // Reject raw IP addresses — allowlist is domain-name-only
        if host.parse::<IpAddr>().is_ok() {
            return DomainValidationResult::Denied(format!(
                "raw IP address '{}' not allowed; use domain names only",
                host
            ));
        }

        for pattern in &self.patterns {
            if pattern.matches(host) {
                return DomainValidationResult::Allowed;
            }
        }

        DomainValidationResult::Denied(format!(
            "host '{}' not in allowlist: [{}]",
            host,
            self.patterns
                .iter()
                .map(|p| p.pattern())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    /// Check if a domain is allowed, with DNS resolution to verify the resolved
    /// IPs are not in private ranges (prevents DNS rebinding attacks — Finding 5).
    pub async fn is_allowed_with_dns(&self, host: &str) -> DomainValidationResult {
        // First do the standard domain check
        let result = self.is_allowed(host);
        if !result.is_allowed() {
            return result;
        }

        // Resolve DNS and check all IPs
        match tokio::net::lookup_host(format!("{}:443", host)).await {
            Ok(addrs) => {
                for addr in addrs {
                    if is_private_ip(&addr.ip()) {
                        return DomainValidationResult::Denied(format!(
                            "domain '{}' resolves to private IP {}",
                            host,
                            addr.ip()
                        ));
                    }
                }
                DomainValidationResult::Allowed
            }
            Err(_) => {
                // DNS resolution failure — fail closed
                DomainValidationResult::Denied(format!("DNS resolution failed for '{}'", host))
            }
        }
    }

    /// Get all patterns in the allowlist.
    pub fn patterns(&self) -> &[DomainPattern] {
        &self.patterns
    }

    /// Check if the allowlist is empty.
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Get the number of patterns.
    pub fn len(&self) -> usize {
        self.patterns.len()
    }
}

impl Default for DomainAllowlist {
    fn default() -> Self {
        Self::new(&crate::sandbox::config::default_allowlist())
    }
}

/// Parse host from a URL string.
pub fn extract_host(url: &str) -> Option<String> {
    // Determine scheme and extract the rest
    let rest = if let Some(stripped) = url.strip_prefix("https://") {
        stripped
    } else if let Some(stripped) = url.strip_prefix("http://") {
        stripped
    } else {
        return None;
    };

    // Find the end of the host (start of path, query, or end of string)
    let host_end = rest.find('/').unwrap_or(rest.len());
    let host_and_port = &rest[..host_end];

    // Remove port if present
    let host = if let Some(bracket_idx) = host_and_port.find('[') {
        // IPv6 address
        let close_bracket = host_and_port.find(']')?;
        &host_and_port[bracket_idx + 1..close_bracket]
    } else if let Some(colon_idx) = host_and_port.rfind(':') {
        // Check if this is a port (all digits after colon)
        let after_colon = &host_and_port[colon_idx + 1..];
        if after_colon.chars().all(|c| c.is_ascii_digit()) {
            &host_and_port[..colon_idx]
        } else {
            host_and_port
        }
    } else {
        host_and_port
    };

    if host.is_empty() {
        None
    } else {
        Some(host.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let pattern = DomainPattern::new("api.example.com");
        assert!(pattern.matches("api.example.com"));
        assert!(pattern.matches("API.EXAMPLE.COM"));
        assert!(!pattern.matches("foo.api.example.com"));
        assert!(!pattern.matches("example.com"));
    }

    #[test]
    fn test_wildcard_match() {
        let pattern = DomainPattern::new("*.example.com");
        assert!(pattern.matches("api.example.com"));
        assert!(pattern.matches("foo.bar.example.com"));
        assert!(pattern.matches("example.com")); // Base domain also matches
        assert!(!pattern.matches("exampleXcom"));
        assert!(!pattern.matches("other.com"));
    }

    #[test]
    fn test_allowlist_allows() {
        let allowlist =
            DomainAllowlist::new(&["crates.io".to_string(), "*.github.com".to_string()]);

        assert!(allowlist.is_allowed("crates.io").is_allowed());
        assert!(allowlist.is_allowed("api.github.com").is_allowed());
        assert!(
            !allowlist
                .is_allowed("raw.githubusercontent.com")
                .is_allowed()
        );
    }

    #[test]
    fn test_allowlist_denies() {
        let allowlist = DomainAllowlist::new(&["crates.io".to_string()]);

        let result = allowlist.is_allowed("evil.com");
        assert!(!result.is_allowed());
    }

    #[test]
    fn test_empty_allowlist() {
        let allowlist = DomainAllowlist::empty();
        assert!(!allowlist.is_allowed("anything.com").is_allowed());
    }

    #[test]
    fn test_extract_host() {
        assert_eq!(
            extract_host("https://api.example.com/v1/endpoint"),
            Some("api.example.com".to_string())
        );
        assert_eq!(
            extract_host("http://localhost:8080/api"),
            Some("localhost".to_string())
        );
        assert_eq!(
            extract_host("https://EXAMPLE.COM"),
            Some("example.com".to_string())
        );
        assert_eq!(extract_host("not-a-url"), None);
    }
}
