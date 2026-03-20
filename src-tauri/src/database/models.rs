use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subject {
    pub id: String,
    pub name: String,
    pub grade_level: Option<String>,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonPlan {
    pub id: String,
    pub subject_id: String,
    pub title: String,
    pub content: String,
    pub source_doc_id: Option<String>,
    pub source_table_index: Option<i32>,
    pub learning_objectives: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub id: String,
    pub lesson_plan_id: String,
    pub key: String,
    pub value: String,
    pub created_at: String,
}

/// Input for creating a new subject.
#[derive(Debug, Deserialize)]
pub struct NewSubject {
    pub name: String,
    pub grade_level: Option<String>,
    pub description: Option<String>,
}

/// Input for creating a new lesson plan.
#[derive(Debug, Deserialize)]
pub struct NewLessonPlan {
    pub subject_id: String,
    pub title: String,
    pub content: Option<String>,
    pub source_doc_id: Option<String>,
    pub source_table_index: Option<i32>,
    pub learning_objectives: Option<String>,
}

/// Input for setting a metadata key-value on a lesson plan.
#[derive(Debug, Deserialize)]
pub struct NewMetadata {
    pub lesson_plan_id: String,
    pub key: String,
    pub value: String,
}

/// A vector search result with the matched rowid and distance.
#[derive(Debug, Clone, Serialize)]
pub struct VectorSearchResult {
    pub lesson_plan_id: String,
    pub distance: f64,
}

// ── Reference Documents ──────────────────────────────────────

/// A reference document extracted from a Google Doc for RAG context.
/// Not shown in the library — only feeds AI search and context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceDoc {
    pub id: String,
    pub source_doc_id: Option<String>,
    pub source_doc_name: Option<String>,
    pub title: String,
    pub content_html: String,
    pub content_text: String,
    pub created_at: String,
}

// ── Teaching Templates ───────────────────────────────────────

/// A teaching template extracted from a teacher's Google Docs during digest.
/// Captures formatting patterns (colors, table structure, time slots, recurring
/// elements) so AI-generated plans can match the teacher's style.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingTemplate {
    pub id: String,
    pub source_doc_id: Option<String>,
    pub source_doc_name: Option<String>,
    pub template_json: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Structured representation of a teaching template's JSON content.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TeachingTemplateSchema {
    #[serde(default)]
    pub color_scheme: ColorScheme,
    #[serde(default)]
    pub table_structure: TableStructure,
    #[serde(default)]
    pub time_slots: Vec<String>,
    #[serde(default)]
    pub content_patterns: ContentPatterns,
    #[serde(default)]
    pub recurring_elements: RecurringElements,
    /// Routine non-academic events that appear consistently across most days
    /// at similar times (e.g., breakfast, lunch, recess, dismissal).
    /// Included in weekly/daily plan prompts but excluded for single-lesson requests.
    #[serde(default)]
    pub daily_routine: Vec<DailyRoutineEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ColorScheme {
    #[serde(default)]
    pub mappings: Vec<ColorMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorMapping {
    pub color: String,
    pub category: String,
    pub frequency: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TableStructure {
    #[serde(default)]
    pub layout_type: String,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub row_categories: Vec<String>,
    #[serde(default)]
    pub column_count: usize,
    /// Semantic label for what columns represent (e.g., "days_of_week", "data_columns").
    #[serde(default)]
    pub column_semantic: Option<String>,
    /// Semantic label for what rows represent (e.g., "time_slots", "categories").
    #[serde(default)]
    pub row_semantic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContentPatterns {
    #[serde(default)]
    pub cell_content_types: Vec<String>,
    #[serde(default)]
    pub has_links: bool,
    #[serde(default)]
    pub has_rich_formatting: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecurringElements {
    #[serde(default)]
    pub subjects: Vec<String>,
    #[serde(default)]
    pub activities: Vec<String>,
}

/// A recurring event that appears consistently across most days at a similar time,
/// detected purely by frequency analysis (≥60% of day columns at the same time slot).
/// Examples: breakfast, lunch, recess, gym, specials, dismissal, morning meeting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyRoutineEvent {
    /// Display name of the routine event (e.g., "Lunch", "Recess").
    pub name: String,
    /// The time slot where this event typically occurs (e.g., "11:30-12:00").
    pub time_slot: Option<String>,
    /// Which days of the week this event occurs on (e.g., ["Monday", "Tuesday", "Wednesday"]).
    #[serde(default)]
    pub days: Vec<String>,
    /// Background color associated with this event's cells (e.g., "#ffff00").
    /// Extracted from the most common cell background-color at this time slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg_color: Option<String>,
}

// ── Tags ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct NewTag {
    pub name: String,
    pub color: Option<String>,
}

// ── Library ──────────────────────────────────────────────────

/// A lesson plan card for the Library view, enriched with tags.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryPlanCard {
    pub id: String,
    pub title: String,
    pub status: String,
    pub source_type: String,
    pub version: i32,
    pub tags: Vec<Tag>,
    pub created_at: String,
    pub updated_at: String,
}

/// Query parameters for listing library plans.
#[derive(Debug, Deserialize)]
pub struct LibraryQuery {
    pub source_type: Option<String>,
    pub search: Option<String>,
    pub tag_ids: Option<Vec<String>>,
}

// ── FTS Search ──────────────────────────────────────────────

/// A full-text search result with plan ID and FTS5 rank score.
#[derive(Debug, Clone, Serialize)]
pub struct FtsSearchResult {
    pub lesson_plan_id: String,
    pub title: String,
    pub rank: f64,
}

// ── Hybrid Search ───────────────────────────────────────────

/// A hybrid search result combining FTS5 and vector search scores.
/// Higher score = better match.
#[derive(Debug, Clone, Serialize)]
pub struct HybridSearchResult {
    pub lesson_plan_id: String,
    pub title: String,
    pub score: f64,
}

// ── Plan Versions ───────────────────────────────────────────

/// A snapshot of a lesson plan at a particular version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanVersion {
    pub id: String,
    pub plan_id: String,
    pub version: i32,
    pub title: String,
    pub content: String,
    pub learning_objectives: Option<String>,
    pub created_at: String,
}
