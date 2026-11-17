ALTER TABLE bounties ADD COLUMN assignment_expires_at TEXT;
ALTER TABLE bounties ADD COLUMN lease_warned_id INTEGER NOT NULL DEFAULT 0;
ALTER TABLE bounties ADD COLUMN completion_cost REAL;
ALTER TABLE agents ADD COLUMN timeout_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agents ADD COLUMN abandon_count INTEGER NOT NULL DEFAULT 0;
CREATE TABLE lease_incidents (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bounty_id TEXT NOT NULL REFERENCES bounties(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    lease_id INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('timeout','abandon')),
    created_at TEXT NOT NULL
);
CREATE INDEX lease_incidents_agent ON lease_incidents(agent_id, id);
