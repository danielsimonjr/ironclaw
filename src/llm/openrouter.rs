//! OpenRouter LLM provider implementation.
//!
//! OpenRouter (<https://openrouter.ai>) is a unified API gateway that provides
//! access to models from OpenAI, Anthropic, Google, Meta, and many others
//! through a single OpenAI-compatible endpoint.

use async_trait::async_trait;
use reqwest::Client;
use rust_decimal::Decimal;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};

use crate::config::OpenRouterConfig;
use crate::error::LlmError;
use crate::llm::costs;
use crate::llm::provider::{
    ChatMessage, CompletionRequest, CompletionResponse, FinishReason, LlmProvider, ModelMetadata,
    Role, ToolCall, ToolCompletionRequest, ToolCompletionResponse,
};

/// OpenRouter LLM provider.
///
/// Sends requests to the OpenRouter API using the OpenAI-compatible chat
/// completions format. Supports tool calling and model listing.
pub struct OpenRouterProvider {
    client: Client,
    config: OpenRouterConfig,
    active_model: std::sync::RwLock<String>,
}

impl OpenRouterProvider {
    /// Create a new OpenRouter provider.
    pub fn new(config: OpenRouterConfig) -> Result<Self, LlmError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_else(|_| Client::new());

        let active_model = std::sync::RwLock::new(config.model.clone());
        Ok(Self {
            client,
            config,
            active_model,
        })
    }

    fn api_url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.config.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    fn api_key(&self) -> String {
        self.config.api_key.expose_secret().to_string()
    }

    /// Send a request to the OpenRouter chat completions API.
    async fn send_request<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        body: &T,
    ) -> Result<R, LlmError> {
        let url = self.api_url("chat/completions");

        tracing::debug!("Sending request to OpenRouter: {}", url);

        if tracing::enabled!(tracing::Level::DEBUG)
            && let Ok(json) = serde_json::to_string(body)
        {
            tracing::debug!("OpenRouter request body: {}", json);
        }

        let mut req = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key()))
            .header("Content-Type", "application/json")
            .header("X-Title", "IronClaw");

        if let Some(ref referer) = self.config.referer {
            req = req.header("HTTP-Referer", referer.as_str());
        }

        let response = req.json(body).send().await.map_err(|e| {
            tracing::error!("OpenRouter request failed: {}", e);
            LlmError::RequestFailed {
                provider: "openrouter".to_string(),
                reason: e.to_string(),
            }
        })?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!("OpenRouter response status: {}", status);
        tracing::debug!("OpenRouter response body: {}", response_text);

        if !status.is_success() {
            if status.as_u16() == 401 || status.as_u16() == 403 {
                return Err(LlmError::AuthFailed {
                    provider: "openrouter".to_string(),
                });
            }
            if status.as_u16() == 429 {
                return Err(LlmError::RateLimited {
                    provider: "openrouter".to_string(),
                    retry_after: None,
                });
            }
            return Err(LlmError::RequestFailed {
                provider: "openrouter".to_string(),
                reason: format!("HTTP {}: {}", status, response_text),
            });
        }

        serde_json::from_str(&response_text).map_err(|e| LlmError::InvalidResponse {
            provider: "openrouter".to_string(),
            reason: format!("JSON parse error: {}. Raw: {}", e, response_text),
        })
    }

    /// Fetch available models from the OpenRouter `/models` endpoint.
    async fn fetch_models(&self) -> Result<Vec<OpenRouterModelEntry>, LlmError> {
        let url = self.api_url("models");

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key()))
            .send()
            .await
            .map_err(|e| LlmError::RequestFailed {
                provider: "openrouter".to_string(),
                reason: format!("Failed to fetch models: {}", e),
            })?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(LlmError::RequestFailed {
                provider: "openrouter".to_string(),
                reason: format!("HTTP {}: {}", status, response_text),
            });
        }

        #[derive(Deserialize)]
        struct ModelsResponse {
            data: Vec<OpenRouterModelEntry>,
        }

        let resp: ModelsResponse =
            serde_json::from_str(&response_text).map_err(|e| LlmError::InvalidResponse {
                provider: "openrouter".to_string(),
                reason: format!("JSON parse error: {}", e),
            })?;

        Ok(resp.data)
    }
}

