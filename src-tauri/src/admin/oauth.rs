use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::State;
use thiserror::Error;
use tracing::info;

use crate::AppState;

#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("Token refresh failed: {0}")]
    TokenRefresh(String),

    #[error("Not configured: {0}")]
    NotConfigured(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
}

impl Default for OAuthConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret: String::new(),
            redirect_uri: "http://localhost:1420/oauth/callback".to_string(),
            scopes: vec![
                "https://www.googleapis.com/auth/drive.readonly".to_string(),
                "https://www.googleapis.com/auth/documents.readonly".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenStorage {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub token_type: String,
}

impl TokenStorage {
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }
}

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

pub struct OAuthClient {
    pub config: OAuthConfig,
    token_file: PathBuf,
    config_file: PathBuf,
    status_file: PathBuf,
}

impl OAuthClient {
    pub fn new(data_dir: &Path) -> Self {
        let dir = data_dir.join("com.madison.chalk");
        fs::create_dir_all(&dir).ok();
        Self {
            config: OAuthConfig::default(),
            token_file: dir.join("oauth_tokens.json"),
            config_file: dir.join("oauth_config.json"),
            status_file: dir.join("onboarding_status.json"),
        }
    }

    pub fn load_config(&mut self) -> Result<bool, OAuthError> {
        if self.config_file.exists() {
            let content = fs::read_to_string(&self.config_file)?;
            self.config = serde_json::from_str(&content)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn save_config(&self, config: &OAuthConfig) -> Result<(), OAuthError> {
        let content = serde_json::to_string_pretty(config)?;
        fs::write(&self.config_file, content)?;
        Ok(())
    }

    pub fn get_authorization_url(&self) -> String {
        let scopes = self.config.scopes.join(" ");
        format!(
            "https://accounts.google.com/o/oauth2/v2/auth?\
             client_id={}&redirect_uri={}&response_type=code&\
             scope={}&access_type=offline&prompt=consent",
            self.config.client_id, self.config.redirect_uri, scopes
        )
    }

    /// Extract config and token file path for use outside the MutexGuard.
    pub fn exchange_params(&self) -> (OAuthConfig, PathBuf) {
        (self.config.clone(), self.token_file.clone())
    }

    pub fn load_tokens(&self) -> Result<Option<TokenStorage>, OAuthError> {
        if !self.token_file.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&self.token_file)?;
        let tokens: TokenStorage = serde_json::from_str(&content)?;
        Ok(Some(tokens))
    }

    pub fn save_tokens(&self, tokens: &TokenStorage) -> Result<(), OAuthError> {
        let content = serde_json::to_string_pretty(tokens)?;
        fs::write(&self.token_file, content)?;
        Ok(())
    }

    pub fn load_onboarding_status(&self) -> OnboardingStatus {
        if self.status_file.exists() {
            if let Ok(content) = fs::read_to_string(&self.status_file) {
                if let Ok(status) = serde_json::from_str(&content) {
                    return status;
                }
            }
        }
        OnboardingStatus {
            oauth_configured: self.config_file.exists(),
            tokens_stored: self.token_file.exists(),
            ..Default::default()
        }
    }

    pub fn save_onboarding_status(&self, status: &OnboardingStatus) -> Result<(), OAuthError> {
        let content = serde_json::to_string_pretty(status)?;
        fs::write(&self.status_file, content)?;
        Ok(())
    }
}

/// Exchange an authorization code for tokens (async, no MutexGuard held).
pub async fn exchange_code(
    config: &OAuthConfig,
    code: &str,
    token_file: &Path,
) -> Result<TokenStorage, OAuthError> {
    let client = reqwest::Client::new();

    let params = [
        ("client_id", config.client_id.as_str()),
        ("client_secret", config.client_secret.as_str()),
        ("code", code),
        ("grant_type", "authorization_code"),
        ("redirect_uri", config.redirect_uri.as_str()),
    ];

    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await?;

    let body: serde_json::Value = response.json().await?;

    let access_token = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| OAuthError::TokenRefresh("Missing access_token in response".into()))?
        .to_string();

    let refresh_token = body
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let expires_in = body
        .get("expires_in")
        .and_then(|v| v.as_u64())
        .unwrap_or(3600);

    let token_type = body
        .get("token_type")
        .and_then(|v| v.as_str())
        .unwrap_or("Bearer")
        .to_string();

    let expires_at = Utc::now() + chrono::Duration::seconds(expires_in as i64);

