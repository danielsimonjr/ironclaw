//! AWS Bedrock LLM provider.
//!
//! Supports AWS Bedrock models (Claude, Llama, Mistral, Titan, etc.) using
//! AWS Signature V4 authentication. Implements the Bedrock Runtime
//! `InvokeModel` / `Converse` API via direct HTTP calls.

use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::LlmError;
use crate::llm::costs;
use crate::llm::provider::{
    ChatMessage, CompletionRequest, CompletionResponse, FinishReason, LlmProvider, Role, ToolCall,
    ToolCompletionRequest, ToolCompletionResponse, ToolDefinition,
};

/// AWS Bedrock provider configuration.
#[derive(Debug, Clone)]
pub struct BedrockConfig {
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
    pub model_id: String,
}

impl BedrockConfig {
    pub fn new(
        region: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
        model_id: impl Into<String>,
    ) -> Self {
        Self {
            region: region.into(),
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token: None,
            model_id: model_id.into(),
        }
    }

    pub fn with_session_token(mut self, token: impl Into<String>) -> Self {
        self.session_token = Some(token.into());
        self
    }
}

/// AWS Bedrock LLM provider using the Converse API.
pub struct BedrockProvider {
    client: reqwest::Client,
    config: BedrockConfig,
    input_cost: Decimal,
    output_cost: Decimal,
    active_model: RwLock<String>,
}

impl BedrockProvider {
    /// Create a new Bedrock provider.
    pub fn new(config: BedrockConfig) -> Self {
        let (input_cost, output_cost) =
            costs::model_cost(&config.model_id).unwrap_or_else(costs::default_cost);
        let model_id = config.model_id.clone();
        Self {
            client: reqwest::Client::new(),
            config,
            input_cost,
            output_cost,
            active_model: RwLock::new(model_id),
        }
    }

    /// Build the Bedrock Converse API endpoint URL.
    fn converse_url(&self, model_id: &str) -> String {
        format!(
            "https://bedrock-runtime.{}.amazonaws.com/model/{}/converse",
            self.config.region, model_id
        )
    }

    /// Sign a request using AWS Signature V4.
    ///
    /// This is a simplified SigV4 implementation suitable for Bedrock API calls.
    fn sign_request(
        &self,
        method: &str,
        url: &url::Url,
        payload_hash: &str,
        timestamp: &str,
        date: &str,
    ) -> HashMap<String, String> {
        use hmac::{Hmac, Mac};
        use sha2::{Digest, Sha256};

        type HmacSha256 = Hmac<Sha256>;

        let host = url.host_str().unwrap_or_default();
        let path = url.path();
        let service = "bedrock";

        // Canonical request
        let canonical_headers = format!(
            "host:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
            host, payload_hash, timestamp
        );
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";

        let canonical_request = format!(
            "{}\n{}\n\n{}\n{}\n{}",
            method, path, canonical_headers, signed_headers, payload_hash
        );

        // String to sign
        let credential_scope = format!("{}/{}/bedrock/aws4_request", date, self.config.region);
        let mut hasher = Sha256::new();
        hasher.update(canonical_request.as_bytes());
        let canonical_hash = hex::encode(hasher.finalize());
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            timestamp, credential_scope, canonical_hash
        );

        // Signing key
        let k_date = {
            let mut mac = HmacSha256::new_from_slice(
                format!("AWS4{}", self.config.secret_access_key).as_bytes(),
            )
            .expect("HMAC key");
            mac.update(date.as_bytes());
            mac.finalize().into_bytes()
        };
        let k_region = {
            let mut mac = HmacSha256::new_from_slice(&k_date).expect("HMAC key");
            mac.update(self.config.region.as_bytes());
            mac.finalize().into_bytes()
        };
        let k_service = {
            let mut mac = HmacSha256::new_from_slice(&k_region).expect("HMAC key");
            mac.update(service.as_bytes());
            mac.finalize().into_bytes()
        };
        let k_signing = {
            let mut mac = HmacSha256::new_from_slice(&k_service).expect("HMAC key");
            mac.update(b"aws4_request");
            mac.finalize().into_bytes()
        };

