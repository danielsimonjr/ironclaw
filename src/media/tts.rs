//! Text-to-speech support.
//!
//! Provides a trait-based abstraction for TTS providers, with an
//! OpenAI TTS API implementation. Supports multiple audio output
//! formats and voice configurations.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::MediaError;

/// Supported TTS audio output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TtsFormat {
    /// MPEG Audio Layer III.
    Mp3,
    /// Waveform Audio File Format.
    Wav,
    /// Ogg Vorbis container.
    Ogg,
    /// Opus audio codec.
    Opus,
}

impl std::fmt::Display for TtsFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mp3 => write!(f, "mp3"),
            Self::Wav => write!(f, "wav"),
            Self::Ogg => write!(f, "ogg"),
            Self::Opus => write!(f, "opus"),
        }
    }
}

impl TtsFormat {
    /// Get the MIME type for this audio format.
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Mp3 => "audio/mpeg",
            Self::Wav => "audio/wav",
            Self::Ogg => "audio/ogg",
            Self::Opus => "audio/opus",
        }
    }

    /// Get the file extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Wav => "wav",
            Self::Ogg => "ogg",
            Self::Opus => "opus",
        }
    }

    /// Parse from a format string.
    pub fn from_str_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "mp3" => Some(Self::Mp3),
            "wav" => Some(Self::Wav),
            "ogg" => Some(Self::Ogg),
            "opus" => Some(Self::Opus),
            _ => None,
        }
    }
}

/// Voice configuration for TTS synthesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsVoice {
    /// Voice identifier (provider-specific, e.g., "alloy", "echo", "nova").
    pub name: String,
    /// Language code (e.g., "en", "es", "fr").
    pub language: String,
    /// Gender of the voice.
    pub gender: VoiceGender,
}

/// Gender classification for TTS voices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceGender {
    /// Male voice.
    Male,
    /// Female voice.
    Female,
    /// Neutral or unspecified gender.
    Neutral,
}

impl std::fmt::Display for VoiceGender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Male => write!(f, "male"),
            Self::Female => write!(f, "female"),
            Self::Neutral => write!(f, "neutral"),
        }
    }
}

impl TtsVoice {
    /// Create a new voice configuration.
    pub fn new(name: impl Into<String>, language: impl Into<String>, gender: VoiceGender) -> Self {
        Self {
            name: name.into(),
            language: language.into(),
            gender,
        }
    }
}

/// Trait for text-to-speech providers.
#[async_trait]
pub trait TtsProvider: Send + Sync {
    /// Synthesize speech from text.
    ///
    /// # Arguments
    /// * `text` - The text to convert to speech
    /// * `voice` - Voice configuration to use
    /// * `format` - Desired audio output format
    ///
    /// # Returns
    /// Raw audio bytes in the requested format.
    async fn synthesize(
        &self,
        text: &str,
        voice: &TtsVoice,
        format: TtsFormat,
    ) -> Result<Vec<u8>, MediaError>;

    /// Get the provider name.
    fn name(&self) -> &str;

    /// Check if the provider is available and configured.
    fn is_available(&self) -> bool;

    /// List available voices for this provider.
    fn available_voices(&self) -> Vec<TtsVoice>;
}

/// OpenAI TTS API provider.
///
/// Uses the OpenAI `/v1/audio/speech` endpoint to synthesize speech.
/// Supports voices: alloy, echo, fable, onyx, nova, shimmer.
pub struct OpenAiTtsProvider {
    api_key: String,
    base_url: String,
    model: String,
    max_text_length: usize,
}

