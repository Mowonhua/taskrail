//! 文件职责：评论与提问挂起：评论、状态、历史和收件箱必须原子提交。
use super::*;

impl Repository {
    /// 函数职责：在同一读快照校验任务存在并读取评论；草稿对管理员可见。
    pub fn bounty_comments(&self, id: &str) -> Result<Vec<BountyComment>> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        find_bounty(&tx, id)?;
        let comments = list(&tx, id)?;
        tx.commit()?;
        Ok(comments)
    }

    /// 函数职责：同一读快照内复核活跃身份、任务可见性和评论，拒绝时不返回正文。
    pub fn agent_bounty_comments(&self, id: &str, agent_id: &str) -> Result<Vec<BountyComment>> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        visible_bounty(&tx, id, agent_id)?;
        let comments = list(&tx, id)?;
        tx.commit()?;
        Ok(comments)
    }

    /// 函数职责：以人工身份评论；回复挂起任务时，评论、恢复和双通知原子提交。
    pub fn create_bounty_comment(
        &self,
        id: &str,
        input: CreateBountyComment,
    ) -> Result<BountyCommentResult> {
        self.write(|tx| {
            let bounty = find_bounty(tx, id)?;
            add(tx, &bounty, None, "human", &input.body)
        })
    }

    /// 函数职责：写锁内复核活跃身份与可见性；仅非执行者的发布者回复可恢复任务。
    pub fn agent_create_comment(
        &self,
        id: &str,
        agent_id: &str,
        input: CreateBountyComment,
    ) -> Result<BountyCommentResult> {
        self.write(|tx| {
            let (bounty, agent) = visible_bounty(tx, id, agent_id)?;
            add(tx, &bounty, Some(agent_id), &agent.name, &input.body)
        })
    }

    /// 函数职责：冻结当前租约并保存提问和发布者通知。
    /// 约束条件：fencing 先于正文校验，严格复用心跳的错误优先级；冻结保留当前令牌及持有者。
    pub fn agent_request_info(
        &self,
        id: &str,
        agent_id: &str,
        input: RequestInfoInput,
    ) -> Result<Bounty> {
        self.write(|tx| {
            let agent = agents::active_agent(tx, agent_id)?;
            let timestamp = now();
            let bounty = leases::fenced_bounty(tx, id, agent_id, input.lease_id, &timestamp)?;
            let body = nonempty(&input.question)?;
            leases::settle_active(tx, &bounty, &timestamp)?;
            tx.execute("UPDATE bounties SET status='WAITING_INFO',lease_expires_at=NULL,lease_warned_id=0,updated_at=?2 WHERE id=?1", params![id,timestamp])?;
            let comment = insert(tx, id, Some(agent_id), &agent.name, "question", body, &timestamp)?;
            let updated = find_bounty(tx, id)?;
            let detail = json!({"lease_id":updated.lease_id,"comment_id":comment.id,"question":body});
            event(tx, &updated, "info_requested", &format!("agent:{agent_id}"), Some(bounty.status), detail)?;
            if updated.created_by_agent_id.as_deref() != Some(agent_id) {
                events::notify_active(tx, updated.created_by_agent_id.as_deref(), &updated, "task.info_requested", payload(&updated, &comment))?;
            }
            Ok(updated)
        })
    }
}

fn visible_bounty(conn: &Connection, id: &str, agent_id: &str) -> Result<(Bounty, AgentSummary)> {
    let agent = agents::active_agent(conn, agent_id)?;
    let bounty = find_bounty(conn, id)?;
    if !agents::visible(&agent, &bounty) {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "BOUNTY_NOT_FOUND",
            "悬赏不存在",
        ));
    }
    Ok((bounty, agent))
}

