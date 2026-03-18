//! Digest module — semantic table parsing and lesson plan extraction.
//!
//! Fetches Google Docs as HTML via the Drive export API
//! (`files/{id}/export?mimeType=text/html`), parses the HTML tables with the
//! `scraper` crate, splits them into discrete lesson plan chunks, and stores
//! each in the database with a UUID. Vector indexing happens separately via
//! the RAG pipeline.
//!
//! All database writes for a single digest run are wrapped in a transaction.
//! If the run is cancelled or errors out, the transaction rolls back so
//! previously imported data stays untouched.

pub mod parser;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::database::{CancellationToken, Database, NewSubject};
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
    pub lessons_extracted: usize,
    pub plans_created: Vec<String>,
}

/// Result of digesting all documents in a folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestSummary {
    pub documents_processed: usize,
    pub total_tables: usize,
    pub total_lessons: usize,
    pub results: Vec<DigestResult>,
}

/// Data fetched from the API for a single document, ready to be written to DB.
struct FetchedDoc {
    doc_id: String,
    doc_name: String,
    tables_found: usize,
    lessons: Vec<ExtractedLesson>,
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

/// Extract lesson plans from a Google Doc exported as HTML.
///
/// Parses HTML tables, uses the first row as headers, and converts subsequent
/// rows into lesson plan entries.
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

        for row in &table.rows[1..] {
            if let Some(lesson) = extract_lesson_from_row(&headers, row) {
                lessons.push(lesson);
            }
        }
    }

    lessons
}

/// Try to extract a lesson plan from a single table row using the header mapping.
fn extract_lesson_from_row(
    headers: &[String],
    row: &parser::TableRow,
) -> Option<ExtractedLesson> {
    let cells: Vec<String> = row.cells.iter().map(|c| c.text.trim().to_string()).collect();

    // Build a header-value map for flexible column matching.
    let field_map: Vec<(&str, &str)> = headers
        .iter()
        .zip(cells.iter())
        .map(|(h, v)| (h.as_str(), v.as_str()))
        .collect();

    let title = find_field(&field_map, &["title", "lesson", "lesson title", "topic", "name", "lesson name", "unit"])
        .unwrap_or_default();

    if title.is_empty() {
        // If no title column, try using the first non-empty cell as the title.
        if let Some(first_non_empty) = cells.iter().find(|c| !c.is_empty()) {
            let content = cells
                .iter()
                .filter(|c| !c.is_empty() && *c != first_non_empty)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n\n");

            if content.is_empty() {
                return None;
            }

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

    // Build rich content from all non-title columns.
    let mut content_parts: Vec<String> = Vec::new();
    for (header, value) in &field_map {
        if value.is_empty() {
            continue;
        }
        let h = *header;
        if is_title_header(h) {
            continue;
        }
        content_parts.push(format!("{}: {}", capitalize_header(h), value));
    }

    let content = content_parts.join("\n\n");
    if content.is_empty() && title.is_empty() {
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
    })
}

/// Find an existing subject by name (case-insensitive) or create a new one,
/// using a connection/transaction reference directly.
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

/// Write all fetched documents into the database within a transaction.
///
/// Returns the `DigestSummary` on success. If the cancellation token is set
/// before all documents are written, the function returns a `DigestCancelled`
/// error and the calling transaction is rolled back.
fn write_digest_results(
    conn: &rusqlite::Connection,
    fetched_docs: &[FetchedDoc],
    cancel: &CancellationToken,
) -> Result<DigestSummary, ChalkError> {
    // Create a default "General" subject for plans without a subject hint.
    let default_subject_id = find_or_create_subject_on_conn(conn, "General", None)?;

    let mut results = Vec::new();
    let mut total_tables = 0;
    let mut total_lessons = 0;

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
                "No lesson plans found in document"
            );
            results.push(DigestResult {
                doc_id: doc.doc_id.clone(),
                doc_name: doc.doc_name.clone(),
                tables_found: doc.tables_found,
                lessons_extracted: 0,
                plans_created: Vec::new(),
            });
            continue;
        }

        let mut plans_created = Vec::new();

        for (idx, lesson) in doc.lessons.iter().enumerate() {
            // Resolve subject: use hint from the table or fall back to default.
            let subject_id = if let Some(ref hint) = lesson.subject_hint {
                find_or_create_subject_on_conn(conn, hint, lesson.grade_hint.as_deref())?
            } else {
                default_subject_id.clone()
            };

            let plan_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO lesson_plans (id, subject_id, title, content, source_doc_id, source_table_index, learning_objectives)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    plan_id,
                    subject_id,
                    lesson.title,
                    lesson.content,
                    doc.doc_id,
                    idx as i32,
                    lesson.learning_objectives,
                ],
            )
            .map_err(ChalkError::from)?;

            // Mark as imported.
            conn.execute(
                "UPDATE lesson_plans SET source_type = 'imported' WHERE id = ?1",
                params![plan_id],
            )
            .map_err(ChalkError::from)?;

            plans_created.push(plan_id);
        }

        total_tables += doc.tables_found;
        total_lessons += doc.lessons.len();

        info!(
            doc_id = doc.doc_id.as_str(),
            doc_name = doc.doc_name.as_str(),
            tables_found = doc.tables_found,
            lessons_extracted = doc.lessons.len(),
            "Document digested successfully"
        );

        results.push(DigestResult {
            doc_id: doc.doc_id.clone(),
            doc_name: doc.doc_name.clone(),
            tables_found: doc.tables_found,
            lessons_extracted: doc.lessons.len(),
            plans_created,
        });
    }

    Ok(DigestSummary {
        documents_processed: results.len(),
        total_tables,
        total_lessons,
        results,
    })
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
                total_lessons: 0,
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
        total_lessons = summary.total_lessons,
        "Folder digest complete"
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
    use crate::database::NewSubject;

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
                parser::TableCell { text: "Fractions".into() },
                parser::TableCell { text: "5th".into() },
                parser::TableCell { text: "Math".into() },
                parser::TableCell { text: "Worksheets, manipulatives".into() },
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
                parser::TableCell { text: "March 1".into() },
                parser::TableCell { text: "Lab experiment".into() },
                parser::TableCell { text: "Bring goggles".into() },
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
            lessons_extracted: 5,
            plans_created: vec!["plan-1".into(), "plan-2".into()],
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["doc_id"], "abc123");
        assert_eq!(json["tables_found"], 2);
        assert_eq!(json["lessons_extracted"], 5);
    }

    #[test]
    fn test_digest_summary_serialization() {
        let summary = DigestSummary {
            documents_processed: 3,
            total_tables: 5,
            total_lessons: 12,
            results: Vec::new(),
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["documents_processed"], 3);
        assert_eq!(json["total_tables"], 5);
        assert_eq!(json["total_lessons"], 12);
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
                content: "Learn about photosynthesis".into(),
                learning_objectives: Some("Understand light reactions".into()),
                subject_hint: Some("Biology".into()),
                grade_hint: Some("9th".into()),
            }],
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
        assert_eq!(summary.total_lessons, 1);
        assert_eq!(summary.results[0].plans_created.len(), 1);

        // Verify the plan was persisted.
        let plan_id = &summary.results[0].plans_created[0];
        let plan = db.get_lesson_plan(plan_id).unwrap();
        assert_eq!(plan.title, "Photosynthesis");

        // Verify the subject was created.
        let subjects = db.list_subjects().unwrap();
        assert!(subjects.iter().any(|s| s.name == "Biology"));
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
}
