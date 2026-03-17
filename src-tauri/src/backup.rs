//! Library backup/restore — full app state export to `.chalk-backup.zip`
//! and import with auto-backup of existing data before replacing.
//!
//! Exports all user data (lesson plans, tags, assignments, chat history,
//! settings, feature flags) as JSON inside a zip. Vector embeddings are
//! intentionally excluded and re-generated on import.

use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::io::{Read as IoRead, Write as IoWrite};
use std::path::{Path, PathBuf};

use crate::database::Database;
use crate::errors::ChalkError;
use crate::AppState;

// ── Backup data model ───────────────────────────────────────

const BACKUP_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupData {
    pub format_version: u32,
    pub app_version: String,
    pub created_at: String,
    pub subjects: Vec<SubjectRow>,
    pub lesson_plans: Vec<LessonPlanRow>,
    pub metadata: Vec<MetadataRow>,
    pub tags: Vec<TagRow>,
    pub plan_tags: Vec<PlanTagRow>,
    pub app_settings: Vec<AppSettingRow>,
    pub chat_conversations: Vec<ChatConversationRow>,
    pub chat_messages: Vec<ChatMessageRow>,
    pub feature_flags: Vec<FeatureFlagRow>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubjectRow {
    pub id: String,
    pub name: String,
    pub grade_level: Option<String>,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LessonPlanRow {
    pub id: String,
    pub subject_id: String,
    pub title: String,
    pub content: String,
    pub source_doc_id: Option<String>,
    pub source_table_index: Option<i32>,
    pub learning_objectives: Option<String>,
    pub status: String,
    pub source_type: String,
    pub version: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetadataRow {
    pub id: String,
    pub lesson_plan_id: String,
    pub key: String,
    pub value: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TagRow {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlanTagRow {
    pub plan_id: String,
    pub tag_id: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppSettingRow {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatConversationRow {
    pub id: String,
    pub title: String,
    pub plan_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatMessageRow {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub context_plan_ids: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FeatureFlagRow {
    pub name: String,
    pub enabled: bool,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Summary info about a backup file (shown before import).
#[derive(Debug, Serialize)]
pub struct BackupInfo {
    pub format_version: u32,
    pub app_version: String,
    pub created_at: String,
    pub plan_count: usize,
    pub tag_count: usize,
    pub conversation_count: usize,
    pub setting_count: usize,
}

// ── Export ───────────────────────────────────────────────────

/// Helper: prepare + query_map + collect in a way that satisfies the borrow checker.
fn query_collect<T, F>(
    conn: &rusqlite::Connection,
    sql: &str,
    map_fn: F,
) -> Result<Vec<T>, rusqlite::Error>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], map_fn)?;
    rows.collect()
}

/// Collect all user data from the database into a `BackupData`.
pub fn collect_backup_data(db: &Database) -> Result<BackupData, ChalkError> {
    db.with_conn(|conn| {
        let subjects = query_collect(conn,
            "SELECT id, name, grade_level, description, created_at, updated_at FROM subjects",
            |row| Ok(SubjectRow {
                id: row.get(0)?, name: row.get(1)?, grade_level: row.get(2)?,
                description: row.get(3)?, created_at: row.get(4)?, updated_at: row.get(5)?,
            }),
        )?;

        let lesson_plans = query_collect(conn,
            "SELECT id, subject_id, title, content, source_doc_id, source_table_index,
                    learning_objectives, status, source_type, version, created_at, updated_at
             FROM lesson_plans",
            |row| Ok(LessonPlanRow {
                id: row.get(0)?, subject_id: row.get(1)?, title: row.get(2)?,
                content: row.get(3)?, source_doc_id: row.get(4)?,
                source_table_index: row.get(5)?, learning_objectives: row.get(6)?,
                status: row.get(7)?, source_type: row.get(8)?, version: row.get(9)?,
                created_at: row.get(10)?, updated_at: row.get(11)?,
            }),
        )?;

        let metadata = query_collect(conn,
            "SELECT id, lesson_plan_id, key, value, created_at FROM metadata",
            |row| Ok(MetadataRow {
                id: row.get(0)?, lesson_plan_id: row.get(1)?, key: row.get(2)?,
                value: row.get(3)?, created_at: row.get(4)?,
            }),
        )?;

        let tags = query_collect(conn,
            "SELECT id, name, color, created_at FROM tags",
            |row| Ok(TagRow {
                id: row.get(0)?, name: row.get(1)?, color: row.get(2)?,
                created_at: row.get(3)?,
            }),
        )?;

        let plan_tags = query_collect(conn,
            "SELECT plan_id, tag_id, created_at FROM plan_tags",
            |row| Ok(PlanTagRow {
                plan_id: row.get(0)?, tag_id: row.get(1)?, created_at: row.get(2)?,
            }),
        )?;

        let app_settings = query_collect(conn,
            "SELECT key, value, updated_at FROM app_settings",
            |row| Ok(AppSettingRow {
                key: row.get(0)?, value: row.get(1)?, updated_at: row.get(2)?,
            }),
        )?;

        let chat_conversations = query_collect(conn,
            "SELECT id, title, plan_id, created_at, updated_at FROM chat_conversations",
            |row| Ok(ChatConversationRow {
                id: row.get(0)?, title: row.get(1)?, plan_id: row.get(2)?,
                created_at: row.get(3)?, updated_at: row.get(4)?,
            }),
        )?;

        let chat_messages = query_collect(conn,
            "SELECT id, conversation_id, role, content, context_plan_ids, created_at
             FROM chat_messages",
            |row| Ok(ChatMessageRow {
                id: row.get(0)?, conversation_id: row.get(1)?, role: row.get(2)?,
                content: row.get(3)?, context_plan_ids: row.get(4)?, created_at: row.get(5)?,
            }),
        )?;

        let feature_flags = {
            let has_table: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='feature_flags'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);

            if has_table {
                query_collect(conn,
                    "SELECT name, enabled, description, created_at, updated_at FROM feature_flags",
                    |row| {
                        let enabled: i32 = row.get(1)?;
                        Ok(FeatureFlagRow {
                            name: row.get(0)?, enabled: enabled != 0,
                            description: row.get(2)?, created_at: row.get(3)?,
                            updated_at: row.get(4)?,
                        })
                    },
                )?
            } else {
                Vec::new()
            }
        };

        Ok(BackupData {
            format_version: BACKUP_FORMAT_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            subjects,
            lesson_plans,
            metadata,
            tags,
            plan_tags,
            app_settings,
            chat_conversations,
            chat_messages,
            feature_flags,
        })
    })
    .map_err(ChalkError::from)
}

/// Write backup data to a zip file at the given path.
pub fn write_backup_zip(data: &BackupData, path: &Path) -> Result<(), ChalkError> {
    let file = std::fs::File::create(path)
        .map_err(|e| ChalkError::io_write(format!("Failed to create backup file: {e}")))?;

    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("backup.json", options)
        .map_err(|e| ChalkError::io_write(format!("Failed to start zip entry: {e}")))?;

    let json = serde_json::to_string_pretty(data)?;
    zip.write_all(json.as_bytes())
        .map_err(|e| ChalkError::io_write(format!("Failed to write backup data: {e}")))?;

    zip.finish()
        .map_err(|e| ChalkError::io_write(format!("Failed to finalize zip: {e}")))?;

    tracing::info!(path = %path.display(), plans = data.lesson_plans.len(), "Backup exported");
    Ok(())
}

/// Read and parse backup data from a zip file.
pub fn read_backup_zip(path: &Path) -> Result<BackupData, ChalkError> {
    let file = std::fs::File::open(path)
        .map_err(|e| ChalkError::io_read(format!("Failed to open backup file: {e}")))?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| ChalkError::io_read(format!("Invalid zip file: {e}")))?;

    let mut entry = archive
        .by_name("backup.json")
        .map_err(|e| ChalkError::io_read(format!("backup.json not found in zip: {e}")))?;

    let mut json = String::new();
    entry
        .read_to_string(&mut json)
        .map_err(|e| ChalkError::io_read(format!("Failed to read backup data: {e}")))?;

    let data: BackupData = serde_json::from_str(&json)
        .map_err(|e| ChalkError::io_read(format!("Invalid backup format: {e}")))?;

    if data.format_version > BACKUP_FORMAT_VERSION {
        return Err(ChalkError::io_read(format!(
            "Backup format version {} is newer than supported version {}. Please update Chalk.",
            data.format_version, BACKUP_FORMAT_VERSION
        )));
    }

    Ok(data)
}

/// Get summary info from a backup file without fully importing it.
pub fn peek_backup(path: &Path) -> Result<BackupInfo, ChalkError> {
    let data = read_backup_zip(path)?;
    Ok(BackupInfo {
        format_version: data.format_version,
        app_version: data.app_version,
        created_at: data.created_at,
        plan_count: data.lesson_plans.len(),
        tag_count: data.tags.len(),
        conversation_count: data.chat_conversations.len(),
        setting_count: data.app_settings.len(),
    })
}

// ── Import ──────────────────────────────────────────────────

/// Check if the database has any existing user data.
pub fn has_existing_data(db: &Database) -> Result<bool, ChalkError> {
    db.with_conn(|conn| {
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM lesson_plans", [], |row| row.get(0))
            .unwrap_or(0);
        Ok(count > 0)
    })
    .map_err(ChalkError::from)
}

/// Clear all user data tables (used before import).
fn clear_user_data(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    // Order matters: children before parents (FK constraints).
    conn.execute_batch(
        "DELETE FROM chat_messages;
         DELETE FROM chat_conversations;
         DELETE FROM plan_tags;
         DELETE FROM metadata;
         DELETE FROM lesson_plans;
         DELETE FROM subjects;
         DELETE FROM tags;
         DELETE FROM app_settings;"
    )?;

    // These tables may not exist (created lazily or by specific migrations).
    for table in &["_vec_id_map", "lesson_plan_vectors", "feature_flags"] {
        let exists: bool = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type IN ('table','view') AND name='{table}'"
                ),
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if exists {
            conn.execute(&format!("DELETE FROM {table}"), [])?;
        }
    }

    Ok(())
}

/// Import backup data into the database, replacing all existing data.
pub fn restore_backup_data(db: &Database, data: &BackupData) -> Result<(), ChalkError> {
    db.with_conn(|conn| {
        // Use a transaction for atomicity.
        conn.execute_batch("BEGIN TRANSACTION")?;

        if let Err(e) = restore_inner(conn, data) {
            conn.execute_batch("ROLLBACK").ok();
            return Err(e);
        }

        conn.execute_batch("COMMIT")?;
        Ok(())
    })
    .map_err(ChalkError::from)
}

fn restore_inner(
    conn: &rusqlite::Connection,
    data: &BackupData,
) -> Result<(), crate::database::DatabaseError> {
    clear_user_data(conn)?;

    // Insert subjects.
    for s in &data.subjects {
        conn.execute(
            "INSERT INTO subjects (id, name, grade_level, description, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![s.id, s.name, s.grade_level, s.description, s.created_at, s.updated_at],
        )?;
    }

    // Insert lesson plans.
    for lp in &data.lesson_plans {
        conn.execute(
            "INSERT INTO lesson_plans (id, subject_id, title, content, source_doc_id,
                    source_table_index, learning_objectives, status, source_type, version,
                    created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                lp.id,
                lp.subject_id,
                lp.title,
                lp.content,
                lp.source_doc_id,
                lp.source_table_index,
                lp.learning_objectives,
                lp.status,
                lp.source_type,
                lp.version,
                lp.created_at,
                lp.updated_at,
            ],
        )?;
    }

    // Insert metadata.
    for m in &data.metadata {
        conn.execute(
            "INSERT INTO metadata (id, lesson_plan_id, key, value, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![m.id, m.lesson_plan_id, m.key, m.value, m.created_at],
        )?;
    }

    // Insert tags.
    for t in &data.tags {
        conn.execute(
            "INSERT INTO tags (id, name, color, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![t.id, t.name, t.color, t.created_at],
        )?;
    }

    // Insert plan-tag associations.
    for pt in &data.plan_tags {
        conn.execute(
            "INSERT INTO plan_tags (plan_id, tag_id, created_at) VALUES (?1, ?2, ?3)",
            params![pt.plan_id, pt.tag_id, pt.created_at],
        )?;
    }

    // Insert app settings.
    for s in &data.app_settings {
        conn.execute(
            "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![s.key, s.value, s.updated_at],
        )?;
    }

    // Insert chat conversations.
    for c in &data.chat_conversations {
        conn.execute(
            "INSERT INTO chat_conversations (id, title, plan_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![c.id, c.title, c.plan_id, c.created_at, c.updated_at],
        )?;
    }

    // Insert chat messages.
    for m in &data.chat_messages {
        conn.execute(
            "INSERT INTO chat_messages (id, conversation_id, role, content, context_plan_ids, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![m.id, m.conversation_id, m.role, m.content, m.context_plan_ids, m.created_at],
        )?;
    }

    // Insert feature flags.
    let has_flags: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='feature_flags'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if has_flags {
        for f in &data.feature_flags {
            let enabled_int: i32 = if f.enabled { 1 } else { 0 };
            conn.execute(
                "INSERT INTO feature_flags (name, enabled, description, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(name) DO UPDATE SET
                    enabled = excluded.enabled,
                    description = excluded.description,
                    updated_at = excluded.updated_at",
                params![f.name, enabled_int, f.description, f.created_at, f.updated_at],
            )?;
        }
    }

    tracing::info!(
        plans = data.lesson_plans.len(),
        tags = data.tags.len(),
        conversations = data.chat_conversations.len(),
        "Backup data restored"
    );

    Ok(())
}

/// Generate a timestamped auto-backup filename.
fn auto_backup_path(data_dir: &Path) -> PathBuf {
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let backup_dir = data_dir.join("com.madison.chalk").join("backups");
    std::fs::create_dir_all(&backup_dir).ok();
    backup_dir.join(format!("auto-backup_{timestamp}.chalk-backup.zip"))
}

// ── Tauri Commands ──────────────────────────────────────────

/// Export all app data to a user-chosen zip file.
/// Returns the path where the backup was saved.
#[tauri::command]
pub fn export_backup(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<serde_json::Value, String> {
    let data = collect_backup_data(&state.db).map_err(|e| e.message)?;
    let dest = PathBuf::from(&path);
    write_backup_zip(&data, &dest).map_err(|e| e.message)?;

    Ok(serde_json::json!({
        "path": path,
        "plan_count": data.lesson_plans.len(),
        "tag_count": data.tags.len(),
    }))
}

/// Import a backup zip, replacing all existing data.
/// If the app has existing data, an auto-backup is created first.
/// Returns summary info about the imported data.
#[tauri::command]
pub fn import_backup(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<serde_json::Value, String> {
    let source = PathBuf::from(&path);
    let data = read_backup_zip(&source).map_err(|e| e.message)?;

    // Auto-backup existing data before replacing.
    let auto_backup_file = if has_existing_data(&state.db).unwrap_or(false) {
        let auto_path = auto_backup_path(&state.data_dir);
        let existing_data = collect_backup_data(&state.db).map_err(|e| e.message)?;
        write_backup_zip(&existing_data, &auto_path).map_err(|e| e.message)?;
        tracing::info!(path = %auto_path.display(), "Auto-backup created before import");
        Some(auto_path.to_string_lossy().to_string())
    } else {
        None
    };

    restore_backup_data(&state.db, &data).map_err(|e| e.message)?;

    Ok(serde_json::json!({
        "plan_count": data.lesson_plans.len(),
        "tag_count": data.tags.len(),
        "conversation_count": data.chat_conversations.len(),
        "auto_backup_path": auto_backup_file,
    }))
}

/// Peek at a backup file to get summary info without importing.
#[tauri::command]
pub fn get_backup_info(path: String) -> Result<BackupInfo, String> {
    peek_backup(&PathBuf::from(path)).map_err(|e| e.message)
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{Database, NewLessonPlan, NewSubject, NewTag};

    fn test_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    fn setup_test_data(db: &Database) {
        let subject = db
            .create_subject(&NewSubject {
                name: "Biology".into(),
                grade_level: Some("10th".into()),
                description: Some("Life sciences".into()),
            })
            .unwrap();

        let plan = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Photosynthesis Lab".into(),
                content: Some("<p>Plants convert sunlight to energy</p>".into()),
                source_doc_id: Some("gdoc_123".into()),
                source_table_index: Some(0),
                learning_objectives: Some("Understand photosynthesis".into()),
            })
            .unwrap();

        let tag = db
            .create_tag(&NewTag {
                name: "Science".into(),
                color: Some("#4CAF50".into()),
            })
            .unwrap();

        db.add_tag_to_plan(&plan.id, &tag.id).unwrap();

        db.set_setting("teacher_name", "Madison").unwrap();

        let conv = db.create_conversation("Test chat", Some(&plan.id)).unwrap();
        db.add_chat_message(&conv.id, "user", "Help with photosynthesis", None)
            .unwrap();
        db.add_chat_message(&conv.id, "assistant", "Sure! Here's a plan...", None)
            .unwrap();
    }

    #[test]
    fn test_collect_backup_data() {
        let db = test_db();
        setup_test_data(&db);

        let data = collect_backup_data(&db).unwrap();

        assert_eq!(data.format_version, BACKUP_FORMAT_VERSION);
        assert_eq!(data.subjects.len(), 1);
        assert_eq!(data.subjects[0].name, "Biology");
        assert_eq!(data.lesson_plans.len(), 1);
        assert_eq!(data.lesson_plans[0].title, "Photosynthesis Lab");
        assert_eq!(data.tags.len(), 1);
        assert_eq!(data.plan_tags.len(), 1);
        assert_eq!(data.app_settings.len(), 1);
        assert_eq!(data.chat_conversations.len(), 1);
        assert_eq!(data.chat_messages.len(), 2);
    }

    #[test]
    fn test_export_and_import_roundtrip() {
        let db = test_db();
        setup_test_data(&db);

        let data = collect_backup_data(&db).unwrap();

        // Write to temp file.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        write_backup_zip(&data, &path).unwrap();

        // Read it back.
        let restored = read_backup_zip(&path).unwrap();

        assert_eq!(restored.format_version, data.format_version);
        assert_eq!(restored.subjects.len(), data.subjects.len());
        assert_eq!(restored.lesson_plans.len(), data.lesson_plans.len());
        assert_eq!(restored.tags.len(), data.tags.len());
        assert_eq!(restored.plan_tags.len(), data.plan_tags.len());
        assert_eq!(restored.app_settings.len(), data.app_settings.len());
        assert_eq!(
            restored.chat_conversations.len(),
            data.chat_conversations.len()
        );
        assert_eq!(restored.chat_messages.len(), data.chat_messages.len());
    }

    #[test]
    fn test_peek_backup() {
        let db = test_db();
        setup_test_data(&db);

        let data = collect_backup_data(&db).unwrap();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        write_backup_zip(&data, tmp.path()).unwrap();

        let info = peek_backup(tmp.path()).unwrap();
        assert_eq!(info.plan_count, 1);
        assert_eq!(info.tag_count, 1);
        assert_eq!(info.conversation_count, 1);
        assert_eq!(info.setting_count, 1);
    }

    #[test]
    fn test_restore_replaces_data() {
        let db1 = test_db();
        setup_test_data(&db1);
        let original_data = collect_backup_data(&db1).unwrap();

        // Write backup.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        write_backup_zip(&original_data, tmp.path()).unwrap();

        // Create a fresh DB and add different data.
        let db2 = test_db();
        let s = db2
            .create_subject(&NewSubject {
                name: "Math".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();
        db2.create_lesson_plan(&NewLessonPlan {
            subject_id: s.id,
            title: "Algebra".into(),
            content: None,
            source_doc_id: None,
            source_table_index: None,
            learning_objectives: None,
        })
        .unwrap();

        // Import backup into db2 — should replace Math data with Biology data.
        let backup = read_backup_zip(tmp.path()).unwrap();
        restore_backup_data(&db2, &backup).unwrap();

        let subjects = db2.list_subjects().unwrap();
        assert_eq!(subjects.len(), 1);
        assert_eq!(subjects[0].name, "Biology");

        let plans = db2
            .list_library_plans(&crate::database::LibraryQuery {
                source_type: None,
                search: None,
                tag_ids: None,
            })
            .unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].title, "Photosynthesis Lab");

        let tags = db2.list_tags().unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "Science");

        let convs = db2.list_conversations().unwrap();
        assert_eq!(convs.len(), 1);
    }

    #[test]
    fn test_has_existing_data() {
        let db = test_db();
        assert!(!has_existing_data(&db).unwrap());

        setup_test_data(&db);
        assert!(has_existing_data(&db).unwrap());
    }

    #[test]
    fn test_empty_database_backup() {
        let db = test_db();
        let data = collect_backup_data(&db).unwrap();

        assert_eq!(data.subjects.len(), 0);
        assert_eq!(data.lesson_plans.len(), 0);
        assert_eq!(data.tags.len(), 0);

        let tmp = tempfile::NamedTempFile::new().unwrap();
        write_backup_zip(&data, tmp.path()).unwrap();

        let restored = read_backup_zip(tmp.path()).unwrap();
        assert_eq!(restored.lesson_plans.len(), 0);
    }

    #[test]
    fn test_restore_into_empty_db() {
        let db1 = test_db();
        setup_test_data(&db1);
        let data = collect_backup_data(&db1).unwrap();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        write_backup_zip(&data, tmp.path()).unwrap();

        // Restore into completely empty db.
        let db2 = test_db();
        let backup = read_backup_zip(tmp.path()).unwrap();
        restore_backup_data(&db2, &backup).unwrap();

        let plans = db2
            .list_library_plans(&crate::database::LibraryQuery {
                source_type: None,
                search: None,
                tag_ids: None,
            })
            .unwrap();
        assert_eq!(plans.len(), 1);
    }

    #[test]
    fn test_invalid_zip_returns_error() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"not a zip file").unwrap();

        let result = read_backup_zip(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_zip_missing_backup_json_returns_error() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        {
            let file = std::fs::File::create(tmp.path()).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("wrong.txt", options).unwrap();
            zip.write_all(b"hello").unwrap();
            zip.finish().unwrap();
        }

        let result = read_backup_zip(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_future_format_version_rejected() {
        let data = BackupData {
            format_version: 999,
            app_version: "99.0.0".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            subjects: vec![],
            lesson_plans: vec![],
            metadata: vec![],
            tags: vec![],
            plan_tags: vec![],
            app_settings: vec![],
            chat_conversations: vec![],
            chat_messages: vec![],
            feature_flags: vec![],
        };

        let tmp = tempfile::NamedTempFile::new().unwrap();
        // Manually write a zip with the high version.
        {
            let file = std::fs::File::create(tmp.path()).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("backup.json", options).unwrap();
            let json = serde_json::to_string(&data).unwrap();
            zip.write_all(json.as_bytes()).unwrap();
            zip.finish().unwrap();
        }

        let result = read_backup_zip(tmp.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .message
            .contains("newer than supported"));
    }

    #[test]
    fn test_metadata_preserved_in_roundtrip() {
        let db = test_db();
        setup_test_data(&db);

        // Add metadata to the plan.
        let plans = db
            .list_library_plans(&crate::database::LibraryQuery {
                source_type: None,
                search: None,
                tag_ids: None,
            })
            .unwrap();
        let plan_id = &plans[0].id;
        db.set_metadata(&crate::database::NewMetadata {
            lesson_plan_id: plan_id.clone(),
            key: "duration".into(),
            value: "45 minutes".into(),
        })
        .unwrap();

        let data = collect_backup_data(&db).unwrap();
        assert_eq!(data.metadata.len(), 1);
        assert_eq!(data.metadata[0].key, "duration");

        // Roundtrip.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        write_backup_zip(&data, tmp.path()).unwrap();
        let restored = read_backup_zip(tmp.path()).unwrap();
        assert_eq!(restored.metadata.len(), 1);
        assert_eq!(restored.metadata[0].value, "45 minutes");
    }

    #[test]
    fn test_feature_flags_in_backup() {
        let db = test_db();

        // Run feature flags migration.
        db.with_conn(|conn| {
            conn.execute_batch(crate::feature_flags::FEATURE_FLAGS_MIGRATION.2)?;
            Ok(())
        })
        .unwrap();

        db.set_feature_flag(&crate::feature_flags::FeatureFlagInput {
            name: "dark_mode".into(),
            enabled: true,
            description: Some("Enable dark mode".into()),
        })
        .unwrap();

        let data = collect_backup_data(&db).unwrap();
        assert_eq!(data.feature_flags.len(), 1);
        assert_eq!(data.feature_flags[0].name, "dark_mode");
        assert!(data.feature_flags[0].enabled);
    }

    #[test]
    fn test_chat_context_plan_ids_preserved() {
        let db = test_db();
        let subject = db
            .create_subject(&NewSubject {
                name: "Test".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();
        let plan = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id,
                title: "Test Plan".into(),
                content: None,
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();
        let conv = db.create_conversation("Test", None).unwrap();
        db.add_chat_message(
            &conv.id,
            "user",
            "Hello",
            Some(&format!(r#"["{}"]"#, plan.id)),
        )
        .unwrap();

        let data = collect_backup_data(&db).unwrap();
        assert!(data.chat_messages[0].context_plan_ids.is_some());

        let tmp = tempfile::NamedTempFile::new().unwrap();
        write_backup_zip(&data, tmp.path()).unwrap();
        let restored = read_backup_zip(tmp.path()).unwrap();
        assert_eq!(
            restored.chat_messages[0].context_plan_ids,
            data.chat_messages[0].context_plan_ids
        );
    }
}
