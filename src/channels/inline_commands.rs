//! Inline chat command parser for non-REPL channels.
//!
//! Provides portable slash command parsing that works across all channels
//! (web gateway, Telegram, Slack, HTTP webhook, etc.). Converts recognized
//! commands into [`ParsedCommand`] variants that the agent loop can process.
//!
//! # Usage
//!
//! ```rust
//! use ironclaw::channels::inline_commands::{parse_inline_command, InlineCommandConfig};
//!
//! let config = InlineCommandConfig::default();
//! let result = parse_inline_command("/help", &config);
//! // result is ParsedCommand::Command { name: "help", args: [], raw: "/help" }
//! ```
//!
//! Channels that want inline command support call [`parse_inline_command`] on
//! each incoming message before forwarding it to the agent loop.

use serde::{Deserialize, Serialize};

/// Result of parsing a potential inline command from any channel.
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedCommand {
    /// A recognized slash command.
    Command {
        /// Canonical (lowercased) command name without the prefix.
        name: String,
        /// Positional arguments following the command.
        args: Vec<String>,
        /// The original raw input string.
        raw: String,
    },
    /// An approval response (yes/no/always).
    Approval {
        /// Whether execution was approved.
        approved: bool,
        /// Whether approval applies for the rest of the session.
        always: bool,
    },
    /// Regular user input (not a command).
    UserInput(String),
}

/// Configuration for inline command parsing.
#[derive(Debug, Clone)]
pub struct InlineCommandConfig {
    /// Whether to enable slash command parsing.
    pub enabled: bool,
    /// Prefix for commands (default: "/").
    pub prefix: String,
    /// Commands that are allowed in this channel.
    /// Empty means all recognized commands are allowed.
    pub allowed_commands: Vec<String>,
    /// Commands that are blocked in this channel.
    pub blocked_commands: Vec<String>,
}

impl Default for InlineCommandConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            prefix: "/".to_string(),
            allowed_commands: Vec::new(),
            blocked_commands: vec![
                "quit".to_string(),
                "exit".to_string(),
                "shutdown".to_string(),
            ],
        }
    }
}

/// Category of a command for grouping in help output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandCategory {
    /// Session lifecycle: /new, /clear, /thread.
    Session,
    /// Navigation: /undo, /redo, /resume.
    Navigation,
    /// Information: /help, /version, /tools, /status, /ping.
    Information,
    /// Model selection: /model.
    Model,
    /// Context management: /compact, /summarize.
    Context,
    /// Actions: /suggest, /heartbeat, /interrupt, /cancel.
    Action,
}

/// Descriptor for a recognized command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandInfo {
    /// Command name (without prefix).
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Argument syntax hint (e.g. "[model_name]", "<thread_id>").
    pub args: String,
    /// Grouping category.
    pub category: CommandCategory,
}

