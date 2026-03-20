//! AI Chat module — manages conversations, message persistence, and
//! context-aware generation via RAG.

pub mod openai;
pub mod provider;

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::database::{
    CalendarException, Database, EventOccurrence, RecurringEvent, TeachingTemplateSchema,
};
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

// ── Schedule Intelligence ────────────────────────────────────

/// All the schedule data needed to build schedule-aware prompts.
struct ScheduleContext {
    /// Each recurring event paired with its time occurrences.
    events: Vec<(RecurringEvent, Vec<EventOccurrence>)>,
    /// Calendar exceptions (holidays, half days) for the target week.
    exceptions: Vec<CalendarException>,
}

/// Day-of-week index (0 = Monday … 4 = Friday) to name.
fn day_name(dow: i32) -> &'static str {
    match dow {
        0 => "MONDAY",
        1 => "TUESDAY",
        2 => "WEDNESDAY",
        3 => "THURSDAY",
        4 => "FRIDAY",
        _ => "UNKNOWN",
    }
}

/// Fetch schedule context for the week containing `date_str` (YYYY-MM-DD).
fn get_schedule_context(db: &Database, date_str: &str) -> Option<ScheduleContext> {
    use chrono::Datelike;
    let events = db.list_events_with_occurrences().ok()?;
    if events.is_empty() {
        return None;
    }

    // Compute the Monday–Friday range for the week containing date_str.
    let date = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
    let weekday = date.weekday().num_days_from_monday(); // Mon=0
    let monday = date - chrono::Duration::days(weekday as i64);
    let friday = monday + chrono::Duration::days(4);
    let mon_str = monday.format("%Y-%m-%d").to_string();
    let fri_str = friday.format("%Y-%m-%d").to_string();

    let exceptions = db
        .get_school_calendar()
        .ok()
        .and_then(|cal| {
            db.list_calendar_exceptions_in_range(&cal.id, &mon_str, &fri_str)
                .ok()
        })
        .unwrap_or_default();

    Some(ScheduleContext { events, exceptions })
}

/// Format the schedule context into AI prompt instructions.
///
/// Produces a per-day schedule block that tells the AI exactly which events
/// are fixed, which are teaching slots, and where specials fall.
fn format_schedule_instructions(ctx: &ScheduleContext) -> String {
    use std::collections::HashMap;

    let mut out = String::new();
    out.push_str("\n\n## Weekly Schedule — FOLLOW THIS EXACTLY\n\n");
    out.push_str(
        "The teacher's recurring weekly schedule is listed below, day by day. \
         Events marked [FIXED] should be reproduced as-is. Events marked [TEACHING SLOT] \
         are where you generate lesson content. Events marked [SPECIAL] are day-specific \
         activities (PE, Art, Music, etc.) — reproduce as-is.\n\n",
    );

    // Build a name→RecurringEvent lookup for relationship resolution.
    let event_by_id: HashMap<&str, &RecurringEvent> = ctx
        .events
        .iter()
        .map(|(ev, _)| (ev.id.as_str(), ev))
        .collect();

    // Build per-day list: day_of_week → Vec<(start, end, event)>
    let mut day_events: HashMap<i32, Vec<(&str, &str, &RecurringEvent)>> = HashMap::new();
    for (ev, occs) in &ctx.events {
        for occ in occs {
            day_events
                .entry(occ.day_of_week)
                .or_default()
                .push((&occ.start_time, &occ.end_time, ev));
        }
    }

    // Build exception lookup: day_of_week (0-4) → CalendarException
    use chrono::Datelike;
    let exception_by_dow: HashMap<i32, &CalendarException> = ctx
        .exceptions
        .iter()
        .filter_map(|ex| {
            let d = chrono::NaiveDate::parse_from_str(&ex.date, "%Y-%m-%d").ok()?;
            let dow = d.weekday().num_days_from_monday() as i32;
            Some((dow, ex))
        })
        .collect();

    // Collect events that have details_vary_daily for the reminder.
    let vary_daily_names: Vec<&str> = ctx
        .events
        .iter()
        .filter(|(ev, _)| ev.details_vary_daily)
        .map(|(ev, _)| ev.name.as_str())
        .collect();

    // Collect linked event pairs for relationships section.
    let mut relationships: Vec<(&str, &str)> = Vec::new();
    for (ev, _) in &ctx.events {
        if let Some(ref target_id) = ev.linked_to {
            if let Some(target) = event_by_id.get(target_id.as_str()) {
                relationships.push((&ev.name, &target.name));
            }
        }
    }

    for dow in 0..5 {
        let day = day_name(dow);

        // Check for calendar exceptions.
        if let Some(ex) = exception_by_dow.get(&dow) {
            match ex.exception_type.as_str() {
                "no_school" => {
                    let label = if ex.label.is_empty() {
                        "No School"
                    } else {
                        &ex.label
                    };
                    out.push_str(&format!("{day}:\n- **{label}** — No School\n\n"));
                    continue;
                }
                "half_day" | "early_release" => {
                    let label = if ex.label.is_empty() {
                        &ex.exception_type
                    } else {
                        &ex.label
                    };
                    out.push_str(&format!(
                        "{day}: ⚠️ {label} — truncated schedule\n"
                    ));
                    // Fall through to show events; the AI should truncate at
                    // the early dismissal. We still list the events so the AI
                    // knows what normally happens and can decide what fits.
                }
                _ => {}
            }
        } else {
            out.push_str(&format!("{day}:\n"));
        }

        let mut slots = day_events.get(&dow).cloned().unwrap_or_default();
        // Sort by start_time.
        slots.sort_by(|a, b| a.0.cmp(b.0));

        if slots.is_empty() {
            out.push_str("- (no events scheduled)\n");
        }

        for (start, end, ev) in &slots {
            let tag = match ev.event_type.as_str() {
                "fixed" => "[FIXED - reproduce as-is]",
                "special" => "[SPECIAL - reproduce as-is]",
                "teaching_slot" => "[TEACHING SLOT - generate lesson content]",
                _ => "",
            };

            // Check for linked event relationship.
            let link_note = if let Some(ref target_id) = ev.linked_to {
                if let Some(target) = event_by_id.get(target_id.as_str()) {
                    format!(" → {} [LINKED - intro describes {} activities]", target.name, target.name)
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            out.push_str(&format!(
                "- {start}-{end}: {}{link_note} {tag}\n",
                ev.name
            ));
        }
        out.push('\n');
    }

    // Event relationships section.
    if !relationships.is_empty() {
        out.push_str("### Event Relationships\n");
        for (intro, main) in &relationships {
            out.push_str(&format!(
                "- **{intro}** precedes **{main}** — describe what will happen in the {main} session\n"
            ));
        }
        out.push('\n');
    }

    // Daily variation mandate for events with details_vary_daily.
    if !vary_daily_names.is_empty() {
        out.push_str("### Content That Must Vary Daily\n");
        out.push_str(
            "The following events MUST have **different** content each day of the week:\n\n",
        );
        for name in &vary_daily_names {
            out.push_str(&format!("- **{name}**\n"));
        }
        out.push_str(
            "\nDo NOT repeat the same activities, materials, or focus across multiple days \
             for these events. Each day should feel distinct.\n\n",
        );
    }

    // General variation reminder.
    out.push_str("### Daily Variation — CRITICAL\n\n");
    out.push_str(
        "Each day MUST have **different** lesson content in teaching slots. Only [FIXED] and \
         [SPECIAL] events repeat identically. All [TEACHING SLOT] content must vary day to day.\n\n",
    );

    out.push_str(
        "**Per-Day Specials:** When a special is scheduled on a given day, surrounding time \
         slots shift accordingly. Adapt the schedule per day based on which specials are present.\n\n",
    );

    out
}

// ── LTP Context ─────────────────────────────────────────────

/// Map a month number (1-12) to the month name used in LTP columns.
fn month_number_to_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "January",
    }
}

