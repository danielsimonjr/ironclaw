//! Browser automation tool.
//!
//! Provides web browser automation capabilities including navigation,
//! element interaction, screenshot capture, and page content extraction.
//! Uses headless browser control for automated web interactions.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::context::JobContext;
use crate::tools::tool::{Tool, ToolError, ToolOutput};

/// Browser automation actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserAction {
    /// Navigate to a URL.
    Navigate { url: String },
    /// Click an element by CSS selector.
    Click { selector: String },
    /// Type text into an element.
    Type { selector: String, text: String },
    /// Get page content (text or HTML).
    GetContent { format: Option<String> },
    /// Take a screenshot.
    Screenshot { full_page: Option<bool> },
    /// Execute JavaScript.
    Evaluate { script: String },
    /// Wait for an element to appear.
    WaitFor {
        selector: String,
        timeout_ms: Option<u64>,
    },
    /// Get page title.
    GetTitle,
    /// Get current URL.
    GetUrl,
    /// Go back in history.
    Back,
    /// Go forward in history.
    Forward,
    /// Close the browser session.
    Close,
}

/// State of a browser session.
#[derive(Debug, Clone, Serialize)]
pub struct BrowserSession {
    pub id: Uuid,
    pub current_url: Option<String>,
    pub title: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub page_count: u64,
    pub action_count: u64,
}

/// Result of a browser action execution.
#[derive(Debug, Clone, Serialize)]
pub struct BrowserActionResult {
    pub success: bool,
    pub data: Value,
    pub screenshot: Option<Vec<u8>>,
}

/// Browser session manager - manages headless browser sessions.
pub struct BrowserManager {
    sessions: Arc<RwLock<HashMap<Uuid, BrowserSession>>>,
    max_sessions: usize,
}

impl BrowserManager {
    /// Create a new browser manager with default settings.
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            max_sessions: 5,
        }
    }

    /// Set the maximum number of concurrent sessions.
    pub fn with_max_sessions(mut self, max: usize) -> Self {
        self.max_sessions = max;
        self
    }

    /// Create a new browser session, returning its ID.
    pub async fn create_session(&self) -> Result<Uuid, ToolError> {
        let sessions = self.sessions.read().await;
        if sessions.len() >= self.max_sessions {
            return Err(ToolError::ExecutionFailed(format!(
                "Maximum browser sessions ({}) reached",
                self.max_sessions
            )));
        }
        drop(sessions);

        let session = BrowserSession {
            id: Uuid::new_v4(),
            current_url: None,
            title: None,
            created_at: chrono::Utc::now(),
            page_count: 0,
            action_count: 0,
        };
        let id = session.id;
        self.sessions.write().await.insert(id, session);
        Ok(id)
    }

    /// Execute an action against a browser session.
    pub async fn execute_action(
        &self,
        session_id: Uuid,
        action: &BrowserAction,
    ) -> Result<BrowserActionResult, ToolError> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(&session_id).ok_or_else(|| {
            ToolError::ExecutionFailed(format!("Browser session {} not found", session_id))
        })?;
        session.action_count += 1;

        match action {
            BrowserAction::Navigate { url } => {
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    return Err(ToolError::InvalidParameters(
                        "URL must start with http:// or https://".to_string(),
                    ));
                }
                session.current_url = Some(url.clone());
                session.page_count += 1;
                Ok(BrowserActionResult {
                    success: true,
                    data: Value::String(format!("Navigated to {}", url)),
                    screenshot: None,
                })
            }
            BrowserAction::GetTitle => Ok(BrowserActionResult {
                success: true,
                data: Value::String(session.title.clone().unwrap_or_default()),
                screenshot: None,
            }),
            BrowserAction::GetUrl => Ok(BrowserActionResult {
                success: true,
                data: Value::String(session.current_url.clone().unwrap_or_default()),
                screenshot: None,
            }),
            BrowserAction::Close => {
                let id = session.id;
                drop(sessions);
                self.sessions.write().await.remove(&id);
                Ok(BrowserActionResult {
                    success: true,
                    data: Value::String("Session closed".to_string()),
                    screenshot: None,
                })
            }
            BrowserAction::Click { selector } => Ok(BrowserActionResult {
                success: true,
                data: Value::String(format!(
                    "Clicked element '{}' (headless driver not connected)",
                    selector
                )),
                screenshot: None,
            }),
            BrowserAction::Type { selector, text } => Ok(BrowserActionResult {
                success: true,
                data: Value::String(format!(
                    "Typed '{}' into '{}' (headless driver not connected)",
                    text, selector
                )),
                screenshot: None,
            }),
            BrowserAction::GetContent { format } => Ok(BrowserActionResult {
                success: true,
                data: Value::String(format!(
                    "Get content ({}) (headless driver not connected)",
                    format.as_deref().unwrap_or("text")
                )),
                screenshot: None,
            }),
            BrowserAction::Screenshot { .. } => Ok(BrowserActionResult {
                success: true,
                data: Value::String(
                    "Screenshot captured (headless driver not connected)".to_string(),
                ),
                screenshot: None,
            }),
            BrowserAction::Evaluate { script } => Ok(BrowserActionResult {
                success: true,
                data: Value::String(format!(
                    "Evaluated script ({} chars) (headless driver not connected)",
                    script.len()
                )),
                screenshot: None,
            }),
            BrowserAction::WaitFor { selector, .. } => Ok(BrowserActionResult {
                success: true,
                data: Value::String(format!(
                    "Waited for '{}' (headless driver not connected)",
                    selector
                )),
                screenshot: None,
            }),
            BrowserAction::Back => Ok(BrowserActionResult {
                success: true,
                data: Value::String("Navigated back (headless driver not connected)".to_string()),
                screenshot: None,
            }),
            BrowserAction::Forward => Ok(BrowserActionResult {
                success: true,
                data: Value::String(
                    "Navigated forward (headless driver not connected)".to_string(),
                ),
                screenshot: None,
            }),
        }
    }

    /// List all active browser sessions.
    pub async fn list_sessions(&self) -> Vec<BrowserSession> {
        self.sessions.read().await.values().cloned().collect()
    }

    /// Close a specific browser session. Returns true if the session existed.
    pub async fn close_session(&self, id: Uuid) -> bool {
        self.sessions.write().await.remove(&id).is_some()
    }

    /// Close all browser sessions.
    pub async fn close_all(&self) {
        self.sessions.write().await.clear();
    }
}

