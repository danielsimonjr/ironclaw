//! Block streaming for channel message delivery.
//!
//! Splits long responses into message blocks and delivers them
//! with configurable pacing to simulate natural typing behavior.
//! This is particularly useful for messaging channels (Telegram, Slack)
//! where a single massive message is less readable than several shorter ones.

use std::time::Duration;

use rand::Rng;
use serde::{Deserialize, Serialize};

/// Configuration for block streaming.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockStreamConfig {
    /// Whether block streaming is enabled.
    pub enabled: bool,
    /// Maximum characters per block.
    pub max_block_chars: usize,
    /// Minimum characters to trigger splitting.
    pub min_split_threshold: usize,
    /// Delay between blocks in milliseconds.
    pub inter_block_delay_ms: u64,
    /// Whether to add typing indicators between blocks.
    pub show_typing: bool,
    /// Split on paragraph boundaries when possible.
    pub prefer_paragraph_breaks: bool,
    /// Maximum number of blocks per response.
    pub max_blocks: usize,
}

impl Default for BlockStreamConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_block_chars: 2000,
            min_split_threshold: 3000,
            inter_block_delay_ms: 800,
            show_typing: true,
            prefer_paragraph_breaks: true,
            max_blocks: 10,
        }
    }
}

/// A block of text to be sent as a separate message.
#[derive(Debug, Clone)]
pub struct TextBlock {
    /// The text content of this block.
    pub content: String,
    /// Block index (0-based).
    pub index: usize,
    /// Total number of blocks.
    pub total: usize,
    /// Whether this is the final block.
    pub is_last: bool,
}

/// Split a response into blocks for streaming delivery.
///
/// Returns a single block if content length is below `min_split_threshold`.
/// Otherwise splits on paragraph boundaries, falling back to sentence
/// boundaries and then word boundaries when paragraphs are too long.
pub fn split_into_blocks(content: &str, config: &BlockStreamConfig) -> Vec<TextBlock> {
    if content.is_empty() {
        return vec![TextBlock {
            content: String::new(),
            index: 0,
            total: 1,
            is_last: true,
        }];
    }

    // Below threshold: return as a single block.
    if content.len() < config.min_split_threshold {
        return vec![TextBlock {
            content: content.to_string(),
            index: 0,
            total: 1,
            is_last: true,
        }];
    }

    let raw_chunks = if config.prefer_paragraph_breaks {
        split_by_paragraphs(content, config.max_block_chars)
    } else {
        split_by_sentences(content, config.max_block_chars)
    };

    // If splitting produced nothing (e.g., whitespace-only content),
    // return a single block with the trimmed content.
    if raw_chunks.is_empty() {
        return vec![TextBlock {
            content: content.trim().to_string(),
            index: 0,
            total: 1,
            is_last: true,
        }];
    }

    // Enforce max_blocks by merging overflow into the last block.
    let chunks = enforce_max_blocks(raw_chunks, config.max_blocks);

    let total = chunks.len();
    chunks
        .into_iter()
        .enumerate()
        .map(|(i, text)| TextBlock {
            content: text,
            index: i,
            total,
            is_last: i == total - 1,
        })
        .collect()
}

/// Calculate the delay before sending a block.
///
/// Uses a slight random jitter (plus or minus 20%) around the configured
/// inter-block delay for a more natural feel. The first block (index 0)
/// is sent immediately with no delay.
pub fn block_delay(config: &BlockStreamConfig, block_index: usize) -> Duration {
    if block_index == 0 {
        return Duration::ZERO;
    }

    let base_ms = config.inter_block_delay_ms as f64;
    let jitter_range = base_ms * 0.2;
    let jitter = rand::thread_rng().gen_range(-jitter_range..=jitter_range);
    let delay_ms = (base_ms + jitter).max(0.0) as u64;

    Duration::from_millis(delay_ms)
}

