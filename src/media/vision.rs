//! Vision model integration for image understanding.
//!
//! Enables the agent to understand images by sending them to vision-capable LLMs.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::MediaError;

/// Request for vision model analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionRequest {
    /// The image data (base64-encoded or URL).
    pub image: ImageSource,
    /// Prompt/question about the image.
    pub prompt: String,
    /// Optional detail level ("low", "high", "auto").
    pub detail: Option<String>,
    /// Maximum tokens for the response.
    pub max_tokens: Option<u32>,
}

/// Source of an image for vision analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    /// Base64-encoded image data.
    Base64 { data: String, media_type: String },
    /// URL to an image.
    Url { url: String },
}

/// Response from vision model analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionResponse {
    /// The model's description/analysis of the image.
    pub content: String,
    /// Tokens used for the request.
    pub input_tokens: Option<u32>,
    /// Tokens used for the response.
    pub output_tokens: Option<u32>,
    /// Provider that performed the analysis.
    pub provider: String,
}

/// Trait for vision model providers.
#[async_trait]
pub trait VisionProvider: Send + Sync {
    /// Analyze an image with a vision model.
    async fn analyze(&self, request: VisionRequest) -> Result<VisionResponse, MediaError>;

    /// Get the provider name.
    fn name(&self) -> &str;

    /// Check if the provider supports vision.
    fn is_available(&self) -> bool;
}

/// OpenAI-compatible vision provider (works with GPT-4V, Claude, etc.).
pub struct OpenAiVisionProvider {
    api_key: String,
    base_url: String,
    model: String,
}

impl OpenAiVisionProvider {
    /// Create a new OpenAI vision provider.
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            base_url: "https://api.openai.com/v1".to_string(),
            model,
        }
    }

    /// Use a custom base URL.
    pub fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }
}

#[async_trait]
impl VisionProvider for OpenAiVisionProvider {
    async fn analyze(&self, request: VisionRequest) -> Result<VisionResponse, MediaError> {
        let image_content = match &request.image {
            ImageSource::Base64 { data, media_type } => {
                serde_json::json!({
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:{};base64,{}", media_type, data),
                        "detail": request.detail.as_deref().unwrap_or("auto")
                    }
                })
            }
            ImageSource::Url { url } => {
                serde_json::json!({
                    "type": "image_url",
                    "image_url": {
                        "url": url,
                        "detail": request.detail.as_deref().unwrap_or("auto")
                    }
                })
            }
        };

        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        { "type": "text", "text": request.prompt },
                        image_content
                    ]
                }
            ],
            "max_tokens": request.max_tokens.unwrap_or(1024)
        });

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| MediaError::VisionFailed {
                reason: format!("HTTP request failed: {}", e),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(MediaError::VisionFailed {
                reason: format!("Vision API returned {}: {}", status, body),
            });
        }

        let result: serde_json::Value =
            response
                .json()
                .await
                .map_err(|e| MediaError::VisionFailed {
                    reason: format!("Failed to parse response: {}", e),
                })?;

        let content = result["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let input_tokens = result["usage"]["prompt_tokens"].as_u64().map(|v| v as u32);
        let output_tokens = result["usage"]["completion_tokens"]
            .as_u64()
            .map(|v| v as u32);

        Ok(VisionResponse {
            content,
            input_tokens,
            output_tokens,
            provider: "openai_vision".to_string(),
        })
    }

    fn name(&self) -> &str {
        "openai_vision"
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}
