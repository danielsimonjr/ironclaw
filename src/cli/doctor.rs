//! Doctor diagnostics CLI command.
//!
//! Performs comprehensive health checks and reports actionable fixes.

use std::path::PathBuf;

use crate::settings::Settings;

/// Diagnostic check result.
struct Check {
    name: &'static str,
    status: CheckStatus,
    message: String,
    fix: Option<String>,
}

enum CheckStatus {
    Ok,
    Warning,
    Error,
}

impl Check {
    fn ok(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Ok,
            message: message.into(),
            fix: None,
        }
    }

    fn warn(name: &'static str, message: impl Into<String>, fix: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Warning,
            message: message.into(),
            fix: Some(fix.into()),
        }
    }

    fn error(name: &'static str, message: impl Into<String>, fix: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Error,
            message: message.into(),
            fix: Some(fix.into()),
        }
    }

    fn icon(&self) -> &'static str {
        match self.status {
            CheckStatus::Ok => "[OK]",
            CheckStatus::Warning => "[WARN]",
            CheckStatus::Error => "[ERR]",
        }
    }
}

/// Run comprehensive diagnostics.
pub async fn run_doctor_command() -> anyhow::Result<()> {
    println!("IronClaw Doctor");
    println!("===============\n");
    println!("Running diagnostics...\n");

    let settings = Settings::load();
    let mut checks = Vec::new();

    // 1. Rust version
    checks.push(check_rust_version());

    // 2. Data directory
    checks.push(check_data_directory());

    // 3. Database
    checks.push(check_database(&settings).await);

    // 4. LLM provider
    checks.push(check_llm_provider(&settings));

    // 5. Session / auth
    checks.push(check_session());

    // 6. Secrets
    checks.push(check_secrets(&settings).await);

    // 7. WASM runtime
    checks.push(check_wasm_runtime(&settings));

    // 8. Embeddings
    checks.push(check_embeddings(&settings));

    // 9. Network
    checks.push(check_network().await);

    // 10. Disk space
    checks.push(check_disk_space());

    // Print results
    let mut errors = 0;
    let mut warnings = 0;

    for check in &checks {
        println!("  {} {}: {}", check.icon(), check.name, check.message);
        if let Some(ref fix) = check.fix {
            println!("       Fix: {}", fix);
        }

        match check.status {
            CheckStatus::Error => errors += 1,
            CheckStatus::Warning => warnings += 1,
            CheckStatus::Ok => {}
        }
    }

    println!();
    println!(
        "Summary: {} checks, {} passed, {} warnings, {} errors",
        checks.len(),
        checks.len() - errors - warnings,
        warnings,
        errors
    );

    if errors > 0 {
        println!("\nPlease fix the errors above to ensure IronClaw works correctly.");
    } else if warnings > 0 {
        println!("\nIronClaw should work, but consider addressing the warnings above.");
    } else {
        println!("\nAll checks passed! IronClaw is ready to use.");
    }

    Ok(())
}

fn check_rust_version() -> Check {
    let version = env!("CARGO_PKG_VERSION");
    Check::ok("Version", format!("IronClaw v{}", version))
}

fn check_data_directory() -> Check {
    let data_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ironclaw");

    if data_dir.exists() {
        Check::ok("Data Dir", format!("{} (exists)", data_dir.display()))
    } else {
        Check::warn(
            "Data Dir",
            format!("{} (missing)", data_dir.display()),
            "Run 'ironclaw onboard' to create the data directory",
        )
    }
}

async fn check_database(settings: &Settings) -> Check {
    let has_url = settings.database_url.is_some() || std::env::var("DATABASE_URL").is_ok();

    if !has_url {
        return Check::error(
            "Database",
            "No database URL configured",
            "Set DATABASE_URL environment variable or run 'ironclaw onboard'",
        );
    }

    #[cfg(feature = "postgres")]
    {
        let _ = dotenvy::dotenv();
        let url = std::env::var("DATABASE_URL")
            .ok()
            .or_else(|| settings.database_url.clone());

        if let Some(url) = url {
            let config = deadpool_postgres::Config {
                url: Some(url),
                ..Default::default()
            };

            match config.create_pool(
                Some(deadpool_postgres::Runtime::Tokio1),
                tokio_postgres::NoTls,
            ) {
                Ok(pool) => {
                    match tokio::time::timeout(std::time::Duration::from_secs(5), pool.get()).await
                    {
                        Ok(Ok(client)) => match client.execute("SELECT 1", &[]).await {
                            Ok(_) => return Check::ok("Database", "PostgreSQL connected"),
                            Err(e) => {
                                return Check::error(
                                    "Database",
                                    format!("Query failed: {}", e),
                                    "Check your PostgreSQL server status",
                                );
                            }
                        },
                        Ok(Err(e)) => {
                            return Check::error(
                                "Database",
                                format!("Connection failed: {}", e),
                                "Check DATABASE_URL and PostgreSQL server",
                            );
                        }
                        Err(_) => {
                            return Check::error(
                                "Database",
                                "Connection timed out",
                                "Check if PostgreSQL is running and accessible",
                            );
                        }
                    }
                }
                Err(e) => {
                    return Check::error(
                        "Database",
                        format!("Pool creation failed: {}", e),
                        "Check DATABASE_URL format",
                    );
                }
            }
        }
    }

    Check::ok("Database", "URL configured (not tested)")
}

