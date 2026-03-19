use rusqlite::Connection;
use std::path::{Path, PathBuf};

use super::connection::{DatabaseError, Result};

/// A parsed migration from the migrations/ directory.
#[derive(Debug, Clone)]
struct Migration {
    version: i32,
    description: String,
    up_sql: String,
    down_sql: Option<String>,
}

/// Discover migration SQL files from the given directory.
/// Expects filenames like `001_initial_schema.up.sql` and optional `001_initial_schema.down.sql`.
fn discover_migrations(migrations_dir: &Path) -> Result<Vec<Migration>> {
    let mut ups: Vec<(i32, String, PathBuf)> = Vec::new();
    let mut downs: std::collections::HashMap<i32, PathBuf> = std::collections::HashMap::new();

    if !migrations_dir.exists() {
        tracing::warn!(
            path = %migrations_dir.display(),
            "Migrations directory not found, using embedded migrations"
        );
        return Ok(embedded_migrations());
    }

    let entries = std::fs::read_dir(migrations_dir).map_err(|e| {
        DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
            Some(format!("Cannot read migrations dir: {}", e)),
        ))
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| {
            DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
                Some(format!("Cannot read migration entry: {}", e)),
            ))
        })?;
        let path = entry.path();
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if !filename.ends_with(".sql") {
            continue;
        }

        if filename.ends_with(".up.sql") {
            let stem = filename.trim_end_matches(".up.sql");
            if let Some((version, description)) = parse_migration_stem(stem) {
                ups.push((version, description, path));
            }
        } else if filename.ends_with(".down.sql") {
            let stem = filename.trim_end_matches(".down.sql");
            if let Some((version, _)) = parse_migration_stem(stem) {
                downs.insert(version, path);
            }
        }
    }

    ups.sort_by_key(|(v, _, _)| *v);

    let mut migrations = Vec::new();
    for (version, description, up_path) in ups {
        let up_sql = std::fs::read_to_string(&up_path).map_err(|e| {
            DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
                Some(format!("Cannot read migration {}: {}", up_path.display(), e)),
            ))
        })?;

        let down_sql = if let Some(down_path) = downs.get(&version) {
            Some(std::fs::read_to_string(down_path).map_err(|e| {
                DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
                    Some(format!(
                        "Cannot read down migration {}: {}",
                        down_path.display(),
                        e
                    )),
                ))
            })?)
        } else {
            None
        };

        migrations.push(Migration {
            version,
            description,
            up_sql,
            down_sql,
        });
    }

    if migrations.is_empty() {
        tracing::warn!("No migration files found, using embedded migrations");
        return Ok(embedded_migrations());
    }

    Ok(migrations)
}

/// Parse a migration filename stem like "001_initial_schema" into (1, "initial_schema").
fn parse_migration_stem(stem: &str) -> Option<(i32, String)> {
    let underscore_pos = stem.find('_')?;
    let version_str = &stem[..underscore_pos];
    let version = version_str.parse::<i32>().ok()?;
    let description = stem[underscore_pos + 1..].to_string();
    Some((version, description))
}

