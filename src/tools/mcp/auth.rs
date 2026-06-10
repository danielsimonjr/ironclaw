//! OAuth 2.1 authentication for MCP servers.
//!
//! Implements the MCP Authorization specification using OAuth 2.1 with PKCE.
//! See: https://spec.modelcontextprotocol.io/specification/2025-03-26/basic/authorization/

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

use crate::secrets::{CreateSecretParams, SecretsStore};
use crate::tools::mcp::config::McpServerConfig;

/// OAuth authorization error.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Server does not support OAuth authorization")]
    NotSupported,

    #[error("Failed to discover authorization endpoints: {0}")]
    DiscoveryFailed(String),

    #[error("Authorization denied by user")]
    AuthorizationDenied,

    #[error("State parameter mismatch (possible CSRF)")]
    StateMismatch,

    #[error("Token exchange failed: {0}")]
    TokenExchangeFailed(String),

    #[error("Token expired and refresh failed: {0}")]
    RefreshFailed(String),

    #[error("No access token available")]
    NoToken,

    #[error("Timeout waiting for authorization callback")]
    Timeout,

    #[error("Could not bind to callback port")]
    PortUnavailable,

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Secrets error: {0}")]
    Secrets(String),
}

/// OAuth protected resource metadata.
/// Discovered from /.well-known/oauth-protected-resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectedResourceMetadata {
    /// The protected resource identifier.
    pub resource: String,

    /// Authorization servers that can issue tokens for this resource.
    #[serde(default)]
    pub authorization_servers: Vec<String>,

    /// Scopes supported by this resource.
    #[serde(default)]
    pub scopes_supported: Vec<String>,
}

/// OAuth authorization server metadata.
/// Discovered from /.well-known/oauth-authorization-server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationServerMetadata {
    /// Authorization server issuer.
    pub issuer: String,

    /// Authorization endpoint URL.
    pub authorization_endpoint: String,

    /// Token endpoint URL.
    pub token_endpoint: String,

    /// Dynamic client registration endpoint (if DCR is supported).
    #[serde(default)]
    pub registration_endpoint: Option<String>,

    /// Supported response types.
    #[serde(default)]
    pub response_types_supported: Vec<String>,

    /// Supported grant types.
    #[serde(default)]
    pub grant_types_supported: Vec<String>,

    /// Supported code challenge methods.
    #[serde(default)]
    pub code_challenge_methods_supported: Vec<String>,

    /// Scopes supported by this server.
    #[serde(default)]
    pub scopes_supported: Vec<String>,
}

/// Dynamic Client Registration request.
#[derive(Debug, Clone, Serialize)]
pub struct ClientRegistrationRequest {
    /// Human-readable client name.
    pub client_name: String,

    /// Redirect URIs for OAuth callbacks.
    pub redirect_uris: Vec<String>,

    /// Grant types the client will use.
    pub grant_types: Vec<String>,

    /// Response types the client will use.
    pub response_types: Vec<String>,

    /// Token endpoint authentication method.
    pub token_endpoint_auth_method: String,
}

/// Dynamic Client Registration response.
#[derive(Debug, Clone, Deserialize)]
pub struct ClientRegistrationResponse {
    /// The assigned client ID.
    pub client_id: String,

    /// Client secret (if issued).
    #[serde(default)]
    pub client_secret: Option<String>,

    /// When the client secret expires (if applicable).
    #[serde(default)]
    pub client_secret_expires_at: Option<u64>,

    /// Registration access token for managing the registration.
    #[serde(default)]
    pub registration_access_token: Option<String>,

    /// Registration client URI for managing the registration.
    #[serde(default)]
    pub registration_client_uri: Option<String>,
}

/// Access token with optional refresh token and expiry.
///
/// Token values are wrapped in `SecretString` so accidental `Debug` /
/// serialization / logging cannot leak them. The custom `Debug` impl below
/// further redacts the redacted-token marker for clarity (Findings 12, 13).
#[derive(Clone)]
pub struct AccessToken {
    /// The access token value (protected).
    pub access_token: SecretString,

    /// Token type (usually "Bearer").
    pub token_type: String,

    /// Seconds until expiration (if provided).
    pub expires_in: Option<u64>,

