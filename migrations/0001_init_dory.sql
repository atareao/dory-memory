CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS pgvector;

CREATE TABLE IF NOT EXISTS dory_namespaces (
    name VARCHAR(50) PRIMARY KEY,
    embedding_model VARCHAR(100) NOT NULL,
    dimensions INT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS dory_memories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    namespace VARCHAR(50) REFERENCES dory_namespaces(name) ON DELETE CASCADE NOT NULL,
    content_l0 TEXT NOT NULL,
    content_l1 TEXT,
    content_l2 TEXT,
    embedding VECTOR NOT NULL,
    importance REAL DEFAULT 1.0,
    is_immortal BOOLEAN DEFAULT FALSE,
    tags JSONB DEFAULT '[]'::jsonb,
    fts_tokens TSVECTOR GENERATED ALWAYS AS (
        to_tsvector('english', content_l0 || ' ' || COALESCE(content_l2, ''))
    ) STORED,
    deferred_until TIMESTAMPTZ DEFAULT NULL,
    last_accessed_at TIMESTAMPTZ DEFAULT NOW(),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_dory_tags ON dory_memories USING gin (tags);
CREATE INDEX IF NOT EXISTS idx_dory_fts ON dory_memories USING gin (fts_tokens);
CREATE INDEX IF NOT EXISTS idx_dory_immortal ON dory_memories (is_immortal);
CREATE INDEX IF NOT EXISTS idx_dory_namespace ON dory_memories (namespace);
CREATE INDEX IF NOT EXISTS idx_dory_created_at ON dory_memories (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_dory_deferred ON dory_memories (deferred_until) WHERE deferred_until IS NOT NULL;

CREATE OR REPLACE FUNCTION enforce_dory_immortality()
RETURNS TRIGGER AS $$
BEGIN
    IF OLD.is_immortal THEN
        RAISE EXCEPTION 'Database Guard: Execution denied. Memory node % is marked immortal.', OLD.id;
    END IF;
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_protect_dory_nodes ON dory_memories;
CREATE TRIGGER trg_protect_dory_nodes
BEFORE DELETE ON dory_memories
FOR EACH ROW
EXECUTE FUNCTION enforce_dory_immortality();