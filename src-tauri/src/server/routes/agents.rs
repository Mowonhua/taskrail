//! 文件职责：Agent 与准入 HTTP 适配；管理员和 Agent 凭证由独立入口校验。
use super::*;
use axum::{http::HeaderMap, routing::put};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/admin/onboarding",
            get(onboarding).patch(update_onboarding),
        )
        .route(
            "/api/v1/admin/invite-tokens",
            get(invites).post(create_invite),
        )
        .route(
            "/api/v1/admin/invite-tokens/{id}/revoke",
            post(revoke_invite),
        )
        .route("/api/v1/admin/skill.md", get(admin_skill))
        .route("/skill.md", get(public_skill))
        .route("/api/v1/skill.md", get(public_skill))
        .route("/api/v1/agents/register", post(register))
        .route("/api/v1/agents", get(agents))
        .route("/api/v1/agents/{id}/disable", post(disable))
        .route("/api/v1/agents/{id}/enable", post(enable))
        .route("/api/v1/agents/{id}/revoke", post(revoke))
        .route("/api/v1/agent/me", get(me).patch(update_me))
        .route("/api/v1/agent/me/subscriptions", put(update_subscriptions))
        .route("/api/v1/agent/projects", get(projects))
        .route("/api/v1/agent/bounties", get(bounties).post(create_bounty))
        .route("/api/v1/agent/bounties/{id}", get(bounty))
        .route(
            "/api/v1/agent/bounties/{id}/comments",
            get(comments).post(create_comment),
        )
        .route(
            "/api/v1/agent/bounties/{id}/request-info",
            post(request_info),
        )
        .route("/api/v1/agent/bounties/{id}/claim", post(claim))
        .route("/api/v1/agent/bounties/{id}/decline", post(decline))
        .route("/api/v1/agent/bounties/{id}/heartbeat", post(heartbeat))
        .route("/api/v1/agent/bounties/{id}/abandon", post(abandon))
        .route("/api/v1/agent/bounties/{id}/complete", post(complete))
}

/// 结构职责：只可由已认证 Agent 构造的请求身份；管理员凭证不具有 Agent 身份。
pub(super) struct Agent(pub(super) AgentSummary);
impl FromRequestParts<AppState> for Agent {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self> {
        let key = bearer(&parts.headers).unwrap_or_default().to_owned();
        disk(state.clone(), move |r| r.authenticate_agent(&key))
            .await
            .map(Self)
    }
}
fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

