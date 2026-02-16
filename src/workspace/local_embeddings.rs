//! Local embedding provider using bag-of-words with TF-IDF-style weighting.
//!
//! Generates embeddings locally without any external API calls. Uses a
//! hash-based approach to map words to fixed-dimension vectors, then combines
//! them with TF-IDF-inspired weighting for semantic similarity.
//!
//! This is a lightweight fallback when no external embedding API is available.
//! Quality is lower than neural embedding models but sufficient for basic
//! semantic search and runs entirely offline.

use async_trait::async_trait;

use crate::workspace::embeddings::{EmbeddingError, EmbeddingProvider};

/// Local embedding provider using hash-based bag-of-words.
///
/// Each word is hashed to a position in the embedding vector and weighted
/// by inverse document frequency heuristics. No external API calls needed.
pub struct LocalEmbeddings {
    dimension: usize,
    /// Common English stop words that get reduced weight.
    stop_words: std::collections::HashSet<&'static str>,
}

impl LocalEmbeddings {
    /// Create a new local embedding provider with the specified dimension.
    ///
    /// Recommended dimension: 384 or 768 for compatibility with vector stores.
    pub fn new(dimension: usize) -> Self {
        let stop_words: std::collections::HashSet<&str> = [
            "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has",
            "had", "do", "does", "did", "will", "would", "could", "should", "may", "might",
            "shall", "can", "need", "must", "to", "of", "in", "for", "on", "with", "at", "by",
            "from", "as", "into", "through", "during", "before", "after", "above", "below",
            "between", "out", "off", "over", "under", "again", "further", "then", "once", "and",
            "but", "or", "nor", "not", "so", "yet", "both", "either", "neither", "each", "every",
            "all", "any", "few", "more", "most", "other", "some", "such", "no", "only", "own",
            "same", "than", "too", "very", "just", "because", "if", "when", "where", "how", "what",
            "which", "who", "whom", "this", "that", "these", "those", "i", "me", "my", "we", "our",
            "you", "your", "he", "him", "his", "she", "her", "it", "its", "they", "them", "their",
        ]
        .into_iter()
        .collect();

        Self {
            dimension,
            stop_words,
        }
    }

    /// Tokenize text into lowercase words.
    fn tokenize(&self, text: &str) -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric() && c != '\'')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .collect()
    }

    /// Hash a word to a position in the embedding vector.
    fn word_hash(&self, word: &str) -> usize {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        word.hash(&mut hasher);
        hasher.finish() as usize % self.dimension
    }

    /// Hash a word to a sign (+1 or -1) for the embedding vector.
    fn word_sign(&self, word: &str) -> f32 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        word.hash(&mut hasher);
        "sign".hash(&mut hasher);
        if hasher.finish().is_multiple_of(2) {
            1.0
        } else {
            -1.0
        }
    }

    /// Compute weight for a word (stop words get reduced weight).
    fn word_weight(&self, word: &str) -> f32 {
        if self.stop_words.contains(word.to_lowercase().as_str()) {
            0.1
        } else if word.len() <= 2 {
            0.3
        } else {
            1.0
        }
    }

    /// Generate embedding for a single text.
    fn compute_embedding(&self, text: &str) -> Vec<f32> {
        let tokens = self.tokenize(text);
        let mut embedding = vec![0.0f32; self.dimension];

        if tokens.is_empty() {
            return embedding;
        }

        // Count word frequencies for TF weighting
        let mut word_counts: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for token in &tokens {
            *word_counts.entry(token.as_str()).or_insert(0) += 1;
        }

        let total_tokens = tokens.len() as f32;

        // Build embedding using hashing trick with TF-IDF-style weights
        for (word, &count) in &word_counts {
            let tf = count as f32 / total_tokens;
            let idf_heuristic = (1.0 + (10000.0 / (1.0 + word.len() as f32))).ln();
            let weight = tf * idf_heuristic * self.word_weight(word);

            let pos = self.word_hash(word);
            let sign = self.word_sign(word);
            embedding[pos] += sign * weight;

            // Also scatter to nearby positions for richer representation
            let pos2 = (pos + 1) % self.dimension;
            let pos3 = (pos + self.dimension / 2) % self.dimension;
            embedding[pos2] += sign * weight * 0.5;
            embedding[pos3] += sign * weight * 0.3;
        }

        // Also add bigram features for phrase sensitivity
        for window in tokens.windows(2) {
            let bigram = format!("{}_{}", window[0], window[1]);
            let pos = self.word_hash(&bigram);
            let sign = self.word_sign(&bigram);
            let weight = self.word_weight(&window[0]) * self.word_weight(&window[1]) * 0.5;
            embedding[pos] += sign * weight;
        }

        // Normalize to unit length
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for x in &mut embedding {
                *x /= magnitude;
            }
        }

        embedding
    }
}

