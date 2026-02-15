//! PDF text extraction.
//!
//! Extracts text content from PDF files for processing by the LLM.
//! Uses a lightweight parser that handles common PDF structures.

use crate::error::MediaError;

/// A single page of extracted PDF text.
#[derive(Debug, Clone)]
pub struct PdfPage {
    /// Page number (1-indexed).
    pub page_number: usize,
    /// Extracted text content.
    pub text: String,
}

/// PDF text extractor.
pub struct PdfExtractor {
    /// Maximum number of pages to extract.
    max_pages: usize,
    /// Maximum total text size in bytes.
    max_text_size: usize,
}

impl PdfExtractor {
    /// Create a new PDF extractor with default limits.
    pub fn new() -> Self {
        Self {
            max_pages: 100,
            max_text_size: 1_000_000, // 1MB of text
        }
    }

    /// Set maximum pages to extract.
    pub fn with_max_pages(mut self, max: usize) -> Self {
        self.max_pages = max;
        self
    }

    /// Set maximum text output size.
    pub fn with_max_text_size(mut self, max: usize) -> Self {
        self.max_text_size = max;
        self
    }

    /// Extract text from PDF data.
    ///
    /// Performs a basic text stream extraction from the PDF binary format.
    /// For complex PDFs with custom encodings or scanned images, results
    /// may be incomplete.
    pub fn extract(&self, data: &[u8]) -> Result<Vec<PdfPage>, MediaError> {
        if !data.starts_with(b"%PDF") {
            return Err(MediaError::ProcessingFailed {
                reason: "Not a valid PDF file (missing %PDF header)".to_string(),
            });
        }

        let mut pages = Vec::new();
        let mut total_size = 0;

        // Extract text from PDF stream objects
        let text_blocks = extract_text_streams(data);

        for (i, text) in text_blocks.into_iter().enumerate() {
            if i >= self.max_pages {
                break;
            }

            let text_len = text.len();
            if total_size + text_len > self.max_text_size {
                // Truncate the last page to fit within limits
                let remaining = self.max_text_size - total_size;
                let truncated = &text[..remaining.min(text_len)];
                pages.push(PdfPage {
                    page_number: i + 1,
                    text: format!("{}...[truncated]", truncated),
                });
                break;
            }

            total_size += text_len;
            pages.push(PdfPage {
                page_number: i + 1,
                text,
            });
        }

        if pages.is_empty() {
            // Return a single page with a note about extraction
            pages.push(PdfPage {
                page_number: 1,
                text: "[PDF text extraction produced no readable text. The document may contain only images or use unsupported encodings.]".to_string(),
            });
        }

        Ok(pages)
    }

    /// Extract text and return as a single string.
    pub fn extract_all_text(&self, data: &[u8]) -> Result<String, MediaError> {
        let pages = self.extract(data)?;
        let text = pages
            .iter()
            .map(|p| format!("--- Page {} ---\n{}", p.page_number, p.text))
            .collect::<Vec<_>>()
            .join("\n\n");
        Ok(text)
    }
}

impl Default for PdfExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract text from PDF stream objects.
///
/// This is a simplified extractor that handles the most common case:
/// text within BT/ET (Begin Text/End Text) operators in content streams.
fn extract_text_streams(data: &[u8]) -> Vec<String> {
    let content = String::from_utf8_lossy(data);
    let mut pages = Vec::new();
    let mut current_page = String::new();

    // Look for text between BT (Begin Text) and ET (End Text) markers
    let mut in_text_block = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "BT" {
            in_text_block = true;
            continue;
        }

        if trimmed == "ET" {
            in_text_block = false;
            if !current_page.is_empty() {
                current_page.push('\n');
            }
            continue;
        }

        if in_text_block {
            // Extract text from Tj and TJ operators
            if let Some(text) = extract_tj_text(trimmed) {
                current_page.push_str(&text);
            }
        }

        // Page break marker
        if trimmed.contains("/Type /Page") && !current_page.is_empty() {
            pages.push(std::mem::take(&mut current_page));
        }
    }

    if !current_page.is_empty() {
        pages.push(current_page);
    }

    pages
}

/// Extract text from a Tj or TJ PDF operator line.
fn extract_tj_text(line: &str) -> Option<String> {
    // Simple Tj: (Hello World) Tj
    if line.ends_with("Tj") {
        if let Some(start) = line.find('(') {
            if let Some(end) = line.rfind(')') {
                if start < end {
                    return Some(
                        line[start + 1..end]
                            .replace("\\n", "\n")
                            .replace("\\r", "\r")
                            .replace("\\t", "\t")
                            .replace("\\(", "(")
                            .replace("\\)", ")")
                            .replace("\\\\", "\\"),
                    );
                }
            }
        }
    }

    // TJ array: [(Hello) -50 (World)] TJ
    if line.ends_with("TJ") {
        let mut text = String::new();
        let mut in_paren = false;
        let mut chars = line.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '(' && !in_paren {
                in_paren = true;
            } else if ch == ')' && in_paren {
                in_paren = false;
            } else if in_paren {
                if ch == '\\' {
                    if let Some(&next) = chars.peek() {
                        match next {
                            'n' => {
                                text.push('\n');
                                chars.next();
                            }
                            'r' => {
                                text.push('\r');
                                chars.next();
                            }
                            't' => {
                                text.push('\t');
                                chars.next();
                            }
                            '(' | ')' | '\\' => {
                                text.push(next);
                                chars.next();
                            }
                            _ => text.push(ch),
                        }
                    }
                } else {
                    text.push(ch);
                }
            }
        }

        if !text.is_empty() {
            return Some(text);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rejects_non_pdf() {
        let extractor = PdfExtractor::new();
        assert!(extractor.extract(b"not a pdf").is_err());
    }

    #[test]
    fn test_extract_tj_text() {
        assert_eq!(
            extract_tj_text("(Hello World) Tj"),
            Some("Hello World".to_string())
        );
        assert_eq!(extract_tj_text("no text here"), None);
    }

    #[test]
    fn test_extract_tj_array() {
        assert_eq!(
            extract_tj_text("[(Hello) -50 (World)] TJ"),
            Some("HelloWorld".to_string())
        );
    }

    #[test]
    fn test_extract_empty_pdf() {
        let extractor = PdfExtractor::new();
        let data = b"%PDF-1.4\n%%EOF";
        let pages = extractor.extract(data).unwrap();
        assert!(!pages.is_empty());
    }
}
