#![cfg(not(target_os = "android"))]
//! 文件职责：通过真实 HTTP 和数据库验证 Agent 准入、凭证与草稿隔离契约。
use reqwest::{Client, Method, StatusCode};
use serde_json::{json, Value};
use taskrail_lib::server::{self, ServerConfig, ServerMode};

async fn request(
    client: &Client,
    base: &str,
    method: Method,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = client.request(method, format!("{base}/api/v1{path}"));
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
    let text = response.text().await.unwrap();
    (
        status,
        if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or_else(|_| panic!("{status}: {text}"))
        },
    )
}

#[tokio::test]
async fn registration_credentials_and_draft_isolation() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let running = server::start(ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        db_path: dir.path().join("agents.db"),
        admin_token: "admin-secret-never-export".into(),
        mode: ServerMode::Standalone,
    })
    .await
    .unwrap();
    let base = &running.info.base_url;
    let client = Client::new();
    let admin = Some("admin-secret-never-export");
    let (status, error) = request(
        &client,
        base,
        Method::POST,
        "/agents/register",
        None,
        Some(json!({"name":"first"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(error["error"]["code"], "INVITE_TOKEN_REQUIRED");
    let (status, invite) = request(
        &client,
        base,
        Method::POST,
        "/admin/invite-tokens",
        admin,
        Some(json!({"max_uses":1})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let raw = invite["raw_token"].as_str().unwrap();
    let (status, registered) = request(
        &client,
        base,
        Method::POST,
        "/agents/register",
        Some(raw),
        Some(json!({"name":" first "})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(registered["agent"]["name"], "first");
    assert_eq!(registered["agent"]["tier"], "Unranked");
    assert_eq!(registered["agent"]["subscribe_all_projects"], true);
    assert!(registered["agent"].get("mmr").is_none());
    let key = registered["api_key"].as_str().unwrap();
    let agent_id = registered["agent"]["id"].as_str().unwrap();
    let (status, _) = request(
        &client,
        base,
        Method::POST,
        "/agents/register",
        Some(raw),
        Some(json!({"name":"second"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, me) = request(&client, base, Method::GET, "/agent/me", Some(key), None).await;
    assert_eq!(status, StatusCode::OK);
    for field in ["api_key", "api_key_hash", "mmr"] {
        assert!(me.get(field).is_none());
    }
    for path in [
        "/admin/onboarding",
        "/projects",
        "/bounties",
        "/agents",
        "/agents/unknown/enable",
    ] {
        assert_eq!(
            request(&client, base, Method::GET, path, Some(key), None)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
    }
    let (_, project) = request(
        &client,
        base,
        Method::POST,
        "/projects",
        admin,
        Some(json!({"name":"project"})),
    )
    .await;
    let project_id = project["id"].as_str().unwrap();
    let mut draft_id = String::new();
    for state in ["DRAFT", "OPEN"] {
        let (status, bounty) = request(&client, base, Method::POST, "/bounties", admin, Some(json!({"project_id":project_id,"title":"secret_search","difficulty":"C","assignee_mode":"any","status":state}))).await;
        assert_eq!(status, StatusCode::CREATED);
        if state == "DRAFT" {
            draft_id = bounty["id"].as_str().unwrap().to_owned();
        }
    }
    for query in [
        "",
        "?q=secret_search",
        "?include_drafts=true",
        "?status=DRAFT&q=secret_search",
    ] {
        let (status, list) = request(
            &client,
            base,
            Method::GET,
            &format!("/agent/bounties{query}"),
            Some(key),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(list
            .as_array()
            .unwrap()
            .iter()
            .all(|b| b["status"] != "DRAFT"));
    }
    assert_eq!(
        request(
            &client,
            base,
            Method::GET,
            &format!("/agent/bounties/{draft_id}"),
            Some(key),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    for (action, expected) in [
        ("disable", StatusCode::FORBIDDEN),
        ("enable", StatusCode::OK),
        ("revoke", StatusCode::UNAUTHORIZED),
    ] {
        assert_eq!(
            request(
                &client,
                base,
                Method::POST,
                &format!("/agents/{agent_id}/{action}"),
                admin,
                None
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            request(&client, base, Method::GET, "/agent/me", Some(key), None)
                .await
                .0,
            expected
        );
    }
    assert_eq!(
        request(
            &client,
            base,
            Method::POST,
            &format!("/agents/{agent_id}/enable"),
            admin,
            None
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    running.shutdown().await.unwrap();
}

#[tokio::test]
async fn onboarding_validation_subscriptions_and_directory_filters() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let running = server::start(ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        db_path: dir.path().join("agents.db"),
        admin_token: "admin".into(),
        mode: ServerMode::Standalone,
    })
    .await
    .unwrap();
    let client = Client::new();
    let base = &running.info.base_url;
    let admin = Some("admin");
    for input in [
        json!({"max_uses":0}),
        json!({"max_uses":1.5}),
        json!({"expires_in_hours":0}),
        json!({"expires_in_hours":-1}),
        json!({"expires_in_hours":1e300}),
        json!({"unknown":true}),
    ] {
        assert_eq!(
            request(
                &client,
                base,
                Method::POST,
                "/admin/invite-tokens",
                admin,
                Some(input)
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        request(
            &client,
            base,
            Method::PATCH,
            "/admin/onboarding",
            admin,
            Some(json!({"lan_open_registration":true,"unknown":true}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(&client, base, Method::GET, "/admin/onboarding", admin, None)
            .await
            .1["lan_open_registration"],
        false
    );
    assert_eq!(
        request(
            &client,
            base,
            Method::PATCH,
            "/admin/onboarding",
            admin,
            Some(json!({"lan_open_registration":true}))
        )
        .await
        .0,
        StatusCode::OK
    );
    for input in [
        json!({"name":"  "}),
        json!({"name":"长".repeat(65)}),
        json!({"name":"unknown","status":"active"}),
        json!({"name":"combo","subscribe_all_projects":true,"project_ids":["missing"]}),
    ] {
        assert_eq!(
            request(
                &client,
                base,
                Method::POST,
                "/agents/register",
                None,
                Some(input)
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    let (_, first) = request(
        &client,
        base,
        Method::POST,
        "/projects",
        admin,
        Some(json!({"name":"first"})),
    )
    .await;
    let (_, second) = request(
        &client,
        base,
        Method::POST,
        "/projects",
        admin,
        Some(json!({"name":"second"})),
    )
    .await;
    let first_id = first["id"].as_str().unwrap();
    let second_id = second["id"].as_str().unwrap();
    let (_, registered) = request(
        &client,
        base,
        Method::POST,
        "/agents/register",
        None,
        Some(json!({"name":"agent","project_ids":[first_id,first_id],"capabilities":["rust"]})),
    )
    .await;
    let key = registered["api_key"].as_str().unwrap();
    let id = registered["agent"]["id"].as_str().unwrap();
    assert_eq!(registered["agent"]["project_ids"], json!([first_id]));
    assert_eq!(registered["agent"]["subscribe_all_projects"], false);
    assert_eq!(
        request(
            &client,
            base,
            Method::POST,
            "/agents/register",
            None,
            Some(json!({"name":" agent "}))
        )
        .await
        .1["error"]["code"],
        "AGENT_NAME_TAKEN"
    );
    for token in [None, Some("unknown"), admin] {
        assert_eq!(
            request(&client, base, Method::GET, "/agent/me", token, None)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
    }
    let (_, updated) = request(
        &client,
        base,
        Method::PATCH,
        "/agent/me",
        Some(key),
        Some(json!({"description":"updated","capabilities":["rust","review"]})),
    )
    .await;
    assert_eq!(updated["description"], "updated");
    assert_eq!(updated["capabilities"], json!(["rust", "review"]));
    assert_eq!(
        request(
            &client,
            base,
            Method::PATCH,
            "/agent/me",
            Some(key),
            Some(json!({"tier":"S"}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(
            &client,
            base,
            Method::GET,
            "/agent/projects",
            Some(key),
            None
        )
        .await
        .1
        .as_array()
        .unwrap()
        .len(),
        2
    );
    let mut public_ids = Vec::new();
    for project_id in [first_id, second_id] {
        let (_, bounty) = request(&client,base,Method::POST,"/bounties",admin,Some(json!({"project_id":project_id,"title":"public 100%_","difficulty":"C","assignee_mode":"any","status":"OPEN"}))).await;
        public_ids.push(bounty["id"].as_str().unwrap().to_owned());
    }
    assert_eq!(
        request(
            &client,
            base,
            Method::GET,
            "/agent/bounties?q=100%25_",
            Some(key),
            None
        )
        .await
        .1
        .as_array()
        .unwrap()
        .len(),
        1
    );
    assert_eq!(
        request(
            &client,
            base,
            Method::GET,
            &format!("/agent/bounties/{}", public_ids[1]),
            Some(key),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    // 自身指派跨订阅可见，其他 Agent 的定向任务始终不可见。
    for target in [id, "another-agent"] {
        let (_, bounty) = request(&client,base,Method::POST,"/bounties",admin,Some(json!({"project_id":second_id,"title":"assigned","difficulty":"C","assignee_mode":"specific","assignee_agent_id":target,"status":"OPEN"}))).await;
        let expected = if target == id {
            StatusCode::OK
        } else {
            StatusCode::NOT_FOUND
        };
        assert_eq!(
            request(
                &client,
                base,
                Method::GET,
                &format!("/agent/bounties/{}", bounty["id"].as_str().unwrap()),
                Some(key),
                None
            )
            .await
            .0,
            expected
        );
    }
    assert_eq!(
        request(
            &client,
            base,
            Method::PUT,
            "/agent/me/subscriptions",
            Some(key),
            Some(json!({"project_ids":[second_id]}))
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(
            &client,
            base,
            Method::PUT,
            "/agent/me/subscriptions",
            Some(key),
            Some(json!({"project_ids":["missing"]}))
        )
        .await
        .1["error"]["code"],
        "PROJECT_NOT_FOUND"
    );
    request(
        &client,
        base,
        Method::PATCH,
        &format!("/projects/{first_id}"),
        admin,
        Some(json!({"archived":true})),
    )
    .await;
    assert_eq!(
        request(
            &client,
            base,
            Method::PUT,
            "/agent/me/subscriptions",
            Some(key),
            Some(json!({"project_ids":[first_id]}))
        )
        .await
        .1["error"]["code"],
        "PROJECT_ARCHIVED"
    );
    assert_eq!(
        request(
            &client,
            base,
            Method::POST,
            "/agents/register",
            None,
            Some(json!({"name":"archived","project_ids":[first_id]}))
        )
        .await
        .1["error"]["code"],
        "PROJECT_ARCHIVED"
    );
    assert_eq!(
        request(&client, base, Method::GET, "/agent/me", Some(key), None)
            .await
            .1["project_ids"],
        json!([second_id])
    );
    assert_eq!(
        request(
            &client,
            base,
            Method::GET,
            "/agent/projects",
            Some(key),
            None
        )
        .await
        .1
        .as_array()
        .unwrap()
        .len(),
        1
    );
    assert_eq!(
        request(
            &client,
            base,
            Method::PUT,
            "/agent/me/subscriptions",
            Some(key),
            Some(json!({"subscribe_all_projects":true,"project_ids":[second_id]}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (_, all) = request(
        &client,
        base,
        Method::PUT,
        "/agent/me/subscriptions",
        Some(key),
        Some(json!({"subscribe_all_projects":true})),
    )
    .await;
    assert_eq!(all["project_ids"], json!([]));
    assert_eq!(all["subscribe_all_projects"], true);
    let repo = server::db::Repository::open(&running.info.db_path).unwrap();
    repo.agent_claim_bounty(&public_ids[1], id).unwrap();
    let (_, busy) = request(
        &client,
        base,
        Method::GET,
        "/agents?status=active&activity=busy",
        admin,
        None,
    )
    .await;
    assert_eq!(busy.as_array().unwrap().len(), 1);
    assert_eq!(busy[0]["active_lease_count"], 1);
    assert_eq!(
        request(
            &client,
            base,
            Method::GET,
            "/agents?activity=idle",
            admin,
            None
        )
        .await
        .1,
        json!([])
    );
    assert_eq!(
        request(
            &client,
            base,
            Method::GET,
            "/agents?activity=unknown",
            admin,
            None
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    request(
        &client,
        base,
        Method::POST,
        &format!("/agents/{id}/disable"),
        admin,
        None,
    )
    .await;
    assert_eq!(
        request(
            &client,
            base,
            Method::GET,
            "/agents?status=disabled",
            admin,
            None
        )
        .await
        .1
        .as_array()
        .unwrap()
        .len(),
        1
    );
    assert_eq!(
        request(
            &client,
            base,
            Method::POST,
            "/agents/missing/revoke",
            admin,
            None
        )
        .await
        .1["error"]["code"],
        "AGENT_NOT_FOUND"
    );
    request(
        &client,
        base,
        Method::PATCH,
        "/admin/onboarding",
        admin,
        Some(json!({"lan_open_registration":false})),
    )
    .await;
    assert_eq!(
        request(
            &client,
            base,
            Method::POST,
            "/agents/register",
            None,
            Some(json!({"name":"closed-again"}))
        )
        .await
        .1["error"]["code"],
        "INVITE_TOKEN_REQUIRED"
    );
    running.shutdown().await.unwrap();
}

#[tokio::test]
async fn skill_documents_follow_request_address_and_never_export_admin_credentials() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let running = server::start(ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        db_path: dir.path().join("agents.db"),
        admin_token: "secret-admin-not-in-documents".into(),
        mode: ServerMode::Standalone,
    })
    .await
    .unwrap();
    let client = Client::new();
    let base = &running.info.base_url;
    let admin_token = "secret-admin-not-in-documents";
    let admin = Some(admin_token);
    let address = base.strip_prefix("http://").unwrap().to_owned();
    let no_host = tokio::task::spawn_blocking(move || {
        use std::io::{Read, Write};
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        stream.write_all(b"GET /skill.md HTTP/1.0\r\n\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    })
    .await
    .unwrap();
    assert!(no_host.starts_with("HTTP/1.0 200"));
    assert!(no_host.contains(&format!("base_url: {base}")));
    let (_, created) = request(
        &client,
        base,
        Method::POST,
        "/admin/invite-tokens",
        admin,
        Some(json!({"label":"onboarding","max_uses":2,"expires_in_hours":24})),
    )
    .await;
    let raw = created["raw_token"].as_str().unwrap();
    let id = created["token"]["id"].as_str().unwrap();
    for route in ["/skill.md", "/api/v1/skill.md"] {
        let response = client.get(format!("{base}{route}")).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()["content-type"],
            "text/markdown; charset=utf-8"
        );
        let content = response.text().await.unwrap();
        assert!(content.contains(base));
        assert!(content.contains("<INVITE_TOKEN>"));
        assert!(!content.contains(raw));
        assert!(!content.contains(admin_token));
        for endpoint in [
            "taskrail-agent register",
            "taskrail_me",
            "taskrail_set_subscriptions",
            "taskrail_list_projects",
            "taskrail_list_tasks",
            "taskrail_get_task",
            "Unranked",
            "DRAFT",
        ] {
            assert!(content.contains(endpoint), "{endpoint}");
        }
    }
    let forwarded = client
        .get(format!("{base}/skill.md"))
        .header("X-Forwarded-Proto", "https")
        .header("X-Forwarded-Host", "rail.example:9443")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(forwarded.contains("base_url: https://rail.example:9443"));
    let hosted = client
        .get(format!("{base}/skill.md"))
        .header("Host", "host.example:1234")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(hosted.contains("base_url: http://host.example:1234"));
    for query in [format!("?token={raw}"), format!("?invite_token={raw}")] {
        let content = client
            .get(format!("{base}/skill.md{query}"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(!content.contains(raw));
    }
    assert!(!client
        .get(format!("{base}/skill.md"))
        .bearer_auth(raw)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap()
        .contains(raw));
    let (status, exported) = request(
        &client,
        base,
        Method::GET,
        &format!("/admin/skill.md?base_url=https%3A%2F%2Fexport.example&invite_token={raw}"),
        admin,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(exported["base_url"], "https://export.example");
    assert_eq!(exported["filename"], "SKILL.md");
    assert!(!exported["content"].as_str().unwrap().contains(raw));
    assert!(!exported.to_string().contains(admin_token));
    assert_eq!(
        request(
            &client,
            base,
            Method::GET,
            "/admin/skill.md?base_url=javascript%3Aalert(1)",
            admin,
            None
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (_, list) = request(
        &client,
        base,
        Method::GET,
        "/admin/invite-tokens",
        admin,
        None,
    )
    .await;
    assert_eq!(list[0]["use_count"], 0);
    assert!(!list.to_string().contains(raw));
    assert!(list[0].get("token_hash").is_none());
    let (_, revoked) = request(
        &client,
        base,
        Method::POST,
        &format!("/admin/invite-tokens/{id}/revoke"),
        admin,
        None,
    )
    .await;
    assert!(revoked["revoked_at"].is_string());
    assert_eq!(
        client
            .get(format!("{base}/skill.md?token={raw}"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            &client,
            base,
            Method::POST,
            "/admin/invite-tokens/missing/revoke",
            admin,
            None
        )
        .await
        .1["error"]["code"],
        "INVITE_TOKEN_NOT_FOUND"
    );
    let (_, disposable) = request(
        &client,
        base,
        Method::POST,
        "/admin/invite-tokens",
        admin,
        Some(json!({"label":"delete-me"})),
    )
    .await;
    let disposable_id = disposable["token"]["id"].as_str().unwrap();
    let disposable_raw = disposable["raw_token"].as_str().unwrap();
    let (delete_status, delete_body) = request(
        &client,
        base,
        Method::DELETE,
        &format!("/admin/invite-tokens/{disposable_id}"),
        admin,
        None,
    )
    .await;
    assert_eq!(delete_status, StatusCode::NO_CONTENT);
    assert!(delete_body.is_null());
    assert_eq!(
        client
            .get(format!("{base}/skill.md?token={disposable_raw}"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            &client,
            base,
            Method::DELETE,
            &format!("/admin/invite-tokens/{disposable_id}"),
            admin,
            None,
        )
        .await
        .1["error"]["code"],
        "INVITE_TOKEN_NOT_FOUND"
    );
    request(
        &client,
        base,
        Method::PATCH,
        "/admin/onboarding",
        admin,
        Some(json!({"lan_open_registration":true})),
    )
    .await;
    let content = client
        .get(format!("{base}/skill.md"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(content.contains("已开启局域网免审注册"));
    running.shutdown().await.unwrap();
}
