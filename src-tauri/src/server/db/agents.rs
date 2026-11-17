//! 文件职责：Agent 准入、凭证、订阅与可见性；邀请消耗和注册同事务提交。
use super::*;
use sha2::{Digest, Sha256};

const INVITE_COLUMNS: &str =
    "id,label,token_prefix,max_uses,use_count,expires_at,revoked_at,created_at";
// 显式列出公开字段，避免增加存储字段时意外暴露 MMR 或密钥摘要。
const AGENT_COLUMNS: &str = "id,name,description,capabilities_json,api_key_prefix,tier,placement_completed,subscribe_all_projects,status,invite_token_id,last_seen_at,created_at,updated_at,webhook_url,last_pushed_at,timeout_count,abandon_count";

impl Repository {
    fn agent_summary(&self, conn: &Connection, id: &str) -> Result<AgentSummary> {
        let mut agent = find_agent(conn, id)?;
        self.hub.decorate(&mut agent);
        Ok(agent)
    }

    /// 函数职责：在同一快照读取准入设置及令牌列表；可选配置变更与返回值原子一致。
    pub fn onboarding(&self, update: Option<UpdateOnboarding>) -> Result<Onboarding> {
        self.write(|tx| {
            if let Some(update) = update {
                tx.execute(
                    "UPDATE server_settings SET lan_open_registration=?1,updated_at=?2 WHERE id=1",
                    params![update.lan_open_registration, now()],
                )?;
            }
            Ok(Onboarding {
                lan_open_registration: open_registration(tx)?,
                invite_tokens: list_invites(tx)?,
            })
        })
    }

    pub fn list_invite_tokens(&self) -> Result<Vec<InviteTokenSummary>> {
        list_invites(&self.connect()?)
    }

    /// 函数职责：签发 256 位随机邀请；仅返回本次明文，数据库只写 SHA-256 和显示前缀。
    pub fn create_invite_token(&self, input: CreateInviteToken) -> Result<CreatedInviteToken> {
        if input.max_uses.is_some_and(|n| n < 1) {
            return Err(ApiError::invalid("max_uses 必须至少为 1"));
        }
        let expires_at = input
            .expires_in_hours
            .map(|hours| {
                let millis = hours * 3_600_000.0;
                if !hours.is_finite()
                    || hours <= 0.0
                    || !millis.is_finite()
                    || millis >= i64::MAX as f64
                {
                    return Err(ApiError::invalid("expires_in_hours 必须为有效正数"));
                }
                let duration = chrono::Duration::try_milliseconds(millis.max(1.0) as i64)
                    .ok_or_else(|| ApiError::invalid("过期时间超出范围"))?;
                Utc::now()
                    .checked_add_signed(duration)
                    .map(|v| v.to_rfc3339_opts(SecondsFormat::Millis, true))
                    .ok_or_else(|| ApiError::invalid("过期时间超出范围"))
            })
            .transpose()?;
        let raw_token = credential("tr_inv_")?;
        let token = self.write(|tx| {
            let id = Uuid::new_v4().to_string();
            tx.execute("INSERT INTO invite_tokens(id,label,token_hash,token_prefix,max_uses,expires_at,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![id,input.label,digest(&raw_token),prefix(&raw_token),input.max_uses,expires_at,now()])?;
            find_invite(tx, &id)
        })?;
        Ok(CreatedInviteToken { token, raw_token })
    }

    /// 函数职责：幂等作废邀请；已注册 Agent 的凭证不受邀请作废影响。
    pub fn revoke_invite_token(&self, id: &str) -> Result<InviteTokenSummary> {
        self.write(|tx| {
            find_invite(tx, id)?;
            tx.execute(
                "UPDATE invite_tokens SET revoked_at=COALESCE(revoked_at,?2) WHERE id=?1",
                params![id, now()],
            )?;
            find_invite(tx, id)
        })
    }

    /// 函数职责：校验显式用于文档的邀请，绝不消耗使用次数。
    pub fn validate_invite_token(&self, raw: &str) -> Result<()> {
        valid_invite(&self.connect()?, raw, &now()).map(|_| ())
    }

    /// 函数职责：原子执行准入校验、邀请次数递增、Agent 创建和初始订阅。
    /// 输出说明：失败回滚所有写入；成功仅此返回 api_key 明文。空白邀请视为未提供。
    pub fn register_agent(&self, input: RegisterAgent) -> Result<RegisteredAgent> {
        let name = input.name.trim();
        if !(1..=64).contains(&name.chars().count()) {
            return Err(ApiError::invalid("name 去除首尾空白后须为 1 至 64 字符"));
        }
        let (all, projects) = subscriptions(input.subscribe_all_projects, input.project_ids)?;
        let api_key = credential("tr_ag_")?;
        let (webhook_url, webhook_secret) =
            super::events::webhook_values(input.webhook_url, input.webhook_secret, None)?;
        let agent = self.write(|tx| {
            let timestamp = now();
            let invite_id = match input.invite_token.as_deref().filter(|v| !v.trim().is_empty()) {
                Some(raw) => Some(valid_invite(tx, raw, &timestamp)?),
                None => {
                    if !open_registration(tx)? {
                        return Err(ApiError::new(StatusCode::UNAUTHORIZED,"INVITE_TOKEN_REQUIRED","需要注册邀请令牌"));
                    }
                    None
                }
            };
            let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM agents WHERE name=?1)", [name], |r| r.get(0))?;
            if exists { return Err(ApiError::conflict("AGENT_NAME_TAKEN", "Agent 名称已被使用")); }
            validate_subscriptions(tx, &projects)?;
            let id = Uuid::new_v4().to_string();
            if let Some(invite_id) = &invite_id {
                // BEGIN IMMEDIATE 使校验与消耗跨独立连接互斥；后续写入失败仍会回滚次数。
                tx.execute("UPDATE invite_tokens SET use_count=use_count+1 WHERE id=?1", [invite_id])?;
            }
            tx.execute("INSERT INTO agents(id,name,description,capabilities_json,api_key_hash,api_key_prefix,subscribe_all_projects,invite_token_id,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",params![id,name,input.description,json!(input.capabilities).to_string(),digest(&api_key),prefix(&api_key),all,invite_id,timestamp])?;
            tx.execute("UPDATE agents SET webhook_url=?2,webhook_secret=?3 WHERE id=?1", params![id,webhook_url,webhook_secret])?;
            replace_subscriptions(tx, &id, &projects, &timestamp)?;
            self.agent_summary(tx, &id)
        })?;
        Ok(RegisteredAgent {
            agent,
            api_key,
            webhook_secret,
        })
    }

