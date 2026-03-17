DROP INDEX IF EXISTS idx_tags_name;
DROP INDEX IF EXISTS idx_lesson_plans_source_type;
DROP INDEX IF EXISTS idx_plan_tags_tag;
DROP INDEX IF EXISTS idx_plan_tags_plan;
DROP TABLE IF EXISTS plan_tags;
DROP TABLE IF EXISTS tags;
-- SQLite does not support DROP COLUMN before 3.35.0; these columns will remain.
