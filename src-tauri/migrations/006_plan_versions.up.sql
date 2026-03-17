-- Plan version snapshots: each finalize creates a new row.
CREATE TABLE IF NOT EXISTS plan_versions (
    id           TEXT PRIMARY KEY,
    plan_id      TEXT NOT NULL REFERENCES lesson_plans(id) ON DELETE CASCADE,
    version      INTEGER NOT NULL,
    title        TEXT NOT NULL,
    content      TEXT NOT NULL DEFAULT '',
    learning_objectives TEXT,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),

    UNIQUE(plan_id, version)
);

CREATE INDEX IF NOT EXISTS idx_plan_versions_plan_id ON plan_versions(plan_id);
