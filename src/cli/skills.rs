//! Skills management CLI commands.

use clap::Subcommand;

/// Skills management commands.
#[derive(Subcommand, Debug)]
pub enum SkillsCommand {
    /// List all registered skills.
    List,
    /// Enable a skill.
    Enable {
        /// Skill name.
        name: String,
    },
    /// Disable a skill.
    Disable {
        /// Skill name.
        name: String,
    },
    /// Show skill details.
    Info {
        /// Skill name.
        name: String,
    },
}

/// Run a skills command.
pub async fn run_skills_command(cmd: &SkillsCommand) -> Result<(), Box<dyn std::error::Error>> {
    use crate::skills::SkillRegistry;

    let registry = SkillRegistry::new();
    registry.register_defaults().await;

    match cmd {
        SkillsCommand::List => {
            println!("Registered skills:");
            let skills = registry.list().await;
            for skill in &skills {
                let status = if skill.enabled { "enabled" } else { "disabled" };
                println!(
                    "  {} v{} [{}] - {}",
                    skill.name, skill.version, status, skill.description
                );
                for tool in &skill.tools {
                    let approval = if tool.requires_approval {
                        " (requires approval)"
                    } else {
                        ""
                    };
                    println!("    - {}{}", tool.name, approval);
                }
            }
            if skills.is_empty() {
                println!("  (none registered)");
            }
        }
        SkillsCommand::Enable { name } => {
            if registry.set_enabled(name, true).await {
                println!("Skill '{}' enabled.", name);
            } else {
                println!("Skill '{}' not found.", name);
            }
        }
        SkillsCommand::Disable { name } => {
            if registry.set_enabled(name, false).await {
                println!("Skill '{}' disabled.", name);
            } else {
                println!("Skill '{}' not found.", name);
            }
        }
        SkillsCommand::Info { name } => match registry.get(name).await {
            Some(skill) => {
                println!("Skill: {}", skill.name);
                println!("  Version: {}", skill.version);
                println!("  Description: {}", skill.description);
                println!(
                    "  Status: {}",
                    if skill.enabled { "enabled" } else { "disabled" }
                );
                println!("  Tags: {}", skill.tags.join(", "));
                println!("  Tools:");
                for tool in &skill.tools {
                    println!("    - {}", tool.name);
                }
                if let Some(prompt) = &skill.system_prompt {
                    println!("  System prompt: {}", prompt);
                }
            }
            None => println!("Skill '{}' not found.", name),
        },
    }
    Ok(())
}
