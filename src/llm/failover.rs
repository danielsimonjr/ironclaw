//! Multi-provider failover and cooldown management.
//!
//! Provides automatic failover between LLM providers when one is unavailable,
//! rate-limited, or experiencing errors. Includes cooldown periods for failed
//! providers and priority-based selection.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use rust_decimal::Decimal;
use tokio::sync::RwLock;

use super::provider::{
    CompletionRequest, CompletionResponse, LlmProvider, ModelMetadata, ToolCompletionRequest,
    ToolCompletionResponse,
};
use crate::error::LlmError;

/// State tracking for a single provider.
#[derive(Debug)]
struct ProviderState {
    /// Number of consecutive failures.
    consecutive_failures: u32,
    /// When this provider was last used successfully.
    last_success: Option<Instant>,
    /// When this provider entered cooldown.
    cooldown_until: Option<Instant>,
    /// Total requests made.
    total_requests: u64,
    /// Total errors.
    total_errors: u64,
}

impl ProviderState {
    fn new() -> Self {
        Self {
            consecutive_failures: 0,
            last_success: None,
            cooldown_until: None,
            total_requests: 0,
            total_errors: 0,
        }
    }

    fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.last_success = Some(Instant::now());
        self.cooldown_until = None;
        self.total_requests += 1;
    }

    fn record_failure(&mut self, cooldown: Duration) {
        self.consecutive_failures += 1;
        self.total_requests += 1;
        self.total_errors += 1;

        // Exponential backoff: cooldown * 2^(failures-1), capped at 5 minutes
        let multiplier = 2u32.saturating_pow(self.consecutive_failures.saturating_sub(1));
        let actual_cooldown = cooldown
            .checked_mul(multiplier)
            .unwrap_or(Duration::from_secs(300))
            .min(Duration::from_secs(300));

        self.cooldown_until = Some(Instant::now() + actual_cooldown);
    }

    fn is_available(&self) -> bool {
        match self.cooldown_until {
            Some(until) => Instant::now() >= until,
            None => true,
        }
    }
}

/// A named provider entry in the failover chain.
struct ProviderEntry {
    name: String,
    provider: Arc<dyn LlmProvider>,
    priority: u32,
}

/// Failover-capable LLM provider that wraps multiple backends.
pub struct FailoverProvider {
    providers: Vec<ProviderEntry>,
    states: Arc<RwLock<HashMap<String, ProviderState>>>,
    base_cooldown: Duration,
    max_retries: u32,
}

impl FailoverProvider {
    /// Create a new failover provider.
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            states: Arc::new(RwLock::new(HashMap::new())),
            base_cooldown: Duration::from_secs(30),
            max_retries: 3,
        }
    }

    /// Add a provider to the failover chain.
    pub fn add_provider(
        &mut self,
        name: impl Into<String>,
        provider: Arc<dyn LlmProvider>,
        priority: u32,
    ) {
        let name = name.into();
        self.providers.push(ProviderEntry {
            name: name.clone(),
            provider,
            priority,
        });
        // Sort by priority (lower = higher priority)
        self.providers.sort_by_key(|p| p.priority);
    }

    /// Set the base cooldown duration.
    pub fn with_cooldown(mut self, cooldown: Duration) -> Self {
        self.base_cooldown = cooldown;
        self
    }

    /// Set maximum retries before giving up.
    pub fn with_max_retries(mut self, max: u32) -> Self {
        self.max_retries = max;
        self
    }

    /// Get the list of available providers (not in cooldown).
    pub async fn available_providers(&self) -> Vec<String> {
        let states = self.states.read().await;
        self.providers
            .iter()
            .filter(|p| states.get(&p.name).is_none_or(|s| s.is_available()))
            .map(|p| p.name.clone())
            .collect()
    }

    /// Get provider statistics.
    pub async fn provider_stats(&self) -> HashMap<String, (u64, u64, bool)> {
        let states = self.states.read().await;
        self.providers
            .iter()
            .map(|p| {
                let state = states.get(&p.name);
                let (requests, errors) = state
                    .map(|s| (s.total_requests, s.total_errors))
                    .unwrap_or((0, 0));
                let available = state.is_none_or(|s| s.is_available());
                (p.name.clone(), (requests, errors, available))
            })
            .collect()
    }

    /// Select the next available provider.
    #[allow(dead_code)]
    async fn select_provider(&self) -> Option<&ProviderEntry> {
        let states = self.states.read().await;
        self.providers
            .iter()
            .find(|p| states.get(&p.name).is_none_or(|s| s.is_available()))
    }

    /// Record success for a provider.
    async fn record_success(&self, name: &str) {
        let mut states = self.states.write().await;
        states
            .entry(name.to_string())
            .or_insert_with(ProviderState::new)
            .record_success();
    }

    /// Record failure for a provider.
    async fn record_failure(&self, name: &str) {
        let mut states = self.states.write().await;
        states
            .entry(name.to_string())
            .or_insert_with(ProviderState::new)
            .record_failure(self.base_cooldown);
    }
}

