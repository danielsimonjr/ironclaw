//! Memory document types for the workspace.
//!
//! Includes supermemory-inspired types: connections, spaces, and user profiles.

use std::fmt;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Well-known document paths.
///
/// These are conventional paths that have special meaning in the workspace.
/// Agents can create arbitrary paths beyond these.
pub mod paths {
    /// Long-term curated memory.
    pub const MEMORY: &str = "MEMORY.md";
    /// Agent identity (name, nature, vibe).
    pub const IDENTITY: &str = "IDENTITY.md";
    /// Core values and principles.
    pub const SOUL: &str = "SOUL.md";
    /// Behavior instructions.
    pub const AGENTS: &str = "AGENTS.md";
    /// User context (name, preferences).
    pub const USER: &str = "USER.md";
    /// Periodic checklist for heartbeat.
    pub const HEARTBEAT: &str = "HEARTBEAT.md";
    /// Root runbook/readme.
    pub const README: &str = "README.md";
    /// Daily logs directory.
    pub const DAILY_DIR: &str = "daily/";
    /// Context directory (for identity-related docs).
    pub const CONTEXT_DIR: &str = "context/";
    /// Spaces directory for organized collections.
    pub const SPACES_DIR: &str = "spaces/";
}

/// A memory document stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDocument {
    /// Unique document ID.
    pub id: Uuid,
    /// User identifier.
    pub user_id: String,
    /// Optional agent ID for multi-agent isolation.
    pub agent_id: Option<Uuid>,
    /// File path within the workspace (e.g., "context/vision.md").
    pub path: String,
    /// Full document content.
    pub content: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Flexible metadata.
    pub metadata: serde_json::Value,
}

impl MemoryDocument {
    /// Create a new document with a path.
    pub fn new(
        user_id: impl Into<String>,
        agent_id: Option<Uuid>,
        path: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user_id: user_id.into(),
            agent_id,
            path: path.into(),
            content: String::new(),
            created_at: now,
            updated_at: now,
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    /// Get the file name from the path.
    pub fn file_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// Get the parent directory from the path.
    pub fn parent_dir(&self) -> Option<&str> {
        let idx = self.path.rfind('/')?;
        Some(&self.path[..idx])
    }

    /// Check if the document is empty.
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Get word count.
    pub fn word_count(&self) -> usize {
        self.content.split_whitespace().count()
    }

    /// Check if this is a well-known identity document.
    pub fn is_identity_document(&self) -> bool {
        matches!(
            self.path.as_str(),
            paths::IDENTITY | paths::SOUL | paths::AGENTS | paths::USER
        )
    }
}

/// An entry in a workspace directory listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    /// Path relative to listing directory.
    pub path: String,
    /// True if this is a directory (has children).
    pub is_directory: bool,
    /// Last update timestamp (latest among children for directories).
    pub updated_at: Option<DateTime<Utc>>,
    /// Preview of content (first ~200 chars, None for directories).
    pub content_preview: Option<String>,
}

impl WorkspaceEntry {
    /// Get the entry name (last path component).
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

// ==================== Supermemory-inspired types ====================

/// Type of relationship between two memory documents.
///
/// Inspired by supermemory's connection types:
/// - **Updates**: New info contradicts/replaces existing knowledge.
/// - **Extends**: New info adds to existing knowledge without replacing it.
/// - **Derives**: Inferred connection from patterns across documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionType {
    /// New memory updates/supersedes the target memory.
    Updates,
    /// New memory extends/supplements the target memory.
    Extends,
    /// Connection inferred from patterns across memories.
    Derives,
}

impl fmt::Display for ConnectionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Updates => write!(f, "updates"),
            Self::Extends => write!(f, "extends"),
            Self::Derives => write!(f, "derives"),
        }
    }
}

impl ConnectionType {
    /// Parse from string.
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "updates" | "update" => Some(Self::Updates),
            "extends" | "extend" => Some(Self::Extends),
            "derives" | "derive" | "inferred" => Some(Self::Derives),
            _ => None,
        }
    }
}

/// A typed relationship between two memory documents.
///
/// Connections form a knowledge graph that helps surface related context.
/// When memory A "updates" memory B, searches for B's topic will also
/// surface A (and mark B as superseded).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConnection {
    /// Unique connection ID.
    pub id: Uuid,
    /// Source document ID (the newer/referencing memory).
    pub source_id: Uuid,
    /// Target document ID (the older/referenced memory).
    pub target_id: Uuid,
    /// Relationship type.
    pub connection_type: ConnectionType,
    /// Strength of the connection (0.0-1.0). Higher = stronger.
    pub strength: f32,
    /// Flexible metadata (e.g., why this connection was created).
    pub metadata: serde_json::Value,
    /// When the connection was created.
    pub created_at: DateTime<Utc>,
}

impl MemoryConnection {
    /// Create a new connection.
    pub fn new(source_id: Uuid, target_id: Uuid, connection_type: ConnectionType) -> Self {
        Self {
            id: Uuid::new_v4(),
            source_id,
            target_id,
            connection_type,
            strength: 1.0,
            metadata: serde_json::json!({}),
            created_at: Utc::now(),
        }
    }

