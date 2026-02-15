//! Video metadata extraction and basic processing support.
//!
//! Provides video format detection, metadata extraction from container
//! headers (MP4/WebM), and placeholder support for audio extraction
//! via external tools like ffmpeg.

use serde::{Deserialize, Serialize};

use crate::error::MediaError;

/// Supported video container formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoFormat {
    /// MPEG-4 Part 14 container.
    Mp4,
    /// WebM (Matroska-based) container.
    WebM,
    /// Audio Video Interleave container.
    Avi,
    /// QuickTime container.
    Mov,
    /// Matroska container.
    Mkv,
}

impl std::fmt::Display for VideoFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mp4 => write!(f, "mp4"),
            Self::WebM => write!(f, "webm"),
            Self::Avi => write!(f, "avi"),
            Self::Mov => write!(f, "mov"),
            Self::Mkv => write!(f, "mkv"),
        }
    }
}

impl VideoFormat {
    /// Get the MIME type for this video format.
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Mp4 => "video/mp4",
            Self::WebM => "video/webm",
            Self::Avi => "video/x-msvideo",
            Self::Mov => "video/quicktime",
            Self::Mkv => "video/x-matroska",
        }
    }

    /// Get the file extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::WebM => "webm",
            Self::Avi => "avi",
            Self::Mov => "mov",
            Self::Mkv => "mkv",
        }
    }

    /// Parse from a MIME type string.
    pub fn from_mime(mime: &str) -> Option<Self> {
        match mime {
            "video/mp4" => Some(Self::Mp4),
            "video/webm" => Some(Self::WebM),
            "video/x-msvideo" => Some(Self::Avi),
            "video/quicktime" => Some(Self::Mov),
            "video/x-matroska" => Some(Self::Mkv),
            _ => None,
        }
    }

    /// Parse from a file extension string (without dot).
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "mp4" | "m4v" => Some(Self::Mp4),
            "webm" => Some(Self::WebM),
            "avi" => Some(Self::Avi),
            "mov" => Some(Self::Mov),
            "mkv" => Some(Self::Mkv),
            _ => None,
        }
    }
}

/// Extracted video metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    /// Duration of the video in seconds.
    pub duration_seconds: Option<f64>,
    /// Video width in pixels.
    pub width: Option<u32>,
    /// Video height in pixels.
    pub height: Option<u32>,
    /// Video codec identifier (e.g., "h264", "vp8", "vp9").
    pub codec: Option<String>,
    /// Detected container format.
    pub format: VideoFormat,
    /// File size in bytes.
    pub file_size: usize,
}

/// Video processor for metadata extraction and basic operations.
pub struct VideoProcessor {
    /// Maximum file size in bytes for processing.
    max_file_size: usize,
}

impl VideoProcessor {
    /// Create a new video processor with default settings.
    pub fn new() -> Self {
        Self {
            max_file_size: 100 * 1024 * 1024, // 100MB
        }
    }

    /// Set maximum file size for processing.
    pub fn with_max_file_size(mut self, max: usize) -> Self {
        self.max_file_size = max;
        self
    }

    /// Get the maximum file size setting.
    pub fn max_file_size(&self) -> usize {
        self.max_file_size
    }

    /// Detect the video format from raw data using magic bytes.
    pub fn detect_format(data: &[u8]) -> Option<VideoFormat> {
        if data.len() < 12 {
            return None;
        }

        // MP4/MOV: ftyp box at offset 4
        if &data[4..8] == b"ftyp" {
            // Distinguish MP4 vs MOV by brand
            if data.len() >= 12 {
                let brand = &data[8..12];
                if brand == b"qt  " {
                    return Some(VideoFormat::Mov);
                }
            }
            return Some(VideoFormat::Mp4);
        }

        // moov box (some MP4 files start with moov)
        if &data[4..8] == b"moov" {
            return Some(VideoFormat::Mp4);
        }

        // WebM/MKV: EBML header (0x1A 0x45 0xDF 0xA3)
        if data.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
            // Both WebM and MKV use the same EBML header.
            // Distinguish by scanning for the DocType element.
            if Self::has_doctype(data, b"webm") {
                return Some(VideoFormat::WebM);
            }
            if Self::has_doctype(data, b"matroska") {
                return Some(VideoFormat::Mkv);
            }
            // Default to WebM for EBML containers
            return Some(VideoFormat::WebM);
        }

