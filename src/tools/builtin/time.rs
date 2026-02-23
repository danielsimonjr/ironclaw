//! Time utility tool.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::context::JobContext;
use crate::tools::tool::{Tool, ToolError, ToolOutput};

/// Tool for getting current time and date operations.
pub struct TimeTool;

#[async_trait]
impl Tool for TimeTool {
    fn name(&self) -> &str {
        "time"
    }

    fn description(&self) -> &str {
        "Get current time, convert timezones, or calculate time differences."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["now", "parse", "format", "diff"],
                    "description": "The time operation to perform"
                },
                "timestamp": {
                    "type": "string",
                    "description": "ISO 8601 timestamp (for parse/format/diff operations)"
                },
                "format": {
                    "type": "string",
                    "description": "Output format string (for format operation)"
                },
                "timestamp2": {
                    "type": "string",
                    "description": "Second timestamp (for diff operation)"
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let operation = params
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::InvalidParameters("missing 'operation' parameter".to_string())
            })?;

        let result = match operation {
            "now" => {
                let now = Utc::now();
                serde_json::json!({
                    "iso": now.to_rfc3339(),
                    "unix": now.timestamp(),
                    "unix_millis": now.timestamp_millis()
                })
            }
            "parse" => {
                let timestamp = params
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidParameters("missing 'timestamp' parameter".to_string())
                    })?;

                let dt: DateTime<Utc> = timestamp.parse().map_err(|e| {
                    ToolError::InvalidParameters(format!("invalid timestamp: {}", e))
                })?;

                serde_json::json!({
                    "iso": dt.to_rfc3339(),
                    "unix": dt.timestamp(),
                    "unix_millis": dt.timestamp_millis()
                })
            }
            "diff" => {
                let ts1 = params
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidParameters("missing 'timestamp' parameter".to_string())
                    })?;

                let ts2 = params
                    .get("timestamp2")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidParameters("missing 'timestamp2' parameter".to_string())
                    })?;

                let dt1: DateTime<Utc> = ts1.parse().map_err(|e| {
                    ToolError::InvalidParameters(format!("invalid timestamp: {}", e))
                })?;
                let dt2: DateTime<Utc> = ts2.parse().map_err(|e| {
                    ToolError::InvalidParameters(format!("invalid timestamp2: {}", e))
                })?;

                let diff = dt2.signed_duration_since(dt1);

                serde_json::json!({
                    "seconds": diff.num_seconds(),
                    "minutes": diff.num_minutes(),
                    "hours": diff.num_hours(),
                    "days": diff.num_days()
                })
            }
            _ => {
                return Err(ToolError::InvalidParameters(format!(
                    "unknown operation: {}",
                    operation
                )));
            }
        };

        Ok(ToolOutput::success(result, start.elapsed()))
    }

    fn requires_sanitization(&self) -> bool {
        false // Internal tool, no external data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        let tool = TimeTool;
        assert_eq!(tool.name(), "time");
    }

    #[test]
    fn test_parameters_schema_has_operation_required() {
        let tool = TimeTool;
        let schema = tool.parameters_schema();
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("operation")));
    }

    #[tokio::test]
    async fn test_execute_now() {
        let tool = TimeTool;
        let ctx = JobContext::new("test", "test");
        let params = serde_json::json!({"operation": "now"});
        let output = tool.execute(params, &ctx).await.unwrap();
        assert!(output.result.get("iso").is_some());
        assert!(output.result.get("unix").is_some());
        assert!(output.result.get("unix_millis").is_some());
    }

    #[tokio::test]
    async fn test_execute_parse_valid() {
        let tool = TimeTool;
        let ctx = JobContext::new("test", "test");
        let params = serde_json::json!({
            "operation": "parse",
            "timestamp": "2024-01-15T10:30:00Z"
        });
        let output = tool.execute(params, &ctx).await.unwrap();
        assert!(output.result.get("iso").is_some());
        assert!(output.result.get("unix").is_some());
    }

    #[tokio::test]
    async fn test_execute_parse_invalid_timestamp() {
        let tool = TimeTool;
        let ctx = JobContext::new("test", "test");
        let params = serde_json::json!({
            "operation": "parse",
            "timestamp": "not-a-timestamp"
        });
        let err = tool.execute(params, &ctx).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidParameters(_)));
    }

    #[tokio::test]
    async fn test_execute_diff_valid() {
        let tool = TimeTool;
        let ctx = JobContext::new("test", "test");
        let params = serde_json::json!({
            "operation": "diff",
            "timestamp": "2024-01-15T10:00:00Z",
            "timestamp2": "2024-01-15T11:30:00Z"
        });
        let output = tool.execute(params, &ctx).await.unwrap();
        assert_eq!(output.result.get("seconds").unwrap().as_i64().unwrap(), 5400);
        assert_eq!(output.result.get("minutes").unwrap().as_i64().unwrap(), 90);
        assert_eq!(output.result.get("hours").unwrap().as_i64().unwrap(), 1);
        assert_eq!(output.result.get("days").unwrap().as_i64().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_execute_diff_missing_timestamp2() {
        let tool = TimeTool;
        let ctx = JobContext::new("test", "test");
        let params = serde_json::json!({
            "operation": "diff",
            "timestamp": "2024-01-15T10:00:00Z"
        });
        let err = tool.execute(params, &ctx).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidParameters(_)));
        assert!(err.to_string().contains("timestamp2"));
    }

    #[tokio::test]
    async fn test_execute_unknown_operation() {
        let tool = TimeTool;
        let ctx = JobContext::new("test", "test");
        let params = serde_json::json!({"operation": "format"});
        let err = tool.execute(params, &ctx).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidParameters(_)));
        assert!(err.to_string().contains("unknown operation"));
    }

    #[tokio::test]
    async fn test_execute_missing_operation() {
        let tool = TimeTool;
        let ctx = JobContext::new("test", "test");
        let params = serde_json::json!({});
        let err = tool.execute(params, &ctx).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidParameters(_)));
    }

    #[test]
    fn test_requires_sanitization_false() {
        let tool = TimeTool;
        assert!(!tool.requires_sanitization());
    }
}
