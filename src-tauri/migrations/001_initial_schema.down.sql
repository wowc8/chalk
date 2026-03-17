-- Rollback: drop initial schema tables

DROP INDEX IF EXISTS idx_metadata_unique_key;
DROP INDEX IF EXISTS idx_metadata_lesson_plan;
DROP INDEX IF EXISTS idx_lesson_plans_status;
DROP INDEX IF EXISTS idx_lesson_plans_subject;
DROP TABLE IF EXISTS metadata;
DROP TABLE IF EXISTS lesson_plans;
DROP TABLE IF EXISTS subjects;
