//! Auto-discovery for LLM models across providers.
//!
//! Queries provider APIs to discover available models and their capabilities
//! (context length, tool support, vision support, etc.).

use serde::{Deserialize, Serialize};

use crate::error::LlmError;

/// Information about a discovered LLM model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredModel {
    /// Provider-specific model identifier (e.g. "gpt-4o", "claude-3-opus").
    pub id: String,
    /// Human-readable model name.
    pub name: String,
    /// Which provider this model belongs to.
    pub provider: String,
    /// Maximum context window in tokens.
    pub context_length: u64,
    /// Whether the model supports tool/function calling.
    pub supports_tools: bool,
    /// Whether the model supports vision (image) inputs.
    pub supports_vision: bool,
    /// Whether the model is currently available for use.
    pub is_available: bool,
}

/// Discovers available models from LLM providers.
///
/// Uses HTTP requests to query provider listing endpoints and
/// returns normalized model information.
#[derive(Debug, Clone)]
pub struct ModelDiscovery {
    client: reqwest::Client,
}

impl ModelDiscovery {
    /// Create a new model discovery instance.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Create a model discovery instance with a custom HTTP client.
    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// Discover models from a specific provider.
    ///
    /// The `provider` string should be one of: "openai", "anthropic", "ollama".
    /// An API key is required for OpenAI and Anthropic; Ollama runs locally
    /// and does not require authentication.
    pub async fn discover_models(
        &self,
        provider: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
    ) -> Result<Vec<DiscoveredModel>, LlmError> {
        match provider {
            "openai" => {
                let key = api_key.ok_or_else(|| LlmError::AuthFailed {
                    provider: "openai".to_string(),
                })?;
                self.discover_openai(key).await
            }
            "anthropic" => {
                let key = api_key.ok_or_else(|| LlmError::AuthFailed {
                    provider: "anthropic".to_string(),
                })?;
                self.discover_anthropic(key).await
            }
            "ollama" => {
                let url = base_url.unwrap_or("http://localhost:11434");
                self.discover_ollama(url).await
            }
            "openrouter" => {
                let key = api_key.ok_or_else(|| LlmError::AuthFailed {
                    provider: "openrouter".to_string(),
                })?;
                let url = base_url.unwrap_or("https://openrouter.ai/api/v1");
                self.discover_openrouter(key, url).await
            }
            other => Err(LlmError::RequestFailed {
                provider: other.to_string(),
                reason: format!("Unknown provider for discovery: {other}"),
            }),
        }
    }

    /// Discover available OpenAI models.
    ///
    /// Calls the `GET /v1/models` endpoint and filters for chat-capable models.
    pub async fn discover_openai(&self, api_key: &str) -> Result<Vec<DiscoveredModel>, LlmError> {
        let resp = self
            .client
            .get("https://api.openai.com/v1/models")
            .header("Authorization", format!("Bearer {api_key}"))
            .send()
            .await
            .map_err(|e| LlmError::RequestFailed {
                provider: "openai".to_string(),
                reason: e.to_string(),
            })?;

        if !resp.status().is_success() {
            return Err(LlmError::RequestFailed {
                provider: "openai".to_string(),
                reason: format!("HTTP {}", resp.status()),
            });
        }

        let body: OpenAiModelsResponse =
            resp.json().await.map_err(|e| LlmError::InvalidResponse {
                provider: "openai".to_string(),
                reason: e.to_string(),
            })?;

        let models = body
            .data
            .into_iter()
            .filter(|m| is_openai_chat_model(&m.id))
            .map(|m| {
                let (context_length, supports_tools, supports_vision) =
                    openai_model_capabilities(&m.id);
                DiscoveredModel {
                    id: m.id.clone(),
                    name: m.id,
                    provider: "openai".to_string(),
                    context_length,
                    supports_tools,
                    supports_vision,
                    is_available: true,
                }
            })
            .collect();

        Ok(models)
    }

