//! Memory tools for persistent workspace memory.
//!
//! These tools allow the agent to:
//! - Search past memories, decisions, and context
//! - Read and write files in the workspace
//!
//! # Usage
//!
//! The agent should use `memory_search` before answering questions about
//! prior work, decisions, dates, people, preferences, or todos.
//!
//! Use `memory_write` to persist important facts that should be remembered
//! across sessions.

use std::sync::Arc;

use async_trait::async_trait;

use crate::context::JobContext;
use crate::tools::tool::{Tool, ToolError, ToolOutput};
use crate::workspace::{ConnectionType, ProfileType, Workspace, paths};

/// Identity files that the LLM must not overwrite via tool calls.
/// These are loaded into the system prompt and could be used for prompt
/// injection if an attacker tricks the agent into overwriting them.
const PROTECTED_IDENTITY_FILES: &[&str] =
    &[paths::IDENTITY, paths::SOUL, paths::AGENTS, paths::USER];

/// Tool for searching workspace memory.
///
/// Performs hybrid search (FTS + semantic) across all memory documents.
/// The agent should call this tool before answering questions about
/// prior work, decisions, preferences, or any historical context.
pub struct MemorySearchTool {
    workspace: Arc<Workspace>,
}

impl MemorySearchTool {
    /// Create a new memory search tool.
    pub fn new(workspace: Arc<Workspace>) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for MemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn description(&self) -> &str {
        "Search past memories, decisions, and context. MUST be called before answering \
         questions about prior work, decisions, dates, people, preferences, or todos. \
         Returns relevant snippets with relevance scores."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query. Use natural language to describe what you're looking for."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5, max: 20)",
                    "default": 5,
                    "minimum": 1,
                    "maximum": 20
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("missing 'query' parameter".to_string()))?;

        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .min(20) as usize;

        let results = self
            .workspace
            .search(query, limit)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Search failed: {}", e)))?;

        let output = serde_json::json!({
            "query": query,
            "results": results.iter().map(|r| serde_json::json!({
                "content": r.content,
                "score": r.score,
                "document_id": r.document_id.to_string(),
                "is_hybrid_match": r.is_hybrid(),
            })).collect::<Vec<_>>(),
            "result_count": results.len(),
        });

        Ok(ToolOutput::success(output, start.elapsed()))
    }

    fn requires_sanitization(&self) -> bool {
        false // Internal memory, trusted content
    }
}

/// Tool for writing to workspace memory.
///
/// Use this to persist important information that should be remembered
/// across sessions: decisions, preferences, facts, lessons learned.
pub struct MemoryWriteTool {
    workspace: Arc<Workspace>,
}

