//! Tauri Event Bus — typed event channels for cross-layer communication.
//!
//! Replaces polling with push-based events. The Rust backend emits events via
//! `app.emit(channel, payload)` and the React frontend subscribes via
//! `listen(channel, callback)`.
//!
//! # Channels
//! - `connector:status_changed` — connector auth/sync status changes
//! - `shredder:progress` — document scanning progress updates
//! - `shredder:complete` — scan finished
//! - `cache:invalidated` — a cache entry was evicted or expired
//! - `app:error` — structured domain errors for toast/banner display

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::errors::ChalkError;

// ── Channel constants ────────────────────────────────────────

pub const CHANNEL_CONNECTOR_STATUS: &str = "connector:status_changed";
pub const CHANNEL_SHREDDER_PROGRESS: &str = "shredder:progress";
pub const CHANNEL_SHREDDER_COMPLETE: &str = "shredder:complete";
pub const CHANNEL_CACHE_INVALIDATED: &str = "cache:invalidated";
pub const CHANNEL_APP_ERROR: &str = "app:error";
pub const CHANNEL_FEATURE_FLAG_CHANGED: &str = "feature_flag:changed";

// ── Event Payloads ───────────────────────────────────────────

/// Connector status change event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorStatusPayload {
    pub connector_id: String,
    pub connector_type: String,
    pub status: ConnectorStatus,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorStatus {
    Connected,
    Disconnected,
    Syncing,
    Error,
}

/// Shredder progress event — emitted during document scanning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShredderProgressPayload {
    pub current: u32,
    pub total: u32,
    pub current_document: Option<String>,
    pub tables_found: u32,
}

/// Shredder complete event — emitted when scanning finishes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShredderCompletePayload {
    pub documents_processed: u32,
    pub total_tables: u32,
    pub total_plans: u32,
    pub errors: Vec<String>,
}

/// Cache invalidation event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheInvalidatedPayload {
    pub cache_key: String,
    pub reason: CacheInvalidationReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheInvalidationReason {
    Expired,
    ManualClear,
    DataChanged,
}

/// Feature flag changed event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlagChangedPayload {
    pub flag_name: String,
    pub enabled: bool,
}

/// App error event — wraps ChalkError for frontend display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppErrorPayload {
    pub error: ChalkError,
    pub recoverable: bool,
    /// Suggested action for the frontend (e.g., "reconnect", "retry", "dismiss").
    pub action: Option<String>,
}

// ── Emit helpers ─────────────────────────────────────────────

/// Emit a connector status change event.
pub fn emit_connector_status(app: &AppHandle, payload: ConnectorStatusPayload) {
    if let Err(e) = app.emit(CHANNEL_CONNECTOR_STATUS, &payload) {
        tracing::warn!(error = %e, "Failed to emit connector status event");
    }
}

/// Emit a shredder progress event.
pub fn emit_shredder_progress(app: &AppHandle, payload: ShredderProgressPayload) {
    if let Err(e) = app.emit(CHANNEL_SHREDDER_PROGRESS, &payload) {
        tracing::warn!(error = %e, "Failed to emit shredder progress event");
    }
}

/// Emit a shredder complete event.
pub fn emit_shredder_complete(app: &AppHandle, payload: ShredderCompletePayload) {
    if let Err(e) = app.emit(CHANNEL_SHREDDER_COMPLETE, &payload) {
        tracing::warn!(error = %e, "Failed to emit shredder complete event");
    }
}

/// Emit a cache invalidated event.
pub fn emit_cache_invalidated(app: &AppHandle, payload: CacheInvalidatedPayload) {
    if let Err(e) = app.emit(CHANNEL_CACHE_INVALIDATED, &payload) {
        tracing::warn!(error = %e, "Failed to emit cache invalidated event");
    }
}

/// Emit a feature flag changed event.
pub fn emit_feature_flag_changed(app: &AppHandle, payload: FeatureFlagChangedPayload) {
    if let Err(e) = app.emit(CHANNEL_FEATURE_FLAG_CHANGED, &payload) {
        tracing::warn!(error = %e, "Failed to emit feature flag changed event");
    }
}

