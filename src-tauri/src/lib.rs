pub mod admin;
pub mod cache;
pub mod connectors;
pub mod database;
pub mod errors;
pub mod events;
pub mod feature_flags;
pub mod library;
mod logging;
pub mod privacy;
pub mod safety;
pub mod sentry_integration;
pub mod shredder;
pub mod updater;

use database::Database;
use feature_flags::{FeatureFlag, FeatureFlagInput};
use std::sync::Mutex;

use connectors::dispatcher::ConnectorDispatcher;
use connectors::factory::ConnectorFactory;
use connectors::ConnectorConfig;

pub struct AppState {
    pub dispatcher: Mutex<ConnectorDispatcher>,
    /// Data directory for connector file storage.
    pub data_dir: std::path::PathBuf,
    pub db: Database,
    pub cache: cache::Cache,
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

// ── Feature Flag Tauri Commands ──────────────────────────────

#[tauri::command]
fn list_feature_flags(state: tauri::State<'_, AppState>) -> Result<Vec<FeatureFlag>, String> {
    state.db.list_feature_flags().map_err(|e| e.message)
}

#[tauri::command]
fn get_feature_flag(state: tauri::State<'_, AppState>, name: String) -> Result<FeatureFlag, String> {
    state.db.get_feature_flag(&name).map_err(|e| e.message)
}

#[tauri::command]
fn is_feature_enabled(state: tauri::State<'_, AppState>, name: String) -> Result<bool, String> {
    state.db.is_flag_enabled(&name).map_err(|e| e.message)
}

#[tauri::command]
fn set_feature_flag(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    name: String,
    enabled: bool,
    description: Option<String>,
) -> Result<FeatureFlag, String> {
    let flag = state
        .db
        .set_feature_flag(&FeatureFlagInput {
            name: name.clone(),
            enabled,
            description,
        })
        .map_err(|e| e.message)?;

    // Emit event for frontend reactivity.
    events::emit_feature_flag_changed(
        &app,
        events::FeatureFlagChangedPayload {
            flag_name: name,
            enabled,
        },
    );

    Ok(flag)
}

#[tauri::command]
fn toggle_feature_flag(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<FeatureFlag, String> {
    let flag = state.db.toggle_feature_flag(&name).map_err(|e| e.message)?;

    events::emit_feature_flag_changed(
        &app,
        events::FeatureFlagChangedPayload {
            flag_name: name,
            enabled: flag.enabled,
        },
    );

    Ok(flag)
}

// ── Cache Tauri Commands ─────────────────────────────────────

#[tauri::command]
fn cache_get(state: tauri::State<'_, AppState>, key: String) -> Result<Option<String>, String> {
    state.cache.get(&key).map_err(|e| e.message)
}

#[tauri::command]
fn cache_clear(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.cache.clear().map_err(|e| e.message)?;

    events::emit_cache_invalidated(
        &app,
        events::CacheInvalidatedPayload {
            cache_key: "*".into(),
            reason: events::CacheInvalidationReason::ManualClear,
        },
    );

    Ok(())
}

#[tauri::command]
fn cache_stats(state: tauri::State<'_, AppState>) -> Result<cache::CacheStats, String> {
    state.cache.stats().map_err(|e| e.message)
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

    // Run the feature flags migration.
    db.with_conn(|conn| {
        // Check if migration 3 already applied.
        let version: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _migrations",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if version < 3 {
            conn.execute_batch(feature_flags::FEATURE_FLAGS_MIGRATION.2)?;
            conn.execute(
                "INSERT INTO _migrations (version, description) VALUES (?1, ?2)",
                rusqlite::params![3, "feature_flags"],
            )?;
        }
        Ok(())
    })
    .expect("failed to run feature flags migration");

    // Open the cache database (separate from main DB).
    let cache_path = data_dir.join("com.madison.chalk").join("cache.db");
    let cache = cache::Cache::open(&cache_path).expect("failed to open cache database");

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init());

    // Only register the updater plugin in release builds — it requires a
    // signing pubkey that isn't available during development.
    #[cfg(not(debug_assertions))]
    {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }

    builder
        .manage(AppState {
            dispatcher: Mutex::new(dispatcher),
            data_dir: data_dir.clone(),
            db,
            cache,
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
            list_feature_flags,
            get_feature_flag,
            is_feature_enabled,
            set_feature_flag,
            toggle_feature_flag,
            cache_get,
            cache_clear,
            cache_stats,
            library::create_tag,
            library::list_tags,
            library::update_tag,
            library::delete_tag,
            library::add_tag_to_plan,
            library::remove_tag_from_plan,
            library::get_tags_for_plan,
            library::list_library_plans,
            library::create_plan,
            library::delete_plan,
        ])
        .setup(|app| {
            // Start periodic update checker only in release builds.
            #[cfg(not(debug_assertions))]
            updater::spawn_update_checker(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
