//! 文件职责：管理用户和 Agent 共用的账户、资料与登录会话。
use super::{agents, oauth, *};
use argon2::{
    password_hash::{PasswordHash, SaltString},
    Argon2, PasswordHasher, PasswordVerifier,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::{io::Cursor, sync::OnceLock};

const SESSION_SECONDS: i64 = 30 * 24 * 3600;
const MAX_AVATAR_BYTES: usize = 256 * 1024;
pub(crate) const PRESETS: &[(&str, &str)] = &[
    ("slate", "石板灰"),
    ("blue", "蓝色"),
    ("green", "绿色"),
    ("amber", "琥珀色"),
    ("purple", "紫色"),
    ("rose", "玫瑰色"),
];

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Role {
    User,
    Agent,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
        }
    }
}

/// 系统预制头像由 ID 标识，自定义头像保存经过解码验证的限长图片。
#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Avatar {
    Preset {
        id: String,
    },
    Upload {
        media_type: String,
        data_base64: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Registration {
    pub role: Role,
    pub name: String,
    pub password: String,
    pub avatar: Option<Avatar>,
    pub invite_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Login {
    pub role: Role,
    pub name: Option<String>,
    pub password: Option<String>,
    pub agent_key: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProfilePatch {
    name: Option<String>,
    // 省略字段保留头像，显式 null 清除头像。
    #[serde(default, deserialize_with = "present_avatar")]
    avatar: Option<Option<Avatar>>,
}

fn present_avatar<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<Option<Avatar>>, D::Error> {
    Option::deserialize(d).map(Some)
}

#[derive(Serialize)]
pub(crate) struct Profile {
    id: String,
    role: Role,
    name: String,
    avatar: Option<Avatar>,
}

#[derive(Serialize)]
pub(crate) struct SignedIn {
    /// HTTP MCP 通过已有的 OAuth 授权继续调用，不另发账户会话令牌。
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
    profile: Profile,
}

pub(crate) enum Access {
    Session(String),
    Mcp(String),
}

pub(crate) fn sign_in_required() -> ApiError {
    ApiError::new(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", "请先注册或登录")
}

fn invalid_credentials() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "INVALID_CREDENTIALS",
        "名称或凭证不正确",
    )
}

fn valid_name(raw: &str) -> Result<&str> {
    let name = raw.trim();
    if !(1..=64).contains(&name.chars().count()) || name.chars().any(char::is_control) {
        return Err(ApiError::invalid("名称须为 1 至 64 字符且不能包含控制字符"));
    }
    Ok(name)
}

fn validate_password(password: &str) -> Result<()> {
    if !(12..=128).contains(&password.chars().count()) {
        return Err(ApiError::invalid("密码须为 12 至 128 字符"));
    }
    Ok(())
}

fn password_hash(password: &str) -> Result<String> {
    let mut random = [0; 16];
    getrandom::fill(&mut random).map_err(ApiError::internal)?;
    let salt = SaltString::encode_b64(&random).map_err(ApiError::internal)?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(ApiError::internal)
}

fn verify_password(password: &str, hash: Option<&str>) -> Result<()> {
    static DUMMY: OnceLock<String> = OnceLock::new();
    let encoded = match hash {
        Some(hash) => hash,
        None => {
            if DUMMY.get().is_none() {
                let _ = DUMMY.set(password_hash("taskrail-unavailable-account")?);
            }
            DUMMY.get().expect("占位密码摘要已初始化")
        }
    };
    let parsed = PasswordHash::new(encoded).map_err(ApiError::internal)?;
    let valid = Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok();
    if valid && hash.is_some() {
        Ok(())
    } else {
        Err(invalid_credentials())
    }
}

fn encode_avatar(avatar: Avatar) -> Result<String> {
    match &avatar {
        Avatar::Preset { id } if !PRESETS.iter().any(|(preset, _)| preset == id) => {
            return Err(ApiError::invalid("头像预设不存在"));
        }
        Avatar::Preset { .. } => {}
        Avatar::Upload {
            media_type,
            data_base64,
        } => {
            let format = match media_type.as_str() {
                "image/png" => image::ImageFormat::Png,
                "image/jpeg" => image::ImageFormat::Jpeg,
                "image/webp" => image::ImageFormat::WebP,
                _ => return Err(ApiError::invalid("头像须为 PNG、JPEG 或 WebP")),
            };
            let invalid =
                || ApiError::invalid("头像须为不超过 256 KiB 且宽高不超过 1024 像素的有效图片");
            if data_base64.len() > MAX_AVATAR_BYTES.div_ceil(3) * 4 {
                return Err(invalid());
            }
            let bytes = STANDARD.decode(data_base64).map_err(|_| invalid())?;
            if bytes.len() > MAX_AVATAR_BYTES || image::guess_format(&bytes).ok() != Some(format) {
                return Err(invalid());
            }
            let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(1024);
            limits.max_image_height = Some(1024);
            limits.max_alloc = Some(16 * 1024 * 1024);
            reader.limits(limits);
            reader.decode().map_err(|_| invalid())?;
        }
    }
    serde_json::to_string(&avatar).map_err(ApiError::internal)
}

fn check_name(tx: &Connection, name: &str, own_id: Option<&str>) -> Result<()> {
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE name=?1 AND id IS NOT ?2
             UNION ALL SELECT 1 FROM agents WHERE name=?1 AND id IS NOT ?2)",
        params![name, own_id],
        |row| row.get(0),
    )?;
    if exists {
        return Err(ApiError::conflict("NAME_TAKEN", "名称已被使用"));
    }
    Ok(())
}

