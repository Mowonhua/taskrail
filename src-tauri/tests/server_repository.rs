#![cfg(not(target_os = "android"))]
//! 文件职责：从仓储公开契约验证持久化、状态机与跨连接并发。
use serde_json::json;
use taskrail_lib::server::{db::Repository, models::*};

fn project() -> CreateProject {
    serde_json::from_value(json!({"name":"Taskrail", "icon":"emoji:🚆"})).unwrap()
}

fn bounty(project_id: &str, status: &str) -> CreateBounty {
    serde_json::from_value(json!({"project_id":project_id,"title":"实现内核", "difficulty":"B", "assignee_mode":"any", "status":status})).unwrap()
}

#[test]
fn draft_publish_withdraw_and_claim_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::open(dir.path().join("taskrail.db")).unwrap();
    let p = repo.create_project(project()).unwrap();
    let b = repo.create_bounty(bounty(&p.id, "DRAFT")).unwrap();
    assert_eq!(b.status, BountyStatus::Draft);
    assert!(b.published_at.is_none());
    assert!(repo
        .list_bounties(BountyFilter::default())
        .unwrap()
        .is_empty());
    let b = repo.publish_bounty(&b.id, "human").unwrap();
    assert!(b.published_at.is_some());
    let b = repo.withdraw_bounty(&b.id, "human").unwrap();
    assert!(b.withdrawn_at.is_some());
    assert_eq!(
        repo.try_claim_bounty(&b.id, "agent-1").unwrap_err().code,
        "INVALID_TRANSITION"
    );
    repo.publish_bounty(&b.id, "human").unwrap();
    let b = repo.try_claim_bounty(&b.id, "agent-1").unwrap();
    assert_eq!(b.lease_id, 1);
    assert_eq!(b.status, BountyStatus::InProgress);
    assert_eq!(
        repo.withdraw_bounty(&b.id, "human").unwrap_err().code,
        "BOUNTY_ALREADY_CLAIMED"
    );
    let detail = repo.get_bounty_detail(&b.id).unwrap();
    assert_eq!(
        detail
            .events
            .iter()
            .map(|e| e.event_type.as_str())
            .collect::<Vec<_>>(),
        ["created", "published", "withdrawn", "published", "claimed"]
    );
}

#[test]
fn migrations_persist_and_project_crud_enforces_archive_and_references() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/taskrail.db");
    let repo = Repository::open(&path).unwrap();
    let p = repo.create_project(project()).unwrap();
    let repo = Repository::open(&path).unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "wal"
    );
    assert_eq!(
        repo.get_project(&p.id).unwrap().icon.as_deref(),
        Some("emoji:🚆")
    );
    assert_eq!(repo.list_projects(false).unwrap().len(), 1);
    let patch = serde_json::from_value(json!({"name":"改名","icon":null,"description":"desc","workspace_path":"D:/repo","default_acceptance_criteria":"pass","agent_draft_policy":"draft","arbitrator_chain":["judge"],"archived":true})).unwrap();
    let p = repo.update_project(&p.id, patch).unwrap();
    assert_eq!(p.name, "改名");
    assert!(p.icon.is_none());
    assert_eq!(p.arbitrator_chain, ["judge"]);
    assert_eq!(p.workspace_path, "D:/repo");
    assert_eq!(p.default_acceptance_criteria, "pass");
    assert_eq!(p.agent_draft_policy, AgentDraftPolicy::Draft);
    assert!(repo.list_projects(false).unwrap().is_empty());
    assert_eq!(repo.list_projects(true).unwrap().len(), 1);
    assert_eq!(
        repo.create_bounty(bounty(&p.id, "OPEN")).unwrap_err().code,
        "PROJECT_ARCHIVED"
    );
    repo.delete_project(&p.id).unwrap();
    assert_eq!(
        repo.get_project(&p.id).unwrap_err().code,
        "PROJECT_NOT_FOUND"
    );
    assert_eq!(
        repo.update_project(&p.id, UpdateProject::default())
            .unwrap_err()
            .code,
        "PROJECT_NOT_FOUND"
    );
    assert_eq!(
        repo.delete_project(&p.id).unwrap_err().code,
        "PROJECT_NOT_FOUND"
    );
    let p = repo.create_project(project()).unwrap();
    let b = repo.create_bounty(bounty(&p.id, "DRAFT")).unwrap();
    assert_eq!(
        repo.delete_project(&p.id).unwrap_err().code,
        "PROJECT_HAS_BOUNTIES"
    );
    repo.cancel_bounty(&b.id, "human").unwrap();
    assert_eq!(
        repo.delete_project(&p.id).unwrap_err().code,
        "PROJECT_HAS_BOUNTIES"
    );
    assert_eq!(
        repo.create_bounty(bounty("missing", "DRAFT"))
            .unwrap_err()
            .code,
        "PROJECT_NOT_FOUND"
    );
}

