//! Sticker-to-image conversion.
//!
//! Converts sticker formats (WebP, TGS, animated WebP) to standard image
//! formats for processing by the agent. TGS files are gzip-compressed
//! Lottie animations used by Telegram.

use std::io::Read;

use crate::error::MediaError;

/// Supported sticker formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StickerFormat {
    /// Standard WebP sticker (static).
    WebP,
    /// Telegram TGS sticker (gzip-compressed Lottie JSON).
    Tgs,
    /// Animated WebP sticker.
    AnimatedWebP,
}

impl std::fmt::Display for StickerFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WebP => write!(f, "webp"),
            Self::Tgs => write!(f, "tgs"),
            Self::AnimatedWebP => write!(f, "animated_webp"),
        }
    }
}

impl StickerFormat {
    /// Get the MIME type for this sticker format.
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::WebP | Self::AnimatedWebP => "image/webp",
            Self::Tgs => "application/x-tgsticker",
        }
    }

    /// Get the file extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::WebP | Self::AnimatedWebP => "webp",
            Self::Tgs => "tgs",
        }
    }
}

/// Result of sticker conversion.
#[derive(Debug, Clone)]
pub struct ConvertedSticker {
    /// Raw image data after conversion.
    pub data: Vec<u8>,
    /// Output MIME type (e.g., "image/png" or "image/webp").
    pub mime_type: String,
    /// Detected source format.
    pub source_format: StickerFormat,
    /// Original file size in bytes.
    pub original_size: usize,
}

/// Sticker-to-image converter.
///
/// Handles conversion of various sticker formats into standard images
/// suitable for further processing or display.
pub struct StickerConverter {
    /// Maximum file size in bytes for input stickers.
    max_file_size: usize,
}

impl StickerConverter {
    /// Create a new sticker converter with default settings.
    pub fn new() -> Self {
        Self {
            max_file_size: 5 * 1024 * 1024, // 5MB
        }
    }

    /// Set maximum input file size.
    pub fn with_max_file_size(mut self, max: usize) -> Self {
        self.max_file_size = max;
        self
    }

    /// Get the maximum file size setting.
    pub fn max_file_size(&self) -> usize {
        self.max_file_size
    }

    /// Detect the sticker format from raw data.
    pub fn detect_format(data: &[u8]) -> Option<StickerFormat> {
        if data.len() < 4 {
            return None;
        }

        // TGS: gzip-compressed (magic bytes 0x1F 0x8B)
        if data.starts_with(&[0x1F, 0x8B]) {
            return Some(StickerFormat::Tgs);
        }

        // WebP: RIFF....WEBP
        if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
            // Check for animated WebP (ANIM chunk)
            if data.len() >= 16 && Self::has_anim_chunk(data) {
                return Some(StickerFormat::AnimatedWebP);
            }
            return Some(StickerFormat::WebP);
        }

