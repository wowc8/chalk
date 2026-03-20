-- Add date metadata columns to lesson_plans for chronological organization.
-- week_start_date / week_end_date: ISO date strings (e.g. "2024-09-02").
-- school_year: display label (e.g. "2024-25").

ALTER TABLE lesson_plans ADD COLUMN week_start_date TEXT;
ALTER TABLE lesson_plans ADD COLUMN week_end_date TEXT;
ALTER TABLE lesson_plans ADD COLUMN school_year TEXT;

CREATE INDEX IF NOT EXISTS idx_lesson_plans_school_year ON lesson_plans(school_year);
CREATE INDEX IF NOT EXISTS idx_lesson_plans_week_start ON lesson_plans(week_start_date);