    /// 函数职责：按密钥摘要认证并更新最近活动时间；停用和吊销分别返回 403 与 401。
    pub fn authenticate_agent(&self, api_key: &str) -> Result<AgentSummary> {
        self.write(|tx| {
            let id: Option<String> = tx
                .query_row(
                    "SELECT id FROM agents WHERE api_key_hash=?1",
                    [digest(api_key)],
                    |r| r.get(0),
                )
                .optional()?;
            let id = id.ok_or_else(|| {
                ApiError::new(
                    StatusCode::UNAUTHORIZED,
                    "UNAUTHORIZED",
                    "需要有效的 Agent Bearer api_key",
                )
            })?;
            active_agent(tx, &id)?;
            tx.execute(
                "UPDATE agents SET last_seen_at=?2 WHERE id=?1",
                params![id, now()],
            )?;
            self.agent_summary(tx, &id)
        })
    }

    pub fn list_agents(&self, filter: AgentFilter) -> Result<Vec<AgentSummary>> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let ids = {
            let mut stmt = tx.prepare("SELECT id FROM agents WHERE (?1 IS NULL OR status=?1) ORDER BY created_at DESC,id DESC")?;
            let rows = stmt.query_map([filter.status.map(AgentStatus::as_str)], |r| {
                r.get::<_, String>(0)
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut agents = Vec::with_capacity(ids.len());
        for id in ids {
            let agent = self.agent_summary(&tx, &id)?;
            let matches = match filter.activity {
                None => true,
                Some(AgentActivity::Idle) => agent.active_lease_count == 0,
                Some(AgentActivity::Busy) => agent.active_lease_count > 0,
            };
            if matches {
                agents.push(agent);
            }
        }
        tx.commit()?;
        Ok(agents)
    }

    /// 函数职责：切换凭证状态；吊销为终态，重复吊销幂等，启用或停用不能恢复吊销凭证。
    pub fn set_agent_status(&self, id: &str, status: AgentStatus) -> Result<AgentSummary> {
        self.write(|tx| {
            let agent = self.agent_summary(tx, id)?;
            if agent.status == AgentStatus::Revoked && status != AgentStatus::Revoked {
                return Err(invalid_transition());
            }
            tx.execute(
                "UPDATE agents SET status=?2,updated_at=?3 WHERE id=?1",
                params![id, status.as_str(), now()],
            )?;
            if status != AgentStatus::Active {
                self.hub.disconnect(id);
            }
            self.agent_summary(tx, id)
        })
    }

    pub fn update_agent(&self, id: &str, input: UpdateAgent) -> Result<AgentSummary> {
        self.write(|tx| {
            let mut agent = active_agent(tx, id)?;
            if let Some(description) = input.description {
                agent.description = description;
            }
            if let Some(capabilities) = input.capabilities {
                agent.capabilities = capabilities;
            }
            tx.execute(
                "UPDATE agents SET description=?2,capabilities_json=?3,updated_at=?4 WHERE id=?1",
                params![
                    id,
                    agent.description,
                    json!(agent.capabilities).to_string(),
                    now()
                ],
            )?;
            self.agent_summary(tx, id)
        })
    }

    /// 函数职责：整体替换订阅；先校验全部项目再写入，任何错误保留原有订阅。
    pub fn update_agent_subscriptions(
        &self,
        id: &str,
        input: AgentSubscriptions,
    ) -> Result<AgentSummary> {
        let (all, projects) = subscriptions(input.subscribe_all_projects, input.project_ids)?;
        self.write(|tx| {
            active_agent(tx, id)?;
            validate_subscriptions(tx, &projects)?;
            let timestamp = now();
            tx.execute(
                "UPDATE agents SET subscribe_all_projects=?2,updated_at=?3 WHERE id=?1",
                params![id, all, timestamp],
            )?;
            replace_subscriptions(tx, id, &projects, &timestamp)?;
            self.agent_summary(tx, id)
        })
    }

    /// 函数职责：返回可供 Agent 选择订阅的全部未归档项目。
    pub fn agent_projects(&self, id: &str) -> Result<Vec<Project>> {
        active_agent(&self.connect()?, id)?;
        self.list_projects(false)
    }

    /// 函数职责：返回已发布的订阅范围内公开任务及自身发布/指派/持有/仲裁任务；任何参数都不能显示 DRAFT。
    pub fn agent_bounties(&self, id: &str, mut filter: BountyFilter) -> Result<Vec<Bounty>> {
        let agent = active_agent(&self.connect()?, id)?;
        filter.include_drafts = false;
        Ok(self
            .list_bounties(filter)?
            .into_iter()
            .filter(|b| visible(&agent, b))
            .collect())
    }

    /// 函数职责：读取任务详情并应用相同可见性规则；拒绝时不返回正文或事件。
    pub fn agent_bounty_detail(&self, agent_id: &str, id: &str) -> Result<BountyDetail> {
        let agent = active_agent(&self.connect()?, agent_id)?;
        let detail = self.get_bounty_detail(id)?;
        if !visible(&agent, &detail.bounty) {
            return Err(ApiError::new(
                StatusCode::NOT_FOUND,
                "BOUNTY_NOT_FOUND",
                "悬赏不存在",
            ));
        }
        Ok(detail)
    }
}

fn digest(raw: &str) -> String {
    format!("{:x}", Sha256::digest(raw.as_bytes()))
}
fn credential(kind: &str) -> Result<String> {
    Ok(format!("{kind}{}", crate::server::random_admin_token()?))
}
fn prefix(raw: &str) -> String {
    raw.chars().take(15).collect()
}
fn open_registration(conn: &Connection) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT lan_open_registration FROM server_settings WHERE id=1",
        [],
        |r| r.get(0),
    )?)
}
fn invite_row(r: &Row<'_>) -> rusqlite::Result<InviteTokenSummary> {
    Ok(InviteTokenSummary {
        id: r.get(0)?,
        label: r.get(1)?,
        token_prefix: r.get(2)?,
        max_uses: r.get(3)?,
        use_count: r.get(4)?,
        expires_at: r.get(5)?,
        revoked_at: r.get(6)?,
        created_at: r.get(7)?,
    })
}
fn list_invites(conn: &Connection) -> Result<Vec<InviteTokenSummary>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {INVITE_COLUMNS} FROM invite_tokens ORDER BY created_at DESC,id DESC"
    ))?;
    let rows = stmt.query_map([], invite_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
fn find_invite(conn: &Connection, id: &str) -> Result<InviteTokenSummary> {
    conn.query_row(
        &format!("SELECT {INVITE_COLUMNS} FROM invite_tokens WHERE id=?1"),
        [id],
        invite_row,
    )
    .optional()?
    .ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "INVITE_TOKEN_NOT_FOUND",
            "邀请令牌不存在",
        )
    })
}

