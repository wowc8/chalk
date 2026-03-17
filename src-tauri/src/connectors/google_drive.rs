// GoogleDriveConnector — implements LessonPlanConnector for Google Drive.
// Extracted from admin/oauth.rs during the connector architecture refactor.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::info;

use super::{
    AuthStatus, ConnectorConfig, ConnectorError, ConnectorInfo, Document, FreshnessStatus,
    LessonPlanConnector, Source,
};

/// Embedded OAuth client ID for the distributed desktop app.
/// This is a public client using PKCE — no client secret is needed.
/// Replace this placeholder with the real client ID from your Google Cloud project.
const EMBEDDED_CLIENT_ID: &str = "PLACEHOLDER.apps.googleusercontent.com";

// ── PKCE Helpers ────────────────────────────────────────────────

/// Generate a cryptographically random PKCE code verifier (43–128 chars, base64url).
pub fn generate_code_verifier() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen::<u8>()).collect();
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Derive the S256 code challenge from a code verifier.
pub fn generate_code_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

/// OAuth configuration for Google Drive.
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

/// Stored OAuth tokens.
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

/// Internal mutable state for the Google Drive connector.
struct GoogleDriveState {
    oauth_config: OAuthConfig,
    tokens: Option<TokenStorage>,
    /// Whether we're using the embedded (PKCE) credentials vs user-provided.
    using_embedded: bool,
    /// PKCE code verifier for the current auth flow (ephemeral, per-session).
    pkce_verifier: Option<String>,
}

/// Google Drive connector implementing the LessonPlanConnector trait.
pub struct GoogleDriveConnector {
    config: ConnectorConfig,
    data_dir: PathBuf,
    state: Mutex<GoogleDriveState>,
}

impl GoogleDriveConnector {
    /// Create a new GoogleDriveConnector from a ConnectorConfig.
    pub fn new(config: &ConnectorConfig, data_dir: &Path) -> Result<Self, ConnectorError> {
        let oauth_config = if let Some(ref creds) = config.credentials {
            serde_json::from_str(creds).map_err(ConnectorError::Json)?
        } else {
            OAuthConfig::default()
        };

        // Try to load existing tokens from disk.
        let token_file = data_dir.join("com.madison.chalk").join("oauth_tokens.json");
        let tokens = if token_file.exists() {
            let content = fs::read_to_string(&token_file)?;
            serde_json::from_str(&content).ok()
        } else {
            None
        };

        Ok(Self {
            config: config.clone(),
            data_dir: data_dir.to_path_buf(),
            state: Mutex::new(GoogleDriveState {
                oauth_config,
                tokens,
                using_embedded: false,
                pkce_verifier: None,
            }),
        })
    }

    /// Get the path to the token file.
    fn token_file(&self) -> PathBuf {
        self.data_dir
            .join("com.madison.chalk")
            .join("oauth_tokens.json")
    }

    /// Get the path to the OAuth config file.
    fn config_file(&self) -> PathBuf {
        self.data_dir
            .join("com.madison.chalk")
            .join("oauth_config.json")
    }

    /// Get a valid access token, refreshing if necessary.
    /// This is a blocking call — use from sync context only.
    pub fn get_valid_access_token_blocking(&self) -> Result<String, ConnectorError> {
        let state = self.state.lock().map_err(|e| ConnectorError::Other(e.to_string()))?;
        if let Some(ref tokens) = state.tokens {
            if !tokens.is_expired() {
                return Ok(tokens.access_token.clone());
            }
        }
        // If expired or no tokens, return error (async refresh handled at command level).
        Err(ConnectorError::Auth(
            "Token expired or not available — use async refresh".into(),
        ))
    }

    /// Load embedded credentials (PKCE flow, no client secret).
    /// Returns true if embedded credentials are available.
    pub fn load_embedded_credentials(&self) -> bool {
        if EMBEDDED_CLIENT_ID == "PLACEHOLDER.apps.googleusercontent.com" {
            return false;
        }
        if let Ok(mut state) = self.state.lock() {
            state.oauth_config = OAuthConfig {
                client_id: EMBEDDED_CLIENT_ID.to_string(),
                client_secret: String::new(), // PKCE — no secret needed
                ..OAuthConfig::default()
            };
            state.using_embedded = true;
            true
        } else {
            false
        }
    }

