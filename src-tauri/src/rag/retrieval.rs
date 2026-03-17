//! RAG retrieval: find relevant lesson plan history and assemble context
//! for the AI chat.

use crate::database::{Database, LessonPlan, VectorSearchResult};
use crate::errors::ChalkError;
use crate::rag::embeddings::EmbeddingClient;

/// A retrieved lesson plan with its similarity score, ready for context injection.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RetrievedContext {
    pub plan_id: String,
    pub title: String,
    pub content: String,
    pub learning_objectives: Option<String>,
    pub distance: f64,
}

/// Maximum number of plans to retrieve for context.
const MAX_CONTEXT_PLANS: usize = 5;
/// Maximum total characters of context to include (to stay within LLM token limits).
const MAX_CONTEXT_CHARS: usize = 8000;

/// Retrieve the most relevant lesson plans for a given query using vector search.
///
/// 1. Embeds the query text
/// 2. Searches sqlite-vec for similar plan embeddings
/// 3. Fetches full plan content for the top matches
/// 4. Trims to stay within context budget
pub async fn retrieve_relevant_plans(
    db: &Database,
    embedding_client: &EmbeddingClient,
    query: &str,
) -> Result<Vec<RetrievedContext>, ChalkError> {
    // Generate query embedding.
    let query_embedding = embedding_client.embed_one(query).await?;

    // Search for similar plans.
    let search_results: Vec<VectorSearchResult> = db
        .search_similar(&query_embedding, MAX_CONTEXT_PLANS)
        .map_err(|e| ChalkError::db_query(format!("Vector search failed: {e}")))?;

    if search_results.is_empty() {
        return Ok(Vec::new());
    }

    // Fetch full plan content for each match.
    let mut contexts = Vec::with_capacity(search_results.len());
    let mut total_chars = 0;

    for result in &search_results {
        let plan: LessonPlan = match db.get_lesson_plan(&result.lesson_plan_id) {
            Ok(p) => p,
            Err(_) => continue, // Plan may have been deleted since embedding.
        };

        let content_len = plan.content.len() + plan.title.len();
        if total_chars + content_len > MAX_CONTEXT_CHARS && !contexts.is_empty() {
            break; // Budget exceeded; stop adding more context.
        }

        total_chars += content_len;
        contexts.push(RetrievedContext {
            plan_id: plan.id,
            title: plan.title,
            content: plan.content,
            learning_objectives: plan.learning_objectives,
            distance: result.distance,
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
    parts.push("Here are relevant lesson plans from your teaching history:\n".to_string());

    for (i, ctx) in contexts.iter().enumerate() {
        let mut entry = format!("--- Plan {} ---\nTitle: {}\n", i + 1, ctx.title);
        if let Some(ref obj) = ctx.learning_objectives {
            if !obj.is_empty() {
                entry.push_str(&format!("Objectives: {obj}\n"));
            }
        }
        // Truncate very long content to keep prompt manageable.
        let content = if ctx.content.len() > 2000 {
            format!("{}...", &ctx.content[..2000])
        } else {
            ctx.content.clone()
        };
        entry.push_str(&format!("Content:\n{content}\n"));
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
            learning_objectives: Some("Understand photosynthesis".to_string()),
            distance: 0.15,
        }];

        let formatted = format_context_for_prompt(&contexts);
        assert!(formatted.contains("Plan 1"));
        assert!(formatted.contains("Photosynthesis Lab"));
        assert!(formatted.contains("Understand photosynthesis"));
        assert!(formatted.contains("light reactions"));
    }

    #[test]
    fn test_format_context_truncates_long_content() {
        let long_content = "x".repeat(3000);
        let contexts = vec![RetrievedContext {
            plan_id: "abc".to_string(),
            title: "Long Plan".to_string(),
            content: long_content,
            learning_objectives: None,
            distance: 0.1,
        }];

        let formatted = format_context_for_prompt(&contexts);
        assert!(formatted.contains("..."));
        // Should be truncated to ~2000 chars content + metadata.
        assert!(formatted.len() < 2500);
    }

    #[test]
    fn test_format_context_multiple_plans() {
        let contexts = vec![
            RetrievedContext {
                plan_id: "a".to_string(),
                title: "Plan A".to_string(),
                content: "Content A".to_string(),
                learning_objectives: None,
                distance: 0.1,
            },
            RetrievedContext {
                plan_id: "b".to_string(),
                title: "Plan B".to_string(),
                content: "Content B".to_string(),
                learning_objectives: Some("Goals B".to_string()),
                distance: 0.2,
            },
        ];

        let formatted = format_context_for_prompt(&contexts);
        assert!(formatted.contains("Plan 1"));
        assert!(formatted.contains("Plan 2"));
        assert!(formatted.contains("Plan A"));
        assert!(formatted.contains("Plan B"));
        assert!(formatted.contains("Goals B"));
    }
}
