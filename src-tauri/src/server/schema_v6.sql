ALTER TABLE bounties ADD COLUMN active_duration_ms INTEGER NOT NULL DEFAULT 0;
ALTER TABLE bounties ADD COLUMN active_started_at TEXT;
CREATE TABLE bounty_comments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bounty_id TEXT NOT NULL REFERENCES bounties(id) ON DELETE CASCADE,
    author_type TEXT NOT NULL CHECK (author_type IN ('human','agent','system')),
    author_id TEXT,
    author_name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('comment','question','reply','status')),
    body TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX bounty_comments_order ON bounty_comments(bounty_id, id);
