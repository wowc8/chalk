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

/// Classification of a recurring event for prompt generation.
/// - `fixed`: Same content every day (breakfast, lunch, recess, dismissal).
/// - `variable`: Same time slot but content should change daily (centers, lessons, morning work).
/// - `day_specific`: Only occurs on certain days (PE on Monday, Drama on Wednesday).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RoutineEventType {
    Fixed,
    Variable,
    DaySpecific,
}

impl Default for RoutineEventType {
    fn default() -> Self {
        RoutineEventType::Fixed
    }
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
    /// Classification: fixed (same daily), variable (different content daily), or day_specific.
    #[serde(default)]
    pub event_type: RoutineEventType,
}

// ── LTP Documents ────────────────────────────────────────────

/// An imported Long-Term Plan document (LTP or school calendar HTML file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LtpDocument {
    pub id: String,
    pub filename: String,
    pub file_hash: String,
    pub school_year: Option<String>,
    pub doc_type: String,
    pub raw_html: String,
    pub imported_at: String,
    pub updated_at: String,
}

/// A parsed grid cell from an LTP document (resolved W3C grid output).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LtpGridCell {
    pub id: String,
    pub document_id: String,
    pub row_index: i32,
    pub col_index: i32,
    pub subject: Option<String>,
    pub month: Option<String>,
    pub content_html: Option<String>,
    pub content_text: Option<String>,
    pub background_color: Option<String>,
    pub unit_name: Option<String>,
    pub unit_color: Option<String>,
}

/// A school calendar entry parsed from a calendar-type LTP document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchoolCalendarEntry {
    pub id: String,
    pub document_id: String,
    pub date: Option<String>,
    pub day_number: Option<i32>,
    pub unit_name: Option<String>,
    pub unit_color: Option<String>,
    pub is_holiday: bool,
    pub holiday_name: Option<String>,
    pub notes: Option<String>,
}

/// Result of an LTP import operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LtpImportResult {
    /// Document was imported (new or updated).
    Imported(LtpDocument),
    /// Document was skipped because content hash matches existing.
    Skipped { id: String, filename: String },
}

// ── LTP Context ─────────────────────────────────────────────

/// Subject-level LTP content for a given month.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LtpSubjectContext {
    pub subject: String,
    pub content: String,
}

/// Per-day activity details extracted from LTP grid cells.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LtpDailyDetail {
    /// Day of the week (e.g., "Monday").
    pub day: String,
    /// Activity entries for this day (center names, materials, small group tasks).
    pub entries: Vec<String>,
}

/// A relationship between paired events (e.g., "New Center Intro" introduces "Centers: 60 Min").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRelationship {
    /// The introductory/setup event name.
    pub intro_event: String,
    /// The main event that follows.
    pub main_event: String,
    /// Description of the relationship.
    pub description: String,
}

/// Structured LTP context for a given date, ready for AI prompt injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LtpContext {
    /// The month name (e.g., "March").
    pub month: String,
    /// The current unit name (e.g., "Unit 3: Wind and Water"), if known.
    pub unit_name: Option<String>,
    /// Subject-by-subject LTP content for this month.
    pub subjects: Vec<LtpSubjectContext>,
    /// Calendar notes for the week (holidays, half days, etc.).
    pub calendar_notes: Vec<String>,
    /// Per-day activity details from LTP cells (centers, materials, small groups).
    #[serde(default)]
    pub daily_details: Vec<LtpDailyDetail>,
    /// Paired event relationships detected from the template.
    #[serde(default)]
    pub event_relationships: Vec<EventRelationship>,
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
