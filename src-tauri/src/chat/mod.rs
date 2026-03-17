//! AI Chat module — manages conversations, message persistence, and
//! context-aware generation via RAG.

use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::errors::ChalkError;
use crate::rag::embeddings::EmbeddingClient;
use crate::rag::retrieval;
use crate::AppState;

// ── Models ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConversation {
    pub id: String,
    pub title: String,
    pub plan_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub context_plan_ids: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageInput {
    pub conversation_id: Option<String>,
    pub message: String,
    pub plan_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SendMessageResponse {
    pub conversation_id: String,
    pub user_message: ChatMessage,
    pub assistant_message: ChatMessage,
    pub context_plans: Vec<retrieval::RetrievedContext>,
}

// ── Database CRUD ───────────────────────────────────────────

impl Database {
    pub fn create_conversation(
        &self,
        title: &str,
        plan_id: Option<&str>,
    ) -> crate::database::Result<ChatConversation> {
        let id = uuid::Uuid::new_v4().to_string();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO chat_conversations (id, title, plan_id) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, title, plan_id],
            )?;
            conn.query_row(
                "SELECT id, title, plan_id, created_at, updated_at FROM chat_conversations WHERE id = ?1",
                rusqlite::params![id],
                |row| {
                    Ok(ChatConversation {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        plan_id: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .map_err(|e| crate::database::DatabaseError::Sqlite(e))
        })
    }

    pub fn get_conversation(&self, id: &str) -> crate::database::Result<ChatConversation> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, title, plan_id, created_at, updated_at FROM chat_conversations WHERE id = ?1",
                rusqlite::params![id],
                |row| {
                    Ok(ChatConversation {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        plan_id: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => crate::database::DatabaseError::NotFound,
                other => crate::database::DatabaseError::Sqlite(other),
            })
        })
    }

    pub fn list_conversations(&self) -> crate::database::Result<Vec<ChatConversation>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, plan_id, created_at, updated_at
                 FROM chat_conversations ORDER BY updated_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(ChatConversation {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    plan_id: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn delete_conversation(&self, id: &str) -> crate::database::Result<()> {
        self.with_conn(|conn| {
            let deleted = conn.execute(
                "DELETE FROM chat_conversations WHERE id = ?1",
                rusqlite::params![id],
            )?;
            if deleted == 0 {
                return Err(crate::database::DatabaseError::NotFound);
            }
            Ok(())
        })
    }

    pub fn add_chat_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        context_plan_ids: Option<&str>,
    ) -> crate::database::Result<ChatMessage> {
        let id = uuid::Uuid::new_v4().to_string();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO chat_messages (id, conversation_id, role, content, context_plan_ids) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, conversation_id, role, content, context_plan_ids],
            )?;
            // Update conversation's updated_at.
            conn.execute(
                "UPDATE chat_conversations SET updated_at = datetime('now') WHERE id = ?1",
                rusqlite::params![conversation_id],
            )?;
            conn.query_row(
                "SELECT id, conversation_id, role, content, context_plan_ids, created_at FROM chat_messages WHERE id = ?1",
                rusqlite::params![id],
                |row| {
                    Ok(ChatMessage {
                        id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        role: row.get(2)?,
                        content: row.get(3)?,
                        context_plan_ids: row.get(4)?,
                        created_at: row.get(5)?,
                    })
                },
            )
            .map_err(|e| crate::database::DatabaseError::Sqlite(e))
        })
    }

    pub fn get_chat_messages(
        &self,
        conversation_id: &str,
    ) -> crate::database::Result<Vec<ChatMessage>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, conversation_id, role, content, context_plan_ids, created_at
                 FROM chat_messages WHERE conversation_id = ?1 ORDER BY created_at ASC",
            )?;
            let rows = stmt.query_map(rusqlite::params![conversation_id], |row| {
                Ok(ChatMessage {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    context_plan_ids: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }
}

// ── Chat Completion ─────────────────────────────────────────

/// System prompt for the Chalk AI assistant.
const SYSTEM_PROMPT: &str = r#"You are Chalk, an AI teaching assistant embedded in a lesson plan application. Your role is to help teachers create, refine, and improve their lesson plans.

You have access to the teacher's lesson plan history. When relevant plans are found in their history, they will be provided as context. Use this context to:
- Reference what has worked before ("You taught a similar topic in your Photosynthesis Lab — here's what you covered...")
- Suggest improvements based on patterns in their teaching style
- Help maintain consistency across their curriculum
- Build on existing materials rather than starting from scratch

Be concise, practical, and focused on actionable teaching advice. Match the teacher's style when you can see it in their history."#;

/// OpenAI chat completion request/response types.
#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<CompletionMessage>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Serialize, Deserialize, Clone)]
struct CompletionMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<CompletionChoice>,
}

#[derive(Deserialize)]
struct CompletionChoice {
    message: CompletionMessage,
}

/// Send a chat message with RAG context to the AI.
async fn generate_response(
    api_key: &str,
    base_url: &str,
    model: &str,
    history: &[ChatMessage],
    user_message: &str,
    rag_context: &str,
) -> Result<String, ChalkError> {
    let mut messages = Vec::new();

    // System prompt with optional RAG context.
    let system_content = if rag_context.is_empty() {
        SYSTEM_PROMPT.to_string()
    } else {
        format!("{SYSTEM_PROMPT}\n\n{rag_context}")
    };

    messages.push(CompletionMessage {
        role: "system".to_string(),
        content: system_content,
    });

    // Include conversation history (last 20 messages to stay within token budget).
    let history_slice = if history.len() > 20 {
        &history[history.len() - 20..]
    } else {
        history
    };

    for msg in history_slice {
        if msg.role == "user" || msg.role == "assistant" {
            messages.push(CompletionMessage {
                role: msg.role.clone(),
                content: msg.content.clone(),
            });
        }
    }

    // The current user message.
    messages.push(CompletionMessage {
        role: "user".to_string(),
        content: user_message.to_string(),
    });

    let request = ChatCompletionRequest {
        model: model.to_string(),
        messages,
        max_tokens: 2048,
        temperature: 0.7,
    };

    let url = format!("{base_url}/chat/completions");
    let client = reqwest::Client::new();

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| ChalkError::connector_api(format!("Chat API request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "unknown".to_string());
        return Err(ChalkError::connector_api(format!(
            "Chat API returned {status}: {body}"
        )));
    }

    let result: ChatCompletionResponse = response
        .json()
        .await
        .map_err(|e| ChalkError::connector_api(format!("Failed to parse chat response: {e}")))?;

    result
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .ok_or_else(|| ChalkError::connector_api("Empty response from chat API"))
}

// ── Tauri Commands ──────────────────────────────────────────

/// Send a message in a chat conversation with RAG-enhanced context.
/// Creates a new conversation if `conversation_id` is None.
#[tauri::command]
pub async fn send_chat_message(
    state: tauri::State<'_, AppState>,
    input: SendMessageInput,
) -> Result<SendMessageResponse, String> {
    let db = &state.db;

    // Get or create conversation.
    let conversation_id = match input.conversation_id {
        Some(id) => {
            // Verify it exists.
            db.get_conversation(&id).map_err(|e| format!("{e}"))?;
            id
        }
        None => {
            // Create new conversation; use first ~50 chars of message as title.
            let title = if input.message.len() > 50 {
                format!("{}...", &input.message[..47])
            } else {
                input.message.clone()
            };
            let conv = db
                .create_conversation(&title, input.plan_id.as_deref())
                .map_err(|e| format!("{e}"))?;
            conv.id
        }
    };

    // Get API configuration.
    let api_key = db
        .get_setting("openai_api_key")
        .map_err(|e| format!("{e}"))?
        .ok_or_else(|| "OpenAI API key not configured. Set it in Settings.".to_string())?;

    let base_url = db
        .get_setting("openai_base_url")
        .map_err(|e| format!("{e}"))?
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

    let model = db
        .get_setting("chat_model")
        .map_err(|e| format!("{e}"))?
        .unwrap_or_else(|| "gpt-4o-mini".to_string());

    // RAG: retrieve relevant plans.
    let embedding_client = EmbeddingClient::new(api_key.clone());
    let context_plans = retrieval::retrieve_relevant_plans(db, &embedding_client, &input.message)
        .await
        .unwrap_or_default(); // Don't fail the whole request if RAG fails.

    let rag_context = retrieval::format_context_for_prompt(&context_plans);
    let context_plan_ids: Vec<&str> = context_plans.iter().map(|c| c.plan_id.as_str()).collect();
    let context_ids_json = if context_plan_ids.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&context_plan_ids).unwrap_or_default())
    };

    // Store the user message.
    let user_msg = db
        .add_chat_message(
            &conversation_id,
            "user",
            &input.message,
            context_ids_json.as_deref(),
        )
        .map_err(|e| format!("{e}"))?;

    // Get conversation history for context.
    let history = db
        .get_chat_messages(&conversation_id)
        .map_err(|e| format!("{e}"))?;

    // Generate AI response.
    let ai_response =
        generate_response(&api_key, &base_url, &model, &history, &input.message, &rag_context)
            .await
            .map_err(|e| e.message)?;

    // Store the assistant message.
    let assistant_msg = db
        .add_chat_message(&conversation_id, "assistant", &ai_response, None)
        .map_err(|e| format!("{e}"))?;

    Ok(SendMessageResponse {
        conversation_id,
        user_message: user_msg,
        assistant_message: assistant_msg,
        context_plans,
    })
}