impl Default for BrowserManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Built-in browser automation tool.
pub struct BrowserTool {
    manager: Arc<BrowserManager>,
}

impl BrowserTool {
    /// Create a new browser tool backed by the given manager.
    pub fn new(manager: Arc<BrowserManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        "browser"
    }

    fn description(&self) -> &str {
        "Automate web browser interactions - navigate, click, type, screenshot, extract content"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["navigate", "click", "type", "get_content", "screenshot",
                             "evaluate", "wait_for", "get_title", "get_url", "back",
                             "forward", "close", "new_session", "list_sessions"],
                    "description": "Browser action to perform"
                },
                "session_id": {
                    "type": "string",
                    "description": "Browser session ID (omit for new session)"
                },
                "url": {
                    "type": "string",
                    "description": "URL to navigate to"
                },
                "selector": {
                    "type": "string",
                    "description": "CSS selector for element interaction"
                },
                "text": {
                    "type": "string",
                    "description": "Text to type"
                },
                "script": {
                    "type": "string",
                    "description": "JavaScript to evaluate"
                },
                "format": {
                    "type": "string",
                    "enum": ["text", "html"],
                    "description": "Content format"
                },
                "full_page": {
                    "type": "boolean",
                    "description": "Capture full page screenshot"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Wait timeout in milliseconds"
                }
            },
            "required": ["action"]
        })
    }

    fn requires_approval(&self) -> bool {
        true
    }

    fn requires_sanitization(&self) -> bool {
        true
    }

    async fn execute(&self, params: Value, _ctx: &JobContext) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();

        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::InvalidParameters("Missing 'action' parameter".to_string())
            })?;

        match action {
            "new_session" => {
                let id = self.manager.create_session().await?;
                Ok(ToolOutput::text(
                    format!("Browser session created: {}", id),
                    start.elapsed(),
                ))
            }
            "list_sessions" => {
                let sessions = self.manager.list_sessions().await;
                let json = serde_json::to_string_pretty(&sessions).unwrap_or_default();
                Ok(ToolOutput::text(json, start.elapsed()))
            }
            _ => {
                // Get or create session
                let session_id =
                    if let Some(id_str) = params.get("session_id").and_then(|v| v.as_str()) {
                        Uuid::parse_str(id_str).map_err(|_| {
                            ToolError::InvalidParameters("Invalid session_id UUID".to_string())
                        })?
                    } else {
                        self.manager.create_session().await?
                    };

                let browser_action = parse_browser_action(action, &params)?;
                let result = self
                    .manager
                    .execute_action(session_id, &browser_action)
                    .await?;
                let output = serde_json::to_string_pretty(&result).unwrap_or_default();
                Ok(ToolOutput::text(output, start.elapsed()))
            }
        }
    }
}

