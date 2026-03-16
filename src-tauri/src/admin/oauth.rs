use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

pub struct OAuthClient {
    config: OAuthConfig,
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

    pub async fn exchange_code(&self, code: &str) -> Result<TokenStorage, OAuthError> {
        let client = reqwest::Client::new();

        let params = [
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret.as_str()),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", self.config.redirect_uri.as_str()),
        ];

        let response = client
            .post("https://oauth2.googleapis.com/token")
            .form(&params)
            .send()
            .await?;

        let token_response: HashMap<String, serde_json::Value> = response.json().await?;

        let access_token = token_response
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| OAuthError::TokenRefresh("Missing access_token".to_string()))?
            .to_string();

        let refresh_token = token_response
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let expires_in = token_response
            .get("expires_in")
            .and_then(|v| v.as_u64())
            .unwrap_or(3600);

        let token_type = token_response
            .get("token_type")
            .and_then(|v| v.as_str())
            .unwrap_or("Bearer")
            .to_string();

        let expires_at = Utc::now() + chrono::Duration::seconds(expires_in as i64);

        let token_storage = TokenStorage {
            access_token,
            refresh_token,
            expires_at,
            token_type,
        };

        self.save_tokens(&token_storage)?;
        info!("OAuth tokens exchanged and saved successfully");
        Ok(token_storage)
    }

    pub async fn refresh_token(&self, refresh_token: &str) -> Result<TokenStorage, OAuthError> {
        let client = reqwest::Client::new();

        let params = [
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ];

        let response = client
            .post("https://oauth2.googleapis.com/token")
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(OAuthError::TokenRefresh(
                "Failed to refresh token".to_string(),
            ));
        }

        let token_response: HashMap<String, serde_json::Value> = response.json().await?;

        let access_token = token_response
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| OAuthError::TokenRefresh("Missing access_token".to_string()))?
            .to_string();

        let expires_in = token_response
            .get("expires_in")
            .and_then(|v| v.as_u64())
            .unwrap_or(3600);

        let token_type = token_response
            .get("token_type")
            .and_then(|v| v.as_str())
            .unwrap_or("Bearer")
            .to_string();

        let expires_at = Utc::now() + chrono::Duration::seconds(expires_in as i64);

        let token_storage = TokenStorage {
            access_token,
            refresh_token: Some(refresh_token.to_string()),
            expires_at,
            token_type,
        };

        self.save_tokens(&token_storage)?;
        info!("OAuth token refreshed successfully");
        Ok(token_storage)
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

    pub async fn get_access_token(&self) -> Result<String, OAuthError> {
        if let Some(tokens) = self.load_tokens()? {
            if tokens.is_expired() {
                if let Some(ref refresh) = tokens.refresh_token {
                    let new_tokens = self.refresh_token(refresh).await?;
                    return Ok(new_tokens.access_token);
                }
                return Err(OAuthError::TokenRefresh(
                    "Token expired and no refresh token available".to_string(),
                ));
            }
            Ok(tokens.access_token)
        } else {
            Err(OAuthError::TokenRefresh(
                "No tokens available".to_string(),
            ))
        }
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
            folder_selected: false,
            folder_accessible: false,
            initial_shred_complete: false,
            selected_folder_id: None,
            selected_folder_name: None,
        }
    }

    pub fn save_onboarding_status(&self, status: &OnboardingStatus) -> Result<(), OAuthError> {
        let content = serde_json::to_string_pretty(status)?;
        fs::write(&self.status_file, content)?;
        Ok(())
    }
}

