use crate::database::{
    FtsSearchResult, HybridSearchResult, LessonPlan, LibraryPlanCard, LibraryQuery, NewLessonPlan,
    NewTag, PlanVersion, SchoolYearGroup, Tag,
};
use crate::rag::embeddings::EmbeddingClient;
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
        .search_fts_with_fallback(&query, limit.unwrap_or(20))
        .map_err(|e| format!("{}", e))
}

/// Hybrid search combining FTS5 keyword matching with FTS5-only fallback.
/// When embedding support is wired up on the frontend, pass `query_embedding`
/// for full hybrid (FTS5 + vector) search. Without it, uses FTS5-only.
#[tauri::command]
pub fn search_plans_hybrid(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<HybridSearchResult>, String> {
    state
        .db
        .search_hybrid_fts_only(&query, limit.unwrap_or(20))
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
        week_start_date: plan.week_start_date,
        week_end_date: plan.week_end_date,
        school_year: plan.school_year,
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
            "SELECT id, subject_id, title, content, source_doc_id, source_table_index, learning_objectives, status, week_start_date, week_end_date, school_year, created_at, updated_at
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
                    week_start_date: row.get(8)?,
                    week_end_date: row.get(9)?,
                    school_year: row.get(10)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                })
            },
        )
        .map_err(|e| e.into())
    })
    .map_err(|e| format!("{}", e))
}

// ── Plan versioning commands ─────────────────────────────────

