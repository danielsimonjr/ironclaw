//! Configuration editor for the web UI.
//!
//! Provides a structured view of all configuration sections and fields,
//! with validation and persistence through the database settings system.

use serde_json::Value;

use crate::channels::web::types::*;

/// Build the full configuration schema with current values.
pub fn build_config_schema(settings: &std::collections::HashMap<String, Value>) -> ConfigResponse {
    ConfigResponse {
        sections: vec![
            build_agent_section(settings),
            build_llm_section(settings),
            build_safety_section(settings),
            build_gateway_section(settings),
            build_heartbeat_section(settings),
            build_routine_section(settings),
            build_sandbox_section(settings),
            build_channels_section(settings),
            build_embeddings_section(settings),
        ],
    }
}

fn get_or_default(
    settings: &std::collections::HashMap<String, Value>,
    key: &str,
    default: Value,
) -> Value {
    settings.get(key).cloned().unwrap_or(default)
}

fn build_agent_section(settings: &std::collections::HashMap<String, Value>) -> ConfigSection {
    ConfigSection {
        name: "agent".to_string(),
        description: "Agent behavior settings".to_string(),
        fields: vec![
            ConfigField {
                key: "agent.name".to_string(),
                label: "Agent Name".to_string(),
                description: "Display name for the agent".to_string(),
                field_type: ConfigFieldType::String,
                value: get_or_default(
                    settings,
                    "agent.name",
                    Value::String("IronClaw".to_string()),
                ),
                default_value: Value::String("IronClaw".to_string()),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "agent.max_parallel_jobs".to_string(),
                label: "Max Parallel Jobs".to_string(),
                description: "Maximum number of concurrent jobs".to_string(),
                field_type: ConfigFieldType::Integer,
                value: get_or_default(settings, "agent.max_parallel_jobs", Value::Number(3.into())),
                default_value: Value::Number(3.into()),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "agent.job_timeout_secs".to_string(),
                label: "Job Timeout".to_string(),
                description: "Maximum time for a job in seconds".to_string(),
                field_type: ConfigFieldType::Duration,
                value: get_or_default(
                    settings,
                    "agent.job_timeout_secs",
                    Value::Number(600.into()),
                ),
                default_value: Value::Number(600.into()),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "agent.use_planning".to_string(),
                label: "Enable Planning".to_string(),
                description: "Use planning phase before tool execution".to_string(),
                field_type: ConfigFieldType::Boolean,
                value: get_or_default(settings, "agent.use_planning", Value::Bool(true)),
                default_value: Value::Bool(true),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "agent.stuck_threshold_secs".to_string(),
                label: "Stuck Threshold".to_string(),
                description: "Seconds before a job is considered stuck".to_string(),
                field_type: ConfigFieldType::Duration,
                value: get_or_default(
                    settings,
                    "agent.stuck_threshold_secs",
                    Value::Number(300.into()),
                ),
                default_value: Value::Number(300.into()),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "agent.session_idle_timeout_secs".to_string(),
                label: "Session Idle Timeout".to_string(),
                description: "Seconds before an idle session is pruned".to_string(),
                field_type: ConfigFieldType::Duration,
                value: get_or_default(
                    settings,
                    "agent.session_idle_timeout_secs",
                    Value::Number(1800.into()),
                ),
                default_value: Value::Number(1800.into()),
                required: false,
                sensitive: false,
            },
        ],
    }
}