/// Look up LTP context for a given date string (YYYY-MM-DD).
///
/// Returns None if no LTP documents are imported.
pub fn get_ltp_context(
    db: &Database,
    date_str: &str,
) -> Result<Option<crate::database::LtpContext>, crate::errors::ChalkError> {
    use crate::database::{LtpContext, LtpSubjectContext};
    use crate::errors::{ErrorCode, ErrorDomain};

    // Check if any LTP data exists.
    let has_ltp = db.has_ltp_documents().map_err(|e| ChalkError {
        domain: ErrorDomain::Database,
        code: ErrorCode::DbQueryFailed,
        message: format!("Failed to check LTP documents: {e}"),
        details: None,
    })?;

    if !has_ltp {
        return Ok(None);
    }

    // Parse the date to extract month.
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() < 2 {
        return Err(ChalkError {
            domain: ErrorDomain::Chat,
            code: ErrorCode::InternalError,
            message: format!("Invalid date format: {date_str}. Expected YYYY-MM-DD."),
            details: None,
        });
    }
    let month_num: u32 = parts[1].parse().map_err(|_| ChalkError {
        domain: ErrorDomain::Chat,
        code: ErrorCode::InternalError,
        message: format!("Invalid month in date: {date_str}"),
        details: None,
    })?;

    let month_name = month_number_to_name(month_num);

    // Get LTP cells for this month.
    let cells = db.get_ltp_cells_for_month(month_name).map_err(|e| ChalkError {
        domain: ErrorDomain::Database,
        code: ErrorCode::DbQueryFailed,
        message: format!("Failed to query LTP cells: {e}"),
        details: None,
    })?;

    // Get the unit name for this month.
    let unit_name = db.get_unit_for_month(month_name).map_err(|e| ChalkError {
        domain: ErrorDomain::Database,
        code: ErrorCode::DbQueryFailed,
        message: format!("Failed to query unit: {e}"),
        details: None,
    })?;

    // Group cells by subject, deduplicating by subject name.
    let mut subjects: Vec<LtpSubjectContext> = Vec::new();
    let mut seen_subjects = std::collections::HashSet::new();

    for cell in &cells {
        if let Some(ref subject) = cell.subject {
            if let Some(ref content) = cell.content_text {
                let subject_lower = subject.to_lowercase();
                if !seen_subjects.contains(&subject_lower) && !content.trim().is_empty() {
                    seen_subjects.insert(subject_lower);
                    subjects.push(LtpSubjectContext {
                        subject: subject.clone(),
                        content: content.trim().to_string(),
                    });
                }
            }
        }
    }

    // Get calendar entries for the week surrounding the date (Mon-Sun).
    let mut calendar_notes = Vec::new();
    if let Ok(year) = parts[0].parse::<i32>() {
        if let Ok(day) = parts[2].parse::<u32>() {
            // Compute the Monday of the target week and the following Sunday.
            if let Some(target_date) = simple_date(year, month_num, day) {
                let weekday = day_of_week(year, month_num, day); // 0=Mon, 6=Sun
                let monday = add_days(target_date, -(weekday as i32));
                let sunday = add_days(target_date, 6 - weekday as i32);

                let start = format_date(monday);
                let end = format_date(sunday);

                if let Ok(entries) = db.get_calendar_entries_for_range(&start, &end) {
                    for entry in entries {
                        if entry.is_holiday {
                            if let Some(ref name) = entry.holiday_name {
                                let date_label = entry.date.as_deref().unwrap_or("unknown");
                                calendar_notes.push(format!("{date_label}: {name}"));
                            }
                        } else if let Some(ref notes) = entry.notes {
                            if !notes.is_empty() {
                                let date_label = entry.date.as_deref().unwrap_or("unknown");
                                calendar_notes.push(format!("{date_label}: {notes}"));
                            }
                        }
                    }
                }
            }
        }
    }

    // Build per-day details from LTP cells that have day-specific content.
    // LTP cells with different content per subject+month represent daily activities.
    let daily_details = extract_daily_details(&cells);

    // Detect event relationships from the daily routine patterns.
    // E.g., "New Center Intro" followed by "Centers: X Minutes" are paired.
    let event_relationships = detect_event_relationships(&cells);

    Ok(Some(LtpContext {
        month: month_name.to_string(),
        unit_name,
        subjects,
        calendar_notes,
        daily_details,
        event_relationships,
    }))
}

/// Extract per-day activity details from LTP grid cells.
/// Groups cells by column index (which often corresponds to days or sub-periods)
/// and extracts activity-level content for each.
fn extract_daily_details(cells: &[crate::database::LtpGridCell]) -> Vec<crate::database::LtpDailyDetail> {
    use crate::database::LtpDailyDetail;
    use std::collections::BTreeMap;

    // Group non-empty content by col_index to capture per-column (per-day/period) variety.
    let mut by_col: BTreeMap<i32, Vec<String>> = BTreeMap::new();
    for cell in cells {
        if let Some(ref text) = cell.content_text {
            let trimmed = text.trim();
            if !trimmed.is_empty() && trimmed.len() > 3 {
                by_col.entry(cell.col_index).or_default().push(trimmed.to_string());
            }
        }
    }

    // Only include if there are multiple columns with distinct content (i.e., daily variety).
    if by_col.len() < 2 {
        return Vec::new();
    }

    // Map column indices to day names if they look like a 5-day week pattern.
    let day_names = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday"];
    let cols: Vec<i32> = by_col.keys().copied().collect();

    cols.into_iter()
        .enumerate()
        .filter_map(|(i, col)| {
            let entries = by_col.get(&col)?;
            if entries.is_empty() {
                return None;
            }
            let day = if i < day_names.len() {
                day_names[i].to_string()
            } else {
                format!("Column {}", col)
            };
            // Deduplicate and limit entries to keep prompt concise.
            let mut unique: Vec<String> = Vec::new();
            for e in entries {
                if !unique.contains(e) && unique.len() < 8 {
                    unique.push(e.clone());
                }
            }
            Some(LtpDailyDetail { day, entries: unique })
        })
        .collect()
}

/// Detect paired event relationships from LTP cell content.
/// Looks for patterns like "New Center Intro" followed by "Centers: X Min"
/// to tell the AI these events are related.
fn detect_event_relationships(cells: &[crate::database::LtpGridCell]) -> Vec<crate::database::EventRelationship> {
    use crate::database::EventRelationship;

    let mut relationships = Vec::new();

    // Scan cell content for common paired event patterns.
    let mut has_center_intro = false;
    let mut has_centers_block = false;

    for cell in cells {
        if let Some(ref text) = cell.content_text {
            let lower = text.to_lowercase();
            if lower.contains("center intro") || lower.contains("new center") {
                has_center_intro = true;
            }
            if lower.contains("centers:") || lower.contains("center time") || lower.contains("centers -") {
                has_centers_block = true;
            }
        }
    }

    if has_center_intro && has_centers_block {
        relationships.push(EventRelationship {
            intro_event: "New Center Intro".to_string(),
            main_event: "Centers (time block)".to_string(),
            description: "The intro event describes what will be in the following centers session. \
                Generate specific, different center activities (materials, themes, small group tasks) for each day."
                .to_string(),
        });
    }

    relationships
}

/// Format LTP context as a prompt section for AI injection.
pub fn format_ltp_context(context: &crate::database::LtpContext) -> String {
    let mut out = String::new();
    out.push_str("\n\n--- LONG-TERM PLAN CONTEXT FOR THIS WEEK ---\n");
    out.push_str(&format!("Month: {}\n", context.month));

    if let Some(ref unit) = context.unit_name {
        out.push_str(&format!("Current Unit: {}\n", unit));
    }

    if !context.subjects.is_empty() {
        out.push_str("\nSubject Guidance:\n");
        for subj in &context.subjects {
            out.push_str(&format!("• {}: {}\n", subj.subject, subj.content));
        }
    }

    // Per-day activity details from LTP cells (centers, small groups, materials).
    if !context.daily_details.is_empty() {
        out.push_str("\n### Per-Day Activity Details from LTP\n");
        out.push_str(
            "Use these specific activities to vary content across the week. \
             Each day should draw from its own LTP entries below:\n\n"
        );
        for detail in &context.daily_details {
            out.push_str(&format!("**{}:**\n", detail.day));
            for entry in &detail.entries {
                out.push_str(&format!("  • {}\n", entry));
            }
        }
        out.push('\n');
    }

    // Event relationship hints (paired events like "New Center Intro" → "Centers: 60 Min").
    if !context.event_relationships.is_empty() {
        out.push_str("### Event Relationships\n");
        out.push_str(
            "These events are paired — the first event introduces or sets up the second. \
             Generate specific, matching content for both:\n\n"
        );
        for rel in &context.event_relationships {
            out.push_str(&format!("• \"{}\" → \"{}\": {}\n", rel.intro_event, rel.main_event, rel.description));
        }
        out.push('\n');
    }

    if !context.calendar_notes.is_empty() {
        out.push_str("\nCalendar Notes:\n");
        for note in &context.calendar_notes {
            out.push_str(&format!("• {}\n", note));
        }
    }

    out.push_str("--- END LONG-TERM PLAN CONTEXT ---");
    out
}