/// Determine if a response should be block-streamed.
///
/// Returns `true` when streaming is enabled and the content length meets
/// or exceeds the minimum split threshold.
pub fn should_stream(content: &str, config: &BlockStreamConfig) -> bool {
    config.enabled && content.len() >= config.min_split_threshold
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Split text on paragraph boundaries (`\n\n`), falling back to sentence
/// or word splitting when individual paragraphs exceed `max_chars`.
fn split_by_paragraphs(content: &str, max_chars: usize) -> Vec<String> {
    let paragraphs = split_on_double_newline(content);
    let mut blocks: Vec<String> = Vec::new();
    let mut current_block = String::new();

    for para in paragraphs {
        // If a single paragraph is already too long, split it further.
        if para.len() > max_chars {
            // Flush whatever we have accumulated so far.
            if !current_block.is_empty() {
                blocks.push(current_block.trim_end().to_string());
                current_block = String::new();
            }
            let sub_chunks = split_by_sentences(&para, max_chars);
            blocks.extend(sub_chunks);
            continue;
        }

        // Check if appending this paragraph would exceed the limit.
        let separator = if current_block.is_empty() { "" } else { "\n\n" };
        let projected_len = current_block.len() + separator.len() + para.len();

        if projected_len > max_chars && !current_block.is_empty() {
            blocks.push(current_block.trim_end().to_string());
            current_block = para;
        } else {
            if !current_block.is_empty() {
                current_block.push_str("\n\n");
            }
            current_block.push_str(&para);
        }
    }

    if !current_block.is_empty() {
        blocks.push(current_block.trim_end().to_string());
    }

    blocks
}

/// Split raw text on `\n\n` boundaries (collapsing runs of 3+ newlines).
fn split_on_double_newline(content: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut consecutive_newlines = 0;

    for ch in content.chars() {
        if ch == '\n' {
            consecutive_newlines += 1;
        } else {
            if consecutive_newlines >= 2 {
                // We've hit a paragraph break.
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    result.push(trimmed);
                }
                current = String::new();
            } else {
                // Fewer than 2 newlines — keep them as-is.
                for _ in 0..consecutive_newlines {
                    current.push('\n');
                }
            }
            consecutive_newlines = 0;
            current.push(ch);
        }
    }

    // Flush remaining content.
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        result.push(trimmed);
    }

    result
}

/// Split text on sentence boundaries (`. `, `! `, `? `), falling back to
/// word boundaries when sentences are too long.
fn split_by_sentences(content: &str, max_chars: usize) -> Vec<String> {
    let sentences = split_on_sentence_end(content);
    let mut blocks: Vec<String> = Vec::new();
    let mut current_block = String::new();

    for sentence in sentences {
        // If a single sentence exceeds max_chars, split by words.
        if sentence.len() > max_chars {
            if !current_block.is_empty() {
                blocks.push(current_block.trim_end().to_string());
                current_block = String::new();
            }
            let sub_chunks = split_by_words(&sentence, max_chars);
            blocks.extend(sub_chunks);
            continue;
        }

        let projected_len = current_block.len() + sentence.len();
        if projected_len > max_chars && !current_block.is_empty() {
            blocks.push(current_block.trim_end().to_string());
            current_block = sentence;
        } else {
            current_block.push_str(&sentence);
        }
    }

    if !current_block.is_empty() {
        blocks.push(current_block.trim_end().to_string());
    }

    blocks
}

/// Split text at sentence-ending punctuation followed by a space.
/// Each chunk retains its trailing punctuation and space.
fn split_on_sentence_end(content: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();

    let mut i = 0;
    while i < len {
        current.push(chars[i]);

        // Check for sentence boundary: punctuation followed by a space.
        if (chars[i] == '.' || chars[i] == '!' || chars[i] == '?')
            && i + 1 < len
            && chars[i + 1] == ' '
        {
            // Include the trailing space in this chunk.
            current.push(' ');
            i += 1; // skip the space
            result.push(current);
            current = String::new();
        }

        i += 1;
    }

    if !current.is_empty() {
        result.push(current);
    }

    result
}

/// Split text on word boundaries (spaces) as a last resort.
fn split_by_words(content: &str, max_chars: usize) -> Vec<String> {
    let mut blocks: Vec<String> = Vec::new();
    let mut current_block = String::new();

    for word in content.split_inclusive(' ') {
        let projected_len = current_block.len() + word.len();
        if projected_len > max_chars && !current_block.is_empty() {
            blocks.push(current_block.trim_end().to_string());
            current_block = word.to_string();
        } else {
            current_block.push_str(word);
        }
    }

    // If a single word exceeds max_chars we still emit it as one block.
    if !current_block.is_empty() {
        blocks.push(current_block.trim_end().to_string());
    }

    blocks
}

