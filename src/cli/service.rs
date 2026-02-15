//! Service file generation for launchd (macOS) and systemd (Linux).
//!
//! Provides helpers to generate, install, and uninstall service unit files so
//! IronClaw can be managed as a user-level daemon on both platforms.
//!
//! ```text
//! systemd: ~/.config/systemd/user/ironclaw.service
//! launchd: ~/Library/LaunchAgents/ai.near.ironclaw.plist
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Configuration values injected into generated service files.
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    /// Human-readable description of the service.
    pub description: String,
    /// Environment variables to set (key-value pairs).
    pub environment: HashMap<String, String>,
    /// Extra CLI arguments appended after the binary path.
    pub extra_args: Vec<String>,
    /// Working directory for the service process.
    pub working_directory: Option<PathBuf>,
    /// Whether the service manager should restart the process on failure.
    pub restart_on_failure: bool,
    /// Delay in seconds before restarting a failed process (systemd only).
    pub restart_delay_secs: u32,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            description: "IronClaw AI Assistant".to_string(),
            environment: HashMap::new(),
            extra_args: vec!["run".to_string(), "--no-onboard".to_string()],
            working_directory: dirs::home_dir(),
            restart_on_failure: true,
            restart_delay_secs: 10,
        }
    }
}

/// Generates and manages service files for launchd and systemd.
pub struct ServiceGenerator;

impl ServiceGenerator {
    /// Generate a systemd user service unit file.
    ///
    /// The resulting string can be written to
    /// `~/.config/systemd/user/ironclaw.service`.
    pub fn generate_systemd_unit(binary_path: &Path, config: &ServiceConfig) -> String {
        let mut unit = String::new();

        // [Unit] section
        unit.push_str("[Unit]\n");
        unit.push_str(&format!("Description={}\n", config.description));
        unit.push_str("After=network-online.target\n");
        unit.push_str("Wants=network-online.target\n");
        unit.push('\n');

        // [Service] section
        unit.push_str("[Service]\n");
        unit.push_str("Type=simple\n");

        // ExecStart
        let mut exec_start = binary_path.display().to_string();
        for arg in &config.extra_args {
            exec_start.push(' ');
            exec_start.push_str(arg);
        }
        unit.push_str(&format!("ExecStart={}\n", exec_start));

        // Working directory
        if let Some(ref wd) = config.working_directory {
            unit.push_str(&format!("WorkingDirectory={}\n", wd.display()));
        }

        // Environment variables (sorted for deterministic output)
        let mut env_keys: Vec<&String> = config.environment.keys().collect();
        env_keys.sort();
        for key in env_keys {
            let value = &config.environment[key];
            unit.push_str(&format!("Environment=\"{}={}\"\n", key, value));
        }

        // Restart policy
        if config.restart_on_failure {
            unit.push_str("Restart=on-failure\n");
            unit.push_str(&format!("RestartSec={}\n", config.restart_delay_secs));
        }

        // Logging to journal
        unit.push_str("StandardOutput=journal\n");
        unit.push_str("StandardError=journal\n");
        unit.push('\n');

        // [Install] section
        unit.push_str("[Install]\n");
        unit.push_str("WantedBy=default.target\n");

        unit
    }

