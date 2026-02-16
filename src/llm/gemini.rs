//! Google Gemini LLM provider.
//!
//! Supports Gemini models (gemini-2.0-flash, gemini-2.5-pro, etc.) via the
//! Google Generative AI REST API. Uses API key authentication.

use std::sync::RwLock;

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::LlmError;
use crate::llm::costs;
use crate::llm::provider::{
    ChatMessage, CompletionRequest, CompletionResponse, FinishReason, LlmProvider, ModelMetadata,
    Role, ToolCall, ToolCompletionRequest, ToolCompletionResponse, ToolDefinition,
};

/// Google Gemini provider configuration.
#[derive(Debug, Clone)]
pub struct GeminiConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

impl GeminiConfig {
    /// Create a new Gemini config with default base URL.
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            base_url: "https://generativelanguage.googleapis.com".to_string(),
        }
    }
}

/// Google Gemini LLM provider.
pub struct GeminiProvider {
    client: reqwest::Client,
    config: GeminiConfig,
    input_cost: Decimal,
    output_cost: Decimal,
    active_model: RwLock<String>,
}

impl GeminiProvider {
    /// Create a new Gemini provider.
    pub fn new(config: GeminiConfig) -> Self {
        let (input_cost, output_cost) =
            costs::model_cost(&config.model).unwrap_or_else(costs::default_cost);
        let model_name = config.model.clone();
        Self {
            client: reqwest::Client::new(),
            config,
            input_cost,
            output_cost,
            active_model: RwLock::new(model_name),
        }
    }

    /// Convert IronClaw messages to Gemini API format.
    fn convert_messages(messages: &[ChatMessage]) -> (Option<String>, Vec<GeminiContent>) {
        let mut system_instruction = None;
        let mut contents = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => match system_instruction {
                    Some(ref mut s) => {
                        let s: &mut String = s;
                        s.push('\n');
                        s.push_str(&msg.content);
                    }
                    None => system_instruction = Some(msg.content.clone()),
                },
                Role::User | Role::Tool => {
                    contents.push(GeminiContent {
                        role: "user".to_string(),
                        parts: vec![GeminiPart::Text {
                            text: msg.content.clone(),
                        }],
                    });
                }
                Role::Assistant => {
                    let mut parts = Vec::new();
                    if !msg.content.is_empty() {
                        parts.push(GeminiPart::Text {
                            text: msg.content.clone(),
                        });
                    }
                    if let Some(ref tool_calls) = msg.tool_calls {
                        for tc in tool_calls {
                            parts.push(GeminiPart::FunctionCall {
                                function_call: GeminiFunctionCall {
                                    name: tc.name.clone(),
                                    args: tc.arguments.clone(),
                                },
                            });
                        }
                    }
                    if parts.is_empty() {
                        parts.push(GeminiPart::Text {
                            text: String::new(),
                        });
                    }
                    contents.push(GeminiContent {
                        role: "model".to_string(),
                        parts,
                    });
                }
            }
        }

        (system_instruction, contents)
    }

    /// Convert IronClaw tool definitions to Gemini format.
    fn convert_tools(tools: &[ToolDefinition]) -> Vec<GeminiToolDeclaration> {
        tools
            .iter()
            .map(|t| GeminiToolDeclaration {
                function_declarations: vec![GeminiFunctionDeclaration {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                }],
            })
            .collect()
    }

    /// Build the URL for the Gemini API endpoint.
    fn build_url(&self, model: &str) -> String {
        format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            self.config.base_url, model, self.config.api_key
        )
    }
}

// -- Gemini API request/response types --

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<GeminiToolDeclaration>,
}