    /// Discover available Anthropic models.
    ///
    /// Calls the `GET /v1/models` endpoint to list available Claude models.
    pub async fn discover_anthropic(
        &self,
        api_key: &str,
    ) -> Result<Vec<DiscoveredModel>, LlmError> {
        let resp = self
            .client
            .get("https://api.anthropic.com/v1/models")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await
            .map_err(|e| LlmError::RequestFailed {
                provider: "anthropic".to_string(),
                reason: e.to_string(),
            })?;

        if !resp.status().is_success() {
            return Err(LlmError::RequestFailed {
                provider: "anthropic".to_string(),
                reason: format!("HTTP {}", resp.status()),
            });
        }

        let body: AnthropicModelsResponse =
            resp.json().await.map_err(|e| LlmError::InvalidResponse {
                provider: "anthropic".to_string(),
                reason: e.to_string(),
            })?;

        let models = body
            .data
            .into_iter()
            .map(|m| {
                let (supports_tools, supports_vision) = anthropic_model_capabilities(&m.id);
                DiscoveredModel {
                    id: m.id.clone(),
                    name: m.display_name.unwrap_or_else(|| m.id.clone()),
                    provider: "anthropic".to_string(),
                    context_length: m.context_window.unwrap_or(200_000),
                    supports_tools,
                    supports_vision,
                    is_available: true,
                }
            })
            .collect();

        Ok(models)
    }

    /// Discover available Ollama models running locally.
    ///
    /// Calls the `GET /api/tags` endpoint on the local Ollama server.
    pub async fn discover_ollama(&self, base_url: &str) -> Result<Vec<DiscoveredModel>, LlmError> {
        let url = format!("{}/api/tags", base_url.trim_end_matches('/'));

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| LlmError::RequestFailed {
                provider: "ollama".to_string(),
                reason: e.to_string(),
            })?;

        if !resp.status().is_success() {
            return Err(LlmError::RequestFailed {
                provider: "ollama".to_string(),
                reason: format!("HTTP {}", resp.status()),
            });
        }

        let body: OllamaTagsResponse =
            resp.json().await.map_err(|e| LlmError::InvalidResponse {
                provider: "ollama".to_string(),
                reason: e.to_string(),
            })?;

        let models = body
            .models
            .into_iter()
            .map(|m| DiscoveredModel {
                id: m.name.clone(),
                name: m.name,
                provider: "ollama".to_string(),
                context_length: m.context_length.unwrap_or(4_096),
                supports_tools: false,
                supports_vision: false,
                is_available: true,
            })
            .collect();

        Ok(models)
    }

    /// Discover available models from OpenRouter.
    ///
    /// Calls the `GET /models` endpoint on the OpenRouter API.
    pub async fn discover_openrouter(
        &self,
        api_key: &str,
        base_url: &str,
    ) -> Result<Vec<DiscoveredModel>, LlmError> {
        let url = format!("{}/models", base_url.trim_end_matches('/'));

        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .send()
            .await
            .map_err(|e| LlmError::RequestFailed {
                provider: "openrouter".to_string(),
                reason: e.to_string(),
            })?;

        if !resp.status().is_success() {
            return Err(LlmError::RequestFailed {
                provider: "openrouter".to_string(),
                reason: format!("HTTP {}", resp.status()),
            });
        }

        let body: OpenRouterModelsResponse =
            resp.json().await.map_err(|e| LlmError::InvalidResponse {
                provider: "openrouter".to_string(),
                reason: e.to_string(),
            })?;

        let models = body
            .data
            .into_iter()
            .map(|m| DiscoveredModel {
                id: m.id.clone(),
                name: m.name.unwrap_or_else(|| m.id.clone()),
                provider: "openrouter".to_string(),
                context_length: m.context_length.unwrap_or(4_096),
                supports_tools: true,   // Most OpenRouter models support tools
                supports_vision: false, // Conservative default
                is_available: true,
            })
            .collect();

        Ok(models)
    }

    /// Discover models from all known providers.
    ///
    /// Queries each provider that has credentials supplied. Errors from
    /// individual providers are logged and skipped so that one failing
    /// provider does not block discovery of the others.
    pub async fn discover_all(
        &self,
        openai_key: Option<&str>,
        anthropic_key: Option<&str>,
        ollama_url: Option<&str>,
    ) -> Vec<DiscoveredModel> {
        let mut all_models = Vec::new();

        if let Some(key) = openai_key {
            match self.discover_openai(key).await {
                Ok(models) => all_models.extend(models),
                Err(e) => {
                    tracing::warn!("OpenAI model discovery failed: {e}");
                }
            }
        }

        if let Some(key) = anthropic_key {
            match self.discover_anthropic(key).await {
                Ok(models) => all_models.extend(models),
                Err(e) => {
                    tracing::warn!("Anthropic model discovery failed: {e}");
                }
            }
        }

        let ollama = ollama_url.unwrap_or("http://localhost:11434");
        match self.discover_ollama(ollama).await {
            Ok(models) => all_models.extend(models),
            Err(e) => {
                tracing::debug!("Ollama model discovery failed (may not be running): {e}");
            }
        }

        all_models
    }
}

