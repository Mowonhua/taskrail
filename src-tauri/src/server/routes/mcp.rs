//! 文件职责：通过 Streamable HTTP 暴露 Agent 工具及 OAuth 授权端点。
use super::*;
use crate::agent_tools::ToolServer;
use crate::server::db::oauth::Grant;
use axum::{
    extract::{DefaultBodyLimit, Form},
    http::{header, HeaderMap, HeaderValue},
    response::Html,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use url::Url;

const SCOPE: &str = "taskrail:agent";
const AUTH: &str = "/oauth/authorize";
const TOKEN: &str = "/oauth/token";
const REGISTER: &str = "/oauth/register";
const DISCOVERY: &str = "/.well-known/oauth-protected-resource/mcp";

#[derive(Clone)]
struct OAuth {
    app: AppState,
    issuer: String,
    authority: String,
    resource: String,
}

pub(super) fn router(app: AppState) -> Router {
    let Ok(issuer) = crate::agent_cli::service_url(&app.public_url) else {
        return Router::new();
    };
    if issuer.scheme() != "https" && !loopback(&issuer) {
        // LAN 或公网通过 HTTPS 反向代理公开 OAuth；本机回环可直接使用 HTTP。
        return Router::new();
    }
    let origin = issuer.origin().ascii_serialization();
    let authority = issuer[url::Position::BeforeHost..url::Position::AfterPort].to_owned();
    let oauth = OAuth {
        app: app.clone(),
        authority,
        resource: format!("{origin}/mcp"),
        issuer: origin,
    };
    let agent_api = Router::new()
        .merge(agents::router())
        .merge(events::router())
        .with_state(app);
    let handler = ToolServer::Http(agent_api);
    let mut config = StreamableHttpServerConfig::default()
        .with_json_response(true)
        .with_allowed_hosts([oauth.authority.clone()])
        .with_allowed_origins([oauth.issuer.clone()]);
    config.legacy_session_mode = false;
    let service: StreamableHttpService<ToolServer, LocalSessionManager> =
        StreamableHttpService::new(move || Ok(handler.clone()), Default::default(), config);
    let endpoint = Router::new()
        .route_service("/mcp", service)
        .route_layer(middleware::from_fn_with_state(oauth.clone(), authenticate));
    Router::new()
        .merge(endpoint)
        .route(DISCOVERY, get(resource_metadata))
        .route(
            "/.well-known/oauth-protected-resource",
            get(resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(server_metadata),
        )
        .route(AUTH, get(authorize_request).post(consent))
        .route(TOKEN, post(exchange))
        .route(REGISTER, post(register_client))
        .layer(DefaultBodyLimit::max(32 * 1024))
        .layer(middleware::from_fn_with_state(
            oauth.clone(),
            trusted_origin,
        ))
        .with_state(oauth)
}

fn loopback(url: &Url) -> bool {
    matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
}

async fn trusted_origin(State(oauth): State<OAuth>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .or_else(|| request.uri().authority().map(|value| value.as_str()));
    let valid_host = host.is_some_and(|value| value.eq_ignore_ascii_case(&oauth.authority));
    let mut origins = headers.get_all(header::ORIGIN).iter();
    let valid_origin = origins
        .next()
        .is_none_or(|origin| origin == oauth.issuer.as_str())
        && origins.next().is_none();
    if !valid_host || !valid_origin {
        return StatusCode::FORBIDDEN.into_response();
    }
    next.run(request).await
}

async fn authenticate(State(oauth): State<OAuth>, mut request: Request, next: Next) -> Response {
    let mut auth = request.headers().get_all(header::AUTHORIZATION).iter();
    let token = auth
        .next()
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, token)| {
            scheme.eq_ignore_ascii_case("bearer") && !token.is_empty() && token.len() <= 256
        })
        .map(|(_, token)| token.to_owned());
    if let Some(token) = token.filter(|_| auth.next().is_none()) {
        let resource = oauth.resource.clone();
        match disk(oauth.app.clone(), move |repo| {
            repo.oauth_identity(&token, &resource)
        })
        .await
        {
            Ok(agent) => {
                request
                    .extensions_mut()
                    .insert(crate::server::McpIdentity(agent));
                request.headers_mut().remove(header::AUTHORIZATION);
                return next.run(request).await;
            }
            Err(error) if error.status.is_server_error() => return error.into_response(),
            Err(_) => {}
        }
    }
    let challenge = format!(
        "Bearer resource_metadata=\"{}{}\", scope=\"{SCOPE}\"",
        oauth.issuer, DISCOVERY
    );
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, challenge)],
        Json(json!({"error":"invalid_token"})),
    )
        .into_response()
}

async fn resource_metadata(State(oauth): State<OAuth>) -> Json<serde_json::Value> {
    Json(json!({
        "resource":oauth.resource, "authorization_servers":[oauth.issuer],
        "bearer_methods_supported":["header"], "scopes_supported":[SCOPE],
    }))
}

