//! Canvas hosting for agent-driven UI (A2UI).
//!
//! Allows the agent to create, update, and serve dynamic HTML/JS/CSS
//! canvases that run in the browser alongside the chat interface.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Default maximum number of canvases.
const DEFAULT_MAX_CANVASES: usize = 50;

/// Default maximum content size in bytes (1 MB).
const DEFAULT_MAX_CONTENT_SIZE: usize = 1_048_576;

/// A canvas is an agent-generated web component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Canvas {
    pub id: Uuid,
    pub title: String,
    pub content_type: CanvasContentType,
    pub html: String,
    pub css: Option<String>,
    pub js: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: u32,
    pub pinned: bool,
}

/// The type of content a canvas holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanvasContentType {
    Html,
    Markdown,
    Chart,
    Form,
    Table,
    Code,
    Custom,
}

impl CanvasContentType {
    /// Parse a content type from a string.
    pub fn from_str_loose(s: &str) -> Result<Self, CanvasError> {
        match s.to_lowercase().as_str() {
            "html" => Ok(Self::Html),
            "markdown" | "md" => Ok(Self::Markdown),
            "chart" => Ok(Self::Chart),
            "form" => Ok(Self::Form),
            "table" => Ok(Self::Table),
            "code" => Ok(Self::Code),
            "custom" => Ok(Self::Custom),
            other => Err(CanvasError::InvalidContent {
                reason: format!("Unknown content type: {other}"),
            }),
        }
    }
}

/// Manages active canvases.
pub struct CanvasManager {
    canvases: Arc<RwLock<HashMap<Uuid, Canvas>>>,
    max_canvases: usize,
    max_content_size: usize,
}

impl CanvasManager {
    /// Create a new canvas manager with default limits.
    pub fn new() -> Self {
        Self {
            canvases: Arc::new(RwLock::new(HashMap::new())),
            max_canvases: DEFAULT_MAX_CANVASES,
            max_content_size: DEFAULT_MAX_CONTENT_SIZE,
        }
    }

    /// Create a new canvas manager with custom limits.
    pub fn with_limits(max_canvases: usize, max_content_size: usize) -> Self {
        Self {
            canvases: Arc::new(RwLock::new(HashMap::new())),
            max_canvases,
            max_content_size,
        }
    }

    /// Create a new canvas.
    pub async fn create(
        &self,
        title: String,
        content_type: CanvasContentType,
        html: String,
        css: Option<String>,
        js: Option<String>,
    ) -> Result<Canvas, CanvasError> {
        self.validate_content_size(&html, &css, &js)?;

        let mut canvases = self.canvases.write().await;

        if canvases.len() >= self.max_canvases {
            return Err(CanvasError::LimitReached {
                max: self.max_canvases,
            });
        }

        let now = Utc::now();
        let canvas = Canvas {
            id: Uuid::new_v4(),
            title,
            content_type,
            html,
            css,
            js,
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
            version: 1,
            pinned: false,
        };

        canvases.insert(canvas.id, canvas.clone());

        Ok(canvas)
    }

    /// Update an existing canvas. Only provided fields are changed.
    pub async fn update(
        &self,
        id: Uuid,
        html: Option<String>,
        css: Option<String>,
        js: Option<String>,
    ) -> Result<Canvas, CanvasError> {
        let mut canvases = self.canvases.write().await;

        let canvas = canvases.get_mut(&id).ok_or(CanvasError::NotFound { id })?;

        // Validate sizes using the values that will actually be stored.
        let effective_html = html.as_deref().unwrap_or(&canvas.html);
        let effective_css = css.as_ref().or(canvas.css.as_ref());
        let effective_js = js.as_ref().or(canvas.js.as_ref());

        self.validate_content_size(
            effective_html,
            &effective_css.cloned(),
            &effective_js.cloned(),
        )?;

        if let Some(h) = html {
            canvas.html = h;
        }
        if let Some(c) = css {
            canvas.css = Some(c);
        }
        if let Some(j) = js {
            canvas.js = Some(j);
        }

        canvas.version += 1;
        canvas.updated_at = Utc::now();

        Ok(canvas.clone())
    }

