#![cfg(not(target_os = "android"))]
//! 评论、冻结租约、回复恢复与有效耗时的事务集成验证。
use chrono::{SecondsFormat, Utc};
use reqwest::{Client, StatusCode};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use taskrail_lib::server::{
    self, db::Repository, models::*, RunningServer, ServerConfig, ServerMode,
};

struct Kernel {
    repo: Repository,
    project: Project,
    _dir: tempfile::TempDir,
}
impl Kernel {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::open(dir.path().join("comments.db")).unwrap();
        repo.onboarding(Some(UpdateOnboarding {
            lan_open_registration: true,
        }))
        .unwrap();
        let project = repo
            .create_project(serde_json::from_value(json!({"name":"comments"})).unwrap())
            .unwrap();
        Self {
            repo,
            project,
            _dir: dir,
        }
    }
    fn conn(&self) -> Connection {
        let conn = Connection::open(self.repo.path()).unwrap();
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        conn
    }
    fn agent(&self, name: &str) -> RegisteredAgent {
        self.repo
            .register_agent(serde_json::from_value(json!({"name":name})).unwrap())
            .unwrap()
    }
    fn bounty(&self) -> Bounty {
        self.repo.create_bounty(serde_json::from_value(json!({"project_id":self.project.id,"title":"task","difficulty":"C","assignee_mode":"any","status":"OPEN"})).unwrap()).unwrap()
    }
    fn snapshot(&self, id: &str) -> Bounty {
        self.repo.get_bounty_detail(id).unwrap().bounty
    }
    fn inbox(&self, agent: &str, kind: &str) -> Vec<AgentEvent> {
        self.repo
            .agent_events(agent, Some(0), 500)
            .unwrap()
            .events
            .into_iter()
            .filter(|e| e.event_type == kind)
            .collect()
    }
    async fn start(&self) -> RunningServer {
        server::start(ServerConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            db_path: self.repo.path().to_owned(),
            admin_token: "comments-admin".into(),
            mode: ServerMode::Standalone,
        })
        .await
        .unwrap()
    }
}
fn input(body: &str) -> CreateBountyComment {
    CreateBountyComment { body: body.into() }
}
fn question(lease_id: i64, body: &str) -> RequestInfoInput {
    RequestInfoInput {
        lease_id,
        question: body.into(),
    }
}
fn error<T: std::fmt::Debug>(result: server::error::Result<T>, status: StatusCode, code: &str) {
    let e = result.unwrap_err();
    assert_eq!(e.status, status);
    assert_eq!(e.code, code);
}

#[tokio::test]
async fn http_restore_cancelled_task_resets_execution_and_preserves_history() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let k = Kernel::new();
    let worker = k.agent("restore-worker");
    let bounty = k.bounty();
    let id = &bounty.id;
    let claimed = k.repo.agent_claim_bounty(id, &worker.agent.id).unwrap();
    k.repo
        .agent_request_info(id, &worker.agent.id, question(claimed.lease_id, "需要目录"))
        .unwrap();
    k.repo.cancel_bounty(id, "human").unwrap();

    let running = k.start().await;
    let client = Client::new();
    let admin = format!("{}/api/v1/bounties/{id}/restore-draft", running.info.base_url);
    let agent = format!("{}/api/v1/agent/bounties/{id}", running.info.base_url);
    let denied = client.post(&admin).bearer_auth(&worker.api_key).send().await.unwrap();
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    let restored = client.post(&admin).bearer_auth("comments-admin").send().await.unwrap();
    assert_eq!(restored.status(), StatusCode::OK);
    let restored: Bounty = restored.json().await.unwrap();
    assert_eq!(restored.id, bounty.id);
    assert_eq!(restored.status, BountyStatus::Draft);
    assert_eq!(restored.lease_id, claimed.lease_id);
    assert!(restored.lease_agent_id.is_none());
    assert!(restored.published_at.is_none());
    assert!(restored.cancelled_at.is_none());
    let detail = k.repo.get_bounty_detail(id).unwrap();
    assert_eq!(detail.comments[0].body, "需要目录");
    assert_eq!(detail.events.last().unwrap().event_type, "restored_to_draft");
    let hidden = client.get(&agent).bearer_auth(&worker.api_key).send().await.unwrap();
    assert_eq!(hidden.status(), StatusCode::NOT_FOUND);
    let repeated = client.post(&admin).bearer_auth("comments-admin").send().await.unwrap();
    assert_eq!(repeated.status(), StatusCode::CONFLICT);

    k.repo.publish_bounty(id, "human").unwrap();
    let reclaimed = k.repo.agent_claim_bounty(id, &worker.agent.id).unwrap();
    assert!(reclaimed.lease_id > claimed.lease_id);
    error(
        k.repo.agent_heartbeat_bounty(id, &worker.agent.id, LeaseHeartbeat { lease_id: claimed.lease_id }),
        StatusCode::CONFLICT,
        "STALE_LEASE",
    );
    running.shutdown().await.unwrap();
}

