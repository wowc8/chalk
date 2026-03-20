pub mod admin;
pub mod backup;
pub mod cache;
pub mod chat;
pub mod connectors;
pub mod database;
pub mod errors;
pub mod events;
pub mod feature_flags;
pub mod library;
mod logging;
pub mod privacy;
pub mod rag;
pub mod safety;
pub mod sentry_integration;
pub mod digest;
pub mod updater;

use database::{CancellationToken, Database};
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
    /// Cancellation token for the digest operation.
    pub digest_cancel: Mutex<CancellationToken>,
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

// ── Teaching Template Tauri Commands ────────────────────────

/// Get the active teaching template (most recently extracted).
#[tauri::command]
fn get_active_teaching_template(
    state: tauri::State<AppState>,
) -> Result<serde_json::Value, String> {
    match state.db.get_active_teaching_template() {
        Ok(template) => {
            let schema: database::TeachingTemplateSchema =
                serde_json::from_str(&template.template_json).map_err(|e| format!("{e}"))?;
            Ok(serde_json::json!({
                "id": template.id,
                "source_doc_id": template.source_doc_id,
                "source_doc_name": template.source_doc_name,
                "template": schema,
                "created_at": template.created_at,
                "updated_at": template.updated_at,
            }))
        }
        Err(database::DatabaseError::NotFound) => {
            Ok(serde_json::json!(null))
        }
        Err(e) => Err(format!("{e}")),
    }
}

/// List all teaching templates.
#[tauri::command]
fn list_teaching_templates(
    state: tauri::State<AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let templates = state.db.list_teaching_templates().map_err(|e| format!("{e}"))?;
    templates
        .into_iter()
        .map(|t| {
            let schema: database::TeachingTemplateSchema =
                serde_json::from_str(&t.template_json).map_err(|e| format!("{e}"))?;
            Ok(serde_json::json!({
                "id": t.id,
                "source_doc_id": t.source_doc_id,
                "source_doc_name": t.source_doc_name,
                "template": schema,
                "created_at": t.created_at,
                "updated_at": t.updated_at,
            }))
        })
        .collect()
}

// ── LTP Document Tauri Commands ─────────────────────────────

/// Import a Long-Term Plan HTML document with duplicate detection.
///
/// Reads the file, computes SHA-256 hash, and either imports or skips.
#[tauri::command]
fn import_ltp_document(
    state: tauri::State<AppState>,
    path: String,
    school_year: Option<String>,
    doc_type: Option<String>,
) -> Result<serde_json::Value, String> {
    use sha2::{Digest, Sha256};

    let raw_html = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read file: {e}"))?;

    if raw_html.is_empty() {
        return Err("File is empty".into());
    }

    let filename = std::path::Path::new(&path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("imported.html")
        .to_string();

    let file_hash = format!("{:x}", Sha256::digest(raw_html.as_bytes()));
    let doc_type = doc_type.unwrap_or_else(|| "ltp".to_string());

    let result = state
        .db
        .import_ltp_document(
            &filename,
            &file_hash,
            school_year.as_deref(),
            &doc_type,
            &raw_html,
        )
        .map_err(|e| format!("{e}"))?;

    match result {
        database::LtpImportResult::Imported(doc) => {
            // Parse the LTP HTML and store grid cells.
            let parse_result = digest::ltp_parser::parse_ltp_html(&raw_html);
            let cells_stored = parse_result.cells.len();

            for cell in &parse_result.cells {
                if let Err(e) = state.db.insert_ltp_grid_cell(
                    &doc.id,
                    cell.row_index,
                    cell.col_index,
                    cell.subject.as_deref(),
                    cell.month.as_deref(),
                    cell.content_html.as_deref(),
                    cell.content_text.as_deref(),
                    cell.background_color.as_deref(),
                    cell.unit_name.as_deref(),
                    cell.unit_color.as_deref(),
                ) {
                    tracing::warn!(
                        row = cell.row_index,
                        col = cell.col_index,
                        error = %e,
                        "Failed to insert LTP grid cell"
                    );
                }
            }

            tracing::info!(
                filename = doc.filename.as_str(),
                doc_type = doc.doc_type.as_str(),
                cells_stored,
                months = parse_result.month_headers.len(),
                subjects = parse_result.subject_labels.len(),
                "LTP document imported and parsed"
            );
            Ok(serde_json::json!({
                "status": "imported",
                "id": doc.id,
                "filename": doc.filename,
                "doc_type": doc.doc_type,
                "school_year": doc.school_year,
                "cells_parsed": cells_stored,
                "months": parse_result.month_headers,
                "subjects": parse_result.subject_labels,
            }))
        }
        database::LtpImportResult::Skipped { id, filename } => {
            tracing::info!(
                filename = filename.as_str(),
                "LTP document skipped (unchanged)"
            );
            Ok(serde_json::json!({
                "status": "skipped",
                "id": id,
                "filename": filename,
            }))
        }
    }
}

/// List all imported LTP documents.
#[tauri::command]
fn list_ltp_documents(
    state: tauri::State<AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let docs = state.db.list_ltp_documents().map_err(|e| format!("{e}"))?;
    Ok(docs
        .into_iter()
        .map(|d| {
            serde_json::json!({
                "id": d.id,
                "filename": d.filename,
                "file_hash": d.file_hash,
                "school_year": d.school_year,
                "doc_type": d.doc_type,
                "imported_at": d.imported_at,
                "updated_at": d.updated_at,
            })
        })
        .collect())
}

/// Get grid cells for an LTP document.
#[tauri::command]
fn get_ltp_grid_cells(
    state: tauri::State<AppState>,
    document_id: String,
) -> Result<Vec<serde_json::Value>, String> {
    let cells = state
        .db
        .list_ltp_grid_cells(&document_id)
        .map_err(|e| format!("{e}"))?;
    Ok(cells
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "document_id": c.document_id,
                "row_index": c.row_index,
                "col_index": c.col_index,
                "subject": c.subject,
                "month": c.month,
                "content_html": c.content_html,
                "content_text": c.content_text,
                "background_color": c.background_color,
                "unit_name": c.unit_name,
                "unit_color": c.unit_color,
            })
        })
        .collect())
}

