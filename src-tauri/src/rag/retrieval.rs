//! RAG retrieval: find relevant reference documents and assemble context
//! for the AI chat. Uses hybrid search (FTS5 + vector) when embeddings are
//! available, falling back to FTS5-only when they are not.
//!
//! Reference documents are extracted from the teacher's Google Docs during
//! digest and stored separately from user-created lesson plans.

use crate::database::{Database, HybridSearchResult, ReferenceDoc};
use crate::errors::ChalkError;
use crate::rag::embeddings::EmbeddingClient;

/// A retrieved reference document with its similarity score, ready for context injection.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RetrievedContext {
    pub plan_id: String,
    pub title: String,
    pub content: String,
    /// Original HTML from the Google Docs export, preserving tables, colors, and formatting.
    pub content_html: String,
    pub learning_objectives: Option<String>,
    pub distance: f64,
}

/// Maximum number of reference docs to retrieve for context.
const MAX_CONTEXT_DOCS: usize = 5;
/// Maximum total characters of context to include (to stay within LLM token limits).
/// Increased from 8000 to accommodate HTML content alongside plain text.
const MAX_CONTEXT_CHARS: usize = 12000;
/// Relative weight for FTS5 results in hybrid scoring.
const FTS_WEIGHT: f64 = 1.0;
/// Relative weight for vector results in hybrid scoring.
const VEC_WEIGHT: f64 = 1.0;

/// Retrieve the most relevant reference documents for a given query using hybrid search.
///
/// 1. Embeds the query text
/// 2. Runs hybrid search (FTS5 + sqlite-vec) with RRF re-ranking on reference_docs
/// 3. Fetches full reference doc content for the top matches
/// 4. Trims to stay within context budget
///
/// Falls back to FTS5-only search if embedding generation fails.
pub async fn retrieve_relevant_plans(
    db: &Database,
    embedding_client: &EmbeddingClient,
    query: &str,
) -> Result<Vec<RetrievedContext>, ChalkError> {
    // Try to generate query embedding for hybrid search.
    let search_results: Vec<HybridSearchResult> = match embedding_client.embed_one(query).await {
        Ok(query_embedding) => {
            // Full hybrid search: FTS5 + vector on reference_docs.
            db.search_ref_docs_hybrid(query, &query_embedding, MAX_CONTEXT_DOCS, FTS_WEIGHT, VEC_WEIGHT)
                .map_err(|e| ChalkError::db_query(format!("Hybrid search failed: {e}")))?
        }
        Err(_) => {
            // Fallback to FTS5-only when embeddings unavailable.
            db.search_ref_docs_hybrid_fts_only(query, MAX_CONTEXT_DOCS)
                .map_err(|e| ChalkError::db_query(format!("FTS search failed: {e}")))?
        }
    };

    if search_results.is_empty() {
        return Ok(Vec::new());
    }

    // Fetch full reference doc content for each match.
    let mut contexts = Vec::with_capacity(search_results.len());
    let mut total_chars = 0;

    for result in &search_results {
        let doc: ReferenceDoc = match db.get_reference_doc(&result.lesson_plan_id) {
            Ok(d) => d,
            Err(_) => continue, // Doc may have been deleted since indexing.
        };

        // Budget accounts for both plain text and HTML content.
        let content_len = doc.content_text.len() + doc.content_html.len() + doc.title.len();
        if total_chars + content_len > MAX_CONTEXT_CHARS && !contexts.is_empty() {
            break; // Budget exceeded; stop adding more context.
        }

        total_chars += content_len;
        contexts.push(RetrievedContext {
            plan_id: doc.id,
            title: doc.title,
            content: doc.content_text,
            content_html: doc.content_html,
            learning_objectives: None,
            distance: 1.0 - result.score, // Convert score to distance-like metric
        });
    }

    Ok(contexts)
}

