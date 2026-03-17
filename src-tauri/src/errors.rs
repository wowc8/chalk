//! Domain error types — structured error hierarchy for the entire Chalk application.
//!
//! Provides a unified error enum (`ChalkError`) that maps to structured JSON
//! responses for the frontend. Each variant carries context (domain, code, message)
//! so React can pattern-match on error types for user-facing messages.

use serde::{Deserialize, Serialize};

/// Error domain — which subsystem produced the error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorDomain {
    Database,
    Connector,
    #[serde(rename = "oauth")]
    OAuth,
    Shredder,
    Rag,
    Chat,
    Cache,
    FeatureFlag,
    Io,
    Unknown,
}

/// Machine-readable error code for frontend pattern matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    // Database
    DbConnectionFailed,
    DbQueryFailed,
    DbNotFound,
    DbMigrationFailed,

    // Connector / OAuth
    OauthNotConfigured,
    OauthTokenExpired,
    OauthTokenRefreshFailed,
    ConnectorNotFound,
    ConnectorApiError,

    // Shredder
    ShredderParseFailed,
    ShredderNoTables,

    // RAG / Chat
    RagEmbeddingFailed,
    RagSearchFailed,
    ChatApiKeyMissing,
    ChatCompletionFailed,

    // Cache
    CacheExpired,
    CacheMiss,

    // Feature flags
    FlagNotFound,

    // IO
    IoReadFailed,
    IoWriteFailed,

    // Catch-all
    InternalError,
}

/// The unified domain error type for Chalk.
///
/// Serializes to structured JSON with `domain`, `code`, `message`, and optional `details`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChalkError {
    pub domain: ErrorDomain,
    pub code: ErrorCode,
    pub message: String,
    /// Optional structured details (e.g., field names, IDs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl std::fmt::Display for ChalkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}::{:?}] {}", self.domain, self.code, self.message)
    }
}

impl std::error::Error for ChalkError {}

impl ChalkError {
    /// Create a new error with domain, code, and message.
    pub fn new(domain: ErrorDomain, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            domain,
            code,
            message: message.into(),
            details: None,
        }
    }

    /// Attach optional structured details.
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    // ── Convenience constructors ─────────────────────────────

    pub fn db_not_found(entity: &str, id: &str) -> Self {
        Self::new(
            ErrorDomain::Database,
            ErrorCode::DbNotFound,
            format!("{entity} not found: {id}"),
        )
    }

    pub fn db_query(msg: impl Into<String>) -> Self {
        Self::new(ErrorDomain::Database, ErrorCode::DbQueryFailed, msg)
    }

    pub fn oauth_not_configured(msg: impl Into<String>) -> Self {
        Self::new(ErrorDomain::OAuth, ErrorCode::OauthNotConfigured, msg)
    }

    pub fn oauth_token_expired() -> Self {
        Self::new(
            ErrorDomain::OAuth,
            ErrorCode::OauthTokenExpired,
            "OAuth token has expired — please reconnect",
        )
    }

    pub fn connector_api(msg: impl Into<String>) -> Self {
        Self::new(ErrorDomain::Connector, ErrorCode::ConnectorApiError, msg)
    }

    pub fn cache_miss(key: &str) -> Self {
        Self::new(
            ErrorDomain::Cache,
            ErrorCode::CacheMiss,
            format!("Cache miss for key: {key}"),
        )
    }

    pub fn cache_expired(key: &str) -> Self {
        Self::new(
            ErrorDomain::Cache,
            ErrorCode::CacheExpired,
            format!("Cache entry expired for key: {key}"),
        )
    }

    pub fn flag_not_found(flag: &str) -> Self {
        Self::new(
            ErrorDomain::FeatureFlag,
            ErrorCode::FlagNotFound,
            format!("Feature flag not found: {flag}"),
        )
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(ErrorDomain::Unknown, ErrorCode::InternalError, msg)
    }

    pub fn io_read(msg: impl Into<String>) -> Self {
        Self::new(ErrorDomain::Io, ErrorCode::IoReadFailed, msg)
    }

    pub fn io_write(msg: impl Into<String>) -> Self {
        Self::new(ErrorDomain::Io, ErrorCode::IoWriteFailed, msg)
    }
}

// ── Conversions from existing error types ────────────────────

impl From<rusqlite::Error> for ChalkError {
    fn from(e: rusqlite::Error) -> Self {
        match &e {
            rusqlite::Error::QueryReturnedNoRows => {
                Self::new(ErrorDomain::Database, ErrorCode::DbNotFound, e.to_string())
            }
            _ => Self::new(ErrorDomain::Database, ErrorCode::DbQueryFailed, e.to_string()),
        }
    }
}

