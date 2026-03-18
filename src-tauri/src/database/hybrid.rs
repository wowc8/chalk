//! Hybrid search: combine FTS5 keyword results with sqlite-vec semantic results
//! using Reciprocal Rank Fusion (RRF) for score normalization and re-ranking.

use std::collections::HashMap;

use rusqlite::params;

use super::connection::{Database, Result};
use super::models::HybridSearchResult;

/// Constant `k` for RRF scoring: `1 / (k + rank)`.
/// A value of 60 is standard in information retrieval literature.
const RRF_K: f64 = 60.0;

impl Database {
    /// Perform hybrid search combining FTS5 keyword results and sqlite-vec
    /// semantic results. Merges and re-ranks using Reciprocal Rank Fusion.
    ///
    /// - `fts_query`: the user's text query for FTS5 (will be sanitized)
    /// - `query_embedding`: pre-computed embedding vector for semantic search
    /// - `limit`: max results to return
    /// - `fts_weight`: relative weight for FTS5 results (default 1.0)
    /// - `vec_weight`: relative weight for vector results (default 1.0)
    pub fn search_hybrid(
        &self,
        fts_query: &str,
        query_embedding: &[f32],
        limit: usize,
        fts_weight: f64,
        vec_weight: f64,
    ) -> Result<Vec<HybridSearchResult>> {
        // Fetch both result sets. Use a larger per-source limit to give RRF
        // enough candidates to work with.
        let per_source_limit = limit * 3;

        let fts_results = self.search_fts(fts_query, per_source_limit)?;
        let vec_results = self.search_similar(query_embedding, per_source_limit)?;

        // Build RRF score map: plan_id -> accumulated score.
        let mut scores: HashMap<String, f64> = HashMap::new();

        // FTS5 results: rank position 0..n, lower rank value = better match.
        // Already sorted by rank (best first).
        for (position, result) in fts_results.iter().enumerate() {
            let rrf_score = fts_weight / (RRF_K + position as f64 + 1.0);
            *scores.entry(result.lesson_plan_id.clone()).or_insert(0.0) += rrf_score;
        }

        // Vector results: sorted by distance (ascending = closest first).
        for (position, result) in vec_results.iter().enumerate() {
            let rrf_score = vec_weight / (RRF_K + position as f64 + 1.0);
            *scores.entry(result.lesson_plan_id.clone()).or_insert(0.0) += rrf_score;
        }

        if scores.is_empty() {
            return Ok(Vec::new());
        }

        // Sort by RRF score descending (highest = best).
        let mut scored: Vec<(String, f64)> = scores.into_iter().collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        // Fetch titles for the top results.
        self.with_conn(|conn| {
            let mut results = Vec::with_capacity(scored.len());
            for (plan_id, score) in scored {
                let title: Option<String> = conn
                    .query_row(
                        "SELECT title FROM lesson_plans WHERE id = ?1",
                        params![plan_id],
                        |row| row.get(0),
                    )
                    .ok();

                if let Some(title) = title {
                    results.push(HybridSearchResult {
                        lesson_plan_id: plan_id,
                        title,
                        score,
                    });
                }
            }
            Ok(results)
        })
    }