// ── Simple date arithmetic (no external crate) ────────────────

/// Compact date representation as days since epoch (for arithmetic).
fn simple_date(year: i32, month: u32, day: u32) -> Option<i64> {
    if month < 1 || month > 12 || day < 1 || day > 31 {
        return None;
    }
    // Days from year 0 using a simplified calculation.
    let y = if month <= 2 { year - 1 } else { year } as i64;
    let m = if month <= 2 { month + 9 } else { month - 3 } as i64;
    let days = 365 * y + y / 4 - y / 100 + y / 400 + (m * 306 + 5) / 10 + (day as i64 - 1);
    Some(days)
}

fn add_days(days: i64, delta: i32) -> i64 {
    days + delta as i64
}

fn format_date(days: i64) -> String {
    // Reverse the simple_date calculation.
    let y = (10000 * days + 14780) / 3652425;
    let mut doy = days - (365 * y + y / 4 - y / 100 + y / 400);
    if doy < 0 {
        let y2 = y - 1;
        doy = days - (365 * y2 + y2 / 4 - y2 / 100 + y2 / 400);
    }
    let mi = (100 * doy + 52) / 3060;
    let month = if mi < 10 { mi + 3 } else { mi - 9 };
    let year = y + (if month <= 2 { 1 } else { 0 });
    let day = doy - (mi * 306 + 5) / 10 + 1;
    format!("{:04}-{:02}-{:02}", year, month, day)
}