fn build_llm_section(settings: &std::collections::HashMap<String, Value>) -> ConfigSection {
    ConfigSection {
        name: "llm".to_string(),
        description: "LLM provider configuration".to_string(),
        fields: vec![
            ConfigField {
                key: "llm.backend".to_string(),
                label: "LLM Backend".to_string(),
                description: "Which LLM provider to use".to_string(),
                field_type: ConfigFieldType::Select {
                    options: vec![
                        "nearai".to_string(),
                        "openai".to_string(),
                        "anthropic".to_string(),
                        "ollama".to_string(),
                        "openai_compatible".to_string(),
                    ],
                },
                value: get_or_default(settings, "llm.backend", Value::String("nearai".to_string())),
                default_value: Value::String("nearai".to_string()),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "llm.model".to_string(),
                label: "Model".to_string(),
                description: "Model name or identifier".to_string(),
                field_type: ConfigFieldType::String,
                value: get_or_default(settings, "llm.model", Value::String(String::new())),
                default_value: Value::String(String::new()),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "llm.api_key".to_string(),
                label: "API Key".to_string(),
                description: "API key for the selected LLM provider".to_string(),
                field_type: ConfigFieldType::Secret,
                value: get_or_default(settings, "llm.api_key", Value::String(String::new())),
                default_value: Value::String(String::new()),
                required: false,
                sensitive: true,
            },
            ConfigField {
                key: "llm.base_url".to_string(),
                label: "Base URL".to_string(),
                description: "API base URL (for OpenAI-compatible providers)".to_string(),
                field_type: ConfigFieldType::Url,
                value: get_or_default(settings, "llm.base_url", Value::String(String::new())),
                default_value: Value::String(String::new()),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "llm.nearai_session_token".to_string(),
                label: "NEAR AI Session Token".to_string(),
                description: "Session token for NEAR AI authentication".to_string(),
                field_type: ConfigFieldType::Secret,
                value: get_or_default(
                    settings,
                    "llm.nearai_session_token",
                    Value::String(String::new()),
                ),
                default_value: Value::String(String::new()),
                required: false,
                sensitive: true,
            },
        ],
    }
}

fn build_safety_section(settings: &std::collections::HashMap<String, Value>) -> ConfigSection {
    ConfigSection {
        name: "safety".to_string(),
        description: "Safety layer settings".to_string(),
        fields: vec![
            ConfigField {
                key: "safety.max_output_length".to_string(),
                label: "Max Output Length".to_string(),
                description: "Maximum length of tool output in characters".to_string(),
                field_type: ConfigFieldType::Integer,
                value: get_or_default(
                    settings,
                    "safety.max_output_length",
                    Value::Number(100_000.into()),
                ),
                default_value: Value::Number(100_000.into()),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "safety.injection_check_enabled".to_string(),
                label: "Injection Check".to_string(),
                description: "Enable prompt injection detection on tool outputs".to_string(),
                field_type: ConfigFieldType::Boolean,
                value: get_or_default(
                    settings,
                    "safety.injection_check_enabled",
                    Value::Bool(true),
                ),
                default_value: Value::Bool(true),
                required: false,
                sensitive: false,
            },
        ],
    }
}

fn build_gateway_section(settings: &std::collections::HashMap<String, Value>) -> ConfigSection {
    ConfigSection {
        name: "gateway".to_string(),
        description: "Web gateway settings".to_string(),
        fields: vec![
            ConfigField {
                key: "gateway.enabled".to_string(),
                label: "Gateway Enabled".to_string(),
                description: "Whether the web UI gateway is enabled".to_string(),
                field_type: ConfigFieldType::Boolean,
                value: get_or_default(settings, "gateway.enabled", Value::Bool(true)),
                default_value: Value::Bool(true),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "gateway.port".to_string(),
                label: "Port".to_string(),
                description: "Port for the web gateway".to_string(),
                field_type: ConfigFieldType::Integer,
                value: get_or_default(settings, "gateway.port", Value::Number(3000.into())),
                default_value: Value::Number(3000.into()),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "gateway.auth_token".to_string(),
                label: "Auth Token".to_string(),
                description: "Bearer token for gateway authentication".to_string(),
                field_type: ConfigFieldType::Secret,
                value: get_or_default(settings, "gateway.auth_token", Value::String(String::new())),
                default_value: Value::String(String::new()),
                required: false,
                sensitive: true,
            },
        ],
    }
}