    /// Refresh token for obtaining new access tokens (protected).
    pub refresh_token: Option<SecretString>,

    /// Scopes granted.
    pub scope: Option<String>,
}

impl AccessToken {
    /// Expose the access token bytes. Callers must ensure the result is
    /// not logged or re-serialized — it is the bare token.
    pub fn expose_access_token(&self) -> &str {
        self.access_token.expose_secret()
    }

    /// Expose the refresh token bytes if present.
    pub fn expose_refresh_token(&self) -> Option<&str> {
        self.refresh_token.as_ref().map(|s| s.expose_secret())
    }
}

impl std::fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccessToken")
            .field("access_token", &"[REDACTED]")
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("scope", &self.scope)
            .finish()
    }
}

/// Token response from the authorization server.
///
/// Manual `Debug` impl redacts `access_token` and `refresh_token` so the
/// raw response cannot leak via `tracing` / `format!("{:?}", ...)` /
/// `Result::unwrap()` panics (Finding 13).
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

impl std::fmt::Debug for TokenResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenResponse")
            .field("access_token", &"[REDACTED]")
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("scope", &self.scope)
            .finish()
    }
}

/// PKCE verifier and challenge pair.
#[derive(Debug, Clone)]
pub struct PkceChallenge {
    /// Code verifier (high-entropy random string).
    pub verifier: String,
    /// Code challenge (S256 hash of verifier).
    pub challenge: String,
}

impl PkceChallenge {
    /// Generate a new PKCE challenge pair.
    pub fn generate() -> Self {
        let mut verifier_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut verifier_bytes);
        let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

        Self {
            verifier,
            challenge,
        }
    }
}

/// Discover protected resource metadata from an MCP server.
pub async fn discover_protected_resource(
    server_url: &str,
) -> Result<ProtectedResourceMetadata, AuthError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| AuthError::Http(e.to_string()))?;

    // Parse the server URL to extract the origin (scheme + host + port)
    // The .well-known endpoints are always at the root of the origin, not under any path
    let parsed = reqwest::Url::parse(server_url)
        .map_err(|e| AuthError::DiscoveryFailed(format!("Invalid server URL: {}", e)))?;
    let origin = parsed.origin().ascii_serialization();

    // Try the well-known endpoint at the origin root
    let well_known_url = format!("{}/.well-known/oauth-protected-resource", origin);

    let response = client
        .get(&well_known_url)
        .send()
        .await
        .map_err(|e| AuthError::DiscoveryFailed(e.to_string()))?;

    if !response.status().is_success() {
        return Err(AuthError::NotSupported);
    }

    response
        .json()
        .await
        .map_err(|e| AuthError::DiscoveryFailed(format!("Invalid metadata: {}", e)))
}

/// Discover authorization server metadata.
pub async fn discover_authorization_server(
    auth_server_url: &str,
) -> Result<AuthorizationServerMetadata, AuthError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| AuthError::Http(e.to_string()))?;

    let base_url = auth_server_url.trim_end_matches('/');
    let well_known_url = format!("{}/.well-known/oauth-authorization-server", base_url);

    let response = client
        .get(&well_known_url)
        .send()
        .await
        .map_err(|e| AuthError::DiscoveryFailed(e.to_string()))?;

    if !response.status().is_success() {
        return Err(AuthError::DiscoveryFailed(format!(
            "HTTP {}",
            response.status()
        )));
    }

    response
        .json()
        .await
        .map_err(|e| AuthError::DiscoveryFailed(format!("Invalid metadata: {}", e)))
}

/// Discover OAuth endpoints for an MCP server.
///
/// First checks if endpoints are explicitly configured, then falls back to discovery.
pub async fn discover_oauth_endpoints(
    server_config: &McpServerConfig,
) -> Result<(String, String), AuthError> {
    let oauth = server_config
        .oauth
        .as_ref()
        .ok_or(AuthError::NotSupported)?;

    // If endpoints are explicitly configured, use them
    if let (Some(auth_url), Some(token_url)) = (&oauth.authorization_url, &oauth.token_url) {
        return Ok((auth_url.clone(), token_url.clone()));
    }

    // Try to discover from the server
    let resource_meta = discover_protected_resource(&server_config.url).await?;

    // Get the first authorization server
    let auth_server_url = resource_meta
        .authorization_servers
        .first()
        .ok_or_else(|| AuthError::DiscoveryFailed("No authorization servers listed".to_string()))?;

    // Discover the authorization server metadata
    let auth_meta = discover_authorization_server(auth_server_url).await?;

    Ok((auth_meta.authorization_endpoint, auth_meta.token_endpoint))
}