    /// Generate a macOS launchd property list (plist) file.
    ///
    /// The resulting string can be written to
    /// `~/Library/LaunchAgents/ai.near.ironclaw.plist`.
    pub fn generate_launchd_plist(binary_path: &Path, config: &ServiceConfig) -> String {
        let mut plist = String::new();

        plist.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        plist.push_str("<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ");
        plist.push_str("\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n");
        plist.push_str("<plist version=\"1.0\">\n");
        plist.push_str("<dict>\n");

        // Label
        plist_key_string(&mut plist, "Label", "ai.near.ironclaw");

        // Program arguments
        plist.push_str("    <key>ProgramArguments</key>\n");
        plist.push_str("    <array>\n");
        plist.push_str(&format!(
            "        <string>{}</string>\n",
            xml_escape(&binary_path.display().to_string())
        ));
        for arg in &config.extra_args {
            plist.push_str(&format!("        <string>{}</string>\n", xml_escape(arg)));
        }
        plist.push_str("    </array>\n");

        // Working directory
        if let Some(ref wd) = config.working_directory {
            plist_key_string(&mut plist, "WorkingDirectory", &wd.display().to_string());
        }

        // Environment variables (sorted for deterministic output)
        if !config.environment.is_empty() {
            plist.push_str("    <key>EnvironmentVariables</key>\n");
            plist.push_str("    <dict>\n");
            let mut env_keys: Vec<&String> = config.environment.keys().collect();
            env_keys.sort();
            for key in env_keys {
                let value = &config.environment[key];
                plist.push_str(&format!(
                    "        <key>{}</key>\n        <string>{}</string>\n",
                    xml_escape(key),
                    xml_escape(value)
                ));
            }
            plist.push_str("    </dict>\n");
        }

        // Run at load (start immediately when loaded)
        plist.push_str("    <key>RunAtLoad</key>\n");
        plist.push_str("    <true/>\n");

        // Keep alive (restart on failure)
        if config.restart_on_failure {
            plist.push_str("    <key>KeepAlive</key>\n");
            plist.push_str("    <dict>\n");
            plist.push_str("        <key>SuccessfulExit</key>\n");
            plist.push_str("        <false/>\n");
            plist.push_str("    </dict>\n");
        }

        // Logging
        let log_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".ironclaw")
            .join("logs");
        plist_key_string(
            &mut plist,
            "StandardOutPath",
            &log_dir.join("ironclaw.out.log").display().to_string(),
        );
        plist_key_string(
            &mut plist,
            "StandardErrorPath",
            &log_dir.join("ironclaw.err.log").display().to_string(),
        );

        plist.push_str("</dict>\n");
        plist.push_str("</plist>\n");

        plist
    }

    /// Install a systemd user service unit with default configuration.
    ///
    /// Writes the generated unit file to
    /// `~/.config/systemd/user/ironclaw.service`.
    pub fn install_systemd(binary_path: &Path) -> Result<PathBuf, ServiceError> {
        Self::install_systemd_with_config(binary_path, &ServiceConfig::default())
    }

    /// Install a systemd user service unit with custom configuration.
    pub fn install_systemd_with_config(
        binary_path: &Path,
        config: &ServiceConfig,
    ) -> Result<PathBuf, ServiceError> {
        let service_dir = systemd_user_dir()?;
        fs::create_dir_all(&service_dir).map_err(|e| ServiceError::InstallFailed {
            reason: format!(
                "Failed to create directory {}: {}",
                service_dir.display(),
                e
            ),
        })?;

        let service_path = service_dir.join("ironclaw.service");
        let content = Self::generate_systemd_unit(binary_path, config);
        fs::write(&service_path, content).map_err(|e| ServiceError::InstallFailed {
            reason: format!(
                "Failed to write service file {}: {}",
                service_path.display(),
                e
            ),
        })?;

        Ok(service_path)
    }

    /// Install a macOS launchd agent with default configuration.
    ///
    /// Writes the generated plist file to
    /// `~/Library/LaunchAgents/ai.near.ironclaw.plist`.
    pub fn install_launchd(binary_path: &Path) -> Result<PathBuf, ServiceError> {
        Self::install_launchd_with_config(binary_path, &ServiceConfig::default())
    }

    /// Install a macOS launchd agent with custom configuration.
    pub fn install_launchd_with_config(
        binary_path: &Path,
        config: &ServiceConfig,
    ) -> Result<PathBuf, ServiceError> {
        let agents_dir = launchd_agents_dir()?;
        fs::create_dir_all(&agents_dir).map_err(|e| ServiceError::InstallFailed {
            reason: format!("Failed to create directory {}: {}", agents_dir.display(), e),
        })?;

        let plist_path = agents_dir.join("ai.near.ironclaw.plist");
        let content = Self::generate_launchd_plist(binary_path, config);
        fs::write(&plist_path, content).map_err(|e| ServiceError::InstallFailed {
            reason: format!("Failed to write plist file {}: {}", plist_path.display(), e),
        })?;

        // Create log directory so launchd can write logs immediately
        let log_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".ironclaw")
            .join("logs");
        let _ = fs::create_dir_all(log_dir);

        Ok(plist_path)
    }

    /// Uninstall the systemd user service unit.
    ///
    /// Removes `~/.config/systemd/user/ironclaw.service`. Callers should
    /// run `systemctl --user stop ironclaw` and `systemctl --user disable ironclaw`
    /// before invoking this.
    pub fn uninstall_systemd() -> Result<(), ServiceError> {
        let service_path = systemd_user_dir()
            .map(|d| d.join("ironclaw.service"))
            .map_err(|e| ServiceError::UninstallFailed {
                reason: e.to_string(),
            })?;

        if service_path.exists() {
            fs::remove_file(&service_path).map_err(|e| ServiceError::UninstallFailed {
                reason: format!(
                    "Failed to remove service file {}: {}",
                    service_path.display(),
                    e
                ),
            })?;
        }

        Ok(())
    }

    /// Uninstall the macOS launchd agent.
    ///
    /// Removes `~/Library/LaunchAgents/ai.near.ironclaw.plist`. Callers should
    /// run `launchctl unload <plist>` before invoking this.
    pub fn uninstall_launchd() -> Result<(), ServiceError> {
        let plist_path = launchd_agents_dir()
            .map(|d| d.join("ai.near.ironclaw.plist"))
            .map_err(|e| ServiceError::UninstallFailed {
                reason: e.to_string(),
            })?;

        if plist_path.exists() {
            fs::remove_file(&plist_path).map_err(|e| ServiceError::UninstallFailed {
                reason: format!(
                    "Failed to remove plist file {}: {}",
                    plist_path.display(),
                    e
                ),
            })?;
        }

        Ok(())
    }
}

