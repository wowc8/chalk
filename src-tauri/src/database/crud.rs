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
}
