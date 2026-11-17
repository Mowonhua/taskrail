//! 文件职责：仲裁顺位、回避、裁决与超时顺延；状态、评论、历史和通知共用调用方写事务。
use super::*;

impl Repository {
    /// 函数职责：读取或整体替换全局仲裁链；配置可包含尚未注册的符号 ID。
    pub fn arbitration_config(
        &self,
        update: Option<ArbitrationConfig>,
    ) -> Result<ArbitrationConfig> {
        self.write(|tx| {
            if let Some(update) = update {
                let chain = normalize_chain(update.arbitrator_chain)?;
                tx.execute(
                    "UPDATE server_settings SET arbitrator_chain_json=?1,updated_at=?2 WHERE id=1",
                    params![json!(chain).to_string(), now()],
                )?;
            }
            global_config(tx)
        })
    }

    /// 函数职责：按 fencing 顺序验证执行租约，冻结耗时并锁定首位非执行者的活跃裁判。
    /// 输出说明：没有可用裁判时保留待仲裁状态，由管理员兜底；失败回滚计时、锁与通知。
    pub fn agent_submit_arbitration(
        &self,
        id: &str,
        agent_id: &str,
        input: SubmitArbitrationInput,
    ) -> Result<Bounty> {
        self.write(|tx| {
            agents::active_agent(tx, agent_id)?;
            let timestamp = now();
            let bounty = leases::fenced_bounty(tx, id, agent_id, input.lease_id, &timestamp)?;
            if !bounty.require_arbitration { return Err(invalid_transition()); }
            if input.completion_cost.is_some_and(|cost| !cost.is_finite() || cost < 0.0) {
                return Err(ApiError::invalid("completion_cost 必须为有限非负数"));
            }
            leases::settle_active(tx, &bounty, &timestamp)?;
            let judge = candidates(tx, &bounty)?.into_iter().find(|id| id != agent_id);
            let expires = judge.as_ref().map(|_| leases::deadline());
            let tried: Vec<_> = judge.iter().cloned().collect();
            let note = input.completion_note.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
            tx.execute("UPDATE bounties SET status='PENDING_ARBITRATION',lease_expires_at=NULL,lease_warned_id=0,completion_note=?2,completion_cost=?3,arbitrator_agent_id=?4,arbitration_expires_at=?5,arbitration_tried_ids_json=?6,updated_at=?7 WHERE id=?1",params![id,note,input.completion_cost,judge,expires,json!(tried).to_string(),timestamp])?;
            let updated = find_bounty(tx, id)?;
            let detail = json!({"lease_id":updated.lease_id,"arbitrator_agent_id":judge,"arbitration_expires_at":expires});
            event(tx, &updated, "arbitration_requested", &format!("agent:{agent_id}"), Some(bounty.status), detail.clone())?;
            notify(tx, judge.as_deref(), &updated, "arbitration.requested", detail)?;
            Ok(updated)
        })
    }

    /// 函数职责：当前活跃裁判批准交付；None 仅供已通过管理员鉴权的 HTTP 适配使用。
    /// 约束条件：先检查待仲裁状态，再复核裁判身份和执行者回避；人工 actor 固定为 human。
    pub fn approve_arbitration(
        &self,
        id: &str,
        agent_id: Option<&str>,
        input: ApproveArbitrationInput,
    ) -> Result<Bounty> {
        self.write(|tx| {
            let bounty = find_bounty(tx, id)?;
            let (actor, name) = authorize(tx, &bounty, agent_id)?;
            let timestamp = now();
            let grade = input.quality_grade.unwrap_or(QualityGrade::Perfect);
            tx.execute("UPDATE bounties SET status='COMPLETED',completed_at=?2,quality_grade=?3,calibrated_difficulty=?4,arbitration_expires_at=NULL,lease_expires_at=NULL,updated_at=?2 WHERE id=?1",params![id,timestamp,grade.as_str(),input.calibrated_difficulty.map(Tier::as_str)])?;
            let comment = input.comment.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(|body| comments::insert(tx,id,agent_id,&name,"comment",body,&timestamp)).transpose()?;
            rating::settle(tx, &find_bounty(tx, id)?, RatingOutcome::Completed, true, &timestamp)?;
            let updated = find_bounty(tx, id)?;
            let detail = json!({"lease_id":bounty.lease_id,"quality_grade":grade,"calibrated_difficulty":input.calibrated_difficulty,"comment_id":comment.as_ref().map(|c|c.id)});
            event(tx, &updated, "arbitration_approved", &actor, Some(bounty.status), detail.clone())?;
            notify(tx, bounty.lease_agent_id.as_deref(), &updated, "arbitration.approved", detail)?;
            dependencies::unlock_dependents(tx, id, &timestamp)?;
            Ok(updated)
        })
    }