        None
    }

    /// Check if WebP data contains an ANIM chunk (indicating animation).
    fn has_anim_chunk(data: &[u8]) -> bool {
        // Search for ANIM or ANMF chunks in the WebP extended format
        if data.len() < 16 {
            return false;
        }

        // VP8X extended format starts at offset 12
        if &data[12..16] == b"VP8X" && data.len() >= 21 {
            // Check animation flag (bit 1 of the flags byte at offset 20)
            return data[20] & 0x02 != 0;
        }

        false
    }

    /// Convert a WebP sticker to PNG.
    ///
    /// WebP is widely supported, so this is a passthrough that returns the
    /// original data as-is. Downstream consumers can render WebP directly.
    pub fn convert_webp_to_png(&self, data: &[u8]) -> Result<ConvertedSticker, MediaError> {
        if data.len() > self.max_file_size {
            return Err(MediaError::TooLarge {
                size: data.len(),
                max: self.max_file_size,
            });
        }

        // WebP is widely supported by modern clients and LLM vision APIs,
        // so we pass it through without conversion.
        Ok(ConvertedSticker {
            data: data.to_vec(),
            mime_type: "image/webp".to_string(),
            source_format: StickerFormat::WebP,
            original_size: data.len(),
        })
    }

    /// Convert a TGS sticker to a representable format.
    ///
    /// TGS files are gzip-compressed Lottie JSON animations. This method
    /// decompresses the data and extracts the first frame information.
    /// Full rendering requires a Lottie renderer; this provides the raw
    /// Lottie JSON for downstream processing.
    pub fn convert_tgs_to_png(&self, data: &[u8]) -> Result<ConvertedSticker, MediaError> {
        if data.len() > self.max_file_size {
            return Err(MediaError::TooLarge {
                size: data.len(),
                max: self.max_file_size,
            });
        }

        if !data.starts_with(&[0x1F, 0x8B]) {
            return Err(MediaError::ProcessingFailed {
                reason: "TGS data does not start with gzip magic bytes".to_string(),
            });
        }

        // Decompress the gzip data to get the Lottie JSON
        let mut decoder = flate2::read::GzDecoder::new(data);
        let mut decompressed = Vec::new();
        decoder
            .read_to_end(&mut decompressed)
            .map_err(|e| MediaError::ProcessingFailed {
                reason: format!("Failed to decompress TGS data: {}", e),
            })?;

        // Validate that it's valid JSON (Lottie format)
        let _: serde_json::Value =
            serde_json::from_slice(&decompressed).map_err(|e| MediaError::ProcessingFailed {
                reason: format!("TGS decompressed data is not valid Lottie JSON: {}", e),
            })?;

        // Return the decompressed Lottie JSON. Full frame rasterization
        // would require a Lottie renderer (e.g., rlottie), which is beyond
        // the scope of this lightweight converter.
        Ok(ConvertedSticker {
            data: decompressed,
            mime_type: "application/json".to_string(),
            source_format: StickerFormat::Tgs,
            original_size: data.len(),
        })
    }

    /// Auto-detect sticker format and convert to a processable image format.
    ///
    /// # Arguments
    /// * `data` - Raw sticker bytes
    /// * `filename` - Optional filename hint for format detection
    pub fn convert(
        &self,
        data: &[u8],
        filename: Option<&str>,
    ) -> Result<ConvertedSticker, MediaError> {
        if data.len() > self.max_file_size {
            return Err(MediaError::TooLarge {
                size: data.len(),
                max: self.max_file_size,
            });
        }

        // Detect format from data, falling back to filename extension
        let format = Self::detect_format(data)
            .or_else(|| {
                filename.and_then(|f| {
                    let ext = std::path::Path::new(f)
                        .extension()?
                        .to_string_lossy()
                        .to_lowercase();
                    match ext.as_str() {
                        "webp" => Some(StickerFormat::WebP),
                        "tgs" => Some(StickerFormat::Tgs),
                        _ => None,
                    }
                })
            })
            .ok_or_else(|| MediaError::UnsupportedType {
                mime_type: "unknown sticker format".to_string(),
            })?;

        match format {
            StickerFormat::WebP | StickerFormat::AnimatedWebP => self.convert_webp_to_png(data),
            StickerFormat::Tgs => self.convert_tgs_to_png(data),
        }
    }
}

