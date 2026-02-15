//! Hook types and data structures.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Error type for hook operations.
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    #[error("Hook {name} failed: {reason}")]
    ExecutionFailed { name: String, reason: String },

    #[error("Hook {name} timed out after {timeout_ms}ms")]
    Timeout { name: String, timeout_ms: u64 },

    #[error("Hook {name} returned invalid result: {reason}")]
    InvalidResult { name: String, reason: String },

    #[error("Hook registration failed: {reason}")]
    RegistrationFailed { reason: String },
}

/// Types of lifecycle hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HookType {
    /// Fires before an inbound message is processed.
    BeforeInbound,
    /// Fires before a response is sent outbound.
    BeforeOutbound,
    /// Fires before a tool call is executed.
    BeforeToolCall,
    /// Fires when a new session starts.
    OnSessionStart,
    /// Fires when a session ends.
    OnSessionEnd,
    /// Fires to transform the response before sending.
    TransformResponse,
    /// Fires to transcribe audio content.
    TranscribeAudio,
}

impl fmt::Display for HookType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BeforeInbound => write!(f, "beforeInbound"),
            Self::BeforeOutbound => write!(f, "beforeOutbound"),
            Self::BeforeToolCall => write!(f, "beforeToolCall"),
            Self::OnSessionStart => write!(f, "onSessionStart"),
            Self::OnSessionEnd => write!(f, "onSessionEnd"),
            Self::TransformResponse => write!(f, "transformResponse"),
            Self::TranscribeAudio => write!(f, "transcribeAudio"),
        }
    }
}

/// Priority level for hook execution ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum HookPriority {
    /// Runs first (system-level hooks).
    System = 0,
    /// Runs early (important plugins).
    High = 10,
    /// Default priority.
    #[default]
    Normal = 50,
    /// Runs late (logging, analytics).
    Low = 90,
}

/// Where the hook originates from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookSource {
    /// Built-in hook (part of the core system).
    Builtin,
    /// Hook from a plugin/extension.
    Plugin { name: String },
    /// Hook defined in workspace (inline code).
    Workspace { path: String },
    /// Hook from configuration file.
    Config,
}

/// What action a hook performs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookAction {
    /// Execute a shell command.
    Shell { command: String },
    /// Call an HTTP endpoint.
    Http { url: String, method: String },
    /// Evaluate inline code (simple template expressions).
    Inline { code: String },
    /// Send to a webhook URL.
    Webhook { url: String },
}

/// Context passed to hooks when they fire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookContext {
    /// The hook event type.
    pub event: HookEvent,
    /// User ID of the session.
    pub user_id: String,
    /// Channel the message came from.
    pub channel: String,
    /// Thread ID if applicable.
    pub thread_id: Option<String>,
    /// Additional metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Events that trigger hooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookEvent {
    /// An inbound message was received.
    InboundMessage { content: String, sender: String },
    /// An outbound response is about to be sent.
    OutboundResponse { content: String },
    /// A tool call is about to be executed.
    ToolCall {
        tool_name: String,
        parameters: serde_json::Value,
    },
    /// A session has started.
    SessionStart { session_id: String },
    /// A session has ended.
    SessionEnd { session_id: String, reason: String },
    /// Transform a response.
    TransformResponse { content: String },
    /// Transcribe audio.
    TranscribeAudio {
        audio_url: String,
        mime_type: String,
    },
}

/// Outcome of a hook execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HookOutcome {
    /// Continue processing normally.
    Continue,
    /// Continue with modified data.
    Modified(serde_json::Value),
    /// Skip/block the operation.
    Block { reason: String },
    /// An error occurred.
    Error { message: String },
}

/// A registered hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    /// Unique name for this hook.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// When this hook fires.
    pub hook_type: HookType,
    /// What the hook does.
    pub action: HookAction,
    /// Execution priority.
    pub priority: HookPriority,
    /// Where this hook came from.
    pub source: HookSource,
    /// Whether the hook is currently enabled.
    pub enabled: bool,
    /// Maximum execution time in milliseconds.
    pub timeout_ms: u64,
}

impl Default for Hook {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            hook_type: HookType::BeforeInbound,
            action: HookAction::Inline {
                code: String::new(),
            },
            priority: HookPriority::Normal,
            source: HookSource::Config,
            enabled: true,
            timeout_ms: 5000,
        }
    }
}

/// Registration request for a new hook.
#[derive(Debug, Clone)]
pub struct HookRegistration {
    pub hook: Hook,
}

/// Result of a beforeInbound hook.
#[derive(Debug, Clone)]
pub struct InboundHookResult {
    /// Whether to continue processing.
    pub allow: bool,
    /// Modified message content (if changed).
    pub modified_content: Option<String>,
    /// Reason if blocked.
    pub block_reason: Option<String>,
}

/// Result of a beforeOutbound hook.
#[derive(Debug, Clone)]
pub struct OutboundHookResult {
    /// Whether to send the response.
    pub allow: bool,
    /// Modified response content (if changed).
    pub modified_content: Option<String>,
    /// Reason if blocked.
    pub block_reason: Option<String>,
}

/// Result of a beforeToolCall hook.
#[derive(Debug, Clone)]
pub struct ToolCallHookResult {
    /// Whether to execute the tool.
    pub allow: bool,
    /// Modified parameters (if changed).
    pub modified_params: Option<serde_json::Value>,
    /// Reason if blocked.
    pub block_reason: Option<String>,
}

/// Result of a transformResponse hook.
#[derive(Debug, Clone)]
pub struct TransformResponseResult {
    /// The transformed response content.
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_type_display() {
        assert_eq!(HookType::BeforeInbound.to_string(), "beforeInbound");
        assert_eq!(HookType::BeforeOutbound.to_string(), "beforeOutbound");
        assert_eq!(HookType::BeforeToolCall.to_string(), "beforeToolCall");
        assert_eq!(HookType::OnSessionStart.to_string(), "onSessionStart");
        assert_eq!(HookType::OnSessionEnd.to_string(), "onSessionEnd");
        assert_eq!(HookType::TransformResponse.to_string(), "transformResponse");
    }

    #[test]
    fn test_hook_priority_ordering() {
        assert!(HookPriority::System < HookPriority::High);
        assert!(HookPriority::High < HookPriority::Normal);
        assert!(HookPriority::Normal < HookPriority::Low);
    }

    #[test]
    fn test_default_hook() {
        let hook = Hook::default();
        assert!(hook.enabled);
        assert_eq!(hook.timeout_ms, 5000);
        assert_eq!(hook.priority, HookPriority::Normal);
    }
}
