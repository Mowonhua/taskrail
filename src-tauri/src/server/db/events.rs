//! 文件职责：持久化收件箱、发布目标快照、Ack 与 Webhook 投递记录。
use super::agents::active_agent;
use super::*;

/// 函数职责：在状态变更事务内通知仍 active 的指定身份；不向停用、吊销或历史未知身份投递。
pub(super) fn notify_active(
    tx: &Transaction<'_>,
    agent_id: Option<&str>,
    bounty: &Bounty,
    kind: &str,
    mut data: serde_json::Value,
) -> Result<()> {
    if let Some(id) = agent_id {
        redact_payload(tx, &mut data, id)?;
    }
    let payload = json!({"project_id":bounty.project_id,"data":data}).to_string();
    tx.execute("INSERT INTO agent_events(agent_id,bounty_id,event_type,payload_json,created_at) SELECT id,?2,?3,?4,?5 FROM agents WHERE id=?1 AND status='active'", params![agent_id,bounty.id,kind,payload,now()])?;
    Ok(())
}

/// 函数职责：与悬赏状态变化共用写事务；撤回只投递给本轮已记录的 active 接收者。
/// 约束条件：发布只接受 OPEN 快照；撤回只保存标识，不能序列化 DRAFT 内容。
pub(super) fn dispatch(tx: &Transaction<'_>, bounty: &Bounty, withdrawn: bool) -> Result<()> {
    let targets = if withdrawn {
        let mut stmt = tx.prepare("SELECT d.agent_id FROM bounty_dispatches d JOIN agents a ON a.id=d.agent_id WHERE d.bounty_id=?1 AND a.status='active' ORDER BY a.id")?;
        let rows = stmt.query_map([&bounty.id], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        if bounty.status != BountyStatus::Open {
            return Err(invalid_transition());
        }
        if bounty.is_blocked {
            return Ok(());
        }
        let mut stmt = tx.prepare("SELECT id FROM agents WHERE status='active' AND ((?1='specific' AND id=?2) OR (?1!='specific' AND (subscribe_all_projects=1 OR EXISTS(SELECT 1 FROM agent_subscriptions s WHERE s.agent_id=agents.id AND s.project_id=?3)) AND (?1='any' OR (CASE tier WHEN 'S' THEN 4 WHEN 'A' THEN 3 WHEN 'B' THEN 2 ELSE 1 END)>=?4))) ORDER BY id")?;
        let rank = match bounty.assignee_min_tier {
            Some(Tier::S) => 4,
            Some(Tier::A) => 3,
            Some(Tier::B) => 2,
            _ => 1,
        };
        let rows = stmt.query_map(
            params![
                bounty.assignee_mode.as_str(),
                bounty.assignee_agent_id,
                bounty.project_id,
                rank
            ],
            |r| r.get::<_, String>(0),
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let kind = if withdrawn {
        "task.withdrawn"
    } else if bounty.assignee_mode == AssigneeMode::Specific {
        "task.assigned"
    } else {
        "task.published"
    };
    let timestamp = now();
    for id in targets {
        let mut data = if withdrawn {
            json!({"bounty_id":bounty.id,"project_id":bounty.project_id})
        } else {
            json!(bounty)
        };
        redact_payload(tx, &mut data, &id)?;
        let payload = json!({"project_id":bounty.project_id,"data":data}).to_string();
        tx.execute("INSERT INTO agent_events(agent_id,bounty_id,event_type,payload_json,created_at) VALUES(?1,?2,?3,?4,?5)",params![id,bounty.id,kind,payload,timestamp])?;
        if !withdrawn {
            tx.execute("INSERT OR IGNORE INTO bounty_dispatches(bounty_id,agent_id,created_at) VALUES(?1,?2,?3)",params![bounty.id,id,timestamp])?;
        }
    }
    if withdrawn {
        tx.execute(
            "DELETE FROM bounty_dispatches WHERE bounty_id=?1",
            [&bounty.id],
        )?;
    }
    Ok(())
}

/// 所有含悬赏快照的事件都按接收者处理；覆盖发布事件的直接快照和评论等事件的嵌套快照。
fn redact_payload(conn: &Connection, data: &mut serde_json::Value, agent_id: &str) -> Result<()> {
    let value = if data.get("bounty").is_some() {
        &mut data["bounty"]
    } else {
        data
    };
    if value.get("blocked_by_summaries").is_some() {
        let mut bounty: Bounty =
            serde_json::from_value(value.clone()).map_err(ApiError::internal)?;
        dependencies::redact(conn, &mut bounty, agent_id)?;
        *value = serde_json::to_value(bounty).map_err(ApiError::internal)?;
    }
    Ok(())
}

/// 函数职责：校验 HTTP(S) 地址及非空白密钥；清空地址同时清空密钥，缺省密钥保留或随机生成。
pub(crate) fn webhook_values(
    url: Option<String>,
    secret: Option<String>,
    old: Option<String>,
) -> Result<(Option<String>, Option<String>)> {
    if let Some(secret) = &secret {
        if secret.trim().is_empty() {
            return Err(ApiError::invalid("webhook_secret 不能为空白"));
        }
    }
    let Some(url) = url else {
        return Ok((None, None));
    };
    let parsed =
        url::Url::parse(&url).map_err(|_| ApiError::invalid("webhook_url 必须为 HTTP(S) URL"))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || url.chars().any(char::is_whitespace)
        || parsed.cannot_be_a_base()
    {
        return Err(ApiError::invalid("webhook_url 必须为 HTTP(S) URL"));
    }
    let secret = match secret.or(old) {
        Some(secret) => secret,
        None => format!("tr_whsec_{}", crate::server::random_admin_token()?),
    };
    Ok((Some(url), Some(secret)))
}

fn event_row(r: &Row<'_>) -> rusqlite::Result<AgentEvent> {
    let payload: serde_json::Value = json_value(r, 3)?;
    Ok(AgentEvent {
        event_id: r.get(0)?,
        bounty_id: r.get(1)?,
        event_type: r.get(2)?,
        project_id: payload["project_id"].as_str().map(str::to_owned),
        data: payload["data"].clone(),
        created_at: r.get(4)?,
    })
}

/// 结构职责：内部单次回调快照；密钥不进入任何公开 DTO。
pub(crate) struct WebhookDelivery {
    pub agent_id: String,
    pub url: String,
    pub secret: String,
    pub event: AgentEvent,
    pub attempt_count: i64,
}

impl Repository {
    /// 函数职责：以一致读快照分页，只查询指定 Agent 的事件；Ack 不隐式受拉取影响。
    pub fn agent_events(&self, id: &str, cursor: Option<i64>, limit: usize) -> Result<AgentInbox> {
        if limit == 0 || cursor.is_some_and(|v| v < 0) {
            return Err(ApiError::invalid("cursor 必须非负，limit 必须大于 0"));
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        active_agent(&tx, id)?;
        let acked_event_id =
            tx.query_row("SELECT acked_event_id FROM agents WHERE id=?1", [id], |r| {
                r.get::<_, i64>(0)
            })?;
        let latest_event_id = tx.query_row(
            "SELECT COALESCE(MAX(event_id),0) FROM agent_events WHERE agent_id=?1",
            [id],
            |r| r.get(0),
        )?;
        let cursor = cursor.unwrap_or(acked_event_id);
        let limit = limit.min(500);
        let mut events = {
            let mut stmt=tx.prepare("SELECT event_id,bounty_id,event_type,payload_json,created_at FROM agent_events WHERE agent_id=?1 AND event_id>?2 ORDER BY event_id LIMIT ?3")?;
            let rows = stmt.query_map(params![id, cursor, (limit + 1) as i64], event_row)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let has_more = events.len() > limit;
        events.truncate(limit);
        let cursor = events.last().map_or(cursor, |e| e.event_id);
        tx.commit()?;
        Ok(AgentInbox {
            events,
            cursor,
            acked_event_id,
            latest_event_id,
            has_more,
        })
    }

    /// 函数职责：原子推进确认游标；超前确认钳位到该 Agent 自身最大事件号，永不回退。
    /// 调用契约：确认是累计的；接收方须先通过 Inbox 补齐并处理此前事件，不能以回调成功替代处理确认。
    pub fn ack_agent_events(&self, id: &str, event_id: i64) -> Result<i64> {
        if event_id < 0 {
            return Err(ApiError::invalid("event_id 必须非负"));
        }
        self.write(|tx| {
            active_agent(tx,id)?;
            tx.execute("UPDATE agents SET acked_event_id=MAX(acked_event_id,MIN(?2,(SELECT COALESCE(MAX(event_id),0) FROM agent_events WHERE agent_id=?1))) WHERE id=?1",params![id,event_id])?;
            Ok(tx.query_row("SELECT acked_event_id FROM agents WHERE id=?1",[id],|r|r.get(0))?)
        })
    }

    /// 函数职责：读取回调公开配置与最近 24 小时失败事件数，永不回显已有密钥。
    pub fn webhook_config(&self, id: &str) -> Result<WebhookConfig> {
        let conn = self.connect()?;
        active_agent(&conn, id)?;
        webhook_config(&conn, id)
    }

    /// 函数职责：原子替换回调配置；仅新生成或显式更新密钥时返回一次明文。
    pub fn update_webhook(&self, id: &str, input: UpdateWebhook) -> Result<WebhookConfig> {
        self.write(|tx| {
            active_agent(tx, id)?;
            let old: Option<String> =
                tx.query_row("SELECT webhook_secret FROM agents WHERE id=?1", [id], |r| {
                    r.get(0)
                })?;
            let reveal =
                input.webhook_url.is_some() && (input.webhook_secret.is_some() || old.is_none());
            let (url, secret) = webhook_values(input.webhook_url, input.webhook_secret, old)?;
            tx.execute(
                "UPDATE agents SET webhook_url=?2,webhook_secret=?3,updated_at=?4 WHERE id=?1",
                params![id, url, secret, now()],
            )?;
            let mut config = webhook_config(tx, id)?;
            if reveal {
                config.webhook_secret = secret;
            }
            Ok(config)
        })
    }

    /// 函数职责：在数据库写锁内登记连接，与停用/吊销互斥，防止鉴权后建连的竞态。
    pub(crate) fn connect_stream(&self, id: &str) -> Result<super::super::push::StreamGuard> {
        self.write(|tx| {
            active_agent(tx, id)?;
            self.hub.connect(id)
        })
    }

    /// 函数职责：记录长连接最近写出时间，同时复核 Agent 仍处于 active 状态。
    pub(crate) fn mark_stream_pushed(&self, id: &str) -> Result<()> {
        self.write(|tx| {
            active_agent(tx, id)?;
            tx.execute(
                "UPDATE agents SET last_pushed_at=?2 WHERE id=?1",
                params![id, now()],
            )?;
            Ok(())
        })
    }

    /// 函数职责：每个 Agent 仅选最小 pending 事件，队头到期才参与本轮投递。
    /// 队头退避时不放行后续事件，也不占用批次名额；转为 delivered/failed 后才放行下一条。
    /// 调用方仍须在发送前复核状态和配置。
    pub(crate) fn due_webhooks(&self) -> Result<Vec<WebhookDelivery>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT e.event_id,e.bounty_id,e.event_type,e.payload_json,e.created_at,
                    a.id,a.webhook_url,a.webhook_secret,e.attempt_count
             FROM agent_events e JOIN agents a ON a.id=e.agent_id
             WHERE e.delivery_status='pending'
               AND e.event_id=(SELECT MIN(e2.event_id) FROM agent_events e2
                               WHERE e2.agent_id=e.agent_id AND e2.delivery_status='pending')
               AND (e.next_attempt_at IS NULL OR e.next_attempt_at<=?1)
               AND a.status='active' AND a.webhook_url IS NOT NULL AND a.webhook_secret IS NOT NULL
             ORDER BY e.event_id LIMIT 32",
        )?;
        let rows = stmt.query_map([now()], |r| {
            Ok(WebhookDelivery {
                event: event_row(r)?,
                agent_id: r.get(5)?,
                url: r.get(6)?,
                secret: r.get(7)?,
                attempt_count: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 函数职责：发送前再次校验状态与配置，配置变更或停用后丢弃旧回调快照。
    pub(crate) fn webhook_current_values(&self, id: &str, url: &str, secret: &str) -> Result<bool> {
        Ok(self.connect()?.query_row("SELECT EXISTS(SELECT 1 FROM agents WHERE id=?1 AND status='active' AND webhook_url=?2 AND webhook_secret=?3)",params![id,url,secret],|r|r.get(0))?)
    }

    /// 函数职责：提交单次投递结果；失败重试时间及次数持久化，成功不推进 Inbox Ack。
    pub(crate) fn finish_webhook(&self, d: &WebhookDelivery, error: Option<String>) -> Result<()> {
        self.write(|tx| {
            let timestamp = now();
            let attempts = d.attempt_count + 1;
            let failed = error.is_some();
            let status = if !failed {
                "delivered"
            } else if attempts >= 5 {
                "failed"
            } else {
                "pending"
            };
            let next_attempt_at = if failed && attempts < 5 {
                let delay = chrono::Duration::milliseconds(100 * (1_i64 << (attempts - 1)));
                Some((Utc::now() + delay).to_rfc3339_opts(SecondsFormat::Millis, true))
            } else {
                None
            };
            tx.execute(
                "UPDATE agent_events SET delivery_status=?2,attempt_count=?3,next_attempt_at=?4,last_error=?5,delivered_at=?6 WHERE event_id=?1 AND delivery_status='pending'",
                params![d.event.event_id, status, attempts, next_attempt_at, error, (!failed).then_some(&timestamp)],
            )?;
            if !failed {
                tx.execute(
                    "UPDATE agents SET last_pushed_at=?2 WHERE id=?1",
                    params![d.agent_id, timestamp],
                )?;
            }
            Ok(())
        })
    }
}

fn webhook_config(conn: &Connection, id: &str) -> Result<WebhookConfig> {
    Ok(conn.query_row("SELECT webhook_url,webhook_secret IS NOT NULL,last_pushed_at,(SELECT COUNT(*) FROM agent_events WHERE agent_id=?1 AND last_error IS NOT NULL AND created_at>=?2) FROM agents WHERE id=?1",params![id,(Utc::now()-chrono::Duration::hours(24)).to_rfc3339_opts(SecondsFormat::Millis,true)],|r| {
        let url:Option<String>=r.get(0)?;
        Ok(WebhookConfig{webhook_configured:url.is_some(),webhook_url:url,has_secret:r.get(1)?,last_pushed_at:r.get(2)?,recent_failed_count:r.get(3)?,webhook_secret:None})
    })?)
}