    /// Check whether embedded credentials are configured.
    pub fn has_embedded_credentials() -> bool {
        EMBEDDED_CLIENT_ID != "PLACEHOLDER.apps.googleusercontent.com"
    }

    /// Load OAuth config from disk into the connector state.
    /// Falls back to embedded credentials if no config file exists.
    pub fn load_oauth_config(&self) -> Result<bool, ConnectorError> {
        let config_file = self.config_file();
        if config_file.exists() {
            let content = fs::read_to_string(&config_file)?;
            let oauth_config: OAuthConfig = serde_json::from_str(&content)?;
            let mut state = self.state.lock().map_err(|e| ConnectorError::Other(e.to_string()))?;
            state.oauth_config = oauth_config;
            state.using_embedded = false;
            Ok(true)
        } else if self.load_embedded_credentials() {
            info!("Using embedded OAuth credentials (PKCE flow)");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Save OAuth config to disk.
    pub fn save_oauth_config(&self, config: &OAuthConfig) -> Result<(), ConnectorError> {
        let config_file = self.config_file();
        let content = serde_json::to_string_pretty(config)?;
        fs::write(&config_file, content)?;
        let mut state = self.state.lock().map_err(|e| ConnectorError::Other(e.to_string()))?;
        state.oauth_config = config.clone();
        Ok(())
    }

    /// Get the OAuth config, token file path, and optional PKCE verifier for async operations.
    pub fn exchange_params(&self) -> Result<(OAuthConfig, PathBuf, Option<String>), ConnectorError> {
        let state = self.state.lock().map_err(|e| ConnectorError::Other(e.to_string()))?;
        Ok((
            state.oauth_config.clone(),
            self.token_file(),
            state.pkce_verifier.clone(),
        ))
    }

    /// Get the authorization URL for Google OAuth.
    /// When using embedded credentials or no-secret creds, generates PKCE challenge.
    pub fn get_authorization_url(&self) -> Result<String, ConnectorError> {
        let mut state = self.state.lock().map_err(|e| ConnectorError::Other(e.to_string()))?;
        let scopes = state.oauth_config.scopes.join(" ");
        let mut url = format!(
            "https://accounts.google.com/o/oauth2/v2/auth?\
             client_id={}&redirect_uri={}&response_type=code&\
             scope={}&access_type=offline&prompt=consent",
            state.oauth_config.client_id, state.oauth_config.redirect_uri, scopes
        );

        // Always generate PKCE params for embedded credentials.
        // Also generate for user-provided creds with empty secret (extra security).
        if state.using_embedded || state.oauth_config.client_secret.is_empty() {
            let verifier = generate_code_verifier();
            let challenge = generate_code_challenge(&verifier);
            url.push_str(&format!(
                "&code_challenge={}&code_challenge_method=S256",
                challenge
            ));
            state.pkce_verifier = Some(verifier);
        }

        Ok(url)
    }

    /// Update stored tokens after an exchange or refresh.
    pub fn update_tokens(&self, tokens: TokenStorage) -> Result<(), ConnectorError> {
        let mut state = self.state.lock().map_err(|e| ConnectorError::Other(e.to_string()))?;
        state.tokens = Some(tokens);
        Ok(())
    }

    /// Get current OAuth config (clone).
    pub fn oauth_config(&self) -> Result<OAuthConfig, ConnectorError> {
        let state = self.state.lock().map_err(|e| ConnectorError::Other(e.to_string()))?;
        Ok(state.oauth_config.clone())
    }
}

impl LessonPlanConnector for GoogleDriveConnector {
    fn info(&self) -> ConnectorInfo {
        ConnectorInfo {
            id: self.config.id.clone(),
            connector_type: "google_drive".to_string(),
            display_name: self.config.display_name.clone(),
            icon: "google-drive".to_string(),
            description: "Connect to Google Drive to import lesson plans".to_string(),
        }
    }

    fn auth_status(&self) -> AuthStatus {
        let state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => return AuthStatus::Disconnected,
        };
        match &state.tokens {
            Some(tokens) if !tokens.is_expired() => AuthStatus::Connected,
            Some(_) => AuthStatus::Expired,
            None => AuthStatus::Disconnected,
        }
    }

    fn authenticate(&self) -> Result<AuthStatus, ConnectorError> {
        // OAuth flow is interactive — actual auth is handled by Tauri commands.
        // This just checks current state.
        Ok(self.auth_status())
    }

    fn disconnect(&self) -> Result<(), ConnectorError> {
        let mut state = self.state.lock().map_err(|e| ConnectorError::Other(e.to_string()))?;
        state.tokens = None;

        // Remove token file from disk.
        let token_file = self.token_file();
        if token_file.exists() {
            fs::remove_file(&token_file)?;
        }
        info!(connector_id = self.config.id.as_str(), "Google Drive disconnected");
        Ok(())
    }

    fn list_sources(
        &self,
        _parent_id: Option<&str>,
    ) -> Result<Vec<Source>, ConnectorError> {
        // Synchronous listing is not practical for Google Drive (needs async HTTP).
        // The actual listing is done via async Tauri commands.
        // This returns an empty vec; real listing goes through list_sources_async.
        Err(ConnectorError::Other(
            "Use async Tauri commands for Google Drive source listing".into(),
        ))
    }

    fn fetch_document(&self, _id: &str) -> Result<Document, ConnectorError> {
        // Synchronous fetch is not practical for Google Drive.
        Err(ConnectorError::Other(
            "Use async Tauri commands for Google Drive document fetching".into(),
        ))
    }

    fn check_freshness(&self, _id: &str) -> Result<FreshnessStatus, ConnectorError> {
        Err(ConnectorError::Other(
            "Use async Tauri commands for Google Drive freshness checks".into(),
        ))
    }
}

// ── Async Google Drive API helpers ─────────────────────────────────

/// Exchange an authorization code for tokens.
/// When `code_verifier` is provided, uses PKCE flow (no client_secret).
pub async fn exchange_code(
    config: &OAuthConfig,
    code: &str,
    token_file: &Path,
    code_verifier: Option<&str>,
) -> Result<TokenStorage, ConnectorError> {
    let client = reqwest::Client::new();

    let mut form: Vec<(&str, &str)> = vec![
        ("client_id", config.client_id.as_str()),
        ("code", code),
        ("grant_type", "authorization_code"),
        ("redirect_uri", config.redirect_uri.as_str()),
    ];

    // PKCE flow: send code_verifier instead of client_secret.
    // For user-provided credentials with a secret, send the secret.
    if let Some(verifier) = code_verifier {
        form.push(("code_verifier", verifier));
    }
    if !config.client_secret.is_empty() {
        form.push(("client_secret", config.client_secret.as_str()));
    }

    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&form)
        .send()
        .await?;