    /// Get a canvas by ID.
    pub async fn get(&self, id: Uuid) -> Option<Canvas> {
        self.canvases.read().await.get(&id).cloned()
    }

    /// List all canvases, sorted by creation time (newest first).
    pub async fn list(&self) -> Vec<Canvas> {
        let canvases = self.canvases.read().await;
        let mut list: Vec<Canvas> = canvases.values().cloned().collect();
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        list
    }

    /// Delete a canvas by ID. Returns `true` if it existed.
    pub async fn delete(&self, id: Uuid) -> bool {
        self.canvases.write().await.remove(&id).is_some()
    }

    /// Pin or unpin a canvas.
    pub async fn pin(&self, id: Uuid, pinned: bool) -> Result<(), CanvasError> {
        let mut canvases = self.canvases.write().await;
        let canvas = canvases.get_mut(&id).ok_or(CanvasError::NotFound { id })?;
        canvas.pinned = pinned;
        canvas.updated_at = Utc::now();
        Ok(())
    }

    /// Render a canvas as a complete HTML page.
    pub async fn render(&self, id: Uuid) -> Result<String, CanvasError> {
        let canvases = self.canvases.read().await;
        let canvas = canvases.get(&id).ok_or(CanvasError::NotFound { id })?;

        let css_block = canvas
            .css
            .as_deref()
            .map(|css| format!("<style>\n{css}\n</style>"))
            .unwrap_or_default();

        let js_block = canvas
            .js
            .as_deref()
            .map(|js| format!("<script>\n{js}\n</script>"))
            .unwrap_or_default();

        let html = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title}</title>
{css_block}
</head>
<body>
{body}
{js_block}
</body>
</html>"#,
            title = html_escape(&canvas.title),
            css_block = css_block,
            body = canvas.html,
            js_block = js_block,
        );

        Ok(html)
    }

    /// Validate that total content size is within limits.
    fn validate_content_size(
        &self,
        html: &str,
        css: &Option<String>,
        js: &Option<String>,
    ) -> Result<(), CanvasError> {
        let total =
            html.len() + css.as_ref().map_or(0, |c| c.len()) + js.as_ref().map_or(0, |j| j.len());

        if total > self.max_content_size {
            return Err(CanvasError::ContentTooLarge {
                size: total,
                max: self.max_content_size,
            });
        }

        Ok(())
    }
}

impl Default for CanvasManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Escape HTML special characters in a string (for use in title tags, etc.).
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Canvas operation errors.
#[derive(Debug, thiserror::Error)]
pub enum CanvasError {
    #[error("Canvas not found: {id}")]
    NotFound { id: Uuid },

    #[error("Maximum canvas count reached: {max}")]
    LimitReached { max: usize },

    #[error("Content too large: {size} bytes exceeds {max} byte limit")]
    ContentTooLarge { size: usize, max: usize },

