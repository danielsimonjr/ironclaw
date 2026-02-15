//! Cron/routine management CLI commands.

use clap::Subcommand;

#[derive(Subcommand, Debug, Clone)]
pub enum CronCommand {
    /// List all scheduled routines
    List {
        /// Show only enabled routines
        #[arg(long)]
        enabled_only: bool,
    },

    /// Show details of a specific routine
    Show {
        /// Routine name or ID
        name: String,
    },

    /// Enable a routine
    Enable {
        /// Routine name
        name: String,
    },

    /// Disable a routine
    Disable {
        /// Routine name
        name: String,
    },

    /// Show execution history for a routine
    History {
        /// Routine name
        name: String,

        /// Maximum number of runs to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Trigger a routine immediately
    Run {
        /// Routine name
        name: String,
    },
}

/// Run a cron command.
pub async fn run_cron_command(cmd: CronCommand) -> anyhow::Result<()> {
    match cmd {
        CronCommand::List { enabled_only } => list_routines(enabled_only).await,
        CronCommand::Show { name } => show_routine(&name).await,
        CronCommand::Enable { name } => toggle_routine(&name, true).await,
        CronCommand::Disable { name } => toggle_routine(&name, false).await,
        CronCommand::History { name, limit } => show_history(&name, limit).await,
        CronCommand::Run { name } => trigger_routine(&name).await,
    }
}

async fn list_routines(enabled_only: bool) -> anyhow::Result<()> {
    let db = connect_db().await?;

    let routines = db
        .list_routines("default")
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list routines: {}", e))?;

    let filtered: Vec<_> = if enabled_only {
        routines.into_iter().filter(|r| r.enabled).collect()
    } else {
        routines
    };

    if filtered.is_empty() {
        println!("No routines found.");
        return Ok(());
    }

    println!("Scheduled Routines ({}):", filtered.len());
    println!();

    for routine in &filtered {
        let status = if routine.enabled {
            "enabled"
        } else {
            "disabled"
        };
        let trigger = match &routine.trigger {
            crate::agent::routine::Trigger::Cron { schedule } => format!("cron({})", schedule),
            crate::agent::routine::Trigger::Event { channel, pattern } => {
                format!("event({}:{})", channel.as_deref().unwrap_or("*"), pattern)
            }
            crate::agent::routine::Trigger::Webhook { path, .. } => {
                format!("webhook({})", path.as_deref().unwrap_or("/"))
            }
            crate::agent::routine::Trigger::Manual => "manual".to_string(),
        };

        println!("  {} [{}] trigger={}", routine.name, status, trigger);
        println!("    {}", routine.description);
        if let Some(ref next) = routine.next_fire_at {
            println!("    Next run: {}", next);
        }
        println!("    Run count: {}", routine.run_count);
        println!();
    }

    Ok(())
}

async fn show_routine(name: &str) -> anyhow::Result<()> {
    let db = connect_db().await?;

    let routine = db
        .get_routine_by_name("default", name)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get routine: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("Routine '{}' not found", name))?;

    println!("Routine: {}", routine.name);
    println!("  ID: {}", routine.id);
    println!("  Description: {}", routine.description);
    println!("  Enabled: {}", routine.enabled);
    println!(
        "  Trigger: {:?}",
        serde_json::to_string(&routine.trigger).unwrap_or_default()
    );
    println!(
        "  Action: {:?}",
        serde_json::to_string(&routine.action).unwrap_or_default()
    );
    println!("  Run count: {}", routine.run_count);
    println!("  Consecutive failures: {}", routine.consecutive_failures);
    if let Some(ref last) = routine.last_run_at {
        println!("  Last run: {}", last);
    }
    if let Some(ref next) = routine.next_fire_at {
        println!("  Next run: {}", next);
    }

    Ok(())
}

async fn toggle_routine(name: &str, enabled: bool) -> anyhow::Result<()> {
    let db = connect_db().await?;

    let mut routine = db
        .get_routine_by_name("default", name)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get routine: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("Routine '{}' not found", name))?;

    routine.enabled = enabled;
    db.update_routine(&routine)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to update routine: {}", e))?;

    let status = if enabled { "enabled" } else { "disabled" };
    println!("Routine '{}' {}.", name, status);

    Ok(())
}

async fn show_history(name: &str, limit: usize) -> anyhow::Result<()> {
    let db = connect_db().await?;

    let routine = db
        .get_routine_by_name("default", name)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get routine: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("Routine '{}' not found", name))?;

    let runs = db
        .list_routine_runs(routine.id, limit as i64)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list runs: {}", e))?;

    if runs.is_empty() {
        println!("No execution history for routine '{}'.", name);
        return Ok(());
    }

    println!("Execution history for '{}' (last {}):", name, runs.len());
    println!();

    for run in &runs {
        let status = match run.status {
            crate::agent::routine::RunStatus::Running => "running",
            crate::agent::routine::RunStatus::Ok => "ok",
            crate::agent::routine::RunStatus::Attention => "attention",
            crate::agent::routine::RunStatus::Failed => "failed",
        };

        println!("  {} [{}]", run.started_at, status);
        if let Some(ref completed) = run.completed_at {
            let duration = *completed - run.started_at;
            println!("    Duration: {}s", duration.num_seconds());
        }
        if let Some(ref summary) = run.result_summary {
            println!("    Result: {}", summary);
        }
    }

    Ok(())
}

async fn trigger_routine(name: &str) -> anyhow::Result<()> {
    let db = connect_db().await?;

    let routine = db
        .get_routine_by_name("default", name)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get routine: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("Routine '{}' not found", name))?;

    println!("Triggered routine '{}' (ID: {})", routine.name, routine.id);
    println!("Note: The routine will execute on the next agent loop iteration.");

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
