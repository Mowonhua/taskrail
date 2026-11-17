#![cfg(not(target_os = "android"))]
//! 文件职责：验证 Agent 发单鉴权、项目审核策略、发布者身份及迁移与分发的原子性。
use reqwest::{Client, Method, StatusCode};
use rusqlite::{params, Connection, TransactionBehavior};
use serde_json::{json, Value};
use taskrail_lib::server::{
    self, db::Repository, models::*, RunningServer, ServerConfig, ServerMode,
};

const ADMIN: &str = "publishing-admin";

fn input(project_id: &str, extra: Value) -> Value {
    let mut value =
        json!({"project_id":project_id,"title":"巡检反馈","difficulty":"B","assignee_mode":"any"});
    value
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    value
}

struct Fixture {
    server: RunningServer,
    repo: Repository,
    client: Client,
    _dir: tempfile::TempDir,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("publishing.db");
        let server = server::start(ServerConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            db_path: path.clone(),
            admin_token: ADMIN.into(),
            mode: ServerMode::Standalone,
        })
        .await
        .unwrap();
        let repo = Repository::open(path).unwrap();
        repo.onboarding(Some(UpdateOnboarding {
            lan_open_registration: true,
        }))
        .unwrap();
        Self {
            server,
            repo,
            client: Client::new(),
            _dir: dir,
        }
    }

    fn project(&self, policy: &str) -> Project {
        self.repo
            .create_project(
                serde_json::from_value(json!({"name":policy,"agent_draft_policy":policy})).unwrap(),
            )
            .unwrap()
    }

    fn agent(&self, name: &str, extra: Value) -> RegisteredAgent {
        let mut value = json!({"name":name});
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.repo
            .register_agent(serde_json::from_value(value).unwrap())
            .unwrap()
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        key: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = self.client.request(
            method,
            format!("{}/api/v1{path}", self.server.info.base_url),
        );
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
}

#[tokio::test]
async fn open_policy_binds_publisher_and_dispatches() {
    let f = Fixture::new().await;
    let project = f.project("open");
    // 同名 human 的 Agent 仍使用带前缀的身份，不能与管理员混淆。
    let publisher = f.agent("human", json!({}));
    let (status, bounty) = f
        .request(
            Method::POST,
            "/agent/bounties",
            Some(&publisher.api_key),
            Some(input(&project.id, json!({}))),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{bounty}");
    assert_eq!(bounty["status"], "OPEN");
    assert!(bounty["published_at"].is_string());
    assert_eq!(
        bounty["created_by"],
        format!("agent:{}", publisher.agent.id)
    );
    assert_eq!(bounty["created_by_agent_id"], publisher.agent.id);
    assert_eq!(bounty["created_by_agent_name"], "human");
    let events = f
        .repo
        .agent_events(&publisher.agent.id, None, 100)
        .unwrap()
        .events;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "task.published");
    assert_eq!(events[0].data, bounty);
    f.server.shutdown().await.unwrap();
}

fn assert_public(value: &Value) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                assert!(
                    !["api_key", "api_key_hash", "webhook_secret", "mmr"].contains(&key.as_str()),
                    "{key}"
                );
                assert_public(value);
            }
        }
        Value::Array(values) => values.iter().for_each(assert_public),
        _ => {}
    }
}

