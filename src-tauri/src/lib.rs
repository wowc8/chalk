pub mod admin;
pub mod connectors;
pub mod database;
mod logging;
pub mod safety;
pub mod shredder;

use admin::oauth::OAuthClient;
use connectors::ConnectorDispatcher;
use std::sync::Mutex;

pub struct AppState {
    pub oauth_client: Mutex<OAuthClient>,
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Hold the guard so logs flush on shutdown.
    let _log_guard = logging::init();

    let data_dir = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let oauth_client = OAuthClient::new(&data_dir);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            oauth_client: Mutex::new(oauth_client),
        })
        .manage(ConnectorDispatcher::new())
        .invoke_handler(tauri::generate_handler![
            greet,
            log_frontend_error,
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
            connectors::dispatcher::list_connectors,
            connectors::dispatcher::get_connection_details,
            connectors::dispatcher::disconnect_connector,
            connectors::dispatcher::rescan_connector,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