/// Update a single LTP grid cell's text content (inline editing).
#[tauri::command]
fn update_ltp_grid_cell(
    state: tauri::State<AppState>,
    cell_id: String,
    content_text: String,
) -> Result<serde_json::Value, String> {
    let cell = state
        .db
        .update_ltp_grid_cell(&cell_id, &content_text)
        .map_err(|e| format!("{e}"))?;
    Ok(serde_json::json!({
        "id": cell.id,
        "content_text": cell.content_text,
    }))
}

/// Get school calendar entries for an LTP document.
#[tauri::command]
fn get_school_calendar_entries(
    state: tauri::State<AppState>,
    document_id: String,
) -> Result<Vec<serde_json::Value>, String> {
    let entries = state
        .db
        .list_school_calendar_entries(&document_id)
        .map_err(|e| format!("{e}"))?;
    Ok(entries
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "document_id": e.document_id,
                "date": e.date,
                "day_number": e.day_number,
                "unit_name": e.unit_name,
                "unit_color": e.unit_color,
                "is_holiday": e.is_holiday,
                "holiday_name": e.holiday_name,
                "notes": e.notes,
            })
        })
        .collect())
}

/// Delete an imported LTP document and its associated grid cells/calendar entries.
#[tauri::command]
fn delete_ltp_document(
    state: tauri::State<AppState>,
    id: String,
) -> Result<(), String> {
    state
        .db
        .delete_ltp_document(&id)
        .map_err(|e| format!("{e}"))
}

// ── HTML File Import ────────────────────────────────────────

