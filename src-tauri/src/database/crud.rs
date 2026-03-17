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
        self.with_conn(|conn| {
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

            // Search by title
            if let Some(search) = &query.search {
                if !search.is_empty() {
                    conditions.push(format!("lp.title LIKE ?{}", param_index));
                    param_index += 1;
                    param_values.push(Box::new(format!("%{}%", search)));
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
        })
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
}
