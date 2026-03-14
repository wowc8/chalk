# ARCHITECTURE.md — Chalk

## 1. System Philosophy

Clean Architecture principles with Documentation-Driven Development (DDD). Business logic is separated from external interfaces.

## 2. High-Level Directory Structure

```
/
├── docs/                      # The System Brain
│   ├── ARCHITECTURE.md
│   ├── MODULES.md
│   └── DECISION_LOG.md
├── src-tauri/                 # RUST BACKEND
│   ├── src/
│   │   ├── admin/             # AI Admin Agent & Setup Orchestration
│   │   ├── connectors/        # External API handlers (Google, OneDrive, LMS)
│   │   ├── shredder/          # Semantic table parsing logic
│   │   ├── database/          # SQLite & sqlite-vec (RAG engine)
│   │   └── safety/            # Backup-before-write protocols
│   │   └── logging.rs         # Structured JSON logging
├── src/                       # REACT FRONTEND
│   ├── components/            # UI components (Atomic design)
│   ├── editor/                # TipTap editor & Batman-mode overlays
│   ├── state/                 # Global state management
│   └── hooks/                 # Custom hooks for backend communication
└── CLAUDE.md                  # Operational guardrails for AI coding agents
```

## 3. Data Lifecycle & Orchestration

### 3.1 Ingestion & Shredding
Fetch → Shred (identify tables/headers, split into discrete lessons) → Index (UUID + semantic vector in sqlite-vec).

### 3.2 The Freshness Router
Remote-First check: Before any RAG operation, verify `last_modified` timestamp. If remote is newer, invalidate local cache and re-shred.

## 4. Safety & Integrity Protocols

### 4.1 The "Append" Guardrail
No writing to master doc without verified backup. Sequence: User Request → Copy File → Verify → Append. Failure at any step logs full stack trace.

### 4.2 Structured Logging
JSON Structured Logging via the `tracing` crate. Logs are written to the OS-specific data directory (`~/Library/Application Support/com.madison.chalk/logs` on macOS). Frontend console errors are piped to the Rust backend for consolidated file-based logging.

## 5. UI/UX Standards

### 5.1 Design Language
"Soft Minimalism." Low contrast, high whitespace, translucency effects (vibrancy/acrylic).
Animations: Framer Motion with spring physics (stiffness: 300, damping: 30).

### 5.2 The "Batman" Overlay
Frosted Glass overlay during AI generation. Pane locked to input. Retro-comic onomatopoeias with floating animations.

## 6. AI Agent Maintenance Rules

- Update spec before adding a feature; update `DECISION_LOG.md` after.
- Check `.log` files before attempting fixes on failures.
- No SQLite schema changes without updating `MODULES.md`.
