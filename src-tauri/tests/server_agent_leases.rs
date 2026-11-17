#![cfg(not(target_os = "android"))]
//! 文件职责：通过仓储与 HTTP 验证并发租约、令牌隔离、指派回退及直接交付。
use reqwest::{Client, StatusCode};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use taskrail_lib::server::{self, error::Result, RunningServer, ServerConfig, ServerMode};
use taskrail_lib::server::{db::Repository, models::*};

struct Kernel {
    repo: Repository,
    project: Project,
    _dir: tempfile::TempDir,
}

impl Kernel {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::open(dir.path().join("leases.db")).unwrap();
        repo.onboarding(Some(UpdateOnboarding {
            lan_open_registration: true,
        }))
        .unwrap();
        let project = repo
            .create_project(serde_json::from_value(json!({"name":"leases"})).unwrap())
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

    fn bounty(&self, extra: Value) -> Bounty {
        let mut body = json!({"project_id":self.project.id,"title":"任务","difficulty":"C","assignee_mode":"any","status":"OPEN"});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.repo
            .create_bounty(serde_json::from_value(body).unwrap())
            .unwrap()
    }

    fn snapshot(&self, id: &str) -> Bounty {
        self.repo.get_bounty_detail(id).unwrap().bounty
    }

    fn agent_bounty(&self, publisher: &str, extra: Value) -> Bounty {
        let mut body = json!({"project_id":self.project.id,"title":"Agent 任务","difficulty":"C","assignee_mode":"any"});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.repo
            .agent_create_bounty(publisher, serde_json::from_value(body).unwrap())
            .unwrap()
    }

    fn inbox(&self, agent_id: &str, kind: &str) -> Vec<AgentEvent> {
        self.repo
            .agent_events(agent_id, Some(0), 500)
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
            admin_token: "leases-admin".into(),
            mode: ServerMode::Standalone,
        })
        .await
        .unwrap()
    }
}

fn error(result: Result<Bounty>, code: &str, status: StatusCode) {
    let error = result.unwrap_err();
    assert_eq!(error.code, code);
    assert_eq!(error.status, status);
}

fn completion(lease_id: i64) -> CompleteBounty {
    CompleteBounty {
        lease_id,
        completion_note: None,
        completion_cost: None,
    }
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
            for value in values {
                assert_public(value);
            }
        }
        _ => {}
    }
}

async fn post(
    client: &Client,
    server: &RunningServer,
    key: Option<&str>,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = client.post(format!("{}/api/v1{path}", server.info.base_url));
    if let Some(key) = key {
        request = request.bearer_auth(key);
    }
    if let Some(body) = body {
        request = request
            .header("Content-Type", "application/json")
            .body(body.to_string());
    }
    let response = request.send().await.unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    (
        status,
        serde_json::from_str(&text).unwrap_or_else(|_| panic!("{status}: {text}")),
    )
}

