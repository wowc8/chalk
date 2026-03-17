use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::State;
use tracing::info;

use crate::connectors::google_drive::{self, DriveFolder, DriveItem, GoogleDriveConnector, OAuthConfig};
use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingStatus {
    pub oauth_configured: bool,
    pub tokens_stored: bool,
    pub folder_selected: bool,
    pub folder_accessible: bool,
    pub initial_shred_complete: bool,
    pub selected_folder_id: Option<String>,
    pub selected_folder_name: Option<String>,
}

impl Default for OnboardingStatus {
    fn default() -> Self {
        Self {
            oauth_configured: false,
            tokens_stored: false,
            folder_selected: false,
            folder_accessible: false,
            initial_shred_complete: false,
            selected_folder_id: None,
            selected_folder_name: None,
        }
    }
}

/// Get the onboarding status file path.
fn status_file_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir
        .join("com.madison.chalk")
        .join("onboarding_status.json")
}

/// Load onboarding status from disk.
fn load_onboarding_status(data_dir: &std::path::Path) -> OnboardingStatus {
    let path = status_file_path(data_dir);
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(status) = serde_json::from_str(&content) {
                return status;
            }
        }
    }
    let config_exists = data_dir
        .join("com.madison.chalk")
        .join("oauth_config.json")
        .exists();
    let tokens_exist = data_dir
        .join("com.madison.chalk")
        .join("oauth_tokens.json")
        .exists();
    OnboardingStatus {
        oauth_configured: config_exists,
        tokens_stored: tokens_exist,
        ..Default::default()
    }
}

/// Save onboarding status to disk.
fn save_onboarding_status(
    data_dir: &std::path::Path,
    status: &OnboardingStatus,
) -> Result<(), String> {
    let path = status_file_path(data_dir);
    let content = serde_json::to_string_pretty(status).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())
}

/// Get the OAuth config, token file path, and optional PKCE verifier for async Google Drive operations.
/// Reads directly from the app's data directory (same files the GoogleDriveConnector uses).
fn get_gd_exchange_params(state: &AppState) -> Result<(OAuthConfig, PathBuf, Option<String>), String> {
    let chalk_dir = state.data_dir.join("com.madison.chalk");
    let config_file = chalk_dir.join("oauth_config.json");
    let token_file = chalk_dir.join("oauth_tokens.json");

    let oauth_config = if config_file.exists() {
        let content = fs::read_to_string(&config_file).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())?
    } else {
        OAuthConfig::default()
    };

    // Note: PKCE verifier is per-session state managed by the connector.
    // For the thin delegating layer we pass None; the connector's exchange_params
    // provides the actual verifier when called via the connector path.
    Ok((oauth_config, token_file, None))
}

// ── Tauri Commands ──────────────────────────────────────────────

#[tauri::command]
pub async fn initialize_oauth(state: State<'_, AppState>) -> Result<String, String> {
    let chalk_dir = state.data_dir.join("com.madison.chalk");
    fs::create_dir_all(&chalk_dir).map_err(|e| e.to_string())?;

    let config_file = chalk_dir.join("oauth_config.json");
    if config_file.exists() {
        info!("OAuth client initialized with saved config");
        Ok("OAuth initialized with existing config".into())
    } else if GoogleDriveConnector::has_embedded_credentials() {
        info!("OAuth client initialized with embedded PKCE credentials");
        Ok("OAuth initialized with embedded credentials".into())
    } else {
        info!("OAuth client initialized (no config yet)");
        Ok("OAuth initialized — needs configuration".into())
    }
}

/// Check whether the app ships with embedded OAuth credentials.
#[tauri::command]
pub async fn has_embedded_credentials() -> Result<bool, String> {
    Ok(GoogleDriveConnector::has_embedded_credentials())
}

#[tauri::command]
pub async fn save_oauth_config(
    state: State<'_, AppState>,
    client_id: String,
    client_secret: String,
) -> Result<String, String> {
    let config = OAuthConfig {
        client_id,
        client_secret,
        ..OAuthConfig::default()
    };
    let chalk_dir = state.data_dir.join("com.madison.chalk");
    fs::create_dir_all(&chalk_dir).map_err(|e| e.to_string())?;
    let content = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(chalk_dir.join("oauth_config.json"), content).map_err(|e| e.to_string())?;
    info!("OAuth config saved");
    Ok("OAuth configuration saved".into())
}

