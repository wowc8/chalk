//! AI Chat module — manages conversations, message persistence, and
//! context-aware generation via RAG.

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::database::Database;
use crate::errors::ChalkError;
use crate::events;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
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

/// Send a chat message with RAG context to the AI (non-streaming).
async fn generate_response(
    api_key: &str,
    base_url: &str,
    model: &str,
    history: &[ChatMessage],
    user_message: &str,
    rag_context: &str,
) -> Result<String, ChalkError> {
    let messages = build_messages(history, user_message, rag_context);

    let request = ChatCompletionRequest {
        model: model.to_string(),
        messages,
        max_tokens: 2048,
        temperature: 0.7,
        stream: None,
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

/// SSE streaming delta types for OpenAI streaming responses.
#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

/// Build the messages array for a chat completion request.
fn build_messages(
    history: &[ChatMessage],
    user_message: &str,
    rag_context: &str,
) -> Vec<CompletionMessage> {
    let mut messages = Vec::new();

    let system_content = if rag_context.is_empty() {
        SYSTEM_PROMPT.to_string()
    } else {
        format!("{SYSTEM_PROMPT}\n\n{rag_context}")
    };

    messages.push(CompletionMessage {
        role: "system".to_string(),
        content: system_content,
    });

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

    messages.push(CompletionMessage {
        role: "user".to_string(),
        content: user_message.to_string(),
    });

    messages
}

/// Stream a chat completion response, emitting tokens via Tauri events.
async fn generate_response_stream(
    app: &tauri::AppHandle,
    api_key: &str,
    base_url: &str,
    model: &str,
    history: &[ChatMessage],
    user_message: &str,
    rag_context: &str,
    conversation_id: &str,
) -> Result<String, ChalkError> {
    let messages = build_messages(history, user_message, rag_context);

    let request = ChatCompletionRequest {
        model: model.to_string(),
        messages,
        max_tokens: 2048,
        temperature: 0.7,
        stream: Some(true),
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

    // Read SSE stream line by line.
    let mut full_content = String::new();
    let mut stream = response.bytes_stream();

    use futures_util::StreamExt;
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| {
            ChalkError::connector_api(format!("Stream read error: {e}"))
        })?;

        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // Process complete SSE lines from buffer.
        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim_end_matches('\r').to_string();
            buffer = buffer[line_end + 1..].to_string();

            if line.is_empty() {
                continue;
            }

            if let Some(data) = line.strip_prefix("data: ") {
                if data.trim() == "[DONE]" {
                    break;
                }

                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                    for choice in &chunk.choices {
                        if let Some(content) = &choice.delta.content {
                            full_content.push_str(content);
                            events::emit_chat_stream_token(
                                app,
                                events::ChatStreamTokenPayload {
                                    conversation_id: conversation_id.to_string(),
                                    token: content.clone(),
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    if full_content.is_empty() {
        return Err(ChalkError::connector_api("Empty streaming response from chat API"));
    }

    Ok(full_content)
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

/// Send a message with streaming response via Tauri events.
/// Returns the conversation_id and user message immediately; the assistant
/// response streams via `chat:stream_token` events, with `chat:stream_done`
/// or `chat:stream_error` emitted when complete.
#[derive(Debug, Serialize)]
pub struct StreamStartResponse {
    pub conversation_id: String,
    pub user_message: ChatMessage,
    pub context_plans: Vec<retrieval::RetrievedContext>,
}

#[tauri::command]
pub async fn send_chat_message_stream(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: SendMessageInput,
) -> Result<StreamStartResponse, String> {
    let db = &state.db;

    // Get or create conversation.
    let conversation_id = match input.conversation_id {
        Some(id) => {
            db.get_conversation(&id).map_err(|e| format!("{e}"))?;
            id
        }
        None => {
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
        .unwrap_or_default();

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

    let response = StreamStartResponse {
        conversation_id: conversation_id.clone(),
        user_message: user_msg,
        context_plans,
    };

    // Spawn the streaming generation in the background so we return immediately.
    let conv_id = conversation_id.clone();
    let msg = input.message.clone();
    let ctx_ids = context_ids_json.clone();
    let app_clone = app.clone();

    tauri::async_runtime::spawn(async move {
        let state = app_clone.state::<AppState>();
        match generate_response_stream(
            &app_clone,
            &api_key,
            &base_url,
            &model,
            &history,
            &msg,
            &rag_context,
            &conv_id,
        )
        .await
        {
            Ok(full_content) => {
                // Store the completed assistant message.
                match state.db.add_chat_message(&conv_id, "assistant", &full_content, None) {
                    Ok(assistant_msg) => {
                        events::emit_chat_stream_done(
                            &app_clone,
                            events::ChatStreamDonePayload {
                                conversation_id: conv_id,
                                message_id: assistant_msg.id,
                                full_content,
                                context_plan_ids: ctx_ids,
                            },
                        );
                    }
                    Err(e) => {
                        events::emit_chat_stream_error(
                            &app_clone,
                            events::ChatStreamErrorPayload {
                                conversation_id: conv_id,
                                error: format!("Failed to save message: {e}"),
                            },
                        );
                    }
                }
            }
            Err(e) => {
                events::emit_chat_stream_error(
                    &app_clone,
                    events::ChatStreamErrorPayload {
                        conversation_id: conv_id,
                        error: e.message,
                    },
                );
            }
        }
    });

    Ok(response)
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
    fn test_build_messages_without_context() {
        let history: Vec<ChatMessage> = vec![];
        let messages = build_messages(&history, "Hello", "");

        assert_eq!(messages.len(), 2); // system + user
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[0].content, SYSTEM_PROMPT);
        assert_eq!(messages[1].role, "user");
        assert_eq!(messages[1].content, "Hello");
    }

    #[test]
    fn test_build_messages_with_rag_context() {
        let history: Vec<ChatMessage> = vec![];
        let rag_ctx = "Relevant plan: Photosynthesis Lab";
        let messages = build_messages(&history, "Help me", rag_ctx);

        assert_eq!(messages.len(), 2);
        assert!(messages[0].content.contains(SYSTEM_PROMPT));
        assert!(messages[0].content.contains(rag_ctx));
    }

    #[test]
    fn test_build_messages_with_history() {
        let history = vec![
            ChatMessage {
                id: "1".into(),
                conversation_id: "c1".into(),
                role: "user".into(),
                content: "Hi".into(),
                context_plan_ids: None,
                created_at: "2024-01-01".into(),
            },
            ChatMessage {
                id: "2".into(),
                conversation_id: "c1".into(),
                role: "assistant".into(),
                content: "Hello!".into(),
                context_plan_ids: None,
                created_at: "2024-01-01".into(),
            },
        ];
        let messages = build_messages(&history, "New question", "");

        // system + 2 history + user = 4
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[1].content, "Hi");
        assert_eq!(messages[2].content, "Hello!");
        assert_eq!(messages[3].content, "New question");
    }

    #[test]
    fn test_build_messages_truncates_long_history() {
        // Create 25 messages — should only keep last 20.
        let history: Vec<ChatMessage> = (0..25)
            .map(|i| ChatMessage {
                id: format!("msg-{i}"),
                conversation_id: "c1".into(),
                role: if i % 2 == 0 { "user" } else { "assistant" }.into(),
                content: format!("Message {i}"),
                context_plan_ids: None,
                created_at: "2024-01-01".into(),
            })
            .collect();

        let messages = build_messages(&history, "Final", "");
        // system + 20 history + user = 22
        assert_eq!(messages.len(), 22);
        // First history message should be index 5 (25-20=5).
        assert_eq!(messages[1].content, "Message 5");
    }

    #[test]
    fn test_build_messages_skips_system_role_in_history() {
        let history = vec![
            ChatMessage {
                id: "1".into(),
                conversation_id: "c1".into(),
                role: "system".into(),
                content: "Should be skipped".into(),
                context_plan_ids: None,
                created_at: "2024-01-01".into(),
            },
            ChatMessage {
                id: "2".into(),
                conversation_id: "c1".into(),
                role: "user".into(),
                content: "Included".into(),
                context_plan_ids: None,
                created_at: "2024-01-01".into(),
            },
        ];
        let messages = build_messages(&history, "Hi", "");

        // system + 1 user from history (system skipped) + user = 3
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1].content, "Included");
    }

    #[test]
    fn test_stream_chunk_deserialization() {
        let json = r#"{"id":"chatcmpl-abc","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("Hello"));
    }

    #[test]
    fn test_stream_chunk_empty_delta() {
        let json = r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert!(chunk.choices[0].delta.content.is_none());
    }

    #[test]
    fn test_chat_completion_request_serialization_no_stream() {
        let req = ChatCompletionRequest {
            model: "gpt-4o-mini".into(),
            messages: vec![CompletionMessage {
                role: "user".into(),
                content: "Hello".into(),
            }],
            max_tokens: 2048,
            temperature: 0.7,
            stream: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("stream").is_none());
    }

    #[test]
    fn test_chat_completion_request_serialization_with_stream() {
        let req = ChatCompletionRequest {
            model: "gpt-4o".into(),
            messages: vec![],
            max_tokens: 2048,
            temperature: 0.7,
            stream: Some(true),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["stream"], true);
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
