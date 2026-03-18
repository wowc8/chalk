-- FTS5 virtual table for full-text search across lesson plan content.
-- Indexes title, content, and learning_objectives for keyword search.
-- Uses content-sync (content=lesson_plans) so the FTS index stays in sync
-- with the source table via triggers.

CREATE VIRTUAL TABLE IF NOT EXISTS lesson_plans_fts USING fts5(
    title,
    content,
    learning_objectives,
    content=lesson_plans,
    content_rowid=rowid
);

-- Populate the FTS index with existing data.
INSERT INTO lesson_plans_fts(rowid, title, content, learning_objectives)
    SELECT rowid, title, content, COALESCE(learning_objectives, '')
    FROM lesson_plans;

-- Triggers to keep the FTS index in sync with the lesson_plans table.

CREATE TRIGGER IF NOT EXISTS lesson_plans_fts_insert AFTER INSERT ON lesson_plans BEGIN
    INSERT INTO lesson_plans_fts(rowid, title, content, learning_objectives)
        VALUES (NEW.rowid, NEW.title, NEW.content, COALESCE(NEW.learning_objectives, ''));
END;

CREATE TRIGGER IF NOT EXISTS lesson_plans_fts_update AFTER UPDATE ON lesson_plans BEGIN
    INSERT INTO lesson_plans_fts(lesson_plans_fts, rowid, title, content, learning_objectives)
        VALUES ('delete', OLD.rowid, OLD.title, OLD.content, COALESCE(OLD.learning_objectives, ''));
    INSERT INTO lesson_plans_fts(rowid, title, content, learning_objectives)
        VALUES (NEW.rowid, NEW.title, NEW.content, COALESCE(NEW.learning_objectives, ''));
END;

CREATE TRIGGER IF NOT EXISTS lesson_plans_fts_delete AFTER DELETE ON lesson_plans BEGIN
    INSERT INTO lesson_plans_fts(lesson_plans_fts, rowid, title, content, learning_objectives)
        VALUES ('delete', OLD.rowid, OLD.title, OLD.content, COALESCE(OLD.learning_objectives, ''));
END;