    let body: serde_json::Value = response.json().await?;

    let access_token = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ConnectorError::Auth("Missing access_token in response".into()))?
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

/// Refresh an expired access token.
/// Conditionally includes client_secret (omitted for PKCE/embedded flows).
pub async fn refresh_access_token(
    config: &OAuthConfig,
    refresh_token: &str,
    token_file: &Path,
) -> Result<TokenStorage, ConnectorError> {
    let client = reqwest::Client::new();

    let mut params: Vec<(&str, &str)> = vec![
        ("client_id", config.client_id.as_str()),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
    ];
    if !config.client_secret.is_empty() {
        params.push(("client_secret", config.client_secret.as_str()));
    }

    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(ConnectorError::Auth("Refresh request failed".into()));
    }

    let body: serde_json::Value = response.json().await?;

    let access_token = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ConnectorError::Auth("Missing access_token in response".into()))?
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

/// Get a valid access token, refreshing if needed.
pub async fn get_valid_access_token(
    config: &OAuthConfig,
    token_file: &Path,
) -> Result<String, ConnectorError> {
    if !token_file.exists() {
        return Err(ConnectorError::NotConnected("No tokens stored".into()));
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

    Err(ConnectorError::Auth(
        "Token expired and no refresh token available".into(),
    ))
}

/// A Drive folder entry (used for browsing).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveFolder {
    pub id: String,
    pub name: String,
    pub mime_type: String,
}