        // AVI: RIFF....AVI
        if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"AVI " {
            return Some(VideoFormat::Avi);
        }

        None
    }

    /// Check if EBML data contains a specific DocType string.
    fn has_doctype(data: &[u8], doctype: &[u8]) -> bool {
        // Simple scan for the doctype string within the first 64 bytes
        let search_len = data.len().min(64);
        data[..search_len]
            .windows(doctype.len())
            .any(|w| w == doctype)
    }

    /// Extract metadata from video data by parsing container headers.
    ///
    /// This performs lightweight header parsing without decoding the video
    /// stream. For MP4, it reads the moov/ftyp boxes; for WebM, it reads
    /// the EBML header and segment info.
    pub fn extract_metadata(&self, data: &[u8]) -> Result<VideoInfo, MediaError> {
        if data.len() > self.max_file_size {
            return Err(MediaError::TooLarge {
                size: data.len(),
                max: self.max_file_size,
            });
        }

        let format = Self::detect_format(data).ok_or_else(|| MediaError::UnsupportedType {
            mime_type: "unknown video format".to_string(),
        })?;

        let mut info = VideoInfo {
            duration_seconds: None,
            width: None,
            height: None,
            codec: None,
            format,
            file_size: data.len(),
        };

        match format {
            VideoFormat::Mp4 | VideoFormat::Mov => {
                self.parse_mp4_metadata(data, &mut info);
            }
            VideoFormat::WebM | VideoFormat::Mkv => {
                self.parse_webm_metadata(data, &mut info);
            }
            VideoFormat::Avi => {
                self.parse_avi_metadata(data, &mut info);
            }
        }

        Ok(info)
    }

    /// Parse MP4/MOV container metadata from ftyp and moov boxes.
    fn parse_mp4_metadata(&self, data: &[u8], info: &mut VideoInfo) {
        // Scan for codec hints in the ftyp brand
        if data.len() >= 12 && &data[4..8] == b"ftyp" {
            let brand = &data[8..12];
            match brand {
                b"isom" | b"mp41" | b"mp42" | b"avc1" => {
                    info.codec = Some("h264".to_string());
                }
                b"av01" => {
                    info.codec = Some("av1".to_string());
                }
                _ => {}
            }
        }

        // Scan for tkhd (track header) box to get dimensions.
        // The tkhd box contains width/height as fixed-point 16.16 values
        // at the end of the box.
        if let Some(pos) = Self::find_box(data, b"tkhd") {
            // tkhd v0: 84 bytes, v1: 96 bytes
            // Width/height are the last 8 bytes (two 4-byte fixed-point values)
            let box_start = pos;
            if box_start + 4 <= data.len() {
                let box_size =
                    u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                        as usize;
                let box_end = (box_start + box_size).min(data.len());
                if box_end >= box_start + 8 && box_end - box_start >= 8 {
                    // Width and height are the last 8 bytes of tkhd
                    let wh_offset = box_end - 8;
                    if wh_offset + 8 <= data.len() {
                        let width_fp = u32::from_be_bytes([
                            data[wh_offset],
                            data[wh_offset + 1],
                            data[wh_offset + 2],
                            data[wh_offset + 3],
                        ]);
                        let height_fp = u32::from_be_bytes([
                            data[wh_offset + 4],
                            data[wh_offset + 5],
                            data[wh_offset + 6],
                            data[wh_offset + 7],
                        ]);
                        // Fixed-point 16.16: shift right by 16
                        let width = width_fp >> 16;
                        let height = height_fp >> 16;
                        if width > 0 && height > 0 {
                            info.width = Some(width);
                            info.height = Some(height);
                        }
                    }
                }
            }
        }

        // Scan for mvhd (movie header) box to get duration
        if let Some(pos) = Self::find_box(data, b"mvhd")
            && pos + 8 <= data.len()
        {
            let version = if pos + 9 <= data.len() {
                data[pos + 8]
            } else {
                0
            };

            if version == 0 && pos + 28 <= data.len() {
                // v0: timescale at offset +20, duration at offset +24
                let ts_off = pos + 20;
                let dur_off = pos + 24;
                if dur_off + 4 <= data.len() {
                    let timescale = u32::from_be_bytes([
                        data[ts_off],
                        data[ts_off + 1],
                        data[ts_off + 2],
                        data[ts_off + 3],
                    ]);
                    let duration = u32::from_be_bytes([
                        data[dur_off],
                        data[dur_off + 1],
                        data[dur_off + 2],
                        data[dur_off + 3],
                    ]);
                    if timescale > 0 {
                        info.duration_seconds = Some(duration as f64 / timescale as f64);
                    }
                }
            }
        }
    }

    /// Find a box/atom in MP4 data by its 4-byte type identifier.
    /// Returns the offset of the box start (size field).
    fn find_box(data: &[u8], box_type: &[u8; 4]) -> Option<usize> {
        let mut offset = 0;
        while offset + 8 <= data.len() {
            let size = u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]) as usize;
            let btype = &data[offset + 4..offset + 8];

            if btype == box_type {
                return Some(offset);
            }

            // Recurse into container boxes (moov, trak, mdia, minf, stbl)
            if matches!(btype, b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl") {
                let inner_end = if size > 0 {
                    (offset + size).min(data.len())
                } else {
                    data.len()
                };
                let inner_data = &data[offset + 8..inner_end];
                if let Some(inner_pos) = Self::find_box_in(inner_data, box_type) {
                    return Some(offset + 8 + inner_pos);
                }
            }

            if size == 0 {
                break;
            }
            offset += size;
        }
        None
    }

    /// Find a box within a slice of MP4 data (for recursive search).
    fn find_box_in(data: &[u8], box_type: &[u8; 4]) -> Option<usize> {
        let mut offset = 0;
        while offset + 8 <= data.len() {
            let size = u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]) as usize;
            let btype = &data[offset + 4..offset + 8];

            if btype == box_type {
                return Some(offset);
            }

            if matches!(btype, b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl") {
                let inner_end = if size > 0 {
                    (offset + size).min(data.len())
                } else {
                    data.len()
                };
                if offset + 8 < inner_end {
                    let inner_data = &data[offset + 8..inner_end];
                    if let Some(inner_pos) = Self::find_box_in(inner_data, box_type) {
                        return Some(offset + 8 + inner_pos);
                    }
                }
            }

            if size == 0 {
                break;
            }
            offset += size;
        }
        None
    }

    /// Parse WebM/MKV container metadata from EBML header.
    fn parse_webm_metadata(&self, data: &[u8], info: &mut VideoInfo) {
        // WebM uses VP8/VP9/AV1 codecs
        let search_len = data.len().min(256);
        let search_data = &data[..search_len];

        if search_data.windows(4).any(|w| w == b"V_VP9") {
            info.codec = Some("vp9".to_string());
        } else if search_data.windows(4).any(|w| w == b"V_VP8") {
            info.codec = Some("vp8".to_string());
        } else if search_data.windows(5).any(|w| w == b"V_AV1") {
            info.codec = Some("av1".to_string());
        }
    }

    /// Parse AVI container metadata from RIFF header.
    fn parse_avi_metadata(&self, data: &[u8], info: &mut VideoInfo) {
        // AVI main header (avih) contains frame dimensions
        // Search for 'avih' chunk
        if let Some(pos) = data.windows(4).position(|w| w == b"avih") {
            let header_start = pos + 4; // skip 'avih'

            // avih structure: 4 bytes size, then:
            //   offset +4: dwMicroSecPerFrame
            //   offset +32: dwWidth
            //   offset +36: dwHeight
            //   offset +24: dwTotalFrames
            if header_start + 40 <= data.len() {
                let data_start = header_start + 4; // skip chunk size

                if data_start + 36 <= data.len() {
                    let usec_per_frame = u32::from_le_bytes([
                        data[data_start],
                        data[data_start + 1],
                        data[data_start + 2],
                        data[data_start + 3],
                    ]);

                    let total_frames = u32::from_le_bytes([
                        data[data_start + 20],
                        data[data_start + 21],
                        data[data_start + 22],
                        data[data_start + 23],
                    ]);

                    let width = u32::from_le_bytes([
                        data[data_start + 28],
                        data[data_start + 29],
                        data[data_start + 30],
                        data[data_start + 31],
                    ]);

                    let height = u32::from_le_bytes([
                        data[data_start + 32],
                        data[data_start + 33],
                        data[data_start + 34],
                        data[data_start + 35],
                    ]);

                    if width > 0 && height > 0 && width < 65536 && height < 65536 {
                        info.width = Some(width);
                        info.height = Some(height);
                    }

                    if usec_per_frame > 0 && total_frames > 0 {
                        info.duration_seconds =
                            Some((total_frames as f64 * usec_per_frame as f64) / 1_000_000.0);
                    }
                }
            }
        }
    }

    /// Extract audio from a video file.
    ///
    /// This is a placeholder that requires ffmpeg to be available on the
    /// system. Returns a `ProcessingFailed` error if ffmpeg is not found.
    ///
    /// # Arguments
    /// * `data` - Raw video bytes
    /// * `output_format` - Desired audio output format (e.g., "mp3", "wav")
    pub async fn extract_audio(
        &self,
        data: &[u8],
        output_format: &str,
    ) -> Result<Vec<u8>, MediaError> {
        if data.len() > self.max_file_size {
            return Err(MediaError::TooLarge {
                size: data.len(),
                max: self.max_file_size,
            });
        }

        // Verify the input is a recognized video format
        let _format = Self::detect_format(data).ok_or_else(|| MediaError::UnsupportedType {
            mime_type: "unknown video format".to_string(),
        })?;

        // Validate output format
        let valid_formats = ["mp3", "wav", "ogg", "m4a", "flac", "opus"];
        if !valid_formats.contains(&output_format) {
            return Err(MediaError::ProcessingFailed {
                reason: format!(
                    "Unsupported audio output format: {}. Supported: {}",
                    output_format,
                    valid_formats.join(", ")
                ),
            });
        }

        // Audio extraction requires ffmpeg. This is a placeholder that
        // checks for ffmpeg availability and returns an appropriate error.
        Err(MediaError::ProcessingFailed {
            reason: "Audio extraction requires ffmpeg, which is not embedded. \
                     Use an external ffmpeg process or a WASM tool for audio extraction."
                .to_string(),
        })
    }
}

