//! Log redaction layer for sensitive data.
//!
//! Automatically redacts sensitive data (API keys, tokens, passwords, emails,
//! private keys, etc.) from strings before they are written to logs. This
//! prevents accidental exposure of secrets in log output.
//!
//! # Usage
//!
//! ```rust
//! use ironclaw::safety::log_redaction::{LogRedactor, RedactionPattern};
//!
//! let redactor = LogRedactor::new();
//! let input = "Using key sk-abcdefghijklmnopqrstuvwxyz for auth";
//! let output = redactor.redact(input);
//! assert!(output.contains("[REDACTED_API_KEY]"));
//! assert!(!output.contains("sk-abcdefghijklmnopqrstuvwxyz"));
//! ```

use std::borrow::Cow;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Patterns that should be redacted from log output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionPattern {
    /// Human-readable name for this pattern.
    pub name: String,
    /// Regex pattern to match.
    pub pattern: String,
    /// What to replace matched content with.
    pub replacement: String,
}

/// A compiled redaction pattern ready for execution.
struct CompiledPattern {
    /// Retained for diagnostics; not read in normal operation.
    #[allow(dead_code)]
    name: String,
    regex: Regex,
    replacement: String,
}

/// Log redaction engine that strips sensitive data from strings.
///
/// Patterns are compiled once at construction time and reused for every
/// call to [`redact`](Self::redact) or [`redact_owned`](Self::redact_owned).
/// When no patterns match, `redact` returns a zero-copy `Cow::Borrowed`.
pub struct LogRedactor {
    patterns: Vec<CompiledPattern>,
}

impl LogRedactor {
    /// Create a new redactor with the default set of redaction patterns.
    pub fn new() -> Self {
        let defaults = default_patterns();
        let patterns = defaults
            .into_iter()
            .filter_map(|p| compile_pattern(p).ok())
            .collect();

        Self { patterns }
    }

    /// Add a custom redaction pattern (builder style).
    ///
    /// Returns `Self` unchanged if the pattern fails to compile as a regex.
    pub fn with_pattern(mut self, pattern: RedactionPattern) -> Self {
        match compile_pattern(pattern) {
            Ok(compiled) => {
                self.patterns.push(compiled);
            }
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    "Failed to compile custom redaction pattern, skipping"
                );
            }
        }
        self
    }

    /// Redact sensitive data from the input string.
    ///
    /// Returns `Cow::Borrowed` when no patterns match (zero-copy fast path).
    /// Returns `Cow::Owned` with all matches replaced when redaction occurs.
    pub fn redact<'a>(&self, input: &'a str) -> Cow<'a, str> {
        if self.patterns.is_empty() {
            return Cow::Borrowed(input);
        }

        // Quick check: does any pattern match at all?
        let any_match = self.patterns.iter().any(|p| p.regex.is_match(input));
        if !any_match {
            return Cow::Borrowed(input);
        }

        let mut result = input.to_string();
        for pattern in &self.patterns {
            // Use regex replace_all which handles the replacement syntax
            let replaced = pattern
                .regex
                .replace_all(&result, pattern.replacement.as_str());
            if let Cow::Owned(new_result) = replaced {
                result = new_result;
            }
        }

        Cow::Owned(result)
    }

    /// Redact sensitive data and always return an owned `String`.
    pub fn redact_owned(&self, input: &str) -> String {
        self.redact(input).into_owned()
    }

    /// Return the number of compiled patterns.
    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }
}

impl Default for LogRedactor {
    fn default() -> Self {
        Self::new()
    }
}

/// Compile a `RedactionPattern` into a `CompiledPattern`.
fn compile_pattern(pattern: RedactionPattern) -> Result<CompiledPattern, regex::Error> {
    let regex = Regex::new(&pattern.pattern)?;
    Ok(CompiledPattern {
        name: pattern.name,
        regex,
        replacement: pattern.replacement,
    })
}