async fn server_metadata(State(oauth): State<OAuth>) -> Json<serde_json::Value> {
    let endpoint = |path| format!("{}{path}", oauth.issuer);
    Json(json!({
        "issuer":oauth.issuer, "authorization_endpoint":endpoint(AUTH),
        "token_endpoint":endpoint(TOKEN), "registration_endpoint":endpoint(REGISTER),
        "response_types_supported":["code"], "response_modes_supported":["query"],
        "grant_types_supported":["authorization_code","refresh_token"],
        "token_endpoint_auth_methods_supported":["none"], "code_challenge_methods_supported":["S256"],
        "authorization_response_iss_parameter_supported":true,
        "scopes_supported":[SCOPE],
    }))
}

#[derive(Deserialize)]
struct ClientRegistration {
    redirect_uris: Vec<String>,
    token_endpoint_auth_method: Option<String>,
}

fn redirect_uri(raw: &str) -> Option<Url> {
    let url = Url::parse(raw).ok()?;
    if (url.scheme() == "https" || (url.scheme() == "http" && loopback(&url)))
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
    {
        Some(url)
    } else {
        None
    }
}

fn redirect_matches(registered: &str, requested: &str) -> bool {
    if registered == requested {
        return true;
    }
    let (Some(mut registered), Some(mut requested)) =
        (redirect_uri(registered), redirect_uri(requested))
    else {
        return false;
    };
    // RFC 8252 只允许原生客户端改变 HTTP 回环 IP 地址的端口，路径与查询仍须匹配。
    if registered.scheme() != "http"
        || !matches!(registered.host_str(), Some("127.0.0.1" | "[::1]"))
    {
        return false;
    }
    let _ = registered.set_port(None);
    let _ = requested.set_port(None);
    registered == requested
}

async fn register_client(
    State(oauth): State<OAuth>,
    Json(input): Json<ClientRegistration>,
) -> Response {
    if input
        .token_endpoint_auth_method
        .as_deref()
        .is_some_and(|method| method != "none")
        || !(1..=10).contains(&input.redirect_uris.len())
        || input
            .redirect_uris
            .iter()
            .any(|value| value.len() > 2048 || redirect_uri(value).is_none())
    {
        return oauth_error("invalid_client_metadata");
    }
    let redirects = input.redirect_uris;
    let save = redirects.clone();
    match disk(oauth.app, move |repo| repo.oauth_register_client(&save)).await {
        Ok(client_id) => private((StatusCode::CREATED, Json(json!({
            "client_id":client_id, "redirect_uris":redirects, "token_endpoint_auth_method":"none",
            "grant_types":["authorization_code","refresh_token"], "response_types":["code"],
        }))).into_response()),
        Err(error) => error.into_response(),
    }
}

#[derive(Deserialize)]
struct AuthorizeQuery {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    code_challenge: String,
    code_challenge_method: String,
    resource: Option<String>,
    scope: Option<String>,
    state: Option<String>,
}

async fn authorize_request(
    State(oauth): State<OAuth>,
    Query(query): Query<AuthorizeQuery>,
) -> Response {
    if query.response_type != "code"
        || query.code_challenge_method != "S256"
        || query.code_challenge.len() != 43
        || URL_SAFE_NO_PAD.decode(&query.code_challenge).is_err()
        || query.scope.as_deref().is_some_and(|scope| scope != SCOPE)
        || query
            .resource
            .as_deref()
            .is_some_and(|resource| resource != oauth.resource)
        || query.state.as_ref().is_some_and(|state| state.len() > 1024)
    {
        return oauth_error("invalid_request");
    }
    let Some(redirect) = redirect_uri(&query.redirect_uri) else {
        return oauth_error("invalid_request");
    };
    let client = query.client_id.clone();
    let allowed = match disk(oauth.app.clone(), move |repo| repo.oauth_redirects(&client)).await {
        Ok(Some(redirects)) => redirects
            .iter()
            .any(|uri| redirect_matches(uri, &query.redirect_uri)),
        Ok(None) => false,
        Err(error) => return error.into_response(),
    };
    if !allowed {
        return oauth_error("invalid_request");
    }
    let host = redirect.origin().ascii_serialization();
    let grant = Grant {
        client_id: query.client_id,
        redirect_uri: query.redirect_uri,
        resource: oauth.resource,
        state: query.state,
        challenge: query.code_challenge,
    };
    match disk(oauth.app, move |repo| repo.oauth_begin(grant)).await {
        Ok(flow) => consent_page(&flow, &host, None),
        Err(error) => error.into_response(),
    }
}