#[derive(Debug, Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponse,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiToolDeclaration {
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiCandidate {
    content: Option<GeminiContent>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiUsageMetadata {
    prompt_token_count: Option<u32>,
    candidates_token_count: Option<u32>,
    #[allow(dead_code)]
    total_token_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct GeminiErrorResponse {
    error: Option<GeminiErrorDetail>,
}

#[derive(Debug, Deserialize)]
struct GeminiErrorDetail {
    message: String,
    #[allow(dead_code)]
    status: Option<String>,
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn cost_per_token(&self) -> (Decimal, Decimal) {
        (self.input_cost, self.output_cost)
    }

    fn active_model_name(&self) -> String {
        self.active_model
            .read()
            .map(|m| m.clone())
            .unwrap_or_else(|_| self.config.model.clone())
    }

    fn set_model(&self, model: &str) -> Result<(), LlmError> {
        if let Ok(mut active) = self.active_model.write() {
            *active = model.to_string();
            Ok(())
        } else {
            Err(LlmError::RequestFailed {
                provider: "gemini".to_string(),
                reason: "Failed to acquire model lock".to_string(),
            })
        }
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let model = self.active_model_name();
        let (system_instruction, contents) = Self::convert_messages(&request.messages);

        let gemini_req = GeminiRequest {
            contents,
            system_instruction: system_instruction.map(|text| GeminiSystemInstruction {
                parts: vec![GeminiPart::Text { text }],
            }),
            generation_config: Some(GeminiGenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
                top_p: None,
            }),
            tools: Vec::new(),
        };

        let url = self.build_url(&model);
        let response = self
            .client
            .post(&url)
            .json(&gemini_req)
            .send()
            .await
            .map_err(|e| LlmError::RequestFailed {
                provider: "gemini".to_string(),
                reason: format!("HTTP request failed: {}", e),
            })?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(LlmError::AuthFailed {
                provider: "gemini".to_string(),
            });
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(LlmError::RateLimited {
                provider: "gemini".to_string(),
                retry_after: None,
            });
        }
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let msg = serde_json::from_str::<GeminiErrorResponse>(&error_text)
                .ok()
                .and_then(|e| e.error.map(|d| d.message))
                .unwrap_or(error_text);
            return Err(LlmError::RequestFailed {
                provider: "gemini".to_string(),
                reason: format!("Status {}: {}", status, msg),
            });
        }

        let gemini_resp: GeminiResponse =
            response.json().await.map_err(|e| LlmError::RequestFailed {
                provider: "gemini".to_string(),
                reason: format!("Failed to parse response: {}", e),
            })?;

        let candidate = gemini_resp
            .candidates
            .and_then(|mut c| {
                if c.is_empty() {
                    None
                } else {
                    Some(c.remove(0))
                }
            })
            .ok_or_else(|| LlmError::RequestFailed {
                provider: "gemini".to_string(),
                reason: "No candidates in response".to_string(),
            })?;

        let content = candidate
            .content
            .map(|c| {
                c.parts
                    .iter()
                    .filter_map(|p| match p {
                        GeminiPart::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();

        let usage = gemini_resp.usage_metadata.unwrap_or(GeminiUsageMetadata {
            prompt_token_count: Some(0),
            candidates_token_count: Some(0),
            total_token_count: Some(0),
        });

        let finish_reason = match candidate.finish_reason.as_deref() {
            Some("STOP") => FinishReason::Stop,
            Some("MAX_TOKENS") => FinishReason::Length,
            Some("SAFETY") => FinishReason::ContentFilter,
            _ => FinishReason::Stop,
        };

        Ok(CompletionResponse {
            content,
            input_tokens: usage.prompt_token_count.unwrap_or(0),
            output_tokens: usage.candidates_token_count.unwrap_or(0),
            finish_reason,
            response_id: None,
        })
    }

    async fn complete_with_tools(
        &self,
        request: ToolCompletionRequest,
    ) -> Result<ToolCompletionResponse, LlmError> {
        let model = self.active_model_name();
        let (system_instruction, contents) = Self::convert_messages(&request.messages);
        let tools = Self::convert_tools(&request.tools);

        let gemini_req = GeminiRequest {
            contents,
            system_instruction: system_instruction.map(|text| GeminiSystemInstruction {
                parts: vec![GeminiPart::Text { text }],
            }),
            generation_config: Some(GeminiGenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
                top_p: None,
            }),
            tools,
        };

        let url = self.build_url(&model);
        let response = self
            .client
            .post(&url)
            .json(&gemini_req)
            .send()
            .await
            .map_err(|e| LlmError::RequestFailed {
                provider: "gemini".to_string(),
                reason: format!("HTTP request failed: {}", e),
            })?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(LlmError::AuthFailed {
                provider: "gemini".to_string(),
            });
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(LlmError::RateLimited {
                provider: "gemini".to_string(),
                retry_after: None,
            });
        }
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LlmError::RequestFailed {
                provider: "gemini".to_string(),
                reason: format!("Status {}: {}", status, error_text),
            });
        }

        let gemini_resp: GeminiResponse =
            response.json().await.map_err(|e| LlmError::RequestFailed {
                provider: "gemini".to_string(),
                reason: format!("Failed to parse response: {}", e),
            })?;

        let candidate = gemini_resp.candidates.and_then(|mut c| {
            if c.is_empty() {
                None
            } else {
                Some(c.remove(0))
            }
        });

        let mut text_content = None;
        let mut tool_calls = Vec::new();

        if let Some(candidate) = candidate
            && let Some(content) = candidate.content
        {
            for part in content.parts {
                match part {
                    GeminiPart::Text { text } => {
                        if !text.is_empty() {
                            text_content = Some(text);
                        }
                    }
                    GeminiPart::FunctionCall { function_call } => {
                        tool_calls.push(ToolCall {
                            id: format!("call_{}", uuid::Uuid::new_v4()),
                            name: function_call.name,
                            arguments: function_call.args,
                        });
                    }
                    _ => {}
                }
            }
        }

        let usage = gemini_resp.usage_metadata.unwrap_or(GeminiUsageMetadata {
            prompt_token_count: Some(0),
            candidates_token_count: Some(0),
            total_token_count: Some(0),
        });

        let finish_reason = if !tool_calls.is_empty() {
            FinishReason::ToolUse
        } else {
            FinishReason::Stop
        };

        Ok(ToolCompletionResponse {
            content: text_content,
            tool_calls,
            input_tokens: usage.prompt_token_count.unwrap_or(0),
            output_tokens: usage.candidates_token_count.unwrap_or(0),
            finish_reason,
            response_id: None,
        })
    }

    async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        let url = format!(
            "{}/v1beta/models?key={}",
            self.config.base_url, self.config.api_key
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| LlmError::RequestFailed {
                provider: "gemini".to_string(),
                reason: format!("Failed to list models: {}", e),
            })?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        #[derive(Deserialize)]
        struct ModelList {
            models: Option<Vec<ModelInfo>>,
        }
        #[derive(Deserialize)]
        struct ModelInfo {
            name: String,
        }

        let list: ModelList = response.json().await.map_err(|e| LlmError::RequestFailed {
            provider: "gemini".to_string(),
            reason: format!("Failed to parse model list: {}", e),
        })?;

        Ok(list
            .models
            .unwrap_or_default()
            .into_iter()
            .map(|m| m.name.trim_start_matches("models/").to_string())
            .collect())
    }

    async fn model_metadata(&self) -> Result<ModelMetadata, LlmError> {
        let model = self.active_model_name();
        let url = format!(
            "{}/v1beta/models/{}?key={}",
            self.config.base_url, model, self.config.api_key
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| LlmError::RequestFailed {
                provider: "gemini".to_string(),
                reason: format!("Failed to get model metadata: {}", e),
            })?;

        if !response.status().is_success() {
            return Ok(ModelMetadata {
                id: model,
                context_length: None,
            });
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct GeminiModelInfo {
            #[allow(dead_code)]
            name: String,
            input_token_limit: Option<u32>,
        }

        let info: GeminiModelInfo = response.json().await.map_err(|e| LlmError::RequestFailed {
            provider: "gemini".to_string(),
            reason: format!("Failed to parse model info: {}", e),
        })?;

        Ok(ModelMetadata {
            id: model,
            context_length: info.input_token_limit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_messages_basic() {
        let messages = vec![
            ChatMessage::system("You are helpful."),
            ChatMessage::user("Hello"),
            ChatMessage::assistant("Hi there!"),
        ];

        let (system, contents) = GeminiProvider::convert_messages(&messages);
        assert_eq!(system, Some("You are helpful.".to_string()));
        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0].role, "user");
        assert_eq!(contents[1].role, "model");
    }

    #[test]
    fn test_convert_tools() {
        let tools = vec![ToolDefinition {
            name: "search".to_string(),
            description: "Search the web".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        }];

        let gemini_tools = GeminiProvider::convert_tools(&tools);
        assert_eq!(gemini_tools.len(), 1);
        assert_eq!(gemini_tools[0].function_declarations[0].name, "search");
    }

    #[test]
    fn test_gemini_config_default_url() {
        let config = GeminiConfig::new("test-key", "gemini-2.0-flash");
        assert_eq!(config.base_url, "https://generativelanguage.googleapis.com");
        assert_eq!(config.model, "gemini-2.0-flash");
    }

    #[test]
    fn test_build_url() {
        let config = GeminiConfig::new("test-key-123", "gemini-2.0-flash");
        let provider = GeminiProvider::new(config);
        let url = provider.build_url("gemini-2.0-flash");
        assert!(url.contains("gemini-2.0-flash:generateContent"));
        assert!(url.contains("key=test-key-123"));
    }
}
