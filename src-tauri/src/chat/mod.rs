//! AI Chat module — manages conversations, message persistence, and
//! context-aware generation via RAG.

pub mod openai;
pub mod provider;

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::database::{Database, TeachingTemplateSchema};
use crate::errors::ChalkError;
use crate::events;
use crate::rag::embeddings::EmbeddingClient;
use crate::rag::retrieval;
use crate::AppState;

use provider::{AiProviderConfig, AiProviderFactory, CompletionMessage};

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
    /// Current content of the active lesson plan in the editor.
    pub plan_content: Option<String>,
    /// Title of the active lesson plan.
    pub plan_title: Option<String>,
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
const SYSTEM_PROMPT: &str = r#"You are Chalk — a seasoned, collaborative teaching partner embedded in a lesson plan editor. Think of yourself as the experienced colleague down the hall who's seen hundreds of lesson plans, knows what works in a real classroom, and always has a practical suggestion ready.

## Your Expertise
You bring deep knowledge in curriculum design and backwards planning (Understanding by Design), differentiation strategies (tiered activities, flexible grouping, scaffolding for ELLs and IEP students), formative and summative assessment design (exit tickets, rubrics, performance tasks), grade-level developmental expectations from Pre-K through elementary, classroom management woven into instructional flow, and cross-curricular integration opportunities.

## How You Work
- **Be specific, not generic.** Instead of "add an assessment," say "try a quick exit ticket where students sketch the water cycle and label three stages — that tells you in 30 seconds who got it."
- **Proactively flag gaps.** If a lesson plan is missing learning objectives, assessment, differentiation, or closure, mention it — don't wait to be asked.
- **Suggest improvements.** When you see a solid plan, still offer one or two ways to level it up: a higher-order thinking question, a turn-and-talk moment, a formative check.
- **Speak like a colleague.** Warm, direct, and encouraging. Skip the jargon-heavy academic tone. Say "this is a strong opening hook" not "the anticipatory set demonstrates pedagogical soundness."
- **Reference best practices naturally.** Bloom's Taxonomy, Webb's DOK, Gradual Release of Responsibility, UDL — weave these in when relevant, but explain them in plain language.
- **Respect the teacher's style.** Build on what they already do well. Frame suggestions as options, not mandates.

## What You Help With
- Drafting clear, measurable learning objectives aligned to standards
- Designing engaging activities and instructional sequences with strong pacing
- Creating formative checks and summative assessments
- Building in differentiation (enrichment, intervention, accommodations)
- Strengthening transitions, closures, and hooks
- Refining language and clarity in plan documents

## Context Awareness
When the teacher is editing a specific lesson plan, its current content will be provided as "CURRENT LESSON PLAN" below. This is the live editor content — the teacher may have edited it since your last response. Always read it carefully before making changes.

You also have access to the teacher's document history via RAG. When relevant reference documents are found, they will be provided as additional context. Use this to:
- Reference what has worked before
- Suggest improvements based on patterns in their teaching style
- Help maintain consistency across their curriculum

## Shared Workspace Model

You and the teacher share a workspace: the **editor** is where the lesson plan lives, and the **chat** is where you discuss it.

### Writing to the Editor
When you create, modify, or update lesson plan content, write it directly to the editor — do NOT paste full lesson plans into the chat. To do this, wrap the HTML content in special markers:

<<<EDITOR_UPDATE>>>
<h2>Morning Circle</h2>
<p>Students gather on the rug for...</p>
<table><tr><th>Time</th><th>Activity</th></tr>...</table>
<<<END_EDITOR_UPDATE>>>

- Content inside the markers MUST be valid HTML (the editor uses TipTap — supports `<h1>`–`<h3>`, `<p>`, `<strong>`, `<em>`, `<u>`, `<ul>`/`<ol>`/`<li>`, `<table>`/`<tr>`/`<th>`/`<td>`, `<blockquote>`, `style="background-color: COLOR"` on `<td>`/`<th>` for cell background colors, `<mark data-color="COLOR">` for inline text highlighting, `<span style="color: COLOR">` for text color)
- For color-coded cells, use `style="background-color: COLOR"` directly on `<td>` or `<th>` elements (e.g. `<td style="background-color: #FFFF00">content</td>`). Use `<mark data-color="COLOR">text</mark>` only for inline text highlights within a cell, not for the cell's background color
- The editor update REPLACES the entire editor content, so include the full plan — not just the changed section
- CRITICAL: Always base your editor update on the CURRENT LESSON PLAN content provided below. The teacher may have manually edited the plan since your last response. Never overwrite their changes — merge your updates with their current content.