/// Build the default set of redaction patterns.
///
/// These cover the most common secret formats encountered in practice.
fn default_patterns() -> Vec<RedactionPattern> {
    vec![
        // 1. API keys (e.g. OpenAI sk-... keys, including sk-proj- variant)
        RedactionPattern {
            name: "api_key".to_string(),
            pattern: r"sk-(?:proj-)?[a-zA-Z0-9]{20,}".to_string(),
            replacement: "[REDACTED_API_KEY]".to_string(),
        },
        // 2. Bearer tokens
        RedactionPattern {
            name: "bearer_token".to_string(),
            pattern: r"(Bearer\s+[a-zA-Z0-9_\-\.]{20,})".to_string(),
            replacement: "[REDACTED_BEARER]".to_string(),
        },
        // 3. AWS Access Key IDs
        RedactionPattern {
            name: "aws_key".to_string(),
            pattern: r"(AKIA[0-9A-Z]{16})".to_string(),
            replacement: "[REDACTED_AWS_KEY]".to_string(),
        },
        // 4. AWS secret keys (context-aware: preceded by common labels)
        RedactionPattern {
            name: "aws_secret".to_string(),
            pattern:
                r"(?i)(?:aws_secret_access_key|aws_secret|secret_key)\s*[=:]\s*([a-zA-Z0-9/+]{40})"
                    .to_string(),
            replacement: "[REDACTED_AWS_SECRET]".to_string(),
        },
        // 5. Passwords in URLs (e.g. postgres://user:password@host)
        RedactionPattern {
            name: "password_in_url".to_string(),
            pattern: r"(://[^:]+:)[^@]+(@)".to_string(),
            replacement: "${1}[REDACTED]${2}".to_string(),
        },
        // 6. Email addresses
        RedactionPattern {
            name: "email".to_string(),
            pattern: r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}".to_string(),
            replacement: "[REDACTED_EMAIL]".to_string(),
        },
        // 7. JWT tokens (three base64url segments separated by dots)
        RedactionPattern {
            name: "jwt".to_string(),
            pattern: r"eyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}".to_string(),
            replacement: "[REDACTED_JWT]".to_string(),
        },
        // 8. Private keys (PEM headers)
        RedactionPattern {
            name: "private_key".to_string(),
            pattern: r"-----BEGIN (?:RSA |EC |DSA )?PRIVATE KEY-----".to_string(),
            replacement: "[REDACTED_PRIVATE_KEY]".to_string(),
        },
        // 9. Hex secrets following common labels (32+ hex chars after
        //    "secret", "token", "key", or "password")
        RedactionPattern {
            name: "hex_secret".to_string(),
            pattern: r"(?i)(?:secret|token|key|password)\s*[=:]\s*([a-fA-F0-9]{32,})".to_string(),
            replacement: "[REDACTED_HEX_SECRET]".to_string(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Construction ──────────────────────────────────────────────

    #[test]
    fn test_new_has_default_patterns() {
        let redactor = LogRedactor::new();
        assert!(
            redactor.pattern_count() > 0,
            "default redactor should have patterns"
        );
    }

    #[test]
    fn test_with_pattern_adds_custom() {
        let redactor = LogRedactor::new().with_pattern(RedactionPattern {
            name: "custom".to_string(),
            pattern: r"CUSTOM-[0-9]+".to_string(),
            replacement: "[CUSTOM]".to_string(),
        });
        let base_count = LogRedactor::new().pattern_count();
        assert_eq!(redactor.pattern_count(), base_count + 1);
    }

    #[test]
    fn test_with_pattern_invalid_regex_is_skipped() {
        let base_count = LogRedactor::new().pattern_count();
        let redactor = LogRedactor::new().with_pattern(RedactionPattern {
            name: "bad".to_string(),
            pattern: r"[invalid".to_string(),
            replacement: "[X]".to_string(),
        });
        assert_eq!(redactor.pattern_count(), base_count);
    }

    // ── Zero-copy fast path ───────────────────────────────────────

    #[test]
    fn test_clean_input_returns_borrowed() {
        let redactor = LogRedactor::new();
        let input = "Just a normal log line with no secrets.";
        let result = redactor.redact(input);
        assert!(
            matches!(result, Cow::Borrowed(_)),
            "clean input should return Cow::Borrowed"
        );
        assert_eq!(result, input);
    }

    // ── API key redaction ─────────────────────────────────────────

    #[test]
    fn test_redact_openai_api_key() {
        let redactor = LogRedactor::new();
        let input = "Calling API with key sk-abcdefghijklmnopqrstuvwxyz123";
        let result = redactor.redact(input);
        assert!(
            result.contains("[REDACTED_API_KEY]"),
            "should redact API key"
        );
        assert!(
            !result.contains("sk-abcdefghijklmnopqrstuvwxyz123"),
            "original key should not appear"
        );
    }

    #[test]
    fn test_redact_sk_proj_key() {
        let redactor = LogRedactor::new();
        let input = "key=sk-proj-abc123def456ghi789jkl012mno";
        let result = redactor.redact(input);
        assert!(result.contains("[REDACTED_API_KEY]"));
    }

    // ── Bearer token redaction ────────────────────────────────────

    #[test]
    fn test_redact_bearer_token() {
        let redactor = LogRedactor::new();
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload.signature";
        let result = redactor.redact(input);
        assert!(
            result.contains("[REDACTED_BEARER]") || result.contains("[REDACTED_JWT]"),
            "should redact bearer/JWT token, got: {}",
            result,
        );
    }

    // ── AWS key redaction ─────────────────────────────────────────

    #[test]
    fn test_redact_aws_access_key() {
        let redactor = LogRedactor::new();
        let input = "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE";
        let result = redactor.redact(input);
        assert!(
            result.contains("[REDACTED_AWS_KEY]"),
            "should redact AWS key, got: {}",
            result,
        );
        assert!(!result.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn test_redact_aws_secret() {
        let redactor = LogRedactor::new();
        let input = "aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
        let result = redactor.redact(input);
        assert!(
            result.contains("[REDACTED_AWS_SECRET]"),
            "should redact AWS secret, got: {}",
            result,
        );
        assert!(!result.contains("wJalrXUtnFEMI"));
    }

    // ── Password in URL redaction ─────────────────────────────────

    #[test]
    fn test_redact_password_in_postgres_url() {
        let redactor = LogRedactor::new();
        let input = "DATABASE_URL=postgres://admin:s3cretP@ss@db.example.com:5432/mydb";
        let result = redactor.redact(input);
        assert!(
            result.contains("[REDACTED]"),
            "should redact password in URL, got: {}",
            result,
        );
        assert!(!result.contains("s3cretP@ss"), "password should not appear");
        // The host should survive redaction
        assert!(
            result.contains("db.example.com") || result.contains("[REDACTED_EMAIL]"),
            "host or redacted host should appear"
        );
    }

    #[test]
    fn test_redact_password_in_mysql_url() {
        let redactor = LogRedactor::new();
        let input = "mysql://root:hunter2@localhost/app";
        let result = redactor.redact(input);
        assert!(
            result.contains("[REDACTED]"),
            "should redact MySQL password"
        );
        assert!(!result.contains("hunter2"));
    }

    // ── Email redaction ───────────────────────────────────────────

    #[test]
    fn test_redact_email() {
        let redactor = LogRedactor::new();
        let input = "User logged in: alice.smith@example.com";
        let result = redactor.redact(input);
        assert!(
            result.contains("[REDACTED_EMAIL]"),
            "should redact email, got: {}",
            result,
        );
        assert!(!result.contains("alice.smith@example.com"));
    }

    #[test]
    fn test_redact_multiple_emails() {
        let redactor = LogRedactor::new();
        let input = "From: a@b.com To: c@d.org";
        let result = redactor.redact(input);
        // Both should be redacted
        assert!(!result.contains("a@b.com"));
        assert!(!result.contains("c@d.org"));
    }

    // ── JWT redaction ─────────────────────────────────────────────

    #[test]
    fn test_redact_jwt() {
        let redactor = LogRedactor::new();
        let input = "token=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let result = redactor.redact(input);
        assert!(
            result.contains("[REDACTED_JWT]"),
            "should redact JWT, got: {}",
            result,
        );
        assert!(!result.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
    }

    // ── Private key redaction ─────────────────────────────────────

    #[test]
    fn test_redact_rsa_private_key() {
        let redactor = LogRedactor::new();
        let input = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA...";
        let result = redactor.redact(input);
        assert!(
            result.contains("[REDACTED_PRIVATE_KEY]"),
            "should redact RSA private key header"
        );
    }

    #[test]
    fn test_redact_ec_private_key() {
        let redactor = LogRedactor::new();
        let input = "-----BEGIN EC PRIVATE KEY-----\ndata...";
        let result = redactor.redact(input);
        assert!(
            result.contains("[REDACTED_PRIVATE_KEY]"),
            "should redact EC private key header"
        );
    }

    #[test]
    fn test_redact_generic_private_key() {
        let redactor = LogRedactor::new();
        let input = "-----BEGIN PRIVATE KEY-----\ndata...";
        let result = redactor.redact(input);
        assert!(
            result.contains("[REDACTED_PRIVATE_KEY]"),
            "should redact generic private key header"
        );
    }

    #[test]
    fn test_redact_dsa_private_key() {
        let redactor = LogRedactor::new();
        let input = "-----BEGIN DSA PRIVATE KEY-----\ndata...";
        let result = redactor.redact(input);
        assert!(
            result.contains("[REDACTED_PRIVATE_KEY]"),
            "should redact DSA private key header"
        );
    }

    // ── Hex secret redaction ──────────────────────────────────────

    #[test]
    fn test_redact_hex_secret_after_label() {
        let redactor = LogRedactor::new();
        let input = "secret=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let result = redactor.redact(input);
        assert!(
            result.contains("[REDACTED_HEX_SECRET]"),
            "should redact hex secret, got: {}",
            result,
        );
    }

    #[test]
    fn test_hex_without_label_not_redacted() {
        let redactor = LogRedactor::new();
        // A lone hex string without a context label should NOT be redacted
        // by the hex_secret pattern (it is context-aware).
        let input = "hash: 0123456789abcdef0123456789abcdef";
        let result = redactor.redact(input);
        // "hash" is not in the label set, so it should pass through
        assert!(
            !result.contains("[REDACTED_HEX_SECRET]"),
            "hex without known label should not be redacted, got: {}",
            result,
        );
    }

    #[test]
    fn test_hex_secret_with_token_label() {
        let redactor = LogRedactor::new();
        let input = "token: aabbccdd00112233aabbccdd00112233aabbccdd00112233aabbccdd00112233";
        let result = redactor.redact(input);
        assert!(
            result.contains("[REDACTED_HEX_SECRET]"),
            "should redact hex token, got: {}",
            result,
        );
    }

    // ── Multiple patterns in one string ───────────────────────────

    #[test]
    fn test_redact_multiple_secrets() {
        let redactor = LogRedactor::new();
        let input = "key=sk-abcdefghijklmnopqrstuvwxyz123 email=test@example.com";
        let result = redactor.redact(input);
        assert!(result.contains("[REDACTED_API_KEY]"));
        assert!(result.contains("[REDACTED_EMAIL]"));
    }

    // ── redact_owned ──────────────────────────────────────────────

    #[test]
    fn test_redact_owned_returns_string() {
        let redactor = LogRedactor::new();
        let result: String = redactor.redact_owned("clean text");
        assert_eq!(result, "clean text");
    }

    #[test]
    fn test_redact_owned_with_secret() {
        let redactor = LogRedactor::new();
        let result: String = redactor.redact_owned("key sk-abcdefghijklmnopqrstuvwxyz123 is used");
        assert!(result.contains("[REDACTED_API_KEY]"));
    }

    // ── Custom pattern ────────────────────────────────────────────

    #[test]
    fn test_custom_pattern_works() {
        let redactor = LogRedactor::new().with_pattern(RedactionPattern {
            name: "internal_id".to_string(),
            pattern: r"INTERNAL-[A-Z0-9]{12}".to_string(),
            replacement: "[REDACTED_INTERNAL]".to_string(),
        });
        let input = "Processing INTERNAL-A1B2C3D4E5F6 record";
        let result = redactor.redact(input);
        assert!(result.contains("[REDACTED_INTERNAL]"));
        assert!(!result.contains("INTERNAL-A1B2C3D4E5F6"));
    }

    // ── Edge cases ────────────────────────────────────────────────

    #[test]
    fn test_empty_input() {
        let redactor = LogRedactor::new();
        let result = redactor.redact("");
        assert_eq!(result, "");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn test_sk_too_short_not_redacted() {
        let redactor = LogRedactor::new();
        // sk- followed by fewer than 20 chars should not trigger
        let input = "prefix sk-short";
        let result = redactor.redact(input);
        assert!(
            !result.contains("[REDACTED_API_KEY]"),
            "short sk- prefix should not be redacted"
        );
    }

    #[test]
    fn test_default_impl() {
        // Verify the Default trait implementation works
        let redactor = LogRedactor::default();
        assert!(redactor.pattern_count() > 0);
    }

    // ── Serialization round-trip ──────────────────────────────────

    #[test]
    fn test_redaction_pattern_serde_roundtrip() {
        let pattern = RedactionPattern {
            name: "test".to_string(),
            pattern: r"foo\d+".to_string(),
            replacement: "[FOO]".to_string(),
        };
        let json = serde_json::to_string(&pattern).expect("serialize should succeed");
        let deserialized: RedactionPattern =
            serde_json::from_str(&json).expect("deserialize should succeed");
        assert_eq!(deserialized.name, "test");
        assert_eq!(deserialized.pattern, r"foo\d+");
        assert_eq!(deserialized.replacement, "[FOO]");
    }

    // ── Real-world log line examples ──────────────────────────────

    #[test]
    fn test_real_world_config_dump() {
        let redactor = LogRedactor::new();
        let input = r#"Config loaded: {"database_url": "postgres://ironclaw:p4$$w0rd@db.prod.internal:5432/ironclaw", "openai_key": "sk-proj-abc123def456ghi789jkl012mno345pq"}"#;
        let result = redactor.redact(input);
        assert!(
            !result.contains("p4$$w0rd"),
            "password in URL should be redacted"
        );
        assert!(
            !result.contains("sk-proj-abc123def456ghi789jkl012mno345pq"),
            "API key should be redacted"
        );
    }

    #[test]
    fn test_real_world_auth_header() {
        let redactor = LogRedactor::new();
        let input = "Sending request with Bearer eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL2V4YW1wbGUuY29tIiwiZXhwIjoxNzA5MjQ2NDAwfQ.signature_value_here_1234567890";
        let result = redactor.redact(input);
        assert!(
            !result.contains("eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9"),
            "JWT should be redacted"
        );
    }

    #[test]
    fn test_real_world_aws_credentials() {
        let redactor = LogRedactor::new();
        let input = "Loading credentials: aws_access_key_id=AKIAIOSFODNN7EXAMPLE aws_secret_access_key=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
        let result = redactor.redact(input);
        assert!(
            !result.contains("AKIAIOSFODNN7EXAMPLE"),
            "AWS access key should be redacted"
        );
        assert!(
            !result.contains("wJalrXUtnFEMI"),
            "AWS secret should be redacted"
        );
    }

    #[test]
    fn test_real_world_pem_in_log() {
        let redactor = LogRedactor::new();
        let input = "Certificate loaded:\n-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASC\n-----END PRIVATE KEY-----";
        let result = redactor.redact(input);
        assert!(
            result.contains("[REDACTED_PRIVATE_KEY]"),
            "private key header should be redacted"
        );
    }
}
