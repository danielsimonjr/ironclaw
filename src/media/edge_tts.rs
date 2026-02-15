//! Microsoft Edge TTS provider.
//!
//! Uses Microsoft Edge's text-to-speech service for free, high-quality
//! speech synthesis. Supports multiple voices and languages.
//!
//! Edge TTS communicates via WebSocket to the speech service endpoint
//! and returns audio in MP3 format.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::MediaError;
use crate::media::tts::{TtsFormat, TtsProvider, TtsVoice, VoiceGender};

/// Edge TTS voice metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeVoice {
    /// Short name (e.g., "en-US-AriaNeural").
    pub short_name: String,
    /// Display name.
    pub display_name: String,
    /// Locale (e.g., "en-US").
    pub locale: String,
    /// Gender.
    pub gender: String,
    /// Voice type (e.g., "Neural").
    pub voice_type: String,
}

/// Microsoft Edge TTS provider.
///
/// Provides free, high-quality neural TTS without requiring API keys.
/// Uses the same service that powers Microsoft Edge's "Read Aloud" feature.
pub struct EdgeTtsProvider {
    /// Maximum text length per request.
    max_text_length: usize,
    /// Default voice to use.
    default_voice: String,
    /// WebSocket endpoint for the TTS service.
    endpoint: String,
}

impl EdgeTtsProvider {
    /// Edge TTS WebSocket endpoint.
    const DEFAULT_ENDPOINT: &'static str =
        "wss://speech.platform.bing.com/consumer/speech/synthesize/readaloud/edge/v1";

    /// Create a new Edge TTS provider.
    pub fn new() -> Self {
        Self {
            max_text_length: 10_000,
            default_voice: "en-US-AriaNeural".to_string(),
            endpoint: Self::DEFAULT_ENDPOINT.to_string(),
        }
    }

    /// Set the default voice.
    pub fn with_default_voice(mut self, voice: String) -> Self {
        self.default_voice = voice;
        self
    }

    /// Set maximum text length.
    pub fn with_max_text_length(mut self, max: usize) -> Self {
        self.max_text_length = max;
        self
    }

    /// Get the default voice name.
    pub fn default_voice(&self) -> &str {
        &self.default_voice
    }

    /// Get the WebSocket endpoint.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Get the maximum text length.
    pub fn max_text_length(&self) -> usize {
        self.max_text_length
    }

    /// Get the list of popular Edge TTS voices.
    pub fn popular_voices() -> Vec<EdgeVoice> {
        vec![
            EdgeVoice {
                short_name: "en-US-AriaNeural".to_string(),
                display_name: "Aria".to_string(),
                locale: "en-US".to_string(),
                gender: "Female".to_string(),
                voice_type: "Neural".to_string(),
            },
            EdgeVoice {
                short_name: "en-US-GuyNeural".to_string(),
                display_name: "Guy".to_string(),
                locale: "en-US".to_string(),
                gender: "Male".to_string(),
                voice_type: "Neural".to_string(),
            },
            EdgeVoice {
                short_name: "en-US-JennyNeural".to_string(),
                display_name: "Jenny".to_string(),
                locale: "en-US".to_string(),
                gender: "Female".to_string(),
                voice_type: "Neural".to_string(),
            },
            EdgeVoice {
                short_name: "en-GB-SoniaNeural".to_string(),
                display_name: "Sonia".to_string(),
                locale: "en-GB".to_string(),
                gender: "Female".to_string(),
                voice_type: "Neural".to_string(),
            },
            EdgeVoice {
                short_name: "en-GB-RyanNeural".to_string(),
                display_name: "Ryan".to_string(),
                locale: "en-GB".to_string(),
                gender: "Male".to_string(),
                voice_type: "Neural".to_string(),
            },
            EdgeVoice {
                short_name: "es-ES-ElviraNeural".to_string(),
                display_name: "Elvira".to_string(),
                locale: "es-ES".to_string(),
                gender: "Female".to_string(),
                voice_type: "Neural".to_string(),
            },
            EdgeVoice {
                short_name: "fr-FR-DeniseNeural".to_string(),
                display_name: "Denise".to_string(),
                locale: "fr-FR".to_string(),
                gender: "Female".to_string(),
                voice_type: "Neural".to_string(),
            },
            EdgeVoice {
                short_name: "de-DE-KatjaNeural".to_string(),
                display_name: "Katja".to_string(),
                locale: "de-DE".to_string(),
                gender: "Female".to_string(),
                voice_type: "Neural".to_string(),
            },
            EdgeVoice {
                short_name: "ja-JP-NanamiNeural".to_string(),
                display_name: "Nanami".to_string(),
                locale: "ja-JP".to_string(),
                gender: "Female".to_string(),
                voice_type: "Neural".to_string(),
            },
            EdgeVoice {
                short_name: "zh-CN-XiaoxiaoNeural".to_string(),
                display_name: "Xiaoxiao".to_string(),
                locale: "zh-CN".to_string(),
                gender: "Female".to_string(),
                voice_type: "Neural".to_string(),
            },
        ]
    }