#[test]
fn concurrent_claims_have_one_winner_and_old_tokens_cannot_write_after_reclaim() {
    let k = Kernel::new();
    let agents: Vec<_> = (0..8).map(|i| k.agent(&format!("worker-{i}"))).collect();
    let b = k.bounty(json!({}));
    let gate = std::sync::Arc::new(std::sync::Barrier::new(8));
    let threads: Vec<_> = agents
        .iter()
        .map(|a| {
            let repo = Repository::open(k.repo.path()).unwrap();
            let gate = gate.clone();
            let id = b.id.clone();
            let agent_id = a.agent.id.clone();
            std::thread::spawn(move || {
                gate.wait();
                repo.agent_claim_bounty(&id, &agent_id)
            })
        })
        .collect();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(results
        .iter()
        .filter_map(|r| r.as_ref().err())
        .all(|e| e.code == "BOUNTY_ALREADY_CLAIMED"));
    let claimed = k.snapshot(&b.id);
    assert_eq!(claimed.lease_id, 1);
    assert!(claimed.lease_agent_name.is_some());
    assert_eq!(claimed.lease_agent_tier, Some(AgentTier::Unranked));
    let holder = claimed.lease_agent_id.as_deref().unwrap();
    k.repo
        .agent_abandon_bounty(
            &b.id,
            holder,
            AbandonBounty {
                lease_id: 1,
                reason: None,
            },
        )
        .unwrap();
    let next = k
        .repo
        .agent_claim_bounty(&b.id, &agents[0].agent.id)
        .unwrap();
    assert_eq!(next.lease_id, 2);
    for error in [
        k.repo
            .agent_heartbeat_bounty(&b.id, holder, LeaseHeartbeat { lease_id: 1 })
            .unwrap_err(),
        k.repo
            .agent_abandon_bounty(
                &b.id,
                holder,
                AbandonBounty {
                    lease_id: 1,
                    reason: None,
                },
            )
            .unwrap_err(),
        k.repo
            .agent_complete_bounty(
                &b.id,
                holder,
                CompleteBounty {
                    lease_id: 1,
                    completion_note: None,
                    completion_cost: None,
                },
            )
            .unwrap_err(),
    ] {
        assert_eq!(error.code, "STALE_LEASE");
    }
    assert_eq!(k.snapshot(&b.id).lease_id, 2);
    assert_eq!(
        k.conn()
            .query_row(
                "SELECT abandon_count FROM agents WHERE id=?1",
                [holder],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(k.conn().query_row("SELECT COUNT(*) FROM lease_incidents WHERE bounty_id=?1 AND kind='abandon' AND lease_id=1", [&b.id], |r| r.get::<_, i64>(0)).unwrap(), 1);
}

#[test]
fn completion_preserves_holder_token_and_normalizes_note_without_arbitration() {
    let k = Kernel::new();
    let a = k.agent("worker");
    let b = k.bounty(json!({}));
    let lease = k.repo.agent_claim_bounty(&b.id, &a.agent.id).unwrap();
    let completed = k
        .repo
        .agent_complete_bounty(
            &b.id,
            &a.agent.id,
            CompleteBounty {
                lease_id: lease.lease_id,
                completion_note: Some("  验收通过 \n".into()),
                completion_cost: Some(2.5),
            },
        )
        .unwrap();
    assert_eq!(completed.status, BountyStatus::Completed);
    assert_eq!(completed.lease_agent_id, Some(a.agent.id));
    assert_eq!(completed.lease_id, lease.lease_id);
    assert_eq!(completed.completion_note.as_deref(), Some("验收通过"));
    assert_eq!(completed.completion_cost, Some(2.5));
    assert!(completed.completed_at.is_some());
    assert!(completed.lease_expires_at.is_none());
    k.repo.sweep_leases().unwrap();
    assert_eq!(k.snapshot(&b.id).status, BountyStatus::Completed);
}

#[test]
fn expired_lease_is_reclaimed_once_and_notifies_former_holder() {
    let k = Kernel::new();
    let a = k.agent("worker");
    let b = k.bounty(json!({"assignee_mode":"specific","assignee_agent_id":a.agent.id}));
    k.repo.agent_claim_bounty(&b.id, &a.agent.id).unwrap();
    k.conn()
        .execute(
            "UPDATE bounties SET lease_expires_at=?2 WHERE id=?1",
            params![b.id, "2000-01-01T00:00:00.000Z"],
        )
        .unwrap();
    k.repo.sweep_leases().unwrap();
    k.repo.sweep_leases().unwrap();
    let open = k.snapshot(&b.id);
    assert_eq!(open.status, BountyStatus::Open);
    assert_eq!(open.assignee_mode, AssigneeMode::Any);
    assert!(open.lease_agent_id.is_none());
    assert_eq!(open.lease_id, 1);
    assert_eq!(
        k.conn()
            .query_row(
                "SELECT timeout_count FROM agents WHERE id=?1",
                [&a.agent.id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    let events = k
        .repo
        .agent_events(&a.agent.id, Some(0), 100)
        .unwrap()
        .events;
    assert_eq!(
        events
            .iter()
            .filter(|e| e.event_type == "lease.expired")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.event_type == "task.published")
            .count(),
        1
    );
}

#[test]
fn claim_enforces_tiers_assignment_publication_and_active_identity() {
    let k = Kernel::new();
    let a = k.agent("worker");
    let other = k.agent("other");
    for (tier, rank) in [("Unranked", 1), ("C", 1), ("B", 2), ("A", 3), ("S", 4)] {
        k.conn()
            .execute(
                "UPDATE agents SET tier=?2 WHERE id=?1",
                params![a.agent.id, tier],
            )
            .unwrap();
        for (minimum, min_rank) in [("C", 1), ("B", 2), ("A", 3), ("S", 4)] {
            let b = k.bounty(json!({"assignee_mode":"min_tier","assignee_min_tier":minimum}));
            let result = k.repo.agent_claim_bounty(&b.id, &a.agent.id);
            if rank >= min_rank {
                assert_eq!(result.unwrap().status, BountyStatus::InProgress);
            } else {
                error(result, "TIER_INSUFFICIENT", StatusCode::FORBIDDEN);
                assert_eq!(k.snapshot(&b.id).lease_id, 0);
            }
        }
    }
    let specific = k.bounty(json!({"assignee_mode":"specific","assignee_agent_id":a.agent.id}));
    assert_eq!(specific.assignee_agent_name.as_deref(), Some("worker"));
    assert_eq!(specific.assignee_agent_tier, Some(AgentTier::S));
    error(
        k.repo.agent_claim_bounty(&specific.id, &other.agent.id),
        "ASSIGNEE_MISMATCH",
        StatusCode::FORBIDDEN,
    );
    let draft = k.bounty(
        json!({"status":"DRAFT","assignee_mode":"specific","assignee_agent_id":a.agent.id}),
    );
    error(
        k.repo.agent_claim_bounty(&draft.id, &a.agent.id),
        "BOUNTY_NOT_FOUND",
        StatusCode::NOT_FOUND,
    );
    k.repo.cancel_bounty(&draft.id, "human").unwrap();
    error(
        k.repo.agent_claim_bounty(&draft.id, &a.agent.id),
        "BOUNTY_NOT_FOUND",
        StatusCode::NOT_FOUND,
    );
    let cancelled = k.bounty(json!({}));
    k.repo.cancel_bounty(&cancelled.id, "human").unwrap();
    error(
        k.repo.agent_claim_bounty(&cancelled.id, &a.agent.id),
        "INVALID_TRANSITION",
        StatusCode::CONFLICT,
    );
    k.repo
        .set_agent_status(&a.agent.id, AgentStatus::Disabled)
        .unwrap();
    error(
        k.repo.agent_claim_bounty(&specific.id, &a.agent.id),
        "AGENT_DISABLED",
        StatusCode::FORBIDDEN,
    );
    k.repo
        .set_agent_status(&a.agent.id, AgentStatus::Revoked)
        .unwrap();
    error(
        k.repo.agent_claim_bounty(&specific.id, &a.agent.id),
        "AGENT_REVOKED",
        StatusCode::UNAUTHORIZED,
    );
    assert_eq!(k.snapshot(&specific.id).status, BountyStatus::Open);
    // 抢单不依赖订阅；未订阅者可直接接取已知的公开任务。
    k.repo
        .update_agent_subscriptions(
            &other.agent.id,
            serde_json::from_value(json!({"subscribe_all_projects":false})).unwrap(),
        )
        .unwrap();
    let b = k.bounty(json!({}));
    let claimed = k.repo.agent_claim_bounty(&b.id, &other.agent.id).unwrap();
    let ttl = chrono::DateTime::parse_from_rfc3339(claimed.lease_expires_at.as_deref().unwrap())
        .unwrap()
        - chrono::DateTime::parse_from_rfc3339(&claimed.updated_at).unwrap();
    assert!((900_000..901_000).contains(&ttl.num_milliseconds()));
    let event = k
        .repo
        .get_bounty_detail(&b.id)
        .unwrap()
        .events
        .pop()
        .unwrap();
    assert_eq!(event.event_type, "claimed");
    assert_eq!(event.actor, format!("agent:{}", other.agent.id));
    assert_eq!(event.from_status, Some(BountyStatus::Open));
    assert_eq!(event.to_status, BountyStatus::InProgress);
    assert_eq!(event.detail["lease_agent_name"], "other");
    assert_eq!(event.detail["lease_agent_tier"], "Unranked");
    assert_eq!(event.detail["lease_id"], 1);
    assert_public(&serde_json::to_value(claimed).unwrap());
    assert_public(
        &serde_json::to_value(k.repo.list_agents(AgentFilter::default()).unwrap()).unwrap(),
    );
}

#[test]
fn assignment_windows_follow_open_entries_and_decline_notifies_and_redispatches() {
    let k = Kernel::new();
    let publisher = k.agent("publisher");
    let target = k.agent("target");
    let subscriber = k.agent("subscriber");
    let assigned = json!({"assignee_mode":"specific","assignee_agent_id":target.agent.id});
    let manual = k.bounty(assigned.clone());
    assert!(manual.assignment_expires_at.is_some());
    let mut draft_input = assigned.clone();
    draft_input["status"] = json!("DRAFT");
    let draft = k.bounty(draft_input);
    assert!(draft.assignment_expires_at.is_none());
    let published = k.repo.publish_bounty(&draft.id, "human").unwrap();
    assert!(published.assignment_expires_at.is_some());
    let withdrawn = k.repo.withdraw_bounty(&draft.id, "human").unwrap();
    assert!(withdrawn.assignment_expires_at.is_none());
    let republished = k.repo.publish_bounty(&draft.id, "human").unwrap();
    assert!(republished.assignment_expires_at.is_some());
    assert!(k
        .repo
        .cancel_bounty(&draft.id, "human")
        .unwrap()
        .assignment_expires_at
        .is_none());
    let b = k.agent_bounty(&publisher.agent.id, assigned.clone());
    assert!(b.assignment_expires_at.is_some());
    error(
        k.repo
            .agent_decline_bounty(&b.id, &subscriber.agent.id, DeclineBounty::default()),
        "ASSIGNEE_MISMATCH",
        StatusCode::FORBIDDEN,
    );
    let open = k
        .repo
        .agent_decline_bounty(
            &b.id,
            &target.agent.id,
            DeclineBounty {
                reason: Some("无空闲资源".into()),
            },
        )
        .unwrap();
    assert_eq!(open.assignee_mode, AssigneeMode::Any);
    assert!(open.assignee_agent_id.is_none());
    assert!(open.assignee_min_tier.is_none());
    assert!(open.assignment_expires_at.is_none());
    let notices = k.inbox(&publisher.agent.id, "task.assignment_declined");
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].bounty_id.as_deref(), Some(b.id.as_str()));
    assert_eq!(notices[0].data["reason"], "无空闲资源");
    assert_eq!(notices[0].data["declined_by_agent_name"], "target");
    assert!(k
        .inbox(&subscriber.agent.id, "task.published")
        .iter()
        .any(|e| e.bounty_id.as_deref() == Some(&b.id)));
    let event = k
        .repo
        .get_bounty_detail(&b.id)
        .unwrap()
        .events
        .pop()
        .unwrap();
    assert_eq!(event.event_type, "declined");
    assert_eq!(event.from_status, Some(BountyStatus::Open));
    assert_eq!(event.to_status, BountyStatus::Open);
    assert_eq!(event.actor, format!("agent:{}", target.agent.id));
    error(
        k.repo
            .agent_decline_bounty(&b.id, &target.agent.id, DeclineBounty::default()),
        "INVALID_TRANSITION",
        StatusCode::CONFLICT,
    );
    k.repo
        .update_project(
            &k.project.id,
            serde_json::from_value(json!({"agent_draft_policy":"draft"})).unwrap(),
        )
        .unwrap();
    let draft = k.agent_bounty(&publisher.agent.id, assigned);
    assert!(draft.assignment_expires_at.is_none());
    error(
        k.repo
            .agent_decline_bounty(&draft.id, &target.agent.id, DeclineBounty::default()),
        "BOUNTY_NOT_FOUND",
        StatusCode::NOT_FOUND,
    );
    let open = k.repo.publish_bounty(&draft.id, "human").unwrap();
    assert!(open.assignment_expires_at.is_some());
    let claimed = k
        .repo
        .agent_claim_bounty(&draft.id, &target.agent.id)
        .unwrap();
    assert!(claimed.assignment_expires_at.is_none());
    error(
        k.repo
            .agent_decline_bounty(&draft.id, &target.agent.id, DeclineBounty::default()),
        "INVALID_TRANSITION",
        StatusCode::CONFLICT,
    );
}

#[test]
fn fencing_error_order_and_completion_validation_preserve_the_lease() {
    let k = Kernel::new();
    let holder = k.agent("holder");
    let other = k.agent("other");
    let b = k.bounty(json!({}));
    let claimed = k.repo.agent_claim_bounty(&b.id, &holder.agent.id).unwrap();
    for (agent, token, code, status) in [
        (&other.agent.id, 0, "STALE_LEASE", StatusCode::CONFLICT),
        (
            &other.agent.id,
            1,
            "LEASE_OWNER_MISMATCH",
            StatusCode::FORBIDDEN,
        ),
    ] {
        error(
            k.repo
                .agent_heartbeat_bounty(&b.id, agent, LeaseHeartbeat { lease_id: token }),
            code,
            status,
        );
        error(
            k.repo.agent_abandon_bounty(
                &b.id,
                agent,
                AbandonBounty {
                    lease_id: token,
                    reason: None,
                },
            ),
            code,
            status,
        );
        error(
            k.repo
                .agent_complete_bounty(&b.id, agent, completion(token)),
            code,
            status,
        );
    }
    for cost in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut input = completion(1);
        input.completion_cost = Some(cost);
        error(
            k.repo.agent_complete_bounty(&b.id, &holder.agent.id, input),
            "VALIDATION_ERROR",
            StatusCode::BAD_REQUEST,
        );
    }
    assert_eq!(
        serde_json::to_value(k.snapshot(&b.id)).unwrap(),
        serde_json::to_value(&claimed).unwrap()
    );
    k.conn()
        .execute(
            "UPDATE bounties SET lease_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
            [&b.id],
        )
        .unwrap();
    error(
        k.repo
            .agent_heartbeat_bounty(&b.id, &holder.agent.id, LeaseHeartbeat { lease_id: 1 }),
        "STALE_LEASE",
        StatusCode::CONFLICT,
    );
    error(
        k.repo.agent_abandon_bounty(
            &b.id,
            &holder.agent.id,
            AbandonBounty {
                lease_id: 1,
                reason: None,
            },
        ),
        "STALE_LEASE",
        StatusCode::CONFLICT,
    );
    error(
        k.repo
            .agent_complete_bounty(&b.id, &holder.agent.id, completion(1)),
        "STALE_LEASE",
        StatusCode::CONFLICT,
    );
    let arbitration = k.bounty(json!({"require_arbitration":true}));
    k.repo
        .agent_claim_bounty(&arbitration.id, &holder.agent.id)
        .unwrap();
    error(
        k.repo
            .agent_complete_bounty(&arbitration.id, &holder.agent.id, completion(1)),
        "ARBITRATION_REQUIRED",
        StatusCode::CONFLICT,
    );
    for status in [
        "WAITING_INFO",
        "PENDING_ARBITRATION",
        "COMPLETED",
        "CANCELLED",
    ] {
        k.conn()
            .execute(
                "UPDATE bounties SET status=?2 WHERE id=?1",
                params![arbitration.id, status],
            )
            .unwrap();
        error(
            k.repo
                .agent_complete_bounty(&arbitration.id, &holder.agent.id, completion(1)),
            "INVALID_TRANSITION",
            StatusCode::CONFLICT,
        );
    }
    let draft = k.bounty(json!({"status":"DRAFT"}));
    for id in [&draft.id, "missing"] {
        error(
            k.repo
                .agent_heartbeat_bounty(id, &holder.agent.id, LeaseHeartbeat { lease_id: 0 }),
            "BOUNTY_NOT_FOUND",
            StatusCode::NOT_FOUND,
        );
        error(
            k.repo.agent_abandon_bounty(
                id,
                &holder.agent.id,
                AbandonBounty {
                    lease_id: 0,
                    reason: None,
                },
            ),
            "BOUNTY_NOT_FOUND",
            StatusCode::NOT_FOUND,
        );
        error(
            k.repo
                .agent_complete_bounty(id, &holder.agent.id, completion(0)),
            "BOUNTY_NOT_FOUND",
            StatusCode::NOT_FOUND,
        );
    }
    for note in [None, Some("  \n\t".to_string())] {
        let b = k.bounty(json!({}));
        k.repo.agent_claim_bounty(&b.id, &holder.agent.id).unwrap();
        let done = k
            .repo
            .agent_complete_bounty(
                &b.id,
                &holder.agent.id,
                CompleteBounty {
                    lease_id: 1,
                    completion_note: note,
                    completion_cost: Some(0.0),
                },
            )
            .unwrap();
        assert!(done.completion_note.is_none());
        assert_eq!(done.completion_cost, Some(0.0));
        error(
            k.repo
                .agent_complete_bounty(&b.id, &holder.agent.id, completion(1)),
            "INVALID_TRANSITION",
            StatusCode::CONFLICT,
        );
    }
}

#[test]
fn warning_is_once_until_heartbeat_and_sweeps_do_not_expire_suspended_tasks() {
    let k = Kernel::new();
    let a = k.agent("worker");
    let b = k.bounty(json!({}));
    k.repo.agent_claim_bounty(&b.id, &a.agent.id).unwrap();
    let near = (chrono::Utc::now() + chrono::Duration::minutes(2))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    k.conn()
        .execute(
            "UPDATE bounties SET lease_expires_at=?2 WHERE id=?1",
            params![b.id, near],
        )
        .unwrap();
    k.repo.sweep_leases().unwrap();
    k.repo.sweep_leases().unwrap();
    assert_eq!(k.inbox(&a.agent.id, "lease.expiring").len(), 1);
    let renewed = k
        .repo
        .agent_heartbeat_bounty(&b.id, &a.agent.id, LeaseHeartbeat { lease_id: 1 })
        .unwrap();
    assert!(renewed.lease_expires_at.as_ref().unwrap() > &near);
    assert_eq!(
        k.conn()
            .query_row(
                "SELECT lease_warned_id FROM bounties WHERE id=?1",
                [&b.id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    k.repo.sweep_leases().unwrap();
    assert_eq!(k.inbox(&a.agent.id, "lease.expiring").len(), 1);
    k.conn()
        .execute(
            "UPDATE bounties SET lease_expires_at=?2 WHERE id=?1",
            params![b.id, near],
        )
        .unwrap();
    k.repo.sweep_leases().unwrap();
    assert_eq!(k.inbox(&a.agent.id, "lease.expiring").len(), 2);
    for status in ["WAITING_INFO", "PENDING_ARBITRATION"] {
        k.conn().execute("UPDATE bounties SET status=?2,lease_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1", params![b.id,status]).unwrap();
        k.repo.sweep_leases().unwrap();
        assert_eq!(k.snapshot(&b.id).status.as_str(), status);
    }
    assert_eq!(
        k.conn()
            .query_row("SELECT COUNT(*) FROM lease_incidents", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn assignment_timeout_notifies_publisher_and_filters_inactive_recipients() {
    let k = Kernel::new();
    let publisher = k.agent("publisher");
    let target = k.agent("target");
    let subscriber = k.agent("subscriber");
    let disabled = k.agent("disabled");
    let revoked = k.agent("revoked");
    k.repo
        .set_agent_status(&disabled.agent.id, AgentStatus::Disabled)
        .unwrap();
    k.repo
        .set_agent_status(&revoked.agent.id, AgentStatus::Revoked)
        .unwrap();
    let b = k.agent_bounty(
        &publisher.agent.id,
        json!({"assignee_mode":"specific","assignee_agent_id":target.agent.id}),
    );
    k.conn()
        .execute(
            "UPDATE bounties SET assignment_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
            [&b.id],
        )
        .unwrap();
    k.repo.sweep_leases().unwrap();
    k.repo.sweep_leases().unwrap();
    let open = k.snapshot(&b.id);
    assert_eq!(open.assignee_mode, AssigneeMode::Any);
    assert!(open.assignee_agent_id.is_none());
    assert!(open.assignment_expires_at.is_none());
    assert_eq!(
        k.inbox(&publisher.agent.id, "task.assignment_expired")
            .len(),
        1
    );
    assert_eq!(k.inbox(&subscriber.agent.id, "task.published").len(), 1);
    for a in [&disabled, &revoked] {
        assert_eq!(
            k.conn()
                .query_row(
                    "SELECT COUNT(*) FROM agent_events WHERE agent_id=?1",
                    [&a.agent.id],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }
    let event = k
        .repo
        .get_bounty_detail(&b.id)
        .unwrap()
        .events
        .pop()
        .unwrap();
    assert_eq!(event.event_type, "assignment_expired");
    assert_eq!(event.actor, "system");
    // 发布者停用后仍回退并分发，但不写发布者专属通知。
    k.repo
        .set_agent_status(&publisher.agent.id, AgentStatus::Disabled)
        .unwrap();
    let next = k.bounty(json!({"assignee_mode":"specific","assignee_agent_id":target.agent.id}));
    k.conn()
        .execute(
            "UPDATE bounties SET created_by_agent_id=?2 WHERE id=?1",
            params![next.id, publisher.agent.id],
        )
        .unwrap();
    k.repo
        .agent_decline_bounty(&next.id, &target.agent.id, DeclineBounty::default())
        .unwrap();
    assert_eq!(k.conn().query_row("SELECT COUNT(*) FROM agent_events WHERE agent_id=?1 AND event_type='task.assignment_declined'", [&publisher.agent.id], |r| r.get::<_,i64>(0)).unwrap(), 0);
}

#[test]
fn admin_release_supports_held_states_resets_specific_window_and_never_records_incident() {
    let k = Kernel::new();
    let a = k.agent("worker");
    let subscriber = k.agent("subscriber");
    for status in ["IN_PROGRESS", "WAITING_INFO", "PENDING_ARBITRATION"] {
        let b = k.bounty(json!({"assignee_mode":"specific","assignee_agent_id":a.agent.id}));
        k.repo.agent_claim_bounty(&b.id, &a.agent.id).unwrap();
        k.conn()
            .execute(
                "UPDATE bounties SET status=?2 WHERE id=?1",
                params![b.id, status],
            )
            .unwrap();
        let open = k.repo.admin_release_lease(&b.id).unwrap();
        assert_eq!(open.status, BountyStatus::Open);
        assert_eq!(open.assignee_mode, AssigneeMode::Specific);
        assert!(open.assignment_expires_at.is_some());
        assert!(open.lease_agent_id.is_none());
        assert_eq!(open.lease_id, 1);
        let events = k.inbox(&a.agent.id, "lease.released");
        let released = events
            .iter()
            .find(|e| e.bounty_id.as_deref() == Some(&b.id))
            .unwrap();
        assert_eq!(released.data["lease_agent_name"], "worker");
        assert!(k
            .inbox(&a.agent.id, "task.assigned")
            .iter()
            .any(|e| e.bounty_id.as_deref() == Some(&b.id)));
        let event = k
            .repo
            .get_bounty_detail(&b.id)
            .unwrap()
            .events
            .pop()
            .unwrap();
        assert_eq!(event.event_type, "lease_released");
        assert_eq!(event.actor, "human");
        assert_eq!(event.from_status.unwrap().as_str(), status);
        assert_eq!(
            k.repo
                .agent_claim_bounty(&b.id, &a.agent.id)
                .unwrap()
                .lease_id,
            2
        );
    }
    for status in [
        "OPEN",
        "DRAFT",
        "COMPLETED",
        "CANCELLED",
        "WAITING_INFO",
        "PENDING_ARBITRATION",
    ] {
        let b = k.bounty(json!({}));
        k.conn()
            .execute(
                "UPDATE bounties SET status=?2 WHERE id=?1",
                params![b.id, status],
            )
            .unwrap();
        error(
            k.repo.admin_release_lease(&b.id),
            "INVALID_TRANSITION",
            StatusCode::CONFLICT,
        );
    }
    let b = k.bounty(json!({}));
    k.repo.agent_claim_bounty(&b.id, &a.agent.id).unwrap();
    k.repo
        .set_agent_status(&a.agent.id, AgentStatus::Disabled)
        .unwrap();
    k.repo.admin_release_lease(&b.id).unwrap();
    assert!(k
        .inbox(&subscriber.agent.id, "task.published")
        .iter()
        .any(|e| e.bounty_id.as_deref() == Some(&b.id)));
    assert_eq!(
        k.conn()
            .query_row("SELECT COUNT(*) FROM lease_incidents", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        k.conn()
            .query_row(
                "SELECT timeout_count+abandon_count FROM agents WHERE id=?1",
                [&a.agent.id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

fn stored_state(conn: &Connection) -> Vec<Vec<Vec<rusqlite::types::Value>>> {
    [
        "bounties",
        "agents",
        "bounty_events",
        "agent_events",
        "bounty_dispatches",
        "lease_incidents",
    ]
    .iter()
    .map(|table| {
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
            .unwrap();
        let columns = stmt.column_count();
        stmt.query_map([], |row| (0..columns).map(|i| row.get(i)).collect())
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    })
    .collect()
}

#[test]
fn failed_history_incident_notification_or_redispatch_rolls_back_the_entire_mutation() {
    let k = Kernel::new();
    let publisher = k.agent("publisher");
    let a = k.agent("worker");
    for (operation, tables) in [
        ("claim", vec!["bounty_events"]),
        (
            "decline",
            vec!["bounty_events", "agent_events", "bounty_dispatches"],
        ),
        (
            "abandon",
            vec![
                "lease_incidents",
                "bounty_events",
                "agent_events",
                "bounty_dispatches",
            ],
        ),
        ("complete", vec!["bounty_events"]),
        (
            "release",
            vec!["bounty_events", "agent_events", "bounty_dispatches"],
        ),
        ("warning", vec!["agent_events"]),
        (
            "timeout",
            vec![
                "lease_incidents",
                "bounty_events",
                "agent_events",
                "bounty_dispatches",
            ],
        ),
        (
            "assignment",
            vec!["bounty_events", "agent_events", "bounty_dispatches"],
        ),
    ] {
        for table in tables {
            let b = k.agent_bounty(
                &publisher.agent.id,
                json!({"assignee_mode":"specific","assignee_agent_id":a.agent.id}),
            );
            if matches!(
                operation,
                "abandon" | "complete" | "release" | "warning" | "timeout"
            ) {
                k.repo.agent_claim_bounty(&b.id, &a.agent.id).unwrap();
            }
            let conn = k.conn();
            if operation == "warning" {
                let near = (chrono::Utc::now() + chrono::Duration::minutes(2))
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
                conn.execute(
                    "UPDATE bounties SET lease_expires_at=?2 WHERE id=?1",
                    params![b.id, near],
                )
                .unwrap();
            } else if operation == "timeout" {
                conn.execute(
                    "UPDATE bounties SET lease_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
                    [&b.id],
                )
                .unwrap();
            } else if operation == "assignment" {
                conn.execute("UPDATE bounties SET assignment_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",[&b.id]).unwrap();
            }
            let before = stored_state(&conn);
            conn.execute_batch(&format!("CREATE TRIGGER fail_insert BEFORE INSERT ON {table} BEGIN SELECT RAISE(ABORT,'injected failure'); END;")).unwrap();
            let result = match operation {
                "claim" => k.repo.agent_claim_bounty(&b.id, &a.agent.id).map(|_| ()),
                "decline" => k
                    .repo
                    .agent_decline_bounty(&b.id, &a.agent.id, DeclineBounty::default())
                    .map(|_| ()),
                "abandon" => k
                    .repo
                    .agent_abandon_bounty(
                        &b.id,
                        &a.agent.id,
                        AbandonBounty {
                            lease_id: 1,
                            reason: None,
                        },
                    )
                    .map(|_| ()),
                "complete" => k
                    .repo
                    .agent_complete_bounty(&b.id, &a.agent.id, completion(1))
                    .map(|_| ()),
                "release" => k.repo.admin_release_lease(&b.id).map(|_| ()),
                _ => k.repo.sweep_leases(),
            };
            assert_eq!(
                result.unwrap_err().code,
                "INTERNAL_ERROR",
                "{operation}/{table}"
            );
            assert_eq!(stored_state(&conn), before, "{operation}/{table}");
            conn.execute_batch("DROP TRIGGER fail_insert;").unwrap();
            // 清理本轮到期条件，避免下一轮扫描先处理此前故意保留的失败任务。
            conn.execute(
                "UPDATE bounties SET status='CANCELLED' WHERE id=?1",
                [&b.id],
            )
            .unwrap();
        }
    }
}

#[test]
fn inactive_holder_still_times_out_without_private_notification() {
    let k = Kernel::new();
    let holder = k.agent("holder");
    let other = k.agent("other");
    let b = k.bounty(json!({}));
    k.repo.agent_claim_bounty(&b.id, &holder.agent.id).unwrap();
    k.repo
        .set_agent_status(&holder.agent.id, AgentStatus::Disabled)
        .unwrap();
    k.conn()
        .execute(
            "UPDATE bounties SET lease_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
            [&b.id],
        )
        .unwrap();
    k.repo.sweep_leases().unwrap();
    let summary = k
        .repo
        .list_agents(AgentFilter::default())
        .unwrap()
        .into_iter()
        .find(|a| a.id == holder.agent.id)
        .unwrap();
    assert_eq!(summary.timeout_count, 1);
    assert_eq!(summary.abandon_count, 0);
    assert!(
        k.inbox(&other.agent.id, "task.published")
            .iter()
            .filter(|e| e.bounty_id.as_deref() == Some(&b.id))
            .count()
            == 2
    );
    assert_eq!(k.conn().query_row("SELECT COUNT(*) FROM agent_events WHERE agent_id=?1 AND event_type='lease.expired'",[&holder.agent.id],|r|r.get::<_,i64>(0)).unwrap(),0);
}

#[tokio::test]
async fn http_execution_routes_validate_auth_bodies_and_support_empty_decline() {
    let k = Kernel::new();
    let a = k.agent("worker");
    let other = k.agent("other");
    let server = k.start().await;
    let client = Client::new();
    let b = k.bounty(json!({}));
    for action in ["claim", "decline", "heartbeat", "abandon", "complete"] {
        for key in [None, Some("wrong"), Some("leases-admin")] {
            let (status, body) = post(
                &client,
                &server,
                key,
                &format!("/agent/bounties/{}/{action}", b.id),
                Some(json!({"lease_id":0})),
            )
            .await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{action}");
            assert_eq!(body["error"]["code"], "UNAUTHORIZED");
        }
    }
    let (status, claimed) = post(
        &client,
        &server,
        Some(&a.api_key),
        &format!("/agent/bounties/{}/claim", b.id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(claimed["lease_id"], 1);
    assert_public(&claimed);
    let (status, body) = post(
        &client,
        &server,
        Some(&other.api_key),
        &format!("/agent/bounties/{}/heartbeat", b.id),
        Some(json!({"lease_id":1})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "LEASE_OWNER_MISMATCH");
    for action in ["heartbeat", "abandon", "complete"] {
        for body in [
            None,
            Some(json!({})),
            Some(json!({"lease_id":"1"})),
            Some(json!({"lease_id":1,"extra":true})),
        ] {
            let (status, error) = post(
                &client,
                &server,
                Some(&a.api_key),
                &format!("/agent/bounties/{}/{action}", b.id),
                body,
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{action}");
            assert_eq!(error["error"]["code"], "VALIDATION_ERROR");
        }
    }
    let (status, _) = post(
        &client,
        &server,
        Some(&a.api_key),
        &format!("/agent/bounties/{}/heartbeat", b.id),
        Some(json!({"lease_id":1})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = post(
        &client,
        &server,
        Some(&a.api_key),
        &format!("/agent/bounties/{}/abandon", b.id),
        Some(json!({"lease_id":1,"reason":"资源繁忙"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, claimed) = post(
        &client,
        &server,
        Some(&a.api_key),
        &format!("/agent/bounties/{}/claim", b.id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(claimed["lease_id"], 2);
    let (status, body) = post(
        &client,
        &server,
        Some(&a.api_key),
        &format!("/agent/bounties/{}/complete", b.id),
        Some(json!({"lease_id":2,"completion_cost":-1})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    let (status, done) = post(
        &client,
        &server,
        Some(&a.api_key),
        &format!("/agent/bounties/{}/complete", b.id),
        Some(json!({"lease_id":2,"completion_note":" 测试通过 ","completion_cost":3.5})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(done["status"], "COMPLETED");
    assert_eq!(done["completion_note"], "测试通过");
    for body in [None, Some(json!({"reason":"暂不接取"}))] {
        let b = k.bounty(json!({"assignee_mode":"specific","assignee_agent_id":a.agent.id}));
        let (status, open) = post(
            &client,
            &server,
            Some(&a.api_key),
            &format!("/agent/bounties/{}/decline", b.id),
            body,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(open["assignee_mode"], "any");
    }
    let assigned = k.bounty(json!({"assignee_mode":"specific","assignee_agent_id":a.agent.id}));
    for body in [json!({"extra":1}), json!({"reason":5}), Value::Null] {
        let (status, error) = post(
            &client,
            &server,
            Some(&a.api_key),
            &format!("/agent/bounties/{}/decline", assigned.id),
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error["error"]["code"], "VALIDATION_ERROR");
    }
    k.repo
        .set_agent_status(&a.agent.id, AgentStatus::Disabled)
        .unwrap();
    let (status, body) = post(
        &client,
        &server,
        Some(&a.api_key),
        &format!("/agent/bounties/{}/claim", assigned.id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "AGENT_DISABLED");
    k.repo
        .set_agent_status(&a.agent.id, AgentStatus::Revoked)
        .unwrap();
    let (status, body) = post(
        &client,
        &server,
        Some(&a.api_key),
        &format!("/agent/bounties/{}/decline", assigned.id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "AGENT_REVOKED");
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn http_admin_release_and_running_server_scheduler_are_live_and_shutdown_stops_sweeps() {
    let k = Kernel::new();
    let a = k.agent("worker");
    let b = k.bounty(json!({}));
    k.repo.agent_claim_bounty(&b.id, &a.agent.id).unwrap();
    let server = k.start().await;
    let client = Client::new();
    let path = format!("/bounties/{}/release-lease", b.id);
    for key in [None, Some(a.api_key.as_str())] {
        let (status, error) = post(&client, &server, key, &path, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(error["error"]["code"], "UNAUTHORIZED");
    }
    let (status, open) = post(&client, &server, Some("leases-admin"), &path, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(open["status"], "OPEN");
    k.repo.agent_claim_bounty(&b.id, &a.agent.id).unwrap();
    let assigned = k.bounty(json!({"assignee_mode":"specific","assignee_agent_id":a.agent.id}));
    k.conn()
        .execute(
            "UPDATE bounties SET lease_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
            [&b.id],
        )
        .unwrap();
    k.conn()
        .execute(
            "UPDATE bounties SET assignment_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
            [&assigned.id],
        )
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if k.snapshot(&b.id).status == BountyStatus::Open
                && k.snapshot(&assigned.id).assignee_mode == AssigneeMode::Any
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(k.inbox(&a.agent.id, "lease.expired").len(), 1);
    server.shutdown().await.unwrap();
    k.repo.agent_claim_bounty(&b.id, &a.agent.id).unwrap();
    k.conn()
        .execute(
            "UPDATE bounties SET lease_expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
            [&b.id],
        )
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    assert_eq!(k.snapshot(&b.id).status, BountyStatus::InProgress);
}

#[test]
fn v4_open_specific_assignments_gain_deadlines_and_expire_after_upgrade() {
    // 使用真实 v4 表结构与文件数据库，避免通过 v5 建库后删列掩盖迁移缺陷。
    let file = tempfile::Builder::new()
        .suffix(".db")
        .tempfile_in(env!("CARGO_MANIFEST_DIR"))
        .unwrap();
    let conn = Connection::open(file.path()).unwrap();
    for sql in [
        include_str!("fixtures/server_v1.sql"),
        include_str!("../src/server/schema_v2.sql"),
        include_str!("../src/server/schema_v3.sql"),
        include_str!("../src/server/schema_v4.sql"),
    ] {
        conn.execute_batch(sql).unwrap();
    }
    conn.execute_batch(
        "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL);
         INSERT INTO schema_migrations VALUES(4,'old');
         INSERT INTO projects(id,name,created_at,updated_at) VALUES('p','old','old','old');",
    )
    .unwrap();
    for id in ["publisher", "target", "other"] {
        conn.execute(
            "INSERT INTO agents(id,name,api_key_hash,api_key_prefix,created_at,updated_at) VALUES(?1,?1,?1,?1,'old','old')",
            [id],
        )
        .unwrap();
    }
    for status in [
        "DRAFT",
        "OPEN",
        "IN_PROGRESS",
        "WAITING_INFO",
        "PENDING_ARBITRATION",
        "COMPLETED",
        "CANCELLED",
    ] {
        for mode in ["any", "min_tier", "specific"] {
            conn.execute(
                "INSERT INTO bounties(id,project_id,title,difficulty,assignee_mode,assignee_min_tier,assignee_agent_id,status,created_by,created_by_agent_id,published_at,created_at,updated_at) VALUES(?1,'p','old','C',?2,?3,?4,?5,'agent','publisher','old','old','old')",
                params![
                    format!("{status}-{mode}"),
                    mode,
                    (mode == "min_tier").then_some("C"),
                    (mode == "specific").then_some("target"),
                    status
                ],
            )
            .unwrap();
        }
    }

    let before = chrono::Utc::now().timestamp_millis();
    let repo = Repository::open(file.path()).unwrap();
    let after = chrono::Utc::now().timestamp_millis();
    assert_eq!(
        conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        8
    );
    let id = "OPEN-specific";
    let assigned = repo.get_bounty_detail(id).unwrap().bounty;
    let deadline = assigned.assignment_expires_at.as_deref().unwrap();
    let parsed = chrono::DateTime::parse_from_rfc3339(deadline).unwrap();
    assert_eq!(
        deadline,
        parsed.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    );
    let window_ms = chrono::Duration::minutes(15).num_milliseconds();
    assert!((before + window_ms..=after + window_ms).contains(&parsed.timestamp_millis()));
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM bounties WHERE id!=?1 AND assignment_expires_at IS NOT NULL",
            [id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    let reopened = Repository::open(file.path()).unwrap();
    assert_eq!(
        reopened
            .get_bounty_detail(id)
            .unwrap()
            .bounty
            .assignment_expires_at,
        assigned.assignment_expires_at
    );
    repo.sweep_leases().unwrap();
    let pending = repo.get_bounty_detail(id).unwrap();
    assert_eq!(pending.bounty.assignee_mode, AssigneeMode::Specific);
    assert_eq!(
        pending.bounty.assignment_expires_at,
        assigned.assignment_expires_at
    );
    assert!(pending.events.is_empty());

    // 只推进该任务的截止时间，直接运行生产回收路径，无需等待十五分钟。
    conn.execute(
        "UPDATE bounties SET assignment_expires_at=?2 WHERE id=?1",
        params![
            id,
            (chrono::Utc::now() - chrono::Duration::seconds(1))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        ],
    )
    .unwrap();
    repo.sweep_leases().unwrap();
    repo.sweep_leases().unwrap();
    let detail = repo.get_bounty_detail(id).unwrap();
    assert_eq!(detail.bounty.status, BountyStatus::Open);
    assert_eq!(detail.bounty.assignee_mode, AssigneeMode::Any);
    assert!(detail.bounty.assignee_agent_id.is_none());
    assert!(detail.bounty.assignment_expires_at.is_none());
    assert_eq!(detail.events.len(), 1);
    assert_eq!(detail.events[0].event_type, "assignment_expired");
    assert_eq!(detail.events[0].actor, "system");
    assert_eq!(detail.events[0].detail["assignee_agent_id"], "target");
    for (agent, kind) in [
        ("publisher", "task.assignment_expired"),
        ("other", "task.published"),
    ] {
        let inbox = repo.agent_events(agent, Some(0), 500).unwrap();
        assert_eq!(
            inbox
                .events
                .iter()
                .filter(|e| { e.bounty_id.as_deref() == Some(id) && e.event_type == kind })
                .count(),
            1
        );
    }
    let claimed = repo.agent_claim_bounty(id, "other").unwrap();
    assert_eq!(claimed.status, BountyStatus::InProgress);
    assert_eq!(claimed.lease_agent_id.as_deref(), Some("other"));
}

#[test]
fn v5_upgrade_preserves_v4_data_and_rolls_back_earlier_steps_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    for version in 1..=4 {
        let path = dir.path().join(format!("v{version}.db"));
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(include_str!("fixtures/server_v1.sql"))
            .unwrap();
        for (v, sql) in [
            (2, include_str!("../src/server/schema_v2.sql")),
            (3, include_str!("../src/server/schema_v3.sql")),
            (4, include_str!("../src/server/schema_v4.sql")),
        ] {
            if version >= v {
                conn.execute_batch(sql).unwrap();
            }
        }
        conn.execute_batch(
            "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL);",
        )
        .unwrap();
        for v in 1..=version {
            conn.execute("INSERT INTO schema_migrations VALUES(?1,'old')", [v])
                .unwrap();
        }
        conn.execute_batch("INSERT INTO projects(id,name,created_at,updated_at) VALUES('p','old','old','old'); INSERT INTO bounties(id,project_id,title,difficulty,assignee_mode,status,published_at,created_at,updated_at) VALUES('b','p','old','C','any','OPEN','old','old','old');").unwrap();
        conn.execute_batch("CREATE TABLE lease_incidents(conflict TEXT);")
            .unwrap();
        let schema = |conn: &Connection| {
            conn.query_row(
                "SELECT group_concat(sql,';') FROM (SELECT sql FROM sqlite_master ORDER BY name)",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
        };
        let before = schema(&conn);
        assert_eq!(Repository::open(&path).unwrap_err().code, "INTERNAL_ERROR");
        assert_eq!(schema(&conn), before);
        assert_eq!(
            conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            version
        );
        conn.execute_batch("DROP TABLE lease_incidents;").unwrap();
        for _ in 0..2 {
            let repo = Repository::open(&path).unwrap();
            let b = repo.get_bounty_detail("b").unwrap().bounty;
            assert_eq!(b.title, "old");
            assert!(b.assignment_expires_at.is_none());
            assert!(b.completion_cost.is_none());
            assert!(b.lease_agent_name.is_none());
            assert_eq!(
                conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                8
            );
        }
        assert_eq!(
            conn.query_row("SELECT lease_warned_id FROM bounties", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM pragma_index_info('lease_incidents_agent') WHERE name IN ('agent_id','id')",[],|r|r.get::<_,i64>(0)).unwrap(),2);
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        conn.execute_batch("INSERT INTO agents(id,name,api_key_hash,api_key_prefix,created_at,updated_at) VALUES('a','old','hash','prefix','old','old');").unwrap();
        assert_eq!(
            conn.query_row("SELECT timeout_count+abandon_count FROM agents", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            0
        );
        assert!(conn.execute("INSERT INTO lease_incidents(bounty_id,agent_id,lease_id,kind,created_at) VALUES('b','missing',1,'timeout','old')",[]).is_err());
        assert!(conn.execute("INSERT INTO lease_incidents(bounty_id,agent_id,lease_id,kind,created_at) VALUES('b','a',1,'other','old')",[]).is_err());
        conn.execute("INSERT INTO lease_incidents(bounty_id,agent_id,lease_id,kind,created_at) VALUES('b','a',1,'timeout','old')",[]).unwrap();
        conn.execute("DELETE FROM agents WHERE id='a'", []).unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM lease_incidents", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
