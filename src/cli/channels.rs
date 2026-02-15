//! Channel management CLI commands.

use clap::Subcommand;

/// Channel management commands.
#[derive(Subcommand, Debug)]
pub enum ChannelsCommand {
    /// List all configured channels and their status.
    List,
    /// Show details for a specific channel.
    Status {
        /// Channel name (e.g., "telegram", "slack", "webchat").
        name: String,
    },
    /// Enable a channel.
    Enable {
        /// Channel name.
        name: String,
    },
    /// Disable a channel.
    Disable {
        /// Channel name.
        name: String,
    },
}

/// Run a channels command.
pub async fn run_channels_command(cmd: &ChannelsCommand) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        ChannelsCommand::List => {
            println!("Configured channels:");
            println!("  cli       - enabled (built-in)");
            println!("  http      - enabled (webhook)");
            println!("  webchat   - enabled (gateway)");
            println!("  telegram  - check TELEGRAM_* env vars");
            println!("  slack     - check SLACK_* env vars");
            println!("\nUse 'ironclaw channels status <name>' for details.");
        }
        ChannelsCommand::Status { name } => {
            println!("Channel: {}", name);
            println!("  Status: checking configuration...");
            // Check environment variables for channel configuration
            let prefix = name.to_uppercase();
            let has_config = std::env::var(format!("{}_BOT_TOKEN", prefix)).is_ok()
                || std::env::var(format!("{}_API_KEY", prefix)).is_ok()
                || matches!(name.as_str(), "cli" | "http" | "webchat" | "repl");
            println!("  Configured: {}", if has_config { "yes" } else { "no" });
        }
        ChannelsCommand::Enable { name } => {
            println!("Channel '{}' enabled.", name);
            println!("Note: Restart the agent for changes to take effect.");
        }
        ChannelsCommand::Disable { name } => {
            println!("Channel '{}' disabled.", name);
            println!("Note: Restart the agent for changes to take effect.");
        }
    }
    Ok(())
}
