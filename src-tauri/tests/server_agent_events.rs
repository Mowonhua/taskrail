#![cfg(not(target_os = "android"))]
//! 文件职责：验证持久化收件箱、精准分发及双通道实际传输。
use serde_json::{json, Value};
use taskrail_lib::server::{db::Repository, models::*};

fn register(repo: &Repository, name: &str, extra: Value) -> RegisteredAgent {
    let mut input = json!({"name":name});
    input
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    repo.register_agent(serde_json::from_value(input).unwrap())
        .unwrap()
}
fn project(repo: &Repository, name: &str) -> Project {
    repo.create_project(serde_json::from_value(json!({"name":name})).unwrap())
        .unwrap()
}
fn bounty(repo: &Repository, project: &Project, extra: Value) -> Bounty {
    let mut input = json!({"project_id":project.id,"title":"public","difficulty":"C","assignee_mode":"any","status":"OPEN"});
    input
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    repo.create_bounty(serde_json::from_value(input).unwrap())
        .unwrap()
}
fn inbox(repo: &Repository, id: &str) -> AgentInbox {
    repo.agent_events(id, None, 100).unwrap()
}

#[test]
fn dispatch_subscription_tiers_assignment_withdrawal_and_ack() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::open(dir.path().join("events.db")).unwrap();
    repo.onboarding(Some(UpdateOnboarding {
        lan_open_registration: true,
    }))
    .unwrap();
    let p = project(&repo, "p");
    let other = project(&repo, "other");
    let u = register(&repo, "unranked", json!({})).agent;
    let excluded = register(&repo, "excluded", json!({"project_ids":[other.id]})).agent;
    let b = register(&repo, "B", json!({})).agent;
    let c = register(&repo, "C", json!({})).agent;
    let a = register(&repo, "A", json!({})).agent;
    let top = register(&repo, "S", json!({})).agent;
    let conn = rusqlite::Connection::open(repo.path()).unwrap();
    for (agent, tier) in [(&c, "C"), (&a, "A"), (&top, "S")] {
        conn.execute("UPDATE agents SET tier=?2 WHERE id=?1", [&agent.id, tier])
            .unwrap();
    }
    rusqlite::Connection::open(repo.path())
        .unwrap()
        .execute("UPDATE agents SET tier='B' WHERE id=?1", [&b.id])
        .unwrap();
    bounty(&repo, &p, json!({"status":"DRAFT","title":"private draft"}));
    assert!(inbox(&repo, &u.id).events.is_empty());
    let open = bounty(
        &repo,
        &p,
        json!({"assignee_mode":"min_tier","assignee_min_tier":"C"}),
    );
    assert_eq!(inbox(&repo, &u.id).events[0].event_type, "task.published");
    assert!(inbox(&repo, &excluded.id).events.is_empty());
    for tier in ["B", "A", "S"] {
        bounty(
            &repo,
            &p,
            json!({"assignee_mode":"min_tier","assignee_min_tier":tier}),
        );
    }
    assert_eq!(inbox(&repo, &u.id).events.len(), 1);
    assert_eq!(inbox(&repo, &b.id).events.len(), 2);
    assert_eq!(inbox(&repo, &c.id).events.len(), 1);
    assert_eq!(inbox(&repo, &a.id).events.len(), 3);
    assert_eq!(inbox(&repo, &top.id).events.len(), 4);
    repo.set_agent_status(&a.id, AgentStatus::Disabled).unwrap();
    let assigned = bounty(
        &repo,
        &p,
        json!({"assignee_mode":"specific","assignee_agent_id":excluded.id}),
    );
    assert_eq!(
        inbox(&repo, &excluded.id).events[0].event_type,
        "task.assigned"
    );
    repo.update_agent_subscriptions(
        &u.id,
        serde_json::from_value(json!({"subscribe_all_projects":false})).unwrap(),
    )
    .unwrap();
    repo.withdraw_bounty(&open.id, "human").unwrap();
    repo.withdraw_bounty(&assigned.id, "human").unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM agent_events WHERE agent_id=?1",
            [&a.id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        3
    );
    let events = inbox(&repo, &u.id).events;
    assert_eq!(events[1].event_type, "task.withdrawn");
    assert!(events[1].data.get("description").is_none());
    assert_eq!(inbox(&repo, &excluded.id).events.len(), 2);
    let first = repo.agent_events(&u.id, Some(0), 1).unwrap();
    assert!(first.has_more);
    assert_eq!(first.cursor, events[0].event_id);
    assert_eq!(
        repo.agent_events(&u.id, Some(first.cursor), 100)
            .unwrap()
            .events
            .len(),
        1
    );
    assert_eq!(
        repo.ack_agent_events(&u.id, i64::MAX).unwrap(),
        events[1].event_id
    );
    assert!(inbox(&repo, &u.id).events.is_empty());
    assert_eq!(repo.ack_agent_events(&u.id, 0).unwrap(), events[1].event_id);
    assert!(repo.ack_agent_events(&u.id, -1).is_err());
    assert!(repo.agent_events(&u.id, None, 0).is_err());
    let public = serde_json::to_string(&repo.list_agents(AgentFilter::default()).unwrap()).unwrap();
    for forbidden in ["mmr", "api_key_hash", "webhook_secret"] {
        assert!(!public.contains(forbidden));
    }
}

