//! 文件职责：草稿受邀共创、原子拆单和批量发布；所有权限在写锁内复核。
use super::*;

/// 函数职责：先隐藏非受邀任务，再校验草稿状态；发布后仍保留协作者身份用于明确状态错误。
fn invited_draft(tx: &Transaction<'_>, id: &str, agent_id: &str) -> Result<Bounty> {
    agents::active_agent(tx, agent_id)?;
    let bounty = find_bounty(tx, id)?;
    if bounty.draft_collaborator_agent_id.as_deref() != Some(agent_id) {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "BOUNTY_NOT_FOUND",
            "任务不存在",
        ));
    }
    if bounty.status != BountyStatus::Draft {
        return Err(invalid_transition());
    }
    Ok(bounty)
}

impl Repository {
    /// 函数职责：管理员替换或取消草稿协作者；仅新受邀的活跃 Agent 收到草稿快照。
    pub fn invite_draft_collaborator(
        &self,
        id: &str,
        input: InviteDraftCollaboratorInput,
    ) -> Result<Bounty> {
        self.write(|tx| {
            let bounty = find_bounty(tx, id)?;
            if bounty.status != BountyStatus::Draft {
                return Err(invalid_transition());
            }
            let name = if let Some(agent_id) = input.agent_id.as_deref() {
                if agent_id.trim().is_empty() {
                    return Err(ApiError::invalid("agent_id 不能为空白"));
                }
                let agent: Option<(String, String)> = tx
                    .query_row(
                        "SELECT name,status FROM agents WHERE id=?1",
                        [agent_id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                let (name, status) = agent.ok_or_else(|| {
                    ApiError::new(StatusCode::NOT_FOUND, "AGENT_NOT_FOUND", "Agent 不存在")
                })?;
                if status != "active" {
                    return Err(ApiError::conflict(
                        "AGENT_INACTIVE",
                        "只能邀请活跃状态的 Agent 协助草稿",
                    ));
                }
                Some(name)
            } else {
                None
            };
            tx.execute(
                "UPDATE bounties SET draft_collaborator_agent_id=?2,updated_at=?3 WHERE id=?1",
                params![id, input.agent_id, now()],
            )?;
            let updated = find_bounty(tx, id)?;
            if let Some(agent_id) = input.agent_id.as_deref() {
                event(
                    tx,
                    &updated,
                    "draft_invited",
                    "human",
                    Some(BountyStatus::Draft),
                    json!({"collaborator_agent_id":agent_id,"collaborator_agent_name":name}),
                )?;
                events::notify_active(
                    tx,
                    Some(agent_id),
                    &updated,
                    "draft.invited",
                    json!({"bounty":updated}),
                )?;
            } else {
                event(
                    tx,
                    &updated,
                    "draft_collaborator_cleared",
                    "human",
                    Some(BountyStatus::Draft),
                    json!({}),
                )?;
            }
            Ok(updated)
        })
    }

    /// 函数职责：受邀 Agent 修订正文与依赖并提交人工审阅；不改变租约或计时。
    pub fn agent_update_draft(
        &self,
        id: &str,
        agent_id: &str,
        input: AgentUpdateDraftInput,
    ) -> Result<Bounty> {
        self.write(|tx| {
            let mut bounty = invited_draft(tx,id,agent_id)?;
            if let Some(v) = input.title { bounty.title = v; }
            if let Some(v) = input.description { bounty.description = v; }
            if let Some(v) = input.acceptance_criteria { bounty.acceptance_criteria = v; }
            if bounty.title.trim().is_empty() { return Err(ApiError::invalid("任务标题不能为空")); }
            let timestamp = now();
            if let Some(ids) = input.blocked_by { dependencies::replace(tx,id,ids,Some(agent_id),&timestamp)?; }
            tx.execute("UPDATE bounties SET title=?2,description=?3,acceptance_criteria=?4,draft_needs_human_review=1,updated_at=?5 WHERE id=?1", params![id,bounty.title,bounty.description,bounty.acceptance_criteria,timestamp])?;
            let mut updated = find_bounty(tx,id)?;
            event(tx,&updated,"draft_revised",&format!("agent:{agent_id}"),Some(BountyStatus::Draft),json!({}))?;
            dependencies::redact(tx,&mut updated,agent_id)?;
            Ok(updated)
        })
    }

    /// 函数职责：同项目原子派生子草稿；先创建全部节点，再解析批内 key 与已有依赖并验证 DAG。
    /// 约束条件：key 与已有任务 ID 不重名，避免引用歧义；任一项失败回滚父、子、依赖和历史。
    pub fn agent_split_draft(
        &self,
        id: &str,
        agent_id: &str,
        input: SplitDraftInput,
    ) -> Result<SplitDraftResult> {
        self.write(|tx| {
            let parent = invited_draft(tx,id,agent_id)?;
            if input.items.is_empty() { return Err(ApiError::invalid("items 不能为空")); }
            active_project(tx,&parent.project_id)?;
            let timestamp = now();
            let actor = format!("agent:{agent_id}");
            let ids: Vec<_> = input.items.iter().map(|_|Uuid::new_v4().to_string()).collect();
            let mut keys = std::collections::HashMap::new();
            for (item,child_id) in input.items.iter().zip(&ids) {
                validate_bounty(&item.title,item.assignee_mode,item.assignee_min_tier,item.assignee_agent_id.as_deref())?;
                if let Some(key) = item.key.as_deref() {
                    let key = key.trim();
                    if key.is_empty() || keys.insert(key.to_owned(),child_id.clone()).is_some() {
                        return Err(ApiError::invalid("key 去空白后须非空且批内唯一"));
                    }
                    let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM bounties WHERE id=?1)",[key],|r|r.get(0))?;
                    if exists { return Err(ApiError::invalid("key 不能与现有任务 ID 冲突")); }
                }
            }
            for (item,child_id) in input.items.iter().zip(&ids) {
                tx.execute("INSERT INTO bounties(id,project_id,title,description,acceptance_criteria,difficulty,assignee_mode,assignee_min_tier,assignee_agent_id,require_arbitration,status,created_by,created_by_agent_id,parent_bounty_id,draft_collaborator_agent_id,draft_needs_human_review,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'DRAFT',?11,?12,?13,?12,1,?14,?14)",params![child_id,parent.project_id,item.title,item.description,item.acceptance_criteria,item.difficulty.as_str(),item.assignee_mode.as_str(),item.assignee_min_tier.map(Tier::as_str),item.assignee_agent_id,item.require_arbitration,actor,agent_id,id,timestamp])?;
            }
            for (item,child_id) in input.items.into_iter().zip(&ids) {
                let edges = item.blocked_by.into_iter().map(|v|keys.get(v.trim()).cloned().unwrap_or(v)).collect();
                dependencies::replace(tx,child_id,edges,Some(agent_id),&timestamp)?;
            }
            let mut children = Vec::new();
            for child_id in &ids {
                let mut child = find_bounty(tx,child_id)?;
                event(tx,&child,"created",&actor,None,json!({"parent_bounty_id":id}))?;
                dependencies::redact(tx,&mut child,agent_id)?;
                children.push(child);
            }
            tx.execute("UPDATE bounties SET draft_needs_human_review=1,updated_at=?2 WHERE id=?1",params![id,timestamp])?;
            let mut parent = find_bounty(tx,id)?;
            event(tx,&parent,"draft_split",&actor,Some(BountyStatus::Draft),json!({"child_bounty_ids":ids}))?;
            dependencies::redact(tx,&mut parent,agent_id)?;
            Ok(SplitDraftResult { parent, children })
        })
    }

    /// 函数职责：管理员原子发布草稿集合；先全部校验和写入 OPEN，再启动无阻塞任务的分发。
    pub fn publish_bounties_batch(
        &self,
        input: PublishBountiesBatchInput,
        actor: &str,
    ) -> Result<Vec<Bounty>> {
        let ids = dependencies::normalize(input.bounty_ids)?;
        if ids.is_empty() {
            return Err(ApiError::invalid("bounty_ids 不能为空"));
        }
        self.write(|tx| {
            let timestamp = now();
            for id in &ids {
                let bounty = find_bounty(tx,id)?;
                if bounty.status != BountyStatus::Draft { return Err(invalid_transition()); }
                active_project(tx,&bounty.project_id)?;
            }
            for id in &ids {
                tx.execute("UPDATE bounties SET status='OPEN',published_at=?2,draft_needs_human_review=0,assignment_expires_at=NULL,updated_at=?2 WHERE id=?1",params![id,timestamp])?;
            }
            let mut published = Vec::new();
            for id in &ids {
                let bounty = dependencies::dispatch_ready(tx,id,&timestamp)?;
                event(tx,&bounty,"published",actor,Some(BountyStatus::Draft),json!({}))?;
                published.push(bounty);
            }
            Ok(published)
        })
    }
}
