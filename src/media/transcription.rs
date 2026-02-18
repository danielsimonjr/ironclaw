//! Audio transcription via external APIs.
//!
//! Supports transcription through:
//! - OpenAI Whisper API
//! - Custom HTTP endpoints

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::MediaError;

/// Result of audio transcription.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    /// The transcribed text.
    pub text: String,
    /// Language detected (ISO 639-1 code).
    pub language: Option<String>,
    /// Duration of the audio in seconds.
    pub duration_seconds: Option<f64>,
    /// Provider that performed the transcription.
    pub provider: String,
}

/// Trait for transcription providers.
#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    /// Transcribe audio data.
    ///
    /// # Arguments
    /// * `data` - Raw audio bytes
    /// * `mime_type` - MIME type of the audio (e.g., "audio/mpeg")
    /// * `language` - Optional language hint (ISO 639-1)
    async fn transcribe(
        &self,
        data: &[u8],
        mime_type: &str,
        language: Option<&str>,
    ) -> Result<TranscriptionResult, MediaError>;

    /// Get the provider name.
    fn name(&self) -> &str;

    /// Check if the provider is available and configured.
    fn is_available(&self) -> bool;
}

/// OpenAI Whisper-based transcription provider.
#[allow(dead_code)]
pub struct WhisperProvider {
    api_key: String,
    base_url: String,
    model: String,
}

#[allow(dead_code)]
impl WhisperProvider {
    /// Create a new Whisper provider.
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: "https://api.openai.com/v1".to_string(),
            model: "whisper-1".to_string(),
        }
    }

    /// Use a custom base URL (for OpenAI-compatible endpoints).
    pub fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }

    /// Set the model to use.
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }
}

#[async_trait]
impl TranscriptionProvider for WhisperProvider {
    async fn transcribe(
        &self,
        data: &[u8],
        mime_type: &str,
        language: Option<&str>,
    ) -> Result<TranscriptionResult, MediaError> {
        let extension = match mime_type {
            "audio/mpeg" | "audio/mp3" => "mp3",
            "audio/wav" => "wav",
            "audio/ogg" => "ogg",
            "audio/mp4" | "audio/m4a" => "m4a",
            "audio/flac" => "flac",
            "audio/webm" => "webm",
            _ => "mp3",
        };

        let file_part = reqwest::multipart::Part::bytes(data.to_vec())
            .file_name(format!("audio.{}", extension))
            .mime_str(mime_type)
            .map_err(|e| MediaError::TranscriptionFailed {
                reason: format!("Failed to create multipart: {}", e),
            })?;

        let mut form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", self.model.clone())
            .text("response_format", "json");

        if let Some(lang) = language {
            form = form.text("language", lang.to_string());
        }

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/audio/transcriptions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .multipart(form)
            .send()
            .await
            .map_err(|e| MediaError::TranscriptionFailed {
                reason: format!("HTTP request failed: {}", e),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(MediaError::TranscriptionFailed {
                reason: format!("Whisper API returned {}: {}", status, body),
            });
        }

        #[derive(Deserialize)]
        struct WhisperResponse {
            text: String,
        }

        let result: WhisperResponse =
            response
                .json()
                .await
                .map_err(|e| MediaError::TranscriptionFailed {
                    reason: format!("Failed to parse response: {}", e),
                })?;

        Ok(TranscriptionResult {
            text: result.text,
            language: language.map(String::from),
            duration_seconds: None,
            provider: "whisper".to_string(),
        })
    }

    fn name(&self) -> &str {
        "whisper"
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}
