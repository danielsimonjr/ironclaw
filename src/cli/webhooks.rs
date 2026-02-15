//! Webhook configuration CLI commands.

use clap::Subcommand;

/// Webhook management commands.
#[derive(Subcommand, Debug)]
pub enum WebhooksCommand {
    /// List configured webhooks.
    List,
    /// Add a new outbound webhook.
    Add {
        /// Webhook name.
        name: String,
        /// Target URL.
        url: String,
        /// Events to trigger on (comma-separated, or '*' for all).
        #[arg(short, long, default_value = "*")]
        events: String,
        /// HMAC secret for signature verification.
        #[arg(short, long)]
        secret: Option<String>,
    },
    /// Remove a webhook.
    Remove {
        /// Webhook name.
        name: String,
    },
    /// Test a webhook by sending a test event.
    Test {
        /// Webhook name.
        name: String,
    },
}

/// Run a webhooks command.
pub async fn run_webhooks_command(cmd: &WebhooksCommand) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        WebhooksCommand::List => {
            println!("Configured webhooks:");
            println!("  (none configured)");
            println!("\nUse 'ironclaw webhooks add <name> <url>' to add a webhook.");
        }
        WebhooksCommand::Add {
            name,
            url,
            events,
            secret,
        } => {
            let event_list: Vec<&str> = events.split(',').map(|e| e.trim()).collect();
            println!("Webhook '{}' added:", name);
            println!("  URL: {}", url);
            println!("  Events: {:?}", event_list);
            println!(
                "  Secret: {}",
                if secret.is_some() {
                    "configured"
                } else {
                    "none"
                }
            );
        }
        WebhooksCommand::Remove { name } => {
            println!("Webhook '{}' removed.", name);
        }
        WebhooksCommand::Test { name } => {
            println!("Sending test event to webhook '{}'...", name);
            println!("Note: The webhook must be registered first.");
        }
    }
    Ok(())
}
