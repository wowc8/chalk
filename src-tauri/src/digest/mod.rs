//! Digest module — semantic table parsing and lesson plan extraction.
//!
//! Takes Google Docs JSON (from the Documents API), identifies table structures,
//! splits them into discrete lesson plan chunks, and stores each in the database
//! with a UUID. Vector indexing happens separately via the RAG pipeline.

pub mod parser;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::database::{Database, NewLessonPlan, NewSubject};
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

/// Fetch a Google Doc's structured JSON from the Documents API.
pub async fn fetch_doc_json(
    access_token: &str,
    doc_id: &str,
) -> Result<serde_json::Value, ChalkError> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://docs.googleapis.com/v1/documents/{}",
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
            format!("Google Docs API returned {}: {}", status, body),
        ));
    }

    response.json().await.map_err(|e| {
        ChalkError::new(
            ErrorDomain::Digest,
            ErrorCode::DigestParseFailed,
            format!("Failed to parse document JSON: {}", e),
        )
    })
}

/// Extract lesson plans from a Google Docs JSON document.
///
/// Parses the document body for table structures, uses the first row as headers,
/// and converts subsequent rows into lesson plan entries.
pub fn extract_lessons_from_doc(doc_json: &serde_json::Value) -> Vec<ExtractedLesson> {
    let tables = parser::extract_tables(doc_json);
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

/// Digest a single document: fetch its JSON, extract tables, store lesson plans.
pub async fn digest_document(
    db: &Database,
    access_token: &str,
    doc_id: &str,
    doc_name: &str,
    default_subject_id: &str,
) -> Result<DigestResult, ChalkError> {
    let doc_json = fetch_doc_json(access_token, doc_id).await?;

    let tables = parser::extract_tables(&doc_json);
    let tables_found = tables.len();

    let lessons = extract_lessons_from_doc(&doc_json);
    let lessons_extracted = lessons.len();

    if lessons.is_empty() {
        info!(doc_id, doc_name, "No lesson plans found in document");
        return Ok(DigestResult {
            doc_id: doc_id.to_string(),
            doc_name: doc_name.to_string(),
            tables_found,
            lessons_extracted: 0,
            plans_created: Vec::new(),
        });
    }

    let mut plans_created = Vec::new();

    for (idx, lesson) in lessons.iter().enumerate() {
        // Resolve subject: use hint from the table or fall back to default.
        let subject_id = if let Some(ref hint) = lesson.subject_hint {
            find_or_create_subject(db, hint, lesson.grade_hint.as_deref())?
        } else {
            default_subject_id.to_string()
        };

        let plan = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id,
                title: lesson.title.clone(),
                content: Some(lesson.content.clone()),
                source_doc_id: Some(doc_id.to_string()),
                source_table_index: Some(idx as i32),
                learning_objectives: lesson.learning_objectives.clone(),
            })
            .map_err(ChalkError::from)?;

        // Mark as imported.
        db.with_conn(|conn: &rusqlite::Connection| {
            conn.execute(
                "UPDATE lesson_plans SET source_type = 'imported' WHERE id = ?1",
                rusqlite::params![plan.id],
            )?;
            Ok(())
        })
        .map_err(ChalkError::from)?;

        plans_created.push(plan.id);
    }

    info!(
        doc_id,
        doc_name,
        tables_found,
        lessons_extracted,
        "Document digested successfully"
    );

    Ok(DigestResult {
        doc_id: doc_id.to_string(),
        doc_name: doc_name.to_string(),
        tables_found,
        lessons_extracted,
        plans_created,
    })
}

/// Find an existing subject by name or create a new one.
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

