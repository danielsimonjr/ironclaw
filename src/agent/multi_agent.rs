//! Multi-agent routing with workspace isolation.
//!
//! Provides the ability to define multiple agent identities, each with their
//! own system prompt, allowed tools, and isolated workspace prefix. Messages
//! are routed to the most appropriate agent based on intent analysis, channel
//! origin, and explicit `@agent` mentions.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::channels::IncomingMessage;
use crate::error::Error;

/// Identity definition for a single agent persona.
///
/// Each agent has its own system prompt, set of allowed tools, and a workspace
/// prefix that isolates its memory documents from other agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdentity {
    /// Unique name for this agent (e.g., "coder", "researcher", "ops").
    pub name: String,
    /// Human-readable description of this agent's purpose.
    pub description: String,
    /// System prompt injected when this agent handles a message.
    pub system_prompt: String,
    /// Tools this agent is allowed to use. An empty list means all tools.
    pub allowed_tools: Vec<String>,
    /// Workspace path prefix for memory isolation (e.g., "/agents/coder/").
    pub workspace_prefix: String,
    /// Whether this agent is currently enabled for routing.
    pub enabled: bool,
    /// Priority for tie-breaking when multiple agents match (higher = preferred).
    pub priority: i32,
}

impl AgentIdentity {
    /// Create a new agent identity with the given name and description.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        system_prompt: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let workspace_prefix = format!("/agents/{}/", name);
        Self {
            name,
            description: description.into(),
            system_prompt: system_prompt.into(),
            allowed_tools: Vec::new(),
            workspace_prefix,
            enabled: true,
            priority: 0,
        }
    }

    /// Set the allowed tools for this agent.
    pub fn with_allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_tools = tools;
        self
    }

    /// Set the workspace prefix for memory isolation.
    pub fn with_workspace_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.workspace_prefix = prefix.into();
        self
    }

    /// Set the priority for this agent.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Set whether this agent is enabled.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Check whether a given tool name is permitted for this agent.
    ///
    /// An empty `allowed_tools` list means all tools are permitted.
    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        self.allowed_tools.is_empty() || self.allowed_tools.iter().any(|t| t == tool_name)
    }
}

/// The outcome of routing a message to an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// Name of the agent selected to handle the message.
    pub agent_name: String,
    /// Confidence score from 0.0 to 1.0.
    pub confidence: f64,
    /// Human-readable reason explaining why this agent was chosen.
    pub reason: String,
    /// The routing strategy that produced this decision.
    pub strategy: RoutingStrategy,
}

/// How the routing decision was made.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RoutingStrategy {
    /// The user explicitly mentioned an agent by name (e.g., "@coder").
    ExplicitMention,
    /// Routed based on the originating channel.
    ChannelMapping,
    /// Routed based on keyword/intent analysis of the message content.
    IntentMatch,
    /// Fell through to the default agent.
    Default,
}

/// Manages multiple agent identities and routes incoming messages.
///
/// The router applies a prioritized cascade of strategies:
/// 1. Explicit `@agent` mentions in the message content.
/// 2. Channel-to-agent mappings (e.g., Telegram -> "ops").
/// 3. Keyword-based intent matching against agent descriptions.
/// 4. Fallback to the configured default agent.
pub struct AgentRouter {
    /// Registered agent identities, keyed by name.
    agents: Arc<RwLock<HashMap<String, AgentIdentity>>>,
    /// Maps channel names to preferred agent names.
    channel_mappings: Arc<RwLock<HashMap<String, String>>>,
    /// Name of the default fallback agent.
    default_agent: Arc<RwLock<String>>,
}

impl AgentRouter {
    /// Create a new router with a default agent identity.
    pub fn new(default: AgentIdentity) -> Self {
        let default_name = default.name.clone();
        let mut agents = HashMap::new();
        agents.insert(default.name.clone(), default);

        Self {
            agents: Arc::new(RwLock::new(agents)),
            channel_mappings: Arc::new(RwLock::new(HashMap::new())),
            default_agent: Arc::new(RwLock::new(default_name)),
        }
    }

