//! MIME type detection and media classification.

use std::path::Path;

/// Known media types that IronClaw can handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    /// JPEG, PNG, GIF, WebP, SVG images.
    Image,
    /// MP3, WAV, OGG, M4A audio.
    Audio,
    /// MP4, WebM, AVI video.
    Video,
    /// PDF documents.
    Pdf,
    /// Plain text or code.
    Text,
    /// Telegram/WhatsApp stickers (WebP/TGS).
    Sticker,
    /// Unknown or unsupported type.
    Unknown,
}

impl std::fmt::Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Image => write!(f, "image"),
            Self::Audio => write!(f, "audio"),
            Self::Video => write!(f, "video"),
            Self::Pdf => write!(f, "pdf"),
            Self::Text => write!(f, "text"),
            Self::Sticker => write!(f, "sticker"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Information about a detected media file.
#[derive(Debug, Clone)]
pub struct MediaInfo {
    /// Detected MIME type (e.g., "image/png").
    pub mime_type: String,
    /// Classified media type.
    pub media_type: MediaType,
    /// File extension (without dot).
    pub extension: String,
    /// Whether this type is supported for processing.
    pub supported: bool,
    /// File size in bytes (if known).
    pub size: Option<usize>,
}

/// Detect MIME type from file content (magic bytes) or extension.
pub fn detect_mime_type(data: &[u8], filename: Option<&str>) -> MediaInfo {
    // Magic byte detection
    let (mime, media_type) = detect_from_magic(data)
        .or_else(|| filename.and_then(detect_from_extension))
        .unwrap_or(("application/octet-stream", MediaType::Unknown));

    let extension = filename
        .and_then(|f| Path::new(f).extension())
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| mime_to_extension(mime).to_string());

    let supported = matches!(
        media_type,
        MediaType::Image | MediaType::Audio | MediaType::Pdf | MediaType::Text
    );

    MediaInfo {
        mime_type: mime.to_string(),
        media_type,
        extension,
        supported,
        size: Some(data.len()),
    }
}

/// Detect media type from magic bytes.
fn detect_from_magic(data: &[u8]) -> Option<(&'static str, MediaType)> {
    if data.len() < 4 {
        return None;
    }

    // PNG
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        return Some(("image/png", MediaType::Image));
    }

    // JPEG
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(("image/jpeg", MediaType::Image));
    }

    // GIF
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return Some(("image/gif", MediaType::Image));
    }

    // WebP
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return Some(("image/webp", MediaType::Image));
    }

    // PDF
    if data.starts_with(b"%PDF") {
        return Some(("application/pdf", MediaType::Pdf));
    }

    // MP3
    if data.starts_with(&[0xFF, 0xFB])
        || data.starts_with(&[0xFF, 0xF3])
        || data.starts_with(&[0xFF, 0xF2])
        || data.starts_with(b"ID3")
    {
        return Some(("audio/mpeg", MediaType::Audio));
    }

    // OGG
    if data.starts_with(b"OggS") {
        return Some(("audio/ogg", MediaType::Audio));
    }

    // WAV
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WAVE" {
        return Some(("audio/wav", MediaType::Audio));
    }

    // MP4/M4A
    if data.len() >= 8 && (&data[4..8] == b"ftyp" || &data[4..8] == b"moov") {
        // Check for audio-specific ftyp
        if data.len() >= 12 && &data[8..12] == b"M4A " {
            return Some(("audio/mp4", MediaType::Audio));
        }
        return Some(("video/mp4", MediaType::Video));
    }

    // WebM
    if data.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return Some(("video/webm", MediaType::Video));
    }

    // SVG (text-based)
    if let Ok(text) = std::str::from_utf8(&data[..data.len().min(512)]) {
        let trimmed = text.trim();
        if trimmed.starts_with("<?xml") && trimmed.contains("<svg") || trimmed.starts_with("<svg") {
            return Some(("image/svg+xml", MediaType::Image));
        }
    }

    // TGS (Telegram sticker - gzipped Lottie)
    if data.starts_with(&[0x1F, 0x8B]) {
        // Could be gzip-compressed; check context
        return Some(("application/gzip", MediaType::Sticker));
    }

    None
}

