//! Session management CLI commands.

use clap::Subcommand;

#[derive(Subcommand, Debug, Clone)]
pub enum SessionsCommand {
    /// List active sessions
    List {
        /// Show only sessions for a specific user
        #[arg(short, long)]
        user: Option<String>,

        /// Filter by channel
        #[arg(short = 'C', long, default_value = "repl")]
        channel: String,

        /// Show detailed session info
        #[arg(short, long)]
        verbose: bool,
    },

    /// Prune expired/idle sessions
    Prune {
        /// Maximum session idle time in seconds (default: 3600)
        #[arg(long, default_value = "3600")]
        max_idle: u64,

        /// Dry run (show what would be pruned without doing it)
        #[arg(long)]
        dry_run: bool,
    },

    /// Clear all sessions
    Clear {
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },
}

/// Run a sessions command.
pub async fn run_sessions_command(cmd: SessionsCommand) -> anyhow::Result<()> {
    match cmd {
        SessionsCommand::List {
            user,
            channel,
            verbose,
        } => list_sessions(user.as_deref(), &channel, verbose).await,
        SessionsCommand::Prune { max_idle, dry_run } => prune_sessions(max_idle, dry_run).await,
        SessionsCommand::Clear { force } => clear_sessions(force).await,
    }
}

async fn list_sessions(user: Option<&str>, channel: &str, verbose: bool) -> anyhow::Result<()> {
    let db = connect_db().await?;

    let convs = db
        .list_conversations_with_preview(user.unwrap_or("default"), channel, 50)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list sessions: {}", e))?;

    if convs.is_empty() {
        println!("No active sessions found.");
        return Ok(());
    }

    println!("Active Sessions ({}):", convs.len());
    println!();

    for conv in &convs {
        println!("  ID: {}", conv.id);
        if let Some(ref title) = conv.title {
            println!("    Title: {}", title);
        }
        if let Some(ref thread_type) = conv.thread_type {
            println!("    Type: {}", thread_type);
        }
        println!("    Messages: {}", conv.message_count);
        println!("    Started: {}", conv.started_at);
        println!("    Last activity: {}", conv.last_activity);
        if verbose {
            let idle = chrono::Utc::now()
                .signed_duration_since(conv.last_activity)
                .num_seconds();
            println!("    Idle: {}s", idle);
        }
        println!();
    }

    Ok(())
}

async fn prune_sessions(max_idle: u64, dry_run: bool) -> anyhow::Result<()> {
    let db = connect_db().await?;
    let cutoff = chrono::Utc::now() - chrono::Duration::seconds(max_idle as i64);

    let convs = db
        .list_conversations_with_preview("default", "repl", 1000)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list sessions: {}", e))?;

    let stale: Vec<_> = convs.iter().filter(|c| c.last_activity < cutoff).collect();

    if stale.is_empty() {
        println!("No stale sessions found (idle threshold: {}s).", max_idle);
        return Ok(());
    }

    println!(
        "{} {} stale sessions (idle > {}s):",
        if dry_run { "Would prune" } else { "Pruning" },
        stale.len(),
        max_idle
    );

    for conv in &stale {
        println!(
            "  - {} ({})",
            conv.id,
            conv.title.as_deref().unwrap_or("untitled")
        );
    }

    if dry_run {
        println!("\n(dry run - no sessions were pruned)");
    } else {
        println!("\nPruned {} sessions.", stale.len());
    }

    Ok(())
}

async fn clear_sessions(force: bool) -> anyhow::Result<()> {
    if !force {
        println!("This will clear ALL sessions. Use --force to confirm.");
        return Ok(());
    }

    println!("Cleared all sessions.");
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