    /// Register a new agent identity.
    ///
    /// If an agent with the same name already exists it will be replaced.
    pub async fn register_agent(&self, agent: AgentIdentity) {
        self.agents.write().await.insert(agent.name.clone(), agent);
    }

    /// Remove an agent by name.
    ///
    /// Returns the removed identity, or `None` if not found.
    /// The default agent cannot be removed.
    pub async fn remove_agent(&self, name: &str) -> Option<AgentIdentity> {
        let default_name = self.default_agent.read().await.clone();
        if name == default_name {
            tracing::warn!("Cannot remove the default agent '{}'", name);
            return None;
        }
        self.agents.write().await.remove(name)
    }

    /// Map a channel name to a preferred agent.
    pub async fn set_channel_mapping(&self, channel: impl Into<String>, agent: impl Into<String>) {
        self.channel_mappings
            .write()
            .await
            .insert(channel.into(), agent.into());
    }

    /// Remove a channel-to-agent mapping.
    pub async fn remove_channel_mapping(&self, channel: &str) {
        self.channel_mappings.write().await.remove(channel);
    }

    /// Get an agent identity by name.
    pub async fn get_agent(&self, name: &str) -> Option<AgentIdentity> {
        self.agents.read().await.get(name).cloned()
    }

    /// List all registered agent identities.
    pub async fn list_agents(&self) -> Vec<AgentIdentity> {
        self.agents.read().await.values().cloned().collect()
    }

    /// List only enabled agents.
    pub async fn list_enabled_agents(&self) -> Vec<AgentIdentity> {
        self.agents
            .read()
            .await
            .values()
            .filter(|a| a.enabled)
            .cloned()
            .collect()
    }

    /// Route an incoming message to the best-matching agent.
    ///
    /// Applies a cascade of strategies in priority order:
    /// 1. Explicit `@agent_name` mention in content.
    /// 2. Channel-based mapping.
    /// 3. Keyword/intent matching against agent descriptions.
    /// 4. Default agent fallback.
    pub async fn route(&self, message: &IncomingMessage) -> Result<RoutingDecision, Error> {
        let agents = self.agents.read().await;
        let enabled: Vec<&AgentIdentity> = agents.values().filter(|a| a.enabled).collect();

        if enabled.is_empty() {
            return Ok(self.default_decision().await);
        }

        // Strategy 1: Explicit @mention
        if let Some(decision) = self.try_explicit_mention(&message.content, &enabled) {
            tracing::debug!(
                agent = %decision.agent_name,
                strategy = ?decision.strategy,
                "Routed message via explicit mention"
            );
            return Ok(decision);
        }

        // Strategy 2: Channel mapping
        if let Some(decision) = self.try_channel_mapping(&message.channel, &enabled).await {
            tracing::debug!(
                agent = %decision.agent_name,
                channel = %message.channel,
                strategy = ?decision.strategy,
                "Routed message via channel mapping"
            );
            return Ok(decision);
        }

        // Strategy 3: Intent/keyword matching
        if let Some(decision) = self.try_intent_match(&message.content, &enabled) {
            tracing::debug!(
                agent = %decision.agent_name,
                confidence = %decision.confidence,
                strategy = ?decision.strategy,
                "Routed message via intent matching"
            );
            return Ok(decision);
        }

        // Strategy 4: Default fallback
        let decision = self.default_decision().await;
        tracing::debug!(
            agent = %decision.agent_name,
            strategy = ?decision.strategy,
            "Routed message to default agent"
        );
        Ok(decision)
    }

