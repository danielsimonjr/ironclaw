//! Database repository for workspace persistence.
//!
//! All workspace data is stored in PostgreSQL:
//! - Documents in `memory_documents` table
//! - Chunks in `memory_chunks` table (with FTS and vector indexes)

use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use pgvector::Vector;
use uuid::Uuid;

use crate::error::WorkspaceError;

use crate::workspace::document::{
    ConnectionType, MemoryChunk, MemoryConnection, MemoryDocument, MemorySpace, ProfileType,
    UserProfile, WorkspaceEntry,
};
use crate::workspace::search::{RankedResult, SearchConfig, SearchResult, reciprocal_rank_fusion};

/// Database repository for workspace operations.
pub struct Repository {
    pool: Pool,
}

impl Repository {
    /// Create a new repository with a connection pool.
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Get a connection from the pool.
    async fn conn(&self) -> Result<deadpool_postgres::Object, WorkspaceError> {
        self.pool
            .get()
            .await
            .map_err(|e| WorkspaceError::SearchFailed {
                reason: format!("Failed to get connection: {}", e),
            })
    }

    // ==================== Document Operations ====================

    /// Get a document by its path.
    pub async fn get_document_by_path(
        &self,
        user_id: &str,
        agent_id: Option<Uuid>,
        path: &str,
    ) -> Result<MemoryDocument, WorkspaceError> {
        let conn = self.conn().await?;

        let row = conn
            .query_opt(
                r#"
                SELECT id, user_id, agent_id, path, content,
                       created_at, updated_at, metadata
                FROM memory_documents
                WHERE user_id = $1 AND agent_id IS NOT DISTINCT FROM $2 AND path = $3
                "#,
                &[&user_id, &agent_id, &path],
            )
            .await
            .map_err(|e| WorkspaceError::SearchFailed {
                reason: format!("Query failed: {}", e),
            })?;

        match row {
            Some(row) => Ok(self.row_to_document(&row)),
            None => Err(WorkspaceError::DocumentNotFound {
                doc_type: path.to_string(),
                user_id: user_id.to_string(),
            }),
        }
    }

    /// Get a document by ID.
    pub async fn get_document_by_id(&self, id: Uuid) -> Result<MemoryDocument, WorkspaceError> {
        let conn = self.conn().await?;

        let row = conn
            .query_opt(
                r#"
                SELECT id, user_id, agent_id, path, content,
                       created_at, updated_at, metadata
                FROM memory_documents WHERE id = $1
                "#,
                &[&id],
            )
            .await
            .map_err(|e| WorkspaceError::SearchFailed {
                reason: format!("Query failed: {}", e),
            })?;

        match row {
            Some(row) => Ok(self.row_to_document(&row)),
            None => Err(WorkspaceError::DocumentNotFound {
                doc_type: "unknown".to_string(),
                user_id: "unknown".to_string(),
            }),
        }
    }

    /// Get or create a document by path.
    pub async fn get_or_create_document_by_path(
        &self,
        user_id: &str,
        agent_id: Option<Uuid>,
        path: &str,
    ) -> Result<MemoryDocument, WorkspaceError> {
        // Try to get existing document first
        match self.get_document_by_path(user_id, agent_id, path).await {
            Ok(doc) => return Ok(doc),
            Err(WorkspaceError::DocumentNotFound { .. }) => {}
            Err(e) => return Err(e),
        }

        // Create new document
        let conn = self.conn().await?;
        let id = Uuid::new_v4();
        let now = Utc::now();
        let metadata = serde_json::json!({});

        conn.execute(
            r#"
            INSERT INTO memory_documents (id, user_id, agent_id, path, content, metadata, created_at, updated_at)
            VALUES ($1, $2, $3, $4, '', $5, $6, $7)
            ON CONFLICT (user_id, agent_id, path) DO NOTHING
            "#,
            &[&id, &user_id, &agent_id, &path, &metadata, &now, &now],
        )
        .await
        .map_err(|e| WorkspaceError::SearchFailed {
            reason: format!("Insert failed: {}", e),
        })?;

        // Fetch the document (might have been created by concurrent request)
        self.get_document_by_path(user_id, agent_id, path).await
    }

    /// Update a document's content.
    pub async fn update_document(&self, id: Uuid, content: &str) -> Result<(), WorkspaceError> {
        let conn = self.conn().await?;

        conn.execute(
            "UPDATE memory_documents SET content = $2, updated_at = NOW() WHERE id = $1",
            &[&id, &content],
        )
        .await
        .map_err(|e| WorkspaceError::SearchFailed {
            reason: format!("Update failed: {}", e),
        })?;

        Ok(())
    }

