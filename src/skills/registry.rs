//! Skill registry for managing modular capability bundles.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// A skill — a named bundle of tools, prompts, and policies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Unique skill identifier.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Version string.
    pub version: String,
    /// Tools included in this skill.
    pub tools: Vec<SkillTool>,
    /// System prompt additions when this skill is active.
    pub system_prompt: Option<String>,
    /// Configuration for this skill.
    pub config: SkillConfig,
    /// Whether this skill is currently enabled.
    pub enabled: bool,
    /// Tags for categorization.
    pub tags: Vec<String>,
}

/// A tool reference within a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTool {
    /// Tool name.
    pub name: String,
    /// Whether this tool requires approval within this skill context.
    pub requires_approval: bool,
    /// Custom policy for this tool (allow/deny patterns).
    pub policy: Option<ToolPolicy>,
}

/// Tool policy within a skill context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPolicy {
    /// Allowed parameter patterns.
    pub allow: Vec<String>,
    /// Denied parameter patterns.
    pub deny: Vec<String>,
}

/// Skill-level configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillConfig {
    /// Maximum concurrent tool calls for this skill.
    pub max_concurrent: Option<u32>,
    /// Timeout in seconds for skill operations.
    pub timeout_secs: Option<u64>,
    /// Custom metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Status of a skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillStatus {
    /// Skill is active and available.
    Active,
    /// Skill is installed but disabled.
    Disabled,
    /// Skill has an error and cannot be used.
    Error,
}

/// Registry for managing skills.
pub struct SkillRegistry {
    skills: Arc<RwLock<HashMap<String, Skill>>>,
}

