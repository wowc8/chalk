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

    /// Fuzzy search fallback: scans lesson plans and ranks by Levenshtein similarity.
    /// Used when FTS5 prefix matching returns no results (e.g. typos).
    pub fn search_fuzzy(&self, query: &str, limit: usize) -> Result<Vec<FtsSearchResult>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, COALESCE(content, ''), COALESCE(learning_objectives, '')
                 FROM lesson_plans",
            )?;

            let mut scored: Vec<(FtsSearchResult, f64)> = Vec::new();

            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?;

            for row in rows {
                let (id, title, content, objectives) = row?;

                // Score against each field, weight title highest
                let title_score = fuzzy_score(query, &title) * 1.5;
                let content_score = fuzzy_score(query, &content);
                let obj_score = fuzzy_score(query, &objectives);

                let best_score = title_score.max(content_score).max(obj_score);

                // Threshold: only include reasonable matches
                if best_score >= 0.6 {
                    scored.push((
                        FtsSearchResult {
                            lesson_plan_id: id,
                            title,
                            rank: -best_score, // Negative so higher score = better rank
                        },
                        best_score,
                    ));
                }
            }

            // Sort by score descending
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(limit);

            Ok(scored.into_iter().map(|(result, _)| result).collect())
        })
    }

    /// Search with FTS5 prefix matching, falling back to fuzzy matching on typos.
    pub fn search_fts_with_fallback(&self, query: &str, limit: usize) -> Result<Vec<FtsSearchResult>> {
        let results = self.search_fts(query, limit)?;
        if !results.is_empty() {
            return Ok(results);
        }
        // FTS5 found nothing — try fuzzy fallback for typo tolerance
        self.search_fuzzy(query, limit)
    }

    // ── Reference Doc FTS ─────────────────────────────────────

    /// Search reference documents using FTS5 full-text search.
    pub fn search_ref_docs_fts(&self, query: &str, limit: usize) -> Result<Vec<FtsSearchResult>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let sanitized = sanitize_fts_query(query);
        if sanitized.is_empty() {
            return Ok(Vec::new());
        }

        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT rd.id, rd.title, rank
                 FROM reference_docs_fts fts
                 INNER JOIN reference_docs rd ON rd.rowid = fts.rowid
                 WHERE reference_docs_fts MATCH ?1
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

/// Sanitize a user query for FTS5 by quoting each token as a literal phrase
/// with a `*` wildcard suffix for prefix matching.
/// This prevents FTS5 operator injection (AND, OR, NOT, NEAR, etc)
/// while allowing partial/prefix queries (e.g. "unt" matches "Untitled").
pub(crate) fn sanitize_fts_query(query: &str) -> String {
    let tokens: Vec<String> = query
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|token| {
            // Escape any double quotes within the token
            let escaped = token.replace('"', "\"\"");
            // Append * for prefix matching: "photo"* matches "photosynthesis"
            format!("\"{}\"*", escaped)
        })
        .collect();

    tokens.join(" ")
}

/// Compute Levenshtein edit distance between two strings (case-insensitive).
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.to_lowercase().chars().collect();
    let b: Vec<char> = b.to_lowercase().chars().collect();
    let (m, n) = (a.len(), b.len());

    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

