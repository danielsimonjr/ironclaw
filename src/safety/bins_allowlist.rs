//! Safe binaries allowlist for shell execution.
//!
//! Restricts which binaries can be executed by the shell tool
//! to prevent arbitrary code execution.

use std::collections::HashSet;

/// Manages the allowlist of safe binaries.
pub struct BinsAllowlist {
    /// Set of allowed binary names.
    allowed: HashSet<String>,
    /// Whether the allowlist is enforced (false = allow everything).
    enforced: bool,
}

impl BinsAllowlist {
    /// Create a new allowlist with default safe binaries.
    pub fn new() -> Self {
        let mut allowed = HashSet::new();

        // Basic POSIX utilities
        for bin in &[
            "ls", "cat", "head", "tail", "grep", "find", "wc", "sort", "uniq", "cut", "sed", "awk",
            "tr", "tee", "xargs", "echo", "printf", "date", "whoami", "pwd", "cd", "mkdir",
            "rmdir", "cp", "mv", "rm", "touch", "chmod", "chown", "ln", "readlink", "realpath",
            "basename", "dirname", "stat", "file", "which", "env", "true", "false", "test", "expr",
            "seq",
        ] {
            allowed.insert(bin.to_string());
        }

        // Development tools
        for bin in &[
            "git",
            "cargo",
            "rustc",
            "rustfmt",
            "clippy-driver",
            "node",
            "npm",
            "npx",
            "yarn",
            "pnpm",
            "bun",
            "deno",
            "python",
            "python3",
            "pip",
            "pip3",
            "go",
            "java",
            "javac",
            "mvn",
            "gradle",
            "make",
            "cmake",
            "gcc",
            "g++",
            "clang",
            "clang++",
            "docker",
            "docker-compose",
            "curl",
            "wget",
            "ssh",
            "scp",
            "tar",
            "gzip",
            "gunzip",
            "zip",
            "unzip",
            "diff",
            "patch",
            "jq",
            "yq",
        ] {
            allowed.insert(bin.to_string());
        }

        // Package managers
        for bin in &["apt", "apt-get", "brew", "dnf", "yum", "pacman"] {
            allowed.insert(bin.to_string());
        }

        Self {
            allowed,
            enforced: false, // Off by default, can be enabled
        }
    }

    /// Check if a binary is allowed.
    pub fn is_allowed(&self, binary: &str) -> bool {
        if !self.enforced {
            return true;
        }

        // Extract the binary name from a path
        let name = std::path::Path::new(binary)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(binary);

        self.allowed.contains(name)
    }

    /// Add a binary to the allowlist.
    pub fn allow(&mut self, binary: impl Into<String>) {
        self.allowed.insert(binary.into());
    }

    /// Remove a binary from the allowlist.
    pub fn deny(&mut self, binary: &str) {
        self.allowed.remove(binary);
    }

    /// Enable or disable enforcement.
    pub fn set_enforced(&mut self, enforced: bool) {
        self.enforced = enforced;
    }

    /// Check if the allowlist is being enforced.
    pub fn is_enforced(&self) -> bool {
        self.enforced
    }

    /// List all allowed binaries.
    pub fn list(&self) -> Vec<&str> {
        let mut bins: Vec<_> = self.allowed.iter().map(|s| s.as_str()).collect();
        bins.sort();
        bins
    }

    /// Validate a shell command, extracting the binary name.
    pub fn validate_command(&self, command: &str) -> Result<(), String> {
        if !self.enforced {
            return Ok(());
        }

        let trimmed = command.trim();
        if trimmed.is_empty() {
            return Err("Empty command".to_string());
        }

        // Extract the first word (binary name) handling common patterns
        let binary = if trimmed.starts_with("sudo ") {
            // Check the command after sudo
            trimmed
                .strip_prefix("sudo ")
                .and_then(|rest| rest.split_whitespace().next())
                .unwrap_or(trimmed)
        } else if trimmed.starts_with("env ") {
            // Skip env and its options
            trimmed
                .split_whitespace()
                .skip(1)
                .find(|word| !word.contains('=') && !word.starts_with('-'))
                .unwrap_or(trimmed)
        } else {
            trimmed.split_whitespace().next().unwrap_or(trimmed)
        };

        if self.is_allowed(binary) {
            Ok(())
        } else {
            Err(format!(
                "Binary '{}' is not in the safe bins allowlist. Add it with SAFE_BINS_ADDITIONAL='{}'",
                binary, binary
            ))
        }
    }
}

impl Default for BinsAllowlist {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate LD_PRELOAD, DYLD_INSERT_LIBRARIES, and similar environment variables.
///
/// These variables can be used to inject shared libraries, which is a security risk.
pub fn validate_env_vars() -> Vec<String> {
    let mut warnings = Vec::new();

    let dangerous_vars = [
        "LD_PRELOAD",
        "LD_LIBRARY_PATH",
        "DYLD_INSERT_LIBRARIES",
        "DYLD_LIBRARY_PATH",
        "DYLD_FRAMEWORK_PATH",
        "LD_AUDIT",
        "LD_DEBUG",
        "LD_PROFILE",
    ];

    for var in &dangerous_vars {
        if let Ok(value) = std::env::var(var)
            && !value.is_empty()
        {
            warnings.push(format!(
                "Dangerous environment variable {} is set: {}",
                var,
                if value.len() > 50 {
                    format!("{}...", &value[..47])
                } else {
                    value
                }
            ));
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_allows_common_binaries() {
        let mut list = BinsAllowlist::new();
        list.set_enforced(true);

        assert!(list.is_allowed("ls"));
        assert!(list.is_allowed("git"));
        assert!(list.is_allowed("cargo"));
        assert!(list.is_allowed("python3"));
    }

    #[test]
    fn test_blocks_unknown_when_enforced() {
        let mut list = BinsAllowlist::new();
        list.set_enforced(true);

        assert!(!list.is_allowed("malicious_binary"));
    }

    #[test]
    fn test_allows_everything_when_not_enforced() {
        let list = BinsAllowlist::new();
        assert!(list.is_allowed("anything"));
    }

    #[test]
    fn test_validate_command() {
        let mut list = BinsAllowlist::new();
        list.set_enforced(true);

        assert!(list.validate_command("ls -la").is_ok());
        assert!(list.validate_command("git status").is_ok());
        assert!(list.validate_command("evil_binary --flag").is_err());
    }

    #[test]
    fn test_validate_sudo_command() {
        let mut list = BinsAllowlist::new();
        list.set_enforced(true);

        assert!(list.validate_command("sudo ls -la").is_ok());
        assert!(list.validate_command("sudo evil_binary").is_err());
    }

    #[test]
    fn test_add_remove() {
        let mut list = BinsAllowlist::new();
        list.set_enforced(true);

        list.allow("custom_tool");
        assert!(list.is_allowed("custom_tool"));

        list.deny("custom_tool");
        assert!(!list.is_allowed("custom_tool"));
    }

    #[test]
    fn test_validate_env_vars() {
        // This test just verifies the function runs without panicking
        let warnings = validate_env_vars();
        // We can't predict what env vars are set, but it shouldn't panic
        let _ = warnings;
    }
}
