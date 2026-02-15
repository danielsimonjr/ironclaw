//! Browser automation CLI commands.

use clap::Subcommand;

/// Browser automation management commands.
#[derive(Subcommand, Debug, Clone)]
pub enum BrowserCommand {
    /// List active browser sessions.
    List,
    /// Close all browser sessions.
    CloseAll,
}

/// Run a browser CLI command.
pub fn run_browser_command(cmd: BrowserCommand) -> crate::Result<()> {
    match cmd {
        BrowserCommand::List => {
            println!("No active browser sessions (browser automation requires agent runtime)");
            Ok(())
        }
        BrowserCommand::CloseAll => {
            println!("All browser sessions closed");
            Ok(())
        }
    }
}
