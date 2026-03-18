use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::State;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::connectors::google_drive::{self, DriveFolder, DriveItem, GoogleDriveConnector, OAuthConfig};
use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OnboardingStatus {
    pub oauth_configured: bool,
    pub tokens_stored: bool,
    pub folder_selected: bool,
    pub folder_accessible: bool,
    pub initial_digest_complete: bool,
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
            initial_digest_complete: false,
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

/// Parse a query string parameter from a raw HTTP request line.
fn parse_query_param<'a>(request_line: &'a str, param: &str) -> Option<&'a str> {
    let query_start = request_line.find('?')?;
    let query_end = request_line[query_start..].find(' ').map(|i| query_start + i).unwrap_or(request_line.len());
    let query = &request_line[query_start + 1..query_end];
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            if key == param {
                return Some(value);
            }
        }
    }
    None
}

/// Complete OAuth flow: start a local server, open the browser, capture the
/// callback code automatically, and exchange it for tokens.
#[tauri::command]
pub async fn start_oauth_flow(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // 1. Bind to a random available port on localhost.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Failed to start local OAuth server: {e}"))?;
    let local_port = listener
        .local_addr()
        .map_err(|e| format!("Failed to get local address: {e}"))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{local_port}");

    info!(port = local_port, "OAuth callback server listening");

    // 2. Build the authorization URL with the dynamic redirect_uri.
    let (mut config, token_file, _) = get_gd_exchange_params(&state)?;
    if config.client_id.is_empty() {
        return Err("OAuth not configured — set client_id and client_secret first".into());
    }
    config.redirect_uri = redirect_uri.clone();

    let scopes = config.scopes.join(" ");
    let mut url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?\
         client_id={}&redirect_uri={}&response_type=code&\
         scope={}&access_type=offline&prompt=consent",
        config.client_id, redirect_uri, scopes
    );

    // Generate PKCE params when using embedded credentials or no secret.
    let pkce_verifier = if config.client_secret.is_empty() {
        let verifier = google_drive::generate_code_verifier();
        let challenge = google_drive::generate_code_challenge(&verifier);
        url.push_str(&format!(
            "&code_challenge={}&code_challenge_method=S256",
            challenge
        ));
        Some(verifier)
    } else {
        None
    };

    // 3. Open the authorization URL in the system browser.
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| format!("Failed to open browser: {e}"))?;

    // 4. Wait for the callback (with a 2-minute timeout).
    let (stream, _addr) = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        listener.accept(),
    )
    .await
    .map_err(|_| "OAuth timed out — no response received within 2 minutes".to_string())?
    .map_err(|e| format!("Failed to accept callback connection: {e}"))?;

    let (mut reader, mut writer) = tokio::io::split(stream);

    // 5. Read the HTTP request to extract the authorization code.
    let mut buf = vec![0u8; 4096];
    let n = reader
        .read(&mut buf)
        .await
        .map_err(|e| format!("Failed to read callback request: {e}"))?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Extract the first line: "GET /path?query HTTP/1.1"
    let request_line = request.lines().next().unwrap_or("");

    let code = parse_query_param(request_line, "code")
        .ok_or_else(|| {
            // Check for an error parameter from Google.
            let error = parse_query_param(request_line, "error")
                .unwrap_or("unknown");
            format!("OAuth denied or failed: {error}")
        })?
        .to_string();

    // URL-decode the code (replace %XX sequences).
    let code = percent_decode(&code);

    // 6. Send a success response page to the browser.
    let response_body = r#"<!DOCTYPE html>
