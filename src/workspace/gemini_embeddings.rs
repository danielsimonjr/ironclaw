//! Google Gemini embedding provider.
//!
//! Uses the Gemini `text-embedding-004` model via the Generative AI REST API
//! for semantic search embeddings.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::workspace::embeddings::{EmbeddingError, EmbeddingProvider};

/// Gemini embedding provider using the Google Generative AI API.
pub struct GeminiEmbeddings {
    client: reqwest::Client,
    api_key: String,
    model: String,
    dimension: usize,
    base_url: String,
}

impl GeminiEmbeddings {
    /// Create a new Gemini embedding provider with the default model (text-embedding-004).
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            model: "text-embedding-004".to_string(),
            dimension: 768,
            base_url: "https://generativelanguage.googleapis.com".to_string(),
        }
    }

    /// Use a specific model with custom dimension.
    pub fn with_model(
        api_key: impl Into<String>,
        model: impl Into<String>,
        dimension: usize,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            model: model.into(),
            dimension,
            base_url: "https://generativelanguage.googleapis.com".to_string(),
        }
    }

    /// Set a custom base URL (for testing or proxies).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiEmbedRequest {
    model: String,
    content: GeminiEmbedContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_dimensionality: Option<usize>,
}

#[derive(Debug, Serialize)]
struct GeminiEmbedContent {
    parts: Vec<GeminiEmbedPart>,
}

#[derive(Debug, Serialize)]
struct GeminiEmbedPart {
    text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiBatchEmbedRequest {
    requests: Vec<GeminiEmbedRequest>,
}

#[derive(Debug, Deserialize)]
struct GeminiEmbedResponse {
    embedding: Option<GeminiEmbedding>,
}

#[derive(Debug, Deserialize)]
struct GeminiEmbedding {
    values: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct GeminiBatchEmbedResponse {
    embeddings: Option<Vec<GeminiEmbedding>>,
}

#[async_trait]
impl EmbeddingProvider for GeminiEmbeddings {
    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn max_input_length(&self) -> usize {
        // Gemini text-embedding-004: 2048 tokens (~8k chars)
        8_000
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        if text.len() > self.max_input_length() {
            return Err(EmbeddingError::TextTooLong {
                length: text.len(),
                max: self.max_input_length(),
            });
        }

        let request = GeminiEmbedRequest {
            model: format!("models/{}", self.model),
            content: GeminiEmbedContent {
                parts: vec![GeminiEmbedPart {
                    text: text.to_string(),
                }],
            },
            output_dimensionality: Some(self.dimension),
        };

        let url = format!(
            "{}/v1beta/models/{}:embedContent?key={}",
            self.base_url, self.model, self.api_key
        );

        let response = self.client.post(&url).json(&request).send().await?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(EmbeddingError::AuthFailed);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(std::time::Duration::from_secs);
            return Err(EmbeddingError::RateLimited { retry_after });
        }
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::HttpError(format!(
                "Status {}: {}",
                status, error_text
            )));
        }

        let result: GeminiEmbedResponse = response.json().await.map_err(|e| {
            EmbeddingError::InvalidResponse(format!("Failed to parse response: {}", e))
        })?;

        result
            .embedding
            .map(|e| e.values)
            .ok_or_else(|| EmbeddingError::InvalidResponse("No embedding in response".to_string()))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let requests: Vec<GeminiEmbedRequest> = texts
            .iter()
            .map(|text| GeminiEmbedRequest {
                model: format!("models/{}", self.model),
                content: GeminiEmbedContent {
                    parts: vec![GeminiEmbedPart { text: text.clone() }],
                },
                output_dimensionality: Some(self.dimension),
            })
            .collect();

        let batch_request = GeminiBatchEmbedRequest { requests };

        let url = format!(
            "{}/v1beta/models/{}:batchEmbedContents?key={}",
            self.base_url, self.model, self.api_key
        );

        let response = self.client.post(&url).json(&batch_request).send().await?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(EmbeddingError::AuthFailed);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(EmbeddingError::RateLimited { retry_after: None });
        }
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::HttpError(format!(
                "Status {}: {}",
                status, error_text
            )));
        }

        let result: GeminiBatchEmbedResponse = response.json().await.map_err(|e| {
            EmbeddingError::InvalidResponse(format!("Failed to parse batch response: {}", e))
        })?;

        result
            .embeddings
            .map(|embs| embs.into_iter().map(|e| e.values).collect())
            .ok_or_else(|| {
                EmbeddingError::InvalidResponse("No embeddings in batch response".to_string())
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_embeddings_config() {
        let provider = GeminiEmbeddings::new("test-key");
        assert_eq!(provider.dimension(), 768);
        assert_eq!(provider.model_name(), "text-embedding-004");
        assert_eq!(provider.max_input_length(), 8_000);
    }

    #[test]
    fn test_gemini_embeddings_custom_model() {
        let provider = GeminiEmbeddings::with_model("test-key", "text-embedding-005", 1024);
        assert_eq!(provider.dimension(), 1024);
        assert_eq!(provider.model_name(), "text-embedding-005");
    }

    #[test]
    fn test_gemini_embeddings_custom_base_url() {
        let provider =
            GeminiEmbeddings::new("test-key").with_base_url("https://custom.api.example.com");
        assert_eq!(provider.base_url, "https://custom.api.example.com");
    }
}
