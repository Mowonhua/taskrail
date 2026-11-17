//! 文件职责：执行者准入、租约令牌校验、交付与到期回收；状态和事件共用仓储写事务。
use super::*;

impl Repository {
    /// 函数职责：在写锁内复核身份与准入，原子签发下一轮租约；订阅只影响推送，不限制抢单。
    pub fn agent_claim_bounty(&self, id: &str, agent_id: &str) -> Result<Bounty> {
        self.write(|tx| {
            let agent = agents::active_agent(tx, agent_id)?;
            let bounty = published_bounty(tx, id)?;
            claimable(&bounty)?;
            if bounty.assignee_mode == AssigneeMode::Specific
                && bounty.assignee_agent_id.as_deref() != Some(agent_id)
            {
                return Err(assignee_mismatch());
            }
            if bounty.assignee_mode == AssigneeMode::MinTier {
                let rank = match agent.tier {
                    AgentTier::S => 4,
                    AgentTier::A => 3,
                    AgentTier::B => 2,
                    AgentTier::C | AgentTier::Unranked => 1,
                };
                let minimum = match bounty.assignee_min_tier {
                    Some(Tier::S) => 4,
                    Some(Tier::A) => 3,
                    Some(Tier::B) => 2,
                    _ => 1,
                };
                if rank < minimum {
                    return Err(ApiError::new(
                        StatusCode::FORBIDDEN,
                        "TIER_INSUFFICIENT",
                        "Agent 段位不足",
                    ));
                }
            }
            claim(tx, &bounty, agent_id)
        })
    }

    /// 函数职责：仅被指派者可婉拒公开待接任务；回退公开、通知发布者与重新分发原子提交。
    pub fn agent_decline_bounty(
        &self,
        id: &str,
        agent_id: &str,
        input: DeclineBounty,
    ) -> Result<Bounty> {
        self.write(|tx| {
            let agent = agents::active_agent(tx, agent_id)?;
            let bounty = published_bounty(tx, id)?;
            if bounty.assignee_mode != AssigneeMode::Specific || bounty.status != BountyStatus::Open {
                return Err(invalid_transition());
            }
            if bounty.assignee_agent_id.as_deref() != Some(agent_id) {
                return Err(assignee_mismatch());
            }
            clear_assignment(tx, id, &now())?;
            let updated = find_bounty(tx, id)?;
            let detail = json!({"declined_by_agent_id":agent_id,"declined_by_agent_name":agent.name,"reason":input.reason});
            event(tx, &updated, "declined", &format!("agent:{agent_id}"), Some(bounty.status), detail.clone())?;
            events::notify_active(tx, updated.created_by_agent_id.as_deref(), &updated, "task.assignment_declined", detail)?;
            events::dispatch(tx, &updated, false)?;
            Ok(updated)
        })
    }

    /// 函数职责：持有者在到期前凭当前令牌续期十五分钟，并允许本轮再次收到临期提醒。
    pub fn agent_heartbeat_bounty(
        &self,
        id: &str,
        agent_id: &str,
        input: LeaseHeartbeat,
    ) -> Result<Bounty> {
        self.write(|tx| {
            agents::active_agent(tx, agent_id)?;
            let timestamp = now();
            fenced_bounty(tx, id, agent_id, input.lease_id, &timestamp)?;
            tx.execute("UPDATE bounties SET lease_expires_at=?2,lease_warned_id=0,updated_at=?3 WHERE id=?1", params![id,deadline(),timestamp])?;
            find_bounty(tx, id)
        })
    }

    /// 函数职责：仅有效租约可主动弃单；保留令牌，清除持有者，累计一次弃单并重新分发。
    pub fn agent_abandon_bounty(
        &self,
        id: &str,
        agent_id: &str,
        input: AbandonBounty,
    ) -> Result<Bounty> {
        self.write(|tx| {
            agents::active_agent(tx, agent_id)?;
            let timestamp = now();
            let bounty = fenced_bounty(tx, id, agent_id, input.lease_id, &timestamp)?;
            let updated = reopen(tx, &bounty, &timestamp)?;
            incident(tx, &bounty, "abandon", &timestamp)?;
            event(
                tx,
                &updated,
                "abandoned",
                &format!("agent:{agent_id}"),
                Some(bounty.status),
                json!({"lease_id":bounty.lease_id,"lease_agent_id":agent_id,"reason":input.reason}),
            )?;
            events::dispatch(tx, &updated, false)?;
            Ok(updated)
        })
    }

