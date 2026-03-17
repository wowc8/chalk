//! Feature flags via SQLite — `feature_flags` table with `is_enabled` API.
//!
//! Provides a simple boolean feature flag system backed by SQLite.
//! Each flag has a name, enabled state, and optional description.
//! Flags are managed via Tauri commands and exposed in Settings toggles.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::database::{Database, DatabaseError};
use crate::errors::ChalkError;

/// A feature flag record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlag {
    pub name: String,
    pub enabled: bool,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Input for creating or updating a feature flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlagInput {
    pub name: String,
    pub enabled: bool,
    pub description: Option<String>,
}

// ── Migration ────────────────────────────────────────────────

/// SQL migration for the feature_flags table.
pub const FEATURE_FLAGS_MIGRATION: (i32, &str, &str) = (
    3,
    "feature_flags",
    "CREATE TABLE IF NOT EXISTS feature_flags (
        name        TEXT PRIMARY KEY NOT NULL,
        enabled     INTEGER NOT NULL DEFAULT 0,
        description TEXT,
        created_at  TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
    );",
);

// ── Database operations ──────────────────────────────────────

impl Database {
    /// Check if a feature flag is enabled. Returns `false` for unknown flags.
    pub fn is_flag_enabled(&self, name: &str) -> Result<bool, ChalkError> {
        self.with_conn(|conn| {
            let result = conn.query_row(
                "SELECT enabled FROM feature_flags WHERE name = ?1",
                params![name],
                |row| {
                    let val: i32 = row.get(0)?;
                    Ok(val != 0)
                },
            );
            match result {
                Ok(enabled) => Ok(enabled),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
                Err(e) => Err(DatabaseError::Sqlite(e)),
            }
        })
        .map_err(ChalkError::from)
    }