/// Discover full OAuth metadata including DCR support.
///
/// Returns authorization server metadata which includes registration_endpoint if DCR is supported.
pub async fn discover_full_oauth_metadata(
    server_url: &str,
) -> Result<AuthorizationServerMetadata, AuthError> {
    // Try to discover from the server
    let resource_meta = discover_protected_resource(server_url).await?;

    // Get the first authorization server
    let auth_server_url = resource_meta
        .authorization_servers
        .first()
        .ok_or_else(|| AuthError::DiscoveryFailed("No authorization servers listed".to_string()))?;

    // Discover the authorization server metadata
    discover_authorization_server(auth_server_url).await
}

/// Perform Dynamic Client Registration with an authorization server.
///
/// This allows clients to register themselves at runtime without pre-configured credentials.
pub async fn register_client(
    registration_endpoint: &str,
    redirect_uri: &str,
) -> Result<ClientRegistrationResponse, AuthError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| AuthError::Http(e.to_string()))?;

    let request = ClientRegistrationRequest {
        client_name: "IronClaw".to_string(),
        redirect_uris: vec![redirect_uri.to_string()],
        grant_types: vec![
            "authorization_code".to_string(),
            "refresh_token".to_string(),
        ],
        response_types: vec!["code".to_string()],
        token_endpoint_auth_method: "none".to_string(), // Public client (no secret)
    };

    let response = client
        .post(registration_endpoint)
        .json(&request)
        .send()
        .await
        .map_err(|e| AuthError::DiscoveryFailed(format!("DCR request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AuthError::DiscoveryFailed(format!(
            "DCR failed: HTTP {} - {}",
            status, body
        )));
    }

    response
        .json()
        .await
        .map_err(|e| AuthError::DiscoveryFailed(format!("Invalid DCR response: {}", e)))
}

/// Perform the OAuth 2.1 authorization flow for an MCP server.
///
/// Supports two modes:
/// 1. Pre-configured OAuth: Uses the client_id from server config
/// 2. Dynamic Client Registration: Discovers and registers with the server automatically
///
/// Flow:
/// 1. Discovers authorization endpoints from the server
/// 2. If no client_id configured, attempts Dynamic Client Registration (DCR)
/// 3. Generates PKCE challenge
/// 4. Opens browser for user authorization
/// 5. Receives callback with authorization code
/// 6. Exchanges code for access token
/// 7. Stores token securely
pub async fn authorize_mcp_server(
    server_config: &McpServerConfig,
    secrets: &Arc<dyn SecretsStore + Send + Sync>,
    user_id: &str,
) -> Result<AccessToken, AuthError> {
    // Find an available port for the callback first (needed for DCR)
    let (listener, port) = find_available_port().await?;
    let redirect_uri = format!("http://localhost:{}/callback", port);

    // Determine client_id and endpoints
    let (client_id, authorization_url, token_url, use_pkce, scopes, extra_params) =
        if let Some(oauth) = &server_config.oauth {
            // Pre-configured OAuth
            let (auth_url, tok_url) = discover_oauth_endpoints(server_config).await?;
            (
                oauth.client_id.clone(),
                auth_url,
                tok_url,
                oauth.use_pkce,
                oauth.scopes.clone(),
                oauth.extra_params.clone(),
            )
        } else {
            // Try Dynamic Client Registration
            println!("  Discovering OAuth endpoints...");
            let auth_meta = discover_full_oauth_metadata(&server_config.url).await?;

            let registration_endpoint = auth_meta
                .registration_endpoint
                .ok_or(AuthError::NotSupported)?;

            println!("  Registering client dynamically...");
            let registration = register_client(&registration_endpoint, &redirect_uri).await?;
            println!("  ✓ Client registered: {}", registration.client_id);

            (
                registration.client_id,
                auth_meta.authorization_endpoint,
                auth_meta.token_endpoint,
                true, // Always use PKCE for DCR clients
                auth_meta.scopes_supported,
                HashMap::new(),
            )
        };

    // Generate PKCE challenge
    let pkce = if use_pkce {
        Some(PkceChallenge::generate())
    } else {
        None
    };

    // Build authorization URL (returns expected `state` for CSRF validation)
    let (auth_url, expected_state) = build_authorization_url(
        &authorization_url,
        &client_id,
        &redirect_uri,
        &scopes,
        pkce.as_ref(),
        &extra_params,
    );

    // Open browser
    println!("  Opening browser for {} login...", server_config.name);
    if let Err(e) = open::that(&auth_url) {
        println!("  Could not open browser: {}", e);
        println!("  Please open this URL manually:");
        println!("  {}", auth_url);
    }

    println!("  Waiting for authorization...");

    // Wait for callback (validates state matches expected_state — Finding 12)
    let code =
        wait_for_authorization_callback(listener, &server_config.name, &expected_state).await?;

    println!("  Exchanging code for token...");

    // Exchange code for token
    let token =
        exchange_code_for_token(&token_url, &client_id, &code, &redirect_uri, pkce.as_ref())
            .await?;

    // Store the tokens
    store_tokens(secrets, user_id, server_config, &token).await?;

    // Store the client_id for DCR (needed for token refresh)
    if server_config.oauth.is_none() {
        store_client_id(secrets, user_id, server_config, &client_id).await?;
    }

    Ok(token)
}

