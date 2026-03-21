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

/// Extract recurring schedule events from imported lesson plan templates.
///
/// Reads the active teaching template's `daily_routine` and converts each
/// `DailyRoutineEvent` into a draft event structure suitable for the frontend
/// wizard's Daily Schedule step. Returns an empty array if no template exists.
#[tauri::command]
fn extract_schedule_from_imports(
    state: tauri::State<AppState>,
) -> Result<serde_json::Value, String> {
    let template = match state.db.get_active_teaching_template() {
        Ok(t) => t,
        Err(database::DatabaseError::NotFound) => {
            return Ok(serde_json::json!([]));
        }
        Err(e) => return Err(format!("{e}")),
    };

    let schema: database::TeachingTemplateSchema =
        serde_json::from_str(&template.template_json).map_err(|e| format!("{e}"))?;

    let day_name_to_index = |name: &str| -> Option<i32> {
        match name.to_lowercase().as_str() {
            "monday" | "mon" => Some(0),
            "tuesday" | "tue" | "tues" => Some(1),
            "wednesday" | "wed" => Some(2),
            "thursday" | "thu" | "thur" | "thurs" => Some(3),
            "friday" | "fri" => Some(4),
            _ => None,
        }
    };

    // Parse time ranges like "11:30-12:00" or "11:30 AM - 12:00 PM" into (start, end) HH:MM pairs.
    let parse_time_range = |slot: &str| -> Option<(String, String)> {
        // Try splitting on "-" or "–" (en-dash)
        let parts: Vec<&str> = slot.split(|c| c == '-' || c == '–' || c == '—').collect();
        if parts.len() != 2 {
            return None;
        }
        let start = normalize_time(parts[0].trim());
        let end = normalize_time(parts[1].trim());
        if start.is_some() && end.is_some() {
            Some((start.unwrap(), end.unwrap()))
        } else {
            None
        }
    };

    fn normalize_time(t: &str) -> Option<String> {
        let t = t.trim().to_lowercase();
        // Remove "am"/"pm" suffixes and handle 12-hour format
        let (time_part, is_pm) = if t.ends_with("pm") || t.ends_with("p.m.") {
            (t.trim_end_matches("pm").trim_end_matches("p.m.").trim(), true)
        } else if t.ends_with("am") || t.ends_with("a.m.") {
            (t.trim_end_matches("am").trim_end_matches("a.m.").trim(), false)
        } else {
            (t.as_str(), false)
        };

        let parts: Vec<&str> = time_part.split(':').collect();
        match parts.len() {
            1 => {
                let hour: u32 = parts[0].parse().ok()?;
                let hour24 = if is_pm && hour < 12 { hour + 12 } else if !is_pm && hour == 12 { 0 } else { hour };
                Some(format!("{:02}:00", hour24))
            }
            2 => {
                let hour: u32 = parts[0].parse().ok()?;
                let min: u32 = parts[1].parse().ok()?;
                let hour24 = if is_pm && hour < 12 { hour + 12 } else if !is_pm && hour == 12 { 0 } else { hour };
                Some(format!("{:02}:{:02}", hour24, min))
            }
            _ => None,
        }
    }

    // ── Parse time slots and detect duration-like values ──────────────
    // Some LTP tables use relative offsets (e.g. "0:10-0:30") rather than
    // clock times.  Detect this and fall back to sensible school-day defaults.

    struct ParsedRoutine {
        start: String,
        end: String,
    }

    let parsed_times: Vec<ParsedRoutine> = schema
        .daily_routine
        .iter()
        .map(|routine| {
            if let Some(ref slot) = routine.time_slot {
                let (s, e) = parse_time_range(slot).unwrap_or_default();
                ParsedRoutine { start: s, end: e }
            } else {
                ParsedRoutine {
                    start: String::new(),
                    end: String::new(),
                }
            }
        })
        .collect();

    // Check whether the extracted times look like real school-day clock times.
    // A valid school time has hour ≥ 6 (6 AM) and hour ≤ 18 (6 PM).
    fn is_school_hour(time: &str) -> bool {
        if time.is_empty() {
            return false;
        }
        if let Some(hour_str) = time.split(':').next() {
            if let Ok(h) = hour_str.parse::<u32>() {
                return (6..=18).contains(&h);
            }
        }
        false
    }

    let valid_count = parsed_times
        .iter()
        .filter(|p| is_school_hour(&p.start))
        .count();
    let use_defaults = parsed_times.is_empty() || valid_count == 0;

    // When times are missing or are durations, assign sequential defaults
    // starting at 8:00 AM with 30-minute blocks.
    let default_times: Vec<(String, String)> = if use_defaults {
        let mut times = Vec::new();
        let mut current_minutes: u32 = 8 * 60; // 8:00 AM
        for _ in 0..schema.daily_routine.len() {
            let start_h = current_minutes / 60;
            let start_m = current_minutes % 60;
            let end_minutes = current_minutes + 30;
            let end_h = end_minutes / 60;
            let end_m = end_minutes % 60;
            times.push((
                format!("{:02}:{:02}", start_h, start_m),
                format!("{:02}:{:02}", end_h, end_m),
            ));
            current_minutes = end_minutes;
        }
        times
    } else {
        Vec::new()
    };

    // ── Build draft events with deduplication ─────────────────
    // Deduplicate by (lowercase name, start_time) so that multiple LTP
    // mentions of the same activity at the same time are merged into a
    // single event with the union of their day occurrences.
    use std::collections::HashMap;

    struct MergedEvent {
        name: String,
        event_type: &'static str,
        start_time: String,
        end_time: String,
        days: std::collections::BTreeSet<i32>,
    }

    let mut merged: HashMap<String, MergedEvent> = HashMap::new();
    // Preserve insertion order for stable output.
    let mut insertion_order: Vec<String> = Vec::new();

    for (idx, routine) in schema.daily_routine.iter().enumerate() {
        // Use parsed time if valid, otherwise fall back to defaults
        let (start_time, end_time) = if use_defaults {
            default_times
                .get(idx)
                .cloned()
                .unwrap_or_else(|| ("".to_string(), "".to_string()))
        } else {
            let p = &parsed_times[idx];
            if is_school_hour(&p.start) {
                (p.start.clone(), p.end.clone())
            } else {
                ("".to_string(), "".to_string())
            }
        };

        // Convert day names to day indices.
        // Only default to Mon-Fri for truly "Fixed" events (breakfast, lunch,
        // recess, dismissal). Variable/day-specific events with no days get
        // an empty set so the user can assign them in the review step.
        let day_indices: Vec<i32> = if routine.days.is_empty() {
            match routine.event_type {
                database::RoutineEventType::Fixed => vec![0, 1, 2, 3, 4],
                _ => Vec::new(),
            }
        } else {
            routine.days.iter().filter_map(|d| day_name_to_index(d)).collect()
        };

        let event_type = match routine.event_type {
            database::RoutineEventType::Fixed => "fixed",
            database::RoutineEventType::Variable => "teaching_slot",
            database::RoutineEventType::DaySpecific => "special",
        };

        // Dedup key: lowercase name + start time
        let dedup_key = format!("{}@{}", routine.name.to_lowercase(), start_time);

        if let Some(existing) = merged.get_mut(&dedup_key) {
            // Merge days into the existing event
            for d in &day_indices {
                existing.days.insert(*d);
            }
        } else {
            insertion_order.push(dedup_key.clone());
            merged.insert(dedup_key, MergedEvent {
                name: routine.name.clone(),
                event_type,
                start_time,
                end_time,
                days: day_indices.into_iter().collect(),
            });
        }
    }

    let draft_events: Vec<serde_json::Value> = insertion_order
        .iter()
        .enumerate()
        .filter_map(|(idx, key)| {
            let ev = merged.get(key)?;
            let occurrences: Vec<serde_json::Value> = ev
                .days
                .iter()
                .map(|&day| {
                    serde_json::json!({
                        "day_of_week": day,
                        "start_time": ev.start_time,
                        "end_time": ev.end_time,
                    })
                })
                .collect();

            Some(serde_json::json!({
                "id": format!("extracted-{}", idx),
                "name": ev.name,
                "event_type": ev.event_type,
                "occurrences": occurrences,
                "source": "ltp",
            }))
        })
        .collect();

    Ok(serde_json::json!(draft_events))
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

/// Import a Long-Term Plan from a Google Sheets URL.
///
/// Accepts a Google Sheets URL, converts it to an HTML export URL,
/// downloads the HTML, and runs it through the existing LTP parser.
#[tauri::command]
async fn import_ltp_from_url(
    state: tauri::State<'_, AppState>,
    url: String,
    school_year: Option<String>,
    doc_type: Option<String>,
) -> Result<serde_json::Value, String> {
    use sha2::{Digest, Sha256};

    // Extract sheet ID from Google Sheets URL and build export URL.
    let export_url = convert_sheets_url_to_export(&url)?;

    // Download the HTML.
    let client = reqwest::Client::new();
    let response = client
        .get(&export_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch URL: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "HTTP {} when fetching spreadsheet",
            response.status()
        ));
    }

    let raw_html = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    if raw_html.is_empty() {
        return Err("Downloaded HTML is empty".into());
    }

    // Derive a filename from the URL.
    let filename = extract_filename_from_url(&url);
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
                "LTP document imported from URL and parsed"
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
                "LTP document from URL skipped (unchanged)"
            );
            Ok(serde_json::json!({
                "status": "skipped",
                "id": id,
                "filename": filename,
            }))
        }
    }
}