#[async_trait]
impl LlmProvider for FailoverProvider {
    fn model_name(&self) -> &str {
        self.providers
            .first()
            .map(|p| p.provider.model_name())
            .unwrap_or("failover")
    }

    fn cost_per_token(&self) -> (Decimal, Decimal) {
        self.providers
            .first()
            .map(|p| p.provider.cost_per_token())
            .unwrap_or((Decimal::ZERO, Decimal::ZERO))
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let mut last_error = None;
        let states = self.states.read().await;
        let available: Vec<_> = self
            .providers
            .iter()
            .filter(|p| states.get(&p.name).is_none_or(|s| s.is_available()))
            .collect();
        drop(states);

        for entry in &available {
            tracing::debug!(provider = entry.name, "Attempting completion");

            match entry.provider.complete(request.clone()).await {
                Ok(response) => {
                    self.record_success(&entry.name).await;
                    return Ok(response);
                }
                Err(e) => {
                    tracing::warn!(
                        provider = entry.name,
                        error = %e,
                        "Provider failed, trying next"
                    );
                    self.record_failure(&entry.name).await;
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(LlmError::RequestFailed {
            provider: "failover".to_string(),
            reason: "No providers available".to_string(),
        }))
    }

    async fn complete_with_tools(
        &self,
        request: ToolCompletionRequest,
    ) -> Result<ToolCompletionResponse, LlmError> {
        let mut last_error = None;
        let states = self.states.read().await;
        let available: Vec<_> = self
            .providers
            .iter()
            .filter(|p| states.get(&p.name).is_none_or(|s| s.is_available()))
            .collect();
        drop(states);

        for entry in &available {
            tracing::debug!(provider = entry.name, "Attempting tool completion");

            match entry.provider.complete_with_tools(request.clone()).await {
                Ok(response) => {
                    self.record_success(&entry.name).await;
                    return Ok(response);
                }
                Err(e) => {
                    tracing::warn!(
                        provider = entry.name,
                        error = %e,
                        "Provider failed, trying next"
                    );
                    self.record_failure(&entry.name).await;
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(LlmError::RequestFailed {
            provider: "failover".to_string(),
            reason: "No providers available".to_string(),
        }))
    }

    async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        let mut all_models = Vec::new();
        for entry in &self.providers {
            if let Ok(models) = entry.provider.list_models().await {
                for model in models {
                    if !all_models.contains(&model) {
                        all_models.push(model);
                    }
                }
            }
        }
        Ok(all_models)
    }

    async fn model_metadata(&self) -> Result<ModelMetadata, LlmError> {
        for entry in &self.providers {
            if let Ok(meta) = entry.provider.model_metadata().await {
                return Ok(meta);
            }
        }
        Ok(ModelMetadata {
            id: "failover".to_string(),
            context_length: None,
        })
    }

    fn active_model_name(&self) -> String {
        self.providers
            .first()
            .map(|p| p.provider.active_model_name())
            .unwrap_or_else(|| "failover".to_string())
    }
}

impl Default for FailoverProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_state_cooldown() {
        let mut state = ProviderState::new();
        assert!(state.is_available());

        state.record_failure(Duration::from_secs(1));
        assert!(!state.is_available());

        // After cooldown expires, should be available
        state.cooldown_until = Some(Instant::now() - Duration::from_secs(1));
        assert!(state.is_available());
    }

    #[test]
    fn test_provider_state_success_clears_cooldown() {
        let mut state = ProviderState::new();
        state.record_failure(Duration::from_secs(60));
        assert!(!state.is_available());

        state.record_success();
        assert!(state.is_available());
        assert_eq!(state.consecutive_failures, 0);
    }
}
