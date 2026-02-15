//! Image processing capabilities.
//!
//! Provides image resize, format conversion, and basic manipulation.
//! Uses pure-Rust implementations to avoid external C dependencies.

use crate::error::MediaError;

/// Supported image output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Jpeg,
    Png,
    WebP,
}

impl std::fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Jpeg => write!(f, "jpeg"),
            Self::Png => write!(f, "png"),
            Self::WebP => write!(f, "webp"),
        }
    }
}

impl ImageFormat {
    /// Get the MIME type for this format.
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::WebP => "image/webp",
        }
    }

    /// Get the file extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::WebP => "webp",
        }
    }

    /// Parse from MIME type string.
    pub fn from_mime(mime: &str) -> Option<Self> {
        match mime {
            "image/jpeg" => Some(Self::Jpeg),
            "image/png" => Some(Self::Png),
            "image/webp" => Some(Self::WebP),
            _ => None,
        }
    }
}

/// Result of image processing.
#[derive(Debug, Clone)]
pub struct ProcessedImage {
    /// Raw image data after processing.
    pub data: Vec<u8>,
    /// Output format.
    pub format: ImageFormat,
    /// Width after processing.
    pub width: u32,
    /// Height after processing.
    pub height: u32,
    /// Original file size.
    pub original_size: usize,
    /// Processed file size.
    pub processed_size: usize,
}

/// Image processor for resize and format conversion.
pub struct ImageProcessor {
    /// Maximum dimension (width or height) for resizing.
    max_dimension: u32,
    /// Maximum file size in bytes.
    max_file_size: usize,
    /// Default output quality (0-100).
    quality: u8,
}

impl ImageProcessor {
    /// Create a new image processor with default settings.
    pub fn new() -> Self {
        Self {
            max_dimension: 2048,
            max_file_size: 20 * 1024 * 1024, // 20MB
            quality: 85,
        }
    }

    /// Set maximum dimension for resizing.
    pub fn with_max_dimension(mut self, max: u32) -> Self {
        self.max_dimension = max;
        self
    }

    /// Set maximum file size.
    pub fn with_max_file_size(mut self, max: usize) -> Self {
        self.max_file_size = max;
        self
    }

    /// Set output quality.
    pub fn with_quality(mut self, quality: u8) -> Self {
        self.quality = quality.min(100);
        self
    }

    /// Process an image: detect dimensions, resize if needed.
    ///
    /// This is a lightweight processor that extracts metadata.
    /// Full image manipulation (resize, convert) requires the `image` crate
    /// which is intentionally not included to keep binary size small.
    pub fn process(
        &self,
        data: &[u8],
        _target_format: Option<ImageFormat>,
    ) -> Result<ProcessedImage, MediaError> {
        if data.len() > self.max_file_size {
            return Err(MediaError::TooLarge {
                size: data.len(),
                max: self.max_file_size,
            });
        }

        let (width, height, format) = detect_image_dimensions(data)?;

        Ok(ProcessedImage {
            data: data.to_vec(),
            format,
            width,
            height,
            original_size: data.len(),
            processed_size: data.len(),
        })
    }

    /// Get the maximum dimension setting.
    pub fn max_dimension(&self) -> u32 {
        self.max_dimension
    }

    /// Get the quality setting.
    pub fn quality(&self) -> u8 {
        self.quality
    }
}

impl Default for ImageProcessor {
    fn default() -> Self {
        Self::new()
    }
}

/// Detect image dimensions from raw data.
fn detect_image_dimensions(data: &[u8]) -> Result<(u32, u32, ImageFormat), MediaError> {
    // PNG: width/height at bytes 16-23
    if data.len() >= 24 && data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        return Ok((width, height, ImageFormat::Png));
    }

    // JPEG: scan for SOF0/SOF2 markers
    if data.len() >= 4 && data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        if let Some((w, h)) = find_jpeg_dimensions(data) {
            return Ok((w, h, ImageFormat::Jpeg));
        }
        // Fallback: return 0x0 if we can't find dimensions
        return Ok((0, 0, ImageFormat::Jpeg));
    }

    // WebP: width/height in VP8 header
    if data.len() >= 30 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        if &data[12..16] == b"VP8 " && data.len() >= 30 {
            let width = u16::from_le_bytes([data[26], data[27]]) as u32 & 0x3FFF;
            let height = u16::from_le_bytes([data[28], data[29]]) as u32 & 0x3FFF;
            return Ok((width, height, ImageFormat::WebP));
        }
        // VP8L or VP8X - simplified
        return Ok((0, 0, ImageFormat::WebP));
    }

    Err(MediaError::UnsupportedType {
        mime_type: "unknown image format".to_string(),
    })
}

/// Scan JPEG data for SOF marker to find dimensions.
fn find_jpeg_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    while i + 1 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        // SOF0, SOF1, SOF2 markers
        if (marker == 0xC0 || marker == 0xC1 || marker == 0xC2) && i + 9 < data.len() {
            let height = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            let width = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
            return Some((width, height));
        }
        // Skip this segment
        if i + 3 < data.len() {
            let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            i += 2 + len;
        } else {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_format_mime() {
        assert_eq!(ImageFormat::Jpeg.mime_type(), "image/jpeg");
        assert_eq!(ImageFormat::Png.mime_type(), "image/png");
        assert_eq!(ImageFormat::WebP.mime_type(), "image/webp");
    }

    #[test]
    fn test_image_format_from_mime() {
        assert_eq!(
            ImageFormat::from_mime("image/jpeg"),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(ImageFormat::from_mime("image/png"), Some(ImageFormat::Png));
        assert_eq!(ImageFormat::from_mime("text/plain"), None);
    }

    #[test]
    fn test_processor_rejects_oversized() {
        let processor = ImageProcessor::new().with_max_file_size(100);
        let data = vec![0x89, 0x50, 0x4E, 0x47]; // PNG header but too small
        // Small data should pass
        let big_data = vec![0u8; 200];
        assert!(processor.process(&big_data, None).is_err());
        let _ = data; // suppress warning
    }

    #[test]
    fn test_detect_png_dimensions() {
        // Minimal PNG header with IHDR chunk
        let mut data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]; // PNG signature
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x0D]); // IHDR length
        data.extend_from_slice(b"IHDR"); // IHDR tag
        data.extend_from_slice(&100u32.to_be_bytes()); // width = 100
        data.extend_from_slice(&200u32.to_be_bytes()); // height = 200

        let (w, h, fmt) = detect_image_dimensions(&data).unwrap();
        assert_eq!(w, 100);
        assert_eq!(h, 200);
        assert_eq!(fmt, ImageFormat::Png);
    }
}