impl Default for LocalEmbeddings {
    fn default() -> Self {
        Self::new(768)
    }
}

#[async_trait]
impl EmbeddingProvider for LocalEmbeddings {
    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_name(&self) -> &str {
        "local-bow-tfidf"
    }

    fn max_input_length(&self) -> usize {
        100_000
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        if text.len() > self.max_input_length() {
            return Err(EmbeddingError::TextTooLong {
                length: text.len(),
                max: self.max_input_length(),
            });
        }

        Ok(self.compute_embedding(text))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        Ok(texts.iter().map(|t| self.compute_embedding(t)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_embeddings_basic() {
        let provider = LocalEmbeddings::new(128);
        let embedding = provider.embed("hello world").await.unwrap();
        assert_eq!(embedding.len(), 128);
    }

    #[tokio::test]
    async fn test_local_embeddings_normalized() {
        let provider = LocalEmbeddings::new(256);
        let embedding = provider
            .embed("The quick brown fox jumps over the lazy dog")
            .await
            .unwrap();
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (magnitude - 1.0).abs() < 0.01,
            "Expected unit vector, got magnitude {}",
            magnitude
        );
    }

    #[tokio::test]
    async fn test_local_embeddings_deterministic() {
        let provider = LocalEmbeddings::new(128);
        let emb1 = provider.embed("test input").await.unwrap();
        let emb2 = provider.embed("test input").await.unwrap();
        assert_eq!(emb1, emb2);
    }

    #[tokio::test]
    async fn test_local_embeddings_similarity() {
        let provider = LocalEmbeddings::new(256);
        let emb_cat = provider.embed("cat sitting on mat").await.unwrap();
        let emb_dog = provider.embed("dog sitting on mat").await.unwrap();
        let emb_car = provider.embed("automobile engine repair").await.unwrap();

        // Cosine similarity (embeddings are already normalized)
        let sim_cat_dog: f32 = emb_cat.iter().zip(emb_dog.iter()).map(|(a, b)| a * b).sum();
        let sim_cat_car: f32 = emb_cat.iter().zip(emb_car.iter()).map(|(a, b)| a * b).sum();

        // Cat-dog should be more similar than cat-car
        assert!(
            sim_cat_dog > sim_cat_car,
            "Expected cat-dog ({}) > cat-car ({})",
            sim_cat_dog,
            sim_cat_car
        );
    }

    #[tokio::test]
    async fn test_local_embeddings_batch() {
        let provider = LocalEmbeddings::new(128);
        let texts = vec!["hello".to_string(), "world".to_string()];
        let embeddings = provider.embed_batch(&texts).await.unwrap();
        assert_eq!(embeddings.len(), 2);
        assert_ne!(embeddings[0], embeddings[1]);
    }

    #[tokio::test]
    async fn test_local_embeddings_empty() {
        let provider = LocalEmbeddings::new(128);
        let embedding = provider.embed("").await.unwrap();
        assert_eq!(embedding.len(), 128);
        // All zeros for empty input
        assert!(embedding.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_default_dimension() {
        let provider = LocalEmbeddings::default();
        assert_eq!(provider.dimension(), 768);
    }
}
