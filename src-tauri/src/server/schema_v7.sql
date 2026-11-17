-- 仲裁响应锁独立于执行租约；失败事故保存在 bounty_events 的结构化历史中。
ALTER TABLE server_settings ADD COLUMN arbitrator_chain_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE bounties ADD COLUMN arbitrator_agent_id TEXT;
ALTER TABLE bounties ADD COLUMN arbitration_expires_at TEXT;
ALTER TABLE bounties ADD COLUMN arbitration_tried_ids_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE bounties ADD COLUMN quality_grade TEXT CHECK (quality_grade IN ('perfect','flawed'));
ALTER TABLE bounties ADD COLUMN calibrated_difficulty TEXT CHECK (calibrated_difficulty IN ('C','B','A','S'));
CREATE INDEX bounties_arbitration_deadline ON bounties(status,arbitration_expires_at);
