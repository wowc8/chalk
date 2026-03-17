# DECISION_LOG.md — Chalk

Record of architectural and implementation decisions.

## Format

Each entry follows:
- **Date**: YYYY-MM-DD
- **Decision**: What was decided
- **Rationale**: Why
- **Alternatives considered**: What else was evaluated

---

## 2026-03-14 — Project Initialization

**Decision**: Initialize with Tauri v2 + React 19 + TypeScript + Vite scaffold.

**Rationale**: Tauri v2 provides a lightweight, secure desktop shell with native Rust backend. React 19 + Vite gives fast HMR and modern DX. This matches the spec's core technology stack.

**Alternatives considered**:
- Electron: Heavier runtime, larger bundle size. Rejected for performance reasons.
- Tauri v1: Missing v2 features (plugin system, mobile support). Rejected.

---

## 2026-03-14 — Structured Logging with `tracing`

**Decision**: Use `tracing` + `tracing-subscriber` (JSON format) + `tracing-appender` (rolling daily files) for backend logging. Frontend errors piped via Tauri command.

**Rationale**: `tracing` is the Rust ecosystem standard for structured, async-aware logging. JSON output enables machine parsing. Rolling daily files prevent unbounded log growth. Consolidating frontend errors into the same log stream simplifies debugging.

**Alternatives considered**:
- `log` + `env_logger`: Simpler but lacks structured JSON output and async span support.
- `slog`: Viable but less ecosystem adoption than `tracing`.

---

## 2026-03-14 — SQLite + sqlite-vec Database Layer

**Decision**: Use `rusqlite` (bundled SQLite) with WAL mode and `sqlite-vec` for the local RAG vector store. Schema uses TEXT UUIDs as primary keys. A `_vec_id_map` table bridges TEXT plan IDs to the INTEGER rowids required by vec0 virtual tables.

**Rationale**: `rusqlite` with `bundled` feature avoids system SQLite dependency issues and guarantees version compatibility. WAL mode enables concurrent reads during writes — critical for a desktop app where UI reads shouldn't block background ingestion. `sqlite-vec` provides in-process vector similarity search without an external service, keeping the app fully offline-capable. The mapping table approach cleanly separates the vec0 constraint (integer rowids) from the application's UUID-based IDs.

**Alternatives considered**:
- `sqlx`: Async-first, but adds runtime complexity for a desktop app that doesn't need async DB access. Connection pooling is overkill for single-user SQLite.
- `diesel`: Strong ORM but heavy macro usage and migration tooling doesn't integrate well with sqlite-vec virtual tables.
- External vector DB (Qdrant, ChromaDB): Adds a separate process/service. Rejected to keep the app self-contained and offline-first.
- INTEGER rowids directly: Would require changing the application ID scheme. TEXT UUIDs are more portable and collision-resistant for future sync scenarios.

---

## 2026-03-16 — AI Admin Agent & OAuth Integration

**Decision**: Implement the admin module with file-based OAuth token/config/status persistence, async HTTP functions separated from MutexGuard-holding code, and a React conversational wizard for onboarding.

**Rationale**: Tauri async commands require `Send` futures, but `std::sync::MutexGuard` is not `Send`. Extracting config/paths from the guarded state before `.await` points avoids holding the guard across async boundaries. File-based persistence (JSON) for tokens, config, and onboarding status keeps the admin state inspectable and debuggable without a database dependency. The conversational wizard UI hides OAuth/API complexity from teachers.

**Alternatives considered**:
- `tokio::sync::Mutex`: Would allow holding the guard across awaits but adds unnecessary async overhead for simple file operations.
- Database-backed token storage: More complex than needed for single-user desktop app. JSON files are simpler and human-readable.
- Single-page form instead of wizard: Rejected because the multi-step flow matches the spec's "conversational wizard" requirement and reduces cognitive load.

---

## 2026-03-16 — Sentry Integration with Privacy Consent

**Decision**: Integrate Sentry on both Rust (`sentry` v0.35 crate with `backtrace`, `panic`, `contexts`, `reqwest`, `rustls` features) and React (`@sentry/react` with `ErrorBoundary`). Initialization is gated behind an opt-in privacy consent stored in the `app_settings` SQLite table. A first-launch dialog asks users to opt in/out. A Settings page allows changing the preference and manually submitting bug reports.

**Rationale**: Crash reporting is essential for improving app stability, but teachers handle sensitive student data. Opt-in consent respects privacy (FERPA compliance considerations). Storing consent in SQLite (not a file) leverages the existing database layer and transactional guarantees. The `app_settings` key-value table is generic enough for future settings without schema changes. PII stripping in `beforeSend` (frontend) and `send_default_pii: false` (backend) ensures no student data leaks. The "Send Report" button in Settings gives users agency to proactively report issues.

**Alternatives considered**:
- Always-on Sentry: Rejected — privacy concerns for education software.
- Separate settings file: Rejected — adds another persistence layer when SQLite is already available.
- Sentry `sentry-log` integration: Rejected in favor of `sentry-tracing` since the backend already uses `tracing`.
- Third-party crash reporting (Bugsnag, Crashlytics): Rejected — Sentry is open-source, self-hostable, and already specified in the project spec.
