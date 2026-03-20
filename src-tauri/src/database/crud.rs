use rusqlite::params;

use super::connection::{Database, DatabaseError, Result};
use super::models::*;

impl Database {
    // ── Subjects ──────────────────────────────────────────────

    pub fn create_subject(&self, input: &NewSubject) -> Result<Subject> {
        let id = uuid::Uuid::new_v4().to_string();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO subjects (id, name, grade_level, description) VALUES (?1, ?2, ?3, ?4)",
                params![id, input.name, input.grade_level, input.description],
            )?;
            self.get_subject_inner(conn, &id)
        })
    }

    pub fn get_subject(&self, id: &str) -> Result<Subject> {
        self.with_conn(|conn| self.get_subject_inner(conn, id))
    }

    fn get_subject_inner(
        &self,
        conn: &rusqlite::Connection,
        id: &str,
    ) -> Result<Subject> {
        conn.query_row(
            "SELECT id, name, grade_level, description, created_at, updated_at FROM subjects WHERE id = ?1",
            params![id],
            |row| {
                Ok(Subject {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    grade_level: row.get(2)?,
                    description: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
            other => DatabaseError::Sqlite(other),
        })
    }

    pub fn list_subjects(&self) -> Result<Vec<Subject>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, grade_level, description, created_at, updated_at FROM subjects ORDER BY name",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(Subject {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    grade_level: row.get(2)?,
                    description: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn update_subject(&self, id: &str, input: &NewSubject) -> Result<Subject> {
        self.with_conn(|conn| {
            let updated = conn.execute(
                "UPDATE subjects SET name = ?1, grade_level = ?2, description = ?3, updated_at = datetime('now') WHERE id = ?4",
                params![input.name, input.grade_level, input.description, id],
            )?;
            if updated == 0 {
                return Err(DatabaseError::NotFound);
            }
            self.get_subject_inner(conn, id)
        })
    }

    pub fn delete_subject(&self, id: &str) -> Result<()> {
        self.with_conn(|conn| {
            let deleted = conn.execute("DELETE FROM subjects WHERE id = ?1", params![id])?;
            if deleted == 0 {
                return Err(DatabaseError::NotFound);
            }
            Ok(())
        })
    }

    // ── LTP Documents ──────────────────────────────────────────

    /// Import an LTP document with duplicate detection via SHA-256 hash.
    ///
    /// If a document with the same filename exists and has the same hash,
    /// the import is skipped. If the hash differs, the old parsed data is
    /// deleted and the document is updated with the new content.
    pub fn import_ltp_document(
        &self,
        filename: &str,
        file_hash: &str,
        school_year: Option<&str>,
        doc_type: &str,
        raw_html: &str,
    ) -> Result<LtpImportResult> {
        self.with_conn(|conn| {
            // Check for existing document with same filename.
            let existing: Option<(String, String)> = conn
                .query_row(
                    "SELECT id, file_hash FROM ltp_documents WHERE filename = ?1",
                    params![filename],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .ok();

            match existing {
                Some((existing_id, existing_hash)) if existing_hash == file_hash => {
                    // Same content — skip.
                    Ok(LtpImportResult::Skipped {
                        id: existing_id,
                        filename: filename.to_string(),
                    })
                }
                Some((existing_id, _)) => {
                    // Different content — update document, cascade deletes old
                    // grid_cells/calendar_entries via ON DELETE CASCADE.
                    conn.execute(
                        "DELETE FROM ltp_grid_cells WHERE document_id = ?1",
                        params![existing_id],
                    )?;
                    conn.execute(
                        "DELETE FROM school_calendar_entries WHERE document_id = ?1",
                        params![existing_id],
                    )?;
                    conn.execute(
                        "UPDATE ltp_documents SET file_hash = ?1, school_year = ?2, doc_type = ?3, raw_html = ?4, updated_at = datetime('now') WHERE id = ?5",
                        params![file_hash, school_year, doc_type, raw_html, existing_id],
                    )?;
                    self.get_ltp_document_inner(conn, &existing_id)
                        .map(LtpImportResult::Imported)
                }
                None => {
                    // New document.
                    let id = uuid::Uuid::new_v4().to_string();
                    conn.execute(
                        "INSERT INTO ltp_documents (id, filename, file_hash, school_year, doc_type, raw_html) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![id, filename, file_hash, school_year, doc_type, raw_html],
                    )?;
                    self.get_ltp_document_inner(conn, &id)
                        .map(LtpImportResult::Imported)
                }
            }
        })
    }

    pub fn get_ltp_document(&self, id: &str) -> Result<LtpDocument> {
        self.with_conn(|conn| self.get_ltp_document_inner(conn, id))
    }

    fn get_ltp_document_inner(
        &self,
        conn: &rusqlite::Connection,
        id: &str,
    ) -> Result<LtpDocument> {
        conn.query_row(
            "SELECT id, filename, file_hash, school_year, doc_type, raw_html, imported_at, updated_at
             FROM ltp_documents WHERE id = ?1",
            params![id],
            |row| {
                Ok(LtpDocument {
                    id: row.get(0)?,
                    filename: row.get(1)?,
                    file_hash: row.get(2)?,
                    school_year: row.get(3)?,
                    doc_type: row.get(4)?,
                    raw_html: row.get(5)?,
                    imported_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
            other => DatabaseError::Sqlite(other),
        })
    }

    pub fn list_ltp_documents(&self) -> Result<Vec<LtpDocument>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, filename, file_hash, school_year, doc_type, raw_html, imported_at, updated_at
                 FROM ltp_documents ORDER BY imported_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(LtpDocument {
                    id: row.get(0)?,
                    filename: row.get(1)?,
                    file_hash: row.get(2)?,
                    school_year: row.get(3)?,
                    doc_type: row.get(4)?,
                    raw_html: row.get(5)?,
                    imported_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn delete_ltp_document(&self, id: &str) -> Result<()> {
        self.with_conn(|conn| {
            let deleted =
                conn.execute("DELETE FROM ltp_documents WHERE id = ?1", params![id])?;
            if deleted == 0 {
                return Err(DatabaseError::NotFound);
            }
            Ok(())
        })
    }

    // ── LTP Grid Cells ──────────────────────────────────────────

    pub fn insert_ltp_grid_cell(
        &self,
        document_id: &str,
        row_index: i32,
        col_index: i32,
        subject: Option<&str>,
        month: Option<&str>,
        content_html: Option<&str>,
        content_text: Option<&str>,
        background_color: Option<&str>,
        unit_name: Option<&str>,
        unit_color: Option<&str>,
    ) -> Result<LtpGridCell> {
        let id = uuid::Uuid::new_v4().to_string();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO ltp_grid_cells (id, document_id, row_index, col_index, subject, month, content_html, content_text, background_color, unit_name, unit_color)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![id, document_id, row_index, col_index, subject, month, content_html, content_text, background_color, unit_name, unit_color],
            )?;
            conn.query_row(
                "SELECT id, document_id, row_index, col_index, subject, month, content_html, content_text, background_color, unit_name, unit_color
                 FROM ltp_grid_cells WHERE id = ?1",
                params![id],
                |row| {
                    Ok(LtpGridCell {
                        id: row.get(0)?,
                        document_id: row.get(1)?,
                        row_index: row.get(2)?,
                        col_index: row.get(3)?,
                        subject: row.get(4)?,
                        month: row.get(5)?,
                        content_html: row.get(6)?,
                        content_text: row.get(7)?,
                        background_color: row.get(8)?,
                        unit_name: row.get(9)?,
                        unit_color: row.get(10)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                other => DatabaseError::Sqlite(other),
            })
        })
    }

    pub fn list_ltp_grid_cells(&self, document_id: &str) -> Result<Vec<LtpGridCell>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, document_id, row_index, col_index, subject, month, content_html, content_text, background_color, unit_name, unit_color
                 FROM ltp_grid_cells WHERE document_id = ?1
                 ORDER BY row_index, col_index",
            )?;
            let rows = stmt.query_map(params![document_id], |row| {
                Ok(LtpGridCell {
                    id: row.get(0)?,
                    document_id: row.get(1)?,
                    row_index: row.get(2)?,
                    col_index: row.get(3)?,
                    subject: row.get(4)?,
                    month: row.get(5)?,
                    content_html: row.get(6)?,
                    content_text: row.get(7)?,
                    background_color: row.get(8)?,
                    unit_name: row.get(9)?,
                    unit_color: row.get(10)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    /// Update the text content of a single LTP grid cell.
    pub fn update_ltp_grid_cell(
        &self,
        cell_id: &str,
        content_text: &str,
    ) -> Result<LtpGridCell> {
        self.with_conn(|conn| {
            let updated = conn.execute(
                "UPDATE ltp_grid_cells SET content_text = ?1 WHERE id = ?2",
                params![content_text, cell_id],
            )?;
            if updated == 0 {
                return Err(DatabaseError::NotFound);
            }
            conn.query_row(
                "SELECT id, document_id, row_index, col_index, subject, month, content_html, content_text, background_color, unit_name, unit_color
                 FROM ltp_grid_cells WHERE id = ?1",
                params![cell_id],
                |row| {
                    Ok(LtpGridCell {
                        id: row.get(0)?,
                        document_id: row.get(1)?,
                        row_index: row.get(2)?,
                        col_index: row.get(3)?,
                        subject: row.get(4)?,
                        month: row.get(5)?,
                        content_html: row.get(6)?,
                        content_text: row.get(7)?,
                        background_color: row.get(8)?,
                        unit_name: row.get(9)?,
                        unit_color: row.get(10)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                other => DatabaseError::Sqlite(other),
            })
        })
    }

    // ── School Calendar Entries ──────────────────────────────────

    pub fn insert_school_calendar_entry(
        &self,
        document_id: &str,
        date: Option<&str>,
        day_number: Option<i32>,
        unit_name: Option<&str>,
        unit_color: Option<&str>,
        is_holiday: bool,
        holiday_name: Option<&str>,
        notes: Option<&str>,
    ) -> Result<SchoolCalendarEntry> {
        let id = uuid::Uuid::new_v4().to_string();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO school_calendar_entries (id, document_id, date, day_number, unit_name, unit_color, is_holiday, holiday_name, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![id, document_id, date, day_number, unit_name, unit_color, is_holiday as i32, holiday_name, notes],
            )?;
            conn.query_row(
                "SELECT id, document_id, date, day_number, unit_name, unit_color, is_holiday, holiday_name, notes
                 FROM school_calendar_entries WHERE id = ?1",
                params![id],
                |row| {
                    Ok(SchoolCalendarEntry {
                        id: row.get(0)?,
                        document_id: row.get(1)?,
                        date: row.get(2)?,
                        day_number: row.get(3)?,
                        unit_name: row.get(4)?,
                        unit_color: row.get(5)?,
                        is_holiday: row.get::<_, i32>(6)? != 0,
                        holiday_name: row.get(7)?,
                        notes: row.get(8)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                other => DatabaseError::Sqlite(other),
            })
        })
    }

    pub fn list_school_calendar_entries(
        &self,
        document_id: &str,
    ) -> Result<Vec<SchoolCalendarEntry>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, document_id, date, day_number, unit_name, unit_color, is_holiday, holiday_name, notes
                 FROM school_calendar_entries WHERE document_id = ?1
                 ORDER BY date, day_number",
            )?;
            let rows = stmt.query_map(params![document_id], |row| {
                Ok(SchoolCalendarEntry {
                    id: row.get(0)?,
                    document_id: row.get(1)?,
                    date: row.get(2)?,
                    day_number: row.get(3)?,
                    unit_name: row.get(4)?,
                    unit_color: row.get(5)?,
                    is_holiday: row.get::<_, i32>(6)? != 0,
                    holiday_name: row.get(7)?,
                    notes: row.get(8)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    // ── LTP Context Queries ─────────────────────────────────────

    /// Get all LTP grid cells for a given month name (e.g., "March").
    /// Returns cells from the most recently imported LTP document.
    pub fn get_ltp_cells_for_month(&self, month: &str) -> Result<Vec<LtpGridCell>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT gc.id, gc.document_id, gc.row_index, gc.col_index, gc.subject, gc.month,
                        gc.content_html, gc.content_text, gc.background_color, gc.unit_name, gc.unit_color
                 FROM ltp_grid_cells gc
                 JOIN ltp_documents d ON gc.document_id = d.id
                 WHERE d.doc_type = 'ltp'
                   AND gc.month = ?1
                   AND gc.content_text IS NOT NULL
                   AND gc.content_text != ''
                 ORDER BY d.imported_at DESC, gc.row_index, gc.col_index",
            )?;
            let rows = stmt.query_map(params![month], |row| {
                Ok(LtpGridCell {
                    id: row.get(0)?,
                    document_id: row.get(1)?,
                    row_index: row.get(2)?,
                    col_index: row.get(3)?,
                    subject: row.get(4)?,
                    month: row.get(5)?,
                    content_html: row.get(6)?,
                    content_text: row.get(7)?,
                    background_color: row.get(8)?,
                    unit_name: row.get(9)?,
                    unit_color: row.get(10)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    /// Get the unit name for a given month from the most recent LTP document.
    /// Returns the first non-empty unit_name found for cells in that month.
    pub fn get_unit_for_month(&self, month: &str) -> Result<Option<String>> {
        self.with_conn(|conn| {
            let result: Option<String> = conn
                .query_row(
                    "SELECT gc.unit_name
                     FROM ltp_grid_cells gc
                     JOIN ltp_documents d ON gc.document_id = d.id
                     WHERE d.doc_type = 'ltp'
                       AND gc.month = ?1
                       AND gc.unit_name IS NOT NULL
                       AND gc.unit_name != ''
                     ORDER BY d.imported_at DESC
                     LIMIT 1",
                    params![month],
                    |row| row.get(0),
                )
                .ok();
            Ok(result)
        })
    }

    /// Get school calendar entries for a date range (inclusive).
    /// Dates should be in ISO 8601 format (YYYY-MM-DD).
    pub fn get_calendar_entries_for_range(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<SchoolCalendarEntry>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, document_id, date, day_number, unit_name, unit_color, is_holiday, holiday_name, notes
                 FROM school_calendar_entries
                 WHERE date >= ?1 AND date <= ?2
                 ORDER BY date",
            )?;
            let rows = stmt.query_map(params![start_date, end_date], |row| {
                Ok(SchoolCalendarEntry {
                    id: row.get(0)?,
                    document_id: row.get(1)?,
                    date: row.get(2)?,
                    day_number: row.get(3)?,
                    unit_name: row.get(4)?,
                    unit_color: row.get(5)?,
                    is_holiday: row.get::<_, i32>(6)? != 0,
                    holiday_name: row.get(7)?,
                    notes: row.get(8)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    /// Check if any LTP documents have been imported.
    pub fn has_ltp_documents(&self) -> Result<bool> {
        self.with_conn(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM ltp_documents WHERE doc_type = 'ltp'",
                [],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        })
    }

    // ── Lesson Plans ──────────────────────────────────────────

    pub fn create_lesson_plan(&self, input: &NewLessonPlan) -> Result<LessonPlan> {
        let id = uuid::Uuid::new_v4().to_string();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO lesson_plans (id, subject_id, title, content, source_doc_id, source_table_index, learning_objectives)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    input.subject_id,
                    input.title,
                    input.content.as_deref().unwrap_or(""),
                    input.source_doc_id,
                    input.source_table_index,
                    input.learning_objectives,
                ],
            )?;
            self.get_lesson_plan_inner(conn, &id)
        })
    }

    pub fn get_lesson_plan(&self, id: &str) -> Result<LessonPlan> {
        self.with_conn(|conn| self.get_lesson_plan_inner(conn, id))
    }

    fn get_lesson_plan_inner(
        &self,
        conn: &rusqlite::Connection,
        id: &str,
    ) -> Result<LessonPlan> {
        conn.query_row(
            "SELECT id, subject_id, title, content, source_doc_id, source_table_index, learning_objectives, status, created_at, updated_at
             FROM lesson_plans WHERE id = ?1",
            params![id],
            |row| {
                Ok(LessonPlan {
                    id: row.get(0)?,
                    subject_id: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    source_doc_id: row.get(4)?,
                    source_table_index: row.get(5)?,
                    learning_objectives: row.get(6)?,
                    status: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
            other => DatabaseError::Sqlite(other),
        })
    }

    pub fn list_lesson_plans_by_subject(&self, subject_id: &str) -> Result<Vec<LessonPlan>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, subject_id, title, content, source_doc_id, source_table_index, learning_objectives, status, created_at, updated_at
                 FROM lesson_plans WHERE subject_id = ?1 ORDER BY updated_at DESC",
            )?;
            let rows = stmt.query_map(params![subject_id], |row| {
                Ok(LessonPlan {
                    id: row.get(0)?,
                    subject_id: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    source_doc_id: row.get(4)?,
                    source_table_index: row.get(5)?,
                    learning_objectives: row.get(6)?,
                    status: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn update_lesson_plan_content(&self, id: &str, content: &str) -> Result<LessonPlan> {
        self.with_conn(|conn| {
            let updated = conn.execute(
                "UPDATE lesson_plans SET content = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![content, id],
            )?;
            if updated == 0 {
                return Err(DatabaseError::NotFound);
            }
            self.get_lesson_plan_inner(conn, id)
        })
    }

    pub fn update_lesson_plan_status(&self, id: &str, status: &str) -> Result<LessonPlan> {
        self.with_conn(|conn| {
            let updated = conn.execute(
                "UPDATE lesson_plans SET status = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![status, id],
            )?;
            if updated == 0 {
                return Err(DatabaseError::NotFound);
            }
            self.get_lesson_plan_inner(conn, id)
        })
    }

    pub fn delete_lesson_plan(&self, id: &str) -> Result<()> {
        self.with_conn(|conn| {
            let deleted =
                conn.execute("DELETE FROM lesson_plans WHERE id = ?1", params![id])?;
            if deleted == 0 {
                return Err(DatabaseError::NotFound);
            }
            Ok(())
        })
    }

    /// List all lesson plans that don't have embeddings in the vector table.
    pub fn list_plans_without_embeddings(&self) -> Result<Vec<LessonPlan>> {
        self.with_conn(|conn| {
            // Ensure mapping table exists.
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS _vec_id_map (
                    rowid   INTEGER PRIMARY KEY AUTOINCREMENT,
                    plan_id TEXT NOT NULL UNIQUE
                )",
            )?;

            let mut stmt = conn.prepare(
                "SELECT lp.id, lp.subject_id, lp.title, lp.content, lp.source_doc_id,
                        lp.source_table_index, lp.learning_objectives, lp.status,
                        lp.created_at, lp.updated_at
                 FROM lesson_plans lp
                 LEFT JOIN _vec_id_map vm ON vm.plan_id = lp.id
                 WHERE vm.rowid IS NULL
                 ORDER BY lp.updated_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(LessonPlan {
                    id: row.get(0)?,
                    subject_id: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    source_doc_id: row.get(4)?,
                    source_table_index: row.get(5)?,
                    learning_objectives: row.get(6)?,
                    status: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    // ── Reference Documents ──────────────────────────────────

    pub fn create_reference_doc(
        &self,
        id: &str,
        source_doc_id: Option<&str>,
        source_doc_name: Option<&str>,
        title: &str,
        content_html: &str,
        content_text: &str,
    ) -> Result<ReferenceDoc> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO reference_docs (id, source_doc_id, source_doc_name, title, content_html, content_text)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, source_doc_id, source_doc_name, title, content_html, content_text],
            )?;
            self.get_reference_doc_inner(conn, id)
        })
    }

    pub fn get_reference_doc(&self, id: &str) -> Result<ReferenceDoc> {
        self.with_conn(|conn| self.get_reference_doc_inner(conn, id))
    }

    fn get_reference_doc_inner(
        &self,
        conn: &rusqlite::Connection,
        id: &str,
    ) -> Result<ReferenceDoc> {
        conn.query_row(
            "SELECT id, source_doc_id, source_doc_name, title, content_html, content_text, created_at
             FROM reference_docs WHERE id = ?1",
            params![id],
            |row| {
                Ok(ReferenceDoc {
                    id: row.get(0)?,
                    source_doc_id: row.get(1)?,
                    source_doc_name: row.get(2)?,
                    title: row.get(3)?,
                    content_html: row.get(4)?,
                    content_text: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
            other => DatabaseError::Sqlite(other),
        })
    }

    pub fn list_reference_docs(&self) -> Result<Vec<ReferenceDoc>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, source_doc_id, source_doc_name, title, content_html, content_text, created_at
                 FROM reference_docs ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(ReferenceDoc {
                    id: row.get(0)?,
                    source_doc_id: row.get(1)?,
                    source_doc_name: row.get(2)?,
                    title: row.get(3)?,
                    content_html: row.get(4)?,
                    content_text: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    /// List all reference docs that don't have embeddings in the vector table.
    pub fn list_ref_docs_without_embeddings(&self) -> Result<Vec<ReferenceDoc>> {
        self.with_conn(|conn| {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS _ref_doc_vec_id_map (
                    rowid    INTEGER PRIMARY KEY AUTOINCREMENT,
                    doc_id   TEXT NOT NULL UNIQUE
                )",
            )?;

            let mut stmt = conn.prepare(
                "SELECT rd.id, rd.source_doc_id, rd.source_doc_name, rd.title,
                        rd.content_html, rd.content_text, rd.created_at
                 FROM reference_docs rd
                 LEFT JOIN _ref_doc_vec_id_map vm ON vm.doc_id = rd.id
                 WHERE vm.rowid IS NULL
                 ORDER BY rd.created_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(ReferenceDoc {
                    id: row.get(0)?,
                    source_doc_id: row.get(1)?,
                    source_doc_name: row.get(2)?,
                    title: row.get(3)?,
                    content_html: row.get(4)?,
                    content_text: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn delete_reference_doc(&self, id: &str) -> Result<()> {
        self.with_conn(|conn| {
            let deleted =
                conn.execute("DELETE FROM reference_docs WHERE id = ?1", params![id])?;
            if deleted == 0 {
                return Err(DatabaseError::NotFound);
            }
            Ok(())
        })
    }

    // ── Metadata ──────────────────────────────────────────────

    pub fn set_metadata(&self, input: &NewMetadata) -> Result<Metadata> {
        let id = uuid::Uuid::new_v4().to_string();
        self.with_conn(|conn| {
            // Upsert: if (lesson_plan_id, key) exists, update value; otherwise insert.
            conn.execute(
                "INSERT INTO metadata (id, lesson_plan_id, key, value)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(lesson_plan_id, key) DO UPDATE SET value = excluded.value",
                params![id, input.lesson_plan_id, input.key, input.value],
            )?;
            // Fetch the actual row (may have a different id if upserted).
            conn.query_row(
                "SELECT id, lesson_plan_id, key, value, created_at FROM metadata WHERE lesson_plan_id = ?1 AND key = ?2",
                params![input.lesson_plan_id, input.key],
                |row| {
                    Ok(Metadata {
                        id: row.get(0)?,
                        lesson_plan_id: row.get(1)?,
                        key: row.get(2)?,
                        value: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                other => DatabaseError::Sqlite(other),
            })
        })
    }

    pub fn get_metadata_for_plan(&self, lesson_plan_id: &str) -> Result<Vec<Metadata>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, lesson_plan_id, key, value, created_at FROM metadata WHERE lesson_plan_id = ?1 ORDER BY key",
            )?;
            let rows = stmt.query_map(params![lesson_plan_id], |row| {
                Ok(Metadata {
                    id: row.get(0)?,
                    lesson_plan_id: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn delete_metadata(&self, lesson_plan_id: &str, key: &str) -> Result<()> {
        self.with_conn(|conn| {
            let deleted = conn.execute(
                "DELETE FROM metadata WHERE lesson_plan_id = ?1 AND key = ?2",
                params![lesson_plan_id, key],
            )?;
            if deleted == 0 {
                return Err(DatabaseError::NotFound);
            }
            Ok(())
        })
    }

    // ── Plan Versions ────────────────────────────────────────

    /// Finalize a plan: snapshot current state as a new version, bump the
    /// version counter on the plan, and set status to "finalized".
    pub fn finalize_plan(&self, plan_id: &str) -> Result<PlanVersion> {
        self.with_conn(|conn| {
            // Get current plan
            let plan = self.get_lesson_plan_inner(conn, plan_id)?;

            // Determine next version number
            let next_version: i32 = conn
                .query_row(
                    "SELECT COALESCE(MAX(version), 0) + 1 FROM plan_versions WHERE plan_id = ?1",
                    params![plan_id],
                    |row| row.get(0),
                )
                .map_err(DatabaseError::Sqlite)?;

            let version_id = uuid::Uuid::new_v4().to_string();

            // Insert version snapshot
            conn.execute(
                "INSERT INTO plan_versions (id, plan_id, version, title, content, learning_objectives)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    version_id,
                    plan_id,
                    next_version,
                    plan.title,
                    plan.content,
                    plan.learning_objectives,
                ],
            )?;

            // Update plan's version counter and status
            conn.execute(
                "UPDATE lesson_plans SET version = ?1, status = 'finalized', updated_at = datetime('now') WHERE id = ?2",
                params![next_version, plan_id],
            )?;

            // Return the created version
            conn.query_row(
                "SELECT id, plan_id, version, title, content, learning_objectives, created_at
                 FROM plan_versions WHERE id = ?1",
                params![version_id],
                |row| {
                    Ok(PlanVersion {
                        id: row.get(0)?,
                        plan_id: row.get(1)?,
                        version: row.get(2)?,
                        title: row.get(3)?,
                        content: row.get(4)?,
                        learning_objectives: row.get(5)?,
                        created_at: row.get(6)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                other => DatabaseError::Sqlite(other),
            })
        })
    }

    /// List all versions for a plan, ordered newest first.
    pub fn list_plan_versions(&self, plan_id: &str) -> Result<Vec<PlanVersion>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, plan_id, version, title, content, learning_objectives, created_at
                 FROM plan_versions WHERE plan_id = ?1
                 ORDER BY version DESC",
            )?;
            let rows = stmt.query_map(params![plan_id], |row| {
                Ok(PlanVersion {
                    id: row.get(0)?,
                    plan_id: row.get(1)?,
                    version: row.get(2)?,
                    title: row.get(3)?,
                    content: row.get(4)?,
                    learning_objectives: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    /// Get a specific version by plan_id and version number.
    pub fn get_plan_version(&self, plan_id: &str, version: i32) -> Result<PlanVersion> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, plan_id, version, title, content, learning_objectives, created_at
                 FROM plan_versions WHERE plan_id = ?1 AND version = ?2",
                params![plan_id, version],
                |row| {
                    Ok(PlanVersion {
                        id: row.get(0)?,
                        plan_id: row.get(1)?,
                        version: row.get(2)?,
                        title: row.get(3)?,
                        content: row.get(4)?,
                        learning_objectives: row.get(5)?,
                        created_at: row.get(6)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                other => DatabaseError::Sqlite(other),
            })
        })
    }

    /// Revert a plan to a previous version: restores title, content, and
    /// learning_objectives from the snapshot. Does NOT create a new version.
    pub fn revert_plan_to_version(&self, plan_id: &str, version: i32) -> Result<LessonPlan> {
        self.with_conn(|conn| {
            // Get the version snapshot
            let snapshot = conn
                .query_row(
                    "SELECT title, content, learning_objectives
                     FROM plan_versions WHERE plan_id = ?1 AND version = ?2",
                    params![plan_id, version],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                        ))
                    },
                )
                .map_err(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                    other => DatabaseError::Sqlite(other),
                })?;

            // Apply the snapshot to the plan
            let updated = conn.execute(
                "UPDATE lesson_plans SET title = ?1, content = ?2, learning_objectives = ?3, status = 'draft', updated_at = datetime('now') WHERE id = ?4",
                params![snapshot.0, snapshot.1, snapshot.2, plan_id],
            )?;
            if updated == 0 {
                return Err(DatabaseError::NotFound);
            }

            self.get_lesson_plan_inner(conn, plan_id)
        })
    }

    // ── Tags ──────────────────────────────────────────────────

    pub fn create_tag(&self, input: &NewTag) -> Result<Tag> {
        let id = uuid::Uuid::new_v4().to_string();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO tags (id, name, color) VALUES (?1, ?2, ?3)",
                params![id, input.name, input.color],
            )?;
            conn.query_row(
                "SELECT id, name, color, created_at FROM tags WHERE id = ?1",
                params![id],
                |row| {
                    Ok(Tag {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        color: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                other => DatabaseError::Sqlite(other),
            })
        })
    }

    pub fn list_tags(&self) -> Result<Vec<Tag>> {
        self.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT id, name, color, created_at FROM tags ORDER BY name")?;
            let rows = stmt.query_map([], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn update_tag(&self, id: &str, input: &NewTag) -> Result<Tag> {
        self.with_conn(|conn| {
            let updated = conn.execute(
                "UPDATE tags SET name = ?1, color = ?2 WHERE id = ?3",
                params![input.name, input.color, id],
            )?;
            if updated == 0 {
                return Err(DatabaseError::NotFound);
            }
            conn.query_row(
                "SELECT id, name, color, created_at FROM tags WHERE id = ?1",
                params![id],
                |row| {
                    Ok(Tag {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        color: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                other => DatabaseError::Sqlite(other),
            })
        })
    }

    pub fn delete_tag(&self, id: &str) -> Result<()> {
        self.with_conn(|conn| {
            let deleted = conn.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
            if deleted == 0 {
                return Err(DatabaseError::NotFound);
            }
            Ok(())
        })
    }

    // ── Plan-Tag associations ────────────────────────────────

    pub fn add_tag_to_plan(&self, plan_id: &str, tag_id: &str) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO plan_tags (plan_id, tag_id) VALUES (?1, ?2)",
                params![plan_id, tag_id],
            )?;
            Ok(())
        })
    }

    pub fn remove_tag_from_plan(&self, plan_id: &str, tag_id: &str) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM plan_tags WHERE plan_id = ?1 AND tag_id = ?2",
                params![plan_id, tag_id],
            )?;
            Ok(())
        })
    }

    pub fn get_tags_for_plan(&self, plan_id: &str) -> Result<Vec<Tag>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT t.id, t.name, t.color, t.created_at
                 FROM tags t
                 INNER JOIN plan_tags pt ON pt.tag_id = t.id
                 WHERE pt.plan_id = ?1
                 ORDER BY t.name",
            )?;
            let rows = stmt.query_map(params![plan_id], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    // ── Library queries ──────────────────────────────────────

    pub fn list_library_plans(&self, query: &LibraryQuery) -> Result<Vec<LibraryPlanCard>> {
        let result = self.with_conn(|conn| {
            let mut sql = String::from(
                "SELECT DISTINCT lp.id, lp.title, lp.status, lp.source_type, lp.version, lp.created_at, lp.updated_at
                 FROM lesson_plans lp",
            );
            let mut conditions: Vec<String> = Vec::new();
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            let mut param_index = 1;

            // Filter by tag_ids (join with plan_tags)
            if let Some(tag_ids) = &query.tag_ids {
                if !tag_ids.is_empty() {
                    sql.push_str(" INNER JOIN plan_tags pt ON pt.plan_id = lp.id");
                    let placeholders: Vec<String> = tag_ids
                        .iter()
                        .map(|_| {
                            let p = format!("?{}", param_index);
                            param_index += 1;
                            p
                        })
                        .collect();
                    conditions.push(format!("pt.tag_id IN ({})", placeholders.join(", ")));
                    for tag_id in tag_ids {
                        param_values.push(Box::new(tag_id.clone()));
                    }
                }
            }

            // Filter by source_type
            if let Some(source_type) = &query.source_type {
                conditions.push(format!("lp.source_type = ?{}", param_index));
                param_index += 1;
                param_values.push(Box::new(source_type.clone()));
            }

            // Full-text search via FTS5 with prefix matching
            if let Some(search) = &query.search {
                if !search.is_empty() {
                    let sanitized = super::fts::sanitize_fts_query(search);
                    if !sanitized.is_empty() {
                        sql.push_str(
                            " INNER JOIN lesson_plans_fts fts ON fts.rowid = lp.rowid",
                        );
                        conditions.push(format!("lesson_plans_fts MATCH ?{}", param_index));
                        param_index += 1;
                        param_values.push(Box::new(sanitized));
                    }
                }
            }

            let _ = param_index; // suppress unused warning

            if !conditions.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&conditions.join(" AND "));
            }

            sql.push_str(" ORDER BY lp.updated_at DESC");

            let mut stmt = conn.prepare(&sql)?;
            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|p| p.as_ref()).collect();

            let rows = stmt.query_map(param_refs.as_slice(), |row| {
                Ok(LibraryPlanCard {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    status: row.get(2)?,
                    source_type: row.get(3)?,
                    version: row.get(4)?,
                    tags: Vec::new(), // populated below
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?;

            let mut plans: Vec<LibraryPlanCard> =
                rows.collect::<std::result::Result<Vec<_>, _>>()?;

            // Fetch tags for each plan
            for plan in &mut plans {
                let mut tag_stmt = conn.prepare(
                    "SELECT t.id, t.name, t.color, t.created_at
                     FROM tags t
                     INNER JOIN plan_tags pt ON pt.tag_id = t.id
                     WHERE pt.plan_id = ?1
                     ORDER BY t.name",
                )?;
                let tag_rows = tag_stmt.query_map(params![plan.id], |row| {
                    Ok(Tag {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        color: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                })?;
                plan.tags = tag_rows.collect::<std::result::Result<Vec<_>, _>>()?;
            }

            Ok(plans)
        })?;

        // Fuzzy fallback: if FTS5 returned nothing and we had a search term,
        // try fuzzy matching for typo tolerance.
        if result.is_empty() {
            if let Some(search) = &query.search {
                if !search.trim().is_empty() && query.source_type.is_none() && query.tag_ids.is_none() {
                    // Pure search with no other filters — use fuzzy fallback
                    let fuzzy_results = self.search_fuzzy(search, 20)?;
                    if !fuzzy_results.is_empty() {
                        let fuzzy_ids: Vec<String> = fuzzy_results.iter().map(|r| r.lesson_plan_id.clone()).collect();
                        return self.with_conn(|conn| {
                            let placeholders: String = fuzzy_ids.iter().enumerate()
                                .map(|(i, _)| format!("?{}", i + 1))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let sql = format!(
                                "SELECT lp.id, lp.title, lp.status, lp.source_type, lp.version, lp.created_at, lp.updated_at
                                 FROM lesson_plans lp
                                 WHERE lp.id IN ({})",
                                placeholders
                            );
                            let mut stmt = conn.prepare(&sql)?;
                            let param_refs: Vec<&dyn rusqlite::types::ToSql> = fuzzy_ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();
                            let rows = stmt.query_map(param_refs.as_slice(), |row| {
                                Ok(LibraryPlanCard {
                                    id: row.get(0)?,
                                    title: row.get(1)?,
                                    status: row.get(2)?,
                                    source_type: row.get(3)?,
                                    version: row.get(4)?,
                                    tags: Vec::new(),
                                    created_at: row.get(5)?,
                                    updated_at: row.get(6)?,
                                })
                            })?;
                            let mut plans: Vec<LibraryPlanCard> = rows.collect::<std::result::Result<Vec<_>, _>>()?;

                            // Sort by fuzzy result order
                            let id_order: std::collections::HashMap<&str, usize> = fuzzy_ids.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
                            plans.sort_by_key(|p| id_order.get(p.id.as_str()).copied().unwrap_or(usize::MAX));

                            // Fetch tags
                            for plan in &mut plans {
                                let mut tag_stmt = conn.prepare(
                                    "SELECT t.id, t.name, t.color, t.created_at
                                     FROM tags t
                                     INNER JOIN plan_tags pt ON pt.tag_id = t.id
                                     WHERE pt.plan_id = ?1
                                     ORDER BY t.name",
                                )?;
                                let tag_rows = tag_stmt.query_map(params![plan.id], |row| {
                                    Ok(Tag {
                                        id: row.get(0)?,
                                        name: row.get(1)?,
                                        color: row.get(2)?,
                                        created_at: row.get(3)?,
                                    })
                                })?;
                                plan.tags = tag_rows.collect::<std::result::Result<Vec<_>, _>>()?;
                            }

                            Ok(plans)
                        });
                    }
                }
            }
        }

        Ok(result)
    }

    // ── App Settings ──────────────────────────────────────────

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.with_conn(|conn| {
            match conn.query_row(
                "SELECT value FROM app_settings WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            ) {
                Ok(val) => Ok(Some(val)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(DatabaseError::Sqlite(e)),
            }
        })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, datetime('now'))
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = datetime('now')",
                params![key, value],
            )?;
            Ok(())
        })
    }

    // ── Teaching Templates ───────────────────────────────────

    pub fn create_teaching_template(
        &self,
        source_doc_id: Option<&str>,
        source_doc_name: Option<&str>,
        template_json: &str,
    ) -> Result<TeachingTemplate> {
        let id = uuid::Uuid::new_v4().to_string();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO teaching_templates (id, source_doc_id, source_doc_name, template_json)
                 VALUES (?1, ?2, ?3, ?4)",
                params![id, source_doc_id, source_doc_name, template_json],
            )?;
            self.get_teaching_template_inner(conn, &id)
        })
    }

    pub fn create_teaching_template_on_conn(
        conn: &rusqlite::Connection,
        source_doc_id: Option<&str>,
        source_doc_name: Option<&str>,
        template_json: &str,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO teaching_templates (id, source_doc_id, source_doc_name, template_json)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, source_doc_id, source_doc_name, template_json],
        )?;
        Ok(id)
    }

    pub fn get_teaching_template(&self, id: &str) -> Result<TeachingTemplate> {
        self.with_conn(|conn| self.get_teaching_template_inner(conn, id))
    }

    fn get_teaching_template_inner(
        &self,
        conn: &rusqlite::Connection,
        id: &str,
    ) -> Result<TeachingTemplate> {
        conn.query_row(
            "SELECT id, source_doc_id, source_doc_name, template_json, created_at, updated_at
             FROM teaching_templates WHERE id = ?1",
            params![id],
            |row| {
                Ok(TeachingTemplate {
                    id: row.get(0)?,
                    source_doc_id: row.get(1)?,
                    source_doc_name: row.get(2)?,
                    template_json: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
            other => DatabaseError::Sqlite(other),
        })
    }

    pub fn get_teaching_template_by_source(&self, source_doc_id: &str) -> Result<TeachingTemplate> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, source_doc_id, source_doc_name, template_json, created_at, updated_at
                 FROM teaching_templates WHERE source_doc_id = ?1
                 ORDER BY updated_at DESC LIMIT 1",
                params![source_doc_id],
                |row| {
                    Ok(TeachingTemplate {
                        id: row.get(0)?,
                        source_doc_id: row.get(1)?,
                        source_doc_name: row.get(2)?,
                        template_json: row.get(3)?,
                        created_at: row.get(4)?,
                        updated_at: row.get(5)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                other => DatabaseError::Sqlite(other),
            })
        })
    }

    pub fn list_teaching_templates(&self) -> Result<Vec<TeachingTemplate>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, source_doc_id, source_doc_name, template_json, created_at, updated_at
                 FROM teaching_templates ORDER BY updated_at DESC",
            )?;
            let templates = stmt
                .query_map([], |row| {
                    Ok(TeachingTemplate {
                        id: row.get(0)?,
                        source_doc_id: row.get(1)?,
                        source_doc_name: row.get(2)?,
                        template_json: row.get(3)?,
                        created_at: row.get(4)?,
                        updated_at: row.get(5)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(templates)
        })
    }

    pub fn get_active_teaching_template(&self) -> Result<TeachingTemplate> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, source_doc_id, source_doc_name, template_json, created_at, updated_at
                 FROM teaching_templates ORDER BY updated_at DESC LIMIT 1",
                [],
                |row| {
                    Ok(TeachingTemplate {
                        id: row.get(0)?,
                        source_doc_id: row.get(1)?,
                        source_doc_name: row.get(2)?,
                        template_json: row.get(3)?,
                        created_at: row.get(4)?,
                        updated_at: row.get(5)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                other => DatabaseError::Sqlite(other),
            })
        })
    }

    pub fn delete_teaching_template(&self, id: &str) -> Result<()> {
        self.with_conn(|conn| {
            let rows = conn.execute(
                "DELETE FROM teaching_templates WHERE id = ?1",
                params![id],
            )?;
            if rows == 0 {
                Err(DatabaseError::NotFound)
            } else {
                Ok(())
            }
        })
    }

    pub fn delete_teaching_templates_by_source(
        conn: &rusqlite::Connection,
        source_doc_id: &str,
    ) -> Result<()> {
        conn.execute(
            "DELETE FROM teaching_templates WHERE source_doc_id = ?1",
            params![source_doc_id],
        )?;
        Ok(())
    }

    // ── Recurring Events ──────────────────────────────────────

    pub fn create_recurring_event(&self, input: &NewRecurringEvent) -> Result<RecurringEvent> {
        let id = uuid::Uuid::new_v4().to_string();
        let event_type = input.event_type.as_deref().unwrap_or("fixed");
        let details_vary_daily = input.details_vary_daily.unwrap_or(false);
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO recurring_events (id, name, event_type, linked_to, details_vary_daily)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, input.name, event_type, input.linked_to, details_vary_daily],
            )?;
            self.get_recurring_event_inner(conn, &id)
        })
    }

    pub fn get_recurring_event(&self, id: &str) -> Result<RecurringEvent> {
        self.with_conn(|conn| self.get_recurring_event_inner(conn, id))
    }

    fn get_recurring_event_inner(
        &self,
        conn: &rusqlite::Connection,
        id: &str,
    ) -> Result<RecurringEvent> {
        conn.query_row(
            "SELECT id, name, event_type, linked_to, details_vary_daily, created_at, updated_at
             FROM recurring_events WHERE id = ?1",
            params![id],
            |row| {
                Ok(RecurringEvent {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    event_type: row.get(2)?,
                    linked_to: row.get(3)?,
                    details_vary_daily: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
            other => DatabaseError::Sqlite(other),
        })
    }

    pub fn list_recurring_events(&self) -> Result<Vec<RecurringEvent>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, event_type, linked_to, details_vary_daily, created_at, updated_at
                 FROM recurring_events ORDER BY name",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(RecurringEvent {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    event_type: row.get(2)?,
                    linked_to: row.get(3)?,
                    details_vary_daily: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn update_recurring_event(
        &self,
        id: &str,
        input: &NewRecurringEvent,
    ) -> Result<RecurringEvent> {
        let event_type = input.event_type.as_deref().unwrap_or("fixed");
        let details_vary_daily = input.details_vary_daily.unwrap_or(false);
        self.with_conn(|conn| {
            let updated = conn.execute(
                "UPDATE recurring_events SET name = ?1, event_type = ?2, linked_to = ?3,
                 details_vary_daily = ?4, updated_at = datetime('now') WHERE id = ?5",
                params![input.name, event_type, input.linked_to, details_vary_daily, id],
            )?;
            if updated == 0 {
                return Err(DatabaseError::NotFound);
            }
            self.get_recurring_event_inner(conn, id)
        })
    }

    pub fn delete_recurring_event(&self, id: &str) -> Result<()> {
        self.with_conn(|conn| {
            let deleted =
                conn.execute("DELETE FROM recurring_events WHERE id = ?1", params![id])?;
            if deleted == 0 {
                return Err(DatabaseError::NotFound);
            }
            Ok(())
        })
    }

    // ── Event Occurrences ─────────────────────────────────────

    pub fn create_event_occurrence(&self, input: &NewEventOccurrence) -> Result<EventOccurrence> {
        let id = uuid::Uuid::new_v4().to_string();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO event_occurrences (id, event_id, day_of_week, start_time, end_time)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, input.event_id, input.day_of_week, input.start_time, input.end_time],
            )?;
            conn.query_row(
                "SELECT id, event_id, day_of_week, start_time, end_time
                 FROM event_occurrences WHERE id = ?1",
                params![id],
                |row| {
                    Ok(EventOccurrence {
                        id: row.get(0)?,
                        event_id: row.get(1)?,
                        day_of_week: row.get(2)?,
                        start_time: row.get(3)?,
                        end_time: row.get(4)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                other => DatabaseError::Sqlite(other),
            })
        })
    }

    pub fn list_event_occurrences(&self, event_id: &str) -> Result<Vec<EventOccurrence>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, event_id, day_of_week, start_time, end_time
                 FROM event_occurrences WHERE event_id = ?1 ORDER BY day_of_week, start_time",
            )?;
            let rows = stmt.query_map(params![event_id], |row| {
                Ok(EventOccurrence {
                    id: row.get(0)?,
                    event_id: row.get(1)?,
                    day_of_week: row.get(2)?,
                    start_time: row.get(3)?,
                    end_time: row.get(4)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn delete_event_occurrence(&self, id: &str) -> Result<()> {
        self.with_conn(|conn| {
            let deleted =
                conn.execute("DELETE FROM event_occurrences WHERE id = ?1", params![id])?;
            if deleted == 0 {
                return Err(DatabaseError::NotFound);
            }
            Ok(())
        })
    }

    // ── School Calendar ───────────────────────────────────────

    pub fn get_school_calendar(&self) -> Result<SchoolCalendar> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, year_start, year_end, created_at, updated_at
                 FROM school_calendar ORDER BY created_at DESC LIMIT 1",
                [],
                |row| {
                    Ok(SchoolCalendar {
                        id: row.get(0)?,
                        year_start: row.get(1)?,
                        year_end: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                other => DatabaseError::Sqlite(other),
            })
        })
    }

    pub fn upsert_school_calendar(&self, input: &NewSchoolCalendar) -> Result<SchoolCalendar> {
        self.with_conn(|conn| {
            // Check if a calendar already exists.
            let existing_id: Option<String> = conn
                .query_row(
                    "SELECT id FROM school_calendar ORDER BY created_at DESC LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .ok();

            let id = existing_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

            conn.execute(
                "INSERT INTO school_calendar (id, year_start, year_end)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET
                   year_start = excluded.year_start,
                   year_end = excluded.year_end,
                   updated_at = datetime('now')",
                params![id, input.year_start, input.year_end],
            )?;

            conn.query_row(
                "SELECT id, year_start, year_end, created_at, updated_at
                 FROM school_calendar WHERE id = ?1",
                params![id],
                |row| {
                    Ok(SchoolCalendar {
                        id: row.get(0)?,
                        year_start: row.get(1)?,
                        year_end: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                other => DatabaseError::Sqlite(other),
            })
        })
    }

    // ── Calendar Exceptions ───────────────────────────────────

    pub fn add_calendar_exception(
        &self,
        input: &NewCalendarException,
    ) -> Result<CalendarException> {
        let id = uuid::Uuid::new_v4().to_string();
        let label = input.label.as_deref().unwrap_or("");
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO calendar_exceptions (id, calendar_id, date, exception_type, label)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, input.calendar_id, input.date, input.exception_type, label],
            )?;
            conn.query_row(
                "SELECT id, calendar_id, date, exception_type, label
                 FROM calendar_exceptions WHERE id = ?1",
                params![id],
                |row| {
                    Ok(CalendarException {
                        id: row.get(0)?,
                        calendar_id: row.get(1)?,
                        date: row.get(2)?,
                        exception_type: row.get(3)?,
                        label: row.get(4)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DatabaseError::NotFound,
                other => DatabaseError::Sqlite(other),
            })
        })
    }

    pub fn list_calendar_exceptions(
        &self,
        calendar_id: &str,
    ) -> Result<Vec<CalendarException>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, calendar_id, date, exception_type, label
                 FROM calendar_exceptions WHERE calendar_id = ?1 ORDER BY date",
            )?;
            let rows = stmt.query_map(params![calendar_id], |row| {
                Ok(CalendarException {
                    id: row.get(0)?,
                    calendar_id: row.get(1)?,
                    date: row.get(2)?,
                    exception_type: row.get(3)?,
                    label: row.get(4)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn delete_calendar_exception(&self, id: &str) -> Result<()> {
        self.with_conn(|conn| {
            let deleted =
                conn.execute("DELETE FROM calendar_exceptions WHERE id = ?1", params![id])?;
            if deleted == 0 {
                return Err(DatabaseError::NotFound);
            }
            Ok(())
        })
    }

    // ── Onboarding Status (DB-backed) ─────────────────────────

    pub fn get_onboarding_status_json(&self) -> Result<Option<String>> {
        self.get_setting("onboarding_status")
    }

    pub fn set_onboarding_status_json(&self, json: &str) -> Result<()> {
        self.set_setting("onboarding_status", json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn test_subject_crud() {
        let db = test_db();

        // Create
        let subject = db
            .create_subject(&NewSubject {
                name: "Mathematics".into(),
                grade_level: Some("9th".into()),
                description: Some("Algebra and Geometry".into()),
            })
            .unwrap();
        assert_eq!(subject.name, "Mathematics");

        // Read
        let fetched = db.get_subject(&subject.id).unwrap();
        assert_eq!(fetched.name, "Mathematics");

        // List
        let all = db.list_subjects().unwrap();
        assert_eq!(all.len(), 1);

        // Update
        let updated = db
            .update_subject(
                &subject.id,
                &NewSubject {
                    name: "Math".into(),
                    grade_level: Some("10th".into()),
                    description: None,
                },
            )
            .unwrap();
        assert_eq!(updated.name, "Math");
        assert_eq!(updated.grade_level.as_deref(), Some("10th"));

        // Delete
        db.delete_subject(&subject.id).unwrap();
        assert!(matches!(
            db.get_subject(&subject.id),
            Err(DatabaseError::NotFound)
        ));
    }

    #[test]
    fn test_lesson_plan_crud() {
        let db = test_db();

        let subject = db
            .create_subject(&NewSubject {
                name: "Science".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();

        // Create
        let plan = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Photosynthesis".into(),
                content: Some("Plants convert sunlight...".into()),
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: Some("Understand photosynthesis".into()),
            })
            .unwrap();
        assert_eq!(plan.title, "Photosynthesis");
        assert_eq!(plan.status, "draft");

        // Read
        let fetched = db.get_lesson_plan(&plan.id).unwrap();
        assert_eq!(fetched.content, "Plants convert sunlight...");

        // Update content
        let updated = db
            .update_lesson_plan_content(&plan.id, "Updated content")
            .unwrap();
        assert_eq!(updated.content, "Updated content");

        // Update status
        let published = db
            .update_lesson_plan_status(&plan.id, "published")
            .unwrap();
        assert_eq!(published.status, "published");

        // List by subject
        let plans = db.list_lesson_plans_by_subject(&subject.id).unwrap();
        assert_eq!(plans.len(), 1);

        // Delete
        db.delete_lesson_plan(&plan.id).unwrap();
        assert!(matches!(
            db.get_lesson_plan(&plan.id),
            Err(DatabaseError::NotFound)
        ));
    }

    #[test]
    fn test_metadata_crud() {
        let db = test_db();

        let subject = db
            .create_subject(&NewSubject {
                name: "History".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();

        let plan = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "World War II".into(),
                content: None,
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();

        // Set metadata
        let meta = db
            .set_metadata(&NewMetadata {
                lesson_plan_id: plan.id.clone(),
                key: "duration".into(),
                value: "45 minutes".into(),
            })
            .unwrap();
        assert_eq!(meta.key, "duration");
        assert_eq!(meta.value, "45 minutes");

        // Upsert same key
        let updated = db
            .set_metadata(&NewMetadata {
                lesson_plan_id: plan.id.clone(),
                key: "duration".into(),
                value: "60 minutes".into(),
            })
            .unwrap();
        assert_eq!(updated.value, "60 minutes");

        // List metadata
        let all = db.get_metadata_for_plan(&plan.id).unwrap();
        assert_eq!(all.len(), 1);

        // Delete metadata
        db.delete_metadata(&plan.id, "duration").unwrap();
        let all = db.get_metadata_for_plan(&plan.id).unwrap();
        assert_eq!(all.len(), 0);
    }

    #[test]
    fn test_cascade_delete() {
        let db = test_db();

        let subject = db
            .create_subject(&NewSubject {
                name: "Art".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();

        let plan = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Watercolors".into(),
                content: None,
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();

        db.set_metadata(&NewMetadata {
            lesson_plan_id: plan.id.clone(),
            key: "medium".into(),
            value: "watercolor".into(),
        })
        .unwrap();

        // Deleting the subject should cascade-delete plans and their metadata.
        db.delete_subject(&subject.id).unwrap();
        assert!(matches!(
            db.get_lesson_plan(&plan.id),
            Err(DatabaseError::NotFound)
        ));
        let meta = db.get_metadata_for_plan(&plan.id).unwrap();
        assert_eq!(meta.len(), 0);
    }

    #[test]
    fn test_list_plans_without_embeddings() {
        let db = test_db();

        let subject = db
            .create_subject(&NewSubject {
                name: "Bio".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();

        let plan1 = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Plan 1".into(),
                content: Some("Content 1".into()),
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();

        let plan2 = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Plan 2".into(),
                content: Some("Content 2".into()),
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();

        // Both plans should be listed as unembedded.
        let unembedded = db.list_plans_without_embeddings().unwrap();
        assert_eq!(unembedded.len(), 2);

        // Recreate vec table with smaller dims for test.
        db.with_conn(|conn| {
            conn.execute_batch("DROP TABLE IF EXISTS lesson_plan_vectors")?;
            conn.execute_batch(
                "CREATE VIRTUAL TABLE lesson_plan_vectors USING vec0(embedding float[4])",
            )?;
            Ok(())
        })
        .unwrap();

        // Embed plan1.
        db.upsert_embedding(&plan1.id, &[1.0, 0.0, 0.0, 0.0])
            .unwrap();

        // Now only plan2 should be unembedded.
        let unembedded = db.list_plans_without_embeddings().unwrap();
        assert_eq!(unembedded.len(), 1);
        assert_eq!(unembedded[0].id, plan2.id);
    }

    // ── Plan Version Tests ───────────────────────────────────

    fn create_test_plan(db: &Database) -> LessonPlan {
        let subject = db
            .create_subject(&NewSubject {
                name: "Test Subject".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();
        db.create_lesson_plan(&NewLessonPlan {
            subject_id: subject.id.clone(),
            title: "Test Plan".into(),
            content: Some("Initial content".into()),
            source_doc_id: None,
            source_table_index: None,
            learning_objectives: Some("Learn things".into()),
        })
        .unwrap()
    }

    #[test]
    fn test_finalize_plan_creates_version() {
        let db = test_db();
        let plan = create_test_plan(&db);

        let v1 = db.finalize_plan(&plan.id).unwrap();
        assert_eq!(v1.version, 1);
        assert_eq!(v1.title, "Test Plan");
        assert_eq!(v1.content, "Initial content");
        assert_eq!(v1.learning_objectives.as_deref(), Some("Learn things"));
        assert_eq!(v1.plan_id, plan.id);

        // Plan should now be version 1 with "finalized" status
        let updated_plan = db.get_lesson_plan(&plan.id).unwrap();
        assert_eq!(updated_plan.status, "finalized");
    }

    #[test]
    fn test_finalize_plan_increments_version() {
        let db = test_db();
        let plan = create_test_plan(&db);

        let v1 = db.finalize_plan(&plan.id).unwrap();
        assert_eq!(v1.version, 1);

        // Update content then finalize again
        db.update_lesson_plan_content(&plan.id, "Updated content").unwrap();
        let v2 = db.finalize_plan(&plan.id).unwrap();
        assert_eq!(v2.version, 2);
        assert_eq!(v2.content, "Updated content");
    }

    #[test]
    fn test_list_plan_versions() {
        let db = test_db();
        let plan = create_test_plan(&db);

        // No versions yet
        let versions = db.list_plan_versions(&plan.id).unwrap();
        assert_eq!(versions.len(), 0);

        // Create two versions
        db.finalize_plan(&plan.id).unwrap();
        db.update_lesson_plan_content(&plan.id, "v2 content").unwrap();
        db.finalize_plan(&plan.id).unwrap();

        let versions = db.list_plan_versions(&plan.id).unwrap();
        assert_eq!(versions.len(), 2);
        // Newest first
        assert_eq!(versions[0].version, 2);
        assert_eq!(versions[1].version, 1);
    }

    #[test]
    fn test_get_plan_version() {
        let db = test_db();
        let plan = create_test_plan(&db);

        db.finalize_plan(&plan.id).unwrap();
        db.update_lesson_plan_content(&plan.id, "v2 content").unwrap();
        db.finalize_plan(&plan.id).unwrap();

        let v1 = db.get_plan_version(&plan.id, 1).unwrap();
        assert_eq!(v1.content, "Initial content");

        let v2 = db.get_plan_version(&plan.id, 2).unwrap();
        assert_eq!(v2.content, "v2 content");

        // Non-existent version
        assert!(matches!(
            db.get_plan_version(&plan.id, 99),
            Err(DatabaseError::NotFound)
        ));
    }

    #[test]
    fn test_revert_plan_to_version() {
        let db = test_db();
        let plan = create_test_plan(&db);

        // Finalize v1
        db.finalize_plan(&plan.id).unwrap();

        // Change content and finalize v2
        db.update_lesson_plan_content(&plan.id, "v2 content").unwrap();
        db.finalize_plan(&plan.id).unwrap();

        // Revert to v1
        let reverted = db.revert_plan_to_version(&plan.id, 1).unwrap();
        assert_eq!(reverted.content, "Initial content");
        assert_eq!(reverted.title, "Test Plan");
        assert_eq!(reverted.status, "draft");

        // Non-existent version
        assert!(matches!(
            db.revert_plan_to_version(&plan.id, 99),
            Err(DatabaseError::NotFound)
        ));
    }

    #[test]
    fn test_cascade_delete_plan_removes_versions() {
        let db = test_db();
        let plan = create_test_plan(&db);

        db.finalize_plan(&plan.id).unwrap();
        db.finalize_plan(&plan.id).unwrap();

        let versions = db.list_plan_versions(&plan.id).unwrap();
        assert_eq!(versions.len(), 2);

        // Delete the plan — versions should cascade-delete
        db.delete_lesson_plan(&plan.id).unwrap();
        let versions = db.list_plan_versions(&plan.id).unwrap();
        assert_eq!(versions.len(), 0);
    }

    #[test]
    fn test_finalize_nonexistent_plan() {
        let db = test_db();
        assert!(matches!(
            db.finalize_plan("nonexistent-id"),
            Err(DatabaseError::NotFound)
        ));
    }

    // ── App Settings Tests ────────────────────────────────────

    #[test]
    fn test_app_settings_crud() {
        let db = test_db();

        // Getting a non-existent setting returns None.
        assert_eq!(db.get_setting("teacher_name").unwrap(), None);

        // Set a value.
        db.set_setting("teacher_name", "Madison").unwrap();
        assert_eq!(
            db.get_setting("teacher_name").unwrap(),
            Some("Madison".into())
        );

        // Upsert overwrites the value.
        db.set_setting("teacher_name", "Jordan").unwrap();
        assert_eq!(
            db.get_setting("teacher_name").unwrap(),
            Some("Jordan".into())
        );

        // Multiple keys don't interfere.
        db.set_setting("theme", "dark").unwrap();
        assert_eq!(
            db.get_setting("teacher_name").unwrap(),
            Some("Jordan".into())
        );
        assert_eq!(db.get_setting("theme").unwrap(), Some("dark".into()));
    }

    // ── Teaching Template Tests ──────────────────────────────

    // ── LTP Document Tests ────────────────────────────────────

    #[test]
    fn test_ltp_document_import_new() {
        let db = test_db();

        let result = db
            .import_ltp_document("plan.html", "abc123hash", Some("2025-2026"), "ltp", "<html>content</html>")
            .unwrap();

        match result {
            LtpImportResult::Imported(doc) => {
                assert_eq!(doc.filename, "plan.html");
                assert_eq!(doc.file_hash, "abc123hash");
                assert_eq!(doc.school_year.as_deref(), Some("2025-2026"));
                assert_eq!(doc.doc_type, "ltp");
                assert_eq!(doc.raw_html, "<html>content</html>");
            }
            LtpImportResult::Skipped { .. } => panic!("Expected Imported, got Skipped"),
        }
    }

    #[test]
    fn test_ltp_document_import_skip_duplicate() {
        let db = test_db();

        // First import.
        db.import_ltp_document("plan.html", "samehash", None, "ltp", "<html>content</html>")
            .unwrap();

        // Second import with same hash — should skip.
        let result = db
            .import_ltp_document("plan.html", "samehash", None, "ltp", "<html>content</html>")
            .unwrap();

        match result {
            LtpImportResult::Skipped { filename, .. } => {
                assert_eq!(filename, "plan.html");
            }
            LtpImportResult::Imported(_) => panic!("Expected Skipped, got Imported"),
        }
    }

    #[test]
    fn test_ltp_document_import_overwrite_different_hash() {
        let db = test_db();

        // First import.
        let first = db
            .import_ltp_document("plan.html", "hash1", None, "ltp", "<html>v1</html>")
            .unwrap();
        let first_id = match &first {
            LtpImportResult::Imported(doc) => doc.id.clone(),
            _ => panic!("Expected Imported"),
        };

        // Add a grid cell to the first document.
        db.insert_ltp_grid_cell(&first_id, 0, 0, Some("Math"), Some("Sep"), None, None, None, None, None)
            .unwrap();
        assert_eq!(db.list_ltp_grid_cells(&first_id).unwrap().len(), 1);

        // Second import with different hash — should overwrite.
        let result = db
            .import_ltp_document("plan.html", "hash2", None, "ltp", "<html>v2</html>")
            .unwrap();

        match result {
            LtpImportResult::Imported(doc) => {
                assert_eq!(doc.id, first_id); // Same document ID.
                assert_eq!(doc.file_hash, "hash2");
                assert_eq!(doc.raw_html, "<html>v2</html>");
            }
            _ => panic!("Expected Imported"),
        }

        // Grid cells should have been cleared.
        assert_eq!(db.list_ltp_grid_cells(&first_id).unwrap().len(), 0);
    }

    #[test]
    fn test_ltp_document_list_and_delete() {
        let db = test_db();

        db.import_ltp_document("a.html", "h1", None, "ltp", "<html>a</html>")
            .unwrap();
        db.import_ltp_document("b.html", "h2", None, "calendar", "<html>b</html>")
            .unwrap();

        let docs = db.list_ltp_documents().unwrap();
        assert_eq!(docs.len(), 2);

        let id = docs[0].id.clone();
        db.delete_ltp_document(&id).unwrap();
        assert_eq!(db.list_ltp_documents().unwrap().len(), 1);

        // Delete non-existent.
        assert!(matches!(
            db.delete_ltp_document("nonexistent"),
            Err(DatabaseError::NotFound)
        ));
    }

    #[test]
    fn test_ltp_grid_cell_crud() {
        let db = test_db();

        let doc = match db
            .import_ltp_document("plan.html", "hash", None, "ltp", "<html></html>")
            .unwrap()
        {
            LtpImportResult::Imported(d) => d,
            _ => panic!("Expected Imported"),
        };

        let cell = db
            .insert_ltp_grid_cell(
                &doc.id, 0, 1, Some("Science"), Some("October"),
                Some("<b>Cells</b>"), Some("Cells"), Some("#ff0000"),
                Some("Unit 2"), Some("#00ff00"),
            )
            .unwrap();

        assert_eq!(cell.document_id, doc.id);
        assert_eq!(cell.row_index, 0);
        assert_eq!(cell.col_index, 1);
        assert_eq!(cell.subject.as_deref(), Some("Science"));
        assert_eq!(cell.month.as_deref(), Some("October"));
        assert_eq!(cell.unit_name.as_deref(), Some("Unit 2"));

        let cells = db.list_ltp_grid_cells(&doc.id).unwrap();
        assert_eq!(cells.len(), 1);

        // Deleting the document should cascade-delete grid cells.
        db.delete_ltp_document(&doc.id).unwrap();
        let cells = db.list_ltp_grid_cells(&doc.id).unwrap();
        assert_eq!(cells.len(), 0);
    }

    #[test]
    fn test_school_calendar_entry_crud() {
        let db = test_db();

        let doc = match db
            .import_ltp_document("cal.html", "hash", None, "calendar", "<html></html>")
            .unwrap()
        {
            LtpImportResult::Imported(d) => d,
            _ => panic!("Expected Imported"),
        };

        let entry = db
            .insert_school_calendar_entry(
                &doc.id,
                Some("2025-09-01"),
                Some(1),
                Some("Unit 1"),
                Some("#aabbcc"),
                false,
                None,
                Some("First day"),
            )
            .unwrap();

        assert_eq!(entry.document_id, doc.id);
        assert_eq!(entry.date.as_deref(), Some("2025-09-01"));
        assert_eq!(entry.day_number, Some(1));
        assert!(!entry.is_holiday);
        assert_eq!(entry.notes.as_deref(), Some("First day"));

        // Holiday entry.
        db.insert_school_calendar_entry(
            &doc.id,
            Some("2025-12-25"),
            None,
            None,
            None,
            true,
            Some("Christmas"),
            None,
        )
        .unwrap();

        let entries = db.list_school_calendar_entries(&doc.id).unwrap();
        assert_eq!(entries.len(), 2);

        // Deleting the document should cascade-delete entries.
        db.delete_ltp_document(&doc.id).unwrap();
        let entries = db.list_school_calendar_entries(&doc.id).unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_ltp_cells_for_month() {
        let db = test_db();

        let doc = match db
            .import_ltp_document("plan.html", "hash", None, "ltp", "<html></html>")
            .unwrap()
        {
            LtpImportResult::Imported(d) => d,
            _ => panic!("Expected Imported"),
        };

        // Insert cells for different months.
        db.insert_ltp_grid_cell(&doc.id, 0, 0, Some("Math"), Some("March"), None, Some("Addition and subtraction"), None, Some("Unit 3"), None).unwrap();
        db.insert_ltp_grid_cell(&doc.id, 1, 0, Some("Reading"), Some("March"), None, Some("Phonics: letter groups"), None, Some("Unit 3"), None).unwrap();
        db.insert_ltp_grid_cell(&doc.id, 0, 1, Some("Math"), Some("April"), None, Some("Geometry"), None, Some("Unit 4"), None).unwrap();
        // Empty cell should be excluded.
        db.insert_ltp_grid_cell(&doc.id, 2, 0, Some("Writing"), Some("March"), None, Some(""), None, None, None).unwrap();

        let march_cells = db.get_ltp_cells_for_month("March").unwrap();
        assert_eq!(march_cells.len(), 2);
        assert_eq!(march_cells[0].subject.as_deref(), Some("Math"));
        assert_eq!(march_cells[1].subject.as_deref(), Some("Reading"));

        let april_cells = db.get_ltp_cells_for_month("April").unwrap();
        assert_eq!(april_cells.len(), 1);

        let july_cells = db.get_ltp_cells_for_month("July").unwrap();
        assert_eq!(july_cells.len(), 0);
    }

    #[test]
    fn test_unit_for_month() {
        let db = test_db();

        let doc = match db
            .import_ltp_document("plan.html", "hash", None, "ltp", "<html></html>")
            .unwrap()
        {
            LtpImportResult::Imported(d) => d,
            _ => panic!("Expected Imported"),
        };

        db.insert_ltp_grid_cell(&doc.id, 0, 0, Some("Math"), Some("March"), None, Some("content"), None, Some("Unit 3: Wind and Water"), None).unwrap();

        let unit = db.get_unit_for_month("March").unwrap();
        assert_eq!(unit, Some("Unit 3: Wind and Water".to_string()));

        let no_unit = db.get_unit_for_month("July").unwrap();
        assert_eq!(no_unit, None);
    }

    #[test]
    fn test_calendar_entries_for_range() {
        let db = test_db();

        let doc = match db
            .import_ltp_document("cal.html", "hash", None, "calendar", "<html></html>")
            .unwrap()
        {
            LtpImportResult::Imported(d) => d,
            _ => panic!("Expected Imported"),
        };

        db.insert_school_calendar_entry(&doc.id, Some("2025-03-17"), None, None, None, false, None, Some("St Patrick's")).unwrap();
        db.insert_school_calendar_entry(&doc.id, Some("2025-03-20"), None, None, None, true, Some("SPRING BK"), None).unwrap();
        db.insert_school_calendar_entry(&doc.id, Some("2025-04-01"), None, None, None, false, None, Some("April Fools")).unwrap();

        let entries = db.get_calendar_entries_for_range("2025-03-17", "2025-03-23").unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].notes.as_deref(), Some("St Patrick's"));
        assert!(entries[1].is_holiday);
    }

    #[test]
    fn test_has_ltp_documents() {
        let db = test_db();
        assert!(!db.has_ltp_documents().unwrap());

        db.import_ltp_document("plan.html", "hash", None, "ltp", "<html></html>").unwrap();
        assert!(db.has_ltp_documents().unwrap());
    }

    #[test]
    fn test_teaching_template_crud() {
        let db = test_db();

        let template_json = r#"{"table_structure":{"layout_type":"schedule_grid","columns":["Time","Monday","Tuesday"],"row_categories":[],"column_count":3},"time_slots":["9:00-9:30"],"color_scheme":{"mappings":[]},"content_patterns":{"cell_content_types":[],"has_links":false,"has_rich_formatting":false},"recurring_elements":{"subjects":[],"activities":["Math"]}}"#;

        let template = db
            .create_teaching_template(Some("doc123"), Some("My Plans"), template_json)
            .unwrap();
        assert_eq!(template.source_doc_id.as_deref(), Some("doc123"));
        assert_eq!(template.source_doc_name.as_deref(), Some("My Plans"));

        let fetched = db.get_teaching_template(&template.id).unwrap();
        assert_eq!(fetched.id, template.id);
        assert_eq!(fetched.template_json, template_json);

        let by_source = db.get_teaching_template_by_source("doc123").unwrap();
        assert_eq!(by_source.id, template.id);

        let all = db.list_teaching_templates().unwrap();
        assert_eq!(all.len(), 1);

        let active = db.get_active_teaching_template().unwrap();
        assert_eq!(active.id, template.id);

        db.delete_teaching_template(&template.id).unwrap();
        assert!(matches!(
            db.get_teaching_template(&template.id),
            Err(DatabaseError::NotFound)
        ));
    }

    #[test]
    fn test_teaching_template_get_nonexistent() {
        let db = test_db();
        assert!(matches!(
            db.get_teaching_template("nonexistent"),
            Err(DatabaseError::NotFound)
        ));
    }

    #[test]
    fn test_teaching_template_active_when_none() {
        let db = test_db();
        assert!(matches!(
            db.get_active_teaching_template(),
            Err(DatabaseError::NotFound)
        ));
    }

    #[test]
    fn test_teaching_template_delete_nonexistent() {
        let db = test_db();
        assert!(matches!(
            db.delete_teaching_template("nonexistent"),
            Err(DatabaseError::NotFound)
        ));
    }

    #[test]
    fn test_teaching_template_on_conn_and_delete_by_source() {
        let db = test_db();

        let id = db.with_conn(|conn| {
            Database::create_teaching_template_on_conn(
                conn,
                Some("src_doc"),
                Some("Source Doc"),
                r#"{"table_structure":{"layout_type":"standard_table"}}"#,
            )
        }).unwrap();

        let fetched = db.get_teaching_template(&id).unwrap();
        assert_eq!(fetched.source_doc_id.as_deref(), Some("src_doc"));

        db.with_conn(|conn| {
            Database::delete_teaching_templates_by_source(conn, "src_doc")
        }).unwrap();

        assert!(matches!(
            db.get_teaching_template(&id),
            Err(DatabaseError::NotFound)
        ));
    }

    #[test]
    fn test_teaching_template_json_roundtrip() {
        let db = test_db();

        let schema = TeachingTemplateSchema {
            color_scheme: ColorScheme {
                mappings: vec![ColorMapping {
                    color: "#9900ff".to_string(),
                    category: "header".to_string(),
                    frequency: 5,
                }],
            },
            table_structure: TableStructure {
                layout_type: "schedule_grid".to_string(),
                columns: vec!["Time".to_string(), "Monday".to_string()],
                row_categories: vec!["Math".to_string()],
                column_count: 2,
                column_semantic: Some("days_of_week".to_string()),
                row_semantic: Some("time_slots".to_string()),
            },
            time_slots: vec!["9:00-9:30".to_string()],
            content_patterns: ContentPatterns {
                cell_content_types: vec!["activity_name".to_string()],
                has_links: true,
                has_rich_formatting: false,
            },
            recurring_elements: RecurringElements {
                subjects: vec!["Biology".to_string()],
                activities: vec!["Morning Circle".to_string()],
            },
            daily_routine: vec![],
        };

        let json = serde_json::to_string(&schema).unwrap();
        let template = db
            .create_teaching_template(Some("doc1"), Some("Doc"), &json)
            .unwrap();

        let fetched = db.get_teaching_template(&template.id).unwrap();
        let parsed: TeachingTemplateSchema =
            serde_json::from_str(&fetched.template_json).unwrap();

        assert_eq!(parsed.table_structure.layout_type, "schedule_grid");
        assert_eq!(parsed.color_scheme.mappings.len(), 1);
        assert_eq!(parsed.color_scheme.mappings[0].color, "#9900ff");
        assert_eq!(parsed.time_slots, vec!["9:00-9:30"]);
        assert!(parsed.content_patterns.has_links);
        assert_eq!(parsed.recurring_elements.subjects, vec!["Biology"]);
        assert_eq!(parsed.recurring_elements.activities, vec!["Morning Circle"]);
    }

}
