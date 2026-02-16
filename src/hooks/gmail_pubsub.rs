//! Gmail Pub/Sub integration for real-time email notifications.
//!
//! Receives push notifications from Google Cloud Pub/Sub when new emails
//! arrive in Gmail, then triggers the appropriate hook or routine.
//!
//! Setup requires:
//! 1. A Google Cloud project with Gmail API and Pub/Sub enabled
//! 2. A Pub/Sub topic and push subscription pointing to our webhook
//! 3. Gmail watch set up via `gmail.users.watch()`

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Gmail Pub/Sub configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmailPubSubConfig {
    /// Google Cloud project ID.
    pub project_id: String,
    /// Pub/Sub topic name (e.g., "projects/my-project/topics/gmail-notifications").
    pub topic_name: String,
    /// Label IDs to watch (empty = all inbox).
    #[serde(default)]
    pub label_ids: Vec<String>,
    /// OAuth access token for Gmail API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    /// Path to service account key JSON (alternative to access token).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_account_key_path: Option<String>,
}

/// A Gmail Pub/Sub notification received from Google Cloud.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailNotification {
    /// The email address of the mailbox.
    pub email_address: String,
    /// The history ID (use to fetch changes since last check).
    pub history_id: u64,
}

/// Decoded Pub/Sub push message.
#[derive(Debug, Deserialize)]
pub struct PubSubPushMessage {
    pub message: PubSubMessage,
    #[allow(dead_code)]
    pub subscription: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PubSubMessage {
    pub data: String,
    #[serde(default)]
    pub attributes: std::collections::HashMap<String, String>,
    #[serde(rename = "messageId")]
    #[allow(dead_code)]
    pub message_id: Option<String>,
}

/// Gmail Pub/Sub handler that processes incoming notifications.
pub struct GmailPubSubHandler {
    config: GmailPubSubConfig,
    client: reqwest::Client,
    /// Channel to send notifications to the hook/routine engine.
    notification_tx: mpsc::Sender<GmailNotification>,
    /// Last known history ID for deduplication.
    last_history_id: tokio::sync::RwLock<Option<u64>>,
}

impl GmailPubSubHandler {
    /// Create a new Gmail Pub/Sub handler.
    pub fn new(
        config: GmailPubSubConfig,
        notification_tx: mpsc::Sender<GmailNotification>,
    ) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            notification_tx,
            last_history_id: tokio::sync::RwLock::new(None),
        }
    }

    /// Handle an incoming Pub/Sub push notification.
    ///
    /// This is called by the webhook endpoint when Google Cloud Pub/Sub
    /// sends a push notification.
    pub async fn handle_push(&self, push_message: PubSubPushMessage) -> Result<(), String> {
        // Decode the base64-encoded message data
        let decoded = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &push_message.message.data,
        )
        .map_err(|e| format!("Failed to decode Pub/Sub message: {}", e))?;

        let notification: GmailNotification = serde_json::from_slice(&decoded)
            .map_err(|e| format!("Failed to parse Gmail notification: {}", e))?;

        // Deduplication: skip if we've already processed this history ID
        {
            let last = self.last_history_id.read().await;
            if let Some(last_id) = *last
                && notification.history_id <= last_id
            {
                tracing::debug!(
                    "Skipping duplicate Gmail notification (history_id: {} <= {})",
                    notification.history_id,
                    last_id
                );
                return Ok(());
            }
        }

        // Update last history ID
        *self.last_history_id.write().await = Some(notification.history_id);

        tracing::info!(
            "Gmail notification for {}: history_id={}",
            notification.email_address,
            notification.history_id
        );

        // Send to hook/routine engine
        self.notification_tx
            .send(notification)
            .await
            .map_err(|e| format!("Failed to send notification: {}", e))?;

        Ok(())
    }

    /// Set up a Gmail watch for push notifications.
    ///
    /// Calls `gmail.users.watch()` to register for push notifications.
    /// Must be called periodically (watch expires after ~7 days).
    pub async fn setup_watch(&self) -> Result<WatchResponse, String> {
        let access_token = self
            .config
            .access_token
            .as_ref()
            .ok_or("No access token configured for Gmail API")?;

        let label_ids = if self.config.label_ids.is_empty() {
            vec!["INBOX".to_string()]
        } else {
            self.config.label_ids.clone()
        };

        let request = WatchRequest {
            topic_name: self.config.topic_name.clone(),
            label_ids,
            label_filter_behavior: Some("INCLUDE".to_string()),
        };

        let response = self
            .client
            .post("https://gmail.googleapis.com/gmail/v1/users/me/watch")
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Failed to set up Gmail watch: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Gmail watch setup failed: {}", error_text));
        }

        let watch_response: WatchResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse watch response: {}", e))?;

        tracing::info!(
            "Gmail watch set up, expires at: {}",
            watch_response.expiration
        );

        // Store the initial history ID
        *self.last_history_id.write().await = Some(watch_response.history_id);

        Ok(watch_response)
    }

    /// Stop the Gmail watch.
    pub async fn stop_watch(&self) -> Result<(), String> {
        let access_token = self
            .config
            .access_token
            .as_ref()
            .ok_or("No access token configured for Gmail API")?;

        let response = self
            .client
            .post("https://gmail.googleapis.com/gmail/v1/users/me/stop")
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .map_err(|e| format!("Failed to stop Gmail watch: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Gmail watch stop failed: {}", error_text));
        }

        tracing::info!("Gmail watch stopped");
        Ok(())
    }

    /// Fetch message history since the last known history ID.
    pub async fn fetch_history(&self, start_history_id: u64) -> Result<Vec<HistoryRecord>, String> {
        let access_token = self
            .config
            .access_token
            .as_ref()
            .ok_or("No access token configured for Gmail API")?;

        let url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/history?startHistoryId={}",
            start_history_id
        );

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .map_err(|e| format!("Failed to fetch history: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("History fetch failed: {}", error_text));
        }

        let history_response: HistoryResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse history response: {}", e))?;

        Ok(history_response.history.unwrap_or_default())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WatchRequest {
    topic_name: String,
    label_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label_filter_behavior: Option<String>,
}

/// Response from Gmail watch setup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchResponse {
    /// History ID at the time of watch setup.
    pub history_id: u64,
    /// Watch expiration timestamp (milliseconds since epoch).
    pub expiration: String,
}

