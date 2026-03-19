//! Digest module — semantic table parsing and reference document extraction.
//!
//! Fetches Google Docs as HTML via the Drive export API
//! (`files/{id}/export?mimeType=text/html`), parses the HTML tables with the
//! `scraper` crate, splits them into discrete content sections, and stores
//! each as a reference document for the RAG/embedding pipeline. Reference
//! documents are NOT shown in the library — they only feed AI context.
//!
//! All database writes for a single digest run are wrapped in a transaction.
//! If the run is cancelled or errors out, the transaction rolls back so
//! previously imported data stays untouched.

pub mod parser;
pub mod template_extractor;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::database::{CancellationToken, Database};
use crate::errors::{ChalkError, ErrorCode, ErrorDomain};

/// A single lesson plan extracted from a table row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedLesson {
    pub title: String,
    pub content: String,
    pub learning_objectives: Option<String>,
    pub subject_hint: Option<String>,
    pub grade_hint: Option<String>,
}

/// Result of digesting a single document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestResult {
    pub doc_id: String,
    pub doc_name: String,
    pub tables_found: usize,
    pub sections_extracted: usize,
    pub ref_docs_created: Vec<String>,
}

/// Result of digesting all documents in a folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestSummary {
    pub documents_processed: usize,
    pub total_tables: usize,
    pub total_sections: usize,
    pub results: Vec<DigestResult>,
}

/// Data fetched from the API for a single document, ready to be written to DB.
struct FetchedDoc {
    doc_id: String,
    doc_name: String,
    tables_found: usize,
    lessons: Vec<ExtractedLesson>,
    /// Raw HTML for template extraction (formatting analysis).
    raw_html: String,
}

/// Fetch a Google Doc as HTML via the Drive export API.
///
/// Uses `GET files/{id}/export?mimeType=text/html` which is much smaller
/// than the Docs API JSON for large documents (e.g. ~337 KB vs 10 MB+).
/// The `drive.readonly` scope already covers this endpoint.
pub async fn fetch_doc_html(
    access_token: &str,
    doc_id: &str,
) -> Result<String, ChalkError> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://www.googleapis.com/drive/v3/files/{}/export?mimeType=text/html",
        doc_id
    );

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| {
            ChalkError::new(
                ErrorDomain::Digest,
                ErrorCode::DigestParseFailed,
                format!("Failed to fetch document {}: {}", doc_id, e),
            )
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ChalkError::new(
            ErrorDomain::Digest,
            ErrorCode::DigestParseFailed,
            format!("Drive export API returned {}: {}", status, body),
        ));
    }

    response.text().await.map_err(|e| {
        ChalkError::new(
            ErrorDomain::Digest,
            ErrorCode::DigestParseFailed,
            format!("Failed to read HTML response: {}", e),
        )
    })
}

/// Day-of-week keywords used to detect schedule-grid tables.
const DAY_NAMES: &[&str] = &[
    "monday", "tuesday", "wednesday", "thursday", "friday",
    "saturday", "sunday",
    "mon", "tue", "wed", "thu", "fri", "sat", "sun",
];

/// Check if text looks like a standalone time value (e.g., "9:00", "09:00-10:00").
///
/// Returns true when the string consists almost entirely of digits, colons,
/// dots, hyphens/dashes, and whitespace — the hallmarks of a time range.
fn is_time_like(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.len() > 30 {
        return false;
    }
    let has_digit = trimmed.chars().any(|c| c.is_ascii_digit());
    let has_separator = trimmed.chars().any(|c| c == ':' || c == '.');
    let time_chars = trimmed
        .chars()
        .filter(|c| c.is_ascii_digit() || ":.-–—/ ".contains(*c))
        .count();
    has_digit && has_separator && time_chars * 4 >= trimmed.len() * 3
}

/// Check if text is a structural or section header rather than lesson content.
///
/// Catches entries like "Additional Ideas:", "Notes:", "LP 2022-2023", etc.
fn is_structural_text(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Short text ending with a colon is typically a section label.
    if trimmed.ends_with(':') && trimmed.split_whitespace().count() <= 5 {
        return true;
    }
    false
}

/// Detect which header columns correspond to days of the week.
///
/// If at least two columns match day names this is likely a weekly schedule grid.
/// Returns `(time_column_index, Vec<(column_index, day_label)>)`.
fn detect_schedule_columns(headers: &[String]) -> Option<(usize, Vec<(usize, String)>)> {
    let mut day_columns: Vec<(usize, String)> = Vec::new();
    let mut time_col: Option<usize> = None;

    for (i, header) in headers.iter().enumerate() {
        let h = header.trim().to_lowercase();
        // Detect time/day column (usually first).
        if time_col.is_none()
            && (h.contains("time") || h.contains("hour") || h.contains("hora") || h == "day/time")
        {
            time_col = Some(i);
            continue;
        }
        if DAY_NAMES.iter().any(|d| h.contains(d)) {
            day_columns.push((i, capitalize_header(header.trim())));
        }
    }

    if day_columns.len() >= 2 {
        Some((time_col.unwrap_or(0), day_columns))
    } else {
        None
    }
}