### Chat Messages
Outside the markers, write a brief summary for the chat — what you changed and why. Keep this conversational: 1–3 sentences, no full lesson content.

Example response:
```
I've added a 15-minute science warm-up and reorganized the afternoon block to fit in the art activity you mentioned. The math section stays as you had it.

<<<EDITOR_UPDATE>>>
<h2>Daily Schedule</h2>
<table>...</table>
<<<END_EDITOR_UPDATE>>>
```

### When NOT to Write to the Editor
- Answering questions ("What's a good exit ticket?") → just reply in chat
- Giving feedback on the plan → discuss in chat, suggest changes
- When the teacher hasn't asked you to create or modify content → chat only

### Chat Formatting
In the chat portion (outside markers), use markdown:
- **Bold**, *italics*, bullet lists, numbered lists
- Keep it brief — the editor is where the real content lives"#;

/// Fetch the active teaching template JSON, if one exists.
/// Returns `None` silently if no template is stored — the AI still works, just
/// without template-aware formatting.
fn get_template_json(db: &Database) -> Option<String> {
    match db.get_active_teaching_template() {
        Ok(template) => Some(template.template_json),
        Err(_) => None,
    }
}

/// Convert a teaching template JSON string into structured prompt instructions.
///
/// Instead of dumping raw JSON for the AI to interpret, this function parses
/// the template schema and produces clear, actionable instructions the AI can
/// follow to generate HTML that matches the teacher's actual plan structure.
fn format_template_instructions(template_json: &str) -> String {
    let schema: TeachingTemplateSchema = match serde_json::from_str(template_json) {
        Ok(s) => s,
        Err(_) => return format!(
            "The teacher has a lesson plan template. Use this structure:\n```json\n{template_json}\n```"
        ),
    };

    let mut instructions = String::new();

    instructions.push_str("\n\n## Teacher's Lesson Plan Template — FOLLOW THIS EXACTLY\n\n");
    instructions.push_str(
        "When generating or editing lesson plan content for the editor (inside `<<<EDITOR_UPDATE>>>` markers), \
         you MUST produce an HTML table that matches this teacher's specific schedule structure. \
         Do NOT generate a flat list or generic table — replicate their exact format.\n\n"
    );

    // ── Table Structure ──
    let ts = &schema.table_structure;
    if !ts.columns.is_empty() {
        instructions.push_str("### Table Layout\n");
        if ts.layout_type == "schedule_grid" {
            instructions.push_str(
                "This is a **weekly schedule grid**: days of the week as columns, time slots as rows.\n\n"
            );
        }
        // Include semantic labels when available for clearer AI guidance.
        if let Some(ref col_sem) = ts.column_semantic {
            instructions.push_str(&format!("**Column meaning:** {col_sem}\n"));
        }
        if let Some(ref row_sem) = ts.row_semantic {
            instructions.push_str(&format!("**Row meaning:** {row_sem}\n\n"));
        }
        instructions.push_str(&format!(
            "**Columns ({}):** {}\n\n",
            ts.column_count,
            ts.columns.join(" | ")
        ));
        if !ts.row_categories.is_empty() {
            instructions.push_str(&format!(
                "**Row categories (left column):** {}\n\n",
                ts.row_categories.join(", ")
            ));
        }
    }

    // ── Time Slots ──
    if !schema.time_slots.is_empty() {
        instructions.push_str("### Time Blocks\n");
        instructions.push_str(
            "Each row in the table corresponds to a time block. Use these EXACT time slots as the first column:\n\n"
        );
        for slot in &schema.time_slots {
            instructions.push_str(&format!("- {slot}\n"));
        }
        instructions.push('\n');
    }

    // ── Recurring Elements ──
    let re = &schema.recurring_elements;
    if !re.activities.is_empty() || !re.subjects.is_empty() {
        instructions.push_str("### Recurring Daily Events\n");
        instructions.push_str(
            "These activities appear every day (or most days) at the same time. \
             Keep them in place — do NOT replace them with new content. \
             Only fill the lesson-specific slots with new activities.\n\n"
        );
        if !re.activities.is_empty() {
            instructions.push_str(&format!(
                "**Daily recurring activities:** {}\n\n",
                re.activities.join(", ")
            ));
        }
        if !re.subjects.is_empty() {
            instructions.push_str(&format!(
                "**Regular subjects:** {}\n\n",
                re.subjects.join(", ")
            ));
        }
    }

    // ── Daily Routine Events ──
    if !schema.daily_routine.is_empty() {
        instructions.push_str("### Daily Routine Events\n");
        instructions.push_str(
            "The following recurring events are part of this teacher's daily routine. \
             When generating a **weekly or daily plan**, ALWAYS include these recurring events \
             at their designated day/time slots. Do not skip them or replace them with lesson content. \
             Fill the REMAINING time slots with lesson content.\n\n"
        );
        for event in &schema.daily_routine {
            let days_str = if event.days.is_empty() {
                String::new()
            } else {
                format!(" ({})", event.days.join(", "))
            };
            if let Some(ref ts) = event.time_slot {
                instructions.push_str(&format!("- **{}** at {}{}\n", event.name, ts, days_str));
            } else {
                instructions.push_str(&format!("- **{}**{}\n", event.name, days_str));
            }
        }
        instructions.push_str(
            "\n**IMPORTANT:** When generating a full weekly plan or full daily schedule, \
             ALWAYS include every recurring event listed above at its designated time slot. \
             For a **single-day** request, include only the recurring events that happen on that \
             specific day. If the teacher asks for a **single lesson**, a **specific topic**, \
             or a **lesson plan on X**, focus entirely on that academic content — do NOT insert \
             routine events into a single-lesson response.\n\n"
        );
    }

    // ── Color Scheme ──
    let cs = &schema.color_scheme;
    if !cs.mappings.is_empty() {
        instructions.push_str("### Color Coding (TipTap-compatible)\n");
        instructions.push_str(
            "Apply cell background colors directly on `<td>` and `<th>` elements using \
             `style=\"background-color: COLOR\"`. For example: `<td style=\"background-color: #FFD700\">content</td>`. \
             Do NOT use `<mark>` tags for cell backgrounds — those render as inline text highlights, not cell fills. \
             For text colors, use `<span style=\"color: COLOR\">text</span>`. \
             This is how the teacher visually organizes their schedule.\n\n"
        );
        for mapping in &cs.mappings {
            instructions.push_str(&format!(
                "- `{}` → {} cells → use `style=\"background-color: {}\"`\n",
                mapping.color, mapping.category, mapping.color
            ));
        }
        instructions.push('\n');
    }

    // ── Content Patterns ──
    let cp = &schema.content_patterns;
    instructions.push_str("### Content Detail Level\n");
    instructions.push_str(
        "Each cell should contain **specific, detailed content** — not just a subject name. Include:\n\
         - Specific activity names (e.g., \"Counting with Small Pumpkins\" not just \"Math\")\n\
         - Book titles, song names, game names when relevant\n\
         - Brief description of what students will do\n\n"
    );
    if cp.has_rich_formatting {
        instructions.push_str(
            "Use **rich formatting** inside cells: `<strong>` for labels/headers, \
             `<em>` for emphasis. The teacher's plans use formatted text.\n\n"
        );
    }
    if cp.has_links {
        instructions.push_str(
            "Include `<a href=\"...\">` links to resources where appropriate — \
             the teacher's plans reference external documents.\n\n"
        );
    }

    // ── HTML Skeleton Example ──
    if ts.layout_type == "schedule_grid" && !schema.time_slots.is_empty() && !ts.columns.is_empty() {
        instructions.push_str("### HTML Output Format\n");
        instructions.push_str(
            "Generate a complete `<table>` with this structure. Here is the skeleton — \
             fill every cell with specific lesson content:\n\n```html\n<table>\n  <tr>\n"
        );
        for col in &ts.columns {
            // Apply header color via background-color style
            let header_style = cs.mappings.iter()
                .find(|m| m.category == "header")
                .map(|m| format!(" style=\"background-color: {}\"", m.color))
                .unwrap_or_default();
            instructions.push_str(&format!("    <th{header_style}>{col}</th>\n"));
        }
        instructions.push_str("  </tr>\n");

        // Show first 2-3 time slot rows as example
        let activity_style = cs.mappings.iter()
            .find(|m| m.category == "activity")
            .map(|m| format!(" style=\"background-color: {}\"", m.color))
            .unwrap_or_default();
        let example_slots: Vec<&String> = schema.time_slots.iter().take(3).collect();
        for slot in &example_slots {
            instructions.push_str("  <tr>\n");
            instructions.push_str(&format!("    <td>{slot}</td>\n"));
            for _ in 1..ts.columns.len() {
                instructions.push_str(&format!(
                    "    <td{activity_style}>\
                     <strong>Activity Name</strong><br/>Specific details...\
                     </td>\n"
                ));
            }
            instructions.push_str("  </tr>\n");
        }
        if schema.time_slots.len() > 3 {
            instructions.push_str("  <!-- ... continue for all time slots ... -->\n");
        }
        instructions.push_str("</table>\n```\n\n");
    }

    instructions.push_str(
        "**CRITICAL:** Generate ALL time slot rows, ALL day columns, and fill EVERY cell. \
         An empty cell is better than a missing row. The output must be a complete weekly schedule, \
         not a partial plan.\n"
    );

    instructions
}