/// All recognized commands with their metadata.
pub fn available_commands() -> Vec<CommandInfo> {
    vec![
        // Session
        CommandInfo {
            name: "new".to_string(),
            description: "Start a new conversation thread".to_string(),
            args: String::new(),
            category: CommandCategory::Session,
        },
        CommandInfo {
            name: "clear".to_string(),
            description: "Clear the current conversation".to_string(),
            args: String::new(),
            category: CommandCategory::Session,
        },
        CommandInfo {
            name: "thread".to_string(),
            description: "Switch to a thread by ID, or 'new' for a new thread".to_string(),
            args: "<thread_id|new>".to_string(),
            category: CommandCategory::Session,
        },
        // Navigation
        CommandInfo {
            name: "undo".to_string(),
            description: "Undo the last turn".to_string(),
            args: String::new(),
            category: CommandCategory::Navigation,
        },
        CommandInfo {
            name: "redo".to_string(),
            description: "Redo an undone turn".to_string(),
            args: String::new(),
            category: CommandCategory::Navigation,
        },
        CommandInfo {
            name: "resume".to_string(),
            description: "Resume from a checkpoint".to_string(),
            args: "<checkpoint_id>".to_string(),
            category: CommandCategory::Navigation,
        },
        // Information
        CommandInfo {
            name: "help".to_string(),
            description: "Show available commands".to_string(),
            args: String::new(),
            category: CommandCategory::Information,
        },
        CommandInfo {
            name: "version".to_string(),
            description: "Show the current version".to_string(),
            args: String::new(),
            category: CommandCategory::Information,
        },
        CommandInfo {
            name: "tools".to_string(),
            description: "List available tools".to_string(),
            args: String::new(),
            category: CommandCategory::Information,
        },
        CommandInfo {
            name: "status".to_string(),
            description: "Show agent status".to_string(),
            args: String::new(),
            category: CommandCategory::Information,
        },
        CommandInfo {
            name: "ping".to_string(),
            description: "Check if the agent is alive".to_string(),
            args: String::new(),
            category: CommandCategory::Information,
        },
        // Model
        CommandInfo {
            name: "model".to_string(),
            description: "Show or switch the current model".to_string(),
            args: "[model_name]".to_string(),
            category: CommandCategory::Model,
        },
        // Context
        CommandInfo {
            name: "compact".to_string(),
            description: "Compact the context window".to_string(),
            args: String::new(),
            category: CommandCategory::Context,
        },
        CommandInfo {
            name: "summarize".to_string(),
            description: "Summarize the current thread".to_string(),
            args: String::new(),
            category: CommandCategory::Context,
        },
        // Action
        CommandInfo {
            name: "suggest".to_string(),
            description: "Suggest next steps".to_string(),
            args: String::new(),
            category: CommandCategory::Action,
        },
        CommandInfo {
            name: "heartbeat".to_string(),
            description: "Trigger a manual heartbeat check".to_string(),
            args: String::new(),
            category: CommandCategory::Action,
        },
        CommandInfo {
            name: "interrupt".to_string(),
            description: "Stop the current operation".to_string(),
            args: String::new(),
            category: CommandCategory::Action,
        },
        CommandInfo {
            name: "cancel".to_string(),
            description: "Cancel a running job".to_string(),
            args: "[job_id]".to_string(),
            category: CommandCategory::Action,
        },
    ]
}

/// Canonical set of recognized command names for fast lookup.
fn recognized_commands() -> &'static [&'static str] {
    &[
        "new",
        "clear",
        "thread",
        "undo",
        "redo",
        "resume",
        "help",
        "version",
        "tools",
        "status",
        "ping",
        "model",
        "compact",
        "summarize",
        "summary",
        "suggest",
        "heartbeat",
        "interrupt",
        "stop",
        "cancel",
        "debug",
        "job",
        "list",
    ]
}

/// Parse a message from any channel into a command or user input.
///
/// The function checks for:
/// 1. Approval responses (yes/no/always and variants).
/// 2. Slash commands starting with the configured prefix.
/// 3. Everything else is returned as [`ParsedCommand::UserInput`].
///
/// Commands are matched case-insensitively. The `name` field of a returned
/// [`ParsedCommand::Command`] is always lowercased.
pub fn parse_inline_command(input: &str, config: &InlineCommandConfig) -> ParsedCommand {
    let trimmed = input.trim();

    // Empty or whitespace-only input is regular user input.
    if trimmed.is_empty() {
        return ParsedCommand::UserInput(input.to_string());
    }

    // Check for approval responses first (these don't need the prefix).
    match trimmed.to_lowercase().as_str() {
        "yes" | "y" | "approve" | "ok" => {
            return ParsedCommand::Approval {
                approved: true,
                always: false,
            };
        }
        "always" | "a" | "yes always" | "approve always" => {
            return ParsedCommand::Approval {
                approved: true,
                always: true,
            };
        }
        "no" | "n" | "deny" | "reject" => {
            return ParsedCommand::Approval {
                approved: false,
                always: false,
            };
        }
        _ => {}
    }

    // If command parsing is disabled, everything else is user input.
    if !config.enabled {
        return ParsedCommand::UserInput(input.to_string());
    }

    // Check for the command prefix.
    if !trimmed.starts_with(&config.prefix) {
        return ParsedCommand::UserInput(input.to_string());
    }

    // Strip the prefix and split into command + args.
    let without_prefix = &trimmed[config.prefix.len()..];
    if without_prefix.is_empty() {
        return ParsedCommand::UserInput(input.to_string());
    }

    let parts: Vec<&str> = without_prefix.split_whitespace().collect();
    if parts.is_empty() {
        return ParsedCommand::UserInput(input.to_string());
    }

    let cmd_name = parts[0].to_lowercase();
    let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

    // Check if this is a recognized command.
    if !recognized_commands().contains(&cmd_name.as_str()) {
        return ParsedCommand::UserInput(input.to_string());
    }

    // Check the blocked list.
    if config
        .blocked_commands
        .iter()
        .any(|b| b.to_lowercase() == cmd_name)
    {
        return ParsedCommand::UserInput(input.to_string());
    }

    // Check the allowed list (empty = all allowed).
    if !config.allowed_commands.is_empty()
        && !config
            .allowed_commands
            .iter()
            .any(|a| a.to_lowercase() == cmd_name)
    {
        return ParsedCommand::UserInput(input.to_string());
    }

    ParsedCommand::Command {
        name: cmd_name,
        args,
        raw: input.to_string(),
    }
}

