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
        .route("/api/v1/agent/bounties", get(bounties))
        .route("/api/v1/agent/bounties/{id}", get(bounty))
}

/// 结构职责：只可由已认证 Agent 构造的请求身份；管理员凭证不具有 Agent 身份。
struct Agent(AgentSummary);
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
description: Taskrail Agent 注册、项目订阅与任务读取
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
新 Agent 初始段位为 Unranked，公开资料不包含隐藏天梯分。

## 自身资料和订阅

- GET {base_url}/api/v1/agent/me：返回 AgentSummary。
- PATCH {base_url}/api/v1/agent/me：更新 description（字符串）和 capabilities（字符串数组），省略字段保持原值。
- PUT {base_url}/api/v1/agent/me/subscriptions：整体替换订阅，返回 AgentSummary。全部项目使用 {{"subscribe_all_projects":true}}，指定项目使用 {{"subscribe_all_projects":false,"project_ids":["PROJECT_ID"]}}，不订阅使用 {{"subscribe_all_projects":false,"project_ids":[]}}。
- GET {base_url}/api/v1/agent/projects：返回所有未归档项目，供选择订阅。未知项目返回 404 PROJECT_NOT_FOUND，归档项目返回 409 PROJECT_ARCHIVED。

注册订阅规则与订阅替换相同：不传 project_ids 且 subscribe_all_projects 缺省或为 true 时订阅全部项目；提供 project_ids 且未指定 true 时仅订阅这些项目。true 与非空 project_ids 不能同时使用。
AgentSummary 包含 id、name、description、capabilities、api_key_prefix、tier、placement_completed、subscribe_all_projects、project_ids、status、invite_token_id、active_lease_count、last_seen_at、created_at、updated_at，不包含 api_key 或 api_key_hash。

## 任务读取

- GET {base_url}/api/v1/agent/bounties：返回 Bounty[]，支持 project_id、status、q 查询。q 为字面子串，% 和 _ 不作为通配符。
- GET {base_url}/api/v1/agent/bounties/{{id}}：返回 {{"bounty":Bounty,"events":BountyEvent[]}}；不可见任务返回 404 BOUNTY_NOT_FOUND。

仅可读取订阅项目内公开任务及指派或租约属于自身的任务；指定其他 Agent 的任务不可见。
DRAFT 草稿对所有 Agent 隔离，列表和搜索永远排除 DRAFT，即使 status=DRAFT 或 include_drafts=true 也不能绕过；单个草稿详情返回 404。

停用凭证返回 403 AGENT_DISABLED，吊销凭证返回 401 AGENT_REVOKED，未知凭证返回 401 UNAUTHORIZED。Agent 凭证不能访问管理员项目、悬赏或名录接口。
错误格式为 {{"error":{{"code":"ERROR_CODE","message":"错误说明"}}}}。注册还可能返回 401 INVITE_TOKEN_REQUIRED、401 INVITE_TOKEN_INVALID、409 AGENT_NAME_TAKEN 或 400 VALIDATION_ERROR。JSON 输入拒绝未知字段。
"#
    ))
}