impl MemoryWriteTool {
    /// Create a new memory write tool.
    pub fn new(workspace: Arc<Workspace>) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for MemoryWriteTool {
    fn name(&self) -> &str {
        "memory_write"
    }

    fn description(&self) -> &str {
        "Write to persistent memory (database-backed, NOT the local filesystem). \
         Use for important facts, decisions, preferences, or lessons learned that should \
         be remembered across sessions. Targets: 'memory' for curated long-term facts, \
         'daily_log' for timestamped session notes, 'heartbeat' for the periodic \
         checklist (HEARTBEAT.md), or provide a custom path for arbitrary file creation."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The content to write to memory. Be concise but include relevant context."
                },
                "target": {
                    "type": "string",
                    "description": "Where to write: 'memory' for MEMORY.md, 'daily_log' for today's log, 'heartbeat' for HEARTBEAT.md checklist, or a path like 'projects/alpha/notes.md'",
                    "default": "daily_log"
                },
                "append": {
                    "type": "boolean",
                    "description": "If true, append to existing content. If false, replace entirely.",
                    "default": true
                }
            },
            "required": ["content"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::InvalidParameters("missing 'content' parameter".to_string())
            })?;

        if content.trim().is_empty() {
            return Err(ToolError::InvalidParameters(
                "content cannot be empty".to_string(),
            ));
        }

        let target = params
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("daily_log");

        // Reject writes to identity files that are loaded into the system prompt.
        // An attacker could use prompt injection to trick the agent into overwriting
        // these, poisoning future conversations.
        if PROTECTED_IDENTITY_FILES.contains(&target) {
            return Err(ToolError::NotAuthorized(format!(
                "writing to '{}' is not allowed (identity file protected from tool writes)",
                target,
            )));
        }

        let append = params
            .get("append")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let path = match target {
            "memory" => {
                if append {
                    self.workspace
                        .append_memory(content)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed(format!("Write failed: {}", e)))?;
                } else {
                    self.workspace
                        .write(paths::MEMORY, content)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed(format!("Write failed: {}", e)))?;
                }
                paths::MEMORY.to_string()
            }
            "daily_log" => {
                self.workspace
                    .append_daily_log(content)
                    .await
                    .map_err(|e| ToolError::ExecutionFailed(format!("Write failed: {}", e)))?;
                format!("daily/{}.md", chrono::Utc::now().format("%Y-%m-%d"))
            }
            "heartbeat" => {
                if append {
                    self.workspace
                        .append(paths::HEARTBEAT, content)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed(format!("Write failed: {}", e)))?;
                } else {
                    self.workspace
                        .write(paths::HEARTBEAT, content)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed(format!("Write failed: {}", e)))?;
                }
                paths::HEARTBEAT.to_string()
            }
            path => {
                // Protect identity files from LLM overwrites (prompt injection defense).
                // These files are injected into the system prompt, so poisoning them
                // would let an attacker rewrite the agent's core instructions.
                let normalized = path.trim_start_matches('/');
                if PROTECTED_IDENTITY_FILES
                    .iter()
                    .any(|p| normalized.eq_ignore_ascii_case(p))
                {
                    return Err(ToolError::NotAuthorized(format!(
                        "writing to '{}' is not allowed (identity file protected from tool access)",
                        path
                    )));
                }

                if append {
                    self.workspace
                        .append(path, content)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed(format!("Write failed: {}", e)))?;
                } else {
                    self.workspace
                        .write(path, content)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed(format!("Write failed: {}", e)))?;
                }
                path.to_string()
            }
        };

        let output = serde_json::json!({
            "status": "written",
            "path": path,
            "append": append,
            "content_length": content.len(),
        });

        Ok(ToolOutput::success(output, start.elapsed()))
    }

    fn requires_sanitization(&self) -> bool {
        false // Internal tool
    }
}

/// Tool for reading workspace files.
///
/// Use this to read the full content of any file in the workspace.
pub struct MemoryReadTool {
    workspace: Arc<Workspace>,
}

impl MemoryReadTool {
    /// Create a new memory read tool.
    pub fn new(workspace: Arc<Workspace>) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for MemoryReadTool {
    fn name(&self) -> &str {
        "memory_read"
    }

    fn description(&self) -> &str {
        "Read a file from the workspace memory (database-backed storage). \
         Use this to read files shown by memory_tree. NOT for local filesystem files \
         (use read_file for those). Works with identity files, heartbeat checklist, \
         memory, daily logs, or any custom workspace path."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file (e.g., 'MEMORY.md', 'daily/2024-01-15.md', 'projects/alpha/notes.md')"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("missing 'path' parameter".to_string()))?;

        let doc = self
            .workspace
            .read(path)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Read failed: {}", e)))?;

        let output = serde_json::json!({
            "path": doc.path,
            "content": doc.content,
            "word_count": doc.word_count(),
            "updated_at": doc.updated_at.to_rfc3339(),
        });

        Ok(ToolOutput::success(output, start.elapsed()))
    }

    fn requires_sanitization(&self) -> bool {
        false // Internal memory
    }
}

/// Tool for viewing workspace structure as a tree.
///
/// Returns a hierarchical view of files and directories with configurable depth.
pub struct MemoryTreeTool {
    workspace: Arc<Workspace>,
}

impl MemoryTreeTool {
    /// Create a new memory tree tool.
    pub fn new(workspace: Arc<Workspace>) -> Self {
        Self { workspace }
    }

