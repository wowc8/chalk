use rusqlite::params;

use super::connection::{Database, Result};
use super::models::VectorSearchResult;

impl Database {
    /// Store an embedding vector for a lesson plan.
    /// The `rowid` in the vec0 table maps to the lesson plan by storing a
    /// separate mapping row; we use an integer rowid derived from hashing the
    /// lesson plan UUID for the virtual table, and keep a mapping table.
    pub fn upsert_embedding(&self, lesson_plan_id: &str, embedding: &[f32]) -> Result<()> {
        let json = format!(
            "[{}]",
            embedding
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        self.with_conn(|conn| {
            // Ensure the mapping table exists (idempotent).
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS _vec_id_map (
                    rowid   INTEGER PRIMARY KEY AUTOINCREMENT,
                    plan_id TEXT NOT NULL UNIQUE
                )",
            )?;

            // Upsert the mapping to get a stable integer rowid.
            conn.execute(
                "INSERT INTO _vec_id_map (plan_id) VALUES (?1) ON CONFLICT(plan_id) DO NOTHING",
                params![lesson_plan_id],
            )?;

            let rowid: i64 = conn.query_row(
                "SELECT rowid FROM _vec_id_map WHERE plan_id = ?1",
                params![lesson_plan_id],
                |row| row.get(0),
            )?;

            // Delete any existing vector for this rowid, then insert the new one.
            conn.execute(
                "DELETE FROM lesson_plan_vectors WHERE rowid = ?1",
                params![rowid],
            )?;
            conn.execute(
                "INSERT INTO lesson_plan_vectors (rowid, embedding) VALUES (?1, ?2)",
                params![rowid, json],
            )?;

            Ok(())
        })
    }

    /// Find the `limit` most similar lesson plans to the given query embedding.
    /// Returns results sorted by ascending distance (closest first).
    pub fn search_similar(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<VectorSearchResult>> {
        let json = format!(
            "[{}]",
            query_embedding
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        self.with_conn(|conn| {
            // Ensure mapping table exists for the join.
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS _vec_id_map (
                    rowid   INTEGER PRIMARY KEY AUTOINCREMENT,
                    plan_id TEXT NOT NULL UNIQUE
                )",
            )?;

            // vec0 KNN queries require `k = ?` constraint, not LIMIT.
            // We query the vec table first, then resolve plan_ids via the mapping.
            let mut stmt = conn.prepare(
                "SELECT v.rowid, v.distance
                 FROM lesson_plan_vectors v
                 WHERE v.embedding MATCH ?1
                   AND k = ?2
                 ORDER BY v.distance",
            )?;

            let vec_rows: Vec<(i64, f64)> = stmt
                .query_map(params![json, limit as i64], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            let mut results = Vec::with_capacity(vec_rows.len());
            for (rowid, distance) in vec_rows {
                let plan_id: String = conn.query_row(
                    "SELECT plan_id FROM _vec_id_map WHERE rowid = ?1",
                    params![rowid],
                    |row| row.get(0),
                )?;
                results.push(VectorSearchResult {
                    lesson_plan_id: plan_id,
                    distance,
                });
            }

            Ok(results)
        })
    }

    /// Remove the embedding for a lesson plan.
    pub fn delete_embedding(&self, lesson_plan_id: &str) -> Result<()> {
        self.with_conn(|conn| {
            // Ensure mapping table exists.
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS _vec_id_map (
                    rowid   INTEGER PRIMARY KEY AUTOINCREMENT,
                    plan_id TEXT NOT NULL UNIQUE
                )",
            )?;

            let rowid: std::result::Result<i64, _> = conn.query_row(
                "SELECT rowid FROM _vec_id_map WHERE plan_id = ?1",
                params![lesson_plan_id],
                |row| row.get(0),
            );

            if let Ok(rowid) = rowid {
                conn.execute(
                    "DELETE FROM lesson_plan_vectors WHERE rowid = ?1",
                    params![rowid],
                )?;
                conn.execute(
                    "DELETE FROM _vec_id_map WHERE rowid = ?1",
                    params![rowid],
                )?;
            }

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
    fn test_upsert_and_search_embeddings() {
        let db = test_db();

        // Create a subject and plan first.
        let subject = db
            .create_subject(&crate::database::NewSubject {
                name: "Test".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();

        let plan1 = db
            .create_lesson_plan(&crate::database::NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Plan A".into(),
                content: None,
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();

        let plan2 = db
            .create_lesson_plan(&crate::database::NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Plan B".into(),
                content: None,
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();

        // Use small 4-dimensional vectors for testing.
        // Note: the migration creates float[1536] but sqlite-vec is flexible
        // about dimensions per-row in practice. For tests we'll recreate
        // a smaller table.
        db.with_conn(|conn| {
            conn.execute_batch("DROP TABLE IF EXISTS lesson_plan_vectors")?;
            conn.execute_batch(
                "CREATE VIRTUAL TABLE lesson_plan_vectors USING vec0(embedding float[4])",
            )?;
            Ok(())
        })
        .unwrap();

        let vec_a = [1.0_f32, 0.0, 0.0, 0.0];
        let vec_b = [0.0_f32, 1.0, 0.0, 0.0];

        db.upsert_embedding(&plan1.id, &vec_a).unwrap();
        db.upsert_embedding(&plan2.id, &vec_b).unwrap();

        // Query with something close to vec_a.
        let query = [0.9_f32, 0.1, 0.0, 0.0];
        let results = db.search_similar(&query, 2).unwrap();
        assert_eq!(results.len(), 2);
        // Plan A should be closest.
        assert_eq!(results[0].lesson_plan_id, plan1.id);

        // Upsert should replace.
        let vec_a_new = [0.0_f32, 0.0, 1.0, 0.0];
        db.upsert_embedding(&plan1.id, &vec_a_new).unwrap();

        let results = db.search_similar(&query, 2).unwrap();
        // Now plan B ([0,1,0,0]) should be closer to [0.9,0.1,0,0] than plan A ([0,0,1,0]).
        assert_eq!(results[0].lesson_plan_id, plan2.id);
    }

    #[test]
    fn test_delete_embedding() {
        let db = test_db();

        let subject = db
            .create_subject(&crate::database::NewSubject {
                name: "Test".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();

        let plan = db
            .create_lesson_plan(&crate::database::NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Plan".into(),
                content: None,
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();

        db.with_conn(|conn| {
            conn.execute_batch("DROP TABLE IF EXISTS lesson_plan_vectors")?;
            conn.execute_batch(
                "CREATE VIRTUAL TABLE lesson_plan_vectors USING vec0(embedding float[4])",
            )?;
            Ok(())
        })
        .unwrap();

        db.upsert_embedding(&plan.id, &[1.0, 0.0, 0.0, 0.0])
            .unwrap();
        db.delete_embedding(&plan.id).unwrap();

        let results = db.search_similar(&[1.0, 0.0, 0.0, 0.0], 10).unwrap();
        assert_eq!(results.len(), 0);
    }
}