    /// 函数职责：有效租约持有者直接交付未启用仲裁的任务；完成后保留执行者与令牌供追溯。
    pub fn agent_complete_bounty(
        &self,
        id: &str,
        agent_id: &str,
        input: CompleteBounty,
    ) -> Result<Bounty> {
        self.write(|tx| {
            agents::active_agent(tx, agent_id)?;
            let timestamp = now();
            let bounty = fenced_bounty(tx, id, agent_id, input.lease_id, &timestamp)?;
            if bounty.require_arbitration {
                return Err(ApiError::conflict("ARBITRATION_REQUIRED", "任务需要仲裁，不能直接完成"));
            }
            if input.completion_cost.is_some_and(|cost| !cost.is_finite() || cost < 0.0) {
                return Err(ApiError::invalid("completion_cost 必须为有限非负数"));
            }
            let note = input.completion_note.map(|note| note.trim().to_owned()).filter(|note| !note.is_empty());
            tx.execute("UPDATE bounties SET status='COMPLETED',completion_note=?2,completion_cost=?3,completed_at=?4,lease_expires_at=NULL,lease_warned_id=0,updated_at=?4 WHERE id=?1", params![id,note,input.completion_cost,timestamp])?;
            let updated = find_bounty(tx, id)?;
            event(tx, &updated, "completed", &format!("agent:{agent_id}"), Some(bounty.status), json!({"lease_id":bounty.lease_id,"lease_agent_id":agent_id,"lease_agent_name":bounty.lease_agent_name,"completion_note":updated.completion_note,"completion_cost":updated.completion_cost}))?;
            Ok(updated)
        })
    }

    /// 函数职责：人工释放执行中或持有租约的挂起任务；不记超时/弃单，定向任务重新计响应窗口。
    pub fn admin_release_lease(&self, id: &str) -> Result<Bounty> {
        self.write(|tx| {
            let bounty = find_bounty(tx, id)?;
            if bounty.status != BountyStatus::InProgress
                && !(matches!(bounty.status, BountyStatus::WaitingInfo | BountyStatus::PendingArbitration) && bounty.lease_agent_id.is_some())
            {
                return Err(invalid_transition());
            }
            let timestamp = now();
            let assignment = (bounty.assignee_mode == AssigneeMode::Specific).then(deadline);
            tx.execute("UPDATE bounties SET status='OPEN',lease_agent_id=NULL,lease_expires_at=NULL,lease_warned_id=0,assignment_expires_at=?2,updated_at=?3 WHERE id=?1", params![id,assignment,timestamp])?;
            let updated = find_bounty(tx, id)?;
            let detail = json!({"lease_id":bounty.lease_id,"lease_agent_id":bounty.lease_agent_id,"lease_agent_name":bounty.lease_agent_name});
            event(tx, &updated, "lease_released", "human", Some(bounty.status), detail.clone())?;
            events::notify_active(tx, bounty.lease_agent_id.as_deref(), &updated, "lease.released", detail)?;
            events::dispatch(tx, &updated, false)?;
            Ok(updated)
        })
    }

