//! Plugin management CLI commands.

use clap::Subcommand;

/// Plugin management commands.
#[derive(Subcommand, Debug)]
pub enum PluginsCommand {
    /// List installed plugins.
    List,
    /// Install a plugin from a registry or local path.
    Install {
        /// Plugin name or path.
        source: String,
    },
    /// Remove an installed plugin.
    Remove {
        /// Plugin name.
        name: String,
    },
    /// Show plugin details.
    Info {
        /// Plugin name.
        name: String,
    },
    /// Update all or specific plugins.
    Update {
        /// Plugin name (all if omitted).
        name: Option<String>,
    },
}

/// Run a plugins command.
pub async fn run_plugins_command(cmd: &PluginsCommand) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        PluginsCommand::List => {
            println!("Installed plugins:");
            // List WASM tools and MCP servers
            let tools_dir = dirs::home_dir()
                .unwrap_or_default()
                .join(".ironclaw")
                .join("tools");
            if tools_dir.exists() {
                for entry in std::fs::read_dir(&tools_dir)? {
                    let entry = entry?;
                    if entry.path().extension().is_some_and(|e| e == "wasm") {
                        println!("  {} (WASM tool)", entry.file_name().to_string_lossy());
                    }
                }
            } else {
                println!("  (none installed)");
            }
        }
        PluginsCommand::Install { source } => {
            println!("Installing plugin from: {}", source);
            println!("Use 'ironclaw tool install' for WASM tools.");
            println!("Use 'ironclaw mcp add' for MCP servers.");
        }
        PluginsCommand::Remove { name } => {
            println!("Removing plugin: {}", name);
            println!("Use 'ironclaw tool remove' for WASM tools.");
        }
        PluginsCommand::Info { name } => {
            println!("Plugin: {}", name);
            println!("  Type: WASM tool / MCP server");
            println!("  Use 'ironclaw tool list' or 'ironclaw mcp list' for details.");
        }
        PluginsCommand::Update { name } => match name {
            Some(n) => println!("Updating plugin: {}", n),
            None => println!("Checking all plugins for updates..."),
        },
    }
    Ok(())
}
