//! 文件职责：SQLite 初始化、项目存储与任务状态机；所有写入由仓储维护不变量。
use super::{
    error::{ApiError, Result},
    models::*,
};
use axum::http::StatusCode;
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction, TransactionBehavior};
use serde::de::DeserializeOwned;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

pub(crate) mod accounts;
mod agents;
mod arbitration;
mod comments;
mod dependencies;
mod drafts;
mod events;
mod leases;
pub(crate) mod oauth;
mod rating;

/// 结构职责：可克隆的文件数据库入口；每次调用独立连接，可跨进程共享同一文件。
/// 约束条件：同步接口应在阻塞线程执行，不接受 :memory:，以保证多连接共享数据。
#[derive(Debug, Clone)]
pub struct Repository {
    path: PathBuf,
    pub(crate) hub: super::push::Hub,
}

const PROJECT_COLUMNS: &str = "id,name,icon,description,workspace_path,default_acceptance_criteria,agent_draft_policy,arbitrator_chain_json,archived,created_at,updated_at";
const BOUNTY_COLUMNS: &str = "b.id,b.project_id,b.title,b.description,b.acceptance_criteria,b.difficulty,b.assignee_mode,b.assignee_min_tier,b.assignee_agent_id,b.require_arbitration,b.status,b.created_by,b.lease_id,b.lease_agent_id,b.lease_expires_at,b.rejection_count,b.completion_note,b.published_at,b.withdrawn_at,b.cancelled_at,b.completed_at,b.created_at,b.updated_at,b.created_by_agent_id,a.name,la.name,la.tier,aa.name,aa.tier,b.assignment_expires_at,b.completion_cost,b.active_duration_ms,b.active_started_at,(SELECT COUNT(*) FROM bounty_comments c WHERE c.bounty_id=b.id),b.arbitrator_agent_id,ar.name,ar.tier,b.arbitration_expires_at,b.quality_grade,b.calibrated_difficulty,b.draft_collaborator_agent_id,dc.name,dc.tier,b.draft_needs_human_review,b.parent_bounty_id";
const BOUNTY_SOURCE: &str = "bounties b LEFT JOIN agents a ON a.id=b.created_by_agent_id LEFT JOIN agents la ON la.id=b.lease_agent_id LEFT JOIN agents aa ON aa.id=b.assignee_agent_id LEFT JOIN agents ar ON ar.id=b.arbitrator_agent_id LEFT JOIN agents dc ON dc.id=b.draft_collaborator_agent_id";