#[test]
fn validation_rejects_blank_names_invalid_icons_and_inconsistent_assignees() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::open(dir.path().join("taskrail.db")).unwrap();
    let mut p = project();
    p.name = " \n ".into();
    assert_eq!(repo.create_project(p).unwrap_err().code, "VALIDATION_ERROR");
    for icon in [
        "",
        "🚆",
        "foo:train",
        "emoji:",
        "emoji:ab",
        "emoji:\u{200d}",
        "lucide:",
        "lucide:Train",
        "lucide:a--b",
        "lucide:../train",
    ] {
        let mut p = project();
        p.icon = Some(icon.into());
        assert_eq!(
            repo.create_project(p).unwrap_err().code,
            "VALIDATION_ERROR",
            "{icon}"
        );
    }
    for icon in ["emoji:👨‍👩‍👧‍👦", "emoji:中", "emoji:🇨🇳", "lucide:train-front"]
    {
        let mut p = project();
        p.icon = Some(icon.into());
        repo.create_project(p).unwrap();
    }
    let p = repo.create_project(project()).unwrap();
    let mut b = bounty(&p.id, "DRAFT");
    b.title = " ".into();
    assert_eq!(repo.create_bounty(b).unwrap_err().code, "VALIDATION_ERROR");
    for status in [
        "IN_PROGRESS",
        "WAITING_INFO",
        "PENDING_ARBITRATION",
        "COMPLETED",
        "CANCELLED",
    ] {
        assert_eq!(
            repo.create_bounty(bounty(&p.id, status)).unwrap_err().code,
            "VALIDATION_ERROR"
        );
    }
    for (mode, min, agent) in [
        (AssigneeMode::Any, Some(Tier::B), None),
        (AssigneeMode::Any, None, Some("agent")),
        (AssigneeMode::MinTier, None, None),
        (AssigneeMode::MinTier, Some(Tier::B), Some("agent")),
        (AssigneeMode::Specific, None, None),
        (AssigneeMode::Specific, None, Some(" ")),
        (AssigneeMode::Specific, Some(Tier::B), Some("agent")),
    ] {
        let mut b = bounty(&p.id, "DRAFT");
        b.assignee_mode = mode;
        b.assignee_min_tier = min;
        b.assignee_agent_id = agent.map(str::to_owned);
        assert_eq!(repo.create_bounty(b).unwrap_err().code, "VALIDATION_ERROR");
    }
    for (mode, min, agent) in [
        (AssigneeMode::MinTier, Some(Tier::S), None),
        (AssigneeMode::Specific, None, Some("agent")),
    ] {
        let mut b = bounty(&p.id, "OPEN");
        b.assignee_mode = mode;
        b.assignee_min_tier = min;
        b.assignee_agent_id = agent.map(str::to_owned);
        repo.create_bounty(b).unwrap();
    }
}