    /// Delete a document by its path.
    pub async fn delete_document_by_path(
        &self,
        user_id: &str,
        agent_id: Option<Uuid>,
        path: &str,
    ) -> Result<(), WorkspaceError> {
        let conn = self.conn().await?;

        // First get the document to delete its chunks
        let doc = self.get_document_by_path(user_id, agent_id, path).await?;
        self.delete_chunks(doc.id).await?;

        // Delete the document
        conn.execute(
            r#"
            DELETE FROM memory_documents
            WHERE user_id = $1 AND agent_id IS NOT DISTINCT FROM $2 AND path = $3
            "#,
            &[&user_id, &agent_id, &path],
        )
        .await
        .map_err(|e| WorkspaceError::SearchFailed {
            reason: format!("Delete failed: {}", e),
        })?;

        Ok(())
    }

    /// List files and directories in a directory path.
    ///
    /// Returns immediate children (not recursive).
    /// Empty string lists the root directory.
    pub async fn list_directory(
        &self,
        user_id: &str,
        agent_id: Option<Uuid>,
        directory: &str,
    ) -> Result<Vec<WorkspaceEntry>, WorkspaceError> {
        let conn = self.conn().await?;

        let rows = conn
            .query(
                "SELECT path, is_directory, updated_at, content_preview FROM list_workspace_files($1, $2, $3)",
                &[&user_id, &agent_id, &directory],
            )
            .await
            .map_err(|e| WorkspaceError::SearchFailed {
                reason: format!("List directory failed: {}", e),
            })?;

        Ok(rows
            .iter()
            .map(|row| {
                let updated_at: Option<DateTime<Utc>> = row.get("updated_at");
                WorkspaceEntry {
                    path: row.get("path"),
                    is_directory: row.get("is_directory"),
                    updated_at,
                    content_preview: row.get("content_preview"),
                }
            })
            .collect())
    }

    /// List all file paths in the workspace (flat list).
    pub async fn list_all_paths(
        &self,
        user_id: &str,
        agent_id: Option<Uuid>,
    ) -> Result<Vec<String>, WorkspaceError> {
        let conn = self.conn().await?;

        let rows = conn
            .query(
                r#"
                SELECT path FROM memory_documents
                WHERE user_id = $1 AND agent_id IS NOT DISTINCT FROM $2
                ORDER BY path
                "#,
                &[&user_id, &agent_id],
            )
            .await
            .map_err(|e| WorkspaceError::SearchFailed {
                reason: format!("List paths failed: {}", e),
            })?;

        Ok(rows.iter().map(|row| row.get("path")).collect())
    }

    /// List all documents for a user.
    pub async fn list_documents(
        &self,
        user_id: &str,
        agent_id: Option<Uuid>,
    ) -> Result<Vec<MemoryDocument>, WorkspaceError> {
        let conn = self.conn().await?;

        let rows = conn
            .query(
                r#"
                SELECT id, user_id, agent_id, path, content,
                       created_at, updated_at, metadata
                FROM memory_documents
                WHERE user_id = $1 AND agent_id IS NOT DISTINCT FROM $2
                ORDER BY updated_at DESC
                "#,
                &[&user_id, &agent_id],
            )
            .await
            .map_err(|e| WorkspaceError::SearchFailed {
                reason: format!("Query failed: {}", e),
            })?;

        Ok(rows.iter().map(|r| self.row_to_document(r)).collect())
    }

