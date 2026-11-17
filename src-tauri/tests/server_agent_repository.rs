#![cfg(not(target_os = "android"))]
//! 文件职责：验证 v1 迁移、凭证只存摘要及邀请次数的跨连接原子性。
use rusqlite::Connection;
use serde_json::json;
use sha2::{Digest, Sha256};
use taskrail_lib::server::{db::Repository, models::*};

fn register(name: &str, token: Option<&str>) -> RegisterAgent {
    serde_json::from_value(json!({"name":name,"invite_token":token})).unwrap()
}
fn create_v1(path: &std::path::Path) -> Connection {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(include_str!("fixtures/server_v1.sql"))
        .unwrap();
    conn.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL); INSERT INTO schema_migrations VALUES(1,'v1'); INSERT INTO projects(id,name,created_at,updated_at) VALUES('project','existing','old','old'); INSERT INTO bounties(id,project_id,title,difficulty,assignee_mode,status,created_at,updated_at) VALUES('bounty','project','keep me','C','any','DRAFT','old','old'); INSERT INTO bounty_events(bounty_id,event_type,actor,to_status,created_at) VALUES('bounty','created','human','DRAFT','old');").unwrap();
    conn
}

#[test]
fn v1_upgrade_preserves_projects_bounties_and_events_and_rolls_back_failure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v1.db");
    let conn = create_v1(&path);
    let repo = Repository::open(&path).unwrap();
    assert_eq!(repo.get_project("project").unwrap().name, "existing");
    let detail = repo.get_bounty_detail("bounty").unwrap();
    assert_eq!(detail.bounty.title, "keep me");
    assert_eq!(detail.events.len(), 1);
    let fresh = Connection::open(dir.path().join("fresh.db")).unwrap();
    // 对比实际新建与升级后的表、索引定义，防止两条迁移路径随维护发生漂移。
    drop(Repository::open(dir.path().join("fresh.db")).unwrap());
    let schema = |conn: &Connection| {
        let mut stmt = conn.prepare("SELECT name,sql FROM sqlite_master WHERE type IN ('table','index') AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap();
        stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
    };
    // schema_migrations 的定义由既有 v1 建库决定，只比较业务对象。
    let business = |conn: &Connection| {
        schema(conn)
            .into_iter()
            .filter(|(name, _)| name != "schema_migrations")
            .collect::<Vec<_>>()
    };
    assert_eq!(business(&fresh), business(&conn));
    fresh.close().unwrap();
    assert!(!repo.onboarding(None).unwrap().lan_open_registration);
    assert_eq!(
        conn.query_row(
            "SELECT group_concat(version) FROM schema_migrations",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "1,2,3,4,5,6,7,8"
    );
    repo.onboarding(Some(UpdateOnboarding {
        lan_open_registration: true,
    }))
    .unwrap();
    let repo = Repository::open(&path).unwrap();
    assert!(repo.onboarding(None).unwrap().lan_open_registration);

    let broken_path = dir.path().join("conflict.db");
    let broken = create_v1(&broken_path);
    broken
        .execute("CREATE TABLE agents(conflict TEXT)", [])
        .unwrap();
    assert!(Repository::open(&broken_path).is_err());
    assert_eq!(
        broken
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        broken
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='server_settings'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        broken
            .query_row("SELECT title FROM bounties", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "keep me"
    );
}

#[test]
fn registration_persists_only_hashes_and_failed_registration_does_not_consume_invites() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agents.db");
    let repo = Repository::open(&path).unwrap();
    let created = repo
        .create_invite_token(CreateInviteToken {
            max_uses: Some(3),
            ..Default::default()
        })
        .unwrap();
    let registered = repo
        .register_agent(register(" worker ", Some(&created.raw_token)))
        .unwrap();
    assert!(registered.api_key.starts_with("tr_ag_"));
    assert!(created.raw_token.starts_with("tr_inv_"));
    let conn = Connection::open(&path).unwrap();
    let (hash, tier, mmr): (String, String, f64) = conn
        .query_row("SELECT api_key_hash,tier,mmr FROM agents", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!(
        hash,
        format!("{:x}", Sha256::digest(registered.api_key.as_bytes()))
    );
    assert_eq!(tier, "Unranked");
    assert_eq!(mmr, 1000.0);
    let invite_hash: String = conn
        .query_row("SELECT token_hash FROM invite_tokens", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        invite_hash,
        format!("{:x}", Sha256::digest(created.raw_token.as_bytes()))
    );
    assert_eq!(
        repo.register_agent(register("worker", Some(&created.raw_token)))
            .err()
            .unwrap()
            .code,
        "AGENT_NAME_TAKEN"
    );
    let input = serde_json::from_value(json!({"name":"invalid-project","invite_token":created.raw_token,"project_ids":["missing"]})).unwrap();
    assert_eq!(
        repo.register_agent(input).err().unwrap().code,
        "PROJECT_NOT_FOUND"
    );
    assert_eq!(repo.onboarding(None).unwrap().invite_tokens[0].use_count, 1);
    let me = repo.authenticate_agent(&registered.api_key).unwrap();
    let summary = serde_json::to_string(&me).unwrap();
    assert!(!summary.contains(&registered.api_key));
    assert!(!summary.contains("api_key_hash"));
    assert!(!summary.contains("mmr"));
    assert!(me.last_seen_at.is_some());
    // 连同 WAL 检查，避免明文被遗漏到其他持久化字段。
    for file in std::fs::read_dir(dir.path()).unwrap() {
        let bytes = std::fs::read(file.unwrap().path()).unwrap();
        for secret in [&created.raw_token, &registered.api_key] {
            assert!(!bytes
                .windows(secret.len())
                .any(|part| part == secret.as_bytes()));
        }
    }
    conn.execute_batch("CREATE TRIGGER reject_agent BEFORE INSERT ON agents BEGIN SELECT RAISE(ABORT,'injected write failure'); END;").unwrap();
    assert_eq!(
        repo.register_agent(register("write-fails", Some(&created.raw_token)))
            .err()
            .unwrap()
            .code,
        "INTERNAL_ERROR"
    );
    assert_eq!(repo.onboarding(None).unwrap().invite_tokens[0].use_count, 1);
}

#[test]
fn one_use_invite_allows_exactly_one_registration_across_connections() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::open(dir.path().join("agents.db")).unwrap();
    let invite = repo
        .create_invite_token(CreateInviteToken {
            max_uses: Some(1),
            ..Default::default()
        })
        .unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let threads: Vec<_> = (0..2)
        .map(|i| {
            let repo = repo.clone();
            let raw = invite.raw_token.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                repo.register_agent(register(&format!("agent-{i}"), Some(&raw)))
                    .map(|_| ())
                    .map_err(|e| e.code)
            })
        })
        .collect();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err("INVITE_TOKEN_INVALID"))
            .count(),
        1
    );
    assert_eq!(repo.onboarding(None).unwrap().invite_tokens[0].use_count, 1);
}