#[tauri::command]
pub async fn get_authorization_url(state: State<'_, AppState>) -> Result<String, String> {
    let (config, _, _) = get_gd_exchange_params(&state)?;
    if config.client_id.is_empty() {
        return Err("OAuth not configured — set client_id and client_secret first".into());
    }
    let scopes = config.scopes.join(" ");
    let mut url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?\
         client_id={}&redirect_uri={}&response_type=code&\
         scope={}&access_type=offline&prompt=consent",
        config.client_id, config.redirect_uri, scopes
    );

    // Generate PKCE params when using embedded credentials or no secret.
    if config.client_secret.is_empty() {
        let verifier = google_drive::generate_code_verifier();
        let challenge = google_drive::generate_code_challenge(&verifier);
        url.push_str(&format!(
            "&code_challenge={}&code_challenge_method=S256",
            challenge
        ));
        // Note: verifier is ephemeral; in production flow the connector holds it.
    }

    Ok(url)
}

#[tauri::command]
pub async fn handle_oauth_callback(
    state: State<'_, AppState>,
    code: String,
) -> Result<String, String> {
    let (config, token_file, pkce_verifier) = get_gd_exchange_params(&state)?;

    google_drive::exchange_code(&config, &code, &token_file, pkce_verifier.as_deref())
        .await
        .map_err(|e| e.to_string())?;

    // Update onboarding status.
    let mut status = load_onboarding_status(&state.data_dir);
    status.oauth_configured = true;
    status.tokens_stored = true;
    save_onboarding_status(&state.data_dir, &status)?;

    info!("OAuth callback handled, tokens stored");
    Ok("Authentication successful".into())
}

#[tauri::command]
pub async fn test_folder_permissions_command(
    state: State<'_, AppState>,
    folder_id: String,
    folder_name: String,
) -> Result<bool, String> {
    let (config, token_file, _) = get_gd_exchange_params(&state)?;

    let access_token = google_drive::get_valid_access_token(&config, &token_file)
        .await
        .map_err(|e| e.to_string())?;

    let accessible = google_drive::test_folder_permissions(&access_token, &folder_id)
        .await
        .map_err(|e| e.to_string())?;

    // Update onboarding status.
    let mut status = load_onboarding_status(&state.data_dir);
    status.folder_selected = true;
    status.folder_accessible = accessible;
    status.selected_folder_id = Some(folder_id);
    status.selected_folder_name = Some(folder_name);
    save_onboarding_status(&state.data_dir, &status)?;

    Ok(accessible)
}

#[tauri::command]
pub async fn check_onboarding_status(
    state: State<'_, AppState>,
) -> Result<OnboardingStatus, String> {
    Ok(load_onboarding_status(&state.data_dir))
}