/// Find an available port for the OAuth callback.
pub async fn find_available_port() -> Result<(TcpListener, u16), AuthError> {
    for port in 9876..=9886 {
        if let Ok(listener) = TcpListener::bind(format!("127.0.0.1:{}", port)).await {
            return Ok((listener, port));
        }
    }
    Err(AuthError::PortUnavailable)
}

/// Build the authorization URL with all required parameters.
///
/// Automatically generates a cryptographic `state` parameter for CSRF
/// protection unless one is already present in `extra_params` (Finding 12).
/// Returns `(url, state)` so the caller can validate it on callback.
pub fn build_authorization_url(
    base_url: &str,
    client_id: &str,
    redirect_uri: &str,
    scopes: &[String],
    pkce: Option<&PkceChallenge>,
    extra_params: &HashMap<String, String>,
) -> (String, String) {
    let mut url = format!(
        "{}?client_id={}&response_type=code&redirect_uri={}",
        base_url,
        urlencoding::encode(client_id),
        urlencoding::encode(redirect_uri)
    );

    if !scopes.is_empty() {
        url.push_str(&format!(
            "&scope={}",
            urlencoding::encode(&scopes.join(" "))
        ));
    }

    if let Some(pkce) = pkce {
        url.push_str(&format!(
            "&code_challenge={}&code_challenge_method=S256",
            pkce.challenge
        ));
    }

    // Add CSRF-protection `state` if not already provided. The state is
    // returned to the caller so it can be validated on callback (Finding 12).
    let state = if let Some(existing) = extra_params.get("state") {
        existing.clone()
    } else {
        use rand::RngCore;
        let mut state_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut state_bytes);
        let s: String = state_bytes.iter().map(|b| format!("{:02x}", b)).collect();
        url.push_str(&format!("&state={}", urlencoding::encode(&s)));
        s
    };

    for (key, value) in extra_params {
        url.push_str(&format!(
            "&{}={}",
            urlencoding::encode(key),
            urlencoding::encode(value)
        ));
    }

    (url, state)
}

/// Parse a `code` and `state` pair from a URL-encoded query string.
///
/// Returns `(code, state)`. `state` is `None` if absent. Helper extracted so
/// the parser can be unit-tested without a TCP listener (Finding 12).
fn parse_callback_query(query: &str) -> (Option<String>, Option<String>) {
    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    for param in query.split('&') {
        let parts: Vec<&str> = param.splitn(2, '=').collect();
        if parts.len() != 2 {
            continue;
        }
        let value = urlencoding::decode(parts[1])
            .unwrap_or_else(|_| parts[1].into())
            .into_owned();
        match parts[0] {
            "code" => code = Some(value),
            "state" => state = Some(value),
            _ => {}
        }
    }
    (code, state)
}

