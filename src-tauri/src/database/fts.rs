use rusqlite::params;

use super::connection::{Database, Result};
use super::models::FtsSearchResult;

impl Database {
    /// Search lesson plans using FTS5 full-text search.
    /// Matches against title, content, and learning_objectives.
    /// Returns results ranked by FTS5 relevance (best matches first).
    pub fn search_fts(&self, query: &str, limit: usize) -> Result<Vec<FtsSearchResult>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        // Sanitize for FTS5: wrap each token in double quotes to treat as literals,
        // avoiding injection of FTS5 operators like AND/OR/NOT/NEAR.
        let sanitized = sanitize_fts_query(query);
        if sanitized.is_empty() {
            return Ok(Vec::new());
        }

        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT lp.id, lp.title, rank
                 FROM lesson_plans_fts fts
                 INNER JOIN lesson_plans lp ON lp.rowid = fts.rowid
                 WHERE lesson_plans_fts MATCH ?1
                 ORDER BY rank
                 LIMIT ?2",
            )?;

            let rows = stmt.query_map(params![sanitized, limit as i64], |row| {
                Ok(FtsSearchResult {
                    lesson_plan_id: row.get(0)?,
                    title: row.get(1)?,
                    rank: row.get(2)?,
                })
            })?;

            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    /// Manually rebuild the FTS5 index from the lesson_plans table.
    /// Useful after bulk operations or data recovery.
    pub fn rebuild_fts_index(&self) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute_batch("INSERT INTO lesson_plans_fts(lesson_plans_fts) VALUES('rebuild')")?;
            Ok(())
        })
    }
}

/// Sanitize a user query for FTS5 by quoting each token as a literal phrase.
/// This prevents FTS5 operator injection (AND, OR, NOT, NEAR, etc).
fn sanitize_fts_query(query: &str) -> String {
    let tokens: Vec<String> = query
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|token| {
            // Escape any double quotes within the token
            let escaped = token.replace('"', "\"\"");
            format!("\"{}\"", escaped)
        })
        .collect();

    tokens.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{NewLessonPlan, NewSubject};

    fn test_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    fn create_subject(db: &Database) -> String {
        db.create_subject(&NewSubject {
            name: "Science".into(),
            grade_level: None,
            description: None,
        })
        .unwrap()
        .id
    }

    fn create_plan(db: &Database, subject_id: &str, title: &str, content: &str, objectives: Option<&str>) -> String {
        db.create_lesson_plan(&NewLessonPlan {
            subject_id: subject_id.to_string(),
            title: title.into(),
            content: Some(content.into()),
            source_doc_id: None,
            source_table_index: None,
            learning_objectives: objectives.map(|s| s.to_string()),
        })
        .unwrap()
        .id
    }

    #[test]
    fn test_fts_search_by_title() {
        let db = test_db();
        let subject_id = create_subject(&db);

        create_plan(&db, &subject_id, "Photosynthesis Basics", "Plants use light", None);
        create_plan(&db, &subject_id, "Cell Division", "Mitosis and meiosis", None);

        let results = db.search_fts("photosynthesis", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Photosynthesis Basics");
    }

    #[test]
    fn test_fts_search_by_content() {
        let db = test_db();
        let subject_id = create_subject(&db);

        create_plan(&db, &subject_id, "Lesson One", "The mitochondria is the powerhouse of the cell", None);
        create_plan(&db, &subject_id, "Lesson Two", "Water cycle and precipitation", None);

        let results = db.search_fts("mitochondria", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Lesson One");
    }

    #[test]
    fn test_fts_search_by_learning_objectives() {
        let db = test_db();
        let subject_id = create_subject(&db);

        create_plan(
            &db,
            &subject_id,
            "Algebra Intro",
            "Solving equations",
            Some("Students will understand quadratic equations"),
        );
        create_plan(&db, &subject_id, "Geometry", "Shapes and angles", None);

        let results = db.search_fts("quadratic", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Algebra Intro");
    }

    #[test]
    fn test_fts_search_empty_query() {
        let db = test_db();
        let results = db.search_fts("", 10).unwrap();
        assert_eq!(results.len(), 0);

        let results = db.search_fts("   ", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_fts_search_no_results() {
        let db = test_db();
        let subject_id = create_subject(&db);

        create_plan(&db, &subject_id, "Lesson", "Content here", None);

        let results = db.search_fts("xyzzyx", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_fts_search_multiple_results() {
        let db = test_db();
        let subject_id = create_subject(&db);

        create_plan(&db, &subject_id, "Science Lab", "Experiment with photosynthesis", None);
        create_plan(&db, &subject_id, "Advanced Photosynthesis", "Deep dive into light reactions", None);
        create_plan(&db, &subject_id, "History", "World War II overview", None);

        let results = db.search_fts("photosynthesis", 10).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_fts_search_respects_limit() {
        let db = test_db();
        let subject_id = create_subject(&db);

        for i in 0..5 {
            create_plan(&db, &subject_id, &format!("Biology Lesson {}", i), "Cell biology topics", None);
        }

        let results = db.search_fts("biology", 3).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_fts_index_updated_on_plan_update() {
        let db = test_db();
        let subject_id = create_subject(&db);

        let plan_id = create_plan(&db, &subject_id, "My Lesson", "Unique banana content", None);

        // Should find by original content
        let results = db.search_fts("banana", 10).unwrap();
        assert_eq!(results.len(), 1);

        // Update content
        db.update_lesson_plan_content(&plan_id, "Completely new material about enzymes").unwrap();

        // Should find by new content
        let results = db.search_fts("enzymes", 10).unwrap();
        assert_eq!(results.len(), 1);

        // Should NOT find by old content keyword
        let results = db.search_fts("banana", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_fts_index_updated_on_plan_delete() {
        let db = test_db();
        let subject_id = create_subject(&db);

        let plan_id = create_plan(&db, &subject_id, "Deletable", "Unique content here", None);

        let results = db.search_fts("deletable", 10).unwrap();
        assert_eq!(results.len(), 1);

        db.delete_lesson_plan(&plan_id).unwrap();

        let results = db.search_fts("deletable", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_fts_search_sanitizes_special_chars() {
        let db = test_db();
        let subject_id = create_subject(&db);

        create_plan(&db, &subject_id, "Test Plan", "Normal content", None);

        // FTS5 operators should be treated as literals, not cause errors
        let results = db.search_fts("AND OR NOT", 10).unwrap();
        assert_eq!(results.len(), 0);

        let results = db.search_fts("\"quoted\"", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_rebuild_fts_index() {
        let db = test_db();
        let subject_id = create_subject(&db);

        create_plan(&db, &subject_id, "Rebuild Test", "Content for rebuild", None);

        // Should not error
        db.rebuild_fts_index().unwrap();

        // Data should still be searchable
        let results = db.search_fts("rebuild", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_sanitize_fts_query() {
        assert_eq!(sanitize_fts_query("hello world"), "\"hello\" \"world\"");
        assert_eq!(sanitize_fts_query("  spaces  "), "\"spaces\"");
        assert_eq!(sanitize_fts_query(""), "");
        assert_eq!(sanitize_fts_query("AND OR"), "\"AND\" \"OR\"");
        assert_eq!(
            sanitize_fts_query("has\"quotes"),
            "\"has\"\"quotes\""
        );
    }
}