impl From<std::io::Error> for ChalkError {
    fn from(e: std::io::Error) -> Self {
        Self::new(ErrorDomain::Io, ErrorCode::IoReadFailed, e.to_string())
    }
}

impl From<serde_json::Error> for ChalkError {
    fn from(e: serde_json::Error) -> Self {
        Self::new(ErrorDomain::Unknown, ErrorCode::InternalError, e.to_string())
    }
}

impl From<reqwest::Error> for ChalkError {
    fn from(e: reqwest::Error) -> Self {
        Self::new(
            ErrorDomain::Connector,
            ErrorCode::ConnectorApiError,
            e.to_string(),
        )
    }
}

/// Convert from the existing `DatabaseError` type for backwards compatibility.
impl From<crate::database::DatabaseError> for ChalkError {
    fn from(e: crate::database::DatabaseError) -> Self {
        match e {
            crate::database::DatabaseError::Sqlite(se) => se.into(),
            crate::database::DatabaseError::NotInitialized => Self::new(
                ErrorDomain::Database,
                ErrorCode::DbConnectionFailed,
                "Database not initialized",
            ),
            crate::database::DatabaseError::NotFound => Self::new(
                ErrorDomain::Database,
                ErrorCode::DbNotFound,
                "Record not found",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_serializes_to_structured_json() {
        let err = ChalkError::db_not_found("Subject", "abc-123");
        let json = serde_json::to_value(&err).unwrap();

        assert_eq!(json["domain"], "database");
        assert_eq!(json["code"], "DB_NOT_FOUND");
        assert_eq!(json["message"], "Subject not found: abc-123");
        assert!(json.get("details").is_none());
    }

    #[test]
    fn test_error_with_details() {
        let err = ChalkError::db_not_found("LessonPlan", "xyz")
            .with_details(serde_json::json!({"table": "lesson_plans"}));

        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["details"]["table"], "lesson_plans");
    }

    #[test]
    fn test_error_display() {
        let err = ChalkError::oauth_token_expired();
        let display = format!("{err}");
        assert!(display.contains("OAuth"));
        assert!(display.contains("expired"));
    }

    #[test]
    fn test_convenience_constructors() {
        let err = ChalkError::cache_miss("drive:folder:123");
        assert_eq!(err.domain, ErrorDomain::Cache);
        assert_eq!(err.code, ErrorCode::CacheMiss);

        let err = ChalkError::flag_not_found("dark_mode");
        assert_eq!(err.domain, ErrorDomain::FeatureFlag);
        assert_eq!(err.code, ErrorCode::FlagNotFound);

        let err = ChalkError::internal("unexpected");
        assert_eq!(err.domain, ErrorDomain::Unknown);
        assert_eq!(err.code, ErrorCode::InternalError);
    }

    #[test]
    fn test_from_rusqlite_error() {
        let e = rusqlite::Error::QueryReturnedNoRows;
        let chalk_err: ChalkError = e.into();
        assert_eq!(chalk_err.code, ErrorCode::DbNotFound);
    }

    #[test]
    fn test_from_io_error() {
        let e = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let chalk_err: ChalkError = e.into();
        assert_eq!(chalk_err.domain, ErrorDomain::Io);
    }

    #[test]
    fn test_error_domains_serialize_snake_case() {
        let domains = vec![
            (ErrorDomain::Database, "database"),
            (ErrorDomain::Connector, "connector"),
            (ErrorDomain::OAuth, "oauth"),
            (ErrorDomain::Cache, "cache"),
            (ErrorDomain::FeatureFlag, "feature_flag"),
        ];
        for (domain, expected) in domains {
            let json = serde_json::to_value(domain).unwrap();
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn test_error_codes_serialize_screaming_snake() {
        let codes = vec![
            (ErrorCode::DbNotFound, "DB_NOT_FOUND"),
            (ErrorCode::OauthTokenExpired, "OAUTH_TOKEN_EXPIRED"),
            (ErrorCode::CacheMiss, "CACHE_MISS"),
            (ErrorCode::FlagNotFound, "FLAG_NOT_FOUND"),
        ];
        for (code, expected) in codes {
            let json = serde_json::to_value(code).unwrap();
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn test_deserialize_roundtrip() {
        let err = ChalkError::connector_api("Google API returned 403")
            .with_details(serde_json::json!({"status": 403}));

        let json_str = serde_json::to_string(&err).unwrap();
        let deserialized: ChalkError = serde_json::from_str(&json_str).unwrap();

        assert_eq!(deserialized.domain, ErrorDomain::Connector);
        assert_eq!(deserialized.code, ErrorCode::ConnectorApiError);
        assert_eq!(deserialized.message, "Google API returned 403");
        assert_eq!(deserialized.details.unwrap()["status"], 403);
    }
}
