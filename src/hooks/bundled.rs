//! Bundled hooks — pre-built lifecycle hooks shipped with IronClaw.
//!
//! These hooks provide common functionality that users can enable
//! without writing custom code.

use crate::hooks::types::{Hook, HookAction, HookPriority, HookSource, HookType};

/// Register all bundled hooks with the hook engine.
pub async fn register_bundled_hooks(engine: &crate::hooks::HookEngine) {
    for hook in all_bundled_hooks() {
        if let Err(e) = engine.register(hook.clone()).await {
            tracing::warn!(hook = hook.name, error = %e, "Failed to register bundled hook");
        }
    }
}

/// Get all available bundled hooks (not yet registered).
pub fn all_bundled_hooks() -> Vec<Hook> {
    vec![
        profanity_filter(),
        response_length_guard(),
        sensitive_data_redactor(),
        greeting_injector(),
        session_logger(),
        rate_limit_guard(),
        language_detector(),
        tool_usage_logger(),
    ]
}

/// Hook: Filter profanity from inbound messages.
pub fn profanity_filter() -> Hook {
    Hook {
        name: "builtin:profanity_filter".to_string(),
        description: "Replaces common profanity in inbound messages with asterisks".to_string(),
        hook_type: HookType::BeforeInbound,
        action: HookAction::Inline {
            code: "{{content}}".to_string(),
        },
        priority: HookPriority::High,
        source: HookSource::Builtin,
        enabled: false, // Disabled by default, user opts in
        timeout_ms: 1000,
    }
}

/// Hook: Guard against excessively long responses.
pub fn response_length_guard() -> Hook {
    Hook {
        name: "builtin:response_length_guard".to_string(),
        description: "Truncates outbound responses that exceed a configurable character limit"
            .to_string(),
        hook_type: HookType::BeforeOutbound,
        action: HookAction::Inline {
            code: "{{content}}".to_string(),
        },
        priority: HookPriority::Normal,
        source: HookSource::Builtin,
        enabled: false,
        timeout_ms: 500,
    }
}

/// Hook: Redact sensitive patterns from outbound responses.
pub fn sensitive_data_redactor() -> Hook {
    Hook {
        name: "builtin:sensitive_data_redactor".to_string(),
        description:
            "Redacts sensitive patterns (emails, phone numbers, API keys) from outbound responses"
                .to_string(),
        hook_type: HookType::BeforeOutbound,
        action: HookAction::Inline {
            code: "{{content}}".to_string(),
        },
        priority: HookPriority::High,
        source: HookSource::Builtin,
        enabled: false,
        timeout_ms: 1000,
    }
}

/// Hook: Add a greeting to new session starts.
pub fn greeting_injector() -> Hook {
    Hook {
        name: "builtin:greeting_injector".to_string(),
        description: "Sends a configurable greeting message when a new session starts".to_string(),
        hook_type: HookType::OnSessionStart,
        action: HookAction::Inline {
            code: "Welcome! How can I help you today?".to_string(),
        },
        priority: HookPriority::Normal,
        source: HookSource::Builtin,
        enabled: false,
        timeout_ms: 500,
    }
}

/// Hook: Log session start/end events.
pub fn session_logger() -> Hook {
    Hook {
        name: "builtin:session_logger".to_string(),
        description: "Logs session start and end events for analytics and debugging".to_string(),
        hook_type: HookType::OnSessionEnd,
        action: HookAction::Inline {
            code: String::new(),
        },
        priority: HookPriority::Low,
        source: HookSource::Builtin,
        enabled: false,
        timeout_ms: 500,
    }
}

/// Hook: Rate limit inbound messages per sender.
pub fn rate_limit_guard() -> Hook {
    Hook {
        name: "builtin:rate_limit_guard".to_string(),
        description: "Blocks inbound messages when a sender exceeds the configured rate limit"
            .to_string(),
        hook_type: HookType::BeforeInbound,
        action: HookAction::Inline {
            code: "{{content}}".to_string(),
        },
        priority: HookPriority::System,
        source: HookSource::Builtin,
        enabled: false,
        timeout_ms: 200,
    }
}

/// Hook: Detect and tag message language.
pub fn language_detector() -> Hook {
    Hook {
        name: "builtin:language_detector".to_string(),
        description: "Detects the language of inbound messages and tags them with metadata"
            .to_string(),
        hook_type: HookType::BeforeInbound,
        action: HookAction::Inline {
            code: "{{content}}".to_string(),
        },
        priority: HookPriority::Normal,
        source: HookSource::Builtin,
        enabled: false,
        timeout_ms: 1000,
    }
}

