//! OAuth 2.0 / 2.1 authorization flow implementation.
//!
//! Provides a complete OAuth flow for tool and extension authentication:
//! 1. Generate authorization URL
//! 2. Start local callback server
//! 3. Exchange authorization code for tokens
//! 4. Refresh tokens when expired
//!
//! Supports Authorization Code with PKCE (RFC 7636), which is the
//! recommended flow for native/CLI applications.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// OAuth 2.0 client configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    /// OAuth client ID.
    pub client_id: String,
    /// OAuth client secret (optional for public clients using PKCE).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Authorization endpoint URL.
    pub authorize_url: String,
    /// Token endpoint URL.
    pub token_url: String,
    /// Redirect URI (typically http://localhost:<port>/callback).
    pub redirect_uri: String,
    /// Scopes to request.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Whether to use PKCE (recommended for public clients).
    #[serde(default = "default_true")]
    pub use_pkce: bool,
}

fn default_true() -> bool {
    true
}

/// Token response from an OAuth provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    /// Access token for API calls.
    pub access_token: String,
    /// Token type (usually "Bearer").
    #[serde(default = "default_bearer")]
    pub token_type: String,
    /// Refresh token for getting new access tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Time until the access token expires (in seconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
    /// Scopes granted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// When this token set was obtained.
    #[serde(skip)]
    pub obtained_at: Option<Instant>,
}

fn default_bearer() -> String {
    "Bearer".to_string()
}

impl OAuthTokens {
    /// Check if the access token has expired (with a 60-second buffer).
    pub fn is_expired(&self) -> bool {
        match (self.expires_in, self.obtained_at) {
            (Some(expires_in), Some(obtained_at)) => {
                let buffer = Duration::from_secs(60);
                obtained_at.elapsed() + buffer > Duration::from_secs(expires_in)
            }
            _ => false, // If we don't know, assume it's still valid
        }
    }

    /// Check if the token can be refreshed.
    pub fn can_refresh(&self) -> bool {
        self.refresh_token.is_some()
    }
}

/// PKCE verifier and challenge pair (RFC 7636).
#[derive(Debug, Clone)]
pub struct PkceChallenge {
    /// The code verifier (random string sent with token request).
    pub verifier: String,
    /// The code challenge (SHA256 hash of verifier, sent with auth request).
    pub challenge: String,
    /// Challenge method (always "S256").
    pub method: String,
}

impl PkceChallenge {
    /// Generate a new PKCE challenge pair.
    pub fn generate() -> Self {
        use rand::Rng;
        use sha2::{Digest, Sha256};

        // Generate 32 random bytes for the verifier
        let random_bytes: Vec<u8> = (0..32).map(|_| rand::thread_rng().r#gen::<u8>()).collect();
        let verifier = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            &random_bytes,
        );

        // SHA256 hash the verifier to create the challenge
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let hash = hasher.finalize();
        let challenge = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            hash.as_slice(),
        );

        Self {
            verifier,
            challenge,
            method: "S256".to_string(),
        }
    }
}

/// OAuth flow manager that handles the complete authorization lifecycle.
pub struct OAuthFlowManager {
    client: reqwest::Client,
    /// Active flows keyed by state parameter.
    pending_flows: Arc<Mutex<HashMap<String, PendingFlow>>>,
    /// Stored tokens keyed by provider name.
    tokens: Arc<Mutex<HashMap<String, OAuthTokens>>>,
}

/// A pending OAuth authorization flow.
struct PendingFlow {
    config: OAuthConfig,
    pkce: Option<PkceChallenge>,
    #[allow(dead_code)]
    started_at: Instant,
}

