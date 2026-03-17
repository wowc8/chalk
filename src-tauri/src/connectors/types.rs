use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Authentication state for a connector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatus {
    Connected,
    Disconnected,
    Expired,
}

/// Summary info about a connector for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorInfo {
    pub id: String,
    pub connector_type: String,
    pub display_name: String,
    pub auth_status: AuthStatus,
}

/// Detailed connection information for Settings page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionDetails {
    pub id: String,
    pub connector_type: String,
    pub display_name: String,
    pub auth_status: AuthStatus,
    pub account_email: Option<String>,
    pub source_name: Option<String>,
    pub source_id: Option<String>,
    pub last_scan_at: Option<DateTime<Utc>>,
    pub document_count: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_status_serializes_to_snake_case() {
        let json = serde_json::to_string(&AuthStatus::Connected).unwrap();
        assert_eq!(json, "\"connected\"");

        let json = serde_json::to_string(&AuthStatus::Disconnected).unwrap();
        assert_eq!(json, "\"disconnected\"");

        let json = serde_json::to_string(&AuthStatus::Expired).unwrap();
        assert_eq!(json, "\"expired\"");
    }

    #[test]
    fn connector_info_roundtrip() {
        let info = ConnectorInfo {
            id: "gd-1".into(),
            connector_type: "google_drive".into(),
            display_name: "My Drive".into(),
            auth_status: AuthStatus::Connected,
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: ConnectorInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "gd-1");
        assert_eq!(back.auth_status, AuthStatus::Connected);
    }

    #[test]
    fn connection_details_with_optionals() {
        let details = ConnectionDetails {
            id: "gd-1".into(),
            connector_type: "google_drive".into(),
            display_name: "My Drive".into(),
            auth_status: AuthStatus::Connected,
            account_email: Some("teacher@school.edu".into()),
            source_name: Some("Lesson Plans 2024".into()),
            source_id: Some("folder-abc123".into()),
            last_scan_at: Some(Utc::now()),
            document_count: Some(42),
        };
        let json = serde_json::to_string(&details).unwrap();
        assert!(json.contains("teacher@school.edu"));
        assert!(json.contains("42"));
    }

    #[test]
    fn connection_details_without_optionals() {
        let details = ConnectionDetails {
            id: "gd-1".into(),
            connector_type: "google_drive".into(),
            display_name: "Google Drive".into(),
            auth_status: AuthStatus::Disconnected,
            account_email: None,
            source_name: None,
            source_id: None,
            last_scan_at: None,
            document_count: None,
        };
        let json = serde_json::to_string(&details).unwrap();
        let back: ConnectionDetails = serde_json::from_str(&json).unwrap();
        assert!(back.account_email.is_none());
        assert_eq!(back.auth_status, AuthStatus::Disconnected);
    }
}