#[test]
fn drafts_edit_publish_filter_and_non_drafts_reject_edits() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::open(dir.path().join("taskrail.db")).unwrap();
    let p = repo.create_project(project()).unwrap();
    let b = repo.create_bounty(bounty(&p.id, "DRAFT")).unwrap();
    let patch = serde_json::from_value(json!({"title":"改写%_","description":"description-match","acceptance_criteria":"criteria-match","difficulty":"S","assignee_mode":"specific","assignee_agent_id":"agent","require_arbitration":true})).unwrap();
    let b = repo.update_bounty(&b.id, patch, "human").unwrap();
    assert_eq!(b.title, "改写%_");
    assert_eq!(b.difficulty, Tier::S);
    assert!(b.require_arbitration);
    let patch = serde_json::from_value(
        json!({"assignee_mode":"min_tier","assignee_agent_id":null,"assignee_min_tier":"A"}),
    )
    .unwrap();
    let b = repo.update_bounty(&b.id, patch, "human").unwrap();
    assert_eq!(b.assignee_min_tier, Some(Tier::A));
    assert!(b.assignee_agent_id.is_none());
    let invalid = serde_json::from_value(json!({"assignee_mode":"any"})).unwrap();
    assert_eq!(
        repo.update_bounty(&b.id, invalid, "human")
            .unwrap_err()
            .code,
        "VALIDATION_ERROR"
    );
    assert_eq!(
        repo.get_bounty_detail(&b.id).unwrap().bounty.assignee_mode,
        AssigneeMode::MinTier
    );
    for q in [&b.id, "改写", "description-match", "criteria-match", "%_"] {
        let filter = BountyFilter {
            include_drafts: true,
            q: Some(q.to_owned()),
            ..Default::default()
        };
        assert_eq!(repo.list_bounties(filter).unwrap().len(), 1);
    }
    assert!(repo
        .list_bounties(BountyFilter {
            status: Some(BountyStatus::Draft),
            ..Default::default()
        })
        .unwrap()
        .is_empty());
    assert!(repo
        .list_bounties(BountyFilter {
            include_drafts: true,
            project_id: Some("other".into()),
            ..Default::default()
        })
        .unwrap()
        .is_empty());
    let b = repo.publish_bounty(&b.id, "human").unwrap();
    assert_eq!(
        repo.list_bounties(BountyFilter {
            project_id: Some(p.id),
            status: Some(BountyStatus::Open),
            ..Default::default()
        })
        .unwrap()
        .len(),
        1
    );
    assert_eq!(
        repo.publish_bounty(&b.id, "human").unwrap_err().code,
        "INVALID_TRANSITION"
    );
    assert_eq!(
        repo.update_bounty(&b.id, UpdateBounty::default(), "human")
            .unwrap_err()
            .code,
        "INVALID_TRANSITION"
    );
    let open = repo.create_bounty(bounty(&b.project_id, "OPEN")).unwrap();
    assert!(open.published_at.is_some());
    assert_eq!(
        repo.get_bounty_detail(&open.id).unwrap().events[0].to_status,
        BountyStatus::Open
    );
}

#[test]
fn cancellation_and_missing_bounty_errors_cover_all_states() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::open(dir.path().join("taskrail.db")).unwrap();
    let p = repo.create_project(project()).unwrap();
    for initial in [
        "DRAFT",
        "OPEN",
        "WAITING_INFO",
        "IN_PROGRESS",
        "PENDING_ARBITRATION",
        "COMPLETED",
        "CANCELLED",
    ] {
        let b = repo.create_bounty(bounty(&p.id, "DRAFT")).unwrap();
        // 后续执行期状态由其他功能提供；此处仅构造合法数据库快照验证取消边界。
        let conn = rusqlite::Connection::open(repo.path()).unwrap();
        conn.execute(
            "UPDATE bounties SET status=?2 WHERE id=?1",
            rusqlite::params![b.id, initial],
        )
        .unwrap();
        let result = repo.cancel_bounty(&b.id, "human");
        if matches!(initial, "DRAFT" | "OPEN" | "WAITING_INFO") {
            let cancelled = result.unwrap();
            assert_eq!(cancelled.status, BountyStatus::Cancelled);
            assert!(cancelled.cancelled_at.is_some());
            let detail = repo.get_bounty_detail(&b.id).unwrap();
            assert_eq!(detail.events.last().unwrap().event_type, "cancelled");
        } else {
            assert_eq!(result.unwrap_err().code, "INVALID_TRANSITION");
        }
        if initial != "DRAFT" {
            assert_eq!(
                repo.update_bounty(&b.id, UpdateBounty::default(), "human")
                    .unwrap_err()
                    .code,
                "INVALID_TRANSITION"
            );
        }
    }
    for result in [
        repo.publish_bounty("missing", "human"),
        repo.withdraw_bounty("missing", "human"),
        repo.cancel_bounty("missing", "human"),
        repo.try_claim_bounty("missing", "agent"),
        repo.update_bounty("missing", UpdateBounty::default(), "human"),
    ] {
        assert_eq!(result.unwrap_err().code, "BOUNTY_NOT_FOUND");
    }
    assert_eq!(
        repo.get_bounty_detail("missing").unwrap_err().code,
        "BOUNTY_NOT_FOUND"
    );
}