/// Embedded fallback migrations for when SQL files aren't available (tests, bundled app).
fn embedded_migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            description: "initial_schema".to_string(),
            up_sql: include_str!("../../migrations/001_initial_schema.up.sql").to_string(),
            down_sql: Some(
                include_str!("../../migrations/001_initial_schema.down.sql").to_string(),
            ),
        },
        Migration {
            version: 2,
            description: "vector_tables".to_string(),
            up_sql: include_str!("../../migrations/002_vector_tables.up.sql").to_string(),
            down_sql: Some(
                include_str!("../../migrations/002_vector_tables.down.sql").to_string(),
            ),
        },
        Migration {
            version: 3,
            description: "app_settings".to_string(),
            up_sql: include_str!("../../migrations/003_app_settings.up.sql").to_string(),
            down_sql: Some(
                include_str!("../../migrations/003_app_settings.down.sql").to_string(),
            ),
        },
        Migration {
            version: 4,
            description: "tags_and_library".to_string(),
            up_sql: include_str!("../../migrations/004_tags_and_library.up.sql").to_string(),
            down_sql: Some(
                include_str!("../../migrations/004_tags_and_library.down.sql").to_string(),
            ),
        },
        Migration {
            version: 5,
            description: "chat_tables".to_string(),
            up_sql: include_str!("../../migrations/005_chat_tables.up.sql").to_string(),
            down_sql: Some(
                include_str!("../../migrations/005_chat_tables.down.sql").to_string(),
            ),
        },
        Migration {
            version: 6,
            description: "plan_versions".to_string(),
            up_sql: include_str!("../../migrations/006_plan_versions.up.sql").to_string(),
            down_sql: Some(
                include_str!("../../migrations/006_plan_versions.down.sql").to_string(),
            ),
        },
        Migration {
            version: 7,
            description: "fts5_fulltext_search".to_string(),
            up_sql: include_str!("../../migrations/007_fts5_fulltext_search.up.sql").to_string(),
            down_sql: Some(
                include_str!("../../migrations/007_fts5_fulltext_search.down.sql").to_string(),
            ),
        },
        Migration {
            version: 8,
            description: "reference_docs".to_string(),
            up_sql: include_str!("../../migrations/008_reference_docs.up.sql").to_string(),
            down_sql: Some(
                include_str!("../../migrations/008_reference_docs.down.sql").to_string(),
            ),
        },
        Migration {
            version: 9,
            description: "teaching_templates".to_string(),
            up_sql: include_str!("../../migrations/009_teaching_templates.up.sql").to_string(),
            down_sql: Some(
                include_str!("../../migrations/009_teaching_templates.down.sql").to_string(),
            ),
        },
    ]
}

/// Run all pending up-migrations on the given connection.
/// Uses a `_migrations` table to track which versions have been applied.
pub fn run_all(conn: &Connection) -> Result<()> {
    run_all_from(conn, None)
}

/// Run all pending migrations, optionally reading SQL files from `migrations_dir`.
/// Falls back to embedded migrations if the directory is not provided or empty.
pub fn run_all_from(conn: &Connection, migrations_dir: Option<&Path>) -> Result<()> {
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

    let migrations = match migrations_dir {
        Some(dir) => discover_migrations(dir)?,
        None => embedded_migrations(),
    };

    for migration in &migrations {
        if migration.version > current_version {
            tracing::info!(
                version = migration.version,
                description = %migration.description,
                "Applying migration"
            );
            conn.execute_batch(&migration.up_sql)?;
            conn.execute(
                "INSERT INTO _migrations (version, description) VALUES (?1, ?2)",
                rusqlite::params![migration.version, migration.description],
            )?;
        }
    }

    Ok(())
}

/// Rollback the most recently applied migration (for development use).
/// Returns the version that was rolled back, or None if no migrations to rollback.
pub fn rollback_last(conn: &Connection, migrations_dir: Option<&Path>) -> Result<Option<i32>> {
    // Ensure the tracking table exists.
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

    if current_version == 0 {
        return Ok(None);
    }

    let migrations = match migrations_dir {
        Some(dir) => discover_migrations(dir)?,
        None => embedded_migrations(),
    };

    let migration = migrations
        .iter()
        .find(|m| m.version == current_version)
        .ok_or_else(|| {
            DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
                Some(format!(
                    "No migration found for version {} to rollback",
                    current_version
                )),
            ))
        })?;

    let down_sql = migration.down_sql.as_ref().ok_or_else(|| {
        DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!(
                "No down migration for version {}",
                current_version
            )),
        ))
    })?;

    tracing::info!(
        version = migration.version,
        description = %migration.description,
        "Rolling back migration"
    );
    conn.execute_batch(down_sql)?;
    conn.execute(
        "DELETE FROM _migrations WHERE version = ?1",
        rusqlite::params![current_version],
    )?;

    Ok(Some(current_version))
}

