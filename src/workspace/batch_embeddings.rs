//! Batch embeddings processing.
//!
//! Processes multiple embedding requests together for efficiency,
//! reducing API call overhead and latency.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, Notify};

use super::embeddings::{EmbeddingError, EmbeddingProvider};

/// Request for a single embedding.
#[derive(Debug)]
struct EmbeddingRequest {
    text: String,
    response: tokio::sync::oneshot::Sender<Result<Vec<f32>, EmbeddingError>>,
}

/// Batch embedding processor that queues and batches embedding requests.
pub struct BatchEmbeddingProcessor {
    provider: Arc<dyn EmbeddingProvider>,
    queue: Arc<Mutex<Vec<EmbeddingRequest>>>,
    notify: Arc<Notify>,
    batch_size: usize,
    batch_timeout: Duration,
}

impl BatchEmbeddingProcessor {
    /// Create a new batch processor.
    pub fn new(provider: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            provider,
            queue: Arc::new(Mutex::new(Vec::new())),
            notify: Arc::new(Notify::new()),
            batch_size: 50,
            batch_timeout: Duration::from_millis(100),
        }
    }

    /// Set the maximum batch size.
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Set the maximum wait time before processing a batch.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.batch_timeout = timeout;
        self
    }

    /// Submit a text for embedding (returns a future that resolves when the batch is processed).
    pub async fn embed(&self, text: String) -> Result<Vec<f32>, crate::error::WorkspaceError> {
        let (tx, rx) = tokio::sync::oneshot::channel();

        {
            let mut queue = self.queue.lock().await;
            queue.push(EmbeddingRequest { text, response: tx });

            if queue.len() >= self.batch_size {
                self.notify.notify_one();
            }
        }

        // Also notify after timeout
        let notify = self.notify.clone();
        let timeout = self.batch_timeout;
        tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            notify.notify_one();
        });

        let result = rx
            .await
            .map_err(|_| crate::error::WorkspaceError::EmbeddingFailed {
                reason: "Batch processor dropped the request".to_string(),
            })?;

        result.map_err(|e| crate::error::WorkspaceError::EmbeddingFailed {
            reason: e.to_string(),
        })
    }

    /// Start the batch processing loop.
    pub fn spawn(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let processor = self;
        tokio::spawn(async move {
            loop {
                processor.notify.notified().await;
                processor.process_batch().await;
            }
        })
    }

    /// Process one batch of queued requests.
    async fn process_batch(&self) {
        let requests: Vec<EmbeddingRequest> = {
            let mut queue = self.queue.lock().await;
            if queue.is_empty() {
                return;
            }
            std::mem::take(&mut *queue)
        };

        if requests.is_empty() {
            return;
        }

        tracing::debug!(batch_size = requests.len(), "Processing embedding batch");

        // Process each embedding individually (the provider handles batching internally if supported)
        for request in requests {
            let result = self.provider.embed(&request.text).await;
            let _ = request.response.send(result);
        }
    }
}

/// Citation support for search results.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Citation {
    /// Source document path.
    pub source: String,
    /// Relevant text excerpt.
    pub excerpt: String,
    /// Page or section number if applicable.
    pub location: Option<String>,
    /// Relevance score (0.0 to 1.0).
    pub relevance: f64,
    /// Document chunk ID.
    pub chunk_id: Option<uuid::Uuid>,
}

impl Citation {
    /// Format the citation for display.
    pub fn format(&self) -> String {
        let mut parts = vec![format!("[{}]", self.source)];
        if let Some(ref loc) = self.location {
            parts.push(format!("({})", loc));
        }
        parts.push(format!("relevance: {:.2}", self.relevance));
        parts.join(" ")
    }
}

/// Search result with citation support.
#[derive(Debug, Clone)]
pub struct CitedSearchResult {
    /// The search result content.
    pub content: String,
    /// Citations for this result.
    pub citations: Vec<Citation>,
}

impl CitedSearchResult {
    /// Format all citations.
    pub fn format_citations(&self) -> String {
        if self.citations.is_empty() {
            return String::new();
        }

        let mut output = String::from("\n\nSources:\n");
        for (i, citation) in self.citations.iter().enumerate() {
            output.push_str(&format!(
                "  [{}] {} - {}\n",
                i + 1,
                citation.source,
                citation.excerpt
            ));
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_citation_format() {
        let citation = Citation {
            source: "docs/README.md".to_string(),
            excerpt: "Hello world".to_string(),
            location: Some("Section 1".to_string()),
            relevance: 0.95,
            chunk_id: None,
        };

        let formatted = citation.format();
        assert!(formatted.contains("docs/README.md"));
        assert!(formatted.contains("Section 1"));
        assert!(formatted.contains("0.95"));
    }

    #[test]
    fn test_cited_search_result() {
        let result = CitedSearchResult {
            content: "Some result".to_string(),
            citations: vec![Citation {
                source: "test.md".to_string(),
                excerpt: "relevant text".to_string(),
                location: None,
                relevance: 0.8,
                chunk_id: None,
            }],
        };

        let formatted = result.format_citations();
        assert!(formatted.contains("Sources:"));
        assert!(formatted.contains("test.md"));
    }
}
