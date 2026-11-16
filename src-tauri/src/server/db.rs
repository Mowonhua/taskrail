//! 文件职责：SQLite 迁移、项目存储与悬赏状态机；所有写入由仓储维护不变量。
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

mod agents;

/// 结构职责：可克隆的文件数据库入口；每次调用独立连接，可跨进程共享同一文件。
/// 约束条件：同步接口应在阻塞线程执行，不接受 :memory:，以保证多连接共享数据。
#[derive(Debug, Clone)]
pub struct Repository {
    path: PathBuf,
}

const PROJECT_COLUMNS: &str = "id,name,icon,description,workspace_path,default_acceptance_criteria,agent_draft_policy,arbitrator_chain_json,archived,created_at,updated_at";
const BOUNTY_COLUMNS: &str = "id,project_id,title,description,acceptance_criteria,difficulty,assignee_mode,assignee_min_tier,assignee_agent_id,require_arbitration,status,created_by,lease_id,lease_agent_id,lease_expires_at,rejection_count,completion_note,published_at,withdrawn_at,cancelled_at,completed_at,created_at,updated_at";

impl Repository {
    /// 函数职责：初始化文件、连接参数和版本化迁移；重复打开不重置数据。
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if path.as_os_str().is_empty() || path == Path::new(":memory:") {
            return Err(ApiError::invalid("db 必须是文件路径"));
        }
        let path = std::path::absolute(path).map_err(ApiError::internal)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(ApiError::internal)?;
        }
        let repo = Self { path };
        let mut conn = repo.connect()?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);")?;
        let version: i64 = tx.query_row(
            "SELECT COALESCE(MAX(version),0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )?;
        if version > 2 {
            return Err(ApiError::internal("数据库版本高于当前服务支持版本"));
        }
        if version == 0 {
            tx.execute_batch(include_str!("schema.sql"))?;
        } else if version == 1 {
            tx.execute_batch(include_str!("schema_v2.sql"))?;
        }
        if version < 2 {
            tx.execute(
                "INSERT INTO schema_migrations(version,applied_at) VALUES(2,?1)",
                [now()],
            )?;
        }
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

    pub fn create_project(&self, input: CreateProject) -> Result<Project> {
        validate_project(&input.name, input.icon.as_deref())?;
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
            if let Some(v) = patch.arbitrator_chain { p.arbitrator_chain = v; }
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
                    "项目已有悬赏，不能删除",
                ));
            }
            tx.execute("DELETE FROM projects WHERE id=?1", [id])?;
            Ok(())
        })
    }

    pub fn create_bounty(&self, input: CreateBounty) -> Result<Bounty> {
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
            active_project(tx, &input.project_id)?;
            let id = Uuid::new_v4().to_string();
            let timestamp = now();
            let published = (input.status == BountyStatus::Open).then_some(&timestamp);
            tx.execute("INSERT INTO bounties(id,project_id,title,description,acceptance_criteria,difficulty,assignee_mode,assignee_min_tier,assignee_agent_id,require_arbitration,status,created_by,published_at,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?14)",
                params![id,input.project_id,input.title,input.description,input.acceptance_criteria,input.difficulty.as_str(),input.assignee_mode.as_str(),input.assignee_min_tier.map(Tier::as_str),input.assignee_agent_id,input.require_arbitration,input.status.as_str(),input.created_by,published,timestamp])?;
            let b = find_bounty(tx, &id)?;
            event(tx, &b, "created", &b.created_by, None, json!({}))?;
            Ok(b)
        })
    }

    pub fn list_bounties(&self, filter: BountyFilter) -> Result<Vec<Bounty>> {
        let conn = self.connect()?;
        // instr 按字面子串搜索，用户输入 % 和 _ 不会扩大匹配范围。
        let mut stmt = conn.prepare(&format!("SELECT {BOUNTY_COLUMNS} FROM bounties WHERE (?1 OR status<>'DRAFT') AND (?2 IS NULL OR project_id=?2) AND (?3 IS NULL OR status=?3) AND (?4 IS NULL OR instr(lower(id),lower(?4))>0 OR instr(lower(title),lower(?4))>0 OR instr(lower(description),lower(?4))>0 OR instr(lower(acceptance_criteria),lower(?4))>0) ORDER BY created_at DESC,id"))?;
        let rows = stmt.query_map(
            params![
                filter.include_drafts,
                filter.project_id,
                filter.status.map(BountyStatus::as_str),
                filter.q
            ],
            bounty_row,
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 函数职责：在同一读快照中返回悬赏和完整事件，避免并发写入造成两者不一致。
    pub fn get_bounty_detail(&self, id: &str) -> Result<BountyDetail> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let bounty = find_bounty(&tx, id)?;
        let events = {
            let mut stmt = tx.prepare("SELECT id,bounty_id,event_type,actor,from_status,to_status,detail_json,created_at FROM bounty_events WHERE bounty_id=?1 ORDER BY id")?;
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
        tx.commit()?;
        Ok(BountyDetail { bounty, events })
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
    /// 函数职责：原子接取 OPEN 悬赏并签发递增令牌和十五分钟租约。
    /// 约束条件：此入口只维护租约原子性；Agent 身份、段位、订阅与仲裁准入由未来的授权层校验。
    pub fn try_claim_bounty(&self, id: &str, agent_id: &str) -> Result<Bounty> {
        if agent_id.trim().is_empty() {
            return Err(ApiError::invalid("agent_id 不能为空"));
        }
        self.transition(id, agent_id, Transition::Claim)
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
                Transition::Claim => {
                    if b.status == BountyStatus::InProgress || b.lease_agent_id.is_some() { return Err(already_claimed()); }
                    if b.status != BountyStatus::Open { return Err(invalid_transition()); }
                    (BountyStatus::InProgress,"claimed","")
                }
            };
            let timestamp = now();
            // WHERE 再次约束读取到的状态与 fencing token；事务同时保护事件写入。
            let changed = if matches!(action,Transition::Claim) {
                let expires = (Utc::now()+chrono::Duration::minutes(15)).to_rfc3339_opts(SecondsFormat::Millis,true);
                tx.execute("UPDATE bounties SET status='IN_PROGRESS',lease_id=lease_id+1,lease_agent_id=?2,lease_expires_at=?3,updated_at=?4 WHERE id=?1 AND status='OPEN' AND lease_agent_id IS NULL AND lease_id=?5",
                    params![id,actor,expires,timestamp,b.lease_id])?
            } else if matches!(action,Transition::Withdraw) {
                tx.execute("UPDATE bounties SET status='DRAFT',withdrawn_at=?2,updated_at=?2 WHERE id=?1 AND status='OPEN' AND lease_agent_id IS NULL AND lease_id=?3",params![id,timestamp,b.lease_id])?
            } else {
                // column 仅来自上方固定动作分支，不接受调用方提供 SQL 标识符。
                tx.execute(&format!("UPDATE bounties SET status=?2,{column}=?3,updated_at=?3 WHERE id=?1 AND status=?4 AND lease_id=?5"),params![id,to.as_str(),timestamp,b.status.as_str(),b.lease_id])?
            };
            if changed != 1 { return Err(invalid_transition()); }
            let updated = find_bounty(tx,id)?;
            let detail = if matches!(action,Transition::Claim) { json!({"lease_id":updated.lease_id,"lease_agent_id":updated.lease_agent_id,"lease_expires_at":updated.lease_expires_at}) } else { json!({}) };
            event(tx,&updated,event_type,actor,Some(b.status),detail)?;
            Ok(updated)
        })
    }
}

#[derive(Clone, Copy)]
enum Transition {
    Publish,
    Withdraw,
    Cancel,
    Claim,
}
fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
fn invalid_transition() -> ApiError {
    ApiError::conflict("INVALID_TRANSITION", "当前状态不允许此操作")
}
fn already_claimed() -> ApiError {
    ApiError::conflict("BOUNTY_ALREADY_CLAIMED", "悬赏已被接取，不能撤回或重复接取")
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
        return Err(ApiError::invalid("悬赏标题不能为空"));
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
            "归档项目不能创建或发布悬赏",
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
    conn.query_row(
        &format!("SELECT {BOUNTY_COLUMNS} FROM bounties WHERE id=?1"),
        [id],
        bounty_row,
    )
    .optional()?
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "BOUNTY_NOT_FOUND", "悬赏不存在"))
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
    fn future_schema_is_rejected_without_modification() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("taskrail.db");
        Repository::open(&path).unwrap();
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations(version,applied_at) VALUES(3,'future')",
            [],
        )
        .unwrap();
        assert_eq!(Repository::open(&path).unwrap_err().code, "INTERNAL_ERROR");
        assert_eq!(
            conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
    }
}
