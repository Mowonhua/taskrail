//! 通过真实子进程和 HTTP 服务验证两种 MCP 连接共用工具与 Agent 权限。
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use reqwest::{Client, Response, StatusCode};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};
use taskrail_lib::server::{start, ServerConfig, ServerMode};

fn initialize() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-11-25","clientInfo":{"name":"taskrail-test","version":"1"},"capabilities":{}
    }})
}

fn rpc(id: i64, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}

async fn mcp(client: &Client, url: &str, token: Option<&str>, payload: &Value) -> Response {
    let req = client
        .post(url)
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2025-11-25")
        .json(payload);
    match token {
        Some(token) => req.bearer_auth(token),
        None => req,
    }
    .send()
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_stdio_and_oauth_http_share_agent_tools_and_permissions() {
    let folder = tempfile::tempdir().unwrap();
    let database = folder.path().join("taskrail.db");
    let config = || ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        db_path: database.clone(),
        admin_token: "test-admin".into(),
        mode: ServerMode::Standalone,
    };
    let server = start(config()).await.unwrap();
    let base = &server.info.base_url;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();

    let invite: Value = client
        .post(format!("{base}/api/v1/admin/invite-tokens"))
        .bearer_auth("test-admin")
        .json(&json!({"label":"mcp","max_uses":2}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let invite = invite["raw_token"].as_str().unwrap();

    let project: Value = client
        .post(format!("{base}/api/v1/projects"))
        .bearer_auth("test-admin")
        .json(&json!({"name":"mcp-project"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let task: Value = client
        .post(format!("{base}/api/v1/bounties"))
        .bearer_auth("test-admin")
        .json(&json!({
            "project_id":project["id"],"title":"mcp-task","status":"OPEN",
            "difficulty":"C","assignee_mode":"any",
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();

    let profile = folder.path().join("connection.json");
    std::fs::write(
        &profile,
        json!({"base_url":base,"invite_token":invite}).to_string(),
    )
    .unwrap();
    let command = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_taskrail-agent"));
        command.arg("--config").arg(&profile);
        command
    };
    let denied = command().args(["tasks", "list"]).output().unwrap();
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("请先注册或登录"));

    let credentials = json!({"name":"native-agent","password":"a-unique-test-password"});
    let registration = folder.path().join("registration.json");
    std::fs::write(&registration, credentials.to_string()).unwrap();
    let output = command()
        .args(["account", "register", "--json"])
        .arg(registration)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let signed_in: Value = serde_json::from_slice(&output.stdout).unwrap();
    let agent = &signed_in["profile"];
    assert_eq!(agent["role"], "agent");
    let connection: Value = serde_json::from_slice(&std::fs::read(&profile).unwrap()).unwrap();
    assert!(connection.get("invite_token").is_none());
    let api_key = connection["api_key"].as_str().unwrap();
    assert!(!String::from_utf8_lossy(&output.stdout).contains(api_key));

    let output = command().args(["tasks", "list"]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tasks: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(tasks[0]["id"], task["id"]);

    let stdio_config = folder.path().join("stdio.json");
    std::fs::write(&stdio_config, json!({"base_url":base}).to_string()).unwrap();
    let mut stdio = Command::new(env!("CARGO_BIN_EXE_taskrail-agent"))
        .arg("--config")
        .arg(&stdio_config)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = stdio.stdin.take().unwrap();
    let mut output = BufReader::new(stdio.stdout.take().unwrap());
    let mut send = |value: &Value| {
        writeln!(input, "{value}").unwrap();
        input.flush().unwrap();
    };
    let mut receive = || -> Value {
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    };
    send(&initialize());
    assert_eq!(receive()["result"]["serverInfo"]["name"], "taskrail");
    send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    send(&rpc(2, "tools/list", json!({})));
    let stdio_tools = receive()["result"]["tools"].clone();
    send(&rpc(
        20,
        "tools/call",
        json!({"name":"taskrail_claim_task","arguments":{}}),
    ));
    assert_eq!(
        receive()["result"]["structuredContent"]["error"]["message"],
        "请先注册或登录"
    );
    send(&rpc(
        21,
        "tools/call",
        json!({"name":"taskrail_login","arguments":credentials}),
    ));
    let logged_in = receive()["result"]["structuredContent"]["data"].clone();
    assert_eq!(logged_in["profile"]["id"], agent["id"]);
    assert!(logged_in.get("token").is_none());
    send(&rpc(
        3,
        "tools/call",
        json!({"name":"taskrail_me","arguments":{}}),
    ));
    assert_eq!(
        receive()["result"]["structuredContent"]["data"]["id"],
        agent["id"]
    );
    send(&rpc(
        4,
        "tools/call",
        json!({
            "name":"taskrail_claim_task","arguments":{"task_id":task["id"]},
        }),
    ));
    let claimed = receive()["result"]["structuredContent"]["data"].clone();
    assert_eq!(claimed["status"], "IN_PROGRESS");
    let lease_args = json!({"task_id":task["id"],"lease_id":claimed["lease_id"]});
    drop(input);
    assert!(stdio.wait().unwrap().success());

    let resource = format!("{base}/mcp");
    let denied = mcp(&client, &resource, None, &initialize()).await;
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    let resource_metadata = denied.headers()["www-authenticate"]
        .to_str()
        .unwrap()
        .split('"')
        .nth(1)
        .unwrap()
        .to_owned();
    let discovery: Value = client
        .get(resource_metadata)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(discovery["resource"], resource);
    let metadata: Value = client
        .get(format!("{base}/.well-known/oauth-authorization-server"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        metadata["code_challenge_methods_supported"],
        json!(["S256"])
    );
    let token_endpoint = metadata["token_endpoint"].as_str().unwrap();
    let callback = "http://127.0.0.1:9876/callback";
    assert_eq!(client.post(metadata["registration_endpoint"].as_str().unwrap())
        .json(&json!({"redirect_uris":["http://remote.example/callback"],"token_endpoint_auth_method":"none"}))
        .send().await.unwrap().status(), StatusCode::BAD_REQUEST);

    let registration: Value = client
        .post(metadata["registration_endpoint"].as_str().unwrap())
        .json(&json!({"redirect_uris":["http://127.0.0.1/callback"],"token_endpoint_auth_method":"none"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let client_id = registration["client_id"].as_str().unwrap();
    let verifier = "A".repeat(43);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let authorization = |redirect| {
        client
            .get(metadata["authorization_endpoint"].as_str().unwrap())
            .query(&[
                ("response_type", "code"),
                ("client_id", client_id),
                ("redirect_uri", redirect),
                ("code_challenge", &challenge),
                ("code_challenge_method", "S256"),
                ("resource", &resource),
                ("state", "test-state"),
            ])
    };
    assert_eq!(
        authorization("https://unregistered.example/callback")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let form = authorization(callback)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .text()
        .await
        .unwrap();
    let flow = form
        .split("name=\"flow\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    assert!(!form.contains("name=\"name\""));
    let consent = |flow| {
        client
            .post(metadata["authorization_endpoint"].as_str().unwrap())
            .form(&[("flow", flow), ("decision", "allow")])
    };
    assert_eq!(
        consent(flow)
            .header("Origin", "https://untrusted.example")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = consent(flow).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let redirect = url::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
    let params: std::collections::HashMap<_, _> = redirect.query_pairs().into_owned().collect();
    assert_eq!(&params["state"], "test-state");
    assert_eq!(&params["iss"], base);
    let code = &params["code"];
    let exchange = |code: &str, verifier: &str| {
        client.post(token_endpoint).form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", client_id),
            ("redirect_uri", callback),
            ("resource", &resource),
            ("code_verifier", verifier),
        ])
    };
    let wrong_verifier = "B".repeat(43);
    assert_eq!(
        exchange(code, &wrong_verifier)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let token: Value = exchange(code, &verifier)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        exchange(code, &verifier).send().await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    let access = token["access_token"].as_str().unwrap();
    let init = mcp(&client, &resource, Some(access), &initialize()).await;
    assert_eq!(
        init.status(),
        StatusCode::OK,
        "{}",
        init.text().await.unwrap()
    );
    let response = mcp(
        &client,
        &resource,
        Some(access),
        &rpc(2, "tools/list", json!({})),
    )
    .await;
    let result: Value = response.error_for_status().unwrap().json().await.unwrap();
    assert_eq!(result["result"]["tools"], stdio_tools);
    let unregistered: Value = mcp(
        &client,
        &resource,
        Some(access),
        &rpc(
            20,
            "tools/call",
            json!({"name":"taskrail_claim_task","arguments":{}}),
        ),
    )
    .await
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(
        unregistered["result"]["structuredContent"]["error"]["message"],
        "请先注册或登录"
    );
    let login: Value = mcp(
        &client,
        &resource,
        Some(access),
        &rpc(
            21,
            "tools/call",
            json!({"name":"taskrail_login","arguments":credentials}),
        ),
    )
    .await
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(
        login["result"]["structuredContent"]["data"]["profile"]["id"],
        agent["id"]
    );
    assert!(login["result"]["structuredContent"]["data"]
        .get("token")
        .is_none());
    let response = mcp(
        &client,
        &resource,
        Some(access),
        &rpc(
            3,
            "tools/call",
            json!({"name":"taskrail_me","arguments":{}}),
        ),
    )
    .await;
    let result: Value = response.error_for_status().unwrap().json().await.unwrap();
    assert_eq!(
        result["result"]["structuredContent"]["data"]["id"],
        agent["id"]
    );
    let renewed: Value = mcp(
        &client,
        &resource,
        Some(access),
        &rpc(
            4,
            "tools/call",
            json!({"name":"taskrail_heartbeat","arguments":lease_args}),
        ),
    )
    .await
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(
        renewed["result"]["structuredContent"]["data"]["lease_id"],
        claimed["lease_id"]
    );
    let input_file = folder.path().join("complete.json");
    std::fs::write(&input_file, lease_args.to_string()).unwrap();
    let complete = command()
        .args(["tasks", "complete", "--json"])
        .arg(input_file)
        .output()
        .unwrap();
    assert!(
        complete.status.success(),
        "{}",
        String::from_utf8_lossy(&complete.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&complete.stdout).unwrap()["status"],
        "COMPLETED"
    );
    assert_eq!(
        mcp(&client, &resource, Some(api_key), &initialize())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(format!("{base}/api/v1/agent/me"))
            .bearer_auth(access)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let refresh = token["refresh_token"].as_str().unwrap();
    let rotate = || {
        client.post(token_endpoint).form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("resource", &resource),
            ("refresh_token", refresh),
        ])
    };
    let rotated: Value = rotate()
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        rotate().send().await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        mcp(&client, &resource, Some(access), &initialize())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );

    // 同一客户端的第二个授权拥有独立的身份，注册后可直接调用业务工具。
    let new_form = authorization(callback)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .text()
        .await
        .unwrap();
    let new_flow = new_form
        .split("name=\"flow\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let new_response = consent(new_flow).send().await.unwrap();
    assert_eq!(new_response.status(), StatusCode::SEE_OTHER);
    let callback_url =
        url::Url::parse(new_response.headers()["location"].to_str().unwrap()).unwrap();
    let code = callback_url
        .query_pairs()
        .find(|(name, _)| name == "code")
        .unwrap()
        .1;
    let new_token: Value = exchange(&code, &verifier)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let new_access = new_token["access_token"].as_str().unwrap();
    let registered: Value = mcp(
        &client,
        &resource,
        Some(new_access),
        &rpc(
            22,
            "tools/call",
            json!({"name":"taskrail_register","arguments":{
                "name":"oauth-agent","password":"another-test-password","invite_token":invite,
                "avatar":{"kind":"preset","id":"blue"}
            }}),
        ),
    )
    .await
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
    let registered = &registered["result"]["structuredContent"]["data"]["profile"];
    assert_eq!(registered["name"], "oauth-agent");
    assert_eq!(registered["avatar"]["id"], "blue");
    let new_me: Value = mcp(
        &client,
        &resource,
        Some(new_access),
        &rpc(
            23,
            "tools/call",
            json!({"name":"taskrail_me","arguments":{}}),
        ),
    )
    .await
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(
        new_me["result"]["structuredContent"]["data"]["id"],
        registered["id"]
    );

    let rotated_access = rotated["access_token"].as_str().unwrap();
    client
        .post(format!(
            "{base}/api/v1/agents/{}/revoke",
            agent["id"].as_str().unwrap()
        ))
        .bearer_auth("test-admin")
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert_eq!(
        mcp(&client, &resource, Some(rotated_access), &initialize())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    server.shutdown().await.unwrap();
}
