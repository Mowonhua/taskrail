CREATE TABLE server_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    lan_open_registration INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
INSERT INTO server_settings(id,lan_open_registration,updated_at)
VALUES(1,0,strftime('%Y-%m-%dT%H:%M:%fZ','now'));
CREATE TABLE invite_tokens (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL DEFAULT '',
    token_hash TEXT NOT NULL UNIQUE,
    token_prefix TEXT NOT NULL,
    max_uses INTEGER,
    use_count INTEGER NOT NULL DEFAULT 0,
    expires_at TEXT,
    revoked_at TEXT,
    created_at TEXT NOT NULL
);
CREATE TABLE agents (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    capabilities_json TEXT NOT NULL DEFAULT '[]',
    api_key_hash TEXT NOT NULL UNIQUE,
    api_key_prefix TEXT NOT NULL,
    tier TEXT NOT NULL DEFAULT 'Unranked' CHECK (tier IN ('Unranked','C','B','A','S')),
    mmr REAL NOT NULL DEFAULT 1000.0,
    placement_completed INTEGER NOT NULL DEFAULT 0,
    subscribe_all_projects INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','disabled','revoked')),
    invite_token_id TEXT REFERENCES invite_tokens(id),
    last_seen_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE agent_subscriptions (
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    PRIMARY KEY (agent_id, project_id)
);
CREATE INDEX bounties_agent_lease ON bounties(lease_agent_id,status,lease_expires_at);
