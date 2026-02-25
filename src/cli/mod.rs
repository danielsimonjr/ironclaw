//! CLI command handling.
//!
//! Provides subcommands for:
//! - Running the agent (`run`)
//! - Interactive onboarding wizard (`onboard`)
//! - Managing configuration (`config list`, `config get`, `config set`)
//! - Managing WASM tools (`tool install`, `tool list`, `tool remove`)
//! - Managing MCP servers (`mcp add`, `mcp auth`, `mcp list`, `mcp test`)
//! - Querying workspace memory (`memory search`, `memory read`, `memory write`)
//! - Checking system health (`status`, `doctor`)
//! - Gateway management (`gateway start`, `gateway stop`, `gateway status`)
//! - Session management (`sessions list`, `sessions prune`)
//! - Hook management (`hooks list`, `hooks add`, `hooks remove`)
//! - Cron/routine management (`cron list`, `cron enable`, `cron history`)
//! - Log querying (`logs tail`, `logs search`, `logs job`)
//! - Message sending (`message send`)
//! - Shell completion generation (`completion`)
//! - Channel management (`channels list`, `channels status`, `channels enable`)
//! - Plugin management (`plugins list`, `plugins install`, `plugins remove`)
//! - Webhook management (`webhooks list`, `webhooks add`, `webhooks remove`)
//! - Skills management (`skills list`, `skills enable`, `skills disable`)
//! - Agent management (`agents list`, `agents info`, `agents set-default`)
//! - Node management (`nodes list`, `nodes add`, `nodes remove`, `nodes ping`)

mod agents;
mod browser;
mod channels;
mod completion;
mod config;
mod cron;
mod doctor;
mod gateway;
mod hooks;
mod logs;
mod mcp;
pub mod memory;
mod message;
mod nodes;
mod pairing;
mod plugins;
mod service;
mod sessions;
mod skills;
pub mod status;
mod tool;
mod webhooks;

pub use agents::{AgentsCommand, run_agents_command};
pub use browser::{BrowserCommand, run_browser_command};
pub use channels::{ChannelsCommand, run_channels_command};
pub use completion::generate_completions;
pub use config::{ConfigCommand, run_config_command};
pub use cron::{CronCommand, run_cron_command};
pub use doctor::run_doctor_command;
pub use gateway::{GatewayCommand, run_gateway_command};
pub use hooks::{HooksCommand, run_hooks_command};
pub use logs::{LogsCommand, run_logs_command};
pub use mcp::{McpCommand, run_mcp_command};
pub use memory::MemoryCommand;
#[cfg(feature = "postgres")]
pub use memory::run_memory_command;
pub use memory::run_memory_command_with_db;
pub use message::{MessageCommand, run_message_command};
pub use nodes::{Node, NodeManager, NodeStatus, NodesCommand, run_nodes_command};
pub use pairing::{PairingCommand, run_pairing_command, run_pairing_command_with_store};
pub use plugins::{PluginsCommand, run_plugins_command};
pub use service::{
    ServiceConfig, ServiceError, ServiceGenerator, generate_launchd_plist, generate_systemd_unit,
    install_launchd, install_systemd,
};
pub use sessions::{SessionsCommand, run_sessions_command};
pub use skills::{SkillsCommand, run_skills_command};
pub use status::run_status_command;
pub use tool::{ToolCommand, run_tool_command};
pub use webhooks::{WebhooksCommand, run_webhooks_command};

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "ironclaw")]
#[command(
    about = "Secure personal AI assistant that protects your data and expands its capabilities"
)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Run in interactive CLI mode only (disable other channels)
    #[arg(long, global = true)]
    pub cli_only: bool,

    /// Skip database connection (for testing)
    #[arg(long, global = true)]
    pub no_db: bool,

    /// Single message mode - send one message and exit
    #[arg(short, long, global = true)]
    pub message: Option<String>,

    /// Configuration file path (optional, uses env vars by default)
    #[arg(short, long, global = true)]
    pub config: Option<std::path::PathBuf>,

    /// Skip first-run onboarding check
    #[arg(long, global = true)]
    pub no_onboard: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run the agent (default if no subcommand given)
    Run,

    /// Interactive onboarding wizard
    Onboard {
        /// Skip authentication (use existing session)
        #[arg(long)]
        skip_auth: bool,

        /// Reconfigure channels only
        #[arg(long)]
        channels_only: bool,
    },

    /// Manage configuration settings
    #[command(subcommand)]
    Config(ConfigCommand),

    /// Manage WASM tools
    #[command(subcommand)]
    Tool(ToolCommand),

    /// Manage MCP servers (hosted tool providers)
    #[command(subcommand)]
    Mcp(McpCommand),

    /// Query and manage workspace memory
    #[command(subcommand)]
    Memory(MemoryCommand),

    /// DM pairing (approve inbound requests from unknown senders)
    #[command(subcommand)]
    Pairing(PairingCommand),

    /// Show system health and diagnostics
    Status,

    /// Run comprehensive diagnostics
    Doctor,

    /// Manage the web gateway
    #[command(subcommand)]
    Gateway(GatewayCommand),

    /// Manage sessions
    #[command(subcommand)]
    Sessions(SessionsCommand),

    /// Manage lifecycle hooks
    #[command(subcommand)]
    Hooks(HooksCommand),

    /// Manage scheduled routines (cron jobs)
    #[command(subcommand)]
    Cron(CronCommand),

    /// Query and search logs
    #[command(subcommand)]
    Logs(LogsCommand),

    /// Send messages to channels
    #[command(subcommand)]
    Message(MessageCommand),

    /// Manage input channels (telegram, slack, webchat, etc.)
    #[command(subcommand)]
    Channels(ChannelsCommand),

    /// Manage plugins (WASM tools and MCP servers)
    #[command(subcommand)]
    Plugins(PluginsCommand),

    /// Manage outbound webhooks
    #[command(subcommand)]
    Webhooks(WebhooksCommand),

    /// Manage skills (capability bundles)
    #[command(subcommand)]
    Skills(SkillsCommand),

    /// Manage agent identities
    #[command(subcommand)]
    Agents(AgentsCommand),

    /// Manage remote IronClaw nodes
    #[command(subcommand)]
    Nodes(NodesCommand),

    /// Manage browser automation sessions
    #[command(subcommand)]
    Browser(BrowserCommand),

    /// Generate shell completion scripts
    Completion {
        /// Shell to generate completions for (bash, zsh, fish, powershell, elvish)
        shell: String,
    },

    /// Self-update to the latest version
    Update {
        /// Check for updates without installing
        #[arg(long)]
        check: bool,
    },

    /// Run as a sandboxed worker inside a Docker container (internal use).
    /// This is invoked automatically by the orchestrator, not by users directly.
    Worker {
        /// Job ID to execute.
        #[arg(long)]
        job_id: uuid::Uuid,

        /// URL of the orchestrator's internal API.
        #[arg(long, default_value = "http://host.docker.internal:50051")]
        orchestrator_url: String,

        /// Maximum iterations before stopping.
        #[arg(long, default_value = "50")]
        max_iterations: u32,
    },

    /// Run as a Claude Code bridge inside a Docker container (internal use).
    /// Spawns the `claude` CLI and streams output back to the orchestrator.
    ClaudeBridge {
        /// Job ID to execute.
        #[arg(long)]
        job_id: uuid::Uuid,

        /// URL of the orchestrator's internal API.
        #[arg(long, default_value = "http://host.docker.internal:50051")]
        orchestrator_url: String,

        /// Maximum agentic turns for Claude Code.
        #[arg(long, default_value = "50")]
        max_turns: u32,

        /// Claude model to use (e.g. "sonnet", "opus").
        #[arg(long, default_value = "sonnet")]
        model: String,
    },
}