/// Constant-time check that the returned `state` equals the expected one.
///
/// Uses `subtle::ConstantTimeEq` to avoid leaking the state via timing
/// (Finding 12).
fn state_matches(expected: &str, actual: &str) -> bool {
    use subtle::ConstantTimeEq;
    let e = expected.as_bytes();
    let a = actual.as_bytes();
    if e.len() != a.len() {
        return false;
    }
    e.ct_eq(a).into()
}

/// Wait for the authorization callback and extract the code.
///
/// Validates the `state` query parameter against `expected_state` using
/// constant-time comparison; mismatch or absence is rejected as CSRF
/// (Finding 12).
pub async fn wait_for_authorization_callback(
    listener: TcpListener,
    server_name: &str,
    expected_state: &str,
) -> Result<String, AuthError> {
    let timeout = Duration::from_secs(300);

    tokio::time::timeout(timeout, async {
        loop {
            let (mut socket, _) = listener
                .accept()
                .await
                .map_err(|e| AuthError::Http(e.to_string()))?;

            let mut reader = BufReader::new(&mut socket);
            let mut request_line = String::new();
            reader
                .read_line(&mut request_line)
                .await
                .map_err(|e| AuthError::Http(e.to_string()))?;

            // Parse GET /callback?code=xxx&state=yyy HTTP/1.1
            if let Some(path) = request_line.split_whitespace().nth(1)
                && path.starts_with("/callback")
                && let Some(query) = path.split('?').nth(1)
            {
                // Check for error first
                if query.contains("error=") {
                    let response = "HTTP/1.1 400 Bad Request\r\n\r\nAuthorization denied";
                    let _ = socket.write_all(response.as_bytes()).await;
                    return Err(AuthError::AuthorizationDenied);
                }

                let (code, state) = parse_callback_query(query);

                // CSRF protection: state MUST be present and match expected.
                let state_ok = match state.as_deref() {
                    Some(actual) => state_matches(expected_state, actual),
                    None => false,
                };
                if !state_ok {
                    let response = "HTTP/1.1 400 Bad Request\r\n\r\nState parameter mismatch";
                    let _ = socket.write_all(response.as_bytes()).await;
                    return Err(AuthError::StateMismatch);
                }

                if let Some(code) = code {
                    // Send success response
                    let response = format!(
                        "HTTP/1.1 200 OK\r\n\
                         Content-Type: text/html\r\n\
                         \r\n\
                         <!DOCTYPE html><html><body style=\"font-family: sans-serif; \
                         display: flex; justify-content: center; align-items: center; \
                         height: 100vh; margin: 0; background: #191919; color: white;\">\
                         <div style=\"text-align: center;\">\
                         <h1>✓ {} Connected!</h1>\
                         <p>You can close this window.</p>\
                         </div></body></html>",
                        server_name
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;

                    return Ok(code);
                }
            }

            let response = "HTTP/1.1 404 Not Found\r\n\r\n";
            let _ = socket.write_all(response.as_bytes()).await;
        }
    })
    .await
    .map_err(|_| AuthError::Timeout)?
}

/// Exchange the authorization code for an access token.
pub async fn exchange_code_for_token(
    token_url: &str,
    client_id: &str,
    code: &str,
    redirect_uri: &str,
    pkce: Option<&PkceChallenge>,
) -> Result<AccessToken, AuthError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| AuthError::Http(e.to_string()))?;

    let mut params = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("redirect_uri", redirect_uri.to_string()),
        ("client_id", client_id.to_string()),
    ];

    if let Some(pkce) = pkce {
        params.push(("code_verifier", pkce.verifier.clone()));
    }

    let response = client
        .post(token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| AuthError::TokenExchangeFailed(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        // Don't capture the response body — it may contain the raw token
        // (Finding 13).
        let _ = response.text().await;
        return Err(AuthError::TokenExchangeFailed(format!("HTTP {}", status)));
    }

    let token_response: TokenResponse = response
        .json()
        .await
        .map_err(|e| AuthError::TokenExchangeFailed(format!("Invalid response: {}", e)))?;

    Ok(AccessToken {
        access_token: SecretString::from(token_response.access_token),
        token_type: token_response.token_type,
        expires_in: token_response.expires_in,
        refresh_token: token_response.refresh_token.map(SecretString::from),
        scope: token_response.scope,
    })
}