#[test]
fn claim_and_withdraw_are_exclusive_across_independent_connections() {
    use std::sync::{Arc, Barrier};
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::open(dir.path().join("taskrail.db")).unwrap();
    let p = repo.create_project(project()).unwrap();
    for _ in 0..24 {
        let b = repo.create_bounty(bounty(&p.id, "OPEN")).unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let claimant = repo.clone();
        let withdrawer = repo.clone();
        let claim_id = b.id.clone();
        let withdraw_id = b.id.clone();
        let claim_gate = barrier.clone();
        let withdraw_gate = barrier.clone();
        let claim = std::thread::spawn(move || {
            claim_gate.wait();
            claimant.try_claim_bounty(&claim_id, "agent")
        });
        let withdraw = std::thread::spawn(move || {
            withdraw_gate.wait();
            withdrawer.withdraw_bounty(&withdraw_id, "human")
        });
        barrier.wait();
        let claim = claim.join().unwrap();
        let withdraw = withdraw.join().unwrap();
        assert_ne!(claim.is_ok(), withdraw.is_ok());
        let detail = repo.get_bounty_detail(&b.id).unwrap();
        assert_eq!(detail.events.len(), 2);
        match (claim, withdraw) {
            (Ok(claimed), Err(error)) => {
                assert_eq!(error.code, "BOUNTY_ALREADY_CLAIMED");
                assert_eq!(claimed.lease_id, 1);
                assert_eq!(claimed.lease_agent_id.as_deref(), Some("agent"));
                let expires = chrono::DateTime::parse_from_rfc3339(
                    claimed.lease_expires_at.as_deref().unwrap(),
                )
                .unwrap();
                assert!(
                    (expires.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_minutes() >= 14
                );
                assert_eq!(detail.bounty.status, BountyStatus::InProgress);
            }
            (Err(error), Ok(_)) => {
                assert_eq!(error.code, "INVALID_TRANSITION");
                assert_eq!(detail.bounty.status, BountyStatus::Draft);
                assert_eq!(detail.bounty.lease_id, 0);
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn competing_claims_have_one_winner_and_fencing_token_increases() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::open(dir.path().join("taskrail.db")).unwrap();
    let p = repo.create_project(project()).unwrap();
    let b = repo.create_bounty(bounty(&p.id, "OPEN")).unwrap();
    let gate = std::sync::Arc::new(std::sync::Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|i| {
            let repo = repo.clone();
            let gate = gate.clone();
            let id = b.id.clone();
            std::thread::spawn(move || {
                gate.wait();
                repo.try_claim_bounty(&id, &format!("agent-{i}"))
            })
        })
        .collect();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(results
        .iter()
        .filter_map(|r| r.as_ref().err())
        .all(|e| e.code == "BOUNTY_ALREADY_CLAIMED"));
    let conn = rusqlite::Connection::open(repo.path()).unwrap();
    conn.execute(
        "UPDATE bounties SET status='OPEN',lease_agent_id=NULL,lease_expires_at=NULL WHERE id=?1",
        [&b.id],
    )
    .unwrap();
    assert_eq!(repo.try_claim_bounty(&b.id, "next").unwrap().lease_id, 2);
}

#[test]
fn failed_event_insert_rolls_back_state_and_timestamp() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::open(dir.path().join("taskrail.db")).unwrap();
    let p = repo.create_project(project()).unwrap();
    let b = repo.create_bounty(bounty(&p.id, "DRAFT")).unwrap();
    let conn = rusqlite::Connection::open(repo.path()).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_publish BEFORE INSERT ON bounty_events WHEN NEW.event_type='published' BEGIN SELECT RAISE(ABORT,'event write failure'); END;").unwrap();
    assert_eq!(
        repo.publish_bounty(&b.id, "human").unwrap_err().code,
        "INTERNAL_ERROR"
    );
    let detail = repo.get_bounty_detail(&b.id).unwrap();
    assert_eq!(detail.bounty.status, BountyStatus::Draft);
    assert!(detail.bounty.published_at.is_none());
    assert_eq!(detail.events.len(), 1);
}
