-- Tags table: teacher-customizable labels for lesson plans
CREATE TABLE IF NOT EXISTS tags (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL UNIQUE,
    color       TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Join table: many-to-many relationship between lesson plans and tags
CREATE TABLE IF NOT EXISTS plan_tags (
    plan_id     TEXT NOT NULL,
    tag_id      TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (plan_id, tag_id),
    FOREIGN KEY (plan_id) REFERENCES lesson_plans(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

-- Add source_type to lesson_plans to distinguish 'created' vs 'imported'
ALTER TABLE lesson_plans ADD COLUMN source_type TEXT NOT NULL DEFAULT 'created';

-- Add version column to lesson_plans
ALTER TABLE lesson_plans ADD COLUMN version INTEGER NOT NULL DEFAULT 1;

CREATE INDEX IF NOT EXISTS idx_plan_tags_plan ON plan_tags(plan_id);
CREATE INDEX IF NOT EXISTS idx_plan_tags_tag ON plan_tags(tag_id);
CREATE INDEX IF NOT EXISTS idx_lesson_plans_source_type ON lesson_plans(source_type);
CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);
