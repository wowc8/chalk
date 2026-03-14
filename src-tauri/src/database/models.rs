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