fn check_llm_provider(settings: &Settings) -> Check {
    let backend = std::env::var("LLM_BACKEND")
        .ok()
        .or_else(|| settings.llm_backend.clone())
        .unwrap_or_else(|| "nearai".to_string());

    match backend.as_str() {
        "nearai" | "near_ai" => {
            let has_token = std::env::var("NEARAI_SESSION_TOKEN").is_ok();
            let session_path = crate::llm::session::default_session_path();
            if has_token || session_path.exists() {
                Check::ok("LLM Provider", format!("NEAR AI ({})", backend))
            } else {
                Check::warn(
                    "LLM Provider",
                    "NEAR AI configured but no session found",
                    "Run 'ironclaw onboard' to authenticate",
                )
            }
        }
        "openai" => {
            if std::env::var("OPENAI_API_KEY").is_ok() {
                Check::ok("LLM Provider", "OpenAI (configured)")
            } else {
                Check::error(
                    "LLM Provider",
                    "OpenAI selected but OPENAI_API_KEY not set",
                    "Set OPENAI_API_KEY environment variable",
                )
            }
        }
        "anthropic" => {
            if std::env::var("ANTHROPIC_API_KEY").is_ok() {
                Check::ok("LLM Provider", "Anthropic (configured)")
            } else {
                Check::error(
                    "LLM Provider",
                    "Anthropic selected but ANTHROPIC_API_KEY not set",
                    "Set ANTHROPIC_API_KEY environment variable",
                )
            }
        }
        "ollama" => {
            let base_url = std::env::var("OLLAMA_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:11434".to_string());
            Check::ok("LLM Provider", format!("Ollama ({})", base_url))
        }
        other => Check::warn(
            "LLM Provider",
            format!("Unknown backend: {}", other),
            "Use one of: nearai, openai, anthropic, ollama, openai_compatible",
        ),
    }
}

fn check_session() -> Check {
    let session_path = crate::llm::session::default_session_path();
    if session_path.exists() {
        Check::ok("Session", "Session file found")
    } else {
        Check::warn(
            "Session",
            "No session file",
            "Run 'ironclaw onboard' to create a session",
        )
    }
}

async fn check_secrets(settings: &Settings) -> Check {
    let has_key = settings.secrets_master_key_source != crate::settings::KeySource::None
        || std::env::var("SECRETS_MASTER_KEY").is_ok()
        || crate::secrets::keychain::has_master_key().await;

    if has_key {
        Check::ok("Secrets", "Master key configured")
    } else {
        Check::warn(
            "Secrets",
            "No master key configured",
            "Secrets will be generated during onboarding",
        )
    }
}

fn check_wasm_runtime(settings: &Settings) -> Check {
    let tools_dir = settings.wasm.tools_dir.clone().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".ironclaw")
            .join("tools")
    });

    if tools_dir.exists() {
        let count = std::fs::read_dir(&tools_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().is_some_and(|ext| ext == "wasm"))
                    .count()
            })
            .unwrap_or(0);
        Check::ok("WASM Tools", format!("{} tools installed", count))
    } else {
        Check::warn(
            "WASM Tools",
            "Tools directory not found",
            format!("Create {}", tools_dir.display()),
        )
    }
}

fn check_embeddings(settings: &Settings) -> Check {
    let enabled = settings.embeddings.enabled
        || std::env::var("OPENAI_API_KEY").is_ok()
        || std::env::var("EMBEDDING_ENABLED")
            .map(|v| v == "true")
            .unwrap_or(false);

    if enabled {
        Check::ok(
            "Embeddings",
            format!(
                "{} ({})",
                settings.embeddings.provider, settings.embeddings.model
            ),
        )
    } else {
        Check::warn(
            "Embeddings",
            "Disabled (semantic search won't work)",
            "Set OPENAI_API_KEY or configure embeddings in settings",
        )
    }
}

async fn check_network() -> Check {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build();

    match client {
        Ok(client) => match client.get("https://api.near.ai/health").send().await {
            Ok(resp) if resp.status().is_success() => Check::ok("Network", "NEAR AI API reachable"),
            Ok(resp) => Check::warn(
                "Network",
                format!("NEAR AI API returned {}", resp.status()),
                "Check your internet connection",
            ),
            Err(_) => Check::warn(
                "Network",
                "Cannot reach NEAR AI API",
                "Check your internet connection and firewall settings",
            ),
        },
        Err(e) => Check::error(
            "Network",
            format!("HTTP client error: {}", e),
            "TLS library may be misconfigured",
        ),
    }
}

fn check_disk_space() -> Check {
    let data_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ironclaw");

    // Use fs4 to check available space
    match fs4::available_space(&data_dir.parent().unwrap_or(&data_dir)) {
        Ok(space) => {
            let gb = space as f64 / (1024.0 * 1024.0 * 1024.0);
            if gb < 1.0 {
                Check::warn(
                    "Disk Space",
                    format!("{:.1} GB available", gb),
                    "Consider freeing up disk space",
                )
            } else {
                Check::ok("Disk Space", format!("{:.1} GB available", gb))
            }
        }
        Err(_) => Check::ok("Disk Space", "Could not determine (assumed OK)"),
    }
}