    /// 函数职责：前三次打回恢复原租约和计时；第四次以结构化失败历史结束本轮交付并重新分发。
    /// 约束条件：保留 lease_id 防止旧请求跨轮写入；评论、失败证据、清理和分发必须原子提交。
    pub fn reject_arbitration(
        &self,
        id: &str,
        agent_id: Option<&str>,
        input: RejectArbitrationInput,
    ) -> Result<Bounty> {
        self.write(|tx| {
            let bounty = find_bounty(tx, id)?;
            let (actor, name) = authorize(tx, &bounty, agent_id)?;
            let body = input.comment.trim();
            if body.is_empty() { return Err(ApiError::invalid("打回评论不能为空")); }
            let timestamp = now();
            let comment = comments::insert(tx,id,agent_id,&name,"comment",body,&timestamp)?;
            let next = bounty.rejection_count + 1;
            let failed = next >= 4;
            if failed {
                let mut failed_bounty = bounty.clone();
                failed_bounty.rejection_count = next;
                rating::settle(tx, &failed_bounty, RatingOutcome::ArbitrationFailed, true, &timestamp)?;
                let fallback = bounty.assignee_mode == AssigneeMode::Specific && bounty.assignee_agent_id == bounty.lease_agent_id;
                let assignment = (bounty.assignee_mode == AssigneeMode::Specific && !fallback).then(leases::deadline);
                tx.execute("UPDATE bounties SET status='OPEN',rejection_count=0,lease_agent_id=NULL,lease_expires_at=NULL,lease_warned_id=0,arbitrator_agent_id=NULL,arbitration_expires_at=NULL,arbitration_tried_ids_json='[]',completion_note=NULL,completion_cost=NULL,active_duration_ms=0,active_started_at=NULL,assignee_mode=CASE WHEN ?2 THEN 'any' ELSE assignee_mode END,assignee_agent_id=CASE WHEN ?2 THEN NULL ELSE assignee_agent_id END,assignee_min_tier=CASE WHEN ?2 THEN NULL ELSE assignee_min_tier END,assignment_expires_at=?3,updated_at=?4 WHERE id=?1", params![id,fallback,assignment,timestamp])?;
            } else {
                tx.execute("UPDATE bounties SET status='IN_PROGRESS',rejection_count=?2,lease_expires_at=?3,lease_warned_id=0,active_started_at=?4,arbitrator_agent_id=NULL,arbitration_expires_at=NULL,arbitration_tried_ids_json='[]',updated_at=?4 WHERE id=?1",params![id,next,leases::deadline(),timestamp])?;
            }
            let updated = find_bounty(tx, id)?;
            let detail = json!({"failed_agent_id":if failed { bounty.lease_agent_id.as_deref() } else { None },"lease_id":bounty.lease_id,"rejection_count":next,"active_duration_ms":bounty.active_duration_ms,"comment_id":comment.id,"comment":body});
            event(tx, &updated, if failed {"arbitration_failed"} else {"arbitration_rejected"}, &actor, Some(bounty.status), detail.clone())?;
            notify(tx, bounty.lease_agent_id.as_deref(), &updated, if failed {"arbitration.failed"} else {"arbitration.rejected"}, detail)?;
            if failed { events::dispatch(tx, &updated, false)?; }
            Ok(updated)
        })
    }
}

/// 函数职责：输入只拒绝空白项并按首次出现顺序去重；ID 原值保留，运行时再按名录筛选。
pub(super) fn normalize_chain(chain: Vec<String>) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for id in chain {
        if id.trim().is_empty() {
            return Err(ApiError::invalid("arbitrator_chain 不能包含空白项"));
        }
        if !result.contains(&id) {
            result.push(id);
        }
    }
    Ok(result)
}

fn global_config(conn: &Connection) -> Result<ArbitrationConfig> {
    Ok(ArbitrationConfig {
        arbitrator_chain: conn.query_row(
            "SELECT arbitrator_chain_json FROM server_settings WHERE id=1",
            [],
            |r| json_value(r, 0),
        )?,
    })
}

/// 项目非空链整体覆盖全局，即使项目链当前没有活跃成员，也不能跨配置补入全局裁判。
fn candidates(conn: &Connection, bounty: &Bounty) -> Result<Vec<String>> {
    let mut chain = find_project(conn, &bounty.project_id)?.arbitrator_chain;
    if chain.is_empty() {
        chain = global_config(conn)?.arbitrator_chain;
    }
    let mut active = Vec::new();
    for id in normalize_chain(chain)? {
        if conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM agents WHERE id=?1 AND status='active')",
            [&id],
            |r| r.get::<_, bool>(0),
        )? {
            active.push(id);
        }
    }
    Ok(active)
}