/// Convert a Google Sheets URL to an HTML export URL.
///
/// Accepts URLs like:
///   https://docs.google.com/spreadsheets/d/SHEET_ID/edit...
///   https://docs.google.com/spreadsheets/d/SHEET_ID/...
/// Returns:
///   https://docs.google.com/spreadsheets/d/SHEET_ID/export?format=html
fn convert_sheets_url_to_export(url: &str) -> Result<String, String> {
    // Match the sheet ID from the URL path.
    let prefix = "/spreadsheets/d/";
    let start = url
        .find(prefix)
        .ok_or_else(|| "Not a valid Google Sheets URL. Expected a URL containing /spreadsheets/d/".to_string())?
        + prefix.len();

    let rest = &url[start..];
    let sheet_id = rest
        .split('/')
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Could not extract sheet ID from URL".to_string())?;

    Ok(format!(
        "https://docs.google.com/spreadsheets/d/{}/export?format=html",
        sheet_id
    ))
}

/// Extract a reasonable filename from a Google Sheets URL.
fn extract_filename_from_url(url: &str) -> String {
    let prefix = "/spreadsheets/d/";
    if let Some(start) = url.find(prefix) {
        let rest = &url[start + prefix.len()..];
        if let Some(sheet_id) = rest.split('/').next() {
            let short_id = if sheet_id.len() > 12 {
                &sheet_id[..12]
            } else {
                sheet_id
            };
            return format!("gsheet-{}.html", short_id);
        }
    }
    "url-import.html".to_string()
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

/// Get LTP context for a given date to inject into AI prompts.
///
/// Maps the date to a month, looks up LTP grid cells for that month,
/// and retrieves any nearby calendar entries (holidays, half days).
/// Returns structured data with unit info, subject content, and calendar notes.
#[tauri::command]
fn get_ltp_context_for_date(
    state: tauri::State<AppState>,
    date: String,
) -> Result<serde_json::Value, String> {
    let context = chat::get_ltp_context(&state.db, &date).map_err(|e| format!("{e}"))?;
    match context {
        Some(ctx) => Ok(serde_json::to_value(ctx).map_err(|e| format!("{e}"))?),
        None => Ok(serde_json::json!(null)),
    }
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

// ── Recurring Events Tauri Commands ──────────────────────────

#[tauri::command]
fn get_recurring_events(
    state: tauri::State<AppState>,
) -> Result<Vec<database::RecurringEvent>, String> {
    state.db.list_recurring_events().map_err(|e| format!("{e}"))
}

#[tauri::command]
fn create_recurring_event(
    state: tauri::State<AppState>,
    input: database::NewRecurringEvent,
) -> Result<database::RecurringEvent, String> {
    state
        .db
        .create_recurring_event(&input)
        .map_err(|e| format!("{e}"))
}

#[tauri::command]
fn update_recurring_event(
    state: tauri::State<AppState>,
    id: String,
    input: database::NewRecurringEvent,
) -> Result<database::RecurringEvent, String> {
    state
        .db
        .update_recurring_event(&id, &input)
        .map_err(|e| format!("{e}"))
}

#[tauri::command]
fn delete_recurring_event(
    state: tauri::State<AppState>,
    id: String,
) -> Result<(), String> {
    state
        .db
        .delete_recurring_event(&id)
        .map_err(|e| format!("{e}"))
}

// ── School Calendar Tauri Commands ──────────────────────────

#[tauri::command]
fn get_school_calendar(
    state: tauri::State<AppState>,
) -> Result<serde_json::Value, String> {
    match state.db.get_school_calendar() {
        Ok(cal) => serde_json::to_value(cal).map_err(|e| format!("{e}")),
        Err(database::DatabaseError::NotFound) => Ok(serde_json::json!(null)),
        Err(e) => Err(format!("{e}")),
    }
}

#[tauri::command]
fn update_school_calendar(
    state: tauri::State<AppState>,
    input: database::NewSchoolCalendar,
) -> Result<database::SchoolCalendar, String> {
    state
        .db
        .upsert_school_calendar(&input)
        .map_err(|e| format!("{e}"))
}

#[tauri::command]
fn add_calendar_exception(
    state: tauri::State<AppState>,
    input: database::NewCalendarException,
) -> Result<database::CalendarException, String> {
    state
        .db
        .add_calendar_exception(&input)
        .map_err(|e| format!("{e}"))
}

#[tauri::command]
fn delete_calendar_exception(
    state: tauri::State<AppState>,
    id: String,
) -> Result<(), String> {
    state
        .db
        .delete_calendar_exception(&id)
        .map_err(|e| format!("{e}"))
}

// ── Event Occurrence Tauri Commands ─────────────────────────

#[tauri::command]
fn create_event_occurrence(
    state: tauri::State<AppState>,
    input: database::NewEventOccurrence,
) -> Result<database::EventOccurrence, String> {
    state
        .db
        .create_event_occurrence(&input)
        .map_err(|e| format!("{e}"))
}

#[tauri::command]
fn list_event_occurrences(
    state: tauri::State<AppState>,
    event_id: String,
) -> Result<Vec<database::EventOccurrence>, String> {
    state
        .db
        .list_event_occurrences(&event_id)
        .map_err(|e| format!("{e}"))
}

#[tauri::command]
fn delete_event_occurrence(
    state: tauri::State<AppState>,
    id: String,
) -> Result<(), String> {
    state
        .db
        .delete_event_occurrence(&id)
        .map_err(|e| format!("{e}"))
}

#[tauri::command]
fn list_calendar_exceptions(
    state: tauri::State<AppState>,
    calendar_id: String,
) -> Result<Vec<database::CalendarException>, String> {
    state
        .db
        .list_calendar_exceptions(&calendar_id)
        .map_err(|e| format!("{e}"))
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
            import_ltp_from_url,
            list_ltp_documents,
            delete_ltp_document,
            get_ltp_grid_cells,
            update_ltp_grid_cell,
            get_school_calendar_entries,
            get_ltp_context_for_date,
            get_active_teaching_template,
            extract_schedule_from_imports,
            list_teaching_templates,
            import_html_file,
            get_recurring_events,
            create_recurring_event,
            update_recurring_event,
            delete_recurring_event,
            get_school_calendar,
            update_school_calendar,
            add_calendar_exception,
            delete_calendar_exception,
            create_event_occurrence,
            list_event_occurrences,
            delete_event_occurrence,
            list_calendar_exceptions,
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
            library::list_library_plans_chronological,
            library::update_plan_dates,
            library::duplicate_plan_as_template,
            chat::send_chat_message,
            chat::send_chat_message_stream,
            chat::get_chat_messages_cmd,
            chat::list_conversations,
            chat::delete_conversation,
            chat::vectorize_plan,
            chat::vectorize_all_plans,
            chat::save_ai_config,
            chat::get_ai_config,
            chat::validate_openai_key,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_sheets_url_edit() {
        let url = "https://docs.google.com/spreadsheets/d/1aBcDeFgHiJkLmNoPqRsTuVwXyZ/edit#gid=0";
        let result = convert_sheets_url_to_export(url).unwrap();
        assert_eq!(
            result,
            "https://docs.google.com/spreadsheets/d/1aBcDeFgHiJkLmNoPqRsTuVwXyZ/export?format=html"
        );
    }

    #[test]
    fn test_convert_sheets_url_bare() {
        let url = "https://docs.google.com/spreadsheets/d/ABCDEF123456/";
        let result = convert_sheets_url_to_export(url).unwrap();
        assert_eq!(
            result,
            "https://docs.google.com/spreadsheets/d/ABCDEF123456/export?format=html"
        );
    }

    #[test]
    fn test_convert_sheets_url_invalid() {
        let url = "https://example.com/not-a-spreadsheet";
        assert!(convert_sheets_url_to_export(url).is_err());
    }

    #[test]
    fn test_extract_filename_from_url_long_id() {
        let url = "https://docs.google.com/spreadsheets/d/1aBcDeFgHiJkLmNoPqRsTuVwXyZ/edit";
        let filename = extract_filename_from_url(url);
        assert_eq!(filename, "gsheet-1aBcDeFgHiJk.html");
    }

    #[test]
    fn test_extract_filename_from_url_short_id() {
        let url = "https://docs.google.com/spreadsheets/d/SHORT/edit";
        let filename = extract_filename_from_url(url);
        assert_eq!(filename, "gsheet-SHORT.html");
    }

    #[test]
    fn test_extract_filename_from_url_invalid() {
        let url = "https://example.com/whatever";
        let filename = extract_filename_from_url(url);
        assert_eq!(filename, "url-import.html");
    }
}