fn profile(tx: &Connection, id: &str) -> Result<Profile> {
    let (name, avatar, agent): (String, Option<String>, Option<String>) = tx
        .query_row(
            "SELECT name, avatar_json, agent_id FROM accounts WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(sign_in_required)?;
    let avatar = avatar
        .map(|raw| serde_json::from_str(&raw))
        .transpose()
        .map_err(ApiError::internal)?;
    let role = if agent.is_some() {
        Role::Agent
    } else {
        Role::User
    };
    Ok(Profile {
        id: id.to_owned(),
        role,
        name,
        avatar,
    })
}

fn complete_sign_in(tx: &Transaction<'_>, id: &str, grant: Option<&str>) -> Result<SignedIn> {
    let profile = profile(tx, id)?;
    if let Role::Agent = profile.role {
        agents::active_agent(tx, id)?;
    }
    let token = if let Some(grant) = grant {
        if !matches!(profile.role, Role::Agent) {
            return Err(invalid_credentials());
        }
        oauth::bind_agent(tx, grant, id)?;
        None
    } else {
        let token = format!("tr_ac_{}", super::super::random_admin_token()?);
        tx.execute(
            "DELETE FROM account_sessions WHERE expires <= unixepoch()",
            [],
        )?;
        tx.execute(
            "INSERT INTO account_sessions(token_hash, account_id, expires) VALUES (?1, ?2, unixepoch() + ?3)",
            params![oauth::hash(&token), id, SESSION_SECONDS],
        )?;
        Some(token)
    };
    Ok(SignedIn { token, profile })
}

fn account_id_for_session(conn: &Connection, token: &str, role: Option<&str>) -> Result<String> {
    if !token.starts_with("tr_ac_") || token.len() != 70 {
        return Err(sign_in_required());
    }
    conn.query_row(
        "SELECT a.id FROM account_sessions s JOIN accounts a ON a.id=s.account_id
         WHERE s.token_hash=?1 AND s.expires>unixepoch() AND (?2 IS NULL OR a.role=?2)",
        params![oauth::hash(token), role],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(sign_in_required)
}

pub(super) fn agent_id_for_session(conn: &Connection, token: &str) -> Result<String> {
    account_id_for_session(conn, token, Some("agent"))
}

fn profile_id(conn: &Connection, access: &Access) -> Result<String> {
    let id = match access {
        Access::Session(token) => account_id_for_session(conn, token, None)?,
        Access::Mcp(hash) => oauth::bound_agent(conn, hash)?.ok_or_else(sign_in_required)?,
    };
    let is_agent: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1 AND role='agent')",
        [&id],
        |row| row.get(0),
    )?;
    if is_agent {
        agents::active_agent(conn, &id)?;
    }
    Ok(id)
}

impl Repository {
    /// 账号、Agent、邀请码消耗和首个会话在同一事务内提交。
    pub(crate) fn register_account(
        &self,
        input: Registration,
        grant: Option<&str>,
    ) -> Result<SignedIn> {
        let name = valid_name(&input.name)?;
        validate_password(&input.password)?;
        let avatar = input.avatar.map(encode_avatar).transpose()?;
        let hash = password_hash(&input.password)?;
        self.write(|tx| {
            check_name(tx, name, None)?;
            let (id, agent_id) = match input.role {
                Role::Agent => {
                    let registered = self.register_agent_in(tx, RegisterAgent {
                        name: name.to_owned(), description: String::new(), capabilities: Vec::new(),
                        invite_token: input.invite_token, subscribe_all_projects: None,
                        project_ids: None, webhook_url: None, webhook_secret: None,
                    })?;
                    let id = registered.agent.id;
                    (id.clone(), Some(id))
                }
                Role::User => {
                    if input.invite_token.is_some() {
                        return Err(ApiError::invalid("用户注册不使用 Agent 邀请令牌"));
                    }
                    (Uuid::new_v4().to_string(), None)
                }
            };
            tx.execute(
                "INSERT INTO accounts(id, role, name, password_hash, avatar_json, agent_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, input.role.as_str(), name, hash, avatar, agent_id],
            )?;
            complete_sign_in(tx, &id, grant)
        })
    }

