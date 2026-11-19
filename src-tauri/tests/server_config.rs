#![cfg(not(target_os = "android"))]
//! 文件职责：验证 CLI 优先级、缺省、非法输入与真实二进制启动。
use std::ffi::OsString;
use taskrail_lib::server::{config::parse_config, ServerMode};

fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[test]
fn cli_overrides_environment_and_preserves_paths() {
    let c = parse_config(
        args(&[
            "--bind",
            "127.0.0.1:0",
            "--db",
            "目录/data.db",
            "--admin-token",
            "cli-token",
        ]),
        |_| Some("invalid-env".into()),
    )
    .unwrap();
    assert_eq!(c.bind.to_string(), "127.0.0.1:0");
    assert_eq!(c.db_path, std::path::Path::new("目录/data.db"));
    assert_eq!(c.admin_token, "cli-token");
    assert!(matches!(c.mode, ServerMode::Standalone));
    let c = parse_config(
        args(&["--bind=[::1]:9000", "--db=data.db", "--admin-token=secret"]),
        |_| None,
    )
    .unwrap();
    assert_eq!(c.bind.to_string(), "[::1]:9000");
}

#[test]
fn env_fallbacks_and_defaults_are_deterministic() {
    let c = parse_config([], |key| match key {
        "TASKRAIL_BIND" => Some("0.0.0.0:9001".into()),
        "TASKRAIL_DB" => Some("env.db".into()),
        "TASKRAIL_ADMIN_TOKEN" => Some("env-token".into()),
        _ => None,
    })
    .unwrap();
    assert_eq!(c.bind.to_string(), "0.0.0.0:9001");
    assert_eq!(c.db_path, std::path::Path::new("env.db"));
    assert_eq!(c.admin_token, "env-token");
    let c = parse_config([], |key| {
        (key == "TASKRAIL_ADMIN_TOKEN").then(|| "token".into())
    })
    .unwrap();
    assert_eq!(c.bind.to_string(), "127.0.0.1:8420");
    assert_eq!(c.db_path, std::path::Path::new("taskrail.db"));
}

#[test]
fn invalid_or_ambiguous_config_is_rejected() {
    for values in [
        vec![],
        vec!["--admin-token", " "],
        vec!["--admin-token"],
        vec!["--bind", "--db", "x"],
        vec!["--unknown", "x"],
        vec!["--admin-token", "x", "--bind", "bad"],
        vec!["--admin-token", "x", "--db", ""],
        vec!["--admin-token", "x", "--db", ":memory:"],
        vec!["--admin-token", "x", "--admin-token", "y"],
    ] {
        assert!(parse_config(args(&values), |_| None).is_err(), "{values:?}");
    }
    assert!(parse_config([], |key| (key == "TASKRAIL_ADMIN_TOKEN")
        .then(|| " ".into()))
    .is_err());
}

#[test]
fn binary_help_and_missing_token_exit_codes() {
    let exe = env!("CARGO_BIN_EXE_taskrail-server");
    let output = std::process::Command::new(exe)
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("TASKRAIL_ADMIN_TOKEN"));
    let output = std::process::Command::new(exe)
        .env_remove("TASKRAIL_ADMIN_TOKEN")
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[tokio::test]
async fn binary_starts_with_environment_and_exposes_shared_kernel() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    use std::io::{BufRead, BufReader};
    // kill/wait 包装保证断言失败时也回收子进程；优雅关闭由内核测试覆盖。
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let dir = tempfile::tempdir().unwrap();
    // 将程序移入独立目录，验证它可以在没有配套模板文件的部署中运行。
    let built_exe = std::path::Path::new(env!("CARGO_BIN_EXE_taskrail-server"));
    let deployed_exe = dir.path().join(built_exe.file_name().unwrap());
    std::fs::copy(built_exe, &deployed_exe).unwrap();
    let mut child = Child(
        std::process::Command::new(deployed_exe)
            .current_dir(dir.path())
            .env("TASKRAIL_BIND", "127.0.0.1:0")
            .env("TASKRAIL_DB", dir.path().join("daemon.db"))
            .env("TASKRAIL_ADMIN_TOKEN", "daemon-token")
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
    let line = tokio::time::timeout(std::time::Duration::from_secs(15), receiver)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let base = line
        .trim()
        .strip_prefix("Taskrail server listening on ")
        .unwrap();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let response = client
        .get(format!("{base}/api/v1/admin/status"))
        .bearer_auth("daemon-token")
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let status: serde_json::Value = serde_json::from_str(&response.text().await.unwrap()).unwrap();
    assert_eq!(status["mode"], "standalone");
    assert_eq!(status["version"], env!("CARGO_PKG_VERSION"));
    assert!(status["db_path"].as_str().unwrap().ends_with("daemon.db"));

    let skill = client
        .get(format!("{base}/skill.md"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(skill.contains("# Taskrail Agent"));
    assert!(skill.contains(&format!("base_url: {base}")));
    assert!(skill.contains("taskrail-agent register"));
    assert!(skill.contains(r#""invite_token": "<INVITE_TOKEN>""#));
}
