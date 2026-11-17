#![cfg(not(target_os = "android"))]
//! 仲裁配置、回避、裁决与超时顺延的仓储和 HTTP 集成验证。
use reqwest::{Client, StatusCode};
use rusqlite::params;
use rusqlite::Connection;
use serde_json::{json, Value};
use taskrail_lib::server::db::Repository;
use taskrail_lib::server::{self, models::*, RunningServer, ServerConfig, ServerMode};

struct Kernel {
    repo: Repository,
    project: Project,
    _file: tempfile::NamedTempFile,
}
impl Kernel {
    fn new() -> Self {
        let file = tempfile::NamedTempFile::new().unwrap();
        let repo = Repository::open(file.path()).unwrap();
        repo.onboarding(Some(UpdateOnboarding {
            lan_open_registration: true,
        }))
        .unwrap();
        let project = repo
            .create_project(serde_json::from_value(json!({"name":"arbitration"})).unwrap())
            .unwrap();
        Self {
            repo,
            project,
            _file: file,
        }
    }
    fn conn(&self) -> Connection {
        let conn = Connection::open(self.repo.path()).unwrap();
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        conn
    }
    fn agent(&self, name: &str) -> RegisteredAgent {
        self.repo
            .register_agent(serde_json::from_value(json!({"name":name})).unwrap())
            .unwrap()
    }
    fn chain(&self, ids: &[&str]) {
        self.repo
            .arbitration_config(Some(ArbitrationConfig {
                arbitrator_chain: ids.iter().map(|id| (*id).into()).collect(),
            }))
            .unwrap();
    }
    fn bounty(&self, assignee: Option<&str>) -> Bounty {
        self.repo.create_bounty(serde_json::from_value(json!({"project_id":self.project.id,"title":"delivery","difficulty":"C","assignee_mode":if assignee.is_some() {"specific"} else {"any"},"assignee_agent_id":assignee,"status":"OPEN","require_arbitration":true})).unwrap()).unwrap()
    }
    fn pending(&self, worker: &str) -> Bounty {
        let b = self.bounty(None);
        let b = self.repo.agent_claim_bounty(&b.id, worker).unwrap();
        self.submit(&b, worker)
    }
    fn submit(&self, b: &Bounty, worker: &str) -> Bounty {
        self.repo
            .agent_submit_arbitration(
                &b.id,
                worker,
                SubmitArbitrationInput {
                    lease_id: b.lease_id,
                    completion_note: Some("  delivered  ".into()),
                    completion_cost: Some(2.5),
                },
            )
            .unwrap()
    }
    fn inbox(&self, id: &str, kind: &str) -> Vec<AgentEvent> {
        self.repo
            .agent_events(id, Some(0), 500)
            .unwrap()
            .events
            .into_iter()
            .filter(|e| e.event_type == kind)
            .collect()
    }
    fn stored_count(&self, id: &str, kind: &str) -> i64 {
        self.conn()
            .query_row(
                "SELECT COUNT(*) FROM agent_events WHERE agent_id=?1 AND event_type=?2",
                params![id, kind],
                |r| r.get(0),
            )
            .unwrap()
    }
    fn snapshot(&self, id: &str) -> Bounty {
        self.repo.get_bounty_detail(id).unwrap().bounty
    }
    async fn start(&self) -> RunningServer {
        server::start(ServerConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            db_path: self.repo.path().to_owned(),
            admin_token: "arbitration-admin".into(),
            mode: ServerMode::Standalone,
        })
        .await
        .unwrap()
    }
}
fn error<T: std::fmt::Debug>(result: server::error::Result<T>, status: StatusCode, code: &str) {
    let e = result.unwrap_err();
    assert_eq!(e.status, status);
    assert_eq!(e.code, code);
}
fn reject() -> RejectArbitrationInput {
    RejectArbitrationInput {
        comment: "  fix the result  ".into(),
    }
}