    /// Set connection strength.
    pub fn with_strength(mut self, strength: f32) -> Self {
        self.strength = strength.clamp(0.0, 1.0);
        self
    }

    /// Set metadata.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// A named collection for organizing memories.
///
/// Spaces let users group related memories together, similar to
/// folders but with richer semantics. A document can belong to
/// multiple spaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySpace {
    /// Unique space ID.
    pub id: Uuid,
    /// Owner user ID.
    pub user_id: String,
    /// Human-readable space name (unique per user).
    pub name: String,
    /// Description of what this space contains.
    pub description: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl MemorySpace {
    /// Create a new space.
    pub fn new(user_id: impl Into<String>, name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user_id: user_id.into(),
            name: name.into(),
            description: String::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }
}

/// Type of user profile entry.
///
/// Mirrors supermemory's dual-layer profile:
/// - **Static**: Stable facts that rarely change (name, location, preferences).
/// - **Dynamic**: Temporary/evolving facts (current project, recent activities).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileType {
    /// Stable, rarely changing facts.
    Static,
    /// Frequently updated, evolving facts.
    Dynamic,
}

impl fmt::Display for ProfileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Static => write!(f, "static"),
            Self::Dynamic => write!(f, "dynamic"),
        }
    }
}

/// An automatically maintained user profile.
///
/// Built incrementally from interactions. Static facts (name, location)
/// persist indefinitely. Dynamic facts (current project, recent activities)
/// evolve as the user's context changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    /// Unique profile ID.
    pub id: Uuid,
    /// Owner user ID.
    pub user_id: String,
    /// Profile type (static or dynamic).
    pub profile_type: ProfileType,
    /// Key for this fact (e.g., "name", "location", "current_project").
    pub key: String,
    /// The fact value.
    pub value: String,
    /// Confidence score (0.0-1.0). Higher = more confident.
    pub confidence: f32,
    /// Source of this fact (e.g., "user_stated", "inferred", "observed").
    pub source: String,
    /// When this fact was first recorded.
    pub created_at: DateTime<Utc>,
    /// When this fact was last updated.
    pub updated_at: DateTime<Utc>,
}

impl UserProfile {
    /// Create a new profile entry.
    pub fn new(
        user_id: impl Into<String>,
        profile_type: ProfileType,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user_id: user_id.into(),
            profile_type,
            key: key.into(),
            value: value.into(),
            confidence: 1.0,
            source: "user_stated".to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Set the confidence.
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set the source.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }
}

/// Extended metadata for a memory document.
///
/// Captures temporal, importance, and provenance information
/// inspired by supermemory's dual-timestamp and decay model.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// URL the content was ingested from (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    /// When the event described in the document occurred (vs when it was stored).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_date: Option<NaiveDate>,
    /// Importance score (0.0-1.0). Decays over time if not accessed.
    pub importance: f32,
    /// Number of times this document was accessed/retrieved.
    pub access_count: i64,
    /// When this document was last accessed by a search or read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_accessed_at: Option<DateTime<Utc>>,
    /// User-defined or auto-generated tags for categorization.
    #[serde(default)]
    pub tags: Vec<String>,
}

impl DocumentMetadata {
    /// Parse from the flexible metadata JSON on a MemoryDocument.
    pub fn from_json(value: &serde_json::Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }

    /// Convert to JSON for storage.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    /// Merge non-default fields into existing metadata JSON.
    pub fn merge_into(&self, existing: &serde_json::Value) -> serde_json::Value {
        let mut base = existing.clone();
        if let serde_json::Value::Object(ref mut map) = base {
            let new = self.to_json();
            if let serde_json::Value::Object(new_map) = new {
                for (k, v) in new_map {
                    map.insert(k, v);
                }
            }
        }
        base
    }
}

/// A chunk of a memory document for search indexing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryChunk {
    /// Unique chunk ID.
    pub id: Uuid,
    /// Parent document ID.
    pub document_id: Uuid,
    /// Position in the document (0-based).
    pub chunk_index: i32,
    /// Chunk text content.
    pub content: String,
    /// Embedding vector (if generated).
    pub embedding: Option<Vec<f32>>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