fn build_heartbeat_section(settings: &std::collections::HashMap<String, Value>) -> ConfigSection {
    ConfigSection {
        name: "heartbeat".to_string(),
        description: "Periodic heartbeat settings".to_string(),
        fields: vec![
            ConfigField {
                key: "heartbeat.enabled".to_string(),
                label: "Heartbeat Enabled".to_string(),
                description: "Enable periodic proactive heartbeat checks".to_string(),
                field_type: ConfigFieldType::Boolean,
                value: get_or_default(settings, "heartbeat.enabled", Value::Bool(false)),
                default_value: Value::Bool(false),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "heartbeat.interval_secs".to_string(),
                label: "Heartbeat Interval".to_string(),
                description: "Seconds between heartbeat checks".to_string(),
                field_type: ConfigFieldType::Duration,
                value: get_or_default(
                    settings,
                    "heartbeat.interval_secs",
                    Value::Number(1800.into()),
                ),
                default_value: Value::Number(1800.into()),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "heartbeat.notify_channel".to_string(),
                label: "Notify Channel".to_string(),
                description: "Channel to notify on heartbeat findings".to_string(),
                field_type: ConfigFieldType::String,
                value: get_or_default(
                    settings,
                    "heartbeat.notify_channel",
                    Value::String(String::new()),
                ),
                default_value: Value::String(String::new()),
                required: false,
                sensitive: false,
            },
        ],
    }
}

fn build_routine_section(settings: &std::collections::HashMap<String, Value>) -> ConfigSection {
    ConfigSection {
        name: "routines".to_string(),
        description: "Scheduled routine settings".to_string(),
        fields: vec![
            ConfigField {
                key: "routines.enabled".to_string(),
                label: "Routines Enabled".to_string(),
                description: "Enable the scheduled routines system".to_string(),
                field_type: ConfigFieldType::Boolean,
                value: get_or_default(settings, "routines.enabled", Value::Bool(true)),
                default_value: Value::Bool(true),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "routines.cron_check_interval_secs".to_string(),
                label: "Cron Check Interval".to_string(),
                description: "Seconds between cron routine polling".to_string(),
                field_type: ConfigFieldType::Duration,
                value: get_or_default(
                    settings,
                    "routines.cron_check_interval_secs",
                    Value::Number(15.into()),
                ),
                default_value: Value::Number(15.into()),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "routines.max_concurrent_routines".to_string(),
                label: "Max Concurrent Routines".to_string(),
                description: "Maximum routines executing simultaneously".to_string(),
                field_type: ConfigFieldType::Integer,
                value: get_or_default(
                    settings,
                    "routines.max_concurrent_routines",
                    Value::Number(10.into()),
                ),
                default_value: Value::Number(10.into()),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "routines.default_cooldown_secs".to_string(),
                label: "Default Cooldown".to_string(),
                description: "Default cooldown between routine fires in seconds".to_string(),
                field_type: ConfigFieldType::Duration,
                value: get_or_default(
                    settings,
                    "routines.default_cooldown_secs",
                    Value::Number(300.into()),
                ),
                default_value: Value::Number(300.into()),
                required: false,
                sensitive: false,
            },
        ],
    }
}

fn build_sandbox_section(settings: &std::collections::HashMap<String, Value>) -> ConfigSection {
    ConfigSection {
        name: "sandbox".to_string(),
        description: "Docker sandbox settings".to_string(),
        fields: vec![
            ConfigField {
                key: "sandbox.enabled".to_string(),
                label: "Sandbox Enabled".to_string(),
                description: "Enable Docker container sandboxing for jobs".to_string(),
                field_type: ConfigFieldType::Boolean,
                value: get_or_default(settings, "sandbox.enabled", Value::Bool(true)),
                default_value: Value::Bool(true),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "sandbox.policy".to_string(),
                label: "Sandbox Policy".to_string(),
                description: "Container access policy".to_string(),
                field_type: ConfigFieldType::Select {
                    options: vec![
                        "readonly".to_string(),
                        "workspace_write".to_string(),
                        "full_access".to_string(),
                    ],
                },
                value: get_or_default(
                    settings,
                    "sandbox.policy",
                    Value::String("readonly".to_string()),
                ),
                default_value: Value::String("readonly".to_string()),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "sandbox.timeout_secs".to_string(),
                label: "Sandbox Timeout".to_string(),
                description: "Command timeout in seconds".to_string(),
                field_type: ConfigFieldType::Duration,
                value: get_or_default(settings, "sandbox.timeout_secs", Value::Number(120.into())),
                default_value: Value::Number(120.into()),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "sandbox.memory_limit_mb".to_string(),
                label: "Memory Limit (MB)".to_string(),
                description: "Container memory limit in megabytes".to_string(),
                field_type: ConfigFieldType::Integer,
                value: get_or_default(
                    settings,
                    "sandbox.memory_limit_mb",
                    Value::Number(2048.into()),
                ),
                default_value: Value::Number(2048.into()),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "sandbox.image".to_string(),
                label: "Docker Image".to_string(),
                description: "Docker image for the sandbox container".to_string(),
                field_type: ConfigFieldType::String,
                value: get_or_default(
                    settings,
                    "sandbox.image",
                    Value::String("ghcr.io/nearai/sandbox:latest".to_string()),
                ),
                default_value: Value::String("ghcr.io/nearai/sandbox:latest".to_string()),
                required: false,
                sensitive: false,
            },
        ],
    }
}