#[test]
fn chains_preserve_order_allow_symbol_ids_and_use_project_override() {
    let k = Kernel::new();
    let worker = k.agent("worker");
    let global = k.agent("global");
    let local = k.agent("local");
    k.chain(&[&global.agent.id, "judge", &global.agent.id]);
    assert_eq!(
        k.repo.arbitration_config(None).unwrap().arbitrator_chain,
        vec![global.agent.id.clone(), "judge".into()]
    );
    let b = k.pending(&worker.agent.id);
    assert_eq!(
        b.arbitrator_agent_id.as_deref(),
        Some(global.agent.id.as_str())
    );
    k.repo
        .update_project(
            &k.project.id,
            serde_json::from_value(
                json!({"arbitrator_chain":[local.agent.id,"judge",local.agent.id]}),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        k.repo.get_project(&k.project.id).unwrap().arbitrator_chain,
        vec![local.agent.id.clone(), "judge".into()]
    );
    let b = k.pending(&worker.agent.id);
    assert_eq!(
        b.arbitrator_agent_id.as_deref(),
        Some(local.agent.id.as_str())
    );
    k.repo
        .set_agent_status(&local.agent.id, AgentStatus::Disabled)
        .unwrap();
    let b = k.pending(&worker.agent.id);
    assert!(b.arbitrator_agent_id.is_none());
    assert!(b.arbitration_expires_at.is_none());
    k.repo
        .update_project(
            &k.project.id,
            serde_json::from_value(json!({"arbitrator_chain":[]})).unwrap(),
        )
        .unwrap();
    assert_eq!(
        k.pending(&worker.agent.id).arbitrator_agent_id,
        Some(global.agent.id)
    );
    for chain in [vec![" "], vec!["ok", ""]] {
        error(
            k.repo.arbitration_config(Some(ArbitrationConfig {
                arbitrator_chain: chain.iter().map(|s| (*s).into()).collect(),
            })),
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
        );
        error(
            k.repo.update_project(
                &k.project.id,
                serde_json::from_value(json!({"arbitrator_chain":chain})).unwrap(),
            ),
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
        );
        error(
            k.repo.create_project(
                serde_json::from_value(json!({"name":"invalid","arbitrator_chain":chain})).unwrap(),
            ),
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
        );
    }
}

#[test]
fn sole_active_judge_cannot_claim_and_primary_worker_uses_backup() {
    let k = Kernel::new();
    let primary = k.agent("primary");
    let backup = k.agent("backup");
    k.chain(&[&primary.agent.id, &backup.agent.id, "missing"]);
    k.repo
        .set_agent_status(&backup.agent.id, AgentStatus::Disabled)
        .unwrap();
    let b = k.bounty(None);
    error(
        k.repo.agent_claim_bounty(&b.id, &primary.agent.id),
        StatusCode::FORBIDDEN,
        "ARBITRATOR_CONFLICT",
    );
    assert_eq!(k.snapshot(&b.id).lease_id, 0);
    k.repo
        .set_agent_status(&backup.agent.id, AgentStatus::Active)
        .unwrap();
    let b = k.repo.agent_claim_bounty(&b.id, &primary.agent.id).unwrap();
    let b = k.submit(&b, &primary.agent.id);
    assert_eq!(b.arbitrator_agent_id, Some(backup.agent.id));
    k.chain(&[]);
    let b = k.pending(&primary.agent.id);
    assert!(b.arbitrator_agent_id.is_none());
}

#[test]
fn submission_freezes_timer_and_approval_records_quality_without_leaks() {
    let k = Kernel::new();
    let worker = k.agent("worker");
    let judge = k.agent("judge");
    let other = k.agent("other");
    k.chain(&[&judge.agent.id]);
    let b = k.bounty(Some(&worker.agent.id));
    let b = k.repo.agent_claim_bounty(&b.id, &worker.agent.id).unwrap();
    let start = (chrono::Utc::now() - chrono::Duration::seconds(10))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    k.conn()
        .execute(
            "UPDATE bounties SET active_started_at=?2 WHERE id=?1",
            params![b.id, start],
        )
        .unwrap();
    let b = k.submit(&b, &worker.agent.id);
    assert_eq!(b.status, BountyStatus::PendingArbitration);
    assert!(b.lease_expires_at.is_none());
    assert!(b.active_started_at.is_none());
    assert!(b.active_duration_ms >= 10_000);
    assert_eq!(b.completion_note.as_deref(), Some("delivered"));
    assert_eq!(b.completion_cost, Some(2.5));
    let expires =
        chrono::DateTime::parse_from_rfc3339(b.arbitration_expires_at.as_ref().unwrap()).unwrap();
    assert!((890..=900)
        .contains(&(expires.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_seconds()));
    let events = k.inbox(&judge.agent.id, "arbitration.requested");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data["bounty"]["id"], b.id);
    let snapshot = serde_json::to_value(&b).unwrap();
    for key in ["api_key", "api_key_hash", "mmr", "webhook_secret"] {
        assert!(!snapshot.to_string().contains(key));
    }
    assert_eq!(b.arbitrator_agent_name.as_deref(), Some("judge"));
    assert_eq!(b.arbitrator_agent_tier, Some(AgentTier::Unranked));
    assert!(k.repo.agent_bounty_detail(&judge.agent.id, &b.id).is_ok());
    assert!(k
        .repo
        .agent_bounties(&judge.agent.id, BountyFilter::default())
        .unwrap()
        .iter()
        .any(|v| v.id == b.id));
    assert!(k.repo.agent_bounty_comments(&b.id, &judge.agent.id).is_ok());
    error(
        k.repo.agent_bounty_detail(&other.agent.id, &b.id),
        StatusCode::NOT_FOUND,
        "BOUNTY_NOT_FOUND",
    );
    for id in [&worker.agent.id, &other.agent.id] {
        error(
            k.repo
                .approve_arbitration(&b.id, Some(id), ApproveArbitrationInput::default()),
            StatusCode::FORBIDDEN,
            "ARBITRATOR_MISMATCH",
        );
    }
    let approved = k
        .repo
        .approve_arbitration(
            &b.id,
            Some(&judge.agent.id),
            ApproveArbitrationInput {
                quality_grade: Some(QualityGrade::Flawed),
                calibrated_difficulty: Some(Tier::S),
                comment: Some("  accepted  ".into()),
            },
        )
        .unwrap();
    assert_eq!(approved.status, BountyStatus::Completed);
    assert_eq!(approved.quality_grade, Some(QualityGrade::Flawed));
    assert_eq!(approved.calibrated_difficulty, Some(Tier::S));
    assert!(approved.completed_at.is_some());
    assert!(approved.arbitration_expires_at.is_none());
    assert_eq!(approved.active_duration_ms, b.active_duration_ms);
    assert_eq!(approved.lease_id, b.lease_id);
    assert_eq!(approved.lease_agent_id, b.lease_agent_id);
    let detail = k.repo.get_bounty_detail(&b.id).unwrap();
    assert_eq!(detail.comments[0].body, "accepted");
    assert_eq!(detail.comments[0].kind, "comment");
    assert_eq!(
        detail.events.last().unwrap().event_type,
        "arbitration_approved"
    );
    assert_eq!(k.inbox(&worker.agent.id, "arbitration.approved").len(), 1);
    error(
        k.repo.reject_arbitration(&b.id, None, reject()),
        StatusCode::CONFLICT,
        "INVALID_TRANSITION",
    );
}

#[test]
fn three_rejections_resume_then_fourth_reopens_and_redispatches() {
    let k = Kernel::new();
    let worker = k.agent("worker");
    let judge = k.agent("judge");
    let watcher = k.agent("watcher");
    k.chain(&[&judge.agent.id]);
    let b = k.bounty(Some(&worker.agent.id));
    let mut b = k.repo.agent_claim_bounty(&b.id, &worker.agent.id).unwrap();
    let lease = b.lease_id;
    for round in 1..=4 {
        b = k.submit(&b, &worker.agent.id);
        let active = b.active_duration_ms;
        error(
            k.repo.reject_arbitration(
                &b.id,
                Some(&judge.agent.id),
                RejectArbitrationInput {
                    comment: " \n ".into(),
                },
            ),
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
        );
        b = k
            .repo
            .reject_arbitration(&b.id, Some(&judge.agent.id), reject())
            .unwrap();
        assert_eq!(b.lease_id, lease);
        assert!(b.arbitrator_agent_id.is_none());
        assert!(b.arbitration_expires_at.is_none());
        assert_eq!(
            k.conn()
                .query_row(
                    "SELECT arbitration_tried_ids_json FROM bounties WHERE id=?1",
                    [&b.id],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "[]"
        );
        if round <= 3 {
            assert_eq!(b.status, BountyStatus::InProgress);
            assert_eq!(b.rejection_count, round);
            assert_eq!(b.active_duration_ms, active);
            assert!(b.active_started_at.is_some());
            assert!(
                b.lease_expires_at.as_deref().unwrap() > chrono::Utc::now().to_rfc3339().as_str()
            );
            assert_eq!(
                k.inbox(&worker.agent.id, "arbitration.rejected").len(),
                round as usize
            );
        } else {
            assert_eq!(b.status, BountyStatus::Open);
            assert_eq!(b.rejection_count, 0);
            assert_eq!(b.active_duration_ms, 0);
            assert!(b.active_started_at.is_none());
            assert!(b.lease_agent_id.is_none());
            assert!(b.lease_expires_at.is_none());
            assert!(b.completion_note.is_none());
            assert!(b.completion_cost.is_none());
            assert_eq!(b.assignee_mode, AssigneeMode::Any);
            assert!(b.assignment_expires_at.is_none());
            assert_eq!(k.inbox(&worker.agent.id, "arbitration.failed").len(), 1);
            assert_eq!(k.inbox(&watcher.agent.id, "task.published").len(), 1);
            let detail = k.repo.get_bounty_detail(&b.id).unwrap();
            let failed = detail.events.last().unwrap();
            assert_eq!(failed.event_type, "arbitration_failed");
            assert_eq!(failed.detail["failed_agent_id"], worker.agent.id);
            assert_eq!(failed.detail["lease_id"], lease);
            assert_eq!(failed.detail["rejection_count"], 4);
            assert_eq!(failed.detail["active_duration_ms"], active);
            assert_eq!(failed.detail["comment"], "fix the result");
            assert_eq!(
                failed.detail["comment_id"],
                detail.comments.last().unwrap().id
            );
            assert_eq!(detail.comments.len(), 4);
        }
    }
    let claimed = k.repo.agent_claim_bounty(&b.id, &worker.agent.id).unwrap();
    assert_eq!(claimed.lease_id, lease + 1);
}

#[test]
fn abandon_resets_rejection_count_for_next_worker() {
    let k = Kernel::new();
    let first = k.agent("worker-a");
    let next = k.agent("worker-b");
    let judge = k.agent("judge");
    k.chain(&[&judge.agent.id]);
    let b = k.bounty(Some(&first.agent.id));
    let mut b = k.repo.agent_claim_bounty(&b.id, &first.agent.id).unwrap();
    for round in 1..=3 {
        b = k.submit(&b, &first.agent.id);
        b = k
            .repo
            .reject_arbitration(&b.id, Some(&judge.agent.id), reject())
            .unwrap();
        assert_eq!(b.status, BountyStatus::InProgress);
        assert_eq!(b.rejection_count, round);
    }
    let reopened = k
        .repo
        .agent_abandon_bounty(
            &b.id,
            &first.agent.id,
            AbandonBounty {
                lease_id: b.lease_id,
                reason: None,
            },
        )
        .unwrap();
    assert_next_worker_rejection_budget(&k, &reopened, &next.agent.id, &judge.agent.id);
}

#[test]
fn lease_expiry_resets_rejection_count_for_next_worker() {
    let k = Kernel::new();
    let first = k.agent("worker-a");
    let next = k.agent("worker-b");
    let judge = k.agent("judge");
    k.chain(&[&judge.agent.id]);
    let b = k.bounty(Some(&first.agent.id));
    let mut b = k.repo.agent_claim_bounty(&b.id, &first.agent.id).unwrap();
    for round in 1..=3 {
        b = k.submit(&b, &first.agent.id);
        b = k
            .repo
            .reject_arbitration(&b.id, Some(&judge.agent.id), reject())
            .unwrap();
        assert_eq!(b.status, BountyStatus::InProgress);
        assert_eq!(b.rejection_count, round);
    }
    k.conn()
        .execute(
            "UPDATE bounties SET lease_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
            [&b.id],
        )
        .unwrap();
    k.repo.sweep_leases().unwrap();
    let reopened = k.snapshot(&b.id);
    assert_next_worker_rejection_budget(&k, &reopened, &next.agent.id, &judge.agent.id);
}

fn assert_next_worker_rejection_budget(k: &Kernel, reopened: &Bounty, worker: &str, judge: &str) {
    assert_eq!(reopened.status, BountyStatus::Open);
    let claimed = k.repo.agent_claim_bounty(&reopened.id, worker).unwrap();
    assert_eq!(claimed.lease_id, reopened.lease_id + 1);
    let pending = k.submit(&claimed, worker);
    let rejected = k
        .repo
        .reject_arbitration(&pending.id, Some(judge), reject())
        .unwrap();
    assert_eq!(rejected.status, BountyStatus::InProgress);
    assert_eq!(rejected.rejection_count, 1);
    assert_eq!(rejected.lease_agent_id.as_deref(), Some(worker));
    assert_eq!(rejected.lease_id, claimed.lease_id);
    let notifications = k.inbox(worker, "arbitration.rejected");
    assert_eq!(notifications.len(), 1);
    assert_eq!(
        notifications[0].bounty_id.as_deref(),
        Some(reopened.id.as_str())
    );
    assert_eq!(notifications[0].data["rejection_count"], 1);
    assert_eq!(notifications[0].data["lease_id"], claimed.lease_id);
    assert!(k.inbox(worker, "arbitration.failed").is_empty());
    for b in [reopened, &claimed] {
        assert_eq!(b.rejection_count, 0);
        assert!(b.completion_note.is_none());
        assert!(b.completion_cost.is_none());
        assert!(b.arbitrator_agent_id.is_none());
        assert!(b.arbitration_expires_at.is_none());
    }
    assert_eq!(
        k.conn()
            .query_row(
                "SELECT arbitration_tried_ids_json FROM bounties WHERE id=?1",
                [&reopened.id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "[]"
    );
}

#[test]
fn timeout_skips_inactive_worker_and_tried_then_human_can_approve_or_reject() {
    let k = Kernel::new();
    let worker = k.agent("worker");
    let primary = k.agent("primary");
    let disabled = k.agent("disabled");
    let backup = k.agent("backup");
    k.chain(&[
        &worker.agent.id,
        &primary.agent.id,
        &disabled.agent.id,
        &backup.agent.id,
    ]);
    k.repo
        .set_agent_status(&disabled.agent.id, AgentStatus::Disabled)
        .unwrap();
    let mut b = k.pending(&worker.agent.id);
    assert_eq!(b.arbitrator_agent_id, Some(primary.agent.id.clone()));
    for expected in [Some(backup.agent.id.clone()), None] {
        k.conn()
            .execute(
                "UPDATE bounties SET arbitration_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
                [&b.id],
            )
            .unwrap();
        k.repo.sweep_leases().unwrap();
        b = k.snapshot(&b.id);
        assert_eq!(b.arbitrator_agent_id, expected);
        assert_eq!(b.status, BountyStatus::PendingArbitration);
        assert!(b.active_started_at.is_none());
    }
    assert!(b.arbitration_expires_at.is_none());
    let detail = k.repo.get_bounty_detail(&b.id).unwrap();
    assert_eq!(
        detail.events.last().unwrap().event_type,
        "arbitration_human_fallback"
    );
    assert!(detail
        .events
        .iter()
        .any(|e| e.event_type == "arbitration_reassigned"));
    assert_eq!(k.inbox(&primary.agent.id, "arbitration.timed_out").len(), 1);
    assert_eq!(k.inbox(&backup.agent.id, "arbitration.requested").len(), 1);
    assert_eq!(k.inbox(&backup.agent.id, "arbitration.timed_out").len(), 1);
    assert_eq!(
        k.stored_count(&disabled.agent.id, "arbitration.requested"),
        0
    );
    let total = detail.events.len();
    k.repo.sweep_leases().unwrap();
    assert_eq!(k.repo.get_bounty_detail(&b.id).unwrap().events.len(), total);
    error(
        k.repo.approve_arbitration(
            &b.id,
            Some(&backup.agent.id),
            ApproveArbitrationInput::default(),
        ),
        StatusCode::FORBIDDEN,
        "ARBITRATOR_MISMATCH",
    );
    let resumed = k.repo.reject_arbitration(&b.id, None, reject()).unwrap();
    assert_eq!(resumed.status, BountyStatus::InProgress);
    let b = k.submit(&resumed, &worker.agent.id);
    assert_eq!(b.arbitrator_agent_id, Some(primary.agent.id.clone()));
    let b = k
        .repo
        .approve_arbitration(&b.id, None, ApproveArbitrationInput::default())
        .unwrap();
    assert_eq!(b.quality_grade, Some(QualityGrade::Perfect));
    assert_eq!(
        k.repo
            .get_bounty_detail(&b.id)
            .unwrap()
            .events
            .last()
            .unwrap()
            .actor,
        "human"
    );
}

#[test]
fn submit_fencing_validation_and_release_clear_arbitration_state() {
    let k = Kernel::new();
    let worker = k.agent("worker");
    let judge = k.agent("judge");
    k.chain(&[&judge.agent.id]);
    let b = k.bounty(None);
    let b = k.repo.agent_claim_bounty(&b.id, &worker.agent.id).unwrap();
    let submit = |id: &str, agent: &str, lease_id, cost| {
        k.repo.agent_submit_arbitration(
            id,
            agent,
            SubmitArbitrationInput {
                lease_id,
                completion_note: Some("  ".into()),
                completion_cost: cost,
            },
        )
    };
    error(
        submit("missing", &worker.agent.id, b.lease_id, Some(-1.0)),
        StatusCode::NOT_FOUND,
        "BOUNTY_NOT_FOUND",
    );
    error(
        submit(&b.id, &judge.agent.id, b.lease_id - 1, Some(-1.0)),
        StatusCode::CONFLICT,
        "STALE_LEASE",
    );
    error(
        submit(&b.id, &judge.agent.id, b.lease_id, Some(-1.0)),
        StatusCode::FORBIDDEN,
        "LEASE_OWNER_MISMATCH",
    );
    for cost in [-1.0, f64::NAN, f64::INFINITY] {
        error(
            submit(&b.id, &worker.agent.id, b.lease_id, Some(cost)),
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
        );
    }
    k.conn()
        .execute(
            "UPDATE bounties SET require_arbitration=0 WHERE id=?1",
            [&b.id],
        )
        .unwrap();
    error(
        submit(&b.id, &worker.agent.id, b.lease_id, Some(-1.0)),
        StatusCode::CONFLICT,
        "INVALID_TRANSITION",
    );
    k.conn().execute("UPDATE bounties SET require_arbitration=1,lease_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",[&b.id]).unwrap();
    error(
        submit(&b.id, &worker.agent.id, b.lease_id, None),
        StatusCode::CONFLICT,
        "STALE_LEASE",
    );
    k.conn()
        .execute(
            "UPDATE bounties SET lease_expires_at='2999-01-01T00:00:00.000Z' WHERE id=?1",
            [&b.id],
        )
        .unwrap();
    let pending = submit(&b.id, &worker.agent.id, b.lease_id, None).unwrap();
    assert!(pending.completion_note.is_none());
    error(
        submit(&b.id, &worker.agent.id, b.lease_id, None),
        StatusCode::CONFLICT,
        "INVALID_TRANSITION",
    );
    let b = k.repo.reject_arbitration(&b.id, None, reject()).unwrap();
    let b = k.submit(&b, &worker.agent.id);
    let released = k.repo.admin_release_lease(&b.id).unwrap();
    assert_eq!(released.status, BountyStatus::Open);
    assert_eq!(released.rejection_count, 0);
    assert!(released.completion_note.is_none());
    assert!(released.completion_cost.is_none());
    assert!(released.arbitrator_agent_id.is_none());
    assert!(released.arbitration_expires_at.is_none());
    assert_eq!(released.lease_id, b.lease_id);
    assert_eq!(
        k.conn()
            .query_row(
                "SELECT arbitration_tried_ids_json FROM bounties WHERE id=?1",
                [&b.id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "[]"
    );
}

#[test]
fn arbitration_writes_roll_back_when_history_comments_or_notifications_fail() {
    for failure in ["history", "comment", "notification", "dispatch"] {
        let k = Kernel::new();
        let worker = k.agent("worker");
        let judge = k.agent("judge");
        k.chain(&[&judge.agent.id]);
        let b = k.bounty(None);
        let b = k.repo.agent_claim_bounty(&b.id, &worker.agent.id).unwrap();
        let snapshot = || serde_json::to_value(k.repo.get_bounty_detail(&b.id).unwrap()).unwrap();
        if failure == "history" || failure == "notification" {
            let before = snapshot();
            let sql = if failure == "history" {
                "CREATE TRIGGER fail BEFORE INSERT ON bounty_events BEGIN SELECT RAISE(ABORT,'history'); END;"
            } else {
                "CREATE TRIGGER fail BEFORE INSERT ON agent_events BEGIN SELECT RAISE(ABORT,'notification'); END;"
            };
            k.conn().execute_batch(sql).unwrap();
            assert!(k
                .repo
                .agent_submit_arbitration(
                    &b.id,
                    &worker.agent.id,
                    SubmitArbitrationInput {
                        lease_id: b.lease_id,
                        completion_note: None,
                        completion_cost: None
                    }
                )
                .is_err());
            assert_eq!(snapshot(), before);
            k.conn().execute_batch("DROP TRIGGER fail;").unwrap();
        }
        let b = k.submit(&b, &worker.agent.id);
        if failure == "dispatch" {
            k.conn()
                .execute("UPDATE bounties SET rejection_count=3 WHERE id=?1", [&b.id])
                .unwrap();
        }
        k.conn().execute_batch(match failure {
            "history" => "CREATE TRIGGER fail BEFORE INSERT ON bounty_events BEGIN SELECT RAISE(ABORT,'history'); END;",
            "comment" => "CREATE TRIGGER fail BEFORE INSERT ON bounty_comments BEGIN SELECT RAISE(ABORT,'comment'); END;",
            "dispatch" => "CREATE TRIGGER fail BEFORE INSERT ON agent_events WHEN NEW.event_type='task.published' BEGIN SELECT RAISE(ABORT,'dispatch'); END;",
            _ => "CREATE TRIGGER fail BEFORE INSERT ON agent_events BEGIN SELECT RAISE(ABORT,'notification'); END;",
        }).unwrap();
        let before = snapshot();
        let inbox_before =
            serde_json::to_value(k.repo.agent_events(&worker.agent.id, Some(0), 500).unwrap())
                .unwrap();
        assert!(k
            .repo
            .reject_arbitration(&b.id, Some(&judge.agent.id), reject())
            .is_err());
        assert_eq!(snapshot(), before);
        assert_eq!(
            serde_json::to_value(k.repo.agent_events(&worker.agent.id, Some(0), 500).unwrap())
                .unwrap(),
            inbox_before
        );
        if failure != "dispatch" {
            assert!(k
                .repo
                .approve_arbitration(
                    &b.id,
                    None,
                    ApproveArbitrationInput {
                        comment: Some("approve".into()),
                        ..Default::default()
                    }
                )
                .is_err());
            assert_eq!(snapshot(), before);
        }
    }
}

#[test]
fn failed_delivery_preserves_other_assignment_window_and_inactive_agents_get_no_notification() {
    let k = Kernel::new();
    let worker = k.agent("worker");
    let judge = k.agent("judge");
    let target = k.agent("target");
    k.chain(&[&judge.agent.id]);
    let b = k.pending(&worker.agent.id);
    k.conn().execute("UPDATE bounties SET rejection_count=3,assignee_mode='specific',assignee_agent_id=?2 WHERE id=?1",params![b.id,target.agent.id]).unwrap();
    k.repo
        .set_agent_status(&worker.agent.id, AgentStatus::Disabled)
        .unwrap();
    let b = k.repo.reject_arbitration(&b.id, None, reject()).unwrap();
    assert_eq!(b.assignee_mode, AssigneeMode::Specific);
    assert_eq!(b.assignee_agent_id, Some(target.agent.id.clone()));
    assert!(b.assignment_expires_at.is_some());
    assert_eq!(k.stored_count(&worker.agent.id, "arbitration.failed"), 0);
    assert_eq!(k.inbox(&target.agent.id, "task.assigned").len(), 1);
    k.repo
        .set_agent_status(&worker.agent.id, AgentStatus::Active)
        .unwrap();
    let b = k.pending(&worker.agent.id);
    k.repo
        .set_agent_status(&judge.agent.id, AgentStatus::Disabled)
        .unwrap();
    error(
        k.repo.approve_arbitration(
            &b.id,
            Some(&judge.agent.id),
            ApproveArbitrationInput::default(),
        ),
        StatusCode::FORBIDDEN,
        "AGENT_DISABLED",
    );
    k.repo
        .set_agent_status(&worker.agent.id, AgentStatus::Disabled)
        .unwrap();
    k.repo
        .approve_arbitration(&b.id, None, ApproveArbitrationInput::default())
        .unwrap();
    assert_eq!(k.stored_count(&worker.agent.id, "arbitration.approved"), 0);
}

#[tokio::test]
async fn http_contracts_admin_config_strict_dtos_agent_permissions_and_human_fallback() {
    let k = Kernel::new();
    let worker = k.agent("worker");
    let judge = k.agent("judge");
    let other = k.agent("other");
    let server = k.start().await;
    let client = Client::new();
    let base = &server.info.base_url;
    let config = format!("{base}/api/v1/admin/arbitration");
    for key in [None, Some(worker.api_key.as_str())] {
        let mut request = client.get(&config);
        if let Some(key) = key {
            request = request.bearer_auth(key);
        }
        assert_eq!(
            request.send().await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }
    let response = client
        .patch(&config)
        .bearer_auth("arbitration-admin")
        .json(&json!({"arbitrator_chain":[judge.agent.id,"judge",judge.agent.id]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"arbitrator_chain":[judge.agent.id,"judge"]})
    );
    assert_eq!(
        client
            .get(&config)
            .bearer_auth("arbitration-admin")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()["arbitrator_chain"],
        json!([judge.agent.id, "judge"])
    );
    for payload in [
        json!({"arbitrator_chain":[" "]}),
        json!({"arbitrator_chain":[],"extra":true}),
        json!({}),
    ] {
        let response = client
            .patch(&config)
            .bearer_auth("arbitration-admin")
            .json(&payload)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.json::<Value>().await.unwrap()["error"]["code"],
            "VALIDATION_ERROR"
        );
    }
    let b = k.bounty(Some(&worker.agent.id));
    let b = k.repo.agent_claim_bounty(&b.id, &worker.agent.id).unwrap();
    let submit = format!("{base}/api/v1/agent/bounties/{}/submit-arbitration", b.id);
    for payload in [
        json!({"lease_id":b.lease_id,"extra":1}),
        json!({"lease_id":b.lease_id,"completion_cost":-1}),
        json!({}),
    ] {
        assert_eq!(
            client
                .post(&submit)
                .bearer_auth(&worker.api_key)
                .json(&payload)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let response = client
        .post(&submit)
        .bearer_auth(&worker.api_key)
        .json(&json!({"lease_id":b.lease_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let pending = response.json::<Value>().await.unwrap();
    assert_eq!(pending["arbitrator_agent_id"], judge.agent.id);
    assert!(pending["completion_note"].is_null());
    let approve = format!("{base}/api/v1/agent/bounties/{}/arbitration/approve", b.id);
    let reject_url = format!("{base}/api/v1/agent/bounties/{}/arbitration/reject", b.id);
    for url in [&approve, &reject_url] {
        let response = client
            .post(url)
            .bearer_auth(&other.api_key)
            .json(&json!({"comment":"reject"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            response.json::<Value>().await.unwrap()["error"]["code"],
            "ARBITRATOR_MISMATCH"
        );
    }
    for payload in [
        json!({"quality_grade":"unknown"}),
        json!({"calibrated_difficulty":"D"}),
        json!({"extra":1}),
    ] {
        assert_eq!(
            client
                .post(&approve)
                .bearer_auth(&judge.api_key)
                .json(&payload)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    for payload in [
        json!({"comment":" "}),
        json!({"comment":"x","extra":1}),
        json!({}),
    ] {
        assert_eq!(
            client
                .post(&reject_url)
                .bearer_auth(&judge.api_key)
                .json(&payload)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let response = client
        .post(&reject_url)
        .bearer_auth(&judge.api_key)
        .json(&json!({"comment":"fix"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap()["status"],
        "IN_PROGRESS"
    );
    client
        .post(&submit)
        .bearer_auth(&worker.api_key)
        .json(&json!({"lease_id":b.lease_id}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let response = client
        .post(&approve)
        .bearer_auth(&judge.api_key)
        .json(&json!({"quality_grade":"flawed","calibrated_difficulty":"A","comment":"verified"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let result = response.json::<Value>().await.unwrap();
    assert_eq!(result["quality_grade"], "flawed");
    assert_eq!(result["calibrated_difficulty"], "A");
    k.chain(&[]);
    let pending = k.pending(&worker.agent.id);
    let admin_approve = format!("{base}/api/v1/bounties/{}/arbitration/approve", pending.id);
    let admin_reject = format!("{base}/api/v1/bounties/{}/arbitration/reject", pending.id);
    assert_eq!(
        client
            .post(&admin_approve)
            .bearer_auth(&judge.api_key)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let response = client
        .post(&admin_reject)
        .bearer_auth("arbitration-admin")
        .json(&json!({"comment":"human fix"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let b = k.snapshot(&pending.id);
    k.submit(&b, &worker.agent.id);
    let response = client
        .post(&admin_approve)
        .bearer_auth("arbitration-admin")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap()["quality_grade"],
        "perfect"
    );
    let b = k.pending(&worker.agent.id);
    let response = client
        .post(format!(
            "{base}/api/v1/bounties/{}/arbitration/approve",
            b.id
        ))
        .bearer_auth("arbitration-admin")
        .json(&json!({"comment":"human acceptance","quality_grade":"flawed"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    server.shutdown().await.unwrap();
}

#[test]
fn v6_migration_is_atomic_and_preserves_history() {
    for failure in ["none", "index", "version"] {
        let file = tempfile::NamedTempFile::new().unwrap();
        let conn = Connection::open(file.path()).unwrap();
        conn.execute_batch(include_str!("fixtures/server_v1.sql"))
            .unwrap();
        for schema in [
            include_str!("../src/server/schema_v2.sql"),
            include_str!("../src/server/schema_v3.sql"),
            include_str!("../src/server/schema_v4.sql"),
            include_str!("../src/server/schema_v5.sql"),
            include_str!("../src/server/schema_v6.sql"),
        ] {
            conn.execute_batch(schema).unwrap();
        }
        conn.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL); INSERT INTO schema_migrations VALUES(6,'old'); INSERT INTO projects(id,name,created_at,updated_at) VALUES('p','legacy','old','old');").unwrap();
        conn.execute_batch("INSERT INTO agents(id,name,api_key_hash,api_key_prefix,created_at,updated_at) VALUES('worker','legacy-worker','hash','prefix','old','old'); INSERT INTO bounties(id,project_id,title,difficulty,assignee_mode,status,lease_id,lease_agent_id,published_at,created_at,updated_at,active_duration_ms) VALUES('b','p','legacy task','C','any','PENDING_ARBITRATION',42,'worker','old','old','old',1234); INSERT INTO bounty_events(bounty_id,event_type,actor,to_status,detail_json,created_at) VALUES('b','legacy','human','PENDING_ARBITRATION','{\"keep\":true}','old'); INSERT INTO bounty_comments(bounty_id,author_type,author_name,kind,body,created_at) VALUES('b','human','human','comment','keep comment','old'); INSERT INTO lease_incidents(bounty_id,agent_id,lease_id,kind,created_at) VALUES('b','worker',41,'timeout','old'); INSERT INTO agent_events(agent_id,bounty_id,event_type,payload_json,created_at) VALUES('worker','b','legacy','{}','old');").unwrap();
        if failure == "version" {
            conn.execute_batch("CREATE TRIGGER fail_v7 BEFORE INSERT ON schema_migrations WHEN NEW.version=7 BEGIN SELECT RAISE(ABORT,'fail'); END;").unwrap();
        }
        if failure == "index" {
            conn.execute_batch("CREATE TABLE bounties_arbitration_deadline(conflict TEXT);")
                .unwrap();
        }
        let schema = || {
            conn.query_row(
                "SELECT group_concat(sql,';') FROM (SELECT sql FROM sqlite_master ORDER BY name)",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
        };
        let before = schema();
        let result = Repository::open(file.path());
        if failure != "none" {
            assert!(result.is_err());
            assert_eq!(schema(), before);
            assert_eq!(
                conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                6
            );
            assert_eq!(conn.query_row("SELECT COUNT(*) FROM pragma_table_info('bounties') WHERE name='arbitrator_agent_id'",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        } else {
            let repo = result.unwrap();
            assert_eq!(repo.get_project("p").unwrap().name, "legacy");
            let detail = repo.get_bounty_detail("b").unwrap();
            assert_eq!(detail.bounty.lease_id, 42);
            assert_eq!(detail.bounty.active_duration_ms, 1234);
            assert_eq!(detail.bounty.status, BountyStatus::PendingArbitration);
            assert!(detail.bounty.arbitrator_agent_id.is_none());
            assert!(detail.bounty.arbitration_expires_at.is_none());
            assert!(detail.bounty.quality_grade.is_none());
            assert!(detail.bounty.calibrated_difficulty.is_none());
            assert_eq!(detail.events[0].detail, json!({"keep":true}));
            assert_eq!(detail.comments[0].body, "keep comment");
            assert!(repo
                .arbitration_config(None)
                .unwrap()
                .arbitrator_chain
                .is_empty());
            assert_eq!(
                conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                9
            );
            Repository::open(file.path()).unwrap();
            for sql in [
                "UPDATE bounties SET quality_grade='invalid' WHERE id='b'",
                "UPDATE bounties SET calibrated_difficulty='D' WHERE id='b'",
            ] {
                assert!(conn.execute(sql, []).is_err());
            }
        }
        for table in [
            "projects",
            "bounties",
            "agents",
            "bounty_events",
            "bounty_comments",
            "lease_incidents",
            "agent_events",
        ] {
            assert_eq!(
                conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
        }
    }
}

#[test]
fn timeout_notification_failure_rolls_back_reassignment_and_all_inboxes() {
    let k = Kernel::new();
    let worker = k.agent("worker");
    let first = k.agent("first");
    let second = k.agent("second");
    k.chain(&[&first.agent.id, &second.agent.id]);
    let b = k.pending(&worker.agent.id);
    k.conn()
        .execute(
            "UPDATE bounties SET arbitration_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
            [&b.id],
        )
        .unwrap();
    k.conn().execute_batch("CREATE TRIGGER fail BEFORE INSERT ON agent_events WHEN NEW.event_type='arbitration.requested' BEGIN SELECT RAISE(ABORT,'new judge notification'); END;").unwrap();
    let before = serde_json::to_value(k.repo.get_bounty_detail(&b.id).unwrap()).unwrap();
    assert!(k.repo.sweep_leases().is_err());
    assert_eq!(
        serde_json::to_value(k.repo.get_bounty_detail(&b.id).unwrap()).unwrap(),
        before
    );
    assert_eq!(k.stored_count(&first.agent.id, "arbitration.timed_out"), 0);
    assert_eq!(k.stored_count(&second.agent.id, "arbitration.requested"), 0);
    k.conn().execute_batch("DROP TRIGGER fail;").unwrap();
    k.repo.sweep_leases().unwrap();
    assert_eq!(k.snapshot(&b.id).arbitrator_agent_id, Some(second.agent.id));
}