        // Signature
        let signature = {
            let mut mac = HmacSha256::new_from_slice(&k_signing).expect("HMAC key");
            mac.update(string_to_sign.as_bytes());
            hex::encode(mac.finalize().into_bytes())
        };

        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.config.access_key_id, credential_scope, signed_headers, signature
        );

        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), authorization);
        headers.insert("x-amz-date".to_string(), timestamp.to_string());
        headers.insert("x-amz-content-sha256".to_string(), payload_hash.to_string());
        if let Some(ref token) = self.config.session_token {
            headers.insert("x-amz-security-token".to_string(), token.clone());
        }
        headers
    }

    /// Make an authenticated request to the Bedrock Converse API.
    async fn converse(&self, request: &ConverseRequest) -> Result<ConverseResponse, LlmError> {
        let model = self.active_model_name();
        let url_str = self.converse_url(&model);
        let url: url::Url = url_str.parse().map_err(|e| LlmError::RequestFailed {
            provider: "bedrock".to_string(),
            reason: format!("Invalid URL: {}", e),
        })?;

        let body = serde_json::to_string(request).map_err(|e| LlmError::RequestFailed {
            provider: "bedrock".to_string(),
            reason: format!("Failed to serialize request: {}", e),
        })?;

        // Compute payload hash
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(body.as_bytes());
        let payload_hash = hex::encode(hasher.finalize());

        // Generate timestamp
        let now = chrono::Utc::now();
        let timestamp = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date = now.format("%Y%m%d").to_string();

        let auth_headers = self.sign_request("POST", &url, &payload_hash, &timestamp, &date);

        let mut req_builder = self
            .client
            .post(url_str)
            .header("Content-Type", "application/json");

        for (key, value) in &auth_headers {
            req_builder = req_builder.header(key, value);
        }

        let response =
            req_builder
                .body(body)
                .send()
                .await
                .map_err(|e| LlmError::RequestFailed {
                    provider: "bedrock".to_string(),
                    reason: format!("HTTP request failed: {}", e),
                })?;

        let status = response.status();
        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(LlmError::AuthFailed {
                provider: "bedrock".to_string(),
            });
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS
            || status == reqwest::StatusCode::SERVICE_UNAVAILABLE
        {
            return Err(LlmError::RateLimited {
                provider: "bedrock".to_string(),
                retry_after: None,
            });
        }
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LlmError::RequestFailed {
                provider: "bedrock".to_string(),
                reason: format!("Status {}: {}", status, error_text),
            });
        }

        response.json().await.map_err(|e| LlmError::RequestFailed {
            provider: "bedrock".to_string(),
            reason: format!("Failed to parse response: {}", e),
        })
    }
}

// -- Bedrock Converse API types --

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConverseRequest {
    messages: Vec<ConverseMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<Vec<ConverseSystemContent>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inference_config: Option<ConverseInferenceConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_config: Option<ConverseToolConfig>,
}

