//! NEAR AI Chat API provider implementation.
//!
//! This provider uses the NEAR AI chat-api which provides a unified interface
//! to multiple LLM models (OpenAI, Anthropic, etc.) with user authentication.

use async_trait::async_trait;
use reqwest::Client;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};

use crate::config::NearAiConfig;
use crate::error::LlmError;
use crate::llm::provider::{
    ChatMessage, CompletionRequest, CompletionResponse, FinishReason, LlmProvider, Role, ToolCall,
    ToolCompletionRequest, ToolCompletionResponse,
};

/// NEAR AI Chat API provider.
pub struct NearAiProvider {
    client: Client,
    config: NearAiConfig,
}

impl NearAiProvider {
    /// Create a new NEAR AI provider.
    pub fn new(config: NearAiConfig) -> Self {
        // Create client with reasonable timeout
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120)) // 2 minute timeout for LLM calls
            .build()
            .unwrap_or_else(|_| Client::new());

        Self { client, config }
    }

    fn api_url(&self, path: &str) -> String {
        format!(
            "{}/v1/{}",
            self.config.base_url,
            path.trim_start_matches('/')
        )
    }

    async fn send_request<T: Serialize + std::fmt::Debug, R: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R, LlmError> {
        let url = self.api_url(path);

        tracing::debug!("Sending request to NEAR AI: {}", url);
        tracing::debug!("Request body: {:?}", body);

        let response = self
            .client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.session_token.expose_secret()),
            )
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("NEAR AI request failed: {}", e);
                e
            })?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!("NEAR AI response status: {}", status);
        tracing::debug!("NEAR AI response body: {}", response_text);

        if !status.is_success() {
            // Try to parse as JSON error
            if let Ok(error) = serde_json::from_str::<NearAiErrorResponse>(&response_text) {
                if status.as_u16() == 429 {
                    return Err(LlmError::RateLimited {
                        provider: "nearai".to_string(),
                        retry_after: None,
                    });
                }
                return Err(LlmError::RequestFailed {
                    provider: "nearai".to_string(),
                    reason: error.error,
                });
            }

            return Err(LlmError::RequestFailed {
                provider: "nearai".to_string(),
                reason: format!("HTTP {}: {}", status, response_text),
            });
        }

        // Try to parse as our expected type
        match serde_json::from_str::<R>(&response_text) {
            Ok(parsed) => Ok(parsed),
            Err(e) => {
                tracing::debug!("Response is not expected JSON format: {}", e);
                tracing::debug!("Will try alternative parsing in caller");
                Err(LlmError::InvalidResponse {
                    provider: "nearai".to_string(),
                    reason: format!("Parse error: {}. Raw: {}", e, response_text),
                })
            }
        }
    }
}

