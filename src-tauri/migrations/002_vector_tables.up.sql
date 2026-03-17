-- Vector similarity search table for lesson plan embeddings

CREATE VIRTUAL TABLE IF NOT EXISTS lesson_plan_vectors USING vec0(
    embedding float[1536]
);
