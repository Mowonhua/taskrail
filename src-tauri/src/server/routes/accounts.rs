//! 文件职责：为用户、原生 Agent 与 MCP 提供统一账户 HTTP 契约。
use super::*;
use crate::server::{db::accounts::*, McpIdentity};
use axum::{
    extract::DefaultBodyLimit,
    http::{header, HeaderMap, HeaderValue},
    Extension,
};
use tokio::sync::{Semaphore, SemaphorePermit};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/me", get(me).patch(update_me))
        .route("/api/v1/auth/avatars", get(avatars))
        .route("/api/v1/auth/logout", post(logout))
        .layer(DefaultBodyLimit::max(384 * 1024))
        .layer(middleware::from_fn(no_store))
}

async fn no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn account_work_slot() -> Result<SemaphorePermit<'static>> {
    // 限制同时进行的密码校验与图片解码，避免昂贵请求占满阻塞池。
    static SLOTS: Semaphore = Semaphore::const_new(4);
    SLOTS.try_acquire().map_err(|_| {
        ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "AUTH_RATE_LIMITED",
            "请求过多，请稍后重试",
        )
    })
}

fn grant(identity: Option<Extension<McpIdentity>>, role: Role) -> Result<Option<String>> {
    match identity {
        Some(Extension(identity)) if matches!(role, Role::Agent) => Ok(Some(identity.access_hash)),
        Some(_) => Err(ApiError::invalid("MCP 仅支持 Agent 身份")),
        None => Ok(None),
    }
}

async fn register(
    State(state): State<AppState>,
    identity: Option<Extension<McpIdentity>>,
    Body(input): Body<Registration>,
) -> Result<(StatusCode, Json<SignedIn>)> {
    let grant = grant(identity, input.role)?;
    let permit = account_work_slot()?;
    let signed_in = disk(state, move |repo| {
        let _permit = permit;
        repo.register_account(input, grant.as_deref())
    })
    .await?;
    Ok((StatusCode::CREATED, Json(signed_in)))
}

async fn login(
    State(state): State<AppState>,
    identity: Option<Extension<McpIdentity>>,
    Body(input): Body<Login>,
) -> Result<Json<SignedIn>> {
    let grant = grant(identity, input.role)?;
    let permit = account_work_slot()?;
    disk(state, move |repo| {
        let _permit = permit;
        repo.login_account(input, grant.as_deref())
    })
    .await
    .map(Json)
}

fn access(headers: &HeaderMap, identity: Option<Extension<McpIdentity>>) -> Result<Access> {
    if let Some(Extension(identity)) = identity {
        return Ok(Access::Mcp(identity.access_hash));
    }
    agents::bearer(headers)
        .map(|token| Access::Session(token.to_owned()))
        .ok_or_else(sign_in_required)
}

async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
    identity: Option<Extension<McpIdentity>>,
) -> Result<Json<Profile>> {
    let access = access(&headers, identity)?;
    disk(state, move |repo| repo.account_profile(access))
        .await
        .map(Json)
}

async fn update_me(
    State(state): State<AppState>,
    headers: HeaderMap,
    identity: Option<Extension<McpIdentity>>,
    Body(input): Body<ProfilePatch>,
) -> Result<Json<Profile>> {
    let access = access(&headers, identity)?;
    let permit = account_work_slot()?;
    disk(state, move |repo| {
        let _permit = permit;
        repo.update_account_profile(access, input)
    })
    .await
    .map(Json)
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<StatusCode> {
    let token = agents::bearer(&headers)
        .ok_or_else(sign_in_required)?
        .to_owned();
    disk(state, move |repo| repo.logout_account(&token)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn avatars() -> Json<Vec<serde_json::Value>> {
    Json(
        PRESETS
            .iter()
            .map(|(id, label)| json!({"id":id, "label":label}))
            .collect(),
    )
}
