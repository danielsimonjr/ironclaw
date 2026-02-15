//! Log query CLI commands.

use clap::Subcommand;

#[derive(Subcommand, Debug, Clone)]
pub enum LogsCommand {
    /// Show recent logs
    Tail {
        /// Number of lines to show
        #[arg(short, long, default_value = "50")]
        lines: usize,

        /// Filter by log level (debug, info, warn, error)
        #[arg(short = 'l', long)]
        level: Option<String>,

        /// Filter by module/target
        #[arg(short, long)]
        target: Option<String>,

        /// Follow new log entries (like tail -f)
        #[arg(short, long)]
        follow: bool,
    },

    /// Search logs
    Search {
        /// Search pattern (case-insensitive)
        pattern: String,

        /// Maximum results
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Show job execution logs
    Job {
        /// Job ID
        job_id: uuid::Uuid,
    },
}

/// Run a logs command.
pub async fn run_logs_command(cmd: LogsCommand) -> anyhow::Result<()> {
    match cmd {
        LogsCommand::Tail {
            lines,
            level,
            target,
            follow,
        } => tail_logs(lines, level, target, follow).await,
        LogsCommand::Search { pattern, limit } => search_logs(&pattern, limit).await,
        LogsCommand::Job { job_id } => job_logs(job_id).await,
    }
}

async fn tail_logs(
    lines: usize,
    level: Option<String>,
    target: Option<String>,
    follow: bool,
) -> anyhow::Result<()> {
    // Read log file from default location
    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".ironclaw")
        .join("logs");

    let log_file = log_dir.join("ironclaw.log");

    if !log_file.exists() {
        println!("No log file found at {}", log_file.display());
        println!("\nTo enable file logging, set:");
        println!("  RUST_LOG=ironclaw=info");
        println!("  IRONCLAW_LOG_FILE={}", log_file.display());
        return Ok(());
    }

    let content = std::fs::read_to_string(&log_file)?;
    let all_lines: Vec<&str> = content.lines().collect();

    // Apply filters
    let filtered: Vec<&&str> = all_lines
        .iter()
        .filter(|line| {
            if let Some(ref lvl) = level {
                let upper = lvl.to_uppercase();
                if !line.to_uppercase().contains(&upper) {
                    return false;
                }
            }
            if let Some(ref tgt) = target {
                if !line.contains(tgt) {
                    return false;
                }
            }
            true
        })
        .collect();

    // Show last N lines
    let start = if filtered.len() > lines {
        filtered.len() - lines
    } else {
        0
    };

    for line in &filtered[start..] {
        println!("{}", line);
    }

    if follow {
        println!(
            "\n(Following mode not yet available in CLI. Use `tail -f {}` directly.)",
            log_file.display()
        );
    }

    Ok(())
}

async fn search_logs(pattern: &str, limit: usize) -> anyhow::Result<()> {
    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".ironclaw")
        .join("logs");

    let log_file = log_dir.join("ironclaw.log");

    if !log_file.exists() {
        println!("No log file found at {}", log_file.display());
        return Ok(());
    }

    let content = std::fs::read_to_string(&log_file)?;
    let pattern_lower = pattern.to_lowercase();

    let matches: Vec<&str> = content
        .lines()
        .filter(|line| line.to_lowercase().contains(&pattern_lower))
        .collect();

    if matches.is_empty() {
        println!("No log entries matching '{}'", pattern);
        return Ok(());
    }

    let shown = matches.len().min(limit);
    println!("Found {} matches (showing {}):", matches.len(), shown);
    println!();

    for line in &matches[..shown] {
        println!("{}", line);
    }

    Ok(())
}

async fn job_logs(job_id: uuid::Uuid) -> anyhow::Result<()> {
    let db = connect_db().await?;

    let events = db
        .list_job_events(job_id)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get job events: {}", e))?;

    if events.is_empty() {
        println!("No events found for job {}", job_id);
        return Ok(());
    }

    println!("Events for job {}:", job_id);
    println!();

    for event in &events {
        println!(
            "  [{}] {} ({})",
            event.created_at, event.event_type, event.data,
        );
    }

    Ok(())
}

async fn connect_db() -> anyhow::Result<std::sync::Arc<dyn crate::db::Database>> {
    let _ = dotenvy::dotenv();
    let config = crate::config::Config::from_env()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    crate::db::connect_from_config(&config.database)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))
}
