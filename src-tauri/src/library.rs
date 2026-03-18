use crate::database::{
    FtsSearchResult, LessonPlan, LibraryPlanCard, LibraryQuery, NewLessonPlan, NewTag, PlanVersion,
    Tag,
};
use crate::AppState;

// ── Tag commands ─────────────────────────────────────────────

#[tauri::command]
pub fn create_tag(
    state: tauri::State<'_, AppState>,
    name: String,
    color: Option<String>,
) -> Result<Tag, String> {
    state
        .db
        .create_tag(&NewTag { name, color })
        .map_err(|e| format!("{}", e))
}

#[tauri::command]
pub fn list_tags(state: tauri::State<'_, AppState>) -> Result<Vec<Tag>, String> {
    state.db.list_tags().map_err(|e| format!("{}", e))
}

#[tauri::command]
pub fn update_tag(
    state: tauri::State<'_, AppState>,
    id: String,
    name: String,
    color: Option<String>,
) -> Result<Tag, String> {
    state
        .db
        .update_tag(&id, &NewTag { name, color })
        .map_err(|e| format!("{}", e))
}

#[tauri::command]
pub fn delete_tag(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    state.db.delete_tag(&id).map_err(|e| format!("{}", e))
}

// ── Plan-Tag associations ────────────────────────────────────

#[tauri::command]
pub fn add_tag_to_plan(
    state: tauri::State<'_, AppState>,
    plan_id: String,
    tag_id: String,
) -> Result<(), String> {
    state
        .db
        .add_tag_to_plan(&plan_id, &tag_id)
        .map_err(|e| format!("{}", e))
}

#[tauri::command]
pub fn remove_tag_from_plan(
    state: tauri::State<'_, AppState>,
    plan_id: String,
    tag_id: String,
) -> Result<(), String> {
    state
        .db
        .remove_tag_from_plan(&plan_id, &tag_id)
        .map_err(|e| format!("{}", e))
}

#[tauri::command]
pub fn get_tags_for_plan(
    state: tauri::State<'_, AppState>,
    plan_id: String,
) -> Result<Vec<Tag>, String> {
    state
        .db
        .get_tags_for_plan(&plan_id)
        .map_err(|e| format!("{}", e))
}

// ── Library plan commands ────────────────────────────────────

#[tauri::command]
pub fn list_library_plans(
    state: tauri::State<'_, AppState>,
    source_type: Option<String>,
    search: Option<String>,
    tag_ids: Option<Vec<String>>,
) -> Result<Vec<LibraryPlanCard>, String> {
    state
        .db
        .list_library_plans(&LibraryQuery {
            source_type,
            search,
            tag_ids,
        })
        .map_err(|e| format!("{}", e))
}

#[tauri::command]
pub fn search_plans_fts(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<FtsSearchResult>, String> {
    state
        .db
        .search_fts(&query, limit.unwrap_or(20))
        .map_err(|e| format!("{}", e))
}

#[tauri::command]
pub fn create_plan(
    state: tauri::State<'_, AppState>,
    title: String,
    subject_id: String,
    content: Option<String>,
    source_type: Option<String>,
) -> Result<LibraryPlanCard, String> {
    // Ensure the subject exists — the frontend may pass a placeholder like "default"
    // for manually-created plans where the user hasn't chosen a subject yet.
    let resolved_subject_id = match state.db.get_subject(&subject_id) {
        Ok(_) => subject_id,
        Err(_) => {
            let subject = state
                .db
                .create_subject(&crate::database::NewSubject {
                    name: "General".into(),
                    grade_level: None,
                    description: None,
                })
                .map_err(|e| format!("{}", e))?;
            subject.id
        }
    };

    let plan = state
        .db
        .create_lesson_plan(&NewLessonPlan {
            subject_id: resolved_subject_id,
            title,
            content,
            source_doc_id: None,
            source_table_index: None,
            learning_objectives: None,
        })
        .map_err(|e| format!("{}", e))?;

    // Set source_type if provided (defaults to 'created' via schema)
    if let Some(st) = &source_type {
        state
            .db
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE lesson_plans SET source_type = ?1 WHERE id = ?2",
                    rusqlite::params![st, plan.id],
                )?;
                Ok(())
            })
            .map_err(|e| format!("{}", e))?;
    }

    // Return as LibraryPlanCard
    Ok(LibraryPlanCard {
        id: plan.id,
        title: plan.title,
        status: plan.status,
        source_type: source_type.unwrap_or_else(|| "created".to_string()),
        version: 1,
        tags: Vec::new(),
        created_at: plan.created_at,
        updated_at: plan.updated_at,
    })
}

