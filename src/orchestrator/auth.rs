//! Per-job bearer token authentication for worker-to-orchestrator communication.
//!
//! Security properties:
//! - Tokens are cryptographically random (32 bytes, hex-encoded) using `OsRng` (C-6)
//! - Tokens are scoped to a specific job_id
//! - Tokens are ephemeral (in-memory only, never persisted)
//! - Tokens have a configurable TTL (default 4h) and are rejected after expiry (A-7)
//! - A token for Job A cannot access endpoints for Job B

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use rand::RngCore;
use rand::rngs::OsRng;
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Default token TTL (4 hours). Tokens older than this are rejected (A-7).
const DEFAULT_TOKEN_TTL: Duration = Duration::from_secs(4 * 3600);

/// Entry in the token store containing the token and its creation time.
struct TokenEntry {
    token: String,
    created_at: Instant,
}

/// In-memory store for per-job authentication tokens with TTL (A-7).
#[derive(Clone)]
pub struct TokenStore {
    /// Maps job_id -> token entry. Never logged or persisted.
    tokens: Arc<RwLock<HashMap<Uuid, TokenEntry>>>,
    /// Maximum age for tokens before they are considered expired.
    ttl: Duration,
}

impl TokenStore {
    pub fn new() -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            ttl: DEFAULT_TOKEN_TTL,
        }
    }

    /// Create a token store with a custom TTL.
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            ttl,
        }
    }

    /// Generate and store a new token for a job.
    pub async fn create_token(&self, job_id: Uuid) -> String {
        let token = generate_token();
        self.tokens.write().await.insert(
            job_id,
            TokenEntry {
                token: token.clone(),
                created_at: Instant::now(),
            },
        );
        token
    }

    /// Validate a token for a specific job (constant-time comparison + TTL check).
    ///
    /// Returns `false` if:
    /// - No token exists for the job
    /// - The token does not match (constant-time)
    /// - The token has expired (A-7)
    pub async fn validate(&self, job_id: Uuid, token: &str) -> bool {
        self.tokens
            .read()
            .await
            .get(&job_id)
            .map(|entry| {
                let token_matches: bool = entry.token.as_bytes().ct_eq(token.as_bytes()).into();
                let not_expired = entry.created_at.elapsed() < self.ttl;
                token_matches && not_expired
            })
            .unwrap_or(false)
    }

    /// Remove a token (on container cleanup).
    pub async fn revoke(&self, job_id: Uuid) {
        self.tokens.write().await.remove(&job_id);
    }

    /// Get the number of active tokens (for diagnostics).
    pub async fn active_count(&self) -> usize {
        self.tokens.read().await.len()
    }

    /// Remove all expired tokens (A-7).
    pub async fn cleanup_expired(&self) -> usize {
        let mut tokens = self.tokens.write().await;
        let before = tokens.len();
        tokens.retain(|_, entry| entry.created_at.elapsed() < self.ttl);
        before - tokens.len()
    }
}

impl Default for TokenStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a cryptographically random token using `OsRng` (C-6).
/// 32 bytes, hex-encoded = 64 chars.
fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex_encode(&bytes)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Axum middleware that validates worker bearer tokens.
///
/// Extracts the job_id from the path (`/worker/{job_id}/...`) and validates
/// the `Authorization: Bearer <token>` header against the token store.
///
/// Wire up with `axum::middleware::from_fn_with_state(token_store, worker_auth_middleware)`.
pub async fn worker_auth_middleware(
    State(token_store): State<TokenStore>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = request.uri().path().to_string();
    let job_id = extract_job_id_from_path(&path).ok_or(StatusCode::BAD_REQUEST)?;

    let token = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if !token_store.validate(job_id, token).await {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(request).await)
}

/// Extract job UUID from a path like `/worker/{uuid}/...`
fn extract_job_id_from_path(path: &str) -> Option<Uuid> {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if parts.len() >= 2 && parts[0] == "worker" {
        Uuid::parse_str(parts[1]).ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_create_and_validate() {
        let store = TokenStore::new();
        let job_id = Uuid::new_v4();

        let token = store.create_token(job_id).await;
        assert_eq!(token.len(), 64); // 32 bytes hex = 64 chars

        assert!(store.validate(job_id, &token).await);
        assert!(!store.validate(job_id, "wrong-token").await);
        assert!(!store.validate(Uuid::new_v4(), &token).await);
    }

    #[tokio::test]
    async fn test_token_revoke() {
        let store = TokenStore::new();
        let job_id = Uuid::new_v4();

        let token = store.create_token(job_id).await;
        assert!(store.validate(job_id, &token).await);

        store.revoke(job_id).await;
        assert!(!store.validate(job_id, &token).await);
    }

    #[tokio::test]
    async fn test_token_ttl_expiry() {
        // A-7: Tokens expire after TTL
        let store = TokenStore::with_ttl(Duration::from_millis(50));
        let job_id = Uuid::new_v4();

        let token = store.create_token(job_id).await;
        assert!(store.validate(job_id, &token).await);

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !store.validate(job_id, &token).await,
            "Token should be rejected after TTL expiry"
        );
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let store = TokenStore::with_ttl(Duration::from_millis(50));
        let job1 = Uuid::new_v4();
        let job2 = Uuid::new_v4();

        store.create_token(job1).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        store.create_token(job2).await;

        // job1 token is expired, job2 is fresh
        let removed = store.cleanup_expired().await;
        assert_eq!(removed, 1);
        assert_eq!(store.active_count().await, 1);
    }

    #[test]
    fn test_extract_job_id() {
        let id = Uuid::new_v4();
        let path = format!("/worker/{}/llm/complete", id);
        assert_eq!(extract_job_id_from_path(&path), Some(id));

        assert_eq!(extract_job_id_from_path("/other/path"), None);
        assert_eq!(extract_job_id_from_path("/worker/not-a-uuid/foo"), None);
    }

    #[test]
    fn test_token_is_random() {
        let t1 = generate_token();
        let t2 = generate_token();
        assert_ne!(t1, t2);
    }
}
