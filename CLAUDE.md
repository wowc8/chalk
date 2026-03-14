# CLAUDE.md — AI Agent Guardrails for LPA

## Project Overview
Lesson Plan Architect (LPA) — a Tauri v2 desktop app for teachers to manage and generate lesson plans.

## Tech Stack
- **Shell**: Tauri v2 (Rust backend)
- **Frontend**: React 19 + TypeScript + Vite
- **Styling**: Tailwind CSS + Framer Motion
- **Database**: SQLite (WAL mode) + sqlite-vec
- **Editor**: TipTap

## Mandatory Rules for AI Agents

1. **Read before writing**: Always read `docs/ARCHITECTURE.md` and `docs/DECISION_LOG.md` before making changes.
2. **Update docs first**: Update the relevant spec/architecture doc *before* adding a feature. Update `docs/DECISION_LOG.md` *after* implementing.
3. **Check logs on failure**: Before attempting fixes on failures, check `.log` files in the OS data directory.
4. **No schema changes without docs**: Never alter SQLite schema without updating `docs/MODULES.md`.
5. **Backup-before-write**: Never write to external documents (Google Docs, OneDrive) without creating a verified backup first.
6. **No rogue behavior**: Stick to the requested task scope. Do not refactor, add features, or change architecture without explicit approval.

## Directory Structure
```
src-tauri/src/       — Rust backend (admin, connectors, shredder, database, safety)
src/                 — React frontend (components, editor, state, hooks)
docs/                — Architecture docs, decision log, module specs
```

## Logging
- Backend: `tracing` crate with JSON output to OS data directory.
- Frontend errors are piped to the backend via the `log_frontend_error` Tauri command.

## Build & Run
```bash
npm install          # Install frontend deps
npm run tauri dev    # Development mode
npm run tauri build  # Production build
```
