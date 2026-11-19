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

pub(crate) struct ConsentResult {
    pub grant: Grant,
    pub code: String,
}

/// OAuth 协议错误由 HTTP 层映射为协议规定的 JSON 字段。
fn invalid_grant() -> ApiError {
    ApiError::new(
        StatusCode::BAD_REQUEST,
        "invalid_grant",
        "授权已失效或参数不匹配",
    )
}

/// 旧库中的 OAuth 授权均已关联 Agent。保留令牌和代码，为新授权开放空身份。
pub(super) fn migrate_unbound_grants(tx: &Transaction<'_>) -> Result<()> {
    let legacy: bool = tx.query_row(
        "SELECT [notnull] FROM pragma_table_info('mcp_codes') WHERE name='agent_id'",
        [],
        |row| row.get(0),
    )?;
    if !legacy {
        return Ok(());
    }
    tx.execute_batch(
        "ALTER TABLE mcp_codes RENAME TO mcp_codes_legacy;
         ALTER TABLE mcp_tokens RENAME TO mcp_tokens_legacy;",
    )?;
    tx.execute_batch(include_str!("../schema.sql"))?;
    tx.execute_batch(
        "INSERT INTO mcp_codes SELECT * FROM mcp_codes_legacy;
         INSERT INTO mcp_tokens SELECT * FROM mcp_tokens_legacy;
         DROP TABLE mcp_codes_legacy;
         DROP TABLE mcp_tokens_legacy;",
    )?;
    Ok(())
}

fn invalid_token() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "invalid_token",
        "MCP 访问令牌无效",
    )
}

pub(super) fn bound_agent(conn: &Connection, access_hash: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT agent_id FROM mcp_tokens WHERE access_hash=?1 AND access_expires>unixepoch()",
        [access_hash],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(invalid_token)
}

/// 授权只能绑定一次身份。令牌被刷新或并发绑定到另一身份时，调用方回滚账户写入。
pub(super) fn bind_agent(tx: &Transaction<'_>, access_hash: &str, id: &str) -> Result<()> {
    match bound_agent(tx, access_hash)? {
        Some(bound) if bound != id => Err(ApiError::conflict(
            "MCP_IDENTITY_BOUND",
            "此 MCP 授权已关联另一 Agent",
        )),
        Some(_) => Ok(()),
        None => {
            tx.execute(
                "UPDATE mcp_tokens SET agent_id=?2 WHERE access_hash=?1",
                params![access_hash, id],
            )?;
            Ok(())
        }
    }
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

    /// 浏览器只授权宿主连接。Agent 使用工具注册或登录后，授权才关联 Agent。
    pub(crate) fn oauth_consent(&self, flow: &str, resource: &str) -> Result<ConsentResult> {
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
            tx.execute("DELETE FROM mcp_codes WHERE expires <= unixepoch()", [])?;
            tx.execute("INSERT INTO mcp_codes(hash, client_id, redirect_uri, resource, challenge, expires) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![hash(&code), grant.client_id, grant.redirect_uri, grant.resource, grant.challenge, Utc::now().timestamp() + CODE_SECONDS])?;
            Ok(ConsentResult { grant, code })
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
            let agent: Option<String> = tx.query_row(
                "DELETE FROM mcp_codes WHERE hash=?1 AND client_id=?2 AND redirect_uri=?3 AND resource=?4 AND challenge=?5 AND expires > unixepoch() RETURNING agent_id",
                params![hash(code), client, redirect, resource, challenge], |row| row.get(0),
            ).optional()?.ok_or_else(invalid_grant)?;
            if let Some(id) = &agent {
                agents::active_agent(tx, id).map_err(|_| invalid_grant())?;
            }
            issue(tx, client, agent.as_deref(), resource)
        })
    }

    pub(crate) fn oauth_refresh(
        &self,
        refresh: &str,
        client: &str,
        resource: &str,
    ) -> Result<Tokens> {
        self.write(|tx| {
            let agent: Option<String> = tx.query_row(
                "DELETE FROM mcp_tokens WHERE refresh_hash=?1 AND client_id=?2 AND resource=?3 AND refresh_expires > unixepoch() RETURNING agent_id",
                params![hash(refresh), client, resource], |row| row.get(0),
            ).optional()?.ok_or_else(invalid_grant)?;
            if let Some(id) = &agent {
                agents::active_agent(tx, id).map_err(|_| invalid_grant())?;
            }
            issue(tx, client, agent.as_deref(), resource)
        })
    }

    pub(crate) fn oauth_identity(
        &self,
        token: &str,
        resource: &str,
    ) -> Result<super::super::McpIdentity> {
        let access_hash = hash(token);
        self.write(|tx| {
            let id: Option<String> = tx.query_row(
                "SELECT agent_id FROM mcp_tokens WHERE access_hash=?1 AND resource=?2 AND access_expires > unixepoch()",
                params![access_hash, resource], |row| row.get(0),
            ).optional()?.ok_or_else(invalid_token)?;
            let agent = id.map(|id| self.agent_seen(tx, &id)).transpose()?;
            Ok(super::super::McpIdentity { agent, access_hash })
        })
    }
}

fn issue(
    tx: &Transaction<'_>,
    client: &str,
    agent: Option<&str>,
    resource: &str,
) -> Result<Tokens> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrading_oauth_tables_preserves_existing_agent_grants() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("legacy.db");
        let legacy = include_str!("../schema.sql")
            .split("CREATE TABLE IF NOT EXISTS accounts")
            .next()
            .unwrap()
            .replace(
                "\n    agent_id TEXT REFERENCES agents(id)",
                "\n    agent_id TEXT NOT NULL REFERENCES agents(id)",
            );
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(&legacy).unwrap();
        conn.execute_batch(
            "INSERT INTO agents(id,name,api_key_hash,api_key_prefix,created_at,updated_at)
                 VALUES ('agent','legacy','digest','prefix','before','before');
             INSERT INTO mcp_clients VALUES ('client','[]');",
        )
        .unwrap();
        let resource = "http://127.0.0.1/mcp";
        conn.execute(
            "INSERT INTO mcp_tokens VALUES (?1,?2,'client','agent',?3,unixepoch()+3600,unixepoch()+7200)",
            params![hash("legacy-access"), hash("legacy-refresh"), resource],
        ).unwrap();
        drop(conn);

        let repo = Repository::open(&path).unwrap();
        assert_eq!(
            repo.oauth_identity("legacy-access", resource)
                .unwrap()
                .agent
                .unwrap()
                .id,
            "agent"
        );
        let refreshed = repo
            .oauth_refresh("legacy-refresh", "client", resource)
            .unwrap();
        let reopened = Repository::open(&path).unwrap();
        assert_eq!(
            reopened
                .oauth_identity(&refreshed.access_token, resource)
                .unwrap()
                .agent
                .unwrap()
                .id,
            "agent"
        );
    }
}
