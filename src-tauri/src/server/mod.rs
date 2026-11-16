//! 文件职责：同构 HTTP 服务的启动、会话鉴权配置与优雅关闭；不依赖 Tauri。
pub mod config;
pub mod db;
pub mod error;
pub mod models;
mod routes;

use db::Repository;
use error::{ApiError, Result};
use models::EmbeddedServerInfo;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{net::SocketAddr, path::PathBuf};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerMode {
    Embedded,
    Standalone,
}

/// 结构职责：两个进程入口共享的服务配置；bind 支持端口 0，token 必须非空。
pub struct ServerConfig {
    pub bind: SocketAddr,
    pub db_path: PathBuf,
    pub admin_token: String,
    pub mode: ServerMode,
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub repository: Repository,
    pub token_digest: [u8; 32],
    pub mode: ServerMode,
    /// 无 Host 请求的文档使用实际监听地址；直接构造 Router 时使用缺省地址。
    pub base_url: String,
}

/// 函数职责：创建共享路由；管理员凭证比较固定长度摘要，避免按首个不同字节提前退出。
pub fn router(repository: Repository, admin_token: &str, mode: ServerMode) -> Result<axum::Router> {
    router_at(
        repository,
        admin_token,
        mode,
        "http://127.0.0.1:8420".into(),
    )
}

fn router_at(
    repository: Repository,
    admin_token: &str,
    mode: ServerMode,
    base_url: String,
) -> Result<axum::Router> {
    if admin_token.trim().is_empty() {
        return Err(ApiError::invalid("admin_token 不能为空"));
    }
    let state = AppState {
        repository,
        token_digest: Sha256::digest(admin_token.as_bytes()).into(),
        mode,
        base_url,
    };
    Ok(routes::router(state))
}

/// 结构职责：持有监听服务和关闭信号；shutdown 等待在途请求，Drop 发送关闭信号。
pub struct RunningServer {
    pub info: EmbeddedServerInfo,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<std::io::Result<()>>>,
}

impl RunningServer {
    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(signal) = self.shutdown.take() {
            let _ = signal.send(());
        }
        if let Some(task) = self.task.take() {
            task.await
                .map_err(ApiError::internal)?
                .map_err(ApiError::internal)?;
        }
        Ok(())
    }

    /// 函数职责：等待外部退出信号或监听任务异常；异常不被静默留在后台任务中。
    pub async fn run_until(mut self, signal: impl std::future::Future<Output = ()>) -> Result<()> {
        let mut task = self.task.take().expect("服务任务只可取出一次");
        tokio::select! {
            result = &mut task => result.map_err(ApiError::internal)?.map_err(ApiError::internal),
            () = signal => {
                if let Some(sender) = self.shutdown.take() { let _ = sender.send(()); }
                task.await.map_err(ApiError::internal)?.map_err(ApiError::internal)
            }
        }
    }
}
impl Drop for RunningServer {
    fn drop(&mut self) {
        if let Some(signal) = self.shutdown.take() {
            let _ = signal.send(());
        }
    }
}

/// 函数职责：完成迁移与监听后才返回实际地址；SQLite 初始化不占用异步工作线程。
pub async fn start(config: ServerConfig) -> Result<RunningServer> {
    let repository = tokio::task::spawn_blocking(move || Repository::open(config.db_path))
        .await
        .map_err(ApiError::internal)??;
    let listener = TcpListener::bind(config.bind)
        .await
        .map_err(ApiError::internal)?;
    let address = listener.local_addr().map_err(ApiError::internal)?;
    let app = router_at(
        repository.clone(),
        &config.admin_token,
        config.mode,
        format!("http://{address}"),
    )?;
    let info = EmbeddedServerInfo {
        base_url: format!("http://{address}"),
        admin_token: config.admin_token,
        db_path: repository.path().to_string_lossy().into_owned(),
    };
    let (shutdown, receiver) = oneshot::channel();
    let task = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = receiver.await;
            })
            .await
    });
    Ok(RunningServer {
        info,
        shutdown: Some(shutdown),
        task: Some(task),
    })
}

/// 函数职责：从系统随机源生成 32 字节会话密钥，以 64 位十六进制传递给 WebView。
pub fn random_admin_token() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(ApiError::internal)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
