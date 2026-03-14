# DECISION_LOG.md — LPA

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
