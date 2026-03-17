use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

/// The GitHub Releases endpoint for update checks.
const UPDATE_ENDPOINT: &str =
    "https://github.com/wowc8/chalk/releases/latest/download/latest.json";

/// Status info returned to the frontend when checking for updates.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatus {
    pub available: bool,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub body: Option<String>,
}

/// Check for available updates and return status info.
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<UpdateStatus, String> {
    let current_version = app
        .config()
        .version
        .clone()
        .unwrap_or_else(|| "0.0.0".to_string());

    let updater = match app
        .updater_builder()
        .endpoints(vec![UPDATE_ENDPOINT.parse().expect("invalid update URL")])
        .map_err(|e| e.to_string())?
        .build()
    {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!(error = %e, "Updater not configured");
            return Ok(UpdateStatus {
                available: false,
                current_version,
                latest_version: None,
                body: None,
            });
        }
    };

    match updater.check().await {
        Ok(Some(update)) => Ok(UpdateStatus {
            available: true,
            current_version,
            latest_version: Some(update.version.clone()),
            body: update.body.clone(),
        }),
        Ok(None) => Ok(UpdateStatus {
            available: false,
            current_version,
            latest_version: None,
            body: None,
        }),
        Err(e) => {
            tracing::warn!(error = %e, "Update check failed");
            Ok(UpdateStatus {
                available: false,
                current_version,
                latest_version: None,
                body: None,
            })
        }
    }
}

/// Download and install an available update, then restart the app.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let updater = app
        .updater_builder()
        .endpoints(vec![UPDATE_ENDPOINT.parse().expect("invalid update URL")])
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;

    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No update available".to_string())?;

    tracing::info!(
        version = %update.version,
        "Downloading and installing update"
    );

    let mut downloaded: u64 = 0;
    let mut total: Option<u64> = None;

    update
        .download_and_install(
            |chunk_len, content_len| {
                downloaded += chunk_len as u64;
                if total.is_none() {
                    total = content_len;
                }
                if let Some(t) = total {
                    let pct = (downloaded as f64 / t as f64 * 100.0) as u32;
                    tracing::debug!(downloaded, total = t, percent = pct, "Download progress");
                }
            },
            || {
                tracing::info!("Download complete, restarting app");
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    app.restart();
}

/// Spawn a background task that checks for updates periodically.
pub fn spawn_update_checker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Initial check after a short delay to not block startup.
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        loop {
            tracing::debug!("Periodic update check");
            let updater = match app
                .updater_builder()
                .endpoints(vec![UPDATE_ENDPOINT.parse().expect("invalid update URL")])
                .and_then(|b| b.build().map_err(Into::into))
            {
                Ok(u) => u,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to build updater for periodic check");
                    tokio::time::sleep(std::time::Duration::from_secs(6 * 3600)).await;
                    continue;
                }
            };

            match updater.check().await {
                Ok(Some(update)) => {
                    tracing::info!(
                        version = %update.version,
                        "Update available (periodic check)"
                    );
                    // Emit event to frontend so it can show a banner.
                    let _ = app.emit("update-available", UpdateStatus {
                        available: true,
                        current_version: app
                            .config()
                            .version
                            .clone()
                            .unwrap_or_else(|| "0.0.0".to_string()),
                        latest_version: Some(update.version.clone()),
                        body: update.body.clone(),
                    });
                }
                Ok(None) => {
                    tracing::debug!("No update available");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Periodic update check failed");
                }
            }

            // Check every 6 hours.
            tokio::time::sleep(std::time::Duration::from_secs(6 * 3600)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_status_serialization() {
        let status = UpdateStatus {
            available: true,
            current_version: "0.1.0".to_string(),
            latest_version: Some("0.2.0".to_string()),
            body: Some("Bug fixes and improvements".to_string()),
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"available\":true"));
        assert!(json.contains("\"current_version\":\"0.1.0\""));
        assert!(json.contains("\"latest_version\":\"0.2.0\""));
        assert!(json.contains("Bug fixes"));
    }

    #[test]
    fn test_update_status_no_update() {
        let status = UpdateStatus {
            available: false,
            current_version: "0.1.0".to_string(),
            latest_version: None,
            body: None,
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"available\":false"));
        assert!(json.contains("\"latest_version\":null"));
    }

    #[test]
    fn test_update_endpoint_is_valid_url() {
        assert!(UPDATE_ENDPOINT.starts_with("https://"));
        assert!(UPDATE_ENDPOINT.contains("github.com"));
        assert!(UPDATE_ENDPOINT.ends_with("latest.json"));
    }
}