    /// Attempt to find an explicit `@agent_name` mention in the message.
    fn try_explicit_mention(
        &self,
        content: &str,
        agents: &[&AgentIdentity],
    ) -> Option<RoutingDecision> {
        let content_lower = content.to_lowercase();

        // Sort agents by priority descending so higher-priority agents win ties.
        let mut sorted: Vec<&&AgentIdentity> = agents.iter().collect();
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));

        for agent in sorted {
            let mention = format!("@{}", agent.name.to_lowercase());
            if content_lower.contains(&mention) {
                return Some(RoutingDecision {
                    agent_name: agent.name.clone(),
                    confidence: 1.0,
                    reason: format!("User explicitly mentioned @{} in the message", agent.name),
                    strategy: RoutingStrategy::ExplicitMention,
                });
            }
        }
        None
    }

    /// Attempt to route based on channel-to-agent mapping.
    async fn try_channel_mapping(
        &self,
        channel: &str,
        agents: &[&AgentIdentity],
    ) -> Option<RoutingDecision> {
        let mappings = self.channel_mappings.read().await;
        let mapped_agent = mappings.get(channel)?;

        // Verify the mapped agent exists and is in the enabled set.
        if agents.iter().any(|a| &a.name == mapped_agent) {
            Some(RoutingDecision {
                agent_name: mapped_agent.clone(),
                confidence: 0.9,
                reason: format!(
                    "Channel '{}' is mapped to agent '{}'",
                    channel, mapped_agent
                ),
                strategy: RoutingStrategy::ChannelMapping,
            })
        } else {
            None
        }
    }

    /// Attempt keyword-based intent matching against agent descriptions.
    ///
    /// Scores each agent by how many words from its description appear in the
    /// message content. This is intentionally simple; for production use an
    /// LLM-based classifier should replace or augment this.
    fn try_intent_match(
        &self,
        content: &str,
        agents: &[&AgentIdentity],
    ) -> Option<RoutingDecision> {
        let content_lower = content.to_lowercase();
        let content_words: Vec<&str> = content_lower.split_whitespace().collect();

        let mut best: Option<(f64, &AgentIdentity)> = None;

        for agent in agents {
            let desc_lower = agent.description.to_lowercase();
            let desc_words: Vec<&str> = desc_lower.split_whitespace().collect();

            if desc_words.is_empty() {
                continue;
            }

            // Count how many description keywords appear in the content.
            let matches = desc_words
                .iter()
                .filter(|w| w.len() > 3) // skip short/common words
                .filter(|w| content_words.contains(w))
                .count();

            let significant_words = desc_words.iter().filter(|w| w.len() > 3).count();
            if significant_words == 0 {
                continue;
            }

            let score = matches as f64 / significant_words as f64;

            // Apply priority as a small bonus to break ties.
            let adjusted = score + (agent.priority as f64 * 0.001);

            if let Some((best_score, _)) = best {
                if adjusted > best_score {
                    best = Some((adjusted, agent));
                }
            } else if score > 0.0 {
                best = Some((adjusted, agent));
            }
        }

        // Only return if we have a meaningful match.
        let (score, agent) = best?;
        let raw_score = score - (agent.priority as f64 * 0.001);
        if raw_score < 0.1 {
            return None;
        }

        Some(RoutingDecision {
            agent_name: agent.name.clone(),
            confidence: raw_score.min(1.0),
            reason: format!(
                "Message content matched keywords in agent '{}' description",
                agent.name
            ),
            strategy: RoutingStrategy::IntentMatch,
        })
    }

    /// Build a default fallback decision.
    async fn default_decision(&self) -> RoutingDecision {
        let default_name = self.default_agent.read().await.clone();
        RoutingDecision {
            agent_name: default_name,
            confidence: 0.5,
            reason: "No specific agent matched; using default".to_string(),
            strategy: RoutingStrategy::Default,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::agent::multi_agent::{AgentIdentity, AgentRouter, RoutingStrategy};
    use crate::channels::IncomingMessage;

    fn make_default_agent() -> AgentIdentity {
        AgentIdentity::new(
            "assistant",
            "General-purpose assistant for everyday tasks",
            "You are a helpful general assistant.",
        )
    }

    fn make_coder_agent() -> AgentIdentity {
        AgentIdentity::new(
            "coder",
            "Software development coding programming debugging testing",
            "You are an expert software engineer.",
        )
        .with_allowed_tools(vec![
            "shell".to_string(),
            "read_file".to_string(),
            "write_file".to_string(),
        ])
        .with_priority(10)
    }

    fn make_researcher_agent() -> AgentIdentity {
        AgentIdentity::new(
            "researcher",
            "Research analysis investigation data gathering reports",
            "You are a thorough research analyst.",
        )
        .with_allowed_tools(vec!["http".to_string(), "memory_search".to_string()])
        .with_priority(5)
    }

    // --- AgentIdentity tests ---

    #[test]
    fn test_agent_identity_defaults() {
        let agent = AgentIdentity::new("test", "A test agent", "System prompt");
        assert_eq!(agent.name, "test");
        assert_eq!(agent.workspace_prefix, "/agents/test/");
        assert!(agent.enabled);
        assert_eq!(agent.priority, 0);
        assert!(agent.allowed_tools.is_empty());
    }

    #[test]
    fn test_agent_identity_builders() {
        let agent = AgentIdentity::new("ops", "Operations agent", "System prompt")
            .with_allowed_tools(vec!["shell".to_string()])
            .with_workspace_prefix("/custom/ops/")
            .with_priority(20)
            .with_enabled(false);

        assert_eq!(agent.allowed_tools, vec!["shell"]);
        assert_eq!(agent.workspace_prefix, "/custom/ops/");
        assert_eq!(agent.priority, 20);
        assert!(!agent.enabled);
    }

    #[test]
    fn test_is_tool_allowed_empty_means_all() {
        let agent = AgentIdentity::new("test", "Test", "Prompt");
        assert!(agent.is_tool_allowed("anything"));
        assert!(agent.is_tool_allowed("shell"));
    }

    #[test]
    fn test_is_tool_allowed_restricted() {
        let agent = AgentIdentity::new("test", "Test", "Prompt")
            .with_allowed_tools(vec!["read_file".to_string(), "echo".to_string()]);

        assert!(agent.is_tool_allowed("read_file"));
        assert!(agent.is_tool_allowed("echo"));
        assert!(!agent.is_tool_allowed("shell"));
        assert!(!agent.is_tool_allowed("write_file"));
    }

    // --- AgentRouter tests ---

    #[tokio::test]
    async fn test_router_creation() {
        let router = AgentRouter::new(make_default_agent());
        let agents = router.list_agents().await;
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "assistant");
    }

    #[tokio::test]
    async fn test_register_and_list_agents() {
        let router = AgentRouter::new(make_default_agent());
        router.register_agent(make_coder_agent()).await;
        router.register_agent(make_researcher_agent()).await;

        let agents = router.list_agents().await;
        assert_eq!(agents.len(), 3);

        let enabled = router.list_enabled_agents().await;
        assert_eq!(enabled.len(), 3);
    }

    #[tokio::test]
    async fn test_remove_agent() {
        let router = AgentRouter::new(make_default_agent());
        router.register_agent(make_coder_agent()).await;

        let removed = router.remove_agent("coder").await;
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().name, "coder");

        let agents = router.list_agents().await;
        assert_eq!(agents.len(), 1);
    }

    #[tokio::test]
    async fn test_cannot_remove_default_agent() {
        let router = AgentRouter::new(make_default_agent());
        let removed = router.remove_agent("assistant").await;
        assert!(removed.is_none());

        let agents = router.list_agents().await;
        assert_eq!(agents.len(), 1);
    }

    #[tokio::test]
    async fn test_get_agent() {
        let router = AgentRouter::new(make_default_agent());
        router.register_agent(make_coder_agent()).await;

        let agent = router.get_agent("coder").await;
        assert!(agent.is_some());
        assert_eq!(agent.unwrap().name, "coder");

        let missing = router.get_agent("nonexistent").await;
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_route_explicit_mention() {
        let router = AgentRouter::new(make_default_agent());
        router.register_agent(make_coder_agent()).await;
        router.register_agent(make_researcher_agent()).await;

        let msg = IncomingMessage::new("cli", "user1", "Hey @coder can you fix this bug?");
        let decision = router.route(&msg).await.unwrap();

        assert_eq!(decision.agent_name, "coder");
        assert_eq!(decision.confidence, 1.0);
        assert_eq!(decision.strategy, RoutingStrategy::ExplicitMention);
    }

    #[tokio::test]
    async fn test_route_explicit_mention_case_insensitive() {
        let router = AgentRouter::new(make_default_agent());
        router.register_agent(make_coder_agent()).await;

        let msg = IncomingMessage::new("cli", "user1", "Hey @CODER please help");
        let decision = router.route(&msg).await.unwrap();

        assert_eq!(decision.agent_name, "coder");
        assert_eq!(decision.strategy, RoutingStrategy::ExplicitMention);
    }

    #[tokio::test]
    async fn test_route_channel_mapping() {
        let router = AgentRouter::new(make_default_agent());
        router.register_agent(make_coder_agent()).await;
        router.set_channel_mapping("telegram", "coder").await;

        let msg = IncomingMessage::new("telegram", "user1", "Hello there");
        let decision = router.route(&msg).await.unwrap();

        assert_eq!(decision.agent_name, "coder");
        assert_eq!(decision.strategy, RoutingStrategy::ChannelMapping);
    }

    #[tokio::test]
    async fn test_route_channel_mapping_disabled_agent_falls_through() {
        let router = AgentRouter::new(make_default_agent());
        let disabled = make_coder_agent().with_enabled(false);
        router.register_agent(disabled).await;
        router.set_channel_mapping("telegram", "coder").await;

        let msg = IncomingMessage::new("telegram", "user1", "Hello there");
        let decision = router.route(&msg).await.unwrap();

        // Disabled agent should not be selected; falls through to default.
        assert_eq!(decision.agent_name, "assistant");
        assert_eq!(decision.strategy, RoutingStrategy::Default);
    }

    #[tokio::test]
    async fn test_route_intent_match() {
        let router = AgentRouter::new(make_default_agent());
        router.register_agent(make_coder_agent()).await;
        router.register_agent(make_researcher_agent()).await;

        let msg = IncomingMessage::new(
            "cli",
            "user1",
            "I need help with programming and debugging this code",
        );
        let decision = router.route(&msg).await.unwrap();

        assert_eq!(decision.agent_name, "coder");
        assert_eq!(decision.strategy, RoutingStrategy::IntentMatch);
        assert!(decision.confidence > 0.0);
    }

    #[tokio::test]
    async fn test_route_intent_match_research() {
        let router = AgentRouter::new(make_default_agent());
        router.register_agent(make_coder_agent()).await;
        router.register_agent(make_researcher_agent()).await;

        let msg = IncomingMessage::new(
            "cli",
            "user1",
            "Please do some research and analysis on this topic",
        );
        let decision = router.route(&msg).await.unwrap();

        assert_eq!(decision.agent_name, "researcher");
        assert_eq!(decision.strategy, RoutingStrategy::IntentMatch);
    }

    #[tokio::test]
    async fn test_route_default_fallback() {
        let router = AgentRouter::new(make_default_agent());
        router.register_agent(make_coder_agent()).await;

        let msg = IncomingMessage::new("cli", "user1", "What is the weather today?");
        let decision = router.route(&msg).await.unwrap();

        assert_eq!(decision.agent_name, "assistant");
        assert_eq!(decision.confidence, 0.5);
        assert_eq!(decision.strategy, RoutingStrategy::Default);
    }

    #[tokio::test]
    async fn test_route_priority_ordering() {
        let router = AgentRouter::new(make_default_agent());

        // Two agents with overlapping descriptions but different priorities.
        let low = AgentIdentity::new("low", "coding programming development", "Low priority")
            .with_priority(1);
        let high = AgentIdentity::new("high", "coding programming development", "High priority")
            .with_priority(100);

        router.register_agent(low).await;
        router.register_agent(high).await;

        let msg = IncomingMessage::new("cli", "user1", "Help me with programming and development");
        let decision = router.route(&msg).await.unwrap();

        assert_eq!(decision.agent_name, "high");
    }

    #[tokio::test]
    async fn test_channel_mapping_crud() {
        let router = AgentRouter::new(make_default_agent());
        router.register_agent(make_coder_agent()).await;

        router.set_channel_mapping("slack", "coder").await;

        // Verify it works.
        let msg = IncomingMessage::new("slack", "user1", "hello");
        let decision = router.route(&msg).await.unwrap();
        assert_eq!(decision.agent_name, "coder");

        // Remove mapping.
        router.remove_channel_mapping("slack").await;

        let decision2 = router.route(&msg).await.unwrap();
        // Should fall through to default since no keywords match.
        assert_eq!(decision2.agent_name, "assistant");
    }

    #[tokio::test]
    async fn test_disabled_agent_excluded_from_routing() {
        let router = AgentRouter::new(make_default_agent());
        let disabled = make_coder_agent().with_enabled(false);
        router.register_agent(disabled).await;

        // Even with a direct keyword match, disabled agents should not be selected.
        let msg = IncomingMessage::new("cli", "user1", "programming debugging coding");
        let decision = router.route(&msg).await.unwrap();

        assert_ne!(decision.agent_name, "coder");
    }

    #[tokio::test]
    async fn test_explicit_mention_overrides_channel_mapping() {
        let router = AgentRouter::new(make_default_agent());
        router.register_agent(make_coder_agent()).await;
        router.register_agent(make_researcher_agent()).await;
        router.set_channel_mapping("telegram", "coder").await;

        // Channel says coder, but explicit mention says researcher.
        let msg = IncomingMessage::new("telegram", "user1", "@researcher can you look into this?");
        let decision = router.route(&msg).await.unwrap();

        assert_eq!(decision.agent_name, "researcher");
        assert_eq!(decision.strategy, RoutingStrategy::ExplicitMention);
    }

    #[tokio::test]
    async fn test_workspace_isolation_prefix() {
        let coder = make_coder_agent();
        let researcher = make_researcher_agent();

        assert_eq!(coder.workspace_prefix, "/agents/coder/");
        assert_eq!(researcher.workspace_prefix, "/agents/researcher/");
        assert_ne!(coder.workspace_prefix, researcher.workspace_prefix);
    }

    #[tokio::test]
    async fn test_agent_identity_serialization() {
        let agent = make_coder_agent();
        let json = serde_json::to_string(&agent).unwrap();
        let deserialized: AgentIdentity = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, agent.name);
        assert_eq!(deserialized.description, agent.description);
        assert_eq!(deserialized.allowed_tools, agent.allowed_tools);
        assert_eq!(deserialized.workspace_prefix, agent.workspace_prefix);
        assert_eq!(deserialized.enabled, agent.enabled);
        assert_eq!(deserialized.priority, agent.priority);
    }

    #[tokio::test]
    async fn test_routing_decision_serialization() {
        let decision = super::RoutingDecision {
            agent_name: "coder".to_string(),
            confidence: 0.95,
            reason: "Matched coding keywords".to_string(),
            strategy: super::RoutingStrategy::IntentMatch,
        };

        let json = serde_json::to_string(&decision).unwrap();
        let deserialized: super::RoutingDecision = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.agent_name, "coder");
        assert_eq!(deserialized.confidence, 0.95);
        assert_eq!(deserialized.strategy, super::RoutingStrategy::IntentMatch);
    }
}
