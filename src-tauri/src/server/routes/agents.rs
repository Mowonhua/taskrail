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
        .route(
            "/api/v1/admin/invite-tokens/{id}",
            axum::routing::delete(delete_invite),
        )
        .route("/api/v1/admin/skill.md", get(admin_skill))
        .route("/skill.md", get(public_skill))
        .route("/api/v1/skill.md", get(public_skill))
        .route("/api/v1/agents/register", post(register))
        .route("/api/v1/agents", get(agents))
        .route("/api/v1/agents/{id}", get(agent_detail))
        .route("/api/v1/agents/{id}/disable", post(disable))
        .route("/api/v1/agents/{id}/enable", post(enable))
        .route("/api/v1/agents/{id}/revoke", post(revoke))
        .route("/api/v1/agent/me", get(me).patch(update_me))
        .route("/api/v1/agent/me/subscriptions", put(update_subscriptions))
        .route("/api/v1/agent/projects", get(projects))
        .route("/api/v1/agent/bounties", get(bounties).post(create_bounty))
        .route(
            "/api/v1/agent/bounties/{id}",
            get(bounty).patch(update_draft),
        )
        .route("/api/v1/agent/bounties/{id}/split", post(split_draft))
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
        .route(
            "/api/v1/agent/bounties/{id}/submit-arbitration",
            post(submit_arbitration),
        )
        .route(
            "/api/v1/agent/bounties/{id}/arbitration/approve",
            post(approve_arbitration),
        )
        .route(
            "/api/v1/agent/bounties/{id}/arbitration/reject",
            post(reject_arbitration),
        )
}

/// 结构职责：只可由已认证 Agent 构造的请求身份；管理员凭证不具有 Agent 身份。
pub(super) struct Agent(pub(super) AgentSummary);
impl FromRequestParts<AppState> for Agent {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self> {
        if let Some(identity) = parts.extensions.remove::<crate::server::McpIdentity>() {
            return Ok(Self(identity.0));
        }
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
async fn delete_invite(State(state): State<AppState>, Id(id): Id) -> Result<StatusCode> {
    disk(state, move |r| r.delete_invite_token(&id)).await?;
    Ok(StatusCode::NO_CONTENT)
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
async fn agent_detail(State(state): State<AppState>, Id(id): Id) -> Result<Json<AgentDetail>> {
    disk(state, move |r| r.get_agent_detail(&id))
        .await
        .map(Json)
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

async fn update_draft(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Id(id): Id,
    Body(input): Body<AgentUpdateDraftInput>,
) -> Result<Json<Bounty>> {
    disk(state, move |r| r.agent_update_draft(&id, &agent.id, input))
        .await
        .map(Json)
}
async fn split_draft(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Id(id): Id,
    Body(input): Body<SplitDraftInput>,
) -> Result<(StatusCode, Json<SplitDraftResult>)> {
    Ok((
        StatusCode::CREATED,
        Json(disk(state, move |r| r.agent_split_draft(&id, &agent.id, input)).await?),
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
    let open = disk(state, move |r| {
        if let Some(raw) = &invite {
            r.validate_invite_token(raw)?;
        }
        Ok(r.onboarding(None)?.lan_open_registration)
    })
    .await?;
    let policy = if open {
        "已开启局域网免审注册；可省略 invite_token。显式提供邀请时仍须有效。"
    } else {
        "需要有效邀请令牌；请向管理员获取 invite_token。"
    };
    Ok(crate::agent_skill::render(base_url, policy))
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

async fn submit_arbitration(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Id(id): Id,
    Body(input): Body<SubmitArbitrationInput>,
) -> Result<Json<Bounty>> {
    disk(state, move |r| {
        r.agent_submit_arbitration(&id, &agent.id, input)
    })
    .await
    .map(Json)
}
async fn approve_arbitration(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Id(id): Id,
    Body(input): Body<ApproveArbitrationInput>,
) -> Result<Json<Bounty>> {
    disk(state, move |r| {
        r.approve_arbitration(&id, Some(&agent.id), input)
    })
    .await
    .map(Json)
}
async fn reject_arbitration(
    State(state): State<AppState>,
    Agent(agent): Agent,
    Id(id): Id,
    Body(input): Body<RejectArbitrationInput>,
) -> Result<Json<Bounty>> {
    disk(state, move |r| {
        r.reject_arbitration(&id, Some(&agent.id), input)
    })
    .await
    .map(Json)
}
