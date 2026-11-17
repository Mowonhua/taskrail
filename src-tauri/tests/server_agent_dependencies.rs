#![cfg(not(target_os = "android"))]
//! 草稿共创、依赖图和解锁通知的事务集成验证。
use reqwest::StatusCode;
use rusqlite::{params, Connection};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use taskrail_lib::server::{self, db::Repository, models::*};

fn dto<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).unwrap()
}
struct Kernel {
    repo: Repository,
    project: Project,
    _dir: tempfile::TempDir,
}
impl Kernel {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::open(dir.path().join("dependencies.db")).unwrap();
        repo.onboarding(Some(UpdateOnboarding {
            lan_open_registration: true,
        }))
        .unwrap();
        let project = repo
            .create_project(dto(json!({"name":"dependencies"})))
            .unwrap();
        Self {
            repo,
            project,
            _dir: dir,
        }
    }
    fn agent(&self, name: &str) -> RegisteredAgent {
        self.repo.register_agent(dto(json!({"name":name}))).unwrap()
    }
    fn bounty(&self, extra: Value) -> Bounty {
        let mut value = json!({"project_id":self.project.id,"title":"task","difficulty":"C","assignee_mode":"any","status":"DRAFT"});
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.repo.create_bounty(dto(value)).unwrap()
    }
    fn snapshot(&self, id: &str) -> Bounty {
        self.repo.get_bounty_detail(id).unwrap().bounty
    }
    fn conn(&self) -> Connection {
        let conn = Connection::open(self.repo.path()).unwrap();
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        conn
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
    fn complete(&self, id: &str, agent: &str) {
        let b = self.repo.agent_claim_bounty(id, agent).unwrap();
        self.repo
            .agent_complete_bounty(id, agent, dto(json!({"lease_id":b.lease_id})))
            .unwrap();
    }
}
fn error<T: std::fmt::Debug>(result: server::error::Result<T>, status: StatusCode, code: &str) {
    let e = result.unwrap_err();
    assert_eq!(e.status, status);
    assert_eq!(e.code, code);
}

#[test]
fn blocked_open_defers_assignment_dispatch_and_claim_until_completion() {
    let k = Kernel::new();
    let a = k.agent("worker");
    let prerequisite = k.bounty(json!({"status":"OPEN"}));
    let b = k.bounty(json!({"status":"OPEN","blocked_by":[prerequisite.id,prerequisite.id],"assignee_mode":"specific","assignee_agent_id":a.agent.id}));
    let value = serde_json::to_value(&b).unwrap();
    assert_eq!(value["is_blocked"], true);
    assert_eq!(value["blocked_by"], json!([prerequisite.id]));
    assert!(b.assignment_expires_at.is_none());
    assert!(k.inbox(&a.agent.id, "task.assigned").is_empty());
    error(
        k.repo.agent_claim_bounty(&b.id, &a.agent.id),
        StatusCode::CONFLICT,
        "BOUNTY_BLOCKED",
    );
    error(
        k.repo
            .agent_decline_bounty(&b.id, &a.agent.id, DeclineBounty::default()),
        StatusCode::CONFLICT,
        "BOUNTY_BLOCKED",
    );
    k.complete(&prerequisite.id, &a.agent.id);
    let unlocked = k.snapshot(&b.id);
    assert!(!serde_json::to_value(&unlocked).unwrap()["is_blocked"]
        .as_bool()
        .unwrap());
    assert!(unlocked.assignment_expires_at.is_some());
    let events = k.inbox(&a.agent.id, "task.assigned");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data["is_blocked"], false);
    k.repo.agent_claim_bounty(&b.id, &a.agent.id).unwrap();
}

#[test]
fn invited_collaboration_is_isolated_and_discussion_never_starts_execution() {
    let k = Kernel::new();
    let creator = k.agent("creator");
    let collaborator = k.agent("collaborator");
    let outsider = k.agent("outsider");
    k.repo
        .update_project(&k.project.id, dto(json!({"agent_draft_policy":"draft"})))
        .unwrap();
    let draft = k.repo.agent_create_bounty(&creator.agent.id, dto(json!({"project_id":k.project.id,"title":"secret","difficulty":"C","assignee_mode":"any"}))).unwrap();
    let b = k
        .repo
        .invite_draft_collaborator(&draft.id, dto(json!({"agent_id":collaborator.agent.id})))
        .unwrap();
    assert_eq!(
        b.draft_collaborator_agent_name.as_deref(),
        Some("collaborator")
    );
    assert_eq!(b.draft_collaborator_agent_tier, Some(AgentTier::Unranked));
    let invited = k.inbox(&collaborator.agent.id, "draft.invited");
    assert_eq!(invited.len(), 1);
    assert_eq!(invited[0].data["bounty"]["title"], "secret");
    for a in [&creator, &outsider] {
        for query in [
            json!({}),
            json!({"include_drafts":true}),
            json!({"status":"DRAFT","include_drafts":true}),
        ] {
            assert!(k
                .repo
                .agent_bounties(&a.agent.id, dto(query))
                .unwrap()
                .is_empty());
        }
        error(
            k.repo.agent_bounty_detail(&a.agent.id, &b.id),
            StatusCode::NOT_FOUND,
            "BOUNTY_NOT_FOUND",
        );
        error(
            k.repo.agent_bounty_comments(&b.id, &a.agent.id),
            StatusCode::NOT_FOUND,
            "BOUNTY_NOT_FOUND",
        );
        error(
            k.repo
                .agent_create_comment(&b.id, &a.agent.id, dto(json!({"body":"guess"}))),
            StatusCode::NOT_FOUND,
            "BOUNTY_NOT_FOUND",
        );
        error(
            k.repo
                .agent_update_draft(&b.id, &a.agent.id, dto(json!({"title":"guess"}))),
            StatusCode::NOT_FOUND,
            "BOUNTY_NOT_FOUND",
        );
        error(
            k.repo
                .agent_split_draft(&b.id, &a.agent.id, dto(json!({"items":[]}))),
            StatusCode::NOT_FOUND,
            "BOUNTY_NOT_FOUND",
        );
        assert!(k
            .repo
            .agent_events(&a.agent.id, Some(0), 500)
            .unwrap()
            .events
            .is_empty());
    }
    assert_eq!(
        k.repo
            .agent_bounties(&collaborator.agent.id, dto(json!({"status":"DRAFT"})))
            .unwrap()
            .len(),
        1
    );
    k.repo
        .agent_bounty_detail(&collaborator.agent.id, &b.id)
        .unwrap();
    let revised = k
        .repo
        .agent_update_draft(
            &b.id,
            &collaborator.agent.id,
            dto(json!({"title":"revised","description":"body","acceptance_criteria":"tested"})),
        )
        .unwrap();
    assert_eq!(revised.title, "revised");
    assert_eq!(revised.description, "body");
    assert_eq!(revised.acceptance_criteria, "tested");
    assert!(revised.draft_needs_human_review);
    error(
        k.repo
            .agent_update_draft(&b.id, &collaborator.agent.id, dto(json!({"title":"  "}))),
        StatusCode::BAD_REQUEST,
        "VALIDATION_ERROR",
    );
    let human = k
        .repo
        .create_bounty_comment(&b.id, dto(json!({"body":"please split"})))
        .unwrap();
    assert!(!human.bounty.draft_needs_human_review);
    let notices = k.inbox(&collaborator.agent.id, "comment.added");
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].data["comment"]["body"], "please split");
    let reply = k
        .repo
        .agent_create_comment(
            &b.id,
            &collaborator.agent.id,
            dto(json!({"body":"will do"})),
        )
        .unwrap();
    assert!(reply.bounty.draft_needs_human_review);
    assert_eq!(
        k.repo
            .agent_bounty_comments(&b.id, &collaborator.agent.id)
            .unwrap()
            .len(),
        2
    );
    k.repo.sweep_leases().unwrap();
    let snapshot = k.snapshot(&b.id);
    assert_eq!(snapshot.status, BountyStatus::Draft);
    assert_eq!(snapshot.lease_id, 0);
    assert!(snapshot.lease_agent_id.is_none());
    assert!(snapshot.lease_expires_at.is_none());
    assert!(snapshot.active_started_at.is_none());
    assert!(snapshot.assignment_expires_at.is_none());
    assert!(snapshot.arbitrator_agent_id.is_none());
    assert!(snapshot.arbitration_expires_at.is_none());
    assert_eq!(snapshot.active_duration_ms, 0);
    assert_eq!(
        k.repo
            .list_agents(AgentFilter::default())
            .unwrap()
            .iter()
            .map(|a| a.active_lease_count)
            .sum::<i64>(),
        0
    );
    for a in [&creator, &outsider] {
        assert!(k
            .repo
            .agent_events(&a.agent.id, Some(0), 500)
            .unwrap()
            .events
            .is_empty());
    }
    assert_eq!(
        k.repo
            .get_bounty_detail(&b.id)
            .unwrap()
            .events
            .iter()
            .filter(|e| e.event_type == "draft_revised")
            .count(),
        1
    );
    k.repo
        .invite_draft_collaborator(&b.id, dto(json!({"agent_id":outsider.agent.id})))
        .unwrap();
    error(
        k.repo.agent_bounty_detail(&collaborator.agent.id, &b.id),
        StatusCode::NOT_FOUND,
        "BOUNTY_NOT_FOUND",
    );
    k.repo
        .invite_draft_collaborator(&b.id, dto(json!({"agent_id":null})))
        .unwrap();
    error(
        k.repo.agent_bounty_detail(&outsider.agent.id, &b.id),
        StatusCode::NOT_FOUND,
        "BOUNTY_NOT_FOUND",
    );
    assert_eq!(
        k.repo
            .get_bounty_detail(&b.id)
            .unwrap()
            .events
            .last()
            .unwrap()
            .event_type,
        "draft_collaborator_cleared"
    );
}

