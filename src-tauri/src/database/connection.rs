use rusqlite::Connection;
use sqlite_vec::sqlite3_vec_init;
use std::path::PathBuf;
use std::sync::Mutex;

use super::migrations;

#[derive(thiserror::Error, Debug)]
pub enum DatabaseError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Database not initialized")]
    NotInitialized,

    #[error("Record not found")]
    NotFound,
}

pub type Result<T> = std::result::Result<T, DatabaseError>;

/// Thread-safe database handle wrapping a SQLite connection in WAL mode.
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open (or create) the database at the given path with WAL mode and sqlite-vec loaded.
    pub fn open(path: &PathBuf) -> Result<Self> {
        // Register sqlite-vec as an auto-extension before opening the connection.
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite3_vec_init as *const (),
            )));
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let conn = Connection::open(path)?;

        // Enable WAL mode for concurrent reads and better write performance.
        conn.pragma_update(None, "journal_mode", "wal")?;
        // Reasonable busy timeout for multi-threaded access.
        conn.pragma_update(None, "busy_timeout", 5000)?;
        // Enable foreign key enforcement.
        conn.pragma_update(None, "foreign_keys", "ON")?;

        let db = Self {
            conn: Mutex::new(conn),
        };

        db.run_migrations()?;

        tracing::info!(path = %path.display(), "Database opened (WAL mode, sqlite-vec loaded)");

        Ok(db)
    }

    /// Open an in-memory database (for tests).
    pub fn open_in_memory() -> Result<Self> {
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite3_vec_init as *const (),
            )));
        }

        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        let db = Self {
            conn: Mutex::new(conn),
        };

        db.run_migrations()?;
        Ok(db)
    }

    /// Returns the default database path under the OS data directory.
    pub fn default_path() -> PathBuf {
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("com.madison.chalk").join("chalk.db")
    }

    /// Execute a closure with a reference to the underlying connection.
    pub fn with_conn<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        let conn = self.conn.lock().map_err(|_| {
            DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
                Some("Mutex poisoned".to_string()),
            ))
        })?;
        f(&conn)
    }

    fn run_migrations(&self) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| {
            DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
                Some("Mutex poisoned".to_string()),
            ))
        })?;
        migrations::run_all(&conn)?;
        Ok(())
    }
}