/// Store access and refresh tokens securely.
pub async fn store_tokens(
    secrets: &Arc<dyn SecretsStore + Send + Sync>,
    user_id: &str,
    server_config: &McpServerConfig,
    token: &AccessToken,
) -> Result<(), AuthError> {
    // Store access token
    let params = CreateSecretParams::new(
        server_config.token_secret_name(),
        token.access_token.expose_secret(),
    )
    .with_provider(format!("mcp:{}", server_config.name));

    secrets
        .create(user_id, params)
        .await
        .map_err(|e| AuthError::Secrets(e.to_string()))?;

    // Store refresh token if present
    if let Some(ref refresh_token) = token.refresh_token {
        let params = CreateSecretParams::new(
            server_config.refresh_token_secret_name(),
            refresh_token.expose_secret(),
        )
        .with_provider(format!("mcp:{}", server_config.name));

        secrets
            .create(user_id, params)
            .await
            .map_err(|e| AuthError::Secrets(e.to_string()))?;
    }

    Ok(())
}

/// Store the DCR client ID for future token refresh.
pub async fn store_client_id(
    secrets: &Arc<dyn SecretsStore + Send + Sync>,
    user_id: &str,
    server_config: &McpServerConfig,
    client_id: &str,
) -> Result<(), AuthError> {
    let params = CreateSecretParams::new(server_config.client_id_secret_name(), client_id)
        .with_provider(format!("mcp:{}", server_config.name));

    secrets
        .create(user_id, params)
        .await
        .map(|_| ())
        .map_err(|e| AuthError::Secrets(e.to_string()))
}

/// Get the client ID for a server (from config or stored DCR).
async fn get_client_id(
    server_config: &McpServerConfig,
    secrets: &Arc<dyn SecretsStore + Send + Sync>,
    user_id: &str,
) -> Result<String, AuthError> {
    // First check if OAuth is configured with a client_id
    if let Some(ref oauth) = server_config.oauth {
        return Ok(oauth.client_id.clone());
    }

    // Otherwise try to get the DCR client_id from secrets
    match secrets
        .get_decrypted(user_id, &server_config.client_id_secret_name())
        .await
    {
        Ok(client_id) => Ok(client_id.expose().to_string()),
        Err(crate::secrets::SecretError::NotFound(_)) => Err(AuthError::RefreshFailed(
            "No client ID found. Please re-authenticate.".to_string(),
        )),
        Err(e) => Err(AuthError::Secrets(e.to_string())),
    }
}

/// Get the stored access token for an MCP server.
pub async fn get_access_token(
    server_config: &McpServerConfig,
    secrets: &Arc<dyn SecretsStore + Send + Sync>,
    user_id: &str,
) -> Result<Option<String>, AuthError> {
    match secrets
        .get_decrypted(user_id, &server_config.token_secret_name())
        .await
    {
        Ok(token) => Ok(Some(token.expose().to_string())),
        Err(crate::secrets::SecretError::NotFound(_)) => Ok(None),
        Err(e) => Err(AuthError::Secrets(e.to_string())),
    }
}

/// Check if a server has valid authentication.
///
/// Returns true if:
/// - A valid access token is stored (regardless of how it was obtained)
/// - The server doesn't require authentication at all
pub async fn is_authenticated(
    server_config: &McpServerConfig,
    secrets: &Arc<dyn SecretsStore + Send + Sync>,
    user_id: &str,
) -> bool {
    // Check if we have a stored token (from either pre-configured OAuth or DCR)
    secrets
        .exists(user_id, &server_config.token_secret_name())
        .await
        .unwrap_or(false)
}