#[test]
fn waiting_freezes_lease_and_counts_only_active_segments() {
    let k = Kernel::new();
    let a = k.agent("worker");
    let b = k.bounty();
    let b = k.repo.agent_claim_bounty(&b.id, &a.agent.id).unwrap();
    assert!(b.active_started_at.is_some());
    let start =
        (Utc::now() - chrono::Duration::seconds(10)).to_rfc3339_opts(SecondsFormat::Millis, true);
    k.conn()
        .execute(
            "UPDATE bounties SET active_started_at=?2 WHERE id=?1",
            params![b.id, start],
        )
        .unwrap();
    let body = "  ## Question\n\n$E=mc^2$\n```rust\nfn main() {}\n```  ";
    let waiting = k
        .repo
        .agent_request_info(&b.id, &a.agent.id, question(b.lease_id, body))
        .unwrap();
    assert_eq!(waiting.status, BountyStatus::WaitingInfo);
    assert_eq!(waiting.comment_count, 1);
    assert!(waiting.active_started_at.is_none());
    assert!(waiting.lease_expires_at.is_none());
    assert!((10_000..12_000).contains(&waiting.active_duration_ms));
    let detail = k.repo.get_bounty_detail(&b.id).unwrap();
    assert_eq!(detail.comments[0].body, body.trim());
    assert_eq!(detail.comments[0].kind, "question");
    assert_eq!(detail.events.last().unwrap().event_type, "info_requested");
    k.conn().execute("UPDATE bounties SET lease_expires_at='2000-01-01T00:00:00.000Z',updated_at='2000-01-01T00:00:00.000Z' WHERE id=?1",[&b.id]).unwrap();
    for _ in 0..20 {
        k.repo.sweep_leases().unwrap();
    }
    assert_eq!(k.snapshot(&b.id).status, BountyStatus::WaitingInfo);
    assert_eq!(
        k.snapshot(&b.id).active_duration_ms,
        waiting.active_duration_ms
    );
    assert!(k.inbox(&a.agent.id, "lease.expiring").is_empty());
    assert!(k.inbox(&a.agent.id, "lease.expired").is_empty());
    let own = k
        .repo
        .agent_create_comment(&b.id, &a.agent.id, input("more context"))
        .unwrap();
    assert_eq!(own.bounty.status, BountyStatus::WaitingInfo);
    assert_eq!(own.comment.kind, "comment");
    let resumed = k
        .repo
        .create_bounty_comment(&b.id, input("answer"))
        .unwrap();
    assert_eq!(resumed.bounty.status, BountyStatus::InProgress);
    assert_eq!(resumed.comment.kind, "reply");
    assert_eq!(resumed.bounty.lease_id, b.lease_id);
    assert_eq!(
        resumed.bounty.active_duration_ms,
        waiting.active_duration_ms
    );
    let expiry =
        chrono::DateTime::parse_from_rfc3339(resumed.bounty.lease_expires_at.as_ref().unwrap())
            .unwrap();
    assert!((890..=900).contains(&(expiry.with_timezone(&Utc) - Utc::now()).num_seconds()));
    for kind in ["comment.replied", "task.resumed"] {
        let e = k.inbox(&a.agent.id, kind);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].data["body"], "answer");
        assert_eq!(e[0].data["comment_id"], resumed.comment.id);
        assert_eq!(e[0].data["bounty"]["status"], "IN_PROGRESS");
    }
    let start =
        (Utc::now() - chrono::Duration::seconds(20)).to_rfc3339_opts(SecondsFormat::Millis, true);
    k.conn()
        .execute(
            "UPDATE bounties SET active_started_at=?2 WHERE id=?1",
            params![b.id, start],
        )
        .unwrap();
    let done = k
        .repo
        .agent_complete_bounty(
            &b.id,
            &a.agent.id,
            CompleteBounty {
                lease_id: b.lease_id,
                completion_note: None,
                completion_cost: None,
            },
        )
        .unwrap();
    assert!((30_000..34_000).contains(&done.active_duration_ms));
    assert!(done.active_started_at.is_none());
}

#[test]
fn fencing_order_and_invalid_questions_leave_all_writes_unchanged() {
    let k = Kernel::new();
    let a = k.agent("worker");
    let other = k.agent("other");
    let b = k.bounty();
    let b = k.repo.agent_claim_bounty(&b.id, &a.agent.id).unwrap();
    for body in ["", " \n\t "] {
        error(
            k.repo
                .agent_request_info(&b.id, &a.agent.id, question(b.lease_id, body)),
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
        );
    }
    error(
        k.repo
            .agent_request_info(&b.id, &other.agent.id, question(b.lease_id + 1, "")),
        StatusCode::CONFLICT,
        "STALE_LEASE",
    );
    error(
        k.repo
            .agent_request_info(&b.id, &other.agent.id, question(b.lease_id, "")),
        StatusCode::FORBIDDEN,
        "LEASE_OWNER_MISMATCH",
    );
    assert_eq!(k.snapshot(&b.id).comment_count, 0);
    assert_eq!(k.repo.get_bounty_detail(&b.id).unwrap().events.len(), 2);
    k.conn()
        .execute(
            "UPDATE bounties SET lease_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
            [&b.id],
        )
        .unwrap();
    error(
        k.repo
            .agent_request_info(&b.id, &a.agent.id, question(b.lease_id, "?")),
        StatusCode::CONFLICT,
        "STALE_LEASE",
    );
    k.conn()
        .execute(
            "UPDATE bounties SET status='WAITING_INFO' WHERE id=?1",
            [&b.id],
        )
        .unwrap();
    error(
        k.repo
            .agent_request_info(&b.id, &a.agent.id, question(b.lease_id, "?")),
        StatusCode::CONFLICT,
        "INVALID_TRANSITION",
    );
    k.conn()
        .execute("UPDATE bounties SET status='DRAFT' WHERE id=?1", [&b.id])
        .unwrap();
    error(
        k.repo
            .agent_request_info(&b.id, &other.agent.id, question(b.lease_id + 1, "?")),
        StatusCode::NOT_FOUND,
        "BOUNTY_NOT_FOUND",
    );
    k.conn()
        .execute(
            "UPDATE bounties SET status='OPEN',published_at=NULL WHERE id=?1",
            [&b.id],
        )
        .unwrap();
    error(
        k.repo
            .agent_request_info(&b.id, &a.agent.id, question(b.lease_id, "?")),
        StatusCode::NOT_FOUND,
        "BOUNTY_NOT_FOUND",
    );
    k.conn()
        .execute(
            "UPDATE bounties SET published_at='old',lease_agent_id=NULL WHERE id=?1",
            [&b.id],
        )
        .unwrap();
    error(
        k.repo
            .agent_request_info(&b.id, &a.agent.id, question(b.lease_id, "?")),
        StatusCode::CONFLICT,
        "STALE_LEASE",
    );
}