impl Repository {
    /// 函数职责：原子初始化当前表、索引与默认配置；重复打开不覆盖已有数据与配置。
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if path.as_os_str().is_empty() || path == Path::new(":memory:") {
            return Err(ApiError::invalid("db 必须是文件路径"));
        }
        let path = std::path::absolute(path).map_err(ApiError::internal)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(ApiError::internal)?;
        }
        let repo = Self {
            path,
            hub: super::push::Hub::default(),
        };
        let mut conn = repo.connect()?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(include_str!("schema.sql"))?;
        comments::migrate(&tx)?;
        oauth::migrate_unbound_grants(&tx)?;
        tx.commit()?;
        Ok(repo)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn connect(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path)?;
        conn.busy_timeout(Duration::from_millis(5000))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(conn)
    }

    /// 写事务先取得 SQLite 写锁再读取状态；其他连接不能在校验与提交之间抢单。
    fn write<T>(&self, op: impl FnOnce(&Transaction<'_>) -> Result<T>) -> Result<T> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let value = op(&tx)?;
        tx.commit()?;
        Ok(value)
    }

    pub fn create_project(&self, mut input: CreateProject) -> Result<Project> {
        validate_project(&input.name, input.icon.as_deref())?;
        input.arbitrator_chain = arbitration::normalize_chain(input.arbitrator_chain)?;
        self.write(|tx| {
            let id = Uuid::new_v4().to_string();
            let timestamp = now();
            tx.execute("INSERT INTO projects(id,name,icon,description,workspace_path,default_acceptance_criteria,agent_draft_policy,arbitrator_chain_json,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",
                params![id,input.name,input.icon,input.description,input.workspace_path,input.default_acceptance_criteria,input.agent_draft_policy.as_str(),json!(input.arbitrator_chain).to_string(),timestamp])?;
            find_project(tx, &id)
        })
    }

    pub fn list_projects(&self, include_archived: bool) -> Result<Vec<Project>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(&format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE (?1 OR archived=0) ORDER BY created_at,id"))?;
        let rows = stmt.query_map([include_archived], project_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_project(&self, id: &str) -> Result<Project> {
        find_project(&self.connect()?, id)
    }

    pub fn update_project(&self, id: &str, patch: UpdateProject) -> Result<Project> {
        self.write(|tx| {
            let mut p = find_project(tx, id)?;
            if let Some(v) = patch.name { p.name = v; }
            patch.icon.apply(&mut p.icon);
            if let Some(v) = patch.description { p.description = v; }
            if let Some(v) = patch.workspace_path { p.workspace_path = v; }
            if let Some(v) = patch.default_acceptance_criteria { p.default_acceptance_criteria = v; }
            if let Some(v) = patch.agent_draft_policy { p.agent_draft_policy = v; }
            if let Some(v) = patch.arbitrator_chain { p.arbitrator_chain = arbitration::normalize_chain(v)?; }
            if let Some(v) = patch.archived { p.archived = v; }
            validate_project(&p.name, p.icon.as_deref())?;
            tx.execute("UPDATE projects SET name=?2,icon=?3,description=?4,workspace_path=?5,default_acceptance_criteria=?6,agent_draft_policy=?7,arbitrator_chain_json=?8,archived=?9,updated_at=?10 WHERE id=?1",
                params![id,p.name,p.icon,p.description,p.workspace_path,p.default_acceptance_criteria,p.agent_draft_policy.as_str(),json!(p.arbitrator_chain).to_string(),p.archived,now()])?;
            find_project(tx, id)
        })
    }

    pub fn delete_project(&self, id: &str) -> Result<()> {
        self.write(|tx| {
            find_project(tx, id)?;
            let referenced: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM bounties WHERE project_id=?1)",
                [id],
                |r| r.get(0),
            )?;
            if referenced {
                return Err(ApiError::conflict(
                    "PROJECT_HAS_BOUNTIES",
                    "项目已有任务，不能删除",
                ));
            }
            tx.execute("DELETE FROM projects WHERE id=?1", [id])?;
            Ok(())
        })
    }

    pub fn create_bounty(&self, input: CreateBounty) -> Result<Bounty> {
        self.create_bounty_with_session(input, None)
    }

    /// 函数职责：创建任务并在同一写事务内校验账户会话、记录实际创建者。
    /// 输入说明：不传会话时允许管理员创建匿名任务。
    pub(crate) fn create_bounty_with_session(
        &self,
        input: CreateBounty,
        user_session: Option<&str>,
    ) -> Result<Bounty> {
        if !matches!(input.status, BountyStatus::Draft | BountyStatus::Open) {
            return Err(ApiError::invalid("初始状态必须为 DRAFT 或 OPEN"));
        }
        validate_bounty(
            &input.title,
            input.assignee_mode,
            input.assignee_min_tier,
            input.assignee_agent_id.as_deref(),
        )?;
        if input.created_by.trim().is_empty() {
            return Err(ApiError::invalid("created_by 不能为空"));
        }
        self.write(|tx| {
            let creator = user_session
                .map(|token| accounts::user_id_for_session(tx, token))
                .transpose()?;
            active_project(tx, &input.project_id)?;
            let id = Uuid::new_v4().to_string();
            let timestamp = now();
            let published = (input.status == BountyStatus::Open).then_some(&timestamp);
            let assignment: Option<String> = None;
            tx.execute("INSERT INTO bounties(id,project_id,title,description,acceptance_criteria,difficulty,assignee_mode,assignee_min_tier,assignee_agent_id,require_arbitration,status,created_by,published_at,created_at,updated_at,assignment_expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?14,?15)",
                params![id,input.project_id,input.title,input.description,input.acceptance_criteria,input.difficulty.as_str(),input.assignee_mode.as_str(),input.assignee_min_tier.map(Tier::as_str),input.assignee_agent_id,input.require_arbitration,input.status.as_str(),input.created_by,published,timestamp,assignment])?;
            if let Some(account_id) = creator {
                tx.execute(
                    "INSERT INTO bounty_user_creators(bounty_id, account_id) VALUES(?1, ?2)",
                    params![id, account_id],
                )?;
            }
            dependencies::replace(tx, &id, input.blocked_by, None, &timestamp)?;
            let mut b = find_bounty(tx, &id)?;
            event(tx, &b, "created", &b.created_by, None, json!({}))?;
            if b.status == BountyStatus::Open { b = dependencies::dispatch_ready(tx, &id, &timestamp)?; }
            Ok(b)
        })
    }

    /// 函数职责：以认证 Agent 身份提交任务，按目标项目当前策略决定公开或待审。
    /// 输出说明：身份复核、策略读取、创建事件与 OPEN 分发原子提交；DRAFT 不写入任何收件箱。
    /// 实现思路：先取得写锁再复核身份和策略，防止停用或策略切换穿插在校验与写入之间。
    pub fn agent_create_bounty(&self, agent_id: &str, input: AgentCreateBounty) -> Result<Bounty> {
        validate_bounty(
            &input.title,
            input.assignee_mode,
            input.assignee_min_tier,
            input.assignee_agent_id.as_deref(),
        )?;
        self.write(|tx| {
            let agent = agents::active_agent(tx, agent_id)?;
            let project = find_project(tx, &input.project_id)?;
            if project.archived {
                return Err(ApiError::conflict("PROJECT_ARCHIVED", "归档项目不能创建或发布任务"));
            }
            let id = Uuid::new_v4().to_string();
            let timestamp = now();
            let status = match project.agent_draft_policy {
                AgentDraftPolicy::Open => BountyStatus::Open,
                AgentDraftPolicy::Draft => BountyStatus::Draft,
            };
            let published = (status == BountyStatus::Open).then_some(&timestamp);
            let actor = format!("agent:{agent_id}");
            let assignment: Option<String> = None;
            tx.execute("INSERT INTO bounties(id,project_id,title,description,acceptance_criteria,difficulty,assignee_mode,assignee_min_tier,assignee_agent_id,require_arbitration,status,created_by,created_by_agent_id,published_at,created_at,updated_at,assignment_expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?15,?16)",
                params![id,input.project_id,input.title,input.description,input.acceptance_criteria,input.difficulty.as_str(),input.assignee_mode.as_str(),input.assignee_min_tier.map(Tier::as_str),input.assignee_agent_id,input.require_arbitration,status.as_str(),actor,agent_id,published,timestamp,assignment])?;
            dependencies::replace(tx, &id, input.blocked_by, Some(agent_id), &timestamp)?;
            let mut bounty = find_bounty(tx, &id)?;
            event(tx, &bounty, "created", &actor, None, json!({
                "publisher_agent_id": agent_id,
                "publisher_agent_name": agent.name,
                "policy": project.agent_draft_policy.as_str(),
            }))?;
            if status == BountyStatus::Open {
                bounty = dependencies::dispatch_ready(tx, &id, &timestamp)?;
            }
            dependencies::redact(tx, &mut bounty, agent_id)?;
            Ok(bounty)
        })
    }

    pub fn list_bounties(&self, filter: BountyFilter) -> Result<Vec<Bounty>> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        list_bounties_on(&tx, filter)
    }

    /// 函数职责：在同一读快照返回任务、关系摘要、历史和评论。
    pub fn get_bounty_detail(&self, id: &str) -> Result<BountyDetail> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let mut detail = bounty_detail_on(&tx, id)?;
        detail.creator = accounts::bounty_creator(&tx, &detail.bounty)?;
        Ok(detail)
    }

    pub fn update_bounty(&self, id: &str, patch: UpdateBounty, actor: &str) -> Result<Bounty> {
        self.write(|tx| {
            let mut b = find_bounty(tx, id)?;
            if b.status != BountyStatus::Draft { return Err(invalid_transition()); }
            if let Some(v) = patch.project_id { b.project_id = v; }
            if let Some(v) = patch.title { b.title = v; }
            if let Some(v) = patch.description { b.description = v; }
            if let Some(v) = patch.acceptance_criteria { b.acceptance_criteria = v; }
            if let Some(v) = patch.difficulty { b.difficulty = v; }
            if let Some(v) = patch.assignee_mode { b.assignee_mode = v; }
            patch.assignee_min_tier.apply(&mut b.assignee_min_tier);
            patch.assignee_agent_id.apply(&mut b.assignee_agent_id);
            if let Some(v) = patch.require_arbitration { b.require_arbitration = v; }
            active_project(tx, &b.project_id)?;
            validate_bounty(&b.title,b.assignee_mode,b.assignee_min_tier,b.assignee_agent_id.as_deref())?;
            tx.execute("UPDATE bounties SET project_id=?2,title=?3,description=?4,acceptance_criteria=?5,difficulty=?6,assignee_mode=?7,assignee_min_tier=?8,assignee_agent_id=?9,require_arbitration=?10,updated_at=?11 WHERE id=?1",
                params![id,b.project_id,b.title,b.description,b.acceptance_criteria,b.difficulty.as_str(),b.assignee_mode.as_str(),b.assignee_min_tier.map(Tier::as_str),b.assignee_agent_id,b.require_arbitration,now()])?;
            if let Some(ids) = patch.blocked_by { dependencies::replace(tx, id, ids, None, &now())?; }
            let b = find_bounty(tx, id)?;
            event(tx,&b,"updated",actor,Some(BountyStatus::Draft),json!({}))?;
            Ok(b)
        })
    }

    pub fn publish_bounty(&self, id: &str, actor: &str) -> Result<Bounty> {
        self.transition(id, actor, Transition::Publish)
    }
    pub fn withdraw_bounty(&self, id: &str, actor: &str) -> Result<Bounty> {
        self.transition(id, actor, Transition::Withdraw)
    }
    pub fn cancel_bounty(&self, id: &str, actor: &str) -> Result<Bounty> {
        self.transition(id, actor, Transition::Cancel)
    }

    pub fn restore_bounty_draft(&self, id: &str, actor: &str) -> Result<Bounty> {
        self.write(|tx| {
            // 保留递增的 lease_id，确保重新发布和接取后也不能使用旧令牌。
            // 清除 published_at，使恢复后的草稿即使再次丢弃也不会被当成已发布任务。
            let changed = tx.execute(
                "UPDATE bounties SET status='DRAFT', published_at=NULL, withdrawn_at=NULL,
                    cancelled_at=NULL, completed_at=NULL, assignment_expires_at=NULL,
                    lease_agent_id=NULL, lease_expires_at=NULL, lease_warned_id=0,
                    active_duration_ms=0, active_started_at=NULL, rejection_count=0,
                    arbitrator_agent_id=NULL, arbitration_expires_at=NULL, arbitration_tried_ids_json='[]',
                    completion_note=NULL, completion_cost=NULL, quality_grade=NULL,
                    calibrated_difficulty=NULL, draft_collaborator_agent_id=NULL,
                    draft_needs_human_review=0, updated_at=?2
                 WHERE id=?1 AND status='CANCELLED'",
                params![id, now()],
            )?;
            if changed != 1 {
                find_bounty(tx, id)?;
                return Err(invalid_transition());
            }
            // 下一次撤回只应通知本轮发布的派单对象。
            tx.execute("DELETE FROM bounty_dispatches WHERE bounty_id=?1", [id])?;
            let updated = find_bounty(tx, id)?;
            event(tx, &updated, "restored_to_draft", actor, Some(BountyStatus::Cancelled), json!({}))?;
            Ok(updated)
        })
    }

    /// 函数职责：永久删除已取消任务及其详情记录。
    /// 实现思路：在同一写事务中检查下游依赖并删除，避免级联删除依赖边解除后续任务的阻塞。
    pub fn delete_bounty(&self, id: &str) -> Result<()> {
        self.write(|tx| {
            if find_bounty(tx, id)?.status != BountyStatus::Cancelled {
                return Err(invalid_transition());
            }
            if !dependencies::related(tx, id, false)?.is_empty() {
                return Err(ApiError::conflict(
                    "BOUNTY_HAS_DEPENDENTS",
                    "该任务仍是其他任务的前置任务",
                ));
            }
            tx.execute("DELETE FROM bounties WHERE id=?1", [id])?;
            Ok(())
        })
    }

    fn transition(&self, id: &str, actor: &str, action: Transition) -> Result<Bounty> {
        self.write(|tx| {
            let b = find_bounty(tx,id)?;
            let (to,event_type,column) = match action {
                Transition::Publish => {
                    if b.status != BountyStatus::Draft { return Err(invalid_transition()); }
                    active_project(tx,&b.project_id)?;
                    (BountyStatus::Open,"published","published_at")
                }
                Transition::Withdraw => {
                    if b.status == BountyStatus::InProgress || b.lease_agent_id.is_some() {
                        return Err(already_claimed());
                    }
                    if b.status != BountyStatus::Open { return Err(invalid_transition()); }
                    (BountyStatus::Draft,"withdrawn","withdrawn_at")
                }
                Transition::Cancel => {
                    if !matches!(b.status,BountyStatus::Draft | BountyStatus::Open | BountyStatus::WaitingInfo) { return Err(invalid_transition()); }
                    (BountyStatus::Cancelled,"cancelled","cancelled_at")
                }
            };
            let timestamp = now();
            // WHERE 再次约束读取到的状态与 fencing token；事务同时保护事件写入。
            let changed = if matches!(action,Transition::Withdraw) {
                tx.execute("UPDATE bounties SET status='DRAFT',withdrawn_at=?2,draft_needs_human_review=0,assignment_expires_at=NULL,updated_at=?2 WHERE id=?1 AND status='OPEN' AND lease_agent_id IS NULL AND lease_id=?3",params![id,timestamp,b.lease_id])?
            } else {
                let assignment = (matches!(action,Transition::Publish) && !b.is_blocked && b.assignee_mode == AssigneeMode::Specific).then(leases::deadline);
                // column 仅来自上方固定动作分支，不接受调用方提供 SQL 标识符。
                tx.execute(&format!("UPDATE bounties SET status=?2,{column}=?3,draft_needs_human_review=0,assignment_expires_at=?6,updated_at=?3 WHERE id=?1 AND status=?4 AND lease_id=?5"),params![id,to.as_str(),timestamp,b.status.as_str(),b.lease_id,assignment])?
            };
            if changed != 1 { return Err(invalid_transition()); }
            if matches!(action, Transition::Cancel) {
                tx.execute("UPDATE bounties SET active_started_at=NULL,lease_expires_at=NULL,lease_warned_id=0 WHERE id=?1", [id])?;
            }
            let updated = find_bounty(tx,id)?;
            if matches!(action, Transition::Cancel) && b.status == BountyStatus::WaitingInfo {
                events::notify_active(tx, b.lease_agent_id.as_deref(), &updated, "task.cancelled", json!({"bounty":updated,"lease_id":b.lease_id,"lease_agent_id":b.lease_agent_id}))?;
            }
            if matches!(action, Transition::Withdraw) || (matches!(action, Transition::Publish) && !updated.is_blocked) {
                events::dispatch(tx, &updated, matches!(action, Transition::Withdraw))?;
            }
            event(tx,&updated,event_type,actor,Some(b.status),json!({}))?;
            Ok(updated)
        })
    }
}

