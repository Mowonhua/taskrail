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
