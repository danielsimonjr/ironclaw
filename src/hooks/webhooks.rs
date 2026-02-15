//! Outbound webhook support.
//!
//! Sends notifications to external HTTP endpoints when events occur.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Configuration for an outbound webhook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundWebhook {
    /// Unique name for this webhook.
    pub name: String,
    /// Target URL.
    pub url: String,
    /// Events that trigger this webhook.
    pub events: Vec<String>,
    /// Secret for HMAC signature verification.
    pub secret: Option<String>,
    /// Whether the webhook is enabled.
    pub enabled: bool,
    /// Custom headers to include.
    pub headers: HashMap<String, String>,
    /// Maximum retry attempts.
    pub max_retries: u32,
    /// Timeout in milliseconds.
    pub timeout_ms: u64,
}

impl Default for OutboundWebhook {
    fn default() -> Self {
        Self {
            name: String::new(),
            url: String::new(),
            events: Vec::new(),
            secret: None,
            enabled: true,
            headers: HashMap::new(),
            max_retries: 3,
            timeout_ms: 10000,
        }
    }
}

/// Payload sent to outbound webhooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    /// Event type.
    pub event: String,
    /// Timestamp.
    pub timestamp: String,
    /// Event data.
    pub data: serde_json::Value,
}

/// Manager for outbound webhooks.
pub struct WebhookManager {
    webhooks: Arc<RwLock<Vec<OutboundWebhook>>>,
    client: reqwest::Client,
}

impl WebhookManager {
    /// Create a new webhook manager.
    pub fn new() -> Self {
        Self {
            webhooks: Arc::new(RwLock::new(Vec::new())),
            client: reqwest::Client::new(),
        }
    }

    /// Register a webhook.
    pub async fn register(&self, webhook: OutboundWebhook) {
        self.webhooks.write().await.push(webhook);
    }

    /// Remove a webhook by name.
    pub async fn remove(&self, name: &str) -> bool {
        let mut webhooks = self.webhooks.write().await;
        let before = webhooks.len();
        webhooks.retain(|w| w.name != name);
        webhooks.len() < before
    }

    /// List all registered webhooks.
    pub async fn list(&self) -> Vec<OutboundWebhook> {
        self.webhooks.read().await.clone()
    }

    /// Fire webhooks for a given event.
    pub async fn fire(&self, event: &str, data: serde_json::Value) {
        let webhooks = self.webhooks.read().await;
        let matching: Vec<_> = webhooks
            .iter()
            .filter(|w| w.enabled && w.events.iter().any(|e| e == event || e == "*"))
            .cloned()
            .collect();
        drop(webhooks);

        for webhook in matching {
            let payload = WebhookPayload {
                event: event.to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                data: data.clone(),
            };

            let client = self.client.clone();
            tokio::spawn(async move {
                if let Err(e) = send_webhook(&client, &webhook, &payload).await {
                    tracing::warn!(
                        webhook = webhook.name,
                        error = %e,
                        "Outbound webhook failed"
                    );
                }
            });
        }
    }
}

impl Default for WebhookManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Send a webhook with retries.
async fn send_webhook(
    client: &reqwest::Client,
    webhook: &OutboundWebhook,
    payload: &WebhookPayload,
) -> Result<(), String> {
    let body = serde_json::to_string(payload).map_err(|e| e.to_string())?;

    for attempt in 0..=webhook.max_retries {
        let mut request = client
            .post(&webhook.url)
            .header("Content-Type", "application/json")
            .header("X-Webhook-Event", &payload.event)
            .timeout(std::time::Duration::from_millis(webhook.timeout_ms))
            .body(body.clone());

        // Add HMAC signature if secret is set
        if let Some(ref secret) = webhook.secret {
            let signature = compute_hmac(secret, &body);
            request = request.header("X-Webhook-Signature", signature);
        }

        // Add custom headers
        for (key, value) in &webhook.headers {
            request = request.header(key, value);
        }

        match request.send().await {
            Ok(response) if response.status().is_success() => {
                return Ok(());
            }
            Ok(response) => {
                let status = response.status();
                if attempt < webhook.max_retries {
                    let backoff = std::time::Duration::from_millis(100 * 2u64.pow(attempt));
                    tokio::time::sleep(backoff).await;
                    continue;
                }
                return Err(format!("HTTP {}", status));
            }
            Err(e) => {
                if attempt < webhook.max_retries {
                    let backoff = std::time::Duration::from_millis(100 * 2u64.pow(attempt));
                    tokio::time::sleep(backoff).await;
                    continue;
                }
                return Err(e.to_string());
            }
        }
    }

    Err("Max retries exceeded".to_string())
}

/// Compute HMAC-SHA256 signature for webhook verification.
fn compute_hmac(secret: &str, payload: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC key can be any length");
    mac.update(payload.as_bytes());
    let result = mac.finalize();

    format!("sha256={}", hex::encode(result.into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_list() {
        let manager = WebhookManager::new();

        let webhook = OutboundWebhook {
            name: "test".to_string(),
            url: "https://example.com/hook".to_string(),
            events: vec!["message.sent".to_string()],
            ..Default::default()
        };

        manager.register(webhook).await;
        let list = manager.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "test");
    }

    #[tokio::test]
    async fn test_remove() {
        let manager = WebhookManager::new();

        manager
            .register(OutboundWebhook {
                name: "to_remove".to_string(),
                url: "https://example.com".to_string(),
                events: vec!["*".to_string()],
                ..Default::default()
            })
            .await;

        assert!(manager.remove("to_remove").await);
        assert!(manager.list().await.is_empty());
    }
}