impl Default for ModelDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

// ── API response types ────────────────────────────────────────────────────

/// OpenAI `/v1/models` response.
#[derive(Debug, Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModel>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModel {
    id: String,
}

/// Anthropic `/v1/models` response.
#[derive(Debug, Deserialize)]
struct AnthropicModelsResponse {
    data: Vec<AnthropicModel>,
}

#[derive(Debug, Deserialize)]
struct AnthropicModel {
    id: String,
    display_name: Option<String>,
    #[serde(rename = "context_window")]
    context_window: Option<u64>,
}

/// Ollama `/api/tags` response.
#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
    #[serde(default)]
    context_length: Option<u64>,
}

/// OpenRouter `/models` response.
#[derive(Debug, Deserialize)]
struct OpenRouterModelsResponse {
    data: Vec<OpenRouterModel>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModel {
    id: String,
    name: Option<String>,
    #[serde(default)]
    context_length: Option<u64>,
}

// ── Capability heuristics ─────────────────────────────────────────────────

/// Returns true if the model ID looks like a chat-capable OpenAI model.
fn is_openai_chat_model(id: &str) -> bool {
    let prefixes = ["gpt-4", "gpt-3.5", "o1", "o3", "o4"];
    prefixes.iter().any(|p| id.starts_with(p))
}

/// Infer (context_length, supports_tools, supports_vision) from an OpenAI model ID.
fn openai_model_capabilities(id: &str) -> (u64, bool, bool) {
    if id.starts_with("gpt-4o") || id.starts_with("gpt-4-turbo") {
        (128_000, true, true)
    } else if id.starts_with("gpt-4") {
        (8_192, true, false)
    } else if id.starts_with("o1") || id.starts_with("o3") || id.starts_with("o4") {
        (200_000, true, true)
    } else if id.starts_with("gpt-3.5") {
        (16_385, true, false)
    } else {
        (4_096, false, false)
    }
}

/// Infer (supports_tools, supports_vision) from an Anthropic model ID.
fn anthropic_model_capabilities(id: &str) -> (bool, bool) {
    // All current Claude 3+ models support tools and vision.
    if id.contains("claude-3") || id.contains("claude-4") {
        (true, true)
    } else {
        (true, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovered_model_serialize() {
        let model = DiscoveredModel {
            id: "gpt-4o".to_string(),
            name: "GPT-4o".to_string(),
            provider: "openai".to_string(),
            context_length: 128_000,
            supports_tools: true,
            supports_vision: true,
            is_available: true,
        };
        let json = serde_json::to_string(&model).unwrap();
        assert!(json.contains("gpt-4o"));
        assert!(json.contains("128000"));
    }

    #[test]
    fn test_discovered_model_deserialize() {
        let json = r#"{
            "id": "claude-3-opus",
            "name": "Claude 3 Opus",
            "provider": "anthropic",
            "context_length": 200000,
            "supports_tools": true,
            "supports_vision": true,
            "is_available": true
        }"#;
        let model: DiscoveredModel = serde_json::from_str(json).unwrap();
        assert_eq!(model.id, "claude-3-opus");
        assert_eq!(model.provider, "anthropic");
        assert_eq!(model.context_length, 200_000);
        assert!(model.supports_tools);
    }

    #[test]
    fn test_is_openai_chat_model() {
        assert!(is_openai_chat_model("gpt-4o"));
        assert!(is_openai_chat_model("gpt-4-turbo"));
        assert!(is_openai_chat_model("gpt-3.5-turbo"));
        assert!(is_openai_chat_model("o1-preview"));
        assert!(is_openai_chat_model("o3-mini"));
        assert!(!is_openai_chat_model("dall-e-3"));
        assert!(!is_openai_chat_model("text-embedding-ada-002"));
        assert!(!is_openai_chat_model("whisper-1"));
    }

    #[test]
    fn test_openai_model_capabilities() {
        let (ctx, tools, vision) = openai_model_capabilities("gpt-4o");
        assert_eq!(ctx, 128_000);
        assert!(tools);
        assert!(vision);

        let (ctx, tools, vision) = openai_model_capabilities("gpt-4-turbo-2024-04-09");
        assert_eq!(ctx, 128_000);
        assert!(tools);
        assert!(vision);

        let (ctx, tools, vision) = openai_model_capabilities("gpt-4-0613");
        assert_eq!(ctx, 8_192);
        assert!(tools);
        assert!(!vision);

        let (ctx, tools, vision) = openai_model_capabilities("gpt-3.5-turbo");
        assert_eq!(ctx, 16_385);
        assert!(tools);
        assert!(!vision);

        let (ctx, tools, vision) = openai_model_capabilities("o1-preview");
        assert_eq!(ctx, 200_000);
        assert!(tools);
        assert!(vision);
    }

    #[test]
    fn test_anthropic_model_capabilities() {
        let (tools, vision) = anthropic_model_capabilities("claude-3-opus-20240229");
        assert!(tools);
        assert!(vision);

        let (tools, vision) = anthropic_model_capabilities("claude-3-5-sonnet-20241022");
        assert!(tools);
        assert!(vision);

        let (tools, vision) = anthropic_model_capabilities("claude-2.1");
        assert!(tools);
        assert!(!vision);
    }

    #[test]
    fn test_model_discovery_default() {
        let discovery = ModelDiscovery::new();
        // Verify the instance is created without panicking.
        assert!(std::mem::size_of_val(&discovery) > 0);
    }

    #[test]
    fn test_model_discovery_with_client() {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let discovery = ModelDiscovery::with_client(client);
        assert!(std::mem::size_of_val(&discovery) > 0);
    }

    #[tokio::test]
    async fn test_discover_models_unknown_provider() {
        let discovery = ModelDiscovery::new();
        let err = discovery
            .discover_models("unknown_provider", None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, LlmError::RequestFailed { .. }));
    }

    #[tokio::test]
    async fn test_discover_models_missing_api_key() {
        let discovery = ModelDiscovery::new();

        let err = discovery
            .discover_models("openai", None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, LlmError::AuthFailed { .. }));

        let err = discovery
            .discover_models("anthropic", None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, LlmError::AuthFailed { .. }));
    }

    #[tokio::test]
    async fn test_discover_all_no_credentials() {
        let discovery = ModelDiscovery::new();
        // With no credentials and no local Ollama, should return empty without panicking.
        let models = discovery
            .discover_all(None, None, Some("http://localhost:1"))
            .await;
        assert!(models.is_empty());
    }
}
