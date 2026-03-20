-- Long-Term Plan documents and parsed grid/calendar data.

CREATE TABLE IF NOT EXISTS ltp_documents (
    id          TEXT PRIMARY KEY,
    filename    TEXT NOT NULL,
    file_hash   TEXT NOT NULL,              -- SHA-256 of raw HTML content
    school_year TEXT,                        -- e.g. "2025-2026"
    doc_type    TEXT NOT NULL DEFAULT 'ltp', -- 'ltp' or 'calendar'
    raw_html    TEXT NOT NULL,
    imported_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Unique constraint on filename so duplicate detection works via hash comparison.
CREATE UNIQUE INDEX IF NOT EXISTS idx_ltp_documents_filename ON ltp_documents(filename);

-- Parsed grid cells from an LTP document (resolved W3C grid output).
CREATE TABLE IF NOT EXISTS ltp_grid_cells (
    id               TEXT PRIMARY KEY,
    document_id      TEXT NOT NULL REFERENCES ltp_documents(id) ON DELETE CASCADE,
    row_index        INTEGER NOT NULL,
    col_index        INTEGER NOT NULL,
    subject          TEXT,
    month            TEXT,
    content_html     TEXT,
    content_text     TEXT,
    background_color TEXT,
    unit_name        TEXT,
    unit_color       TEXT
);

CREATE INDEX IF NOT EXISTS idx_ltp_grid_cells_document ON ltp_grid_cells(document_id);

-- School calendar entries parsed from calendar-type LTP documents.
CREATE TABLE IF NOT EXISTS school_calendar_entries (
    id           TEXT PRIMARY KEY,
    document_id  TEXT NOT NULL REFERENCES ltp_documents(id) ON DELETE CASCADE,
    date         TEXT,            -- ISO 8601 date string
    day_number   INTEGER,
    unit_name    TEXT,
    unit_color   TEXT,
    is_holiday   INTEGER NOT NULL DEFAULT 0,
    holiday_name TEXT,
    notes        TEXT
);

CREATE INDEX IF NOT EXISTS idx_school_calendar_entries_document ON school_calendar_entries(document_id);