/// 函数职责：仅当认领者是有效链唯一活跃成员时阻止认领；空链与人工兜底仍允许其他执行者。
pub(super) fn check_claim(conn: &Connection, bounty: &Bounty, agent_id: &str) -> Result<()> {
    if bounty.require_arbitration {
        let active = candidates(conn, bounty)?;
        if active.len() == 1 && active[0] == agent_id {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "ARBITRATOR_CONFLICT",
                "唯一仲裁者不得接取开启仲裁的任务；请增配裁判或由发布者调整为人工/无仲裁",
            ));
        }
    }
    Ok(())
}

fn authorize(
    conn: &Connection,
    bounty: &Bounty,
    agent_id: Option<&str>,
) -> Result<(String, String)> {
    if bounty.status != BountyStatus::PendingArbitration {
        return Err(invalid_transition());
    }
    match agent_id {
        None => Ok(("human".into(), "human".into())),
        Some(id) => {
            let agent = agents::active_agent(conn, id)?;
            if bounty.arbitrator_agent_id.as_deref() != Some(id)
                || bounty.lease_agent_id.as_deref() == Some(id)
            {
                return Err(ApiError::new(
                    StatusCode::FORBIDDEN,
                    "ARBITRATOR_MISMATCH",
                    "Agent 不是当前指定裁判或与执行者身份冲突",
                ));
            }
            Ok((format!("agent:{id}"), agent.name))
        }
    }
}

/// 所有仲裁通知附带变更后的公开任务快照；与历史共用事务并仅向 active 身份投递。
fn notify(
    tx: &Transaction<'_>,
    target: Option<&str>,
    bounty: &Bounty,
    kind: &str,
    mut detail: serde_json::Value,
) -> Result<()> {
    detail["bounty"] = json!(bounty);
    events::notify_active(tx, target, bounty, kind, detail)
}

/// 函数职责：仅顺延到本轮尚未尝试的活跃非执行者；耗尽后清空锁并保持待仲裁供人工处理。
pub(super) fn sweep(tx: &Transaction<'_>, timestamp: &str) -> Result<()> {
    let pending = {
        let mut stmt = tx.prepare(&format!("SELECT {BOUNTY_COLUMNS} FROM {BOUNTY_SOURCE} WHERE b.status='PENDING_ARBITRATION' AND b.arbitrator_agent_id IS NOT NULL AND b.arbitration_expires_at<=?1 ORDER BY b.id"))?;
        let rows = stmt.query_map([timestamp], bounty_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for bounty in pending {
        let mut tried: Vec<String> = tx.query_row(
            "SELECT arbitration_tried_ids_json FROM bounties WHERE id=?1",
            [&bounty.id],
            |r| json_value(r, 0),
        )?;
        let judge = candidates(tx, &bounty)?.into_iter().find(|id| {
            Some(id) != bounty.lease_agent_id.as_ref()
                && !tried.contains(id)
                && Some(id) != bounty.arbitrator_agent_id.as_ref()
        });
        if let Some(old) = &bounty.arbitrator_agent_id {
            if !tried.contains(old) {
                tried.push(old.clone());
            }
        }
        if let Some(id) = &judge {
            tried.push(id.clone());
        }
        let expires = judge.as_ref().map(|_| leases::deadline());
        tx.execute("UPDATE bounties SET arbitrator_agent_id=?2,arbitration_expires_at=?3,arbitration_tried_ids_json=?4,updated_at=?5 WHERE id=?1",params![bounty.id,judge,expires,json!(tried).to_string(),timestamp])?;
        let updated = find_bounty(tx, &bounty.id)?;
        let detail = json!({"lease_id":bounty.lease_id,"previous_arbitrator_agent_id":bounty.arbitrator_agent_id,"arbitrator_agent_id":judge,"arbitration_expires_at":expires});
        event(
            tx,
            &updated,
            if judge.is_some() {
                "arbitration_reassigned"
            } else {
                "arbitration_human_fallback"
            },
            "system",
            Some(bounty.status),
            detail.clone(),
        )?;
        notify(
            tx,
            bounty.arbitrator_agent_id.as_deref(),
            &updated,
            "arbitration.timed_out",
            detail.clone(),
        )?;
        notify(
            tx,
            judge.as_deref(),
            &updated,
            "arbitration.requested",
            detail,
        )?;
    }
    Ok(())
}
