//! 文件职责：依赖图校验、关联摘要、Agent 视角脱敏和完成解锁；复用调用方事务。
use super::*;

/// 函数职责：读取稳定依赖顺序及当前状态；调用方须使用同一快照读取任务。
pub(super) fn hydrate(conn: &Connection, bounty: &mut Bounty) -> Result<()> {
    let mut stmt = conn.prepare("SELECT b.id,b.title,b.status FROM bounty_dependencies d JOIN bounties b ON b.id=d.depends_on_bounty_id WHERE d.bounty_id=?1 ORDER BY d.rowid")?;
    bounty.blocked_by_summaries = stmt
        .query_map([&bounty.id], summary_row)?
        .collect::<rusqlite::Result<_>>()?;
    bounty.blocked_by = bounty
        .blocked_by_summaries
        .iter()
        .map(|b| b.id.clone())
        .collect();
    bounty.is_blocked = bounty
        .blocked_by_summaries
        .iter()
        .any(|b| b.status != BountyStatus::Completed);
    Ok(())
}

fn summary_row(r: &Row<'_>) -> rusqlite::Result<BountyDependencySummary> {
    Ok(BountyDependencySummary {
        id: r.get(0)?,
        title: r.get(1)?,
        status: enum_value(r, 2)?,
    })
}