/// Gmail history response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryResponse {
    history: Option<Vec<HistoryRecord>>,
    #[allow(dead_code)]
    next_page_token: Option<String>,
    #[allow(dead_code)]
    history_id: Option<String>,
}

/// A single history record from Gmail.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    /// The history ID.
    pub id: String,
    /// Messages added.
    #[serde(default)]
    pub messages_added: Vec<HistoryMessage>,
}

/// A message reference in history.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryMessage {
    pub message: MessageRef,
}

/// Reference to a Gmail message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRef {
    pub id: String,
    #[serde(rename = "threadId")]
    pub thread_id: String,
}

/// Create a notification channel for Gmail pub/sub.
pub fn create_notification_channel() -> (
    mpsc::Sender<GmailNotification>,
    mpsc::Receiver<GmailNotification>,
) {
    mpsc::channel(256)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gmail_notification_deserialization() {
        let json = r#"{"emailAddress": "user@gmail.com", "historyId": 12345}"#;
        let notification: GmailNotification = serde_json::from_str(json).unwrap();
        assert_eq!(notification.email_address, "user@gmail.com");
        assert_eq!(notification.history_id, 12345);
    }

    #[test]
    fn test_pubsub_push_message_deserialization() {
        let json = r#"{
            "message": {
                "data": "eyJlbWFpbEFkZHJlc3MiOiJ0ZXN0QGdtYWlsLmNvbSIsImhpc3RvcnlJZCI6MTIzNDV9",
                "attributes": {},
                "messageId": "msg-1"
            },
            "subscription": "projects/test/subscriptions/gmail-sub"
        }"#;
        let msg: PubSubPushMessage = serde_json::from_str(json).unwrap();
        assert!(!msg.message.data.is_empty());
    }

    #[test]
    fn test_watch_response_deserialization() {
        let json = r#"{"historyId": 99999, "expiration": "1700000000000"}"#;
        let resp: WatchResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.history_id, 99999);
    }

    #[tokio::test]
    async fn test_create_notification_channel() {
        let (tx, mut rx) = create_notification_channel();
        tx.send(GmailNotification {
            email_address: "test@gmail.com".to_string(),
            history_id: 1,
        })
        .await
        .unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.email_address, "test@gmail.com");
    }
}