pub async fn test_folder_permissions(
    access_token: &str,
    folder_id: &str,
) -> Result<bool, OAuthError> {
    let client = reqwest::Client::new();

    let response = client
        .get(&format!(
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

pub async fn list_drive_folders_api(
    access_token: &str,
) -> Result<Vec<DriveFolder>, OAuthError> {
    let client = reqwest::Client::new();

    let response = client
        .get("https://www.googleapis.com/drive/v3/files")
        .query(&[
            ("q", "mimeType='application/vnd.google-apps.folder' and trashed=false"),
            ("fields", "files(id,name,mimeType)"),
            ("pageSize", "100"),
            ("orderBy", "name"),
        ])
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await?;

    let body: serde_json::Value = response.json().await?;

    let folders = body
        .get("files")
        .and_then(|f| f.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    Some(DriveFolder {
                        id: item.get("id")?.as_str()?.to_string(),
                        name: item.get("name")?.as_str()?.to_string(),
                        mime_type: item.get("mimeType")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(folders)
}

/// Synchronously get the access token from storage (no refresh attempt).
/// Used by Tauri commands to extract the token before releasing the mutex.
fn get_valid_access_token(client: &OAuthClient) -> Result<String, OAuthError> {
    if let Some(tokens) = client.load_tokens()? {
        if tokens.is_expired() {
            return Err(OAuthError::TokenRefresh(
                "Token expired — re-authenticate".to_string(),
            ));
        }
        Ok(tokens.access_token)
    } else {
        Err(OAuthError::TokenRefresh(
            "No tokens available".to_string(),
        ))
    }
}

// --- Tauri Commands ---

#[tauri::command]
pub async fn initialize_oauth(state: State<'_, AppState>) -> Result<String, String> {
    let mut client = state.oauth_client.lock().map_err(|e| e.to_string())?;
    match client.load_config() {
        Ok(true) => {
            info!("OAuth client initialized with saved config");
            Ok("OAuth initialized with existing config".to_string())
        }
        Ok(false) => {
            info!("OAuth client initialized (no config yet)");
            Ok("OAuth initialized — needs configuration".to_string())
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
    Ok("OAuth configuration saved".to_string())
}

#[tauri::command]
pub async fn get_authorization_url(state: State<'_, AppState>) -> Result<String, String> {
    let mut client = state.oauth_client.lock().map_err(|e| e.to_string())?;
    client.load_config().map_err(|e| e.to_string())?;
    if client.config.client_id.is_empty() {
        return Err("OAuth not configured — set client_id and client_secret first".to_string());
    }
    Ok(client.get_authorization_url())
}

#[tauri::command]
pub async fn handle_oauth_callback(
    state: State<'_, AppState>,
    code: String,
) -> Result<String, String> {
    // Extract config before async work to avoid holding MutexGuard across await
    let config = {
        let client = state.oauth_client.lock().map_err(|e| e.to_string())?;
        client.config.clone()
    };

    // Build a temporary OAuthClient-like exchange using the extracted config
    let http = reqwest::Client::new();
    let params = [
        ("client_id", config.client_id.as_str()),
        ("client_secret", config.client_secret.as_str()),
        ("code", code.as_str()),
        ("grant_type", "authorization_code"),
        ("redirect_uri", config.redirect_uri.as_str()),
    ];
    let response = http
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let token_response: HashMap<String, serde_json::Value> =
        response.json().await.map_err(|e| e.to_string())?;

    let access_token = token_response
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or("Missing access_token")?
        .to_string();
    let refresh_token = token_response
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let expires_in = token_response
        .get("expires_in")
        .and_then(|v| v.as_u64())
        .unwrap_or(3600);
    let token_type = token_response
        .get("token_type")
        .and_then(|v| v.as_str())
        .unwrap_or("Bearer")
        .to_string();

    let tokens = TokenStorage {
        access_token,
        refresh_token,
        expires_at: Utc::now() + chrono::Duration::seconds(expires_in as i64),
        token_type,
    };

    // Re-acquire lock for synchronous file writes
    let client = state.oauth_client.lock().map_err(|e| e.to_string())?;
    client.save_tokens(&tokens).map_err(|e| e.to_string())?;
    let mut status = client.load_onboarding_status();
    status.oauth_configured = true;
    status.tokens_stored = true;
    client
        .save_onboarding_status(&status)
        .map_err(|e| e.to_string())?;

    info!("OAuth callback handled, tokens stored");
    Ok("Authentication successful".to_string())
}

#[tauri::command]
pub async fn test_folder_permissions_command(
    state: State<'_, AppState>,
    folder_id: String,
    folder_name: String,
) -> Result<bool, String> {
    let access_token = {
        let client = state.oauth_client.lock().map_err(|e| e.to_string())?;
        get_valid_access_token(&client).map_err(|e| e.to_string())?
    };

    let accessible = test_folder_permissions(&access_token, &folder_id)
        .await
        .map_err(|e| e.to_string())?;

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
    let access_token = {
        let client = state.oauth_client.lock().map_err(|e| e.to_string())?;
        get_valid_access_token(&client).map_err(|e| e.to_string())?
    };
    list_drive_folders_api(&access_token)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn trigger_initial_shred(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (access_token, folder_id) = {
        let client = state.oauth_client.lock().map_err(|e| e.to_string())?;
        let status = client.load_onboarding_status();
        if !status.tokens_stored {
            return Err("Not authenticated — complete OAuth first".to_string());
        }
        if !status.folder_selected {
            return Err("No folder selected — choose a folder first".to_string());
        }
        let folder_id = status
            .selected_folder_id
            .ok_or("No folder ID stored")?;
        let token = get_valid_access_token(&client).map_err(|e| e.to_string())?;
        (token, folder_id)
    };

    // Fetch file list from the selected folder
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

    // Mark shred as complete
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
    use chrono::Utc;
    use tempfile::TempDir;

    #[test]
    fn test_token_storage_expired() {
        let token = TokenStorage {
            access_token: "test".to_string(),
            refresh_token: Some("refresh".to_string()),
            expires_at: Utc::now() - chrono::Duration::seconds(10),
            token_type: "Bearer".to_string(),
        };
        assert!(token.is_expired());
    }

    #[test]
    fn test_token_storage_not_expired() {
        let token = TokenStorage {
            access_token: "test".to_string(),
            refresh_token: Some("refresh".to_string()),
            expires_at: Utc::now() + chrono::Duration::seconds(3600),
            token_type: "Bearer".to_string(),
        };
        assert!(!token.is_expired());
    }

    #[test]
    fn test_oauth_config_default() {
        let config = OAuthConfig::default();
        assert_eq!(config.redirect_uri, "http://localhost:1420/oauth/callback");
        assert_eq!(config.scopes.len(), 2);
        assert!(config.client_id.is_empty());
    }

    #[test]
    fn test_oauth_client_new() {
        let dir = TempDir::new().unwrap();
        let client = OAuthClient::new(dir.path());
        assert!(client.token_file.to_str().unwrap().contains("oauth_tokens"));
        assert!(client.config_file.to_str().unwrap().contains("oauth_config"));
    }

    #[test]
    fn test_save_and_load_config() {
        let dir = TempDir::new().unwrap();
        let mut client = OAuthClient::new(dir.path());
        let config = OAuthConfig {
            client_id: "test_id".to_string(),
            client_secret: "test_secret".to_string(),
            ..OAuthConfig::default()
        };
        client.save_config(&config).unwrap();
        assert!(client.load_config().unwrap());
        assert_eq!(client.config.client_id, "test_id");
    }

    #[test]
    fn test_save_and_load_tokens() {
        let dir = TempDir::new().unwrap();
        let client = OAuthClient::new(dir.path());

        let tokens = TokenStorage {
            access_token: "access_123".to_string(),
            refresh_token: Some("refresh_456".to_string()),
            expires_at: Utc::now() + chrono::Duration::seconds(3600),
            token_type: "Bearer".to_string(),
        };

        client.save_tokens(&tokens).unwrap();
        let loaded = client.load_tokens().unwrap().unwrap();
        assert_eq!(loaded.access_token, "access_123");
        assert_eq!(loaded.refresh_token, Some("refresh_456".to_string()));
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
            client_id: "my_client_id".to_string(),
            client_secret: "secret".to_string(),
            ..OAuthConfig::default()
        };
        let url = client.get_authorization_url();
        assert!(url.contains("my_client_id"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
        assert!(url.contains("drive.readonly"));
    }

    #[test]
    fn test_onboarding_status_default() {
        let dir = TempDir::new().unwrap();
        let client = OAuthClient::new(dir.path());
        let status = client.load_onboarding_status();
        assert!(!status.oauth_configured);
        assert!(!status.tokens_stored);
        assert!(!status.folder_selected);
        assert!(!status.initial_shred_complete);
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
            selected_folder_id: Some("folder_abc".to_string()),
            selected_folder_name: Some("My Lessons".to_string()),
        };
        client.save_onboarding_status(&status).unwrap();

        let loaded = client.load_onboarding_status();
        assert!(loaded.oauth_configured);
        assert!(loaded.tokens_stored);
        assert!(loaded.folder_selected);
        assert_eq!(loaded.selected_folder_id, Some("folder_abc".to_string()));
    }

    #[test]
    fn test_token_serialization_roundtrip() {
        let token = TokenStorage {
            access_token: "abc".to_string(),
            refresh_token: None,
            expires_at: Utc::now(),
            token_type: "Bearer".to_string(),
        };
        let json = serde_json::to_string(&token).unwrap();
        let deserialized: TokenStorage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.access_token, "abc");
        assert!(deserialized.refresh_token.is_none());
    }

    #[test]
    fn test_oauth_error_display() {
        let err = OAuthError::TokenRefresh("test error".to_string());
        assert_eq!(err.to_string(), "Token refresh failed: test error");
    }

    #[test]
    fn test_drive_folder_serialization() {
        let folder = DriveFolder {
            id: "id_1".to_string(),
            name: "My Folder".to_string(),
            mime_type: "application/vnd.google-apps.folder".to_string(),
        };
        let json = serde_json::to_string(&folder).unwrap();
        let deserialized: DriveFolder = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "id_1");
        assert_eq!(deserialized.name, "My Folder");
    }
}