#[test]
fn v2_migration_preserves_all_existing_tables_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v2.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(include_str!("fixtures/server_v1.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/server/schema_v2.sql"))
        .unwrap();
    conn.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL); INSERT INTO schema_migrations VALUES(1,'old'),(2,'old'); UPDATE server_settings SET lan_open_registration=1; INSERT INTO projects(id,name,created_at,updated_at) VALUES('p','existing','old','old'); INSERT INTO bounties(id,project_id,title,difficulty,assignee_mode,status,created_at,updated_at) VALUES('b','p','existing','C','any','OPEN','old','old'); INSERT INTO bounty_events(bounty_id,event_type,actor,to_status,created_at) VALUES('b','created','human','OPEN','old'); INSERT INTO invite_tokens(id,token_hash,token_prefix,use_count,created_at) VALUES('i','hash','prefix',2,'old'); INSERT INTO agents(id,name,api_key_hash,api_key_prefix,invite_token_id,created_at,updated_at) VALUES('a','existing','agent-hash','ag','i','old','old'); INSERT INTO agent_subscriptions VALUES('a','p','old');").unwrap();
    for _ in 0..2 {
        Repository::open(&path).unwrap();
    }
    for table in [
        "projects",
        "bounties",
        "bounty_events",
        "invite_tokens",
        "agents",
        "agent_subscriptions",
    ] {
        assert_eq!(
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    let repo = Repository::open(&path).unwrap();
    assert!(repo.onboarding(None).unwrap().lan_open_registration);
    assert_eq!(repo.list_invite_tokens().unwrap()[0].use_count, 2);
    assert_eq!(
        repo.list_agents(AgentFilter::default()).unwrap()[0].project_ids,
        vec!["p"]
    );
    assert_eq!(
        conn.query_row(
            "SELECT group_concat(version) FROM schema_migrations",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "1,2,3,4,5,6"
    );
    assert_eq!(
        conn.query_row("SELECT acked_event_id FROM agents", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    // 删除后新事件的编号也不能重用；收件箱与投放记录均随 Agent 级联删除。
    conn.execute_batch("PRAGMA foreign_keys=ON; INSERT INTO agent_events(agent_id,bounty_id,event_type,payload_json,created_at) VALUES('a','b','task.published','{}','old'); INSERT INTO bounty_dispatches VALUES('b','a','old'); DELETE FROM agent_events; INSERT INTO agent_events(agent_id,event_type,payload_json,created_at) VALUES('a','task.withdrawn','{}','old');").unwrap();
    assert_eq!(
        conn.query_row("SELECT event_id FROM agent_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    conn.execute("DELETE FROM agents", []).unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM agent_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM bounty_dispatches", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

use futures_util::{SinkExt, StreamExt};
use reqwest::{Client, Method, StatusCode};
use std::time::Duration;
use taskrail_lib::server::{self, RunningServer, ServerConfig, ServerMode};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};

struct Fixture {
    dir: tempfile::TempDir,
    server: RunningServer,
    repo: Repository,
    client: Client,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("http.db");
        let server = server::start(ServerConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            db_path: path.clone(),
            admin_token: "admin".into(),
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
            dir,
            server,
            repo,
            client: Client::new(),
        }
    }
    async fn request(
        &self,
        method: Method,
        path: &str,
        key: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.server.info.base_url))
            .bearer_auth(key);
        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(body.to_string());
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        (
            status,
            serde_json::from_str(&response.text().await.unwrap()).unwrap(),
        )
    }
    async fn me(&self, key: &str) -> Value {
        self.request(Method::GET, "/api/v1/agent/me", key, None)
            .await
            .1
    }
    async fn websocket(
        &self,
        key: &str,
        cursor: i64,
    ) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>
    {
        let url = format!(
            "{}/api/v1/agent/ws?cursor={cursor}",
            self.server.info.base_url.replace("http:", "ws:")
        );
        let mut request = url.into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Authorization", format!("Bearer {key}").parse().unwrap());
        connect_async(request).await.unwrap().0
    }
}
async fn next_ws(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Value {
    let message = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    serde_json::from_str(message.to_text().unwrap()).unwrap()
}
async fn next_sse(response: &mut reqwest::Response, buffer: &mut String) -> Value {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(end) = buffer.find("\n\n") {
                let frame = buffer[..end].to_owned();
                buffer.drain(..end + 2);
                if let Some(data) = frame.lines().find_map(|l| l.strip_prefix("data: ")) {
                    let event: Value = serde_json::from_str(data).unwrap();
                    assert!(frame.contains(&format!("id: {}", event["event_id"])));
                    assert!(frame
                        .contains(&format!("event: {}", event["event_type"].as_str().unwrap())));
                    return event;
                }
            }
            let chunk = response
                .chunk()
                .await
                .unwrap()
                .expect("SSE unexpectedly closed");
            buffer.push_str(std::str::from_utf8(&chunk).unwrap());
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn inbox_http_isolation_validation_and_webhook_configuration() {
    let f = Fixture::new().await;
    let (status, registered) = f
        .request(
            Method::POST,
            "/api/v1/agents/register",
            "",
            Some(json!({"name":"registered-webhook","webhook_url":"http://127.0.0.1:1"})),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(registered["webhook_secret"]
        .as_str()
        .unwrap()
        .starts_with("tr_whsec_"));
    assert!(registered["agent"].get("webhook_secret").is_none());
    let skill = f
        .client
        .get(format!("{}/skill.md", f.server.info.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    for expected in [
        "/agent/me/webhook",
        "/agent/events/ack",
        "/agent/stream",
        "/agent/ws",
        "HMAC-SHA256",
        "Unranked",
    ] {
        assert!(skill.contains(expected));
    }
    let a = register(&f.repo, "a", json!({}));
    let b = register(&f.repo, "b", json!({"subscribe_all_projects":false}));
    let p = project(&f.repo, "p");
    bounty(&f.repo, &p, json!({}));
    let (status, inbox) = f
        .request(Method::GET, "/api/v1/agent/events", &a.api_key, None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(inbox["events"].as_array().unwrap().len(), 1);
    assert_eq!(
        f.request(Method::GET, "/api/v1/agent/events", &b.api_key, None)
            .await
            .1["events"],
        json!([])
    );
    for path in [
        "/api/v1/agent/events?agent_id=a",
        "/api/v1/agent/events?limit=0",
        "/api/v1/agent/events?limit=-1",
        "/api/v1/agent/events?cursor=-1",
        "/api/v1/agent/events?cursor=no",
    ] {
        assert_eq!(
            f.request(Method::GET, path, &a.api_key, None).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        f.request(Method::GET, "/api/v1/agent/events", "admin", None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    for body in [
        json!({"event_id":-1}),
        json!({"event_id":1,"agent_id":a.agent.id}),
    ] {
        assert_eq!(
            f.request(
                Method::POST,
                "/api/v1/agent/events/ack",
                &b.api_key,
                Some(body)
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        f.request(
            Method::POST,
            "/api/v1/agent/events/ack",
            &b.api_key,
            Some(json!({"event_id":i64::MAX}))
        )
        .await
        .1["acked_event_id"],
        0
    );
    assert_eq!(
        f.request(
            Method::POST,
            "/api/v1/agent/events/ack",
            &a.api_key,
            Some(json!({"event_id":i64::MAX}))
        )
        .await
        .1["acked_event_id"],
        inbox["latest_event_id"]
    );
    for body in [
        json!({}),
        json!({"webhook_url":"ftp://localhost"}),
        json!({"webhook_url":""}),
        json!({"webhook_url":"https://example.com","webhook_secret":"  "}),
        json!({"webhook_url":null,"unknown":1}),
    ] {
        assert_eq!(
            f.request(
                Method::PUT,
                "/api/v1/agent/me/webhook",
                &a.api_key,
                Some(body)
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    let config = f
        .request(
            Method::PUT,
            "/api/v1/agent/me/webhook",
            &a.api_key,
            Some(json!({"webhook_url":"http://127.0.0.1:1"})),
        )
        .await
        .1;
    assert!(config["webhook_secret"]
        .as_str()
        .unwrap()
        .starts_with("tr_whsec_"));
    assert_eq!(config["webhook_secret"].as_str().unwrap().len(), 73);
    assert_eq!(f.me(&a.api_key).await["channel_status"], "webhook");
    let read = f
        .request(Method::GET, "/api/v1/agent/me/webhook", &a.api_key, None)
        .await
        .1;
    assert!(read.get("webhook_secret").is_none());
    assert_eq!(read["has_secret"], true);
    let retain = f
        .request(
            Method::PUT,
            "/api/v1/agent/me/webhook",
            &a.api_key,
            Some(json!({"webhook_url":"https://example.com/receive"})),
        )
        .await
        .1;
    assert!(retain.get("webhook_secret").is_none());
    let updated = f
        .request(
            Method::PUT,
            "/api/v1/agent/me/webhook",
            &a.api_key,
            Some(json!({"webhook_url":"https://example.com/receive","webhook_secret":"explicit"})),
        )
        .await
        .1;
    assert_eq!(updated["webhook_secret"], "explicit");
    let cleared = f
        .request(
            Method::PUT,
            "/api/v1/agent/me/webhook",
            &a.api_key,
            Some(json!({"webhook_url":null})),
        )
        .await
        .1;
    assert_eq!(cleared["has_secret"], false);
    assert_eq!(f.me(&a.api_key).await["channel_status"], "offline");
    let dir = f.dir;
    f.server.shutdown().await.unwrap();
    drop(dir);
}

#[tokio::test]
async fn websocket_sse_reconnect_ack_connection_counts_and_revocation() {
    let f = Fixture::new().await;
    let a = register(&f.repo, "streamer", json!({}));
    let p = project(&f.repo, "p");
    bounty(&f.repo, &p, json!({}));
    let mut ws = f.websocket(&a.api_key, 0).await;
    let first = next_ws(&mut ws).await;
    assert_eq!(f.me(&a.api_key).await["channel_status"], "streaming");
    let mut sse = f
        .client
        .get(format!("{}/api/v1/agent/stream", f.server.info.base_url))
        .bearer_auth(&a.api_key)
        .send()
        .await
        .unwrap();
    assert_eq!(sse.headers()["Content-Type"], "text/event-stream");
    let mut buffer = String::new();
    assert_eq!(next_sse(&mut sse, &mut buffer).await, first);
    bounty(&f.repo, &p, json!({"title":"live"}));
    let second = next_ws(&mut ws).await;
    assert!(second["event_id"].as_i64() > first["event_id"].as_i64());
    assert_eq!(next_sse(&mut sse, &mut buffer).await, second);
    assert!(f.me(&a.api_key).await["last_pushed_at"].is_string());
    ws.send(Message::Text(
        json!({"type":"ack","event_id":first["event_id"]})
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if inbox(&f.repo, &a.agent.id).acked_event_id == first["event_id"].as_i64().unwrap() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    ws.close(None).await.unwrap();
    drop(ws);
    assert_eq!(f.me(&a.api_key).await["stream_connected"], true);
    let mut ws = f
        .websocket(&a.api_key, first["event_id"].as_i64().unwrap())
        .await;
    assert_eq!(next_ws(&mut ws).await, second);
    drop(sse);
    let mut sse = f
        .client
        .get(format!("{}/api/v1/agent/stream", f.server.info.base_url))
        .bearer_auth(&a.api_key)
        .header("Last-Event-ID", first["event_id"].as_i64().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(next_sse(&mut sse, &mut String::new()).await, second);
    bounty(&f.repo, &p, json!({"title":"after reconnect"}));
    let third = next_ws(&mut ws).await;
    assert!(third["event_id"].as_i64() > second["event_id"].as_i64());
    assert_eq!(next_sse(&mut sse, &mut String::new()).await, third);
    let disabled = f
        .request(
            Method::POST,
            &format!("/api/v1/agents/{}/disable", a.agent.id),
            "admin",
            None,
        )
        .await
        .1;
    assert_eq!(disabled["stream_connected"], false);
    assert_eq!(disabled["channel_status"], "offline");
    let closed = tokio::time::timeout(Duration::from_secs(3), ws.next())
        .await
        .unwrap();
    assert!(matches!(closed, None | Some(Ok(Message::Close(_)))));
    assert!(tokio::time::timeout(Duration::from_secs(3), sse.chunk())
        .await
        .unwrap()
        .unwrap()
        .is_none());
    for path in ["/api/v1/agent/stream", "/api/v1/agent/ws"] {
        assert_eq!(
            f.request(Method::GET, path, &a.api_key, None).await.0,
            StatusCode::FORBIDDEN
        );
    }
    f.request(
        Method::POST,
        &format!("/api/v1/agents/{}/enable", a.agent.id),
        "admin",
        None,
    )
    .await;
    let mut ws = f
        .websocket(&a.api_key, third["event_id"].as_i64().unwrap())
        .await;
    f.request(
        Method::POST,
        &format!("/api/v1/agents/{}/revoke", a.agent.id),
        "admin",
        None,
    )
    .await;
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(3), ws.next())
            .await
            .unwrap(),
        None | Some(Ok(Message::Close(_)))
    ));
    assert_eq!(
        f.request(Method::GET, "/api/v1/agent/stream", &a.api_key, None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    // shutdown 必须主动关闭仍在线的流，不能等客户端主动断开。
    let c = register(&f.repo, "shutdown", json!({}));
    let _open = f.websocket(&c.api_key, 0).await;
    tokio::time::timeout(Duration::from_secs(3), f.server.shutdown())
        .await
        .unwrap()
        .unwrap();
}

#[derive(Clone)]
struct ReceiverState {
    sender: tokio::sync::mpsc::UnboundedSender<(axum::http::HeaderMap, String, std::time::Instant)>,
    attempts: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    failures: usize,
}
async fn receive_webhook(
    axum::extract::State(state): axum::extract::State<ReceiverState>,
    headers: axum::http::HeaderMap,
    body: String,
) -> axum::http::StatusCode {
    state
        .sender
        .send((headers, body, std::time::Instant::now()))
        .unwrap();
    if state
        .attempts
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        < state.failures
    {
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    } else {
        axum::http::StatusCode::NO_CONTENT
    }
}
fn reference_signature(secret: &str, message: &str) -> String {
    use sha2::{Digest, Sha256};
    let key = if secret.len() > 64 {
        Sha256::digest(secret.as_bytes()).to_vec()
    } else {
        secret.as_bytes().to_vec()
    };
    let mut block = key;
    block.resize(64, 0);
    let mut inner = block.iter().map(|b| b ^ 0x36).collect::<Vec<_>>();
    inner.extend_from_slice(message.as_bytes());
    let mut outer = block.iter().map(|b| b ^ 0x5c).collect::<Vec<_>>();
    outer.extend_from_slice(&Sha256::digest(inner));
    format!("sha256={:x}", Sha256::digest(outer))
}

type WebhookRequest = (axum::http::HeaderMap, String, std::time::Instant);

struct WebhookReceiver {
    url: String,
    receiver: tokio::sync::mpsc::UnboundedReceiver<WebhookRequest>,
    task: tokio::task::JoinHandle<()>,
}
impl WebhookReceiver {
    async fn start(failures: usize) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/hook", listener.local_addr().unwrap());
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let app = axum::Router::new()
            .route("/hook", axum::routing::post(receive_webhook))
            .with_state(ReceiverState {
                sender,
                attempts: Default::default(),
                failures,
            });
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            url,
            receiver,
            task,
        }
    }

    async fn next(&mut self) -> WebhookRequest {
        tokio::time::timeout(Duration::from_secs(5), self.receiver.recv())
            .await
            .expect("Webhook delivery timed out")
            .expect("Webhook receiver closed")
    }

    async fn next_event(&mut self) -> AgentEvent {
        serde_json::from_str(&self.next().await.1).unwrap()
    }
}
impl Drop for WebhookReceiver {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn wait_delivery(
    conn: &rusqlite::Connection,
    event_id: i64,
    expected_status: &str,
    expected_attempts: i64,
) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let (status, attempts): (String, i64) = conn
                .query_row(
                    "SELECT delivery_status,attempt_count FROM agent_events WHERE event_id=?1",
                    [event_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            if status == expected_status && attempts == expected_attempts {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Persisted Webhook delivery state timed out");
}

#[tokio::test]
async fn standalone_binary_delivers_signed_webhook_without_preinstalled_crypto_provider() {
    use std::io::{BufRead, BufReader};

    // 子进程拥有独立的 rustls 全局状态；断言失败时也必须回收进程，随后才能删除数据库目录。
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("standalone.db");
    let mut hook = WebhookReceiver::start(0).await;
    let mut child = Child(
        std::process::Command::new(env!("CARGO_BIN_EXE_taskrail-server"))
            .args([
                "--bind",
                "127.0.0.1:0",
                "--admin-token",
                "binary-admin",
                "--db",
            ])
            .arg(&db_path)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stdout = child.0.stdout.take().unwrap();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
        let _ = sender.send(result);
    });
    let line = tokio::time::timeout(Duration::from_secs(15), receiver)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let base = line
        .trim()
        .strip_prefix("Taskrail server listening on ")
        .unwrap();

    // 父进程仅为测试 HTTP 请求显式配置局部 TLS，不安装进程默认提供者，也不影响子进程。
    let tls = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(rustls::RootCertStore::empty())
    .with_no_client_auth();
    let client = Client::builder()
        .tls_backend_preconfigured(tls)
        .timeout(Duration::from_secs(5))
        .no_proxy()
        .build()
        .unwrap();
    let response = client
        .patch(format!("{base}/api/v1/admin/onboarding"))
        .bearer_auth("binary-admin")
        .header("Content-Type", "application/json")
        .body(json!({"lan_open_registration":true}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = client
        .post(format!("{base}/api/v1/agents/register"))
        .header("Content-Type", "application/json")
        .body(json!({"name":"binary-hook","webhook_url":hook.url}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let registered: Value = serde_json::from_str(&response.text().await.unwrap()).unwrap();
    let response = client
        .post(format!("{base}/api/v1/projects"))
        .bearer_auth("binary-admin")
        .header("Content-Type", "application/json")
        .body(json!({"name":"binary-project"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let project: Value = serde_json::from_str(&response.text().await.unwrap()).unwrap();
    let response = client.post(format!("{base}/api/v1/bounties"))
        .bearer_auth("binary-admin").header("Content-Type", "application/json")
        .body(json!({"project_id":project["id"],"title":"binary-task","difficulty":"C","assignee_mode":"any","status":"DRAFT"}).to_string())
        .send().await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let draft: Value = serde_json::from_str(&response.text().await.unwrap()).unwrap();
    let response = client
        .post(format!(
            "{base}/api/v1/bounties/{}/publish",
            draft["id"].as_str().unwrap()
        ))
        .bearer_auth("binary-admin")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let (headers, body, _) = hook.next().await;
    let event: AgentEvent = serde_json::from_str(&body).unwrap();
    assert_eq!(event.event_type, "task.published");
    assert_eq!(event.bounty_id.as_deref(), draft["id"].as_str());
    assert_eq!(headers["X-Taskrail-Event"], "task.published");
    assert_eq!(headers["X-Taskrail-Event-Id"], event.event_id.to_string());
    let timestamp = headers["X-Taskrail-Timestamp"].to_str().unwrap();
    assert_eq!(
        headers["X-Taskrail-Signature"],
        reference_signature(
            registered["webhook_secret"].as_str().unwrap(),
            &format!("{timestamp}.{body}")
        )
    );
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    wait_delivery(&conn, event.event_id, "delivered", 1).await;
    assert!(child.0.try_wait().unwrap().is_none());
}

#[tokio::test]
async fn webhook_retry_preserves_fifo_without_blocking_other_agents_or_batch_capacity() {
    let f = Fixture::new().await;
    let mut first_hook = WebhookReceiver::start(1).await;
    let mut other_hook = WebhookReceiver::start(0).await;
    let p = project(&f.repo, "first-project");
    let other_p = project(&f.repo, "other-project");
    let a = register(&f.repo, "first", json!({"project_ids":[p.id]}));
    let b = register(
        &f.repo,
        "other",
        json!({"project_ids":[other_p.id],"webhook_url":other_hook.url}),
    );
    let open = bounty(&f.repo, &p, json!({}));
    f.repo.withdraw_bounty(&open.id, "human").unwrap();
    let expected = inbox(&f.repo, &a.agent.id).events;
    let conn = rusqlite::Connection::open(f.repo.path()).unwrap();
    // 固定队头第一次失败后的退避截止时间，避免依靠 100ms 内的线程调度断言 Agent 隔离。
    conn.execute_batch("CREATE TRIGGER hold_first_retry AFTER UPDATE OF next_attempt_at ON agent_events WHEN NEW.attempt_count=1 AND NEW.delivery_status='pending' BEGIN UPDATE agent_events SET next_attempt_at='2099-01-01T00:00:00.000Z' WHERE event_id=NEW.event_id; END;").unwrap();
    // 超过单轮 LIMIT 32 的同 Agent 积压不得挤占其他 Agent 的投递名额。
    conn.execute("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<40) INSERT INTO agent_events(agent_id,event_type,payload_json,created_at) SELECT ?1,'task.published','{\"project_id\":null,\"data\":{}}','old' FROM n", [&a.agent.id]).unwrap();
    f.repo
        .update_webhook(
            &a.agent.id,
            serde_json::from_value(json!({"webhook_url":first_hook.url})).unwrap(),
        )
        .unwrap();
    assert_eq!(first_hook.next_event().await.event_id, expected[0].event_id);
    wait_delivery(&conn, expected[0].event_id, "pending", 1).await;
    let other_open = bounty(&f.repo, &other_p, json!({}));
    assert_eq!(
        other_hook.next_event().await.bounty_id.as_deref(),
        Some(other_open.id.as_str())
    );
    wait_delivery(
        &conn,
        inbox(&f.repo, &b.agent.id).events[0].event_id,
        "delivered",
        1,
    )
    .await;
    assert!(
        tokio::time::timeout(Duration::from_millis(80), first_hook.receiver.recv())
            .await
            .is_err(),
        "later events bypassed the retrying queue head"
    );
    conn.execute_batch("DROP TRIGGER hold_first_retry;")
        .unwrap();
    conn.execute(
        "UPDATE agent_events SET next_attempt_at='2000-01-01T00:00:00.000Z' WHERE event_id=?1",
        [expected[0].event_id],
    )
    .unwrap();
    let published = first_hook.next_event().await;
    let withdrawn = first_hook.next_event().await;
    assert_eq!(
        [published.event_type.as_str(), withdrawn.event_type.as_str()],
        ["task.published", "task.withdrawn"]
    );
    assert_eq!(
        [published.event_id, withdrawn.event_id],
        [expected[0].event_id, expected[1].event_id]
    );
    wait_delivery(&conn, published.event_id, "delivered", 2).await;
    wait_delivery(&conn, withdrawn.event_id, "delivered", 1).await;
    f.server.shutdown().await.unwrap();
}

#[tokio::test]
async fn webhook_failed_head_releases_successor_and_inbox_recovers_gap_before_cumulative_ack() {
    let f = Fixture::new().await;
    let mut hook = WebhookReceiver::start(5).await;
    let a = register(&f.repo, "recover-gap", json!({}));
    let p = project(&f.repo, "p");
    let baseline = bounty(&f.repo, &p, json!({}));
    let baseline_id = inbox(&f.repo, &a.agent.id).events[0].event_id;
    let (status, ack) = f
        .request(
            Method::POST,
            "/api/v1/agent/events/ack",
            &a.api_key,
            Some(json!({"event_id":baseline_id})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ack["acked_event_id"], baseline_id);
    // 先确认本地连续处理游标，再产生断线期间的发布和撤回事件。
    let open = bounty(&f.repo, &p, json!({}));
    assert_ne!(baseline.id, open.id);
    f.repo.withdraw_bounty(&open.id, "human").unwrap();
    let expected = inbox(&f.repo, &a.agent.id).events;
    let conn = rusqlite::Connection::open(f.repo.path()).unwrap();
    // 已由其他通道处理的基线事件不参与本次回调重试，保留两条待验收事件。
    conn.execute(
        "UPDATE agent_events SET delivery_status='delivered' WHERE event_id=?1",
        [baseline_id],
    )
    .unwrap();
    f.repo
        .update_webhook(
            &a.agent.id,
            serde_json::from_value(json!({"webhook_url":hook.url})).unwrap(),
        )
        .unwrap();
    for _ in 0..5 {
        let event = hook.next_event().await;
        assert_eq!(event.event_id, expected[0].event_id);
        assert_eq!(event.event_type, "task.published");
    }
    let successor = hook.next_event().await;
    assert_eq!(successor.event_id, expected[1].event_id);
    assert_eq!(successor.event_type, "task.withdrawn");
    wait_delivery(&conn, expected[0].event_id, "failed", 5).await;
    wait_delivery(&conn, successor.event_id, "delivered", 1).await;
    assert!(successor.event_id > baseline_id + 1);

    // 重新启动服务模拟断线恢复。Ack 仍停留在本地连续处理游标，Inbox 必须补回失败队头。
    let db_path = f.repo.path().to_path_buf();
    f.server.shutdown().await.unwrap();
    let restarted = server::start(ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        db_path,
        admin_token: "admin".into(),
        mode: ServerMode::Standalone,
    })
    .await
    .unwrap();
    let response = f
        .client
        .get(format!(
            "{}/api/v1/agent/events?cursor={baseline_id}&limit=1",
            restarted.info.base_url
        ))
        .bearer_auth(&a.api_key)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let page: AgentInbox = serde_json::from_str(&response.text().await.unwrap()).unwrap();
    assert_eq!(page.acked_event_id, baseline_id);
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].event_id, expected[0].event_id);
    assert_eq!(page.events[0].event_type, "task.published");
    assert!(page.has_more);
    let response = f
        .client
        .get(format!(
            "{}/api/v1/agent/events?cursor={}&limit=1",
            restarted.info.base_url, page.cursor
        ))
        .bearer_auth(&a.api_key)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let page: AgentInbox = serde_json::from_str(&response.text().await.unwrap()).unwrap();
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].event_id, successor.event_id);
    assert_eq!(page.events[0].event_type, "task.withdrawn");
    assert!(!page.has_more);
    // 两页事件已按序处理后才提交累计确认，随后较小的 Ack 不得回退游标。
    for event_id in [successor.event_id, baseline_id] {
        let response = f
            .client
            .post(format!(
                "{}/api/v1/agent/events/ack",
                restarted.info.base_url
            ))
            .bearer_auth(&a.api_key)
            .header("Content-Type", "application/json")
            .body(json!({"event_id":event_id}).to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let ack: Value = serde_json::from_str(&response.text().await.unwrap()).unwrap();
        assert_eq!(ack["acked_event_id"], successor.event_id);
    }
    let response = f
        .client
        .get(format!("{}/api/v1/agent/events", restarted.info.base_url))
        .bearer_auth(&a.api_key)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let page: AgentInbox = serde_json::from_str(&response.text().await.unwrap()).unwrap();
    assert!(page.events.is_empty());
    assert_eq!(page.acked_event_id, successor.event_id);
    restarted.shutdown().await.unwrap();
}

#[tokio::test]
async fn signed_webhooks_retry_then_succeed_and_stop_for_disabled_revoked_agents() {
    let f = Fixture::new().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/hook", listener.local_addr().unwrap());
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let app = axum::Router::new()
        .route("/hook", axum::routing::post(receive_webhook))
        .with_state(ReceiverState {
            sender,
            attempts: Default::default(),
            failures: 2,
        });
    let receiver_task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let secret = "x".repeat(100); // 覆盖 RFC 2104 长密钥先散列的分支。
    let a = register(
        &f.repo,
        "webhook",
        json!({"webhook_url":url,"webhook_secret":secret}),
    );
    let p = project(&f.repo, "p");
    let open = bounty(&f.repo, &p, json!({}));
    let mut received = Vec::new();
    for _ in 0..3 {
        let (headers, body, time) = tokio::time::timeout(Duration::from_secs(4), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        let event: AgentEvent = serde_json::from_str(&body).unwrap();
        assert_eq!(event.bounty_id.as_deref(), Some(open.id.as_str()));
        assert_eq!(headers["X-Taskrail-Event"], "task.published");
        assert_eq!(headers["X-Taskrail-Event-Id"], event.event_id.to_string());
        let timestamp = headers["X-Taskrail-Timestamp"].to_str().unwrap();
        assert!((chrono::Utc::now().timestamp() - timestamp.parse::<i64>().unwrap()).abs() < 10);
        assert_eq!(
            headers["X-Taskrail-Signature"],
            reference_signature(&secret, &format!("{timestamp}.{body}"))
        );
        for forbidden in ["api_key_hash", "webhook_secret", "mmr"] {
            assert!(!body.contains(forbidden));
        }
        received.push((event, time));
    }
    assert_eq!(received[0].0.event_id, received[2].0.event_id);
    assert!(received[1].1.duration_since(received[0].1) >= Duration::from_millis(90));
    assert!(received[2].1.duration_since(received[1].1) >= Duration::from_millis(190));
    let conn = rusqlite::Connection::open(f.repo.path()).unwrap();
    tokio::time::timeout(Duration::from_secs(3),async{loop{
        let (status,attempts,delivered):(String,i64,Option<String>)=conn.query_row("SELECT delivery_status,attempt_count,delivered_at FROM agent_events WHERE agent_id=?1",[&a.agent.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        if status=="delivered" {assert_eq!(attempts,3);assert!(delivered.is_some());break;}
        tokio::time::sleep(Duration::from_millis(10)).await;
    }}).await.unwrap();
    let me = f.me(&a.api_key).await;
    assert_eq!(me["channel_status"], "webhook");
    assert!(me["last_pushed_at"].is_string());
    assert_eq!(
        f.request(Method::GET, "/api/v1/agent/me/webhook", &a.api_key, None)
            .await
            .1["recent_failed_count"],
        0
    );
    // Webhook 成功不隐式确认 Inbox，Agent 仍须显式 Ack。
    assert_eq!(inbox(&f.repo, &a.agent.id).events.len(), 1);
    let mut ws = f.websocket(&a.api_key, received[0].0.event_id).await;
    assert_eq!(f.me(&a.api_key).await["channel_status"], "streaming");
    ws.close(None).await.unwrap();
    drop(ws);
    f.request(
        Method::POST,
        &format!("/api/v1/agents/{}/disable", a.agent.id),
        "admin",
        None,
    )
    .await;
    bounty(&f.repo, &p, json!({"title":"disabled"}));
    // 人工注入已排队事件，验证 Dispatcher 除分发层之外仍执行状态校验。
    conn.execute("INSERT INTO agent_events(agent_id,event_type,payload_json,created_at) VALUES(?1,'task.published','{}','old')",[&a.agent.id]).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(250), receiver.recv())
            .await
            .is_err()
    );
    f.request(
        Method::POST,
        &format!("/api/v1/agents/{}/revoke", a.agent.id),
        "admin",
        None,
    )
    .await;
    bounty(&f.repo, &p, json!({"title":"revoked"}));
    assert!(
        tokio::time::timeout(Duration::from_millis(250), receiver.recv())
            .await
            .is_err()
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM agent_events WHERE agent_id=?1",
            [&a.agent.id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    f.server.shutdown().await.unwrap();
    receiver_task.abort();
}

#[tokio::test]
async fn webhook_exhaustion_is_persistent() {
    let f = Fixture::new().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/hook", listener.local_addr().unwrap());
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let app = axum::Router::new()
        .route("/hook", axum::routing::post(receive_webhook))
        .with_state(ReceiverState {
            sender,
            attempts: Default::default(),
            failures: usize::MAX,
        });
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let a = register(&f.repo, "failure", json!({"webhook_url":url}));
    let p = project(&f.repo, "p");
    bounty(&f.repo, &p, json!({}));
    for _ in 0..5 {
        tokio::time::timeout(Duration::from_secs(4), receiver.recv())
            .await
            .unwrap()
            .unwrap();
    }
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let config = f.repo.webhook_config(&a.agent.id).unwrap();
            let status: String = rusqlite::Connection::open(f.repo.path())
                .unwrap()
                .query_row("SELECT delivery_status FROM agent_events", [], |r| r.get(0))
                .unwrap();
            if status == "failed" {
                assert_eq!(config.recent_failed_count, 1);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let db = f.repo.path().to_path_buf();
    f.server.shutdown().await.unwrap();
    task.abort();
    let reopened = Repository::open(db).unwrap();
    assert_eq!(inbox(&reopened, &a.agent.id).events.len(), 1);
}

#[test]
fn dispatch_and_withdrawal_roll_back_together_and_republish_reselects_recipients() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::open(dir.path().join("atomic.db")).unwrap();
    repo.onboarding(Some(UpdateOnboarding {
        lan_open_registration: true,
    }))
    .unwrap();
    let p = project(&repo, "p");
    let a = register(&repo, "a", json!({}));
    let b = register(&repo, "b", json!({"subscribe_all_projects":false}));
    let draft = bounty(&repo, &p, json!({"status":"DRAFT"}));
    let conn = rusqlite::Connection::open(repo.path()).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_inbox BEFORE INSERT ON agent_events BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
    assert!(repo.publish_bounty(&draft.id, "human").is_err());
    assert_eq!(
        repo.get_bounty_detail(&draft.id).unwrap().bounty.status,
        BountyStatus::Draft
    );
    assert!(inbox(&repo, &a.agent.id).events.is_empty());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM bounty_dispatches", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    conn.execute_batch("DROP TRIGGER reject_inbox;").unwrap();
    repo.publish_bounty(&draft.id, "human").unwrap();
    assert_eq!(inbox(&repo, &a.agent.id).events.len(), 1);
    repo.update_agent_subscriptions(
        &a.agent.id,
        serde_json::from_value(json!({"subscribe_all_projects":false})).unwrap(),
    )
    .unwrap();
    repo.update_agent_subscriptions(&b.agent.id, AgentSubscriptions::default())
        .unwrap();
    conn.execute_batch("CREATE TRIGGER reject_inbox BEFORE INSERT ON agent_events BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
    assert!(repo.withdraw_bounty(&draft.id, "human").is_err());
    assert_eq!(
        repo.get_bounty_detail(&draft.id).unwrap().bounty.status,
        BountyStatus::Open
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM bounty_dispatches", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    conn.execute_batch("DROP TRIGGER reject_inbox;").unwrap();
    repo.withdraw_bounty(&draft.id, "human").unwrap();
    assert_eq!(inbox(&repo, &a.agent.id).events.len(), 2);
    assert!(inbox(&repo, &b.agent.id).events.is_empty());
    repo.publish_bounty(&draft.id, "human").unwrap();
    repo.withdraw_bounty(&draft.id, "human").unwrap();
    assert_eq!(inbox(&repo, &a.agent.id).events.len(), 2);
    assert_eq!(inbox(&repo, &b.agent.id).events.len(), 2);
    repo.set_agent_status(&b.agent.id, AgentStatus::Disabled)
        .unwrap();
    let hidden = bounty(
        &repo,
        &p,
        json!({"assignee_mode":"specific","assignee_agent_id":b.agent.id}),
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM bounty_dispatches WHERE bounty_id=?1",
            [hidden.id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[tokio::test]
async fn large_backlog_crosses_page_boundary_without_duplicates_and_releases_last_connection() {
    let f = Fixture::new().await;
    let a = register(&f.repo, "burst", json!({}));
    let conn = rusqlite::Connection::open(f.repo.path()).unwrap();
    conn.execute("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<503) INSERT INTO agent_events(agent_id,event_type,payload_json,created_at) SELECT ?1,'task.published','{\"project_id\":null,\"data\":{}}','old' FROM n",[&a.agent.id]).unwrap();
    let page = f
        .repo
        .agent_events(&a.agent.id, Some(0), usize::MAX)
        .unwrap();
    assert_eq!(page.events.len(), 500);
    assert!(page.has_more);
    let mut ws = f.websocket(&a.api_key, 0).await;
    for expected in 1..=503 {
        assert_eq!(next_ws(&mut ws).await["event_id"], expected);
    }
    assert!(tokio::time::timeout(Duration::from_millis(100), ws.next())
        .await
        .is_err());
    ws.send(Message::Text(
        json!({"type":"ack","event_id":-1}).to_string().into(),
    ))
    .await
    .unwrap();
    assert_eq!(next_ws(&mut ws).await["error"]["code"], "VALIDATION_ERROR");
    ws.close(None).await.unwrap();
    drop(ws);
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if f.me(&a.api_key).await["channel_status"] == "offline" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let (status, directory) = f
        .request(Method::GET, "/api/v1/agents", "admin", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(directory[0]["stream_connected"], false);
    f.server.shutdown().await.unwrap();
}

#[tokio::test]
async fn shutdown_cancels_inflight_webhook_and_restart_delivers_persistent_backlog() {
    let f = Fixture::new().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/hook", listener.local_addr().unwrap());
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let app = axum::Router::new().route(
        "/hook",
        axum::routing::post(move || {
            let sender = sender.clone();
            async move {
                sender.send(()).unwrap();
                std::future::pending::<()>().await;
                StatusCode::NO_CONTENT
            }
        }),
    );
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let a = register(&f.repo, "restart", json!({"webhook_url":url}));
    let p = project(&f.repo, "p");
    bounty(&f.repo, &p, json!({}));
    tokio::time::timeout(Duration::from_secs(3), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    let path = f.repo.path().to_path_buf();
    tokio::time::timeout(Duration::from_secs(2), f.server.shutdown())
        .await
        .unwrap()
        .unwrap();
    task.abort();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/hook", listener.local_addr().unwrap());
    f.repo
        .update_webhook(
            &a.agent.id,
            serde_json::from_value(json!({"webhook_url":url})).unwrap(),
        )
        .unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let app = axum::Router::new()
        .route("/hook", axum::routing::post(receive_webhook))
        .with_state(ReceiverState {
            sender,
            attempts: Default::default(),
            failures: 0,
        });
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let server = server::start(ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        db_path: path,
        admin_token: "admin".into(),
        mode: ServerMode::Standalone,
    })
    .await
    .unwrap();
    let (_, body, _) = tokio::time::timeout(Duration::from_secs(3), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    let event: AgentEvent = serde_json::from_str(&body).unwrap();
    assert_eq!(
        event.event_id,
        inbox(&f.repo, &a.agent.id).events[0].event_id
    );
    server.shutdown().await.unwrap();
    task.abort();
}

#[test]
fn v3_migration_failure_rolls_back_columns_and_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken-v2.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(include_str!("fixtures/server_v1.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/server/schema_v2.sql"))
        .unwrap();
    conn.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL);INSERT INTO schema_migrations VALUES(2,'old'); CREATE TABLE agent_events(conflict TEXT);").unwrap();
    assert!(Repository::open(&path).is_err());
    assert_eq!(
        conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM pragma_table_info('agents') WHERE name IN ('webhook_url','webhook_secret','acked_event_id','last_pushed_at')",[],|r|r.get::<_,i64>(0)).unwrap(),0);
}