#[derive(Clone, Copy)]
enum Transition {
    Publish,
    Withdraw,
    Cancel,
}
fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
fn invalid_transition() -> ApiError {
    ApiError::conflict("INVALID_TRANSITION", "当前状态不允许此操作")
}
fn already_claimed() -> ApiError {
    ApiError::conflict("BOUNTY_ALREADY_CLAIMED", "任务已被接取，不能撤回或重复接取")
}

fn validate_project(name: &str, icon: Option<&str>) -> Result<()> {
    if name.trim().is_empty() {
        return Err(ApiError::invalid("项目名称不能为空"));
    }
    if let Some(icon) = icon {
        use unicode_segmentation::UnicodeSegmentation;
        let icon = icon.trim();
        let valid = if let Some(name) = icon.strip_prefix("lucide:") {
            name.len() <= 64
                && name.split('-').all(|part| {
                    !part.is_empty()
                        && part
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                })
        } else if let Some(value) = icon.strip_prefix("emoji:") {
            let value = value.trim();
            // 与 WebView 契约一致：单字素、最多 64 个 UTF-16 单元且包含可见字符。
            static VISIBLE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
            value.graphemes(true).count() == 1
                && value.encode_utf16().count() <= 64
                && !value
                    .chars()
                    .any(|c| c.is_control() || c == '\u{2028}' || c == '\u{2029}')
                && VISIBLE
                    .get_or_init(|| {
                        regex::Regex::new(r"[\p{L}\p{N}\p{P}\p{S}]")
                            .expect("静态 Unicode 字符类别合法")
                    })
                    .is_match(value)
        } else {
            false
        };
        if !valid {
            return Err(ApiError::invalid(
                "icon 必须为单字素 emoji: 或合法 lucide: 名称",
            ));
        }
    }
    Ok(())
}

