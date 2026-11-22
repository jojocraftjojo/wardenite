-- Wardenite initiales Schema (SQLite)
CREATE TABLE guild_settings (
    guild_id        INTEGER PRIMARY KEY,
    modlog_channel  INTEGER,
    welcome_channel INTEGER,
    welcome_message TEXT,
    autorole_id     INTEGER,
    block_invites   INTEGER NOT NULL DEFAULT 0,
    spam_max_messages INTEGER NOT NULL DEFAULT 0,
    banned_words    TEXT NOT NULL DEFAULT '[]',   -- JSON-Array
    warn_thresholds TEXT NOT NULL DEFAULT '[]'    -- JSON, siehe moderation::Threshold
);

CREATE TABLE warns (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    guild_id     INTEGER NOT NULL,
    user_id      INTEGER NOT NULL,
    moderator_id INTEGER NOT NULL,
    reason       TEXT,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_warns_user ON warns (guild_id, user_id);

CREATE TABLE mod_actions (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    guild_id     INTEGER NOT NULL,
    user_id      INTEGER NOT NULL,
    moderator_id INTEGER NOT NULL,
    action       TEXT NOT NULL,   -- ban | kick | timeout | warn | purge
    reason       TEXT,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE ticket_panels (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    guild_id    INTEGER NOT NULL,
    channel_id  INTEGER NOT NULL,
    message_id  INTEGER,
    category_id INTEGER,          -- Discord-Kategorie fuer neue Tickets
    support_role_id INTEGER,
    title       TEXT NOT NULL DEFAULT 'Support',
    description TEXT NOT NULL DEFAULT 'Klicke auf den Button, um ein Ticket zu erstellen.'
);

CREATE TABLE tickets (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    guild_id   INTEGER NOT NULL,
    number     INTEGER NOT NULL,
    channel_id INTEGER NOT NULL,
    opener_id  INTEGER NOT NULL,
    state      TEXT NOT NULL DEFAULT 'open',   -- open | claimed | closed
    claimed_by INTEGER,
    closed_by  INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    closed_at  TEXT,
    UNIQUE (guild_id, number)
);

CREATE TABLE ticket_messages (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    ticket_id INTEGER NOT NULL REFERENCES tickets (id) ON DELETE CASCADE,
    author_id INTEGER NOT NULL,
    author_name TEXT NOT NULL,
    content   TEXT NOT NULL,
    sent_at   TEXT NOT NULL
);

CREATE TABLE sessions (
    id         TEXT PRIMARY KEY,
    user_id    INTEGER NOT NULL,
    username   TEXT NOT NULL,
    access_token TEXT NOT NULL,
    expires_at TEXT NOT NULL
);
