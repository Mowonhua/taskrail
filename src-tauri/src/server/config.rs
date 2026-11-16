//! 文件职责：无头服务的命令行和环境变量解析；显式参数覆盖环境变量。
use super::{
    error::{ApiError, Result},
    ServerConfig, ServerMode,
};
use std::{collections::HashMap, ffi::OsString};

pub const USAGE: &str = "taskrail-server [--bind IP:PORT] [--db PATH] --admin-token TOKEN\n环境变量: TASKRAIL_BIND, TASKRAIL_DB, TASKRAIL_ADMIN_TOKEN\n默认 bind: 127.0.0.1:8420，默认 db: ./taskrail.db；admin-token 必填。";

/// 函数职责：解析不含程序名的参数；通过注入环境读取器测试，不修改进程全局环境。
pub fn parse_config(
    args: impl IntoIterator<Item = OsString>,
    mut env: impl FnMut(&str) -> Option<OsString>,
) -> Result<ServerConfig> {
    let mut values = HashMap::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let flag = arg
            .to_str()
            .ok_or_else(|| ApiError::invalid("参数名必须为 UTF-8"))?;
        let (key, inline) = flag
            .split_once('=')
            .map_or((flag, None), |(k, v)| (k, Some(OsString::from(v))));
        if !matches!(key, "--bind" | "--db" | "--admin-token") {
            return Err(ApiError::invalid(format!("未知参数 {key}")));
        }
        let value = inline
            .or_else(|| args.next())
            .ok_or_else(|| ApiError::invalid(format!("{key} 缺少值")))?;
        if value.is_empty() || value.to_str().is_some_and(|v| v.starts_with("--")) {
            return Err(ApiError::invalid(format!("{key} 缺少有效值")));
        }
        if values.insert(key.to_owned(), value).is_some() {
            return Err(ApiError::invalid(format!("重复参数 {key}")));
        }
    }
    let bind = values
        .remove("--bind")
        .or_else(|| env("TASKRAIL_BIND"))
        .unwrap_or_else(|| "127.0.0.1:8420".into());
    let bind = bind
        .to_str()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ApiError::invalid("bind 必须为 IP:PORT"))?;
    let db_path = values
        .remove("--db")
        .or_else(|| env("TASKRAIL_DB"))
        .unwrap_or_else(|| "taskrail.db".into());
    if db_path.is_empty() || db_path == ":memory:" {
        return Err(ApiError::invalid("db 必须为文件路径"));
    }
    let token = values
        .remove("--admin-token")
        .or_else(|| env("TASKRAIL_ADMIN_TOKEN"))
        .ok_or_else(|| ApiError::invalid("必须设置 --admin-token 或 TASKRAIL_ADMIN_TOKEN"))?;
    let admin_token = token
        .into_string()
        .map_err(|_| ApiError::invalid("admin-token 必须为 UTF-8"))?;
    if admin_token.trim().is_empty() {
        return Err(ApiError::invalid("admin-token 不能为空"));
    }
    Ok(ServerConfig {
        bind,
        db_path: db_path.into(),
        admin_token,
        mode: ServerMode::Standalone,
    })
}

/// 函数职责：等待 Ctrl+C；Unix 同时处理 SIGTERM，供守护进程管理器优雅关闭。
pub async fn shutdown_signal() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut term) => tokio::select! {
                result = tokio::signal::ctrl_c() => { if let Err(e) = result { eprintln!("Ctrl+C: {e}"); } },
                _ = term.recv() => {},
            },
            Err(e) => eprintln!("SIGTERM: {e}"),
        }
    }
    #[cfg(not(unix))]
    if let Err(e) = tokio::signal::ctrl_c().await {
        eprintln!("Ctrl+C: {e}");
    }
}
