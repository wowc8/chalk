-- Schedule intelligence tables: recurring events, occurrences, school calendar, exceptions.

CREATE TABLE IF NOT EXISTS recurring_events (
    id                TEXT PRIMARY KEY NOT NULL,
    name              TEXT NOT NULL,
    event_type        TEXT NOT NULL DEFAULT 'fixed',
    linked_to         TEXT,
    details_vary_daily INTEGER NOT NULL DEFAULT 0,
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (linked_to) REFERENCES recurring_events(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS event_occurrences (
    id              TEXT PRIMARY KEY NOT NULL,
    event_id        TEXT NOT NULL,
    day_of_week     INTEGER NOT NULL,
    start_time      TEXT NOT NULL,
    end_time        TEXT NOT NULL,
    FOREIGN KEY (event_id) REFERENCES recurring_events(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS school_calendar (
    id              TEXT PRIMARY KEY NOT NULL,
    year_start      TEXT NOT NULL,
    year_end        TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS calendar_exceptions (
    id              TEXT PRIMARY KEY NOT NULL,
    calendar_id     TEXT NOT NULL,
    date            TEXT NOT NULL,
    exception_type  TEXT NOT NULL,
    label           TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (calendar_id) REFERENCES school_calendar(id) ON DELETE CASCADE
);

-- Migrate onboarding status from file to app_settings.
-- The status is stored as a JSON string under the key 'onboarding_status'.
-- No data migration needed here; the Rust code will check the DB first and
-- fall back to the file on first run, then persist to DB going forward.