fn build_channels_section(settings: &std::collections::HashMap<String, Value>) -> ConfigSection {
    ConfigSection {
        name: "channels".to_string(),
        description: "Input channel settings".to_string(),
        fields: vec![
            ConfigField {
                key: "channels.cli_enabled".to_string(),
                label: "CLI Enabled".to_string(),
                description: "Enable the command-line REPL channel".to_string(),
                field_type: ConfigFieldType::Boolean,
                value: get_or_default(settings, "channels.cli_enabled", Value::Bool(true)),
                default_value: Value::Bool(true),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "channels.wasm_channels_enabled".to_string(),
                label: "WASM Channels Enabled".to_string(),
                description: "Enable WASM-based channels (Telegram, Slack, etc.)".to_string(),
                field_type: ConfigFieldType::Boolean,
                value: get_or_default(
                    settings,
                    "channels.wasm_channels_enabled",
                    Value::Bool(true),
                ),
                default_value: Value::Bool(true),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "channels.telegram_owner_id".to_string(),
                label: "Telegram Owner ID".to_string(),
                description: "Telegram user ID that owns the bot (restricts access)".to_string(),
                field_type: ConfigFieldType::Integer,
                value: get_or_default(settings, "channels.telegram_owner_id", Value::Null),
                default_value: Value::Null,
                required: false,
                sensitive: false,
            },
        ],
    }
}

fn build_embeddings_section(settings: &std::collections::HashMap<String, Value>) -> ConfigSection {
    ConfigSection {
        name: "embeddings".to_string(),
        description: "Vector embedding settings".to_string(),
        fields: vec![
            ConfigField {
                key: "embeddings.enabled".to_string(),
                label: "Embeddings Enabled".to_string(),
                description: "Enable vector embeddings for hybrid search".to_string(),
                field_type: ConfigFieldType::Boolean,
                value: get_or_default(settings, "embeddings.enabled", Value::Bool(false)),
                default_value: Value::Bool(false),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "embeddings.provider".to_string(),
                label: "Embedding Provider".to_string(),
                description: "Provider for generating embeddings".to_string(),
                field_type: ConfigFieldType::Select {
                    options: vec!["openai".to_string(), "nearai".to_string()],
                },
                value: get_or_default(
                    settings,
                    "embeddings.provider",
                    Value::String("openai".to_string()),
                ),
                default_value: Value::String("openai".to_string()),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "embeddings.model".to_string(),
                label: "Embedding Model".to_string(),
                description: "Model to use for generating embeddings".to_string(),
                field_type: ConfigFieldType::String,
                value: get_or_default(
                    settings,
                    "embeddings.model",
                    Value::String("text-embedding-3-small".to_string()),
                ),
                default_value: Value::String("text-embedding-3-small".to_string()),
                required: false,
                sensitive: false,
            },
            ConfigField {
                key: "embeddings.api_key".to_string(),
                label: "Embedding API Key".to_string(),
                description: "API key for the embedding provider".to_string(),
                field_type: ConfigFieldType::Secret,
                value: get_or_default(settings, "embeddings.api_key", Value::String(String::new())),
                default_value: Value::String(String::new()),
                required: false,
                sensitive: true,
            },
        ],
    }
}