#[test]
fn invite_revocation_expiration_and_lan_gate_are_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::open(dir.path().join("agents.db")).unwrap();
    assert_eq!(
        repo.register_agent(register("closed", None))
            .err()
            .unwrap()
            .code,
        "INVITE_TOKEN_REQUIRED"
    );
    repo.onboarding(Some(UpdateOnboarding {
        lan_open_registration: true,
    }))
    .unwrap();
    assert!(repo.register_agent(register("open", None)).is_ok());
    assert_eq!(
        repo.register_agent(register("invalid", Some("wrong")))
            .err()
            .unwrap()
            .code,
        "INVITE_TOKEN_INVALID"
    );
    let revoked = repo.create_invite_token(Default::default()).unwrap();
    repo.revoke_invite_token(&revoked.token.id).unwrap();
    assert_eq!(
        repo.register_agent(register("revoked", Some(&revoked.raw_token)))
            .err()
            .unwrap()
            .code,
        "INVITE_TOKEN_INVALID"
    );
    let expired = repo.create_invite_token(Default::default()).unwrap();
    Connection::open(repo.path())
        .unwrap()
        .execute(
            "UPDATE invite_tokens SET expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
            [&expired.token.id],
        )
        .unwrap();
    assert_eq!(
        repo.register_agent(register("expired", Some(&expired.raw_token)))
            .err()
            .unwrap()
            .code,
        "INVITE_TOKEN_INVALID"
    );
    repo.onboarding(Some(UpdateOnboarding {
        lan_open_registration: false,
    }))
    .unwrap();
    assert_eq!(
        repo.register_agent(register("closed-again", None))
            .err()
            .unwrap()
            .code,
        "INVITE_TOKEN_REQUIRED"
    );
}

#[test]
fn subscription_defaults_and_empty_selections_remain_distinct() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::open(dir.path().join("agents.db")).unwrap();
    repo.onboarding(Some(UpdateOnboarding {
        lan_open_registration: true,
    }))
    .unwrap();
    for (i, (selection, expected)) in [
        (json!({}), true),
        (json!({"subscribe_all_projects":true}), true),
        (
            json!({"subscribe_all_projects":true,"project_ids":[]}),
            true,
        ),
        (json!({"project_ids":[]}), false),
        (json!({"subscribe_all_projects":false}), false),
    ]
    .into_iter()
    .enumerate()
    {
        let mut input = selection;
        input["name"] = json!(format!("agent-{i}"));
        let registered = repo
            .register_agent(serde_json::from_value(input).unwrap())
            .unwrap();
        assert_eq!(registered.agent.subscribe_all_projects, expected);
        assert!(registered.agent.project_ids.is_empty());
        let updated = repo
            .update_agent_subscriptions(
                &registered.agent.id,
                AgentSubscriptions {
                    subscribe_all_projects: Some(!expected),
                    project_ids: None,
                },
            )
            .unwrap();
        assert_eq!(updated.subscribe_all_projects, !expected);
        let updated = repo
            .update_agent_subscriptions(&registered.agent.id, AgentSubscriptions::default())
            .unwrap();
        assert!(updated.subscribe_all_projects);
    }
}
