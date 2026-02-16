//! Agent management API routes for the web gateway.
//!
//! Provides REST endpoints for managing agents (identities) from the web UI:
//! - List agents
//! - Get agent details
//! - Create agent
//! - Update agent settings
//! - Delete agent
//! - Set default agent

use axum::{Json, extract::Path, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};

/// Agent info returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Agent identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description of the agent's role.
    pub description: String,
    /// Whether this is the default agent.
    pub is_default: bool,
    /// Whether the agent is currently active.
    pub active: bool,
    /// Agent-specific system prompt override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Agent-specific model override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Tools enabled for this agent.
    #[serde(default)]
    pub enabled_tools: Vec<String>,
    /// Skills enabled for this agent.
    #[serde(default)]
    pub enabled_skills: Vec<String>,
    /// Workspace/memory space for this agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// When this agent was created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

/// Request to create a new agent.
#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub system_prompt: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub enabled_tools: Vec<String>,
    #[serde(default)]
    pub enabled_skills: Vec<String>,
    pub workspace: Option<String>,
}

/// Request to update an agent.
#[derive(Debug, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub system_prompt: Option<String>,
    pub model: Option<String>,
    pub enabled_tools: Option<Vec<String>>,
    pub enabled_skills: Option<Vec<String>>,
    pub workspace: Option<String>,
    pub active: Option<bool>,
}

/// Response envelope for API responses.
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

/// In-memory agent store for the web gateway.
///
/// In production, this would be backed by the database. This provides
/// the API layer that the web UI interacts with.
pub struct AgentStore {
    agents: tokio::sync::RwLock<Vec<AgentInfo>>,
}

impl AgentStore {
    /// Create a new agent store with a default agent.
    pub fn new() -> Self {
        let default_agent = AgentInfo {
            id: "default".to_string(),
            name: "Default Agent".to_string(),
            description: "The primary IronClaw agent".to_string(),
            is_default: true,
            active: true,
            system_prompt: None,
            model: None,
            enabled_tools: Vec::new(),
            enabled_skills: Vec::new(),
            workspace: None,
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        };

        Self {
            agents: tokio::sync::RwLock::new(vec![default_agent]),
        }
    }

    /// List all agents.
    pub async fn list(&self) -> Vec<AgentInfo> {
        self.agents.read().await.clone()
    }

    /// Get an agent by ID.
    pub async fn get(&self, id: &str) -> Option<AgentInfo> {
        self.agents
            .read()
            .await
            .iter()
            .find(|a| a.id == id)
            .cloned()
    }

    /// Create a new agent.
    pub async fn create(&self, req: CreateAgentRequest) -> Result<AgentInfo, String> {
        let mut agents = self.agents.write().await;

        if agents.iter().any(|a| a.id == req.id) {
            return Err(format!("Agent with id '{}' already exists", req.id));
        }

        let agent = AgentInfo {
            id: req.id,
            name: req.name,
            description: req.description,
            is_default: false,
            active: true,
            system_prompt: req.system_prompt,
            model: req.model,
            enabled_tools: req.enabled_tools,
            enabled_skills: req.enabled_skills,
            workspace: req.workspace,
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        };

        agents.push(agent.clone());
        Ok(agent)
    }

    /// Update an existing agent.
    pub async fn update(&self, id: &str, req: UpdateAgentRequest) -> Result<AgentInfo, String> {
        let mut agents = self.agents.write().await;

        let agent = agents
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| format!("Agent '{}' not found", id))?;

        if let Some(name) = req.name {
            agent.name = name;
        }
        if let Some(description) = req.description {
            agent.description = description;
        }
        if let Some(prompt) = req.system_prompt {
            agent.system_prompt = Some(prompt);
        }
        if let Some(model) = req.model {
            agent.model = Some(model);
        }
        if let Some(tools) = req.enabled_tools {
            agent.enabled_tools = tools;
        }
        if let Some(skills) = req.enabled_skills {
            agent.enabled_skills = skills;
        }
        if let Some(workspace) = req.workspace {
            agent.workspace = Some(workspace);
        }
        if let Some(active) = req.active {
            agent.active = active;
        }

        Ok(agent.clone())
    }

    /// Delete an agent (cannot delete the default agent).
    pub async fn delete(&self, id: &str) -> Result<(), String> {
        let mut agents = self.agents.write().await;

        if let Some(agent) = agents.iter().find(|a| a.id == id)
            && agent.is_default
        {
            return Err("Cannot delete the default agent".to_string());
        }

        let before = agents.len();
        agents.retain(|a| a.id != id);

        if agents.len() == before {
            Err(format!("Agent '{}' not found", id))
        } else {
            Ok(())
        }
    }

    /// Set an agent as the default.
    pub async fn set_default(&self, id: &str) -> Result<AgentInfo, String> {
        let mut agents = self.agents.write().await;

        if !agents.iter().any(|a| a.id == id) {
            return Err(format!("Agent '{}' not found", id));
        }

        for agent in agents.iter_mut() {
            agent.is_default = agent.id == id;
        }

        agents
            .iter()
            .find(|a| a.id == id)
            .cloned()
            .ok_or_else(|| "Agent not found after update".to_string())
    }
}

impl Default for AgentStore {
    fn default() -> Self {
        Self::new()
    }
}

// -- Axum handler functions --

