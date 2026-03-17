pub mod admin;
pub mod connectors;
pub mod database;
mod logging;
pub mod privacy;
pub mod safety;
pub mod sentry_integration;
pub mod shredder;
pub mod updater;

use std::sync::Mutex;

use connectors::dispatcher::ConnectorDispatcher;
use connectors::factory::ConnectorFactory;
use connectors::ConnectorConfig;
use database::Database;

pub struct AppState {
    pub dispatcher: Mutex<ConnectorDispatcher>,
    /// Data directory for connector file storage.
    pub data_dir: std::path::PathBuf,
    pub db: Database,
}

/// Tauri command: pipe frontend console errors to the backend structured log.
#[tauri::command]
fn log_frontend_error(message: String, source: Option<String>, line: Option<u32>) {
    tracing::error!(
        origin = "frontend",
        source = source.as_deref().unwrap_or("unknown"),
        line = line,
        "{message}"
    );
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Check if the privacy consent dialog has been shown.
#[tauri::command]
fn get_privacy_consent_status(state: tauri::State<AppState>) -> Result<serde_json::Value, String> {
    let shown = privacy::has_seen_consent(&state.db);
    let enabled = privacy::is_crash_reporting_enabled(&state.db);
    Ok(serde_json::json!({
        "consent_shown": shown,
        "crash_reporting_enabled": enabled
    }))
}

/// Save the user's privacy consent choice.
#[tauri::command]
fn save_privacy_consent(
    state: tauri::State<AppState>,
    consented: bool,
) -> Result<(), String> {
    privacy::save_consent(&state.db, consented)
}

/// Send a user-submitted crash report.
#[tauri::command]
fn send_crash_report(message: String) -> Result<(), String> {
    if message.trim().is_empty() {
        return Err("Report message cannot be empty".into());
    }
    sentry_integration::send_user_report(&message);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Hold the guard so logs flush on shutdown.
    let _log_guard = logging::init();

    let data_dir = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));

    // Initialize the connector dispatcher.
    let mut dispatcher = ConnectorDispatcher::new();

    // Create a default Google Drive connector (Phase 1 — single connector).
    // In the future, configs will be loaded from SQLite.
    let gd_config = ConnectorConfig {
        id: "google_drive_default".to_string(),
        connector_type: "google_drive".to_string(),
        display_name: "Google Drive".to_string(),
        credentials: None,
        source_id: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        last_sync_at: None,
    };

    match ConnectorFactory::create(&gd_config, &data_dir) {
        Ok(connector) => {
            dispatcher.register(connector);
            tracing::info!("Google Drive connector registered via factory");
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to create Google Drive connector");
        }
    }

    // Open the database
    let db_path = Database::default_path();
    let db = Database::open(&db_path).expect("failed to open database");

    // Check consent and conditionally init Sentry
    let consent = privacy::is_crash_reporting_enabled(&db);
    let _sentry_guard = sentry_integration::init_if_consented(consent);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            dispatcher: Mutex::new(dispatcher),
            data_dir: data_dir.clone(),
            db,
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            log_frontend_error,
            get_privacy_consent_status,
            save_privacy_consent,
            send_crash_report,
            connectors::commands::list_connectors,
            connectors::commands::list_connected_connectors,
            connectors::commands::disconnect_connector,
            admin::oauth::initialize_oauth,
            admin::oauth::has_embedded_credentials,
            admin::oauth::save_oauth_config,
            admin::oauth::get_authorization_url,
            admin::oauth::handle_oauth_callback,
            admin::oauth::test_folder_permissions_command,
            admin::oauth::check_onboarding_status,
            admin::oauth::list_drive_folders,
            admin::oauth::list_drive_subfolders,
            admin::oauth::list_drive_items,
            admin::oauth::select_single_document,
            admin::oauth::trigger_initial_shred,
            admin::oauth::list_scanned_documents,
            updater::check_for_update,
            updater::install_update,
        ])
        .setup(|app| {
            // Start periodic update checker in background.
            updater::spawn_update_checker(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
