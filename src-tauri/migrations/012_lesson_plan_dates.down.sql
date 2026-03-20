-- SQLite doesn't support DROP COLUMN before 3.35.0; recreate table.
-- For simplicity, drop the indexes only (columns are harmless if left).
DROP INDEX IF EXISTS idx_lesson_plans_school_year;
DROP INDEX IF EXISTS idx_lesson_plans_week_start;
