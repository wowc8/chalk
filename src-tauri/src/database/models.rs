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
