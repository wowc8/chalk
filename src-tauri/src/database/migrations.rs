use rusqlite::Connection;

use super::connection::Result;

/// Each migration is a (version, description, sql) tuple.
/// Migrations are applied in order and tracked in the `_migrations` table.
const MIGRATIONS: &[(i32, &str, &str)] = &[
    (1, "initial_schema", MIGRATION_001),
    (2, "vector_tables", MIGRATION_002),
];

const MIGRATION_001: &str = "
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
";

const MIGRATION_002: &str = "
CREATE VIRTUAL TABLE IF NOT EXISTS lesson_plan_vectors USING vec0(
    embedding float[1536]
);
";

pub fn run_all(conn: &Connection) -> Result<()> {
    // Create the migrations tracking table.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version     INTEGER PRIMARY KEY,
            description TEXT NOT NULL,
            applied_at  TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )?;

    let current_version: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM _migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    for &(version, description, sql) in MIGRATIONS {
        if version > current_version {
            tracing::info!(version, description, "Applying migration");
            conn.execute_batch(sql)?;
            conn.execute(
                "INSERT INTO _migrations (version, description) VALUES (?1, ?2)",
                rusqlite::params![version, description],
            )?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrations_are_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();

        // Run twice — should not error.
        run_all(&conn).unwrap();
        run_all(&conn).unwrap();

        let version: i32 = conn
            .query_row("SELECT MAX(version) FROM _migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 2);
    }
}
