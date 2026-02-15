-- V9: Supermemory-inspired memory features
--
-- Adds knowledge graph connections, spaces (collections), user profiles,
-- and enhanced document metadata for temporal awareness and importance decay.

-- ==================== Memory Connections (Knowledge Graph) ====================

CREATE TABLE IF NOT EXISTS memory_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_id UUID NOT NULL REFERENCES memory_documents(id) ON DELETE CASCADE,
    target_id UUID NOT NULL REFERENCES memory_documents(id) ON DELETE CASCADE,
    connection_type TEXT NOT NULL CHECK (connection_type IN ('updates', 'extends', 'derives')),
    strength REAL NOT NULL DEFAULT 1.0 CHECK (strength >= 0.0 AND strength <= 1.0),
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_connection UNIQUE (source_id, target_id, connection_type),
    CONSTRAINT no_self_connection CHECK (source_id != target_id)
);

CREATE INDEX idx_memory_connections_source ON memory_connections(source_id);
CREATE INDEX idx_memory_connections_target ON memory_connections(target_id);
CREATE INDEX idx_memory_connections_type ON memory_connections(connection_type);

-- ==================== Memory Spaces (Collections) ====================

CREATE TABLE IF NOT EXISTS memory_spaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_space_name UNIQUE (user_id, name)
);

CREATE INDEX idx_memory_spaces_user ON memory_spaces(user_id);

-- Trigger to auto-update updated_at on memory_spaces
CREATE TRIGGER update_memory_spaces_updated_at
    BEFORE UPDATE ON memory_spaces
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- ==================== Space Membership ====================

CREATE TABLE IF NOT EXISTS memory_space_members (
    space_id UUID NOT NULL REFERENCES memory_spaces(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES memory_documents(id) ON DELETE CASCADE,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (space_id, document_id)
);

CREATE INDEX idx_memory_space_members_doc ON memory_space_members(document_id);

-- ==================== User Profiles ====================

CREATE TABLE IF NOT EXISTS memory_profiles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    profile_type TEXT NOT NULL CHECK (profile_type IN ('static', 'dynamic')),
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 1.0 CHECK (confidence >= 0.0 AND confidence <= 1.0),
    source TEXT NOT NULL DEFAULT 'user_stated',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_profile_key UNIQUE (user_id, key)
);

CREATE INDEX idx_memory_profiles_user ON memory_profiles(user_id);
CREATE INDEX idx_memory_profiles_type ON memory_profiles(user_id, profile_type);

-- Trigger to auto-update updated_at on memory_profiles
CREATE TRIGGER update_memory_profiles_updated_at
    BEFORE UPDATE ON memory_profiles
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- ==================== Enhanced Document Metadata Columns ====================

-- Source URL for content ingested from the web
ALTER TABLE memory_documents ADD COLUMN IF NOT EXISTS source_url TEXT;

-- When the event described in the document occurred (vs created_at = when stored)
ALTER TABLE memory_documents ADD COLUMN IF NOT EXISTS event_date DATE;

-- Importance score with decay (0.0-1.0)
ALTER TABLE memory_documents ADD COLUMN IF NOT EXISTS importance REAL NOT NULL DEFAULT 0.5;

-- Access tracking for recency-based ranking
ALTER TABLE memory_documents ADD COLUMN IF NOT EXISTS access_count BIGINT NOT NULL DEFAULT 0;
ALTER TABLE memory_documents ADD COLUMN IF NOT EXISTS last_accessed_at TIMESTAMPTZ;

-- Tags for categorization
ALTER TABLE memory_documents ADD COLUMN IF NOT EXISTS tags TEXT[] NOT NULL DEFAULT '{}';

-- Index for tag-based filtering
CREATE INDEX idx_memory_documents_tags ON memory_documents USING GIN(tags);

-- Index for importance-based ordering
CREATE INDEX idx_memory_documents_importance ON memory_documents(importance DESC);

-- Index for source URL lookups
CREATE INDEX idx_memory_documents_source_url ON memory_documents(source_url) WHERE source_url IS NOT NULL;