#[tokio::test]
async fn publishing_authentication_and_validation_boundaries() {
    let f = Fixture::new().await;
    let p = f.project("open");
    let publisher = f.agent("publisher", json!({}));
    let disabled = f.agent("disabled", json!({}));
    let revoked = f.agent("revoked", json!({}));
    f.repo
        .set_agent_status(&disabled.agent.id, AgentStatus::Disabled)
        .unwrap();
    f.repo
        .set_agent_status(&revoked.agent.id, AgentStatus::Revoked)
        .unwrap();
    for (key, expected, code) in [
        (None, StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
        (Some("wrong"), StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
        (Some(ADMIN), StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
        (
            Some(disabled.api_key.as_str()),
            StatusCode::FORBIDDEN,
            "AGENT_DISABLED",
        ),
        (
            Some(revoked.api_key.as_str()),
            StatusCode::UNAUTHORIZED,
            "AGENT_REVOKED",
        ),
    ] {
        let (status, error) = f
            .request(
                Method::POST,
                "/agent/bounties",
                key,
                Some(input(&p.id, json!({}))),
            )
            .await;
        assert_eq!(status, expected, "{error}");
        assert_eq!(error["error"]["code"], code);
    }
    // 仓储自身也须复核状态，不依赖先前 HTTP 认证结果。
    for (agent, code) in [(&disabled, "AGENT_DISABLED"), (&revoked, "AGENT_REVOKED")] {
        assert_eq!(
            f.repo
                .agent_create_bounty(
                    &agent.agent.id,
                    serde_json::from_value(input(&p.id, json!({}))).unwrap()
                )
                .unwrap_err()
                .code,
            code
        );
    }
    for extra in [
        json!({"unknown":true}),
        json!({"status":"OPEN"}),
        json!({"status":"DRAFT"}),
        json!({"created_by":"human"}),
        json!({"created_by_agent_id":publisher.agent.id}),
        json!({"created_by_agent_name":"forged"}),
        json!({"title":" \t\n"}),
        json!({"assignee_mode":"any","assignee_agent_id":publisher.agent.id}),
        json!({"assignee_mode":"any","assignee_min_tier":"B"}),
        json!({"assignee_mode":"min_tier"}),
        json!({"assignee_mode":"min_tier","assignee_min_tier":"B","assignee_agent_id":publisher.agent.id}),
        json!({"assignee_mode":"specific"}),
        json!({"assignee_mode":"specific","assignee_agent_id":" "}),
        json!({"assignee_mode":"specific","assignee_agent_id":publisher.agent.id,"assignee_min_tier":"C"}),
        json!({"difficulty":"Unranked"}),
    ] {
        let (status, error) = f
            .request(
                Method::POST,
                "/agent/bounties",
                Some(&publisher.api_key),
                Some(input(&p.id, extra)),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
        assert_eq!(error["error"]["code"], "VALIDATION_ERROR");
    }
    let (status, error) = f
        .request(
            Method::POST,
            "/agent/bounties",
            Some(&publisher.api_key),
            Some(input("missing", json!({}))),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(error["error"]["code"], "PROJECT_NOT_FOUND");
    f.repo
        .update_project(
            &p.id,
            serde_json::from_value(json!({"archived":true})).unwrap(),
        )
        .unwrap();
    let (status, error) = f
        .request(
            Method::POST,
            "/agent/bounties",
            Some(&publisher.api_key),
            Some(input(&p.id, json!({}))),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"]["code"], "PROJECT_ARCHIVED");
    let conn = Connection::open(f.repo.path()).unwrap();
    for table in [
        "bounties",
        "bounty_events",
        "agent_events",
        "bounty_dispatches",
    ] {
        assert_eq!(count(&conn, table), 0);
    }
    let skill = f
        .client
        .get(format!("{}/skill.md", f.server.info.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    for text in [
        "POST",
        "/api/v1/agent/bounties",
        "agent_draft_policy",
        "created_by_agent_id",
        "DRAFT",
        "require_arbitration",
    ] {
        assert!(skill.contains(text), "{text}");
    }
    f.server.shutdown().await.unwrap();
}

#[tokio::test]
async fn dispatch_targets_match_subscriptions_tiers_and_specific_assignment() {
    let f = Fixture::new().await;
    let p = f.project("open");
    let publisher = f.agent("publisher", json!({"subscribe_all_projects":false,"webhook_url":"http://127.0.0.1:1/events","webhook_secret":"publisher-secret"}));
    let unranked = f.agent("unranked", json!({"project_ids":[p.id]}));
    let ranked = f.agent("ranked", json!({}));
    let excluded = f.agent("excluded", json!({"subscribe_all_projects":false}));
    let disabled = f.agent("disabled", json!({}));
    let revoked = f.agent("revoked", json!({}));
    f.repo
        .set_agent_status(&disabled.agent.id, AgentStatus::Disabled)
        .unwrap();
    f.repo
        .set_agent_status(&revoked.agent.id, AgentStatus::Revoked)
        .unwrap();
    let conn = Connection::open(f.repo.path()).unwrap();
    conn.execute(
        "UPDATE agents SET tier='B',mmr=1234 WHERE id=?1",
        [&ranked.agent.id],
    )
    .unwrap();
    for (extra, targets, event_type) in [
        (
            json!({}),
            vec![&unranked.agent.id, &ranked.agent.id],
            "task.published",
        ),
        (
            json!({"assignee_mode":"min_tier","assignee_min_tier":"C"}),
            vec![&unranked.agent.id, &ranked.agent.id],
            "task.published",
        ),
        (
            json!({"assignee_mode":"min_tier","assignee_min_tier":"B"}),
            vec![&ranked.agent.id],
            "task.published",
        ),
        (
            json!({"assignee_mode":"specific","assignee_agent_id":excluded.agent.id}),
            vec![&excluded.agent.id],
            "task.assigned",
        ),
    ] {
        let body = input(&p.id, extra);
        let (status, bounty) = f
            .request(
                Method::POST,
                "/agent/bounties",
                Some(&publisher.api_key),
                Some(body),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{bounty}");
        assert_public(&bounty);
        assert_eq!(bounty["created_by_agent_id"], publisher.agent.id);
        assert_eq!(bounty["created_by_agent_name"], publisher.agent.name);
        let id = bounty["id"].as_str().unwrap();
        let detail = f.repo.get_bounty_detail(id).unwrap();
        assert_eq!(detail.events.len(), 1);
        assert_eq!(
            detail.events[0].actor,
            format!("agent:{}", publisher.agent.id)
        );
        assert_eq!(detail.events[0].event_type, "created");
        assert_eq!(
            detail.events[0].detail,
            json!({"publisher_agent_id":publisher.agent.id,"publisher_agent_name":"publisher","policy":"open"})
        );
        let mut expected: Vec<String> = targets.iter().map(|id| (*id).clone()).collect();
        expected.sort();
        for table in ["agent_events", "bounty_dispatches"] {
            let actual = conn
                .prepare(&format!(
                    "SELECT agent_id FROM {table} WHERE bounty_id=?1 ORDER BY agent_id"
                ))
                .unwrap()
                .query_map([id], |r| r.get::<_, String>(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(actual, expected, "{table}");
        }
        let mut stmt = conn
            .prepare("SELECT event_type,payload_json FROM agent_events WHERE bounty_id=?1")
            .unwrap();
        for row in stmt
            .query_map([id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .unwrap()
        {
            let (kind, payload) = row.unwrap();
            assert_eq!(kind, event_type);
            let payload: Value = serde_json::from_str(&payload).unwrap();
            assert_public(&payload);
            assert_eq!(payload["data"], bounty);
        }
        let key = if event_type == "task.assigned" {
            &excluded.api_key
        } else {
            &ranked.api_key
        };
        let (status, detail) = f
            .request(
                Method::GET,
                &format!("/agent/bounties/{id}"),
                Some(key),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(detail["bounty"], bounty);
        assert_public(&detail);
        let claimed = f.repo.try_claim_bounty(id, targets[0]).unwrap();
        assert_eq!(claimed.status, BountyStatus::InProgress);
        assert_eq!(
            claimed.created_by_agent_id.as_ref(),
            Some(&publisher.agent.id)
        );
    }
    // LEFT JOIN 同时覆盖当前名称和人工创建的空发布者身份。
    conn.execute(
        "UPDATE agents SET name='renamed' WHERE id=?1",
        [&publisher.agent.id],
    )
    .unwrap();
    let list = f.repo.list_bounties(BountyFilter::default()).unwrap();
    assert!(list
        .iter()
        .all(|b| b.created_by_agent_name.as_deref() == Some("renamed")));
    assert!(list.iter().all(|b| f
        .repo
        .get_bounty_detail(&b.id)
        .unwrap()
        .bounty
        .created_by_agent_name
        .as_deref()
        == Some("renamed")));
    let human = f
        .repo
        .create_bounty(serde_json::from_value(input(&p.id, json!({"status":"DRAFT"}))).unwrap())
        .unwrap();
    assert_eq!(human.created_by, "human");
    assert!(human.created_by_agent_id.is_none());
    assert!(human.created_by_agent_name.is_none());
    f.server.shutdown().await.unwrap();
}

#[tokio::test]
async fn draft_policy_isolates_all_agents_until_admin_publish_or_cancel() {
    let f = Fixture::new().await;
    let p = f.project("draft");
    let publisher = f.agent("publisher", json!({}));
    let recipient = f.agent("recipient", json!({"subscribe_all_projects":false}));
    let observer = f.agent("observer", json!({}));
    let conn = Connection::open(f.repo.path()).unwrap();
    let mut ids = Vec::new();
    for extra in [
        json!({}),
        json!({"assignee_mode":"specific","assignee_agent_id":publisher.agent.id}),
        json!({"assignee_mode":"specific","assignee_agent_id":recipient.agent.id}),
        json!({"title":"UNAPPROVED_PRIVATE_BODY"}),
    ] {
        let (status, bounty) = f
            .request(
                Method::POST,
                "/agent/bounties",
                Some(&publisher.api_key),
                Some(input(&p.id, extra)),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{bounty}");
        assert_eq!(bounty["status"], "DRAFT");
        assert_eq!(bounty["published_at"], Value::Null);
        assert_eq!(bounty["created_by_agent_id"], publisher.agent.id);
        ids.push(bounty["id"].as_str().unwrap().to_owned());
    }
    for table in ["agent_events", "bounty_dispatches"] {
        assert_eq!(count(&conn, table), 0);
    }
    for agent in [&publisher, &recipient, &observer] {
        for query in [
            "",
            "?include_drafts=true",
            "?status=DRAFT&include_drafts=true",
            "?q=巡检反馈&include_drafts=true",
            "?q=UNAPPROVED_PRIVATE_BODY&include_drafts=true",
        ] {
            let (status, list) = f
                .request(
                    Method::GET,
                    &format!("/agent/bounties{query}"),
                    Some(&agent.api_key),
                    None,
                )
                .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(list, json!([]));
        }
        for id in &ids {
            let (status, error) = f
                .request(
                    Method::GET,
                    &format!("/agent/bounties/{id}"),
                    Some(&agent.api_key),
                    None,
                )
                .await;
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(error["error"]["code"], "BOUNTY_NOT_FOUND");
            for method in [Method::GET, Method::POST] {
                let (status, error) = f
                    .request(
                        method,
                        &format!("/agent/bounties/{id}/comments"),
                        Some(&agent.api_key),
                        Some(json!({"body":"private draft"})),
                    )
                    .await;
                assert_eq!(status, StatusCode::NOT_FOUND);
                assert_eq!(error["error"]["code"], "BOUNTY_NOT_FOUND");
            }
        }
        assert!(f
            .repo
            .agent_events(&agent.agent.id, None, 100)
            .unwrap()
            .events
            .is_empty());
    }
    let (_, hidden) = f.request(Method::GET, "/bounties", Some(ADMIN), None).await;
    assert_eq!(hidden, json!([]));
    let (status, pending) = f
        .request(
            Method::GET,
            "/bounties?status=DRAFT&include_drafts=true",
            Some(ADMIN),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pending.as_array().unwrap().len(), 4);
    assert_public(&pending);
    // 策略切换仅影响后续提交，不隐式公开已经存在的草稿。
    f.repo
        .update_project(
            &p.id,
            serde_json::from_value(json!({"agent_draft_policy":"open"})).unwrap(),
        )
        .unwrap();
    assert!(ids
        .iter()
        .all(|id| f.repo.get_bounty_detail(id).unwrap().bounty.status == BountyStatus::Draft));
    let id = &ids[0];
    let (status, edited) = f.request(Method::PATCH, &format!("/bounties/{id}"), Some(ADMIN), Some(json!({"title":"人工确认","description":"已审阅 Markdown","acceptance_criteria":"验收通过","difficulty":"A","require_arbitration":true}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(edited["created_by_agent_id"], publisher.agent.id);
    assert_eq!(
        edited["created_by"],
        format!("agent:{}", publisher.agent.id)
    );
    let (status, published) = f
        .request(
            Method::POST,
            &format!("/bounties/{id}/publish"),
            Some(ADMIN),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(published["status"], "OPEN");
    assert_eq!(published["title"], "人工确认");
    assert_eq!(published["require_arbitration"], true);
    assert_eq!(published["created_by_agent_id"], publisher.agent.id);
    assert_eq!(published["created_by_agent_name"], "publisher");
    assert_eq!(published["created_by"], edited["created_by"]);
    assert!(published["published_at"].is_string());
    for agent in [&publisher, &observer] {
        let inbox = f.repo.agent_events(&agent.agent.id, None, 100).unwrap();
        assert_eq!(inbox.events.len(), 1);
        assert_eq!(inbox.events[0].event_type, "task.published");
        assert_eq!(inbox.events[0].data, published);
        let (status, detail) = f
            .request(
                Method::GET,
                &format!("/agent/bounties/{id}"),
                Some(&agent.api_key),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_public(&detail);
        assert_eq!(detail["bounty"], published);
    }
    assert!(f
        .repo
        .agent_events(&recipient.agent.id, None, 100)
        .unwrap()
        .events
        .is_empty());
    let id = &ids[2];
    let (_, assigned) = f
        .request(
            Method::POST,
            &format!("/bounties/{id}/publish"),
            Some(ADMIN),
            None,
        )
        .await;
    assert_eq!(assigned["status"], "OPEN");
    let inbox = f.repo.agent_events(&recipient.agent.id, None, 100).unwrap();
    assert_eq!(inbox.events.len(), 1);
    assert_eq!(inbox.events[0].event_type, "task.assigned");
    assert_eq!(inbox.events[0].data, assigned);
    for id in [&ids[1], &ids[3]] {
        let (status, cancelled) = f
            .request(
                Method::POST,
                &format!("/bounties/{id}/cancel"),
                Some(ADMIN),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(cancelled["status"], "CANCELLED");
        assert_eq!(cancelled["published_at"], Value::Null);
        assert_eq!(cancelled["created_by_agent_id"], publisher.agent.id);
        assert_eq!(cancelled["created_by"], edited["created_by"]);
        assert!(cancelled["cancelled_at"].is_string());
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM agent_events WHERE bounty_id=?1",
                [id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        for agent in [&publisher, &recipient, &observer] {
            for query in [
                "",
                "?status=CANCELLED",
                "?q=UNAPPROVED_PRIVATE_BODY",
                "?q=巡检反馈",
                "?include_drafts=true",
            ] {
                let (status, list) = f
                    .request(
                        Method::GET,
                        &format!("/agent/bounties{query}"),
                        Some(&agent.api_key),
                        None,
                    )
                    .await;
                assert_eq!(status, StatusCode::OK);
                assert!(
                    !list
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|item| item["id"] == *id),
                    "discarded draft {id} leaked in {query} for {}",
                    agent.agent.id
                );
            }
            let (status, error) = f
                .request(
                    Method::GET,
                    &format!("/agent/bounties/{id}"),
                    Some(&agent.api_key),
                    None,
                )
                .await;
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(error["error"]["code"], "BOUNTY_NOT_FOUND");
            for method in [Method::GET, Method::POST] {
                let (status, error) = f
                    .request(
                        method,
                        &format!("/agent/bounties/{id}/comments"),
                        Some(&agent.api_key),
                        Some(json!({"body":"discarded draft"})),
                    )
                    .await;
                assert_eq!(status, StatusCode::NOT_FOUND);
                assert_eq!(error["error"]["code"], "BOUNTY_NOT_FOUND");
            }
        }
    }
    f.server.shutdown().await.unwrap();
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

fn legacy(path: &std::path::Path, version: i64) -> Connection {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(include_str!("fixtures/server_v1.sql"))
        .unwrap();
    if version >= 2 {
        conn.execute_batch(include_str!("../src/server/schema_v2.sql"))
            .unwrap();
    }
    if version >= 3 {
        conn.execute_batch(include_str!("../src/server/schema_v3.sql"))
            .unwrap();
    }
    conn.execute_batch(
        "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL);",
    )
    .unwrap();
    for v in 1..=version {
        conn.execute("INSERT INTO schema_migrations VALUES(?1,'old')", [v])
            .unwrap();
    }
    conn.execute_batch("INSERT INTO projects(id,name,created_at,updated_at) VALUES('p','existing','old','old'); INSERT INTO bounties(id,project_id,title,difficulty,assignee_mode,status,created_at,updated_at) VALUES('b','p','existing','C','any','OPEN','old','old'); INSERT INTO bounty_events(bounty_id,event_type,actor,to_status,created_at) VALUES('b','created','human','OPEN','old');").unwrap();
    if version >= 2 {
        conn.execute_batch("INSERT INTO invite_tokens(id,token_hash,token_prefix,use_count,created_at) VALUES('i','invite-hash','prefix',2,'old'); INSERT INTO agents(id,name,api_key_hash,api_key_prefix,invite_token_id,created_at,updated_at) VALUES('a','existing','agent-hash','ag','i','old','old'); INSERT INTO agent_subscriptions VALUES('a','p','old');").unwrap();
    }
    if version >= 3 {
        conn.execute_batch("UPDATE agents SET webhook_url='http://localhost/events',webhook_secret='secret',acked_event_id=1; INSERT INTO agent_events(agent_id,bounty_id,event_type,payload_json,created_at) VALUES('a','b','task.published','{}','old'); INSERT INTO bounty_dispatches VALUES('b','a','old');").unwrap();
    }
    conn
}

#[test]
fn v3_upgrade_preserves_data_identity_defaults_foreign_key_and_index() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v3.db");
    let conn = legacy(&path, 3);
    for _ in 0..2 {
        let repo = Repository::open(&path).unwrap();
        let detail = repo.get_bounty_detail("b").unwrap();
        assert_eq!(detail.bounty.title, "existing");
        assert_eq!(detail.bounty.created_by, "human");
        assert!(detail.bounty.created_by_agent_id.is_none());
        assert!(detail.bounty.created_by_agent_name.is_none());
        assert_eq!(detail.events.len(), 1);
        assert_eq!(detail.events[0].created_at, "old");
        assert_eq!(repo.get_project("p").unwrap().name, "existing");
        for table in [
            "projects",
            "bounties",
            "bounty_events",
            "agents",
            "invite_tokens",
            "agent_subscriptions",
            "agent_events",
            "bounty_dispatches",
        ] {
            assert_eq!(count(&conn, table), 1, "{table}");
        }
    }
    assert_eq!(
        conn.query_row(
            "SELECT group_concat(version) FROM schema_migrations",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "1,2,3,4,5,6,7,8"
    );
    assert_eq!(
        conn.query_row(
            "SELECT webhook_secret,acked_event_id FROM agents",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        )
        .unwrap(),
        ("secret".into(), 1)
    );
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM pragma_index_info('bounties_publisher_agent') WHERE name='created_by_agent_id'", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    assert!(conn
        .execute("UPDATE bounties SET created_by_agent_id='missing'", [])
        .is_err());
    conn.execute("UPDATE bounties SET created_by_agent_id='a'", [])
        .unwrap();
    assert!(conn.execute("DELETE FROM agents WHERE id='a'", []).is_err());
    let b = Repository::open(&path)
        .unwrap()
        .get_bounty_detail("b")
        .unwrap()
        .bounty;
    assert_eq!(b.created_by_agent_name.as_deref(), Some("existing"));
    assert_public(&serde_json::to_value(b).unwrap());
}

#[test]
fn v4_failure_rolls_back_all_prior_migrations_and_future_schema_is_untouched() {
    let dir = tempfile::tempdir().unwrap();
    for version in [1, 2, 3] {
        let path = dir.path().join(format!("broken-{version}.db"));
        let conn = legacy(&path, version);
        // 索引名与表名冲突使 v4 在新增列后失败，检验 DDL 与全部版本记录整体回滚。
        conn.execute_batch("CREATE TABLE bounties_publisher_agent(conflict TEXT);")
            .unwrap();
        let before = conn
            .query_row(
                "SELECT group_concat(sql,';') FROM (SELECT sql FROM sqlite_master ORDER BY name)",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(Repository::open(&path).unwrap_err().code, "INTERNAL_ERROR");
        assert_eq!(
            conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            version
        );
        let after = conn
            .query_row(
                "SELECT group_concat(sql,';') FROM (SELECT sql FROM sqlite_master ORDER BY name)",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(count(&conn, "bounties"), 1);
        conn.execute_batch("DROP TABLE bounties_publisher_agent;")
            .unwrap();
        assert!(Repository::open(&path)
            .unwrap()
            .get_bounty_detail("b")
            .unwrap()
            .bounty
            .created_by_agent_id
            .is_none());
    }
    let path = dir.path().join("future.db");
    Repository::open(&path).unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute("INSERT INTO schema_migrations VALUES(9,'future')", [])
        .unwrap();
    assert_eq!(Repository::open(&path).unwrap_err().code, "INTERNAL_ERROR");
    assert_eq!(
        conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        9
    );
    assert_eq!(count(&conn, "schema_migrations"), 2);
}

#[tokio::test]
async fn creation_event_or_dispatch_failure_rolls_back_the_entire_submission() {
    let f = Fixture::new().await;
    let p = f.project("open");
    let publisher = f.agent("publisher", json!({}));
    let conn = Connection::open(f.repo.path()).unwrap();
    for table in ["bounty_events", "agent_events", "bounty_dispatches"] {
        conn.execute_batch(&format!("CREATE TRIGGER fail_insert BEFORE INSERT ON {table} BEGIN SELECT RAISE(ABORT,'test failure'); END;")).unwrap();
        let (status, error) = f
            .request(
                Method::POST,
                "/agent/bounties",
                Some(&publisher.api_key),
                Some(input(&p.id, json!({}))),
            )
            .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error["error"]["code"], "INTERNAL_ERROR");
        for table in [
            "bounties",
            "bounty_events",
            "agent_events",
            "bounty_dispatches",
        ] {
            assert_eq!(count(&conn, table), 0);
        }
        conn.execute_batch("DROP TRIGGER fail_insert;").unwrap();
    }
    f.server.shutdown().await.unwrap();
}

#[tokio::test]
async fn policy_changes_and_submissions_use_one_atomic_write_snapshot() {
    let f = Fixture::new().await;
    let p = f.project("open");
    let publisher = f.agent("publisher", json!({}));
    let mut conn = Connection::open(f.repo.path()).unwrap();
    // 让策略变更先持有写锁：提交者只能在变更提交后读取策略。
    for (policy, expected) in [("draft", BountyStatus::Draft), ("open", BountyStatus::Open)] {
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        tx.execute(
            "UPDATE projects SET agent_draft_policy=?2 WHERE id=?1",
            params![p.id, policy],
        )
        .unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let repo = f.repo.clone();
        let agent_id = publisher.agent.id.clone();
        let body = input(&p.id, json!({}));
        let other = barrier.clone();
        let submit = std::thread::spawn(move || {
            other.wait();
            repo.agent_create_bounty(&agent_id, serde_json::from_value(body).unwrap())
                .unwrap()
        });
        barrier.wait();
        tx.commit().unwrap();
        assert_eq!(submit.join().unwrap().status, expected);
    }
    // 独立连接同时争夺写锁；插入时的数据库策略是状态与 created 事件的共同依据。
    conn.execute_batch("CREATE TRIGGER check_policy BEFORE INSERT ON bounties WHEN NEW.created_by_agent_id IS NOT NULL BEGIN SELECT CASE WHEN NEW.status <> (SELECT CASE agent_draft_policy WHEN 'open' THEN 'OPEN' ELSE 'DRAFT' END FROM projects WHERE id=NEW.project_id) THEN RAISE(ABORT,'policy mismatch') END; END;").unwrap();
    for iteration in 0..24 {
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let repo = f.repo.clone();
        let project_id = p.id.clone();
        let policy = if iteration % 2 == 0 { "draft" } else { "open" };
        let other = barrier.clone();
        let change = std::thread::spawn(move || {
            other.wait();
            repo.update_project(
                &project_id,
                serde_json::from_value(json!({"agent_draft_policy":policy})).unwrap(),
            )
            .unwrap()
        });
        barrier.wait();
        let bounty = f
            .repo
            .agent_create_bounty(
                &publisher.agent.id,
                serde_json::from_value(input(&p.id, json!({}))).unwrap(),
            )
            .unwrap();
        change.join().unwrap();
        let detail = f.repo.get_bounty_detail(&bounty.id).unwrap();
        let is_open = bounty.status == BountyStatus::Open;
        assert_eq!(
            detail.events[0].detail["policy"],
            if is_open { "open" } else { "draft" }
        );
        assert_eq!(bounty.published_at.is_some(), is_open);
        for table in ["agent_events", "bounty_dispatches"] {
            assert_eq!(
                conn.query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE bounty_id=?1"),
                    [&bounty.id],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                i64::from(is_open)
            );
        }
    }
    f.server.shutdown().await.unwrap();
}
