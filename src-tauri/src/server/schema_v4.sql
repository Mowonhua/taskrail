ALTER TABLE bounties ADD COLUMN created_by_agent_id TEXT REFERENCES agents(id);
CREATE INDEX IF NOT EXISTS bounties_publisher_agent ON bounties(created_by_agent_id);