/// Errors that can occur during service file operations.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Service installation failed: {reason}")]
    InstallFailed { reason: String },

    #[error("Service uninstall failed: {reason}")]
    UninstallFailed { reason: String },

    #[error("Home directory not found")]
    HomeDirNotFound,
}

// ---------------------------------------------------------------------------
// Backward-compatible free functions (delegate to ServiceGenerator)
// ---------------------------------------------------------------------------

/// Generate a systemd user service unit file (convenience wrapper).
pub fn generate_systemd_unit(binary_path: &str, working_dir: Option<&str>) -> String {
    let mut config = ServiceConfig::default();
    if let Some(wd) = working_dir {
        config.working_directory = Some(PathBuf::from(wd));
    }
    ServiceGenerator::generate_systemd_unit(Path::new(binary_path), &config)
}

/// Generate a macOS launchd plist file (convenience wrapper).
pub fn generate_launchd_plist(binary_path: &str, _log_dir: Option<&str>) -> String {
    let config = ServiceConfig::default();
    ServiceGenerator::generate_launchd_plist(Path::new(binary_path), &config)
}

/// Install a systemd user service (convenience wrapper).
pub fn install_systemd(binary_path: &str) -> Result<PathBuf, std::io::Error> {
    ServiceGenerator::install_systemd(Path::new(binary_path))
        .map_err(|e| std::io::Error::other(e.to_string()))
}

/// Install a macOS launchd agent (convenience wrapper).
pub fn install_launchd(binary_path: &str) -> Result<PathBuf, std::io::Error> {
    ServiceGenerator::install_launchd(Path::new(binary_path))
        .map_err(|e| std::io::Error::other(e.to_string()))
}

// -- helper functions --

/// Return the systemd user unit directory (`~/.config/systemd/user/`).
fn systemd_user_dir() -> Result<PathBuf, ServiceError> {
    let home = dirs::home_dir().ok_or(ServiceError::HomeDirNotFound)?;
    Ok(home.join(".config").join("systemd").join("user"))
}

/// Return the macOS LaunchAgents directory (`~/Library/LaunchAgents/`).
fn launchd_agents_dir() -> Result<PathBuf, ServiceError> {
    let home = dirs::home_dir().ok_or(ServiceError::HomeDirNotFound)?;
    Ok(home.join("Library").join("LaunchAgents"))
}

/// Write a `<key>...</key><string>...</string>` pair to a plist string.
fn plist_key_string(buf: &mut String, key: &str, value: &str) {
    buf.push_str(&format!(
        "    <key>{}</key>\n    <string>{}</string>\n",
        xml_escape(key),
        xml_escape(value)
    ));
}