impl OpenAiTtsProvider {
    /// Create a new OpenAI TTS provider.
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: "https://api.openai.com/v1".to_string(),
            model: "tts-1".to_string(),
            max_text_length: 4096,
        }
    }

    /// Use a custom base URL (for OpenAI-compatible endpoints).
    pub fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }

    /// Set the TTS model to use (e.g., "tts-1" or "tts-1-hd").
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    /// Set the maximum text length for a single request.
    pub fn with_max_text_length(mut self, max: usize) -> Self {
        self.max_text_length = max;
        self
    }

    /// Map TtsFormat to the OpenAI API response_format parameter.
    fn format_to_api_param(format: TtsFormat) -> &'static str {
        match format {
            TtsFormat::Mp3 => "mp3",
            TtsFormat::Wav => "wav",
            TtsFormat::Ogg => "opus", // OpenAI uses "opus" for Ogg container
            TtsFormat::Opus => "opus",
        }
    }

    /// Get the default available voices for the OpenAI TTS API.
    fn default_voices() -> Vec<TtsVoice> {
        vec![
            TtsVoice::new("alloy", "en", VoiceGender::Neutral),
            TtsVoice::new("echo", "en", VoiceGender::Male),
            TtsVoice::new("fable", "en", VoiceGender::Neutral),
            TtsVoice::new("onyx", "en", VoiceGender::Male),
            TtsVoice::new("nova", "en", VoiceGender::Female),
            TtsVoice::new("shimmer", "en", VoiceGender::Female),
        ]
    }
}

#[async_trait]
impl TtsProvider for OpenAiTtsProvider {
    async fn synthesize(
        &self,
        text: &str,
        voice: &TtsVoice,
        format: TtsFormat,
    ) -> Result<Vec<u8>, MediaError> {
        if text.is_empty() {
            return Err(MediaError::ProcessingFailed {
                reason: "Cannot synthesize empty text".to_string(),
            });
        }

        if text.len() > self.max_text_length {
            return Err(MediaError::ProcessingFailed {
                reason: format!(
                    "Text length {} exceeds maximum {} characters",
                    text.len(),
                    self.max_text_length
                ),
            });
        }

        let body = serde_json::json!({
            "model": self.model,
            "input": text,
            "voice": voice.name,
            "response_format": Self::format_to_api_param(format),
        });

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/audio/speech", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| MediaError::ProcessingFailed {
                reason: format!("TTS HTTP request failed: {}", e),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(MediaError::ProcessingFailed {
                reason: format!("OpenAI TTS API returned {}: {}", status, body),
            });
        }

        let audio_bytes = response
            .bytes()
            .await
            .map_err(|e| MediaError::ProcessingFailed {
                reason: format!("Failed to read TTS response body: {}", e),
            })?;

