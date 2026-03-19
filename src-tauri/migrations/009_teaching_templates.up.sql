-- Teaching templates: captures HOW a teacher formats their lesson plans.
-- Extracted during digest to reproduce the teacher's style when AI generates new plans.
-- Stores a JSON blob with color scheme, table structure, time slots, content patterns,
-- and recurring elements.
CREATE TABLE IF NOT EXISTS teaching_templates (
    id              TEXT PRIMARY KEY,
    source_doc_id   TEXT,
    source_doc_name TEXT,
    template_json   TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_teaching_templates_source ON teaching_templates(source_doc_id);