impl OAuthFlowManager {
    /// Create a new OAuth flow manager.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            pending_flows: Arc::new(Mutex::new(HashMap::new())),
            tokens: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start an OAuth authorization flow.
    ///
    /// Returns the authorization URL that the user should open in their browser.
    pub async fn start_flow(&self, provider: &str, config: OAuthConfig) -> Result<String, String> {
        // Generate state parameter for CSRF protection
        use rand::Rng;
        let state: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        // Generate PKCE challenge if enabled
        let pkce = if config.use_pkce {
            Some(PkceChallenge::generate())
        } else {
            None
        };

        // Build authorization URL
        let mut params = vec![
            ("response_type", "code".to_string()),
            ("client_id", config.client_id.clone()),
            ("redirect_uri", config.redirect_uri.clone()),
            ("state", state.clone()),
        ];

        if !config.scopes.is_empty() {
            params.push(("scope", config.scopes.join(" ")));
        }

        if let Some(ref pkce) = pkce {
            params.push(("code_challenge", pkce.challenge.clone()));
            params.push(("code_challenge_method", pkce.method.clone()));
        }

        let auth_url = format!(
            "{}?{}",
            config.authorize_url,
            params
                .iter()
                .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
                .collect::<Vec<_>>()
                .join("&")
        );

        // Store the pending flow
        let mut flows = self.pending_flows.lock().await;
        flows.insert(
            state.clone(),
            PendingFlow {
                config,
                pkce,
                started_at: Instant::now(),
            },
        );

        tracing::info!("OAuth flow started for provider '{}'", provider);

        Ok(auth_url)
    }

    /// Handle the OAuth callback with the authorization code.
    ///
    /// Exchanges the code for tokens and stores them.
    pub async fn handle_callback(
        &self,
        provider: &str,
        state: &str,
        code: &str,
    ) -> Result<OAuthTokens, String> {
        // Retrieve the pending flow
        let flow = {
            let mut flows = self.pending_flows.lock().await;
            flows
                .remove(state)
                .ok_or_else(|| "Invalid or expired OAuth state parameter".to_string())?
        };

        // Exchange authorization code for tokens
        let mut params = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code.to_string()),
            ("redirect_uri", flow.config.redirect_uri.clone()),
            ("client_id", flow.config.client_id.clone()),
        ];

        if let Some(ref secret) = flow.config.client_secret {
            params.push(("client_secret", secret.clone()));
        }

        if let Some(ref pkce) = flow.pkce {
            params.push(("code_verifier", pkce.verifier.clone()));
        }

        let response = self
            .client
            .post(&flow.config.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("Token exchange failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Token exchange error: {}", error_text));
        }

        let mut tokens: OAuthTokens = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse token response: {}", e))?;

        tokens.obtained_at = Some(Instant::now());

        // Store the tokens
        let mut stored = self.tokens.lock().await;
        stored.insert(provider.to_string(), tokens.clone());

        tracing::info!("OAuth tokens obtained for provider '{}'", provider);

        Ok(tokens)
    }

    /// Refresh an expired access token.
    pub async fn refresh_token(
        &self,
        provider: &str,
        config: &OAuthConfig,
    ) -> Result<OAuthTokens, String> {
        let stored = self.tokens.lock().await;
        let current = stored
            .get(provider)
            .ok_or_else(|| format!("No tokens stored for provider '{}'", provider))?;

        let refresh_token = current
            .refresh_token
            .as_ref()
            .ok_or_else(|| format!("No refresh token for provider '{}'", provider))?;

        let mut params = vec![
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh_token.clone()),
            ("client_id", config.client_id.clone()),
        ];

        if let Some(ref secret) = config.client_secret {
            params.push(("client_secret", secret.clone()));
        }

        drop(stored); // Release lock before HTTP call

        let response = self
            .client
            .post(&config.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("Token refresh failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Token refresh error: {}", error_text));
        }

        let mut tokens: OAuthTokens = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse refresh response: {}", e))?;

        tokens.obtained_at = Some(Instant::now());

        // Keep the refresh token if the new response doesn't include one
        if tokens.refresh_token.is_none() {
            let stored = self.tokens.lock().await;
            if let Some(current) = stored.get(provider) {
                tokens.refresh_token = current.refresh_token.clone();
            }
        }

        // Store the refreshed tokens
        let mut stored = self.tokens.lock().await;
        stored.insert(provider.to_string(), tokens.clone());

        tracing::info!("OAuth tokens refreshed for provider '{}'", provider);

        Ok(tokens)
    }

    /// Get a valid access token for a provider, refreshing if needed.
    pub async fn get_token(&self, provider: &str, config: &OAuthConfig) -> Result<String, String> {
        let stored = self.tokens.lock().await;
        let tokens = stored
            .get(provider)
            .ok_or_else(|| {
                format!(
                    "No tokens for provider '{}'. Start an OAuth flow first.",
                    provider
                )
            })?
            .clone();
        drop(stored);

        if tokens.is_expired() && tokens.can_refresh() {
            let refreshed = self.refresh_token(provider, config).await?;
            Ok(refreshed.access_token)
        } else {
            Ok(tokens.access_token)
        }
    }

    /// Check if tokens exist for a provider.
    pub async fn has_tokens(&self, provider: &str) -> bool {
        self.tokens.lock().await.contains_key(provider)
    }

    /// Remove stored tokens for a provider.
    pub async fn revoke(&self, provider: &str) {
        self.tokens.lock().await.remove(provider);
        tracing::info!("OAuth tokens revoked for provider '{}'", provider);
    }

    /// Clean up expired pending flows (older than 10 minutes).
    pub async fn cleanup_expired_flows(&self) {
        let mut flows = self.pending_flows.lock().await;
        flows.retain(|_, flow| flow.started_at.elapsed() < Duration::from_secs(600));
    }
}