/// Import a local HTML file and extract a teaching template from it.
///
/// Runs the HTML through the same template extraction pipeline used by
/// Google Drive digest. Stores the extracted template in the database
/// and returns a summary of what was extracted.
#[tauri::command]
async fn import_html_file(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<serde_json::Value, String> {
    use tracing::info;

    // Read the HTML file from disk.
    let html = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read file: {e}"))?;

    if html.is_empty() {
        return Err("File is empty".into());
    }

    let file_name = std::path::Path::new(&path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("imported.html")
        .to_string();

    // Use a stable source_doc_id based on the file name so re-imports overwrite.
    let source_doc_id = format!("html-import:{}", file_name);

    // Try AI-assisted extraction if an API key is configured, otherwise heuristic.
    let ai_provider = digest::create_ai_provider_from_db(&state.db);
    let (schema, method) = match ai_provider {
        Some(provider) => {
            info!(file_name = file_name.as_str(), "Using AI to identify planning template table from HTML import");
            digest::template_extractor::extract_template_with_ai(&html, provider.as_ref()).await
        }
        None => {
            info!(file_name = file_name.as_str(), "No AI provider — using heuristic for HTML import");
            (digest::template_extractor::extract_template(&html), "heuristic")
        }
    };

    let template_json = serde_json::to_string(&schema)
        .map_err(|e| format!("Failed to serialize template: {e}"))?;

    // Delete any previous template from this same file, then store the new one.
    state.db.with_transaction(|conn| {
        database::Database::delete_teaching_templates_by_source(conn, &source_doc_id)
            .map_err(|e| database::DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ABORT),
                Some(format!("{e}")),
            )))?;
        database::Database::create_teaching_template_on_conn(
            conn,
            Some(&source_doc_id),
            Some(&file_name),
            &template_json,
        )
        .map_err(|e| database::DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ABORT),
            Some(format!("{e}")),
        )))?;
        Ok(())
    }).map_err(|e| format!("Failed to store template: {e}"))?;

    // Also extract lessons and store as reference docs for RAG context.
    let lessons = digest::extract_lessons_from_doc(&html);
    let ref_docs_created = if !lessons.is_empty() {
        state.db.with_transaction(|conn| {
            let mut created = Vec::new();
            for lesson in &lessons {
                let ref_doc_id = uuid::Uuid::new_v4().to_string();
                let content_text = digest::strip_html_tags(&lesson.content);
                conn.execute(
                    "INSERT INTO reference_docs (id, source_doc_id, source_doc_name, title, content_html, content_text)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        ref_doc_id,
                        source_doc_id,
                        file_name,
                        lesson.title,
                        lesson.content,
                        content_text,
                    ],
                )?;
                created.push(ref_doc_id);
            }
            Ok(created)
        }).map_err(|e| format!("Failed to store reference docs: {e}"))?
    } else {
        Vec::new()
    };

    info!(
        file_name = file_name.as_str(),
        method = method,
        time_slots = schema.time_slots.len(),
        daily_routine = schema.daily_routine.len(),
        colors = schema.color_scheme.mappings.len(),
        lessons = lessons.len(),
        ref_docs = ref_docs_created.len(),
        "HTML file imported successfully"
    );

    Ok(serde_json::json!({
        "file_name": file_name,
        "method": method,
        "time_slots": schema.time_slots.len(),
        "daily_routine_events": schema.daily_routine.len(),
        "colors": schema.color_scheme.mappings.len(),
        "lessons_extracted": lessons.len(),
        "ref_docs_created": ref_docs_created.len(),
        "template": schema,
    }))
}

// ── App Settings Tauri Commands ─────────────────────────────

#[tauri::command]
fn get_app_setting(state: tauri::State<AppState>, key: String) -> Result<Option<String>, String> {
    state.db.get_setting(&key).map_err(|e| format!("{e}"))
}

#[tauri::command]
fn set_app_setting(state: tauri::State<AppState>, key: String, value: String) -> Result<(), String> {
    state.db.set_setting(&key, &value).map_err(|e| format!("{e}"))
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

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .manage(AppState {
            dispatcher: Mutex::new(dispatcher),
            data_dir: data_dir.clone(),
            db,
            cache,
            digest_cancel: Mutex::new(CancellationToken::new()),
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            log_frontend_error,
            get_privacy_consent_status,
            save_privacy_consent,
            send_crash_report,
            get_app_setting,
            set_app_setting,
            connectors::commands::list_connectors,
            connectors::commands::list_connected_connectors,
            connectors::commands::disconnect_connector,
            admin::oauth::initialize_oauth,
            admin::oauth::has_embedded_credentials,
            admin::oauth::save_oauth_config,
            admin::oauth::get_authorization_url,
            admin::oauth::handle_oauth_callback,
            admin::oauth::start_oauth_flow,
            admin::oauth::test_folder_permissions_command,
            admin::oauth::check_onboarding_status,
            admin::oauth::list_drive_folders,
            admin::oauth::list_drive_subfolders,
            admin::oauth::list_drive_items,
            admin::oauth::select_single_document,
            admin::oauth::trigger_initial_digest,
            admin::oauth::cancel_digest,
            admin::oauth::list_scanned_documents,
            updater::check_for_update,
            updater::install_update,
            list_feature_flags,
            get_feature_flag,
            is_feature_enabled,
            set_feature_flag,
            toggle_feature_flag,
            import_ltp_document,
            list_ltp_documents,
            delete_ltp_document,
            get_ltp_grid_cells,
            update_ltp_grid_cell,
            get_school_calendar_entries,
            get_active_teaching_template,
            list_teaching_templates,
            import_html_file,
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
            library::search_plans_fts,
            library::search_plans_hybrid,
            library::create_plan,
            library::get_plan,
            library::update_plan_content,
            library::update_plan_title,
            library::finalize_plan,
            library::list_plan_versions,
            library::get_plan_version,
            library::revert_plan_version,
            library::delete_plan,
            chat::send_chat_message,
            chat::send_chat_message_stream,
            chat::get_chat_messages_cmd,
            chat::list_conversations,
            chat::delete_conversation,
            chat::vectorize_plan,
            chat::vectorize_all_plans,
            chat::save_ai_config,
            chat::get_ai_config,
            backup::export_backup,
            backup::import_backup,
            backup::get_backup_info,
        ])
        .setup(|_app| {
            // Start periodic update checker only in release builds.
            #[cfg(not(debug_assertions))]
            updater::spawn_update_checker(_app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