fn valid_invite(conn: &Connection, raw: &str, timestamp: &str) -> Result<String> {
    conn.query_row("SELECT id FROM invite_tokens WHERE token_hash=?1 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>?2) AND (max_uses IS NULL OR use_count<max_uses)",params![digest(raw),timestamp],|r| r.get(0)).optional()?
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED,"INVITE_TOKEN_INVALID","邀请令牌无效、已过期或已耗尽"))
}

fn subscriptions(all: Option<bool>, projects: Option<Vec<String>>) -> Result<(bool, Vec<String>)> {
    if all == Some(true) && projects.as_ref().is_some_and(|ids| !ids.is_empty()) {
        return Err(ApiError::invalid("全局订阅不能同时指定 project_ids"));
    }
    let subscribe_all = all == Some(true) || (projects.is_none() && all != Some(false));
    let mut projects = projects.unwrap_or_default();
    projects.sort();
    projects.dedup();
    Ok((subscribe_all, projects))
}
fn validate_subscriptions(conn: &Connection, projects: &[String]) -> Result<()> {
    for project in projects {
        active_project(conn, project)?;
    }
    Ok(())
}
fn replace_subscriptions(
    tx: &Transaction<'_>,
    id: &str,
    projects: &[String],
    timestamp: &str,
) -> Result<()> {
    tx.execute("DELETE FROM agent_subscriptions WHERE agent_id=?1", [id])?;
    for project in projects {
        tx.execute(
            "INSERT INTO agent_subscriptions(agent_id,project_id,created_at) VALUES(?1,?2,?3)",
            params![id, project, timestamp],
        )?;
    }
    Ok(())
}
fn find_agent(conn: &Connection, id: &str) -> Result<AgentSummary> {
    let mut agent = conn
        .query_row(
            &format!("SELECT {AGENT_COLUMNS} FROM agents WHERE id=?1"),
            [id],
            |r| {
                Ok(AgentSummary {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    description: r.get(2)?,
                    capabilities: json_value(r, 3)?,
                    api_key_prefix: r.get(4)?,
                    tier: enum_value(r, 5)?,
                    placement_completed: r.get(6)?,
                    subscribe_all_projects: r.get(7)?,
                    status: enum_value(r, 8)?,
                    invite_token_id: r.get(9)?,
                    last_seen_at: r.get(10)?,
                    created_at: r.get(11)?,
                    updated_at: r.get(12)?,
                    project_ids: vec![],
                    active_lease_count: 0,
                    channel_status: "offline".into(),
                    stream_connected: false,
                    webhook_configured: false,
                    webhook_url: r.get(13)?,
                    last_pushed_at: r.get(14)?,
                    timeout_count: r.get(15)?,
                    abandon_count: r.get(16)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "AGENT_NOT_FOUND", "Agent 不存在"))?;
    let mut stmt = conn.prepare(
        "SELECT project_id FROM agent_subscriptions WHERE agent_id=?1 ORDER BY project_id",
    )?;
    agent.project_ids = stmt
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    // 冻结状态保留租约持有者，期限可为空；只有执行中的租约需要校验期限。
    agent.active_lease_count = conn.query_row("SELECT COUNT(*) FROM bounties WHERE lease_agent_id=?1 AND ((status='IN_PROGRESS' AND lease_expires_at>?2) OR status IN ('WAITING_INFO','PENDING_ARBITRATION'))", params![id,now()], |r| r.get(0))?;
    Ok(agent)
}
pub(super) fn active_agent(conn: &Connection, id: &str) -> Result<AgentSummary> {
    let agent = find_agent(conn, id)?;
    match agent.status {
        AgentStatus::Active => Ok(agent),
        AgentStatus::Disabled => Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "AGENT_DISABLED",
            "Agent 已停用",
        )),
        AgentStatus::Revoked => Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "AGENT_REVOKED",
            "Agent 凭证已吊销",
        )),
    }
}
pub(super) fn visible(agent: &AgentSummary, bounty: &Bounty) -> bool {
    // 发布者权限同样受发布门槛约束，不能读取草稿或未发布即取消的任务。
    bounty.status != BountyStatus::Draft
        && bounty.published_at.is_some()
        && (bounty.created_by_agent_id.as_deref() == Some(&agent.id)
            || bounty.assignee_agent_id.as_deref() == Some(&agent.id)
            || bounty.lease_agent_id.as_deref() == Some(&agent.id)
            || bounty.arbitrator_agent_id.as_deref() == Some(&agent.id)
            || (bounty.assignee_mode != AssigneeMode::Specific
                && (agent.subscribe_all_projects
                    || agent.project_ids.contains(&bounty.project_id))))
}