impl Default for OAuthFlowManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_config() {
        let config = OAuthConfig {
            client_id: "test-client".to_string(),
            client_secret: Some("secret".to_string()),
            authorize_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            redirect_uri: "http://localhost:8080/callback".to_string(),
            scopes: vec!["openid".to_string(), "email".to_string()],
            use_pkce: true,
        };
        assert_eq!(config.client_id, "test-client");
        assert!(config.use_pkce);
    }

    #[test]
    fn test_pkce_challenge() {
        let pkce = PkceChallenge::generate();
        assert!(!pkce.verifier.is_empty());
        assert!(!pkce.challenge.is_empty());
        assert_eq!(pkce.method, "S256");

        // Verifier and challenge should be different
        assert_ne!(pkce.verifier, pkce.challenge);
    }

    #[test]
    fn test_pkce_challenge_unique() {
        let pkce1 = PkceChallenge::generate();
        let pkce2 = PkceChallenge::generate();
        assert_ne!(pkce1.verifier, pkce2.verifier);
    }

    #[test]
    fn test_token_expiry() {
        let tokens = OAuthTokens {
            access_token: "test".to_string(),
            token_type: "Bearer".to_string(),
            refresh_token: Some("refresh".to_string()),
            expires_in: Some(3600),
            scope: None,
            obtained_at: Some(Instant::now()),
        };
        assert!(!tokens.is_expired());
        assert!(tokens.can_refresh());
    }

    #[test]
    fn test_token_expired() {
        let tokens = OAuthTokens {
            access_token: "test".to_string(),
            token_type: "Bearer".to_string(),
            refresh_token: None,
            expires_in: Some(0),
            scope: None,
            obtained_at: Some(Instant::now() - Duration::from_secs(100)),
        };
        assert!(tokens.is_expired());
        assert!(!tokens.can_refresh());
    }

    #[tokio::test]
    async fn test_flow_manager_start() {
        let manager = OAuthFlowManager::new();
        let config = OAuthConfig {
            client_id: "test".to_string(),
            client_secret: None,
            authorize_url: "https://example.com/auth".to_string(),
            token_url: "https://example.com/token".to_string(),
            redirect_uri: "http://localhost:8080/callback".to_string(),
            scopes: vec!["read".to_string()],
            use_pkce: true,
        };

        let url = manager.start_flow("test-provider", config).await.unwrap();
        assert!(url.starts_with("https://example.com/auth"));
        assert!(url.contains("code_challenge"));
        assert!(url.contains("state="));
    }

    #[tokio::test]
    async fn test_has_tokens() {
        let manager = OAuthFlowManager::new();
        assert!(!manager.has_tokens("nonexistent").await);
    }

    #[tokio::test]
    async fn test_cleanup_expired_flows() {
        let manager = OAuthFlowManager::new();
        manager.cleanup_expired_flows().await;
        // Should not panic
    }
}
