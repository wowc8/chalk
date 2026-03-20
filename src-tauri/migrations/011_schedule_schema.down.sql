DROP TABLE IF EXISTS calendar_exceptions;
DROP TABLE IF EXISTS school_calendar;
DROP TABLE IF EXISTS event_occurrences;
DROP TABLE IF EXISTS recurring_events;

-- Remove onboarding_status from app_settings if present.
DELETE FROM app_settings WHERE key = 'onboarding_status';
