// Connectors module — Trait + Factory + Dispatcher pattern for data sources.
// See LPA Master Spec section 3.3 for architecture details.

pub mod commands;
pub mod dispatcher;
pub mod factory;
pub mod google_drive;

use serde::{Deserialize, Serialize};

/// Errors that connectors can return.
#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Auth error: {0}")]
    Auth(String),

    #[error("Not connected: {0}")]
    NotConnected(String),

    #[error("Connector error: {0}")]
    Other(String),
}

/// Authentication status of a connector.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatus {
    Connected,
    Disconnected,
    Expired,
}

/// Identity and display info for a connector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorInfo {
    pub id: String,
    pub connector_type: String,
    pub display_name: String,
    pub icon: String,
    pub description: String,
}

/// A browsable source (folder or document) from a connector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub source_type: SourceType,
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Folder,
    Document,
}

/// A fetched document from any connector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub name: String,
    pub content: String,
    pub source_connector: String,
    pub modified_at: Option<String>,
}

/// Freshness check result for a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreshnessStatus {
    pub id: String,
    pub is_stale: bool,
    pub remote_modified_at: Option<String>,
}

/// Freshness report (from dispatcher's check_all_freshness).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreshnessReport {
    pub connector_id: String,
    pub statuses: Vec<FreshnessStatus>,
}

/// Persisted configuration for a connector instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorConfig {
    pub id: String,
    pub connector_type: String,
    pub display_name: String,
    pub credentials: Option<String>,
    pub source_id: Option<String>,
    pub created_at: String,
    pub last_sync_at: Option<String>,
}

/// The core trait that every data source must implement.
///
/// All methods are async-compatible via returning Results.
/// The trait uses `Send + Sync` so connectors can be stored in shared state.
pub trait LessonPlanConnector: Send + Sync {
    // Identity
    fn info(&self) -> ConnectorInfo;
    fn auth_status(&self) -> AuthStatus;

    // Authentication
    fn authenticate(&self) -> Result<AuthStatus, ConnectorError>;
    fn disconnect(&self) -> Result<(), ConnectorError>;

    // Data access
    fn list_sources(
        &self,
        parent_id: Option<&str>,
    ) -> Result<Vec<Source>, ConnectorError>;
    fn fetch_document(&self, id: &str) -> Result<Document, ConnectorError>;
    fn check_freshness(&self, id: &str) -> Result<FreshnessStatus, ConnectorError>;

    // Writing (Phase 3, optional — not all connectors support writing)
    fn supports_write(&self) -> bool {
        false
    }
}