/// 函数职责：复用调用方快照按 ID 读取评论；Agent 名称缺失时回退写入名称。
pub(super) fn list(conn: &Connection, id: &str) -> Result<Vec<BountyComment>> {
    let mut stmt = conn.prepare("SELECT c.id,c.bounty_id,c.author_type,c.author_id,COALESCE(a.name,c.author_name),c.kind,c.body,c.created_at FROM bounty_comments c LEFT JOIN agents a ON c.author_type='agent' AND a.id=c.author_id WHERE c.bounty_id=?1 ORDER BY c.id")?;
    let rows = stmt.query_map([id], |r| {
        Ok(BountyComment {
            id: r.get(0)?,
            bounty_id: r.get(1)?,
            author_type: r.get(2)?,
            author_id: r.get(3)?,
            author_name: r.get(4)?,
            kind: r.get(5)?,
            body: r.get(6)?,
            created_at: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn nonempty(body: &str) -> Result<&str> {
    let body = body.trim();
    if body.is_empty() {
        return Err(ApiError::invalid("评论或问题正文不能为空"));
    }
    Ok(body)
}

fn insert(
    tx: &Transaction<'_>,
    id: &str,
    author_id: Option<&str>,
    name: &str,
    kind: &str,
    body: &str,
    timestamp: &str,
) -> Result<BountyComment> {
    let author_type = if author_id.is_some() {
        "agent"
    } else {
        "human"
    };
    tx.execute("INSERT INTO bounty_comments(bounty_id,author_type,author_id,author_name,kind,body,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![id,author_type,author_id,name,kind,body,timestamp])?;
    Ok(BountyComment {
        id: tx.last_insert_rowid(),
        bounty_id: id.into(),
        author_type: author_type.into(),
        author_id: author_id.map(str::to_owned),
        author_name: name.into(),
        kind: kind.into(),
        body: body.into(),
        created_at: timestamp.into(),
    })
}

/// 只有管理员或非当前执行者的发布者可以回复恢复；执行者补充评论始终不改变冻结状态。
fn add(
    tx: &Transaction<'_>,
    bounty: &Bounty,
    author_id: Option<&str>,
    name: &str,
    body: &str,
) -> Result<BountyCommentResult> {
    if bounty.status == BountyStatus::Cancelled {
        return Err(invalid_transition());
    }
    let body = nonempty(body)?;
    let resume = bounty.status == BountyStatus::WaitingInfo
        && (author_id.is_none()
            || (author_id == bounty.created_by_agent_id.as_deref()
                && author_id != bounty.lease_agent_id.as_deref()));
    let timestamp = now();
    let comment = insert(
        tx,
        &bounty.id,
        author_id,
        name,
        if resume { "reply" } else { "comment" },
        body,
        &timestamp,
    )?;
    if resume {
        tx.execute("UPDATE bounties SET status='IN_PROGRESS',lease_expires_at=?2,lease_warned_id=0,active_started_at=?3,updated_at=?3 WHERE id=?1", params![bounty.id,leases::deadline(),timestamp])?;
    } else {
        tx.execute(
            "UPDATE bounties SET updated_at=?2 WHERE id=?1",
            params![bounty.id, timestamp],
        )?;
    }
    let updated = find_bounty(tx, &bounty.id)?;
    let actor = author_id.map_or_else(|| "human".into(), |id| format!("agent:{id}"));
    let detail = if resume {
        json!({"lease_id":updated.lease_id,"lease_agent_id":updated.lease_agent_id,"comment_id":comment.id,"lease_expires_at":updated.lease_expires_at})
    } else {
        json!({"comment_id":comment.id})
    };
    event(
        tx,
        &updated,
        if resume { "resumed" } else { "commented" },
        &actor,
        Some(bounty.status),
        detail,
    )?;
    let data = payload(&updated, &comment);
    if resume {
        for kind in ["comment.replied", "task.resumed"] {
            events::notify_active(
                tx,
                updated.lease_agent_id.as_deref(),
                &updated,
                kind,
                data.clone(),
            )?;
        }
    } else if updated.status != BountyStatus::Draft && updated.published_at.is_some() {
        // 草稿评论只留在管理员时间线，不能通过通知绕过 Agent 草稿隔离。
        // 发布者与执行者可能相同；同一身份每条评论最多收到一条通知，并排除评论者自己。
        let mut targets = std::collections::BTreeSet::new();
        for target in [
            updated.lease_agent_id.as_deref(),
            updated.created_by_agent_id.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if Some(target) != author_id {
                targets.insert(target);
            }
        }
        for target in targets {
            events::notify_active(tx, Some(target), &updated, "comment.added", data.clone())?;
        }
    }
    Ok(BountyCommentResult {
        bounty: updated,
        comment,
    })
}

/// 所有通道使用同一持久化快照；只序列化公开 DTO，保留回复正文和恢复后的租约信息供唤醒。
fn payload(bounty: &Bounty, comment: &BountyComment) -> serde_json::Value {
    json!({"bounty":bounty,"comment":comment,"comment_id":comment.id,"body":comment.body,"lease_id":bounty.lease_id,"lease_expires_at":bounty.lease_expires_at})
}