    /// Recursively build tree structure.
    ///
    /// Returns a compact format where directories end with `/` and may have children.
    async fn build_tree(
        &self,
        path: &str,
        current_depth: usize,
        max_depth: usize,
    ) -> Result<Vec<serde_json::Value>, ToolError> {
        if current_depth > max_depth {
            return Ok(Vec::new());
        }

        let entries = self
            .workspace
            .list(path)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Tree failed: {}", e)))?;

        let mut result = Vec::new();
        for entry in entries {
            // Directories end with `/`, files don't
            let display_path = if entry.is_directory {
                format!("{}/", entry.name())
            } else {
                entry.name().to_string()
            };

            if entry.is_directory && current_depth < max_depth {
                let children =
                    Box::pin(self.build_tree(&entry.path, current_depth + 1, max_depth)).await?;
                if children.is_empty() {
                    result.push(serde_json::Value::String(display_path));
                } else {
                    result.push(serde_json::json!({ display_path: children }));
                }
            } else {
                result.push(serde_json::Value::String(display_path));
            }
        }

        Ok(result)
    }
}

#[async_trait]
impl Tool for MemoryTreeTool {
    fn name(&self) -> &str {
        "memory_tree"
    }

    fn description(&self) -> &str {
        "View the workspace memory structure as a tree (database-backed storage). \
         Use memory_read to read files shown here, NOT read_file. \
         The workspace is separate from the local filesystem."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Root path to start from (empty string for workspace root)",
                    "default": ""
                },
                "depth": {
                    "type": "integer",
                    "description": "Maximum depth to traverse (1 = immediate children only)",
                    "default": 1,
                    "minimum": 1,
                    "maximum": 10
                }
            }
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let path = params.get("path").and_then(|v| v.as_str()).unwrap_or("");

        let depth = params
            .get("depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .clamp(1, 10) as usize;

        let tree = self.build_tree(path, 1, depth).await?;

        // Compact output: just the tree array
        Ok(ToolOutput::success(
            serde_json::Value::Array(tree),
            start.elapsed(),
        ))
    }

    fn requires_sanitization(&self) -> bool {
        false // Internal tool
    }
}

// ==================== Supermemory-inspired tools ====================

/// Tool for creating typed connections between memory documents.
///
/// Connections form a knowledge graph: memories can update, extend, or derive
/// from each other. This helps surface related context during search.
pub struct MemoryConnectTool {
    workspace: Arc<Workspace>,
}

impl MemoryConnectTool {
    /// Create a new memory connect tool.
    pub fn new(workspace: Arc<Workspace>) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for MemoryConnectTool {
    fn name(&self) -> &str {
        "memory_connect"
    }

    fn description(&self) -> &str {
        "Create, list, or delete connections between memory documents. Connections form a \
         knowledge graph. Types: 'updates' (new info supersedes old), 'extends' (supplements \
         existing), 'derives' (inferred pattern). Use 'list' action to see existing connections."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "list", "delete"],
                    "description": "Action to perform",
                    "default": "create"
                },
                "source_path": {
                    "type": "string",
                    "description": "Path of the source document (for create)"
                },
                "target_path": {
                    "type": "string",
                    "description": "Path of the target document (for create)"
                },
                "connection_type": {
                    "type": "string",
                    "enum": ["updates", "extends", "derives"],
                    "description": "Type of connection (for create)",
                    "default": "extends"
                },
                "document_path": {
                    "type": "string",
                    "description": "Path of document to list connections for (for list)"
                },
                "connection_id": {
                    "type": "string",
                    "description": "UUID of connection to delete (for delete)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("create");

        match action {
            "create" => {
                let source_path = params
                    .get("source_path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidParameters("missing 'source_path' for create".to_string())
                    })?;
                let target_path = params
                    .get("target_path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidParameters("missing 'target_path' for create".to_string())
                    })?;
                let ct_str = params
                    .get("connection_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("extends");
                let connection_type = ConnectionType::from_str_loose(ct_str).ok_or_else(|| {
                    ToolError::InvalidParameters(format!(
                        "invalid connection_type '{}'. Use: updates, extends, derives",
                        ct_str
                    ))
                })?;

                let conn = self
                    .workspace
                    .connect(source_path, target_path, connection_type)
                    .await
                    .map_err(|e| {
                        ToolError::ExecutionFailed(format!("Create connection failed: {}", e))
                    })?;

                Ok(ToolOutput::success(
                    serde_json::json!({
                        "status": "created",
                        "connection_id": conn.id.to_string(),
                        "source": source_path,
                        "target": target_path,
                        "type": ct_str,
                    }),
                    start.elapsed(),
                ))
            }
            "list" => {
                let doc_path = params
                    .get("document_path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidParameters("missing 'document_path' for list".to_string())
                    })?;
                let doc = self.workspace.read(doc_path).await.map_err(|e| {
                    ToolError::ExecutionFailed(format!("Read document failed: {}", e))
                })?;

                let connections = self.workspace.get_connections(doc.id).await.map_err(|e| {
                    ToolError::ExecutionFailed(format!("List connections failed: {}", e))
                })?;

                let output: Vec<serde_json::Value> = connections
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "id": c.id.to_string(),
                            "source_id": c.source_id.to_string(),
                            "target_id": c.target_id.to_string(),
                            "type": c.connection_type.to_string(),
                            "strength": c.strength,
                            "created_at": c.created_at.to_rfc3339(),
                        })
                    })
                    .collect();

                Ok(ToolOutput::success(
                    serde_json::json!({
                        "document": doc_path,
                        "connections": output,
                        "count": connections.len(),
                    }),
                    start.elapsed(),
                ))
            }
            "delete" => {
                let conn_id_str = params
                    .get("connection_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidParameters(
                            "missing 'connection_id' for delete".to_string(),
                        )
                    })?;
                let conn_id = uuid::Uuid::parse_str(conn_id_str).map_err(|_| {
                    ToolError::InvalidParameters(format!("invalid UUID: '{}'", conn_id_str))
                })?;

                self.workspace
                    .delete_connection(conn_id)
                    .await
                    .map_err(|e| {
                        ToolError::ExecutionFailed(format!("Delete connection failed: {}", e))
                    })?;

                Ok(ToolOutput::success(
                    serde_json::json!({ "status": "deleted", "connection_id": conn_id_str }),
                    start.elapsed(),
                ))
            }
            other => Err(ToolError::InvalidParameters(format!(
                "unknown action '{}'. Use: create, list, delete",
                other
            ))),
        }
    }

    fn requires_sanitization(&self) -> bool {
        false
    }
}

