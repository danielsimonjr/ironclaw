//! Hooks management CLI commands.

use clap::Subcommand;

#[derive(Subcommand, Debug, Clone)]
pub enum HooksCommand {
    /// List registered hooks
    List {
        /// Filter by hook type (beforeInbound, beforeOutbound, beforeToolCall, etc.)
        #[arg(short, long)]
        r#type: Option<String>,
    },

    /// Add a new hook
    Add {
        /// Hook name
        name: String,

        /// Hook type (beforeInbound, beforeOutbound, beforeToolCall, onSessionStart, onSessionEnd, transformResponse)
        #[arg(short, long)]
        r#type: String,

        /// Shell command to execute
        #[arg(long, group = "action")]
        shell: Option<String>,

        /// HTTP URL to call
        #[arg(long, group = "action")]
        http: Option<String>,

        /// Webhook URL to POST to
        #[arg(long, group = "action")]
        webhook: Option<String>,

        /// Priority (system, high, normal, low)
        #[arg(short, long, default_value = "normal")]
        priority: String,
    },

    /// Remove a hook
    Remove {
        /// Hook name
        name: String,

        /// Hook type
        #[arg(short, long)]
        r#type: String,
    },

    /// Enable a hook
    Enable {
        /// Hook name
        name: String,

        /// Hook type
        #[arg(short, long)]
        r#type: String,
    },

    /// Disable a hook
    Disable {
        /// Hook name
        name: String,

        /// Hook type
        #[arg(short, long)]
        r#type: String,
    },
}

/// Run a hooks command.
pub async fn run_hooks_command(cmd: HooksCommand) -> anyhow::Result<()> {
    match cmd {
        HooksCommand::List { r#type } => list_hooks(r#type).await,
        HooksCommand::Add {
            name,
            r#type,
            shell,
            http,
            webhook,
            priority,
        } => add_hook(name, r#type, shell, http, webhook, priority).await,
        HooksCommand::Remove { name, r#type } => remove_hook(name, r#type).await,
        HooksCommand::Enable { name, r#type } => toggle_hook(name, r#type, true).await,
        HooksCommand::Disable { name, r#type } => toggle_hook(name, r#type, false).await,
    }
}

async fn list_hooks(type_filter: Option<String>) -> anyhow::Result<()> {
    let engine = crate::hooks::HookEngine::new();

    let hooks = engine.list_hooks().await;

    if hooks.is_empty() {
        println!("No hooks registered.");
        println!("\nTo add a hook:");
        println!("  ironclaw hooks add my-hook --type beforeInbound --shell 'echo hello'");
        return Ok(());
    }

    println!("Registered Hooks:");
    println!();

    for hook in &hooks {
        let type_str = hook.hook_type.to_string();
        if let Some(ref filter) = type_filter
            && !type_str.contains(filter)
        {
            continue;
        }

        let status = if hook.enabled { "enabled" } else { "disabled" };
        println!("  {} [{}] ({})", hook.name, type_str, status);
        if !hook.description.is_empty() {
            println!("    Description: {}", hook.description);
        }
        println!("    Priority: {:?}", hook.priority);
        println!("    Source: {:?}", hook.source);
        println!();
    }

    Ok(())
}

async fn add_hook(
    name: String,
    hook_type: String,
    shell: Option<String>,
    http: Option<String>,
    webhook: Option<String>,
    priority: String,
) -> anyhow::Result<()> {
    let ht = parse_hook_type(&hook_type)?;
    let action = if let Some(cmd) = shell {
        crate::hooks::HookAction::Shell { command: cmd }
    } else if let Some(url) = http {
        crate::hooks::HookAction::Http {
            url,
            method: "POST".to_string(),
        }
    } else if let Some(url) = webhook {
        crate::hooks::HookAction::Webhook { url }
    } else {
        anyhow::bail!("Must specify one of --shell, --http, or --webhook");
    };

    let prio = match priority.to_lowercase().as_str() {
        "system" => crate::hooks::HookPriority::System,
        "high" => crate::hooks::HookPriority::High,
        "normal" => crate::hooks::HookPriority::Normal,
        "low" => crate::hooks::HookPriority::Low,
        _ => anyhow::bail!(
            "Invalid priority: {}. Use system, high, normal, or low",
            priority
        ),
    };

    let hook = crate::hooks::Hook {
        name: name.clone(),
        description: String::new(),
        hook_type: ht,
        action,
        priority: prio,
        source: crate::hooks::HookSource::Config,
        enabled: true,
        timeout_ms: 5000,
    };

    let engine = crate::hooks::HookEngine::new();
    engine
        .register(hook)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    println!("Hook '{}' added for {} event.", name, hook_type);
    Ok(())
}

async fn remove_hook(name: String, hook_type: String) -> anyhow::Result<()> {
    let ht = parse_hook_type(&hook_type)?;
    let engine = crate::hooks::HookEngine::new();

    if engine.unregister(ht, &name).await {
        println!("Hook '{}' removed.", name);
    } else {
        println!("Hook '{}' not found for {} event.", name, hook_type);
    }

    Ok(())
}

async fn toggle_hook(name: String, hook_type: String, enabled: bool) -> anyhow::Result<()> {
    let ht = parse_hook_type(&hook_type)?;
    let engine = crate::hooks::HookEngine::new();

    if engine.set_enabled(ht, &name, enabled).await {
        let status = if enabled { "enabled" } else { "disabled" };
        println!("Hook '{}' {}.", name, status);
    } else {
        println!("Hook '{}' not found for {} event.", name, hook_type);
    }

    Ok(())
}

fn parse_hook_type(s: &str) -> anyhow::Result<crate::hooks::HookType> {
    match s.to_lowercase().as_str() {
        "beforeinbound" | "before_inbound" | "before-inbound" => {
            Ok(crate::hooks::HookType::BeforeInbound)
        }
        "beforeoutbound" | "before_outbound" | "before-outbound" => {
            Ok(crate::hooks::HookType::BeforeOutbound)
        }
        "beforetoolcall" | "before_tool_call" | "before-tool-call" => {
            Ok(crate::hooks::HookType::BeforeToolCall)
        }
        "onsessionstart" | "on_session_start" | "on-session-start" => {
            Ok(crate::hooks::HookType::OnSessionStart)
        }
        "onsessionend" | "on_session_end" | "on-session-end" => {
            Ok(crate::hooks::HookType::OnSessionEnd)
        }
        "transformresponse" | "transform_response" | "transform-response" => {
            Ok(crate::hooks::HookType::TransformResponse)
        }
        "transcribeaudio" | "transcribe_audio" | "transcribe-audio" => {
            Ok(crate::hooks::HookType::TranscribeAudio)
        }
        _ => anyhow::bail!(
            "Unknown hook type: {}. Valid types: beforeInbound, beforeOutbound, beforeToolCall, onSessionStart, onSessionEnd, transformResponse, transcribeAudio",
            s
        ),
    }
}
