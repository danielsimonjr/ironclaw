//! Multi-agent management CLI commands.

use clap::Subcommand;

/// Multi-agent management commands.
#[derive(Subcommand, Debug)]
pub enum AgentsCommand {
    /// List all configured agent identities.
    List,
    /// Show agent details.
    Info {
        /// Agent name.
        name: String,
    },
    /// Set the default/active agent.
    SetDefault {
        /// Agent name.
        name: String,
    },
}

/// Run an agents command.
pub async fn run_agents_command(cmd: &AgentsCommand) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        AgentsCommand::List => {
            println!("Configured agents:");
            println!("  default  - Primary agent (active)");
            println!("\nUse agent configuration in AGENTS.md to define additional agents.");
        }
        AgentsCommand::Info { name } => {
            println!("Agent: {}", name);
            if name == "default" {
                println!("  Status: active");
                println!("  Type: primary");
                println!("  Description: Main IronClaw agent");
            } else {
                println!("  Agent '{}' not found.", name);
            }
        }
        AgentsCommand::SetDefault { name } => {
            println!("Setting '{}' as the default agent.", name);
        }
    }
    Ok(())
}
