//! 文件职责：持久化 OAuth 客户端、一次性授权代码与可轮换令牌。
use super::{agents, *};

pub(crate) const ACCESS_SECONDS: i64 = 3600;
const REFRESH_SECONDS: i64 = 30 * 24 * 3600;
const FLOW_SECONDS: i64 = 600;
const CODE_SECONDS: i64 = 120;

pub(crate) fn hash(value: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct Grant {
    pub client_id: String,
    pub redirect_uri: String,
    pub resource: String,
    pub state: Option<String>,
    pub challenge: String,
}

#[derive(serde::Serialize)]
pub(crate) struct Tokens {
    access_token: String,
    refresh_token: String,
    token_type: &'static str,
    expires_in: i64,
    scope: &'static str,
}

/// 浏览器仅在新建 Agent 时显示一次密钥，供 OAuth 失效后重新授权同一身份。
pub(crate) struct ConsentResult {
    pub grant: Grant,
    pub code: String,
    pub new_agent_key: Option<String>,
}

/// OAuth 协议错误由 HTTP 层映射为协议规定的 JSON 字段。
fn invalid_grant() -> ApiError {
    ApiError::new(
        StatusCode::BAD_REQUEST,
        "invalid_grant",
        "授权已失效或参数不匹配",
    )
}

impl Repository {
    pub(crate) fn oauth_register_client(&self, redirects: &[String]) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        self.write(|tx| {
            tx.execute(
                "INSERT INTO mcp_clients(id, redirects) VALUES (?1, ?2)",
                params![id, json!(redirects).to_string()],
            )?;
            Ok(id)
        })
    }

    pub(crate) fn oauth_redirects(&self, id: &str) -> Result<Option<Vec<String>>> {
        let conn = self.connect()?;
        let raw: Option<String> = conn
            .query_row(
                "SELECT redirects FROM mcp_clients WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()?;
        raw.map(|raw| serde_json::from_str(&raw).map_err(ApiError::internal))
            .transpose()
    }

    pub(crate) fn oauth_begin(&self, grant: Grant) -> Result<String> {
        let flow = super::super::random_admin_token()?;
        let expires = Utc::now().timestamp() + FLOW_SECONDS;
        self.write(|tx| {
            tx.execute("DELETE FROM mcp_flows WHERE expires <= unixepoch()", [])?;
            tx.execute(
                "INSERT INTO mcp_flows(hash, grant_json, expires) VALUES (?1, ?2, ?3)",
                params![hash(&flow), json!(grant).to_string(), expires],
            )?;
            Ok(flow)
        })
    }

    /// 消耗授权表单与发码在同一事务内。无效邀请、停用身份和重复提交不会创建授权。
    pub(crate) fn oauth_consent(
        &self,
        flow: &str,
        resource: &str,
        name: String,
        credential: String,
    ) -> Result<ConsentResult> {
        let code = super::super::random_admin_token()?;
        self.write(|tx| {
            let raw: String = tx.query_row(
                "DELETE FROM mcp_flows WHERE hash=?1 AND expires > unixepoch() RETURNING grant_json",
                [hash(flow)], |row| row.get(0),
            ).optional()?.ok_or_else(invalid_grant)?;
            let grant: Grant = serde_json::from_str(&raw).map_err(ApiError::internal)?;
            if grant.resource != resource {
                return Err(invalid_grant());
            }
            let (agent_id, new_agent_key) = if credential.starts_with("tr_ag_") {
                let id = agents::agent_id_for_key(tx, &credential)?;
                agents::active_agent(tx, &id)?;
                (id, None)
            } else {
                let agent = self.register_agent_in(tx, RegisterAgent {
                    name, description: String::new(), capabilities: Vec::new(),
                    invite_token: (!credential.is_empty()).then_some(credential),
                    subscribe_all_projects: None, project_ids: None,
                    webhook_url: None, webhook_secret: None,
                })?;
                (agent.agent.id, Some(agent.api_key))
            };
            tx.execute("DELETE FROM mcp_codes WHERE expires <= unixepoch()", [])?;
            tx.execute("INSERT INTO mcp_codes(hash, agent_id, client_id, redirect_uri, resource, challenge, expires) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![hash(&code), agent_id, grant.client_id, grant.redirect_uri, grant.resource, grant.challenge, Utc::now().timestamp() + CODE_SECONDS])?;
            Ok(ConsentResult { grant, code, new_agent_key })
        })
    }

    pub(crate) fn oauth_deny(&self, flow: &str, resource: &str) -> Result<Grant> {
        self.write(|tx| {
            let raw: String = tx.query_row(
                "DELETE FROM mcp_flows WHERE hash=?1 AND expires > unixepoch() RETURNING grant_json",
                [hash(flow)], |row| row.get(0),
            ).optional()?.ok_or_else(invalid_grant)?;
            let grant: Grant = serde_json::from_str(&raw).map_err(ApiError::internal)?;
            if grant.resource != resource {
                return Err(invalid_grant());
            }
            Ok(grant)
        })
    }

    /// 交换绑定客户端、回调、PKCE 和资源的授权代码；只允许一次成功提交。
    pub(crate) fn oauth_exchange(
        &self,
        code: &str,
        client: &str,
        redirect: &str,
        resource: &str,
        challenge: &str,
    ) -> Result<Tokens> {
        self.write(|tx| {
            let agent: String = tx.query_row(
                "DELETE FROM mcp_codes WHERE hash=?1 AND client_id=?2 AND redirect_uri=?3 AND resource=?4 AND challenge=?5 AND expires > unixepoch() RETURNING agent_id",
                params![hash(code), client, redirect, resource, challenge], |row| row.get(0),
            ).optional()?.ok_or_else(invalid_grant)?;
            agents::active_agent(tx, &agent).map_err(|_| invalid_grant())?;
            issue(tx, client, &agent, resource)
        })
    }

    pub(crate) fn oauth_refresh(
        &self,
        refresh: &str,
        client: &str,
        resource: &str,
    ) -> Result<Tokens> {
        self.write(|tx| {
            let agent: String = tx.query_row(
                "DELETE FROM mcp_tokens WHERE refresh_hash=?1 AND client_id=?2 AND resource=?3 AND refresh_expires > unixepoch() RETURNING agent_id",
                params![hash(refresh), client, resource], |row| row.get(0),
            ).optional()?.ok_or_else(invalid_grant)?;
            agents::active_agent(tx, &agent).map_err(|_| invalid_grant())?;
            issue(tx, client, &agent, resource)
        })
    }

    pub(crate) fn oauth_identity(&self, token: &str, resource: &str) -> Result<AgentSummary> {
        self.write(|tx| {
            let id: String = tx.query_row(
                "SELECT agent_id FROM mcp_tokens WHERE access_hash=?1 AND resource=?2 AND access_expires > unixepoch()",
                params![hash(token), resource], |row| row.get(0),
            ).optional()?.ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "invalid_token", "MCP 访问令牌无效"))?;
            self.agent_seen(tx, &id)
        })
    }
}

fn issue(tx: &Transaction<'_>, client: &str, agent: &str, resource: &str) -> Result<Tokens> {
    let access = super::super::random_admin_token()?;
    let refresh = super::super::random_admin_token()?;
    let now = Utc::now().timestamp();
    tx.execute("DELETE FROM mcp_tokens WHERE refresh_expires <= ?1", [now])?;
    tx.execute("INSERT INTO mcp_tokens(access_hash, refresh_hash, client_id, agent_id, resource, access_expires, refresh_expires) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![hash(&access), hash(&refresh), client, agent, resource, now + ACCESS_SECONDS, now + REFRESH_SECONDS])?;
    Ok(Tokens {
        access_token: access,
        refresh_token: refresh,
        token_type: "Bearer",
        expires_in: ACCESS_SECONDS,
        scope: "taskrail:agent",
    })
}