impl Cli {
    /// Check if we should run the agent (default behavior or explicit `run` command).
    pub fn should_run_agent(&self) -> bool {
        matches!(self.command, None | Some(Command::Run))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_no_args() {
        let cli = Cli::try_parse_from(["ironclaw"]).unwrap();
        assert!(cli.command.is_none());
        assert!(!cli.cli_only);
        assert!(!cli.no_db);
        assert!(cli.message.is_none());
    }

    #[test]
    fn parse_cli_only_flag() {
        let cli = Cli::try_parse_from(["ironclaw", "--cli-only"]).unwrap();
        assert!(cli.cli_only);
    }

    #[test]
    fn parse_no_db_flag() {
        let cli = Cli::try_parse_from(["ironclaw", "--no-db"]).unwrap();
        assert!(cli.no_db);
    }

    #[test]
    fn parse_message_flag() {
        let cli = Cli::try_parse_from(["ironclaw", "-m", "hello"]).unwrap();
        assert_eq!(cli.message.as_deref(), Some("hello"));
    }

    #[test]
    fn parse_message_long_flag() {
        let cli = Cli::try_parse_from(["ironclaw", "--message", "world"]).unwrap();
        assert_eq!(cli.message.as_deref(), Some("world"));
    }

    #[test]
    fn should_run_agent_no_command() {
        let cli = Cli::try_parse_from(["ironclaw"]).unwrap();
        assert!(cli.should_run_agent());
    }

    #[test]
    fn should_run_agent_run_command() {
        let cli = Cli::try_parse_from(["ironclaw", "run"]).unwrap();
        assert!(cli.should_run_agent());
    }

    #[test]
    fn should_run_agent_false_for_status() {
        let cli = Cli::try_parse_from(["ironclaw", "status"]).unwrap();
        assert!(!cli.should_run_agent());
    }

    #[test]
    fn should_run_agent_false_for_doctor() {
        let cli = Cli::try_parse_from(["ironclaw", "doctor"]).unwrap();
        assert!(!cli.should_run_agent());
    }

    #[test]
    fn command_run_variant() {
        let cli = Cli::try_parse_from(["ironclaw", "run"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Run)));
    }

    #[test]
    fn command_status_variant() {
        let cli = Cli::try_parse_from(["ironclaw", "status"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Status)));
    }

    #[test]
    fn command_doctor_variant() {
        let cli = Cli::try_parse_from(["ironclaw", "doctor"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Doctor)));
    }

    #[test]
    fn command_completion_variant() {
        let cli = Cli::try_parse_from(["ironclaw", "completion", "bash"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Completion { .. })));
    }

    #[test]
    fn parse_config_flag() {
        let cli = Cli::try_parse_from(["ironclaw", "-c", "/tmp/config.toml"]).unwrap();
        assert!(cli.config.is_some());
    }

    #[test]
    fn parse_no_onboard_flag() {
        let cli = Cli::try_parse_from(["ironclaw", "--no-onboard"]).unwrap();
        assert!(cli.no_onboard);
    }
}
