//! Error types for IronClaw.

use std::time::Duration;

use uuid::Uuid;

/// Top-level error type for the agent.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("Channel error: {0}")]
    Channel(#[from] ChannelError),

    #[error("LLM error: {0}")]
    Llm(#[from] LlmError),

    #[error("Tool error: {0}")]
    Tool(#[from] ToolError),

    #[error("Safety error: {0}")]
    Safety(#[from] SafetyError),

    #[error("Job error: {0}")]
    Job(#[from] JobError),

    #[error("Estimation error: {0}")]
    Estimation(#[from] EstimationError),

    #[error("Evaluation error: {0}")]
    Evaluation(#[from] EvaluationError),

    #[error("Repair error: {0}")]
    Repair(#[from] RepairError),

    #[error("Workspace error: {0}")]
    Workspace(#[from] WorkspaceError),

    #[error("Orchestrator error: {0}")]
    Orchestrator(#[from] OrchestratorError),

    #[error("Worker error: {0}")]
    Worker(#[from] WorkerError),

    #[error("Hook error: {0}")]
    Hook(#[from] HookError),

    #[error("Media error: {0}")]
    Media(#[from] MediaError),

    #[error("Skills error: {0}")]
    Skills(#[from] SkillsError),
}

/// Configuration-related errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    MissingEnvVar(String),

    #[error("Missing required configuration: {key}. {hint}")]
    MissingRequired { key: String, hint: String },

    #[error("Invalid configuration value for {key}: {message}")]
    InvalidValue { key: String, message: String },

    #[error("Failed to parse configuration: {0}")]
    ParseError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Database-related errors.
#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("Connection pool error: {0}")]
    Pool(String),

    #[error("Query failed: {0}")]
    Query(String),

    #[error("Entity not found: {entity} with id {id}")]
    NotFound { entity: String, id: String },

    #[error("Constraint violation: {0}")]
    Constraint(String),

    #[error("Migration failed: {0}")]
    Migration(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[cfg(feature = "postgres")]
    #[error("PostgreSQL error: {0}")]
    Postgres(#[from] tokio_postgres::Error),

    #[cfg(feature = "postgres")]
    #[error("Pool build error: {0}")]
    PoolBuild(#[from] deadpool_postgres::BuildError),

    #[cfg(feature = "postgres")]
    #[error("Pool runtime error: {0}")]
    PoolRuntime(#[from] deadpool_postgres::PoolError),

    #[cfg(feature = "libsql")]
    #[error("LibSQL error: {0}")]
    LibSql(#[from] libsql::Error),
}

/// Channel-related errors.
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("Channel {name} failed to start: {reason}")]
    StartupFailed { name: String, reason: String },

    #[error("Channel {name} disconnected: {reason}")]
    Disconnected { name: String, reason: String },

    #[error("Failed to send response on channel {name}: {reason}")]
    SendFailed { name: String, reason: String },

    #[error("Invalid message format: {0}")]
    InvalidMessage(String),

    #[error("Authentication failed for channel {name}: {reason}")]
    AuthFailed { name: String, reason: String },

    #[error("Rate limited on channel {name}")]
    RateLimited { name: String },

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Channel health check failed: {name}")]
    HealthCheckFailed { name: String },
}

/// LLM provider errors.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("Provider {provider} request failed: {reason}")]
    RequestFailed { provider: String, reason: String },

    #[error("Provider {provider} rate limited, retry after {retry_after:?}")]
    RateLimited {
        provider: String,
        retry_after: Option<Duration>,
    },

    #[error("Invalid response from {provider}: {reason}")]
    InvalidResponse { provider: String, reason: String },

    #[error("Context length exceeded: {used} tokens used, {limit} allowed")]
    ContextLengthExceeded { used: usize, limit: usize },

    #[error("Model {model} not available on provider {provider}")]
    ModelNotAvailable { provider: String, model: String },

    #[error("Authentication failed for provider {provider}")]
    AuthFailed { provider: String },

    #[error("Session expired for provider {provider}")]
    SessionExpired { provider: String },

    #[error("Session renewal failed for provider {provider}: {reason}")]
    SessionRenewalFailed { provider: String, reason: String },

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Tool execution errors.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool {name} not found")]
    NotFound { name: String },

    #[error("Tool {name} execution failed: {reason}")]
    ExecutionFailed { name: String, reason: String },

    #[error("Tool {name} timed out after {timeout:?}")]
    Timeout { name: String, timeout: Duration },

    #[error("Invalid parameters for tool {name}: {reason}")]
    InvalidParameters { name: String, reason: String },

    #[error("Tool {name} is disabled: {reason}")]
    Disabled { name: String, reason: String },

    #[error("Sandbox error for tool {name}: {reason}")]
    Sandbox { name: String, reason: String },

    #[error("Tool {name} requires authentication")]
    AuthRequired { name: String },

    #[error("Tool builder failed: {0}")]
    BuilderFailed(String),
}

/// Safety/sanitization errors.
#[derive(Debug, thiserror::Error)]
pub enum SafetyError {
    #[error("Potential prompt injection detected: {pattern}")]
    InjectionDetected { pattern: String },

    #[error("Output exceeded maximum length: {length} > {max}")]
    OutputTooLarge { length: usize, max: usize },

    #[error("Blocked content pattern detected: {pattern}")]
    BlockedContent { pattern: String },

    #[error("Validation failed: {reason}")]
    ValidationFailed { reason: String },

    #[error("Policy violation: {rule}")]
    PolicyViolation { rule: String },
}

/// Job-related errors.
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("Job {id} not found")]
    NotFound { id: Uuid },

    #[error("Job {id} already in state {state}, cannot transition to {target}")]
    InvalidTransition {
        id: Uuid,
        state: String,
        target: String,
    },

    #[error("Job {id} failed: {reason}")]
    Failed { id: Uuid, reason: String },

    #[error("Job {id} stuck for {duration:?}")]
    Stuck { id: Uuid, duration: Duration },

    #[error("Maximum parallel jobs ({max}) exceeded")]
    MaxJobsExceeded { max: usize },

    #[error("Job {id} context error: {reason}")]
    ContextError { id: Uuid, reason: String },
}

/// Estimation errors.
#[derive(Debug, thiserror::Error)]
pub enum EstimationError {
    #[error("Insufficient data for estimation: need {needed} samples, have {have}")]
    InsufficientData { needed: usize, have: usize },

    #[error("Estimation calculation failed: {reason}")]
    CalculationFailed { reason: String },

    #[error("Invalid estimation parameters: {reason}")]
    InvalidParameters { reason: String },
}

/// Evaluation errors.
#[derive(Debug, thiserror::Error)]
pub enum EvaluationError {
    #[error("Evaluation failed for job {job_id}: {reason}")]
    Failed { job_id: Uuid, reason: String },

    #[error("Missing required evaluation data: {field}")]
    MissingData { field: String },

    #[error("Invalid evaluation criteria: {reason}")]
    InvalidCriteria { reason: String },
}

/// Self-repair errors.
#[derive(Debug, thiserror::Error)]
pub enum RepairError {
    #[error("Repair failed for {target_type} {target_id}: {reason}")]
    Failed {
        target_type: String,
        target_id: Uuid,
        reason: String,
    },

    #[error("Maximum repair attempts ({max}) exceeded for {target_type} {target_id}")]
    MaxAttemptsExceeded {
        target_type: String,
        target_id: Uuid,
        max: u32,
    },

    #[error("Cannot diagnose issue for {target_type} {target_id}: {reason}")]
    DiagnosisFailed {
        target_type: String,
        target_id: Uuid,
        reason: String,
    },
}

/// Workspace/memory errors.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("Document not found: {doc_type} for user {user_id}")]
    DocumentNotFound { doc_type: String, user_id: String },

    #[error("Search failed: {reason}")]
    SearchFailed { reason: String },

    #[error("Embedding generation failed: {reason}")]
    EmbeddingFailed { reason: String },

    #[error("Document chunking failed: {reason}")]
    ChunkingFailed { reason: String },

    #[error("Invalid document type: {doc_type}")]
    InvalidDocType { doc_type: String },

    #[error("Workspace not initialized for user {user_id}")]
    NotInitialized { user_id: String },

    #[error("Heartbeat error: {reason}")]
    HeartbeatError { reason: String },
}

/// Orchestrator errors (internal API, container management).
#[derive(Debug, thiserror::Error)]
pub enum OrchestratorError {
    #[error("Container creation failed for job {job_id}: {reason}")]
    ContainerCreationFailed { job_id: Uuid, reason: String },

    #[error("Container not found for job {job_id}")]
    ContainerNotFound { job_id: Uuid },

    #[error("Container for job {job_id} is in unexpected state: {state}")]
    InvalidContainerState { job_id: Uuid, state: String },

    #[error("Worker authentication failed: {reason}")]
    AuthFailed { reason: String },

    #[error("Internal API error: {reason}")]
    ApiError { reason: String },

    #[error("Docker error: {reason}")]
    Docker { reason: String },

    #[error("Job {job_id} timed out in container")]
    ContainerTimeout { job_id: Uuid },
}

/// Worker errors (container-side execution).
#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("Failed to connect to orchestrator at {url}: {reason}")]
    ConnectionFailed { url: String, reason: String },

    #[error("LLM proxy request failed: {reason}")]
    LlmProxyFailed { reason: String },

    #[error("Secret resolution failed for {secret_name}: {reason}")]
    SecretResolveFailed { secret_name: String, reason: String },

    #[error("Orchestrator returned error for job {job_id}: {reason}")]
    OrchestratorRejected { job_id: Uuid, reason: String },

    #[error("Worker execution failed: {reason}")]
    ExecutionFailed { reason: String },

    #[error("Missing worker token (IRONCLAW_WORKER_TOKEN not set)")]
    MissingToken,
}

/// Hook-related errors.
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    #[error("Hook {name} failed: {reason}")]
    ExecutionFailed { name: String, reason: String },

    #[error("Hook {name} timed out after {timeout_ms}ms")]
    Timeout { name: String, timeout_ms: u64 },

    #[error("Hook registration failed: {reason}")]
    RegistrationFailed { reason: String },
}

/// Media processing errors.
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("Unsupported media type: {mime_type}")]
    UnsupportedType { mime_type: String },

    #[error("Media processing failed: {reason}")]
    ProcessingFailed { reason: String },

    #[error("Media file too large: {size} bytes exceeds {max} byte limit")]
    TooLarge { size: usize, max: usize },

    #[error("Media download failed: {reason}")]
    DownloadFailed { reason: String },

    #[error("Transcription failed: {reason}")]
    TranscriptionFailed { reason: String },

    #[error("Vision processing failed: {reason}")]
    VisionFailed { reason: String },

    #[error("Recursive processing failed: {reason}")]
    RecursiveProcessingFailed { reason: String },

    #[error("Recursive processing exceeded max depth ({max_depth})")]
    MaxDepthExceeded { max_depth: u32 },

    #[error("Recursive processing exceeded max iterations ({max_iterations})")]
    MaxIterationsExceeded { max_iterations: u32 },

    #[error("Chunk index {index} out of range (total: {total})")]
    ChunkOutOfRange { index: usize, total: usize },
}

/// Skills system errors.
#[derive(Debug, thiserror::Error)]
pub enum SkillsError {
    #[error("Skill {name} not found")]
    NotFound { name: String },

    #[error("Skill {name} failed: {reason}")]
    ExecutionFailed { name: String, reason: String },

    #[error("Invalid skill definition: {reason}")]
    InvalidDefinition { reason: String },
}

/// Result type alias for the agent.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    // --- ConfigError ---

    #[test]
    fn test_config_error_missing_env_var_display() {
        let err = ConfigError::MissingEnvVar("DATABASE_URL".to_string());
        assert!(err.to_string().contains("DATABASE_URL"));
        assert!(err
            .to_string()
            .contains("Missing required environment variable"));
    }

    #[test]
    fn test_config_error_missing_required_display() {
        let err = ConfigError::MissingRequired {
            key: "api_key".to_string(),
            hint: "Set API_KEY env var".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("api_key"));
        assert!(msg.contains("Set API_KEY env var"));
    }

    #[test]
    fn test_config_error_invalid_value_display() {
        let err = ConfigError::InvalidValue {
            key: "port".to_string(),
            message: "must be a number".to_string(),
        };
        assert!(err.to_string().contains("port"));
        assert!(err.to_string().contains("must be a number"));
    }

    #[test]
    fn test_config_error_parse_error_display() {
        let err = ConfigError::ParseError("bad toml".to_string());
        assert!(err.to_string().contains("bad toml"));
    }

    #[test]
    fn test_config_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err = ConfigError::from(io_err);
        assert!(err.to_string().contains("file missing"));
    }

    // --- DatabaseError ---

    #[test]
    fn test_database_error_not_found_display() {
        let err = DatabaseError::NotFound {
            entity: "user".to_string(),
            id: "abc-123".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("user"));
        assert!(msg.contains("abc-123"));
    }

    #[test]
    fn test_database_error_pool_display() {
        let err = DatabaseError::Pool("connection refused".to_string());
        assert!(err.to_string().contains("connection refused"));
    }

    // --- ChannelError ---

    #[test]
    fn test_channel_error_startup_failed_display() {
        let err = ChannelError::StartupFailed {
            name: "repl".to_string(),
            reason: "port in use".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("repl"));
        assert!(msg.contains("port in use"));
    }

    #[test]
    fn test_channel_error_rate_limited_display() {
        let err = ChannelError::RateLimited {
            name: "telegram".to_string(),
        };
        assert!(err.to_string().contains("telegram"));
    }

    // --- LlmError ---

    #[test]
    fn test_llm_error_rate_limited_display() {
        let err = LlmError::RateLimited {
            provider: "openai".to_string(),
            retry_after: Some(Duration::from_secs(30)),
        };
        let msg = err.to_string();
        assert!(msg.contains("openai"));
        assert!(msg.contains("30"));
    }

    #[test]
    fn test_llm_error_context_length_display() {
        let err = LlmError::ContextLengthExceeded {
            used: 50000,
            limit: 32000,
        };
        let msg = err.to_string();
        assert!(msg.contains("50000"));
        assert!(msg.contains("32000"));
    }

    // --- ToolError ---

    #[test]
    fn test_tool_error_not_found_display() {
        let err = ToolError::NotFound {
            name: "calculator".to_string(),
        };
        assert!(err.to_string().contains("calculator"));
    }

    #[test]
    fn test_tool_error_timeout_display() {
        let err = ToolError::Timeout {
            name: "shell".to_string(),
            timeout: Duration::from_secs(60),
        };
        let msg = err.to_string();
        assert!(msg.contains("shell"));
        assert!(msg.contains("60"));
    }

    // --- SafetyError ---

    #[test]
    fn test_safety_error_injection_detected_display() {
        let err = SafetyError::InjectionDetected {
            pattern: "ignore previous".to_string(),
        };
        assert!(err.to_string().contains("ignore previous"));
    }

    #[test]
    fn test_safety_error_output_too_large_display() {
        let err = SafetyError::OutputTooLarge {
            length: 100000,
            max: 50000,
        };
        let msg = err.to_string();
        assert!(msg.contains("100000"));
        assert!(msg.contains("50000"));
    }

    // --- JobError ---

    #[test]
    fn test_job_error_not_found_display() {
        let id = Uuid::new_v4();
        let err = JobError::NotFound { id };
        assert!(err.to_string().contains(&id.to_string()));
    }

    #[test]
    fn test_job_error_stuck_display() {
        let id = Uuid::new_v4();
        let err = JobError::Stuck {
            id,
            duration: Duration::from_secs(300),
        };
        let msg = err.to_string();
        assert!(msg.contains(&id.to_string()));
        assert!(msg.contains("300"));
    }

    #[test]
    fn test_job_error_max_jobs_exceeded() {
        let err = JobError::MaxJobsExceeded { max: 10 };
        assert!(err.to_string().contains("10"));
    }

    // --- EstimationError ---

    #[test]
    fn test_estimation_error_insufficient_data() {
        let err = EstimationError::InsufficientData {
            needed: 100,
            have: 5,
        };
        let msg = err.to_string();
        assert!(msg.contains("100"));
        assert!(msg.contains("5"));
    }

    // --- EvaluationError ---

    #[test]
    fn test_evaluation_error_missing_data() {
        let err = EvaluationError::MissingData {
            field: "score".to_string(),
        };
        assert!(err.to_string().contains("score"));
    }

    // --- RepairError ---

    #[test]
    fn test_repair_error_max_attempts() {
        let id = Uuid::new_v4();
        let err = RepairError::MaxAttemptsExceeded {
            target_type: "job".to_string(),
            target_id: id,
            max: 3,
        };
        let msg = err.to_string();
        assert!(msg.contains("job"));
        assert!(msg.contains("3"));
    }

    // --- WorkspaceError ---

    #[test]
    fn test_workspace_error_document_not_found() {
        let err = WorkspaceError::DocumentNotFound {
            doc_type: "IDENTITY.md".to_string(),
            user_id: "user-1".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("IDENTITY.md"));
        assert!(msg.contains("user-1"));
    }

    // --- OrchestratorError ---

    #[test]
    fn test_orchestrator_error_container_timeout() {
        let id = Uuid::new_v4();
        let err = OrchestratorError::ContainerTimeout { job_id: id };
        assert!(err.to_string().contains(&id.to_string()));
    }

    // --- WorkerError ---

    #[test]
    fn test_worker_error_missing_token() {
        let err = WorkerError::MissingToken;
        assert!(err.to_string().contains("IRONCLAW_WORKER_TOKEN"));
    }

    #[test]
    fn test_worker_error_connection_failed() {
        let err = WorkerError::ConnectionFailed {
            url: "http://localhost:50051".to_string(),
            reason: "refused".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("localhost:50051"));
        assert!(msg.contains("refused"));
    }

    // --- HookError ---

    #[test]
    fn test_hook_error_timeout() {
        let err = HookError::Timeout {
            name: "rate_limit".to_string(),
            timeout_ms: 5000,
        };
        let msg = err.to_string();
        assert!(msg.contains("rate_limit"));
        assert!(msg.contains("5000"));
    }

    // --- MediaError ---

    #[test]
    fn test_media_error_too_large() {
        let err = MediaError::TooLarge {
            size: 1000000,
            max: 500000,
        };
        let msg = err.to_string();
        assert!(msg.contains("1000000"));
        assert!(msg.contains("500000"));
    }

    #[test]
    fn test_media_error_max_depth_exceeded() {
        let err = MediaError::MaxDepthExceeded { max_depth: 5 };
        assert!(err.to_string().contains("5"));
    }

    // --- SkillsError ---

    #[test]
    fn test_skills_error_not_found() {
        let err = SkillsError::NotFound {
            name: "summarize".to_string(),
        };
        assert!(err.to_string().contains("summarize"));
    }

    // --- From conversions into top-level Error ---

    #[test]
    fn test_error_from_config_error() {
        let inner = ConfigError::MissingEnvVar("TEST".to_string());
        let err = Error::from(inner);
        assert!(err.to_string().contains("Configuration error"));
    }

    #[test]
    fn test_error_from_database_error() {
        let inner = DatabaseError::Query("syntax error".to_string());
        let err = Error::from(inner);
        assert!(err.to_string().contains("Database error"));
    }

    #[test]
    fn test_error_from_tool_error() {
        let inner = ToolError::NotFound {
            name: "x".to_string(),
        };
        let err = Error::from(inner);
        assert!(err.to_string().contains("Tool error"));
    }

    #[test]
    fn test_error_from_safety_error() {
        let inner = SafetyError::PolicyViolation {
            rule: "no-exec".to_string(),
        };
        let err = Error::from(inner);
        assert!(err.to_string().contains("Safety error"));
    }

    #[test]
    fn test_error_from_job_error() {
        let inner = JobError::MaxJobsExceeded { max: 5 };
        let err = Error::from(inner);
        assert!(err.to_string().contains("Job error"));
    }

    // --- Debug trait ---

    #[test]
    fn test_error_debug_is_implemented() {
        let err = Error::Config(ConfigError::ParseError("test".to_string()));
        let debug = format!("{:?}", err);
        assert!(!debug.is_empty());
    }

    #[test]
    fn test_tool_error_debug_is_implemented() {
        let err = ToolError::BuilderFailed("oops".to_string());
        let debug = format!("{:?}", err);
        assert!(!debug.is_empty());
    }
}