    /// Get a specific feature flag. Returns an error if not found.
    pub fn get_feature_flag(&self, name: &str) -> Result<FeatureFlag, ChalkError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT name, enabled, description, created_at, updated_at
                 FROM feature_flags WHERE name = ?1",
                params![name],
                |row| {
                    let enabled: i32 = row.get(1)?;
                    Ok(FeatureFlag {
                        name: row.get(0)?,
                        enabled: enabled != 0,
                        description: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                other => DatabaseError::Sqlite(other),
            })
        })
        .map_err(ChalkError::from)
    }

    /// List all feature flags.
    pub fn list_feature_flags(&self) -> Result<Vec<FeatureFlag>, ChalkError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT name, enabled, description, created_at, updated_at
                 FROM feature_flags ORDER BY name",
            )?;
            let rows = stmt.query_map([], |row| {
                let enabled: i32 = row.get(1)?;
                Ok(FeatureFlag {
                    name: row.get(0)?,
                    enabled: enabled != 0,
                    description: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
        .map_err(ChalkError::from)
    }

    /// Create or update a feature flag (upsert).
    pub fn set_feature_flag(&self, input: &FeatureFlagInput) -> Result<FeatureFlag, ChalkError> {
        self.with_conn(|conn| {
            let enabled_int: i32 = if input.enabled { 1 } else { 0 };
            conn.execute(
                "INSERT INTO feature_flags (name, enabled, description)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(name) DO UPDATE SET
                    enabled = excluded.enabled,
                    description = excluded.description,
                    updated_at = datetime('now')",
                params![input.name, enabled_int, input.description],
            )?;

            conn.query_row(
                "SELECT name, enabled, description, created_at, updated_at
                 FROM feature_flags WHERE name = ?1",
                params![input.name],
                |row| {
                    let enabled: i32 = row.get(1)?;
                    Ok(FeatureFlag {
                        name: row.get(0)?,
                        enabled: enabled != 0,
                        description: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                other => DatabaseError::Sqlite(other),
            })
        })
        .map_err(ChalkError::from)
    }

    /// Toggle a feature flag. Returns the new state.
    pub fn toggle_feature_flag(&self, name: &str) -> Result<FeatureFlag, ChalkError> {
        self.with_conn(|conn| {
            let updated = conn.execute(
                "UPDATE feature_flags
                 SET enabled = CASE WHEN enabled = 0 THEN 1 ELSE 0 END,
                     updated_at = datetime('now')
                 WHERE name = ?1",
                params![name],
            )?;

            if updated == 0 {
                return Err(DatabaseError::NotFound);
            }

            conn.query_row(
                "SELECT name, enabled, description, created_at, updated_at
                 FROM feature_flags WHERE name = ?1",
                params![name],
                |row| {
                    let enabled: i32 = row.get(1)?;
                    Ok(FeatureFlag {
                        name: row.get(0)?,
                        enabled: enabled != 0,
                        description: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                other => DatabaseError::Sqlite(other),
            })
        })
        .map_err(ChalkError::from)
    }

    /// Delete a feature flag.
    pub fn delete_feature_flag(&self, name: &str) -> Result<(), ChalkError> {
        self.with_conn(|conn| {
            let deleted =
                conn.execute("DELETE FROM feature_flags WHERE name = ?1", params![name])?;
            if deleted == 0 {
                return Err(DatabaseError::NotFound);
            }
            Ok(())
        })
        .map_err(ChalkError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;

    fn test_db() -> Database {
        let db = Database::open_in_memory().unwrap();
        // Run the feature flags migration manually.
        db.with_conn(|conn| {
            conn.execute_batch(FEATURE_FLAGS_MIGRATION.2)?;
            Ok(())
        })
        .unwrap();
        db
    }

    #[test]
    fn test_is_flag_enabled_unknown_returns_false() {
        let db = test_db();
        assert!(!db.is_flag_enabled("nonexistent").unwrap());
    }

    #[test]
    fn test_set_and_get_flag() {
        let db = test_db();
        let flag = db
            .set_feature_flag(&FeatureFlagInput {
                name: "dark_mode".into(),
                enabled: true,
                description: Some("Enable dark mode UI".into()),
            })
            .unwrap();

        assert_eq!(flag.name, "dark_mode");
        assert!(flag.enabled);
        assert_eq!(flag.description.as_deref(), Some("Enable dark mode UI"));

        let fetched = db.get_feature_flag("dark_mode").unwrap();
        assert!(fetched.enabled);
    }

    #[test]
    fn test_is_flag_enabled() {
        let db = test_db();
        db.set_feature_flag(&FeatureFlagInput {
            name: "beta_editor".into(),
            enabled: false,
            description: None,
        })
        .unwrap();

        assert!(!db.is_flag_enabled("beta_editor").unwrap());

        db.set_feature_flag(&FeatureFlagInput {
            name: "beta_editor".into(),
            enabled: true,
            description: None,
        })
        .unwrap();

        assert!(db.is_flag_enabled("beta_editor").unwrap());
    }

    #[test]
    fn test_list_feature_flags() {
        let db = test_db();
        db.set_feature_flag(&FeatureFlagInput {
            name: "alpha".into(),
            enabled: true,
            description: None,
        })
        .unwrap();
        db.set_feature_flag(&FeatureFlagInput {
            name: "beta".into(),
            enabled: false,
            description: Some("Beta feature".into()),
        })
        .unwrap();

        let flags = db.list_feature_flags().unwrap();
        assert_eq!(flags.len(), 2);
        assert_eq!(flags[0].name, "alpha");
        assert_eq!(flags[1].name, "beta");
    }

    #[test]
    fn test_toggle_feature_flag() {
        let db = test_db();
        db.set_feature_flag(&FeatureFlagInput {
            name: "toggle_test".into(),
            enabled: false,
            description: None,
        })
        .unwrap();

        let toggled = db.toggle_feature_flag("toggle_test").unwrap();
        assert!(toggled.enabled);

        let toggled = db.toggle_feature_flag("toggle_test").unwrap();
        assert!(!toggled.enabled);
    }

    #[test]
    fn test_toggle_nonexistent_flag_errors() {
        let db = test_db();
        let result = db.toggle_feature_flag("nope");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_feature_flag() {
        let db = test_db();
        db.set_feature_flag(&FeatureFlagInput {
            name: "to_delete".into(),
            enabled: true,
            description: None,
        })
        .unwrap();

        db.delete_feature_flag("to_delete").unwrap();
        assert!(!db.is_flag_enabled("to_delete").unwrap());
    }

    #[test]
    fn test_delete_nonexistent_flag_errors() {
        let db = test_db();
        let result = db.delete_feature_flag("nope");
        assert!(result.is_err());
    }

    #[test]
    fn test_upsert_flag() {
        let db = test_db();
        db.set_feature_flag(&FeatureFlagInput {
            name: "upsert_test".into(),
            enabled: false,
            description: Some("v1".into()),
        })
        .unwrap();

        let updated = db
            .set_feature_flag(&FeatureFlagInput {
                name: "upsert_test".into(),
                enabled: true,
                description: Some("v2".into()),
            })
            .unwrap();

        assert!(updated.enabled);
        assert_eq!(updated.description.as_deref(), Some("v2"));

        // Should still be just one flag.
        let flags = db.list_feature_flags().unwrap();
        assert_eq!(flags.len(), 1);
    }

    #[test]
    fn test_flag_serialization() {
        let flag = FeatureFlag {
            name: "test_flag".into(),
            enabled: true,
            description: Some("A test flag".into()),
            created_at: "2026-01-01 00:00:00".into(),
            updated_at: "2026-01-01 00:00:00".into(),
        };
        let json = serde_json::to_value(&flag).unwrap();
        assert_eq!(json["name"], "test_flag");
        assert_eq!(json["enabled"], true);
    }

    #[test]
    fn test_get_nonexistent_flag_errors() {
        let db = test_db();
        let result = db.get_feature_flag("does_not_exist");
        assert!(result.is_err());
    }
}
