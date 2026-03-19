-- Reference documents table: stores digested content from Google Docs
-- for RAG/embedding context. NOT shown in the library — only feeds AI.
CREATE TABLE IF NOT EXISTS reference_docs (
    id              TEXT PRIMARY KEY,
    source_doc_id   TEXT,
    source_doc_name TEXT,
    title           TEXT NOT NULL,
    content_html    TEXT NOT NULL DEFAULT '',
    content_text    TEXT NOT NULL DEFAULT '',
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_reference_docs_source ON reference_docs(source_doc_id);

-- Vector table for reference doc embeddings (1536-dim, same as lesson plans).
CREATE VIRTUAL TABLE IF NOT EXISTS reference_doc_vectors USING vec0(embedding float[1536]);

-- FTS5 index for keyword search across reference doc content.
CREATE VIRTUAL TABLE IF NOT EXISTS reference_docs_fts USING fts5(
    title,
    content_text,
    content=reference_docs,
    content_rowid=rowid
);

-- Populate FTS index with any existing data (empty on fresh install).
INSERT INTO reference_docs_fts(rowid, title, content_text)
    SELECT rowid, title, content_text
    FROM reference_docs;

-- Triggers to keep reference_docs_fts in sync.
CREATE TRIGGER IF NOT EXISTS reference_docs_fts_insert AFTER INSERT ON reference_docs BEGIN
    INSERT INTO reference_docs_fts(rowid, title, content_text)
        VALUES (NEW.rowid, NEW.title, NEW.content_text);
END;

CREATE TRIGGER IF NOT EXISTS reference_docs_fts_update AFTER UPDATE ON reference_docs BEGIN
    INSERT INTO reference_docs_fts(reference_docs_fts, rowid, title, content_text)
        VALUES ('delete', OLD.rowid, OLD.title, OLD.content_text);
    INSERT INTO reference_docs_fts(rowid, title, content_text)
        VALUES (NEW.rowid, NEW.title, NEW.content_text);
END;

CREATE TRIGGER IF NOT EXISTS reference_docs_fts_delete AFTER DELETE ON reference_docs BEGIN
    INSERT INTO reference_docs_fts(reference_docs_fts, rowid, title, content_text)
        VALUES ('delete', OLD.rowid, OLD.title, OLD.content_text);
END;

-- Clean up previously imported lesson plans that flooded the library.
-- These are now stored as reference_docs instead.
DELETE FROM lesson_plans WHERE source_type = 'imported';