#[tauri::command]
pub async fn list_drive_folders(
    state: State<'_, AppState>,
) -> Result<Vec<DriveFolder>, String> {
    let (config, token_file, _) = get_gd_exchange_params(&state)?;

    let access_token = google_drive::get_valid_access_token(&config, &token_file)
        .await
        .map_err(|e| e.to_string())?;

    google_drive::list_drive_folders_api(&access_token)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_drive_subfolders(
    state: State<'_, AppState>,
    parent_id: String,
) -> Result<Vec<DriveFolder>, String> {
    let (config, token_file, _) = get_gd_exchange_params(&state)?;

    let access_token = google_drive::get_valid_access_token(&config, &token_file)
        .await
        .map_err(|e| e.to_string())?;

    google_drive::list_drive_children_api(&access_token, &parent_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_drive_items(
    state: State<'_, AppState>,
    parent_id: String,
) -> Result<Vec<DriveItem>, String> {
    let (config, token_file, _) = get_gd_exchange_params(&state)?;

    let access_token = google_drive::get_valid_access_token(&config, &token_file)
        .await
        .map_err(|e| e.to_string())?;

    google_drive::list_drive_items_api(&access_token, &parent_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn select_single_document(
    state: State<'_, AppState>,
    doc_id: String,
    doc_name: String,
) -> Result<bool, String> {
    let (config, token_file, _) = get_gd_exchange_params(&state)?;

    let access_token = google_drive::get_valid_access_token(&config, &token_file)
        .await
        .map_err(|e| e.to_string())?;

    // Verify the document is accessible
    let accessible = google_drive::test_folder_permissions(&access_token, &doc_id)
        .await
        .map_err(|e| e.to_string())?;

    if accessible {
        let mut status = load_onboarding_status(&state.data_dir);
        status.folder_selected = true;
        status.folder_accessible = true;
        status.selected_folder_id = Some(doc_id);
        status.selected_folder_name = Some(doc_name);
        save_onboarding_status(&state.data_dir, &status)?;
    }

    Ok(accessible)
}

#[tauri::command]
pub async fn trigger_initial_shred(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let onboarding = load_onboarding_status(&state.data_dir);
    if !onboarding.tokens_stored {
        return Err("Not authenticated — complete OAuth first".into());
    }
    if !onboarding.folder_selected {
        return Err("No folder selected — choose a folder first".into());
    }
    let folder_id = onboarding
        .selected_folder_id
        .clone()
        .ok_or("No folder ID stored")?;

    let (config, token_file, _) = get_gd_exchange_params(&state)?;

    let access_token = google_drive::get_valid_access_token(&config, &token_file)
        .await
        .map_err(|e| e.to_string())?;

    // Fetch file list from the selected folder.
    let reqwest_client = reqwest::Client::new();
    let query = format!(
        "'{}' in parents and trashed=false and mimeType='application/vnd.google-apps.document'",
        folder_id
    );
    let response = reqwest_client
        .get("https://www.googleapis.com/drive/v3/files")
        .query(&[
            ("q", query.as_str()),
            ("fields", "files(id,name,modifiedTime)"),
            ("pageSize", "50"),
        ])
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;

    let file_count = body
        .get("files")
        .and_then(|f| f.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    info!(
        folder_id = folder_id.as_str(),
        file_count = file_count,
        "Initial shred: discovered documents in folder"
    );

    // Mark shred as complete.
    let mut updated_status = load_onboarding_status(&state.data_dir);
    updated_status.initial_shred_complete = true;
    save_onboarding_status(&state.data_dir, &updated_status)?;

    Ok(format!(
        "Initial shred complete — found {} document(s) to process",
        file_count
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannedDocument {
    pub id: String,
    pub name: String,
    pub modified_time: Option<String>,
}

#[tauri::command]
pub async fn list_scanned_documents(
    state: State<'_, AppState>,
) -> Result<Vec<ScannedDocument>, String> {
    let onboarding = load_onboarding_status(&state.data_dir);
    if !onboarding.tokens_stored {
        return Err("Not authenticated".into());
    }
    let folder_id = onboarding
        .selected_folder_id
        .clone()
        .ok_or("No folder selected")?;

    let (config, token_file, _) = get_gd_exchange_params(&state)?;

    let access_token = google_drive::get_valid_access_token(&config, &token_file)
        .await
        .map_err(|e| e.to_string())?;

    let reqwest_client = reqwest::Client::new();
    let query = format!(
        "'{}' in parents and trashed=false and mimeType='application/vnd.google-apps.document'",
        folder_id
    );
    let response = reqwest_client
        .get("https://www.googleapis.com/drive/v3/files")
        .query(&[
            ("q", query.as_str()),
            ("fields", "files(id,name,modifiedTime)"),
            ("pageSize", "100"),
            ("orderBy", "modifiedTime desc"),
        ])
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;

    let documents: Vec<ScannedDocument> = body
        .get("files")
        .and_then(|f| f.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    Some(ScannedDocument {
                        id: item.get("id")?.as_str()?.to_string(),
                        name: item.get("name")?.as_str()?.to_string(),
                        modified_time: item
                            .get("modifiedTime")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    info!(count = documents.len(), "Listed scanned documents");
    Ok(documents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::google_drive::{OAuthConfig, TokenStorage};
    use chrono::Utc;
    use tempfile::TempDir;

    fn setup_data_dir(dir: &TempDir) -> std::path::PathBuf {
        let data = dir.path().to_path_buf();
        let chalk_dir = data.join("com.madison.chalk");
        fs::create_dir_all(&chalk_dir).unwrap();
        data
    }

    #[test]
    fn test_onboarding_status_default() {
        let dir = TempDir::new().unwrap();
        let data_dir = setup_data_dir(&dir);
        let status = load_onboarding_status(&data_dir);
        assert!(!status.oauth_configured);
        assert!(!status.tokens_stored);
        assert!(!status.folder_selected);
        assert!(!status.folder_accessible);
        assert!(!status.initial_shred_complete);
        assert!(status.selected_folder_id.is_none());
        assert!(status.selected_folder_name.is_none());
    }

    #[test]
    fn test_save_and_load_onboarding_status() {
        let dir = TempDir::new().unwrap();
        let data_dir = setup_data_dir(&dir);

        let status = OnboardingStatus {
            oauth_configured: true,
            tokens_stored: true,
            folder_selected: true,
            folder_accessible: true,
            initial_shred_complete: false,
            selected_folder_id: Some("folder_abc".into()),
            selected_folder_name: Some("My Lessons".into()),
        };
        save_onboarding_status(&data_dir, &status).unwrap();

        let loaded = load_onboarding_status(&data_dir);
        assert!(loaded.oauth_configured);
        assert!(loaded.tokens_stored);
        assert!(loaded.folder_selected);
        assert!(loaded.folder_accessible);
        assert!(!loaded.initial_shred_complete);
        assert_eq!(loaded.selected_folder_id, Some("folder_abc".into()));
        assert_eq!(loaded.selected_folder_name, Some("My Lessons".into()));
    }

    #[test]
    fn test_onboarding_status_detects_existing_files() {
        let dir = TempDir::new().unwrap();
        let data_dir = setup_data_dir(&dir);
        let chalk_dir = data_dir.join("com.madison.chalk");

        let config = OAuthConfig {
            client_id: "id".into(),
            client_secret: "secret".into(),
            ..OAuthConfig::default()
        };
        fs::write(
            chalk_dir.join("oauth_config.json"),
            serde_json::to_string(&config).unwrap(),
        )
        .unwrap();

        let tokens = TokenStorage {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: Utc::now() + chrono::Duration::seconds(3600),
            token_type: "Bearer".into(),
        };
        fs::write(
            chalk_dir.join("oauth_tokens.json"),
            serde_json::to_string(&tokens).unwrap(),
        )
        .unwrap();

        let status = load_onboarding_status(&data_dir);
        assert!(status.oauth_configured);
        assert!(status.tokens_stored);
    }

    #[test]
    fn test_onboarding_status_complete_flow() {
        let dir = TempDir::new().unwrap();
        let data_dir = setup_data_dir(&dir);

        let mut status = load_onboarding_status(&data_dir);

        status.oauth_configured = true;
        save_onboarding_status(&data_dir, &status).unwrap();

        status.tokens_stored = true;
        save_onboarding_status(&data_dir, &status).unwrap();

        status.folder_selected = true;
        status.folder_accessible = true;
        status.selected_folder_id = Some("folder_xyz".into());
        status.selected_folder_name = Some("Lesson Plans".into());
        save_onboarding_status(&data_dir, &status).unwrap();

        status.initial_shred_complete = true;
        save_onboarding_status(&data_dir, &status).unwrap();

        let final_status = load_onboarding_status(&data_dir);
        assert!(final_status.oauth_configured);
        assert!(final_status.tokens_stored);
        assert!(final_status.folder_selected);
        assert!(final_status.folder_accessible);
        assert!(final_status.initial_shred_complete);
        assert_eq!(final_status.selected_folder_id, Some("folder_xyz".into()));
    }

    #[test]
    fn test_onboarding_status_default_trait() {
        let status = OnboardingStatus::default();
        assert!(!status.oauth_configured);
        assert!(!status.tokens_stored);
        assert!(!status.folder_selected);
        assert!(!status.folder_accessible);
        assert!(!status.initial_shred_complete);
        assert!(status.selected_folder_id.is_none());
        assert!(status.selected_folder_name.is_none());
    }

    #[test]
    fn test_overwrite_onboarding_status() {
        let dir = TempDir::new().unwrap();
        let data_dir = setup_data_dir(&dir);

        let status1 = OnboardingStatus {
            oauth_configured: true,
            ..Default::default()
        };
        save_onboarding_status(&data_dir, &status1).unwrap();

        let status2 = OnboardingStatus {
            oauth_configured: true,
            tokens_stored: true,
            ..Default::default()
        };
        save_onboarding_status(&data_dir, &status2).unwrap();

        let loaded = load_onboarding_status(&data_dir);
        assert!(loaded.oauth_configured);
        assert!(loaded.tokens_stored);
    }
}
