//! Media handling module.
//!
//! Provides processing capabilities for various media types:
//! - Image processing (resize, format conversion)
//! - Audio transcription (via external APIs)
//! - PDF text extraction
//! - MIME type detection
//! - Vision model integration (image understanding)
//! - Media caching
//! - Sticker-to-image conversion

mod cache;
mod detection;
mod image;
mod pdf;
mod transcription;
mod vision;

pub use cache::MediaCache;
pub use detection::{MediaInfo, MediaType, detect_mime_type, validate_media_url};
pub use image::{ImageFormat, ImageProcessor, ProcessedImage};
pub use pdf::{PdfExtractor, PdfPage};
pub use transcription::{TranscriptionProvider, TranscriptionResult};
pub use vision::{VisionProvider, VisionRequest, VisionResponse};
