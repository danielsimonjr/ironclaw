//! Hook execution engine.
//!
//! Manages hook registration, ordering, and execution.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use super::types::{
    Hook, HookContext, HookError, HookEvent, HookOutcome, HookType, InboundHookResult,
    OutboundHookResult, ToolCallHookResult, TransformResponseResult,
};

/// Engine that manages and executes lifecycle hooks.
pub struct HookEngine {
    hooks: Arc<RwLock<HashMap<HookType, Vec<Hook>>>>,
}

impl HookEngine {
    /// Create a new hook engine.
    pub fn new() -> Self {
        Self {
            hooks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new hook.
    pub async fn register(&self, hook: Hook) -> Result<(), HookError> {
        if hook.name.is_empty() {
            return Err(HookError::RegistrationFailed {
                reason: "Hook name cannot be empty".to_string(),
            });
        }

        let mut hooks = self.hooks.write().await;
        let entry = hooks.entry(hook.hook_type).or_default();

        // Check for duplicate names within the same hook type
        if entry.iter().any(|h| h.name == hook.name) {
            return Err(HookError::RegistrationFailed {
                reason: format!(
                    "Hook '{}' already registered for {:?}",
                    hook.name, hook.hook_type
                ),
            });
        }

        entry.push(hook);
        // Sort by priority
        entry.sort_by_key(|h| h.priority);

        Ok(())
    }

    /// Unregister a hook by name.
    pub async fn unregister(&self, hook_type: HookType, name: &str) -> bool {
        let mut hooks = self.hooks.write().await;
        if let Some(entry) = hooks.get_mut(&hook_type) {
            let len_before = entry.len();
            entry.retain(|h| h.name != name);
            return entry.len() < len_before;
        }
        false
    }

    /// List all registered hooks.
    pub async fn list_hooks(&self) -> Vec<Hook> {
        let hooks = self.hooks.read().await;
        hooks.values().flatten().cloned().collect()
    }

    /// List hooks for a specific type.
    pub async fn list_hooks_by_type(&self, hook_type: HookType) -> Vec<Hook> {
        let hooks = self.hooks.read().await;
        hooks.get(&hook_type).cloned().unwrap_or_default()
    }

    /// Enable or disable a hook.
    pub async fn set_enabled(&self, hook_type: HookType, name: &str, enabled: bool) -> bool {
        let mut hooks = self.hooks.write().await;
        if let Some(entry) = hooks.get_mut(&hook_type)
            && let Some(hook) = entry.iter_mut().find(|h| h.name == name)
        {
            hook.enabled = enabled;
            return true;
        }
        false
    }

    /// Execute beforeInbound hooks.
    ///
    /// Returns whether to continue processing and any modified content.
    pub async fn run_before_inbound(
        &self,
        content: &str,
        sender: &str,
        ctx: &HookContext,
    ) -> Result<InboundHookResult, HookError> {
        let hooks = self.hooks.read().await;
        let entries = hooks.get(&HookType::BeforeInbound);

        let Some(entries) = entries else {
            return Ok(InboundHookResult {
                allow: true,
                modified_content: None,
                block_reason: None,
            });
        };

        let mut current_content = content.to_string();

        for hook in entries.iter().filter(|h| h.enabled) {
            let event = HookEvent::InboundMessage {
                content: current_content.clone(),
                sender: sender.to_string(),
            };

            match self.execute_hook(hook, &event, ctx).await? {
                HookOutcome::Continue => {}
                HookOutcome::Modified(value) => {
                    if let Some(new_content) = value.as_str() {
                        current_content = new_content.to_string();
                    }
                }
                HookOutcome::Block { reason } => {
                    return Ok(InboundHookResult {
                        allow: false,
                        modified_content: None,
                        block_reason: Some(reason),
                    });
                }
                HookOutcome::Error { message } => {
                    tracing::warn!(
                        hook = hook.name,
                        error = message,
                        "beforeInbound hook error"
                    );
                }
            }
        }

        let modified = if current_content != content {
            Some(current_content)
        } else {
            None
        };

        Ok(InboundHookResult {
            allow: true,
            modified_content: modified,
            block_reason: None,
        })
    }

    /// Execute beforeOutbound hooks.
    pub async fn run_before_outbound(
        &self,
        content: &str,
        ctx: &HookContext,
    ) -> Result<OutboundHookResult, HookError> {
        let hooks = self.hooks.read().await;
        let entries = hooks.get(&HookType::BeforeOutbound);

        let Some(entries) = entries else {
            return Ok(OutboundHookResult {
                allow: true,
                modified_content: None,
                block_reason: None,
            });
        };

        let mut current_content = content.to_string();

        for hook in entries.iter().filter(|h| h.enabled) {
            let event = HookEvent::OutboundResponse {
                content: current_content.clone(),
            };

            match self.execute_hook(hook, &event, ctx).await? {
                HookOutcome::Continue => {}
                HookOutcome::Modified(value) => {
                    if let Some(new_content) = value.as_str() {
                        current_content = new_content.to_string();
                    }
                }
                HookOutcome::Block { reason } => {
                    return Ok(OutboundHookResult {
                        allow: false,
                        modified_content: None,
                        block_reason: Some(reason),
                    });
                }
                HookOutcome::Error { message } => {
                    tracing::warn!(
                        hook = hook.name,
                        error = message,
                        "beforeOutbound hook error"
                    );
                }
            }
        }

        let modified = if current_content != content {
            Some(current_content)
        } else {
            None
        };

        Ok(OutboundHookResult {
            allow: true,
            modified_content: modified,
            block_reason: None,
        })
    }

    /// Execute beforeToolCall hooks.
    pub async fn run_before_tool_call(
        &self,
        tool_name: &str,
        params: &serde_json::Value,
        ctx: &HookContext,
    ) -> Result<ToolCallHookResult, HookError> {
        let hooks = self.hooks.read().await;
        let entries = hooks.get(&HookType::BeforeToolCall);

        let Some(entries) = entries else {
            return Ok(ToolCallHookResult {
                allow: true,
                modified_params: None,
                block_reason: None,
            });
        };

        let mut current_params = params.clone();

        for hook in entries.iter().filter(|h| h.enabled) {
            let event = HookEvent::ToolCall {
                tool_name: tool_name.to_string(),
                parameters: current_params.clone(),
            };

            match self.execute_hook(hook, &event, ctx).await? {
                HookOutcome::Continue => {}
                HookOutcome::Modified(value) => {
                    current_params = value;
                }
                HookOutcome::Block { reason } => {
                    return Ok(ToolCallHookResult {
                        allow: false,
                        modified_params: None,
                        block_reason: Some(reason),
                    });
                }
                HookOutcome::Error { message } => {
                    tracing::warn!(
                        hook = hook.name,
                        error = message,
                        "beforeToolCall hook error"
                    );
                }
            }
        }

        let modified = if current_params != *params {
            Some(current_params)
        } else {
            None
        };

        Ok(ToolCallHookResult {
            allow: true,
            modified_params: modified,
            block_reason: None,
        })
    }

    /// Execute onSessionStart hooks (fire-and-forget).
    pub async fn run_on_session_start(&self, session_id: &str, ctx: &HookContext) {
        let hooks = self.hooks.read().await;
        let entries = hooks.get(&HookType::OnSessionStart);

        let Some(entries) = entries else {
            return;
        };

        for hook in entries.iter().filter(|h| h.enabled) {
            let event = HookEvent::SessionStart {
                session_id: session_id.to_string(),
            };

            if let Err(e) = self.execute_hook(hook, &event, ctx).await {
                tracing::warn!(
                    hook = hook.name,
                    error = %e,
                    "onSessionStart hook error"
                );
            }
        }
    }

    /// Execute onSessionEnd hooks (fire-and-forget).
    pub async fn run_on_session_end(&self, session_id: &str, reason: &str, ctx: &HookContext) {
        let hooks = self.hooks.read().await;
        let entries = hooks.get(&HookType::OnSessionEnd);

        let Some(entries) = entries else {
            return;
        };

        for hook in entries.iter().filter(|h| h.enabled) {
            let event = HookEvent::SessionEnd {
                session_id: session_id.to_string(),
                reason: reason.to_string(),
            };

            if let Err(e) = self.execute_hook(hook, &event, ctx).await {
                tracing::warn!(
                    hook = hook.name,
                    error = %e,
                    "onSessionEnd hook error"
                );
            }
        }
    }

    /// Execute transformResponse hooks.
    pub async fn run_transform_response(
        &self,
        content: &str,
        ctx: &HookContext,
    ) -> Result<TransformResponseResult, HookError> {
        let hooks = self.hooks.read().await;
        let entries = hooks.get(&HookType::TransformResponse);

        let Some(entries) = entries else {
            return Ok(TransformResponseResult {
                content: content.to_string(),
            });
        };

        let mut current_content = content.to_string();

        for hook in entries.iter().filter(|h| h.enabled) {
            let event = HookEvent::TransformResponse {
                content: current_content.clone(),
            };

            match self.execute_hook(hook, &event, ctx).await? {
                HookOutcome::Continue => {}
                HookOutcome::Modified(value) => {
                    if let Some(new_content) = value.as_str() {
                        current_content = new_content.to_string();
                    }
                }
                HookOutcome::Block { .. } | HookOutcome::Error { .. } => {
                    // transformResponse hooks can't block, only modify
                }
            }
        }

        Ok(TransformResponseResult {
            content: current_content,
        })
    }

    /// Execute a single hook and return its outcome.
    async fn execute_hook(
        &self,
        hook: &Hook,
        event: &HookEvent,
        _ctx: &HookContext,
    ) -> Result<HookOutcome, HookError> {
        let timeout = tokio::time::Duration::from_millis(hook.timeout_ms);

        let result = tokio::time::timeout(timeout, async {
            match &hook.action {
                super::types::HookAction::Shell { command } => {
                    self.execute_shell_hook(command, event).await
                }
                super::types::HookAction::Http { url, method } => {
                    self.execute_http_hook(url, method, event).await
                }
                super::types::HookAction::Inline { code } => {
                    self.execute_inline_hook(code, event).await
                }
                super::types::HookAction::Webhook { url } => {
                    self.execute_webhook_hook(url, event).await
                }
            }
        })
        .await;

        match result {
            Ok(outcome) => outcome,
            Err(_) => Err(HookError::Timeout {
                name: hook.name.clone(),
                timeout_ms: hook.timeout_ms,
            }),
        }
    }

    async fn execute_shell_hook(
        &self,
        command: &str,
        event: &HookEvent,
    ) -> Result<HookOutcome, HookError> {
        let event_json = serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string());

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .env("HOOK_EVENT", &event_json)
            .output()
            .await
            .map_err(|e| HookError::ExecutionFailed {
                name: command.to_string(),
                reason: e.to_string(),
            })?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim().is_empty() {
                Ok(HookOutcome::Continue)
            } else {
                // Try to parse as JSON outcome
                match serde_json::from_str::<HookOutcome>(stdout.trim()) {
                    Ok(outcome) => Ok(outcome),
                    Err(_) => Ok(HookOutcome::Modified(serde_json::Value::String(
                        stdout.trim().to_string(),
                    ))),
                }
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Ok(HookOutcome::Error {
                message: stderr.to_string(),
            })
        }
    }

    async fn execute_http_hook(
        &self,
        url: &str,
        method: &str,
        event: &HookEvent,
    ) -> Result<HookOutcome, HookError> {
        let client = reqwest::Client::new();
        let request = match method.to_uppercase().as_str() {
            "POST" => client.post(url).json(event),
            "PUT" => client.put(url).json(event),
            _ => client.post(url).json(event),
        };

        let response = request
            .send()
            .await
            .map_err(|e| HookError::ExecutionFailed {
                name: url.to_string(),
                reason: e.to_string(),
            })?;

        if response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            if body.trim().is_empty() {
                Ok(HookOutcome::Continue)
            } else {
                match serde_json::from_str::<HookOutcome>(&body) {
                    Ok(outcome) => Ok(outcome),
                    Err(_) => Ok(HookOutcome::Continue),
                }
            }
        } else {
            Ok(HookOutcome::Error {
                message: format!("HTTP {} returned {}", method, response.status()),
            })
        }
    }