#[test]
fn invitation_validation_and_published_mutation_error_priority() {
    let k = Kernel::new();
    let a = k.agent("active");
    let b = k.bounty(json!({}));
    error(
        k.repo
            .invite_draft_collaborator(&b.id, dto(json!({"agent_id":" "}))),
        StatusCode::BAD_REQUEST,
        "VALIDATION_ERROR",
    );
    error(
        k.repo
            .invite_draft_collaborator(&b.id, dto(json!({"agent_id":"missing"}))),
        StatusCode::NOT_FOUND,
        "AGENT_NOT_FOUND",
    );
    for state in [AgentStatus::Disabled, AgentStatus::Revoked] {
        let inactive = k.agent(state.as_str());
        k.repo.set_agent_status(&inactive.agent.id, state).unwrap();
        error(
            k.repo
                .invite_draft_collaborator(&b.id, dto(json!({"agent_id":inactive.agent.id}))),
            StatusCode::CONFLICT,
            "AGENT_INACTIVE",
        );
    }
    k.repo
        .invite_draft_collaborator(&b.id, dto(json!({"agent_id":a.agent.id})))
        .unwrap();
    k.repo.publish_bounty(&b.id, "human").unwrap();
    error(
        k.repo
            .invite_draft_collaborator(&b.id, dto(json!({"agent_id":"missing"}))),
        StatusCode::CONFLICT,
        "INVALID_TRANSITION",
    );
    error(
        k.repo
            .agent_update_draft(&b.id, &a.agent.id, AgentUpdateDraftInput::default()),
        StatusCode::CONFLICT,
        "INVALID_TRANSITION",
    );
    error(
        k.repo
            .agent_split_draft(&b.id, &a.agent.id, dto(json!({"items":[]}))),
        StatusCode::CONFLICT,
        "INVALID_TRANSITION",
    );
    let other = k.agent("other");
    error(
        k.repo
            .agent_update_draft(&b.id, &other.agent.id, AgentUpdateDraftInput::default()),
        StatusCode::NOT_FOUND,
        "BOUNTY_NOT_FOUND",
    );
    k.repo.withdraw_bounty(&b.id, "human").unwrap();
    k.repo
        .set_agent_status(&a.agent.id, AgentStatus::Disabled)
        .unwrap();
    k.repo
        .create_bounty_comment(&b.id, dto(json!({"body":"no delivery"})))
        .unwrap();
    error(
        k.repo
            .agent_update_draft(&b.id, &a.agent.id, AgentUpdateDraftInput::default()),
        StatusCode::FORBIDDEN,
        "AGENT_DISABLED",
    );
    assert_eq!(
        k.conn()
            .query_row(
                "SELECT COUNT(*) FROM agent_events WHERE event_type='comment.added'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

fn split_items() -> Value {
    json!({"items":[{"key":" later ","title":"downstream","difficulty":"B","assignee_mode":"any","blocked_by":["first"]},{"key":"first","title":"upstream","difficulty":"C","assignee_mode":"any"}]})
}

#[test]
fn split_forward_keys_existing_dependencies_and_batch_publish_preserve_order() {
    let k = Kernel::new();
    let a = k.agent("splitter");
    let upstream = k.bounty(json!({"status":"OPEN"}));
    let parent = k.bounty(json!({}));
    k.repo
        .invite_draft_collaborator(&parent.id, dto(json!({"agent_id":a.agent.id})))
        .unwrap();
    let mut input = split_items();
    input["items"][0]["blocked_by"] = json!(["first", upstream.id, "first"]);
    let result = k
        .repo
        .agent_split_draft(&parent.id, &a.agent.id, dto(input))
        .unwrap();
    assert!(result.parent.draft_needs_human_review);
    assert_eq!(result.children.len(), 2);
    let child = &result.children[0];
    assert_eq!(
        child.blocked_by,
        vec![result.children[1].id.clone(), upstream.id.clone()]
    );
    for c in &result.children {
        assert_eq!(c.status, BountyStatus::Draft);
        assert_eq!(c.project_id, parent.project_id);
        assert_eq!(c.created_by, format!("agent:{}", a.agent.id));
        assert_eq!(c.created_by_agent_id.as_deref(), Some(a.agent.id.as_str()));
        assert_eq!(c.parent_bounty_id.as_deref(), Some(parent.id.as_str()));
        assert_eq!(
            c.draft_collaborator_agent_id.as_deref(),
            Some(a.agent.id.as_str())
        );
        assert!(c.draft_needs_human_review);
        assert!(c.published_at.is_none());
    }
    assert_eq!(
        k.repo
            .get_bounty_detail(&parent.id)
            .unwrap()
            .child_bounties
            .len(),
        2
    );
    assert_eq!(
        k.repo
            .get_bounty_detail(&result.children[1].id)
            .unwrap()
            .blocks[0]
            .id,
        child.id
    );
    let ids = vec![
        child.id.clone(),
        result.children[1].id.clone(),
        child.id.clone(),
    ];
    let published = k
        .repo
        .publish_bounties_batch(dto(json!({"bounty_ids":ids})), "human")
        .unwrap();
    assert_eq!(published.len(), 2);
    assert_eq!(published[0].id, child.id);
    assert!(published[0].is_blocked);
    assert!(!published[1].is_blocked);
    for c in &published {
        assert!(!c.draft_needs_human_review);
        assert_eq!(c.status, BountyStatus::Open);
    }
    assert_eq!(
        k.inbox(&a.agent.id, "task.published")
            .iter()
            .filter(|e| e.bounty_id.as_deref() == Some(child.id.as_str()))
            .count(),
        0
    );
    k.complete(&upstream.id, &a.agent.id);
    assert!(k.snapshot(&child.id).is_blocked);
    k.complete(&result.children[1].id, &a.agent.id);
    assert!(!k.snapshot(&child.id).is_blocked);
    k.repo.agent_claim_bounty(&child.id, &a.agent.id).unwrap();
}

#[test]
fn all_cycles_missing_cancelled_and_hidden_dependencies_roll_back() {
    let k = Kernel::new();
    let a = k.agent("editor");
    let first = k.bounty(json!({}));
    let second = k.bounty(json!({"blocked_by":[first.id]}));
    let third = k.bounty(json!({"blocked_by":[second.id]}));
    k.repo
        .invite_draft_collaborator(&first.id, dto(json!({"agent_id":a.agent.id})))
        .unwrap();
    for ids in [json!([first.id]), json!([second.id]), json!([third.id])] {
        let before = serde_json::to_value(k.repo.get_bounty_detail(&first.id).unwrap()).unwrap();
        error(
            k.repo.update_bounty(
                &first.id,
                dto(json!({"title":"should roll back","blocked_by":ids})),
                "human",
            ),
            StatusCode::CONFLICT,
            "DEPENDENCY_CYCLE",
        );
        assert_eq!(
            serde_json::to_value(k.repo.get_bounty_detail(&first.id).unwrap()).unwrap(),
            before
        );
    }
    for b in [&second, &third] {
        k.repo
            .invite_draft_collaborator(&b.id, dto(json!({"agent_id":a.agent.id})))
            .unwrap();
    }
    for target in [&first.id, &second.id, &third.id] {
        error(
            k.repo
                .agent_update_draft(&first.id, &a.agent.id, dto(json!({"blocked_by":[target]}))),
            StatusCode::CONFLICT,
            "DEPENDENCY_CYCLE",
        );
    }
    for (target, status, code) in [
        (
            "missing".to_string(),
            StatusCode::NOT_FOUND,
            "BOUNTY_NOT_FOUND",
        ),
        (" ".to_string(), StatusCode::BAD_REQUEST, "VALIDATION_ERROR"),
    ] {
        error(
            k.repo
                .update_bounty(&first.id, dto(json!({"blocked_by":[target]})), "human"),
            status,
            code,
        );
    }
    let cancelled = k.bounty(json!({}));
    k.repo.cancel_bounty(&cancelled.id, "human").unwrap();
    error(
        k.repo.update_bounty(
            &first.id,
            dto(json!({"blocked_by":[cancelled.id]})),
            "human",
        ),
        StatusCode::CONFLICT,
        "INVALID_TRANSITION",
    );
    let hidden = k.bounty(json!({"title":"hidden prerequisite"}));
    error(
        k.repo.agent_update_draft(
            &first.id,
            &a.agent.id,
            dto(json!({"blocked_by":[hidden.id]})),
        ),
        StatusCode::NOT_FOUND,
        "BOUNTY_NOT_FOUND",
    );
    let count = k
        .conn()
        .query_row("SELECT COUNT(*) FROM bounties", [], |r| r.get::<_, i64>(0))
        .unwrap();
    for target in [hidden.id.as_str(), "missing", cancelled.id.as_str(), " "] {
        let input = json!({"project_id":k.project.id,"title":"new","difficulty":"C","assignee_mode":"any","blocked_by":[target]});
        assert!(k
            .repo
            .agent_create_bounty(&a.agent.id, dto(input.clone()))
            .is_err());
        if target != hidden.id {
            let mut input = input;
            input["status"] = json!("OPEN");
            assert!(k.repo.create_bounty(dto(input)).is_err());
        }
    }
    assert_eq!(
        k.conn()
            .query_row("SELECT COUNT(*) FROM bounties", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        count
    );
}

#[test]
fn split_rejects_cycles_ambiguous_keys_invalid_items_and_archived_projects_atomically() {
    let k = Kernel::new();
    let a = k.agent("splitter");
    let parent = k.bounty(json!({}));
    k.repo
        .invite_draft_collaborator(&parent.id, dto(json!({"agent_id":a.agent.id})))
        .unwrap();
    let mut self_cycle = split_items();
    self_cycle["items"][0]["blocked_by"] = json!(["later"]);
    let mut two_cycle = split_items();
    two_cycle["items"][1]["blocked_by"] = json!(["later"]);
    let mut three_cycle = split_items();
    three_cycle["items"][1]["blocked_by"] = json!(["third"]);
    three_cycle["items"].as_array_mut().unwrap().push(json!({"key":"third","title":"third","difficulty":"C","assignee_mode":"any","blocked_by":["later"]}));
    let mut duplicate = split_items();
    duplicate["items"][1]["key"] = json!("later");
    let mut collision = split_items();
    collision["items"][1]["key"] = json!(parent.id);
    let mut empty_key = split_items();
    empty_key["items"][1]["key"] = json!("  ");
    let mut blank_title = split_items();
    blank_title["items"][1]["title"] = json!("  ");
    let mut missing = split_items();
    missing["items"][1]["blocked_by"] = json!(["missing"]);
    for (input, code) in [
        (self_cycle, "DEPENDENCY_CYCLE"),
        (two_cycle, "DEPENDENCY_CYCLE"),
        (three_cycle, "DEPENDENCY_CYCLE"),
        (duplicate, "VALIDATION_ERROR"),
        (collision, "VALIDATION_ERROR"),
        (empty_key, "VALIDATION_ERROR"),
        (blank_title, "VALIDATION_ERROR"),
        (missing, "BOUNTY_NOT_FOUND"),
        (json!({"items":[]}), "VALIDATION_ERROR"),
    ] {
        let before = serde_json::to_value(k.repo.get_bounty_detail(&parent.id).unwrap()).unwrap();
        assert_eq!(
            k.repo
                .agent_split_draft(&parent.id, &a.agent.id, dto(input))
                .unwrap_err()
                .code,
            code
        );
        assert_eq!(
            k.conn()
                .query_row("SELECT COUNT(*) FROM bounties", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            serde_json::to_value(k.repo.get_bounty_detail(&parent.id).unwrap()).unwrap(),
            before
        );
    }
    k.repo
        .update_project(&k.project.id, dto(json!({"archived":true})))
        .unwrap();
    error(
        k.repo
            .agent_split_draft(&parent.id, &a.agent.id, dto(split_items())),
        StatusCode::CONFLICT,
        "PROJECT_ARCHIVED",
    );
}

#[test]
fn dependency_write_and_batch_publish_failures_restore_all_rows_and_events() {
    let k = Kernel::new();
    let a = k.agent("editor");
    let prerequisite = k.bounty(json!({}));
    let first = k.bounty(json!({"blocked_by":[prerequisite.id]}));
    let second = k.bounty(json!({"assignee_mode":"specific","assignee_agent_id":a.agent.id}));
    k.repo
        .invite_draft_collaborator(&first.id, dto(json!({"agent_id":a.agent.id})))
        .unwrap();
    let before = serde_json::to_value(k.repo.get_bounty_detail(&first.id).unwrap()).unwrap();
    let conn = k.conn();
    conn.execute_batch("CREATE TRIGGER fail_dependency BEFORE INSERT ON bounty_dependencies BEGIN SELECT RAISE(ABORT,'dependency'); END;").unwrap();
    error(
        k.repo.update_bounty(
            &first.id,
            dto(json!({"title":"changed","blocked_by":[second.id]})),
            "human",
        ),
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    );
    error(
        k.repo
            .agent_split_draft(&first.id, &a.agent.id, dto(split_items())),
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    );
    assert_eq!(
        serde_json::to_value(k.repo.get_bounty_detail(&first.id).unwrap()).unwrap(),
        before
    );
    conn.execute_batch("DROP TRIGGER fail_dependency;").unwrap();
    for ids in [json!([]), json!([" "]), json!([first.id, "missing"])] {
        assert!(k
            .repo
            .publish_bounties_batch(dto(json!({"bounty_ids":ids})), "human")
            .is_err());
        assert_eq!(k.snapshot(&first.id).status, BountyStatus::Draft);
    }
    conn.execute_batch("CREATE TRIGGER fail_dispatch BEFORE INSERT ON agent_events WHEN NEW.event_type='task.assigned' BEGIN SELECT RAISE(ABORT,'dispatch'); END;").unwrap();
    error(
        k.repo
            .publish_bounties_batch(dto(json!({"bounty_ids":[first.id,second.id]})), "human"),
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    );
    for id in [&first.id, &second.id] {
        let b = k.snapshot(id);
        assert_eq!(b.status, BountyStatus::Draft);
        assert!(b.published_at.is_none());
        assert!(b.assignment_expires_at.is_none());
    }
    assert_eq!(
        serde_json::to_value(k.repo.get_bounty_detail(&first.id).unwrap()).unwrap(),
        before
    );
    assert!(k.inbox(&a.agent.id, "task.published").is_empty());
    conn.execute_batch("DROP TRIGGER fail_dispatch;").unwrap();
    k.repo.publish_bounty(&second.id, "human").unwrap();
    error(
        k.repo
            .publish_bounties_batch(dto(json!({"bounty_ids":[first.id,second.id]})), "human"),
        StatusCode::CONFLICT,
        "INVALID_TRANSITION",
    );
    assert_eq!(k.snapshot(&first.id).status, BountyStatus::Draft);
    k.repo
        .update_project(&k.project.id, dto(json!({"archived":true})))
        .unwrap();
    error(
        k.repo
            .publish_bounties_batch(dto(json!({"bounty_ids":[first.id]})), "human"),
        StatusCode::CONFLICT,
        "PROJECT_ARCHIVED",
    );
}

#[test]
fn fan_in_and_chain_unlock_once_through_direct_and_arbitrated_completion() {
    let k = Kernel::new();
    let worker = k.agent("worker");
    let judge = k.agent("judge");
    k.repo
        .arbitration_config(Some(ArbitrationConfig {
            arbitrator_chain: vec![judge.agent.id.clone()],
        }))
        .unwrap();
    let a = k.bounty(json!({"status":"OPEN"}));
    let b = k.bounty(json!({"status":"OPEN","require_arbitration":true}));
    let c = k.bounty(json!({"status":"OPEN","blocked_by":[a.id,b.id]}));
    let d=k.bounty(json!({"status":"OPEN","blocked_by":[c.id],"assignee_mode":"specific","assignee_agent_id":worker.agent.id}));
    k.complete(&a.id, &worker.agent.id);
    assert!(k.snapshot(&c.id).is_blocked);
    let claimed = k.repo.agent_claim_bounty(&b.id, &worker.agent.id).unwrap();
    k.repo
        .agent_submit_arbitration(
            &b.id,
            &worker.agent.id,
            dto(json!({"lease_id":claimed.lease_id})),
        )
        .unwrap();
    assert!(k.snapshot(&c.id).is_blocked);
    k.repo
        .approve_arbitration(
            &b.id,
            Some(&judge.agent.id),
            ApproveArbitrationInput::default(),
        )
        .unwrap();
    assert!(!k.snapshot(&c.id).is_blocked);
    assert!(k.snapshot(&d.id).is_blocked);
    let notices = k.inbox(&worker.agent.id, "task.published");
    let notices: Vec<_> = notices
        .iter()
        .filter(|e| e.bounty_id.as_deref() == Some(c.id.as_str()))
        .collect();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].data["is_blocked"], false);
    k.complete(&c.id, &worker.agent.id);
    let d = k.snapshot(&d.id);
    assert!(!d.is_blocked);
    assert!(d.assignment_expires_at.is_some());
    let notices = k.inbox(&worker.agent.id, "task.assigned");
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].data["id"], d.id);
    assert_eq!(notices[0].data["is_blocked"], false);
    for (id, unlocked_by) in [(&c.id, &b.id), (&d.id, &c.id)] {
        let detail = k.repo.get_bounty_detail(id).unwrap();
        let events: Vec<_> = detail
            .events
            .iter()
            .filter(|e| e.event_type == "dependency_unlocked")
            .collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].actor, "system");
        assert_eq!(events[0].from_status, Some(BountyStatus::Open));
        assert_eq!(events[0].to_status, BountyStatus::Open);
        assert_eq!(events[0].detail["unlocked_by_bounty_id"], *unlocked_by);
    }
    error(
        k.repo
            .approve_arbitration(&b.id, None, ApproveArbitrationInput::default()),
        StatusCode::CONFLICT,
        "INVALID_TRANSITION",
    );
    assert_eq!(k.inbox(&worker.agent.id, "task.assigned").len(), 1);
    k.repo.agent_claim_bounty(&d.id, &worker.agent.id).unwrap();
}