/// Tool for managing memory spaces (named collections).
///
/// Spaces let users organize memories into thematic collections.
/// A document can belong to multiple spaces.
pub struct MemorySpacesTool {
    workspace: Arc<Workspace>,
}

impl MemorySpacesTool {
    /// Create a new memory spaces tool.
    pub fn new(workspace: Arc<Workspace>) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for MemorySpacesTool {
    fn name(&self) -> &str {
        "memory_spaces"
    }

    fn description(&self) -> &str {
        "Manage memory spaces (named collections). Create spaces to organize memories by \
         topic/project. Add or remove documents from spaces. List spaces and their contents."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "list", "add", "remove", "contents", "delete"],
                    "description": "Action to perform"
                },
                "name": {
                    "type": "string",
                    "description": "Space name (for create, add, remove, contents, delete)"
                },
                "description": {
                    "type": "string",
                    "description": "Space description (for create)"
                },
                "document_path": {
                    "type": "string",
                    "description": "Document path to add/remove from a space"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::InvalidParameters("missing 'action' parameter".to_string())
            })?;

        match action {
            "create" => {
                let name = params.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidParameters("missing 'name' for create".to_string())
                })?;
                let description = params
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let space = self
                    .workspace
                    .create_space(name, description)
                    .await
                    .map_err(|e| {
                        ToolError::ExecutionFailed(format!("Create space failed: {}", e))
                    })?;

                Ok(ToolOutput::success(
                    serde_json::json!({
                        "status": "created",
                        "space_id": space.id.to_string(),
                        "name": name,
                    }),
                    start.elapsed(),
                ))
            }
            "list" => {
                let spaces = self.workspace.list_spaces().await.map_err(|e| {
                    ToolError::ExecutionFailed(format!("List spaces failed: {}", e))
                })?;

                let output: Vec<serde_json::Value> = spaces
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "id": s.id.to_string(),
                            "name": s.name,
                            "description": s.description,
                            "created_at": s.created_at.to_rfc3339(),
                        })
                    })
                    .collect();

                Ok(ToolOutput::success(
                    serde_json::json!({
                        "spaces": output,
                        "count": spaces.len(),
                    }),
                    start.elapsed(),
                ))
            }
            "add" => {
                let name = params.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidParameters("missing 'name' for add".to_string())
                })?;
                let doc_path = params
                    .get("document_path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidParameters("missing 'document_path' for add".to_string())
                    })?;

                self.workspace
                    .add_to_space(name, doc_path)
                    .await
                    .map_err(|e| {
                        ToolError::ExecutionFailed(format!("Add to space failed: {}", e))
                    })?;

                Ok(ToolOutput::success(
                    serde_json::json!({
                        "status": "added",
                        "space": name,
                        "document": doc_path,
                    }),
                    start.elapsed(),
                ))
            }
            "remove" => {
                let name = params.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidParameters("missing 'name' for remove".to_string())
                })?;
                let doc_path = params
                    .get("document_path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidParameters(
                            "missing 'document_path' for remove".to_string(),
                        )
                    })?;

                self.workspace
                    .remove_from_space(name, doc_path)
                    .await
                    .map_err(|e| {
                        ToolError::ExecutionFailed(format!("Remove from space failed: {}", e))
                    })?;

                Ok(ToolOutput::success(
                    serde_json::json!({
                        "status": "removed",
                        "space": name,
                        "document": doc_path,
                    }),
                    start.elapsed(),
                ))
            }
            "contents" => {
                let name = params.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidParameters("missing 'name' for contents".to_string())
                })?;

                let docs = self
                    .workspace
                    .list_space_documents(name)
                    .await
                    .map_err(|e| {
                        ToolError::ExecutionFailed(format!("List space contents failed: {}", e))
                    })?;

                let output: Vec<serde_json::Value> = docs
                    .iter()
                    .map(|d| {
                        serde_json::json!({
                            "path": d.path,
                            "word_count": d.word_count(),
                            "updated_at": d.updated_at.to_rfc3339(),
                        })
                    })
                    .collect();

                Ok(ToolOutput::success(
                    serde_json::json!({
                        "space": name,
                        "documents": output,
                        "count": docs.len(),
                    }),
                    start.elapsed(),
                ))
            }
            "delete" => {
                let name = params.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidParameters("missing 'name' for delete".to_string())
                })?;

                self.workspace.delete_space(name).await.map_err(|e| {
                    ToolError::ExecutionFailed(format!("Delete space failed: {}", e))
                })?;

                Ok(ToolOutput::success(
                    serde_json::json!({ "status": "deleted", "space": name }),
                    start.elapsed(),
                ))
            }
            other => Err(ToolError::InvalidParameters(format!(
                "unknown action '{}'. Use: create, list, add, remove, contents, delete",
                other
            ))),
        }
    }

    fn requires_sanitization(&self) -> bool {
        false
    }
}