/// GET /api/agents - List all agents.
pub async fn list_agents(
    axum::extract::State(store): axum::extract::State<std::sync::Arc<AgentStore>>,
) -> impl IntoResponse {
    let agents = store.list().await;
    Json(ApiResponse::ok(agents))
}

/// GET /api/agents/:id - Get agent by ID.
pub async fn get_agent(
    axum::extract::State(store): axum::extract::State<std::sync::Arc<AgentStore>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match store.get(&id).await {
        Some(agent) => (StatusCode::OK, Json(ApiResponse::ok(agent))),
        None => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<AgentInfo>::err(format!(
                "Agent '{}' not found",
                id
            ))),
        ),
    }
}

/// POST /api/agents - Create a new agent.
pub async fn create_agent(
    axum::extract::State(store): axum::extract::State<std::sync::Arc<AgentStore>>,
    Json(req): Json<CreateAgentRequest>,
) -> impl IntoResponse {
    match store.create(req).await {
        Ok(agent) => (StatusCode::CREATED, Json(ApiResponse::ok(agent))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<AgentInfo>::err(e)),
        ),
    }
}

/// PATCH /api/agents/:id - Update an agent.
pub async fn update_agent(
    axum::extract::State(store): axum::extract::State<std::sync::Arc<AgentStore>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> impl IntoResponse {
    match store.update(&id, req).await {
        Ok(agent) => (StatusCode::OK, Json(ApiResponse::ok(agent))),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<AgentInfo>::err(e)),
        ),
    }
}

/// DELETE /api/agents/:id - Delete an agent.
pub async fn delete_agent(
    axum::extract::State(store): axum::extract::State<std::sync::Arc<AgentStore>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match store.delete(&id).await {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::ok("Agent deleted".to_string())),
        ),
        Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::<String>::err(e))),
    }
}

/// POST /api/agents/:id/default - Set agent as default.
pub async fn set_default_agent(
    axum::extract::State(store): axum::extract::State<std::sync::Arc<AgentStore>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match store.set_default(&id).await {
        Ok(agent) => (StatusCode::OK, Json(ApiResponse::ok(agent))),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<AgentInfo>::err(e)),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_agent_store_default() {
        let store = AgentStore::new();
        let agents = store.list().await;
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].id, "default");
        assert!(agents[0].is_default);
    }

    #[tokio::test]
    async fn test_create_agent() {
        let store = AgentStore::new();
        let req = CreateAgentRequest {
            id: "research".to_string(),
            name: "Research Agent".to_string(),
            description: "Helps with research".to_string(),
            system_prompt: Some("You are a research assistant.".to_string()),
            model: None,
            enabled_tools: vec!["memory_search".to_string()],
            enabled_skills: Vec::new(),
            workspace: None,
        };

        let agent = store.create(req).await.unwrap();
        assert_eq!(agent.id, "research");
        assert!(!agent.is_default);

        let agents = store.list().await;
        assert_eq!(agents.len(), 2);
    }

    #[tokio::test]
    async fn test_create_duplicate() {
        let store = AgentStore::new();
        let req = CreateAgentRequest {
            id: "default".to_string(),
            name: "Duplicate".to_string(),
            description: String::new(),
            system_prompt: None,
            model: None,
            enabled_tools: Vec::new(),
            enabled_skills: Vec::new(),
            workspace: None,
        };

        assert!(store.create(req).await.is_err());
    }

    #[tokio::test]
    async fn test_update_agent() {
        let store = AgentStore::new();
        let req = UpdateAgentRequest {
            name: Some("Updated Name".to_string()),
            description: None,
            system_prompt: None,
            model: Some("gpt-4".to_string()),
            enabled_tools: None,
            enabled_skills: None,
            workspace: None,
            active: None,
        };

        let updated = store.update("default", req).await.unwrap();
        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.model.unwrap(), "gpt-4");
    }

    #[tokio::test]
    async fn test_delete_default_fails() {
        let store = AgentStore::new();
        assert!(store.delete("default").await.is_err());
    }

    #[tokio::test]
    async fn test_delete_agent() {
        let store = AgentStore::new();
        let req = CreateAgentRequest {
            id: "temp".to_string(),
            name: "Temp".to_string(),
            description: String::new(),
            system_prompt: None,
            model: None,
            enabled_tools: Vec::new(),
            enabled_skills: Vec::new(),
            workspace: None,
        };
        store.create(req).await.unwrap();
        assert_eq!(store.list().await.len(), 2);

        store.delete("temp").await.unwrap();
        assert_eq!(store.list().await.len(), 1);
    }

    #[tokio::test]
    async fn test_set_default() {
        let store = AgentStore::new();
        let req = CreateAgentRequest {
            id: "new-default".to_string(),
            name: "New Default".to_string(),
            description: String::new(),
            system_prompt: None,
            model: None,
            enabled_tools: Vec::new(),
            enabled_skills: Vec::new(),
            workspace: None,
        };
        store.create(req).await.unwrap();

        store.set_default("new-default").await.unwrap();

        let agents = store.list().await;
        assert!(
            !agents
                .iter()
                .find(|a| a.id == "default")
                .unwrap()
                .is_default
        );
        assert!(
            agents
                .iter()
                .find(|a| a.id == "new-default")
                .unwrap()
                .is_default
        );
    }
}
