//! Bearer token authentication middleware for the web gateway.

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use subtle::ConstantTimeEq;

/// Shared auth state injected via axum middleware state.
#[derive(Clone)]
pub struct AuthState {
    pub token: String,
}

/// Auth middleware that validates bearer token from header or query param.
///
/// SSE connections can't set headers from `EventSource`, so we also accept
/// `?token=xxx` as a query parameter.
pub async fn auth_middleware(
    State(auth): State<AuthState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    // Try Authorization header first (constant-time comparison)
    if let Some(auth_header) = headers.get("authorization")
        && let Ok(value) = auth_header.to_str()
        && let Some(token) = value.strip_prefix("Bearer ")
        && bool::from(token.as_bytes().ct_eq(auth.token.as_bytes()))
    {
        return next.run(request).await;
    }

    // Fall back to query parameter for SSE EventSource (constant-time comparison).
    // URL-decode the token value before comparison so that percent-encoded
    // characters (e.g. `%20`) are handled correctly (A-1).
    if let Some(query) = request.uri().query() {
        for pair in query.split('&') {
            if let Some(raw_token) = pair.strip_prefix("token=") {
                let decoded =
                    urlencoding::decode(raw_token).unwrap_or(std::borrow::Cow::Borrowed(raw_token));
                if bool::from(decoded.as_bytes().ct_eq(auth.token.as_bytes())) {
                    return next.run(request).await;
                }
            }
        }
    }

    (StatusCode::UNAUTHORIZED, "Invalid or missing auth token").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_state_clone() {
        let state = AuthState {
            token: "test-token".to_string(),
        };
        let cloned = state.clone();
        assert_eq!(cloned.token, "test-token");
    }

    #[test]
    fn test_url_decode_token() {
        // Verify that URL-encoded tokens are properly decoded (A-1)
        let encoded = "ABC%20DEF";
        let decoded = urlencoding::decode(encoded).unwrap();
        assert_eq!(decoded, "ABC DEF");

        // Verify constant-time comparison after decoding
        let expected = "ABC DEF";
        assert!(bool::from(decoded.as_bytes().ct_eq(expected.as_bytes())));
    }
}