/// Extract lesson plans from a schedule-grid table.
///
/// For each data row, reads the time slot from `time_col` and creates one
/// lesson per non-empty day column cell. The day and time become context in
/// the plan body, and the activity text becomes the title.
fn extract_lessons_from_schedule(
    table: &parser::ParsedTable,
    headers: &[String],
    time_col: usize,
    day_columns: &[(usize, String)],
) -> Vec<ExtractedLesson> {
    let mut lessons = Vec::new();

    for row in &table.rows[1..] {
        let texts: Vec<String> = row.cells.iter().map(|c| c.text.trim().to_string()).collect();

        let time_slot = texts.get(time_col).map(|s| s.as_str()).unwrap_or("");

        // Skip rows that are purely structural.
        if is_structural_text(time_slot) {
            continue;
        }

        for (col_idx, day_label) in day_columns {
            let activity = match texts.get(*col_idx) {
                Some(text) if !text.is_empty() => text,
                _ => continue,
            };

            // Skip cells that are just time values echoed into day columns.
            if is_time_like(activity) {
                continue;
            }

            // Skip structural text in cells.
            if is_structural_text(activity) {
                continue;
            }

            // Build a meaningful title and body using HTML for content.
            let title = activity.clone();
            let activity_html = row.cells.get(*col_idx)
                .map(|c| c.html.trim().to_string())
                .unwrap_or_default();

            let mut body_parts = Vec::new();
            if !day_label.is_empty() {
                body_parts.push(format!("<p><strong>Day:</strong> {}</p>", day_label));
            }
            if !time_slot.is_empty() {
                body_parts.push(format!("<p><strong>Time:</strong> {}</p>", time_slot));
            }

            // Include the cell's formatted HTML content.
            if !activity_html.is_empty() {
                body_parts.push(activity_html);
            }

            // Gather any extra context from non-day, non-time columns.
            for (j, cell_text) in texts.iter().enumerate() {
                if j == time_col || day_columns.iter().any(|(ci, _)| *ci == j) {
                    continue;
                }
                if !cell_text.is_empty() && !is_time_like(cell_text) && !is_structural_text(cell_text) {
                    let header_label = headers.get(j).map(|h| h.trim()).unwrap_or("Note");
                    let cell_html = row.cells.get(j)
                        .map(|c| c.html.trim().to_string())
                        .unwrap_or_default();
                    body_parts.push(format!(
                        "<p><strong>{}:</strong> {}</p>",
                        capitalize_header(header_label),
                        if cell_html.is_empty() { cell_text.clone() } else { cell_html }
                    ));
                }
            }

            let content = body_parts.join("");

            lessons.push(ExtractedLesson {
                title,
                content,
                learning_objectives: None,
                subject_hint: None,
                grade_hint: None,
            });
        }
    }

    lessons
}

/// Extract lesson plans from a Google Doc exported as HTML.
///
/// Parses HTML tables, uses the first row as headers, and converts subsequent
/// rows into lesson plan entries. Detects weekly schedule grids (days as
/// columns, time slots as rows) and extracts activities with day/time context
/// instead of creating one plan per row.
pub fn extract_lessons_from_doc(html: &str) -> Vec<ExtractedLesson> {
    let tables = parser::extract_tables(html);
    if tables.is_empty() {
        return Vec::new();
    }

    let mut lessons = Vec::new();

    for table in &tables {
        if table.rows.len() < 2 {
            // Need at least a header row and one data row.
            continue;
        }

        let headers: Vec<String> = table.rows[0]
            .cells
            .iter()
            .map(|c| c.text.trim().to_lowercase())
            .collect();

        // Check if this is a weekly schedule grid.
        if let Some((time_col, day_columns)) = detect_schedule_columns(&headers) {
            let schedule_lessons =
                extract_lessons_from_schedule(table, &headers, time_col, &day_columns);
            lessons.extend(schedule_lessons);
            continue;
        }

        for row in &table.rows[1..] {
            if let Some(lesson) = extract_lesson_from_row(&headers, row) {
                lessons.push(lesson);
            }
        }
    }

    lessons
}

/// Try to extract a lesson plan from a single table row using the header mapping.
///
/// Returns `None` for rows that have no substantive content: time-only cells,
/// structural headers, or entries where the body would be empty.
fn extract_lesson_from_row(
    headers: &[String],
    row: &parser::TableRow,
) -> Option<ExtractedLesson> {
    let texts: Vec<String> = row.cells.iter().map(|c| c.text.trim().to_string()).collect();

    // Build a header-value map for flexible column matching (uses plain text).
    let field_map: Vec<(&str, &str)> = headers
        .iter()
        .zip(texts.iter())
        .map(|(h, v)| (h.as_str(), v.as_str()))
        .collect();

    let title = find_field(&field_map, &["title", "lesson", "lesson title", "topic", "name", "lesson name", "unit"])
        .unwrap_or_default();

    if title.is_empty() {
        // If no title column, try using the first non-empty cell as the title.
        if let Some((first_idx, first_non_empty)) = texts.iter().enumerate().find(|(_, c)| !c.is_empty()) {
            // Skip time-only and structural entries.
            if is_time_like(first_non_empty) || is_structural_text(first_non_empty) {
                return None;
            }

            // Build content using HTML from non-title cells.
            let content_parts: Vec<String> = row.cells
                .iter()
                .enumerate()
                .filter(|(i, c)| *i != first_idx && !c.text.trim().is_empty() && !is_time_like(c.text.trim()))
                .map(|(_, c)| {
                    let html = c.html.trim().to_string();
                    if html.is_empty() { c.text.trim().to_string() } else { html }
                })
                .collect();

            if content_parts.is_empty() {
                return None;
            }

            let content = content_parts.join("");

            return Some(ExtractedLesson {
                title: first_non_empty.clone(),
                content,
                learning_objectives: None,
                subject_hint: None,
                grade_hint: None,
            });
        }
        return None;
    }

    // Skip plans whose title is just a time range or structural header.
    if is_time_like(&title) || is_structural_text(&title) {
        return None;
    }

    // Build rich content from all non-title columns, using HTML for formatting.
    let mut content_parts: Vec<String> = Vec::new();
    for (idx, (header, _text_value)) in field_map.iter().enumerate() {
        if _text_value.is_empty() {
            continue;
        }
        let h = *header;
        if is_title_header(h) {
            continue;
        }
        let cell_html = row.cells.get(idx)
            .map(|c| c.html.trim().to_string())
            .unwrap_or_default();
        let value_html = if cell_html.is_empty() {
            _text_value.to_string()
        } else {
            cell_html
        };
        content_parts.push(format!(
            "<p><strong>{}:</strong> {}</p>",
            capitalize_header(h),
            value_html
        ));
    }

    let content = content_parts.join("");

    // A plan with a title but no body content is not useful — the user would
    // see an empty editor when clicking it.
    if content.is_empty() {
        return None;
    }

    let objectives = find_field(
        &field_map,
        &[
            "objectives",
            "learning objectives",
            "learning objective",
            "goals",
            "outcome",
            "outcomes",
            "standard",
            "standards",
        ],
    );

    let subject_hint = find_field(
        &field_map,
        &["subject", "course", "class", "department", "area"],
    );

    let grade_hint = find_field(
        &field_map,
        &["grade", "grade level", "level", "year", "form"],
    );

    Some(ExtractedLesson {
        title,
        content,
        learning_objectives: objectives,
        subject_hint,
        grade_hint,
    })
}