/// Emit an app error event for frontend toast/banner display.
pub fn emit_app_error(app: &AppHandle, error: ChalkError, recoverable: bool, action: Option<&str>) {
    let payload = AppErrorPayload {
        error,
        recoverable,
        action: action.map(String::from),
    };
    if let Err(e) = app.emit(CHANNEL_APP_ERROR, &payload) {
        tracing::warn!(error = %e, "Failed to emit app error event");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connector_status_payload_serializes() {
        let payload = ConnectorStatusPayload {
            connector_id: "gdrive-1".into(),
            connector_type: "google_drive".into(),
            status: ConnectorStatus::Connected,
            message: Some("Authenticated successfully".into()),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["status"], "connected");
        assert_eq!(json["connector_type"], "google_drive");
    }

    #[test]
    fn test_shredder_progress_payload_serializes() {
        let payload = ShredderProgressPayload {
            current: 3,
            total: 10,
            current_document: Some("Biology Unit.gdoc".into()),
            tables_found: 7,
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["current"], 3);
        assert_eq!(json["total"], 10);
        assert_eq!(json["tables_found"], 7);
    }

    #[test]
    fn test_shredder_complete_payload_serializes() {
        let payload = ShredderCompletePayload {
            documents_processed: 12,
            total_tables: 45,
            total_plans: 42,
            errors: vec!["Failed to parse doc X".into()],
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["documents_processed"], 12);
        assert_eq!(json["errors"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_cache_invalidated_payload_serializes() {
        let payload = CacheInvalidatedPayload {
            cache_key: "drive:folder:abc123".into(),
            reason: CacheInvalidationReason::Expired,
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["reason"], "expired");
    }

    #[test]
    fn test_feature_flag_changed_payload_serializes() {
        let payload = FeatureFlagChangedPayload {
            flag_name: "dark_mode".into(),
            enabled: true,
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["flag_name"], "dark_mode");
        assert_eq!(json["enabled"], true);
    }

    #[test]
    fn test_app_error_payload_serializes() {
        let error = ChalkError::oauth_token_expired();
        let payload = AppErrorPayload {
            error,
            recoverable: true,
            action: Some("reconnect".into()),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["recoverable"], true);
        assert_eq!(json["action"], "reconnect");
        assert_eq!(json["error"]["domain"], "oauth");
        assert_eq!(json["error"]["code"], "OAUTH_TOKEN_EXPIRED");
    }

    #[test]
    fn test_connector_status_variants() {
        let statuses = vec![
            (ConnectorStatus::Connected, "connected"),
            (ConnectorStatus::Disconnected, "disconnected"),
            (ConnectorStatus::Syncing, "syncing"),
            (ConnectorStatus::Error, "error"),
        ];
        for (status, expected) in statuses {
            let json = serde_json::to_value(status).unwrap();
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn test_cache_invalidation_reason_variants() {
        let reasons = vec![
            (CacheInvalidationReason::Expired, "expired"),
            (CacheInvalidationReason::ManualClear, "manual_clear"),
            (CacheInvalidationReason::DataChanged, "data_changed"),
        ];
        for (reason, expected) in reasons {
            let json = serde_json::to_value(reason).unwrap();
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn test_payload_deserialization_roundtrip() {
        let payload = ConnectorStatusPayload {
            connector_id: "test-1".into(),
            connector_type: "google_drive".into(),
            status: ConnectorStatus::Syncing,
            message: None,
        };
        let json_str = serde_json::to_string(&payload).unwrap();
        let deserialized: ConnectorStatusPayload = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.connector_id, "test-1");
        assert_eq!(deserialized.status, ConnectorStatus::Syncing);
        assert!(deserialized.message.is_none());
    }

    #[test]
    fn test_channel_constants() {
        assert_eq!(CHANNEL_CONNECTOR_STATUS, "connector:status_changed");
        assert_eq!(CHANNEL_SHREDDER_PROGRESS, "shredder:progress");
        assert_eq!(CHANNEL_SHREDDER_COMPLETE, "shredder:complete");
        assert_eq!(CHANNEL_CACHE_INVALIDATED, "cache:invalidated");
        assert_eq!(CHANNEL_APP_ERROR, "app:error");
        assert_eq!(CHANNEL_FEATURE_FLAG_CHANGED, "feature_flag:changed");
    }
}