#[derive(Debug, Serialize)]
struct ConverseMessage {
    role: String,
    content: Vec<ConverseContent>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum ConverseContent {
    Text {
        text: String,
    },
    ToolUse {
        #[serde(rename = "toolUse")]
        tool_use: ConverseToolUse,
    },
    ToolResult {
        #[serde(rename = "toolResult")]
        tool_result: ConverseToolResult,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConverseToolUse {
    tool_use_id: String,
    name: String,
    input: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConverseToolResult {
    tool_use_id: String,
    content: Vec<ConverseToolResultContent>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ConverseToolResultContent {
    text: String,
}

#[derive(Debug, Serialize)]
struct ConverseSystemContent {
    text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConverseInferenceConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
struct ConverseToolConfig {
    tools: Vec<ConverseTool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConverseTool {
    tool_spec: ConverseToolSpec,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConverseToolSpec {
    name: String,
    description: String,
    input_schema: ConverseInputSchema,
}

#[derive(Debug, Serialize)]
struct ConverseInputSchema {
    json: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConverseResponse {
    output: Option<ConverseOutput>,
    usage: Option<ConverseUsage>,
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConverseOutput {
    message: Option<ConverseOutputMessage>,
}

#[derive(Debug, Deserialize)]
struct ConverseOutputMessage {
    content: Vec<ConverseContent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConverseUsage {
    input_tokens: u32,
    output_tokens: u32,
}

/// Convert IronClaw messages to Bedrock Converse API format.
fn convert_messages(
    messages: &[ChatMessage],
) -> (Option<Vec<ConverseSystemContent>>, Vec<ConverseMessage>) {
    let mut system = Vec::new();
    let mut converse_msgs = Vec::new();

    for msg in messages {
        match msg.role {
            Role::System => {
                system.push(ConverseSystemContent {
                    text: msg.content.clone(),
                });
            }
            Role::User => {
                converse_msgs.push(ConverseMessage {
                    role: "user".to_string(),
                    content: vec![ConverseContent::Text {
                        text: msg.content.clone(),
                    }],
                });
            }
            Role::Assistant => {
                let mut content = Vec::new();
                if !msg.content.is_empty() {
                    content.push(ConverseContent::Text {
                        text: msg.content.clone(),
                    });
                }
                if let Some(ref tool_calls) = msg.tool_calls {
                    for tc in tool_calls {
                        content.push(ConverseContent::ToolUse {
                            tool_use: ConverseToolUse {
                                tool_use_id: tc.id.clone(),
                                name: tc.name.clone(),
                                input: tc.arguments.clone(),
                            },
                        });
                    }
                }
                if content.is_empty() {
                    content.push(ConverseContent::Text {
                        text: String::new(),
                    });
                }
                converse_msgs.push(ConverseMessage {
                    role: "assistant".to_string(),
                    content,
                });
            }
            Role::Tool => {
                converse_msgs.push(ConverseMessage {
                    role: "user".to_string(),
                    content: vec![ConverseContent::ToolResult {
                        tool_result: ConverseToolResult {
                            tool_use_id: msg.tool_call_id.clone().unwrap_or_default(),
                            content: vec![ConverseToolResultContent {
                                text: msg.content.clone(),
                            }],
                        },
                    }],
                });
            }
        }
    }

    let system_opt = if system.is_empty() {
        None
    } else {
        Some(system)
    };

    (system_opt, converse_msgs)
}

/// Convert IronClaw tool definitions to Bedrock Converse tool config.
fn convert_tools(tools: &[ToolDefinition]) -> ConverseToolConfig {
    ConverseToolConfig {
        tools: tools
            .iter()
            .map(|t| ConverseTool {
                tool_spec: ConverseToolSpec {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    input_schema: ConverseInputSchema {
                        json: t.parameters.clone(),
                    },
                },
            })
            .collect(),
    }
}

#[async_trait]
impl LlmProvider for BedrockProvider {
    fn model_name(&self) -> &str {
        &self.config.model_id
    }

    fn cost_per_token(&self) -> (Decimal, Decimal) {
        (self.input_cost, self.output_cost)
    }

    fn active_model_name(&self) -> String {
        self.active_model
            .read()
            .map(|m| m.clone())
            .unwrap_or_else(|_| self.config.model_id.clone())
    }

    fn set_model(&self, model: &str) -> Result<(), LlmError> {
        if let Ok(mut active) = self.active_model.write() {
            *active = model.to_string();
            Ok(())
        } else {
            Err(LlmError::RequestFailed {
                provider: "bedrock".to_string(),
                reason: "Failed to acquire model lock".to_string(),
            })
        }
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let (system, messages) = convert_messages(&request.messages);

        let converse_req = ConverseRequest {
            messages,
            system,
            inference_config: Some(ConverseInferenceConfig {
                max_tokens: request.max_tokens,
                temperature: request.temperature,
            }),
            tool_config: None,
        };

        let resp = self.converse(&converse_req).await?;

        let content = resp
            .output
            .and_then(|o| o.message)
            .map(|m| {
                m.content
                    .iter()
                    .filter_map(|c| match c {
                        ConverseContent::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();

        let usage = resp.usage.unwrap_or(ConverseUsage {
            input_tokens: 0,
            output_tokens: 0,
        });

        let finish_reason = match resp.stop_reason.as_deref() {
            Some("end_turn") | Some("stop") => FinishReason::Stop,
            Some("max_tokens") => FinishReason::Length,
            Some("tool_use") => FinishReason::ToolUse,
            Some("content_filtered") => FinishReason::ContentFilter,
            _ => FinishReason::Stop,
        };

        Ok(CompletionResponse {
            content,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            finish_reason,
            response_id: None,
        })
    }

    async fn complete_with_tools(
        &self,
        request: ToolCompletionRequest,
    ) -> Result<ToolCompletionResponse, LlmError> {
        let (system, messages) = convert_messages(&request.messages);
        let tool_config = convert_tools(&request.tools);

        let converse_req = ConverseRequest {
            messages,
            system,
            inference_config: Some(ConverseInferenceConfig {
                max_tokens: request.max_tokens,
                temperature: request.temperature,
            }),
            tool_config: Some(tool_config),
        };

        let resp = self.converse(&converse_req).await?;

        let mut text_content = None;
        let mut tool_calls = Vec::new();

        if let Some(output) = resp.output
            && let Some(message) = output.message
        {
            for content in message.content {
                match content {
                    ConverseContent::Text { text } => {
                        if !text.is_empty() {
                            text_content = Some(text);
                        }
                    }
                    ConverseContent::ToolUse { tool_use } => {
                        tool_calls.push(ToolCall {
                            id: tool_use.tool_use_id,
                            name: tool_use.name,
                            arguments: tool_use.input,
                        });
                    }
                    _ => {}
                }
            }
        }

        let usage = resp.usage.unwrap_or(ConverseUsage {
            input_tokens: 0,
            output_tokens: 0,
        });

        let finish_reason = if !tool_calls.is_empty() {
            FinishReason::ToolUse
        } else {
            match resp.stop_reason.as_deref() {
                Some("end_turn") | Some("stop") => FinishReason::Stop,
                Some("max_tokens") => FinishReason::Length,
                Some("content_filtered") => FinishReason::ContentFilter,
                _ => FinishReason::Stop,
            }
        };

        Ok(ToolCompletionResponse {
            content: text_content,
            tool_calls,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            finish_reason,
            response_id: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_messages() {
        let messages = vec![
            ChatMessage::system("You are helpful."),
            ChatMessage::user("Hello"),
            ChatMessage::assistant("Hi!"),
        ];

        let (system, msgs) = convert_messages(&messages);
        assert!(system.is_some());
        assert_eq!(system.unwrap().len(), 1);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[1].role, "assistant");
    }

    #[test]
    fn test_convert_tools() {
        let tools = vec![ToolDefinition {
            name: "search".to_string(),
            description: "Search the web".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        }];

        let config = convert_tools(&tools);
        assert_eq!(config.tools.len(), 1);
        assert_eq!(config.tools[0].tool_spec.name, "search");
    }

    #[test]
    fn test_bedrock_config() {
        let config = BedrockConfig::new(
            "us-east-1",
            "AKIAIOSFODNN7EXAMPLE",
            "secret",
            "anthropic.claude-3-5-sonnet-20241022-v2:0",
        );
        assert_eq!(config.region, "us-east-1");
        assert_eq!(config.model_id, "anthropic.claude-3-5-sonnet-20241022-v2:0");
    }

    #[test]
    fn test_converse_url() {
        let config = BedrockConfig::new("us-west-2", "key", "secret", "anthropic.claude-v2");
        let provider = BedrockProvider::new(config);
        let url = provider.converse_url("anthropic.claude-v2");
        assert_eq!(
            url,
            "https://bedrock-runtime.us-west-2.amazonaws.com/model/anthropic.claude-v2/converse"
        );
    }
}