#[test]
fn publisher_reply_resumes_and_inactive_recipients_receive_no_events() {
    let k = Kernel::new();
    let publisher = k.agent("publisher");
    let worker = k.agent("worker");
    let b=k.repo.agent_create_bounty(&publisher.agent.id,serde_json::from_value(json!({"project_id":k.project.id,"title":"question","difficulty":"C","assignee_mode":"any"})).unwrap()).unwrap();
    let b = k.repo.agent_claim_bounty(&b.id, &worker.agent.id).unwrap();
    k.repo
        .agent_request_info(&b.id, &worker.agent.id, question(b.lease_id, "question"))
        .unwrap();
    let events = k.inbox(&publisher.agent.id, "task.info_requested");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data["body"], "question");
    let reply = k
        .repo
        .agent_create_comment(&b.id, &publisher.agent.id, input("publisher answer"))
        .unwrap();
    assert_eq!(reply.bounty.status, BountyStatus::InProgress);
    assert_eq!(
        reply.comment.author_id.as_deref(),
        Some(publisher.agent.id.as_str())
    );
    assert_eq!(reply.comment.kind, "reply");
    assert_eq!(reply.bounty.lease_agent_id, b.lease_agent_id);
    assert_eq!(reply.bounty.lease_id, b.lease_id);
    let detail = k.repo.get_bounty_detail(&b.id).unwrap();
    let event = detail.events.last().unwrap();
    assert_eq!(event.event_type, "resumed");
    assert_eq!(event.actor, format!("agent:{}", publisher.agent.id));
    assert_eq!(event.detail["comment_id"], reply.comment.id);
    assert_eq!(
        event.detail["lease_expires_at"],
        json!(reply.bounty.lease_expires_at)
    );
    k.repo
        .agent_request_info(&b.id, &worker.agent.id, question(b.lease_id, "again"))
        .unwrap();
    k.repo
        .set_agent_status(&worker.agent.id, AgentStatus::Disabled)
        .unwrap();
    k.repo
        .create_bounty_comment(&b.id, input("human answer"))
        .unwrap();
    k.repo
        .set_agent_status(&worker.agent.id, AgentStatus::Active)
        .unwrap();
    assert_eq!(k.inbox(&worker.agent.id, "comment.replied").len(), 1);
    assert_eq!(k.inbox(&worker.agent.id, "task.resumed").len(), 1);
    k.repo
        .set_agent_status(&publisher.agent.id, AgentStatus::Revoked)
        .unwrap();
    k.repo
        .agent_request_info(&b.id, &worker.agent.id, question(b.lease_id, "last"))
        .unwrap();
    assert_eq!(k.conn().query_row("SELECT COUNT(*) FROM agent_events WHERE agent_id=?1 AND event_type='task.info_requested'",[&publisher.agent.id],|r|r.get::<_,i64>(0)).unwrap(),2);
}

#[test]
fn cancellation_preserves_fencing_and_active_total_but_clears_deadline() {
    let k = Kernel::new();
    let worker = k.agent("worker");
    let b = k.bounty();
    let b = k.repo.agent_claim_bounty(&b.id, &worker.agent.id).unwrap();
    let waiting = k
        .repo
        .agent_request_info(&b.id, &worker.agent.id, question(b.lease_id, "why?"))
        .unwrap();
    let cancelled = k.repo.cancel_bounty(&b.id, "human").unwrap();
    assert_eq!(cancelled.status, BountyStatus::Cancelled);
    assert_eq!(cancelled.lease_id, b.lease_id);
    assert_eq!(cancelled.lease_agent_id, b.lease_agent_id);
    assert!(cancelled.lease_expires_at.is_none());
    assert!(cancelled.active_started_at.is_none());
    assert_eq!(cancelled.active_duration_ms, waiting.active_duration_ms);
    assert_eq!(k.inbox(&worker.agent.id, "task.cancelled").len(), 1);
    error(
        k.repo.create_bounty_comment(&b.id, input("reply")),
        StatusCode::CONFLICT,
        "INVALID_TRANSITION",
    );
    error(
        k.repo
            .agent_create_comment(&b.id, &worker.agent.id, input("reply")),
        StatusCode::CONFLICT,
        "INVALID_TRANSITION",
    );
    assert_eq!(k.snapshot(&b.id).comment_count, 1);
}

#[test]
fn each_claim_cycle_starts_from_zero_after_abandon_expiry_or_release() {
    let k = Kernel::new();
    let a = k.agent("worker");
    for action in ["abandon", "expire", "release", "waiting-release"] {
        let b = k.bounty();
        let b = k.repo.agent_claim_bounty(&b.id, &a.agent.id).unwrap();
        k.conn().execute("UPDATE bounties SET active_duration_ms=1200,active_started_at='2020-01-01T00:00:00.000Z' WHERE id=?1",[&b.id]).unwrap();
        let open = match action {
            "abandon" => k
                .repo
                .agent_abandon_bounty(
                    &b.id,
                    &a.agent.id,
                    AbandonBounty {
                        lease_id: b.lease_id,
                        reason: None,
                    },
                )
                .unwrap(),
            "expire" => {
                k.conn().execute("UPDATE bounties SET lease_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",[&b.id]).unwrap();
                k.repo.sweep_leases().unwrap();
                k.snapshot(&b.id)
            }
            "waiting-release" => {
                k.repo
                    .agent_request_info(&b.id, &a.agent.id, question(b.lease_id, "?"))
                    .unwrap();
                k.repo.admin_release_lease(&b.id).unwrap()
            }
            _ => k.repo.admin_release_lease(&b.id).unwrap(),
        };
        let event = k
            .repo
            .get_bounty_detail(&b.id)
            .unwrap()
            .events
            .pop()
            .unwrap();
        assert!(event.detail["active_duration_ms"].as_i64().unwrap() >= 1200);
        assert_eq!(open.status, BountyStatus::Open);
        assert_eq!(open.active_duration_ms, 0);
        assert!(open.active_started_at.is_none());
        let next = k.repo.agent_claim_bounty(&b.id, &a.agent.id).unwrap();
        assert_eq!(next.lease_id, b.lease_id + 1);
        assert_eq!(next.active_duration_ms, 0);
        assert!(next.active_started_at.is_some());
    }
}