/// Refresh an access token using the refresh token.
///
/// Works with both pre-configured OAuth and Dynamic Client Registration (DCR).
/// For DCR, retrieves the client_id from stored secrets.
pub async fn refresh_access_token(
    server_config: &McpServerConfig,
    secrets: &Arc<dyn SecretsStore + Send + Sync>,
    user_id: &str,
) -> Result<AccessToken, AuthError> {
    // Get client_id (from config or stored DCR)
    let client_id = get_client_id(server_config, secrets, user_id).await?;

    // Get the refresh token
    let refresh_token = secrets
        .get_decrypted(user_id, &server_config.refresh_token_secret_name())
        .await
        .map_err(|e| AuthError::RefreshFailed(format!("No refresh token: {}", e)))?;

    // Discover the token endpoint
    let token_url = if let Some(ref oauth) = server_config.oauth {
        if let Some(ref url) = oauth.token_url {
            url.clone()
        } else {
            // Discover from server
            let auth_meta = discover_full_oauth_metadata(&server_config.url).await?;
            auth_meta.token_endpoint
        }
    } else {
        // DCR - always discover
        let auth_meta = discover_full_oauth_metadata(&server_config.url).await?;
        auth_meta.token_endpoint
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| AuthError::Http(e.to_string()))?;

    let params = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_token.expose().to_string()),
        ("client_id", client_id),
    ];

    let response = client
        .post(&token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| AuthError::RefreshFailed(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        // Don't capture the response body — successful-token bodies are
        // sometimes returned even with a non-2xx wrapper, and the body may
        // contain the raw access/refresh token. Surface only the status
        // (Finding 13).
        let _ = response.text().await; // drain to free the connection
        return Err(AuthError::RefreshFailed(format!("HTTP {}", status)));
    }

    let token_response: TokenResponse = response
        .json()
        .await
        .map_err(|e| AuthError::RefreshFailed(format!("Invalid response: {}", e)))?;

    let token = AccessToken {
        access_token: SecretString::from(token_response.access_token),
        token_type: token_response.token_type,
        expires_in: token_response.expires_in,
        refresh_token: token_response.refresh_token.map(SecretString::from),
        scope: token_response.scope,
    };

    // Store the new tokens
    store_tokens(secrets, user_id, server_config, &token).await?;

    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_response_debug_redacts_secrets() {
        // Finding 13: TokenResponse must never leak the raw token via
        // {:?} formatting (used by tracing, .unwrap() panics, etc.).
        let tr = TokenResponse {
            access_token: "super-secret-access-token-xyz".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: Some(3600),
            refresh_token: Some("super-secret-refresh-token-abc".to_string()),
            scope: Some("read".to_string()),
        };
        let dbg = format!("{:?}", tr);
        assert!(!dbg.contains("super-secret-access-token-xyz"));
        assert!(!dbg.contains("super-secret-refresh-token-abc"));
        assert!(dbg.contains("[REDACTED]"));
    }

    #[test]
    fn test_access_token_debug_redacts_secrets() {
        let at = AccessToken {
            access_token: SecretString::from("plain-access-XYZ".to_string()),
            token_type: "Bearer".to_string(),
            expires_in: Some(60),
            refresh_token: Some(SecretString::from("plain-refresh-ABC".to_string())),
            scope: None,
        };
        let dbg = format!("{:?}", at);
        assert!(!dbg.contains("plain-access-XYZ"));
        assert!(!dbg.contains("plain-refresh-ABC"));
        assert!(dbg.contains("[REDACTED]"));
    }

    #[test]
    fn test_pkce_challenge_generation() {
        let pkce = PkceChallenge::generate();

        // Verifier should be base64url encoded
        assert!(!pkce.verifier.is_empty());
        assert!(!pkce.verifier.contains('+'));
        assert!(!pkce.verifier.contains('/'));
        assert!(!pkce.verifier.contains('='));

        // Challenge should be different from verifier
        assert_ne!(pkce.verifier, pkce.challenge);

        // Two challenges should be different
        let pkce2 = PkceChallenge::generate();
        assert_ne!(pkce.verifier, pkce2.verifier);
    }

    #[test]
    fn test_build_authorization_url() {
        let (url, state) = build_authorization_url(
            "https://auth.example.com/authorize",
            "client-123",
            "http://localhost:9876/callback",
            &["read".to_string(), "write".to_string()],
            None,
            &HashMap::new(),
        );

        assert!(url.starts_with("https://auth.example.com/authorize?"));
        assert!(url.contains("client_id=client-123"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("redirect_uri="));
        assert!(url.contains("scope=read%20write"));
        // state was generated and is included in URL
        assert!(!state.is_empty());
        assert!(url.contains(&format!("state={}", urlencoding::encode(&state))));
    }

    #[test]
    fn test_build_authorization_url_with_pkce() {
        let pkce = PkceChallenge::generate();
        let (url, _state) = build_authorization_url(
            "https://auth.example.com/authorize",
            "client-123",
            "http://localhost:9876/callback",
            &[],
            Some(&pkce),
            &HashMap::new(),
        );

        assert!(url.contains(&format!("code_challenge={}", pkce.challenge)));
        assert!(url.contains("code_challenge_method=S256"));
    }

    #[test]
    fn test_build_authorization_url_with_extra_params() {
        let mut extra = HashMap::new();
        extra.insert("owner".to_string(), "user".to_string());
        extra.insert("state".to_string(), "abc123".to_string());

        let (url, state) = build_authorization_url(
            "https://auth.example.com/authorize",
            "client-123",
            "http://localhost:9876/callback",
            &[],
            None,
            &extra,
        );

        assert!(url.contains("owner=user"));
        assert!(url.contains("state=abc123"));
        // When state is provided in extra_params, it's returned verbatim
        assert_eq!(state, "abc123");
    }

    #[test]
    fn test_state_matches_constant_time() {
        assert!(state_matches("abc123", "abc123"));
        assert!(!state_matches("abc123", "abc124"));
        assert!(!state_matches("abc123", "abc12"));
        assert!(!state_matches("", "abc"));
        assert!(state_matches("", ""));
    }

    #[test]
    fn test_parse_callback_query_extracts_code_and_state() {
        let (code, state) = parse_callback_query("code=abc&state=xyz");
        assert_eq!(code.as_deref(), Some("abc"));
        assert_eq!(state.as_deref(), Some("xyz"));
    }

    #[test]
    fn test_parse_callback_query_url_decodes() {
        let (code, state) = parse_callback_query("code=a%2Bb&state=s%20t");
        assert_eq!(code.as_deref(), Some("a+b"));
        assert_eq!(state.as_deref(), Some("s t"));
    }

    #[test]
    fn test_parse_callback_query_missing_state() {
        let (code, state) = parse_callback_query("code=abc");
        assert_eq!(code.as_deref(), Some("abc"));
        assert!(state.is_none());
    }

    #[tokio::test]
    async fn test_wait_for_callback_rejects_mismatched_state() {
        // Bind a listener, fire a fake callback request with the wrong state,
        // and assert StateMismatch is returned (Finding 12 TDD).
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let server_handle = tokio::spawn(async move {
            wait_for_authorization_callback(listener, "test", "expected-state").await
        });

        // Brief wait for the listener to be in accept()
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let req =
            b"GET /callback?code=somecode&state=wrong-state HTTP/1.1\r\nHost: localhost\r\n\r\n";
        stream.write_all(req).await.unwrap();

        let result = server_handle.await.unwrap();
        assert!(matches!(result, Err(AuthError::StateMismatch)));
    }

    #[tokio::test]
    async fn test_wait_for_callback_accepts_matched_state() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let server_handle = tokio::spawn(async move {
            wait_for_authorization_callback(listener, "test", "right-state").await
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let req =
            b"GET /callback?code=goodcode&state=right-state HTTP/1.1\r\nHost: localhost\r\n\r\n";
        stream.write_all(req).await.unwrap();

        let result = server_handle.await.unwrap();
        assert_eq!(result.unwrap(), "goodcode");
    }

    #[tokio::test]
    async fn test_wait_for_callback_rejects_missing_state() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let server_handle = tokio::spawn(async move {
            wait_for_authorization_callback(listener, "test", "expected").await
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let req = b"GET /callback?code=somecode HTTP/1.1\r\nHost: localhost\r\n\r\n";
        stream.write_all(req).await.unwrap();

        let result = server_handle.await.unwrap();
        assert!(matches!(result, Err(AuthError::StateMismatch)));
    }
}