    let tokens = TokenStorage {
        access_token,
        refresh_token,
        expires_at,
        token_type,
    };

    let content = serde_json::to_string_pretty(&tokens)?;
    fs::write(token_file, content)?;
    info!("OAuth tokens exchanged and saved");
    Ok(tokens)
}

/// Refresh an expired access token (async, no MutexGuard held).
pub async fn refresh_access_token(
    config: &OAuthConfig,
    refresh_token: &str,
    token_file: &Path,
) -> Result<TokenStorage, OAuthError> {
    let client = reqwest::Client::new();

    let params = [
        ("client_id", config.client_id.as_str()),
        ("client_secret", config.client_secret.as_str()),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
    ];

    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(OAuthError::TokenRefresh("Refresh request failed".into()));
    }

    let body: serde_json::Value = response.json().await?;

    let access_token = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| OAuthError::TokenRefresh("Missing access_token in response".into()))?
        .to_string();

    let expires_in = body
        .get("expires_in")
        .and_then(|v| v.as_u64())
        .unwrap_or(3600);

    let token_type = body
        .get("token_type")
        .and_then(|v| v.as_str())
        .unwrap_or("Bearer")
        .to_string();

    let expires_at = Utc::now() + chrono::Duration::seconds(expires_in as i64);

    let tokens = TokenStorage {
        access_token,
        refresh_token: Some(refresh_token.to_string()),
        expires_at,
        token_type,
    };

    let content = serde_json::to_string_pretty(&tokens)?;
    fs::write(token_file, content)?;
    info!("OAuth token refreshed successfully");
    Ok(tokens)
}

/// Get a valid access token, refreshing if needed (async, no MutexGuard held).
pub async fn get_valid_access_token(
    config: &OAuthConfig,
    token_file: &Path,
) -> Result<String, OAuthError> {
    if !token_file.exists() {
        return Err(OAuthError::NotConfigured("No tokens stored".into()));
    }
    let content = fs::read_to_string(token_file)?;
    let tokens: TokenStorage = serde_json::from_str(&content)?;

    if !tokens.is_expired() {
        return Ok(tokens.access_token);
    }

    if let Some(ref refresh) = tokens.refresh_token {
        let new_tokens = refresh_access_token(config, refresh, token_file).await?;
        return Ok(new_tokens.access_token);
    }

    Err(OAuthError::TokenRefresh(
        "Token expired and no refresh token available".into(),
    ))
}