#[tauri::command]
pub fn get_plan(state: tauri::State<'_, AppState>, id: String) -> Result<LessonPlan, String> {
    state
        .db
        .get_lesson_plan(&id)
        .map_err(|e| format!("{}", e))
}

#[tauri::command]
pub fn update_plan_content(
    state: tauri::State<'_, AppState>,
    id: String,
    content: String,
) -> Result<LessonPlan, String> {
    state
        .db
        .update_lesson_plan_content(&id, &content)
        .map_err(|e| format!("{}", e))
}

#[tauri::command]
pub fn update_plan_title(
    state: tauri::State<'_, AppState>,
    id: String,
    title: String,
) -> Result<LessonPlan, String> {
    state.db.with_conn(|conn| {
        let updated = conn.execute(
            "UPDATE lesson_plans SET title = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![title, id],
        )?;
        if updated == 0 {
            return Err(crate::database::DatabaseError::NotFound);
        }
        conn.query_row(
            "SELECT id, subject_id, title, content, source_doc_id, source_table_index, learning_objectives, status, created_at, updated_at
             FROM lesson_plans WHERE id = ?1",
            rusqlite::params![id],
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
        .map_err(|e| e.into())
    })
    .map_err(|e| format!("{}", e))
}

// ── Plan versioning commands ─────────────────────────────────

#[tauri::command]
pub fn finalize_plan(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<PlanVersion, String> {
    state
        .db
        .finalize_plan(&id)
        .map_err(|e| format!("{}", e))
}

#[tauri::command]
pub fn list_plan_versions(
    state: tauri::State<'_, AppState>,
    plan_id: String,
) -> Result<Vec<PlanVersion>, String> {
    state
        .db
        .list_plan_versions(&plan_id)
        .map_err(|e| format!("{}", e))
}

#[tauri::command]
pub fn get_plan_version(
    state: tauri::State<'_, AppState>,
    plan_id: String,
    version: i32,
) -> Result<PlanVersion, String> {
    state
        .db
        .get_plan_version(&plan_id, version)
        .map_err(|e| format!("{}", e))
}

#[tauri::command]
pub fn revert_plan_version(
    state: tauri::State<'_, AppState>,
    plan_id: String,
    version: i32,
) -> Result<LessonPlan, String> {
    state
        .db
        .revert_plan_to_version(&plan_id, version)
        .map_err(|e| format!("{}", e))
}

#[tauri::command]
pub fn delete_plan(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .db
        .delete_lesson_plan(&id)
        .map_err(|e| format!("{}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;

    fn test_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn test_tag_crud() {
        let db = test_db();

        // Create
        let tag = db
            .create_tag(&NewTag {
                name: "Biology".into(),
                color: Some("#4CAF50".into()),
            })
            .unwrap();
        assert_eq!(tag.name, "Biology");
        assert_eq!(tag.color, Some("#4CAF50".to_string()));

        // List
        let tags = db.list_tags().unwrap();
        assert_eq!(tags.len(), 1);

        // Update
        let updated = db
            .update_tag(
                &tag.id,
                &NewTag {
                    name: "Bio".into(),
                    color: Some("#66BB6A".into()),
                },
            )
            .unwrap();
        assert_eq!(updated.name, "Bio");

        // Delete
        db.delete_tag(&tag.id).unwrap();
        let tags = db.list_tags().unwrap();
        assert_eq!(tags.len(), 0);
    }

    #[test]
    fn test_plan_tag_associations() {
        let db = test_db();

        // Create subject + plan + tag
        let subject = db
            .create_subject(&crate::database::NewSubject {
                name: "Science".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();
        let plan = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Photosynthesis".into(),
                content: Some("Plants...".into()),
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();
        let tag = db
            .create_tag(&NewTag {
                name: "Grade 10".into(),
                color: None,
            })
            .unwrap();

        // Associate
        db.add_tag_to_plan(&plan.id, &tag.id).unwrap();
        let tags = db.get_tags_for_plan(&plan.id).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "Grade 10");

        // Duplicate add is ignored
        db.add_tag_to_plan(&plan.id, &tag.id).unwrap();
        let tags = db.get_tags_for_plan(&plan.id).unwrap();
        assert_eq!(tags.len(), 1);

        // Remove
        db.remove_tag_from_plan(&plan.id, &tag.id).unwrap();
        let tags = db.get_tags_for_plan(&plan.id).unwrap();
        assert_eq!(tags.len(), 0);
    }

    #[test]
    fn test_library_query_all() {
        let db = test_db();

        let subject = db
            .create_subject(&crate::database::NewSubject {
                name: "Math".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();
        db.create_lesson_plan(&NewLessonPlan {
            subject_id: subject.id.clone(),
            title: "Algebra Basics".into(),
            content: None,
            source_doc_id: None,
            source_table_index: None,
            learning_objectives: None,
        })
        .unwrap();

        let plans = db
            .list_library_plans(&LibraryQuery {
                source_type: None,
                search: None,
                tag_ids: None,
            })
            .unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].title, "Algebra Basics");
    }

    #[test]
    fn test_library_query_search() {
        let db = test_db();

        let subject = db
            .create_subject(&crate::database::NewSubject {
                name: "English".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();
        db.create_lesson_plan(&NewLessonPlan {
            subject_id: subject.id.clone(),
            title: "Shakespeare Analysis".into(),
            content: None,
            source_doc_id: None,
            source_table_index: None,
            learning_objectives: None,
        })
        .unwrap();
        db.create_lesson_plan(&NewLessonPlan {
            subject_id: subject.id.clone(),
            title: "Poetry Workshop".into(),
            content: None,
            source_doc_id: None,
            source_table_index: None,
            learning_objectives: None,
        })
        .unwrap();

        let plans = db
            .list_library_plans(&LibraryQuery {
                source_type: None,
                search: Some("shakespeare".into()),
                tag_ids: None,
            })
            .unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].title, "Shakespeare Analysis");
    }

    #[test]
    fn test_library_query_by_tag() {
        let db = test_db();

        let subject = db
            .create_subject(&crate::database::NewSubject {
                name: "Art".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();
        let plan1 = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Watercolors".into(),
                content: None,
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();
        let plan2 = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Oil Painting".into(),
                content: None,
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();

        let tag = db
            .create_tag(&NewTag {
                name: "Beginner".into(),
                color: None,
            })
            .unwrap();
        db.add_tag_to_plan(&plan1.id, &tag.id).unwrap();

        // Query with tag filter
        let plans = db
            .list_library_plans(&LibraryQuery {
                source_type: None,
                search: None,
                tag_ids: Some(vec![tag.id.clone()]),
            })
            .unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].title, "Watercolors");
        assert_eq!(plans[0].tags.len(), 1);

        // Query without filter should return both
        let all = db
            .list_library_plans(&LibraryQuery {
                source_type: None,
                search: None,
                tag_ids: None,
            })
            .unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_cascade_delete_tag_removes_associations() {
        let db = test_db();

        let subject = db
            .create_subject(&crate::database::NewSubject {
                name: "History".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();
        let plan = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "WW2".into(),
                content: None,
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();
        let tag = db
            .create_tag(&NewTag {
                name: "Advanced".into(),
                color: None,
            })
            .unwrap();
        db.add_tag_to_plan(&plan.id, &tag.id).unwrap();

        // Deleting the tag should cascade-delete from plan_tags
        db.delete_tag(&tag.id).unwrap();
        let tags = db.get_tags_for_plan(&plan.id).unwrap();
        assert_eq!(tags.len(), 0);
    }

    #[test]
    fn test_cascade_delete_plan_removes_tag_associations() {
        let db = test_db();

        let subject = db
            .create_subject(&crate::database::NewSubject {
                name: "PE".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();
        let plan = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Basketball".into(),
                content: None,
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();
        let tag = db
            .create_tag(&NewTag {
                name: "Sports".into(),
                color: None,
            })
            .unwrap();
        db.add_tag_to_plan(&plan.id, &tag.id).unwrap();

        // Delete the plan — tag should still exist, but association gone
        db.delete_lesson_plan(&plan.id).unwrap();
        let tags = db.list_tags().unwrap();
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn test_tag_name_uniqueness() {
        let db = test_db();

        db.create_tag(&NewTag {
            name: "Math".into(),
            color: None,
        })
        .unwrap();

        // Duplicate name should fail
        let result = db.create_tag(&NewTag {
            name: "Math".into(),
            color: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_library_plans_include_tags() {
        let db = test_db();

        let subject = db
            .create_subject(&crate::database::NewSubject {
                name: "Music".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();
        let plan = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Guitar Basics".into(),
                content: None,
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();
        let tag1 = db
            .create_tag(&NewTag {
                name: "Beginner".into(),
                color: Some("#FF0".into()),
            })
            .unwrap();
        let tag2 = db
            .create_tag(&NewTag {
                name: "Instruments".into(),
                color: None,
            })
            .unwrap();
        db.add_tag_to_plan(&plan.id, &tag1.id).unwrap();
        db.add_tag_to_plan(&plan.id, &tag2.id).unwrap();

        let plans = db
            .list_library_plans(&LibraryQuery {
                source_type: None,
                search: None,
                tag_ids: None,
            })
            .unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].tags.len(), 2);
    }

    #[test]
    fn test_library_query_fts_searches_content() {
        let db = test_db();

        let subject = db
            .create_subject(&crate::database::NewSubject {
                name: "Science".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();
        db.create_lesson_plan(&NewLessonPlan {
            subject_id: subject.id.clone(),
            title: "Lesson One".into(),
            content: Some("The mitochondria is the powerhouse of the cell".into()),
            source_doc_id: None,
            source_table_index: None,
            learning_objectives: None,
        })
        .unwrap();
        db.create_lesson_plan(&NewLessonPlan {
            subject_id: subject.id.clone(),
            title: "Lesson Two".into(),
            content: Some("Water cycle and evaporation".into()),
            source_doc_id: None,
            source_table_index: None,
            learning_objectives: None,
        })
        .unwrap();

        // Search for content keyword — should find via FTS5
        let plans = db
            .list_library_plans(&LibraryQuery {
                source_type: None,
                search: Some("mitochondria".into()),
                tag_ids: None,
            })
            .unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].title, "Lesson One");
    }

    #[test]
    fn test_library_query_fts_searches_objectives() {
        let db = test_db();

        let subject = db
            .create_subject(&crate::database::NewSubject {
                name: "Math".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();
        db.create_lesson_plan(&NewLessonPlan {
            subject_id: subject.id.clone(),
            title: "Algebra Intro".into(),
            content: Some("Solving linear equations".into()),
            source_doc_id: None,
            source_table_index: None,
            learning_objectives: Some("Students will understand quadratic equations".into()),
        })
        .unwrap();

        // Search by learning_objectives keyword
        let plans = db
            .list_library_plans(&LibraryQuery {
                source_type: None,
                search: Some("quadratic".into()),
                tag_ids: None,
            })
            .unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].title, "Algebra Intro");
    }
}
