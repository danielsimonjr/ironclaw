//! Large document processing using Recursive Language Model (RLM) techniques.
//!
//! Implements the approach from "Recursive Language Models" (Zhang, Kraska, Khattab 2025)
//! for processing documents that exceed LLM context windows. Instead of sending the
//! entire document into the LLM context, the document is treated as an external
//! environment that the LLM can programmatically examine through structured operations:
//!
//! - **Slicing**: Read character or line ranges from the document
//! - **Chunk access**: Read pre-computed document chunks
//! - **Search**: Grep for patterns within the document
//! - **Recursive sub-queries**: Dispatch LLM calls on document subsets
//! - **Batched sub-queries**: Parallel LLM calls across multiple chunks
//!
//! The LLM never sees the full document in its context. Instead, it receives metadata
//! about the document dimensions and issues operations to inspect relevant portions,
//! then recursively calls itself (or a smaller model) on those portions to build up
//! an answer.

use std::sync::Arc;
use std::time::Instant;

use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::error::MediaError;
use crate::llm::{ChatMessage, CompletionRequest, CompletionResponse, LlmProvider};
use crate::workspace::{ChunkConfig, chunk_document};

// ---------------------------------------------------------------------------
// Document context: the external "environment" holding the full document
// ---------------------------------------------------------------------------

/// A loaded document context that the LLM can inspect via operations.
///
/// Corresponds to the RLM concept of treating the prompt as an external
/// environment variable. The LLM never receives the raw text directly;
/// instead it issues [`RlmOperation`]s against this context.
#[derive(Debug, Clone)]
pub struct DocumentContext {
    /// The full raw text of the document.
    full_text: String,
    /// Pre-computed lines for efficient line-based access.
    lines: Vec<String>,
    /// Pre-computed overlapping chunks for chunk-based access.
    chunks: Vec<String>,
    /// Metadata about the document dimensions.
    pub metadata: DocumentMetadata,
}

/// Metadata describing the dimensions of a loaded document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Total number of characters.
    pub total_chars: usize,
    /// Total number of lines.
    pub total_lines: usize,
    /// Total number of words (whitespace-delimited).
    pub total_words: usize,
    /// Number of pre-computed chunks.
    pub total_chunks: usize,
    /// Approximate characters per chunk.
    pub chars_per_chunk: usize,
}

impl DocumentContext {
    /// Create a new document context from raw text.
    ///
    /// The text is pre-processed into lines and overlapping chunks using
    /// the provided [`ChunkConfig`].
    pub fn new(text: String, chunk_config: ChunkConfig) -> Self {
        let lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
        let chunks = chunk_document(&text, chunk_config);
        let total_words = text.split_whitespace().count();
        let chars_per_chunk = if chunks.is_empty() {
            text.len()
        } else {
            text.len() / chunks.len()
        };

        let metadata = DocumentMetadata {
            total_chars: text.len(),
            total_lines: lines.len(),
            total_words,
            total_chunks: chunks.len(),
            chars_per_chunk,
        };

        Self {
            full_text: text,
            lines,
            chunks,
            metadata,
        }
    }

    /// Read a character range from the document.
    pub fn read_slice(&self, start: usize, end: usize) -> &str {
        let start = start.min(self.full_text.len());
        let end = end.min(self.full_text.len());
        &self.full_text[start..end]
    }

    /// Read a range of lines (0-indexed, inclusive start, exclusive end).
    pub fn read_lines(&self, start: usize, end: usize) -> Vec<&str> {
        let start = start.min(self.lines.len());
        let end = end.min(self.lines.len());
        self.lines[start..end].iter().map(|s| s.as_str()).collect()
    }

    /// Read a specific chunk by index.
    pub fn read_chunk(&self, index: usize) -> Result<&str, MediaError> {
        self.chunks
            .get(index)
            .map(|s| s.as_str())
            .ok_or(MediaError::ChunkOutOfRange {
                index,
                total: self.chunks.len(),
            })
    }

    /// Read multiple chunks by index.
    pub fn read_chunks(&self, indices: &[usize]) -> Result<Vec<(usize, &str)>, MediaError> {
        let mut result = Vec::with_capacity(indices.len());
        for &idx in indices {
            let chunk = self.read_chunk(idx)?;
            result.push((idx, chunk));
        }
        Ok(result)
    }

    /// Search for a regex pattern, returning matching line numbers and content.
    pub fn search(&self, pattern: &str) -> Result<Vec<SearchMatch>, MediaError> {
        let regex = Regex::new(pattern).map_err(|e| MediaError::RecursiveProcessingFailed {
            reason: format!("Invalid search pattern: {e}"),
        })?;

        let mut matches = Vec::new();
        for (i, line) in self.lines.iter().enumerate() {
            if regex.is_match(line) {
                matches.push(SearchMatch {
                    line_number: i,
                    content: line.clone(),
                });
            }
        }
        Ok(matches)
    }

    /// Get a summary of the first N characters for orientation.
    pub fn preview(&self, max_chars: usize) -> &str {
        let end = max_chars.min(self.full_text.len());
        &self.full_text[..end]
    }
}

/// A search match within the document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    /// Line number (0-indexed).
    pub line_number: usize,
    /// The matching line content.
    pub content: String,
}