/// Get all messages in a conversation.
#[tauri::command]
pub fn get_chat_messages_cmd(
    state: tauri::State<'_, AppState>,
    conversation_id: String,
) -> Result<Vec<ChatMessage>, String> {
    state
        .db
        .get_chat_messages(&conversation_id)
        .map_err(|e| format!("{e}"))
}

/// List all conversations, most recent first.
#[tauri::command]
pub fn list_conversations(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ChatConversation>, String> {
    state.db.list_conversations().map_err(|e| format!("{e}"))
}

/// Delete a conversation and all its messages.
#[tauri::command]
pub fn delete_conversation(
    state: tauri::State<'_, AppState>,
    conversation_id: String,
) -> Result<(), String> {
    state
        .db
        .delete_conversation(&conversation_id)
        .map_err(|e| format!("{e}"))
}

/// Vectorize a lesson plan: generate embedding and store in sqlite-vec.
/// Called when a plan is finalized or updated.
#[tauri::command]
pub async fn vectorize_plan(
    state: tauri::State<'_, AppState>,
    plan_id: String,
) -> Result<(), String> {
    let db = &state.db;

    let api_key = db
        .get_setting("openai_api_key")
        .map_err(|e| format!("{e}"))?
        .ok_or_else(|| "OpenAI API key not configured".to_string())?;

    let plan = db.get_lesson_plan(&plan_id).map_err(|e| format!("{e}"))?;

    let embedding_text = crate::rag::chunker::create_embedding_text(
        &plan.title,
        &plan.content,
        plan.learning_objectives.as_deref(),
    );

    let client = EmbeddingClient::new(api_key);
    let embedding = client
        .embed_one(&embedding_text)
        .await
        .map_err(|e| e.message)?;

    db.upsert_embedding(&plan_id, &embedding)
        .map_err(|e| format!("{e}"))?;

    tracing::info!(plan_id = %plan_id, "Plan vectorized successfully");
    Ok(())
}

/// Vectorize all existing plans that don't have embeddings yet.
#[tauri::command]
pub async fn vectorize_all_plans(state: tauri::State<'_, AppState>) -> Result<u32, String> {
    let db = &state.db;

    let api_key = db
        .get_setting("openai_api_key")
        .map_err(|e| format!("{e}"))?
        .ok_or_else(|| "OpenAI API key not configured".to_string())?;

    // Get all plans that don't have embeddings.
    let plans_without_embeddings = db
        .list_plans_without_embeddings()
        .map_err(|e| format!("{e}"))?;

    if plans_without_embeddings.is_empty() {
        return Ok(0);
    }

    let client = EmbeddingClient::new(api_key);
    let mut count = 0u32;

    for plan in &plans_without_embeddings {
        let embedding_text = crate::rag::chunker::create_embedding_text(
            &plan.title,
            &plan.content,
            plan.learning_objectives.as_deref(),
        );

        match client.embed_one(&embedding_text).await {
            Ok(embedding) => {
                if let Err(e) = db.upsert_embedding(&plan.id, &embedding) {
                    tracing::warn!(plan_id = %plan.id, error = %e, "Failed to store embedding");
                } else {
                    count += 1;
                }
            }
            Err(e) => {
                tracing::warn!(plan_id = %plan.id, error = %e.message, "Failed to generate embedding");
            }
        }
    }

    tracing::info!(count, "Vectorized plans");
    Ok(count)
}

/// Save AI configuration settings.
#[tauri::command]
pub fn save_ai_config(
    state: tauri::State<'_, AppState>,
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
) -> Result<(), String> {
    let db = &state.db;

    if let Some(key) = api_key {
        db.set_setting("openai_api_key", &key)
            .map_err(|e| format!("{e}"))?;
    }
    if let Some(url) = base_url {
        db.set_setting("openai_base_url", &url)
            .map_err(|e| format!("{e}"))?;
    }
    if let Some(m) = model {
        db.set_setting("chat_model", &m)
            .map_err(|e| format!("{e}"))?;
    }

    Ok(())
}

/// Get current AI configuration (without the API key for security).
#[tauri::command]
pub fn get_ai_config(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let db = &state.db;
    let has_key = db
        .get_setting("openai_api_key")
        .map_err(|e| format!("{e}"))?
        .is_some();
    let base_url = db
        .get_setting("openai_base_url")
        .map_err(|e| format!("{e}"))?
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
    let model = db
        .get_setting("chat_model")
        .map_err(|e| format!("{e}"))?
        .unwrap_or_else(|| "gpt-4o-mini".to_string());

    Ok(serde_json::json!({
        "has_api_key": has_key,
        "base_url": base_url,
        "model": model,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn test_conversation_crud() {
        let db = test_db();

        // Create conversation.
        let conv = db.create_conversation("Test Chat", None).unwrap();
        assert_eq!(conv.title, "Test Chat");
        assert!(conv.plan_id.is_none());

        // Get conversation.
        let fetched = db.get_conversation(&conv.id).unwrap();
        assert_eq!(fetched.title, "Test Chat");

        // List conversations.
        let all = db.list_conversations().unwrap();
        assert_eq!(all.len(), 1);

        // Delete conversation.
        db.delete_conversation(&conv.id).unwrap();
        assert!(db.get_conversation(&conv.id).is_err());
    }

    #[test]
    fn test_conversation_with_plan() {
        let db = test_db();

        // Need a subject + plan first.
        let subject = db
            .create_subject(&crate::database::NewSubject {
                name: "Science".into(),
                grade_level: None,
                description: None,
            })
            .unwrap();
        let plan = db
            .create_lesson_plan(&crate::database::NewLessonPlan {
                subject_id: subject.id.clone(),
                title: "Test Plan".into(),
                content: None,
                source_doc_id: None,
                source_table_index: None,
                learning_objectives: None,
            })
            .unwrap();

        let conv = db
            .create_conversation("Plan Chat", Some(&plan.id))
            .unwrap();
        assert_eq!(conv.plan_id.as_deref(), Some(plan.id.as_str()));
    }

    #[test]
    fn test_chat_messages() {
        let db = test_db();
        let conv = db.create_conversation("Test", None).unwrap();

        // Add messages.
        let msg1 = db
            .add_chat_message(&conv.id, "user", "Hello!", None)
            .unwrap();
        assert_eq!(msg1.role, "user");
        assert_eq!(msg1.content, "Hello!");

        let msg2 = db
            .add_chat_message(&conv.id, "assistant", "Hi there!", None)
            .unwrap();
        assert_eq!(msg2.role, "assistant");

        // With context plan IDs.
        let msg3 = db
            .add_chat_message(
                &conv.id,
                "user",
                "Tell me about photosynthesis",
                Some(r#"["plan-1","plan-2"]"#),
            )
            .unwrap();
        assert_eq!(
            msg3.context_plan_ids.as_deref(),
            Some(r#"["plan-1","plan-2"]"#)
        );

        // Get all messages.
        let messages = db.get_chat_messages(&conv.id).unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[2].role, "user");
    }

    #[test]
    fn test_cascade_delete_messages() {
        let db = test_db();
        let conv = db.create_conversation("Test", None).unwrap();

        db.add_chat_message(&conv.id, "user", "Hello", None)
            .unwrap();
        db.add_chat_message(&conv.id, "assistant", "Hi", None)
            .unwrap();

        // Delete conversation should cascade to messages.
        db.delete_conversation(&conv.id).unwrap();
        let messages = db.get_chat_messages(&conv.id).unwrap();
        assert_eq!(messages.len(), 0);
    }
}
