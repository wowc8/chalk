-- Chat conversations and messages for the AI chat feature.
-- Each conversation is tied to an optional lesson plan for context.

CREATE TABLE IF NOT EXISTS chat_conversations (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL DEFAULT 'New Chat',
    plan_id     TEXT,                           -- optional: lesson plan this chat is about
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (plan_id) REFERENCES lesson_plans(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS chat_messages (
    id              TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role            TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
    content         TEXT NOT NULL,
    context_plan_ids TEXT,                      -- JSON array of plan IDs used as RAG context
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (conversation_id) REFERENCES chat_conversations(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_chat_messages_conversation
    ON chat_messages(conversation_id);

CREATE INDEX IF NOT EXISTS idx_chat_conversations_plan
    ON chat_conversations(plan_id);