// ---------------------------------------------------------------------------
// Operations the LLM can request on the document context
// ---------------------------------------------------------------------------

/// An operation the LLM can request against the document context.
///
/// These map to the REPL operations available in the original RLM paper:
/// reading slices, searching, and launching recursive sub-queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum RlmOperation {
    /// Read a character range: `context[start:end]`
    #[serde(rename = "read_slice")]
    ReadSlice { start: usize, end: usize },

    /// Read a line range (0-indexed, exclusive end).
    #[serde(rename = "read_lines")]
    ReadLines { start: usize, end: usize },

    /// Read a specific chunk by index.
    #[serde(rename = "read_chunk")]
    ReadChunk { index: usize },

    /// Search for a regex pattern across the document.
    #[serde(rename = "search")]
    Search { pattern: String },

    /// Get document metadata (dimensions, chunk count, etc.).
    #[serde(rename = "get_metadata")]
    GetMetadata,

    /// Get a preview of the document (first N characters).
    #[serde(rename = "preview")]
    Preview {
        #[serde(default = "default_preview_chars")]
        max_chars: usize,
    },

    /// Launch a recursive LLM sub-query on specific chunks.
    ///
    /// The sub-query receives the concatenated chunk text as its context
    /// and the given prompt as the question.
    #[serde(rename = "sub_query")]
    SubQuery {
        chunk_indices: Vec<usize>,
        prompt: String,
    },

    /// Launch batched parallel LLM sub-queries, one per chunk group.
    ///
    /// Each entry maps a list of chunk indices to a prompt. All queries
    /// run concurrently against the LLM.
    #[serde(rename = "batch_sub_query")]
    BatchSubQuery { queries: Vec<SubQuerySpec> },

    /// Submit the final answer.
    #[serde(rename = "final_answer")]
    FinalAnswer { answer: String },
}

fn default_preview_chars() -> usize {
    2000
}

/// Specification for a single sub-query in a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubQuerySpec {
    /// Chunk indices to include as context.
    pub chunk_indices: Vec<usize>,
    /// The prompt/question to answer.
    pub prompt: String,
}

/// Result of executing an [`RlmOperation`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    /// The operation that was executed.
    pub op: String,
    /// The output text produced by the operation.
    pub output: String,
    /// Whether this is the final answer.
    pub is_final: bool,
}

// ---------------------------------------------------------------------------
// Processing configuration and result types
// ---------------------------------------------------------------------------

/// Configuration for the RLM large document processor.
#[derive(Debug, Clone)]
pub struct RlmConfig {
    /// Maximum number of iteration rounds the root LLM can take.
    /// Each round involves the LLM requesting operations and receiving results.
    pub max_iterations: u32,
    /// Maximum recursion depth for sub-queries.
    pub max_depth: u32,
    /// Maximum characters to include in a single sub-query context.
    pub max_sub_query_context_chars: usize,
    /// Maximum total characters returned per operation result.
    /// Prevents runaway output from large slices.
    pub max_operation_output_chars: usize,
    /// Chunk configuration for splitting the document.
    pub chunk_config: ChunkConfig,
    /// Temperature for LLM calls (lower = more focused).
    pub temperature: f32,
    /// Maximum tokens for LLM responses.
    pub max_tokens: u32,
}

impl Default for RlmConfig {
    fn default() -> Self {
        Self {
            max_iterations: 15,
            max_depth: 2,
            max_sub_query_context_chars: 200_000,
            max_operation_output_chars: 50_000,
            chunk_config: ChunkConfig::default().with_chunk_size(600),
            temperature: 0.1,
            max_tokens: 4096,
        }
    }
}

impl RlmConfig {
    /// Set maximum iterations.
    pub fn with_max_iterations(mut self, n: u32) -> Self {
        self.max_iterations = n;
        self
    }

    /// Set maximum recursion depth.
    pub fn with_max_depth(mut self, d: u32) -> Self {
        self.max_depth = d;
        self
    }

    /// Set chunk configuration.
    pub fn with_chunk_config(mut self, config: ChunkConfig) -> Self {
        self.chunk_config = config;
        self
    }

    /// Set temperature.
    pub fn with_temperature(mut self, t: f32) -> Self {
        self.temperature = t;
        self
    }

    /// Set max tokens.
    pub fn with_max_tokens(mut self, t: u32) -> Self {
        self.max_tokens = t;
        self
    }
}

/// Statistics about the processing run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingStats {
    /// Number of iteration rounds used.
    pub iterations_used: u32,
    /// Total number of operations executed.
    pub total_operations: u32,
    /// Number of sub-queries dispatched.
    pub sub_queries: u32,
    /// Total input tokens consumed across all LLM calls.
    pub total_input_tokens: u32,
    /// Total output tokens generated across all LLM calls.
    pub total_output_tokens: u32,
    /// Chunks that were accessed during processing.
    pub chunks_accessed: Vec<usize>,
    /// Wall-clock processing time in milliseconds.
    pub elapsed_ms: u64,
}

/// Result of processing a large document.
#[derive(Debug, Clone)]
pub struct ProcessingResult {
    /// The final answer produced by the recursive processing.
    pub answer: String,
    /// Processing statistics.
    pub stats: ProcessingStats,
}

// ---------------------------------------------------------------------------
// System prompt generation
// ---------------------------------------------------------------------------

