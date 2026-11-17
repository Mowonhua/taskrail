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
ALTER TABLE bounties ADD COLUMN created_by_agent_id TEXT REFERENCES agents(id);
CREATE INDEX IF NOT EXISTS bounties_publisher_agent ON bounties(created_by_agent_id);
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

-- 仲裁响应锁独立于执行租约；失败事故保存在 bounty_events 的结构化历史中。
ALTER TABLE server_settings ADD COLUMN arbitrator_chain_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE bounties ADD COLUMN arbitrator_agent_id TEXT;
ALTER TABLE bounties ADD COLUMN arbitration_expires_at TEXT;
ALTER TABLE bounties ADD COLUMN arbitration_tried_ids_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE bounties ADD COLUMN quality_grade TEXT CHECK (quality_grade IN ('perfect','flawed'));
ALTER TABLE bounties ADD COLUMN calibrated_difficulty TEXT CHECK (calibrated_difficulty IN ('C','B','A','S'));
CREATE INDEX bounties_arbitration_deadline ON bounties(status,arbitration_expires_at);
ALTER TABLE agents ADD COLUMN settled_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agents ADD COLUMN completed_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agents ADD COLUMN failed_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agents ADD COLUMN verified_as_wins INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agents ADD COLUMN verified_s_wins INTEGER NOT NULL DEFAULT 0;
CREATE TABLE agent_rating_ledger (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    bounty_id TEXT NOT NULL REFERENCES bounties(id) ON DELETE CASCADE,
    bounty_title TEXT NOT NULL,
    project_id TEXT NOT NULL,
    lease_id INTEGER NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('completed','arbitration_failed','timeout','abandon')),
    arbitrated INTEGER NOT NULL DEFAULT 0,
    self_dealt INTEGER NOT NULL DEFAULT 0,
    original_difficulty TEXT NOT NULL CHECK (original_difficulty IN ('C','B','A','S')),
    effective_difficulty TEXT NOT NULL CHECK (effective_difficulty IN ('C','B','A','S')),
    quality_grade TEXT CHECK (quality_grade IN ('perfect','flawed')),
    rejection_count INTEGER NOT NULL DEFAULT 0,
    quality_score REAL NOT NULL,
    active_duration_ms INTEGER NOT NULL,
    time_efficiency REAL NOT NULL,
    completion_cost REAL,
    cost_efficiency REAL NOT NULL,
    actual_score REAL NOT NULL,
    expected_score REAL NOT NULL,
    k_factor REAL NOT NULL,
    is_placement INTEGER NOT NULL DEFAULT 0,
    raw_delta REAL NOT NULL,
    applied_delta REAL NOT NULL,
    mmr_before REAL NOT NULL,
    mmr_after REAL NOT NULL,
    tier_before TEXT NOT NULL CHECK (tier_before IN ('Unranked','C','B','A','S')),
    tier_after TEXT NOT NULL CHECK (tier_after IN ('Unranked','C','B','A','S')),
    cap_reason TEXT,
    tier_gate_reason TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX agent_rating_ledger_agent_order ON agent_rating_ledger(agent_id, id DESC);

ALTER TABLE bounties ADD COLUMN draft_collaborator_agent_id TEXT REFERENCES agents(id);
ALTER TABLE bounties ADD COLUMN draft_needs_human_review INTEGER NOT NULL DEFAULT 0;
ALTER TABLE bounties ADD COLUMN parent_bounty_id TEXT REFERENCES bounties(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS bounties_parent ON bounties(parent_bounty_id);
CREATE INDEX IF NOT EXISTS bounties_collaborator ON bounties(draft_collaborator_agent_id);
CREATE TABLE bounty_dependencies (
    bounty_id TEXT NOT NULL REFERENCES bounties(id) ON DELETE CASCADE,
    depends_on_bounty_id TEXT NOT NULL REFERENCES bounties(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    PRIMARY KEY (bounty_id, depends_on_bounty_id),
    CHECK (bounty_id <> depends_on_bounty_id)
);
CREATE INDEX bounty_dependencies_reverse ON bounty_dependencies(depends_on_bounty_id, bounty_id);