/// Model entry from the OpenRouter `/models` API.
#[derive(Debug, Deserialize)]
struct OpenRouterModelEntry {
    id: String,
    #[serde(default)]
    context_length: Option<u32>,
}

#[async_trait]
impl LlmProvider for OpenRouterProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let messages: Vec<ChatCompletionMessage> =
            req.messages.into_iter().map(|m| m.into()).collect();

        let request = ChatCompletionRequest {
            model: self.active_model_name(),
            messages,
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            tools: None,
            tool_choice: None,
        };

        let response: ChatCompletionResponse = self.send_request(&request).await?;

        let choice =
            response
                .choices
                .into_iter()
                .next()
                .ok_or_else(|| LlmError::InvalidResponse {
                    provider: "openrouter".to_string(),
                    reason: "No choices in response".to_string(),
                })?;

        let content = choice.message.content.unwrap_or_default();
        let finish_reason = parse_finish_reason(choice.finish_reason.as_deref());

        Ok(CompletionResponse {
            content,
            finish_reason,
            input_tokens: response.usage.prompt_tokens,
            output_tokens: response.usage.completion_tokens,
            response_id: None,
        })
    }

    async fn complete_with_tools(
        &self,
        req: ToolCompletionRequest,
    ) -> Result<ToolCompletionResponse, LlmError> {
        let messages: Vec<ChatCompletionMessage> =
            req.messages.into_iter().map(|m| m.into()).collect();

        let tools: Vec<ChatCompletionTool> = req
            .tools
            .into_iter()
            .map(|t| ChatCompletionTool {
                tool_type: "function".to_string(),
                function: ChatCompletionFunction {
                    name: t.name,
                    description: Some(t.description),
                    parameters: Some(t.parameters),
                },
            })
            .collect();

        let request = ChatCompletionRequest {
            model: self.active_model_name(),
            messages,
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            tools: if tools.is_empty() { None } else { Some(tools) },
            tool_choice: req.tool_choice,
        };

        let response: ChatCompletionResponse = self.send_request(&request).await?;

        let choice =
            response
                .choices
                .into_iter()
                .next()
                .ok_or_else(|| LlmError::InvalidResponse {
                    provider: "openrouter".to_string(),
                    reason: "No choices in response".to_string(),
                })?;

        let content = choice.message.content;
        let tool_calls: Vec<ToolCall> = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|tc| {
                let arguments = serde_json::from_str(&tc.function.arguments)
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                ToolCall {
                    id: tc.id,
                    name: tc.function.name,
                    arguments,
                }
            })
            .collect();

        let finish_reason = match choice.finish_reason.as_deref() {
            Some("tool_calls") => FinishReason::ToolUse,
            _ if !tool_calls.is_empty() => FinishReason::ToolUse,
            other => parse_finish_reason(other),
        };

        Ok(ToolCompletionResponse {
            content,
            tool_calls,
            finish_reason,
            input_tokens: response.usage.prompt_tokens,
            output_tokens: response.usage.completion_tokens,
            response_id: None,
        })
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn cost_per_token(&self) -> (Decimal, Decimal) {
        // Look up cost for the underlying model (OpenRouter model IDs use
        // "provider/model" format which model_cost() strips automatically).
        let active = self.active_model_name();
        costs::model_cost(&active).unwrap_or_else(costs::default_cost)
    }

    async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        let models = self.fetch_models().await?;
        Ok(models.into_iter().map(|m| m.id).collect())
    }

    async fn model_metadata(&self) -> Result<ModelMetadata, LlmError> {
        let active = self.active_model_name();
        let models = self.fetch_models().await?;
        let current = models.iter().find(|m| m.id == active);
        Ok(ModelMetadata {
            id: active,
            context_length: current.and_then(|m| m.context_length),
        })
    }

    fn active_model_name(&self) -> String {
        self.active_model
            .read()
            .expect("active_model lock poisoned")
            .clone()
    }

    fn set_model(&self, model: &str) -> Result<(), LlmError> {
        let mut guard = self
            .active_model
            .write()
            .expect("active_model lock poisoned");
        *guard = model.to_string();
        Ok(())
    }
}