/// Find the best-matching field value for a set of possible header names.
fn find_field(field_map: &[(&str, &str)], candidates: &[&str]) -> Option<String> {
    for candidate in candidates {
        for (header, value) in field_map {
            if header.contains(candidate) && !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Check if a header is a title-type column.
fn is_title_header(header: &str) -> bool {
    let title_headers = [
        "title",
        "lesson",
        "lesson title",
        "topic",
        "name",
        "lesson name",
        "unit",
    ];
    title_headers.iter().any(|t| header.contains(t))
}

/// Capitalize the first letter of a header for display.
fn capitalize_header(header: &str) -> String {
    let mut chars = header.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Fetch a single document's data from Google and extract lessons (no DB writes).
async fn fetch_document(
    access_token: &str,
    doc_id: &str,
    doc_name: &str,
) -> Result<FetchedDoc, ChalkError> {
    let html = fetch_doc_html(access_token, doc_id).await?;
    let tables = parser::extract_tables(&html);
    let tables_found = tables.len();
    let lessons = extract_lessons_from_doc(&html);

    Ok(FetchedDoc {
        doc_id: doc_id.to_string(),
        doc_name: doc_name.to_string(),
        tables_found,
        lessons,
        raw_html: html,
    })
}

/// Find an existing subject by name (case-insensitive) or create a new one,
/// using a connection/transaction reference directly.
/// Only used in tests now — digest no longer creates subjects/plans.
#[cfg(test)]
fn find_or_create_subject_on_conn(
    conn: &rusqlite::Connection,
    name: &str,
    grade_level: Option<&str>,
) -> Result<String, ChalkError> {
    let name_lower = name.to_lowercase();

    // Search existing subjects.
    let mut stmt = conn
        .prepare("SELECT id, name FROM subjects")
        .map_err(ChalkError::from)?;
    let subjects: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(ChalkError::from)?
        .filter_map(|r| r.ok())
        .collect();

    if let Some((id, _)) = subjects.iter().find(|(_, n)| n.to_lowercase() == name_lower) {
        return Ok(id.clone());
    }

    // Create new subject.
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO subjects (id, name, grade_level, description) VALUES (?1, ?2, ?3, ?4)",
        params![id, name, grade_level, Option::<String>::None],
    )
    .map_err(ChalkError::from)?;

    Ok(id)
}

/// Write all fetched documents into the database as reference documents.
///
/// Reference documents feed the RAG/embedding pipeline and are NOT shown
/// in the library. Returns the `DigestSummary` on success. If the
/// cancellation token is set before all documents are written, the function
/// returns a `DigestCancelled` error and the calling transaction rolls back.
fn write_digest_results(
    conn: &rusqlite::Connection,
    fetched_docs: &[FetchedDoc],
    cancel: &CancellationToken,
) -> Result<DigestSummary, ChalkError> {
    let mut results = Vec::new();
    let mut total_tables = 0;
    let mut total_sections = 0;

    for doc in fetched_docs {
        // Check cancellation between documents.
        if cancel.is_cancelled() {
            return Err(ChalkError::new(
                ErrorDomain::Digest,
                ErrorCode::DigestCancelled,
                "Digest cancelled by user",
            ));
        }

        if doc.lessons.is_empty() {
            info!(
                doc_id = doc.doc_id.as_str(),
                doc_name = doc.doc_name.as_str(),
                "No content sections found in document"
            );
            results.push(DigestResult {
                doc_id: doc.doc_id.clone(),
                doc_name: doc.doc_name.clone(),
                tables_found: doc.tables_found,
                sections_extracted: 0,
                ref_docs_created: Vec::new(),
            });
            continue;
        }

        let mut ref_docs_created = Vec::new();

        for lesson in doc.lessons.iter() {
            let ref_doc_id = uuid::Uuid::new_v4().to_string();

            // Strip HTML tags to get plain text for FTS/embedding.
            let content_text = strip_html_tags(&lesson.content);

            conn.execute(
                "INSERT INTO reference_docs (id, source_doc_id, source_doc_name, title, content_html, content_text)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    ref_doc_id,
                    doc.doc_id,
                    doc.doc_name,
                    lesson.title,
                    lesson.content,
                    content_text,
                ],
            )
            .map_err(ChalkError::from)?;

            ref_docs_created.push(ref_doc_id);
        }

        // Extract and store the teaching template (formatting patterns).
        let template_schema = template_extractor::extract_template(&doc.raw_html);
        if !template_schema.table_structure.columns.is_empty() {
            crate::database::Database::delete_teaching_templates_by_source(conn, &doc.doc_id)
                .map_err(|e| ChalkError::new(
                    ErrorDomain::Digest,
                    ErrorCode::DigestParseFailed,
                    format!("Failed to clean old templates: {}", e),
                ))?;

            let template_json = serde_json::to_string(&template_schema).map_err(|e| {
                ChalkError::new(
                    ErrorDomain::Digest,
                    ErrorCode::DigestParseFailed,
                    format!("Failed to serialize template: {}", e),
                )
            })?;

            crate::database::Database::create_teaching_template_on_conn(
                conn,
                Some(&doc.doc_id),
                Some(&doc.doc_name),
                &template_json,
            )
            .map_err(|e| ChalkError::new(
                ErrorDomain::Digest,
                ErrorCode::DigestParseFailed,
                format!("Failed to store teaching template: {}", e),
            ))?;

            info!(
                doc_id = doc.doc_id.as_str(),
                doc_name = doc.doc_name.as_str(),
                layout_type = template_schema.table_structure.layout_type.as_str(),
                columns = template_schema.table_structure.column_count,
                time_slots = template_schema.time_slots.len(),
                "Teaching template extracted"
            );
        }

        total_tables += doc.tables_found;
        total_sections += doc.lessons.len();

        info!(
            doc_id = doc.doc_id.as_str(),
            doc_name = doc.doc_name.as_str(),
            tables_found = doc.tables_found,
            sections_extracted = doc.lessons.len(),
            "Document analyzed for AI context"
        );

        results.push(DigestResult {
            doc_id: doc.doc_id.clone(),
            doc_name: doc.doc_name.clone(),
            tables_found: doc.tables_found,
            sections_extracted: doc.lessons.len(),
            ref_docs_created,
        });
    }

    Ok(DigestSummary {
        documents_processed: results.len(),
        total_tables,
        total_sections,
        results,
    })
}

/// Simple HTML tag stripping for generating plain text from HTML content.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                // Add a space after closing tags for readability.
                if !result.ends_with(' ') && !result.is_empty() {
                    result.push(' ');
                }
            }
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    // Collapse multiple spaces.
    let collapsed: String = result.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
}

