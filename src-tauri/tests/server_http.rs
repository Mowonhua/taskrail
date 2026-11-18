#![cfg(not(target_os = "android"))]
//! 文件职责：通过真实本地 TCP 验证鉴权、JSON 契约、CORS 与仓储状态转换。
use reqwest::{Client, Method, StatusCode};
use serde_json::{json, Value};
use taskrail_lib::server::{self, db::Repository, ServerConfig, ServerMode};

async fn request(
    client: &Client,
    base: &str,
    method: Method,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = client.request(method, format!("{base}/api/v1{path}"));
    if let Some(token) = token {
        req = req.bearer_auth(token);
    }
    if let Some(body) = body {
        req = req
            .header("Content-Type", "application/json")
            .body(body.to_string());
    }
    let response = req.send().await.unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    let value = if text.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text).unwrap()
    };
    (status, value)
}

#[tokio::test]
async fn http_auth_cors_health_and_validation() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let running = server::start(ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        db_path: dir.path().join("taskrail.db"),
        admin_token: "secret".into(),
        mode: ServerMode::Embedded,
    })
    .await
    .unwrap();
    let base = &running.info.base_url;
    let client = Client::new();
    let (status, health) = request(&client, base, Method::GET, "/health", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        health,
        json!({"ok":true,"mode":"embedded","version":env!("CARGO_PKG_VERSION")})
    );
    for path in [
        "/admin/status",
        "/admin/unknown",
        "/projects",
        "/projects/missing",
        "/bounties",
        "/bounties/missing",
        "/bounties/missing/withdraw",
    ] {
        for token in [None, Some("wrong"), Some("")] {
            let (status, error) = request(&client, base, Method::GET, path, token, None).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
            assert_eq!(error["error"]["code"], "UNAUTHORIZED");
        }
    }
    let (status, admin) = request(
        &client,
        base,
        Method::GET,
        "/admin/status",
        Some("secret"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(admin["db_path"], running.info.db_path);
    assert_eq!(admin["mode"], "embedded");
    assert_eq!(admin["ok"], true);
    assert!(admin.get("admin_token").is_none());
    let preflight = client
        .request(Method::OPTIONS, format!("{base}/api/v1/bounties"))
        .header("Origin", "tauri://localhost")
        .header("Access-Control-Request-Method", "POST")
        .header(
            "Access-Control-Request-Headers",
            "authorization,content-type",
        )
        .send()
        .await
        .unwrap();
    assert!(preflight.status().is_success());
    assert_eq!(preflight.headers()["access-control-allow-origin"], "*");
    let allowed_headers = preflight.headers()["access-control-allow-headers"]
        .to_str()
        .unwrap();
    assert!(allowed_headers.contains("authorization"));
    assert!(allowed_headers.contains("content-type"));
    let unauthorized = client
        .get(format!("{base}/api/v1/projects"))
        .header("Origin", "http://localhost:1420")
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(unauthorized.headers()["access-control-allow-origin"], "*");
    for body in [
        json!({"name":" "}),
        json!({"name":"ok","icon":"bad"}),
        json!({"name":"ok","unknown":true}),
    ] {
        let (status, error) = request(
            &client,
            base,
            Method::POST,
            "/projects",
            Some("secret"),
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error["error"]["code"], "VALIDATION_ERROR");
    }
    for path in [
        "/projects?include_archived=bad",
        "/bounties?status=UNKNOWN",
        "/bounties?include_drafts=bad",
        "/bounties/%FF",
    ] {
        let (status, error) = request(&client, base, Method::GET, path, Some("secret"), None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error["error"]["code"], "VALIDATION_ERROR");
    }
    let bad_json = client
        .post(format!("{base}/api/v1/projects"))
        .bearer_auth("secret")
        .header("Content-Type", "application/json")
        .body("{")
        .send()
        .await
        .unwrap();
    assert_eq!(bad_json.status(), StatusCode::BAD_REQUEST);
    let error: Value = serde_json::from_str(&bad_json.text().await.unwrap()).unwrap();
    assert_eq!(error["error"]["code"], "VALIDATION_ERROR");
    let (status, error) = request(&client, base, Method::GET, "/no-such-route", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(error["error"]["code"], "NOT_FOUND");
    let (status, error) = request(
        &client,
        base,
        Method::PUT,
        "/projects",
        Some("secret"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(error["error"]["code"], "METHOD_NOT_ALLOWED");
    running.shutdown().await.unwrap();
}

#[tokio::test]
async fn http_delete_cancelled_bounty_preserves_dependencies() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let running = server::start(ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        db_path: dir.path().join("taskrail.db"),
        admin_token: "secret".into(),
        mode: ServerMode::Standalone,
    })
    .await
    .unwrap();
    let base = &running.info.base_url;
    let client = Client::new();
    let auth = Some("secret");
    let (status, project) = request(
        &client,
        base,
        Method::POST,
        "/projects",
        auth,
        Some(json!({"name":"发布"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let create = |title: &'static str, blocked_by: Vec<Value>| {
        request(
            &client,
            base,
            Method::POST,
            "/bounties",
            auth,
            Some(json!({
                "project_id":project["id"], "title":title, "difficulty":"C",
                "assignee_mode":"any", "status":"OPEN", "blocked_by":blocked_by
            })),
        )
    };
    let (status, prerequisite) = create("前置任务", vec![]).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, dependent) = create("后续任务", vec![prerequisite["id"].clone()]).await;
    assert_eq!(status, StatusCode::CREATED);
    let prerequisite_path = format!("/bounties/{}", prerequisite["id"].as_str().unwrap());
    let dependent_path = format!("/bounties/{}", dependent["id"].as_str().unwrap());
    let (status, _) = request(
        &client,
        base,
        Method::DELETE,
        &prerequisite_path,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, error) = request(
        &client,
        base,
        Method::DELETE,
        &prerequisite_path,
        auth,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"]["code"], "INVALID_TRANSITION");

    let (status, _) = request(
        &client,
        base,
        Method::POST,
        &format!("{prerequisite_path}/cancel"),
        auth,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, error) = request(
        &client,
        base,
        Method::DELETE,
        &prerequisite_path,
        auth,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"]["code"], "BOUNTY_HAS_DEPENDENTS");
    let (_, detail) = request(&client, base, Method::GET, &dependent_path, auth, None).await;
    assert_eq!(detail["bounty"]["is_blocked"], true);
    assert_eq!(detail["bounty"]["blocked_by"], json!([prerequisite["id"]]));

    let (status, _) = request(
        &client,
        base,
        Method::POST,
        &format!("{dependent_path}/cancel"),
        auth,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    for path in [&dependent_path, &prerequisite_path] {
        let (status, body) = request(&client, base, Method::DELETE, path, auth, None).await;
        assert_eq!((status, body), (StatusCode::NO_CONTENT, Value::Null));
        let (status, _) = request(&client, base, Method::GET, path, auth, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    let (_, list) = request(
        &client,
        base,
        Method::GET,
        "/bounties?status=CANCELLED",
        auth,
        None,
    )
    .await;
    assert_eq!(list, json!([]));
    running.shutdown().await.unwrap();
}

#[tokio::test]
async fn http_project_crud_and_bounty_lifecycle() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("taskrail.db");
    let running = server::start(ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        db_path: db_path.clone(),
        admin_token: "secret".into(),
        mode: ServerMode::Standalone,
    })
    .await
    .unwrap();
    let repo = Repository::open(db_path).unwrap();
    let base = &running.info.base_url;
    let client = Client::new();
    let auth = Some("secret");
    let (status,p) = request(&client,base,Method::POST,"/projects",auth,Some(json!({"name":"内核","icon":"lucide:train","description":"project","workspace_path":"D:/repo","default_acceptance_criteria":"tests pass","arbitrator_chain":["judge"]}))).await;
    assert_eq!(status, StatusCode::CREATED);
    let pid = p["id"].as_str().unwrap();
    assert_eq!(p["arbitrator_chain"], json!(["judge"]));
    assert_eq!(p["agent_draft_policy"], "open");
    assert_eq!(p["archived"], false);
    let project_path = format!("/projects/{pid}");
    let (_, fetched) = request(&client, base, Method::GET, &project_path, auth, None).await;
    assert_eq!(fetched, p);
    let (_, updated) = request(
        &client,
        base,
        Method::PATCH,
        &project_path,
        auth,
        Some(json!({"name":"renamed","icon":null,"archived":true})),
    )
    .await;
    assert_eq!(updated["name"], "renamed");
    assert_eq!(updated["icon"], Value::Null);
    let (_, projects) = request(&client, base, Method::GET, "/projects", auth, None).await;
    assert_eq!(projects, json!([]));
    let (_, projects) = request(
        &client,
        base,
        Method::GET,
        "/projects?include_archived=true",
        auth,
        None,
    )
    .await;
    assert_eq!(projects.as_array().unwrap().len(), 1);
    let creation = json!({"project_id":pid,"title":"草稿","description":"markdown **text**","acceptance_criteria":"pass","difficulty":"B","assignee_mode":"any","status":"DRAFT","require_arbitration":true});
    let (status, error) = request(
        &client,
        base,
        Method::POST,
        "/bounties",
        auth,
        Some(creation.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"]["code"], "PROJECT_ARCHIVED");
    request(
        &client,
        base,
        Method::PATCH,
        &project_path,
        auth,
        Some(json!({"archived":false})),
    )
    .await;
    let (status, b) = request(
        &client,
        base,
        Method::POST,
        "/bounties",
        auth,
        Some(creation),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(b["status"], "DRAFT");
    assert_eq!(b["lease_id"], 0);
    let bid = b["id"].as_str().unwrap();
    let path = format!("/bounties/{bid}");
    let (_, hidden) = request(&client, base, Method::GET, "/bounties", auth, None).await;
    assert_eq!(hidden, json!([]));
    let (_, listed) = request(
        &client,
        base,
        Method::GET,
        &format!("/bounties?include_drafts=true&status=DRAFT&project_id={pid}&q=pass"),
        auth,
        None,
    )
    .await;
    assert_eq!(listed.as_array().unwrap().len(), 1);
    let (_, detail) = request(&client, base, Method::GET, &path, auth, None).await;
    assert_eq!(detail["bounty"], b);
    assert_eq!(detail["events"][0]["event_type"], "created");
    let (status, b) = request(
        &client,
        base,
        Method::PATCH,
        &path,
        auth,
        Some(json!({"title":"修改草稿"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(b["title"], "修改草稿");
    let (status, b) = request(
        &client,
        base,
        Method::POST,
        &format!("{path}/publish"),
        auth,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(b["status"], "OPEN");
    assert!(b["published_at"].is_string());
    let (status, error) = request(
        &client,
        base,
        Method::PATCH,
        &path,
        auth,
        Some(json!({"title":"blocked"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"]["code"], "INVALID_TRANSITION");
    let (status, b) = request(
        &client,
        base,
        Method::POST,
        &format!("{path}/withdraw"),
        auth,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(b["status"], "DRAFT");
    assert!(b["withdrawn_at"].is_string());
    request(
        &client,
        base,
        Method::POST,
        &format!("{path}/publish"),
        auth,
        None,
    )
    .await;
    repo.onboarding(Some(server::models::UpdateOnboarding {
        lan_open_registration: true,
    }))
    .unwrap();
    let agent = repo
        .register_agent(serde_json::from_value(json!({"name":"agent"})).unwrap())
        .unwrap()
        .agent;
    repo.agent_claim_bounty(bid, &agent.id).unwrap();
    let (status, error) = request(
        &client,
        base,
        Method::POST,
        &format!("{path}/withdraw"),
        auth,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"]["code"], "BOUNTY_ALREADY_CLAIMED");
    let (status, error) = request(&client, base, Method::DELETE, &project_path, auth, None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"]["code"], "PROJECT_HAS_BOUNTIES");
    let (_, detail) = request(&client, base, Method::GET, &path, auth, None).await;
    let events = detail["events"].as_array().unwrap();
    assert_eq!(
        events
            .iter()
            .map(|e| e["event_type"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "created",
            "updated",
            "published",
            "withdrawn",
            "published",
            "claimed"
        ]
    );
    assert_eq!(events[2]["from_status"], "DRAFT");
    assert_eq!(events[2]["to_status"], "OPEN");
    for status in ["DRAFT", "OPEN", "WAITING_INFO"] {
        let (_,b) = request(&client,base,Method::POST,"/bounties",auth,Some(json!({"project_id":pid,"title":"cancel","difficulty":"C","assignee_mode":"any","status":"DRAFT"}))).await;
        let id = b["id"].as_str().unwrap();
        if status != "DRAFT" {
            rusqlite::Connection::open(repo.path())
                .unwrap()
                .execute("UPDATE bounties SET status=?2 WHERE id=?1", [id, status])
                .unwrap();
        }
        let (code, b) = request(
            &client,
            base,
            Method::POST,
            &format!("/bounties/{id}/cancel"),
            auth,
            None,
        )
        .await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(b["status"], "CANCELLED");
        assert!(b["cancelled_at"].is_string());
    }
    for (path, code) in [
        ("/projects/missing", "PROJECT_NOT_FOUND"),
        ("/bounties/missing", "BOUNTY_NOT_FOUND"),
    ] {
        let (status, error) = request(&client, base, Method::GET, path, auth, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(error["error"]["code"], code);
    }
    let (_, p) = request(
        &client,
        base,
        Method::POST,
        "/projects",
        auth,
        Some(json!({"name":"empty"})),
    )
    .await;
    let (status, body) = request(
        &client,
        base,
        Method::DELETE,
        &format!("/projects/{}", p["id"].as_str().unwrap()),
        auth,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_null());
    running.shutdown().await.unwrap();
}

#[tokio::test]
async fn server_restart_preserves_data_changes_token_and_releases_port() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("taskrail.db");
    let first_token = server::random_admin_token().unwrap();
    let second_token = server::random_admin_token().unwrap();
    assert_eq!(first_token.len(), 64);
    assert!(first_token.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(first_token, second_token);
    let first = server::start(ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        db_path: path.clone(),
        admin_token: first_token.clone(),
        mode: ServerMode::Embedded,
    })
    .await
    .unwrap();
    let client = Client::new();
    let (_, p) = request(
        &client,
        &first.info.base_url,
        Method::POST,
        "/projects",
        Some(&first_token),
        Some(json!({"name":"persist"})),
    )
    .await;
    let bind = first
        .info
        .base_url
        .strip_prefix("http://")
        .unwrap()
        .parse()
        .unwrap();
    first.shutdown().await.unwrap();
    let second = server::start(ServerConfig {
        bind,
        db_path: path,
        admin_token: second_token.clone(),
        mode: ServerMode::Embedded,
    })
    .await
    .unwrap();
    let (status, _) = request(
        &client,
        &second.info.base_url,
        Method::GET,
        "/projects",
        Some(&first_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (_, projects) = request(
        &client,
        &second.info.base_url,
        Method::GET,
        "/projects",
        Some(&second_token),
        None,
    )
    .await;
    assert_eq!(projects[0], p);
    // 触发不可恢复的数据库错误，仍返回标准信封而非 SQL 内容。
    rusqlite::Connection::open(&second.info.db_path)
        .unwrap()
        .execute("DROP TABLE projects", [])
        .unwrap();
    let (status, error) = request(
        &client,
        &second.info.base_url,
        Method::GET,
        "/projects",
        Some(&second_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        error,
        json!({"error":{"code":"INTERNAL_ERROR","message":"服务内部错误"}})
    );
    second.run_until(async {}).await.unwrap();
}