    pub(crate) fn login_account(&self, input: Login, grant: Option<&str>) -> Result<SignedIn> {
        match (input.role, input.name, input.password, input.agent_key) {
            (Role::Agent, None, None, Some(key)) if key.starts_with("tr_ag_") && key.len() == 70 => {
                self.write(|tx| {
                    let id = agents::agent_id_for_key(tx, &key).map_err(|error| {
                        if error.status == StatusCode::UNAUTHORIZED { invalid_credentials() } else { error }
                    })?;
                    let agent = agents::active_agent(tx, &id)?;
                    tx.execute(
                        "INSERT INTO accounts(id, role, name, agent_id) VALUES (?1, 'agent', ?2, ?1) ON CONFLICT(id) DO NOTHING",
                        params![id, agent.name],
                    )?;
                    complete_sign_in(tx, &id, grant)
                })
            }
            (role, Some(name), Some(password), None) => self.login_with_password(role, &name, &password, grant),
            _ => Err(ApiError::invalid("请提供名称与密码，或提供已有 Agent 密钥")),
        }
    }

    fn login_with_password(
        &self,
        role: Role,
        name: &str,
        password: &str,
        grant: Option<&str>,
    ) -> Result<SignedIn> {
        let name = valid_name(name).map_err(|_| invalid_credentials())?;
        validate_password(password).map_err(|_| invalid_credentials())?;
        let account: Option<(String, Option<String>)> = self
            .connect()?
            .query_row(
                "SELECT id, password_hash FROM accounts WHERE name=?1 AND role=?2",
                params![name, role.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((id, _)) = &account {
            self.write(|tx| {
                let attempts: i64 = tx.query_row(
                    "INSERT INTO account_login_attempts(account_id, until, attempts) VALUES (?1, unixepoch()+60, 1)
                     ON CONFLICT(account_id) DO UPDATE SET
                         attempts=CASE WHEN until<=unixepoch() THEN 1 ELSE min(attempts+1, 11) END,
                         until=CASE WHEN until<=unixepoch() THEN unixepoch()+60 ELSE until END
                     RETURNING attempts", [id], |row| row.get(0),
                )?;
                if attempts > 10 {
                    return Err(ApiError::new(StatusCode::TOO_MANY_REQUESTS, "AUTH_RATE_LIMITED", "登录尝试过多，请稍后重试"));
                }
                Ok(())
            })?;
        }
        verify_password(
            password,
            account.as_ref().and_then(|(_, hash)| hash.as_deref()),
        )?;
        let (id, _) = account.ok_or_else(invalid_credentials)?;
        self.write(|tx| {
            tx.execute(
                "DELETE FROM account_login_attempts WHERE account_id=?1",
                [&id],
            )?;
            complete_sign_in(tx, &id, grant)
        })
    }

    pub(crate) fn account_profile(&self, access: Access) -> Result<Profile> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        profile(&tx, &profile_id(&tx, &access)?)
    }

    pub(crate) fn update_account_profile(
        &self,
        access: Access,
        input: ProfilePatch,
    ) -> Result<Profile> {
        let name = input.name.as_deref().map(valid_name).transpose()?;
        let avatar = input
            .avatar
            .map(|avatar| avatar.map(encode_avatar).transpose())
            .transpose()?;
        self.write(|tx| {
            let id = profile_id(tx, &access)?;
            if let Some(name) = name {
                check_name(tx, name, Some(&id))?;
                tx.execute("UPDATE accounts SET name=?2 WHERE id=?1", params![id, name])?;
                // Agent 领域继续按原 ID 保留任务历史，名称同步给名录和任务详情。
                tx.execute(
                    "UPDATE agents SET name=?2,updated_at=?3 WHERE id=?1",
                    params![id, name, now()],
                )?;
            }
            if let Some(avatar) = avatar {
                tx.execute(
                    "UPDATE accounts SET avatar_json=?2 WHERE id=?1",
                    params![id, avatar],
                )?;
            }
            profile(tx, &id)
        })
    }

    pub(crate) fn logout_account(&self, token: &str) -> Result<()> {
        self.write(|tx| {
            account_id_for_session(tx, token, None)?;
            tx.execute(
                "DELETE FROM account_sessions WHERE token_hash=?1",
                [oauth::hash(token)],
            )?;
            Ok(())
        })
    }
}
