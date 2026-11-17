//! 文件职责：HTTP 鉴权、输入提取与仓储适配；SQLite 调用在阻塞线程池执行。
use super::{
    db::Repository,
    error::{ApiError, Result},
    models::*,
    AppState,
};
use axum::{
    extract::{FromRequest, FromRequestParts, Path, Query, Request, State},
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        request::Parts,
        Method, StatusCode,
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tower_http::cors::{Any, CorsLayer};

mod agents;
mod events;

pub(super) fn router(state: AppState) -> Router {
    Router::new()
        .merge(agents::router())
        .merge(events::router())
        .route("/api/v1/health", get(health))
        .route("/api/v1/admin/status", get(admin_status))
        .route(
            "/api/v1/admin/arbitration",
            get(arbitration_config).patch(update_arbitration_config),
        )
        .route(
            "/api/v1/bounties/{id}/arbitration/approve",
            post(approve_arbitration),
        )
        .route(
            "/api/v1/bounties/{id}/arbitration/reject",
            post(reject_arbitration),
        )
        .route("/api/v1/projects", get(list_projects).post(create_project))
        .route(
            "/api/v1/projects/{id}",
            get(get_project)
                .patch(update_project)
                .delete(delete_project),
        )
        .route("/api/v1/bounties", get(list_bounties).post(create_bounty))
        .route(
            "/api/v1/bounties/{id}",
            get(bounty_detail).patch(update_bounty),
        )
        .route(
            "/api/v1/bounties/{id}/comments",
            get(bounty_comments).post(create_bounty_comment),
        )
        .route("/api/v1/bounties/{id}/publish", post(publish_bounty))
        .route("/api/v1/bounties/{id}/withdraw", post(withdraw_bounty))
        .route("/api/v1/bounties/{id}/cancel", post(cancel_bounty))
        .route("/api/v1/bounties/{id}/release-lease", post(release_lease))
        .fallback(|| async { ApiError::new(StatusCode::NOT_FOUND, "NOT_FOUND", "接口不存在") })
        .method_not_allowed_fallback(|| async {
            ApiError::new(
                StatusCode::METHOD_NOT_ALLOWED,
                "METHOD_NOT_ALLOWED",
                "请求方法不支持",
            )
        })
        .layer(middleware::from_fn_with_state(state.clone(), authorize))
        // CORS 必须位于鉴权外层，使预检和 401 响应也包含允许跨域的响应头。
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_headers([
                    AUTHORIZATION,
                    CONTENT_TYPE,
                    axum::http::HeaderName::from_static("last-event-id"),
                ])
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PATCH,
                    Method::PUT,
                    Method::DELETE,
                    Method::OPTIONS,
                ]),
        )
        .with_state(state)
}

async fn authorize(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let protected = path != "/api/v1/agents/register"
        && [
            "/api/v1/projects",
            "/api/v1/bounties",
            "/api/v1/admin",
            "/api/v1/agents",
        ]
        .iter()
        .any(|prefix| {
            path == *prefix
                || path
                    .strip_prefix(prefix)
                    .is_some_and(|tail| tail.starts_with('/'))
        });
    if protected {
        let token = request
            .headers()
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        let digest: [u8; 32] = Sha256::digest(token.unwrap_or("").as_bytes()).into();
        if token.is_none() || !bool::from(digest.ct_eq(&state.token_digest)) {
            return ApiError::new(
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "需要有效的管理员 Bearer token",
            )
            .into_response();
        }
    }
    next.run(request).await
}

