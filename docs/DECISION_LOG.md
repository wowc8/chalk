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