/// Format a help message listing available commands.
///
/// The output is plain-text (no ANSI escapes) so it renders well in any
/// channel. Commands that are blocked or not in the allowed list for the
/// given config are omitted.
pub fn format_help(config: &InlineCommandConfig) -> String {
    let commands = available_commands();
    let prefix = &config.prefix;

    let visible: Vec<&CommandInfo> = commands
        .iter()
        .filter(|cmd| {
            // Exclude blocked commands.
            if config
                .blocked_commands
                .iter()
                .any(|b| b.to_lowercase() == cmd.name)
            {
                return false;
            }
            // If an allowed list is specified, only include those.
            if !config.allowed_commands.is_empty()
                && !config
                    .allowed_commands
                    .iter()
                    .any(|a| a.to_lowercase() == cmd.name)
            {
                return false;
            }
            true
        })
        .collect();

    // Group by category.
    let categories = [
        (CommandCategory::Session, "Session"),
        (CommandCategory::Navigation, "Navigation"),
        (CommandCategory::Information, "Information"),
        (CommandCategory::Model, "Model"),
        (CommandCategory::Context, "Context"),
        (CommandCategory::Action, "Action"),
    ];

    let mut lines: Vec<String> = Vec::new();
    lines.push("Available commands:".to_string());
    lines.push(String::new());

    for (cat, label) in &categories {
        let in_cat: Vec<&&CommandInfo> = visible.iter().filter(|c| c.category == *cat).collect();
        if in_cat.is_empty() {
            continue;
        }

        lines.push(format!("  {label}"));

        for cmd in in_cat {
            let name_and_args = if cmd.args.is_empty() {
                format!("{prefix}{}", cmd.name)
            } else {
                format!("{prefix}{} {}", cmd.name, cmd.args)
            };
            lines.push(format!("    {name_and_args:<30} {}", cmd.description));
        }

        lines.push(String::new());
    }

    lines.push("  Approval responses".to_string());
    lines.push("    yes (y)                        Approve tool execution".to_string());
    lines.push("    no (n)                         Deny tool execution".to_string());
    lines.push("    always (a)                     Approve for this session".to_string());

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // Basic command recognition
    // ---------------------------------------------------------------

    #[test]
    fn test_parse_help() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/help", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "help".to_string(),
                args: vec![],
                raw: "/help".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_undo() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/undo", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "undo".to_string(),
                args: vec![],
                raw: "/undo".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_redo() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/redo", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "redo".to_string(),
                args: vec![],
                raw: "/redo".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_new() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/new", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "new".to_string(),
                args: vec![],
                raw: "/new".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_clear() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/clear", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "clear".to_string(),
                args: vec![],
                raw: "/clear".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_compact() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/compact", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "compact".to_string(),
                args: vec![],
                raw: "/compact".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_version() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/version", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "version".to_string(),
                args: vec![],
                raw: "/version".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_tools() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/tools", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "tools".to_string(),
                args: vec![],
                raw: "/tools".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_status() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/status", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "status".to_string(),
                args: vec![],
                raw: "/status".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_ping() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/ping", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "ping".to_string(),
                args: vec![],
                raw: "/ping".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_interrupt() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/interrupt", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "interrupt".to_string(),
                args: vec![],
                raw: "/interrupt".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_heartbeat() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/heartbeat", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "heartbeat".to_string(),
                args: vec![],
                raw: "/heartbeat".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_suggest() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/suggest", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "suggest".to_string(),
                args: vec![],
                raw: "/suggest".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_summarize() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/summarize", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "summarize".to_string(),
                args: vec![],
                raw: "/summarize".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_summary_alias() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/summary", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "summary".to_string(),
                args: vec![],
                raw: "/summary".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_cancel() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/cancel", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "cancel".to_string(),
                args: vec![],
                raw: "/cancel".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_debug() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/debug", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "debug".to_string(),
                args: vec![],
                raw: "/debug".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_stop_alias() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/stop", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "stop".to_string(),
                args: vec![],
                raw: "/stop".to_string(),
            }
        );
    }

    // ---------------------------------------------------------------
    // Commands with arguments
    // ---------------------------------------------------------------

    #[test]
    fn test_parse_model_with_arg() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/model gpt-4o", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "model".to_string(),
                args: vec!["gpt-4o".to_string()],
                raw: "/model gpt-4o".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_model_no_args() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/model", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "model".to_string(),
                args: vec![],
                raw: "/model".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_thread_with_id() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/thread abc-123-def", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "thread".to_string(),
                args: vec!["abc-123-def".to_string()],
                raw: "/thread abc-123-def".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_thread_new() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/thread new", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "thread".to_string(),
                args: vec!["new".to_string()],
                raw: "/thread new".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_resume_with_id() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/resume 550e8400-e29b-41d4-a716-446655440000", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "resume".to_string(),
                args: vec!["550e8400-e29b-41d4-a716-446655440000".to_string()],
                raw: "/resume 550e8400-e29b-41d4-a716-446655440000".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_command_multiple_args() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/model claude-3 opus", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "model".to_string(),
                args: vec!["claude-3".to_string(), "opus".to_string()],
                raw: "/model claude-3 opus".to_string(),
            }
        );
    }

    // ---------------------------------------------------------------
    // Case-insensitive matching
    // ---------------------------------------------------------------

    #[test]
    fn test_case_insensitive_help() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/HELP", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "help".to_string(),
                args: vec![],
                raw: "/HELP".to_string(),
            }
        );
    }

    #[test]
    fn test_case_insensitive_mixed() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/Model GPT-4", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "model".to_string(),
                args: vec!["GPT-4".to_string()],
                raw: "/Model GPT-4".to_string(),
            }
        );
    }

    #[test]
    fn test_case_insensitive_undo() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/UNDO", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "undo".to_string(),
                args: vec![],
                raw: "/UNDO".to_string(),
            }
        );
    }

    // ---------------------------------------------------------------
    // Approval responses
    // ---------------------------------------------------------------

    #[test]
    fn test_approval_yes() {
        let config = InlineCommandConfig::default();
        assert_eq!(
            parse_inline_command("yes", &config),
            ParsedCommand::Approval {
                approved: true,
                always: false,
            }
        );
    }

    #[test]
    fn test_approval_y() {
        let config = InlineCommandConfig::default();
        assert_eq!(
            parse_inline_command("y", &config),
            ParsedCommand::Approval {
                approved: true,
                always: false,
            }
        );
    }

    #[test]
    fn test_approval_approve() {
        let config = InlineCommandConfig::default();
        assert_eq!(
            parse_inline_command("approve", &config),
            ParsedCommand::Approval {
                approved: true,
                always: false,
            }
        );
    }

    #[test]
    fn test_approval_ok() {
        let config = InlineCommandConfig::default();
        assert_eq!(
            parse_inline_command("ok", &config),
            ParsedCommand::Approval {
                approved: true,
                always: false,
            }
        );
    }

    #[test]
    fn test_approval_no() {
        let config = InlineCommandConfig::default();
        assert_eq!(
            parse_inline_command("no", &config),
            ParsedCommand::Approval {
                approved: false,
                always: false,
            }
        );
    }

    #[test]
    fn test_approval_n() {
        let config = InlineCommandConfig::default();
        assert_eq!(
            parse_inline_command("n", &config),
            ParsedCommand::Approval {
                approved: false,
                always: false,
            }
        );
    }

    #[test]
    fn test_approval_deny() {
        let config = InlineCommandConfig::default();
        assert_eq!(
            parse_inline_command("deny", &config),
            ParsedCommand::Approval {
                approved: false,
                always: false,
            }
        );
    }

    #[test]
    fn test_approval_reject() {
        let config = InlineCommandConfig::default();
        assert_eq!(
            parse_inline_command("reject", &config),
            ParsedCommand::Approval {
                approved: false,
                always: false,
            }
        );
    }

    #[test]
    fn test_approval_always() {
        let config = InlineCommandConfig::default();
        assert_eq!(
            parse_inline_command("always", &config),
            ParsedCommand::Approval {
                approved: true,
                always: true,
            }
        );
    }

    #[test]
    fn test_approval_a() {
        let config = InlineCommandConfig::default();
        assert_eq!(
            parse_inline_command("a", &config),
            ParsedCommand::Approval {
                approved: true,
                always: true,
            }
        );
    }

    #[test]
    fn test_approval_yes_always() {
        let config = InlineCommandConfig::default();
        assert_eq!(
            parse_inline_command("yes always", &config),
            ParsedCommand::Approval {
                approved: true,
                always: true,
            }
        );
    }

    #[test]
    fn test_approval_approve_always() {
        let config = InlineCommandConfig::default();
        assert_eq!(
            parse_inline_command("approve always", &config),
            ParsedCommand::Approval {
                approved: true,
                always: true,
            }
        );
    }

    #[test]
    fn test_approval_case_insensitive() {
        let config = InlineCommandConfig::default();
        assert_eq!(
            parse_inline_command("YES", &config),
            ParsedCommand::Approval {
                approved: true,
                always: false,
            }
        );
        assert_eq!(
            parse_inline_command("No", &config),
            ParsedCommand::Approval {
                approved: false,
                always: false,
            }
        );
        assert_eq!(
            parse_inline_command("ALWAYS", &config),
            ParsedCommand::Approval {
                approved: true,
                always: true,
            }
        );
    }

    // ---------------------------------------------------------------
    // Custom prefix
    // ---------------------------------------------------------------

    #[test]
    fn test_custom_prefix_bang() {
        let config = InlineCommandConfig {
            prefix: "!".to_string(),
            ..InlineCommandConfig::default()
        };
        let result = parse_inline_command("!help", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "help".to_string(),
                args: vec![],
                raw: "!help".to_string(),
            }
        );
    }

    #[test]
    fn test_custom_prefix_does_not_match_default() {
        let config = InlineCommandConfig {
            prefix: "!".to_string(),
            ..InlineCommandConfig::default()
        };
        let result = parse_inline_command("/help", &config);
        assert_eq!(result, ParsedCommand::UserInput("/help".to_string()));
    }

    #[test]
    fn test_custom_prefix_with_args() {
        let config = InlineCommandConfig {
            prefix: "!".to_string(),
            ..InlineCommandConfig::default()
        };
        let result = parse_inline_command("!model gpt-4", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "model".to_string(),
                args: vec!["gpt-4".to_string()],
                raw: "!model gpt-4".to_string(),
            }
        );
    }

    // ---------------------------------------------------------------
    // Blocked commands
    // ---------------------------------------------------------------

    #[test]
    fn test_blocked_quit() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/quit", &config);
        assert_eq!(result, ParsedCommand::UserInput("/quit".to_string()));
    }

    #[test]
    fn test_blocked_exit() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/exit", &config);
        assert_eq!(result, ParsedCommand::UserInput("/exit".to_string()));
    }

    #[test]
    fn test_blocked_shutdown() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/shutdown", &config);
        assert_eq!(result, ParsedCommand::UserInput("/shutdown".to_string()));
    }

    #[test]
    fn test_blocked_custom() {
        let config = InlineCommandConfig {
            blocked_commands: vec!["model".to_string()],
            ..InlineCommandConfig::default()
        };
        let result = parse_inline_command("/model gpt-4", &config);
        assert_eq!(result, ParsedCommand::UserInput("/model gpt-4".to_string()));
    }

    #[test]
    fn test_blocked_case_insensitive() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/QUIT", &config);
        assert_eq!(result, ParsedCommand::UserInput("/QUIT".to_string()));
    }

    // ---------------------------------------------------------------
    // Allowed commands filtering
    // ---------------------------------------------------------------

    #[test]
    fn test_allowed_commands_only() {
        let config = InlineCommandConfig {
            allowed_commands: vec!["help".to_string(), "ping".to_string()],
            blocked_commands: vec![],
            ..InlineCommandConfig::default()
        };
        // Allowed
        assert!(matches!(
            parse_inline_command("/help", &config),
            ParsedCommand::Command { .. }
        ));
        assert!(matches!(
            parse_inline_command("/ping", &config),
            ParsedCommand::Command { .. }
        ));
        // Not allowed
        assert_eq!(
            parse_inline_command("/undo", &config),
            ParsedCommand::UserInput("/undo".to_string())
        );
    }

    #[test]
    fn test_allowed_empty_means_all() {
        let config = InlineCommandConfig {
            allowed_commands: vec![],
            blocked_commands: vec![],
            ..InlineCommandConfig::default()
        };
        assert!(matches!(
            parse_inline_command("/undo", &config),
            ParsedCommand::Command { .. }
        ));
        assert!(matches!(
            parse_inline_command("/help", &config),
            ParsedCommand::Command { .. }
        ));
    }

    #[test]
    fn test_allowed_and_blocked_interaction() {
        // Blocked takes precedence even if allowed.
        let config = InlineCommandConfig {
            allowed_commands: vec!["help".to_string(), "quit".to_string()],
            blocked_commands: vec!["quit".to_string()],
            ..InlineCommandConfig::default()
        };
        assert!(matches!(
            parse_inline_command("/help", &config),
            ParsedCommand::Command { .. }
        ));
        assert_eq!(
            parse_inline_command("/quit", &config),
            ParsedCommand::UserInput("/quit".to_string())
        );
    }

    // ---------------------------------------------------------------
    // Unknown commands are user input
    // ---------------------------------------------------------------

    #[test]
    fn test_unknown_command_is_user_input() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/foobar", &config);
        assert_eq!(result, ParsedCommand::UserInput("/foobar".to_string()));
    }

    #[test]
    fn test_unknown_command_with_args_is_user_input() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/unknown arg1 arg2", &config);
        assert_eq!(
            result,
            ParsedCommand::UserInput("/unknown arg1 arg2".to_string())
        );
    }

    // ---------------------------------------------------------------
    // Edge cases
    // ---------------------------------------------------------------

    #[test]
    fn test_empty_input() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("", &config);
        assert_eq!(result, ParsedCommand::UserInput("".to_string()));
    }

    #[test]
    fn test_whitespace_only() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("   ", &config);
        assert_eq!(result, ParsedCommand::UserInput("   ".to_string()));
    }

    #[test]
    fn test_just_prefix() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/", &config);
        assert_eq!(result, ParsedCommand::UserInput("/".to_string()));
    }

    #[test]
    fn test_prefix_with_spaces() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/   ", &config);
        // After trimming the prefix part, split_whitespace yields no tokens.
        assert_eq!(result, ParsedCommand::UserInput("/   ".to_string()));
    }

    #[test]
    fn test_regular_message() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("Hello, how are you?", &config);
        assert_eq!(
            result,
            ParsedCommand::UserInput("Hello, how are you?".to_string())
        );
    }

    #[test]
    fn test_slash_in_middle_of_text() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("Please use /help for info", &config);
        assert_eq!(
            result,
            ParsedCommand::UserInput("Please use /help for info".to_string())
        );
    }

    #[test]
    fn test_command_with_leading_whitespace() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("  /help  ", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "help".to_string(),
                args: vec![],
                raw: "  /help  ".to_string(),
            }
        );
    }

    #[test]
    fn test_disabled_config() {
        let config = InlineCommandConfig {
            enabled: false,
            ..InlineCommandConfig::default()
        };
        let result = parse_inline_command("/help", &config);
        assert_eq!(result, ParsedCommand::UserInput("/help".to_string()));
    }

    #[test]
    fn test_disabled_still_detects_approvals() {
        let config = InlineCommandConfig {
            enabled: false,
            ..InlineCommandConfig::default()
        };
        assert_eq!(
            parse_inline_command("yes", &config),
            ParsedCommand::Approval {
                approved: true,
                always: false,
            }
        );
    }

    // ---------------------------------------------------------------
    // Help formatting
    // ---------------------------------------------------------------

    #[test]
    fn test_format_help_contains_all_categories() {
        let config = InlineCommandConfig {
            blocked_commands: vec![],
            ..InlineCommandConfig::default()
        };
        let help = format_help(&config);
        assert!(help.contains("Session"));
        assert!(help.contains("Navigation"));
        assert!(help.contains("Information"));
        assert!(help.contains("Model"));
        assert!(help.contains("Context"));
        assert!(help.contains("Action"));
    }

    #[test]
    fn test_format_help_contains_commands() {
        let config = InlineCommandConfig::default();
        let help = format_help(&config);
        assert!(help.contains("/help"));
        assert!(help.contains("/undo"));
        assert!(help.contains("/model"));
        assert!(help.contains("/compact"));
    }

    #[test]
    fn test_format_help_excludes_blocked() {
        let config = InlineCommandConfig {
            blocked_commands: vec!["help".to_string()],
            ..InlineCommandConfig::default()
        };
        let help = format_help(&config);
        assert!(!help.contains("/help"));
        // Other commands should still be present.
        assert!(help.contains("/undo"));
    }

    #[test]
    fn test_format_help_allowed_filter() {
        let config = InlineCommandConfig {
            allowed_commands: vec!["help".to_string(), "ping".to_string()],
            blocked_commands: vec![],
            ..InlineCommandConfig::default()
        };
        let help = format_help(&config);
        assert!(help.contains("/help"));
        assert!(help.contains("/ping"));
        assert!(!help.contains("/undo"));
        assert!(!help.contains("/model"));
    }

    #[test]
    fn test_format_help_uses_prefix() {
        let config = InlineCommandConfig {
            prefix: "!".to_string(),
            blocked_commands: vec![],
            ..InlineCommandConfig::default()
        };
        let help = format_help(&config);
        assert!(help.contains("!help"));
        assert!(!help.contains("/help"));
    }

    #[test]
    fn test_format_help_contains_approval_section() {
        let config = InlineCommandConfig::default();
        let help = format_help(&config);
        assert!(help.contains("Approval responses"));
        assert!(help.contains("yes (y)"));
        assert!(help.contains("no (n)"));
        assert!(help.contains("always (a)"));
    }

    // ---------------------------------------------------------------
    // available_commands
    // ---------------------------------------------------------------

    #[test]
    fn test_available_commands_non_empty() {
        let cmds = available_commands();
        assert!(!cmds.is_empty());
        assert!(cmds.len() >= 15);
    }

    #[test]
    fn test_available_commands_categories() {
        let cmds = available_commands();
        let categories: Vec<CommandCategory> = cmds.iter().map(|c| c.category).collect();
        assert!(categories.contains(&CommandCategory::Session));
        assert!(categories.contains(&CommandCategory::Navigation));
        assert!(categories.contains(&CommandCategory::Information));
        assert!(categories.contains(&CommandCategory::Model));
        assert!(categories.contains(&CommandCategory::Context));
        assert!(categories.contains(&CommandCategory::Action));
    }

    // ---------------------------------------------------------------
    // InlineCommandConfig defaults
    // ---------------------------------------------------------------

    #[test]
    fn test_default_config() {
        let config = InlineCommandConfig::default();
        assert!(config.enabled);
        assert_eq!(config.prefix, "/");
        assert!(config.allowed_commands.is_empty());
        assert!(config.blocked_commands.contains(&"quit".to_string()));
        assert!(config.blocked_commands.contains(&"exit".to_string()));
        assert!(config.blocked_commands.contains(&"shutdown".to_string()));
    }

    // ---------------------------------------------------------------
    // Extra argument handling
    // ---------------------------------------------------------------

    #[test]
    fn test_cancel_with_job_id() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/cancel job-xyz-123", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "cancel".to_string(),
                args: vec!["job-xyz-123".to_string()],
                raw: "/cancel job-xyz-123".to_string(),
            }
        );
    }

    #[test]
    fn test_args_preserve_case() {
        let config = InlineCommandConfig::default();
        let result = parse_inline_command("/model Claude-3-Opus", &config);
        assert_eq!(
            result,
            ParsedCommand::Command {
                name: "model".to_string(),
                args: vec!["Claude-3-Opus".to_string()],
                raw: "/model Claude-3-Opus".to_string(),
            }
        );
    }
}