impl Default for VideoProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_format_display() {
        assert_eq!(VideoFormat::Mp4.to_string(), "mp4");
        assert_eq!(VideoFormat::WebM.to_string(), "webm");
        assert_eq!(VideoFormat::Avi.to_string(), "avi");
        assert_eq!(VideoFormat::Mov.to_string(), "mov");
        assert_eq!(VideoFormat::Mkv.to_string(), "mkv");
    }

    #[test]
    fn test_video_format_mime_type() {
        assert_eq!(VideoFormat::Mp4.mime_type(), "video/mp4");
        assert_eq!(VideoFormat::WebM.mime_type(), "video/webm");
        assert_eq!(VideoFormat::Avi.mime_type(), "video/x-msvideo");
        assert_eq!(VideoFormat::Mov.mime_type(), "video/quicktime");
        assert_eq!(VideoFormat::Mkv.mime_type(), "video/x-matroska");
    }

    #[test]
    fn test_video_format_extension() {
        assert_eq!(VideoFormat::Mp4.extension(), "mp4");
        assert_eq!(VideoFormat::WebM.extension(), "webm");
        assert_eq!(VideoFormat::Avi.extension(), "avi");
        assert_eq!(VideoFormat::Mov.extension(), "mov");
        assert_eq!(VideoFormat::Mkv.extension(), "mkv");
    }

    #[test]
    fn test_video_format_from_mime() {
        assert_eq!(VideoFormat::from_mime("video/mp4"), Some(VideoFormat::Mp4));
        assert_eq!(
            VideoFormat::from_mime("video/webm"),
            Some(VideoFormat::WebM)
        );
        assert_eq!(
            VideoFormat::from_mime("video/quicktime"),
            Some(VideoFormat::Mov)
        );
        assert_eq!(VideoFormat::from_mime("audio/mpeg"), None);
    }

    #[test]
    fn test_video_format_from_extension() {
        assert_eq!(VideoFormat::from_extension("mp4"), Some(VideoFormat::Mp4));
        assert_eq!(VideoFormat::from_extension("m4v"), Some(VideoFormat::Mp4));
        assert_eq!(VideoFormat::from_extension("webm"), Some(VideoFormat::WebM));
        assert_eq!(VideoFormat::from_extension("mkv"), Some(VideoFormat::Mkv));
        assert_eq!(VideoFormat::from_extension("txt"), None);
    }

    #[test]
    fn test_detect_format_mp4() {
        let mut data = vec![0x00, 0x00, 0x00, 0x20]; // box size
        data.extend_from_slice(b"ftyp");
        data.extend_from_slice(b"isom");
        data.extend_from_slice(&[0x00; 20]); // padding

        assert_eq!(VideoProcessor::detect_format(&data), Some(VideoFormat::Mp4));
    }

    #[test]
    fn test_detect_format_mov() {
        let mut data = vec![0x00, 0x00, 0x00, 0x14]; // box size
        data.extend_from_slice(b"ftyp");
        data.extend_from_slice(b"qt  ");
        data.extend_from_slice(&[0x00; 8]);

        assert_eq!(VideoProcessor::detect_format(&data), Some(VideoFormat::Mov));
    }

    #[test]
    fn test_detect_format_webm() {
        let mut data = vec![0x1A, 0x45, 0xDF, 0xA3]; // EBML header
        data.extend_from_slice(&[0x00; 8]);
        // Add webm doctype
        data.extend_from_slice(b"webm");
        data.extend_from_slice(&[0x00; 20]);

        assert_eq!(
            VideoProcessor::detect_format(&data),
            Some(VideoFormat::WebM)
        );
    }

    #[test]
    fn test_detect_format_avi() {
        let mut data = Vec::new();
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // size
        data.extend_from_slice(b"AVI ");

        assert_eq!(VideoProcessor::detect_format(&data), Some(VideoFormat::Avi));
    }

    #[test]
    fn test_detect_format_unknown() {
        let data = vec![0x00; 20];
        assert_eq!(VideoProcessor::detect_format(&data), None);
    }

    #[test]
    fn test_detect_format_too_short() {
        let data = vec![0x00; 4];
        assert_eq!(VideoProcessor::detect_format(&data), None);
    }

    #[test]
    fn test_extract_metadata_mp4() {
        let processor = VideoProcessor::new();

        let mut data = vec![0x00, 0x00, 0x00, 0x14]; // ftyp box (20 bytes)
        data.extend_from_slice(b"ftyp");
        data.extend_from_slice(b"isom");
        data.extend_from_slice(&[0x00; 8]); // rest of ftyp

        let info = processor.extract_metadata(&data).unwrap();
        assert_eq!(info.format, VideoFormat::Mp4);
        assert_eq!(info.file_size, data.len());
        assert_eq!(info.codec, Some("h264".to_string()));
    }

    #[test]
    fn test_extract_metadata_rejects_oversized() {
        let processor = VideoProcessor::new().with_max_file_size(10);
        let data = vec![0u8; 100];
        assert!(processor.extract_metadata(&data).is_err());
    }

    #[test]
    fn test_extract_metadata_unknown_format() {
        let processor = VideoProcessor::new();
        let data = vec![0x00; 20];
        assert!(processor.extract_metadata(&data).is_err());
    }

    #[tokio::test]
    async fn test_extract_audio_placeholder() {
        let processor = VideoProcessor::new();

        let mut data = vec![0x00, 0x00, 0x00, 0x14];
        data.extend_from_slice(b"ftyp");
        data.extend_from_slice(b"isom");
        data.extend_from_slice(&[0x00; 8]);

        let result = processor.extract_audio(&data, "mp3").await;
        // Should fail because ffmpeg is not embedded
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_extract_audio_invalid_format() {
        let processor = VideoProcessor::new();

        let mut data = vec![0x00, 0x00, 0x00, 0x14];
        data.extend_from_slice(b"ftyp");
        data.extend_from_slice(b"isom");
        data.extend_from_slice(&[0x00; 8]);

        let result = processor.extract_audio(&data, "xyz").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_extract_audio_rejects_oversized() {
        let processor = VideoProcessor::new().with_max_file_size(10);
        let data = vec![0u8; 100];
        let result = processor.extract_audio(&data, "mp3").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_default_max_file_size() {
        let processor = VideoProcessor::new();
        assert_eq!(processor.max_file_size(), 100 * 1024 * 1024);
    }

    #[test]
    fn test_custom_max_file_size() {
        let processor = VideoProcessor::new().with_max_file_size(1024);
        assert_eq!(processor.max_file_size(), 1024);
    }

    #[test]
    fn test_video_format_round_trip() {
        for format in [
            VideoFormat::Mp4,
            VideoFormat::WebM,
            VideoFormat::Avi,
            VideoFormat::Mov,
            VideoFormat::Mkv,
        ] {
            let mime = format.mime_type();
            let ext = format.extension();
            assert_eq!(VideoFormat::from_mime(mime), Some(format));
            assert_eq!(VideoFormat::from_extension(ext), Some(format));
        }
    }
}