#[tauri::command]
pub async fn finalize_plan(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<PlanVersion, String> {
    let version = state
        .db
        .finalize_plan(&id)
        .map_err(|e| format!("{}", e))?;

    // Best-effort auto-vectorize: generate embedding and upsert into sqlite-vec
    // so finalized plans feed the RAG pipeline immediately.
    // Failures here are logged but don't block the finalize operation.
    if let Err(e) = auto_vectorize_plan(&state.db, &id).await {
        tracing::warn!(plan_id = %id, error = %e, "Auto-vectorize after finalize failed (non-fatal)");
    }

    Ok(version)
}

/// Generate an embedding for the given plan and upsert it into the vector DB.
/// Returns an error if the API key is not configured or the embedding call fails.
async fn auto_vectorize_plan(
    db: &crate::database::Database,
    plan_id: &str,
) -> Result<(), String> {
    let api_key = db
        .get_setting("openai_api_key")
        .map_err(|e| format!("{e}"))?
        .ok_or_else(|| "OpenAI API key not configured".to_string())?;

    let base_url = db
        .get_setting("openai_base_url")
        .map_err(|e| format!("{e}"))?
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

    let plan = db.get_lesson_plan(plan_id).map_err(|e| format!("{e}"))?;

    let embedding_text = crate::rag::chunker::create_embedding_text(
        &plan.title,
        &plan.content,
        plan.learning_objectives.as_deref(),
    );

    let client = EmbeddingClient::with_base_url(api_key, base_url);
    let embedding = client
        .embed_one(&embedding_text)
        .await
        .map_err(|e| e.message)?;

    db.upsert_embedding(plan_id, &embedding)
        .map_err(|e| format!("{e}"))?;

    tracing::info!(plan_id = %plan_id, "Plan auto-vectorized on finalize");
    Ok(())
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

/// List lesson plans grouped by school year and month for the chronological library view.
#[tauri::command]
pub fn list_library_plans_chronological(
    state: tauri::State<'_, AppState>,
    search: Option<String>,
) -> Result<Vec<SchoolYearGroup>, String> {
    state
        .db
        .list_library_plans_chronological(search.as_deref())
        .map_err(|e| format!("{}", e))
}

/// Update date metadata on a lesson plan.
#[tauri::command]
pub fn update_plan_dates(
    state: tauri::State<'_, AppState>,
    id: String,
    week_start_date: Option<String>,
    week_end_date: Option<String>,
    school_year: Option<String>,
) -> Result<LessonPlan, String> {
    state
        .db
        .update_lesson_plan_dates(
            &id,
            week_start_date.as_deref(),
            week_end_date.as_deref(),
            school_year.as_deref(),
        )
        .map_err(|e| format!("{}", e))
}

/// Duplicate a lesson plan as a new editable template.
#[tauri::command]
pub fn duplicate_plan_as_template(
    state: tauri::State<'_, AppState>,
    source_plan_id: String,
    new_title: String,
) -> Result<LessonPlan, String> {
    state
        .db
        .duplicate_plan_as_template(&source_plan_id, &new_title)
        .map_err(|e| format!("{}", e))
}

#[tauri::command]
pub fn delete_plan(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    // Delete associated embedding/vector data first (best-effort).
    let _ = state.db.delete_embedding(&id);
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

    // ── Auto-vectorize tests ────────────────────────────────────

    #[tokio::test]
    async fn test_auto_vectorize_returns_error_without_api_key() {
        let db = test_db();

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
                content: Some("Plants convert sunlight".into()),
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();

        // No API key configured — auto_vectorize_plan should return Err
        let result = auto_vectorize_plan(&db, &plan.id).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("API key not configured"));
    }

    #[test]
    fn test_finalize_plan_db_layer_then_upsert_embedding() {
        // Verify that finalize + manual embedding upsert works end-to-end
        // at the DB layer (the integration that auto_vectorize_plan performs).
        let db = test_db();

        let subject = db
            .create_subject(&crate::database::NewSubject {
                name: "Math".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();
        let plan = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Quadratics".into(),
                content: Some("Solving ax^2 + bx + c = 0".into()),
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: Some("Students solve quadratic equations".into()),
            })
            .unwrap();

        // Finalize the plan
        let version = db.finalize_plan(&plan.id).unwrap();
        assert_eq!(version.version, 1);
        assert_eq!(version.title, "Quadratics");

        // Recreate vec table with smaller dims for test.
        db.with_conn(|conn| {
            conn.execute_batch("DROP TABLE IF EXISTS lesson_plan_vectors")?;
            conn.execute_batch(
                "CREATE VIRTUAL TABLE lesson_plan_vectors USING vec0(embedding float[4])",
            )?;
            Ok(())
        })
        .unwrap();

        // Simulate the embedding upsert that auto_vectorize_plan would do
        let fake_embedding = [0.1_f32, 0.2, 0.3, 0.4];
        db.upsert_embedding(&plan.id, &fake_embedding).unwrap();

        // Verify the plan is now searchable via vector similarity
        let results = db.search_similar(&[0.1, 0.2, 0.3, 0.4], 5).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].lesson_plan_id, plan.id);

        // Upsert again (re-finalize scenario) — should replace, not duplicate
        let new_embedding = [0.5_f32, 0.6, 0.7, 0.8];
        db.upsert_embedding(&plan.id, &new_embedding).unwrap();

        let results = db.search_similar(&[0.5, 0.6, 0.7, 0.8], 5).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].lesson_plan_id, plan.id);
    }

    #[test]
    fn test_finalize_plan_status_is_finalized() {
        let db = test_db();

        let subject = db
            .create_subject(&crate::database::NewSubject {
                name: "English".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();
        let plan = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Poetry".into(),
                content: Some("Haiku structure".into()),
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();

        db.finalize_plan(&plan.id).unwrap();

        let updated = db.get_lesson_plan(&plan.id).unwrap();
        assert_eq!(updated.status, "finalized");
    }

    #[test]
    fn test_update_plan_dates() {
        let db = test_db();

        let subject = db
            .create_subject(&crate::database::NewSubject {
                name: "Math".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();
        let plan = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Week 1 Math".into(),
                content: Some("Addition basics".into()),
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();

        // Initially no dates
        assert!(plan.week_start_date.is_none());
        assert!(plan.school_year.is_none());

        // Set dates
        let updated = db
            .update_lesson_plan_dates(
                &plan.id,
                Some("2024-09-02"),
                Some("2024-09-06"),
                Some("2024-25"),
            )
            .unwrap();
        assert_eq!(updated.week_start_date.as_deref(), Some("2024-09-02"));
        assert_eq!(updated.week_end_date.as_deref(), Some("2024-09-06"));
        assert_eq!(updated.school_year.as_deref(), Some("2024-25"));
    }

    #[test]
    fn test_chronological_library_grouping() {
        let db = test_db();

        let subject = db
            .create_subject(&crate::database::NewSubject {
                name: "Science".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();

        // Create plans in different months and school years
        let plan1 = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Sep Week 1".into(),
                content: Some("Content".into()),
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();
        db.update_lesson_plan_dates(&plan1.id, Some("2024-09-02"), Some("2024-09-06"), Some("2024-25"))
            .unwrap();

        let plan2 = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Sep Week 2".into(),
                content: Some("Content".into()),
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();
        db.update_lesson_plan_dates(&plan2.id, Some("2024-09-09"), Some("2024-09-13"), Some("2024-25"))
            .unwrap();

        let plan3 = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Oct Week 1".into(),
                content: Some("Content".into()),
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();
        db.update_lesson_plan_dates(&plan3.id, Some("2024-10-07"), Some("2024-10-11"), Some("2024-25"))
            .unwrap();

        let plan4 = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Last Year Sep".into(),
                content: Some("Content".into()),
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();
        db.update_lesson_plan_dates(&plan4.id, Some("2023-09-04"), Some("2023-09-08"), Some("2023-24"))
            .unwrap();

        // Query chronological library
        let groups = db.list_library_plans_chronological(None).unwrap();
        assert_eq!(groups.len(), 2); // two school years

        // Most recent school year first
        assert_eq!(groups[0].school_year, "2024-25");
        assert_eq!(groups[1].school_year, "2023-24");

        // 2024-25 has 2 months
        assert_eq!(groups[0].months.len(), 2);
        assert_eq!(groups[0].months[0].month_name, "September");
        assert_eq!(groups[0].months[0].plans.len(), 2);
        assert_eq!(groups[0].months[1].month_name, "October");
        assert_eq!(groups[0].months[1].plans.len(), 1);

        // 2023-24 has 1 month
        assert_eq!(groups[1].months.len(), 1);
        assert_eq!(groups[1].months[0].month_name, "September");
    }

    #[test]
    fn test_duplicate_plan_as_template() {
        let db = test_db();

        let subject = db
            .create_subject(&crate::database::NewSubject {
                name: "English".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();
        let plan = db
            .create_lesson_plan(&NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Week 5 Lesson".into(),
                content: Some("<p>Shakespeare analysis</p>".into()),
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: Some("Students analyze themes".into()),
            })
            .unwrap();

        let copy = db
            .duplicate_plan_as_template(&plan.id, "Week 5 Lesson (copy)")
            .unwrap();
        assert_eq!(copy.title, "Week 5 Lesson (copy)");
        assert_eq!(copy.content, "<p>Shakespeare analysis</p>");
        assert_eq!(
            copy.learning_objectives.as_deref(),
            Some("Students analyze themes")
        );
        assert_eq!(copy.status, "draft");
        assert_ne!(copy.id, plan.id);
        // Copy should not have dates
        assert!(copy.week_start_date.is_none());
    }
}