/// Hook: Log tool usage for analytics.
pub fn tool_usage_logger() -> Hook {
    Hook {
        name: "builtin:tool_usage_logger".to_string(),
        description: "Logs tool invocations with parameters and timing for analytics".to_string(),
        hook_type: HookType::BeforeToolCall,
        action: HookAction::Inline {
            code: String::new(),
        },
        priority: HookPriority::Low,
        source: HookSource::Builtin,
        enabled: false,
        timeout_ms: 500,
    }
}

/// Get a bundled hook by name.
pub fn get_bundled_hook(name: &str) -> Option<Hook> {
    all_bundled_hooks().into_iter().find(|h| h.name == name)
}

/// List all bundled hook names.
pub fn list_bundled_hook_names() -> Vec<&'static str> {
    vec![
        "builtin:profanity_filter",
        "builtin:response_length_guard",
        "builtin:sensitive_data_redactor",
        "builtin:greeting_injector",
        "builtin:session_logger",
        "builtin:rate_limit_guard",
        "builtin:language_detector",
        "builtin:tool_usage_logger",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_bundled_hooks_count() {
        let hooks = all_bundled_hooks();
        assert_eq!(hooks.len(), 8);
    }

    #[test]
    fn test_all_bundled_hooks_have_valid_names() {
        for hook in all_bundled_hooks() {
            assert!(!hook.name.is_empty(), "Hook name must not be empty");
            assert!(
                hook.name.starts_with("builtin:"),
                "Bundled hook '{}' must start with 'builtin:' prefix",
                hook.name
            );
        }
    }

    #[test]
    fn test_all_bundled_hooks_have_descriptions() {
        for hook in all_bundled_hooks() {
            assert!(
                !hook.description.is_empty(),
                "Hook '{}' must have a description",
                hook.name
            );
        }
    }

    #[test]
    fn test_all_bundled_hooks_have_builtin_source() {
        for hook in all_bundled_hooks() {
            assert_eq!(
                hook.source,
                HookSource::Builtin,
                "Hook '{}' must have Builtin source",
                hook.name
            );
        }
    }

    #[test]
    fn test_all_bundled_hooks_disabled_by_default() {
        for hook in all_bundled_hooks() {
            assert!(
                !hook.enabled,
                "Bundled hook '{}' must be disabled by default",
                hook.name
            );
        }
    }

    #[test]
    fn test_all_bundled_hooks_have_unique_names() {
        let hooks = all_bundled_hooks();
        let mut names: Vec<&str> = hooks.iter().map(|h| h.name.as_str()).collect();
        let total = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), total, "All bundled hook names must be unique");
    }

    #[test]
    fn test_bundled_hook_lookup_by_name() {
        let hook = get_bundled_hook("builtin:profanity_filter");
        assert!(hook.is_some());
        let hook = hook.unwrap();
        assert_eq!(hook.name, "builtin:profanity_filter");
        assert_eq!(hook.hook_type, HookType::BeforeInbound);
    }

    #[test]
    fn test_bundled_hook_lookup_not_found() {
        let hook = get_bundled_hook("builtin:nonexistent");
        assert!(hook.is_none());
    }

    #[test]
    fn test_bundled_hook_lookup_empty_name() {
        let hook = get_bundled_hook("");
        assert!(hook.is_none());
    }

    #[test]
    fn test_list_bundled_hook_names() {
        let names = list_bundled_hook_names();
        assert_eq!(names.len(), 8);
        assert!(names.contains(&"builtin:profanity_filter"));
        assert!(names.contains(&"builtin:response_length_guard"));
        assert!(names.contains(&"builtin:sensitive_data_redactor"));
        assert!(names.contains(&"builtin:greeting_injector"));
        assert!(names.contains(&"builtin:session_logger"));
        assert!(names.contains(&"builtin:rate_limit_guard"));
        assert!(names.contains(&"builtin:language_detector"));
        assert!(names.contains(&"builtin:tool_usage_logger"));
    }

    #[test]
    fn test_list_bundled_hook_names_matches_hooks() {
        let names = list_bundled_hook_names();
        let hooks = all_bundled_hooks();
        assert_eq!(names.len(), hooks.len());
        for hook in &hooks {
            assert!(
                names.contains(&hook.name.as_str()),
                "Hook '{}' missing from list_bundled_hook_names()",
                hook.name
            );
        }
    }

    #[test]
    fn test_profanity_filter_details() {
        let hook = profanity_filter();
        assert_eq!(hook.hook_type, HookType::BeforeInbound);
        assert_eq!(hook.priority, HookPriority::High);
        assert_eq!(hook.source, HookSource::Builtin);
        assert!(!hook.enabled);
        assert_eq!(hook.timeout_ms, 1000);
    }

    #[test]
    fn test_response_length_guard_details() {
        let hook = response_length_guard();
        assert_eq!(hook.hook_type, HookType::BeforeOutbound);
        assert_eq!(hook.priority, HookPriority::Normal);
        assert!(!hook.enabled);
    }

    #[test]
    fn test_sensitive_data_redactor_details() {
        let hook = sensitive_data_redactor();
        assert_eq!(hook.hook_type, HookType::BeforeOutbound);
        assert_eq!(hook.priority, HookPriority::High);
        assert!(!hook.enabled);
    }

    #[test]
    fn test_greeting_injector_details() {
        let hook = greeting_injector();
        assert_eq!(hook.hook_type, HookType::OnSessionStart);
        assert_eq!(hook.priority, HookPriority::Normal);
        assert!(!hook.enabled);
    }

    #[test]
    fn test_session_logger_details() {
        let hook = session_logger();
        assert_eq!(hook.hook_type, HookType::OnSessionEnd);
        assert_eq!(hook.priority, HookPriority::Low);
        assert!(!hook.enabled);
    }

    #[test]
    fn test_rate_limit_guard_details() {
        let hook = rate_limit_guard();
        assert_eq!(hook.hook_type, HookType::BeforeInbound);
        assert_eq!(hook.priority, HookPriority::System);
        assert!(!hook.enabled);
        assert_eq!(hook.timeout_ms, 200);
    }

    #[test]
    fn test_language_detector_details() {
        let hook = language_detector();
        assert_eq!(hook.hook_type, HookType::BeforeInbound);
        assert_eq!(hook.priority, HookPriority::Normal);
        assert!(!hook.enabled);
    }

    #[test]
    fn test_tool_usage_logger_details() {
        let hook = tool_usage_logger();
        assert_eq!(hook.hook_type, HookType::BeforeToolCall);
        assert_eq!(hook.priority, HookPriority::Low);
        assert!(!hook.enabled);
    }

    #[test]
    fn test_before_inbound_hooks_present() {
        let hooks = all_bundled_hooks();
        let inbound: Vec<_> = hooks
            .iter()
            .filter(|h| h.hook_type == HookType::BeforeInbound)
            .collect();
        assert!(
            !inbound.is_empty(),
            "There must be at least one BeforeInbound bundled hook"
        );
    }

    #[test]
    fn test_before_outbound_hooks_present() {
        let hooks = all_bundled_hooks();
        let outbound: Vec<_> = hooks
            .iter()
            .filter(|h| h.hook_type == HookType::BeforeOutbound)
            .collect();
        assert!(
            !outbound.is_empty(),
            "There must be at least one BeforeOutbound bundled hook"
        );
    }

    #[test]
    fn test_session_hooks_present() {
        let hooks = all_bundled_hooks();
        let session_start: Vec<_> = hooks
            .iter()
            .filter(|h| h.hook_type == HookType::OnSessionStart)
            .collect();
        let session_end: Vec<_> = hooks
            .iter()
            .filter(|h| h.hook_type == HookType::OnSessionEnd)
            .collect();
        assert!(
            !session_start.is_empty(),
            "There must be at least one OnSessionStart bundled hook"
        );
        assert!(
            !session_end.is_empty(),
            "There must be at least one OnSessionEnd bundled hook"
        );
    }

    #[test]
    fn test_before_tool_call_hooks_present() {
        let hooks = all_bundled_hooks();
        let tool_call: Vec<_> = hooks
            .iter()
            .filter(|h| h.hook_type == HookType::BeforeToolCall)
            .collect();
        assert!(
            !tool_call.is_empty(),
            "There must be at least one BeforeToolCall bundled hook"
        );
    }

    #[test]
    fn test_all_bundled_hooks_have_positive_timeout() {
        for hook in all_bundled_hooks() {
            assert!(
                hook.timeout_ms > 0,
                "Hook '{}' must have a positive timeout",
                hook.name
            );
        }
    }

    #[tokio::test]
    async fn test_register_bundled_hooks() {
        let engine = crate::hooks::HookEngine::new();
        register_bundled_hooks(&engine).await;
        let registered = engine.list_hooks().await;
        assert_eq!(registered.len(), 8);
    }

    #[tokio::test]
    async fn test_register_bundled_hooks_idempotent() {
        let engine = crate::hooks::HookEngine::new();
        register_bundled_hooks(&engine).await;
        // Second registration should warn but not panic
        register_bundled_hooks(&engine).await;
        let registered = engine.list_hooks().await;
        // Should still be 8 since duplicates are rejected
        assert_eq!(registered.len(), 8);
    }
}
