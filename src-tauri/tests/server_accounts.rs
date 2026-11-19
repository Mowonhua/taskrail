//! 验证用户与 Agent 通过同一账户接口注册，并保持业务凭证隔离。
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use taskrail_lib::server::{start, ServerConfig, ServerMode};

#[tokio::test]
async fn shared_accounts_validate_names_profiles_and_session_roles() {
    let folder = tempfile::tempdir().unwrap();
    let server = start(ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        db_path: folder.path().join("taskrail.db"),
        admin_token: "test-admin".into(),
        mode: ServerMode::Standalone,
    })
    .await
    .unwrap();
    let base = &server.info.base_url;
    let client = Client::new();
    let auth = |endpoint: &str| format!("{base}/api/v1/auth/{endpoint}");
    let register = |body: Value| client.post(auth("register")).json(&body);
    let login = |body: Value| client.post(auth("login")).json(&body);
    let password = "a-long-test-password";

    let presets: Value = client
        .get(auth("avatars"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(presets
        .as_array()
        .unwrap()
        .iter()
        .any(|preset| preset["id"] == "blue"));
    let registered = register(json!({
        "role":"user","name":"  林  ","password":password,"avatar":{"kind":"preset","id":"blue"}
    }))
    .send()
    .await
    .unwrap();
    assert_eq!(registered.status(), StatusCode::CREATED);
    assert_eq!(registered.headers()["cache-control"], "no-store");
    let user: Value = registered.json().await.unwrap();
    assert_eq!(user["profile"]["name"], "林");
    assert_eq!(user["profile"]["avatar"]["id"], "blue");
    let user_token = user["token"].as_str().unwrap();

    let duplicate = register(json!({"role":"agent","name":"林","password":password}))
        .send()
        .await
        .unwrap();
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    assert_eq!(
        duplicate.json::<Value>().await.unwrap()["error"]["code"],
        "NAME_TAKEN"
    );
    let denied = register(json!({"role":"agent","name":"worker","password":password}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        denied.json::<Value>().await.unwrap()["error"]["code"],
        "INVITE_TOKEN_REQUIRED"
    );

    let invite: Value = client
        .post(format!("{base}/api/v1/admin/invite-tokens"))
        .bearer_auth("test-admin")
        .json(&json!({"label":"test","max_uses":1}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let duplicate_with_invite = register(json!({
        "role":"agent","name":"林","password":password,"invite_token":invite["raw_token"]
    }))
    .send()
    .await
    .unwrap();
    assert_eq!(duplicate_with_invite.status(), StatusCode::CONFLICT);
    let agent: Value = register(json!({
        "role":"agent","name":"worker","password":password,"invite_token":invite["raw_token"]
    }))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
    let agent_token = agent["token"].as_str().unwrap();

    let bounties_url = format!("{base}/api/v1/bounties");
    let project: Value = client
        .post(format!("{base}/api/v1/projects"))
        .bearer_auth("test-admin")
        .json(&json!({"name":"创建者测试"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let create_bounty = || {
        client
            .post(&bounties_url)
            .bearer_auth("test-admin")
            .json(&json!({
                "project_id":project["id"],"title":"记录创建者","difficulty":"C",
                "assignee_mode":"any","status":"OPEN","created_by":"worker"
            }))
    };
    let get_detail = |id: &str| {
        client
            .get(format!("{bounties_url}/{id}"))
            .bearer_auth("test-admin")
    };
    for invalid_token in ["invalid", agent_token] {
        let response = create_bounty()
            .header("X-Taskrail-User-Session", invalid_token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let attributed: Value = create_bounty()
        .header("X-Taskrail-User-Session", user_token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let bounty_id = attributed["id"].as_str().unwrap();
    let detail: Value = get_detail(bounty_id)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        detail["creator"],
        json!({"name":"林","avatar":{"kind":"preset","id":"blue"}})
    );
    let legacy: Value = create_bounty().send().await.unwrap().json().await.unwrap();
    let anonymous: Value = get_detail(legacy["id"].as_str().unwrap())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(anonymous.get("creator").is_none());

    let agent_me = |token| {
        client
            .get(format!("{base}/api/v1/agent/me"))
            .bearer_auth(token)
    };
    let denied = agent_me(user_token).send().await.unwrap();
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        denied.json::<Value>().await.unwrap()["error"]["message"],
        "请先注册或登录"
    );
    assert_eq!(
        client
            .get(format!("{base}/api/v1/admin/status"))
            .bearer_auth(user_token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        agent_me(agent_token).send().await.unwrap().status(),
        StatusCode::OK
    );
    assert_eq!(
        login(json!({"role":"agent","name":"林","password":password}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let patch = |token, body: Value| client.patch(auth("me")).bearer_auth(token).json(&body);
    assert_eq!(
        patch(user_token, json!({"name":"worker","avatar":null}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    // 头像必须是可解码的约定格式，前缀与 MIME 对应仍不足以通过校验。
    let invalid = patch(
        user_token,
        json!({"avatar":{
            "kind":"upload","media_type":"image/png","data_base64":"iVBORw0KGgo="
        }}),
    )
    .send()
    .await
    .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(1, 1)
        .write_to(&mut buf, image::ImageFormat::Png)
        .unwrap();
    use base64::Engine;
    let avatar = json!({"kind":"upload","media_type":"image/png",
        "data_base64":base64::engine::general_purpose::STANDARD.encode(buf.into_inner())
    });
    let uploaded: Value = patch(user_token, json!({"avatar":avatar}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(uploaded["name"], "林");
    assert_eq!(uploaded["avatar"], avatar);
    let renamed: Value = patch(user_token, json!({"name":"林二","avatar":null}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(renamed["id"], user["profile"]["id"]);
    assert_eq!(renamed["avatar"], Value::Null);
    let detail: Value = get_detail(bounty_id)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail["creator"], json!({"name":"林二","avatar":null}));
    assert_eq!(
        patch(agent_token, json!({"name":"worker-renamed"}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let agent_profile: Value = agent_me(agent_token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(agent_profile["id"], agent["profile"]["id"]);
    assert_eq!(agent_profile["name"], "worker-renamed");

    let newest_login = login(json!({"role":"user","name":"林二","password":password}))
        .send()
        .await
        .unwrap();
    assert_eq!(newest_login.status(), StatusCode::OK);
    let newest_login: Value = newest_login.json().await.unwrap();
    assert_eq!(newest_login["profile"], renamed);
    assert_eq!(
        client
            .post(auth("logout"))
            .bearer_auth(user_token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        client
            .get(auth("me"))
            .bearer_auth(user_token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(auth("me"))
            .bearer_auth(newest_login["token"].as_str().unwrap())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    assert_eq!(
        client
            .post(format!(
                "{base}/api/v1/agents/{}/revoke",
                agent["profile"]["id"].as_str().unwrap()
            ))
            .bearer_auth("test-admin")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        agent_me(agent_token).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );

    let invite: Value = client
        .post(format!("{base}/api/v1/admin/invite-tokens"))
        .bearer_auth("test-admin")
        .json(&json!({"label":"legacy"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let legacy: Value = client
        .post(format!("{base}/api/v1/agents/register"))
        .json(&json!({"name":"legacy","invite_token":invite["raw_token"]}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let legacy_session: Value = login(json!({"role":"agent","agent_key":legacy["api_key"]}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(legacy_session["profile"]["id"], legacy["agent"]["id"]);
    assert_eq!(
        agent_me(legacy_session["token"].as_str().unwrap())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    server.shutdown().await.unwrap();
}