    /// 函数职责：扫描临期、过期租约及过期指派；一次扫描共享写锁和时间快照，重复调用不重复结算。
    /// 约束条件：仅 IN_PROGRESS 消耗租约；WAITING_INFO/PENDING_ARBITRATION 不参与到期回收。
    pub fn sweep_leases(&self) -> Result<()> {
        self.write(|tx| {
            let timestamp = now();
            let warning_until = (Utc::now() + chrono::Duration::minutes(3)).to_rfc3339_opts(SecondsFormat::Millis, true);
            let candidates = {
                let mut stmt = tx.prepare(&format!("SELECT {BOUNTY_COLUMNS} FROM {BOUNTY_SOURCE} WHERE (b.status='IN_PROGRESS' AND b.lease_expires_at<=?2 AND (b.lease_expires_at<=?1 OR b.lease_warned_id<b.lease_id)) OR (b.status='OPEN' AND b.assignee_mode='specific' AND b.assignment_expires_at<=?1) ORDER BY b.id"))?;
                let rows = stmt.query_map(params![timestamp,warning_until], bounty_row)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };
            for bounty in candidates {
                if bounty.status == BountyStatus::Open {
                    clear_assignment(tx, &bounty.id, &timestamp)?;
                    let updated = find_bounty(tx, &bounty.id)?;
                    let detail = json!({"assignee_agent_id":bounty.assignee_agent_id,"assignee_agent_name":bounty.assignee_agent_name});
                    event(tx, &updated, "assignment_expired", "system", Some(bounty.status), detail.clone())?;
                    events::notify_active(tx, bounty.created_by_agent_id.as_deref(), &updated, "task.assignment_expired", detail)?;
                    events::dispatch(tx, &updated, false)?;
                } else if bounty.lease_expires_at.as_deref().is_some_and(|expires| expires <= timestamp.as_str()) {
                    let updated = reopen(tx, &bounty, &timestamp)?;
                    incident(tx, &bounty, "timeout", &timestamp)?;
                    let detail = lease_detail(&bounty);
                    event(tx, &updated, "lease_expired", "system", Some(bounty.status), detail.clone())?;
                    events::notify_active(tx, bounty.lease_agent_id.as_deref(), &updated, "lease.expired", detail)?;
                    events::dispatch(tx, &updated, false)?;
                } else {
                    tx.execute("UPDATE bounties SET lease_warned_id=lease_id WHERE id=?1", [&bounty.id])?;
                    events::notify_active(tx, bounty.lease_agent_id.as_deref(), &bounty, "lease.expiring", lease_detail(&bounty))?;
                }
            }
            Ok(())
        })
    }
}

pub(super) fn deadline() -> String {
    (Utc::now() + chrono::Duration::minutes(15)).to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn published_bounty(conn: &Connection, id: &str) -> Result<Bounty> {
    let bounty = find_bounty(conn, id)?;
    if bounty.status == BountyStatus::Draft || bounty.published_at.is_none() {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "BOUNTY_NOT_FOUND",
            "悬赏不存在",
        ));
    }
    Ok(bounty)
}

fn claimable(bounty: &Bounty) -> Result<()> {
    if bounty.status == BountyStatus::InProgress || bounty.lease_agent_id.is_some() {
        return Err(already_claimed());
    }
    if bounty.status != BountyStatus::Open {
        return Err(invalid_transition());
    }
    Ok(())
}

/// 旧仓储入口允许未注册身份；HTTP 入口必须先在同一事务内完成 active_agent 与准入检查。
pub(super) fn claim(tx: &Transaction<'_>, bounty: &Bounty, agent_id: &str) -> Result<Bounty> {
    claimable(bounty)?;
    let timestamp = now();
    let changed = tx.execute("UPDATE bounties SET status='IN_PROGRESS',lease_id=lease_id+1,lease_agent_id=?2,lease_expires_at=?3,lease_warned_id=0,assignment_expires_at=NULL,updated_at=?4 WHERE id=?1 AND status='OPEN' AND lease_agent_id IS NULL AND lease_id=?5", params![bounty.id,agent_id,deadline(),timestamp,bounty.lease_id])?;
    if changed != 1 {
        return Err(already_claimed());
    }
    let updated = find_bounty(tx, &bounty.id)?;
    event(
        tx,
        &updated,
        "claimed",
        &format!("agent:{agent_id}"),
        Some(bounty.status),
        lease_detail(&updated),
    )?;
    Ok(updated)
}