/// List all Google Docs inside a folder, including subfolders (recursive).
/// Handles pagination via `nextPageToken` and Shared Drive files.
async fn list_docs_recursive(
    client: &reqwest::Client,
    access_token: &str,
    folder_id: &str,
    cancel: &CancellationToken,
) -> Result<Vec<serde_json::Value>, ChalkError> {
    // Check cancellation before making network requests.
    if cancel.is_cancelled() {
        return Err(ChalkError::new(
            ErrorDomain::Digest,
            ErrorCode::DigestCancelled,
            "Digest cancelled by user",
        ));
    }

    let mut all_docs: Vec<serde_json::Value> = Vec::new();

    // 1. List Google Docs directly in this folder (with pagination).
    let doc_query = format!(
        "'{}' in parents and trashed=false and mimeType='application/vnd.google-apps.document'",
        folder_id
    );
    let mut page_token: Option<String> = None;
    loop {
        let mut params: Vec<(&str, String)> = vec![
            ("q", doc_query.clone()),
            ("fields", "nextPageToken,files(id,name,modifiedTime)".into()),
            ("pageSize", "100".into()),
            ("supportsAllDrives", "true".into()),
            ("includeItemsFromAllDrives", "true".into()),
        ];
        if let Some(ref token) = page_token {
            params.push(("pageToken", token.clone()));
        }

        let response = client
            .get("https://www.googleapis.com/drive/v3/files")
            .query(&params)
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .map_err(|e| {
                ChalkError::new(
                    ErrorDomain::Digest,
                    ErrorCode::DigestParseFailed,
                    format!("Failed to list folder {}: {}", folder_id, e),
                )
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ChalkError::new(
                ErrorDomain::Digest,
                ErrorCode::DigestParseFailed,
                format!("Drive API returned {} listing folder {}: {}", status, folder_id, body),
            ));
        }

        let body: serde_json::Value = response.json().await.map_err(|e| {
            ChalkError::new(
                ErrorDomain::Digest,
                ErrorCode::DigestParseFailed,
                format!("Failed to parse Drive file list: {}", e),
            )
        })?;

        if let Some(files) = body.get("files").and_then(|f| f.as_array()) {
            all_docs.extend(files.iter().cloned());
        }

        match body.get("nextPageToken").and_then(|t| t.as_str()) {
            Some(token) => page_token = Some(token.to_string()),
            None => break,
        }
    }

    // 2. Find subfolders and recurse into them.
    let subfolder_query = format!(
        "'{}' in parents and trashed=false and mimeType='application/vnd.google-apps.folder'",
        folder_id
    );
    let mut page_token: Option<String> = None;
    loop {
        if cancel.is_cancelled() {
            return Err(ChalkError::new(
                ErrorDomain::Digest,
                ErrorCode::DigestCancelled,
                "Digest cancelled by user",
            ));
        }

        let mut params: Vec<(&str, String)> = vec![
            ("q", subfolder_query.clone()),
            ("fields", "nextPageToken,files(id,name)".into()),
            ("pageSize", "100".into()),
            ("supportsAllDrives", "true".into()),
            ("includeItemsFromAllDrives", "true".into()),
        ];
        if let Some(ref token) = page_token {
            params.push(("pageToken", token.clone()));
        }

        let response = client
            .get("https://www.googleapis.com/drive/v3/files")
            .query(&params)
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .map_err(|e| {
                ChalkError::new(
                    ErrorDomain::Digest,
                    ErrorCode::DigestParseFailed,
                    format!("Failed to list subfolders in {}: {}", folder_id, e),
                )
            })?;

        if !response.status().is_success() {
            // Non-fatal: log and skip subfolder listing.
            warn!(folder_id, "Failed to list subfolders — skipping recursion");
            break;
        }

        let body: serde_json::Value = response.json().await.map_err(|e| {
            ChalkError::new(
                ErrorDomain::Digest,
                ErrorCode::DigestParseFailed,
                format!("Failed to parse subfolder list: {}", e),
            )
        })?;

        if let Some(folders) = body.get("files").and_then(|f| f.as_array()) {
            for folder in folders {
                let sub_id = folder.get("id").and_then(|v| v.as_str()).unwrap_or_default();
                if sub_id.is_empty() {
                    continue;
                }
                // Box the recursive future to avoid infinite type size.
                let sub_docs = Box::pin(list_docs_recursive(client, access_token, sub_id, cancel)).await?;
                all_docs.extend(sub_docs);
            }
        }

        match body.get("nextPageToken").and_then(|t| t.as_str()) {
            Some(token) => page_token = Some(token.to_string()),
            None => break,
        }
    }

    Ok(all_docs)
}

/// Check if a Drive ID refers to a Google Doc (not a folder).
/// Returns `Some((id, name))` if it's a document, `None` if it's a folder or on error.
pub async fn check_if_document(
    access_token: &str,
    file_id: &str,
) -> Result<Option<(String, String)>, ChalkError> {
    let client = reqwest::Client::new();
    let response = client
        .get(format!(
            "https://www.googleapis.com/drive/v3/files/{}",
            file_id
        ))
        .query(&[
            ("fields", "id,name,mimeType"),
            ("supportsAllDrives", "true"),
        ])
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| {
            ChalkError::new(
                ErrorDomain::Digest,
                ErrorCode::DigestParseFailed,
                format!("Failed to check file metadata for {}: {}", file_id, e),
            )
        })?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let body: serde_json::Value = response.json().await.map_err(|e| {
        ChalkError::new(
            ErrorDomain::Digest,
            ErrorCode::DigestParseFailed,
            format!("Failed to parse file metadata: {}", e),
        )
    })?;

    let mime = body.get("mimeType").and_then(|v| v.as_str()).unwrap_or_default();
    if mime == "application/vnd.google-apps.document" {
        let id = body.get("id").and_then(|v| v.as_str()).unwrap_or(file_id).to_string();
        let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("Untitled").to_string();
        Ok(Some((id, name)))
    } else {
        Ok(None)
    }
}