/// Compute day of week: 0=Monday, 6=Sunday (Tomohiko Sakamoto's algorithm).
fn day_of_week(year: i32, month: u32, day: u32) -> u32 {
    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    let w = (y + y / 4 - y / 100 + y / 400 + t[(month - 1) as usize] + day as i32) % 7;
    // Convert from Sunday=0 to Monday=0.
    ((w + 6) % 7) as u32
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
    // Detect whether this is a transposed schedule (days in rows, times in columns).
    let is_transposed = ts.column_semantic.as_deref() == Some("time_slots")
        && ts.row_semantic.as_deref() == Some("days_of_week");
    if !ts.columns.is_empty() {
        instructions.push_str("### Table Layout\n");
        if ts.layout_type == "schedule_grid" {
            if is_transposed {
                instructions.push_str(
                    "This is a **weekly schedule grid** (transposed): time slots/periods as columns, days of the week as rows.\n\n"
                );
            } else {
                instructions.push_str(
                    "This is a **weekly schedule grid**: days of the week as columns, time slots as rows.\n\n"
                );
            }
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
        if is_transposed {
            instructions.push_str(
                "Each column in the table corresponds to a time block. Use these EXACT time slots as column headers:\n\n"
            );
        } else {
            instructions.push_str(
                "Each row in the table corresponds to a time block. Use these EXACT time slots as the first column:\n\n"
            );
        }
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

    // ── Daily Variation Mandate ──
    instructions.push_str("### Daily Variation — CRITICAL\n\n");
    instructions.push_str(
        "Each day of the week MUST have **different** lesson activities. Do NOT copy Monday's \
         content across all days. Only truly recurring events (breakfast, lunch, recess, \
         dismissal, snack) should repeat identically. All other slots — lessons, centers, \
         small groups, morning work, instructional blocks — MUST vary day to day.\n\n"
    );

    // Classify routine events and give the AI specific guidance per type.
    {
        use crate::database::RoutineEventType;

        let fixed: Vec<&str> = schema.daily_routine.iter()
            .filter(|e| e.event_type == RoutineEventType::Fixed)
            .map(|e| e.name.as_str())
            .collect();
        let variable: Vec<&str> = schema.daily_routine.iter()
            .filter(|e| e.event_type == RoutineEventType::Variable)
            .map(|e| e.name.as_str())
            .collect();
        let day_specific: Vec<&str> = schema.daily_routine.iter()
            .filter(|e| e.event_type == RoutineEventType::DaySpecific)
            .map(|e| e.name.as_str())
            .collect();

        if !fixed.is_empty() {
            instructions.push_str(&format!(
                "**Fixed recurring** (same every day): {}\n",
                fixed.join(", ")
            ));
        }
        if !variable.is_empty() {
            instructions.push_str(&format!(
                "**Variable recurring** (same time slot, different content each day): {}\n",
                variable.join(", ")
            ));
        }
        if !day_specific.is_empty() {
            instructions.push_str(&format!(
                "**Day-specific** (only on certain days): {}\n",
                day_specific.join(", ")
            ));
        }
        if !fixed.is_empty() || !variable.is_empty() || !day_specific.is_empty() {
            instructions.push('\n');
        }
    }

    instructions.push_str(
        "**Event Relationships:** When a schedule has an introductory event (e.g., \"New Center Intro\") \
         followed by a block event (e.g., \"Centers: 60 Minutes\"), the intro describes what will happen \
         in the following block. Generate specific center details — materials, activities, small group \
         tasks — that are **different each day** the pair appears.\n\n"
    );

    instructions.push_str(
        "**Per-Day Specials:** Specials like PE, Drama, Music, and Art occur on **specific days only**. \
         When a special is scheduled on a given day, surrounding time slots shift accordingly. Do NOT \
         force a rigid identical time grid across all days — adapt the schedule per day based on which \
         specials are present.\n\n"
    );

    instructions.push_str(
        "**LTP-Informed Variety:** If Long-Term Plan context is provided, use it to pull **different** \
         activities, themes, materials, and focuses for each day of the week while staying within the \
         current unit. Distribute LTP activities across the week rather than repeating the same ones.\n\n"
    );

    // ── Color Scheme ──
    let cs = &schema.color_scheme;
    if !cs.mappings.is_empty() {
        instructions.push_str("### Color Coding (TipTap-compatible)\n");
        instructions.push_str(
            "Apply cell background colors directly on `<td>` and `<th>` elements using \
             `style=\"background-color: COLOR\"`. For example: `<td style=\"background-color: #FFD700\">content</td>`. \
             Do NOT use `<mark>` tags for cell backgrounds — those render as inline text highlights, not cell fills. \
             This is how the teacher visually organizes their schedule.\n\n"
        );
        instructions.push_str(
            "**CRITICAL — TEXT CONTRAST:** EVERY `<td>` or `<th>` that has a `background-color` MUST also \
             include `color: #000000` (black text). This is mandatory — without it, the dark-theme editor \
             renders light/white text on colored backgrounds, making content unreadable. \
             Always write: `style=\"background-color: BG; color: #000000\"`. \
             The ONLY exception is very dark backgrounds (e.g., dark purple #2d1b69, dark navy #1a1a2e) — \
             use `color: #FFFFFF` for those. For ALL pastel/medium colors (yellow, green, blue, pink, \
             orange, light purple, etc.), use `color: #000000`. **NEVER omit the color property** on \
             colored cells.\n\n"
        );
        for mapping in &cs.mappings {
            instructions.push_str(&format!(
                "- `{}` → {} cells → use `style=\"background-color: {}; color: #000000\"`\n",
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
        // Build a lookup: time_slot string → (routine event name, bg_color), so the
        // skeleton can pre-fill recurring events with their correct cell colors.
        let routine_by_slot: std::collections::HashMap<&str, (&str, Option<&str>)> = schema
            .daily_routine
            .iter()
            .filter_map(|ev| ev.time_slot.as_deref().map(|ts| {
                (ts, (ev.name.as_str(), ev.bg_color.as_deref()))
            }))
            .collect();

        instructions.push_str("### HTML Output Format\n");
        instructions.push_str(
            "Generate a complete `<table>` with this structure. Here is the skeleton — \
             fill every cell with specific lesson content that is **UNIQUE per day**. \
             Do NOT repeat the same activity across multiple days. Recurring/routine events \
             are already placed in their correct time slots:\n\n```html\n<table>\n  <tr>\n"
        );

        let header_style = cs.mappings.iter()
            .find(|m| m.category == "header")
            .map(|m| format!(" style=\"background-color: {}; color: #000000\"", m.color))
            .unwrap_or_default();
        if is_transposed {
            // Transposed: first header is "Day", remaining headers are time slots.
            let first_col_label = ts.columns.first().map(|s| s.as_str()).unwrap_or("Day");
            instructions.push_str(&format!("    <th{header_style}>{first_col_label}</th>\n"));
            // Show ALL time slots — the AI must reproduce every one.
            for slot in &schema.time_slots {
                instructions.push_str(&format!("    <th{header_style}>{slot}</th>\n"));
            }
            instructions.push_str("  </tr>\n");

            // Show one example day row with all slots filled.
            instructions.push_str("  <tr>\n");
            instructions.push_str(&format!("    <td{header_style}>Monday</td>\n"));
            for slot in &schema.time_slots {
                if let Some((routine_name, bg)) = routine_by_slot.get(slot.as_str()) {
                    let cell_style = bg
                        .map(|c| format!(" style=\"background-color: {}; color: #000000\"", c))
                        .unwrap_or_default();
                    instructions.push_str(&format!(
                        "    <td{cell_style}>\
                         <strong>{routine_name}</strong>\
                         </td>\n"
                    ));
                } else {
                    instructions.push_str(
                        "    <td>\
                         <strong>Activity Name</strong><br/>Specific details...\
                         </td>\n"
                    );
                }
            }
            instructions.push_str("  </tr>\n");
            instructions.push_str("  <!-- Repeat for Tuesday, Wednesday, Thursday, Friday with DIFFERENT lesson content each day -->\n");
        } else {
            // Standard: columns are day headers, rows are time slots.
            for col in &ts.columns {
                instructions.push_str(&format!("    <th{header_style}>{col}</th>\n"));
            }
            instructions.push_str("  </tr>\n");

            // Show ALL time slot rows — the AI must reproduce every one.
            for slot in &schema.time_slots {
                instructions.push_str("  <tr>\n");
                instructions.push_str(&format!("    <td>{slot}</td>\n"));
                if let Some((routine_name, bg)) = routine_by_slot.get(slot.as_str()) {
                    // This time slot has a recurring event — fill all day columns with it.
                    // Use the event's actual bg_color if known; otherwise no background.
                    let cell_style = bg
                        .map(|c| format!(" style=\"background-color: {}; color: #000000\"", c))
                        .unwrap_or_default();
                    for _ in 1..ts.columns.len() {
                        instructions.push_str(&format!(
                            "    <td{cell_style}>\
                             <strong>{routine_name}</strong>\
                             </td>\n"
                        ));
                    }
                } else {
                    // Non-routine slot: each day column must have DIFFERENT content.
                    for i in 1..ts.columns.len() {
                        let day_hint = ts.columns.get(i).map(|s| s.as_str()).unwrap_or("Day");
                        instructions.push_str(&format!(
                            "    <td>\
                             <strong>[{day_hint} Activity]</strong><br/>Unique details for this day...\
                             </td>\n"
                        ));
                    }
                }
                instructions.push_str("  </tr>\n");
            }
        }
        instructions.push_str("</table>\n```\n\n");
    }

    let slot_count = schema.time_slots.len();
    if is_transposed {
        instructions.push_str(&format!(
            "**CRITICAL — DO NOT SHORTEN THE SCHEDULE:** Your output table MUST contain exactly \
             {slot_count} time slot columns and 5 day rows (Monday–Friday). Do NOT combine, skip, \
             or abbreviate any time slots. Do NOT invent new times — use ONLY the exact times listed \
             above. An empty cell is better than a missing column. The output must be a complete \
             weekly schedule, not a partial plan. Ensure all text is readable — use `color: #000000` \
             (black) on all light-colored cell backgrounds.\n\n"
        ));
    } else {
        instructions.push_str(&format!(
            "**CRITICAL — DO NOT SHORTEN THE SCHEDULE:** Your output table MUST contain exactly \
             {slot_count} time slot rows and all day columns. Do NOT combine, skip, or abbreviate \
             any time slots. Do NOT invent new times — use ONLY the exact times listed above. \
             An empty cell is better than a missing row. The output must be a complete weekly \
             schedule, not a partial plan. Ensure all text is readable — use `color: #000000` \
             (black) on all light-colored cell backgrounds.\n\n"
        ));
    }

    if !schema.daily_routine.is_empty() {
        instructions.push_str(
            "**REMINDER — RECURRING EVENTS ARE MANDATORY:** Every recurring event listed in the \
             Daily Routine Events section above MUST appear in your generated plan at its designated \
             time slot. These are non-negotiable — they represent the teacher's fixed daily schedule \
             (meals, recess, dismissal, etc.). Place them FIRST, then fill remaining slots with \
             lesson content.\n\n"
        );
    }

    // ── Editing Rules ──
    instructions.push_str("### Editing an Existing Schedule\n\n");
    instructions.push_str(
        "When the teacher asks to **modify** an existing schedule (e.g., \"this week we have PE on Monday \
         from 11-11:40\", \"change Tuesday's math to science\", \"add art on Wednesday at 1pm\"), you MUST \
         follow these rules:\n\n\
         1. **REPLACE cell content in-place** — find the target cell(s) by matching the day (column) and \
            time slot (row), then overwrite that cell's inner content. Do NOT insert new `<td>` elements \
            or shift existing cells. Every `<td>` in the original table must remain in the same position.\n\
         2. **Keep the table dimensions fixed** — the output table MUST have the exact same number of rows \
            and columns as the original. Do not add, remove, or reorder any rows or columns.\n\
         3. **Leave unaffected cells unchanged** — if the teacher says to change Monday's 11 AM slot, every \
            other cell in the table (other days in that row, other rows entirely) must keep its original \
            content, colors, and formatting exactly as-is.\n\
         4. **Map to the closest existing slot** — if the requested time range doesn't exactly match a \
            slot label (e.g., \"11-11:40\" when the slot is \"11:00 AM - 11:40 AM\"), find the slot whose \
            time range best overlaps and replace that cell. Never restructure the table or add new time rows.\n\
         5. **Preserve the full table** — always output the COMPLETE table with ALL rows and columns, not \
            just the changed parts. The editor replaces the entire table content, so omitting rows means \
            they disappear.\n\n"
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
    ltp_context: Option<&crate::database::LtpContext>,
    schedule_context: Option<&ScheduleContext>,
) -> Vec<CompletionMessage> {
    let mut messages = Vec::new();

    let mut system_content = SYSTEM_PROMPT.to_string();

    // Inject schedule context if available — tells the AI about the teacher's
    // actual recurring weekly schedule (from the new RecurringEvent data).
    // When present, this supersedes the old template-based recurring events.
    if let Some(sched) = schedule_context {
        system_content.push_str(&format_schedule_instructions(sched));
    }

    // Inject teaching template if available — tells the AI about the teacher's
    // preferred table structure, color scheme, time slots, and recurring elements.
    if let Some(template_json) = teaching_template {
        system_content.push_str(&format_template_instructions(template_json));
    }

    // Inject LTP context if available — tells the AI what the long-term plan
    // says for the current month/unit so it can align lesson content.
    if let Some(ltp_ctx) = ltp_context {
        system_content.push_str(&format_ltp_context(ltp_ctx));
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
    ltp_context: Option<&crate::database::LtpContext>,
    schedule_context: Option<&ScheduleContext>,
) -> Result<String, ChalkError> {
    let messages = build_messages(history, user_message, rag_context, active_plan, teaching_template, ltp_context, schedule_context);
    provider.complete(&messages, 16384, 0.7).await
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
    ltp_context: Option<&crate::database::LtpContext>,
    schedule_context: Option<&ScheduleContext>,
) -> Result<String, ChalkError> {
    let messages = build_messages(history, user_message, rag_context, active_plan, teaching_template, ltp_context, schedule_context);
    let conv_id = conversation_id.to_string();
    let app_handle = app.clone();

    provider
        .complete_stream(&messages, 16384, 0.7, Box::new(move |token| {
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

    // Fetch LTP context for today's date to align lesson content with long-term plan.
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let ltp_context = get_ltp_context(db, &today).unwrap_or(None);

    // Fetch schedule context (RecurringEvent data) for the current week.
    let schedule_context = get_schedule_context(db, &today);

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
            ltp_context.as_ref(),
            schedule_context.as_ref(),
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

    // Fetch LTP context for today's date to align lesson content with long-term plan.
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let ltp_context = get_ltp_context(db, &today).unwrap_or(None);

    // Fetch schedule context (RecurringEvent data) for the current week.
    let schedule_context = get_schedule_context(db, &today);

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
            ltp_context.as_ref(),
            schedule_context.as_ref(),
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

/// Validate an OpenAI API key by making a lightweight models-list request.
/// Returns Ok(true) if the key is valid, Ok(false) if rejected, or Err on network failure.
#[tauri::command]
pub async fn validate_openai_key(api_key: String) -> Result<bool, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    Ok(resp.status().is_success())
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
        let messages = build_messages(&history, "Hello", "", None, None, None, None);

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
        let messages = build_messages(&history, "Help me", rag_ctx, None, None, None, None);

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
        let messages = build_messages(&history, "New question", "", None, None, None, None);

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

        let messages = build_messages(&history, "Final", "", None, None, None, None);
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
        let messages = build_messages(&history, "Hi", "", None, None, None, None);

        // system + 1 user from history (system skipped) + user = 3
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1].content, "Included");
    }

    #[test]
    fn test_build_messages_with_active_plan() {
        let history: Vec<ChatMessage> = vec![];
        let plan = Some(("Photosynthesis Lab", "Students will learn about..."));
        let messages = build_messages(&history, "Help me improve this", "", plan, None, None, None);

        assert_eq!(messages.len(), 2);
        assert!(messages[0].content.contains("CURRENT LESSON PLAN"));
        assert!(messages[0].content.contains("Photosynthesis Lab"));
        assert!(messages[0].content.contains("Students will learn about"));
    }

    #[test]
    fn test_build_messages_with_empty_plan() {
        let history: Vec<ChatMessage> = vec![];
        let plan = Some(("New Plan", ""));
        let messages = build_messages(&history, "Help me", "", plan, None, None, None);

        assert_eq!(messages.len(), 2);
        assert!(messages[0].content.contains("currently empty"));
        assert!(messages[0].content.contains("New Plan"));
    }

    #[test]
    fn test_build_messages_with_plan_and_rag() {
        let history: Vec<ChatMessage> = vec![];
        let plan = Some(("My Plan", "Some content here"));
        let messages = build_messages(&history, "Help", "Related: old plan data", plan, None, None, None);

        assert_eq!(messages.len(), 2);
        assert!(messages[0].content.contains("CURRENT LESSON PLAN"));
        assert!(messages[0].content.contains("TEACHING HISTORY"));
        assert!(messages[0].content.contains("Related: old plan data"));
    }

    #[test]
    fn test_build_messages_with_teaching_template() {
        let history: Vec<ChatMessage> = vec![];
        let template = r##"{"color_scheme":{"mappings":[{"color":"#FFD700","category":"Math","frequency":5}]},"table_structure":{"layout_type":"schedule_grid","columns":["Time","Monday","Tuesday"],"row_categories":["Morning","Afternoon"],"column_count":3},"time_slots":["8:00-9:00","9:00-10:00"],"content_patterns":{"cell_content_types":["activity"],"has_links":false,"has_rich_formatting":true},"recurring_elements":{"subjects":["Math","Reading"],"activities":["Circle Time","Centers"]}}"##;
        let messages = build_messages(&history, "Make a plan", "", None, Some(template), None, None);

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
        let messages = build_messages(&history, "Help", "", plan, Some(template), None, None);

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
    fn test_format_template_instructions_many_time_slots_skeleton_shows_all() {
        // All time slots must appear in the skeleton — the AI must reproduce every one.
        let template = r#"{"color_scheme":{"mappings":[]},"table_structure":{"layout_type":"schedule_grid","columns":["Time","Monday","Tuesday"],"row_categories":[],"column_count":3},"time_slots":["8:00-8:30","8:30-9:00","9:00-9:30","9:30-10:00","10:00-10:30"],"content_patterns":{"cell_content_types":[],"has_links":false,"has_rich_formatting":false},"recurring_elements":{"subjects":[],"activities":[]}}"#;

        let result = format_template_instructions(template);

        // All time slots listed in both the Time Blocks section AND the HTML skeleton.
        assert!(result.contains("8:00-8:30"));
        assert!(result.contains("8:30-9:00"));
        assert!(result.contains("9:00-9:30"));
        assert!(result.contains("9:30-10:00"));
        assert!(result.contains("10:00-10:30"));
        // The skeleton should NOT truncate — no "continue for all" comment.
        assert!(!result.contains("continue for all time slots"));
        // Should specify the exact slot count.
        assert!(result.contains("5 time slot rows"), "Should mention exact count: {}", result);
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

        // Skeleton should pre-fill routine events at matching time slots.
        // "Recess" has time_slot "9:00-9:15" which matches a time_slot in the schema,
        // so the skeleton row for 9:00-9:15 should show "Recess" instead of "Activity Name".
        // The skeleton only shows the first 3 time slots: 8:00-8:45, 9:00-9:15, 11:30-12:00.
        // "Recess" at 9:00-9:15 and "Lunch" at 11:30-12:00 should appear in the skeleton.
        assert!(result.contains("<strong>Recess</strong>"), "Skeleton should pre-fill Recess at its time slot");
        assert!(result.contains("<strong>Lunch</strong>"), "Skeleton should pre-fill Lunch at its time slot");

        // The mandatory reminder should be present.
        assert!(result.contains("RECURRING EVENTS ARE MANDATORY"), "Missing mandatory recurring events reminder");
    }

    #[test]
    fn test_format_template_instructions_no_daily_routine() {
        let template = r#"{"color_scheme":{"mappings":[]},"table_structure":{"layout_type":"schedule_grid","columns":["Time","Monday"],"row_categories":[],"column_count":2},"time_slots":["9:00"],"content_patterns":{"cell_content_types":[],"has_links":false,"has_rich_formatting":false},"recurring_elements":{"subjects":[],"activities":[]},"daily_routine":[]}"#;

        let result = format_template_instructions(template);

        // Should NOT have the daily routine section when empty.
        assert!(!result.contains("Daily Routine Events"));
    }

    #[test]
    fn test_format_template_instructions_skeleton_prefills_routine_events() {
        // Standard layout: 3 time slots, 2 have routine events, 1 does not.
        let template = r##"{"color_scheme":{"mappings":[]},"table_structure":{"layout_type":"schedule_grid","columns":["Time","Monday","Tuesday","Wednesday"],"row_categories":[],"column_count":4},"time_slots":["8:00-8:30","9:00-9:30","11:30-12:00"],"content_patterns":{"cell_content_types":[],"has_links":false,"has_rich_formatting":false},"recurring_elements":{"subjects":[],"activities":[]},"daily_routine":[{"name":"Morning Circle","time_slot":"8:00-8:30"},{"name":"Lunch","time_slot":"11:30-12:00"}]}"##;

        let result = format_template_instructions(template);

        // Routine slots should show the event name, not "Activity Name".
        assert!(result.contains("<strong>Morning Circle</strong>"), "Skeleton should pre-fill Morning Circle");
        assert!(result.contains("<strong>Lunch</strong>"), "Skeleton should pre-fill Lunch");

        // Non-routine slot (9:00-9:30) should still show per-day placeholder hints.
        // Check that day-specific placeholders appear (for the non-routine slot).
        assert!(result.contains("[Monday Activity]"), "Non-routine slots should show per-day placeholder: {}", result);
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
    fn test_format_template_instructions_transposed_schedule() {
        // Transposed: time_slots in columns, days_of_week in rows.
        let template = r##"{"color_scheme":{"mappings":[{"color":"#9900ff","category":"header","frequency":6}]},"table_structure":{"layout_type":"schedule_grid","columns":["Day","8:00-8:30","8:30-9:00","9:00-9:30"],"row_categories":["Monday","Tuesday","Wednesday"],"column_count":4,"column_semantic":"time_slots","row_semantic":"days_of_week"},"time_slots":["8:00-8:30","8:30-9:00","9:00-9:30"],"content_patterns":{"cell_content_types":[],"has_links":false,"has_rich_formatting":false},"recurring_elements":{"subjects":[],"activities":["Math"]},"daily_routine":[{"name":"Recess","time_slot":"9:00-9:30","days":["Monday","Tuesday","Wednesday"]}]}"##;

        let result = format_template_instructions(template);

        // Should describe transposed orientation.
        assert!(result.contains("transposed"), "Should mention transposed layout: {}", result);
        assert!(result.contains("time slots/periods as columns"), "Should describe time as columns");
        assert!(result.contains("days of the week as rows"), "Should describe days as rows");

        // Time blocks should say "column headers" not "first column".
        assert!(result.contains("column headers"), "Time slots should be column headers in transposed");

        // HTML skeleton should have time slots in header row and days as row labels.
        assert!(result.contains("Monday"), "Skeleton should include day names as rows");
        assert!(result.contains("8:00-8:30"), "Skeleton should include time slots in header");

        // Daily routine should still be present.
        assert!(result.contains("Recess"));

        // Skeleton should pre-fill Recess at 9:00-9:30 in transposed layout.
        assert!(result.contains("<strong>Recess</strong>"), "Transposed skeleton should pre-fill Recess");

        // Critical instruction should reference exact slot count and day rows.
        assert!(result.contains("3 time slot columns"), "Should mention exact slot count: {}", result);
        assert!(result.contains("5 day rows"), "Should mention 5 day rows: {}", result);

        // Mandatory reminder should be present.
        assert!(result.contains("RECURRING EVENTS ARE MANDATORY"), "Missing mandatory reminder in transposed");
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

    /// End-to-end pipeline test: extract_template → JSON → format_template_instructions.
    /// Verifies the AI prompt for a realistic 17-slot TK schedule contains ALL time slots,
    /// ALL routine events, ALL colors, and the complete HTML skeleton.
    #[test]
    fn test_e2e_tk_schedule_extraction_to_ai_prompt() {
        use crate::digest::template_extractor::extract_template;

        // ── Realistic 17-slot TK daily schedule (Mrs. Coles style) ──
        let html = r#"<html><body>
            <table>
                <tr>
                    <th style="background-color:#9900ff">Day/Time</th>
                    <th style="background-color:#9900ff">Monday</th>
                    <th style="background-color:#9900ff">Tuesday</th>
                    <th style="background-color:#9900ff">Wednesday</th>
                    <th style="background-color:#9900ff">Thursday</th>
                    <th style="background-color:#9900ff">Friday</th>
                </tr>
                <tr><td>8:15 AM-8:30 AM</td><td style="background-color:#ffff00">Soft Start Breakfast</td><td style="background-color:#ffff00">Soft Start Breakfast</td><td style="background-color:#ffff00">Soft Start Breakfast</td><td style="background-color:#ffff00">Soft Start Breakfast</td><td style="background-color:#ffff00">Soft Start Breakfast</td></tr>
                <tr><td>8:30 AM-9:00 AM</td><td>Morning Circle</td><td>Morning Circle</td><td>Morning Circle</td><td>Morning Circle</td><td>Morning Circle</td></tr>
                <tr><td>9:00 AM-9:10 AM</td><td style="background-color:#d9ead3">Snack/Recess</td><td style="background-color:#d9ead3">Snack/Recess</td><td style="background-color:#d9ead3">Snack/Recess</td><td style="background-color:#d9ead3">Snack/Recess</td><td style="background-color:#d9ead3">Snack/Recess</td></tr>
                <tr><td>9:10 AM-9:30 AM</td><td>Calendar Math</td><td>Calendar Math</td><td>Calendar Math</td><td>Calendar Math</td><td>Calendar Math</td></tr>
                <tr><td>9:30 AM-10:00 AM</td><td>ELA Mini Lesson</td><td>ELA Mini Lesson</td><td>ELA Mini Lesson</td><td>ELA Mini Lesson</td><td>ELA Mini Lesson</td></tr>
                <tr><td>10:00 AM-10:30 AM</td><td>Centers/Small Group</td><td>Centers/Small Group</td><td>Centers/Small Group</td><td>Centers/Small Group</td><td>Centers/Small Group</td></tr>
                <tr><td>10:30 AM-11:00 AM</td><td>Math Lesson</td><td>Math Lesson</td><td>Math Lesson</td><td>Math Lesson</td><td>Math Lesson</td></tr>
                <tr><td>11:00 AM-11:15 AM</td><td style="background-color:#d9ead3">Recess</td><td style="background-color:#d9ead3">Recess</td><td style="background-color:#d9ead3">Recess</td><td style="background-color:#d9ead3">Recess</td><td style="background-color:#d9ead3">Recess</td></tr>
                <tr><td>11:15 AM-11:30 AM</td><td>Lunch Prep</td><td>Lunch Prep</td><td>Lunch Prep</td><td>Lunch Prep</td><td>Lunch Prep</td></tr>
                <tr><td>11:30 AM-12:00 PM</td><td style="background-color:#fce5cd">TK Lunch</td><td style="background-color:#fce5cd">TK Lunch</td><td style="background-color:#fce5cd">TK Lunch</td><td style="background-color:#fce5cd">TK Lunch</td><td style="background-color:#fce5cd">TK Lunch</td></tr>
                <tr><td>12:00 PM-12:45 PM</td><td>Rest Time</td><td>Rest Time</td><td>Rest Time</td><td>Rest Time</td><td>Rest Time</td></tr>
                <tr><td>12:45 PM-1:15 PM</td><td>Science/Social Studies</td><td>Science/Social Studies</td><td>Science/Social Studies</td><td>Science/Social Studies</td><td>Science/Social Studies</td></tr>
                <tr><td>1:15 PM-1:45 PM</td><td>Mandarin</td><td>PE</td><td>Mandarin</td><td>Art</td><td>Music</td></tr>
                <tr><td>1:45 PM-2:00 PM</td><td style="background-color:#d9ead3">Recess</td><td style="background-color:#d9ead3">Recess</td><td style="background-color:#d9ead3">Recess</td><td style="background-color:#d9ead3">Recess</td><td style="background-color:#d9ead3">Recess</td></tr>
                <tr><td>2:00 PM-2:30 PM</td><td>Read Aloud/Art</td><td>Read Aloud/Art</td><td>Read Aloud/Art</td><td>Read Aloud/Art</td><td>Read Aloud/Art</td></tr>
                <tr><td>2:30 PM-2:40 PM</td><td>Pack Up</td><td>Pack Up</td><td>Pack Up</td><td>Pack Up</td><td>Pack Up</td></tr>
                <tr><td>2:40 PM-3:00 PM</td><td style="background-color:#c9daf8">Dismissal</td><td style="background-color:#c9daf8">Dismissal</td><td style="background-color:#c9daf8">Dismissal</td><td style="background-color:#c9daf8">Dismissal</td><td style="background-color:#c9daf8">Dismissal</td></tr>
            </table>
        </body></html>"#;

        // ── Step 1: Extract template from HTML ──
        let template = extract_template(html);

        // Verify extraction basics.
        assert_eq!(template.table_structure.layout_type, "schedule_grid");
        assert_eq!(template.table_structure.column_count, 6);
        assert_eq!(template.time_slots.len(), 17,
            "Expected 17 time slots, got {}: {:?}", template.time_slots.len(), template.time_slots);

        // ── Step 2: Serialize to JSON (same as what gets stored in DB) ──
        let template_json = serde_json::to_string(&template).unwrap();

        // ── Step 3: Generate AI prompt instructions ──
        let prompt = format_template_instructions(&template_json);

        // ── Step 4: Verify ALL 17 time slots appear in the prompt ──
        let expected_slots = [
            "8:15 AM-8:30 AM",
            "8:30 AM-9:00 AM",
            "9:00 AM-9:10 AM",
            "9:10 AM-9:30 AM",
            "9:30 AM-10:00 AM",
            "10:00 AM-10:30 AM",
            "10:30 AM-11:00 AM",
            "11:00 AM-11:15 AM",
            "11:15 AM-11:30 AM",
            "11:30 AM-12:00 PM",
            "12:00 PM-12:45 PM",
            "12:45 PM-1:15 PM",
            "1:15 PM-1:45 PM",
            "1:45 PM-2:00 PM",
            "2:00 PM-2:30 PM",
            "2:30 PM-2:40 PM",
            "2:40 PM-3:00 PM",
        ];
        for slot in &expected_slots {
            assert!(prompt.contains(slot),
                "AI prompt missing time slot '{}'. Full prompt:\n{}", slot, prompt);
        }

        // ── Step 5: Verify routine events appear in the prompt ──
        let expected_routine = [
            "Soft Start Breakfast",
            "Snack/Recess",
            "Recess",
            "TK Lunch",
            "Rest Time",
            "Dismissal",
            "Morning Circle",
            "Calendar Math",
        ];
        for event in &expected_routine {
            assert!(prompt.contains(event),
                "AI prompt missing routine event '{}'. Full prompt:\n{}", event, prompt);
        }

        // ── Step 6: Verify the HTML skeleton has all 17 rows ──
        // Count <tr> in the skeleton — should have 1 header + 17 data rows = 18 total.
        let skeleton_trs = prompt.matches("<tr>").count();
        // The skeleton should show header row + one row per time slot.
        assert!(skeleton_trs >= 18,
            "Skeleton should have ≥18 <tr> tags (1 header + 17 slots), got {}. Prompt:\n{}", skeleton_trs, prompt);

        // ── Step 7: Verify skeleton pre-fills routine events ──
        assert!(prompt.contains("<strong>Soft Start Breakfast</strong>"),
            "Skeleton should pre-fill Soft Start Breakfast");
        assert!(prompt.contains("<strong>TK Lunch</strong>"),
            "Skeleton should pre-fill TK Lunch");
        assert!(prompt.contains("<strong>Dismissal</strong>"),
            "Skeleton should pre-fill Dismissal");

        // Both Recess events should appear in Daily Routine Events section.
        let recess_count = prompt.matches("**Recess** at").count();
        assert_eq!(recess_count, 2,
            "Should have 2 Recess entries (11:00 AM and 1:45 PM) in Daily Routine Events");

        // ── Step 8: Verify color scheme in prompt ──
        assert!(prompt.contains("#9900ff"), "Prompt missing purple header color #9900ff");
        assert!(prompt.contains("background-color"), "Prompt should mention background-color styling");

        // ── Step 9: Verify critical instructions are present ──
        assert!(prompt.contains("DO NOT SHORTEN THE SCHEDULE"),
            "Missing critical 'do not shorten' instruction");
        assert!(prompt.contains("17 time slot rows"),
            "Should mention exactly 17 time slot rows. Prompt:\n{}", prompt);
        assert!(prompt.contains("RECURRING EVENTS ARE MANDATORY"),
            "Missing mandatory recurring events reminder");
        assert!(prompt.contains("TEXT CONTRAST"),
            "Missing text contrast instruction");

        // ── Step 10: Verify the prompt does NOT contain truncation ──
        assert!(!prompt.contains("continue for all"),
            "Prompt should NOT truncate the skeleton");
    }

    // ── LTP context tests ──────────────────────────────────────

    #[test]
    fn test_month_number_to_name() {
        assert_eq!(month_number_to_name(1), "January");
        assert_eq!(month_number_to_name(3), "March");
        assert_eq!(month_number_to_name(12), "December");
    }

    #[test]
    fn test_day_of_week() {
        // 2025-03-17 is a Monday.
        assert_eq!(day_of_week(2025, 3, 17), 0);
        // 2025-03-20 is a Thursday.
        assert_eq!(day_of_week(2025, 3, 20), 3);
        // 2025-03-23 is a Sunday.
        assert_eq!(day_of_week(2025, 3, 23), 6);
    }

    #[test]
    fn test_format_date_roundtrip() {
        let days = simple_date(2025, 3, 20).unwrap();
        assert_eq!(format_date(days), "2025-03-20");

        let days2 = simple_date(2025, 12, 31).unwrap();
        assert_eq!(format_date(days2), "2025-12-31");

        let days3 = simple_date(2025, 1, 1).unwrap();
        assert_eq!(format_date(days3), "2025-01-01");
    }

    #[test]
    fn test_format_ltp_context() {
        use crate::database::{LtpContext, LtpSubjectContext};

        let ctx = LtpContext {
            month: "March".to_string(),
            unit_name: Some("Unit 3: Wind and Water".to_string()),
            subjects: vec![
                LtpSubjectContext {
                    subject: "Math".to_string(),
                    content: "Addition and subtraction".to_string(),
                },
                LtpSubjectContext {
                    subject: "Reading".to_string(),
                    content: "Phonics: letter groups".to_string(),
                },
            ],
            calendar_notes: vec!["2025-03-20: SPRING BK".to_string()],
            daily_details: vec![],
            event_relationships: vec![],
        };

        let result = format_ltp_context(&ctx);
        assert!(result.contains("LONG-TERM PLAN CONTEXT FOR THIS WEEK"));
        assert!(result.contains("Month: March"));
        assert!(result.contains("Unit 3: Wind and Water"));
        assert!(result.contains("Math: Addition and subtraction"));
        assert!(result.contains("Reading: Phonics: letter groups"));
        assert!(result.contains("SPRING BK"));
        assert!(result.contains("END LONG-TERM PLAN CONTEXT"));
    }

    #[test]
    fn test_format_ltp_context_minimal() {
        use crate::database::LtpContext;

        let ctx = LtpContext {
            month: "August".to_string(),
            unit_name: None,
            subjects: vec![],
            calendar_notes: vec![],
            daily_details: vec![],
            event_relationships: vec![],
        };

        let result = format_ltp_context(&ctx);
        assert!(result.contains("Month: August"));
        assert!(!result.contains("Subject Guidance"));
        assert!(!result.contains("Calendar Notes"));
    }

    #[test]
    fn test_format_ltp_context_with_daily_details() {
        use crate::database::{LtpContext, LtpDailyDetail, EventRelationship};

        let ctx = LtpContext {
            month: "March".to_string(),
            unit_name: Some("Unit 5".to_string()),
            subjects: vec![],
            calendar_notes: vec![],
            daily_details: vec![
                LtpDailyDetail {
                    day: "Monday".to_string(),
                    entries: vec!["PE 11:00-11:40".to_string()],
                },
                LtpDailyDetail {
                    day: "Tuesday".to_string(),
                    entries: vec!["New Center Intro: House/Trains".to_string()],
                },
            ],
            event_relationships: vec![
                EventRelationship {
                    intro_event: "New Center Intro".to_string(),
                    main_event: "Centers (time block)".to_string(),
                    description: "The intro sets up center activities".to_string(),
                },
            ],
        };

        let result = format_ltp_context(&ctx);
        assert!(result.contains("Per-Day Activity Details"));
        assert!(result.contains("Monday"));
        assert!(result.contains("PE 11:00-11:40"));
        assert!(result.contains("Tuesday"));
        assert!(result.contains("House/Trains"));
        assert!(result.contains("Event Relationships"));
        assert!(result.contains("New Center Intro"));
        assert!(result.contains("Centers (time block)"));
    }

    #[test]
    fn test_daily_variation_mandate_in_template() {
        // Verify that the daily variation section appears in template instructions.
        let template = r##"{"color_scheme":{"mappings":[]},"table_structure":{"layout_type":"schedule_grid","columns":["Time","Monday","Tuesday"],"row_categories":[],"column_count":3},"time_slots":["9:00-10:00"],"content_patterns":{"cell_content_types":[],"has_links":false,"has_rich_formatting":false},"recurring_elements":{"subjects":[],"activities":[]},"daily_routine":[]}"##;

        let result = format_template_instructions(template);
        assert!(result.contains("Daily Variation"), "Should include daily variation section");
        assert!(result.contains("MUST have **different** lesson activities"), "Should mandate different activities");
        assert!(result.contains("Event Relationships"), "Should include event relationship guidance");
        assert!(result.contains("Per-Day Specials"), "Should include per-day specials guidance");
        assert!(result.contains("LTP-Informed Variety"), "Should include LTP variety guidance");
    }

    #[test]
    fn test_routine_event_classification_in_template() {
        use crate::database::{DailyRoutineEvent, RoutineEventType};

        // Template with classified events.
        let template = serde_json::json!({
            "color_scheme": {"mappings": []},
            "table_structure": {"layout_type": "schedule_grid", "columns": ["Time", "Mon", "Tue"], "row_categories": [], "column_count": 3},
            "time_slots": ["9:00-10:00", "10:00-11:00", "11:00-12:00"],
            "content_patterns": {"cell_content_types": [], "has_links": false, "has_rich_formatting": false},
            "recurring_elements": {"subjects": [], "activities": []},
            "daily_routine": [
                {"name": "Lunch", "time_slot": "11:00-12:00", "days": ["Mon", "Tue"], "event_type": "fixed"},
                {"name": "Centers/Small Group", "time_slot": "9:00-10:00", "days": ["Mon", "Tue"], "event_type": "variable"},
                {"name": "PE", "time_slot": "10:00-11:00", "days": ["Mon"], "event_type": "day_specific"},
            ]
        });

        let result = format_template_instructions(&template.to_string());
        assert!(result.contains("Fixed recurring"), "Should list fixed events: {}", result);
        assert!(result.contains("Lunch"), "Lunch should be in fixed list");
        assert!(result.contains("Variable recurring"), "Should list variable events");
        assert!(result.contains("Centers/Small Group"), "Centers should be in variable list");
        assert!(result.contains("Day-specific"), "Should list day-specific events");
        assert!(result.contains("PE"), "PE should be in day-specific list");
    }

    #[test]
    fn test_build_messages_with_ltp_context() {
        use crate::database::{LtpContext, LtpSubjectContext};

        let history = Vec::new();
        let ltp_ctx = LtpContext {
            month: "March".to_string(),
            unit_name: Some("Unit 3".to_string()),
            subjects: vec![LtpSubjectContext {
                subject: "Math".to_string(),
                content: "Shapes".to_string(),
            }],
            calendar_notes: vec![],
            daily_details: vec![],
            event_relationships: vec![],
        };

        let messages = build_messages(&history, "Plan a lesson", "", None, None, Some(&ltp_ctx), None);
        assert_eq!(messages.len(), 2);
        assert!(messages[0].content.contains("LONG-TERM PLAN CONTEXT"));
        assert!(messages[0].content.contains("Math: Shapes"));
        assert!(messages[0].content.contains("Unit 3"));
    }

    // ── format_schedule_instructions tests ──────────────────────

    fn make_event(id: &str, name: &str, event_type: &str) -> RecurringEvent {
        RecurringEvent {
            id: id.to_string(),
            name: name.to_string(),
            event_type: event_type.to_string(),
            linked_to: None,
            details_vary_daily: false,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn make_occ(event_id: &str, dow: i32, start: &str, end: &str) -> EventOccurrence {
        EventOccurrence {
            id: String::new(),
            event_id: event_id.to_string(),
            day_of_week: dow,
            start_time: start.to_string(),
            end_time: end.to_string(),
        }
    }

    #[test]
    fn test_format_schedule_basic() {
        let ctx = ScheduleContext {
            events: vec![
                (
                    make_event("1", "Breakfast", "fixed"),
                    vec![
                        make_occ("1", 0, "8:15", "9:00"),
                        make_occ("1", 1, "8:15", "9:00"),
                    ],
                ),
                (
                    make_event("2", "Math Block", "teaching_slot"),
                    vec![
                        make_occ("2", 0, "9:30", "10:30"),
                    ],
                ),
            ],
            exceptions: vec![],
        };

        let result = format_schedule_instructions(&ctx);
        assert!(result.contains("MONDAY:"));
        assert!(result.contains("TUESDAY:"));
        assert!(result.contains("8:15-9:00: Breakfast"));
        assert!(result.contains("[FIXED - reproduce as-is]"));
        assert!(result.contains("9:30-10:30: Math Block"));
        assert!(result.contains("[TEACHING SLOT - generate lesson content]"));
    }

    #[test]
    fn test_format_schedule_with_special() {
        let ctx = ScheduleContext {
            events: vec![(
                make_event("1", "PE", "special"),
                vec![make_occ("1", 0, "10:00", "10:50")],
            )],
            exceptions: vec![],
        };

        let result = format_schedule_instructions(&ctx);
        assert!(result.contains("10:00-10:50: PE"));
        assert!(result.contains("[SPECIAL - reproduce as-is]"));
        // Tuesday should have no PE
        assert!(result.contains("TUESDAY:\n- (no events scheduled)"));
    }

    #[test]
    fn test_format_schedule_holiday_skips_day() {
        let ctx = ScheduleContext {
            events: vec![(
                make_event("1", "Breakfast", "fixed"),
                vec![make_occ("1", 0, "8:15", "9:00")],
            )],
            exceptions: vec![CalendarException {
                id: String::new(),
                calendar_id: String::new(),
                // Monday = 2026-03-16 (a Monday)
                date: "2026-03-16".to_string(),
                exception_type: "no_school".to_string(),
                label: "Spring Break".to_string(),
            }],
        };

        let result = format_schedule_instructions(&ctx);
        assert!(result.contains("MONDAY:\n- **Spring Break** — No School"));
        // Should NOT show Breakfast on Monday
        assert!(!result.contains("MONDAY:\n- 8:15"));
    }

    #[test]
    fn test_format_schedule_half_day() {
        let ctx = ScheduleContext {
            events: vec![(
                make_event("1", "Breakfast", "fixed"),
                vec![make_occ("1", 2, "8:15", "9:00")], // Wednesday
            )],
            exceptions: vec![CalendarException {
                id: String::new(),
                calendar_id: String::new(),
                // Wednesday = 2026-03-18
                date: "2026-03-18".to_string(),
                exception_type: "half_day".to_string(),
                label: "Parent Conferences".to_string(),
            }],
        };

        let result = format_schedule_instructions(&ctx);
        assert!(result.contains("Parent Conferences"));
        assert!(result.contains("truncated schedule"));
        // Events still shown so AI can decide what fits
        assert!(result.contains("8:15-9:00: Breakfast"));
    }

    #[test]
    fn test_format_schedule_linked_events() {
        let mut intro = make_event("1", "New Center Intro", "fixed");
        intro.linked_to = Some("2".to_string());

        let centers = make_event("2", "Centers", "teaching_slot");

        let ctx = ScheduleContext {
            events: vec![
                (intro, vec![make_occ("1", 0, "11:30", "11:45")]),
                (centers, vec![make_occ("2", 0, "11:45", "12:30")]),
            ],
            exceptions: vec![],
        };

        let result = format_schedule_instructions(&ctx);
        assert!(result.contains("New Center Intro → Centers [LINKED"));
        assert!(result.contains("**New Center Intro** precedes **Centers**"));
    }

    #[test]
    fn test_format_schedule_vary_daily() {
        let mut ev = make_event("1", "Centers", "teaching_slot");
        ev.details_vary_daily = true;

        let ctx = ScheduleContext {
            events: vec![(ev, vec![make_occ("1", 0, "11:00", "12:00")])],
            exceptions: vec![],
        };

        let result = format_schedule_instructions(&ctx);
        assert!(result.contains("Content That Must Vary Daily"));
        assert!(result.contains("**Centers**"));
        assert!(result.contains("different"));
    }

    #[test]
    fn test_build_messages_with_schedule_context() {
        let history: Vec<ChatMessage> = vec![];
        let ctx = ScheduleContext {
            events: vec![(
                make_event("1", "Lunch", "fixed"),
                vec![make_occ("1", 0, "12:00", "12:30")],
            )],
            exceptions: vec![],
        };

        let messages = build_messages(&history, "Plan my week", "", None, None, None, Some(&ctx));
        assert_eq!(messages.len(), 2);
        assert!(messages[0].content.contains("Weekly Schedule"));
        assert!(messages[0].content.contains("Lunch"));
        assert!(messages[0].content.contains("[FIXED - reproduce as-is]"));
    }
}
