//! 文件职责：Agent 鉴权下的 Inbox/Ack、Webhook 配置和有序 SSE/WS 事件流。
use super::agents::Agent;
use super::*;
use crate::server::push::StreamGuard;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        WebSocketUpgrade,
    },
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
};
use std::{collections::VecDeque, convert::Infallible, time::Duration};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/agent/me/webhook", get(webhook).put(update_webhook))
        .route("/api/v1/agent/events", get(events))
        .route("/api/v1/agent/events/ack", post(ack))
        .route("/api/v1/agent/stream", get(sse))
        .route("/api/v1/agent/ws", get(ws))
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct InboxQuery {
    cursor: Option<i64>,
    limit: Option<usize>,
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct StreamQuery {
    cursor: Option<i64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ack {
    event_id: i64,
}
async fn events(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Params(query): Params<InboxQuery>,
) -> Result<Json<AgentInbox>> {
    disk(state, move |r| {
        r.agent_events(&agent.id, query.cursor, query.limit.unwrap_or(100))
    })
    .await
    .map(Json)
}
async fn ack(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Body(input): Body<Ack>,
) -> Result<Json<serde_json::Value>> {
    let ack = disk(state, move |r| {
        r.ack_agent_events(&agent.id, input.event_id)
    })
    .await?;
    Ok(Json(json!({"acked_event_id":ack})))
}
async fn webhook(
    State(state): State<AppState>,
    Agent(agent): Agent,
) -> Result<Json<WebhookConfig>> {
    disk(state, move |r| r.webhook_config(&agent.id))
        .await
        .map(Json)
}
async fn update_webhook(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Body(input): Body<UpdateWebhook>,
) -> Result<Json<WebhookConfig>> {
    disk(state, move |r| r.update_webhook(&agent.id, input))
        .await
        .map(Json)
}

struct EventStream {
    state: AppState,
    id: String,
    cursor: i64,
    pending: VecDeque<AgentEvent>,
    guard: StreamGuard,
}
impl EventStream {
    async fn open(state: AppState, id: String, cursor: Option<i64>) -> Result<Self> {
        let agent_id = id.clone();
        let (guard, inbox) = disk(state.clone(), move |r| {
            let guard = r.connect_stream(&agent_id)?;
            let inbox = r.agent_events(&agent_id, cursor, 500)?;
            Ok((guard, inbox))
        })
        .await?;
        // 已先登记再读取积压；后续继续按数据库游标读取，无广播订阅空窗或内存队列溢出。
        Ok(Self {
            state,
            id,
            cursor: inbox.cursor,
            pending: inbox.events.into(),
            guard,
        })
    }
    async fn next(&mut self) -> Option<AgentEvent> {
        loop {
            if self.guard.cancel.has_changed().is_err() {
                return None;
            }
            if let Some(event) = self.pending.pop_front() {
                return Some(event);
            }
            let id = self.id.clone();
            let cursor = self.cursor;
            let result = tokio::select! {
                _=self.guard.cancel.changed()=>return None,
                result=disk(self.state.clone(),move|r|r.agent_events(&id,Some(cursor),500))=>result,
            };
            match result {
                Ok(inbox) => {
                    self.cursor = inbox.cursor;
                    self.pending = inbox.events.into();
                }
                Err(_) => return None,
            }
            if self.pending.is_empty() {
                tokio::select! { _=self.guard.cancel.changed()=>return None, _=tokio::time::sleep(Duration::from_millis(50))=>{} }
            }
        }
    }
    async fn pushed(&self) -> Result<()> {
        let id = self.id.clone();
        disk(self.state.clone(), move |r| r.mark_stream_pushed(&id)).await
    }
}
async fn sse(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Params(query): Params<StreamQuery>,
    headers: HeaderMap,
) -> Result<Response> {
    let cursor = match query.cursor {
        Some(v) => Some(v),
        None => headers
            .get("last-event-id")
            .map(|v| {
                v.to_str()
                    .ok()
                    .and_then(|s| s.parse::<i64>().ok())
                    .ok_or_else(|| ApiError::invalid("Last-Event-ID 必须为非负整数"))
            })
            .transpose()?,
    };
    let stream = EventStream::open(state, agent.id, cursor).await?;
    let stream = futures_util::stream::unfold(stream, |mut stream| async move {
        let event = stream.next().await?;
        if stream.pushed().await.is_err() {
            return None;
        }
        let output = Event::default()
            .id(event.event_id.to_string())
            .event(&event.event_type)
            .json_data(&event)
            .ok()?;
        Some((Ok::<_, Infallible>(output), stream))
    });
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response())
}
async fn ws(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Params(query): Params<StreamQuery>,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    let stream = EventStream::open(state, agent.id, query.cursor).await?;
    Ok(upgrade.on_upgrade(move |socket| websocket(socket, stream)))
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ClientMessage {
    Ack { event_id: i64 },
}
async fn websocket(mut socket: WebSocket, mut stream: EventStream) {
    loop {
        tokio::select! {
            event=stream.next()=>match event {
                Some(event)=> {
                    let text=match serde_json::to_string(&event){Ok(v)=>v,Err(_)=>break};
                    let sent=tokio::select! { biased; _=stream.guard.cancel.changed()=>break, result=socket.send(Message::Text(text.into()))=>result };
                    if sent.is_err() || stream.pushed().await.is_err() {break;}
                }
                None=>break,
            },
            message=socket.recv()=>match message {
                Some(Ok(Message::Text(text)))=> {
                    let result=match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(ClientMessage::Ack{event_id})=> {
                            let id=stream.id.clone();
                            disk(stream.state.clone(),move|r|r.ack_agent_events(&id,event_id)).await
                        }
                        Err(_)=>Err(ApiError::invalid("期望 type=ack 和非负 event_id")),
                    };
                    if let Err(error)=result {
                        if let Ok(body)=serde_json::to_string(&json!({"error":{"code":error.code,"message":"无效 Ack"}})) {
                            tokio::select! {
                                biased;
                                _ = stream.guard.cancel.changed() => break,
                                result = socket.send(Message::Text(body.into())) => if result.is_err() { break; },
                            }
                        }
                    }
                }
                Some(Ok(Message::Close(_)))|Some(Err(_))|None=>break,
                _=>{}
            }
        }
    }
    // Drop 先释放名录登记；Close 帧最多等待短暂时间，避免阻碍服务关闭。
    drop(stream);
    let _ = tokio::time::timeout(
        Duration::from_millis(100),
        socket.send(Message::Close(None)),
    )
    .await;
}
