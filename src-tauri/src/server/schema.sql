CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    icon TEXT,
    description TEXT NOT NULL DEFAULT '',
    workspace_path TEXT NOT NULL DEFAULT '',
    default_acceptance_criteria TEXT NOT NULL DEFAULT '',
    agent_draft_policy TEXT NOT NULL DEFAULT 'open',
    arbitrator_chain_json TEXT NOT NULL DEFAULT '[]',
    archived INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE bounties (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    acceptance_criteria TEXT NOT NULL DEFAULT '',
    difficulty TEXT NOT NULL CHECK (difficulty IN ('C','B','A','S')),
    assignee_mode TEXT NOT NULL CHECK (assignee_mode IN ('any','min_tier','specific')),
    assignee_min_tier TEXT CHECK (assignee_min_tier IN ('C','B','A','S')),
    assignee_agent_id TEXT,
    require_arbitration INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL CHECK (status IN ('DRAFT','OPEN','IN_PROGRESS','WAITING_INFO','PENDING_ARBITRATION','COMPLETED','CANCELLED')),
    created_by TEXT NOT NULL DEFAULT 'human',
    lease_id INTEGER NOT NULL DEFAULT 0,
    lease_agent_id TEXT,
    lease_expires_at TEXT,
    rejection_count INTEGER NOT NULL DEFAULT 0,
    completion_note TEXT,
    published_at TEXT,
    withdrawn_at TEXT,
    cancelled_at TEXT,
    completed_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE bounty_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bounty_id TEXT NOT NULL REFERENCES bounties(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    actor TEXT NOT NULL,
    from_status TEXT,
    to_status TEXT NOT NULL,
    detail_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);
CREATE INDEX bounties_project_status ON bounties(project_id, status);
CREATE INDEX bounty_events_order ON bounty_events(bounty_id, id);