    /// Perform hybrid search using only the text query (FTS5 only, no embedding).
    /// Falls back to pure FTS5 when embeddings are unavailable.
    pub fn search_hybrid_fts_only(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<HybridSearchResult>> {
        let fts_results = self.search_fts_with_fallback(query, limit)?;
        Ok(fts_results
            .into_iter()
            .map(|r| HybridSearchResult {
                lesson_plan_id: r.lesson_plan_id,
                title: r.title,
                score: -r.rank, // Negate FTS5 rank so higher = better
            })
            .collect())
    }
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

    fn create_plan(
        db: &Database,
        subject_id: &str,
        title: &str,
        content: &str,
        objectives: Option<&str>,
    ) -> String {
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

    /// Set up a small vec table with 4 dimensions for testing.
    fn setup_small_vec_table(db: &Database) {
        db.with_conn(|conn| {
            conn.execute_batch("DROP TABLE IF EXISTS lesson_plan_vectors")?;
            conn.execute_batch(
                "CREATE VIRTUAL TABLE lesson_plan_vectors USING vec0(embedding float[4])",
            )?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_hybrid_search_combines_fts_and_vec() {
        let db = test_db();
        let subject_id = create_subject(&db);
        setup_small_vec_table(&db);

        let plan_a = create_plan(
            &db,
            &subject_id,
            "Photosynthesis Lab",
            "Plants use sunlight for energy",
            None,
        );
        let plan_b = create_plan(
            &db,
            &subject_id,
            "Cell Division Overview",
            "Mitosis and meiosis in biology",
            None,
        );
        let plan_c = create_plan(
            &db,
            &subject_id,
            "Solar Energy Basics",
            "Understanding solar panels and sunlight energy",
            None,
        );

        // Embeddings: plan_a and plan_c are semantically close
        db.upsert_embedding(&plan_a, &[1.0, 0.0, 0.0, 0.0]).unwrap();
        db.upsert_embedding(&plan_b, &[0.0, 1.0, 0.0, 0.0]).unwrap();
        db.upsert_embedding(&plan_c, &[0.9, 0.1, 0.0, 0.0]).unwrap();

        // Query for "photosynthesis" — FTS5 finds plan_a directly;
        // vector search with embedding close to plan_a also finds plan_a + plan_c.
        let query_embedding = [0.95, 0.05, 0.0, 0.0];
        let results = db
            .search_hybrid("photosynthesis", &query_embedding, 10, 1.0, 1.0)
            .unwrap();

        // plan_a should rank first (found by both FTS5 and vector)
        assert!(!results.is_empty());
        assert_eq!(results[0].lesson_plan_id, plan_a);
        assert_eq!(results[0].title, "Photosynthesis Lab");

        // plan_a's score should be higher than others (boosted by both sources)
        if results.len() > 1 {
            assert!(results[0].score > results[1].score);
        }
    }

    #[test]
    fn test_hybrid_search_empty_query() {
        let db = test_db();
        setup_small_vec_table(&db);

        let results = db
            .search_hybrid("", &[0.0, 0.0, 0.0, 0.0], 10, 1.0, 1.0)
            .unwrap();
        // Empty FTS query returns nothing from FTS; vec search may return nothing
        // with no data. Either way, should not error.
        assert!(results.is_empty());
    }

    #[test]
    fn test_hybrid_search_respects_limit() {
        let db = test_db();
        let subject_id = create_subject(&db);
        setup_small_vec_table(&db);

        for i in 0..10 {
            let plan_id = create_plan(
                &db,
                &subject_id,
                &format!("Biology Lesson {}", i),
                "Cell biology and genetics topics",
                None,
            );
            db.upsert_embedding(&plan_id, &[0.5, 0.5, 0.0, 0.0]).unwrap();
        }

        let results = db
            .search_hybrid("biology", &[0.5, 0.5, 0.0, 0.0], 3, 1.0, 1.0)
            .unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_hybrid_search_fts_only_fallback() {
        let db = test_db();
        let subject_id = create_subject(&db);

        create_plan(
            &db,
            &subject_id,
            "Photosynthesis Lab",
            "Plants and sunlight",
            None,
        );
        create_plan(
            &db,
            &subject_id,
            "Cell Division",
            "Mitosis overview",
            None,
        );

        let results = db.search_hybrid_fts_only("photosynthesis", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Photosynthesis Lab");
        // Score should be positive (negated FTS rank)
        assert!(results[0].score > 0.0);
    }

    #[test]
    fn test_hybrid_search_weights() {
        let db = test_db();
        let subject_id = create_subject(&db);
        setup_small_vec_table(&db);

        let plan_fts = create_plan(
            &db,
            &subject_id,
            "Unique Keyword Albatross",
            "About albatross birds",
            None,
        );
        let plan_vec = create_plan(
            &db,
            &subject_id,
            "Bird Migration Patterns",
            "General bird content",
            None,
        );

        // plan_fts matches FTS for "albatross", plan_vec is semantically close
        // Make embeddings maximally different to ensure vector ranking is clear
        db.upsert_embedding(&plan_fts, &[0.0, 0.0, 0.0, 1.0]).unwrap();
        db.upsert_embedding(&plan_vec, &[1.0, 0.0, 0.0, 0.0]).unwrap();

        // With heavy FTS weight and zero vector weight, the FTS match should dominate
        let results = db
            .search_hybrid("albatross", &[1.0, 0.0, 0.0, 0.0], 10, 10.0, 0.0)
            .unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].lesson_plan_id, plan_fts);

        // With zero FTS weight and heavy vector weight, the vector match should dominate
        let results = db
            .search_hybrid("albatross", &[1.0, 0.0, 0.0, 0.0], 10, 0.0, 10.0)
            .unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].lesson_plan_id, plan_vec);
    }

    #[test]
    fn test_hybrid_search_no_results() {
        let db = test_db();
        let subject_id = create_subject(&db);
        setup_small_vec_table(&db);

        create_plan(&db, &subject_id, "Test", "Content", None);

        let results = db
            .search_hybrid("xyzzyx_nonexistent", &[0.0, 0.0, 0.0, 0.0], 10, 1.0, 1.0)
            .unwrap();
        // FTS returns nothing for nonsense query; vec with zero vector may or may not
        assert!(results.len() <= 1); // At most 1 from vec
    }

    #[test]
    fn test_hybrid_search_deleted_plan_excluded() {
        let db = test_db();
        let subject_id = create_subject(&db);
        setup_small_vec_table(&db);

        let plan_id = create_plan(
            &db,
            &subject_id,
            "Deletable Plan",
            "Content to delete",
            None,
        );
        db.upsert_embedding(&plan_id, &[1.0, 0.0, 0.0, 0.0]).unwrap();

        // Verify it's found
        let results = db
            .search_hybrid("deletable", &[1.0, 0.0, 0.0, 0.0], 10, 1.0, 1.0)
            .unwrap();
        assert_eq!(results.len(), 1);

        // Delete the plan
        db.delete_lesson_plan(&plan_id).unwrap();

        // Should no longer appear (FTS triggers clean it up, title fetch filters stale vec results)
        let results = db
            .search_hybrid("deletable", &[1.0, 0.0, 0.0, 0.0], 10, 1.0, 1.0)
            .unwrap();
        assert!(results.is_empty());
    }
}
