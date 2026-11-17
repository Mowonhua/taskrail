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