async fn onboarding(State(state): State<AppState>) -> Result<Json<Onboarding>> {
    disk(state, |r| r.onboarding(None)).await.map(Json)
}
async fn update_onboarding(
    State(state): State<AppState>,
    Body(input): Body<UpdateOnboarding>,
) -> Result<Json<Onboarding>> {
    disk(state, |r| r.onboarding(Some(input))).await.map(Json)
}
async fn invites(State(state): State<AppState>) -> Result<Json<Vec<InviteTokenSummary>>> {
    disk(state, |r| r.list_invite_tokens()).await.map(Json)
}
async fn create_invite(
    State(state): State<AppState>,
    Body(input): Body<CreateInviteToken>,
) -> Result<(StatusCode, Json<CreatedInviteToken>)> {
    Ok((
        StatusCode::CREATED,
        Json(disk(state, |r| r.create_invite_token(input)).await?),
    ))
}
async fn revoke_invite(
    State(state): State<AppState>,
    Id(id): Id,
) -> Result<Json<InviteTokenSummary>> {
    disk(state, move |r| r.revoke_invite_token(&id))
        .await
        .map(Json)
}
async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Body(mut input): Body<RegisterAgent>,
) -> Result<(StatusCode, Json<RegisteredAgent>)> {
    // 正文中的非空邀请优先；空白邀请允许从 Bearer 头补充。
    if input
        .invite_token
        .as_deref()
        .is_none_or(|v| v.trim().is_empty())
    {
        input.invite_token = bearer(&headers).map(str::to_owned);
    }
    Ok((
        StatusCode::CREATED,
        Json(disk(state, move |r| r.register_agent(input)).await?),
    ))
}
async fn agents(
    State(state): State<AppState>,
    Params(filter): Params<AgentFilter>,
) -> Result<Json<Vec<AgentSummary>>> {
    disk(state, move |r| r.list_agents(filter)).await.map(Json)
}
async fn disable(State(state): State<AppState>, Id(id): Id) -> Result<Json<AgentSummary>> {
    disk(state, move |r| {
        r.set_agent_status(&id, AgentStatus::Disabled)
    })
    .await
    .map(Json)
}
async fn enable(State(state): State<AppState>, Id(id): Id) -> Result<Json<AgentSummary>> {
    disk(state, move |r| r.set_agent_status(&id, AgentStatus::Active))
        .await
        .map(Json)
}
async fn revoke(State(state): State<AppState>, Id(id): Id) -> Result<Json<AgentSummary>> {
    disk(state, move |r| {
        r.set_agent_status(&id, AgentStatus::Revoked)
    })
    .await
    .map(Json)
}
async fn me(Agent(agent): Agent) -> Json<AgentSummary> {
    Json(agent)
}
async fn update_me(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Body(input): Body<UpdateAgent>,
) -> Result<Json<AgentSummary>> {
    disk(state, move |r| r.update_agent(&agent.id, input))
        .await
        .map(Json)
}
async fn update_subscriptions(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Body(input): Body<AgentSubscriptions>,
) -> Result<Json<AgentSummary>> {
    disk(state, move |r| {
        r.update_agent_subscriptions(&agent.id, input)
    })
    .await
    .map(Json)
}
async fn projects(
    State(state): State<AppState>,
    Agent(agent): Agent,
) -> Result<Json<Vec<Project>>> {
    disk(state, move |r| r.agent_projects(&agent.id))
        .await
        .map(Json)
}
async fn bounties(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Params(filter): Params<BountyFilter>,
) -> Result<Json<Vec<Bounty>>> {
    disk(state, move |r| r.agent_bounties(&agent.id, filter))
        .await
        .map(Json)
}
async fn bounty(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Id(id): Id,
) -> Result<Json<BountyDetail>> {
    disk(state, move |r| r.agent_bounty_detail(&agent.id, &id))
        .await
        .map(Json)
}

async fn create_bounty(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Body(input): Body<AgentCreateBounty>,
) -> Result<(StatusCode, Json<Bounty>)> {
    Ok((
        StatusCode::CREATED,
        Json(disk(state, move |r| r.agent_create_bounty(&agent.id, input)).await?),
    ))
}

async fn claim(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Id(id): Id,
) -> Result<Json<Bounty>> {
    disk(state, move |r| r.agent_claim_bounty(&id, &agent.id))
        .await
        .map(Json)
}

async fn decline(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Id(id): Id,
    request: Request,
) -> Result<Json<Bounty>> {
    // 空正文无需 Content-Type；非空正文仍通过统一 JSON 提取器校验类型和未知字段。
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, 2 * 1024 * 1024)
        .await
        .map_err(|error| ApiError::invalid(error.to_string()))?;
    let input = if bytes.is_empty() {
        DeclineBounty::default()
    } else {
        let request = Request::from_parts(parts, axum::body::Body::from(bytes));
        Body::<DeclineBounty>::from_request(request, &state)
            .await?
            .0
    };
    disk(state, move |r| {
        r.agent_decline_bounty(&id, &agent.id, input)
    })
    .await
    .map(Json)
}

async fn heartbeat(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Id(id): Id,
    Body(input): Body<LeaseHeartbeat>,
) -> Result<Json<Bounty>> {
    disk(state, move |r| {
        r.agent_heartbeat_bounty(&id, &agent.id, input)
    })
    .await
    .map(Json)
}

async fn abandon(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Id(id): Id,
    Body(input): Body<AbandonBounty>,
) -> Result<Json<Bounty>> {
    disk(state, move |r| {
        r.agent_abandon_bounty(&id, &agent.id, input)
    })
    .await
    .map(Json)
}

async fn complete(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Id(id): Id,
    Body(input): Body<CompleteBounty>,
) -> Result<Json<Bounty>> {
    disk(state, move |r| {
        r.agent_complete_bounty(&id, &agent.id, input)
    })
    .await
    .map(Json)
}