    fn row_to_document(&self, row: &tokio_postgres::Row) -> MemoryDocument {
        MemoryDocument {
            id: row.get("id"),
            user_id: row.get("user_id"),
            agent_id: row.get("agent_id"),
            path: row.get("path"),
            content: row.get("content"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            metadata: row.get("metadata"),
        }
    }

    // ==================== Chunk Operations ====================

    /// Delete all chunks for a document.
    pub async fn delete_chunks(&self, document_id: Uuid) -> Result<(), WorkspaceError> {
        let conn = self.conn().await?;

        conn.execute(
            "DELETE FROM memory_chunks WHERE document_id = $1",
            &[&document_id],
        )
        .await
        .map_err(|e| WorkspaceError::ChunkingFailed {
            reason: format!("Delete failed: {}", e),
        })?;

        Ok(())
    }

    /// Insert a chunk.
    pub async fn insert_chunk(
        &self,
        document_id: Uuid,
        chunk_index: i32,
        content: &str,
        embedding: Option<&[f32]>,
    ) -> Result<Uuid, WorkspaceError> {
        let conn = self.conn().await?;
        let id = Uuid::new_v4();

        let embedding_vec = embedding.map(|e| Vector::from(e.to_vec()));

        conn.execute(
            r#"
            INSERT INTO memory_chunks (id, document_id, chunk_index, content, embedding)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            &[&id, &document_id, &chunk_index, &content, &embedding_vec],
        )
        .await
        .map_err(|e| WorkspaceError::ChunkingFailed {
            reason: format!("Insert failed: {}", e),
        })?;

        Ok(id)
    }

    /// Update a chunk's embedding.
    pub async fn update_chunk_embedding(
        &self,
        chunk_id: Uuid,
        embedding: &[f32],
    ) -> Result<(), WorkspaceError> {
        let conn = self.conn().await?;
        let embedding_vec = Vector::from(embedding.to_vec());

        conn.execute(
            "UPDATE memory_chunks SET embedding = $2 WHERE id = $1",
            &[&chunk_id, &embedding_vec],
        )
        .await
        .map_err(|e| WorkspaceError::EmbeddingFailed {
            reason: format!("Update failed: {}", e),
        })?;

        Ok(())
    }

    /// Get chunks without embeddings for backfilling.
    pub async fn get_chunks_without_embeddings(
        &self,
        user_id: &str,
        agent_id: Option<Uuid>,
        limit: usize,
    ) -> Result<Vec<MemoryChunk>, WorkspaceError> {
        let conn = self.conn().await?;

        let rows = conn
            .query(
                r#"
                SELECT c.id, c.document_id, c.chunk_index, c.content, c.created_at
                FROM memory_chunks c
                JOIN memory_documents d ON d.id = c.document_id
                WHERE d.user_id = $1 AND d.agent_id IS NOT DISTINCT FROM $2
                  AND c.embedding IS NULL
                LIMIT $3
                "#,
                &[&user_id, &agent_id, &(limit as i64)],
            )
            .await
            .map_err(|e| WorkspaceError::SearchFailed {
                reason: format!("Query failed: {}", e),
            })?;

        Ok(rows
            .iter()
            .map(|row| MemoryChunk {
                id: row.get("id"),
                document_id: row.get("document_id"),
                chunk_index: row.get("chunk_index"),
                content: row.get("content"),
                embedding: None,
                created_at: row.get("created_at"),
            })
            .collect())
    }

    // ==================== Search Operations ====================

    /// Perform hybrid search combining FTS and vector similarity.
    pub async fn hybrid_search(
        &self,
        user_id: &str,
        agent_id: Option<Uuid>,
        query: &str,
        embedding: Option<&[f32]>,
        config: &SearchConfig,
    ) -> Result<Vec<SearchResult>, WorkspaceError> {
        let fts_results = if config.use_fts {
            self.fts_search(user_id, agent_id, query, config.pre_fusion_limit)
                .await?
        } else {
            Vec::new()
        };

        let vector_results = if config.use_vector {
            if let Some(embedding) = embedding {
                self.vector_search(user_id, agent_id, embedding, config.pre_fusion_limit)
                    .await?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        Ok(reciprocal_rank_fusion(fts_results, vector_results, config))
    }

    /// Full-text search using PostgreSQL ts_rank_cd.
    async fn fts_search(
        &self,
        user_id: &str,
        agent_id: Option<Uuid>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<RankedResult>, WorkspaceError> {
        let conn = self.conn().await?;

        let rows = conn
            .query(
                r#"
                SELECT c.id as chunk_id, c.document_id, c.content,
                       ts_rank_cd(c.content_tsv, plainto_tsquery('english', $3)) as rank
                FROM memory_chunks c
                JOIN memory_documents d ON d.id = c.document_id
                WHERE d.user_id = $1 AND d.agent_id IS NOT DISTINCT FROM $2
                  AND c.content_tsv @@ plainto_tsquery('english', $3)
                ORDER BY rank DESC
                LIMIT $4
                "#,
                &[&user_id, &agent_id, &query, &(limit as i64)],
            )
            .await
            .map_err(|e| WorkspaceError::SearchFailed {
                reason: format!("FTS query failed: {}", e),
            })?;

        Ok(rows
            .iter()
            .enumerate()
            .map(|(i, row)| RankedResult {
                chunk_id: row.get("chunk_id"),
                document_id: row.get("document_id"),
                content: row.get("content"),
                rank: (i + 1) as u32,
            })
            .collect())
    }

    /// Vector similarity search using pgvector cosine distance.
    async fn vector_search(
        &self,
        user_id: &str,
        agent_id: Option<Uuid>,
        embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<RankedResult>, WorkspaceError> {
        let conn = self.conn().await?;
        let embedding_vec = Vector::from(embedding.to_vec());

        let rows = conn
            .query(
                r#"
                SELECT c.id as chunk_id, c.document_id, c.content,
                       1 - (c.embedding <=> $3) as similarity
                FROM memory_chunks c
                JOIN memory_documents d ON d.id = c.document_id
                WHERE d.user_id = $1 AND d.agent_id IS NOT DISTINCT FROM $2
                  AND c.embedding IS NOT NULL
                ORDER BY c.embedding <=> $3
                LIMIT $4
                "#,
                &[&user_id, &agent_id, &embedding_vec, &(limit as i64)],
            )
            .await
            .map_err(|e| WorkspaceError::SearchFailed {
                reason: format!("Vector query failed: {}", e),
            })?;

        Ok(rows
            .iter()
            .enumerate()
            .map(|(i, row)| RankedResult {
                chunk_id: row.get("chunk_id"),
                document_id: row.get("document_id"),
                content: row.get("content"),
                rank: (i + 1) as u32,
            })
            .collect())
    }

    // ==================== Connection Operations ====================

    /// Create or update a connection between two memory documents.
    pub async fn create_connection(
        &self,
        conn_data: &MemoryConnection,
    ) -> Result<(), WorkspaceError> {
        let conn = self.conn().await?;
        conn.execute(
            r#"
            INSERT INTO memory_connections (id, source_id, target_id, connection_type, strength, metadata, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (source_id, target_id, connection_type) DO UPDATE
            SET strength = EXCLUDED.strength, metadata = EXCLUDED.metadata
            "#,
            &[
                &conn_data.id,
                &conn_data.source_id,
                &conn_data.target_id,
                &conn_data.connection_type.to_string(),
                &conn_data.strength,
                &conn_data.metadata,
                &conn_data.created_at,
            ],
        )
        .await
        .map_err(|e| WorkspaceError::SearchFailed {
            reason: format!("Insert connection failed: {}", e),
        })?;
        Ok(())
    }

    /// Get all connections for a document (both as source and target).
    pub async fn get_connections(
        &self,
        document_id: Uuid,
    ) -> Result<Vec<MemoryConnection>, WorkspaceError> {
        let conn = self.conn().await?;
        let rows = conn
            .query(
                r#"
                SELECT id, source_id, target_id, connection_type, strength, metadata, created_at
                FROM memory_connections
                WHERE source_id = $1 OR target_id = $1
                ORDER BY created_at DESC
                "#,
                &[&document_id],
            )
            .await
            .map_err(|e| WorkspaceError::SearchFailed {
                reason: format!("Query connections failed: {}", e),
            })?;

        Ok(rows
            .iter()
            .map(|row| {
                let ct_str: String = row.get("connection_type");
                MemoryConnection {
                    id: row.get("id"),
                    source_id: row.get("source_id"),
                    target_id: row.get("target_id"),
                    connection_type: ConnectionType::from_str_loose(&ct_str)
                        .unwrap_or(ConnectionType::Extends),
                    strength: row.get("strength"),
                    metadata: row.get("metadata"),
                    created_at: row.get("created_at"),
                }
            })
            .collect())
    }

    /// Delete a connection by ID.
    pub async fn delete_connection(&self, id: Uuid) -> Result<(), WorkspaceError> {
        let conn = self.conn().await?;
        conn.execute("DELETE FROM memory_connections WHERE id = $1", &[&id])
            .await
            .map_err(|e| WorkspaceError::SearchFailed {
                reason: format!("Delete connection failed: {}", e),
            })?;
        Ok(())
    }

    // ==================== Space Operations ====================

    /// Create or update a memory space.
    pub async fn create_space(&self, space: &MemorySpace) -> Result<(), WorkspaceError> {
        let conn = self.conn().await?;
        conn.execute(
            r#"
            INSERT INTO memory_spaces (id, user_id, name, description, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (user_id, name) DO UPDATE
            SET description = EXCLUDED.description
            "#,
            &[
                &space.id,
                &space.user_id,
                &space.name,
                &space.description,
                &space.created_at,
                &space.updated_at,
            ],
        )
        .await
        .map_err(|e| WorkspaceError::SearchFailed {
            reason: format!("Insert space failed: {}", e),
        })?;
        Ok(())
    }

    /// List all spaces for a user.
    pub async fn list_spaces(&self, user_id: &str) -> Result<Vec<MemorySpace>, WorkspaceError> {
        let conn = self.conn().await?;
        let rows = conn
            .query(
                r#"
                SELECT id, user_id, name, description, created_at, updated_at
                FROM memory_spaces WHERE user_id = $1
                ORDER BY name
                "#,
                &[&user_id],
            )
            .await
            .map_err(|e| WorkspaceError::SearchFailed {
                reason: format!("Query spaces failed: {}", e),
            })?;

        Ok(rows
            .iter()
            .map(|row| MemorySpace {
                id: row.get("id"),
                user_id: row.get("user_id"),
                name: row.get("name"),
                description: row.get("description"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
            .collect())
    }

    /// Get a space by user ID and name.
    pub async fn get_space_by_name(
        &self,
        user_id: &str,
        name: &str,
    ) -> Result<Option<MemorySpace>, WorkspaceError> {
        let conn = self.conn().await?;
        let row = conn
            .query_opt(
                r#"
                SELECT id, user_id, name, description, created_at, updated_at
                FROM memory_spaces WHERE user_id = $1 AND name = $2
                "#,
                &[&user_id, &name],
            )
            .await
            .map_err(|e| WorkspaceError::SearchFailed {
                reason: format!("Query space failed: {}", e),
            })?;

        Ok(row.map(|r| MemorySpace {
            id: r.get("id"),
            user_id: r.get("user_id"),
            name: r.get("name"),
            description: r.get("description"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    /// Add a document to a space.
    pub async fn add_to_space(
        &self,
        space_id: Uuid,
        document_id: Uuid,
    ) -> Result<(), WorkspaceError> {
        let conn = self.conn().await?;
        conn.execute(
            r#"
            INSERT INTO memory_space_members (space_id, document_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
            &[&space_id, &document_id],
        )
        .await
        .map_err(|e| WorkspaceError::SearchFailed {
            reason: format!("Add to space failed: {}", e),
        })?;
        Ok(())
    }

    /// Remove a document from a space.
    pub async fn remove_from_space(
        &self,
        space_id: Uuid,
        document_id: Uuid,
    ) -> Result<(), WorkspaceError> {
        let conn = self.conn().await?;
        conn.execute(
            "DELETE FROM memory_space_members WHERE space_id = $1 AND document_id = $2",
            &[&space_id, &document_id],
        )
        .await
        .map_err(|e| WorkspaceError::SearchFailed {
            reason: format!("Remove from space failed: {}", e),
        })?;
        Ok(())
    }

    /// List all documents in a space.
    pub async fn list_space_documents(
        &self,
        space_id: Uuid,
    ) -> Result<Vec<MemoryDocument>, WorkspaceError> {
        let conn = self.conn().await?;
        let rows = conn
            .query(
                r#"
                SELECT d.id, d.user_id, d.agent_id, d.path, d.content,
                       d.created_at, d.updated_at, d.metadata
                FROM memory_documents d
                JOIN memory_space_members m ON m.document_id = d.id
                WHERE m.space_id = $1
                ORDER BY d.updated_at DESC
                "#,
                &[&space_id],
            )
            .await
            .map_err(|e| WorkspaceError::SearchFailed {
                reason: format!("List space docs failed: {}", e),
            })?;

        Ok(rows.iter().map(|r| self.row_to_document(r)).collect())
    }

    /// Delete a space and its memberships.
    pub async fn delete_space(&self, id: Uuid) -> Result<(), WorkspaceError> {
        let conn = self.conn().await?;
        conn.execute("DELETE FROM memory_spaces WHERE id = $1", &[&id])
            .await
            .map_err(|e| WorkspaceError::SearchFailed {
                reason: format!("Delete space failed: {}", e),
            })?;
        Ok(())
    }

    // ==================== Profile Operations ====================

    /// Upsert a user profile entry.
    pub async fn upsert_profile(&self, profile: &UserProfile) -> Result<(), WorkspaceError> {
        let conn = self.conn().await?;
        conn.execute(
            r#"
            INSERT INTO memory_profiles (id, user_id, profile_type, key, value, confidence, source, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (user_id, key) DO UPDATE
            SET value = EXCLUDED.value, confidence = EXCLUDED.confidence,
                source = EXCLUDED.source, profile_type = EXCLUDED.profile_type,
                updated_at = NOW()
            "#,
            &[
                &profile.id,
                &profile.user_id,
                &profile.profile_type.to_string(),
                &profile.key,
                &profile.value,
                &profile.confidence,
                &profile.source,
                &profile.created_at,
                &profile.updated_at,
            ],
        )
        .await
        .map_err(|e| WorkspaceError::SearchFailed {
            reason: format!("Upsert profile failed: {}", e),
        })?;
        Ok(())
    }

    /// Get all profile entries for a user.
    pub async fn get_profile(&self, user_id: &str) -> Result<Vec<UserProfile>, WorkspaceError> {
        let conn = self.conn().await?;
        let rows = conn
            .query(
                r#"
                SELECT id, user_id, profile_type, key, value, confidence, source, created_at, updated_at
                FROM memory_profiles WHERE user_id = $1
                ORDER BY profile_type, key
                "#,
                &[&user_id],
            )
            .await
            .map_err(|e| WorkspaceError::SearchFailed {
                reason: format!("Query profile failed: {}", e),
            })?;

        Ok(rows.iter().map(|row| self.row_to_profile(row)).collect())
    }

    /// Get profile entries of a specific type for a user.
    pub async fn get_profile_by_type(
        &self,
        user_id: &str,
        profile_type: ProfileType,
    ) -> Result<Vec<UserProfile>, WorkspaceError> {
        let conn = self.conn().await?;
        let rows = conn
            .query(
                r#"
                SELECT id, user_id, profile_type, key, value, confidence, source, created_at, updated_at
                FROM memory_profiles WHERE user_id = $1 AND profile_type = $2
                ORDER BY key
                "#,
                &[&user_id, &profile_type.to_string()],
            )
            .await
            .map_err(|e| WorkspaceError::SearchFailed {
                reason: format!("Query profile by type failed: {}", e),
            })?;

        Ok(rows.iter().map(|row| self.row_to_profile(row)).collect())
    }

    /// Delete a profile entry by user ID and key.
    pub async fn delete_profile_entry(
        &self,
        user_id: &str,
        key: &str,
    ) -> Result<(), WorkspaceError> {
        let conn = self.conn().await?;
        conn.execute(
            "DELETE FROM memory_profiles WHERE user_id = $1 AND key = $2",
            &[&user_id, &key],
        )
        .await
        .map_err(|e| WorkspaceError::SearchFailed {
            reason: format!("Delete profile entry failed: {}", e),
        })?;
        Ok(())
    }

    /// Convert a database row to a `UserProfile`.
    fn row_to_profile(&self, row: &tokio_postgres::Row) -> UserProfile {
        let pt_str: String = row.get("profile_type");
        UserProfile {
            id: row.get("id"),
            user_id: row.get("user_id"),
            profile_type: if pt_str == "static" {
                ProfileType::Static
            } else {
                ProfileType::Dynamic
            },
            key: row.get("key"),
            value: row.get("value"),
            confidence: row.get("confidence"),
            source: row.get("source"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }
    }

    // ==================== Document Metadata Operations ====================

    /// Record an access to a document (increments access_count, updates last_accessed_at).
    pub async fn record_document_access(&self, document_id: Uuid) -> Result<(), WorkspaceError> {
        let conn = self.conn().await?;
        conn.execute(
            r#"
            UPDATE memory_documents
            SET access_count = access_count + 1, last_accessed_at = NOW()
            WHERE id = $1
            "#,
            &[&document_id],
        )
        .await
        .map_err(|e| WorkspaceError::SearchFailed {
            reason: format!("Record access failed: {}", e),
        })?;
        Ok(())
    }

    /// Update document metadata fields (merges into existing metadata).
    pub async fn update_document_metadata(
        &self,
        document_id: Uuid,
        metadata: &serde_json::Value,
    ) -> Result<(), WorkspaceError> {
        let conn = self.conn().await?;
        conn.execute(
            r#"
            UPDATE memory_documents
            SET metadata = metadata || $2
            WHERE id = $1
            "#,
            &[&document_id, &metadata],
        )
        .await
        .map_err(|e| WorkspaceError::SearchFailed {
            reason: format!("Update metadata failed: {}", e),
        })?;
        Ok(())
    }
}