/// Get the current migration version.
pub fn current_version(conn: &Connection) -> Result<i32> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version     INTEGER PRIMARY KEY,
            description TEXT NOT NULL,
            applied_at  TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )?;

    let version: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM _migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        conn
    }

    #[test]
    fn test_migrations_are_idempotent() {
        let conn = test_conn();

        // Run twice — should not error.
        run_all(&conn).unwrap();
        run_all(&conn).unwrap();

        let version: i32 = conn
            .query_row("SELECT MAX(version) FROM _migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 9);
    }

    #[test]
    fn test_embedded_migrations_match_file_count() {
        let migrations = embedded_migrations();
        assert_eq!(migrations.len(), 9);
        assert_eq!(migrations[0].version, 1);
        assert_eq!(migrations[0].description, "initial_schema");
        assert_eq!(migrations[1].version, 2);
        assert_eq!(migrations[1].description, "vector_tables");
        assert_eq!(migrations[2].version, 3);
        assert_eq!(migrations[2].description, "app_settings");
        assert_eq!(migrations[3].version, 4);
        assert_eq!(migrations[3].description, "tags_and_library");
        assert_eq!(migrations[4].version, 5);
        assert_eq!(migrations[4].description, "chat_tables");
        assert_eq!(migrations[5].version, 6);
        assert_eq!(migrations[5].description, "plan_versions");
        assert_eq!(migrations[6].version, 7);
        assert_eq!(migrations[6].description, "fts5_fulltext_search");
        assert_eq!(migrations[7].version, 8);
        assert_eq!(migrations[7].description, "reference_docs");
        assert_eq!(migrations[8].version, 9);
        assert_eq!(migrations[8].description, "teaching_templates");
    }

    #[test]
    fn test_current_version_tracking() {
        let conn = test_conn();

        assert_eq!(current_version(&conn).unwrap(), 0);

        run_all(&conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), 9);
    }

    #[test]
    fn test_rollback_last() {
        let conn = test_conn();

        run_all(&conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), 9);

        // Rollback version 9 (teaching_templates).
        let rolled_back = rollback_last(&conn, None).unwrap();
        assert_eq!(rolled_back, Some(9));
        assert_eq!(current_version(&conn).unwrap(), 8);

        // Rollback version 8 (reference_docs).
        let rolled_back = rollback_last(&conn, None).unwrap();
        assert_eq!(rolled_back, Some(8));
        assert_eq!(current_version(&conn).unwrap(), 7);

        // Rollback version 7 (fts5_fulltext_search).
        let rolled_back = rollback_last(&conn, None).unwrap();
        assert_eq!(rolled_back, Some(7));
        assert_eq!(current_version(&conn).unwrap(), 6);

        // Rollback version 6 (plan_versions).
        let rolled_back = rollback_last(&conn, None).unwrap();
        assert_eq!(rolled_back, Some(6));
        assert_eq!(current_version(&conn).unwrap(), 5);

        // Rollback version 5 (chat_tables).
        let rolled_back = rollback_last(&conn, None).unwrap();
        assert_eq!(rolled_back, Some(5));
        assert_eq!(current_version(&conn).unwrap(), 4);

        // Rollback version 4 (tags_and_library).
        let rolled_back = rollback_last(&conn, None).unwrap();
        assert_eq!(rolled_back, Some(4));
        assert_eq!(current_version(&conn).unwrap(), 3);

        // Rollback version 3 (app_settings).
        let rolled_back = rollback_last(&conn, None).unwrap();
        assert_eq!(rolled_back, Some(3));
        assert_eq!(current_version(&conn).unwrap(), 2);

        // Rollback version 2 (vector tables).
        let rolled_back = rollback_last(&conn, None).unwrap();
        assert_eq!(rolled_back, Some(2));
        assert_eq!(current_version(&conn).unwrap(), 1);

        // Rollback version 1 (initial schema).
        let rolled_back = rollback_last(&conn, None).unwrap();
        assert_eq!(rolled_back, Some(1));
        assert_eq!(current_version(&conn).unwrap(), 0);

        // No more to rollback.
        let rolled_back = rollback_last(&conn, None).unwrap();
        assert_eq!(rolled_back, None);
    }

    #[test]
    fn test_rollback_and_reapply() {
        let conn = test_conn();

        run_all(&conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), 9);

        // Rollback all.
        for _ in 0..9 {
            rollback_last(&conn, None).unwrap();
        }
        assert_eq!(current_version(&conn).unwrap(), 0);

        // Reapply all.
        run_all(&conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), 9);

        // Verify tables exist after reapply.
        let table_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='subjects'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 1);
    }

    #[test]
    fn test_parse_migration_stem() {
        assert_eq!(
            parse_migration_stem("001_initial_schema"),
            Some((1, "initial_schema".to_string()))
        );
        assert_eq!(
            parse_migration_stem("042_add_connectors"),
            Some((42, "add_connectors".to_string()))
        );
        assert_eq!(parse_migration_stem("invalid"), None);
        assert_eq!(parse_migration_stem("abc_desc"), None);
    }

    #[test]
    fn test_discover_migrations_from_dir() {
        let dir = tempfile::tempdir().unwrap();

        // Create migration files.
        std::fs::write(
            dir.path().join("001_test.up.sql"),
            "CREATE TABLE test1 (id INTEGER PRIMARY KEY);",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("001_test.down.sql"),
            "DROP TABLE IF EXISTS test1;",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("002_test2.up.sql"),
            "CREATE TABLE test2 (id INTEGER PRIMARY KEY);",
        )
        .unwrap();

        let migrations = discover_migrations(dir.path()).unwrap();
        assert_eq!(migrations.len(), 2);
        assert_eq!(migrations[0].version, 1);
        assert!(migrations[0].down_sql.is_some());
        assert_eq!(migrations[1].version, 2);
        assert!(migrations[1].down_sql.is_none());
    }

    #[test]
    fn test_run_all_from_directory() {
        let conn = test_conn();
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(
            dir.path().join("001_test.up.sql"),
            "CREATE TABLE dir_test (id INTEGER PRIMARY KEY, name TEXT);",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("001_test.down.sql"),
            "DROP TABLE IF EXISTS dir_test;",
        )
        .unwrap();

        run_all_from(&conn, Some(dir.path())).unwrap();
        assert_eq!(current_version(&conn).unwrap(), 1);

        // Verify the table was created.
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='dir_test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Rollback from directory.
        let rolled_back = rollback_last(&conn, Some(dir.path())).unwrap();
        assert_eq!(rolled_back, Some(1));

        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='dir_test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_discover_nonexistent_dir_falls_back() {
        let migrations =
            discover_migrations(Path::new("/nonexistent/migrations/dir")).unwrap();
        // Falls back to embedded migrations.
        assert_eq!(migrations.len(), 9);
    }

    #[test]
    fn test_partial_migration_applies_only_pending() {
        let conn = test_conn();

        // Apply only version 1 manually.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _migrations (
                version     INTEGER PRIMARY KEY,
                description TEXT NOT NULL,
                applied_at  TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .unwrap();

        let migrations = embedded_migrations();
        conn.execute_batch(&migrations[0].up_sql).unwrap();
        conn.execute(
            "INSERT INTO _migrations (version, description) VALUES (1, 'initial_schema')",
            [],
        )
        .unwrap();

        assert_eq!(current_version(&conn).unwrap(), 1);

        // Run all — should apply remaining versions.
        run_all(&conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), 9);
    }
}
