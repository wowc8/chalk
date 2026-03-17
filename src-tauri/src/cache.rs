//! SQLite read-through cache with TTL for Drive API responses.
//!
//! Stores serialized JSON responses keyed by a string cache key (e.g.,
//! `"drive:folders:parent_id"`). Each entry has a TTL in seconds.
//! On read, expired entries return `None` and are lazily cleaned up.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::errors::ChalkError;

/// Default TTL for cache entries (5 minutes).
pub const DEFAULT_TTL_SECS: i64 = 300;

/// A cache entry stored in SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub key: String,
    pub value: String,
    pub created_at: String,
    pub expires_at: String,
    pub ttl_secs: i64,
}

/// The read-through cache backed by SQLite.
pub struct Cache {
    conn: std::sync::Mutex<Connection>,
}

impl Cache {
    /// Open or create a cache database at the given path.
    pub fn open(path: &std::path::Path) -> Result<Self, ChalkError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "wal")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cache_entries (
                key         TEXT PRIMARY KEY NOT NULL,
                value       TEXT NOT NULL,
                created_at  TEXT NOT NULL DEFAULT (datetime('now')),
                expires_at  TEXT NOT NULL,
                ttl_secs    INTEGER NOT NULL
            )",
        )?;

        Ok(Self {
            conn: std::sync::Mutex::new(conn),
        })
    }

    /// Open an in-memory cache (for tests).
    pub fn open_in_memory() -> Result<Self, ChalkError> {
        let conn = Connection::open_in_memory()?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cache_entries (
                key         TEXT PRIMARY KEY NOT NULL,
                value       TEXT NOT NULL,
                created_at  TEXT NOT NULL DEFAULT (datetime('now')),
                expires_at  TEXT NOT NULL,
                ttl_secs    INTEGER NOT NULL
            )",
        )?;

        Ok(Self {
            conn: std::sync::Mutex::new(conn),
        })
    }

    /// Get a cached value. Returns `None` if the key doesn't exist or has expired.
    pub fn get(&self, key: &str) -> Result<Option<String>, ChalkError> {
        let conn = self.lock_conn()?;
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let result = conn.query_row(
            "SELECT value, expires_at FROM cache_entries WHERE key = ?1",
            params![key],
            |row| {
                let value: String = row.get(0)?;
                let expires_at: String = row.get(1)?;
                Ok((value, expires_at))
            },
        );

        match result {
            Ok((value, expires_at)) => {
                if expires_at <= now {
                    // Expired — delete lazily.
                    conn.execute("DELETE FROM cache_entries WHERE key = ?1", params![key])?;
                    Ok(None)
                } else {
                    Ok(Some(value))
                }
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Set a cached value with a TTL in seconds.
    pub fn set(&self, key: &str, value: &str, ttl_secs: i64) -> Result<(), ChalkError> {
        let conn = self.lock_conn()?;
        let now = Utc::now();
        let created_at = now.format("%Y-%m-%d %H:%M:%S").to_string();
        let expires_at = (now + chrono::Duration::seconds(ttl_secs))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        conn.execute(
            "INSERT INTO cache_entries (key, value, created_at, expires_at, ttl_secs)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                created_at = excluded.created_at,
                expires_at = excluded.expires_at,
                ttl_secs = excluded.ttl_secs",
            params![key, value, created_at, expires_at, ttl_secs],
        )?;

        Ok(())
    }

    /// Set a cached value with the default TTL.
    pub fn set_default(&self, key: &str, value: &str) -> Result<(), ChalkError> {
        self.set(key, value, DEFAULT_TTL_SECS)
    }

    /// Delete a specific cache entry.
    pub fn delete(&self, key: &str) -> Result<bool, ChalkError> {
        let conn = self.lock_conn()?;
        let deleted = conn.execute("DELETE FROM cache_entries WHERE key = ?1", params![key])?;
        Ok(deleted > 0)
    }

    /// Delete all cache entries matching a prefix (e.g., `"drive:"` to clear all Drive cache).
    pub fn delete_by_prefix(&self, prefix: &str) -> Result<u64, ChalkError> {
        let conn = self.lock_conn()?;
        let pattern = format!("{prefix}%");
        let deleted =
            conn.execute("DELETE FROM cache_entries WHERE key LIKE ?1", params![pattern])?;
        Ok(deleted as u64)
    }

    /// Remove all expired entries.
    pub fn cleanup_expired(&self) -> Result<u64, ChalkError> {
        let conn = self.lock_conn()?;
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let deleted = conn.execute(
            "DELETE FROM cache_entries WHERE expires_at <= ?1",
            params![now],
        )?;
        Ok(deleted as u64)
    }

    /// Clear the entire cache.
    pub fn clear(&self) -> Result<(), ChalkError> {
        let conn = self.lock_conn()?;
        conn.execute("DELETE FROM cache_entries", [])?;
        Ok(())
    }

    /// List all non-expired cache keys (for debugging / admin).
    pub fn list_keys(&self) -> Result<Vec<String>, ChalkError> {
        let conn = self.lock_conn()?;
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let mut stmt = conn.prepare(
            "SELECT key FROM cache_entries WHERE expires_at > ?1 ORDER BY key",
        )?;
        let rows = stmt.query_map(params![now], |row| row.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Get a cached value, or call the provided function to compute it,
    /// storing the result in the cache with the given TTL.
    pub fn get_or_set<F>(
        &self,
        key: &str,
        ttl_secs: i64,
        compute: F,
    ) -> Result<String, ChalkError>
    where
        F: FnOnce() -> Result<String, ChalkError>,
    {
        if let Some(cached) = self.get(key)? {
            return Ok(cached);
        }
        let value = compute()?;
        self.set(key, &value, ttl_secs)?;
        Ok(value)
    }

    /// Return cache stats: total entries, expired entries.
    pub fn stats(&self) -> Result<CacheStats, ChalkError> {
        let conn = self.lock_conn()?;
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let total: i64 =
            conn.query_row("SELECT COUNT(*) FROM cache_entries", [], |row| row.get(0))?;
        let expired: i64 = conn.query_row(
            "SELECT COUNT(*) FROM cache_entries WHERE expires_at <= ?1",
            params![now],
            |row| row.get(0),
        )?;

        Ok(CacheStats {
            total_entries: total as u64,
            expired_entries: expired as u64,
            active_entries: (total - expired) as u64,
        })
    }

    fn lock_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>, ChalkError> {
        self.conn.lock().map_err(|_| {
            ChalkError::db_query("Cache mutex poisoned")
        })
    }
}

/// Cache statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_entries: u64,
    pub expired_entries: u64,
    pub active_entries: u64,
}

// ── Helper: parse a UTC datetime string ──────────────────────

pub fn parse_utc(s: &str) -> Option<DateTime<Utc>> {
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|ndt| ndt.and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cache() -> Cache {
        Cache::open_in_memory().unwrap()
    }

    #[test]
    fn test_set_and_get() {
        let cache = test_cache();
        cache.set("key1", "value1", 3600).unwrap();
        let val = cache.get("key1").unwrap();
        assert_eq!(val, Some("value1".to_string()));
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let cache = test_cache();
        let val = cache.get("nonexistent").unwrap();
        assert!(val.is_none());
    }

    #[test]
    fn test_expired_entry_returns_none() {
        let cache = test_cache();
        // Set with TTL of -1 second (already expired).
        cache.set("expired_key", "value", -1).unwrap();
        let val = cache.get("expired_key").unwrap();
        assert!(val.is_none());
    }

    #[test]
    fn test_upsert_overwrites() {
        let cache = test_cache();
        cache.set("key1", "old", 3600).unwrap();
        cache.set("key1", "new", 3600).unwrap();
        let val = cache.get("key1").unwrap();
        assert_eq!(val, Some("new".to_string()));
    }

    #[test]
    fn test_delete() {
        let cache = test_cache();
        cache.set("del_key", "value", 3600).unwrap();
        let deleted = cache.delete("del_key").unwrap();
        assert!(deleted);
        assert!(cache.get("del_key").unwrap().is_none());

        // Delete nonexistent returns false.
        let deleted = cache.delete("nope").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_delete_by_prefix() {
        let cache = test_cache();
        cache.set("drive:folders:abc", "v1", 3600).unwrap();
        cache.set("drive:folders:def", "v2", 3600).unwrap();
        cache.set("other:key", "v3", 3600).unwrap();

        let deleted = cache.delete_by_prefix("drive:").unwrap();
        assert_eq!(deleted, 2);

        assert!(cache.get("drive:folders:abc").unwrap().is_none());
        assert!(cache.get("drive:folders:def").unwrap().is_none());
        assert!(cache.get("other:key").unwrap().is_some());
    }

    #[test]
    fn test_cleanup_expired() {
        let cache = test_cache();
        cache.set("alive", "value", 3600).unwrap();
        cache.set("dead1", "value", -1).unwrap();
        cache.set("dead2", "value", -1).unwrap();

        let cleaned = cache.cleanup_expired().unwrap();
        assert_eq!(cleaned, 2);
        assert!(cache.get("alive").unwrap().is_some());
    }

    #[test]
    fn test_clear() {
        let cache = test_cache();
        cache.set("a", "1", 3600).unwrap();
        cache.set("b", "2", 3600).unwrap();
        cache.clear().unwrap();

        assert!(cache.get("a").unwrap().is_none());
        assert!(cache.get("b").unwrap().is_none());
    }

    #[test]
    fn test_list_keys() {
        let cache = test_cache();
        cache.set("key_a", "1", 3600).unwrap();
        cache.set("key_b", "2", 3600).unwrap();
        cache.set("expired", "3", -1).unwrap();

        let keys = cache.list_keys().unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"key_a".to_string()));
        assert!(keys.contains(&"key_b".to_string()));
    }

    #[test]
    fn test_get_or_set_cached() {
        let cache = test_cache();
        cache.set("computed", "cached_value", 3600).unwrap();

        let val = cache
            .get_or_set("computed", 3600, || Ok("new_value".to_string()))
            .unwrap();
        assert_eq!(val, "cached_value"); // Should return cached, not compute.
    }

    #[test]
    fn test_get_or_set_computes_on_miss() {
        let cache = test_cache();

        let val = cache
            .get_or_set("fresh", 3600, || Ok("computed_value".to_string()))
            .unwrap();
        assert_eq!(val, "computed_value");

        // Should now be cached.
        let val = cache.get("fresh").unwrap();
        assert_eq!(val, Some("computed_value".to_string()));
    }

    #[test]
    fn test_get_or_set_propagates_error() {
        let cache = test_cache();

        let result = cache.get_or_set("fail", 3600, || {
            Err(ChalkError::connector_api("API down"))
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_stats() {
        let cache = test_cache();
        cache.set("alive1", "v", 3600).unwrap();
        cache.set("alive2", "v", 3600).unwrap();
        cache.set("dead1", "v", -1).unwrap();

        let stats = cache.stats().unwrap();
        assert_eq!(stats.total_entries, 3);
        assert_eq!(stats.expired_entries, 1);
        assert_eq!(stats.active_entries, 2);
    }

    #[test]
    fn test_set_default_ttl() {
        let cache = test_cache();
        cache.set_default("default_key", "value").unwrap();
        assert!(cache.get("default_key").unwrap().is_some());
    }

    #[test]
    fn test_json_value_storage() {
        let cache = test_cache();
        let json_val = serde_json::json!({"folders": [{"id": "abc", "name": "Lesson Plans"}]});
        let json_str = serde_json::to_string(&json_val).unwrap();

        cache.set("drive:folders:root", &json_str, 3600).unwrap();

        let retrieved = cache.get("drive:folders:root").unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&retrieved).unwrap();
        assert_eq!(parsed["folders"][0]["id"], "abc");
    }

    #[test]
    fn test_parse_utc() {
        let dt = parse_utc("2026-01-15 10:30:00");
        assert!(dt.is_some());
        let dt = dt.unwrap();
        assert_eq!(dt.year(), 2026);

        let invalid = parse_utc("not a date");
        assert!(invalid.is_none());
    }

    use chrono::Datelike;
}