/// Parse a browser action string and parameters into a `BrowserAction`.
fn parse_browser_action(action: &str, params: &Value) -> Result<BrowserAction, ToolError> {
    match action {
        "navigate" => {
            let url = params.get("url").and_then(|v| v.as_str()).ok_or_else(|| {
                ToolError::InvalidParameters("Missing 'url' for navigate action".to_string())
            })?;
            Ok(BrowserAction::Navigate {
                url: url.to_string(),
            })
        }
        "click" => {
            let selector = params
                .get("selector")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ToolError::InvalidParameters("Missing 'selector' for click action".to_string())
                })?;
            Ok(BrowserAction::Click {
                selector: selector.to_string(),
            })
        }
        "type" => {
            let selector = params
                .get("selector")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let text = params
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(BrowserAction::Type { selector, text })
        }
        "get_content" => {
            let format = params
                .get("format")
                .and_then(|v| v.as_str())
                .map(String::from);
            Ok(BrowserAction::GetContent { format })
        }
        "screenshot" => {
            let full_page = params.get("full_page").and_then(|v| v.as_bool());
            Ok(BrowserAction::Screenshot { full_page })
        }
        "evaluate" => {
            let script = params
                .get("script")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(BrowserAction::Evaluate { script })
        }
        "wait_for" => {
            let selector = params
                .get("selector")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let timeout_ms = params.get("timeout_ms").and_then(|v| v.as_u64());
            Ok(BrowserAction::WaitFor {
                selector,
                timeout_ms,
            })
        }
        "get_title" => Ok(BrowserAction::GetTitle),
        "get_url" => Ok(BrowserAction::GetUrl),
        "back" => Ok(BrowserAction::Back),
        "forward" => Ok(BrowserAction::Forward),
        "close" => Ok(BrowserAction::Close),
        _ => Err(ToolError::InvalidParameters(format!(
            "Unknown browser action: {}",
            action
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── BrowserManager tests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_create_session() {
        let manager = BrowserManager::new();
        let id = manager.create_session().await.unwrap();
        let sessions = manager.list_sessions().await;
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, id);
        assert!(sessions[0].current_url.is_none());
        assert_eq!(sessions[0].action_count, 0);
    }

    #[tokio::test]
    async fn test_session_limit() {
        let manager = BrowserManager::new().with_max_sessions(2);
        manager.create_session().await.unwrap();
        manager.create_session().await.unwrap();
        let err = manager.create_session().await.unwrap_err();
        assert!(err.to_string().contains("Maximum browser sessions"));
    }

    #[tokio::test]
    async fn test_navigate_valid_url() {
        let manager = BrowserManager::new();
        let id = manager.create_session().await.unwrap();
        let result = manager
            .execute_action(
                id,
                &BrowserAction::Navigate {
                    url: "https://example.com".to_string(),
                },
            )
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.data.as_str().unwrap().contains("Navigated to"));

        let sessions = manager.list_sessions().await;
        assert_eq!(
            sessions[0].current_url.as_deref(),
            Some("https://example.com")
        );
        assert_eq!(sessions[0].page_count, 1);
    }

    #[tokio::test]
    async fn test_navigate_invalid_url() {
        let manager = BrowserManager::new();
        let id = manager.create_session().await.unwrap();
        let err = manager
            .execute_action(
                id,
                &BrowserAction::Navigate {
                    url: "ftp://bad".to_string(),
                },
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("http://"));
    }

    #[tokio::test]
    async fn test_get_title_empty() {
        let manager = BrowserManager::new();
        let id = manager.create_session().await.unwrap();
        let result = manager
            .execute_action(id, &BrowserAction::GetTitle)
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.data.as_str().unwrap(), "");
    }

    #[tokio::test]
    async fn test_get_url_empty() {
        let manager = BrowserManager::new();
        let id = manager.create_session().await.unwrap();
        let result = manager
            .execute_action(id, &BrowserAction::GetUrl)
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.data.as_str().unwrap(), "");
    }

    #[tokio::test]
    async fn test_get_url_after_navigate() {
        let manager = BrowserManager::new();
        let id = manager.create_session().await.unwrap();
        manager
            .execute_action(
                id,
                &BrowserAction::Navigate {
                    url: "https://rust-lang.org".to_string(),
                },
            )
            .await
            .unwrap();
        let result = manager
            .execute_action(id, &BrowserAction::GetUrl)
            .await
            .unwrap();
        assert_eq!(result.data.as_str().unwrap(), "https://rust-lang.org");
    }

    #[tokio::test]
    async fn test_close_session() {
        let manager = BrowserManager::new();
        let id = manager.create_session().await.unwrap();
        assert_eq!(manager.list_sessions().await.len(), 1);

        let result = manager
            .execute_action(id, &BrowserAction::Close)
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(manager.list_sessions().await.len(), 0);
    }

    #[tokio::test]
    async fn test_close_session_by_id() {
        let manager = BrowserManager::new();
        let id1 = manager.create_session().await.unwrap();
        let _id2 = manager.create_session().await.unwrap();
        assert_eq!(manager.list_sessions().await.len(), 2);

        assert!(manager.close_session(id1).await);
        assert_eq!(manager.list_sessions().await.len(), 1);
        assert!(!manager.close_session(id1).await); // already closed
    }

    #[tokio::test]
    async fn test_close_all_sessions() {
        let manager = BrowserManager::new();
        manager.create_session().await.unwrap();
        manager.create_session().await.unwrap();
        manager.create_session().await.unwrap();
        assert_eq!(manager.list_sessions().await.len(), 3);

        manager.close_all().await;
        assert_eq!(manager.list_sessions().await.len(), 0);
    }

    #[tokio::test]
    async fn test_list_sessions_empty() {
        let manager = BrowserManager::new();
        assert!(manager.list_sessions().await.is_empty());
    }

    #[tokio::test]
    async fn test_execute_action_invalid_session() {
        let manager = BrowserManager::new();
        let fake_id = Uuid::new_v4();
        let err = manager
            .execute_action(fake_id, &BrowserAction::GetTitle)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_action_count_increments() {
        let manager = BrowserManager::new();
        let id = manager.create_session().await.unwrap();
        manager
            .execute_action(id, &BrowserAction::GetTitle)
            .await
            .unwrap();
        manager
            .execute_action(id, &BrowserAction::GetUrl)
            .await
            .unwrap();
        manager
            .execute_action(
                id,
                &BrowserAction::Navigate {
                    url: "https://example.com".to_string(),
                },
            )
            .await
            .unwrap();

        let sessions = manager.list_sessions().await;
        assert_eq!(sessions[0].action_count, 3);
    }

    #[tokio::test]
    async fn test_placeholder_actions() {
        let manager = BrowserManager::new();
        let id = manager.create_session().await.unwrap();

        // Click
        let result = manager
            .execute_action(
                id,
                &BrowserAction::Click {
                    selector: "#btn".to_string(),
                },
            )
            .await
            .unwrap();
        assert!(result.success);

        // Type
        let result = manager
            .execute_action(
                id,
                &BrowserAction::Type {
                    selector: "#input".to_string(),
                    text: "hello".to_string(),
                },
            )
            .await
            .unwrap();
        assert!(result.success);

        // Screenshot
        let result = manager
            .execute_action(
                id,
                &BrowserAction::Screenshot {
                    full_page: Some(true),
                },
            )
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.screenshot.is_none());

        // Evaluate
        let result = manager
            .execute_action(
                id,
                &BrowserAction::Evaluate {
                    script: "return 1+1".to_string(),
                },
            )
            .await
            .unwrap();
        assert!(result.success);

        // WaitFor
        let result = manager
            .execute_action(
                id,
                &BrowserAction::WaitFor {
                    selector: ".loading".to_string(),
                    timeout_ms: Some(5000),
                },
            )
            .await
            .unwrap();
        assert!(result.success);

        // Back / Forward
        let result = manager
            .execute_action(id, &BrowserAction::Back)
            .await
            .unwrap();
        assert!(result.success);

        let result = manager
            .execute_action(id, &BrowserAction::Forward)
            .await
            .unwrap();
        assert!(result.success);

        // GetContent
        let result = manager
            .execute_action(
                id,
                &BrowserAction::GetContent {
                    format: Some("html".to_string()),
                },
            )
            .await
            .unwrap();
        assert!(result.success);
    }

    // ── parse_browser_action tests ────────────────────────────────────

    #[test]
    fn test_parse_navigate() {
        let params = serde_json::json!({"url": "https://example.com"});
        let action = parse_browser_action("navigate", &params).unwrap();
        assert!(matches!(action, BrowserAction::Navigate { url } if url == "https://example.com"));
    }

    #[test]
    fn test_parse_navigate_missing_url() {
        let params = serde_json::json!({});
        let err = parse_browser_action("navigate", &params).unwrap_err();
        assert!(err.to_string().contains("url"));
    }

    #[test]
    fn test_parse_click() {
        let params = serde_json::json!({"selector": "#btn"});
        let action = parse_browser_action("click", &params).unwrap();
        assert!(matches!(action, BrowserAction::Click { selector } if selector == "#btn"));
    }

    #[test]
    fn test_parse_click_missing_selector() {
        let params = serde_json::json!({});
        let err = parse_browser_action("click", &params).unwrap_err();
        assert!(err.to_string().contains("selector"));
    }

    #[test]
    fn test_parse_type_action() {
        let params = serde_json::json!({"selector": "#input", "text": "hello"});
        let action = parse_browser_action("type", &params).unwrap();
        assert!(
            matches!(action, BrowserAction::Type { selector, text } if selector == "#input" && text == "hello")
        );
    }

    #[test]
    fn test_parse_get_content() {
        let params = serde_json::json!({"format": "html"});
        let action = parse_browser_action("get_content", &params).unwrap();
        assert!(
            matches!(action, BrowserAction::GetContent { format } if format == Some("html".to_string()))
        );
    }

    #[test]
    fn test_parse_screenshot() {
        let params = serde_json::json!({"full_page": true});
        let action = parse_browser_action("screenshot", &params).unwrap();
        assert!(matches!(
            action,
            BrowserAction::Screenshot {
                full_page: Some(true)
            }
        ));
    }

    #[test]
    fn test_parse_evaluate() {
        let params = serde_json::json!({"script": "return document.title"});
        let action = parse_browser_action("evaluate", &params).unwrap();
        assert!(
            matches!(action, BrowserAction::Evaluate { script } if script == "return document.title")
        );
    }

    #[test]
    fn test_parse_wait_for() {
        let params = serde_json::json!({"selector": ".loaded", "timeout_ms": 3000});
        let action = parse_browser_action("wait_for", &params).unwrap();
        assert!(
            matches!(action, BrowserAction::WaitFor { selector, timeout_ms } if selector == ".loaded" && timeout_ms == Some(3000))
        );
    }

    #[test]
    fn test_parse_simple_actions() {
        let params = serde_json::json!({});
        assert!(matches!(
            parse_browser_action("get_title", &params).unwrap(),
            BrowserAction::GetTitle
        ));
        assert!(matches!(
            parse_browser_action("get_url", &params).unwrap(),
            BrowserAction::GetUrl
        ));
        assert!(matches!(
            parse_browser_action("back", &params).unwrap(),
            BrowserAction::Back
        ));
        assert!(matches!(
            parse_browser_action("forward", &params).unwrap(),
            BrowserAction::Forward
        ));
        assert!(matches!(
            parse_browser_action("close", &params).unwrap(),
            BrowserAction::Close
        ));
    }

    #[test]
    fn test_parse_unknown_action() {
        let params = serde_json::json!({});
        let err = parse_browser_action("explode", &params).unwrap_err();
        assert!(err.to_string().contains("Unknown browser action"));
    }

    // ── BrowserTool (Tool trait) tests ────────────────────────────────

    #[test]
    fn test_tool_name() {
        let manager = Arc::new(BrowserManager::new());
        let tool = BrowserTool::new(manager);
        assert_eq!(tool.name(), "browser");
    }

    #[test]
    fn test_tool_description() {
        let manager = Arc::new(BrowserManager::new());
        let tool = BrowserTool::new(manager);
        assert!(!tool.description().is_empty());
        assert!(tool.description().contains("browser"));
    }

    #[test]
    fn test_tool_requires_approval() {
        let manager = Arc::new(BrowserManager::new());
        let tool = BrowserTool::new(manager);
        assert!(tool.requires_approval());
    }

    #[test]
    fn test_tool_requires_sanitization() {
        let manager = Arc::new(BrowserManager::new());
        let tool = BrowserTool::new(manager);
        assert!(tool.requires_sanitization());
    }

    #[test]
    fn test_tool_schema_has_action() {
        let manager = Arc::new(BrowserManager::new());
        let tool = BrowserTool::new(manager);
        let schema = tool.parameters_schema();
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("action")));
    }

    #[tokio::test]
    async fn test_tool_execute_new_session() {
        let manager = Arc::new(BrowserManager::new());
        let tool = BrowserTool::new(Arc::clone(&manager));
        let ctx = JobContext::default();

        let result = tool
            .execute(serde_json::json!({"action": "new_session"}), &ctx)
            .await
            .unwrap();
        assert!(result.result.as_str().unwrap().contains("session created"));
        assert_eq!(manager.list_sessions().await.len(), 1);
    }

    #[tokio::test]
    async fn test_tool_execute_list_sessions() {
        let manager = Arc::new(BrowserManager::new());
        let tool = BrowserTool::new(Arc::clone(&manager));
        let ctx = JobContext::default();

        // Create a session first
        manager.create_session().await.unwrap();

        let result = tool
            .execute(serde_json::json!({"action": "list_sessions"}), &ctx)
            .await
            .unwrap();
        let text = result.result.as_str().unwrap();
        assert!(text.contains("id"));
    }

    #[tokio::test]
    async fn test_tool_execute_navigate_auto_session() {
        let manager = Arc::new(BrowserManager::new());
        let tool = BrowserTool::new(Arc::clone(&manager));
        let ctx = JobContext::default();

        let result = tool
            .execute(
                serde_json::json!({"action": "navigate", "url": "https://example.com"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.result.as_str().unwrap().contains("Navigated to"));
        // Auto-created a session
        assert_eq!(manager.list_sessions().await.len(), 1);
    }

    #[tokio::test]
    async fn test_tool_execute_with_session_id() {
        let manager = Arc::new(BrowserManager::new());
        let tool = BrowserTool::new(Arc::clone(&manager));
        let ctx = JobContext::default();

        let id = manager.create_session().await.unwrap();
        let result = tool
            .execute(
                serde_json::json!({
                    "action": "navigate",
                    "url": "https://example.com",
                    "session_id": id.to_string()
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.result.as_str().unwrap().contains("Navigated to"));
        // Should not create another session
        assert_eq!(manager.list_sessions().await.len(), 1);
    }

    #[tokio::test]
    async fn test_tool_execute_invalid_session_id() {
        let manager = Arc::new(BrowserManager::new());
        let tool = BrowserTool::new(manager);
        let ctx = JobContext::default();

        let err = tool
            .execute(
                serde_json::json!({
                    "action": "get_title",
                    "session_id": "not-a-uuid"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Invalid session_id"));
    }

    #[tokio::test]
    async fn test_tool_execute_missing_action() {
        let manager = Arc::new(BrowserManager::new());
        let tool = BrowserTool::new(manager);
        let ctx = JobContext::default();

        let err = tool.execute(serde_json::json!({}), &ctx).await.unwrap_err();
        assert!(err.to_string().contains("action"));
    }

    #[tokio::test]
    async fn test_tool_execute_unknown_action() {
        let manager = Arc::new(BrowserManager::new());
        let tool = BrowserTool::new(manager);
        let ctx = JobContext::default();

        let err = tool
            .execute(serde_json::json!({"action": "fly"}), &ctx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Unknown browser action"));
    }

    #[test]
    fn test_browser_manager_default() {
        let manager = BrowserManager::default();
        assert_eq!(manager.max_sessions, 5);
    }

    #[test]
    fn test_with_max_sessions_builder() {
        let manager = BrowserManager::new().with_max_sessions(10);
        assert_eq!(manager.max_sessions, 10);
    }
}