/// 提取错误统一转换为 JSON 信封；JSON 语法、字段类型、枚举和查询参数错误均为 400。
struct Body<T>(T);
impl<S: Send + Sync, T: DeserializeOwned + Send> FromRequest<S> for Body<T> {
    type Rejection = ApiError;
    async fn from_request(request: Request, state: &S) -> Result<Self> {
        Json::<T>::from_request(request, state)
            .await
            .map(|Json(v)| Self(v))
            .map_err(|e| ApiError::invalid(e.body_text()))
    }
}
struct Params<T>(T);
impl<S: Send + Sync, T: DeserializeOwned + Send> FromRequestParts<S> for Params<T> {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self> {
        Query::<T>::from_request_parts(parts, state)
            .await
            .map(|Query(v)| Self(v))
            .map_err(|e| ApiError::invalid(e.body_text()))
    }
}
struct Id(String);
impl<S: Send + Sync> FromRequestParts<S> for Id {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self> {
        Path::<String>::from_request_parts(parts, state)
            .await
            .map(|Path(id)| Self(id))
            .map_err(|e| ApiError::invalid(e.body_text()))
    }
}
async fn disk<T: Send + 'static>(
    state: AppState,
    operation: impl FnOnce(Repository) -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(move || operation(state.repository))
        .await
        .map_err(ApiError::internal)?
}
async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({"ok":true,"mode":state.mode,"version":env!("CARGO_PKG_VERSION")}))
}
async fn admin_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(
        json!({"ok":true,"mode":state.mode,"db_path":state.repository.path(),"version":env!("CARGO_PKG_VERSION")}),
    )
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ProjectFilter {
    include_archived: bool,
}
async fn list_projects(
    State(state): State<AppState>,
    Params(f): Params<ProjectFilter>,
) -> Result<Json<Vec<Project>>> {
    disk(state, move |r| r.list_projects(f.include_archived))
        .await
        .map(Json)
}
async fn create_project(
    State(state): State<AppState>,
    Body(input): Body<CreateProject>,
) -> Result<(StatusCode, Json<Project>)> {
    Ok((
        StatusCode::CREATED,
        Json(disk(state, move |r| r.create_project(input)).await?),
    ))
}
async fn get_project(State(state): State<AppState>, Id(id): Id) -> Result<Json<Project>> {
    disk(state, move |r| r.get_project(&id)).await.map(Json)
}
async fn update_project(
    State(state): State<AppState>,
    Id(id): Id,
    Body(input): Body<UpdateProject>,
) -> Result<Json<Project>> {
    disk(state, move |r| r.update_project(&id, input))
        .await
        .map(Json)
}
async fn delete_project(State(state): State<AppState>, Id(id): Id) -> Result<StatusCode> {
    disk(state, move |r| r.delete_project(&id)).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn list_bounties(
    State(state): State<AppState>,
    Params(f): Params<BountyFilter>,
) -> Result<Json<Vec<Bounty>>> {
    disk(state, move |r| r.list_bounties(f)).await.map(Json)
}
async fn create_bounty(
    State(state): State<AppState>,
    Body(input): Body<CreateBounty>,
) -> Result<(StatusCode, Json<Bounty>)> {
    Ok((
        StatusCode::CREATED,
        Json(disk(state, move |r| r.create_bounty(input)).await?),
    ))
}
async fn bounty_detail(State(state): State<AppState>, Id(id): Id) -> Result<Json<BountyDetail>> {
    disk(state, move |r| r.get_bounty_detail(&id))
        .await
        .map(Json)
}
async fn update_bounty(
    State(state): State<AppState>,
    Id(id): Id,
    Body(input): Body<UpdateBounty>,
) -> Result<Json<Bounty>> {
    disk(state, move |r| r.update_bounty(&id, input, "human"))
        .await
        .map(Json)
}
async fn publish_bounty(State(state): State<AppState>, Id(id): Id) -> Result<Json<Bounty>> {
    disk(state, move |r| r.publish_bounty(&id, "human"))
        .await
        .map(Json)
}
async fn release_lease(State(state): State<AppState>, Id(id): Id) -> Result<Json<Bounty>> {
    disk(state, move |r| r.admin_release_lease(&id))
        .await
        .map(Json)
}
async fn withdraw_bounty(State(state): State<AppState>, Id(id): Id) -> Result<Json<Bounty>> {
    disk(state, move |r| r.withdraw_bounty(&id, "human"))
        .await
        .map(Json)
}
async fn cancel_bounty(State(state): State<AppState>, Id(id): Id) -> Result<Json<Bounty>> {
    disk(state, move |r| r.cancel_bounty(&id, "human"))
        .await
        .map(Json)
}

async fn bounty_comments(
    State(state): State<AppState>,
    Id(id): Id,
) -> Result<Json<Vec<BountyComment>>> {
    disk(state, move |r| r.bounty_comments(&id)).await.map(Json)
}
async fn create_bounty_comment(
    State(state): State<AppState>,
    Id(id): Id,
    Body(input): Body<CreateBountyComment>,
) -> Result<(StatusCode, Json<BountyCommentResult>)> {
    Ok((
        StatusCode::CREATED,
        Json(disk(state, move |r| r.create_bounty_comment(&id, input)).await?),
    ))
}

async fn arbitration_config(State(state): State<AppState>) -> Result<Json<ArbitrationConfig>> {
    disk(state, move |r| r.arbitration_config(None))
        .await
        .map(Json)
}
async fn update_arbitration_config(
    State(state): State<AppState>,
    Body(input): Body<ArbitrationConfig>,
) -> Result<Json<ArbitrationConfig>> {
    disk(state, move |r| r.arbitration_config(Some(input)))
        .await
        .map(Json)
}

/// 空正文无需 Content-Type；非空正文仍应用统一 JSON 字段校验和大小限制。
async fn optional_body<T: DeserializeOwned + Default + Send>(
    request: Request,
    state: &AppState,
) -> Result<T> {
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, 2 * 1024 * 1024)
        .await
        .map_err(|e| ApiError::invalid(e.to_string()))?;
    if bytes.is_empty() {
        return Ok(T::default());
    }
    Ok(Body::<T>::from_request(
        Request::from_parts(parts, axum::body::Body::from(bytes)),
        state,
    )
    .await?
    .0)
}
async fn approve_arbitration(
    State(state): State<AppState>,
    Id(id): Id,
    request: Request,
) -> Result<Json<Bounty>> {
    let input = optional_body::<ApproveArbitrationInput>(request, &state).await?;
    disk(state, move |r| r.approve_arbitration(&id, None, input))
        .await
        .map(Json)
}
async fn reject_arbitration(
    State(state): State<AppState>,
    Id(id): Id,
    Body(input): Body<RejectArbitrationInput>,
) -> Result<Json<Bounty>> {
    disk(state, move |r| r.reject_arbitration(&id, None, input))
        .await
        .map(Json)
}
