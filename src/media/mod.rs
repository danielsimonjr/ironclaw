//! Media handling module.
//!
//! Provides processing capabilities for various media types:
//! - Image processing (resize, format conversion)
//! - Audio transcription (via external APIs)
//! - PDF text extraction
//! - MIME type detection
//! - Vision model integration (image understanding)
//! - Media caching
//! - Sticker-to-image conversion (WebP, TGS, animated WebP)
//! - Video metadata extraction (MP4, WebM, AVI, MOV, MKV)
//! - Text-to-speech synthesis (via OpenAI TTS API)

mod cache;
mod detection;
mod edge_tts;
mod image;
mod pdf;
mod sticker;
mod transcription;
mod tts;
mod video;
mod vision;

pub use cache::MediaCache;
pub use detection::{MediaInfo, MediaType, detect_mime_type, validate_media_url};
pub use edge_tts::{EdgeTtsProvider, EdgeVoice};
pub use image::{ImageFormat, ImageProcessor, ProcessedImage};
pub use pdf::{PdfExtractor, PdfPage};
pub use sticker::{ConvertedSticker, StickerConverter, StickerFormat};
pub use transcription::{TranscriptionProvider, TranscriptionResult};
pub use tts::{OpenAiTtsProvider, TtsFormat, TtsProvider, TtsVoice, VoiceGender};
pub use video::{VideoFormat, VideoInfo, VideoProcessor};
pub use vision::{VisionProvider, VisionRequest, VisionResponse};