/// Validate a configuration update.
pub fn validate_config_update(key: &str, value: &Value) -> Result<(), String> {
    match key {
        k if k.ends_with("_secs") || k.ends_with("_timeout") => {
            if !value.is_number() {
                return Err("Duration values must be numbers".to_string());
            }
            if let Some(n) = value.as_i64()
                && n < 0
            {
                return Err("Duration values must be non-negative".to_string());
            }
            Ok(())
        }
        k if k.contains("max_") || k.contains("_size") || k.contains("_count") => {
            if !value.is_number() {
                return Err("Numeric values required".to_string());
            }
            if let Some(n) = value.as_i64()
                && n < 0
            {
                return Err("Value must be non-negative".to_string());
            }
            Ok(())
        }
        k if k.contains("enabled") => {
            if !value.is_boolean() {
                return Err("Boolean value required".to_string());
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Get a list of sensitive config keys that should be masked.
pub fn sensitive_keys() -> Vec<&'static str> {
    vec![
        "llm.api_key",
        "llm.nearai_session_token",
        "gateway.auth_token",
        "embeddings.api_key",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_settings() -> std::collections::HashMap<String, Value> {
        std::collections::HashMap::new()
    }

    // --- Schema structure tests ---

    #[test]
    fn test_build_config_schema_has_all_sections() {
        let schema = build_config_schema(&empty_settings());
        assert_eq!(schema.sections.len(), 9);
        let names: Vec<&str> = schema.sections.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"agent"));
        assert!(names.contains(&"llm"));
        assert!(names.contains(&"safety"));
        assert!(names.contains(&"gateway"));
        assert!(names.contains(&"heartbeat"));
        assert!(names.contains(&"routines"));
        assert!(names.contains(&"sandbox"));
        assert!(names.contains(&"channels"));
        assert!(names.contains(&"embeddings"));
    }

    #[test]
    fn test_agent_section_has_expected_fields() {
        let section = build_agent_section(&empty_settings());
        assert_eq!(section.name, "agent");
        let keys: Vec<&str> = section.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"agent.name"));
        assert!(keys.contains(&"agent.max_parallel_jobs"));
        assert!(keys.contains(&"agent.job_timeout_secs"));
        assert!(keys.contains(&"agent.use_planning"));
        assert!(keys.contains(&"agent.stuck_threshold_secs"));
        assert!(keys.contains(&"agent.session_idle_timeout_secs"));
    }

    #[test]
    fn test_llm_section_has_expected_fields() {
        let section = build_llm_section(&empty_settings());
        assert_eq!(section.name, "llm");
        let keys: Vec<&str> = section.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"llm.backend"));
        assert!(keys.contains(&"llm.model"));
        assert!(keys.contains(&"llm.api_key"));
        assert!(keys.contains(&"llm.base_url"));
    }

    #[test]
    fn test_safety_section_has_expected_fields() {
        let section = build_safety_section(&empty_settings());
        assert_eq!(section.name, "safety");
        let keys: Vec<&str> = section.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"safety.max_output_length"));
        assert!(keys.contains(&"safety.injection_check_enabled"));
    }

    #[test]
    fn test_gateway_section_has_expected_fields() {
        let section = build_gateway_section(&empty_settings());
        assert_eq!(section.name, "gateway");
        let keys: Vec<&str> = section.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"gateway.enabled"));
        assert!(keys.contains(&"gateway.port"));
        assert!(keys.contains(&"gateway.auth_token"));
    }

    #[test]
    fn test_heartbeat_section_has_expected_fields() {
        let section = build_heartbeat_section(&empty_settings());
        assert_eq!(section.name, "heartbeat");
        let keys: Vec<&str> = section.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"heartbeat.enabled"));
        assert!(keys.contains(&"heartbeat.interval_secs"));
        assert!(keys.contains(&"heartbeat.notify_channel"));
    }

    #[test]
    fn test_sandbox_section_has_expected_fields() {
        let section = build_sandbox_section(&empty_settings());
        assert_eq!(section.name, "sandbox");
        let keys: Vec<&str> = section.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"sandbox.enabled"));
        assert!(keys.contains(&"sandbox.policy"));
        assert!(keys.contains(&"sandbox.timeout_secs"));
        assert!(keys.contains(&"sandbox.memory_limit_mb"));
        assert!(keys.contains(&"sandbox.image"));
    }

    #[test]
    fn test_embeddings_section_has_expected_fields() {
        let section = build_embeddings_section(&empty_settings());
        assert_eq!(section.name, "embeddings");
        let keys: Vec<&str> = section.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"embeddings.enabled"));
        assert!(keys.contains(&"embeddings.provider"));
        assert!(keys.contains(&"embeddings.model"));
        assert!(keys.contains(&"embeddings.api_key"));
    }

    // --- Field type correctness tests ---

    #[test]
    fn test_field_types_are_correct() {
        let schema = build_config_schema(&empty_settings());
        let all_fields: Vec<&ConfigField> =
            schema.sections.iter().flat_map(|s| &s.fields).collect();

        // Boolean fields
        let planning = all_fields
            .iter()
            .find(|f| f.key == "agent.use_planning")
            .unwrap();
        assert!(matches!(planning.field_type, ConfigFieldType::Boolean));

        // Duration fields
        let timeout = all_fields
            .iter()
            .find(|f| f.key == "agent.job_timeout_secs")
            .unwrap();
        assert!(matches!(timeout.field_type, ConfigFieldType::Duration));

        // Integer fields
        let max_jobs = all_fields
            .iter()
            .find(|f| f.key == "agent.max_parallel_jobs")
            .unwrap();
        assert!(matches!(max_jobs.field_type, ConfigFieldType::Integer));

        // Select fields
        let backend = all_fields.iter().find(|f| f.key == "llm.backend").unwrap();
        assert!(matches!(backend.field_type, ConfigFieldType::Select { .. }));

        // Secret fields
        let api_key = all_fields.iter().find(|f| f.key == "llm.api_key").unwrap();
        assert!(matches!(api_key.field_type, ConfigFieldType::Secret));

        // URL fields
        let base_url = all_fields.iter().find(|f| f.key == "llm.base_url").unwrap();
        assert!(matches!(base_url.field_type, ConfigFieldType::Url));
    }

    // --- Validation tests ---

    #[test]
    fn test_validate_duration_accepts_valid() {
        assert!(
            validate_config_update("agent.job_timeout_secs", &Value::Number(300.into())).is_ok()
        );
    }

    #[test]
    fn test_validate_duration_rejects_string() {
        let result =
            validate_config_update("heartbeat.interval_secs", &Value::String("abc".to_string()));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Duration values must be numbers");
    }

    #[test]
    fn test_validate_duration_rejects_negative() {
        let result = validate_config_update("sandbox.timeout_secs", &serde_json::json!(-5));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Duration values must be non-negative");
    }

    #[test]
    fn test_validate_numeric_fields() {
        assert!(
            validate_config_update("agent.max_parallel_jobs", &Value::Number(5.into())).is_ok()
        );
        let result = validate_config_update(
            "safety.max_output_length",
            &Value::String("abc".to_string()),
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Numeric values required");
    }

    #[test]
    fn test_validate_numeric_rejects_negative() {
        let result =
            validate_config_update("routines.max_concurrent_routines", &serde_json::json!(-1));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Value must be non-negative");
    }

    #[test]
    fn test_validate_boolean_fields() {
        assert!(validate_config_update("heartbeat.enabled", &Value::Bool(true)).is_ok());
        let result = validate_config_update("sandbox.enabled", &Value::String("yes".to_string()));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Boolean value required");
    }

    #[test]
    fn test_validate_unconstrained_key_accepts_anything() {
        assert!(validate_config_update("agent.name", &Value::String("MyBot".to_string())).is_ok());
        assert!(validate_config_update("sandbox.image", &Value::Number(42.into())).is_ok());
    }

    // --- Sensitive keys tests ---

    #[test]
    fn test_sensitive_keys_identification() {
        let keys = sensitive_keys();
        assert!(keys.contains(&"llm.api_key"));
        assert!(keys.contains(&"llm.nearai_session_token"));
        assert!(keys.contains(&"gateway.auth_token"));
        assert!(keys.contains(&"embeddings.api_key"));
        assert!(!keys.contains(&"agent.name"));
    }

    #[test]
    fn test_sensitive_fields_marked_in_schema() {
        let schema = build_config_schema(&empty_settings());
        let all_fields: Vec<&ConfigField> =
            schema.sections.iter().flat_map(|s| &s.fields).collect();

        let sensitive_field_keys: Vec<&str> = all_fields
            .iter()
            .filter(|f| f.sensitive)
            .map(|f| f.key.as_str())
            .collect();

        assert!(sensitive_field_keys.contains(&"llm.api_key"));
        assert!(sensitive_field_keys.contains(&"llm.nearai_session_token"));
        assert!(sensitive_field_keys.contains(&"gateway.auth_token"));
        assert!(sensitive_field_keys.contains(&"embeddings.api_key"));

        // Non-sensitive fields must not be marked
        let agent_name = all_fields.iter().find(|f| f.key == "agent.name").unwrap();
        assert!(!agent_name.sensitive);
    }

    // --- Default values tests ---

    #[test]
    fn test_default_values_present() {
        let schema = build_config_schema(&empty_settings());
        let all_fields: Vec<&ConfigField> =
            schema.sections.iter().flat_map(|s| &s.fields).collect();

        let agent_name = all_fields.iter().find(|f| f.key == "agent.name").unwrap();
        assert_eq!(
            agent_name.default_value,
            Value::String("IronClaw".to_string())
        );
        assert_eq!(agent_name.value, Value::String("IronClaw".to_string()));

        let max_jobs = all_fields
            .iter()
            .find(|f| f.key == "agent.max_parallel_jobs")
            .unwrap();
        assert_eq!(max_jobs.default_value, Value::Number(3.into()));
    }

    #[test]
    fn test_custom_settings_override_defaults() {
        let mut settings = std::collections::HashMap::new();
        settings.insert(
            "agent.name".to_string(),
            Value::String("CustomBot".to_string()),
        );
        settings.insert(
            "agent.max_parallel_jobs".to_string(),
            Value::Number(10.into()),
        );

        let schema = build_config_schema(&settings);
        let agent = schema.sections.iter().find(|s| s.name == "agent").unwrap();

        let name_field = agent.fields.iter().find(|f| f.key == "agent.name").unwrap();
        assert_eq!(name_field.value, Value::String("CustomBot".to_string()));
        // Default should remain unchanged
        assert_eq!(
            name_field.default_value,
            Value::String("IronClaw".to_string())
        );

        let jobs_field = agent
            .fields
            .iter()
            .find(|f| f.key == "agent.max_parallel_jobs")
            .unwrap();
        assert_eq!(jobs_field.value, Value::Number(10.into()));
        assert_eq!(jobs_field.default_value, Value::Number(3.into()));
    }

    // --- Serialization tests ---

    #[test]
    fn test_config_field_type_serialization() {
        let string_type = ConfigFieldType::String;
        let json = serde_json::to_string(&string_type).unwrap();
        assert_eq!(json, r#""string""#);

        let bool_type = ConfigFieldType::Boolean;
        let json = serde_json::to_string(&bool_type).unwrap();
        assert_eq!(json, r#""boolean""#);

        let select_type = ConfigFieldType::Select {
            options: vec!["a".to_string(), "b".to_string()],
        };
        let json = serde_json::to_string(&select_type).unwrap();
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["select"]["options"][0], "a");
        assert_eq!(parsed["select"]["options"][1], "b");

        let secret_type = ConfigFieldType::Secret;
        let json = serde_json::to_string(&secret_type).unwrap();
        assert_eq!(json, r#""secret""#);

        let duration_type = ConfigFieldType::Duration;
        let json = serde_json::to_string(&duration_type).unwrap();
        assert_eq!(json, r#""duration""#);

        let url_type = ConfigFieldType::Url;
        let json = serde_json::to_string(&url_type).unwrap();
        assert_eq!(json, r#""url""#);
    }

    #[test]
    fn test_config_response_serialization() {
        let schema = build_config_schema(&empty_settings());
        let json = serde_json::to_string(&schema).unwrap();
        let parsed: Value = serde_json::from_str(&json).unwrap();

        assert!(parsed["sections"].is_array());
        assert_eq!(parsed["sections"].as_array().unwrap().len(), 9);

        // Verify first section structure
        let first = &parsed["sections"][0];
        assert!(first["name"].is_string());
        assert!(first["description"].is_string());
        assert!(first["fields"].is_array());

        // Verify field structure
        let field = &first["fields"][0];
        assert!(field["key"].is_string());
        assert!(field["label"].is_string());
        assert!(field["description"].is_string());
        assert!(!field["field_type"].is_null());
        assert!(!field["default_value"].is_null());
    }

    #[test]
    fn test_config_update_request_deserialization() {
        let json = r#"{"key":"agent.name","value":"NewBot"}"#;
        let req: ConfigUpdateRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.key, "agent.name");
        assert_eq!(req.value, Value::String("NewBot".to_string()));
    }

    #[test]
    fn test_config_batch_update_request_deserialization() {
        let json = r#"{"updates":[{"key":"agent.name","value":"Bot"},{"key":"heartbeat.enabled","value":true}]}"#;
        let req: ConfigBatchUpdateRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.updates.len(), 2);
        assert_eq!(req.updates[0].key, "agent.name");
        assert_eq!(req.updates[1].key, "heartbeat.enabled");
        assert_eq!(req.updates[1].value, Value::Bool(true));
    }

    #[test]
    fn test_config_update_response_serialization() {
        let resp = ConfigUpdateResponse {
            key: "agent.name".to_string(),
            success: true,
            message: "Updated successfully".to_string(),
            previous_value: Some(Value::String("OldBot".to_string())),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["key"], "agent.name");
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["previous_value"], "OldBot");
    }

    #[test]
    fn test_config_validation_response_serialization() {
        let resp = ConfigValidationResponse {
            valid: false,
            errors: vec![ConfigValidationError {
                key: "agent.max_parallel_jobs".to_string(),
                message: "Value must be non-negative".to_string(),
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["valid"], false);
        assert_eq!(parsed["errors"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["errors"][0]["key"], "agent.max_parallel_jobs");
    }

    // --- Section descriptions present ---

    #[test]
    fn test_all_sections_have_descriptions() {
        let schema = build_config_schema(&empty_settings());
        for section in &schema.sections {
            assert!(
                !section.description.is_empty(),
                "Section '{}' has empty description",
                section.name
            );
        }
    }

    // --- Select field options ---

    #[test]
    fn test_select_field_options() {
        let schema = build_config_schema(&empty_settings());
        let all_fields: Vec<&ConfigField> =
            schema.sections.iter().flat_map(|s| &s.fields).collect();

        let backend = all_fields.iter().find(|f| f.key == "llm.backend").unwrap();
        match &backend.field_type {
            ConfigFieldType::Select { options } => {
                assert!(options.contains(&"nearai".to_string()));
                assert!(options.contains(&"openai".to_string()));
                assert!(options.contains(&"anthropic".to_string()));
                assert!(options.contains(&"ollama".to_string()));
                assert!(options.contains(&"openai_compatible".to_string()));
            }
            _ => panic!("Expected Select field type for llm.backend"),
        }

        let policy = all_fields
            .iter()
            .find(|f| f.key == "sandbox.policy")
            .unwrap();
        match &policy.field_type {
            ConfigFieldType::Select { options } => {
                assert!(options.contains(&"readonly".to_string()));
                assert!(options.contains(&"workspace_write".to_string()));
                assert!(options.contains(&"full_access".to_string()));
            }
            _ => panic!("Expected Select field type for sandbox.policy"),
        }
    }
}