    #[error("Invalid canvas content: {reason}")]
    InvalidContent { reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Create ---

    #[tokio::test]
    async fn test_create_canvas() {
        let mgr = CanvasManager::new();
        let canvas = mgr
            .create(
                "Dashboard".to_string(),
                CanvasContentType::Html,
                "<h1>Hello</h1>".to_string(),
                None,
                None,
            )
            .await
            .unwrap();

        assert_eq!(canvas.title, "Dashboard");
        assert_eq!(canvas.content_type, CanvasContentType::Html);
        assert_eq!(canvas.html, "<h1>Hello</h1>");
        assert!(canvas.css.is_none());
        assert!(canvas.js.is_none());
        assert_eq!(canvas.version, 1);
        assert!(!canvas.pinned);
        assert!(canvas.metadata.is_empty());
    }

    #[tokio::test]
    async fn test_create_canvas_with_css_and_js() {
        let mgr = CanvasManager::new();
        let canvas = mgr
            .create(
                "Styled Page".to_string(),
                CanvasContentType::Custom,
                "<div>Content</div>".to_string(),
                Some("body { color: red; }".to_string()),
                Some("console.log('hi');".to_string()),
            )
            .await
            .unwrap();

        assert_eq!(canvas.css.as_deref(), Some("body { color: red; }"));
        assert_eq!(canvas.js.as_deref(), Some("console.log('hi');"));
    }

    #[tokio::test]
    async fn test_create_canvas_assigns_unique_ids() {
        let mgr = CanvasManager::new();
        let c1 = mgr
            .create(
                "A".to_string(),
                CanvasContentType::Html,
                "<p>1</p>".to_string(),
                None,
                None,
            )
            .await
            .unwrap();
        let c2 = mgr
            .create(
                "B".to_string(),
                CanvasContentType::Html,
                "<p>2</p>".to_string(),
                None,
                None,
            )
            .await
            .unwrap();

        assert_ne!(c1.id, c2.id);
    }

    #[tokio::test]
    async fn test_create_canvas_timestamps() {
        let mgr = CanvasManager::new();
        let before = Utc::now();
        let canvas = mgr
            .create(
                "T".to_string(),
                CanvasContentType::Html,
                "<p>test</p>".to_string(),
                None,
                None,
            )
            .await
            .unwrap();
        let after = Utc::now();

        assert!(canvas.created_at >= before && canvas.created_at <= after);
        assert_eq!(canvas.created_at, canvas.updated_at);
    }

    // --- Get ---

    #[tokio::test]
    async fn test_get_existing_canvas() {
        let mgr = CanvasManager::new();
        let created = mgr
            .create(
                "Test".to_string(),
                CanvasContentType::Html,
                "<p>hi</p>".to_string(),
                None,
                None,
            )
            .await
            .unwrap();

        let fetched = mgr.get(created.id).await;
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, created.id);
    }

    #[tokio::test]
    async fn test_get_nonexistent_canvas() {
        let mgr = CanvasManager::new();
        let result = mgr.get(Uuid::new_v4()).await;
        assert!(result.is_none());
    }

    // --- Update ---

    #[tokio::test]
    async fn test_update_html_only() {
        let mgr = CanvasManager::new();
        let canvas = mgr
            .create(
                "U".to_string(),
                CanvasContentType::Html,
                "<p>old</p>".to_string(),
                Some("body {}".to_string()),
                Some("alert(1)".to_string()),
            )
            .await
            .unwrap();

        let updated = mgr
            .update(canvas.id, Some("<p>new</p>".to_string()), None, None)
            .await
            .unwrap();

        assert_eq!(updated.html, "<p>new</p>");
        // CSS and JS remain unchanged
        assert_eq!(updated.css.as_deref(), Some("body {}"));
        assert_eq!(updated.js.as_deref(), Some("alert(1)"));
    }

