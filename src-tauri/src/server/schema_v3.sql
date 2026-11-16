ALTER TABLE agents ADD COLUMN webhook_url TEXT;
ALTER TABLE agents ADD COLUMN webhook_secret TEXT;
ALTER TABLE agents ADD COLUMN acked_event_id INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agents ADD COLUMN last_pushed_at TEXT;
CREATE TABLE agent_events (
    event_id INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    bounty_id TEXT,
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    delivery_status TEXT NOT NULL DEFAULT 'pending' CHECK (delivery_status IN ('pending','delivered','failed')),
    attempt_count INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TEXT,
    last_error TEXT,
    delivered_at TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX agent_events_agent_order ON agent_events(agent_id, event_id);
CREATE INDEX agent_events_delivery ON agent_events(delivery_status, next_attempt_at);
CREATE TABLE bounty_dispatches (
    bounty_id TEXT NOT NULL REFERENCES bounties(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    PRIMARY KEY (bounty_id, agent_id)
);