/// Tool for managing the auto-maintained user profile.
///
/// Profiles contain facts about the user organized into static (stable)
/// and dynamic (evolving) categories.
pub struct MemoryProfileTool {
    workspace: Arc<Workspace>,
}

impl MemoryProfileTool {
    /// Create a new memory profile tool.
    pub fn new(workspace: Arc<Workspace>) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for MemoryProfileTool {
    fn name(&self) -> &str {
        "memory_profile"
    }

    fn description(&self) -> &str {
        "Manage the user's profile (auto-maintained facts). Use to store user preferences, \
         context, and facts. Static facts (name, location) rarely change. Dynamic facts \
         (current project, recent focus) evolve. Use 'set' to add/update, 'get' to read, \
         'delete' to remove."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["set", "get", "delete"],
                    "description": "Action to perform"
                },
                "key": {
                    "type": "string",
                    "description": "Profile fact key (e.g., 'name', 'location', 'current_project')"
                },
                "value": {
                    "type": "string",
                    "description": "Profile fact value (for set)"
                },
                "profile_type": {
                    "type": "string",
                    "enum": ["static", "dynamic"],
                    "description": "Fact type: 'static' for stable facts, 'dynamic' for evolving ones",
                    "default": "static"
                },
                "source": {
                    "type": "string",
                    "description": "How this fact was learned: 'user_stated', 'inferred', 'observed'",
                    "default": "user_stated"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &JobContext,
    ) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::InvalidParameters("missing 'action' parameter".to_string())
            })?;

        match action {
            "set" => {
                let key = params.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidParameters("missing 'key' for set".to_string())
                })?;
                let value = params
                    .get("value")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidParameters("missing 'value' for set".to_string())
                    })?;
                let pt_str = params
                    .get("profile_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("static");
                let profile_type = if pt_str == "dynamic" {
                    ProfileType::Dynamic
                } else {
                    ProfileType::Static
                };
                let source = params
                    .get("source")
                    .and_then(|v| v.as_str())
                    .unwrap_or("user_stated");

                self.workspace
                    .set_profile_fact(profile_type, key, value, source)
                    .await
                    .map_err(|e| {
                        ToolError::ExecutionFailed(format!("Set profile failed: {}", e))
                    })?;

                Ok(ToolOutput::success(
                    serde_json::json!({
                        "status": "set",
                        "key": key,
                        "value": value,
                        "type": pt_str,
                    }),
                    start.elapsed(),
                ))
            }
            "get" => {
                let facts = self.workspace.get_profile().await.map_err(|e| {
                    ToolError::ExecutionFailed(format!("Get profile failed: {}", e))
                })?;

                let output: Vec<serde_json::Value> = facts
                    .iter()
                    .map(|f| {
                        serde_json::json!({
                            "key": f.key,
                            "value": f.value,
                            "type": f.profile_type.to_string(),
                            "confidence": f.confidence,
                            "source": f.source,
                            "updated_at": f.updated_at.to_rfc3339(),
                        })
                    })
                    .collect();

                Ok(ToolOutput::success(
                    serde_json::json!({
                        "facts": output,
                        "count": facts.len(),
                    }),
                    start.elapsed(),
                ))
            }
            "delete" => {
                let key = params.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidParameters("missing 'key' for delete".to_string())
                })?;

                self.workspace.delete_profile_fact(key).await.map_err(|e| {
                    ToolError::ExecutionFailed(format!("Delete profile entry failed: {}", e))
                })?;

                Ok(ToolOutput::success(
                    serde_json::json!({ "status": "deleted", "key": key }),
                    start.elapsed(),
                ))
            }
            other => Err(ToolError::InvalidParameters(format!(
                "unknown action '{}'. Use: set, get, delete",
                other
            ))),
        }
    }

    fn requires_sanitization(&self) -> bool {
        false
    }
}