#[test]
fn unlock_notification_failure_rolls_back_completion_rating_and_all_downstream_changes() {
    for arbitrated in [false, true] {
        let k = Kernel::new();
        let worker = k.agent("worker");
        let p = k.bounty(json!({"status":"OPEN","require_arbitration":arbitrated}));
        let first = k.bounty(json!({"status":"OPEN","blocked_by":[p.id]}));
        let second=k.bounty(json!({"status":"OPEN","blocked_by":[p.id],"assignee_mode":"specific","assignee_agent_id":worker.agent.id}));
        let claimed = k.repo.agent_claim_bounty(&p.id, &worker.agent.id).unwrap();
        if arbitrated {
            k.repo
                .agent_submit_arbitration(
                    &p.id,
                    &worker.agent.id,
                    dto(json!({"lease_id":claimed.lease_id})),
                )
                .unwrap();
        }
        let snapshots: Vec<_> = [&p.id, &first.id, &second.id]
            .into_iter()
            .map(|id| serde_json::to_value(k.repo.get_bounty_detail(id).unwrap()).unwrap())
            .collect();
        let rating =
            serde_json::to_value(k.repo.get_agent_detail(&worker.agent.id).unwrap()).unwrap();
        let inbox =
            serde_json::to_value(k.repo.agent_events(&worker.agent.id, Some(0), 500).unwrap())
                .unwrap();
        let conn = k.conn();
        conn.execute_batch("CREATE TRIGGER fail_unlock BEFORE INSERT ON agent_events WHEN NEW.event_type='task.assigned' BEGIN SELECT RAISE(ABORT,'unlock'); END;").unwrap();
        let result = if arbitrated {
            k.repo
                .approve_arbitration(&p.id, None, ApproveArbitrationInput::default())
        } else {
            k.repo.agent_complete_bounty(
                &p.id,
                &worker.agent.id,
                dto(json!({"lease_id":claimed.lease_id})),
            )
        };
        error(result, StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR");
        for (id, before) in [&p.id, &first.id, &second.id].into_iter().zip(snapshots) {
            assert_eq!(
                serde_json::to_value(k.repo.get_bounty_detail(id).unwrap()).unwrap(),
                before
            );
        }
        assert_eq!(
            serde_json::to_value(k.repo.get_agent_detail(&worker.agent.id).unwrap()).unwrap(),
            rating
        );
        assert_eq!(
            serde_json::to_value(k.repo.agent_events(&worker.agent.id, Some(0), 500).unwrap())
                .unwrap(),
            inbox
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM bounty_dispatches WHERE bounty_id IN (?1,?2)",
                params![first.id, second.id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        conn.execute_batch("DROP TRIGGER fail_unlock;").unwrap();
        if arbitrated {
            k.repo
                .approve_arbitration(&p.id, None, ApproveArbitrationInput::default())
                .unwrap();
        } else {
            k.repo
                .agent_complete_bounty(
                    &p.id,
                    &worker.agent.id,
                    dto(json!({"lease_id":claimed.lease_id})),
                )
                .unwrap();
        }
        assert!(!k.snapshot(&first.id).is_blocked);
        assert!(!k.snapshot(&second.id).is_blocked);
    }
}

#[test]
fn agent_snapshots_and_notifications_redact_hidden_draft_titles_and_filter_relations() {
    let k = Kernel::new();
    let a = k.agent("reader");
    let other = k.agent("other");
    let hidden = k.bounty(json!({"title":"confidential draft title"}));
    let visible = k.bounty(json!({"status":"OPEN","blocked_by":[hidden.id]}));
    assert_eq!(
        k.repo
            .get_bounty_detail(&visible.id)
            .unwrap()
            .bounty
            .blocked_by_summaries[0]
            .title,
        hidden.title
    );
    assert_eq!(
        k.repo.list_bounties(dto(json!({}))).unwrap()[0].blocked_by_summaries[0].title,
        hidden.title
    );
    let detail = k
        .repo
        .agent_bounty_detail(&a.agent.id, &visible.id)
        .unwrap();
    assert_eq!(
        detail.bounty.blocked_by_summaries[0].title,
        "[草稿前置任务]"
    );
    assert!(detail.bounty.is_blocked);
    assert_eq!(
        detail.bounty.blocked_by_summaries[0].status,
        BountyStatus::Draft
    );
    assert_eq!(
        k.repo
            .agent_bounties(&a.agent.id, BountyFilter::default())
            .unwrap()[0]
            .blocked_by_summaries[0]
            .title,
        "[草稿前置任务]"
    );
    k.repo
        .update_project(&k.project.id, dto(json!({"agent_draft_policy":"open"})))
        .unwrap();
    let own=k.repo.agent_create_bounty(&a.agent.id,dto(json!({"project_id":k.project.id,"title":"own","difficulty":"C","assignee_mode":"any"}))).unwrap();
    // 管理员可在草稿中引用不可见前置，随后发布；评论通知必须逐接收者脱敏。
    k.repo.withdraw_bounty(&own.id, "human").unwrap();
    k.repo
        .update_bounty(&own.id, dto(json!({"blocked_by":[hidden.id]})), "human")
        .unwrap();
    k.repo.publish_bounty(&own.id, "human").unwrap();
    k.repo
        .create_bounty_comment(&own.id, dto(json!({"body":"check dependencies"})))
        .unwrap();
    let notice = k.inbox(&a.agent.id, "comment.added").pop().unwrap();
    assert_eq!(
        notice.data["bounty"]["blocked_by_summaries"][0]["title"],
        "[草稿前置任务]"
    );
    let parent = k.bounty(json!({"blocked_by":[hidden.id]}));
    k.repo
        .invite_draft_collaborator(&parent.id, dto(json!({"agent_id":a.agent.id})))
        .unwrap();
    let invite = k.inbox(&a.agent.id, "draft.invited").pop().unwrap();
    assert_eq!(
        invite.data["bounty"]["blocked_by_summaries"][0]["title"],
        "[草稿前置任务]"
    );
    let split = k
        .repo
        .agent_split_draft(&parent.id, &a.agent.id, dto(split_items()))
        .unwrap();
    k.repo
        .invite_draft_collaborator(
            &split.children[0].id,
            dto(json!({"agent_id":other.agent.id})),
        )
        .unwrap();
    assert_eq!(
        k.repo
            .get_bounty_detail(&parent.id)
            .unwrap()
            .child_bounties
            .len(),
        2
    );
    assert_eq!(
        k.repo
            .agent_bounty_detail(&a.agent.id, &parent.id)
            .unwrap()
            .child_bounties
            .len(),
        1
    );
    assert_eq!(
        k.repo
            .get_bounty_detail(&split.children[1].id)
            .unwrap()
            .blocks
            .len(),
        1
    );
    assert!(k
        .repo
        .agent_bounty_detail(&a.agent.id, &split.children[1].id)
        .unwrap()
        .blocks
        .is_empty());
    k.repo
        .invite_draft_collaborator(&hidden.id, dto(json!({"agent_id":a.agent.id})))
        .unwrap();
    assert_eq!(
        k.repo
            .agent_bounty_detail(&a.agent.id, &visible.id)
            .unwrap()
            .bounty
            .blocked_by_summaries[0]
            .title,
        hidden.title
    );
    k.repo
        .create_bounty_comment(&own.id, dto(json!({"body":"now visible"})))
        .unwrap();
    assert_eq!(
        k.inbox(&a.agent.id, "comment.added").last().unwrap().data["bounty"]
            ["blocked_by_summaries"][0]["title"],
        hidden.title
    );
    let cancelled = k.repo.cancel_bounty(&parent.id, "human").unwrap();
    assert!(!cancelled.draft_needs_human_review);
    let other_draft = k.bounty(json!({}));
    k.repo
        .invite_draft_collaborator(&other_draft.id, dto(json!({"agent_id":a.agent.id})))
        .unwrap();
    k.repo
        .agent_update_draft(
            &other_draft.id,
            &a.agent.id,
            AgentUpdateDraftInput::default(),
        )
        .unwrap();
    assert!(
        !k.repo
            .publish_bounty(&other_draft.id, "human")
            .unwrap()
            .draft_needs_human_review
    );
    assert!(
        !k.repo
            .withdraw_bounty(&other_draft.id, "human")
            .unwrap()
            .draft_needs_human_review
    );
}

#[tokio::test]
async fn related_summaries_follow_visibility_in_repository_and_http() {
    let k = Kernel::new();
    let reader = k.agent("reader");
    let entitled = k.agent("entitled");
    let prerequisite = k.bounty(json!({"status":"OPEN"}));
    let specific = k.bounty(json!({"status":"OPEN","assignee_mode":"specific","assignee_agent_id":entitled.agent.id,"blocked_by":[prerequisite.id]}));
    let draft = k.bounty(json!({"title":"private downstream","blocked_by":[prerequisite.id]}));
    k.repo
        .invite_draft_collaborator(&draft.id, dto(json!({"agent_id":entitled.agent.id})))
        .unwrap();
    // 同一组关联同时覆盖下游与派生摘要，验证两者遵守相同的访问权限。
    for id in [&specific.id, &draft.id] {
        k.conn()
            .execute(
                "UPDATE bounties SET parent_bounty_id=?2 WHERE id=?1",
                params![id, prerequisite.id],
            )
            .unwrap();
    }
    let running = server::start(server::ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        db_path: k.repo.path().to_owned(),
        admin_token: "dependencies-admin".into(),
        mode: server::ServerMode::Standalone,
    })
    .await
    .unwrap();
    let client = reqwest::Client::new();
    for cancelled in [false, true] {
        if cancelled {
            let result = http(
                &client,
                &running.info.base_url,
                reqwest::Method::POST,
                &format!("/api/v1/bounties/{}/cancel", draft.id),
                Some("dependencies-admin"),
                None,
            )
            .await;
            assert_eq!(result.0, StatusCode::OK);
            assert_eq!(result.1["status"], "CANCELLED");
            assert!(result.1["published_at"].is_null());
        }
        for (agent, expected) in [
            (&reader, vec![]),
            (
                &entitled,
                if cancelled {
                    vec![specific.id.clone()]
                } else {
                    vec![specific.id.clone(), draft.id.clone()]
                },
            ),
        ] {
            let detail = k
                .repo
                .agent_bounty_detail(&agent.agent.id, &prerequisite.id)
                .unwrap();
            let (status, response) = http(
                &client,
                &running.info.base_url,
                reqwest::Method::GET,
                &format!("/api/v1/agent/bounties/{}", prerequisite.id),
                Some(&agent.api_key),
                None,
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            for summaries in [&detail.blocks, &detail.child_bounties] {
                let mut ids: Vec<_> = summaries.iter().map(|s| s.id.clone()).collect();
                let mut expected = expected.clone();
                ids.sort();
                expected.sort();
                assert_eq!(ids, expected);
            }
            assert_eq!(response["blocks"], json!(detail.blocks));
            assert_eq!(response["child_bounties"], json!(detail.child_bounties));
        }
        let admin = k.repo.get_bounty_detail(&prerequisite.id).unwrap();
        let (status, response) = http(
            &client,
            &running.info.base_url,
            reqwest::Method::GET,
            &format!("/api/v1/bounties/{}", prerequisite.id),
            Some("dependencies-admin"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(admin.blocks.len(), 2);
        assert_eq!(admin.child_bounties.len(), 2);
        assert_eq!(response["blocks"], json!(admin.blocks));
        assert_eq!(response["child_bounties"], json!(admin.child_bounties));
        for hidden in [&specific.id, &draft.id] {
            error(
                k.repo.agent_bounty_detail(&reader.agent.id, hidden),
                StatusCode::NOT_FOUND,
                "BOUNTY_NOT_FOUND",
            );
        }
    }
    running.shutdown().await.unwrap();
}

#[tokio::test]
async fn invisible_prerequisites_redact_snapshots_events_and_probes_in_repository_and_http() {
    let k = Kernel::new();
    let reader = k.agent("reader");
    let other = k.agent("other");
    let hidden = k.bounty(json!({"title":"unpublished secret"}));
    let specific = k.bounty(json!({"title":"specific secret","status":"OPEN","assignee_mode":"specific","assignee_agent_id":other.agent.id}));
    let unsubscribed_project = k
        .repo
        .create_project(dto(json!({"name":"private project"})))
        .unwrap();
    let unsubscribed = k.bounty(
        json!({"title":"unsubscribed secret","project_id":unsubscribed_project.id,"status":"OPEN"}),
    );
    k.repo
        .update_agent_subscriptions(
            &reader.agent.id,
            dto(json!({"subscribe_all_projects":false,"project_ids":[k.project.id]})),
        )
        .unwrap();
    let running = server::start(server::ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        db_path: k.repo.path().to_owned(),
        admin_token: "dependencies-admin".into(),
        mode: server::ServerMode::Standalone,
    })
    .await
    .unwrap();
    let client = reqwest::Client::new();
    for prereq in [&hidden, &specific, &unsubscribed] {
        // 先由管理员建立依赖，再取消未发布草稿，保留合法的既有依赖边。
        let downstream = k.bounty(json!({"blocked_by":[prereq.id]}));
        if prereq.id == hidden.id {
            let cancelled = k.repo.cancel_bounty(&hidden.id, "human").unwrap();
            assert_eq!(cancelled.status, BountyStatus::Cancelled);
            assert!(cancelled.published_at.is_none());
        }
        let expected = json!([{"id":prereq.id,"title":"[草稿前置任务]","status":k.snapshot(&prereq.id).status}]);
        k.repo
            .invite_draft_collaborator(&downstream.id, dto(json!({"agent_id":reader.agent.id})))
            .unwrap();
        k.repo
            .create_bounty_comment(&downstream.id, dto(json!({"body":"review prerequisites"})))
            .unwrap();
        let list = k
            .repo
            .agent_bounties(&reader.agent.id, BountyFilter::default())
            .unwrap();
        let listed = list.iter().find(|b| b.id == downstream.id).unwrap();
        assert_eq!(json!(listed.blocked_by_summaries), expected);
        let detail = k
            .repo
            .agent_bounty_detail(&reader.agent.id, &downstream.id)
            .unwrap();
        assert_eq!(json!(detail.bounty.blocked_by_summaries), expected);
        assert!(detail.bounty.is_blocked);
        for kind in ["draft.invited", "comment.added"] {
            let events = k.inbox(&reader.agent.id, kind);
            let event = events
                .iter()
                .find(|e| e.bounty_id.as_deref() == Some(downstream.id.as_str()))
                .unwrap();
            assert_eq!(event.data["bounty"]["blocked_by_summaries"], expected);
        }
        for path in [
            format!("/api/v1/agent/bounties/{}", downstream.id),
            "/api/v1/agent/bounties?include_drafts=true".into(),
        ] {
            let (status, value) = http(
                &client,
                &running.info.base_url,
                reqwest::Method::GET,
                &path,
                Some(&reader.api_key),
                None,
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            let bounty = if let Some(list) = value.as_array() {
                list.iter().find(|b| b["id"] == downstream.id).unwrap()
            } else {
                &value["bounty"]
            };
            assert_eq!(bounty["blocked_by_summaries"], expected);
        }
        let (status, inbox) = http(
            &client,
            &running.info.base_url,
            reqwest::Method::GET,
            "/api/v1/agent/events?cursor=0&limit=500",
            Some(&reader.api_key),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        for kind in ["draft.invited", "comment.added"] {
            let event = inbox["events"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["bounty_id"] == downstream.id && e["event_type"] == kind)
                .unwrap();
            assert_eq!(event["data"]["bounty"]["blocked_by_summaries"], expected);
        }
        assert_eq!(
            k.repo
                .get_bounty_detail(&downstream.id)
                .unwrap()
                .bounty
                .blocked_by_summaries[0]
                .title,
            prereq.title
        );
        let (status, admin) = http(
            &client,
            &running.info.base_url,
            reqwest::Method::GET,
            &format!("/api/v1/bounties/{}", downstream.id),
            Some("dependencies-admin"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            admin["bounty"]["blocked_by_summaries"][0]["title"],
            prereq.title
        );
        error(
            k.repo.agent_bounty_detail(&reader.agent.id, &prereq.id),
            StatusCode::NOT_FOUND,
            "BOUNTY_NOT_FOUND",
        );
        error(
            k.repo.agent_update_draft(
                &downstream.id,
                &reader.agent.id,
                dto(json!({"blocked_by":[prereq.id]})),
            ),
            StatusCode::NOT_FOUND,
            "BOUNTY_NOT_FOUND",
        );
        for (method, path, body) in [
            (
                reqwest::Method::GET,
                format!("/api/v1/agent/bounties/{}", prereq.id),
                None,
            ),
            (
                reqwest::Method::PATCH,
                format!("/api/v1/agent/bounties/{}", downstream.id),
                Some(json!({"blocked_by":[prereq.id]})),
            ),
            (
                reqwest::Method::POST,
                format!("/api/v1/agent/bounties/{}/split", downstream.id),
                Some(
                    json!({"items":[{"title":"probe","difficulty":"C","assignee_mode":"any","blocked_by":[prereq.id]}]}),
                ),
            ),
            (
                reqwest::Method::POST,
                "/api/v1/agent/bounties".into(),
                Some(
                    json!({"project_id":k.project.id,"title":"probe","difficulty":"C","assignee_mode":"any","blocked_by":[prereq.id]}),
                ),
            ),
        ] {
            let (status, value) = http(
                &client,
                &running.info.base_url,
                method,
                &path,
                Some(&reader.api_key),
                body,
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(value["error"]["code"], "BOUNTY_NOT_FOUND");
        }
        // 已发布且当前可见的取消任务仍返回状态冲突；未发布取消草稿对管理员也保留状态冲突。
        if prereq.id != hidden.id {
            k.repo.cancel_bounty(&prereq.id, "human").unwrap();
        }
        error(
            k.repo.update_bounty(
                &downstream.id,
                dto(json!({"blocked_by":[prereq.id]})),
                "human",
            ),
            StatusCode::CONFLICT,
            "INVALID_TRANSITION",
        );
        if prereq.id != hidden.id {
            k.repo
                .invite_draft_collaborator(&downstream.id, dto(json!({"agent_id":other.agent.id})))
                .unwrap();
            error(
                k.repo.agent_update_draft(
                    &downstream.id,
                    &other.agent.id,
                    dto(json!({"blocked_by":[prereq.id]})),
                ),
                StatusCode::CONFLICT,
                "INVALID_TRANSITION",
            );
        }
    }
    running.shutdown().await.unwrap();
}

#[test]
fn inactive_notification_recipients_do_not_abort_dependency_redaction() {
    let k = Kernel::new();
    let hidden = k.bounty(json!({}));
    for status in [AgentStatus::Disabled, AgentStatus::Revoked] {
        let agent = k.agent(&format!("inactive-{status:?}"));
        let draft = k.bounty(json!({"blocked_by":[hidden.id]}));
        k.repo
            .invite_draft_collaborator(&draft.id, dto(json!({"agent_id":agent.agent.id})))
            .unwrap();
        k.repo.set_agent_status(&agent.agent.id, status).unwrap();
        let result = k
            .repo
            .create_bounty_comment(&draft.id, dto(json!({"body":"review while inactive"})))
            .unwrap();
        assert_eq!(result.comment.body, "review while inactive");
        assert_eq!(k.repo.bounty_comments(&draft.id).unwrap().len(), 1);
        let count: i64 = k.conn().query_row("SELECT COUNT(*) FROM agent_events WHERE agent_id=?1 AND event_type='comment.added'", [&agent.agent.id], |r| r.get(0)).unwrap();
        assert_eq!(count, 0);
    }
}

async fn http(
    client: &reqwest::Client,
    base: &str,
    method: reqwest::Method,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = client.request(method, format!("{base}{path}"));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    if let Some(body) = body {
        request = request
            .header("Content-Type", "application/json")
            .body(body.to_string());
    }
    let response = request.send().await.unwrap();
    let status = response.status();
    let value = serde_json::from_str(&response.text().await.unwrap()).unwrap();
    (status, value)
}

#[tokio::test]
async fn http_contracts_auth_draft_isolation_validation_and_skill_documentation() {
    use reqwest::Method;
    let k = Kernel::new();
    let creator = k.agent("creator");
    let collaborator = k.agent("collaborator");
    k.repo
        .update_project(&k.project.id, dto(json!({"agent_draft_policy":"draft"})))
        .unwrap();
    let draft=k.repo.agent_create_bounty(&creator.agent.id,dto(json!({"project_id":k.project.id,"title":"secret","difficulty":"C","assignee_mode":"any"}))).unwrap();
    let running = server::start(server::ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        db_path: k.repo.path().to_owned(),
        admin_token: "dependencies-admin".into(),
        mode: server::ServerMode::Standalone,
    })
    .await
    .unwrap();
    let client = reqwest::Client::new();
    let base = &running.info.base_url;
    let invite = format!("/api/v1/bounties/{}/collaborator", draft.id);
    let detail = format!("/api/v1/agent/bounties/{}", draft.id);
    let split = format!("{detail}/split");
    let comments = format!("{detail}/comments");
    for token in [None, Some(creator.api_key.as_str())] {
        assert_eq!(
            http(
                &client,
                base,
                Method::POST,
                &invite,
                token,
                Some(json!({"agent_id":collaborator.agent.id}))
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            http(
                &client,
                base,
                Method::POST,
                "/api/v1/bounties/publish-batch",
                token,
                Some(json!({"bounty_ids":[draft.id]}))
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
    }
    let admin = Some("dependencies-admin");
    assert_eq!(
        http(
            &client,
            base,
            Method::POST,
            &invite,
            admin,
            Some(json!({"agent_id":collaborator.agent.id,"extra":true}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        http(
            &client,
            base,
            Method::POST,
            &invite,
            admin,
            Some(json!({"agent_id":collaborator.agent.id}))
        )
        .await
        .0,
        StatusCode::OK
    );
    for suffix in [
        "",
        "?status=DRAFT",
        "?include_drafts=true",
        "?status=DRAFT&include_drafts=true&q=secret",
    ] {
        let path = format!("/api/v1/agent/bounties{suffix}");
        let (_, hidden) = http(
            &client,
            base,
            Method::GET,
            &path,
            Some(&creator.api_key),
            None,
        )
        .await;
        assert_eq!(hidden, json!([]));
        let (status, visible) = http(
            &client,
            base,
            Method::GET,
            &path,
            Some(&collaborator.api_key),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(visible.as_array().unwrap().len(), 1);
    }
    for (method, path, body) in [
        (Method::GET, detail.clone(), None),
        (Method::GET, comments.clone(), None),
        (
            Method::POST,
            comments.clone(),
            Some(json!({"body":"guess"})),
        ),
        (
            Method::PATCH,
            detail.clone(),
            Some(json!({"title":"guess"})),
        ),
        (Method::POST, split.clone(), Some(split_items())),
    ] {
        let (status, value) =
            http(&client, base, method, &path, Some(&creator.api_key), body).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(value["error"]["code"], "BOUNTY_NOT_FOUND");
    }
    let auth = Some(collaborator.api_key.as_str());
    assert_eq!(
        http(&client, base, Method::GET, &detail, auth, None)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        http(
            &client,
            base,
            Method::PATCH,
            &detail,
            admin,
            Some(json!({"title":"guess"}))
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    for body in [
        json!({"status":"OPEN"}),
        json!({"draft_needs_human_review":false}),
        json!({"blocked_by":"invalid"}),
        json!({"title":"  "}),
    ] {
        let (status, error) = http(&client, base, Method::PATCH, &detail, auth, Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error["error"]["code"], "VALIDATION_ERROR");
    }
    let (status, revised) = http(
        &client,
        base,
        Method::PATCH,
        &detail,
        auth,
        Some(json!({"title":"revised","description":"body","acceptance_criteria":"pass"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(revised["draft_needs_human_review"], true);
    let (status, comment) = http(
        &client,
        base,
        Method::POST,
        &comments,
        auth,
        Some(json!({"body":"ready to split"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(comment["bounty"]["active_duration_ms"], 0);
    let (status, result) = http(
        &client,
        base,
        Method::POST,
        &split,
        auth,
        Some(split_items()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let ids: Vec<_> = result["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].clone())
        .collect();
    for invalid in [
        json!({"items":[],"extra":true}),
        json!({"items":[{"title":"child","difficulty":"C","assignee_mode":"any","extra":true}]}),
    ] {
        assert_eq!(
            http(&client, base, Method::POST, &split, auth, Some(invalid))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        http(
            &client,
            base,
            Method::POST,
            "/api/v1/bounties/publish-batch",
            admin,
            Some(json!({"bounty_ids":ids,"extra":true}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (status, published) = http(
        &client,
        base,
        Method::POST,
        "/api/v1/bounties/publish-batch",
        admin,
        Some(json!({"bounty_ids":ids})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(published[0]["is_blocked"], true);
    assert_eq!(published[1]["is_blocked"], false);
    assert_eq!(
        http(
            &client,
            base,
            Method::PATCH,
            &format!("/api/v1/agent/bounties/{}", ids[0].as_str().unwrap()),
            auth,
            Some(json!({"title":"late"}))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let skill = client
        .get(format!("{base}/skill.md"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    for text in [
        "GET /api/v1/agent/bounties/{id}",
        "POST /api/v1/agent/bounties/{id}/comments",
        "PATCH /api/v1/agent/bounties/{id}",
        "POST /api/v1/agent/bounties/{id}/split",
        "draft.invited",
        "comment.added",
        "blocked_by",
        "key",
        "DAG",
        "409 DEPENDENCY_CYCLE",
        "409 BOUNTY_BLOCKED",
        "task.published",
        "task.assigned",
        "draft_needs_human_review",
    ] {
        assert!(skill.contains(text), "missing {text}");
    }
    for secret in [
        &creator.api_key,
        &collaborator.api_key,
        &"dependencies-admin".to_owned(),
    ] {
        assert!(!skill.contains(secret));
    }
    for value in [
        revised,
        result,
        published,
        serde_json::to_value(
            k.repo
                .agent_events(&collaborator.agent.id, Some(0), 500)
                .unwrap(),
        )
        .unwrap(),
    ] {
        let encoded = value.to_string();
        for field in [
            "\"mmr\"",
            "\"api_key\"",
            "\"api_key_hash\"",
            "\"webhook_secret\"",
        ] {
            assert!(!encoded.contains(field));
        }
    }
    running.shutdown().await.unwrap();
}

#[test]
fn concurrent_dependency_edits_cannot_commit_a_cycle() {
    let k = Kernel::new();
    let a = k.bounty(json!({}));
    let b = k.bounty(json!({}));
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let threads: Vec<_> = [(a.id.clone(), b.id.clone()), (b.id.clone(), a.id.clone())]
        .into_iter()
        .map(|(id, dependency)| {
            let repo = k.repo.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                repo.update_bounty(&id, dto(json!({"blocked_by":[dependency]})), "human")
            })
        })
        .collect();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results.into_iter().find_map(|r| r.err()).unwrap().code,
        "DEPENDENCY_CYCLE"
    );
    assert_eq!(
        k.conn()
            .query_row("SELECT COUNT(*) FROM bounty_dependencies", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn agent_creation_and_single_publish_respect_pending_and_completed_dependencies() {
    let k = Kernel::new();
    let a = k.agent("publisher");
    let prerequisite = k.bounty(json!({"status":"OPEN"}));
    let input = json!({"project_id":k.project.id,"title":"agent dependent","difficulty":"C","assignee_mode":"specific","assignee_agent_id":a.agent.id,"blocked_by":[prerequisite.id]});
    let b = k.repo.agent_create_bounty(&a.agent.id, dto(input)).unwrap();
    assert!(b.is_blocked);
    assert_eq!(b.blocked_by, vec![prerequisite.id.clone()]);
    assert!(b.assignment_expires_at.is_none());
    assert!(k.inbox(&a.agent.id, "task.assigned").is_empty());
    assert_eq!(
        k.conn()
            .query_row(
                "SELECT COUNT(*) FROM bounty_dispatches WHERE bounty_id=?1",
                [&b.id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    k.complete(&prerequisite.id, &a.agent.id);
    assert_eq!(k.inbox(&a.agent.id, "task.assigned").len(), 1);
    let draft=k.bounty(json!({"blocked_by":[prerequisite.id],"assignee_mode":"specific","assignee_agent_id":a.agent.id}));
    assert!(!draft.is_blocked);
    let published = k.repo.publish_bounty(&draft.id, "human").unwrap();
    assert!(!published.is_blocked);
    assert!(published.assignment_expires_at.is_some());
    assert_eq!(k.inbox(&a.agent.id, "task.assigned").len(), 2);
    let invited = k.bounty(json!({}));
    k.repo
        .invite_draft_collaborator(&invited.id, dto(json!({"agent_id":a.agent.id})))
        .unwrap();
    let b=k.repo.agent_create_bounty(&a.agent.id,dto(json!({"project_id":k.project.id,"title":"visible draft prerequisite","difficulty":"C","assignee_mode":"any","blocked_by":[invited.id,invited.id]}))).unwrap();
    assert!(b.is_blocked);
    assert_eq!(b.blocked_by, vec![invited.id]);
    assert_eq!(b.blocked_by_summaries[0].title, invited.title);
}

#[test]
fn invitation_and_draft_comment_notification_failures_restore_review_and_history() {
    let k = Kernel::new();
    let a = k.agent("collaborator");
    let b = k.bounty(json!({}));
    let before = serde_json::to_value(k.repo.get_bounty_detail(&b.id).unwrap()).unwrap();
    let conn = k.conn();
    conn.execute_batch("CREATE TRIGGER fail_invite BEFORE INSERT ON agent_events WHEN NEW.event_type='draft.invited' BEGIN SELECT RAISE(ABORT,'invite'); END;").unwrap();
    error(
        k.repo
            .invite_draft_collaborator(&b.id, dto(json!({"agent_id":a.agent.id}))),
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    );
    assert_eq!(
        serde_json::to_value(k.repo.get_bounty_detail(&b.id).unwrap()).unwrap(),
        before
    );
    conn.execute_batch("DROP TRIGGER fail_invite;").unwrap();
    k.repo
        .invite_draft_collaborator(&b.id, dto(json!({"agent_id":a.agent.id})))
        .unwrap();
    k.repo
        .agent_create_comment(&b.id, &a.agent.id, dto(json!({"body":"needs review"})))
        .unwrap();
    let before = serde_json::to_value(k.repo.get_bounty_detail(&b.id).unwrap()).unwrap();
    let inbox =
        serde_json::to_value(k.repo.agent_events(&a.agent.id, Some(0), 500).unwrap()).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_comment BEFORE INSERT ON agent_events WHEN NEW.event_type='comment.added' BEGIN SELECT RAISE(ABORT,'comment'); END;").unwrap();
    error(
        k.repo
            .create_bounty_comment(&b.id, dto(json!({"body":"reviewed"}))),
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    );
    assert_eq!(
        serde_json::to_value(k.repo.get_bounty_detail(&b.id).unwrap()).unwrap(),
        before
    );
    assert_eq!(
        serde_json::to_value(k.repo.agent_events(&a.agent.id, Some(0), 500).unwrap()).unwrap(),
        inbox
    );
}