#[test]
fn comments_apply_visibility_draft_isolation_names_and_unique_notifications() {
    let k = Kernel::new();
    let publisher = k.agent("publisher");
    let other = k.agent("other");
    let b=k.repo.agent_create_bounty(&publisher.agent.id,serde_json::from_value(json!({"project_id":k.project.id,"title":"self","difficulty":"C","assignee_mode":"any"})).unwrap()).unwrap();
    let b = k
        .repo
        .agent_claim_bounty(&b.id, &publisher.agent.id)
        .unwrap();
    k.repo
        .agent_request_info(&b.id, &publisher.agent.id, question(b.lease_id, "self?"))
        .unwrap();
    let own = k
        .repo
        .agent_create_comment(&b.id, &publisher.agent.id, input("self comment"))
        .unwrap();
    assert_eq!(own.bounty.status, BountyStatus::WaitingInfo);
    assert!(k
        .inbox(&publisher.agent.id, "task.info_requested")
        .is_empty());
    k.repo
        .agent_create_comment(&b.id, &other.agent.id, input("extra"))
        .unwrap();
    assert_eq!(k.inbox(&publisher.agent.id, "comment.added").len(), 1);
    k.conn()
        .execute(
            "UPDATE agents SET name='renamed' WHERE id=?1",
            [&publisher.agent.id],
        )
        .unwrap();
    let list = k.repo.bounty_comments(&b.id).unwrap();
    assert_eq!(list[0].author_name, "renamed");
    assert!(list.windows(2).all(|w| w[0].id < w[1].id));
    assert_eq!(
        k.conn()
            .query_row(
                "SELECT author_name FROM bounty_comments WHERE id=?1",
                [list[0].id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "publisher"
    );
    // 历史作者缺失时使用落库名称，不需要查询凭证或内部分数。
    k.conn()
        .execute(
            "UPDATE bounty_comments SET author_id='historical' WHERE id=?1",
            [list[0].id],
        )
        .unwrap();
    assert_eq!(
        k.repo.bounty_comments(&b.id).unwrap()[0].author_name,
        "publisher"
    );
    k.conn()
        .execute("UPDATE bounties SET status='DRAFT' WHERE id=?1", [&b.id])
        .unwrap();
    error(
        k.repo.agent_bounty_comments(&b.id, &publisher.agent.id),
        StatusCode::NOT_FOUND,
        "BOUNTY_NOT_FOUND",
    );
    error(
        k.repo
            .agent_create_comment(&b.id, &publisher.agent.id, input("hidden")),
        StatusCode::NOT_FOUND,
        "BOUNTY_NOT_FOUND",
    );
    let before = k
        .repo
        .agent_events(&publisher.agent.id, Some(0), 500)
        .unwrap()
        .events
        .len();
    k.repo
        .create_bounty_comment(&b.id, input("private draft comment"))
        .unwrap();
    assert_eq!(
        k.repo
            .agent_events(&publisher.agent.id, Some(0), 500)
            .unwrap()
            .events
            .len(),
        before
    );
    k.conn().execute("UPDATE bounties SET status='OPEN',assignee_mode='specific',assignee_agent_id=?2 WHERE id=?1",params![b.id,publisher.agent.id]).unwrap();
    error(
        k.repo.agent_bounty_comments(&b.id, &other.agent.id),
        StatusCode::NOT_FOUND,
        "BOUNTY_NOT_FOUND",
    );
    error(
        k.repo
            .agent_create_comment(&b.id, &other.agent.id, input("hidden")),
        StatusCode::NOT_FOUND,
        "BOUNTY_NOT_FOUND",
    );
    k.repo
        .update_agent_subscriptions(
            &other.agent.id,
            serde_json::from_value(json!({"subscribe_all_projects":false,"project_ids":[]}))
                .unwrap(),
        )
        .unwrap();
    let public = k.bounty();
    error(
        k.repo.agent_bounty_comments(&public.id, &other.agent.id),
        StatusCode::NOT_FOUND,
        "BOUNTY_NOT_FOUND",
    );
}

#[test]
fn comment_history_and_notifications_fail_as_one_transaction() {
    for target in ["bounty_comments", "bounty_events", "agent_events"] {
        let k = Kernel::new();
        let a = k.agent("worker");
        let b = k.bounty();
        let b = k.repo.agent_claim_bounty(&b.id, &a.agent.id).unwrap();
        k.conn().execute_batch(&format!("CREATE TRIGGER fail_write BEFORE INSERT ON {target} BEGIN SELECT RAISE(ABORT,'injected'); END;")).unwrap();
        if target != "agent_events" {
            assert!(k
                .repo
                .agent_request_info(&b.id, &a.agent.id, question(b.lease_id, "?"))
                .is_err());
            let after = k.snapshot(&b.id);
            assert_eq!(after.status, BountyStatus::InProgress);
            assert_eq!(after.comment_count, 0);
            assert_eq!(after.active_duration_ms, 0);
            assert_eq!(after.active_started_at, b.active_started_at);
        }
        k.conn().execute_batch("DROP TRIGGER fail_write;").unwrap();
        k.repo
            .agent_request_info(&b.id, &a.agent.id, question(b.lease_id, "?"))
            .unwrap();
        let before = k.repo.get_bounty_detail(&b.id).unwrap();
        k.conn().execute_batch(&format!("CREATE TRIGGER fail_write BEFORE INSERT ON {target} BEGIN SELECT RAISE(ABORT,'injected'); END;")).unwrap();
        assert!(k
            .repo
            .create_bounty_comment(&b.id, input("answer"))
            .is_err());
        let after = k.repo.get_bounty_detail(&b.id).unwrap();
        assert_eq!(
            serde_json::to_value(after).unwrap(),
            serde_json::to_value(before).unwrap()
        );
        assert!(k.inbox(&a.agent.id, "comment.replied").is_empty());
        assert!(k.inbox(&a.agent.id, "task.resumed").is_empty());
    }
}

async fn request(
    client: &Client,
    server: &RunningServer,
    method: reqwest::Method,
    key: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = client
        .request(method, format!("{}/api/v1{path}", server.info.base_url))
        .bearer_auth(key);
    if let Some(body) = body {
        req = req.json(&body);
    }
    let response = req.send().await.unwrap();
    let status = response.status();
    (status, response.json().await.unwrap())
}
fn assert_public(value: &Value) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                assert!(
                    !["mmr", "api_key", "api_key_hash", "webhook_secret"].contains(&key.as_str()),
                    "private field {key}"
                );
                assert_public(value);
            }
        }
        Value::Array(values) => {
            for v in values {
                assert_public(v);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn http_contracts_auth_validation_details_and_draft_privacy() {
    const GET: reqwest::Method = reqwest::Method::GET;
    const POST: reqwest::Method = reqwest::Method::POST;
    let k = Kernel::new();
    let a = k.agent("worker");
    let other = k.agent("other");
    let b = k.bounty();
    let server = k.start().await;
    let client = Client::new();
    let admin = format!("/bounties/{}/comments", b.id);
    let agent = format!("/agent/bounties/{}/comments", b.id);
    let ask = format!("/agent/bounties/{}/request-info", b.id);
    for (method, key, path) in [
        (GET, "", &admin),
        (POST, a.api_key.as_str(), &admin),
        (GET, a.api_key.as_str(), &admin),
        (GET, "comments-admin", &agent),
        (POST, "comments-admin", &ask),
    ] {
        assert_eq!(
            request(
                &client,
                &server,
                method,
                key,
                path,
                Some(json!({"body":"x"}))
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
    }
    for (path, body) in [
        (&admin, json!({"body":" \n "})),
        (&admin, json!({"body":"x","author_type":"agent"})),
        (&admin, json!({"body":null})),
        (&admin, json!({"body":"x","kind":"reply"})),
    ] {
        let (status, v) = request(&client, &server, POST, "comments-admin", path, Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(v["error"]["code"], "VALIDATION_ERROR");
    }
    let (status, comment) = request(
        &client,
        &server,
        POST,
        &a.api_key,
        &agent,
        Some(json!({"body":"**hello** $a^2$"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(comment["comment"]["kind"], "comment");
    assert_eq!(comment["bounty"]["comment_count"], 1);
    assert_public(&comment);
    let b = k.repo.agent_claim_bounty(&b.id, &a.agent.id).unwrap();
    for body in [
        json!({"lease_id":b.lease_id,"question":" "}),
        json!({"lease_id":b.lease_id,"question":"?","kind":"question"}),
        json!({"lease_id":"1","question":"?"}),
        json!({"question":"?"}),
    ] {
        assert_eq!(
            request(&client, &server, POST, &a.api_key, &ask, Some(body))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    let (status, v) = request(
        &client,
        &server,
        POST,
        &other.api_key,
        &ask,
        Some(json!({"lease_id":b.lease_id,"question":"?"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(v["error"]["code"], "LEASE_OWNER_MISMATCH");
    let (status, v) = request(
        &client,
        &server,
        POST,
        &a.api_key,
        &ask,
        Some(json!({"lease_id":b.lease_id,"question":"question $x$"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["status"], "WAITING_INFO");
    assert!(v.get("bounty").is_none());
    assert_public(&v);
    let (status, v) = request(
        &client,
        &server,
        POST,
        "comments-admin",
        &admin,
        Some(json!({"body":"answer"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(v["bounty"]["status"], "IN_PROGRESS");
    assert_eq!(v["comment"]["author_id"], Value::Null);
    assert_eq!(v["comment"]["author_name"], "human");
    for (key, prefix) in [
        ("comments-admin", "/bounties"),
        (a.api_key.as_str(), "/agent/bounties"),
    ] {
        let (_, detail) = request(
            &client,
            &server,
            GET,
            key,
            &format!("{prefix}/{}", b.id),
            None,
        )
        .await;
        assert_eq!(detail["bounty"]["comment_count"], 3);
        assert_eq!(detail["comments"].as_array().unwrap().len(), 3);
        assert_public(&detail);
        let (status, comments) = request(
            &client,
            &server,
            GET,
            key,
            &format!("{prefix}/{}/comments", b.id),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(comments, detail["comments"]);
    }
    k.conn()
        .execute("UPDATE bounties SET status='DRAFT' WHERE id=?1", [&b.id])
        .unwrap();
    for (method, path, body) in [
        (GET, &agent, None),
        (POST, &agent, Some(json!({"body":"hidden"}))),
        (
            POST,
            &ask,
            Some(json!({"lease_id":b.lease_id,"question":"?"})),
        ),
    ] {
        let (status, v) = request(&client, &server, method, &a.api_key, path, body).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(v["error"]["code"], "BOUNTY_NOT_FOUND");
    }
    for action in ["disable", "revoke"] {
        k.repo
            .set_agent_status(
                &a.agent.id,
                if action == "disable" {
                    AgentStatus::Disabled
                } else {
                    AgentStatus::Revoked
                },
            )
            .unwrap();
        assert_eq!(
            request(&client, &server, GET, &a.api_key, &agent, None)
                .await
                .0,
            if action == "disable" {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::UNAUTHORIZED
            }
        );
    }
    let skill = client
        .get(format!("{}/skill.md", server.info.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    for required in [
        "taskrail_list_comments",
        "taskrail_request_info",
        "comment.replied",
        "task.resumed",
        "active_duration_ms",
    ] {
        assert!(skill.contains(required), "missing {required}");
    }
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn specific_publisher_can_read_and_reply_while_third_party_stays_hidden() {
    const GET: reqwest::Method = reqwest::Method::GET;
    const POST: reqwest::Method = reqwest::Method::POST;
    let k = Kernel::new();
    let publisher = k.agent("publisher");
    let worker = k.agent("worker");
    let third_party = k.agent("third-party");
    k.repo
        .update_agent_subscriptions(
            &publisher.agent.id,
            serde_json::from_value(json!({"subscribe_all_projects":false})).unwrap(),
        )
        .unwrap();
    let server = k.start().await;
    let client = Client::new();
    let (status, bounty) = request(
        &client,
        &server,
        POST,
        &publisher.api_key,
        "/agent/bounties",
        Some(json!({"project_id":k.project.id,"title":"specific question","difficulty":"C","assignee_mode":"specific","assignee_agent_id":worker.agent.id})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{bounty}");
    assert_eq!(bounty["status"], "OPEN");
    assert!(bounty["published_at"].is_string());
    let id = bounty["id"].as_str().unwrap();
    let path = format!("/agent/bounties/{id}");
    let (status, claimed) = request(
        &client,
        &server,
        POST,
        &worker.api_key,
        &format!("{path}/claim"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let (status, waiting) = request(
        &client,
        &server,
        POST,
        &worker.api_key,
        &format!("{path}/request-info"),
        Some(json!({"lease_id":claimed["lease_id"],"question":"Which format?"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{waiting}");
    assert_eq!(waiting["status"], "WAITING_INFO");
    let notifications = k.inbox(&publisher.agent.id, "task.info_requested");
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].data["bounty"], waiting);
    for agent in [&publisher, &worker, &third_party] {
        let (status, list) = request(
            &client,
            &server,
            GET,
            &agent.api_key,
            "/agent/bounties",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            list.as_array().unwrap().iter().any(|b| b["id"] == id),
            agent.agent.id != third_party.agent.id,
            "{list}"
        );
        for endpoint in [&path, &format!("{path}/comments")] {
            let (status, value) =
                request(&client, &server, GET, &agent.api_key, endpoint, None).await;
            if agent.agent.id == third_party.agent.id {
                assert_eq!(status, StatusCode::NOT_FOUND);
                assert_eq!(value["error"]["code"], "BOUNTY_NOT_FOUND");
            } else {
                assert_eq!(status, StatusCode::OK, "{value}");
                let comments = if endpoint == &path {
                    &value["comments"]
                } else {
                    &value
                };
                assert_eq!(comments[0]["body"], "Which format?");
            }
        }
    }
    let (status, hidden) = request(
        &client,
        &server,
        POST,
        &third_party.api_key,
        &format!("{path}/comments"),
        Some(json!({"body":"unauthorized reply"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(hidden["error"]["code"], "BOUNTY_NOT_FOUND");
    assert_eq!(k.snapshot(id).status, BountyStatus::WaitingInfo);
    let (status, reply) = request(
        &client,
        &server,
        POST,
        &publisher.api_key,
        &format!("{path}/comments"),
        Some(json!({"body":"Use Markdown."})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    assert_eq!(reply["bounty"]["status"], "IN_PROGRESS");
    assert_eq!(reply["bounty"]["lease_id"], claimed["lease_id"]);
    assert_eq!(reply["bounty"]["lease_agent_id"], worker.agent.id);
    assert!(reply["bounty"]["lease_expires_at"].is_string());
    assert_eq!(reply["comment"]["kind"], "reply");
    let mut wakeup_ids = Vec::new();
    for kind in ["comment.replied", "task.resumed"] {
        let events = k.inbox(&worker.agent.id, kind);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data["bounty"], reply["bounty"]);
        assert_eq!(events[0].data["comment"], reply["comment"]);
        wakeup_ids.push(events[0].event_id);
    }
    assert!(wakeup_ids[0] < wakeup_ids[1]);
    assert!(k
        .repo
        .agent_events(&third_party.agent.id, Some(0), 500)
        .unwrap()
        .events
        .is_empty());
    server.shutdown().await.unwrap();
}

async fn assert_http_activity(
    client: &Client,
    server: &RunningServer,
    agent: &RegisteredAgent,
    expected: i64,
) {
    let (status, me) = request(
        client,
        server,
        reqwest::Method::GET,
        &agent.api_key,
        "/agent/me",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["active_lease_count"], expected);
    for (query, included) in [
        ("", true),
        ("?activity=busy", expected > 0),
        ("?activity=idle", expected == 0),
    ] {
        let (status, list) = request(
            client,
            server,
            reqwest::Method::GET,
            "comments-admin",
            &format!("/agents{query}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let found = list
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == agent.agent.id);
        assert_eq!(found.is_some(), included, "{query}: {list}");
        if let Some(summary) = found {
            assert_eq!(summary["active_lease_count"], expected);
        }
    }
}

#[tokio::test]
async fn frozen_lease_stays_busy_until_cancelled_or_released() {
    let k = Kernel::new();
    let worker = k.agent("worker");
    let idle = k.agent("idle");
    let server = k.start().await;
    let client = Client::new();
    assert_http_activity(&client, &server, &worker, 0).await;
    for (state, action) in [
        ("WAITING_INFO", "cancel"),
        ("WAITING_INFO", "release-lease"),
        ("PENDING_ARBITRATION", "release-lease"),
    ] {
        let bounty = k.bounty();
        let bounty = k
            .repo
            .agent_claim_bounty(&bounty.id, &worker.agent.id)
            .unwrap();
        assert_http_activity(&client, &server, &worker, 1).await;
        k.repo
            .agent_request_info(
                &bounty.id,
                &worker.agent.id,
                question(bounty.lease_id, "Need input"),
            )
            .unwrap();
        // 仲裁提交接口尚未实现，直接构造保留持有者、无期限的冻结快照。
        if state == "PENDING_ARBITRATION" {
            k.conn()
                .execute(
                    "UPDATE bounties SET status=?2 WHERE id=?1",
                    params![bounty.id, state],
                )
                .unwrap();
        }
        assert!(k.snapshot(&bounty.id).lease_expires_at.is_none());
        assert_http_activity(&client, &server, &worker, 1).await;
        assert_http_activity(&client, &server, &idle, 0).await;
        // 冻结状态不使用期限，即使历史快照保留了过期值也仍持有租约。
        k.conn()
            .execute(
                "UPDATE bounties SET lease_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
                [&bounty.id],
            )
            .unwrap();
        assert_http_activity(&client, &server, &worker, 1).await;
        let (status, result) = request(
            &client,
            &server,
            reqwest::Method::POST,
            "comments-admin",
            &format!("/bounties/{}/{action}", bounty.id),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_http_activity(&client, &server, &worker, 0).await;
    }
    server.shutdown().await.unwrap();
}

async fn next_sse(response: &mut reqwest::Response, buffer: &mut String) -> Value {
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        loop {
            if let Some(end) = buffer.find("\n\n") {
                let frame = buffer[..end].to_owned();
                buffer.drain(..end + 2);
                if let Some(data) = frame.lines().find_map(|line| line.strip_prefix("data: ")) {
                    let event: Value = serde_json::from_str(data).unwrap();
                    assert!(frame.contains(&format!("id: {}", event["event_id"])));
                    return event;
                }
            }
            let bytes = response.chunk().await.unwrap().expect("SSE closed");
            buffer.push_str(std::str::from_utf8(&bytes).unwrap());
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn human_and_publisher_replies_reach_inbox_sse_and_signed_webhook() {
    use axum::{extract::State, http::HeaderMap, routing::post, Router};

    use sha2::{Digest, Sha256};
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(HeaderMap, String)>();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route(
            "/events",
            post(
                |State(tx): State<tokio::sync::mpsc::UnboundedSender<(HeaderMap, String)>>,
                 headers: HeaderMap,
                 body: String| async move {
                    tx.send((headers, body)).unwrap();
                    StatusCode::OK
                },
            ),
        )
        .with_state(tx);
    let webhook = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let k = Kernel::new();
    let publisher = k.agent("publisher");
    let worker = k.agent("worker");
    k.repo.update_webhook(&worker.agent.id,serde_json::from_value(json!({"webhook_url":format!("http://{address}/events"),"webhook_secret":"test-comments-secret"})).unwrap()).unwrap();
    let server = k.start().await;
    let client = Client::new();
    for human in [true, false] {
        let b=k.repo.agent_create_bounty(&publisher.agent.id,serde_json::from_value(json!({"project_id":k.project.id,"title":"wake","difficulty":"C","assignee_mode":"any"})).unwrap()).unwrap();
        let b = k.repo.agent_claim_bounty(&b.id, &worker.agent.id).unwrap();
        k.repo
            .agent_request_info(&b.id, &worker.agent.id, question(b.lease_id, "what?"))
            .unwrap();
        // 伪造过期时间并让真实后台扫描多轮，仍必须保持冻结，且不产生警告或回收。
        k.conn()
            .execute(
                "UPDATE bounties SET lease_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
                [&b.id],
            )
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(350)).await;
        assert_eq!(k.snapshot(&b.id).status, BountyStatus::WaitingInfo);
        assert!(k.inbox(&worker.agent.id, "lease.expiring").is_empty());
        assert!(k.inbox(&worker.agent.id, "lease.expired").is_empty());
        let cursor = k
            .repo
            .agent_events(&worker.agent.id, Some(0), 500)
            .unwrap()
            .latest_event_id;
        let mut sse = client
            .get(format!(
                "{}/api/v1/agent/stream?cursor={cursor}",
                server.info.base_url
            ))
            .bearer_auth(&worker.api_key)
            .send()
            .await
            .unwrap();
        assert_eq!(sse.status(), StatusCode::OK);
        let path = if human {
            format!("/bounties/{}/comments", b.id)
        } else {
            format!("/agent/bounties/{}/comments", b.id)
        };
        let (status, result) = request(
            &client,
            &server,
            reqwest::Method::POST,
            if human {
                "comments-admin"
            } else {
                &publisher.api_key
            },
            &path,
            Some(json!({"body":"answer $a=b$"})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let mut buffer = String::new();
        let mut expected = Vec::new();
        for kind in ["comment.replied", "task.resumed"] {
            let event = next_sse(&mut sse, &mut buffer).await;
            assert_eq!(event["event_type"], kind);
            assert_eq!(event["bounty_id"], b.id);
            assert_eq!(event["data"]["comment_id"], result["comment"]["id"]);
            assert_eq!(
                event["data"]["lease_expires_at"],
                result["bounty"]["lease_expires_at"]
            );
            assert_eq!(event["data"]["lease_id"], b.lease_id);
            assert_eq!(event["data"]["body"], "answer $a=b$");
            assert_public(&event);
            expected.push(event);
        }
        drop(sse);
        let inbox = k
            .repo
            .agent_events(&worker.agent.id, Some(cursor), 500)
            .unwrap();
        assert_eq!(
            serde_json::to_value(&inbox.events).unwrap(),
            json!(expected)
        );
        for event in expected {
            let (headers, body) = tokio::time::timeout(std::time::Duration::from_secs(8), async {
                loop {
                    let (headers, body) = rx.recv().await.unwrap();
                    let value: Value = serde_json::from_str(&body).unwrap();
                    if value["event_id"] == event["event_id"] {
                        break (headers, body);
                    }
                }
            })
            .await
            .unwrap();
            assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), event);
            assert_eq!(
                headers["x-taskrail-event"].to_str().unwrap(),
                event["event_type"].as_str().unwrap()
            );
            // 独立构造 HMAC-SHA256，按原始回调字节验证签名。
            let mut inner = [0x36_u8; 64];
            let mut outer = [0x5c_u8; 64];
            for (index, byte) in b"test-comments-secret".iter().enumerate() {
                inner[index] ^= byte;
                outer[index] ^= byte;
            }
            let mut hash = Sha256::new();
            hash.update(inner);
            hash.update(
                format!(
                    "{}.{body}",
                    headers["x-taskrail-timestamp"].to_str().unwrap()
                )
                .as_bytes(),
            );
            let mut mac = Sha256::new();
            mac.update(outer);
            mac.update(hash.finalize());
            let digest: String = mac.finalize().iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(
                headers["x-taskrail-signature"].to_str().unwrap(),
                format!("sha256={digest}")
            );
        }
    }
    server.shutdown().await.unwrap();
    webhook.abort();
    let _ = webhook.await;
}

#[test]
fn failed_second_wakeup_or_publisher_notification_rolls_back_every_write() {
    let k = Kernel::new();
    let publisher = k.agent("publisher");
    let worker = k.agent("worker");
    let b=k.repo.agent_create_bounty(&publisher.agent.id,serde_json::from_value(json!({"project_id":k.project.id,"title":"atomic","difficulty":"C","assignee_mode":"any"})).unwrap()).unwrap();
    let b = k.repo.agent_claim_bounty(&b.id, &worker.agent.id).unwrap();
    let before = k.repo.get_bounty_detail(&b.id).unwrap();
    k.conn().execute_batch("CREATE TRIGGER fail_notification BEFORE INSERT ON agent_events WHEN NEW.event_type='task.info_requested' BEGIN SELECT RAISE(ABORT,'question notification'); END;").unwrap();
    assert!(k
        .repo
        .agent_request_info(&b.id, &worker.agent.id, question(b.lease_id, "?"))
        .is_err());
    assert_eq!(
        serde_json::to_value(k.repo.get_bounty_detail(&b.id).unwrap()).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    k.conn()
        .execute_batch("DROP TRIGGER fail_notification;")
        .unwrap();
    k.repo
        .agent_request_info(&b.id, &worker.agent.id, question(b.lease_id, "?"))
        .unwrap();
    let before = k.repo.get_bounty_detail(&b.id).unwrap();
    k.conn().execute_batch("CREATE TRIGGER fail_notification BEFORE INSERT ON agent_events WHEN NEW.event_type='task.resumed' BEGIN SELECT RAISE(ABORT,'second wakeup'); END;").unwrap();
    assert!(k
        .repo
        .create_bounty_comment(&b.id, input("answer"))
        .is_err());
    assert_eq!(
        serde_json::to_value(k.repo.get_bounty_detail(&b.id).unwrap()).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    assert!(k.inbox(&worker.agent.id, "comment.replied").is_empty());
    assert!(k.inbox(&worker.agent.id, "task.resumed").is_empty());
}

#[test]
fn concurrent_replies_resume_once_and_preserve_all_comments() {
    let k = Kernel::new();
    let worker = k.agent("worker");
    let b = k.bounty();
    let b = k.repo.agent_claim_bounty(&b.id, &worker.agent.id).unwrap();
    k.repo
        .agent_request_info(&b.id, &worker.agent.id, question(b.lease_id, "?"))
        .unwrap();
    let gate = std::sync::Arc::new(std::sync::Barrier::new(4));
    let threads: Vec<_> = (0..4)
        .map(|i| {
            let repo = k.repo.clone();
            let gate = gate.clone();
            let id = b.id.clone();
            std::thread::spawn(move || {
                gate.wait();
                repo.create_bounty_comment(&id, input(&format!("answer {i}")))
                    .unwrap()
            })
        })
        .collect();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(
        results.iter().filter(|r| r.comment.kind == "reply").count(),
        1
    );
    let detail = k.repo.get_bounty_detail(&b.id).unwrap();
    assert_eq!(detail.bounty.status, BountyStatus::InProgress);
    assert_eq!(detail.bounty.comment_count, 5);
    assert_eq!(detail.comments.len(), 5);
    assert_eq!(
        detail
            .events
            .iter()
            .filter(|e| e.event_type == "resumed")
            .count(),
        1
    );
    assert_eq!(k.inbox(&worker.agent.id, "task.resumed").len(), 1);
    assert_eq!(k.inbox(&worker.agent.id, "comment.replied").len(), 1);
    assert_eq!(k.inbox(&worker.agent.id, "comment.added").len(), 3);
}