/// Compute a fuzzy match score for a query against a text.
/// Returns a score between 0.0 and 1.0 where higher is better.
/// Checks each query token against each word in the text,
/// using Levenshtein distance normalized by word length.
fn fuzzy_score(query: &str, text: &str) -> f64 {
    let query_tokens: Vec<&str> = query.split_whitespace().collect();
    if query_tokens.is_empty() || text.is_empty() {
        return 0.0;
    }

    let text_lower = text.to_lowercase();
    let text_words: Vec<&str> = text_lower.split_whitespace().collect();
    if text_words.is_empty() {
        return 0.0;
    }

    let mut total_score = 0.0;

    for q_token in &query_tokens {
        let q_lower = q_token.to_lowercase();
        let mut best_word_score = 0.0_f64;

        for word in &text_words {
            // Substring/prefix match gets a high score
            if word.starts_with(&q_lower) {
                best_word_score = best_word_score.max(1.0);
                break;
            }
            if word.contains(&q_lower) {
                best_word_score = best_word_score.max(0.9);
                continue;
            }

            // For short query tokens, compare against word prefix of same length
            let compare_word = if q_lower.len() < word.len() {
                &word[..q_lower.len().min(word.len())]
            } else {
                word
            };

            let dist = levenshtein(&q_lower, compare_word);
            let max_len = q_lower.len().max(compare_word.len());
            if max_len > 0 {
                let similarity = 1.0 - (dist as f64 / max_len as f64);
                // Only consider it a match if similarity is reasonable
                if similarity >= 0.5 {
                    best_word_score = best_word_score.max(similarity);
                }
            }
        }

        total_score += best_word_score;
    }

    total_score / query_tokens.len() as f64
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
        assert_eq!(sanitize_fts_query("hello world"), "\"hello\"* \"world\"*");
        assert_eq!(sanitize_fts_query("  spaces  "), "\"spaces\"*");
        assert_eq!(sanitize_fts_query(""), "");
        assert_eq!(sanitize_fts_query("AND OR"), "\"AND\"* \"OR\"*");
        assert_eq!(
            sanitize_fts_query("has\"quotes"),
            "\"has\"\"quotes\"*"
        );
    }

    #[test]
    fn test_fts_prefix_matching() {
        let db = test_db();
        let subject_id = create_subject(&db);

        create_plan(&db, &subject_id, "Photosynthesis Basics", "Plants use light", None);
        create_plan(&db, &subject_id, "Cell Division", "Mitosis and meiosis", None);

        // Prefix "photo" should match "Photosynthesis"
        let results = db.search_fts("photo", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Photosynthesis Basics");

        // Single letter prefix
        let results = db.search_fts("ph", 10).unwrap();
        assert_eq!(results.len(), 1);

        // Prefix in content
        let results = db.search_fts("mito", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Cell Division");
    }

    #[test]
    fn test_fts_prefix_multi_token() {
        let db = test_db();
        let subject_id = create_subject(&db);

        create_plan(&db, &subject_id, "Untitled Lesson Plan", "Draft content", None);
        create_plan(&db, &subject_id, "Biology Notes", "Cells and tissues", None);

        // "unt" should match "Untitled"
        let results = db.search_fts("unt", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Untitled Lesson Plan");

        // Multi-token prefix
        let results = db.search_fts("unt les", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Untitled Lesson Plan");
    }

    #[test]
    fn test_fuzzy_search_typo_tolerance() {
        let db = test_db();
        let subject_id = create_subject(&db);

        create_plan(&db, &subject_id, "Photosynthesis Basics", "Plants use light", None);
        create_plan(&db, &subject_id, "Cell Division", "Mitosis and meiosis", None);

        // Typo: "photosythesis" (missing 'n') — FTS5 won't match, fuzzy should
        let results = db.search_fuzzy("photosythesis", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].title, "Photosynthesis Basics");
    }

    #[test]
    fn test_fuzzy_search_empty_query() {
        let db = test_db();
        let results = db.search_fuzzy("", 10).unwrap();
        assert_eq!(results.len(), 0);

        let results = db.search_fuzzy("   ", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_fuzzy_search_no_match() {
        let db = test_db();
        let subject_id = create_subject(&db);

        create_plan(&db, &subject_id, "Photosynthesis", "Plants", None);

        let results = db.search_fuzzy("xyzzyx", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_fuzzy_search_respects_limit() {
        let db = test_db();
        let subject_id = create_subject(&db);

        for i in 0..5 {
            create_plan(&db, &subject_id, &format!("Biology Lesson {}", i), "Cell biology", None);
        }

        let results = db.search_fuzzy("biolgy", 2).unwrap();
        assert!(results.len() <= 2);
    }

    #[test]
    fn test_search_with_fallback_uses_fts_first() {
        let db = test_db();
        let subject_id = create_subject(&db);

        create_plan(&db, &subject_id, "Photosynthesis Basics", "Plants use light", None);

        // Exact prefix — should use FTS5
        let results = db.search_fts_with_fallback("photo", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Photosynthesis Basics");
    }

    #[test]
    fn test_search_with_fallback_uses_fuzzy_on_typo() {
        let db = test_db();
        let subject_id = create_subject(&db);

        create_plan(&db, &subject_id, "Photosynthesis Basics", "Plants use light", None);

        // Typo — FTS5 returns nothing, should fall back to fuzzy
        let results = db.search_fts_with_fallback("photosythesis", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].title, "Photosynthesis Basics");
    }

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("abc", "abc"), 0);
        assert_eq!(levenshtein("photo", "Photo"), 0); // case-insensitive
    }

    #[test]
    fn test_fuzzy_score_exact_prefix() {
        let score = fuzzy_score("photo", "Photosynthesis Basics");
        assert!(score >= 0.9, "Prefix match should score high, got {}", score);
    }

    #[test]
    fn test_fuzzy_score_typo() {
        let score = fuzzy_score("photosythesis", "Photosynthesis Basics");
        assert!(score >= 0.5, "Typo should still score reasonably, got {}", score);
    }

    #[test]
    fn test_fuzzy_score_no_match() {
        let score = fuzzy_score("xyzzyx", "Photosynthesis Basics");
        assert!(score < 0.5, "Unrelated should score low, got {}", score);
    }
}