    async fn execute_inline_hook(
        &self,
        code: &str,
        event: &HookEvent,
    ) -> Result<HookOutcome, HookError> {
        // Simple template evaluation: supports {{content}} substitution
        let result = match event {
            HookEvent::InboundMessage { content, .. } => code.replace("{{content}}", content),
            HookEvent::OutboundResponse { content } => code.replace("{{content}}", content),
            HookEvent::TransformResponse { content } => code.replace("{{content}}", content),
            _ => code.to_string(),
        };

        if result == code {
            Ok(HookOutcome::Continue)
        } else {
            Ok(HookOutcome::Modified(serde_json::Value::String(result)))
        }
    }

    async fn execute_webhook_hook(
        &self,
        url: &str,
        event: &HookEvent,
    ) -> Result<HookOutcome, HookError> {
        self.execute_http_hook(url, "POST", event).await
    }
}

impl Default for HookEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::types::*;

    fn test_context() -> HookContext {
        HookContext {
            event: HookEvent::InboundMessage {
                content: "test".to_string(),
                sender: "user1".to_string(),
            },
            user_id: "user1".to_string(),
            channel: "test".to_string(),
            thread_id: None,
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_register_and_list_hooks() {
        let engine = HookEngine::new();

        let hook = Hook {
            name: "test_hook".to_string(),
            description: "A test hook".to_string(),
            hook_type: HookType::BeforeInbound,
            action: HookAction::Inline {
                code: "pass".to_string(),
            },
            priority: HookPriority::Normal,
            source: HookSource::Config,
            enabled: true,
            timeout_ms: 5000,
        };

        engine.register(hook).await.unwrap();
        let hooks = engine.list_hooks().await;
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].name, "test_hook");
    }

    #[tokio::test]
    async fn test_unregister_hook() {
        let engine = HookEngine::new();

        let hook = Hook {
            name: "to_remove".to_string(),
            description: "Will be removed".to_string(),
            hook_type: HookType::BeforeInbound,
            action: HookAction::Inline {
                code: "pass".to_string(),
            },
            priority: HookPriority::Normal,
            source: HookSource::Config,
            enabled: true,
            timeout_ms: 5000,
        };

        engine.register(hook).await.unwrap();
        assert!(
            engine
                .unregister(HookType::BeforeInbound, "to_remove")
                .await
        );
        assert!(engine.list_hooks().await.is_empty());
    }

    #[tokio::test]
    async fn test_no_hooks_returns_allow() {
        let engine = HookEngine::new();
        let ctx = test_context();

        let result = engine
            .run_before_inbound("hello", "user1", &ctx)
            .await
            .unwrap();
        assert!(result.allow);
        assert!(result.modified_content.is_none());
    }

    #[tokio::test]
    async fn test_duplicate_hook_name_rejected() {
        let engine = HookEngine::new();

        let hook = Hook {
            name: "dup".to_string(),
            hook_type: HookType::BeforeInbound,
            ..Hook::default()
        };

        engine.register(hook.clone()).await.unwrap();
        assert!(engine.register(hook).await.is_err());
    }
}