#[cfg(all(test, feature = "postgres"))]
mod tests {
    use super::*;

    fn make_test_workspace() -> Arc<Workspace> {
        Arc::new(Workspace::new(
            "test_user",
            deadpool_postgres::Pool::builder(deadpool_postgres::Manager::new(
                tokio_postgres::Config::new(),
                tokio_postgres::NoTls,
            ))
            .build()
            .unwrap(),
        ))
    }

    #[test]
    fn test_memory_search_schema() {
        let workspace = make_test_workspace();
        let tool = MemorySearchTool::new(workspace);

        assert_eq!(tool.name(), "memory_search");
        assert!(!tool.requires_sanitization());

        let schema = tool.parameters_schema();
        assert!(schema["properties"]["query"].is_object());
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&"query".into())
        );
    }

    #[test]
    fn test_memory_write_schema() {
        let workspace = make_test_workspace();
        let tool = MemoryWriteTool::new(workspace);

        assert_eq!(tool.name(), "memory_write");

        let schema = tool.parameters_schema();
        assert!(schema["properties"]["content"].is_object());
        assert!(schema["properties"]["target"].is_object());
        assert!(schema["properties"]["append"].is_object());
    }

    #[test]
    fn test_memory_read_schema() {
        let workspace = make_test_workspace();
        let tool = MemoryReadTool::new(workspace);

        assert_eq!(tool.name(), "memory_read");

        let schema = tool.parameters_schema();
        assert!(schema["properties"]["path"].is_object());
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&"path".into())
        );
    }

    #[test]
    fn test_memory_tree_schema() {
        let workspace = make_test_workspace();
        let tool = MemoryTreeTool::new(workspace);

        assert_eq!(tool.name(), "memory_tree");

        let schema = tool.parameters_schema();
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["depth"].is_object());
        assert_eq!(schema["properties"]["depth"]["default"], 1);
    }
}