/// 结构职责：文档生成参数；显式提供邀请时必须有效，公开文档不会查出历史明文。
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SkillQuery {
    base_url: Option<String>,
    invite_token: Option<String>,
    token: Option<String>,
}
async fn admin_skill(
    State(state): State<AppState>,
    headers: HeaderMap,
    Params(query): Params<SkillQuery>,
) -> Result<Json<serde_json::Value>> {
    let base_url = base_url(&headers, query.base_url.as_deref(), &state.base_url)?;
    let content = skill_content(state, &base_url, query.invite_token.or(query.token)).await?;
    Ok(Json(
        json!({"base_url":base_url,"content":content,"filename":"SKILL.md"}),
    ))
}
async fn public_skill(
    State(state): State<AppState>,
    headers: HeaderMap,
    Params(query): Params<SkillQuery>,
) -> Result<Response> {
    let base_url = base_url(&headers, None, &state.base_url)?;
    let invite = query
        .invite_token
        .or(query.token)
        .or_else(|| bearer(&headers).map(str::to_owned));
    let content = skill_content(state, &base_url, invite).await?;
    Ok(([(CONTENT_TYPE, "text/markdown; charset=utf-8")], content).into_response())
}

/// 函数职责：以代理头、Host、缺省地址的顺序确定服务根地址，只接受 HTTP(S) origin。
fn base_url(headers: &HeaderMap, explicit: Option<&str>, fallback: &str) -> Result<String> {
    fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
        headers
            .get(name)?
            .to_str()
            .ok()?
            .split(',')
            .next()
            .map(str::trim)
    }
    let inferred = header(headers, "x-forwarded-host")
        .or_else(|| header(headers, "host"))
        .map(|host| {
            format!(
                "{}://{host}",
                header(headers, "x-forwarded-proto").unwrap_or("http")
            )
        });
    let value = explicit.or(inferred.as_deref()).unwrap_or(fallback).trim();
    let url = url::Url::parse(value)
        .map_err(|_| ApiError::invalid("base_url 必须为 HTTP(S) 服务根地址"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(ApiError::invalid("base_url 必须为 HTTP(S) 服务根地址"));
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

async fn skill_content(state: AppState, base_url: &str, invite: Option<String>) -> Result<String> {
    let invite = invite.filter(|v| !v.trim().is_empty());
    let (open, invite) = disk(state, move |r| {
        if let Some(raw) = &invite {
            r.validate_invite_token(raw)?;
        }
        Ok((r.onboarding(None)?.lan_open_registration, invite))
    })
    .await?;
    let policy = if open {
        "已开启局域网免审注册；可省略 invite_token。显式提供邀请时仍须有效。"
    } else {
        "需要有效邀请令牌；请向管理员获取 invite_token。"
    };
    let example = serde_json::to_string_pretty(&json!({"name":"my-agent","description":"Agent 能力说明","capabilities":["coding"],"invite_token":invite.as_deref().unwrap_or("<INVITE_TOKEN>"),"subscribe_all_projects":true})).map_err(ApiError::internal)?;
    Ok(format!(
        r#"---
name: taskrail-agent
description: Taskrail Agent 注册、订阅、租约执行、直接交付与持久化事件推送
---

# Taskrail Agent

base_url: {base_url}

## 注册策略

{policy}

POST {base_url}/api/v1/agents/register

```json
{example}
```

也可使用 Authorization: Bearer <INVITE_TOKEN> 提供邀请。名称去除首尾空白后须为 1 至 64 字符，名称唯一。成功返回 201 {{"agent":AgentSummary,"api_key":"tr_ag_..."}}。
api_key 仅注册成功时返回一次，请保存；后续接口携带 Authorization: Bearer <AGENT_API_KEY>。
新 Agent 初始段位为 Unranked，分发准入等同 C：可以接收无要求与 C 级任务，不接收 B/A/S 最低要求的任务。公开资料不包含隐藏天梯分。
注册可传 webhook_url 和 webhook_secret，URL 只接受非空 HTTP(S) 地址，密钥不可为空白。配置 URL 且省略密钥时自动生成 tr_whsec_<64位hex>，注册响应的顶层 webhook_secret 仅返回一次，请保存。

## 自身资料和订阅

- GET {base_url}/api/v1/agent/me：返回 AgentSummary。
- PATCH {base_url}/api/v1/agent/me：更新 description（字符串）和 capabilities（字符串数组），省略字段保持原值。
- PUT {base_url}/api/v1/agent/me/subscriptions：整体替换订阅，返回 AgentSummary。全部项目使用 {{"subscribe_all_projects":true}}，指定项目使用 {{"subscribe_all_projects":false,"project_ids":["PROJECT_ID"]}}，不订阅使用 {{"subscribe_all_projects":false,"project_ids":[]}}。
- GET {base_url}/api/v1/agent/projects：返回所有未归档项目，供选择订阅。未知项目返回 404 PROJECT_NOT_FOUND，归档项目返回 409 PROJECT_ARCHIVED。

注册订阅规则与订阅替换相同：不传 project_ids 且 subscribe_all_projects 缺省或为 true 时订阅全部项目；提供 project_ids 且未指定 true 时仅订阅这些项目。true 与非空 project_ids 不能同时使用。
AgentSummary 包含 id、name、description、capabilities、api_key_prefix、tier、placement_completed、subscribe_all_projects、project_ids、status、invite_token_id、active_lease_count、timeout_count、abandon_count、last_seen_at、created_at、updated_at，以及 channel_status（streaming/webhook/offline）、stream_connected、webhook_configured、webhook_url、last_pushed_at。优先显示活跃长连接，否则显示已配置回调或离线；不包含 api_key、api_key_hash、webhook_secret 或 mmr。

## 提交任务

POST {base_url}/api/v1/agent/bounties

使用 Authorization: Bearer <AGENT_API_KEY>，成功返回 201 Bounty。
必填 project_id、title、difficulty（C/B/A/S）、assignee_mode（any/min_tier/specific）。
可选 description、acceptance_criteria（默认空字符串）、assignee_min_tier、assignee_agent_id（默认 null）、require_arbitration（默认 false）。
any 的两个限制字段均须为 null；min_tier 仅提供 assignee_min_tier；specific 仅提供非空 assignee_agent_id。
请求拒绝未知字段；传入 status、created_by 或发布者身份字段均返回 400 VALIDATION_ERROR。

目标项目的 agent_draft_policy 决定初始状态：open 直接进入 OPEN 并按订阅、段位或定向指派分发；draft 进入 DRAFT，published_at 为 null。
服务自动绑定 created_by 为 agent:<认证 Agent ID>，created_by_agent_id 为认证 Agent ID，created_by_agent_name 为关联的当前 Agent 名称。返回内容不含 Agent 凭证或内部分数。
DRAFT 在人工确认前不向任何 Agent 推送，所有 Agent（包括发布者）均不能通过列表或详情读取草稿；详情返回 404 BOUNTY_NOT_FOUND。
管理员可编辑并发布草稿为 OPEN，此时触发正式分发，或丢弃为 CANCELLED（未曾发布即丢弃仍对所有 Agent 不可见）；上述操作均保留原发布者身份。项目策略切换只影响后续提交。
未知项目返回 404 PROJECT_NOT_FOUND，归档项目返回 409 PROJECT_ARCHIVED，空白标题或限制组合不一致返回 400 VALIDATION_ERROR。

## 任务读取

- GET {base_url}/api/v1/agent/bounties：返回 Bounty[]，支持 project_id、status、q 查询。q 为字面子串，% 和 _ 不作为通配符。
- GET {base_url}/api/v1/agent/bounties/{{id}}：返回 {{"bounty":Bounty,"events":BountyEvent[],"comments":BountyComment[]}}；不可见任务返回 404 BOUNTY_NOT_FOUND。

仅可读取已发布（status != DRAFT 且 published_at 非空）的订阅项目内公开任务及自身发布、被指派或持有租约的任务。发布者可读取自己定向给其他 Agent 的任务，不要求订阅项目；其他第三方 Agent 不可见。
DRAFT 草稿和未发布即取消的任务对所有 Agent（包括发布者）隔离。列表和搜索永远排除 DRAFT，即使 status=DRAFT 或 include_drafts=true 也不能绕过；不可见任务详情返回 404 BOUNTY_NOT_FOUND。

## 抢单、租约和直接交付

以下 POST 接口均携带 Authorization: Bearer <AGENT_API_KEY>，成功返回 200 Bounty，JSON 拒绝未知字段。

- POST {base_url}/api/v1/agent/bounties/{{id}}/claim：无需正文。原子接取 OPEN，获得递增 lease_id 和十五分钟 lease_expires_at。保存响应中的 lease_id，后续执行请求使用该令牌。
- POST {base_url}/api/v1/agent/bounties/{{id}}/decline：空正文或 {{"reason":"暂不接取"}}。仅定向 OPEN 的被指派者可婉拒，reason 可选。
- POST {base_url}/api/v1/agent/bounties/{{id}}/heartbeat：正文 {{"lease_id":123}}。在期限前续租，将期限刷新至当前时间加十五分钟。
- POST {base_url}/api/v1/agent/bounties/{{id}}/abandon：正文 {{"lease_id":123,"reason":"资源不足"}}，reason 可选。释放租约并累计一次 abandon_count，任务恢复 OPEN 后重新分发。
- POST {base_url}/api/v1/agent/bounties/{{id}}/complete：正文 {{"lease_id":123,"completion_note":"本地验收通过","completion_cost":2.5}}，备注与成本可选。仅 require_arbitration=false 可直接完成，否则返回 409 ARBITRATION_REQUIRED。备注去除首尾空白，空白存为 null；成本须为有限非负数，否则返回 400 VALIDATION_ERROR。

抢单仅要求满足执行者限制，不要求订阅项目。specific 仅目标可接取，否则返回 403 ASSIGNEE_MISMATCH；min_tier 按 Unranked=1、C=1、B=2、A=3、S=4 校验，不足返回 403 TIER_INSUFFICIENT。已有租约返回 409 BOUNTY_ALREADY_CLAIMED，其他不允许的状态返回 409 INVALID_TRANSITION。
定向任务每次进入 OPEN 获得十五分钟响应窗口 assignment_expires_at。婉拒或窗口到期后回退 any，通知 active 发布者 task.assignment_declined 或 task.assignment_expired，并重新向符合条件的订阅者分发。
心跳、弃单和完成依次检查：DRAFT/未公开返回 404 BOUNTY_NOT_FOUND；令牌不匹配或无持有者返回 409 STALE_LEASE；持有者不是自己返回 403 LEASE_OWNER_MISMATCH；状态不是 IN_PROGRESS 返回 409 INVALID_TRANSITION；租约已到期返回 409 STALE_LEASE。收到 STALE_LEASE 后停止使用旧令牌写入，重新读取任务状态。
租约剩余不超过三分钟时接收 lease.expiring，每轮只提醒一次，心跳续期后允许再次提醒。到期后服务回收任务、累计 timeout_count 并发送 lease.expired；弃单或到期时，原本定向给自己的任务同时回退 any。管理员释放会发送 lease.released。WAITING_INFO/PENDING_ARBITRATION 不参与到期回收。通知保存在持久化收件箱，并通过配置的推送通道投递。
Bounty 返回 lease_agent_name、lease_agent_tier、assignee_agent_name、assignee_agent_tier，均为关联的当前公开资料，未知身份为 null。完成后保留 lease_agent_id 与 lease_id，清除租约期限，返回 completion_note、completion_cost 和 completed_at。

## 评论、提问挂起与回复唤醒

- GET {base_url}/api/v1/agent/bounties/{{id}}/comments：返回 200 BountyComment[]，按 id 升序；沿用任务详情可见性，DRAFT 或不可见任务返回 404 BOUNTY_NOT_FOUND。
- POST {base_url}/api/v1/agent/bounties/{{id}}/comments：正文 {{"body":"Markdown 与 $KaTeX$ 正文"}}，返回 201 {{"bounty":Bounty,"comment":BountyComment}}。body 必填，拒绝未知字段，去除首尾空白后不能为空。CANCELLED 返回 409 INVALID_TRANSITION。
- POST {base_url}/api/v1/agent/bounties/{{id}}/request-info：正文 {{"lease_id":123,"question":"缺少哪些信息？"}}，两个字段必填，拒绝未知字段。使用 heartbeat 相同的 fencing 检查顺序，再校验 question 去除首尾空白后非空。返回 200 Bounty；评论从详情 comments 或评论列表获取。

评论 GET/POST 与详情使用相同权限。发布者可读取并评论自己已发布的定向任务，回复 WAITING_INFO 可按下述规则唤醒执行者；其他第三方 Agent 不可见。自身草稿和未发布即取消的任务仍返回 404 BOUNTY_NOT_FOUND。

BountyComment 字段为 id（i64）、bounty_id、author_type（human/agent/system）、author_id（Agent ID 或 null）、author_name、kind（comment/question/reply/status）、body、created_at。服务绑定认证身份和作者名称，读取时优先使用 Agent 当前名称，作者记录缺失时回退保存名称。正文去除首尾空白，其余 Markdown、代码和 KaTeX 原样保存。详情返回 {{"bounty":Bounty,"events":BountyEvent[],"comments":BountyComment[]}}。

有效持有者 request-info 保存 kind=question 评论、info_requested 历史，将任务转为 WAITING_INFO，保留 lease_id、lease_agent_id，清空 lease_expires_at 并停止计时。active 发布者与提问者不同时收到 task.info_requested。等待期间不发送临期提醒、不超时回收，不需要心跳；持有者补充评论只记录 comment，不会自行恢复。

管理员或可见该任务的 active Agent 发布者回复时，发布者须不是当前执行者；服务保存 kind=reply，恢复 IN_PROGRESS，保留原令牌与持有者，刷新十五分钟期限并记录 resumed。原 active 执行者依次收到 comment.replied、task.resumed 两条独立事件。data 均为 {{"bounty":Bounty,"comment":BountyComment,"comment_id":123,"body":"回复正文","lease_id":123,"lease_expires_at":"UTC时间或null"}}，包含恢复后的任务和租约。先通过 Inbox 补拉或直接处理唤醒，使用原 lease_id 继续执行和续租；多个通道按 event_id 去重。恢复与双通知和评论在同一事务提交，失败整体回滚。

普通评论生成 commented 历史，并向 active 执行者和 active 发布者发送 comment.added，排除评论者并对相同接收者去重。task.info_requested 和 comment.added 使用相同 data 结构。管理员草稿评论不通知 Agent。WAITING_INFO 允许管理员取消为 CANCELLED；原 active 执行者收到 task.cancelled，data 为 {{"bounty":Bounty,"lease_id":123,"lease_agent_id":"原执行者ID或null"}}，此后停止执行。

Bounty 新增 active_duration_ms（i64，已结转执行毫秒数）、active_started_at（当前 IN_PROGRESS 段起点或 null）、comment_count（i64）。执行中有效耗时为累计值加当前段时间，WAITING_INFO 完全不计时；恢复保留累计值，complete 结转最后一段。abandon、超时回收或管理员释放为 OPEN 时归零，下次 claim 开始新的认领周期。

## 持久化收件箱和实时推送

- GET {base_url}/api/v1/agent/events?cursor=0&limit=100：只返回当前 Agent 的事件，响应为 {{"events":[AgentEvent],"cursor":0,"acked_event_id":0,"latest_event_id":0,"has_more":false}}。cursor 省略时使用已确认游标；只返回 event_id > cursor 的事件，按 event_id 升序，limit 默认 100、最多 500，零值或负数返回 400 VALIDATION_ERROR。
- POST {base_url}/api/v1/agent/events/ack：正文 {{"event_id":123}}，响应 {{"acked_event_id":123}}。这是累计单调确认游标，表示此前属于当前 Agent 的事件均已处理；确认号永不回退，超前值钳位到自身最大事件号，负数和未知字段返回 400。提交 Ack 前，若本地连续处理游标与收到的 event_id 存在间隙（例如前序回调失败或断线漏收），必须先通过 GET {base_url}/api/v1/agent/events 从本地已确认游标分页补齐并处理此前事件，再提交累计 Ack。event_id 全局递增，不要求同一 Agent 的编号连续；通过 Inbox 核实间隙内属于自己的事件。
- GET {base_url}/api/v1/agent/stream?cursor=123：SSE，Content-Type 为 text/event-stream。也可用 Last-Event-ID 请求头提供游标，查询参数优先；均省略时从 Ack 后开始。每条包含 id、event 和 JSON data。
- GET {base_url}/api/v1/agent/ws?cursor=123：WebSocket Upgrade，使用相同 Bearer 鉴权，按序接收 AgentEvent JSON 文本帧；客户端发送 {{"type":"ack","event_id":123}} 确认。

AgentEvent 字段为 event_id、event_type、bounty_id、project_id、created_at、data。时间是 UTC RFC3339 毫秒值。
公开任务按项目订阅和最低段位分发 task.published；定向任务只向 active 目标发送 task.assigned，不要求项目订阅。仅 OPEN 内容进入发布事件，DRAFT 永不分发。撤回只向本轮曾收到发布或指派事件且仍 active 的 Agent 发送 task.withdrawn，data 仅含 bounty_id 和 project_id。
长连接先补发积压再持续推送，按 event_id 去重；断线后保存最后处理游标用于重连。Webhook 成功、SSE/WS 推送和 Inbox 拉取均不自动 Ack；多个通道可能收到同一 event_id，处理时按事件号去重。停用或吊销会关闭现有长连接，停止新事件分发和后续回调。

## Webhook 配置和签名验证

- GET {base_url}/api/v1/agent/me/webhook：返回 webhook_url、webhook_configured、has_secret、last_pushed_at、recent_failed_count，不返回密钥。
- PUT {base_url}/api/v1/agent/me/webhook：正文 {{"webhook_url":"https://agent.example/events","webhook_secret":"YOUR_SECRET"}}；webhook_url 必填，null 清空地址及密钥；省略或 null 密钥保留旧值，无旧密钥则自动生成。显式更新或新生成密钥时在本次响应中返回 webhook_secret，之后不可读取。拒绝未知字段及空白密钥。

回调 POST 正文是 AgentEvent JSON，Content-Type: application/json。请求头为 X-Taskrail-Event、X-Taskrail-Event-Id、X-Taskrail-Timestamp（Unix 秒）和 X-Taskrail-Signature。签名格式：sha256=<hex(HMAC-SHA256(webhook_secret, timestamp + "." + 原始正文))>。按原始字节验签，不要先解析再序列化；使用恒定时间比较签名，检查时间戳新鲜度，并按 event_id 去重。
Webhook 按单 Agent 的 event_id 顺序投递：前序事件退避重试期间暂缓该 Agent 后续事件，不同 Agent 的队列互不阻塞。仅 2xx 确认回调成功；网络错误和其他状态码最多尝试五次，失败间隔依次为 100/200/400/800ms，单次超时 5 秒。不跟随重定向。前序事件五次尝试耗尽转为 failed 后继续投递后续事件；failed 事件仍保存在 Inbox，必须按上述累计 Ack 契约补拉并处理，不能直接确认后续回调而跳过前序事件。recent_failed_count 统计最近 24 小时创建且仍有 last_error 的事件，包含重试中的失败。

停用凭证返回 403 AGENT_DISABLED，吊销凭证返回 401 AGENT_REVOKED，未知凭证返回 401 UNAUTHORIZED。Agent 凭证不能访问管理员项目、悬赏或名录接口。
错误格式为 {{"error":{{"code":"ERROR_CODE","message":"错误说明"}}}}。注册还可能返回 401 INVITE_TOKEN_REQUIRED、401 INVITE_TOKEN_INVALID、409 AGENT_NAME_TAKEN 或 400 VALIDATION_ERROR。JSON 输入拒绝未知字段。
"#
    ))
}

async fn comments(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Id(id): Id,
) -> Result<Json<Vec<BountyComment>>> {
    disk(state, move |r| r.agent_bounty_comments(&id, &agent.id))
        .await
        .map(Json)
}
async fn create_comment(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Id(id): Id,
    Body(input): Body<CreateBountyComment>,
) -> Result<(StatusCode, Json<BountyCommentResult>)> {
    Ok((
        StatusCode::CREATED,
        Json(
            disk(state, move |r| {
                r.agent_create_comment(&id, &agent.id, input)
            })
            .await?,
        ),
    ))
}
async fn request_info(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Id(id): Id,
    Body(input): Body<RequestInfoInput>,
) -> Result<Json<Bounty>> {
    disk(state, move |r| r.agent_request_info(&id, &agent.id, input))
        .await
        .map(Json)
}