/// Format retrieved contexts into a prompt-friendly string for the LLM.
pub fn format_context_for_prompt(contexts: &[RetrievedContext]) -> String {
    if contexts.is_empty() {
        return String::new();
    }

    let mut parts = Vec::with_capacity(contexts.len() + 1);
    parts.push("Here are relevant documents from your teaching history:\n".to_string());

    for (i, ctx) in contexts.iter().enumerate() {
        let mut entry = format!("--- Reference {} ---\nTitle: {}\n", i + 1, ctx.title);
        if let Some(ref obj) = ctx.learning_objectives {
            if !obj.is_empty() {
                entry.push_str(&format!("Objectives: {obj}\n"));
            }
        }
        // Include original HTML when available so the AI can preserve formatting.
        if !ctx.content_html.is_empty() {
            let html = if ctx.content_html.len() > 3000 {
                format!("{}...", &ctx.content_html[..3000])
            } else {
                ctx.content_html.clone()
            };
            entry.push_str(&format!("Original HTML (preserve this formatting):\n{html}\n"));
        }
        // Truncate very long plain-text content to keep prompt manageable.
        let content = if ctx.content.len() > 2000 {
            format!("{}...", &ctx.content[..2000])
        } else {
            ctx.content.clone()
        };
        entry.push_str(&format!("Plain text summary:\n{content}\n"));
        parts.push(entry);
    }

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_context_empty() {
        assert_eq!(format_context_for_prompt(&[]), "");
    }

    #[test]
    fn test_format_context_single_plan() {
        let contexts = vec![RetrievedContext {
            plan_id: "abc".to_string(),
            title: "Photosynthesis Lab".to_string(),
            content: "Students will study light reactions.".to_string(),
            content_html: "<table><tr><td style=\"background:#90EE90\">Light Reactions</td></tr></table>".to_string(),
            learning_objectives: Some("Understand photosynthesis".to_string()),
            distance: 0.15,
        }];

        let formatted = format_context_for_prompt(&contexts);
        assert!(formatted.contains("Reference 1"));
        assert!(formatted.contains("Photosynthesis Lab"));
        assert!(formatted.contains("Understand photosynthesis"));
        assert!(formatted.contains("light reactions"));
        assert!(formatted.contains("Original HTML"));
        assert!(formatted.contains("<table>"));
    }

    #[test]
    fn test_format_context_truncates_long_content() {
        let long_content = "x".repeat(3000);
        let contexts = vec![RetrievedContext {
            plan_id: "abc".to_string(),
            title: "Long Plan".to_string(),
            content: long_content,
            content_html: String::new(),
            learning_objectives: None,
            distance: 0.1,
        }];

        let formatted = format_context_for_prompt(&contexts);
        assert!(formatted.contains("..."));
    }

    #[test]
    fn test_format_context_truncates_long_html() {
        let long_html = "<td>x</td>".repeat(600);
        let contexts = vec![RetrievedContext {
            plan_id: "abc".to_string(),
            title: "Long HTML Plan".to_string(),
            content: "Short text".to_string(),
            content_html: long_html,
            learning_objectives: None,
            distance: 0.1,
        }];

        let formatted = format_context_for_prompt(&contexts);
        assert!(formatted.contains("Original HTML"));
        assert!(formatted.contains("..."));
    }

    #[test]
    fn test_format_context_omits_html_section_when_empty() {
        let contexts = vec![RetrievedContext {
            plan_id: "abc".to_string(),
            title: "No HTML Plan".to_string(),
            content: "Some text".to_string(),
            content_html: String::new(),
            learning_objectives: None,
            distance: 0.1,
        }];

        let formatted = format_context_for_prompt(&contexts);
        assert!(!formatted.contains("Original HTML"));
        assert!(formatted.contains("Plain text summary"));
    }

    #[test]
    fn test_format_context_multiple_plans() {
        let contexts = vec![
            RetrievedContext {
                plan_id: "a".to_string(),
                title: "Plan A".to_string(),
                content: "Content A".to_string(),
                content_html: "<b>Content A</b>".to_string(),
                learning_objectives: None,
                distance: 0.1,
            },
            RetrievedContext {
                plan_id: "b".to_string(),
                title: "Plan B".to_string(),
                content: "Content B".to_string(),
                content_html: "<em>Content B</em>".to_string(),
                learning_objectives: Some("Goals B".to_string()),
                distance: 0.2,
            },
        ];

        let formatted = format_context_for_prompt(&contexts);
        assert!(formatted.contains("Reference 1"));
        assert!(formatted.contains("Reference 2"));
        assert!(formatted.contains("Plan A"));
        assert!(formatted.contains("Plan B"));
        assert!(formatted.contains("Goals B"));
    }
}