fn validate_bounty(
    title: &str,
    mode: AssigneeMode,
    min: Option<Tier>,
    agent: Option<&str>,
) -> Result<()> {
    if title.trim().is_empty() {
        return Err(ApiError::invalid("任务标题不能为空"));
    }
    let valid = match mode {
        AssigneeMode::Any => min.is_none() && agent.is_none(),
        AssigneeMode::MinTier => min.is_some() && agent.is_none(),
        AssigneeMode::Specific => min.is_none() && agent.is_some_and(|a| !a.trim().is_empty()),
    };
    if !valid {
        return Err(ApiError::invalid("执行者限制字段组合不一致"));
    }
    Ok(())
}

fn active_project(conn: &Connection, id: &str) -> Result<()> {
    if find_project(conn, id)?.archived {
        return Err(ApiError::conflict(
            "PROJECT_ARCHIVED",
            "归档项目不能创建或发布任务",
        ));
    }
    Ok(())
}
fn find_project(conn: &Connection, id: &str) -> Result<Project> {
    conn.query_row(
        &format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id=?1"),
        [id],
        project_row,
    )
    .optional()?
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "PROJECT_NOT_FOUND", "项目不存在"))
}

fn find_bounty(conn: &Connection, id: &str) -> Result<Bounty> {
    let mut bounty = conn
        .query_row(
            &format!("SELECT {BOUNTY_COLUMNS} FROM {BOUNTY_SOURCE} WHERE b.id=?1"),
            [id],
            bounty_row,
        )
        .optional()?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "BOUNTY_NOT_FOUND", "任务不存在"))?;
    dependencies::hydrate(conn, &mut bounty)?;
    Ok(bounty)
}