/// Build the RLM system prompt that instructs the LLM how to interact
/// with the document context.
fn build_system_prompt(metadata: &DocumentMetadata, query: &str) -> String {
    format!(
        r#"You are an AI assistant tasked with answering a query about a large document.
The document is NOT in your context window. Instead, it is stored externally and you can inspect it by issuing operations.

## Document Dimensions

- Total characters: {total_chars}
- Total lines: {total_lines}
- Total words: {total_words}
- Pre-computed chunks: {total_chunks} (approx {chars_per_chunk} chars each)

## Available Operations

You interact with the document by responding with a JSON array of operations. Each operation is an object with an "op" field. After each round, you will receive the results and can issue more operations.

### Reading operations
- `{{"op": "read_slice", "start": 0, "end": 1000}}` — Read characters [start, end)
- `{{"op": "read_lines", "start": 0, "end": 50}}` — Read lines [start, end)
- `{{"op": "read_chunk", "index": 0}}` — Read pre-computed chunk by index (0 to {max_chunk_idx})
- `{{"op": "preview", "max_chars": 2000}}` — Preview the first N characters
- `{{"op": "get_metadata"}}` — Get document metadata

### Search
- `{{"op": "search", "pattern": "regex_pattern"}}` — Search for a regex pattern; returns matching lines

### Recursive LLM sub-queries
- `{{"op": "sub_query", "chunk_indices": [0, 1, 2], "prompt": "What does this section discuss?"}}` — Send specific chunks to a sub-LLM with a question. Batch information: aim for large chunks per call to minimize cost.
- `{{"op": "batch_sub_query", "queries": [{{"chunk_indices": [0,1], "prompt": "..."}}, {{"chunk_indices": [2,3], "prompt": "..."}}]}}` — Run multiple sub-queries in parallel. Use this to process many chunks concurrently.

### Submitting your answer
- `{{"op": "final_answer", "answer": "Your complete answer here"}}` — Submit your final answer when ready.

## Strategy Guidelines

1. **Start by orienting**: Use `preview` or `get_metadata` to understand the document structure.
2. **Search before reading**: Use `search` to find relevant sections rather than reading everything.
3. **Chunk efficiently**: When using sub-queries, batch as much context as reasonable into each call (aim for large chunks). Prefer `batch_sub_query` over multiple individual `sub_query` calls.
4. **Minimize operations**: Each round has cost. Be strategic about what you inspect.
5. **Build incrementally**: Gather partial answers from sub-queries, then synthesize.

## Response Format

Always respond with a JSON array of operations, e.g.:
```json
[{{"op": "preview", "max_chars": 2000}}, {{"op": "get_metadata"}}]
```

When you have enough information, submit your final answer:
```json
[{{"op": "final_answer", "answer": "The document discusses..."}}]
```

## Query

{query}"#,
        total_chars = metadata.total_chars,
        total_lines = metadata.total_lines,
        total_words = metadata.total_words,
        total_chunks = metadata.total_chunks,
        chars_per_chunk = metadata.chars_per_chunk,
        max_chunk_idx = metadata.total_chunks.saturating_sub(1),
        query = query,
    )
}

/// Build the system prompt for a recursive sub-query.
fn build_sub_query_prompt(chunk_text: &str, prompt: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage::system(
            "You are a helpful assistant. Answer the question based ONLY on the provided context. \
             Be concise and precise. If the context does not contain the answer, say so.",
        ),
        ChatMessage::user(format!(
            "Context:\n---\n{chunk_text}\n---\n\nQuestion: {prompt}"
        )),
    ]
}

// ---------------------------------------------------------------------------
// Operation execution
// ---------------------------------------------------------------------------

/// Execute a single operation against the document context.
fn execute_operation(
    ctx: &DocumentContext,
    op: &RlmOperation,
    max_output_chars: usize,
) -> OperationResult {
    match op {
        RlmOperation::ReadSlice { start, end } => {
            let text = ctx.read_slice(*start, *end);
            let output = truncate_output(text, max_output_chars);
            OperationResult {
                op: "read_slice".into(),
                output,
                is_final: false,
            }
        }
        RlmOperation::ReadLines { start, end } => {
            let lines = ctx.read_lines(*start, *end);
            let output = lines
                .iter()
                .enumerate()
                .map(|(i, l)| format!("{}: {l}", start + i))
                .collect::<Vec<_>>()
                .join("\n");
            let output = truncate_output(&output, max_output_chars);
            OperationResult {
                op: "read_lines".into(),
                output,
                is_final: false,
            }
        }
        RlmOperation::ReadChunk { index } => match ctx.read_chunk(*index) {
            Ok(text) => {
                let output = truncate_output(text, max_output_chars);
                OperationResult {
                    op: "read_chunk".into(),
                    output,
                    is_final: false,
                }
            }
            Err(e) => OperationResult {
                op: "read_chunk".into(),
                output: format!("Error: {e}"),
                is_final: false,
            },
        },
        RlmOperation::Search { pattern } => match ctx.search(pattern) {
            Ok(matches) => {
                let output = if matches.is_empty() {
                    "No matches found.".to_string()
                } else {
                    let text = matches
                        .iter()
                        .map(|m| format!("L{}: {}", m.line_number, m.content))
                        .collect::<Vec<_>>()
                        .join("\n");
                    truncate_output(&text, max_output_chars)
                };
                OperationResult {
                    op: "search".into(),
                    output,
                    is_final: false,
                }
            }
            Err(e) => OperationResult {
                op: "search".into(),
                output: format!("Error: {e}"),
                is_final: false,
            },
        },
        RlmOperation::GetMetadata => {
            let output = serde_json::to_string_pretty(&ctx.metadata)
                .unwrap_or_else(|_| format!("{:?}", ctx.metadata));
            OperationResult {
                op: "get_metadata".into(),
                output,
                is_final: false,
            }
        }
        RlmOperation::Preview { max_chars } => {
            let text = ctx.preview(*max_chars);
            let output = truncate_output(text, max_output_chars);
            OperationResult {
                op: "preview".into(),
                output,
                is_final: false,
            }
        }
        RlmOperation::FinalAnswer { answer } => OperationResult {
            op: "final_answer".into(),
            output: answer.clone(),
            is_final: true,
        },
        // Sub-queries are handled asynchronously in the processor loop,
        // not in this synchronous function.
        RlmOperation::SubQuery { .. } | RlmOperation::BatchSubQuery { .. } => OperationResult {
            op: "sub_query".into(),
            output: "[handled async]".into(),
            is_final: false,
        },
    }
}