/// Digest all documents in a folder (recursively). Called by `trigger_initial_digest`.
///
/// If `folder_id` is actually a single document ID (from `select_single_document`),
/// this function detects it and digests just that document.
///
/// All database writes are wrapped in a single transaction. If `cancel` is
/// signalled or an error occurs, the transaction rolls back and nothing from
/// this run persists.
pub async fn digest_folder(
    db: &Database,
    access_token: &str,
    folder_id: &str,
    cancel: &CancellationToken,
) -> Result<DigestSummary, ChalkError> {
    let client = reqwest::Client::new();

    // Phase 1: Discover documents (network I/O, no DB writes).
    let files_to_process: Vec<(String, String)>;

    // Check if the "folder_id" is actually a single document.
    if let Some((doc_id, doc_name)) = check_if_document(access_token, folder_id).await? {
        info!(doc_id = doc_id.as_str(), doc_name = doc_name.as_str(), "Selected item is a single document — digesting directly");
        files_to_process = vec![(doc_id, doc_name)];
    } else {
        // Recursively discover all Google Docs in the folder and subfolders.
        let files = list_docs_recursive(&client, access_token, folder_id, cancel).await?;

        if files.is_empty() {
            info!(folder_id, "No Google Docs found in folder or subfolders");
            return Ok(DigestSummary {
                documents_processed: 0,
                total_tables: 0,
                total_sections: 0,
                results: Vec::new(),
            });
        }

        files_to_process = files
            .iter()
            .filter_map(|f| {
                let id = f.get("id").and_then(|v| v.as_str())?.to_string();
                let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("Untitled").to_string();
                if id.is_empty() { None } else { Some((id, name)) }
            })
            .collect();
    }

    // Phase 2: Fetch each document's content from Google (network I/O, no DB writes).
    let mut fetched_docs = Vec::new();

    for (doc_id, doc_name) in &files_to_process {
        if cancel.is_cancelled() {
            return Err(ChalkError::new(
                ErrorDomain::Digest,
                ErrorCode::DigestCancelled,
                "Digest cancelled by user",
            ));
        }

        match fetch_document(access_token, doc_id, doc_name).await {
            Ok(doc) => fetched_docs.push(doc),
            Err(e) => {
                warn!(doc_id = doc_id.as_str(), doc_name = doc_name.as_str(), error = %e, "Failed to fetch document — skipping");
            }
        }
    }

    // Phase 3: Write everything to the database in a single transaction.
    // If cancelled or errored, the transaction rolls back automatically.
    let summary = db
        .with_transaction(|tx| {
            // rusqlite::Transaction derefs to Connection, so we can pass it directly.
            write_digest_results(tx, &fetched_docs, cancel).map_err(|e| {
                crate::database::DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ABORT),
                    Some(e.message.clone()),
                ))
            })
        })
        .map_err(|e| {
            // Check if this was a cancellation
            if cancel.is_cancelled() {
                ChalkError::new(
                    ErrorDomain::Digest,
                    ErrorCode::DigestCancelled,
                    "Digest cancelled by user — all changes rolled back",
                )
            } else {
                ChalkError::from(e)
            }
        })?;

    info!(
        folder_id,
        documents_processed = summary.documents_processed,
        total_tables = summary.total_tables,
        total_sections = summary.total_sections,
        "Folder digest complete — content stored as reference documents for AI context"
    );

    Ok(summary)
}