    #[tokio::test]
    async fn test_update_increments_version() {
        let mgr = CanvasManager::new();
        let canvas = mgr
            .create(
                "V".to_string(),
                CanvasContentType::Html,
                "<p>v1</p>".to_string(),
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(canvas.version, 1);

        let v2 = mgr
            .update(canvas.id, Some("<p>v2</p>".to_string()), None, None)
            .await
            .unwrap();
        assert_eq!(v2.version, 2);

        let v3 = mgr
            .update(canvas.id, Some("<p>v3</p>".to_string()), None, None)
            .await
            .unwrap();
        assert_eq!(v3.version, 3);
    }

    #[tokio::test]
    async fn test_update_advances_timestamp() {
        let mgr = CanvasManager::new();
        let canvas = mgr
            .create(
                "T".to_string(),
                CanvasContentType::Html,
                "<p>old</p>".to_string(),
                None,
                None,
            )
            .await
            .unwrap();

        // Small delay to ensure timestamps differ
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let updated = mgr
            .update(canvas.id, Some("<p>new</p>".to_string()), None, None)
            .await
            .unwrap();
        assert!(updated.updated_at > canvas.updated_at);
        // created_at should not change
        assert_eq!(updated.created_at, canvas.created_at);
    }

    #[tokio::test]
    async fn test_update_nonexistent_returns_not_found() {
        let mgr = CanvasManager::new();
        let result = mgr
            .update(Uuid::new_v4(), Some("<p>nope</p>".to_string()), None, None)
            .await;
        assert!(matches!(result, Err(CanvasError::NotFound { .. })));
    }

    #[tokio::test]
    async fn test_update_css_and_js() {
        let mgr = CanvasManager::new();
        let canvas = mgr
            .create(
                "S".to_string(),
                CanvasContentType::Html,
                "<p>body</p>".to_string(),
                None,
                None,
            )
            .await
            .unwrap();

        let updated = mgr
            .update(
                canvas.id,
                None,
                Some("h1 { color: blue; }".to_string()),
                Some("console.log('updated');".to_string()),
            )
            .await
            .unwrap();

        assert_eq!(updated.html, "<p>body</p>"); // unchanged
        assert_eq!(updated.css.as_deref(), Some("h1 { color: blue; }"));
        assert_eq!(updated.js.as_deref(), Some("console.log('updated');"));
    }

    // --- Delete ---

    #[tokio::test]
    async fn test_delete_existing_canvas() {
        let mgr = CanvasManager::new();
        let canvas = mgr
            .create(
                "D".to_string(),
                CanvasContentType::Html,
                "<p>bye</p>".to_string(),
                None,
                None,
            )
            .await
            .unwrap();

        assert!(mgr.delete(canvas.id).await);
        assert!(mgr.get(canvas.id).await.is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_returns_false() {
        let mgr = CanvasManager::new();
        assert!(!mgr.delete(Uuid::new_v4()).await);
    }

    // --- List ---

    #[tokio::test]
    async fn test_list_empty() {
        let mgr = CanvasManager::new();
        let list = mgr.list().await;
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_list_returns_all_canvases() {
        let mgr = CanvasManager::new();
        mgr.create(
            "A".to_string(),
            CanvasContentType::Html,
            "<p>1</p>".to_string(),
            None,
            None,
        )
        .await
        .unwrap();
        mgr.create(
            "B".to_string(),
            CanvasContentType::Chart,
            "<p>2</p>".to_string(),
            None,
            None,
        )
        .await
        .unwrap();
        mgr.create(
            "C".to_string(),
            CanvasContentType::Form,
            "<p>3</p>".to_string(),
            None,
            None,
        )
        .await
        .unwrap();

        let list = mgr.list().await;
        assert_eq!(list.len(), 3);
    }

    #[tokio::test]
    async fn test_list_sorted_newest_first() {
        let mgr = CanvasManager::new();
        let c1 = mgr
            .create(
                "First".to_string(),
                CanvasContentType::Html,
                "<p>1</p>".to_string(),
                None,
                None,
            )
            .await
            .unwrap();

        // Small delay to ensure ordering
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let c2 = mgr
            .create(
                "Second".to_string(),
                CanvasContentType::Html,
                "<p>2</p>".to_string(),
                None,
                None,
            )
            .await
            .unwrap();

        let list = mgr.list().await;
        assert_eq!(list[0].id, c2.id);
        assert_eq!(list[1].id, c1.id);
    }

    // --- Pin ---

    #[tokio::test]
    async fn test_pin_canvas() {
        let mgr = CanvasManager::new();
        let canvas = mgr
            .create(
                "P".to_string(),
                CanvasContentType::Html,
                "<p>pin me</p>".to_string(),
                None,
                None,
            )
            .await
            .unwrap();
        assert!(!canvas.pinned);

        mgr.pin(canvas.id, true).await.unwrap();
        let pinned = mgr.get(canvas.id).await.unwrap();
        assert!(pinned.pinned);

        mgr.pin(canvas.id, false).await.unwrap();
        let unpinned = mgr.get(canvas.id).await.unwrap();
        assert!(!unpinned.pinned);
    }

    #[tokio::test]
    async fn test_pin_nonexistent_returns_not_found() {
        let mgr = CanvasManager::new();
        let result = mgr.pin(Uuid::new_v4(), true).await;
        assert!(matches!(result, Err(CanvasError::NotFound { .. })));
    }

    // --- Limits ---

    #[tokio::test]
    async fn test_max_canvases_enforced() {
        let mgr = CanvasManager::with_limits(2, DEFAULT_MAX_CONTENT_SIZE);

        mgr.create(
            "A".to_string(),
            CanvasContentType::Html,
            "<p>1</p>".to_string(),
            None,
            None,
        )
        .await
        .unwrap();
        mgr.create(
            "B".to_string(),
            CanvasContentType::Html,
            "<p>2</p>".to_string(),
            None,
            None,
        )
        .await
        .unwrap();

        let result = mgr
            .create(
                "C".to_string(),
                CanvasContentType::Html,
                "<p>3</p>".to_string(),
                None,
                None,
            )
            .await;

        assert!(matches!(result, Err(CanvasError::LimitReached { max: 2 })));
    }

    #[tokio::test]
    async fn test_max_content_size_on_create() {
        let mgr = CanvasManager::with_limits(DEFAULT_MAX_CANVASES, 100);

        let big_html = "x".repeat(101);
        let result = mgr
            .create(
                "Big".to_string(),
                CanvasContentType::Html,
                big_html,
                None,
                None,
            )
            .await;

        assert!(matches!(
            result,
            Err(CanvasError::ContentTooLarge {
                size: 101,
                max: 100
            })
        ));
    }

    #[tokio::test]
    async fn test_max_content_size_includes_css_and_js() {
        let mgr = CanvasManager::with_limits(DEFAULT_MAX_CANVASES, 100);

        // HTML(30) + CSS(40) + JS(40) = 110 > 100
        let result = mgr
            .create(
                "Combined".to_string(),
                CanvasContentType::Html,
                "x".repeat(30),
                Some("y".repeat(40)),
                Some("z".repeat(40)),
            )
            .await;

        assert!(matches!(
            result,
            Err(CanvasError::ContentTooLarge {
                size: 110,
                max: 100
            })
        ));
    }

    #[tokio::test]
    async fn test_max_content_size_on_update() {
        let mgr = CanvasManager::with_limits(DEFAULT_MAX_CANVASES, 100);

        let canvas = mgr
            .create(
                "Small".to_string(),
                CanvasContentType::Html,
                "ok".to_string(),
                None,
                None,
            )
            .await
            .unwrap();

        let big_html = "x".repeat(101);
        let result = mgr.update(canvas.id, Some(big_html), None, None).await;

        assert!(matches!(result, Err(CanvasError::ContentTooLarge { .. })));
    }

    // --- Render ---

    #[tokio::test]
    async fn test_render_basic_html() {
        let mgr = CanvasManager::new();
        let canvas = mgr
            .create(
                "Render Test".to_string(),
                CanvasContentType::Html,
                "<h1>Hello World</h1>".to_string(),
                None,
                None,
            )
            .await
            .unwrap();

        let rendered = mgr.render(canvas.id).await.unwrap();

        assert!(rendered.contains("<!DOCTYPE html>"));
        assert!(rendered.contains("<title>Render Test</title>"));
        assert!(rendered.contains("<h1>Hello World</h1>"));
        // No style or script blocks when CSS/JS are None
        assert!(!rendered.contains("<style>"));
        assert!(!rendered.contains("<script>"));
    }

    #[tokio::test]
    async fn test_render_with_css_and_js() {
        let mgr = CanvasManager::new();
        let canvas = mgr
            .create(
                "Full".to_string(),
                CanvasContentType::Html,
                "<div id=\"app\">App</div>".to_string(),
                Some("body { margin: 0; }".to_string()),
                Some("document.getElementById('app');".to_string()),
            )
            .await
            .unwrap();

        let rendered = mgr.render(canvas.id).await.unwrap();

        assert!(rendered.contains("<style>\nbody { margin: 0; }\n</style>"));
        assert!(rendered.contains("<script>\ndocument.getElementById('app');\n</script>"));
        assert!(rendered.contains("<div id=\"app\">App</div>"));
    }

    #[tokio::test]
    async fn test_render_escapes_title() {
        let mgr = CanvasManager::new();
        let canvas = mgr
            .create(
                "<script>alert('xss')</script>".to_string(),
                CanvasContentType::Html,
                "<p>safe</p>".to_string(),
                None,
                None,
            )
            .await
            .unwrap();

        let rendered = mgr.render(canvas.id).await.unwrap();
        assert!(rendered.contains("&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;"));
        assert!(!rendered.contains("<title><script>"));
    }

    #[tokio::test]
    async fn test_render_nonexistent_returns_not_found() {
        let mgr = CanvasManager::new();
        let result = mgr.render(Uuid::new_v4()).await;
        assert!(matches!(result, Err(CanvasError::NotFound { .. })));
    }

    // --- Content type parsing ---

    #[test]
    fn test_content_type_from_str_loose() {
        assert_eq!(
            CanvasContentType::from_str_loose("html").unwrap(),
            CanvasContentType::Html
        );
        assert_eq!(
            CanvasContentType::from_str_loose("HTML").unwrap(),
            CanvasContentType::Html
        );
        assert_eq!(
            CanvasContentType::from_str_loose("markdown").unwrap(),
            CanvasContentType::Markdown
        );
        assert_eq!(
            CanvasContentType::from_str_loose("md").unwrap(),
            CanvasContentType::Markdown
        );
        assert_eq!(
            CanvasContentType::from_str_loose("chart").unwrap(),
            CanvasContentType::Chart
        );
        assert_eq!(
            CanvasContentType::from_str_loose("form").unwrap(),
            CanvasContentType::Form
        );
        assert_eq!(
            CanvasContentType::from_str_loose("table").unwrap(),
            CanvasContentType::Table
        );
        assert_eq!(
            CanvasContentType::from_str_loose("code").unwrap(),
            CanvasContentType::Code
        );
        assert_eq!(
            CanvasContentType::from_str_loose("custom").unwrap(),
            CanvasContentType::Custom
        );
    }

    #[test]
    fn test_content_type_from_str_invalid() {
        let result = CanvasContentType::from_str_loose("unknown_type");
        assert!(matches!(result, Err(CanvasError::InvalidContent { .. })));
    }

    // --- Serialization ---

    #[test]
    fn test_canvas_content_type_serialization() {
        let json = serde_json::to_string(&CanvasContentType::Html).unwrap();
        assert_eq!(json, "\"html\"");

        let json = serde_json::to_string(&CanvasContentType::Markdown).unwrap();
        assert_eq!(json, "\"markdown\"");

        let deserialized: CanvasContentType = serde_json::from_str("\"chart\"").unwrap();
        assert_eq!(deserialized, CanvasContentType::Chart);
    }

    #[test]
    fn test_canvas_serialization_roundtrip() {
        let canvas = Canvas {
            id: Uuid::new_v4(),
            title: "Test".to_string(),
            content_type: CanvasContentType::Form,
            html: "<form></form>".to_string(),
            css: Some("form { display: flex; }".to_string()),
            js: None,
            metadata: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 3,
            pinned: true,
        };

        let json = serde_json::to_string(&canvas).unwrap();
        let deserialized: Canvas = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, canvas.id);
        assert_eq!(deserialized.title, canvas.title);
        assert_eq!(deserialized.content_type, canvas.content_type);
        assert_eq!(deserialized.version, canvas.version);
        assert_eq!(deserialized.pinned, canvas.pinned);
    }

    // --- Concurrent access ---

    #[tokio::test]
    async fn test_concurrent_creates() {
        let mgr = Arc::new(CanvasManager::new());
        let mut handles = vec![];

        for i in 0..20 {
            let mgr = Arc::clone(&mgr);
            handles.push(tokio::spawn(async move {
                mgr.create(
                    format!("Canvas {i}"),
                    CanvasContentType::Html,
                    format!("<p>{i}</p>"),
                    None,
                    None,
                )
                .await
                .unwrap()
            }));
        }

        let results: Vec<Canvas> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        // All should have unique IDs
        let ids: std::collections::HashSet<Uuid> = results.iter().map(|c| c.id).collect();
        assert_eq!(ids.len(), 20);

        // List should return all 20
        assert_eq!(mgr.list().await.len(), 20);
    }

    #[tokio::test]
    async fn test_concurrent_create_and_delete() {
        let mgr = Arc::new(CanvasManager::new());

        // Create some canvases first
        let mut canvas_ids = vec![];
        for i in 0..10 {
            let c = mgr
                .create(
                    format!("Canvas {i}"),
                    CanvasContentType::Html,
                    format!("<p>{i}</p>"),
                    None,
                    None,
                )
                .await
                .unwrap();
            canvas_ids.push(c.id);
        }

        // Concurrently delete even-indexed and create new ones
        let mut handles = vec![];
        for (i, id) in canvas_ids.iter().enumerate() {
            if i % 2 == 0 {
                let mgr = Arc::clone(&mgr);
                let id = *id;
                handles.push(tokio::spawn(async move {
                    mgr.delete(id).await;
                }));
            }
        }
        for i in 10..15 {
            let mgr = Arc::clone(&mgr);
            handles.push(tokio::spawn(async move {
                mgr.create(
                    format!("New {i}"),
                    CanvasContentType::Html,
                    format!("<p>{i}</p>"),
                    None,
                    None,
                )
                .await
                .unwrap();
            }));
        }

        futures::future::join_all(handles).await;

        // 10 original - 5 deleted + 5 new = 10
        assert_eq!(mgr.list().await.len(), 10);
    }

    #[tokio::test]
    async fn test_concurrent_updates() {
        let mgr = Arc::new(CanvasManager::new());
        let canvas = mgr
            .create(
                "Concurrent".to_string(),
                CanvasContentType::Html,
                "<p>start</p>".to_string(),
                None,
                None,
            )
            .await
            .unwrap();

        let mut handles = vec![];
        for i in 0..10 {
            let mgr = Arc::clone(&mgr);
            let id = canvas.id;
            handles.push(tokio::spawn(async move {
                mgr.update(id, Some(format!("<p>update {i}</p>")), None, None)
                    .await
                    .unwrap()
            }));
        }

        let results: Vec<Canvas> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        // All versions should be unique (serialized by the RwLock)
        let versions: std::collections::HashSet<u32> = results.iter().map(|c| c.version).collect();
        assert_eq!(versions.len(), 10);

        // Final version should be 11 (1 initial + 10 updates)
        let final_canvas = mgr.get(canvas.id).await.unwrap();
        assert_eq!(final_canvas.version, 11);
    }

    // --- Default ---

    #[test]
    fn test_canvas_manager_default() {
        let mgr = CanvasManager::default();
        assert_eq!(mgr.max_canvases, DEFAULT_MAX_CANVASES);
        assert_eq!(mgr.max_content_size, DEFAULT_MAX_CONTENT_SIZE);
    }

    // --- Error display ---

    #[test]
    fn test_canvas_error_display() {
        let id = Uuid::new_v4();
        let err = CanvasError::NotFound { id };
        assert_eq!(err.to_string(), format!("Canvas not found: {id}"));

        let err = CanvasError::LimitReached { max: 50 };
        assert_eq!(err.to_string(), "Maximum canvas count reached: 50");

        let err = CanvasError::ContentTooLarge {
            size: 2000,
            max: 1000,
        };
        assert_eq!(
            err.to_string(),
            "Content too large: 2000 bytes exceeds 1000 byte limit"
        );

        let err = CanvasError::InvalidContent {
            reason: "bad".to_string(),
        };
        assert_eq!(err.to_string(), "Invalid canvas content: bad");
    }

    // --- html_escape ---

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a&b"), "a&amp;b");
        assert_eq!(html_escape("\"hello\""), "&quot;hello&quot;");
        assert_eq!(html_escape("it's"), "it&#x27;s");
        assert_eq!(html_escape("safe text"), "safe text");
    }

    // --- Delete after create allows re-creation within limits ---

    #[tokio::test]
    async fn test_delete_frees_slot_for_new_canvas() {
        let mgr = CanvasManager::with_limits(1, DEFAULT_MAX_CONTENT_SIZE);

        let c = mgr
            .create(
                "Only".to_string(),
                CanvasContentType::Html,
                "<p>1</p>".to_string(),
                None,
                None,
            )
            .await
            .unwrap();

        // Can't create another
        let result = mgr
            .create(
                "Over".to_string(),
                CanvasContentType::Html,
                "<p>2</p>".to_string(),
                None,
                None,
            )
            .await;
        assert!(matches!(result, Err(CanvasError::LimitReached { .. })));

        // Delete the first one
        assert!(mgr.delete(c.id).await);

        // Now we can create again
        mgr.create(
            "Replacement".to_string(),
            CanvasContentType::Html,
            "<p>3</p>".to_string(),
            None,
            None,
        )
        .await
        .unwrap();
    }
}