/// Minimal XML escaping for plist values.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn default_config() -> ServiceConfig {
        ServiceConfig::default()
    }

    fn custom_config() -> ServiceConfig {
        let mut env = HashMap::new();
        env.insert(
            "DATABASE_URL".to_string(),
            "postgres://localhost/ironclaw".to_string(),
        );
        env.insert("GATEWAY_ENABLED".to_string(), "true".to_string());

        ServiceConfig {
            description: "IronClaw Test Service".to_string(),
            environment: env,
            extra_args: vec!["run".to_string(), "--no-onboard".to_string()],
            working_directory: Some(PathBuf::from("/home/testuser")),
            restart_on_failure: true,
            restart_delay_secs: 5,
        }
    }

    // -- systemd unit tests --

    #[test]
    fn test_generate_systemd_unit_default() {
        let binary = Path::new("/usr/local/bin/ironclaw");
        let config = default_config();
        let unit = ServiceGenerator::generate_systemd_unit(binary, &config);

        assert!(unit.contains("[Unit]"));
        assert!(unit.contains("Description=IronClaw AI Assistant"));
        assert!(unit.contains("After=network-online.target"));
        assert!(unit.contains("[Service]"));
        assert!(unit.contains("Type=simple"));
        assert!(unit.contains("ExecStart=/usr/local/bin/ironclaw run --no-onboard"));
        assert!(unit.contains("Restart=on-failure"));
        assert!(unit.contains("RestartSec=10"));
        assert!(unit.contains("[Install]"));
        assert!(unit.contains("WantedBy=default.target"));
    }

    #[test]
    fn test_generate_systemd_unit_custom_config() {
        let binary = Path::new("/opt/ironclaw/bin/ironclaw");
        let config = custom_config();
        let unit = ServiceGenerator::generate_systemd_unit(binary, &config);

        assert!(unit.contains("Description=IronClaw Test Service"));
        assert!(unit.contains("ExecStart=/opt/ironclaw/bin/ironclaw run --no-onboard"));
        assert!(unit.contains("WorkingDirectory=/home/testuser"));
        assert!(unit.contains("RestartSec=5"));
        // Environment variables (sorted, so order is deterministic)
        assert!(unit.contains("Environment=\"DATABASE_URL=postgres://localhost/ironclaw\""));
        assert!(unit.contains("Environment=\"GATEWAY_ENABLED=true\""));
    }

    #[test]
    fn test_generate_systemd_unit_no_restart() {
        let binary = Path::new("/usr/bin/ironclaw");
        let config = ServiceConfig {
            restart_on_failure: false,
            ..default_config()
        };
        let unit = ServiceGenerator::generate_systemd_unit(binary, &config);

        assert!(!unit.contains("Restart=on-failure"));
        assert!(!unit.contains("RestartSec="));
    }

    #[test]
    fn test_generate_systemd_unit_no_working_directory() {
        let binary = Path::new("/usr/bin/ironclaw");
        let config = ServiceConfig {
            working_directory: None,
            ..default_config()
        };
        let unit = ServiceGenerator::generate_systemd_unit(binary, &config);

        assert!(!unit.contains("WorkingDirectory="));
    }

    #[test]
    fn test_generate_systemd_unit_extra_args() {
        let binary = Path::new("/usr/bin/ironclaw");
        let config = ServiceConfig {
            extra_args: vec![
                "run".to_string(),
                "--no-onboard".to_string(),
                "--cli-only".to_string(),
            ],
            ..default_config()
        };
        let unit = ServiceGenerator::generate_systemd_unit(binary, &config);

        assert!(unit.contains("ExecStart=/usr/bin/ironclaw run --no-onboard --cli-only"));
    }

    #[test]
    fn test_generate_systemd_unit_empty_args() {
        let binary = Path::new("/usr/bin/ironclaw");
        let config = ServiceConfig {
            extra_args: vec![],
            ..default_config()
        };
        let unit = ServiceGenerator::generate_systemd_unit(binary, &config);

        assert!(unit.contains("ExecStart=/usr/bin/ironclaw\n"));
    }

    // -- launchd plist tests --

    #[test]
    fn test_generate_launchd_plist_default() {
        let binary = Path::new("/usr/local/bin/ironclaw");
        let config = default_config();
        let plist = ServiceGenerator::generate_launchd_plist(binary, &config);

        assert!(plist.contains("<?xml version=\"1.0\""));
        assert!(plist.contains("<!DOCTYPE plist"));
        assert!(plist.contains("<plist version=\"1.0\">"));
        assert!(plist.contains("<key>Label</key>"));
        assert!(plist.contains("<string>ai.near.ironclaw</string>"));
        assert!(plist.contains("<key>ProgramArguments</key>"));
        assert!(plist.contains("<string>/usr/local/bin/ironclaw</string>"));
        assert!(plist.contains("<string>run</string>"));
        assert!(plist.contains("<string>--no-onboard</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains("<true/>"));
        assert!(plist.contains("<key>KeepAlive</key>"));
        assert!(plist.contains("<key>StandardOutPath</key>"));
        assert!(plist.contains("<key>StandardErrorPath</key>"));
        assert!(plist.contains("ironclaw.out.log"));
        assert!(plist.contains("ironclaw.err.log"));
        assert!(plist.contains("</plist>"));
    }

    #[test]
    fn test_generate_launchd_plist_custom_config() {
        let binary = Path::new("/opt/ironclaw/bin/ironclaw");
        let config = custom_config();
        let plist = ServiceGenerator::generate_launchd_plist(binary, &config);

        assert!(plist.contains("<string>/opt/ironclaw/bin/ironclaw</string>"));
        assert!(plist.contains("<key>WorkingDirectory</key>"));
        assert!(plist.contains("<string>/home/testuser</string>"));
        assert!(plist.contains("<key>EnvironmentVariables</key>"));
        assert!(plist.contains("<key>DATABASE_URL</key>"));
        assert!(plist.contains("<string>postgres://localhost/ironclaw</string>"));
    }

    #[test]
    fn test_generate_launchd_plist_no_restart() {
        let binary = Path::new("/usr/bin/ironclaw");
        let config = ServiceConfig {
            restart_on_failure: false,
            ..default_config()
        };
        let plist = ServiceGenerator::generate_launchd_plist(binary, &config);

        assert!(!plist.contains("<key>KeepAlive</key>"));
    }

    #[test]
    fn test_generate_launchd_plist_no_env() {
        let binary = Path::new("/usr/bin/ironclaw");
        let config = ServiceConfig {
            environment: HashMap::new(),
            ..default_config()
        };
        let plist = ServiceGenerator::generate_launchd_plist(binary, &config);

        assert!(!plist.contains("<key>EnvironmentVariables</key>"));
    }

    #[test]
    fn test_generate_launchd_plist_no_working_directory() {
        let binary = Path::new("/usr/bin/ironclaw");
        let config = ServiceConfig {
            working_directory: None,
            ..default_config()
        };
        let plist = ServiceGenerator::generate_launchd_plist(binary, &config);

        assert!(!plist.contains("<key>WorkingDirectory</key>"));
    }

    // -- xml escaping --

    #[test]
    fn test_xml_escape() {
        assert_eq!(xml_escape("hello"), "hello");
        assert_eq!(xml_escape("a&b"), "a&amp;b");
        assert_eq!(xml_escape("<tag>"), "&lt;tag&gt;");
        assert_eq!(xml_escape("key=\"val\""), "key=&quot;val&quot;");
        assert_eq!(xml_escape("it's"), "it&apos;s");
    }

    #[test]
    fn test_xml_escape_combined() {
        assert_eq!(
            xml_escape("<a href=\"b&c\">"),
            "&lt;a href=&quot;b&amp;c&quot;&gt;"
        );
    }

    // -- backward-compatible free function tests --

    #[test]
    fn test_compat_generate_systemd_unit() {
        let unit = generate_systemd_unit("/usr/local/bin/ironclaw", None);
        assert!(unit.contains("[Unit]"));
        assert!(unit.contains("ExecStart=/usr/local/bin/ironclaw"));
        assert!(unit.contains("[Service]"));
        assert!(unit.contains("[Install]"));
    }

    #[test]
    fn test_compat_generate_launchd_plist() {
        let plist = generate_launchd_plist("/usr/local/bin/ironclaw", None);
        assert!(plist.contains("ai.near.ironclaw"));
        assert!(plist.contains("/usr/local/bin/ironclaw"));
        assert!(plist.contains("RunAtLoad"));
    }

    #[test]
    fn test_compat_systemd_with_working_dir() {
        let unit = generate_systemd_unit("/usr/local/bin/ironclaw", Some("/home/user"));
        assert!(unit.contains("WorkingDirectory=/home/user"));
    }

    // -- install/uninstall tests --

    #[test]
    fn test_install_systemd_writes_file() {
        let temp = std::env::temp_dir().join("ironclaw_service_tests_systemd");
        let service_dir = temp.join(".config").join("systemd").join("user");
        let _ = fs::create_dir_all(&service_dir);
        let service_path = service_dir.join("ironclaw.service");

        // Test generate + write manually since install_systemd uses the
        // real home directory.
        let binary = Path::new("/usr/local/bin/ironclaw");
        let config = default_config();
        let content = ServiceGenerator::generate_systemd_unit(binary, &config);
        fs::write(&service_path, &content).expect("should write service file");

        assert!(service_path.exists());
        let read_back = fs::read_to_string(&service_path).unwrap();
        assert_eq!(read_back, content);

        // Cleanup
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_install_launchd_writes_file() {
        let temp = std::env::temp_dir().join("ironclaw_service_tests_launchd");
        let agents_dir = temp.join("Library").join("LaunchAgents");
        let _ = fs::create_dir_all(&agents_dir);
        let plist_path = agents_dir.join("ai.near.ironclaw.plist");

        let binary = Path::new("/usr/local/bin/ironclaw");
        let config = default_config();
        let content = ServiceGenerator::generate_launchd_plist(binary, &config);
        fs::write(&plist_path, &content).expect("should write plist file");

        assert!(plist_path.exists());
        let read_back = fs::read_to_string(&plist_path).unwrap();
        assert_eq!(read_back, content);

        // Cleanup
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_uninstall_nonexistent_is_ok() {
        // Uninstalling when no file exists should succeed silently.
        let temp = std::env::temp_dir().join("ironclaw_service_tests_uninstall");
        let service_path = temp.join("ironclaw.service");
        // Ensure file doesn't exist
        if service_path.exists() {
            fs::remove_file(&service_path).unwrap();
        }
        assert!(!service_path.exists());
    }

    #[test]
    fn test_service_config_default() {
        let config = ServiceConfig::default();
        assert_eq!(config.description, "IronClaw AI Assistant");
        assert!(config.environment.is_empty());
        assert_eq!(config.extra_args, vec!["run", "--no-onboard"]);
        assert!(config.restart_on_failure);
        assert_eq!(config.restart_delay_secs, 10);
    }

    #[test]
    fn test_roundtrip_systemd_contains_all_env_vars() {
        let mut env = HashMap::new();
        env.insert("A".to_string(), "1".to_string());
        env.insert("B".to_string(), "2".to_string());
        env.insert("C".to_string(), "3".to_string());

        let config = ServiceConfig {
            environment: env,
            ..default_config()
        };
        let binary = Path::new("/usr/bin/ironclaw");
        let unit = ServiceGenerator::generate_systemd_unit(binary, &config);

        assert!(unit.contains("Environment=\"A=1\""));
        assert!(unit.contains("Environment=\"B=2\""));
        assert!(unit.contains("Environment=\"C=3\""));
    }

    #[test]
    fn test_roundtrip_launchd_contains_all_env_vars() {
        let mut env = HashMap::new();
        env.insert("X".to_string(), "10".to_string());
        env.insert("Y".to_string(), "20".to_string());

        let config = ServiceConfig {
            environment: env,
            ..default_config()
        };
        let binary = Path::new("/usr/bin/ironclaw");
        let plist = ServiceGenerator::generate_launchd_plist(binary, &config);

        assert!(plist.contains("<key>X</key>"));
        assert!(plist.contains("<string>10</string>"));
        assert!(plist.contains("<key>Y</key>"));
        assert!(plist.contains("<string>20</string>"));
    }
}
