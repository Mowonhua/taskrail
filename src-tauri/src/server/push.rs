//! 文件职责：长连接登记/取消与 Webhook 后台投递；事件内容始终从 SQLite 读取。
use super::{
    db::Repository,
    error::{ApiError, Result},
    models::AgentSummary,
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::watch;

#[derive(Debug, Default)]
struct Connections {
    next_id: u64,
    stopped: bool,
    agents: HashMap<String, HashMap<u64, watch::Sender<bool>>>,
}

/// 结构职责：同一仓储实例共享的实时连接注册表；移除登记立即取消该连接。
#[derive(Debug, Clone, Default)]
pub(crate) struct Hub(Arc<Mutex<Connections>>);
impl Hub {
    pub fn connect(&self, id: &str) -> Result<StreamGuard> {
        let mut hub = self.0.lock().unwrap();
        if hub.stopped {
            return Err(ApiError::internal("服务正在关闭"));
        }
        hub.next_id += 1;
        let connection_id = hub.next_id;
        let (sender, cancel) = watch::channel(false);
        hub.agents
            .entry(id.into())
            .or_default()
            .insert(connection_id, sender);
        Ok(StreamGuard {
            hub: self.clone(),
            agent_id: id.into(),
            connection_id,
            cancel,
        })
    }
    pub fn disconnect(&self, id: &str) {
        self.0.lock().unwrap().agents.remove(id);
    }
    pub fn shutdown(&self) {
        let mut hub = self.0.lock().unwrap();
        hub.stopped = true;
        hub.agents.clear();
    }
    pub fn decorate(&self, agent: &mut AgentSummary) {
        agent.stream_connected = self
            .0
            .lock()
            .unwrap()
            .agents
            .get(&agent.id)
            .is_some_and(|v| !v.is_empty());
        agent.webhook_configured = agent.webhook_url.is_some();
        agent.channel_status = if agent.stream_connected {
            "streaming"
        } else if agent.webhook_configured {
            "webhook"
        } else {
            "offline"
        }
        .into();
    }
}

/// 结构职责：连接存活令牌；流被取消、断开或丢弃时自动释放登记。
pub(crate) struct StreamGuard {
    hub: Hub,
    agent_id: String,
    connection_id: u64,
    pub cancel: watch::Receiver<bool>,
}
impl Drop for StreamGuard {
    fn drop(&mut self) {
        let mut hub = self.hub.0.lock().unwrap();
        if let Some(connections) = hub.agents.get_mut(&self.agent_id) {
            connections.remove(&self.connection_id);
            if connections.is_empty() {
                hub.agents.remove(&self.agent_id);
            }
        }
    }
}

/// 函数职责：按 RFC 2104 对 timestamp.body 的原始 UTF-8 字节签名，输出小写十六进制。
pub(crate) fn signature(secret: &str, timestamp: &str, body: &str) -> String {
    let mut key = [0_u8; 64];
    if secret.len() > 64 {
        key[..32].copy_from_slice(&Sha256::digest(secret.as_bytes()));
    } else {
        key[..secret.len()].copy_from_slice(secret.as_bytes());
    }
    let mut inner = Sha256::new();
    inner.update(key.map(|b| b ^ 0x36));
    inner.update(timestamp.as_bytes());
    inner.update(b".");
    inner.update(body.as_bytes());
    let mut outer = Sha256::new();
    outer.update(key.map(|b| b ^ 0x5c));
    outer.update(inner.finalize());
    format!("sha256={:x}", outer.finalize())
}

/// 函数职责：在共享启动路径初始化 TLS 并预构建回调客户端，构建失败必须使服务启动失败。
pub(crate) fn build_webhook_client() -> Result<reqwest::Client> {
    // 桌面与独立入口均需初始化；已有进程默认提供者时沿用它，避免覆盖宿主的 TLS 配置。
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(ApiError::internal)
}

/// 函数职责：只投递 active 且配置未变的 Agent；失败最多尝试五次，延迟 100/200/400/800ms。
/// 实现思路：客户端由启动路径提供并禁止重定向；队头退避由仓储查询保证，关闭信号取消在途请求。
pub(crate) async fn dispatch_webhooks(
    repository: Repository,
    client: reqwest::Client,
    mut stop: watch::Receiver<bool>,
) {
    loop {
        if *stop.borrow() {
            return;
        }
        let repo = repository.clone();
        let deliveries = tokio::select! {
            _=stop.changed()=>return,
            result=tokio::task::spawn_blocking(move||repo.due_webhooks())=>match result {Ok(Ok(v))=>v,_=>Vec::new()},
        };
        for delivery in deliveries {
            let repo = repository.clone();
            let id = delivery.agent_id.clone();
            let url = delivery.url.clone();
            let secret = delivery.secret.clone();
            // 在网络 await 前再次确认身份；已经发出的 HTTP 请求无法由状态修改撤回。
            let current = tokio::task::spawn_blocking(move || {
                repo.webhook_current_values(&id, &url, &secret)
            })
            .await;
            if !matches!(current, Ok(Ok(true))) {
                continue;
            }
            let body = match serde_json::to_string(&delivery.event) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let timestamp = chrono::Utc::now().timestamp().to_string();
            let request = client
                .post(&delivery.url)
                .header("Content-Type", "application/json")
                .header("X-Taskrail-Event", &delivery.event.event_type)
                .header("X-Taskrail-Event-Id", delivery.event.event_id.to_string())
                .header("X-Taskrail-Timestamp", &timestamp)
                .header(
                    "X-Taskrail-Signature",
                    signature(&delivery.secret, &timestamp, &body),
                )
                .body(body)
                .send();
            let error = tokio::select! {
                biased;
                _=stop.changed()=>return,
                result=request=>match result {
                    Ok(response) if response.status().is_success()=>None,
                    Ok(response)=>Some(format!("HTTP {}",response.status().as_u16())),
                    // reqwest 错误可包含带凭证的 URL，只保存错误类别。
                    Err(error)=>Some(if error.is_timeout(){"request timeout"}else{"network error"}.into()),
                }
            };
            let repo = repository.clone();
            let _ =
                tokio::task::spawn_blocking(move || repo.finish_webhook(&delivery, error)).await;
        }
        tokio::select! { _=stop.changed()=>return, _=tokio::time::sleep(Duration::from_millis(25))=>{} }
    }
}