/// List all Google Docs inside a folder, including subfolders (recursive).
/// Handles pagination via `nextPageToken` and Shared Drive files.
async fn list_docs_recursive(
    client: &reqwest::Client,
    access_token: &str,
    folder_id: &str,
) -> Result<Vec<serde_json::Value>, ChalkError> {
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
                let sub_docs = Box::pin(list_docs_recursive(client, access_token, sub_id)).await?;
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
pub async fn digest_folder(
    db: &Database,
    access_token: &str,
    folder_id: &str,
) -> Result<DigestSummary, ChalkError> {
    let client = reqwest::Client::new();

    // Check if the "folder_id" is actually a single document (selected via select_single_document).
    if let Some((doc_id, doc_name)) = check_if_document(access_token, folder_id).await? {
        info!(doc_id = doc_id.as_str(), doc_name = doc_name.as_str(), "Selected item is a single document — digesting directly");
        let default_subject_id = find_or_create_subject(db, "General", None)?;
        return match digest_document(db, access_token, &doc_id, &doc_name, &default_subject_id).await {
            Ok(result) => Ok(DigestSummary {
                documents_processed: 1,
                total_tables: result.tables_found,
                total_lessons: result.lessons_extracted,
                results: vec![result],
            }),
            Err(e) => Err(e),
        };
    }

    // Recursively discover all Google Docs in the folder and subfolders.
    let files = list_docs_recursive(&client, access_token, folder_id).await?;

    if files.is_empty() {
        info!(folder_id, "No Google Docs found in folder or subfolders");
        return Ok(DigestSummary {
            documents_processed: 0,
            total_tables: 0,
            total_lessons: 0,
            results: Vec::new(),
        });
    }

    // Create a default "General" subject for plans without a subject hint.
    let default_subject_id =
        find_or_create_subject(db, "General", None)?;

    let mut results = Vec::new();
    let mut total_tables = 0;
    let mut total_lessons = 0;

    for file in &files {
        let doc_id = file
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let doc_name = file
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled");

        if doc_id.is_empty() {
            continue;
        }

        match digest_document(db, access_token, doc_id, doc_name, &default_subject_id).await {
            Ok(result) => {
                total_tables += result.tables_found;
                total_lessons += result.lessons_extracted;
                results.push(result);
            }
            Err(e) => {
                warn!(doc_id, doc_name, error = %e, "Failed to digest document — skipping");
            }
        }
    }

    info!(
        folder_id,
        documents_processed = results.len(),
        total_tables,
        total_lessons,
        "Folder digest complete"
    );

    Ok(DigestSummary {
        documents_processed: results.len(),
        total_tables,
        total_lessons,
        results,
    })
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
        let doc_json = serde_json::json!({
            "title": "Q1 Lesson Plans",
            "body": {
                "content": [
                    {
                        "table": {
                            "rows": 3,
                            "columns": 4,
                            "tableRows": [
                                {
                                    "tableCells": [
                                        {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Title\n"}}]}}]},
                                        {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Subject\n"}}]}}]},
                                        {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Duration\n"}}]}}]},
                                        {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Objectives\n"}}]}}]}
                                    ]
                                },
                                {
                                    "tableCells": [
                                        {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Photosynthesis Lab\n"}}]}}]},
                                        {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Biology\n"}}]}}]},
                                        {"content": [{"paragraph": {"elements": [{"textRun": {"content": "45 minutes\n"}}]}}]},
                                        {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Students will understand photosynthesis\n"}}]}}]}
                                    ]
                                },
                                {
                                    "tableCells": [
                                        {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Cell Division\n"}}]}}]},
                                        {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Biology\n"}}]}}]},
                                        {"content": [{"paragraph": {"elements": [{"textRun": {"content": "60 minutes\n"}}]}}]},
                                        {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Students will learn mitosis and meiosis\n"}}]}}]}
                                    ]
                                }
                            ]
                        }
                    }
                ]
            }
        });

        let lessons = extract_lessons_from_doc(&doc_json);
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
        let doc_json = serde_json::json!({
            "title": "Notes",
            "body": {
                "content": [
                    {"paragraph": {"elements": [{"textRun": {"content": "Just some text\n"}}]}}
                ]
            }
        });
        assert!(extract_lessons_from_doc(&doc_json).is_empty());
    }

    #[test]
    fn test_extract_lessons_table_with_only_header() {
        let doc_json = serde_json::json!({
            "title": "Empty Table",
            "body": {
                "content": [{
                    "table": {
                        "rows": 1, "columns": 2,
                        "tableRows": [{
                            "tableCells": [
                                {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Title\n"}}]}}]},
                                {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Content\n"}}]}}]}
                            ]
                        }]
                    }
                }]
            }
        });
        assert!(extract_lessons_from_doc(&doc_json).is_empty());
    }

    #[test]
    fn test_extract_lessons_empty_row_skipped() {
        let doc_json = serde_json::json!({
            "title": "With Empty Row",
            "body": {
                "content": [{
                    "table": {
                        "rows": 3, "columns": 2,
                        "tableRows": [
                            {"tableCells": [
                                {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Title\n"}}]}}]},
                                {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Duration\n"}}]}}]}
                            ]},
                            {"tableCells": [
                                {"content": [{"paragraph": {"elements": [{"textRun": {"content": "\n"}}]}}]},
                                {"content": [{"paragraph": {"elements": [{"textRun": {"content": "\n"}}]}}]}
                            ]},
                            {"tableCells": [
                                {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Algebra Review\n"}}]}}]},
                                {"content": [{"paragraph": {"elements": [{"textRun": {"content": "30 min\n"}}]}}]}
                            ]}
                        ]
                    }
                }]
            }
        });

        let lessons = extract_lessons_from_doc(&doc_json);
        assert_eq!(lessons.len(), 1);
        assert_eq!(lessons[0].title, "Algebra Review");
    }

    #[test]
    fn test_extract_lessons_multi_paragraph_cell() {
        let doc_json = serde_json::json!({
            "title": "Multi Paragraph",
            "body": {
                "content": [{
                    "table": {
                        "rows": 2, "columns": 2,
                        "tableRows": [
                            {"tableCells": [
                                {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Lesson Title\n"}}]}}]},
                                {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Description\n"}}]}}]}
                            ]},
                            {"tableCells": [
                                {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Water Cycle\n"}}]}}]},
                                {"content": [
                                    {"paragraph": {"elements": [{"textRun": {"content": "Part 1: Evaporation\n"}}]}},
                                    {"paragraph": {"elements": [{"textRun": {"content": "Part 2: Condensation\n"}}]}}
                                ]}
                            ]}
                        ]
                    }
                }]
            }
        });

        let lessons = extract_lessons_from_doc(&doc_json);
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
        let doc_json = serde_json::json!({
            "title": "Multi-Table Doc",
            "body": {
                "content": [
                    {"paragraph": {"elements": [{"textRun": {"content": "Unit 1\n"}}]}},
                    {"table": {"rows": 2, "columns": 2, "tableRows": [
                        {"tableCells": [
                            {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Title\n"}}]}}]},
                            {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Duration\n"}}]}}]}
                        ]},
                        {"tableCells": [
                            {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Lesson A\n"}}]}}]},
                            {"content": [{"paragraph": {"elements": [{"textRun": {"content": "30 min\n"}}]}}]}
                        ]}
                    ]}},
                    {"paragraph": {"elements": [{"textRun": {"content": "Unit 2\n"}}]}},
                    {"table": {"rows": 2, "columns": 2, "tableRows": [
                        {"tableCells": [
                            {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Topic\n"}}]}}]},
                            {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Notes\n"}}]}}]}
                        ]},
                        {"tableCells": [
                            {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Lesson B\n"}}]}}]},
                            {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Review chapter 5\n"}}]}}]}
                        ]}
                    ]}}
                ]
            }
        });

        let lessons = extract_lessons_from_doc(&doc_json);
        assert_eq!(lessons.len(), 2);
        assert_eq!(lessons[0].title, "Lesson A");
        assert_eq!(lessons[1].title, "Lesson B");
    }

    #[test]
    fn test_extract_lessons_nested_table_in_cell() {
        let doc_json = serde_json::json!({
            "title": "Nested Table Doc",
            "body": {
                "content": [{
                    "table": {"rows": 2, "columns": 2, "tableRows": [
                        {"tableCells": [
                            {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Title\n"}}]}}]},
                            {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Details\n"}}]}}]}
                        ]},
                        {"tableCells": [
                            {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Geology Unit\n"}}]}}]},
                            {"content": [{"table": {"rows": 1, "columns": 1, "tableRows": [
                                {"tableCells": [
                                    {"content": [{"paragraph": {"elements": [{"textRun": {"content": "Rock types overview\n"}}]}}]}
                                ]}
                            ]}}]}
                        ]}
                    ]}
                }]
            }
        });

        let lessons = extract_lessons_from_doc(&doc_json);
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
}