/// Detect media type from file extension.
fn detect_from_extension(filename: &str) -> Option<(&'static str, MediaType)> {
    let ext = Path::new(filename)
        .extension()?
        .to_string_lossy()
        .to_lowercase();

    match ext.as_str() {
        "jpg" | "jpeg" => Some(("image/jpeg", MediaType::Image)),
        "png" => Some(("image/png", MediaType::Image)),
        "gif" => Some(("image/gif", MediaType::Image)),
        "webp" => Some(("image/webp", MediaType::Image)),
        "svg" => Some(("image/svg+xml", MediaType::Image)),
        "bmp" => Some(("image/bmp", MediaType::Image)),
        "tiff" | "tif" => Some(("image/tiff", MediaType::Image)),
        "ico" => Some(("image/x-icon", MediaType::Image)),
        "mp3" => Some(("audio/mpeg", MediaType::Audio)),
        "wav" => Some(("audio/wav", MediaType::Audio)),
        "ogg" | "oga" => Some(("audio/ogg", MediaType::Audio)),
        "m4a" => Some(("audio/mp4", MediaType::Audio)),
        "flac" => Some(("audio/flac", MediaType::Audio)),
        "aac" => Some(("audio/aac", MediaType::Audio)),
        "mp4" | "m4v" => Some(("video/mp4", MediaType::Video)),
        "webm" => Some(("video/webm", MediaType::Video)),
        "avi" => Some(("video/x-msvideo", MediaType::Video)),
        "mov" => Some(("video/quicktime", MediaType::Video)),
        "mkv" => Some(("video/x-matroska", MediaType::Video)),
        "pdf" => Some(("application/pdf", MediaType::Pdf)),
        "txt" | "md" | "rst" | "csv" => Some(("text/plain", MediaType::Text)),
        "json" => Some(("application/json", MediaType::Text)),
        "xml" => Some(("application/xml", MediaType::Text)),
        "html" | "htm" => Some(("text/html", MediaType::Text)),
        "tgs" => Some(("application/x-tgsticker", MediaType::Sticker)),
        _ => None,
    }
}

/// Get the default file extension for a MIME type.
fn mime_to_extension(mime: &str) -> &str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "audio/mpeg" => "mp3",
        "audio/wav" => "wav",
        "audio/ogg" => "ogg",
        "audio/mp4" => "m4a",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "application/pdf" => "pdf",
        "text/plain" => "txt",
        _ => "bin",
    }
}

/// Validate that a media URL is safe to fetch.
///
/// Prevents SSRF by blocking private/internal IP ranges and suspicious schemes.
pub fn validate_media_url(url: &str) -> Result<(), crate::error::MediaError> {
    let parsed = url::Url::parse(url).map_err(|e| crate::error::MediaError::DownloadFailed {
        reason: format!("Invalid URL: {}", e),
    })?;

    // Only allow http(s) schemes
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(crate::error::MediaError::DownloadFailed {
                reason: format!("Unsupported URL scheme: {}", scheme),
            });
        }
    }

    // Block private/internal addresses (SSRF prevention)
    if let Some(host) = parsed.host_str() {
        let lower = host.to_lowercase();
        if lower == "localhost"
            || lower == "127.0.0.1"
            || lower == "::1"
            || lower == "[::1]"
            || lower == "0.0.0.0"
            || lower.starts_with("10.")
            || lower.starts_with("192.168.")
            || lower.starts_with("172.16.")
            || lower.starts_with("172.17.")
            || lower.starts_with("172.18.")
            || lower.starts_with("172.19.")
            || lower.starts_with("172.2")
            || lower.starts_with("172.3")
            || lower.ends_with(".internal")
            || lower.ends_with(".local")
            || lower == "metadata.google.internal"
            || lower == "169.254.169.254"
        {
            return Err(crate::error::MediaError::DownloadFailed {
                reason: format!("URL points to internal/private address: {}", host),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_png() {
        let data = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let info = detect_mime_type(&data, None);
        assert_eq!(info.mime_type, "image/png");
        assert_eq!(info.media_type, MediaType::Image);
        assert!(info.supported);
    }

    #[test]
    fn test_detect_jpeg() {
        let data = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let info = detect_mime_type(&data, None);
        assert_eq!(info.mime_type, "image/jpeg");
        assert_eq!(info.media_type, MediaType::Image);
    }

    #[test]
    fn test_detect_pdf() {
        let data = b"%PDF-1.4 some content";
        let info = detect_mime_type(data, None);
        assert_eq!(info.mime_type, "application/pdf");
        assert_eq!(info.media_type, MediaType::Pdf);
    }

    #[test]
    fn test_detect_from_extension() {
        let data = b"unknown content";
        let info = detect_mime_type(data, Some("document.pdf"));
        assert_eq!(info.mime_type, "application/pdf");
    }

    #[test]
    fn test_detect_mp3() {
        let data = b"ID3\x04\x00\x00\x00\x00\x00";
        let info = detect_mime_type(data, None);
        assert_eq!(info.mime_type, "audio/mpeg");
        assert_eq!(info.media_type, MediaType::Audio);
    }

    #[test]
    fn test_validate_media_url_blocks_private() {
        assert!(validate_media_url("https://127.0.0.1/image.png").is_err());
        assert!(validate_media_url("https://localhost/image.png").is_err());
        assert!(validate_media_url("https://192.168.1.1/image.png").is_err());
        assert!(validate_media_url("https://169.254.169.254/metadata").is_err());
    }

    #[test]
    fn test_validate_media_url_blocks_bad_schemes() {
        assert!(validate_media_url("file:///etc/passwd").is_err());
        assert!(validate_media_url("ftp://example.com/file").is_err());
    }

    #[test]
    fn test_validate_media_url_allows_public() {
        assert!(validate_media_url("https://example.com/image.png").is_ok());
        assert!(validate_media_url("https://cdn.telegram.org/file.jpg").is_ok());
    }
}