impl MemoryChunk {
    /// Create a new chunk (not persisted yet).
    pub fn new(document_id: Uuid, chunk_index: i32, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            document_id,
            chunk_index,
            content: content.into(),
            embedding: None,
            created_at: Utc::now(),
        }
    }

    /// Set the embedding.
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_document_new() {
        let doc = MemoryDocument::new("user1", None, "context/vision.md");
        assert_eq!(doc.user_id, "user1");
        assert_eq!(doc.path, "context/vision.md");
        assert!(doc.content.is_empty());
    }

    #[test]
    fn test_memory_document_file_name() {
        let doc = MemoryDocument::new("user1", None, "projects/alpha/README.md");
        assert_eq!(doc.file_name(), "README.md");
    }

    #[test]
    fn test_memory_document_parent_dir() {
        let doc = MemoryDocument::new("user1", None, "projects/alpha/README.md");
        assert_eq!(doc.parent_dir(), Some("projects/alpha"));

        let root_doc = MemoryDocument::new("user1", None, "README.md");
        assert_eq!(root_doc.parent_dir(), None);
    }

    #[test]
    fn test_memory_document_word_count() {
        let mut doc = MemoryDocument::new("user1", None, "MEMORY.md");
        assert_eq!(doc.word_count(), 0);

        doc.content = "Hello world, this is a test.".to_string();
        assert_eq!(doc.word_count(), 6);
    }

    #[test]
    fn test_is_identity_document() {
        let identity = MemoryDocument::new("user1", None, paths::IDENTITY);
        assert!(identity.is_identity_document());

        let soul = MemoryDocument::new("user1", None, paths::SOUL);
        assert!(soul.is_identity_document());

        let memory = MemoryDocument::new("user1", None, paths::MEMORY);
        assert!(!memory.is_identity_document());

        let custom = MemoryDocument::new("user1", None, "projects/notes.md");
        assert!(!custom.is_identity_document());
    }

    #[test]
    fn test_workspace_entry_name() {
        let entry = WorkspaceEntry {
            path: "projects/alpha".to_string(),
            is_directory: true,
            updated_at: None,
            content_preview: None,
        };
        assert_eq!(entry.name(), "alpha");
    }

    // ==================== Supermemory type tests ====================

    #[test]
    fn test_connection_type_display() {
        assert_eq!(ConnectionType::Updates.to_string(), "updates");
        assert_eq!(ConnectionType::Extends.to_string(), "extends");
        assert_eq!(ConnectionType::Derives.to_string(), "derives");
    }

    #[test]
    fn test_connection_type_from_str() {
        assert_eq!(
            ConnectionType::from_str_loose("updates"),
            Some(ConnectionType::Updates)
        );
        assert_eq!(
            ConnectionType::from_str_loose("Update"),
            Some(ConnectionType::Updates)
        );
        assert_eq!(
            ConnectionType::from_str_loose("extends"),
            Some(ConnectionType::Extends)
        );
        assert_eq!(
            ConnectionType::from_str_loose("DERIVES"),
            Some(ConnectionType::Derives)
        );
        assert_eq!(
            ConnectionType::from_str_loose("inferred"),
            Some(ConnectionType::Derives)
        );
        assert_eq!(ConnectionType::from_str_loose("unknown"), None);
    }

    #[test]
    fn test_memory_connection_new() {
        let src = Uuid::new_v4();
        let tgt = Uuid::new_v4();
        let conn = MemoryConnection::new(src, tgt, ConnectionType::Updates);

        assert_eq!(conn.source_id, src);
        assert_eq!(conn.target_id, tgt);
        assert_eq!(conn.connection_type, ConnectionType::Updates);
        assert!((conn.strength - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_memory_connection_with_strength() {
        let conn = MemoryConnection::new(Uuid::new_v4(), Uuid::new_v4(), ConnectionType::Extends)
            .with_strength(0.7);
        assert!((conn.strength - 0.7).abs() < 0.001);

        // Clamp to range
        let conn2 = conn.with_strength(2.0);
        assert!((conn2.strength - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_memory_space_new() {
        let space = MemorySpace::new("user1", "Research").with_description("Research papers");
        assert_eq!(space.user_id, "user1");
        assert_eq!(space.name, "Research");
        assert_eq!(space.description, "Research papers");
    }

    #[test]
    fn test_user_profile_new() {
        let profile = UserProfile::new("user1", ProfileType::Static, "name", "Alice")
            .with_confidence(0.95)
            .with_source("user_stated");

        assert_eq!(profile.user_id, "user1");
        assert_eq!(profile.profile_type, ProfileType::Static);
        assert_eq!(profile.key, "name");
        assert_eq!(profile.value, "Alice");
        assert!((profile.confidence - 0.95).abs() < 0.001);
    }

    #[test]
    fn test_document_metadata_roundtrip() {
        let meta = DocumentMetadata {
            source_url: Some("https://example.com".to_string()),
            event_date: Some(NaiveDate::from_ymd_opt(2025, 6, 15).unwrap()),
            importance: 0.8,
            access_count: 5,
            last_accessed_at: None,
            tags: vec!["rust".to_string(), "memory".to_string()],
        };

        let json = meta.to_json();
        let parsed = DocumentMetadata::from_json(&json);

        assert_eq!(parsed.source_url, meta.source_url);
        assert_eq!(parsed.importance, meta.importance);
        assert_eq!(parsed.tags, meta.tags);
    }

    #[test]
    fn test_document_metadata_merge() {
        let existing = serde_json::json!({"custom_field": "keep_me"});
        let meta = DocumentMetadata {
            importance: 0.9,
            tags: vec!["new_tag".to_string()],
            ..Default::default()
        };

        let merged = meta.merge_into(&existing);
        assert_eq!(merged["custom_field"], "keep_me");
        let imp = merged["importance"].as_f64().unwrap();
        assert!((imp - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_profile_type_display() {
        assert_eq!(ProfileType::Static.to_string(), "static");
        assert_eq!(ProfileType::Dynamic.to_string(), "dynamic");
    }
}