impl SkillRegistry {
    /// Create a new skill registry.
    pub fn new() -> Self {
        Self {
            skills: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a skill.
    pub async fn register(&self, skill: Skill) -> Result<(), crate::error::SkillsError> {
        if skill.name.is_empty() {
            return Err(crate::error::SkillsError::InvalidDefinition {
                reason: "Skill name cannot be empty".to_string(),
            });
        }

        let mut skills = self.skills.write().await;
        skills.insert(skill.name.clone(), skill);
        Ok(())
    }

    /// Unregister a skill.
    pub async fn unregister(&self, name: &str) -> bool {
        self.skills.write().await.remove(name).is_some()
    }

    /// Get a skill by name.
    pub async fn get(&self, name: &str) -> Option<Skill> {
        self.skills.read().await.get(name).cloned()
    }

    /// List all registered skills.
    pub async fn list(&self) -> Vec<Skill> {
        self.skills.read().await.values().cloned().collect()
    }

    /// List active (enabled) skills.
    pub async fn list_active(&self) -> Vec<Skill> {
        self.skills
            .read()
            .await
            .values()
            .filter(|s| s.enabled)
            .cloned()
            .collect()
    }

    /// Enable or disable a skill.
    pub async fn set_enabled(&self, name: &str, enabled: bool) -> bool {
        let mut skills = self.skills.write().await;
        if let Some(skill) = skills.get_mut(name) {
            skill.enabled = enabled;
            true
        } else {
            false
        }
    }

    /// Get all tool names from active skills.
    pub async fn active_tools(&self) -> Vec<String> {
        self.skills
            .read()
            .await
            .values()
            .filter(|s| s.enabled)
            .flat_map(|s| s.tools.iter().map(|t| t.name.clone()))
            .collect()
    }

    /// Get combined system prompt additions from active skills.
    pub async fn system_prompt_additions(&self) -> String {
        let skills = self.skills.read().await;
        skills
            .values()
            .filter(|s| s.enabled)
            .filter_map(|s| s.system_prompt.as_deref())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Register built-in default skills.
    pub async fn register_defaults(&self) {
        let coding = Skill {
            name: "coding".to_string(),
            description: "Software development capabilities".to_string(),
            version: "1.0.0".to_string(),
            tools: vec![
                SkillTool {
                    name: "shell".to_string(),
                    requires_approval: true,
                    policy: None,
                },
                SkillTool {
                    name: "read_file".to_string(),
                    requires_approval: false,
                    policy: None,
                },
                SkillTool {
                    name: "write_file".to_string(),
                    requires_approval: true,
                    policy: None,
                },
                SkillTool {
                    name: "apply_patch".to_string(),
                    requires_approval: true,
                    policy: None,
                },
                SkillTool {
                    name: "list_dir".to_string(),
                    requires_approval: false,
                    policy: None,
                },
            ],
            system_prompt: Some(
                "You have software development capabilities. You can read, write, and modify files, and execute shell commands."
                    .to_string(),
            ),
            config: SkillConfig::default(),
            enabled: true,
            tags: vec!["development".to_string(), "coding".to_string()],
        };

        let research = Skill {
            name: "research".to_string(),
            description: "Web research and information gathering".to_string(),
            version: "1.0.0".to_string(),
            tools: vec![
                SkillTool {
                    name: "http".to_string(),
                    requires_approval: false,
                    policy: None,
                },
                SkillTool {
                    name: "memory_search".to_string(),
                    requires_approval: false,
                    policy: None,
                },
                SkillTool {
                    name: "memory_write".to_string(),
                    requires_approval: false,
                    policy: None,
                },
            ],
            system_prompt: Some(
                "You have research capabilities. You can make HTTP requests and search/store information in memory."
                    .to_string(),
            ),
            config: SkillConfig::default(),
            enabled: true,
            tags: vec!["research".to_string(), "web".to_string()],
        };

        let automation = Skill {
            name: "automation".to_string(),
            description: "Task automation and scheduling".to_string(),
            version: "1.0.0".to_string(),
            tools: vec![
                SkillTool {
                    name: "create_job".to_string(),
                    requires_approval: false,
                    policy: None,
                },
                SkillTool {
                    name: "routine_create".to_string(),
                    requires_approval: true,
                    policy: None,
                },
                SkillTool {
                    name: "routine_list".to_string(),
                    requires_approval: false,
                    policy: None,
                },
            ],
            system_prompt: Some(
                "You have automation capabilities. You can create jobs and scheduled routines."
                    .to_string(),
            ),
            config: SkillConfig::default(),
            enabled: true,
            tags: vec!["automation".to_string(), "scheduling".to_string()],
        };

        let _ = self.register(coding).await;
        let _ = self.register(research).await;
        let _ = self.register(automation).await;
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_skill() -> Skill {
        Skill {
            name: "test".to_string(),
            description: "Test skill".to_string(),
            version: "1.0.0".to_string(),
            tools: vec![SkillTool {
                name: "echo".to_string(),
                requires_approval: false,
                policy: None,
            }],
            system_prompt: Some("Test prompt".to_string()),
            config: SkillConfig::default(),
            enabled: true,
            tags: vec!["test".to_string()],
        }
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let registry = SkillRegistry::new();
        registry.register(test_skill()).await.unwrap();

        let skill = registry.get("test").await;
        assert!(skill.is_some());
        assert_eq!(skill.unwrap().name, "test");
    }

    #[tokio::test]
    async fn test_list_active() {
        let registry = SkillRegistry::new();
        let mut skill = test_skill();
        registry.register(skill.clone()).await.unwrap();

        skill.name = "disabled".to_string();
        skill.enabled = false;
        registry.register(skill).await.unwrap();

        let active = registry.list_active().await;
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "test");
    }

    #[tokio::test]
    async fn test_toggle_enabled() {
        let registry = SkillRegistry::new();
        registry.register(test_skill()).await.unwrap();

        registry.set_enabled("test", false).await;
        let skill = registry.get("test").await.unwrap();
        assert!(!skill.enabled);

        registry.set_enabled("test", true).await;
        let skill = registry.get("test").await.unwrap();
        assert!(skill.enabled);
    }

    #[tokio::test]
    async fn test_register_defaults() {
        let registry = SkillRegistry::new();
        registry.register_defaults().await;

        let skills = registry.list().await;
        assert!(skills.len() >= 3);
    }

    #[tokio::test]
    async fn test_system_prompt_additions() {
        let registry = SkillRegistry::new();
        registry.register(test_skill()).await.unwrap();

        let prompt = registry.system_prompt_additions().await;
        assert!(prompt.contains("Test prompt"));
    }
}