#[async_trait]
impl LlmProvider for NearAiProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let messages: Vec<NearAiMessage> = req.messages.into_iter().map(Into::into).collect();

        let request = NearAiRequest {
            model: self.config.model.clone(),
            input: messages,
            temperature: req.temperature,
            max_output_tokens: req.max_tokens,
            stream: Some(false),
            tools: None,
        };

        // Try to get structured response, fall back to alternative formats
        let response: NearAiResponse = match self.send_request("responses", &request).await {
            Ok(r) => r,
            Err(LlmError::InvalidResponse { reason, .. }) if reason.contains("Raw: ") => {
                // Extract the raw JSON from the error
                let raw_text = reason.split("Raw: ").nth(1).unwrap_or("");

                // Try parsing as alternative response format
                if let Ok(alt) = serde_json::from_str::<NearAiAltResponse>(raw_text) {
                    tracing::info!("NEAR AI returned alternative response format");
                    let text = extract_text_from_output(&alt.output);
                    let usage = alt.usage.unwrap_or(NearAiUsage {
                        input_tokens: 0,
                        output_tokens: 0,
                    });
                    return Ok(CompletionResponse {
                        content: text,
                        finish_reason: FinishReason::Stop,
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                    });
                }

                // Check if it's a JSON string (quoted)
                let text = if raw_text.starts_with('"') {
                    serde_json::from_str::<String>(raw_text)
                        .unwrap_or_else(|_| raw_text.to_string())
                } else {
                    raw_text.to_string()
                };

                tracing::info!("NEAR AI returned plain text response");
                return Ok(CompletionResponse {
                    content: text,
                    finish_reason: FinishReason::Stop,
                    input_tokens: 0,
                    output_tokens: 0,
                });
            }
            Err(e) => return Err(e),
        };

        tracing::debug!("NEAR AI response: {:?}", response);

        // Extract text from response output
        // Try multiple formats since API response shape may vary
        let text = response
            .output
            .iter()
            .filter_map(|item| {
                tracing::debug!(
                    "Processing output item: type={}, text={:?}",
                    item.item_type,
                    item.text
                );
                if item.item_type == "message" {
                    // First check for direct text field on item
                    if let Some(ref text) = item.text {
                        return Some(text.clone());
                    }
                    // Then check content array
                    item.content.as_ref().map(|contents| {
                        contents
                            .iter()
                            .filter_map(|c| {
                                tracing::debug!(
                                    "Content item: type={}, text={:?}",
                                    c.content_type,
                                    c.text
                                );
                                // Accept various content types that might contain text
                                match c.content_type.as_str() {
                                    "output_text" | "text" => c.text.clone(),
                                    _ => None,
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("")
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");

        if text.is_empty() {
            tracing::warn!(
                "Empty response from NEAR AI. Raw output: {:?}",
                response.output
            );
        }

        Ok(CompletionResponse {
            content: text,
            finish_reason: FinishReason::Stop,
            input_tokens: response.usage.input_tokens,
            output_tokens: response.usage.output_tokens,
        })
    }

    async fn complete_with_tools(
        &self,
        req: ToolCompletionRequest,
    ) -> Result<ToolCompletionResponse, LlmError> {
        let messages: Vec<NearAiMessage> = req.messages.into_iter().map(Into::into).collect();

        let tools: Vec<NearAiTool> = req
            .tools
            .into_iter()
            .map(|t| NearAiTool {
                tool_type: "function".to_string(),
                name: t.name,
                description: Some(t.description),
                parameters: Some(t.parameters),
            })
            .collect();

        let request = NearAiRequest {
            model: self.config.model.clone(),
            input: messages,
            temperature: req.temperature,
            max_output_tokens: req.max_tokens,
            stream: Some(false),
            tools: if tools.is_empty() { None } else { Some(tools) },
        };

        // Try to get structured response, fall back to alternative formats
        let response: NearAiResponse = match self.send_request("responses", &request).await {
            Ok(r) => r,
            Err(LlmError::InvalidResponse { reason, .. }) if reason.contains("Raw: ") => {
                let raw_text = reason.split("Raw: ").nth(1).unwrap_or("");

                // Try parsing as alternative response format
                if let Ok(alt) = serde_json::from_str::<NearAiAltResponse>(raw_text) {
                    tracing::info!("NEAR AI returned alternative response format (tool request)");
                    let text = extract_text_from_output(&alt.output);
                    let usage = alt.usage.unwrap_or(NearAiUsage {
                        input_tokens: 0,
                        output_tokens: 0,
                    });
                    return Ok(ToolCompletionResponse {
                        content: if text.is_empty() { None } else { Some(text) },
                        tool_calls: vec![],
                        finish_reason: FinishReason::Stop,
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                    });
                }

                let text = if raw_text.starts_with('"') {
                    serde_json::from_str::<String>(raw_text)
                        .unwrap_or_else(|_| raw_text.to_string())
                } else {
                    raw_text.to_string()
                };

                tracing::info!("NEAR AI returned plain text response (tool request)");
                return Ok(ToolCompletionResponse {
                    content: Some(text),
                    tool_calls: vec![],
                    finish_reason: FinishReason::Stop,
                    input_tokens: 0,
                    output_tokens: 0,
                });
            }
            Err(e) => return Err(e),
        };

        // Extract text and tool calls from response
        let mut text = String::new();
        let mut tool_calls = Vec::new();

        for item in &response.output {
            if item.item_type == "message" {
                if let Some(contents) = &item.content {
                    for content in contents {
                        if content.content_type == "output_text" {
                            if let Some(t) = &content.text {
                                text.push_str(t);
                            }
                        }
                    }
                }
            } else if item.item_type == "function_call" {
                if let (Some(name), Some(call_id)) = (&item.name, &item.call_id) {
                    // Parse arguments JSON string into Value
                    let arguments = item
                        .arguments
                        .as_ref()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or(serde_json::Value::Object(Default::default()));

                    tool_calls.push(ToolCall {
                        id: call_id.clone(),
                        name: name.clone(),
                        arguments,
                    });
                }
            }
        }

        let finish_reason = if tool_calls.is_empty() {
            FinishReason::Stop
        } else {
            FinishReason::ToolUse
        };

        Ok(ToolCompletionResponse {
            content: if text.is_empty() { None } else { Some(text) },
            tool_calls,
            finish_reason,
            input_tokens: response.usage.input_tokens,
            output_tokens: response.usage.output_tokens,
        })
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn cost_per_token(&self) -> (Decimal, Decimal) {
        // Default costs - could be model-specific in the future
        // These are approximate and may vary by model
        (dec!(0.000003), dec!(0.000015))
    }
}

// NEAR AI API types

/// Request format for NEAR AI Responses API.
/// See: https://docs.near.ai/api
#[derive(Debug, Serialize)]
struct NearAiRequest {
    /// Model identifier (e.g., "fireworks::accounts/fireworks/models/llama-v3p1-405b-instruct")
    model: String,
    /// Input messages - can be a string or array of message objects
    input: Vec<NearAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<NearAiTool>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NearAiMessage {
    role: String,
    content: String,
}

impl From<ChatMessage> for NearAiMessage {
    fn from(msg: ChatMessage) -> Self {
        let role = match msg.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };
        Self {
            role: role.to_string(),
            content: msg.content,
        }
    }
}

#[derive(Debug, Serialize)]
struct NearAiTool {
    #[serde(rename = "type")]
    tool_type: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<serde_json::Value>,
}

/// Primary response format (output array style)
#[derive(Debug, Deserialize)]
struct NearAiResponse {
    #[allow(dead_code)]
    id: String,
    output: Vec<NearAiOutputItem>,
    usage: NearAiUsage,
}

/// Alternative response format (OpenAI-compatible style)
#[derive(Debug, Deserialize)]
struct NearAiAltResponse {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    object: Option<String>,
    #[allow(dead_code)]
    status: Option<String>,
    /// The actual output content
    output: Option<serde_json::Value>,
    /// Usage stats
    usage: Option<NearAiUsage>,
}

#[derive(Debug, Deserialize)]
struct NearAiOutputItem {
    #[serde(rename = "type")]
    item_type: String,
    #[serde(default)]
    content: Option<Vec<NearAiContent>>,
    // Direct text field (some response formats)
    #[serde(default)]
    text: Option<String>,
    // For function calls
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    call_id: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NearAiContent {
    #[serde(rename = "type")]
    content_type: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NearAiUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct NearAiErrorResponse {
    error: String,
}

/// Extract text content from various output formats.
fn extract_text_from_output(output: &Option<serde_json::Value>) -> String {
    let Some(output) = output else {
        return String::new();
    };

    // If output is a string, return it directly
    if let Some(s) = output.as_str() {
        return s.to_string();
    }

    // If output is an array, try to extract text from items
    if let Some(arr) = output.as_array() {
        let texts: Vec<String> = arr
            .iter()
            .filter_map(|item| {
                // Check for direct text field
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    return Some(text.to_string());
                }
                // Check for content array with text
                if let Some(content) = item.get("content").and_then(|c| c.as_array()) {
                    let content_texts: Vec<String> = content
                        .iter()
                        .filter_map(|c| c.get("text").and_then(|t| t.as_str()).map(String::from))
                        .collect();
                    if !content_texts.is_empty() {
                        return Some(content_texts.join(""));
                    }
                }
                // Check for content as string
                if let Some(content) = item.get("content").and_then(|c| c.as_str()) {
                    return Some(content.to_string());
                }
                None
            })
            .collect();
        return texts.join("");
    }

    // If output is an object, try common fields
    if let Some(obj) = output.as_object() {
        if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
            return text.to_string();
        }
        if let Some(content) = obj.get("content").and_then(|c| c.as_str()) {
            return content.to_string();
        }
        if let Some(message) = obj.get("message").and_then(|m| m.as_str()) {
            return message.to_string();
        }
    }

    // Fallback: return JSON representation
    tracing::warn!("Could not extract text from output: {:?}", output);
    output.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_conversion() {
        let msg = ChatMessage::user("Hello");
        let nearai_msg: NearAiMessage = msg.into();
        assert_eq!(nearai_msg.role, "user");
        assert_eq!(nearai_msg.content, "Hello");
    }

    #[test]
    fn test_system_message_conversion() {
        let msg = ChatMessage::system("You are helpful");
        let nearai_msg: NearAiMessage = msg.into();
        assert_eq!(nearai_msg.role, "system");
    }
}