/// Parse an OpenAI-style finish_reason string into a `FinishReason`.
fn parse_finish_reason(reason: Option<&str>) -> FinishReason {
    match reason {
        Some("stop") => FinishReason::Stop,
        Some("length") => FinishReason::Length,
        Some("tool_calls") => FinishReason::ToolUse,
        Some("content_filter") => FinishReason::ContentFilter,
        _ => FinishReason::Unknown,
    }
}

// ── OpenAI-compatible Chat Completions API types ──────────────────────────

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatCompletionMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ChatCompletionTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatCompletionMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ChatCompletionToolCall>>,
}

impl From<ChatMessage> for ChatCompletionMessage {
    fn from(msg: ChatMessage) -> Self {
        let role = match msg.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };

        let tool_calls = msg.tool_calls.map(|calls| {
            calls
                .into_iter()
                .map(|tc| ChatCompletionToolCall {
                    id: tc.id,
                    call_type: "function".to_string(),
                    function: ChatCompletionToolCallFunction {
                        name: tc.name,
                        arguments: tc.arguments.to_string(),
                    },
                })
                .collect()
        });

        let content = if role == "assistant" && tool_calls.is_some() && msg.content.is_empty() {
            None
        } else {
            Some(msg.content)
        };

        Self {
            role: role.to_string(),
            content,
            tool_call_id: msg.tool_call_id,
            name: msg.name,
            tool_calls,
        }
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: ChatCompletionFunction,
}

#[derive(Debug, Serialize)]
struct ChatCompletionFunction {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    #[allow(dead_code)]
    id: String,
    choices: Vec<ChatCompletionChoice>,
    usage: ChatCompletionUsage,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    message: ChatCompletionResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponseMessage {
    #[allow(dead_code)]
    role: String,
    content: Option<String>,
    tool_calls: Option<Vec<ChatCompletionToolCall>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatCompletionToolCall {
    id: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    call_type: String,
    function: ChatCompletionToolCallFunction,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatCompletionToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    #[allow(dead_code)]
    total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_conversion() {
        let msg = ChatMessage::user("Hello from OpenRouter");
        let chat_msg: ChatCompletionMessage = msg.into();
        assert_eq!(chat_msg.role, "user");
        assert_eq!(chat_msg.content, Some("Hello from OpenRouter".to_string()));
    }

    #[test]
    fn test_tool_message_conversion() {
        let msg = ChatMessage::tool_result("call_456", "my_tool", "result data");
        let chat_msg: ChatCompletionMessage = msg.into();
        assert_eq!(chat_msg.role, "tool");
        assert_eq!(chat_msg.tool_call_id, Some("call_456".to_string()));
        assert_eq!(chat_msg.name, Some("my_tool".to_string()));
    }

    #[test]
    fn test_assistant_with_tool_calls_conversion() {
        use crate::llm::ToolCall;

        let tool_calls = vec![ToolCall {
            id: "call_1".to_string(),
            name: "search".to_string(),
            arguments: serde_json::json!({"query": "test"}),
        }];

        let msg = ChatMessage::assistant_with_tool_calls(None, tool_calls);
        let chat_msg: ChatCompletionMessage = msg.into();

        assert_eq!(chat_msg.role, "assistant");
        let tc = chat_msg.tool_calls.expect("tool_calls present");
        assert_eq!(tc.len(), 1);
        assert_eq!(tc[0].id, "call_1");
        assert_eq!(tc[0].function.name, "search");
    }

    #[test]
    fn test_parse_finish_reason() {
        assert_eq!(parse_finish_reason(Some("stop")), FinishReason::Stop);
        assert_eq!(parse_finish_reason(Some("length")), FinishReason::Length);
        assert_eq!(
            parse_finish_reason(Some("tool_calls")),
            FinishReason::ToolUse
        );
        assert_eq!(
            parse_finish_reason(Some("content_filter")),
            FinishReason::ContentFilter
        );
        assert_eq!(parse_finish_reason(None), FinishReason::Unknown);
        assert_eq!(
            parse_finish_reason(Some("something_else")),
            FinishReason::Unknown
        );
    }
}