/// Test folder permissions via the Google Drive API.
pub async fn test_folder_permissions(
    access_token: &str,
    folder_id: &str,
) -> Result<bool, OAuthError> {
    let client = reqwest::Client::new();

    let response = client
        .get(format!(
            "https://www.googleapis.com/drive/v3/files/{}?fields=capabilities",
            folder_id
        ))
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await?;

    Ok(response.status().is_success())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveFolder {
    pub id: String,
    pub name: String,
    pub mime_type: String,
}

/// Parse Drive API response JSON into filtered, sorted folders.
/// Filters out hidden/system folders (names starting with '.' or '!') and
/// sorts alphabetically by name (case-insensitive).
pub fn parse_drive_folders(body: &serde_json::Value) -> Vec<DriveFolder> {
    let mut folders: Vec<DriveFolder> = body
        .get("files")
        .and_then(|f| f.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let name = item.get("name")?.as_str()?;
                    if name.starts_with('.') || name.starts_with('!') {
                        return None;
                    }
                    Some(DriveFolder {
                        id: item.get("id")?.as_str()?.to_string(),
                        name: name.to_string(),
                        mime_type: item.get("mimeType")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    folders.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    folders
}

/// List folders from Google Drive.
pub async fn list_drive_folders_api(
    access_token: &str,
) -> Result<Vec<DriveFolder>, OAuthError> {
    let client = reqwest::Client::new();

    let response = client
        .get("https://www.googleapis.com/drive/v3/files")
        .query(&[
            (
                "q",
                "mimeType='application/vnd.google-apps.folder' and trashed=false",
            ),
            ("fields", "files(id,name,mimeType)"),
            ("pageSize", "100"),
            ("orderBy", "name"),
        ])
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await?;

    let body: serde_json::Value = response.json().await?;

    Ok(parse_drive_folders(&body))
}

// ── Tauri Commands ──────────────────────────────────────────────

#[tauri::command]
pub async fn initialize_oauth(state: State<'_, AppState>) -> Result<String, String> {
    let mut client = state.oauth_client.lock().map_err(|e| e.to_string())?;
    match client.load_config() {
        Ok(true) => {
            info!("OAuth client initialized with saved config");
            Ok("OAuth initialized with existing config".into())
        }
        Ok(false) => {
            info!("OAuth client initialized (no config yet)");
            Ok("OAuth initialized — needs configuration".into())
        }
        Err(e) => Err(format!("Failed to initialize OAuth: {}", e)),
    }
}

#[tauri::command]
pub async fn save_oauth_config(
    state: State<'_, AppState>,
    client_id: String,
    client_secret: String,
) -> Result<String, String> {
    let mut client = state.oauth_client.lock().map_err(|e| e.to_string())?;
    let config = OAuthConfig {
        client_id,
        client_secret,
        ..OAuthConfig::default()
    };
    client.save_config(&config).map_err(|e| e.to_string())?;
    client.config = config;
    info!("OAuth config saved");
    Ok("OAuth configuration saved".into())
}

#[tauri::command]
pub async fn get_authorization_url(state: State<'_, AppState>) -> Result<String, String> {
    let mut client = state.oauth_client.lock().map_err(|e| e.to_string())?;
    client.load_config().map_err(|e| e.to_string())?;
    if client.config.client_id.is_empty() {
        return Err("OAuth not configured — set client_id and client_secret first".into());
    }
    Ok(client.get_authorization_url())
}

#[tauri::command]
pub async fn handle_oauth_callback(
    state: State<'_, AppState>,
    code: String,
) -> Result<String, String> {
    // Extract what we need, then drop the MutexGuard before awaiting.
    let (config, token_file) = {
        let client = state.oauth_client.lock().map_err(|e| e.to_string())?;
        client.exchange_params()
    };

    exchange_code(&config, &code, &token_file)
        .await
        .map_err(|e| e.to_string())?;

    // Update onboarding status.
    {
        let client = state.oauth_client.lock().map_err(|e| e.to_string())?;
        let mut status = client.load_onboarding_status();
        status.oauth_configured = true;
        status.tokens_stored = true;
        client
            .save_onboarding_status(&status)
            .map_err(|e| e.to_string())?;
    }

    info!("OAuth callback handled, tokens stored");
    Ok("Authentication successful".into())
}

#[tauri::command]
pub async fn test_folder_permissions_command(
    state: State<'_, AppState>,
    folder_id: String,
    folder_name: String,
) -> Result<bool, String> {
    let (config, token_file) = {
        let client = state.oauth_client.lock().map_err(|e| e.to_string())?;
        client.exchange_params()
    };

    let access_token = get_valid_access_token(&config, &token_file)
        .await
        .map_err(|e| e.to_string())?;

    let accessible = test_folder_permissions(&access_token, &folder_id)
        .await
        .map_err(|e| e.to_string())?;

    // Update onboarding status.
    {
        let client = state.oauth_client.lock().map_err(|e| e.to_string())?;
        let mut status = client.load_onboarding_status();
        status.folder_selected = true;
        status.folder_accessible = accessible;
        status.selected_folder_id = Some(folder_id);
        status.selected_folder_name = Some(folder_name);
        client
            .save_onboarding_status(&status)
            .map_err(|e| e.to_string())?;
    }

    Ok(accessible)
}

#[tauri::command]
pub async fn check_onboarding_status(
    state: State<'_, AppState>,
) -> Result<OnboardingStatus, String> {
    let client = state.oauth_client.lock().map_err(|e| e.to_string())?;
    Ok(client.load_onboarding_status())
}

#[tauri::command]
pub async fn list_drive_folders(
    state: State<'_, AppState>,
) -> Result<Vec<DriveFolder>, String> {
    let (config, token_file) = {
        let client = state.oauth_client.lock().map_err(|e| e.to_string())?;
        client.exchange_params()
    };

    let access_token = get_valid_access_token(&config, &token_file)
        .await
        .map_err(|e| e.to_string())?;

    list_drive_folders_api(&access_token)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn trigger_initial_shred(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (config, token_file, folder_id) = {
        let client = state.oauth_client.lock().map_err(|e| e.to_string())?;
        let status = client.load_onboarding_status();
        if !status.tokens_stored {
            return Err("Not authenticated — complete OAuth first".into());
        }
        if !status.folder_selected {
            return Err("No folder selected — choose a folder first".into());
        }
        let folder_id = status
            .selected_folder_id
            .clone()
            .ok_or("No folder ID stored")?;
        let (cfg, tf) = client.exchange_params();
        (cfg, tf, folder_id)
    };

    let access_token = get_valid_access_token(&config, &token_file)
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
    {
        let client = state.oauth_client.lock().map_err(|e| e.to_string())?;
        let mut updated_status = client.load_onboarding_status();
        updated_status.initial_shred_complete = true;
        client
            .save_onboarding_status(&updated_status)
            .map_err(|e| e.to_string())?;
    }

    Ok(format!(
        "Initial shred complete — found {} document(s) to process",
        file_count
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_token_storage_expired() {
        let token = TokenStorage {
            access_token: "test".into(),
            refresh_token: Some("refresh".into()),
            expires_at: Utc::now() - chrono::Duration::seconds(10),
            token_type: "Bearer".into(),
        };
        assert!(token.is_expired());
    }

    #[test]
    fn test_token_storage_not_expired() {
        let token = TokenStorage {
            access_token: "test".into(),
            refresh_token: Some("refresh".into()),
            expires_at: Utc::now() + chrono::Duration::seconds(3600),
            token_type: "Bearer".into(),
        };
        assert!(!token.is_expired());
    }

    #[test]
    fn test_oauth_config_default() {
        let config = OAuthConfig::default();
        assert_eq!(config.redirect_uri, "http://localhost:1420/oauth/callback");
        assert_eq!(config.scopes.len(), 2);
        assert!(config.client_id.is_empty());
        assert!(config.scopes[0].contains("drive.readonly"));
        assert!(config.scopes[1].contains("documents.readonly"));
    }

    #[test]
    fn test_oauth_client_new() {
        let dir = TempDir::new().unwrap();
        let client = OAuthClient::new(dir.path());
        assert!(client.token_file.to_str().unwrap().contains("oauth_tokens"));
        assert!(client.config_file.to_str().unwrap().contains("oauth_config"));
        assert!(client.status_file.to_str().unwrap().contains("onboarding_status"));
    }

    #[test]
    fn test_save_and_load_config() {
        let dir = TempDir::new().unwrap();
        let mut client = OAuthClient::new(dir.path());
        let config = OAuthConfig {
            client_id: "test_id".into(),
            client_secret: "test_secret".into(),
            ..OAuthConfig::default()
        };
        client.save_config(&config).unwrap();
        assert!(client.load_config().unwrap());
        assert_eq!(client.config.client_id, "test_id");
        assert_eq!(client.config.client_secret, "test_secret");
    }

    #[test]
    fn test_load_config_no_file() {
        let dir = TempDir::new().unwrap();
        let mut client = OAuthClient::new(dir.path());
        assert!(!client.load_config().unwrap());
    }

    #[test]
    fn test_save_and_load_tokens() {
        let dir = TempDir::new().unwrap();
        let client = OAuthClient::new(dir.path());

        let tokens = TokenStorage {
            access_token: "access_123".into(),
            refresh_token: Some("refresh_456".into()),
            expires_at: Utc::now() + chrono::Duration::seconds(3600),
            token_type: "Bearer".into(),
        };

        client.save_tokens(&tokens).unwrap();
        let loaded = client.load_tokens().unwrap().unwrap();
        assert_eq!(loaded.access_token, "access_123");
        assert_eq!(loaded.refresh_token, Some("refresh_456".into()));
        assert_eq!(loaded.token_type, "Bearer");
    }

    #[test]
    fn test_load_tokens_no_file() {
        let dir = TempDir::new().unwrap();
        let client = OAuthClient::new(dir.path());
        let result = client.load_tokens().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_authorization_url() {
        let dir = TempDir::new().unwrap();
        let mut client = OAuthClient::new(dir.path());
        client.config = OAuthConfig {
            client_id: "my_client_id".into(),
            client_secret: "secret".into(),
            ..OAuthConfig::default()
        };
        let url = client.get_authorization_url();
        assert!(url.contains("my_client_id"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
        assert!(url.contains("drive.readonly"));
        assert!(url.contains("response_type=code"));
    }

    #[test]
    fn test_authorization_url_empty_client_id() {
        let dir = TempDir::new().unwrap();
        let client = OAuthClient::new(dir.path());
        let url = client.get_authorization_url();
        assert!(url.contains("client_id=&"));
    }

    #[test]
    fn test_onboarding_status_default() {
        let dir = TempDir::new().unwrap();
        let client = OAuthClient::new(dir.path());
        let status = client.load_onboarding_status();
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
        let client = OAuthClient::new(dir.path());

        let status = OnboardingStatus {
            oauth_configured: true,
            tokens_stored: true,
            folder_selected: true,
            folder_accessible: true,
            initial_shred_complete: false,
            selected_folder_id: Some("folder_abc".into()),
            selected_folder_name: Some("My Lessons".into()),
        };
        client.save_onboarding_status(&status).unwrap();

        let loaded = client.load_onboarding_status();
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
        let chalk_dir = dir.path().join("com.madison.chalk");
        fs::create_dir_all(&chalk_dir).unwrap();

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

        let client = OAuthClient::new(dir.path());
        let status = client.load_onboarding_status();
        assert!(status.oauth_configured);
        assert!(status.tokens_stored);
    }

    #[test]
    fn test_token_serialization_roundtrip() {
        let token = TokenStorage {
            access_token: "abc".into(),
            refresh_token: None,
            expires_at: Utc::now(),
            token_type: "Bearer".into(),
        };
        let json = serde_json::to_string(&token).unwrap();
        let deserialized: TokenStorage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.access_token, "abc");
        assert!(deserialized.refresh_token.is_none());
    }

    #[test]
    fn test_oauth_error_display() {
        let err = OAuthError::TokenRefresh("test error".into());
        assert_eq!(err.to_string(), "Token refresh failed: test error");

        let err2 = OAuthError::NotConfigured("no config".into());
        assert_eq!(err2.to_string(), "Not configured: no config");
    }

    #[test]
    fn test_parse_drive_folders_filters_hidden() {
        let body = serde_json::json!({
            "files": [
                {"id": "1", "name": "Lesson Plans", "mimeType": "application/vnd.google-apps.folder"},
                {"id": "2", "name": ".cache", "mimeType": "application/vnd.google-apps.folder"},
                {"id": "3", "name": ".cp", "mimeType": "application/vnd.google-apps.folder"},
                {"id": "4", "name": "!internal", "mimeType": "application/vnd.google-apps.folder"},
                {"id": "5", "name": "Worksheets", "mimeType": "application/vnd.google-apps.folder"},
            ]
        });
        let folders = parse_drive_folders(&body);
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].name, "Lesson Plans");
        assert_eq!(folders[1].name, "Worksheets");
    }

    #[test]
    fn test_parse_drive_folders_sorts_alphabetically() {
        let body = serde_json::json!({
            "files": [
                {"id": "1", "name": "Zebra", "mimeType": "application/vnd.google-apps.folder"},
                {"id": "2", "name": "alpha", "mimeType": "application/vnd.google-apps.folder"},
                {"id": "3", "name": "Beta", "mimeType": "application/vnd.google-apps.folder"},
            ]
        });
        let folders = parse_drive_folders(&body);
        assert_eq!(folders.len(), 3);
        assert_eq!(folders[0].name, "alpha");
        assert_eq!(folders[1].name, "Beta");
        assert_eq!(folders[2].name, "Zebra");
    }

    #[test]
    fn test_parse_drive_folders_empty() {
        let body = serde_json::json!({"files": []});
        let folders = parse_drive_folders(&body);
        assert!(folders.is_empty());
    }

    #[test]
    fn test_parse_drive_folders_no_files_key() {
        let body = serde_json::json!({});
        let folders = parse_drive_folders(&body);
        assert!(folders.is_empty());
    }

    #[test]
    fn test_drive_folder_serialization() {
        let folder = DriveFolder {
            id: "id_1".into(),
            name: "My Folder".into(),
            mime_type: "application/vnd.google-apps.folder".into(),
        };
        let json = serde_json::to_string(&folder).unwrap();
        let deserialized: DriveFolder = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "id_1");
        assert_eq!(deserialized.name, "My Folder");
        assert_eq!(deserialized.mime_type, "application/vnd.google-apps.folder");
    }

    #[test]
    fn test_exchange_params() {
        let dir = TempDir::new().unwrap();
        let mut client = OAuthClient::new(dir.path());
        client.config = OAuthConfig {
            client_id: "ex_id".into(),
            client_secret: "ex_secret".into(),
            ..OAuthConfig::default()
        };
        let (cfg, tf) = client.exchange_params();
        assert_eq!(cfg.client_id, "ex_id");
        assert!(tf.to_str().unwrap().contains("oauth_tokens"));
    }

    #[test]
    fn test_onboarding_status_complete_flow() {
        let dir = TempDir::new().unwrap();
        let client = OAuthClient::new(dir.path());

        let mut status = client.load_onboarding_status();

        status.oauth_configured = true;
        client.save_onboarding_status(&status).unwrap();

        status.tokens_stored = true;
        client.save_onboarding_status(&status).unwrap();

        status.folder_selected = true;
        status.folder_accessible = true;
        status.selected_folder_id = Some("folder_xyz".into());
        status.selected_folder_name = Some("Lesson Plans".into());
        client.save_onboarding_status(&status).unwrap();

        status.initial_shred_complete = true;
        client.save_onboarding_status(&status).unwrap();

        let final_status = client.load_onboarding_status();
        assert!(final_status.oauth_configured);
        assert!(final_status.tokens_stored);
        assert!(final_status.folder_selected);
        assert!(final_status.folder_accessible);
        assert!(final_status.initial_shred_complete);
        assert_eq!(final_status.selected_folder_id, Some("folder_xyz".into()));
    }

    #[test]
    fn test_config_scopes_roundtrip() {
        let config = OAuthConfig {
            client_id: "cid".into(),
            client_secret: "cs".into(),
            redirect_uri: "http://localhost/cb".into(),
            scopes: vec!["scope1".into(), "scope2".into()],
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: OAuthConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.scopes.len(), 2);
        assert_eq!(restored.scopes[0], "scope1");
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

    #[tokio::test]
    async fn test_get_valid_access_token_no_file() {
        let dir = TempDir::new().unwrap();
        let token_file = dir.path().join("nonexistent.json");
        let config = OAuthConfig::default();
        let result = get_valid_access_token(&config, &token_file).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_valid_access_token_valid_token() {
        let dir = TempDir::new().unwrap();
        let token_file = dir.path().join("tokens.json");

        let tokens = TokenStorage {
            access_token: "valid_token".into(),
            refresh_token: None,
            expires_at: Utc::now() + chrono::Duration::seconds(3600),
            token_type: "Bearer".into(),
        };
        fs::write(&token_file, serde_json::to_string(&tokens).unwrap()).unwrap();

        let config = OAuthConfig::default();
        let result = get_valid_access_token(&config, &token_file).await.unwrap();
        assert_eq!(result, "valid_token");
    }

    #[tokio::test]
    async fn test_get_valid_access_token_expired_no_refresh() {
        let dir = TempDir::new().unwrap();
        let token_file = dir.path().join("tokens.json");

        let tokens = TokenStorage {
            access_token: "expired_token".into(),
            refresh_token: None,
            expires_at: Utc::now() - chrono::Duration::seconds(10),
            token_type: "Bearer".into(),
        };
        fs::write(&token_file, serde_json::to_string(&tokens).unwrap()).unwrap();

        let config = OAuthConfig::default();
        let result = get_valid_access_token(&config, &token_file).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("expired"));
    }

    #[test]
    fn test_overwrite_onboarding_status() {
        let dir = TempDir::new().unwrap();
        let client = OAuthClient::new(dir.path());

        let status1 = OnboardingStatus {
            oauth_configured: true,
            ..Default::default()
        };
        client.save_onboarding_status(&status1).unwrap();

        let status2 = OnboardingStatus {
            oauth_configured: true,
            tokens_stored: true,
            ..Default::default()
        };
        client.save_onboarding_status(&status2).unwrap();

        let loaded = client.load_onboarding_status();
        assert!(loaded.oauth_configured);
        assert!(loaded.tokens_stored);
    }

    #[test]
    fn test_save_config_creates_directory() {
        let dir = TempDir::new().unwrap();
        let client = OAuthClient::new(dir.path());
        let config = OAuthConfig {
            client_id: "cid".into(),
            client_secret: "cs".into(),
            ..OAuthConfig::default()
        };
        client.save_config(&config).unwrap();
        assert!(client.config_file.exists());
    }

    #[test]
    fn test_multiple_token_saves() {
        let dir = TempDir::new().unwrap();
        let client = OAuthClient::new(dir.path());

        let tokens1 = TokenStorage {
            access_token: "first".into(),
            refresh_token: Some("r1".into()),
            expires_at: Utc::now() + chrono::Duration::seconds(100),
            token_type: "Bearer".into(),
        };
        client.save_tokens(&tokens1).unwrap();

        let tokens2 = TokenStorage {
            access_token: "second".into(),
            refresh_token: Some("r2".into()),
            expires_at: Utc::now() + chrono::Duration::seconds(200),
            token_type: "Bearer".into(),
        };
        client.save_tokens(&tokens2).unwrap();

        let loaded = client.load_tokens().unwrap().unwrap();
        assert_eq!(loaded.access_token, "second");
        assert_eq!(loaded.refresh_token, Some("r2".into()));
    }
}