/// Create an AI provider from the current app settings.
fn create_provider_from_settings(
    api_key: &str,
    base_url: &str,
    model: &str,
    provider_type: &str,
) -> Result<Box<dyn provider::AiProvider>, ChalkError> {
    let config = AiProviderConfig {
        provider_type: provider_type.to_string(),
        api_key: api_key.to_string(),
        base_url: base_url.to_string(),
        model: model.to_string(),
    };
    AiProviderFactory::create(&config)
}

/// Build the messages array for a chat completion request.
fn build_messages(
    history: &[ChatMessage],
    user_message: &str,
    rag_context: &str,
    active_plan: Option<(&str, &str)>, // (title, content)
    teaching_template: Option<&str>,   // serialized template JSON
) -> Vec<CompletionMessage> {
    let mut messages = Vec::new();

    let mut system_content = SYSTEM_PROMPT.to_string();

    // Inject teaching template if available — tells the AI about the teacher's
    // preferred table structure, color scheme, time slots, and recurring elements.
    if let Some(template_json) = teaching_template {
        system_content.push_str(&format_template_instructions(template_json));
    }

    // Inject active plan context if available.
    if let Some((title, content)) = active_plan {
        if !content.is_empty() {
            // Truncate very long plan content to stay within token budget.
            let truncated = if content.len() > 12000 {
                format!("{}...(truncated)", &content[..12000])
            } else {
                content.to_string()
            };
            system_content.push_str(&format!(
                "\n\n--- CURRENT LESSON PLAN ---\nTitle: {title}\n\n{truncated}\n--- END LESSON PLAN ---"
            ));
        } else {
            system_content.push_str(&format!(
                "\n\nThe teacher is working on a new lesson plan titled \"{title}\" that is currently empty. Help them get started."
            ));
        }
    }

    if !rag_context.is_empty() {
        system_content.push_str(&format!("\n\n--- TEACHING HISTORY ---\n{rag_context}\n--- END HISTORY ---"));
    }

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

/// Generate a non-streaming response using the provider abstraction.
async fn generate_response(
    provider: &dyn provider::AiProvider,
    history: &[ChatMessage],
    user_message: &str,
    rag_context: &str,
    active_plan: Option<(&str, &str)>,
    teaching_template: Option<&str>,
) -> Result<String, ChalkError> {
    let messages = build_messages(history, user_message, rag_context, active_plan, teaching_template);
    provider.complete(&messages, 4096, 0.7).await
}

/// Stream a response using the provider abstraction, emitting tokens via Tauri events.
async fn generate_response_stream(
    app: &tauri::AppHandle,
    provider: &dyn provider::AiProvider,
    history: &[ChatMessage],
    user_message: &str,
    rag_context: &str,
    conversation_id: &str,
    active_plan: Option<(&str, &str)>,
    teaching_template: Option<&str>,
) -> Result<String, ChalkError> {
    let messages = build_messages(history, user_message, rag_context, active_plan, teaching_template);
    let conv_id = conversation_id.to_string();
    let app_handle = app.clone();

    provider
        .complete_stream(&messages, 4096, 0.7, Box::new(move |token| {
            events::emit_chat_stream_token(
                &app_handle,
                events::ChatStreamTokenPayload {
                    conversation_id: conv_id.clone(),
                    token: token.to_string(),
                },
            );
        }))
        .await
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
    let embedding_client = EmbeddingClient::with_base_url(api_key.clone(), base_url.clone());
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

    // Build active plan context from input fields.
    let active_plan = match (input.plan_title.as_deref(), input.plan_content.as_deref()) {
        (Some(title), Some(content)) => Some((title, content)),
        (Some(title), None) => Some((title, "")),
        _ => None,
    };

    // Fetch teaching template for prompt injection.
    let template_json = get_template_json(db);

    // Create provider and generate AI response.
    let ai_provider = create_provider_from_settings(&api_key, &base_url, &model, "openai")
        .map_err(|e| e.message)?;
    let ai_response =
        generate_response(
            ai_provider.as_ref(),
            &history,
            &input.message,
            &rag_context,
            active_plan,
            template_json.as_deref(),
        )
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
    let embedding_client = EmbeddingClient::with_base_url(api_key.clone(), base_url.clone());
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

    // Fetch teaching template for prompt injection.
    let template_json = get_template_json(db);

    // Create provider before spawning so we get early config errors.
    let ai_provider = create_provider_from_settings(&api_key, &base_url, &model, "openai")
        .map_err(|e| e.message)?;

    // Spawn the streaming generation in the background so we return immediately.
    let conv_id = conversation_id.clone();
    let msg = input.message.clone();
    let ctx_ids = context_ids_json.clone();
    let app_clone = app.clone();
    let plan_title_owned = input.plan_title.clone();
    let plan_content_owned = input.plan_content.clone();

    tauri::async_runtime::spawn(async move {
        let active_plan = match (plan_title_owned.as_deref(), plan_content_owned.as_deref()) {
            (Some(title), Some(content)) => Some((title, content)),
            (Some(title), None) => Some((title, "")),
            _ => None,
        };
        let state = app_clone.state::<AppState>();
        match generate_response_stream(
            &app_clone,
            ai_provider.as_ref(),
            &history,
            &msg,
            &rag_context,
            &conv_id,
            active_plan,
            template_json.as_deref(),
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

    let base_url = db
        .get_setting("openai_base_url")
        .map_err(|e| format!("{e}"))?
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

    let plan = db.get_lesson_plan(&plan_id).map_err(|e| format!("{e}"))?;

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

    db.upsert_embedding(&plan_id, &embedding)
        .map_err(|e| format!("{e}"))?;

    tracing::info!(plan_id = %plan_id, "Plan vectorized successfully");
    Ok(())
}

/// Vectorize all existing reference docs that don't have embeddings yet.
#[tauri::command]
pub async fn vectorize_all_plans(state: tauri::State<'_, AppState>) -> Result<u32, String> {
    let db = &state.db;

    let api_key = db
        .get_setting("openai_api_key")
        .map_err(|e| format!("{e}"))?
        .ok_or_else(|| "OpenAI API key not configured".to_string())?;

    let base_url = db
        .get_setting("openai_base_url")
        .map_err(|e| format!("{e}"))?
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

    // Get all reference docs that don't have embeddings.
    let docs_without_embeddings = db
        .list_ref_docs_without_embeddings()
        .map_err(|e| format!("{e}"))?;

    if docs_without_embeddings.is_empty() {
        return Ok(0);
    }

    let client = EmbeddingClient::with_base_url(api_key, base_url);
    let mut count = 0u32;

    for doc in &docs_without_embeddings {
        let embedding_text = crate::rag::chunker::create_embedding_text(
            &doc.title,
            &doc.content_text,
            None,
        );

        match client.embed_one(&embedding_text).await {
            Ok(embedding) => {
                if let Err(e) = db.upsert_ref_doc_embedding(&doc.id, &embedding) {
                    tracing::warn!(doc_id = %doc.id, error = %e, "Failed to store ref doc embedding");
                } else {
                    count += 1;
                }
            }
            Err(e) => {
                tracing::warn!(doc_id = %doc.id, error = %e.message, "Failed to generate ref doc embedding");
            }
        }
    }

    tracing::info!(count, "Vectorized reference docs");
    Ok(count)
}

/// Save AI configuration settings.
/// When an API key is saved, automatically vectorizes any lesson plans that
/// don't have embeddings yet — this covers the case where plans were imported
/// before the key was configured.
#[tauri::command]
pub async fn save_ai_config(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
) -> Result<(), String> {
    let db = &state.db;

    let key_was_set = api_key.is_some();

    if let Some(ref key) = api_key {
        db.set_setting("openai_api_key", key)
            .map_err(|e| format!("{e}"))?;
    }
    if let Some(ref url) = base_url {
        db.set_setting("openai_base_url", url)
            .map_err(|e| format!("{e}"))?;
    }
    if let Some(ref m) = model {
        db.set_setting("chat_model", m)
            .map_err(|e| format!("{e}"))?;
    }

    // When an API key is saved, kick off background vectorization for any
    // reference docs that were imported without embeddings (e.g. before the key was set).
    if key_was_set {
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let state = app_handle.state::<AppState>();
            let db = &state.db;

            let api_key = match db.get_setting("openai_api_key") {
                Ok(Some(k)) if !k.is_empty() => k,
                _ => return,
            };
            let base_url = db
                .get_setting("openai_base_url")
                .unwrap_or(None)
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

            let docs = match db.list_ref_docs_without_embeddings() {
                Ok(d) if !d.is_empty() => d,
                _ => return,
            };

            tracing::info!(count = docs.len(), "Auto-vectorizing reference docs after API key saved");

            let client = EmbeddingClient::with_base_url(api_key, base_url);
            let mut vectorized = 0u32;

            for doc in &docs {
                let text = crate::rag::chunker::create_embedding_text(
                    &doc.title,
                    &doc.content_text,
                    None,
                );
                match client.embed_one(&text).await {
                    Ok(embedding) => {
                        if db.upsert_ref_doc_embedding(&doc.id, &embedding).is_ok() {
                            vectorized += 1;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(doc_id = %doc.id, error = %e, "Auto-vectorize failed for reference doc");
                    }
                }
            }

            tracing::info!(vectorized, total = docs.len(), "Auto-vectorization complete");
        });
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
        let messages = build_messages(&history, "Hello", "", None, None);

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
        let messages = build_messages(&history, "Help me", rag_ctx, None, None);

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
        let messages = build_messages(&history, "New question", "", None, None);

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

        let messages = build_messages(&history, "Final", "", None, None);
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
        let messages = build_messages(&history, "Hi", "", None, None);

        // system + 1 user from history (system skipped) + user = 3
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1].content, "Included");
    }

    #[test]
    fn test_build_messages_with_active_plan() {
        let history: Vec<ChatMessage> = vec![];
        let plan = Some(("Photosynthesis Lab", "Students will learn about..."));
        let messages = build_messages(&history, "Help me improve this", "", plan, None);

        assert_eq!(messages.len(), 2);
        assert!(messages[0].content.contains("CURRENT LESSON PLAN"));
        assert!(messages[0].content.contains("Photosynthesis Lab"));
        assert!(messages[0].content.contains("Students will learn about"));
    }

    #[test]
    fn test_build_messages_with_empty_plan() {
        let history: Vec<ChatMessage> = vec![];
        let plan = Some(("New Plan", ""));
        let messages = build_messages(&history, "Help me", "", plan, None);

        assert_eq!(messages.len(), 2);
        assert!(messages[0].content.contains("currently empty"));
        assert!(messages[0].content.contains("New Plan"));
    }

    #[test]
    fn test_build_messages_with_plan_and_rag() {
        let history: Vec<ChatMessage> = vec![];
        let plan = Some(("My Plan", "Some content here"));
        let messages = build_messages(&history, "Help", "Related: old plan data", plan, None);

        assert_eq!(messages.len(), 2);
        assert!(messages[0].content.contains("CURRENT LESSON PLAN"));
        assert!(messages[0].content.contains("TEACHING HISTORY"));
        assert!(messages[0].content.contains("Related: old plan data"));
    }

    #[test]
    fn test_build_messages_with_teaching_template() {
        let history: Vec<ChatMessage> = vec![];
        let template = r##"{"color_scheme":{"mappings":[{"color":"#FFD700","category":"Math","frequency":5}]},"table_structure":{"layout_type":"schedule_grid","columns":["Time","Monday","Tuesday"],"row_categories":["Morning","Afternoon"],"column_count":3},"time_slots":["8:00-9:00","9:00-10:00"],"content_patterns":{"cell_content_types":["activity"],"has_links":false,"has_rich_formatting":true},"recurring_elements":{"subjects":["Math","Reading"],"activities":["Circle Time","Centers"]}}"##;
        let messages = build_messages(&history, "Make a plan", "", None, Some(template));

        assert_eq!(messages.len(), 2);
        assert!(messages[0].content.contains("Teacher's Lesson Plan Template"));
        // Structured instructions instead of raw JSON.
        assert!(messages[0].content.contains("Time | Monday | Tuesday"));
        assert!(messages[0].content.contains("8:00-9:00"));
        assert!(messages[0].content.contains("9:00-10:00"));
        assert!(messages[0].content.contains("Circle Time"));
        assert!(messages[0].content.contains("Centers"));
        assert!(messages[0].content.contains("#FFD700"));
        assert!(messages[0].content.contains("rich formatting"));
        // Should have HTML skeleton example for schedule grids.
        assert!(messages[0].content.contains("<table>"));
        assert!(messages[0].content.contains("<th"));
    }

    #[test]
    fn test_build_messages_with_template_and_plan() {
        let history: Vec<ChatMessage> = vec![];
        let template = r#"{"color_scheme":{"mappings":[]},"table_structure":{"layout_type":"schedule_grid","columns":["Time","Mon"],"row_categories":[],"column_count":2},"time_slots":[],"content_patterns":{"cell_content_types":[],"has_links":false,"has_rich_formatting":false},"recurring_elements":{"subjects":[],"activities":[]}}"#;
        let plan = Some(("My Plan", "Some content"));
        let messages = build_messages(&history, "Help", "", plan, Some(template));

        assert_eq!(messages.len(), 2);
        // Template section appears before plan section.
        assert!(messages[0].content.contains("Teacher's Lesson Plan Template"));
        assert!(messages[0].content.contains("CURRENT LESSON PLAN"));
    }

    // NOTE: Stream chunk and request serialization tests moved to openai.rs

    // ── format_template_instructions tests ──────────────────────

    #[test]
    fn test_format_template_instructions_schedule_grid() {
        let template = r##"{"color_scheme":{"mappings":[{"color":"#9900ff","category":"header","frequency":6},{"color":"#00ffff","category":"activity","frequency":3}]},"table_structure":{"layout_type":"schedule_grid","columns":["Day/Time","Monday","Tuesday","Wednesday","Thursday","Friday"],"row_categories":["Morning Circle","Centers"],"column_count":6},"time_slots":["8:15-9:00","9:00-9:10","9:10-9:30","9:30-10:00"],"content_patterns":{"cell_content_types":["activity"],"has_links":true,"has_rich_formatting":true},"recurring_elements":{"subjects":["Math","Reading"],"activities":["Soft Start Breakfast","Morning Circle","Recess"]}}"##;

        let result = format_template_instructions(template);

        // Structure markers.
        assert!(result.contains("weekly schedule grid"));
        assert!(result.contains("Day/Time | Monday | Tuesday | Wednesday | Thursday | Friday"));
        assert!(result.contains("6"), "Should mention column count");

        // Time slots listed.
        assert!(result.contains("8:15-9:00"));
        assert!(result.contains("9:00-9:10"));
        assert!(result.contains("9:10-9:30"));
        assert!(result.contains("9:30-10:00"));

        // Recurring elements.
        assert!(result.contains("Soft Start Breakfast"));
        assert!(result.contains("Morning Circle"));
        assert!(result.contains("Recess"));
        assert!(result.contains("do NOT replace them"));

        // Color coding — should use background-color on cells, not mark tags.
        assert!(result.contains("#9900ff"));
        assert!(result.contains("#00ffff"));
        assert!(result.contains("background-color"));

        // Content detail instructions.
        assert!(result.contains("specific, detailed content"));
        assert!(result.contains("rich formatting"));
        assert!(result.contains("links"));

        // HTML skeleton.
        assert!(result.contains("<table>"));
        assert!(result.contains("<th"));
        assert!(result.contains("8:15-9:00"));

        // Row categories.
        assert!(result.contains("Morning Circle"));
        assert!(result.contains("Centers"));
    }

    #[test]
    fn test_format_template_instructions_standard_table() {
        let template = r#"{"color_scheme":{"mappings":[]},"table_structure":{"layout_type":"standard_table","columns":["Title","Subject","Duration","Objectives"],"row_categories":[],"column_count":4},"time_slots":[],"content_patterns":{"cell_content_types":["activity_name","duration","objectives"],"has_links":false,"has_rich_formatting":false},"recurring_elements":{"subjects":[],"activities":[]}}"#;

        let result = format_template_instructions(template);

        // Should NOT say "weekly schedule grid" for standard tables.
        assert!(!result.contains("weekly schedule grid"));
        // Should list columns.
        assert!(result.contains("Title | Subject | Duration | Objectives"));
        // No time slots section.
        assert!(!result.contains("Time Blocks"));
        // No recurring elements section.
        assert!(!result.contains("Recurring Daily Events"));
        // No HTML skeleton for non-schedule tables.
        assert!(!result.contains("<table>") || !result.contains("skeleton"));
    }

    #[test]
    fn test_format_template_instructions_empty_template() {
        let template = "{}";
        let result = format_template_instructions(template);

        // Should still produce the base instructions.
        assert!(result.contains("FOLLOW THIS EXACTLY"));
        assert!(result.contains("specific, detailed content"));
        // No time slots or colors sections.
        assert!(!result.contains("Time Blocks"));
        assert!(!result.contains("Color Coding"));
    }

    #[test]
    fn test_format_template_instructions_invalid_json() {
        let template = "not valid json at all";
        let result = format_template_instructions(template);

        // Should fall back to raw JSON dump.
        assert!(result.contains("not valid json at all"));
    }

    #[test]
    fn test_format_template_instructions_colors_only() {
        let template = r##"{"color_scheme":{"mappings":[{"color":"#ff0000","category":"header","frequency":10},{"color":"#00ff00","category":"highlight","frequency":20}]},"table_structure":{"layout_type":"","columns":[],"row_categories":[],"column_count":0},"time_slots":[],"content_patterns":{"cell_content_types":[],"has_links":false,"has_rich_formatting":false},"recurring_elements":{"subjects":[],"activities":[]}}"##;

        let result = format_template_instructions(template);

        assert!(result.contains("#ff0000"));
        assert!(result.contains("#00ff00"));
        assert!(result.contains("header cells"));
        assert!(result.contains("highlight cells"));
    }

    #[test]
    fn test_format_template_instructions_many_time_slots_skeleton_truncated() {
        // If there are many time slots, the skeleton only shows first 3.
        let template = r#"{"color_scheme":{"mappings":[]},"table_structure":{"layout_type":"schedule_grid","columns":["Time","Monday","Tuesday"],"row_categories":[],"column_count":3},"time_slots":["8:00-8:30","8:30-9:00","9:00-9:30","9:30-10:00","10:00-10:30"],"content_patterns":{"cell_content_types":[],"has_links":false,"has_rich_formatting":false},"recurring_elements":{"subjects":[],"activities":[]}}"#;

        let result = format_template_instructions(template);

        // All time slots listed in the Time Blocks section.
        assert!(result.contains("8:00-8:30"));
        assert!(result.contains("10:00-10:30"));
        // HTML skeleton shows first 3 + ellipsis comment.
        assert!(result.contains("continue for all time slots"));
    }

    #[test]
    fn test_format_template_instructions_daily_routine() {
        let template = r##"{"color_scheme":{"mappings":[]},"table_structure":{"layout_type":"schedule_grid","columns":["Time","Monday","Tuesday","Wednesday","Thursday","Friday"],"row_categories":[],"column_count":6},"time_slots":["8:00-8:45","9:00-9:15","11:30-12:00","2:30-2:45"],"content_patterns":{"cell_content_types":[],"has_links":false,"has_rich_formatting":false},"recurring_elements":{"subjects":[],"activities":[]},"daily_routine":[{"name":"Breakfast","time_slot":"7:45-8:00"},{"name":"Recess","time_slot":"9:00-9:15"},{"name":"Lunch","time_slot":"11:30-12:00"},{"name":"Dismissal","time_slot":"2:30-2:45"}]}"##;

        let result = format_template_instructions(template);

        // Should have the daily routine section.
        assert!(result.contains("Daily Routine Events"), "Missing Daily Routine Events header");
        assert!(result.contains("Breakfast"), "Missing Breakfast");
        assert!(result.contains("7:45-8:00"), "Missing Breakfast time");
        assert!(result.contains("Recess"), "Missing Recess");
        assert!(result.contains("Lunch"), "Missing Lunch");
        assert!(result.contains("Dismissal"), "Missing Dismissal");

        // Should contain single-lesson exclusion instruction.
        assert!(result.contains("single lesson"), "Missing single-lesson exclusion guidance");
    }

    #[test]
    fn test_format_template_instructions_no_daily_routine() {
        let template = r#"{"color_scheme":{"mappings":[]},"table_structure":{"layout_type":"schedule_grid","columns":["Time","Monday"],"row_categories":[],"column_count":2},"time_slots":["9:00"],"content_patterns":{"cell_content_types":[],"has_links":false,"has_rich_formatting":false},"recurring_elements":{"subjects":[],"activities":[]},"daily_routine":[]}"#;

        let result = format_template_instructions(template);

        // Should NOT have the daily routine section when empty.
        assert!(!result.contains("Daily Routine Events"));
    }

    #[test]
    fn test_format_template_instructions_daily_routine_without_time() {
        let template = r##"{"color_scheme":{"mappings":[]},"table_structure":{"layout_type":"","columns":[],"row_categories":[],"column_count":0},"time_slots":[],"content_patterns":{"cell_content_types":[],"has_links":false,"has_rich_formatting":false},"recurring_elements":{"subjects":[],"activities":[]},"daily_routine":[{"name":"Assembly","time_slot":null}]}"##;

        let result = format_template_instructions(template);

        assert!(result.contains("Daily Routine Events"));
        assert!(result.contains("Assembly"));
        // Without a time slot, should just list the name without "at".
        assert!(!result.contains("at null"));
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