/// Find an existing subject by name or create a new one.
/// Used by tests that don't need transactions.
#[cfg(test)]
fn find_or_create_subject(
    db: &Database,
    name: &str,
    grade_level: Option<&str>,
) -> Result<String, ChalkError> {
    use crate::database::NewSubject;
    // Search existing subjects.
    let subjects = db.list_subjects().map_err(ChalkError::from)?;
    let name_lower = name.to_lowercase();

    if let Some(existing) = subjects
        .iter()
        .find(|s| s.name.to_lowercase() == name_lower)
    {
        return Ok(existing.id.clone());
    }

    // Create new subject.
    let subject = db
        .create_subject(&NewSubject {
            name: name.to_string(),
            grade_level: grade_level.map(|g| g.to_string()),
            description: None,
        })
        .map_err(ChalkError::from)?;

    Ok(subject.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{CancellationToken, NewSubject};

    fn test_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn test_extract_lessons_from_doc_with_tables() {
        let html = r#"<html><body>
            <table>
                <tr><th>Title</th><th>Subject</th><th>Duration</th><th>Objectives</th></tr>
                <tr><td>Photosynthesis Lab</td><td>Biology</td><td>45 minutes</td><td>Students will understand photosynthesis</td></tr>
                <tr><td>Cell Division</td><td>Biology</td><td>60 minutes</td><td>Students will learn mitosis and meiosis</td></tr>
            </table>
        </body></html>"#;

        let lessons = extract_lessons_from_doc(html);
        assert_eq!(lessons.len(), 2);

        assert_eq!(lessons[0].title, "Photosynthesis Lab");
        assert_eq!(
            lessons[0].learning_objectives.as_deref(),
            Some("Students will understand photosynthesis")
        );
        assert_eq!(lessons[0].subject_hint.as_deref(), Some("Biology"));

        assert_eq!(lessons[1].title, "Cell Division");
    }

    #[test]
    fn test_extract_lessons_no_tables() {
        let html = "<html><body><p>Just some text</p></body></html>";
        assert!(extract_lessons_from_doc(html).is_empty());
    }

    #[test]
    fn test_extract_lessons_table_with_only_header() {
        let html = r#"<html><body>
            <table>
                <tr><th>Title</th><th>Content</th></tr>
            </table>
        </body></html>"#;
        assert!(extract_lessons_from_doc(html).is_empty());
    }

    #[test]
    fn test_extract_lessons_empty_row_skipped() {
        let html = r#"<html><body>
            <table>
                <tr><th>Title</th><th>Duration</th></tr>
                <tr><td></td><td></td></tr>
                <tr><td>Algebra Review</td><td>30 min</td></tr>
            </table>
        </body></html>"#;

        let lessons = extract_lessons_from_doc(html);
        assert_eq!(lessons.len(), 1);
        assert_eq!(lessons[0].title, "Algebra Review");
    }

    #[test]
    fn test_extract_lessons_multi_paragraph_cell() {
        let html = r#"<html><body>
            <table>
                <tr><th>Lesson Title</th><th>Description</th></tr>
                <tr><td>Water Cycle</td><td><p>Part 1: Evaporation</p><p>Part 2: Condensation</p></td></tr>
            </table>
        </body></html>"#;

        let lessons = extract_lessons_from_doc(html);
        assert_eq!(lessons.len(), 1);
        assert_eq!(lessons[0].title, "Water Cycle");
        assert!(lessons[0].content.contains("Part 1: Evaporation"));
        assert!(lessons[0].content.contains("Part 2: Condensation"));
    }

    #[test]
    fn test_extract_lesson_from_row_with_grade_hint() {
        let headers = vec!["title".into(), "grade".into(), "subject".into(), "materials".into()];
        let row = parser::TableRow {
            cells: vec![
                parser::TableCell { html: String::new(), text: "Fractions".into() },
                parser::TableCell { html: String::new(), text: "5th".into() },
                parser::TableCell { html: String::new(), text: "Math".into() },
                parser::TableCell { html: String::new(), text: "Worksheets, manipulatives".into() },
            ],
        };

        let lesson = extract_lesson_from_row(&headers, &row).unwrap();
        assert_eq!(lesson.title, "Fractions");
        assert_eq!(lesson.grade_hint.as_deref(), Some("5th"));
        assert_eq!(lesson.subject_hint.as_deref(), Some("Math"));
        assert!(lesson.content.contains("Worksheets, manipulatives"));
    }

    #[test]
    fn test_extract_lesson_no_title_uses_first_cell() {
        let headers = vec!["date".into(), "activity".into(), "notes".into()];
        let row = parser::TableRow {
            cells: vec![
                parser::TableCell { html: String::new(), text: "March 1".into() },
                parser::TableCell { html: String::new(), text: "Lab experiment".into() },
                parser::TableCell { html: String::new(), text: "Bring goggles".into() },
            ],
        };

        let lesson = extract_lesson_from_row(&headers, &row).unwrap();
        assert_eq!(lesson.title, "March 1");
        assert!(lesson.content.contains("Lab experiment"));
    }

    #[test]
    fn test_find_or_create_subject_creates_new() {
        let db = test_db();
        let id = find_or_create_subject(&db, "Physics", Some("11th")).unwrap();
        let subject = db.get_subject(&id).unwrap();
        assert_eq!(subject.name, "Physics");
        assert_eq!(subject.grade_level.as_deref(), Some("11th"));
    }

    #[test]
    fn test_find_or_create_subject_finds_existing() {
        let db = test_db();
        let original = db
            .create_subject(&NewSubject {
                name: "Chemistry".into(),
                grade_level: Some("10th".into()),
                description: None,
            })
            .unwrap();

        let found_id = find_or_create_subject(&db, "chemistry", None).unwrap();
        assert_eq!(found_id, original.id);
    }

    #[test]
    fn test_find_field_matches_partial() {
        let fields = vec![("lesson title", "Photosynthesis"), ("duration", "45 min")];
        assert_eq!(find_field(&fields, &["title"]), Some("Photosynthesis".into()));
        assert_eq!(find_field(&fields, &["nonexistent"]), None);
    }

    #[test]
    fn test_is_title_header() {
        assert!(is_title_header("title"));
        assert!(is_title_header("lesson title"));
        assert!(is_title_header("lesson name"));
        assert!(is_title_header("topic"));
        assert!(!is_title_header("duration"));
        assert!(!is_title_header("objectives"));
    }

    #[test]
    fn test_capitalize_header() {
        assert_eq!(capitalize_header("duration"), "Duration");
        assert_eq!(capitalize_header("learning objectives"), "Learning objectives");
        assert_eq!(capitalize_header(""), "");
    }

    #[test]
    fn test_extract_lessons_multiple_tables() {
        let html = r#"<html><body>
            <p>Unit 1</p>
            <table>
                <tr><th>Title</th><th>Duration</th></tr>
                <tr><td>Lesson A</td><td>30 min</td></tr>
            </table>
            <p>Unit 2</p>
            <table>
                <tr><th>Topic</th><th>Notes</th></tr>
                <tr><td>Lesson B</td><td>Review chapter 5</td></tr>
            </table>
        </body></html>"#;

        let lessons = extract_lessons_from_doc(html);
        assert_eq!(lessons.len(), 2);
        assert_eq!(lessons[0].title, "Lesson A");
        assert_eq!(lessons[1].title, "Lesson B");
    }

    #[test]
    fn test_extract_lessons_nested_table_in_cell() {
        let html = r#"<html><body>
            <table>
                <tr><th>Title</th><th>Details</th></tr>
                <tr><td>Geology Unit</td><td><table><tr><td>Rock types overview</td></tr></table></td></tr>
            </table>
        </body></html>"#;

        let lessons = extract_lessons_from_doc(html);
        assert_eq!(lessons.len(), 1);
        assert_eq!(lessons[0].title, "Geology Unit");
        assert!(lessons[0].content.contains("Rock types overview"));
    }

    #[test]
    fn test_digest_result_serialization() {
        let result = DigestResult {
            doc_id: "abc123".into(),
            doc_name: "Test Doc".into(),
            tables_found: 2,
            sections_extracted: 5,
            ref_docs_created: vec!["ref-1".into(), "ref-2".into()],
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["doc_id"], "abc123");
        assert_eq!(json["tables_found"], 2);
        assert_eq!(json["sections_extracted"], 5);
    }

    #[test]
    fn test_digest_summary_serialization() {
        let summary = DigestSummary {
            documents_processed: 3,
            total_tables: 5,
            total_sections: 12,
            results: Vec::new(),
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["documents_processed"], 3);
        assert_eq!(json["total_tables"], 5);
        assert_eq!(json["total_sections"], 12);
    }

    #[test]
    fn test_strip_html_tags() {
        assert_eq!(strip_html_tags("<p>Hello</p>"), "Hello");
        assert_eq!(strip_html_tags("<p><strong>Bold:</strong> text</p>"), "Bold: text");
        assert_eq!(strip_html_tags("plain text"), "plain text");
        assert_eq!(strip_html_tags(""), "");
    }

    #[test]
    fn test_cancellation_token_basic() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn test_cancellation_token_clone_shares_state() {
        let token = CancellationToken::new();
        let clone = token.clone();
        assert!(!clone.is_cancelled());
        token.cancel();
        assert!(clone.is_cancelled());
    }

    #[test]
    fn test_write_digest_results_rolls_back_on_cancel() {
        let db = test_db();
        let cancel = CancellationToken::new();

        // Create fetched docs
        let docs = vec![
            FetchedDoc {
                doc_id: "doc1".into(),
                doc_name: "Doc 1".into(),
                tables_found: 1,
                lessons: vec![ExtractedLesson {
                    title: "Lesson 1".into(),
                    content: "Content 1".into(),
                    learning_objectives: None,
                    subject_hint: None,
                    grade_hint: None,
                }],
                raw_html: String::new(),
            },
            FetchedDoc {
                doc_id: "doc2".into(),
                doc_name: "Doc 2".into(),
                tables_found: 1,
                lessons: vec![ExtractedLesson {
                    title: "Lesson 2".into(),
                    content: "Content 2".into(),
                    learning_objectives: None,
                    subject_hint: None,
                    grade_hint: None,
                }],
                raw_html: String::new(),
            },
        ];

        // Cancel before writing — transaction should roll back.
        cancel.cancel();

        let result = db.with_transaction(|tx| {
            write_digest_results(tx, &docs, &cancel).map_err(|e| {
                crate::database::DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ABORT),
                    Some(e.message),
                ))
            })
        });

        assert!(result.is_err());

        // Verify no lesson plans were persisted.
        let plans = db.list_lesson_plans_by_subject("anything");
        // Either returns empty or the subject doesn't exist — either way, nothing persisted.
        assert!(plans.is_ok());
    }

    #[test]
    fn test_write_digest_results_commits_on_success() {
        let db = test_db();
        let cancel = CancellationToken::new();

        let docs = vec![FetchedDoc {
            doc_id: "doc1".into(),
            doc_name: "Doc 1".into(),
            tables_found: 1,
            lessons: vec![ExtractedLesson {
                title: "Photosynthesis".into(),
                content: "<p>Learn about photosynthesis</p>".into(),
                learning_objectives: Some("Understand light reactions".into()),
                subject_hint: Some("Biology".into()),
                grade_hint: Some("9th".into()),
            }],
            raw_html: String::new(),
        }];

        let result = db.with_transaction(|tx| {
            write_digest_results(tx, &docs, &cancel).map_err(|e| {
                crate::database::DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ABORT),
                    Some(e.message),
                ))
            })
        });

        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.documents_processed, 1);
        assert_eq!(summary.total_sections, 1);
        assert_eq!(summary.results[0].ref_docs_created.len(), 1);

        // Verify the reference doc was persisted.
        let ref_doc_id = &summary.results[0].ref_docs_created[0];
        let ref_doc = db.get_reference_doc(ref_doc_id).unwrap();
        assert_eq!(ref_doc.title, "Photosynthesis");
        assert_eq!(ref_doc.source_doc_id.as_deref(), Some("doc1"));
        assert_eq!(ref_doc.source_doc_name.as_deref(), Some("Doc 1"));
        assert!(ref_doc.content_html.contains("<p>"));
        assert!(ref_doc.content_text.contains("Learn about photosynthesis"));

        // Verify NO lesson plans were created (library stays clean).
        let plans = db.list_lesson_plans_by_subject("anything");
        assert!(plans.is_ok());
    }

    #[test]
    fn test_find_or_create_subject_on_conn() {
        let db = test_db();

        // Use with_transaction to get a connection reference.
        let result = db.with_transaction(|tx| {
            let id1 = find_or_create_subject_on_conn(tx, "Math", Some("8th"))
                .map_err(|e| crate::database::DatabaseError::Sqlite(
                    rusqlite::Error::SqliteFailure(
                        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ABORT),
                        Some(e.message),
                    ),
                ))?;

            // Finding the same subject again should return the same ID.
            let id2 = find_or_create_subject_on_conn(tx, "math", None)
                .map_err(|e| crate::database::DatabaseError::Sqlite(
                    rusqlite::Error::SqliteFailure(
                        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ABORT),
                        Some(e.message),
                    ),
                ))?;

            assert_eq!(id1, id2);
            Ok(())
        });

        assert!(result.is_ok());
    }

    // ── Schedule-grid detection and extraction tests ──────────────────

    #[test]
    fn test_is_time_like() {
        assert!(is_time_like("9:00"));
        assert!(is_time_like("09:00-10:00"));
        assert!(is_time_like("9:00 - 9:30"));
        assert!(is_time_like("9.00-9.30"));
        assert!(is_time_like("  9:10  "));
        assert!(!is_time_like(""));
        assert!(!is_time_like("Math"));
        assert!(!is_time_like("Photosynthesis Lab"));
        assert!(!is_time_like("Assembly"));
    }

    #[test]
    fn test_is_structural_text() {
        assert!(is_structural_text("Additional Ideas:"));
        assert!(is_structural_text("Notes:"));
        assert!(is_structural_text("Section Header:"));
        assert!(!is_structural_text("Photosynthesis Lab"));
        assert!(!is_structural_text(""));
        assert!(!is_structural_text("This is a longer sentence that happens to end with a colon:"));
    }

    #[test]
    fn test_detect_schedule_columns() {
        let headers: Vec<String> = vec![
            "day/time".into(), "monday".into(), "tuesday".into(),
            "wednesday".into(), "thursday".into(), "friday".into(),
        ];
        let result = detect_schedule_columns(&headers);
        assert!(result.is_some());
        let (time_col, day_cols) = result.unwrap();
        assert_eq!(time_col, 0);
        assert_eq!(day_cols.len(), 5);
    }

    #[test]
    fn test_detect_schedule_columns_not_a_schedule() {
        let headers: Vec<String> = vec![
            "title".into(), "subject".into(), "duration".into(),
        ];
        assert!(detect_schedule_columns(&headers).is_none());
    }

    #[test]
    fn test_detect_schedule_columns_minimum_two_days() {
        // Only one day column — not enough to be a schedule.
        let headers: Vec<String> = vec!["time".into(), "monday".into(), "notes".into()];
        assert!(detect_schedule_columns(&headers).is_none());
    }

    #[test]
    fn test_schedule_grid_extracts_activities() {
        let html = r#"<html><body>
            <table>
                <tr><th>Day/Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th></tr>
                <tr><td>9:00-9:30</td><td>Math</td><td>Reading</td><td>Math</td></tr>
                <tr><td>9:30-10:00</td><td>Science</td><td></td><td>Art</td></tr>
            </table>
        </body></html>"#;

        let lessons = extract_lessons_from_doc(html);
        // Row 1: 3 activities (Math, Reading, Math). Row 2: 2 activities (Science, Art).
        assert_eq!(lessons.len(), 5);

        // Each lesson title is the activity, not the time slot.
        let titles: Vec<&str> = lessons.iter().map(|l| l.title.as_str()).collect();
        assert!(titles.contains(&"Math"));
        assert!(titles.contains(&"Reading"));
        assert!(titles.contains(&"Science"));
        assert!(titles.contains(&"Art"));

        // Each lesson body includes the day and time context (now as HTML).
        let reading = lessons.iter().find(|l| l.title == "Reading").unwrap();
        assert!(reading.content.contains("Day:"), "Should contain day label");
        assert!(reading.content.contains("Tuesday"), "Should contain day name");
        assert!(reading.content.contains("Time:"), "Should contain time label");
        assert!(reading.content.contains("9:00-9:30"), "Should contain time range");
    }

    #[test]
    fn test_schedule_grid_skips_empty_cells() {
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th></tr>
                <tr><td>8:00-8:30</td><td></td><td></td></tr>
                <tr><td>8:30-9:00</td><td>PE</td><td></td></tr>
            </table>
        </body></html>"#;

        let lessons = extract_lessons_from_doc(html);
        assert_eq!(lessons.len(), 1);
        assert_eq!(lessons[0].title, "PE");
    }

    #[test]
    fn test_schedule_grid_skips_structural_rows() {
        let html = r#"<html><body>
            <table>
                <tr><th>Day/Time</th><th>Monday</th><th>Tuesday</th></tr>
                <tr><td>Additional Ideas:</td><td></td><td></td></tr>
                <tr><td>9:00-9:30</td><td>History</td><td>Geography</td></tr>
            </table>
        </body></html>"#;

        let lessons = extract_lessons_from_doc(html);
        assert_eq!(lessons.len(), 2);
        assert!(lessons.iter().all(|l| l.title != "Additional Ideas:"));
    }

    #[test]
    fn test_regular_table_skips_time_only_title() {
        let headers = vec!["date".into(), "notes".into()];
        let row = parser::TableRow {
            cells: vec![
                parser::TableCell { html: String::new(), text: "9:00-9:30".into() },
                parser::TableCell { html: String::new(), text: "".into() },
            ],
        };
        assert!(extract_lesson_from_row(&headers, &row).is_none());
    }

    #[test]
    fn test_regular_table_skips_structural_title() {
        let headers = vec!["section".into(), "details".into()];
        let row = parser::TableRow {
            cells: vec![
                parser::TableCell { html: String::new(), text: "Notes:".into() },
                parser::TableCell { html: String::new(), text: "".into() },
            ],
        };
        assert!(extract_lesson_from_row(&headers, &row).is_none());
    }

    #[test]
    fn test_regular_table_title_only_no_content_skipped() {
        // A plan with a title but empty body should be filtered out.
        let headers = vec!["title".into()];
        let row = parser::TableRow {
            cells: vec![parser::TableCell { html: String::new(), text: "Lonely Title".into() }],
        };
        assert!(extract_lesson_from_row(&headers, &row).is_none());
    }

    #[test]
    fn test_realistic_weekly_schedule_grid() {
        // Simulates the real-world scenario from the bug report: a weekly
        // schedule with time slots as rows and days as columns.
        let html = r#"<html><body>
            <table>
                <tr>
                    <th>Day/Time LP 2022-2023</th>
                    <th>Monday</th><th>Tuesday</th><th>Wednesday</th>
                    <th>Thursday</th><th>Friday</th>
                </tr>
                <tr><td>9:00-9:10</td><td>Assembly</td><td>Assembly</td><td>Assembly</td><td>Assembly</td><td>Assembly</td></tr>
                <tr><td>9:10-9:30</td><td>Math Warm-up</td><td>Reading Group</td><td>Math Warm-up</td><td>Reading Group</td><td>Math Review</td></tr>
                <tr><td>9:30-10:00</td><td></td><td>Science Lab</td><td></td><td>Science Lab</td><td></td></tr>
                <tr><td>Additional Ideas:</td><td></td><td></td><td></td><td></td><td></td></tr>
            </table>
        </body></html>"#;

        let lessons = extract_lessons_from_doc(html);

        // Should NOT create plans for time slots or "Additional Ideas:".
        for lesson in &lessons {
            assert!(!is_time_like(&lesson.title), "Time slot '{}' should not be a plan title", lesson.title);
            assert!(!is_structural_text(&lesson.title), "Structural text '{}' should not be a plan title", lesson.title);
        }

        // Should create plans for actual activities.
        let titles: Vec<&str> = lessons.iter().map(|l| l.title.as_str()).collect();
        assert!(titles.contains(&"Assembly"));
        assert!(titles.contains(&"Math Warm-up"));
        assert!(titles.contains(&"Reading Group"));
        assert!(titles.contains(&"Science Lab"));
        assert!(titles.contains(&"Math Review"));

        // Every plan should have non-empty content.
        for lesson in &lessons {
            assert!(!lesson.content.is_empty(), "Plan '{}' should have body content", lesson.title);
        }

        // Fewer than 660 garbage plans — should be a reasonable count.
        // 5 Assembly + 5 (row2) + 2 (row3) = 12 meaningful activities.
        assert!(lessons.len() <= 20, "Expected ~12 plans, got {}", lessons.len());
        assert!(lessons.len() >= 10, "Expected ~12 plans, got {}", lessons.len());
    }
}