<html><head><title>Chalk — Sign-in Complete</title>
<style>body{font-family:system-ui,sans-serif;display:flex;justify-content:center;align-items:center;min-height:100vh;margin:0;background:#1a1a2e;color:#e0e0e0}
.card{text-align:center;padding:2rem;border-radius:12px;background:#16213e;border:1px solid rgba(255,255,255,0.08)}
h1{margin:0 0 .5rem;font-size:1.5rem;color:#4fc3f7}p{margin:0;color:#aaa;font-size:.9rem}</style></head>
<body><div class="card"><h1>Sign-in complete</h1><p>You can close this tab and return to Chalk.</p></div></body></html>"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    let _ = writer.write_all(response.as_bytes()).await;
    let _ = writer.shutdown().await;

    // 7. Exchange the authorization code for tokens.
    google_drive::exchange_code(&config, &code, &token_file, pkce_verifier.as_deref())
        .await
        .map_err(|e| format!("Token exchange failed: {e}"))?;

    // 8. Update onboarding status.
    let mut status = load_onboarding_status(&state.data_dir);
    status.oauth_configured = true;
    status.tokens_stored = true;
    save_onboarding_status(&state.data_dir, &status)?;

    info!("OAuth flow completed automatically via localhost callback");
    Ok("Authentication successful".into())
}

/// Simple percent-decoding for URL query values.
fn percent_decode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push_str(&hex);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
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
pub async fn cancel_digest(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let cancel = state.digest_cancel.lock().map_err(|e| e.to_string())?;
    cancel.cancel();
    info!("Digest cancellation requested by user");
    Ok(())
}

#[tauri::command]
pub async fn trigger_initial_digest(
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

    // Reset and grab the cancellation token for this run.
    let cancel = {
        let mut guard = state.digest_cancel.lock().map_err(|e| e.to_string())?;
        *guard = crate::database::CancellationToken::new();
        guard.clone()
    };

    // Digest all documents in the selected folder (transactional).
    let summary = crate::digest::digest_folder(&state.db, &access_token, &folder_id, &cancel)
        .await
        .map_err(|e| e.to_string())?;

    info!(
        folder_id = folder_id.as_str(),
        documents_processed = summary.documents_processed,
        total_tables = summary.total_tables,
        total_lessons = summary.total_lessons,
        "Initial digest complete"
    );

    // Mark digest as complete.
    let mut updated_status = load_onboarding_status(&state.data_dir);
    updated_status.initial_digest_complete = true;
    save_onboarding_status(&state.data_dir, &updated_status)?;

    // Attempt to vectorize imported plans for RAG search.
    // If no OpenAI API key is configured, skip gracefully.
    let embeddings_skipped = if summary.total_lessons > 0 {
        match state.db.get_setting("openai_api_key") {
            Ok(Some(api_key)) if !api_key.is_empty() => {
                let client = crate::rag::embeddings::EmbeddingClient::new(api_key);
                let plans = state
                    .db
                    .list_plans_without_embeddings()
                    .unwrap_or_default();
                let mut vectorized = 0u32;
                for plan in &plans {
                    let text = crate::rag::chunker::create_embedding_text(
                        &plan.title,
                        &plan.content,
                        plan.learning_objectives.as_deref(),
                    );
                    match client.embed_one(&text).await {
                        Ok(embedding) => {
                            if state.db.upsert_embedding(&plan.id, &embedding).is_ok() {
                                vectorized += 1;
                            }
                        }
                        Err(e) => {
                            warn!(plan_id = %plan.id, error = %e, "Failed to vectorize plan — skipping");
                        }
                    }
                }
                info!(vectorized, "Post-digest vectorization complete");
                false
            }
            _ => {
                info!("No OpenAI API key configured — skipping embeddings. Configure in Settings.");
                true
            }
        }
    } else {
        false
    };

    let mut message = format!(
        "found {} document(s), extracted {} lesson plan(s)",
        summary.documents_processed, summary.total_lessons
    );
    if embeddings_skipped {
        message.push_str("|embeddings_skipped");
    }

    Ok(message)
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

    // Check if the stored ID is a single document rather than a folder.
    if let Ok(Some((doc_id, doc_name))) =
        crate::digest::check_if_document(&access_token, &folder_id).await
    {
        return Ok(vec![ScannedDocument {
            id: doc_id,
            name: doc_name,
            modified_time: None,
        }]);
    }

    // Recursively list all Google Docs in the folder and subfolders.
    let mut documents: Vec<ScannedDocument> = Vec::new();
    let mut folders_to_scan = vec![folder_id.clone()];

    while let Some(current_folder) = folders_to_scan.pop() {
        // List docs in this folder (with pagination).
        let doc_query = format!(
            "'{}' in parents and trashed=false and mimeType='application/vnd.google-apps.document'",
            current_folder
        );
        let mut page_token: Option<String> = None;
        loop {
            let mut params: Vec<(&str, String)> = vec![
                ("q", doc_query.clone()),
                ("fields", "nextPageToken,files(id,name,modifiedTime)".into()),
                ("pageSize", "100".into()),
                ("orderBy", "modifiedTime desc".into()),
                ("supportsAllDrives", "true".into()),
                ("includeItemsFromAllDrives", "true".into()),
            ];
            if let Some(ref token) = page_token {
                params.push(("pageToken", token.clone()));
            }

            let response = reqwest_client
                .get("https://www.googleapis.com/drive/v3/files")
                .query(&params)
                .header("Authorization", format!("Bearer {}", access_token))
                .send()
                .await
                .map_err(|e| e.to_string())?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(format!("Drive API returned {} listing folder: {}", status, body));
            }

            let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;

            if let Some(files) = body.get("files").and_then(|f| f.as_array()) {
                for item in files {
                    if let (Some(id), Some(name)) = (
                        item.get("id").and_then(|v| v.as_str()),
                        item.get("name").and_then(|v| v.as_str()),
                    ) {
                        documents.push(ScannedDocument {
                            id: id.to_string(),
                            name: name.to_string(),
                            modified_time: item
                                .get("modifiedTime")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                        });
                    }
                }
            }

            match body.get("nextPageToken").and_then(|t| t.as_str()) {
                Some(token) => page_token = Some(token.to_string()),
                None => break,
            }
        }

        // List subfolders to recurse into.
        let subfolder_query = format!(
            "'{}' in parents and trashed=false and mimeType='application/vnd.google-apps.folder'",
            current_folder
        );
        let mut page_token: Option<String> = None;
        loop {
            let mut params: Vec<(&str, String)> = vec![
                ("q", subfolder_query.clone()),
                ("fields", "nextPageToken,files(id)".into()),
                ("pageSize", "100".into()),
                ("supportsAllDrives", "true".into()),
                ("includeItemsFromAllDrives", "true".into()),
            ];
            if let Some(ref token) = page_token {
                params.push(("pageToken", token.clone()));
            }

            let response = reqwest_client
                .get("https://www.googleapis.com/drive/v3/files")
                .query(&params)
                .header("Authorization", format!("Bearer {}", access_token))
                .send()
                .await
                .map_err(|e| e.to_string())?;

            if !response.status().is_success() {
                break; // Non-fatal: skip subfolders on error.
            }

            let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;

            if let Some(folders) = body.get("files").and_then(|f| f.as_array()) {
                for folder in folders {
                    if let Some(sub_id) = folder.get("id").and_then(|v| v.as_str()) {
                        folders_to_scan.push(sub_id.to_string());
                    }
                }
            }

            match body.get("nextPageToken").and_then(|t| t.as_str()) {
                Some(token) => page_token = Some(token.to_string()),
                None => break,
            }
        }
    }

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
        assert!(!status.initial_digest_complete);
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
            initial_digest_complete: false,
            selected_folder_id: Some("folder_abc".into()),
            selected_folder_name: Some("My Lessons".into()),
        };
        save_onboarding_status(&data_dir, &status).unwrap();

        let loaded = load_onboarding_status(&data_dir);
        assert!(loaded.oauth_configured);
        assert!(loaded.tokens_stored);
        assert!(loaded.folder_selected);
        assert!(loaded.folder_accessible);
        assert!(!loaded.initial_digest_complete);
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

        status.initial_digest_complete = true;
        save_onboarding_status(&data_dir, &status).unwrap();

        let final_status = load_onboarding_status(&data_dir);
        assert!(final_status.oauth_configured);
        assert!(final_status.tokens_stored);
        assert!(final_status.folder_selected);
        assert!(final_status.folder_accessible);
        assert!(final_status.initial_digest_complete);
        assert_eq!(final_status.selected_folder_id, Some("folder_xyz".into()));
    }

    #[test]
    fn test_onboarding_status_default_trait() {
        let status = OnboardingStatus::default();
        assert!(!status.oauth_configured);
        assert!(!status.tokens_stored);
        assert!(!status.folder_selected);
        assert!(!status.folder_accessible);
        assert!(!status.initial_digest_complete);
        assert!(status.selected_folder_id.is_none());
        assert!(status.selected_folder_name.is_none());
    }

    #[test]
    fn test_parse_query_param_extracts_code() {
        let line = "GET /?code=4/0AQ_abc123&scope=read HTTP/1.1";
        assert_eq!(parse_query_param(line, "code"), Some("4/0AQ_abc123"));
        assert_eq!(parse_query_param(line, "scope"), Some("read"));
        assert_eq!(parse_query_param(line, "missing"), None);
    }

    #[test]
    fn test_parse_query_param_extracts_error() {
        let line = "GET /?error=access_denied HTTP/1.1";
        assert_eq!(parse_query_param(line, "error"), Some("access_denied"));
        assert_eq!(parse_query_param(line, "code"), None);
    }

    #[test]
    fn test_parse_query_param_no_query_string() {
        let line = "GET / HTTP/1.1";
        assert_eq!(parse_query_param(line, "code"), None);
    }

    #[test]
    fn test_percent_decode_basic() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("4%2F0AQ_abc"), "4/0AQ_abc");
        assert_eq!(percent_decode("no+encoding+here"), "no encoding here");
        assert_eq!(percent_decode("plain"), "plain");
    }

    #[test]
    fn test_percent_decode_empty() {
        assert_eq!(percent_decode(""), "");
    }

    #[test]
    fn test_percent_decode_special_chars() {
        assert_eq!(percent_decode("%3D"), "=");
        assert_eq!(percent_decode("%26"), "&");
        assert_eq!(percent_decode("a%2Fb%2Fc"), "a/b/c");
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

    #[test]
    fn test_onboarding_status_deserializes_with_missing_fields() {
        // Simulate an old status file that doesn't have newer fields
        // (e.g., folder_accessible, selected_folder_id, selected_folder_name).
        let old_json = r#"{
            "oauth_configured": true,
            "tokens_stored": true,
            "folder_selected": true,
            "initial_digest_complete": true
        }"#;
        let status: OnboardingStatus = serde_json::from_str(old_json).unwrap();
        assert!(status.oauth_configured);
        assert!(status.tokens_stored);
        assert!(status.folder_selected);
        assert!(status.initial_digest_complete);
        // Missing fields should get defaults
        assert!(!status.folder_accessible);
        assert!(status.selected_folder_id.is_none());
        assert!(status.selected_folder_name.is_none());
    }

    #[test]
    fn test_onboarding_status_survives_minimal_json() {
        // Even with just one field, deserialization should succeed
        let minimal_json = r#"{"oauth_configured": true}"#;
        let status: OnboardingStatus = serde_json::from_str(minimal_json).unwrap();
        assert!(status.oauth_configured);
        assert!(!status.tokens_stored);
        assert!(!status.folder_selected);
        assert!(!status.initial_digest_complete);
    }

    #[test]
    fn test_onboarding_status_empty_json_object() {
        let empty_json = "{}";
        let status: OnboardingStatus = serde_json::from_str(empty_json).unwrap();
        assert!(!status.oauth_configured);
        assert!(!status.tokens_stored);
        assert!(!status.folder_selected);
        assert!(!status.initial_digest_complete);
    }

    #[test]
    fn test_onboarding_status_persists_across_schema_changes() {
        let dir = TempDir::new().unwrap();
        let data_dir = setup_data_dir(&dir);

        // Write an "old" status file missing newer fields
        let old_json = r#"{
            "oauth_configured": true,
            "tokens_stored": true,
            "folder_selected": true,
            "initial_digest_complete": true
        }"#;
        let path = status_file_path(&data_dir);
        fs::write(&path, old_json).unwrap();

        // load_onboarding_status should still return the correct values
        let status = load_onboarding_status(&data_dir);
        assert!(status.oauth_configured);
        assert!(status.tokens_stored);
        assert!(status.folder_selected);
        assert!(status.initial_digest_complete);
        assert!(!status.folder_accessible); // default
    }
}
