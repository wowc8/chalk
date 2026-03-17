-- Initial schema: subjects, lesson_plans, metadata tables

CREATE TABLE IF NOT EXISTS subjects (
    id              TEXT PRIMARY KEY NOT NULL,
    name            TEXT NOT NULL,
    grade_level     TEXT,
    description     TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS lesson_plans (
    id              TEXT PRIMARY KEY NOT NULL,
    subject_id      TEXT NOT NULL,
    title           TEXT NOT NULL,
    content         TEXT NOT NULL DEFAULT '',
    source_doc_id   TEXT,
    source_table_index  INTEGER,
    learning_objectives TEXT,
    status          TEXT NOT NULL DEFAULT 'draft',
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (subject_id) REFERENCES subjects(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS metadata (
    id              TEXT PRIMARY KEY NOT NULL,
    lesson_plan_id  TEXT NOT NULL,
    key             TEXT NOT NULL,
    value           TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (lesson_plan_id) REFERENCES lesson_plans(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_lesson_plans_subject ON lesson_plans(subject_id);
CREATE INDEX IF NOT EXISTS idx_lesson_plans_status ON lesson_plans(status);
CREATE INDEX IF NOT EXISTS idx_metadata_lesson_plan ON metadata(lesson_plan_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_metadata_unique_key ON metadata(lesson_plan_id, key);