/// Truncate output to max_chars, adding a marker if truncated.
fn truncate_output(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        text.to_string()
    } else {
        format!(
            "{}...\n[truncated: showing {max_chars} of {} chars]",
            &text[..max_chars],
            text.len()
        )
    }
}

// ---------------------------------------------------------------------------
// Parse operations from LLM response
// ---------------------------------------------------------------------------

/// Parse operations from the LLM response text.
///
/// The LLM is expected to respond with a JSON array of operations,
/// possibly wrapped in a markdown code block.
fn parse_operations(response: &str) -> Result<Vec<RlmOperation>, MediaError> {
    let trimmed = response.trim();

    // Try to find a JSON array in the response, possibly inside a code block.
    let json_str = extract_json_array(trimmed).unwrap_or(trimmed);

    serde_json::from_str(json_str).map_err(|e| MediaError::RecursiveProcessingFailed {
        reason: format!(
            "Failed to parse operations from LLM response: {e}\nResponse was:\n{trimmed}"
        ),
    })
}

/// Extract a JSON array from text that may include markdown code fences.
fn extract_json_array(text: &str) -> Option<&str> {
    // Try ```json ... ``` first
    if let Some(start) = text.find("```json") {
        let after = &text[start + 7..];
        if let Some(end) = after.find("```") {
            return Some(after[..end].trim());
        }
    }
    // Try ``` ... ```
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        if let Some(end) = after.find("```") {
            let inner = after[..end].trim();
            if inner.starts_with('[') {
                return Some(inner);
            }
        }
    }
    // Try finding a raw JSON array
    if let Some(start) = text.find('[') {
        if let Some(end) = text.rfind(']') {
            if start < end {
                return Some(&text[start..=end]);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// The main processor
// ---------------------------------------------------------------------------

/// Large document processor using Recursive Language Model techniques.
///
/// Implements the RLM approach from Zhang, Kraska & Khattab (2025) for
/// processing documents that exceed LLM context windows. The document is
/// treated as an external environment and the LLM iteratively inspects it
/// through structured operations, optionally launching recursive sub-queries
/// on document subsets.
///
/// # Example
///
/// ```rust,ignore
/// use ironclaw::media::large_doc::{LargeDocumentProcessor, RlmConfig};
///
/// let processor = LargeDocumentProcessor::new(llm_provider, RlmConfig::default());
/// let result = processor.process("very long text...", "What is the main topic?").await?;
/// println!("Answer: {}", result.answer);
/// ```
pub struct LargeDocumentProcessor {
    /// The LLM provider used for all completions.
    llm: Arc<dyn LlmProvider>,
    /// Configuration.
    config: RlmConfig,
}

impl LargeDocumentProcessor {
    /// Create a new processor with the given LLM provider and configuration.
    pub fn new(llm: Arc<dyn LlmProvider>, config: RlmConfig) -> Self {
        Self { llm, config }
    }

    /// Process a large document with a query.
    ///
    /// The document text is chunked and made available as an external context.
    /// The LLM iteratively inspects the document through operations until it
    /// produces a final answer or exhausts the iteration budget.
    pub async fn process(
        &self,
        document_text: &str,
        query: &str,
    ) -> Result<ProcessingResult, MediaError> {
        let start_time = Instant::now();

        let ctx = DocumentContext::new(document_text.to_string(), self.config.chunk_config.clone());

        debug!(
            total_chars = ctx.metadata.total_chars,
            total_chunks = ctx.metadata.total_chunks,
            "RLM processing: loaded document context"
        );

        let system_prompt = build_system_prompt(&ctx.metadata, query);
        let mut messages = vec![
            ChatMessage::system(&system_prompt),
            ChatMessage::user(
                "Begin by issuing operations to inspect the document and answer the query.",
            ),
        ];

        let mut stats = ProcessingStats {
            iterations_used: 0,
            total_operations: 0,
            sub_queries: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            chunks_accessed: Vec::new(),
            elapsed_ms: 0,
        };

        for iteration in 0..self.config.max_iterations {
            stats.iterations_used = iteration + 1;

            debug!(iteration, "RLM iteration");

            // Ask the LLM what operations to perform
            let request = CompletionRequest::new(messages.clone())
                .with_temperature(self.config.temperature)
                .with_max_tokens(self.config.max_tokens);

            let response = self.llm.complete(request).await.map_err(|e| {
                MediaError::RecursiveProcessingFailed {
                    reason: format!("LLM call failed at iteration {iteration}: {e}"),
                }
            })?;

            stats.total_input_tokens += response.input_tokens;
            stats.total_output_tokens += response.output_tokens;

            // Parse the operations the LLM requested
            let operations = match parse_operations(&response.content) {
                Ok(ops) => ops,
                Err(e) => {
                    // If parsing fails, give the LLM a second chance with guidance
                    warn!(iteration, error = %e, "Failed to parse operations, sending retry guidance");
                    messages.push(ChatMessage::assistant(&response.content));
                    messages.push(ChatMessage::user(
                        "Your response was not valid JSON. Please respond with a JSON array of operations. \
                         Example: [{\"op\": \"preview\", \"max_chars\": 2000}]",
                    ));
                    continue;
                }
            };

            // Execute operations and collect results
            let mut results = Vec::new();
            let mut final_answer = None;

            for op in &operations {
                stats.total_operations += 1;

                // Track chunk access
                match op {
                    RlmOperation::ReadChunk { index } => {
                        if !stats.chunks_accessed.contains(index) {
                            stats.chunks_accessed.push(*index);
                        }
                    }
                    RlmOperation::SubQuery { chunk_indices, .. } => {
                        for idx in chunk_indices {
                            if !stats.chunks_accessed.contains(idx) {
                                stats.chunks_accessed.push(*idx);
                            }
                        }
                    }
                    RlmOperation::BatchSubQuery { queries } => {
                        for q in queries {
                            for idx in &q.chunk_indices {
                                if !stats.chunks_accessed.contains(idx) {
                                    stats.chunks_accessed.push(*idx);
                                }
                            }
                        }
                    }
                    _ => {}
                }

                match op {
                    RlmOperation::SubQuery {
                        chunk_indices,
                        prompt,
                    } => {
                        let result = self
                            .execute_sub_query(&ctx, chunk_indices, prompt, 0)
                            .await?;
                        stats.sub_queries += 1;
                        stats.total_input_tokens += result.input_tokens;
                        stats.total_output_tokens += result.output_tokens;
                        results.push(OperationResult {
                            op: "sub_query".into(),
                            output: result.content,
                            is_final: false,
                        });
                    }
                    RlmOperation::BatchSubQuery { queries } => {
                        let batch_results =
                            self.execute_batch_sub_queries(&ctx, queries, 0).await?;
                        for result in batch_results {
                            stats.sub_queries += 1;
                            stats.total_input_tokens += result.input_tokens;
                            stats.total_output_tokens += result.output_tokens;
                            results.push(OperationResult {
                                op: "batch_sub_query".into(),
                                output: result.content,
                                is_final: false,
                            });
                        }
                    }
                    RlmOperation::FinalAnswer { answer } => {
                        final_answer = Some(answer.clone());
                        break;
                    }
                    other => {
                        let result =
                            execute_operation(&ctx, other, self.config.max_operation_output_chars);
                        results.push(result);
                    }
                }
            }

            // If we got a final answer, we're done
            if let Some(answer) = final_answer {
                stats.elapsed_ms = start_time.elapsed().as_millis() as u64;
                stats.chunks_accessed.sort_unstable();
                stats.chunks_accessed.dedup();

                debug!(
                    iterations = stats.iterations_used,
                    sub_queries = stats.sub_queries,
                    chunks_accessed = stats.chunks_accessed.len(),
                    "RLM processing complete"
                );

                return Ok(ProcessingResult { answer, stats });
            }

            // Format results and add to conversation for next iteration
            let results_text = results
                .iter()
                .enumerate()
                .map(|(i, r)| format!("### Result {} ({})\n{}", i + 1, r.op, r.output))
                .collect::<Vec<_>>()
                .join("\n\n");

            messages.push(ChatMessage::assistant(&response.content));
            messages.push(ChatMessage::user(format!(
                "Operation results:\n\n{results_text}\n\n\
                 You have used {used} of {max} iterations. \
                 Issue more operations or submit your final answer.",
                used = iteration + 1,
                max = self.config.max_iterations,
            )));
        }

        // If we exhausted iterations without a final answer, force one
        stats.elapsed_ms = start_time.elapsed().as_millis() as u64;
        stats.chunks_accessed.sort_unstable();
        stats.chunks_accessed.dedup();

        warn!(
            iterations = stats.iterations_used,
            "RLM processing exhausted iterations, forcing final answer"
        );

        // Ask the LLM one more time for a final answer
        messages.push(ChatMessage::user(
            "You have exhausted your iteration budget. Based on everything you have gathered so far, \
             provide your best final answer now. Respond with:\n\
             [{\"op\": \"final_answer\", \"answer\": \"Your answer here\"}]",
        ));

        let request = CompletionRequest::new(messages)
            .with_temperature(self.config.temperature)
            .with_max_tokens(self.config.max_tokens);

        let response = self.llm.complete(request).await.map_err(|e| {
            MediaError::RecursiveProcessingFailed {
                reason: format!("LLM call failed during forced final answer: {e}"),
            }
        })?;

        stats.total_input_tokens += response.input_tokens;
        stats.total_output_tokens += response.output_tokens;

        // Try to parse the final answer
        let answer = if let Ok(ops) = parse_operations(&response.content) {
            ops.into_iter()
                .find_map(|op| {
                    if let RlmOperation::FinalAnswer { answer } = op {
                        Some(answer)
                    } else {
                        None
                    }
                })
                .unwrap_or(response.content)
        } else {
            // If parsing fails, use the raw response as the answer
            response.content
        };

        Ok(ProcessingResult { answer, stats })
    }

    /// Execute a sub-query against specific chunks.
    async fn execute_sub_query(
        &self,
        ctx: &DocumentContext,
        chunk_indices: &[usize],
        prompt: &str,
        depth: u32,
    ) -> Result<CompletionResponse, MediaError> {
        if depth >= self.config.max_depth {
            return Err(MediaError::MaxDepthExceeded {
                max_depth: self.config.max_depth,
            });
        }

        // Gather the chunk text
        let chunk_text = self.gather_chunk_text(ctx, chunk_indices)?;

        debug!(
            chunks = ?chunk_indices,
            text_len = chunk_text.len(),
            depth,
            "Executing RLM sub-query"
        );

        let messages = build_sub_query_prompt(&chunk_text, prompt);
        let request = CompletionRequest::new(messages)
            .with_temperature(self.config.temperature)
            .with_max_tokens(self.config.max_tokens);

        self.llm
            .complete(request)
            .await
            .map_err(|e| MediaError::RecursiveProcessingFailed {
                reason: format!("Sub-query LLM call failed: {e}"),
            })
    }

    /// Execute multiple sub-queries concurrently.
    async fn execute_batch_sub_queries(
        &self,
        ctx: &DocumentContext,
        queries: &[SubQuerySpec],
        depth: u32,
    ) -> Result<Vec<CompletionResponse>, MediaError> {
        if depth >= self.config.max_depth {
            return Err(MediaError::MaxDepthExceeded {
                max_depth: self.config.max_depth,
            });
        }

        let mut handles = Vec::with_capacity(queries.len());

        for query in queries {
            let chunk_text = self.gather_chunk_text(ctx, &query.chunk_indices)?;
            let prompt = query.prompt.clone();
            let llm = self.llm.clone();
            let temperature = self.config.temperature;
            let max_tokens = self.config.max_tokens;

            handles.push(tokio::spawn(async move {
                let messages = build_sub_query_prompt(&chunk_text, &prompt);
                let request = CompletionRequest::new(messages)
                    .with_temperature(temperature)
                    .with_max_tokens(max_tokens);

                llm.complete(request)
                    .await
                    .map_err(|e| MediaError::RecursiveProcessingFailed {
                        reason: format!("Batch sub-query LLM call failed: {e}"),
                    })
            }));
        }

        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            let result = handle
                .await
                .map_err(|e| MediaError::RecursiveProcessingFailed {
                    reason: format!("Sub-query task panicked: {e}"),
                })??;
            results.push(result);
        }

        Ok(results)
    }

    /// Gather text from multiple chunks, respecting the max context size.
    fn gather_chunk_text(
        &self,
        ctx: &DocumentContext,
        chunk_indices: &[usize],
    ) -> Result<String, MediaError> {
        let chunks = ctx.read_chunks(chunk_indices)?;
        let mut text = String::new();
        let mut total_len = 0;

        for (idx, chunk) in &chunks {
            let header = format!("\n--- Chunk {idx} ---\n");
            let needed = header.len() + chunk.len();

            if total_len + needed > self.config.max_sub_query_context_chars {
                text.push_str(&format!(
                    "\n[Remaining chunks truncated: context limit reached at {total_len} chars]\n"
                ));
                break;
            }

            text.push_str(&header);
            text.push_str(chunk);
            total_len += needed;
        }

        Ok(text)
    }
}

// ---------------------------------------------------------------------------
// Convenience function for quick processing
// ---------------------------------------------------------------------------

/// Process a large document with a query using default configuration.
///
/// This is a convenience wrapper around [`LargeDocumentProcessor`].
pub async fn process_large_document(
    llm: Arc<dyn LlmProvider>,
    document_text: &str,
    query: &str,
) -> Result<ProcessingResult, MediaError> {
    let processor = LargeDocumentProcessor::new(llm, RlmConfig::default());
    processor.process(document_text, query).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_text() -> String {
        (0..100)
            .map(|i| format!("Line {i}: This is paragraph {i} of the document with some content for testing chunking and search operations."))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_document_context_creation() {
        let text = sample_text();
        let ctx = DocumentContext::new(text.clone(), ChunkConfig::default().with_chunk_size(50));

        assert_eq!(ctx.metadata.total_lines, 100);
        assert_eq!(ctx.metadata.total_chars, text.len());
        assert!(ctx.metadata.total_chunks > 1);
        assert!(ctx.metadata.total_words > 0);
    }

    #[test]
    fn test_read_slice() {
        let text = "Hello, World! This is a test document.".to_string();
        let ctx = DocumentContext::new(text, ChunkConfig::default());

        assert_eq!(ctx.read_slice(0, 5), "Hello");
        assert_eq!(ctx.read_slice(7, 12), "World");
    }

    #[test]
    fn test_read_slice_clamping() {
        let text = "short".to_string();
        let ctx = DocumentContext::new(text, ChunkConfig::default());

        // Out-of-range should clamp
        assert_eq!(ctx.read_slice(0, 1000), "short");
        assert_eq!(ctx.read_slice(100, 200), "");
    }

    #[test]
    fn test_read_lines() {
        let text = "line 0\nline 1\nline 2\nline 3\nline 4".to_string();
        let ctx = DocumentContext::new(text, ChunkConfig::default());

        let lines = ctx.read_lines(1, 3);
        assert_eq!(lines, vec!["line 1", "line 2"]);
    }

    #[test]
    fn test_read_chunk() {
        let text = sample_text();
        let ctx = DocumentContext::new(text, ChunkConfig::default().with_chunk_size(50));

        assert!(ctx.read_chunk(0).is_ok());
        assert!(!ctx.read_chunk(0).unwrap().is_empty());

        // Out-of-range chunk
        let err = ctx.read_chunk(9999);
        assert!(err.is_err());
    }

    #[test]
    fn test_read_chunks() {
        let text = sample_text();
        let ctx = DocumentContext::new(text, ChunkConfig::default().with_chunk_size(50));

        let chunks = ctx.read_chunks(&[0, 1]).unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].0, 0);
        assert_eq!(chunks[1].0, 1);
    }

    #[test]
    fn test_search() {
        let text = "apple banana\ncherry date\napple fig\ngrape".to_string();
        let ctx = DocumentContext::new(text, ChunkConfig::default());

        let matches = ctx.search("apple").unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].line_number, 0);
        assert_eq!(matches[1].line_number, 2);
    }

    #[test]
    fn test_search_regex() {
        let text = "foo123\nbar456\nbaz789\nfoo000".to_string();
        let ctx = DocumentContext::new(text, ChunkConfig::default());

        let matches = ctx.search(r"foo\d+").unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_search_invalid_regex() {
        let text = "test".to_string();
        let ctx = DocumentContext::new(text, ChunkConfig::default());

        assert!(ctx.search("[invalid").is_err());
    }

    #[test]
    fn test_preview() {
        let text = "A".repeat(5000);
        let ctx = DocumentContext::new(text, ChunkConfig::default());

        assert_eq!(ctx.preview(100).len(), 100);
        assert_eq!(ctx.preview(10000).len(), 5000);
    }

    #[test]
    fn test_parse_operations_json() {
        let input = r#"[{"op": "preview", "max_chars": 1000}]"#;
        let ops = parse_operations(input).unwrap();
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], RlmOperation::Preview { max_chars: 1000 }));
    }

    #[test]
    fn test_parse_operations_code_block() {
        let input = "Here are my operations:\n```json\n[{\"op\": \"get_metadata\"}]\n```";
        let ops = parse_operations(input).unwrap();
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], RlmOperation::GetMetadata));
    }

    #[test]
    fn test_parse_operations_final_answer() {
        let input = r#"[{"op": "final_answer", "answer": "The answer is 42"}]"#;
        let ops = parse_operations(input).unwrap();
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            RlmOperation::FinalAnswer { answer } => assert_eq!(answer, "The answer is 42"),
            _ => panic!("Expected FinalAnswer"),
        }
    }

    #[test]
    fn test_parse_operations_sub_query() {
        let input =
            r#"[{"op": "sub_query", "chunk_indices": [0, 1, 2], "prompt": "Summarize this."}]"#;
        let ops = parse_operations(input).unwrap();
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            RlmOperation::SubQuery {
                chunk_indices,
                prompt,
            } => {
                assert_eq!(chunk_indices, &[0, 1, 2]);
                assert_eq!(prompt, "Summarize this.");
            }
            _ => panic!("Expected SubQuery"),
        }
    }

    #[test]
    fn test_parse_operations_batch() {
        let input = r#"[{"op": "batch_sub_query", "queries": [
            {"chunk_indices": [0, 1], "prompt": "Q1"},
            {"chunk_indices": [2, 3], "prompt": "Q2"}
        ]}]"#;
        let ops = parse_operations(input).unwrap();
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            RlmOperation::BatchSubQuery { queries } => {
                assert_eq!(queries.len(), 2);
                assert_eq!(queries[0].chunk_indices, &[0, 1]);
                assert_eq!(queries[1].prompt, "Q2");
            }
            _ => panic!("Expected BatchSubQuery"),
        }
    }

    #[test]
    fn test_parse_operations_multiple() {
        let input =
            r#"[{"op": "preview", "max_chars": 500}, {"op": "search", "pattern": "error"}]"#;
        let ops = parse_operations(input).unwrap();
        assert_eq!(ops.len(), 2);
    }

    #[test]
    fn test_parse_operations_invalid() {
        assert!(parse_operations("not json at all").is_err());
    }

    #[test]
    fn test_execute_operation_metadata() {
        let text = sample_text();
        let ctx = DocumentContext::new(text, ChunkConfig::default().with_chunk_size(50));
        let result = execute_operation(&ctx, &RlmOperation::GetMetadata, 50_000);
        assert!(!result.is_final);
        assert!(result.output.contains("total_chars"));
    }

    #[test]
    fn test_execute_operation_preview() {
        let text = sample_text();
        let ctx = DocumentContext::new(text, ChunkConfig::default());
        let result = execute_operation(&ctx, &RlmOperation::Preview { max_chars: 100 }, 50_000);
        assert!(!result.is_final);
        assert!(result.output.len() <= 100);
    }

    #[test]
    fn test_execute_operation_search() {
        let text = sample_text();
        let ctx = DocumentContext::new(text, ChunkConfig::default());
        let result = execute_operation(
            &ctx,
            &RlmOperation::Search {
                pattern: "Line 42".into(),
            },
            50_000,
        );
        assert!(result.output.contains("Line 42"));
    }

    #[test]
    fn test_execute_operation_final_answer() {
        let ctx = DocumentContext::new("test".into(), ChunkConfig::default());
        let result = execute_operation(
            &ctx,
            &RlmOperation::FinalAnswer {
                answer: "done".into(),
            },
            50_000,
        );
        assert!(result.is_final);
        assert_eq!(result.output, "done");
    }

    #[test]
    fn test_truncate_output() {
        assert_eq!(truncate_output("short", 100), "short");

        let long = "x".repeat(200);
        let truncated = truncate_output(&long, 50);
        assert!(truncated.contains("[truncated"));
        assert!(truncated.starts_with(&"x".repeat(50)));
    }

    #[test]
    fn test_extract_json_array_raw() {
        let text = r#"[{"op": "get_metadata"}]"#;
        assert_eq!(extract_json_array(text), Some(text));
    }

    #[test]
    fn test_extract_json_array_code_block() {
        let text = "Some text\n```json\n[{\"op\": \"get_metadata\"}]\n```\nMore text";
        assert_eq!(
            extract_json_array(text),
            Some("[{\"op\": \"get_metadata\"}]")
        );
    }

    #[test]
    fn test_extract_json_array_with_surrounding_text() {
        let text = "I'll do this: [{\"op\": \"preview\", \"max_chars\": 100}] and then continue";
        let result = extract_json_array(text).unwrap();
        assert!(result.starts_with('['));
        assert!(result.ends_with(']'));
    }

    #[test]
    fn test_system_prompt_contains_metadata() {
        let metadata = DocumentMetadata {
            total_chars: 50000,
            total_lines: 1000,
            total_words: 8000,
            total_chunks: 20,
            chars_per_chunk: 2500,
        };
        let prompt = build_system_prompt(&metadata, "What is the topic?");
        assert!(prompt.contains("50000"));
        assert!(prompt.contains("1000"));
        assert!(prompt.contains("20"));
        assert!(prompt.contains("What is the topic?"));
        assert!(prompt.contains("read_slice"));
        assert!(prompt.contains("sub_query"));
        assert!(prompt.contains("final_answer"));
    }

    #[test]
    fn test_sub_query_prompt_construction() {
        let messages = build_sub_query_prompt("chunk text here", "What is discussed?");
        assert_eq!(messages.len(), 2);
        assert!(messages[1].content.contains("chunk text here"));
        assert!(messages[1].content.contains("What is discussed?"));
    }

    #[test]
    fn test_rlm_config_builder() {
        let config = RlmConfig::default()
            .with_max_iterations(20)
            .with_max_depth(3)
            .with_temperature(0.5)
            .with_max_tokens(8192);

        assert_eq!(config.max_iterations, 20);
        assert_eq!(config.max_depth, 3);
        assert!((config.temperature - 0.5).abs() < f32::EPSILON);
        assert_eq!(config.max_tokens, 8192);
    }

    #[test]
    fn test_document_metadata_serialization() {
        let metadata = DocumentMetadata {
            total_chars: 1000,
            total_lines: 50,
            total_words: 200,
            total_chunks: 5,
            chars_per_chunk: 200,
        };
        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: DocumentMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_chars, 1000);
        assert_eq!(deserialized.total_chunks, 5);
    }

    #[test]
    fn test_operation_serialization_roundtrip() {
        let ops = vec![
            RlmOperation::ReadSlice { start: 0, end: 100 },
            RlmOperation::Search {
                pattern: "test".into(),
            },
            RlmOperation::SubQuery {
                chunk_indices: vec![0, 1],
                prompt: "summarize".into(),
            },
            RlmOperation::FinalAnswer {
                answer: "done".into(),
            },
        ];

        let json = serde_json::to_string(&ops).unwrap();
        let deserialized: Vec<RlmOperation> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.len(), 4);
    }

    #[test]
    fn test_empty_document() {
        let ctx = DocumentContext::new(String::new(), ChunkConfig::default());
        assert_eq!(ctx.metadata.total_chars, 0);
        assert_eq!(ctx.metadata.total_lines, 0);
        assert_eq!(ctx.metadata.total_words, 0);
        assert_eq!(ctx.metadata.total_chunks, 0);
        assert_eq!(ctx.read_slice(0, 100), "");
        assert!(ctx.read_lines(0, 10).is_empty());
    }

    #[test]
    fn test_single_line_document() {
        let ctx = DocumentContext::new("single line".into(), ChunkConfig::default());
        assert_eq!(ctx.metadata.total_lines, 1);
        assert_eq!(ctx.read_lines(0, 1), vec!["single line"]);
    }
}