        Ok(audio_bytes.to_vec())
    }

    fn name(&self) -> &str {
        "openai_tts"
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn available_voices(&self) -> Vec<TtsVoice> {
        Self::default_voices()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tts_format_display() {
        assert_eq!(TtsFormat::Mp3.to_string(), "mp3");
        assert_eq!(TtsFormat::Wav.to_string(), "wav");
        assert_eq!(TtsFormat::Ogg.to_string(), "ogg");
        assert_eq!(TtsFormat::Opus.to_string(), "opus");
    }

    #[test]
    fn test_tts_format_mime_type() {
        assert_eq!(TtsFormat::Mp3.mime_type(), "audio/mpeg");
        assert_eq!(TtsFormat::Wav.mime_type(), "audio/wav");
        assert_eq!(TtsFormat::Ogg.mime_type(), "audio/ogg");
        assert_eq!(TtsFormat::Opus.mime_type(), "audio/opus");
    }

    #[test]
    fn test_tts_format_extension() {
        assert_eq!(TtsFormat::Mp3.extension(), "mp3");
        assert_eq!(TtsFormat::Wav.extension(), "wav");
        assert_eq!(TtsFormat::Ogg.extension(), "ogg");
        assert_eq!(TtsFormat::Opus.extension(), "opus");
    }

    #[test]
    fn test_tts_format_from_str_name() {
        assert_eq!(TtsFormat::from_str_name("mp3"), Some(TtsFormat::Mp3));
        assert_eq!(TtsFormat::from_str_name("WAV"), Some(TtsFormat::Wav));
        assert_eq!(TtsFormat::from_str_name("Ogg"), Some(TtsFormat::Ogg));
        assert_eq!(TtsFormat::from_str_name("opus"), Some(TtsFormat::Opus));
        assert_eq!(TtsFormat::from_str_name("flac"), None);
    }

    #[test]
    fn test_voice_gender_display() {
        assert_eq!(VoiceGender::Male.to_string(), "male");
        assert_eq!(VoiceGender::Female.to_string(), "female");
        assert_eq!(VoiceGender::Neutral.to_string(), "neutral");
    }

    #[test]
    fn test_tts_voice_new() {
        let voice = TtsVoice::new("alloy", "en", VoiceGender::Neutral);
        assert_eq!(voice.name, "alloy");
        assert_eq!(voice.language, "en");
        assert_eq!(voice.gender, VoiceGender::Neutral);
    }

    #[test]
    fn test_openai_provider_creation() {
        let provider = OpenAiTtsProvider::new("test-key".to_string());
        assert_eq!(provider.name(), "openai_tts");
        assert!(provider.is_available());
    }

    #[test]
    fn test_openai_provider_unavailable_with_empty_key() {
        let provider = OpenAiTtsProvider::new(String::new());
        assert!(!provider.is_available());
    }

    #[test]
    fn test_openai_provider_with_custom_url() {
        let provider = OpenAiTtsProvider::new("key".to_string())
            .with_base_url("https://custom.api.example.com/v1".to_string());
        assert_eq!(provider.base_url, "https://custom.api.example.com/v1");
    }

    #[test]
    fn test_openai_provider_with_custom_model() {
        let provider = OpenAiTtsProvider::new("key".to_string()).with_model("tts-1-hd".to_string());
        assert_eq!(provider.model, "tts-1-hd");
    }

    #[test]
    fn test_openai_provider_available_voices() {
        let provider = OpenAiTtsProvider::new("key".to_string());
        let voices = provider.available_voices();
        assert_eq!(voices.len(), 6);

        let voice_names: Vec<&str> = voices.iter().map(|v| v.name.as_str()).collect();
        assert!(voice_names.contains(&"alloy"));
        assert!(voice_names.contains(&"echo"));
        assert!(voice_names.contains(&"fable"));
        assert!(voice_names.contains(&"onyx"));
        assert!(voice_names.contains(&"nova"));
        assert!(voice_names.contains(&"shimmer"));
    }

    #[test]
    fn test_format_to_api_param() {
        assert_eq!(
            OpenAiTtsProvider::format_to_api_param(TtsFormat::Mp3),
            "mp3"
        );
        assert_eq!(
            OpenAiTtsProvider::format_to_api_param(TtsFormat::Wav),
            "wav"
        );
        assert_eq!(
            OpenAiTtsProvider::format_to_api_param(TtsFormat::Ogg),
            "opus"
        );
        assert_eq!(
            OpenAiTtsProvider::format_to_api_param(TtsFormat::Opus),
            "opus"
        );
    }

    #[tokio::test]
    async fn test_synthesize_rejects_empty_text() {
        let provider = OpenAiTtsProvider::new("test-key".to_string());
        let voice = TtsVoice::new("alloy", "en", VoiceGender::Neutral);
        let result = provider.synthesize("", &voice, TtsFormat::Mp3).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_synthesize_rejects_text_too_long() {
        let provider = OpenAiTtsProvider::new("test-key".to_string()).with_max_text_length(10);
        let voice = TtsVoice::new("alloy", "en", VoiceGender::Neutral);
        let long_text = "a".repeat(100);
        let result = provider
            .synthesize(&long_text, &voice, TtsFormat::Mp3)
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_tts_voice_serialization() {
        let voice = TtsVoice::new("nova", "en", VoiceGender::Female);
        let json = serde_json::to_string(&voice).unwrap();
        let deserialized: TtsVoice = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "nova");
        assert_eq!(deserialized.language, "en");
        assert_eq!(deserialized.gender, VoiceGender::Female);
    }

    #[test]
    fn test_tts_format_serialization() {
        let format = TtsFormat::Opus;
        let json = serde_json::to_string(&format).unwrap();
        let deserialized: TtsFormat = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, TtsFormat::Opus);
    }
}
