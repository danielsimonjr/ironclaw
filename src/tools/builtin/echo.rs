//! Echo tool for testing.

use async_trait::async_trait;

use crate::context::JobContext;
use crate::tools::tool::{Tool, ToolError, ToolOutput};

/// Simple echo tool for testing.
pub struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echoes back the input message. Useful for testing tool execution."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "The message to echo back"
                }
            },
            "required": ["message"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let message = params
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::InvalidParameters("missing 'message' parameter".to_string())
            })?;

        Ok(ToolOutput::text(message, start.elapsed()))
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
        let tool = EchoTool;
        assert_eq!(tool.name(), "echo");
    }

    #[test]
    fn test_description_is_non_empty() {
        let tool = EchoTool;
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_parameters_schema_has_message() {
        let tool = EchoTool;
        let schema = tool.parameters_schema();
        let props = schema.get("properties").unwrap();
        assert!(props.get("message").is_some());
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("message")));
    }

    #[tokio::test]
    async fn test_execute_valid_message() {
        let tool = EchoTool;
        let ctx = JobContext::new("test", "test");
        let params = serde_json::json!({"message": "hello world"});
        let output = tool.execute(params, &ctx).await.unwrap();
        assert_eq!(output.result.as_str().unwrap(), "hello world");
    }

    #[tokio::test]
    async fn test_execute_missing_message() {
        let tool = EchoTool;
        let ctx = JobContext::new("test", "test");
        let params = serde_json::json!({});
        let err = tool.execute(params, &ctx).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidParameters(_)));
    }

    #[test]
    fn test_requires_sanitization_false() {
        let tool = EchoTool;
        assert!(!tool.requires_sanitization());
    }
}