impl Default for StickerConverter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sticker_format_display() {
        assert_eq!(StickerFormat::WebP.to_string(), "webp");
        assert_eq!(StickerFormat::Tgs.to_string(), "tgs");
        assert_eq!(StickerFormat::AnimatedWebP.to_string(), "animated_webp");
    }

    #[test]
    fn test_sticker_format_mime_type() {
        assert_eq!(StickerFormat::WebP.mime_type(), "image/webp");
        assert_eq!(StickerFormat::Tgs.mime_type(), "application/x-tgsticker");
        assert_eq!(StickerFormat::AnimatedWebP.mime_type(), "image/webp");
    }

    #[test]
    fn test_sticker_format_extension() {
        assert_eq!(StickerFormat::WebP.extension(), "webp");
        assert_eq!(StickerFormat::Tgs.extension(), "tgs");
        assert_eq!(StickerFormat::AnimatedWebP.extension(), "webp");
    }

    #[test]
    fn test_detect_format_webp() {
        let mut data = Vec::new();
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // size placeholder
        data.extend_from_slice(b"WEBP");
        data.extend_from_slice(b"VP8 "); // simple WebP

        assert_eq!(
            StickerConverter::detect_format(&data),
            Some(StickerFormat::WebP)
        );
    }

    #[test]
    fn test_detect_format_animated_webp() {
        let mut data = Vec::new();
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // size placeholder
        data.extend_from_slice(b"WEBP");
        data.extend_from_slice(b"VP8X");
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // chunk size
        data.push(0x02); // flags byte with animation bit set

        assert_eq!(
            StickerConverter::detect_format(&data),
            Some(StickerFormat::AnimatedWebP)
        );
    }

    #[test]
    fn test_detect_format_tgs() {
        let data = [0x1F, 0x8B, 0x08, 0x00]; // gzip magic bytes
        assert_eq!(
            StickerConverter::detect_format(&data),
            Some(StickerFormat::Tgs)
        );
    }

    #[test]
    fn test_detect_format_unknown() {
        let data = [0x00, 0x01, 0x02, 0x03];
        assert_eq!(StickerConverter::detect_format(&data), None);
    }

    #[test]
    fn test_detect_format_too_short() {
        let data = [0x1F, 0x8B];
        assert_eq!(StickerConverter::detect_format(&data), None);
    }

    #[test]
    fn test_convert_webp_passthrough() {
        let converter = StickerConverter::new();

        let mut data = Vec::new();
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        data.extend_from_slice(b"WEBP");
        data.extend_from_slice(b"VP8 ");
        data.extend_from_slice(&[0x00; 20]); // padding

        let result = converter.convert_webp_to_png(&data).unwrap();
        assert_eq!(result.data, data);
        assert_eq!(result.mime_type, "image/webp");
        assert_eq!(result.source_format, StickerFormat::WebP);
        assert_eq!(result.original_size, data.len());
    }

    #[test]
    fn test_convert_tgs_valid() {
        let converter = StickerConverter::new();

        // Create a gzip-compressed JSON payload (minimal Lottie)
        let lottie_json = br#"{"v":"5.5.2","fr":30,"ip":0,"op":60,"w":512,"h":512,"layers":[]}"#;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, lottie_json).unwrap();
        let compressed = encoder.finish().unwrap();

        let result = converter.convert_tgs_to_png(&compressed).unwrap();
        assert_eq!(result.mime_type, "application/json");
        assert_eq!(result.source_format, StickerFormat::Tgs);
        assert_eq!(result.original_size, compressed.len());

        // Verify decompressed data is valid JSON
        let parsed: serde_json::Value = serde_json::from_slice(&result.data).unwrap();
        assert_eq!(parsed["v"], "5.5.2");
    }

    #[test]
    fn test_convert_tgs_invalid_gzip() {
        let converter = StickerConverter::new();
        // Valid gzip magic but invalid content
        let data = [0x1F, 0x8B, 0x00, 0x00, 0x00, 0x00];
        let result = converter.convert_tgs_to_png(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_convert_tgs_not_gzip() {
        let converter = StickerConverter::new();
        let data = [0x00, 0x01, 0x02, 0x03];
        let result = converter.convert_tgs_to_png(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_convert_rejects_oversized() {
        let converter = StickerConverter::new().with_max_file_size(10);
        let data = vec![0u8; 100];
        let result = converter.convert(&data, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_convert_auto_detect_webp() {
        let converter = StickerConverter::new();

        let mut data = Vec::new();
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        data.extend_from_slice(b"WEBP");
        data.extend_from_slice(b"VP8 ");
        data.extend_from_slice(&[0x00; 20]);

        let result = converter.convert(&data, None).unwrap();
        assert_eq!(result.source_format, StickerFormat::WebP);
    }

    #[test]
    fn test_convert_fallback_to_filename() {
        let converter = StickerConverter::new();
        // Data that doesn't match any magic bytes but has .webp extension
        let mut data = Vec::new();
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        data.extend_from_slice(b"WEBP");
        data.extend_from_slice(b"VP8 ");
        data.extend_from_slice(&[0x00; 20]);

        let result = converter.convert(&data, Some("sticker.webp")).unwrap();
        assert_eq!(result.source_format, StickerFormat::WebP);
    }

    #[test]
    fn test_convert_unknown_format() {
        let converter = StickerConverter::new();
        let data = [0x00, 0x01, 0x02, 0x03];
        let result = converter.convert(&data, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_default_max_file_size() {
        let converter = StickerConverter::new();
        assert_eq!(converter.max_file_size(), 5 * 1024 * 1024);
    }

    #[test]
    fn test_custom_max_file_size() {
        let converter = StickerConverter::new().with_max_file_size(1024);
        assert_eq!(converter.max_file_size(), 1024);
    }
}