    /// Build SSML from text and voice.
    pub fn build_ssml(text: &str, voice_name: &str) -> String {
        format!(
            r#"<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="en-US"><voice name="{}">{}</voice></speak>"#,
            voice_name,
            escape_xml(text)
        )
    }
}

/// Escape special XML characters.
fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

impl Default for EdgeTtsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TtsProvider for EdgeTtsProvider {
    async fn synthesize(
        &self,
        text: &str,
        voice: &TtsVoice,
        _format: TtsFormat,
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

        // Build SSML
        let _ssml = Self::build_ssml(text, &voice.name);

        // TODO: Connect to Edge TTS WebSocket endpoint and stream audio
        // For now, return an error indicating the WebSocket transport is not yet connected
        Err(MediaError::ProcessingFailed {
            reason: "Edge TTS WebSocket transport not yet connected. \
                     Install tokio-tungstenite for WebSocket support."
                .to_string(),
        })
    }

    fn name(&self) -> &str {
        "edge_tts"
    }

    fn is_available(&self) -> bool {
        // Edge TTS is free and always available (no API key needed)
        true
    }

    fn available_voices(&self) -> Vec<TtsVoice> {
        Self::popular_voices()
            .into_iter()
            .map(|v| {
                let gender = match v.gender.as_str() {
                    "Male" => VoiceGender::Male,
                    "Female" => VoiceGender::Female,
                    _ => VoiceGender::Neutral,
                };
                TtsVoice::new(v.short_name, v.locale, gender)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation_defaults() {
        let provider = EdgeTtsProvider::new();
        assert_eq!(provider.default_voice(), "en-US-AriaNeural");
        assert_eq!(provider.max_text_length(), 10_000);
        assert_eq!(provider.endpoint(), EdgeTtsProvider::DEFAULT_ENDPOINT);
    }

    #[test]
    fn test_provider_default_trait() {
        let provider = EdgeTtsProvider::default();
        assert_eq!(provider.default_voice(), "en-US-AriaNeural");
        assert_eq!(provider.max_text_length(), 10_000);
    }

    #[test]
    fn test_provider_name() {
        let provider = EdgeTtsProvider::new();
        assert_eq!(provider.name(), "edge_tts");
    }

    #[test]
    fn test_is_available_returns_true() {
        let provider = EdgeTtsProvider::new();
        assert!(provider.is_available());
    }

    #[test]
    fn test_with_default_voice() {
        let provider = EdgeTtsProvider::new().with_default_voice("en-GB-RyanNeural".to_string());
        assert_eq!(provider.default_voice(), "en-GB-RyanNeural");
    }

    #[test]
    fn test_with_max_text_length() {
        let provider = EdgeTtsProvider::new().with_max_text_length(5_000);
        assert_eq!(provider.max_text_length(), 5_000);
    }

    #[test]
    fn test_builder_chaining() {
        let provider = EdgeTtsProvider::new()
            .with_default_voice("en-US-GuyNeural".to_string())
            .with_max_text_length(20_000);
        assert_eq!(provider.default_voice(), "en-US-GuyNeural");
        assert_eq!(provider.max_text_length(), 20_000);
    }

    #[test]
    fn test_popular_voices_count() {
        let voices = EdgeTtsProvider::popular_voices();
        assert_eq!(voices.len(), 10);
    }

    #[test]
    fn test_popular_voices_content() {
        let voices = EdgeTtsProvider::popular_voices();
        let names: Vec<&str> = voices.iter().map(|v| v.short_name.as_str()).collect();
        assert!(names.contains(&"en-US-AriaNeural"));
        assert!(names.contains(&"en-US-GuyNeural"));
        assert!(names.contains(&"en-US-JennyNeural"));
        assert!(names.contains(&"en-GB-SoniaNeural"));
        assert!(names.contains(&"en-GB-RyanNeural"));
        assert!(names.contains(&"es-ES-ElviraNeural"));
        assert!(names.contains(&"fr-FR-DeniseNeural"));
        assert!(names.contains(&"de-DE-KatjaNeural"));
        assert!(names.contains(&"ja-JP-NanamiNeural"));
        assert!(names.contains(&"zh-CN-XiaoxiaoNeural"));
    }

    #[test]
    fn test_popular_voices_have_neural_type() {
        let voices = EdgeTtsProvider::popular_voices();
        for voice in &voices {
            assert_eq!(
                voice.voice_type, "Neural",
                "Voice {} should be Neural",
                voice.short_name
            );
        }
    }

    #[test]
    fn test_available_voices_list() {
        let provider = EdgeTtsProvider::new();
        let voices = provider.available_voices();
        assert_eq!(voices.len(), 10);

        // Check first voice is Aria
        assert_eq!(voices[0].name, "en-US-AriaNeural");
        assert_eq!(voices[0].language, "en-US");
        assert_eq!(voices[0].gender, VoiceGender::Female);
    }

    #[test]
    fn test_available_voices_gender_mapping() {
        let provider = EdgeTtsProvider::new();
        let voices = provider.available_voices();

        // Aria is female
        let aria = voices
            .iter()
            .find(|v| v.name == "en-US-AriaNeural")
            .unwrap();
        assert_eq!(aria.gender, VoiceGender::Female);

        // Guy is male
        let guy = voices.iter().find(|v| v.name == "en-US-GuyNeural").unwrap();
        assert_eq!(guy.gender, VoiceGender::Male);

        // Ryan is male
        let ryan = voices
            .iter()
            .find(|v| v.name == "en-GB-RyanNeural")
            .unwrap();
        assert_eq!(ryan.gender, VoiceGender::Male);
    }

    #[test]
    fn test_build_ssml_basic() {
        let ssml = EdgeTtsProvider::build_ssml("Hello world", "en-US-AriaNeural");
        assert!(ssml.contains("en-US-AriaNeural"));
        assert!(ssml.contains("Hello world"));
        assert!(ssml.starts_with("<speak"));
        assert!(ssml.ends_with("</speak>"));
        assert!(ssml.contains("<voice name=\"en-US-AriaNeural\">"));
    }

    #[test]
    fn test_build_ssml_with_special_characters() {
        let ssml = EdgeTtsProvider::build_ssml("5 > 3 & 2 < 4", "en-US-AriaNeural");
        assert!(ssml.contains("5 &gt; 3 &amp; 2 &lt; 4"));
        assert!(!ssml.contains("5 > 3 & 2 < 4"));
    }

    #[test]
    fn test_escape_xml_ampersand() {
        assert_eq!(escape_xml("A & B"), "A &amp; B");
    }

    #[test]
    fn test_escape_xml_less_than() {
        assert_eq!(escape_xml("a < b"), "a &lt; b");
    }

    #[test]
    fn test_escape_xml_greater_than() {
        assert_eq!(escape_xml("a > b"), "a &gt; b");
    }

    #[test]
    fn test_escape_xml_quotes() {
        assert_eq!(escape_xml(r#"say "hello""#), "say &quot;hello&quot;");
    }

    #[test]
    fn test_escape_xml_apostrophe() {
        assert_eq!(escape_xml("it's"), "it&apos;s");
    }

    #[test]
    fn test_escape_xml_all_special_chars() {
        let input = r#"<a & b> "c" 'd'"#;
        let expected = "&lt;a &amp; b&gt; &quot;c&quot; &apos;d&apos;";
        assert_eq!(escape_xml(input), expected);
    }

    #[test]
    fn test_escape_xml_no_special_chars() {
        assert_eq!(escape_xml("plain text"), "plain text");
    }

    #[tokio::test]
    async fn test_synthesize_rejects_empty_text() {
        let provider = EdgeTtsProvider::new();
        let voice = TtsVoice::new("en-US-AriaNeural", "en-US", VoiceGender::Female);
        let result = provider.synthesize("", &voice, TtsFormat::Mp3).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("empty text"),
            "Error should mention empty text: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_synthesize_rejects_text_too_long() {
        let provider = EdgeTtsProvider::new().with_max_text_length(10);
        let voice = TtsVoice::new("en-US-AriaNeural", "en-US", VoiceGender::Female);
        let long_text = "a".repeat(100);
        let result = provider
            .synthesize(&long_text, &voice, TtsFormat::Mp3)
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("exceeds maximum"),
            "Error should mention length limit: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_synthesize_returns_error_for_unimplemented_transport() {
        let provider = EdgeTtsProvider::new();
        let voice = TtsVoice::new("en-US-AriaNeural", "en-US", VoiceGender::Female);
        let result = provider
            .synthesize("Hello world", &voice, TtsFormat::Mp3)
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("WebSocket"),
            "Error should mention WebSocket: {}",
            err
        );
    }

    #[test]
    fn test_edge_voice_serialization() {
        let voice = EdgeVoice {
            short_name: "en-US-AriaNeural".to_string(),
            display_name: "Aria".to_string(),
            locale: "en-US".to_string(),
            gender: "Female".to_string(),
            voice_type: "Neural".to_string(),
        };
        let json = serde_json::to_string(&voice).unwrap();
        assert!(json.contains("en-US-AriaNeural"));
        assert!(json.contains("Aria"));
    }

    #[test]
    fn test_edge_voice_deserialization() {
        let json = r#"{"short_name":"en-US-GuyNeural","display_name":"Guy","locale":"en-US","gender":"Male","voice_type":"Neural"}"#;
        let voice: EdgeVoice = serde_json::from_str(json).unwrap();
        assert_eq!(voice.short_name, "en-US-GuyNeural");
        assert_eq!(voice.display_name, "Guy");
        assert_eq!(voice.locale, "en-US");
        assert_eq!(voice.gender, "Male");
        assert_eq!(voice.voice_type, "Neural");
    }

    #[test]
    fn test_edge_voice_roundtrip_serialization() {
        let original = EdgeVoice {
            short_name: "ja-JP-NanamiNeural".to_string(),
            display_name: "Nanami".to_string(),
            locale: "ja-JP".to_string(),
            gender: "Female".to_string(),
            voice_type: "Neural".to_string(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: EdgeVoice = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.short_name, original.short_name);
        assert_eq!(deserialized.display_name, original.display_name);
        assert_eq!(deserialized.locale, original.locale);
        assert_eq!(deserialized.gender, original.gender);
        assert_eq!(deserialized.voice_type, original.voice_type);
    }

    #[test]
    fn test_default_endpoint_value() {
        assert!(EdgeTtsProvider::DEFAULT_ENDPOINT.starts_with("wss://"));
        assert!(EdgeTtsProvider::DEFAULT_ENDPOINT.contains("speech.platform.bing.com"));
    }

    #[test]
    fn test_available_voices_locales() {
        let provider = EdgeTtsProvider::new();
        let voices = provider.available_voices();
        let locales: Vec<&str> = voices.iter().map(|v| v.language.as_str()).collect();
        assert!(locales.contains(&"en-US"));
        assert!(locales.contains(&"en-GB"));
        assert!(locales.contains(&"es-ES"));
        assert!(locales.contains(&"fr-FR"));
        assert!(locales.contains(&"de-DE"));
        assert!(locales.contains(&"ja-JP"));
        assert!(locales.contains(&"zh-CN"));
    }

    #[test]
    fn test_build_ssml_structure() {
        let ssml = EdgeTtsProvider::build_ssml("Test", "en-US-AriaNeural");
        assert!(ssml.contains(r#"version="1.0""#));
        assert!(ssml.contains(r#"xmlns="http://www.w3.org/2001/10/synthesis""#));
        assert!(ssml.contains(r#"xml:lang="en-US""#));
    }
}