/// 校验顺序属于错误契约：先隐藏未公开任务，再拒绝旧令牌、错误持有者、非执行状态及到期租约。
fn fenced_bounty(
    conn: &Connection,
    id: &str,
    agent_id: &str,
    lease_id: i64,
    timestamp: &str,
) -> Result<Bounty> {
    let bounty = published_bounty(conn, id)?;
    if bounty.lease_id != lease_id || bounty.lease_agent_id.is_none() {
        return Err(stale_lease());
    }
    if bounty.lease_agent_id.as_deref() != Some(agent_id) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "LEASE_OWNER_MISMATCH",
            "Agent 不是租约持有者",
        ));
    }
    if bounty.status != BountyStatus::InProgress {
        return Err(invalid_transition());
    }
    if bounty
        .lease_expires_at
        .as_deref()
        .is_none_or(|expires| expires <= timestamp)
    {
        return Err(stale_lease());
    }
    Ok(bounty)
}

fn assignee_mismatch() -> ApiError {
    ApiError::new(
        StatusCode::FORBIDDEN,
        "ASSIGNEE_MISMATCH",
        "Agent 不是被指派者",
    )
}

fn stale_lease() -> ApiError {
    ApiError::conflict("STALE_LEASE", "租约令牌已失效或租约已到期")
}

fn lease_detail(bounty: &Bounty) -> serde_json::Value {
    json!({"lease_id":bounty.lease_id,"lease_agent_id":bounty.lease_agent_id,"lease_agent_name":bounty.lease_agent_name,"lease_agent_tier":bounty.lease_agent_tier,"lease_expires_at":bounty.lease_expires_at})
}

fn clear_assignment(tx: &Transaction<'_>, id: &str, timestamp: &str) -> Result<()> {
    tx.execute("UPDATE bounties SET assignee_mode='any',assignee_agent_id=NULL,assignee_min_tier=NULL,assignment_expires_at=NULL,updated_at=?2 WHERE id=?1", params![id,timestamp])?;
    Ok(())
}

/// 弃单和超时共用回收规则；令牌留存到下次接取递增，定向给原持有者的任务回退为公开。
fn reopen(tx: &Transaction<'_>, bounty: &Bounty, timestamp: &str) -> Result<Bounty> {
    let fallback = bounty.assignee_mode == AssigneeMode::Specific
        && bounty.assignee_agent_id == bounty.lease_agent_id;
    tx.execute("UPDATE bounties SET status='OPEN',lease_agent_id=NULL,lease_expires_at=NULL,lease_warned_id=0,assignee_mode=CASE WHEN ?3 THEN 'any' ELSE assignee_mode END,assignee_agent_id=CASE WHEN ?3 THEN NULL ELSE assignee_agent_id END,assignee_min_tier=CASE WHEN ?3 THEN NULL ELSE assignee_min_tier END,assignment_expires_at=CASE WHEN ?3 THEN NULL ELSE assignment_expires_at END,updated_at=?2 WHERE id=?1", params![bounty.id,timestamp,fallback])?;
    find_bounty(tx, &bounty.id)
}

fn incident(tx: &Transaction<'_>, bounty: &Bounty, kind: &str, timestamp: &str) -> Result<()> {
    // 兼容旧仓储接口签发给未注册身份的租约；实际 Agent 租约始终存在对应名录记录。
    let Some(agent_id) = bounty.lease_agent_id.as_deref() else {
        return Ok(());
    };
    let column = if kind == "timeout" {
        "timeout_count"
    } else {
        "abandon_count"
    };
    let changed = tx.execute(
        &format!("UPDATE agents SET {column}={column}+1,updated_at=?2 WHERE id=?1"),
        params![agent_id, timestamp],
    )?;
    if changed == 1 {
        tx.execute("INSERT INTO lease_incidents(bounty_id,agent_id,lease_id,kind,created_at) VALUES(?1,?2,?3,?4,?5)", params![bounty.id,agent_id,bounty.lease_id,kind,timestamp])?;
    }
    Ok(())
}