/// 函数职责：读取下游或派生摘要，按创建时间和 ID 排序；管理员保留全部标题。
pub(super) fn related(
    conn: &Connection,
    id: &str,
    children: bool,
) -> Result<Vec<BountyDependencySummary>> {
    let query = if children {
        "SELECT id,title,status FROM bounties WHERE parent_bounty_id=?1 ORDER BY created_at,id"
    } else {
        "SELECT b.id,b.title,b.status FROM bounty_dependencies d JOIN bounties b ON b.id=d.bounty_id WHERE d.depends_on_bounty_id=?1 ORDER BY b.created_at,b.id"
    };
    let mut stmt = conn.prepare(query)?;
    let rows = stmt
        .query_map([id], summary_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// 函数职责：ID 拒绝空白、保留原值并按首次出现去重。
pub(super) fn normalize(ids: Vec<String>) -> Result<Vec<String>> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for id in ids {
        if id.trim().is_empty() {
            return Err(ApiError::invalid("任务 ID 不能包含空白项"));
        }
        if seen.insert(id.clone()) {
            result.push(id);
        }
    }
    Ok(result)
}

fn cycle() -> ApiError {
    ApiError::conflict(
        "DEPENDENCY_CYCLE",
        "存在自依赖或循环依赖，违反有向无环图约束",
    )
}

/// 函数职责：整体替换依赖并结合数据库已有边验证全图无环，失败由外层事务回滚。
/// 错误模式：先按声明顺序校验可见性和取消状态，再写边和检测环；自依赖在 CHECK 前返回领域错误。
pub(super) fn replace(
    tx: &Transaction<'_>,
    id: &str,
    ids: Vec<String>,
    agent_id: Option<&str>,
    timestamp: &str,
) -> Result<()> {
    let ids = normalize(ids)?;
    let agent = agent_id.map(|a| agents::active_agent(tx, a)).transpose()?;
    for target in &ids {
        if target == id {
            return Err(cycle());
        }
        let bounty = find_bounty(tx, target)?;
        // 可见性先于状态检查，避免通过取消状态的错误码探测隐藏任务。
        if agent.as_ref().is_some_and(|a| !agents::visible(a, &bounty)) {
            return Err(ApiError::new(
                StatusCode::NOT_FOUND,
                "BOUNTY_NOT_FOUND",
                "任务不存在",
            ));
        }
        if bounty.status == BountyStatus::Cancelled {
            return Err(invalid_transition());
        }
    }
    tx.execute("DELETE FROM bounty_dependencies WHERE bounty_id=?1", [id])?;
    for target in ids {
        tx.execute("INSERT INTO bounty_dependencies(bounty_id,depends_on_bounty_id,created_at) VALUES(?1,?2,?3)", params![id,target,timestamp])?;
    }
    // UNION 去重可达点对，既避免递归深度依赖 Rust 栈，也保证遇到环时查询仍可终止。
    let cyclic: bool = tx.query_row("WITH RECURSIVE reach(root,node) AS (SELECT bounty_id,depends_on_bounty_id FROM bounty_dependencies UNION SELECT r.root,d.depends_on_bounty_id FROM reach r JOIN bounty_dependencies d ON d.bounty_id=r.node) SELECT EXISTS(SELECT 1 FROM reach WHERE root=node)", [], |r| r.get(0))?;
    if cyclic {
        return Err(cycle());
    }
    Ok(())
}

/// 函数职责：按接收者隐藏不可见前置任务的标题；ID、状态和阻塞计算保持一致。
pub(super) fn redact(conn: &Connection, bounty: &mut Bounty, agent_id: &str) -> Result<()> {
    if !bounty.blocked_by_summaries.is_empty() {
        // 通知先脱敏再过滤 active 接收者，停用或吊销身份不能使业务事务失败。
        let agent = agents::find_agent(conn, agent_id)?;
        for summary in &mut bounty.blocked_by_summaries {
            let prereq = find_bounty(conn, &summary.id)?;
            if !agents::visible(&agent, &prereq) {
                summary.title = "[草稿前置任务]".into();
            }
        }
    }
    Ok(())
}

/// 函数职责：Agent 详情隐藏不可见的下游与派生摘要，前置摘要只脱敏标题。
pub(super) fn redact_detail(
    conn: &Connection,
    detail: &mut BountyDetail,
    agent_id: &str,
) -> Result<()> {
    redact(conn, &mut detail.bounty, agent_id)?;
    let agent = agents::active_agent(conn, agent_id)?;
    let mut blocks = Vec::new();
    for summary in detail.blocks.drain(..) {
        if agents::visible(&agent, &find_bounty(conn, &summary.id)?) {
            blocks.push(summary);
        }
    }
    detail.blocks = blocks;
    let mut children = Vec::new();
    for summary in detail.child_bounties.drain(..) {
        if agents::visible(&agent, &find_bounty(conn, &summary.id)?) {
            children.push(summary);
        }
    }
    detail.child_bounties = children;
    Ok(())
}

pub(super) fn blocked() -> ApiError {
    ApiError::conflict("BOUNTY_BLOCKED", "前置依赖任务尚未全部完成，暂不可抢单")
}

/// 函数职责：发布后仅无阻塞任务启动定向窗口并分发；调用方已完成状态写入。
pub(super) fn dispatch_ready(tx: &Transaction<'_>, id: &str, timestamp: &str) -> Result<Bounty> {
    let mut bounty = find_bounty(tx, id)?;
    if bounty.status != BountyStatus::Open {
        return Err(invalid_transition());
    }
    if !bounty.is_blocked {
        if bounty.assignee_mode == AssigneeMode::Specific {
            tx.execute(
                "UPDATE bounties SET assignment_expires_at=?2,updated_at=?3 WHERE id=?1",
                params![id, leases::deadline(), timestamp],
            )?;
            bounty = find_bounty(tx, id)?;
        }
        events::dispatch(tx, &bounty, false)?;
    }
    Ok(bounty)
}

/// 函数职责：前置完成事务末尾解锁全部就绪下游，历史、窗口和收件箱任一步失败全部回滚。
/// 约束条件：仅从未完成到 COMPLETED 的状态流转调用一次；依赖编辑仅允许 DRAFT。
pub(super) fn unlock_dependents(
    tx: &Transaction<'_>,
    completed_id: &str,
    timestamp: &str,
) -> Result<()> {
    let downstream = related(tx, completed_id, false)?;
    for summary in downstream {
        if summary.status != BountyStatus::Open {
            continue;
        }
        let bounty = find_bounty(tx, &summary.id)?;
        if bounty.is_blocked {
            continue;
        }
        let assignment = (bounty.assignee_mode == AssigneeMode::Specific).then(leases::deadline);
        tx.execute(
            "UPDATE bounties SET assignment_expires_at=?2,updated_at=?3 WHERE id=?1",
            params![bounty.id, assignment, timestamp],
        )?;
        let unlocked = find_bounty(tx, &bounty.id)?;
        event(
            tx,
            &unlocked,
            "dependency_unlocked",
            "system",
            Some(BountyStatus::Open),
            json!({"unlocked_by_bounty_id":completed_id}),
        )?;
        events::dispatch(tx, &unlocked, false)?;
    }
    Ok(())
}