fn list_bounties_on(conn: &Connection, filter: BountyFilter) -> Result<Vec<Bounty>> {
    // instr 按字面子串搜索，用户输入 % 和 _ 不会扩大匹配范围。
    let mut stmt = conn.prepare(&format!("SELECT {BOUNTY_COLUMNS} FROM {BOUNTY_SOURCE} WHERE (?1 OR b.status<>'DRAFT') AND (?2 IS NULL OR b.project_id=?2) AND (?3 IS NULL OR b.status=?3) AND (?4 IS NULL OR instr(lower(b.id),lower(?4))>0 OR instr(lower(b.title),lower(?4))>0 OR instr(lower(b.description),lower(?4))>0 OR instr(lower(b.acceptance_criteria),lower(?4))>0) ORDER BY b.created_at DESC,b.id"))?;
    let rows = stmt.query_map(
        params![
            filter.include_drafts,
            filter.project_id,
            filter.status.map(BountyStatus::as_str),
            filter.q
        ],
        bounty_row,
    )?;
    let mut bounties = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    for bounty in &mut bounties {
        dependencies::hydrate(conn, bounty)?;
    }
    Ok(bounties)
}

fn bounty_detail_on(conn: &Connection, id: &str) -> Result<BountyDetail> {
    let bounty = find_bounty(conn, id)?;
    let events = {
        let mut stmt = conn.prepare("SELECT id,bounty_id,event_type,actor,from_status,to_status,detail_json,created_at FROM bounty_events WHERE bounty_id=?1 ORDER BY id")?;
        let rows = stmt.query_map([id], |r| {
            Ok(BountyEvent {
                id: r.get(0)?,
                bounty_id: r.get(1)?,
                event_type: r.get(2)?,
                actor: r.get(3)?,
                from_status: optional_enum(r, 4)?,
                to_status: enum_value(r, 5)?,
                detail: json_value(r, 6)?,
                created_at: r.get(7)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let comments = comments::list(conn, id)?;
    let blocks = dependencies::related(conn, id, false)?;
    let child_bounties = dependencies::related(conn, id, true)?;
    Ok(BountyDetail {
        creator: None,
        blocks,
        child_bounties,
        bounty,
        events,
        comments,
    })
}

fn event(
    tx: &Transaction<'_>,
    b: &Bounty,
    event_type: &str,
    actor: &str,
    from: Option<BountyStatus>,
    detail: serde_json::Value,
) -> Result<()> {
    tx.execute("INSERT INTO bounty_events(bounty_id,event_type,actor,from_status,to_status,detail_json,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![b.id,event_type,actor,from.map(BountyStatus::as_str),b.status.as_str(),detail.to_string(),b.updated_at])?;
    Ok(())
}

fn decode<T: DeserializeOwned>(value: serde_json::Value, index: usize) -> rusqlite::Result<T> {
    serde_json::from_value(value).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(e))
    })
}
fn enum_value<T: DeserializeOwned>(r: &Row<'_>, i: usize) -> rusqlite::Result<T> {
    decode(json!(r.get::<_, String>(i)?), i)
}
fn optional_enum<T: DeserializeOwned>(r: &Row<'_>, i: usize) -> rusqlite::Result<Option<T>> {
    decode(json!(r.get::<_, Option<String>>(i)?), i)
}
fn json_value<T: DeserializeOwned>(r: &Row<'_>, i: usize) -> rusqlite::Result<T> {
    let value: String = r.get(i)?;
    serde_json::from_str(&value).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, Box::new(e))
    })
}
fn project_row(r: &Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get(0)?,
        name: r.get(1)?,
        icon: r.get(2)?,
        description: r.get(3)?,
        workspace_path: r.get(4)?,
        default_acceptance_criteria: r.get(5)?,
        agent_draft_policy: enum_value(r, 6)?,
        arbitrator_chain: json_value(r, 7)?,
        archived: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
    })
}
fn bounty_row(r: &Row<'_>) -> rusqlite::Result<Bounty> {
    Ok(Bounty {
        id: r.get(0)?,
        project_id: r.get(1)?,
        title: r.get(2)?,
        description: r.get(3)?,
        acceptance_criteria: r.get(4)?,
        difficulty: enum_value(r, 5)?,
        assignee_mode: enum_value(r, 6)?,
        assignee_min_tier: optional_enum(r, 7)?,
        assignee_agent_id: r.get(8)?,
        require_arbitration: r.get(9)?,
        status: enum_value(r, 10)?,
        created_by: r.get(11)?,
        lease_id: r.get(12)?,
        lease_agent_id: r.get(13)?,
        lease_expires_at: r.get(14)?,
        rejection_count: r.get(15)?,
        completion_note: r.get(16)?,
        published_at: r.get(17)?,
        withdrawn_at: r.get(18)?,
        cancelled_at: r.get(19)?,
        completed_at: r.get(20)?,
        created_at: r.get(21)?,
        updated_at: r.get(22)?,
        created_by_agent_id: r.get(23)?,
        created_by_agent_name: r.get(24)?,
        lease_agent_name: r.get(25)?,
        lease_agent_tier: optional_enum(r, 26)?,
        assignee_agent_name: r.get(27)?,
        assignee_agent_tier: optional_enum(r, 28)?,
        assignment_expires_at: r.get(29)?,
        completion_cost: r.get(30)?,
        active_duration_ms: r.get(31)?,
        active_started_at: r.get(32)?,
        comment_count: r.get(33)?,
        arbitrator_agent_id: r.get(34)?,
        arbitrator_agent_name: r.get(35)?,
        arbitrator_agent_tier: optional_enum(r, 36)?,
        arbitration_expires_at: r.get(37)?,
        quality_grade: optional_enum(r, 38)?,
        calibrated_difficulty: optional_enum(r, 39)?,
        draft_collaborator_agent_id: r.get(40)?,
        draft_collaborator_agent_name: r.get(41)?,
        draft_collaborator_agent_tier: optional_enum(r, 42)?,
        draft_needs_human_review: r.get(43)?,
        parent_bounty_id: r.get(44)?,
        blocked_by: vec![],
        blocked_by_summaries: vec![],
        is_blocked: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connections_enable_foreign_keys_wal_and_busy_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::open(dir.path().join("taskrail.db")).unwrap();
        let conn = repo.connect().unwrap();
        for (pragma, expected) in [("foreign_keys", 1), ("busy_timeout", 5000)] {
            assert_eq!(
                conn.query_row(&format!("PRAGMA {pragma}"), [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                expected
            );
        }
        assert_eq!(
            conn.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "wal"
        );
        assert!(conn.execute("INSERT INTO bounty_events(bounty_id,event_type,actor,to_status,created_at) VALUES('missing','created','human','DRAFT','now')",[]).is_err());
    }

    #[test]
    fn initialization_failure_rolls_back_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conflict.db");
        let conn = Connection::open(&path).unwrap();
        // 让建库后段的索引引用缺失列，验证前面创建的表和默认配置也会整体回滚。
        conn.execute_batch("CREATE TABLE bounty_dependencies(keep TEXT); INSERT INTO bounty_dependencies VALUES('preserved');").unwrap();
        for _ in 0..2 {
            assert!(Repository::open(&path).is_err());
            assert_eq!(
                conn.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
            assert_eq!(
                conn.query_row("SELECT keep FROM bounty_dependencies", [], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                "preserved"
            );
        }
    }
}