fn escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn consent_page(flow: &str, host: &str, error: Option<&str>) -> Response {
    let notice = if let Some(error) = error {
        format!("<p role=\"alert\">{}</p>", escape(error))
    } else {
        format!(
            "<p>授权 <strong>{}</strong> 以 Agent 身份访问 Taskrail。</p>",
            escape(host)
        )
    };
    let html = format!(
        r#"<!doctype html>
<html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>授权 Taskrail MCP</title>
<main><h1>授权 Taskrail MCP</h1>{notice}
<form method="post" action="{AUTH}">
<input type="hidden" name="flow" value="{}">
<p><label>新 Agent 名称 <input name="name" maxlength="64" autocomplete="off"></label></p>
<p><label>邀请令牌或现有 Agent 密钥 <input type="password" name="credential" maxlength="256" autocomplete="off"></label></p>
<button name="decision" value="allow">授权</button>
<button name="decision" value="deny">取消</button>
</form></main></html>"#,
        escape(flow)
    );
    authorization_page(html)
}

fn authorization_page(html: String) -> Response {
    let mut response = private(Html(html).into_response());
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'",
        ),
    );
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

fn continue_after_registration(key: &str, callback: &str) -> Response {
    authorization_page(format!(
        r#"<!doctype html>
<html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>已创建 Taskrail Agent</title>
<main><h1>已创建 Taskrail Agent</h1>
<p>请保存此 Agent 密钥。以后重新连接时，用它授权同一个 Agent。</p>
<p><code>{}</code></p>
<p><a href="{}">返回 Agent 宿主</a></p>
</main></html>"#,
        escape(key),
        escape(callback)
    ))
}

#[derive(Deserialize)]
struct Consent {
    flow: String,
    name: String,
    credential: String,
    decision: String,
}

async fn consent(State(oauth): State<OAuth>, Form(input): Form<Consent>) -> Response {
    if input.flow.len() != 64
        || input.name.len() > 256
        || input.credential.len() > 256
        || !matches!(input.decision.as_str(), "allow" | "deny")
    {
        return oauth_error("invalid_request");
    }
    let denied = input.decision == "deny";
    let flow = input.flow.clone();
    let resource = oauth.resource;
    let issuer = oauth.issuer;
    let result = disk(oauth.app, move |repo| {
        if denied {
            repo.oauth_deny(&flow, &resource)
                .map(|grant| (grant, None, None))
        } else {
            repo.oauth_consent(
                &flow,
                &resource,
                input.name,
                input.credential.trim().to_owned(),
            )
            .map(|result| (result.grant, Some(result.code), result.new_agent_key))
        }
    })
    .await;
    match result {
        Ok((grant, code, new_agent_key)) => {
            let mut url = redirect_uri(&grant.redirect_uri).expect("回调地址已在授权请求校验");
            let mut query = url.query_pairs_mut();
            if let Some(code) = code {
                query.append_pair("code", &code);
            } else {
                query.append_pair("error", "access_denied");
            }
            if let Some(state) = grant.state {
                query.append_pair("state", &state);
            }
            query.append_pair("iss", &issuer);
            drop(query);
            if let Some(key) = new_agent_key {
                return continue_after_registration(&key, url.as_str());
            }
            private((StatusCode::SEE_OTHER, [(header::LOCATION, url.to_string())]).into_response())
        }
        Err(error) if error.status.is_server_error() => error.into_response(),
        Err(error) => consent_page(&input.flow, "", Some(&error.message)),
    }
}

#[derive(Deserialize)]
struct TokenRequest {
    grant_type: String,
    client_id: String,
    resource: Option<String>,
    code: Option<String>,
    redirect_uri: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

async fn exchange(
    State(oauth): State<OAuth>,
    headers: HeaderMap,
    Form(input): Form<TokenRequest>,
) -> Response {
    if headers.contains_key(header::AUTHORIZATION)
        || input.scope.as_deref().is_some_and(|scope| scope != SCOPE)
    {
        return oauth_error("invalid_request");
    }
    if input
        .resource
        .as_deref()
        .is_some_and(|value| value != oauth.resource)
    {
        return oauth_error("invalid_target");
    }
    let resource = oauth.resource;
    let client = input.client_id;
    let result = match input.grant_type.as_str() {
        "authorization_code" => {
            let (Some(code), Some(redirect), Some(verifier)) =
                (input.code, input.redirect_uri, input.code_verifier)
            else {
                return oauth_error("invalid_request");
            };
            if !(43..=128).contains(&verifier.len())
                || !verifier
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-._~".contains(&byte))
            {
                return oauth_error("invalid_grant");
            }
            let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
            disk(oauth.app, move |repo| {
                repo.oauth_exchange(&code, &client, &redirect, &resource, &challenge)
            })
            .await
        }
        "refresh_token" => {
            let Some(refresh) = input.refresh_token else {
                return oauth_error("invalid_request");
            };
            disk(oauth.app, move |repo| {
                repo.oauth_refresh(&refresh, &client, &resource)
            })
            .await
        }
        _ => return oauth_error("unsupported_grant_type"),
    };
    match result {
        Ok(tokens) => private(Json(tokens).into_response()),
        Err(error) if error.status.is_server_error() => error.into_response(),
        Err(error) => oauth_error(error.code),
    }
}

fn private(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response
}

fn oauth_error(error: &str) -> Response {
    private((StatusCode::BAD_REQUEST, Json(json!({"error":error}))).into_response())
}