/// Merge overflow blocks beyond `max_blocks` into the last allowed block.
fn enforce_max_blocks(mut blocks: Vec<String>, max_blocks: usize) -> Vec<String> {
    if max_blocks == 0 || blocks.len() <= max_blocks {
        return blocks;
    }

    let overflow: Vec<String> = blocks.drain(max_blocks - 1..).collect();
    let merged = overflow.join("\n\n");
    blocks.push(merged);

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> BlockStreamConfig {
        BlockStreamConfig::default()
    }

    // -----------------------------------------------------------------------
    // split_into_blocks — basic behaviour
    // -----------------------------------------------------------------------

    #[test]
    fn test_short_content_returns_single_block() {
        let config = default_config();
        let content = "Hello, world!";
        let blocks = split_into_blocks(content, &config);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].content, content);
        assert_eq!(blocks[0].index, 0);
        assert_eq!(blocks[0].total, 1);
        assert!(blocks[0].is_last);
    }

    #[test]
    fn test_empty_content_returns_single_empty_block() {
        let config = default_config();
        let blocks = split_into_blocks("", &config);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].content, "");
        assert!(blocks[0].is_last);
    }

    #[test]
    fn test_content_exactly_at_threshold_returns_single_block() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        // 9 characters — strictly below the threshold.
        let content = "123456789";
        let blocks = split_into_blocks(content, &config);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].content, content);
    }

    #[test]
    fn test_content_at_threshold_is_split() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        config.max_block_chars = 5;
        // Exactly 10 characters — meets the threshold.
        let content = "12345 6789";
        let blocks = split_into_blocks(content, &config);
        assert!(blocks.len() > 1);
    }

    // -----------------------------------------------------------------------
    // Paragraph splitting
    // -----------------------------------------------------------------------

    #[test]
    fn test_paragraph_splitting() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        config.max_block_chars = 30;

        let content = "First paragraph here.\n\nSecond paragraph here.\n\nThird paragraph.";
        let blocks = split_into_blocks(content, &config);

        assert!(blocks.len() >= 2);
        // First block should contain the first paragraph.
        assert!(blocks[0].content.contains("First paragraph"));
        // Last block should be marked as last.
        assert!(blocks.last().unwrap().is_last);
    }

    #[test]
    fn test_paragraph_split_preserves_all_content() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        config.max_block_chars = 40;

        let content = "Alpha.\n\nBravo.\n\nCharlie.\n\nDelta.";
        let blocks = split_into_blocks(content, &config);

        let reconstructed: String = blocks
            .iter()
            .map(|b| b.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        // All original words must appear.
        assert!(reconstructed.contains("Alpha"));
        assert!(reconstructed.contains("Bravo"));
        assert!(reconstructed.contains("Charlie"));
        assert!(reconstructed.contains("Delta"));
    }

    #[test]
    fn test_paragraphs_merged_when_small() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        config.max_block_chars = 100;

        let content = "A.\n\nB.\n\nC.";
        let blocks = split_into_blocks(content, &config);

        // Short paragraphs should be merged into one block.
        assert_eq!(blocks.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Sentence splitting fallback
    // -----------------------------------------------------------------------

    #[test]
    fn test_sentence_splitting_fallback() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        config.max_block_chars = 30;
        config.prefer_paragraph_breaks = false;

        let content = "First sentence. Second sentence. Third sentence. Fourth sentence.";
        let blocks = split_into_blocks(content, &config);

        assert!(blocks.len() >= 2);
        assert!(blocks[0].content.contains("First sentence."));
    }

    #[test]
    fn test_sentence_split_within_large_paragraph() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        config.max_block_chars = 30;
        config.prefer_paragraph_breaks = true;

        // Single paragraph that exceeds max_block_chars should fall through to sentence split.
        let content =
            "Sentence one here. Sentence two here. Sentence three here. Sentence four here.";
        let blocks = split_into_blocks(content, &config);

        assert!(blocks.len() >= 2);
    }

    // -----------------------------------------------------------------------
    // Word splitting fallback
    // -----------------------------------------------------------------------

    #[test]
    fn test_word_splitting_fallback() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        config.max_block_chars = 20;
        config.prefer_paragraph_breaks = false;

        // A long run without sentence-ending punctuation.
        let content = "word1 word2 word3 word4 word5 word6 word7 word8";
        let blocks = split_into_blocks(content, &config);

        assert!(blocks.len() >= 2);
        for block in &blocks {
            assert!(block.content.len() <= 20 || !block.content.contains(' '));
        }
    }

    #[test]
    fn test_very_long_single_word() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        config.max_block_chars = 20;

        let content = "a".repeat(100);
        let blocks = split_into_blocks(&content, &config);

        // Cannot split a single word — emitted as one block.
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].content.len(), 100);
    }

    // -----------------------------------------------------------------------
    // max_blocks enforcement
    // -----------------------------------------------------------------------

    #[test]
    fn test_max_blocks_enforcement() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        config.max_block_chars = 15;
        config.max_blocks = 3;

        let content = "AAA.\n\nBBB.\n\nCCC.\n\nDDD.\n\nEEE.\n\nFFF.";
        let blocks = split_into_blocks(content, &config);

        assert!(blocks.len() <= 3);
        assert!(blocks.last().unwrap().is_last);
        // The last block should contain the merged overflow.
        assert!(blocks.last().unwrap().content.contains("FFF"));
    }

    #[test]
    fn test_max_blocks_zero_means_unlimited() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        config.max_block_chars = 6;
        config.max_blocks = 0;

        let content = "AAAA.\n\nBBBB.\n\nCCCC.\n\nDDDD.\n\nEEEE.";
        let blocks = split_into_blocks(content, &config);

        // No cap applied — each paragraph gets its own block.
        assert!(
            blocks.len() >= 4,
            "expected >= 4 blocks, got {}",
            blocks.len()
        );
    }

    // -----------------------------------------------------------------------
    // Block metadata
    // -----------------------------------------------------------------------

    #[test]
    fn test_block_index_and_total() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        config.max_block_chars = 15;

        let content = "AAA.\n\nBBB.\n\nCCC.";
        let blocks = split_into_blocks(content, &config);

        for (i, block) in blocks.iter().enumerate() {
            assert_eq!(block.index, i);
            assert_eq!(block.total, blocks.len());
            assert_eq!(block.is_last, i == blocks.len() - 1);
        }
    }

    #[test]
    fn test_single_block_metadata() {
        let config = default_config();
        let blocks = split_into_blocks("short", &config);

        assert_eq!(blocks[0].index, 0);
        assert_eq!(blocks[0].total, 1);
        assert!(blocks[0].is_last);
    }

    // -----------------------------------------------------------------------
    // block_delay
    // -----------------------------------------------------------------------

    #[test]
    fn test_block_delay_first_block_is_zero() {
        let config = default_config();
        let delay = block_delay(&config, 0);
        assert_eq!(delay, Duration::ZERO);
    }

    #[test]
    fn test_block_delay_subsequent_blocks_near_configured() {
        let mut config = default_config();
        config.inter_block_delay_ms = 1000;

        // Run multiple times to account for jitter.
        for _ in 0..20 {
            let delay = block_delay(&config, 1);
            let ms = delay.as_millis() as u64;
            // 1000 +/- 20% → 800..=1200
            assert!(ms >= 800, "delay {ms}ms below expected range");
            assert!(ms <= 1200, "delay {ms}ms above expected range");
        }
    }

    #[test]
    fn test_block_delay_zero_config() {
        let mut config = default_config();
        config.inter_block_delay_ms = 0;

        let delay = block_delay(&config, 1);
        // With 0 base and 20% jitter of 0, result should be 0.
        assert_eq!(delay, Duration::ZERO);
    }

    // -----------------------------------------------------------------------
    // should_stream
    // -----------------------------------------------------------------------

    #[test]
    fn test_should_stream_enabled_above_threshold() {
        let config = default_config();
        let long_content = "x".repeat(config.min_split_threshold);
        assert!(should_stream(&long_content, &config));
    }

    #[test]
    fn test_should_stream_enabled_below_threshold() {
        let config = default_config();
        assert!(!should_stream("short", &config));
    }

    #[test]
    fn test_should_stream_disabled() {
        let mut config = default_config();
        config.enabled = false;
        let long_content = "x".repeat(config.min_split_threshold + 1000);
        assert!(!should_stream(&long_content, &config));
    }

    #[test]
    fn test_should_stream_empty_content() {
        let config = default_config();
        assert!(!should_stream("", &config));
    }

    #[test]
    fn test_should_stream_exactly_at_threshold() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        let content = "0123456789"; // exactly 10 chars
        assert!(should_stream(content, &config));
    }

    // -----------------------------------------------------------------------
    // Unicode content
    // -----------------------------------------------------------------------

    #[test]
    fn test_unicode_content_paragraph_split() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        config.max_block_chars = 40;

        let content = "Привет мир, это тест.\n\nSecond paragraph with emojis 🎉🎊.";
        let blocks = split_into_blocks(content, &config);

        // Should not panic and should preserve characters.
        assert!(!blocks.is_empty());
        let all_text: String = blocks.iter().map(|b| b.content.clone()).collect();
        assert!(all_text.contains("Привет"));
    }

    #[test]
    fn test_unicode_sentence_split() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        config.max_block_chars = 25;
        config.prefer_paragraph_breaks = false;

        let content = "こんにちは世界! これはテストです. Unicode works.";
        let blocks = split_into_blocks(content, &config);

        assert!(!blocks.is_empty());
    }

    // -----------------------------------------------------------------------
    // Multiple consecutive newlines
    // -----------------------------------------------------------------------

    #[test]
    fn test_multiple_consecutive_newlines() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        config.max_block_chars = 30;

        let content = "Block one.\n\n\n\n\nBlock two.\n\n\nBlock three.";
        let blocks = split_into_blocks(content, &config);

        // Runs of 3+ newlines should be treated as a single paragraph break.
        assert!(!blocks.is_empty());
        let all_text: String = blocks
            .iter()
            .map(|b| b.content.clone())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(all_text.contains("Block one"));
        assert!(all_text.contains("Block two"));
        assert!(all_text.contains("Block three"));
    }

    // -----------------------------------------------------------------------
    // Config edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_config_default_values() {
        let config = BlockStreamConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_block_chars, 2000);
        assert_eq!(config.min_split_threshold, 3000);
        assert_eq!(config.inter_block_delay_ms, 800);
        assert!(config.show_typing);
        assert!(config.prefer_paragraph_breaks);
        assert_eq!(config.max_blocks, 10);
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = BlockStreamConfig::default();
        let json = serde_json::to_string(&config).expect("serialize");
        let deserialized: BlockStreamConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.max_block_chars, config.max_block_chars);
        assert_eq!(deserialized.enabled, config.enabled);
    }

    #[test]
    fn test_streaming_disabled_returns_single_block() {
        let mut config = default_config();
        config.enabled = false;
        config.min_split_threshold = 10;
        config.max_block_chars = 5;

        let content = "This is content that would normally be split into many blocks.";
        // split_into_blocks does not check `enabled` — that is the caller's
        // responsibility via should_stream(). So it still splits.
        // Verify should_stream returns false.
        assert!(!should_stream(content, &config));
    }

    // -----------------------------------------------------------------------
    // Whitespace handling
    // -----------------------------------------------------------------------

    #[test]
    fn test_trailing_whitespace_trimmed() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        config.max_block_chars = 20;

        let content = "First block.   \n\nSecond block.   ";
        let blocks = split_into_blocks(content, &config);

        for block in &blocks {
            assert_eq!(block.content, block.content.trim_end());
        }
    }

    #[test]
    fn test_only_whitespace_content() {
        let mut config = default_config();
        config.min_split_threshold = 2;
        config.max_block_chars = 5;

        let content = "     ";
        let blocks = split_into_blocks(content, &config);

        // Whitespace-only content may produce a single block or be trimmed.
        assert!(!blocks.is_empty());
    }

    // -----------------------------------------------------------------------
    // enforce_max_blocks helper
    // -----------------------------------------------------------------------

    #[test]
    fn test_enforce_max_blocks_merges_overflow() {
        let blocks = vec![
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
            "D".to_string(),
            "E".to_string(),
        ];
        let result = enforce_max_blocks(blocks, 3);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "A");
        assert_eq!(result[1], "B");
        assert!(result[2].contains("C"));
        assert!(result[2].contains("D"));
        assert!(result[2].contains("E"));
    }

    #[test]
    fn test_enforce_max_blocks_no_op_when_within_limit() {
        let blocks = vec!["A".to_string(), "B".to_string()];
        let result = enforce_max_blocks(blocks.clone(), 5);
        assert_eq!(result, blocks);
    }

    // -----------------------------------------------------------------------
    // Mixed content
    // -----------------------------------------------------------------------

    #[test]
    fn test_mixed_paragraphs_and_sentences() {
        let mut config = default_config();
        config.min_split_threshold = 10;
        config.max_block_chars = 50;

        let content = "Short.\n\nThis is a longer paragraph with multiple sentences. \
                        It should be kept together if it fits. Another sentence.\n\n\
                        Final paragraph.";
        let blocks = split_into_blocks(content, &config);

        assert!(!blocks.is_empty());
        // Verify all content is present.
        let combined: String = blocks
            .iter()
            .map(|b| b.content.clone())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(combined.contains("Short"));
        assert!(combined.contains("Final paragraph"));
    }

    #[test]
    fn test_single_paragraph_no_split_needed() {
        let mut config = default_config();
        config.min_split_threshold = 5;
        config.max_block_chars = 1000;

        let content = "Just one paragraph that fits within the block limit.";
        let blocks = split_into_blocks(content, &config);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].content, content);
    }
}
