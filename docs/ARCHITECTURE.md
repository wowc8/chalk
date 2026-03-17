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
│   │   ├── safety/            # Backup-before-write protocols
│   │   ├── logging.rs         # Structured JSON logging
│   │   ├── sentry_integration.rs # Sentry crash reporting (conditional on consent)
│   │   └── privacy.rs         # Privacy consent management
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

### 4.3 Crash Reporting (Sentry)
Sentry is integrated on both Rust (`sentry` crate) and React (`@sentry/react`) sides. Initialization is conditional on the user's privacy consent preference stored in the `app_settings` SQLite table. PII is stripped before sending — no student data, document content, or OAuth tokens. Only OS version, app version, error stack traces, and breadcrumbs are sent. A Sentry `ErrorBoundary` wraps the entire React app to catch unhandled rendering errors. Users can also manually submit bug reports from Settings via `send_crash_report`.

### 4.4 Privacy Consent
On first launch, a consent dialog asks the user to opt in or out of crash reporting. The choice is stored in `app_settings` (key: `crash_reporting_consent`). Users can change this preference anytime in Settings. Sentry is only initialized when consent is granted.

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