/// A Drive item entry (folders and documents).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveItem {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub is_folder: bool,
}

/// Parse Drive API response JSON into filtered, sorted folders.
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

/// Test folder permissions via the Google Drive API.
pub async fn test_folder_permissions(
    access_token: &str,
    folder_id: &str,
) -> Result<bool, ConnectorError> {
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

/// List top-level folders from Google Drive.
pub async fn list_drive_folders_api(
    access_token: &str,
) -> Result<Vec<DriveFolder>, ConnectorError> {
    list_drive_children_api(access_token, "root").await
}

/// List child folders of a given parent folder in Google Drive.
pub async fn list_drive_children_api(
    access_token: &str,
    parent_id: &str,
) -> Result<Vec<DriveFolder>, ConnectorError> {
    let client = reqwest::Client::new();

    let query = format!(
        "'{}' in parents and mimeType='application/vnd.google-apps.folder' and trashed=false",
        parent_id
    );

    let response = client
        .get("https://www.googleapis.com/drive/v3/files")
        .query(&[
            ("q", query.as_str()),
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

/// List both folders and Google Docs in a parent folder.
pub async fn list_drive_items_api(
    access_token: &str,
    parent_id: &str,
) -> Result<Vec<DriveItem>, ConnectorError> {
    let client = reqwest::Client::new();

    let query = format!(
        "'{}' in parents and trashed=false and (mimeType='application/vnd.google-apps.folder' or mimeType='application/vnd.google-apps.document')",
        parent_id
    );

    let response = client
        .get("https://www.googleapis.com/drive/v3/files")
        .query(&[
            ("q", query.as_str()),
            ("fields", "files(id,name,mimeType)"),
            ("pageSize", "100"),
            ("orderBy", "folder,name"),
        ])
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await?;

    let body: serde_json::Value = response.json().await?;

    let items: Vec<DriveItem> = body
        .get("files")
        .and_then(|f| f.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let name = item.get("name")?.as_str()?;
                    if name.starts_with('.') || name.starts_with('!') {
                        return None;
                    }
                    let mime = item.get("mimeType")?.as_str()?.to_string();
                    Some(DriveItem {
                        id: item.get("id")?.as_str()?.to_string(),
                        name: name.to_string(),
                        is_folder: mime == "application/vnd.google-apps.folder",
                        mime_type: mime,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_pkce_code_verifier_length() {
        let verifier = generate_code_verifier();
        assert!(verifier.len() >= 43);
        assert!(verifier.len() <= 128);
    }

    #[test]
    fn test_pkce_code_challenge_deterministic() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge1 = generate_code_challenge(verifier);
        let challenge2 = generate_code_challenge(verifier);
        assert_eq!(challenge1, challenge2);
        assert!(!challenge1.is_empty());
    }

    #[test]
    fn test_pkce_verifier_uniqueness() {
        let v1 = generate_code_verifier();
        let v2 = generate_code_verifier();
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_embedded_credentials_placeholder() {
        assert!(!GoogleDriveConnector::has_embedded_credentials());
    }

    #[test]
    fn test_load_embedded_credentials_placeholder() {
        let dir = TempDir::new().unwrap();
        let config = ConnectorConfig {
            id: "gd-embed".into(),
            connector_type: "google_drive".into(),
            display_name: "Test".into(),
            credentials: None,
            source_id: None,
            created_at: "2026-01-01".into(),
            last_sync_at: None,
        };
        let connector = GoogleDriveConnector::new(&config, dir.path()).unwrap();
        assert!(!connector.load_embedded_credentials());
    }

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
    fn test_google_drive_connector_new() {
        let dir = TempDir::new().unwrap();
        let config = ConnectorConfig {
            id: "gd-1".into(),
            connector_type: "google_drive".into(),
            display_name: "My Drive".into(),
            credentials: None,
            source_id: None,
            created_at: "2026-01-01".into(),
            last_sync_at: None,
        };
        let connector = GoogleDriveConnector::new(&config, dir.path()).unwrap();
        let info = connector.info();
        assert_eq!(info.connector_type, "google_drive");
        assert_eq!(info.display_name, "My Drive");
        assert_eq!(connector.auth_status(), AuthStatus::Disconnected);
    }

    #[test]
    fn test_google_drive_connector_with_oauth_config() {
        let dir = TempDir::new().unwrap();
        let oauth_cfg = OAuthConfig {
            client_id: "test_id".into(),
            client_secret: "test_secret".into(),
            ..OAuthConfig::default()
        };
        let config = ConnectorConfig {
            id: "gd-2".into(),
            connector_type: "google_drive".into(),
            display_name: "Test Drive".into(),
            credentials: Some(serde_json::to_string(&oauth_cfg).unwrap()),
            source_id: None,
            created_at: "2026-01-01".into(),
            last_sync_at: None,
        };
        let connector = GoogleDriveConnector::new(&config, dir.path()).unwrap();
        let loaded_config = connector.oauth_config().unwrap();
        assert_eq!(loaded_config.client_id, "test_id");
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
    fn test_parse_drive_folders_missing_fields() {
        let body = serde_json::json!({
            "files": [
                {"id": "1", "name": "Good", "mimeType": "application/vnd.google-apps.folder"},
                {"id": "2", "mimeType": "application/vnd.google-apps.folder"},
                {"name": "No ID", "mimeType": "application/vnd.google-apps.folder"},
                {"id": "4", "name": "Also Good", "mimeType": "application/vnd.google-apps.folder"},
            ]
        });
        let folders = parse_drive_folders(&body);
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].name, "Also Good");
        assert_eq!(folders[1].name, "Good");
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
    fn test_connector_disconnect() {
        let dir = TempDir::new().unwrap();
        let chalk_dir = dir.path().join("com.madison.chalk");
        fs::create_dir_all(&chalk_dir).unwrap();

        // Create a token file.
        let token_file = chalk_dir.join("oauth_tokens.json");
        fs::write(&token_file, "{}").unwrap();

        let config = ConnectorConfig {
            id: "gd-3".into(),
            connector_type: "google_drive".into(),
            display_name: "Test".into(),
            credentials: None,
            source_id: None,
            created_at: "2026-01-01".into(),
            last_sync_at: None,
        };
        let connector = GoogleDriveConnector::new(&config, dir.path()).unwrap();
        connector.disconnect().unwrap();
        assert!(!token_file.exists());
        assert_eq!(connector.auth_status(), AuthStatus::Disconnected);
    }

    #[test]
    fn test_auth_status_with_valid_tokens() {
        let dir = TempDir::new().unwrap();
        let chalk_dir = dir.path().join("com.madison.chalk");
        fs::create_dir_all(&chalk_dir).unwrap();

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

        let config = ConnectorConfig {
            id: "gd-4".into(),
            connector_type: "google_drive".into(),
            display_name: "Test".into(),
            credentials: None,
            source_id: None,
            created_at: "2026-01-01".into(),
            last_sync_at: None,
        };
        let connector = GoogleDriveConnector::new(&config, dir.path()).unwrap();
        assert_eq!(connector.auth_status(), AuthStatus::Connected);
    }

    #[test]
    fn test_auth_status_with_expired_tokens() {
        let dir = TempDir::new().unwrap();
        let chalk_dir = dir.path().join("com.madison.chalk");
        fs::create_dir_all(&chalk_dir).unwrap();

        let tokens = TokenStorage {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: Utc::now() - chrono::Duration::seconds(10),
            token_type: "Bearer".into(),
        };
        fs::write(
            chalk_dir.join("oauth_tokens.json"),
            serde_json::to_string(&tokens).unwrap(),
        )
        .unwrap();

        let config = ConnectorConfig {
            id: "gd-5".into(),
            connector_type: "google_drive".into(),
            display_name: "Test".into(),
            credentials: None,
            source_id: None,
            created_at: "2026-01-01".into(),
            last_sync_at: None,
        };
        let connector = GoogleDriveConnector::new(&config, dir.path()).unwrap();
        assert_eq!(connector.auth_status(), AuthStatus::Expired);
    }
}
