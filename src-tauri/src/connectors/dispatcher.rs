use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use tauri::State;

use crate::AppState;

use super::types::{AuthStatus, ConnectionDetails, ConnectorInfo};

/// Cached token entry with expiry tracking.
#[derive(Debug, Clone)]
struct CachedToken {
    access_token: String,
    expires_at: DateTime<Utc>,
}

impl CachedToken {
    fn is_valid(&self) -> bool {
        // Consider expired 30 seconds early to avoid edge cases.
        Utc::now() < self.expires_at - chrono::Duration::seconds(30)
    }
}

/// ConnectorDispatcher: singleton managed by Tauri.
///
/// Manages all active connector instances and routes calls.
/// Phase 1 Polish: wraps the existing OAuthClient/Google Drive integration
/// and adds a token cache layer.
pub struct ConnectorDispatcher {
    /// Cache of access tokens keyed by connector ID.
    token_cache: Mutex<HashMap<String, CachedToken>>,
}

impl ConnectorDispatcher {
    pub fn new() -> Self {
        Self {
            token_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Cache an access token for a connector.
    pub fn cache_token(&self, connector_id: &str, access_token: String, expires_at: DateTime<Utc>) {
        if let Ok(mut cache) = self.token_cache.lock() {
            cache.insert(
                connector_id.to_string(),
                CachedToken {
                    access_token,
                    expires_at,
                },
            );
        }
    }

    /// Get a cached token if still valid.
    pub fn get_cached_token(&self, connector_id: &str) -> Option<String> {
        let cache = self.token_cache.lock().ok()?;
        let entry = cache.get(connector_id)?;
        if entry.is_valid() {
            Some(entry.access_token.clone())
        } else {
            None
        }
    }

    /// Invalidate a cached token (e.g., on disconnect).
    pub fn invalidate_token(&self, connector_id: &str) {
        if let Ok(mut cache) = self.token_cache.lock() {
            cache.remove(connector_id);
        }
    }

    /// Clear all cached tokens.
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.token_cache.lock() {
            cache.clear();
        }
    }
}

impl Default for ConnectorDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tauri Commands ─────────────────────────────────────────────────
//
// Thin wrappers that go through the dispatcher. These delegate to the
// existing OAuthClient for Google Drive operations while providing the
// dispatcher routing pattern for future connector types.

/// List all registered connectors with their current status.
#[tauri::command]
pub fn list_connectors(state: State<'_, AppState>) -> Result<Vec<ConnectorInfo>, String> {
    let client = state.oauth_client.lock().map_err(|e| e.to_string())?;
    let status = client.load_onboarding_status();

    let auth_status = if status.tokens_stored {
        AuthStatus::Connected
    } else {
        AuthStatus::Disconnected
    };

    Ok(vec![ConnectorInfo {
        id: "google_drive_default".into(),
        connector_type: "google_drive".into(),
        display_name: "Google Drive".into(),
        auth_status,
    }])
}

/// Get detailed connection info for a specific connector (Settings page).
#[tauri::command]
pub fn get_connection_details(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectionDetails>, String> {
    let client = state.oauth_client.lock().map_err(|e| e.to_string())?;
    let status = client.load_onboarding_status();

    let auth_status = if status.tokens_stored {
        AuthStatus::Connected
    } else {
        AuthStatus::Disconnected
    };

    Ok(vec![ConnectionDetails {
        id: "google_drive_default".into(),
        connector_type: "google_drive".into(),
        display_name: "Google Drive".into(),
        auth_status,
        account_email: None, // TODO: fetch from Google userinfo API
        source_name: status.selected_folder_name,
        source_id: status.selected_folder_id,
        last_scan_at: None, // TODO: persist scan timestamp
        document_count: None,
    }])
}

/// Disconnect a connector: clear tokens, reset onboarding status, invalidate cache.
#[tauri::command]
pub fn disconnect_connector(
    connector_id: String,
    state: State<'_, AppState>,
    dispatcher: State<'_, ConnectorDispatcher>,
) -> Result<(), String> {
    if connector_id != "google_drive_default" {
        return Err(format!("Unknown connector: {}", connector_id));
    }

    let client = state.oauth_client.lock().map_err(|e| e.to_string())?;

    // Reset onboarding status to disconnected state.
    let mut status = client.load_onboarding_status();
    status.tokens_stored = false;
    status.folder_selected = false;
    status.folder_accessible = false;
    status.initial_shred_complete = false;
    status.selected_folder_id = None;
    status.selected_folder_name = None;
    client
        .save_onboarding_status(&status)
        .map_err(|e| e.to_string())?;

    // Delete token file.
    client.delete_tokens().map_err(|e| e.to_string())?;

    // Invalidate cached token.
    dispatcher.invalidate_token(&connector_id);

    tracing::info!(connector_id = %connector_id, "Connector disconnected");
    Ok(())
}

/// Trigger a re-scan of documents for a connector.
#[tauri::command]
pub async fn rescan_connector(
    connector_id: String,
    state: State<'_, AppState>,
) -> Result<u32, String> {
    if connector_id != "google_drive_default" {
        return Err(format!("Unknown connector: {}", connector_id));
    }

    // Delegate to existing trigger_initial_shred logic.
    // Extract needed params outside the lock.
    let (config, token_file, folder_id) = {
        let client = state.oauth_client.lock().map_err(|e| e.to_string())?;
        let status = client.load_onboarding_status();
        let folder_id = status
            .selected_folder_id
            .ok_or("No folder selected for rescan")?;
        let (config, token_file, _) = client.exchange_params();
        (config, token_file, folder_id)
    };

    // Get a valid access token.
    let access_token =
        crate::admin::oauth::get_valid_access_token(&config, &token_file).await
            .map_err(|e| e.to_string())?;

    // List documents in the folder.
    let docs = crate::admin::oauth::list_drive_children_api(&access_token, &folder_id)
        .await
        .map_err(|e| e.to_string())?;

    let count = docs.len() as u32;

    // Update status to reflect re-scan.
    {
        let client = state.oauth_client.lock().map_err(|e| e.to_string())?;
        let mut status = client.load_onboarding_status();
        status.initial_shred_complete = true;
        client
            .save_onboarding_status(&status)
            .map_err(|e| e.to_string())?;
    }

    tracing::info!(connector_id = %connector_id, doc_count = count, "Connector rescanned");
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatcher_new_creates_empty_cache() {
        let d = ConnectorDispatcher::new();
        assert!(d.get_cached_token("any").is_none());
    }

    #[test]
    fn cache_and_retrieve_valid_token() {
        let d = ConnectorDispatcher::new();
        let future = Utc::now() + chrono::Duration::hours(1);
        d.cache_token("gd", "tok123".into(), future);
        assert_eq!(d.get_cached_token("gd"), Some("tok123".into()));
    }

    #[test]
    fn expired_token_not_returned() {
        let d = ConnectorDispatcher::new();
        let past = Utc::now() - chrono::Duration::hours(1);
        d.cache_token("gd", "old_tok".into(), past);
        assert!(d.get_cached_token("gd").is_none());
    }

    #[test]
    fn nearly_expired_token_not_returned() {
        let d = ConnectorDispatcher::new();
        // Expires in 10 seconds — within the 30-second buffer.
        let nearly = Utc::now() + chrono::Duration::seconds(10);
        d.cache_token("gd", "nearly_tok".into(), nearly);
        assert!(d.get_cached_token("gd").is_none());
    }

    #[test]
    fn invalidate_token_removes_entry() {
        let d = ConnectorDispatcher::new();
        let future = Utc::now() + chrono::Duration::hours(1);
        d.cache_token("gd", "tok".into(), future);
        d.invalidate_token("gd");
        assert!(d.get_cached_token("gd").is_none());
    }

    #[test]
    fn invalidate_nonexistent_is_noop() {
        let d = ConnectorDispatcher::new();
        d.invalidate_token("nonexistent"); // should not panic
    }

    #[test]
    fn clear_cache_removes_all() {
        let d = ConnectorDispatcher::new();
        let future = Utc::now() + chrono::Duration::hours(1);
        d.cache_token("gd1", "t1".into(), future);
        d.cache_token("gd2", "t2".into(), future);
        d.clear_cache();
        assert!(d.get_cached_token("gd1").is_none());
        assert!(d.get_cached_token("gd2").is_none());
    }

    #[test]
    fn multiple_connectors_cached_independently() {
        let d = ConnectorDispatcher::new();
        let future = Utc::now() + chrono::Duration::hours(1);
        d.cache_token("gd", "tok_gd".into(), future);
        d.cache_token("od", "tok_od".into(), future);
        assert_eq!(d.get_cached_token("gd"), Some("tok_gd".into()));
        assert_eq!(d.get_cached_token("od"), Some("tok_od".into()));
    }

    #[test]
    fn overwrite_token_updates_value() {
        let d = ConnectorDispatcher::new();
        let future = Utc::now() + chrono::Duration::hours(1);
        d.cache_token("gd", "old".into(), future);
        d.cache_token("gd", "new".into(), future);
        assert_eq!(d.get_cached_token("gd"), Some("new".into()));
    }

    #[test]
    fn default_impl_works() {
        let d = ConnectorDispatcher::default();
        assert!(d.get_cached_token("any").is_none());
    }
}
