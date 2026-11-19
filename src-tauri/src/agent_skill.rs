//! 文件职责：渲染 Skill，并独立安装 Skill 或配置 MCP 宿主。
use crate::{
    agent_cli::{service_url, Connection},
    agent_mcp_install::{self, PreparedConfig, ServerEntry},
    storage::{sync_parent, FileError},
};
use serde::Deserialize;
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

const SKILL: &str = include_str!("../../assets/skills/taskrail-agent/SKILL.md");
const MCP_SKILL: &str = include_str!("../../assets/skills/taskrail-agent-mcp/SKILL.md");

/// 在线文档可说明服务地址与注册方式；安装器仅把凭证写到 Taskrail 私有目录。
pub(crate) fn render(base_url: &str, policy: &str) -> String {
    render_for(base_url, policy, "taskrail-agent")
}

fn render_for(base_url: &str, policy: &str, command: &str) -> String {
    SKILL
        .replace("{{registration_policy}}", policy)
        .replace("{{base_url}}", base_url)
        .replace("{{cli}}", command)
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum SkillAgent {
    Codex,
    ClaudeCode,
    Cursor,
    Shared,
    Dsh,
    Devin,
    Antigravity,
    GrokBuild,
    Zcode,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Scope {
    Project,
    Global,
    Custom,
}

#[derive(Default, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum InstallMode {
    #[default]
    Skill,
    McpStdio,
    McpHttp,
}

/// project 的 directory 是项目根目录，custom 是直接存放 Skill 的目录。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct InstallRequest {
    agent: SkillAgent,
    #[serde(default)]
    additional_agents: Vec<SkillAgent>,
    scope: Scope,
    directory: Option<PathBuf>,
    base_url: String,
    invite_token: Option<String>,
    #[serde(default)]
    mode: InstallMode,

    #[serde(default)]
    include_guide: bool,
}

pub(super) fn invalid() -> FileError {
    FileError::new("SKILL_INSTALL_INPUT", "Agent 接入参数无效。")
}

pub(super) fn io_error(error: std::io::Error) -> FileError {
    FileError::new("SKILL_INSTALL_IO", format!("无法写入 Agent 配置：{error}"))
}

impl InstallRequest {
    fn root(&self, home: &Path) -> Result<PathBuf, FileError> {
        let root = match self.scope {
            Scope::Global => home,
            Scope::Project | Scope::Custom => self.directory.as_deref().ok_or_else(invalid)?,
        };
        if !root.is_absolute() {
            return Err(invalid());
        }
        fs::canonicalize(root).map_err(io_error)
    }

    fn skills_dir(&self, root: &Path, agent: SkillAgent) -> Result<PathBuf, FileError> {
        if self.scope == Scope::Custom {
            return Ok(root.to_owned());
        }
        let global = self.scope == Scope::Global;
        let directory = match (agent, global) {
            (SkillAgent::Codex, _) | (SkillAgent::Cursor, false) => {
                ensure_directory(&root.join(".agents"))?
            }
            _ => host_directory(root, agent, global)?,
        };
        ensure_directory(&directory.join("skills"))
    }
}

/// Skill 与 MCP 共用宿主配置根。各宿主定义自己的用户目录，项目配置相对项目根定位。
pub(super) fn host_directory(
    root: &Path,
    agent: SkillAgent,
    global: bool,
) -> Result<PathBuf, FileError> {
    let path = match (agent, global) {
        (SkillAgent::Codex, _) => root.join(".codex"),
        (SkillAgent::ClaudeCode, _) => root.join(".claude"),
        (SkillAgent::Cursor, _) => root.join(".cursor"),
        (SkillAgent::Shared, true) => root.join("agents"),
        (SkillAgent::Shared, false) => return Err(invalid()),
        (SkillAgent::Dsh, true) => host_override(root, "DSH_HOME", ".dsh")?,
        (SkillAgent::Dsh, false) => root.join(".dsh"),
        (SkillAgent::Devin, true) => {
            let (key, default) = if cfg!(windows) {
                ("APPDATA", "AppData/Roaming")
            } else {
                ("XDG_CONFIG_HOME", ".config")
            };
            let base = host_override(root, key, default)?;
            if base == root.join(default) && cfg!(windows) {
                ensure_directory(&root.join("AppData"))?;
            }
            ensure_directory(&base)?.join("devin")
        }
        (SkillAgent::Devin, false) => root.join(".devin"),
        (SkillAgent::Antigravity, true) => ensure_directory(&root.join(".gemini"))?.join("config"),
        (SkillAgent::Antigravity, false) => root.join(".agents"),
        (SkillAgent::GrokBuild, true) => host_override(root, "GROK_HOME", ".grok")?,
        (SkillAgent::GrokBuild, false) => root.join(".grok"),
        (SkillAgent::Zcode, _) => root.join(".zcode"),
    };
    ensure_directory(&path)
}

fn host_override(home: &Path, key: &str, default: &str) -> Result<PathBuf, FileError> {
    let Some(value) =
        std::env::var_os(key).filter(|value| !value.to_string_lossy().trim().is_empty())
    else {
        return Ok(home.join(default));
    };
    let path = PathBuf::from(value);
    if let Ok(rest) = path.strip_prefix("~") {
        return Ok(home.join(rest));
    }
    if !path.is_absolute() {
        return Err(invalid());
    }
    Ok(path)
}

pub(super) fn ensure_directory(path: &Path) -> Result<PathBuf, FileError> {
    match fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => return Err(io_error(error)),
    }
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(invalid());
        }
    }
    Ok(path.to_owned())
}

fn check_new_skill(path: &Path) -> Result<(), FileError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
        Ok(_) => Err(FileError::new(
            "SKILL_EXISTS",
            "目标目录已安装 Skill。请选择其他目录。",
        )),
    }
}

enum InstallTarget<'a> {
    Markdown { path: PathBuf, content: &'a str },
    Mcp(PreparedConfig),
}

impl InstallTarget<'_> {
    fn path(&self) -> &Path {
        match self {
            Self::Markdown { path, .. } => path,
            Self::Mcp(config) => &config.path,
        }
    }

    fn commit(&self) -> Result<(), FileError> {
        match self {
            Self::Markdown { path, content } => {
                let parent = path.parent().ok_or_else(invalid)?;
                let staging = tempfile::tempdir_in(parent).map_err(io_error)?;
                let prepared = staging.path().join("skill");
                fs::create_dir(&prepared).map_err(io_error)?;
                fs::write(prepared.join("SKILL.md"), content).map_err(io_error)?;
                check_new_skill(path)?;
                fs::rename(prepared, path).map_err(io_error)
            }
            Self::Mcp(config) => config.commit(),
        }
    }

    fn rollback(&self) -> Result<(), FileError> {
        match self {
            Self::Markdown { path, content } => {
                let markdown = path.join("SKILL.md");
                let metadata = fs::symlink_metadata(path).map_err(io_error)?;
                if !metadata.is_dir()
                    || fs::read_dir(path).map_err(io_error)?.count() != 1
                    || fs::read_to_string(&markdown).map_err(io_error)? != *content
                {
                    return Err(invalid());
                }
                fs::remove_file(markdown).map_err(io_error)?;
                fs::remove_dir(path).map_err(io_error)?;
            }
            Self::Mcp(config) => config.rollback()?,
        }
        sync_parent(self.path())
    }
}

fn plan_skill<'a>(
    plan: &mut Vec<InstallTarget<'a>>,
    path: PathBuf,
    content: &'a str,
) -> Result<(), FileError> {
    // Codex、Cursor 和 Antigravity 可读取同一个项目 Skill 目录。
    if !plan.iter().any(|target| target.path() == path) {
        check_new_skill(&path)?;
        plan.push(InstallTarget::Markdown { path, content });
    }
    Ok(())
}

fn commit_plan(plan: &[InstallTarget<'_>]) -> Result<(), (FileError, bool)> {
    let mut written = 0;
    for target in plan {
        let result = target.commit().and_then(|()| {
            written += 1;
            sync_parent(target.path())
        });
        if let Err(error) = result {
            let mut rollback_failed = false;
            for applied in plan[..written].iter().rev() {
                rollback_failed |= applied.rollback().is_err();
            }
            return Err((error, rollback_failed));
        }
    }
    Ok(())
}

fn shell_path(path: &Path) -> Result<String, FileError> {
    let path = path
        .to_str()
        .filter(|path| !path.contains(['\r', '\n', '`']))
        .ok_or_else(invalid)?;
    let escaped = if cfg!(windows) {
        path.replace('\'', "''")
    } else {
        path.replace('\'', "'\\''")
    };
    Ok(format!("'{escaped}'"))
}

/// 只有本机 CLI 与 STDIO 使用私有连接文件；HTTP 模式由宿主通过 OAuth 获取授权。
pub(crate) fn install(
    request: InstallRequest,
    home: &Path,
    app_data: &Path,
    executable: &Path,
    embedded: Option<(&str, &Path)>,
) -> Result<Vec<PathBuf>, FileError> {
    let url = service_url(request.base_url.trim()).map_err(|_| invalid())?;
    if request.invite_token.as_deref().is_some_and(|token| {
        !token.strip_prefix("tr_inv_").is_some_and(|value| {
            value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    }) {
        return Err(invalid());
    }
    let mut agents = vec![request.agent];
    for &agent in &request.additional_agents {
        if !agents.contains(&agent) {
            agents.push(agent);
        }
    }
    if agents.iter().any(|&agent| {
        (request.mode != InstallMode::Skill
            && (request.scope == Scope::Custom || agent == SkillAgent::Shared))
            || (agent == SkillAgent::Dsh
                && (request.mode == InstallMode::McpHttp
                    || (request.mode == InstallMode::McpStdio && request.scope != Scope::Global)))
            || (agent == SkillAgent::Shared && request.scope == Scope::Project)
    }) {
        return Err(invalid());
    }
    let root = request.root(home)?;
    let base_url = url.origin().ascii_serialization();
    let mcp_url = format!("{base_url}/mcp");
    if request.mode == InstallMode::McpHttp
        && url.scheme() != "https"
        && !matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
    {
        return Err(FileError::new(
            "MCP_HTTPS_REQUIRED",
            "远程 MCP 地址必须使用 HTTPS。",
        ));
    }

    // 一批本机安装共用连接文件和 Agent 身份。校验或回滚成功时清理未使用的连接文件。
    let profile = if request.mode == InstallMode::McpHttp {
        None
    } else {
        if !executable.is_file() {
            return Err(FileError::new(
                "AGENT_CLI_NOT_FOUND",
                "找不到 Taskrail Agent 命令行程序。",
            ));
        }
        fs::create_dir_all(app_data).map_err(io_error)?;
        let directory = ensure_directory(&app_data.join("agent-connections"))?;
        let profile = tempfile::Builder::new()
            .prefix("agent-")
            .suffix(".json")
            .tempfile_in(directory)
            .map_err(io_error)?
            .into_temp_path();
        let connection = Connection {
            local_address_file: embedded
                .filter(|(address, _)| *address == base_url)
                .map(|(_, path)| path.to_owned()),
            base_url,
            invite_token: request.invite_token.clone(),
            api_key: None,
        };
        connection
            .save(&profile)
            .map_err(|_| FileError::new("SKILL_INSTALL_IO", "无法写入 Agent 连接配置。"))?;
        Some(profile)
    };
    let mut plan = Vec::new();
    let mut installed_configs = Vec::new();
    let markdown;
    match request.mode {
        InstallMode::Skill => {
            let executable = shell_path(executable)?;
            let profile = shell_path(profile.as_ref().unwrap())?;
            let prefix = if cfg!(windows) { "& " } else { "" };
            let command = format!("{prefix}{executable} --config {profile}");
            markdown = render_for("<BASE_URL>", "首次接入时运行注册命令。", &command);
            for agent in agents {
                let path = request.skills_dir(&root, agent)?.join("taskrail-agent");
                plan_skill(&mut plan, path, &markdown)?;
            }
        }
        InstallMode::McpStdio | InstallMode::McpHttp => {
            let entry = if let Some(profile) = profile.as_ref() {
                ServerEntry::Stdio {
                    executable,
                    profile,
                }
            } else {
                ServerEntry::Http { url: &mcp_url }
            };
            for agent in agents {
                if request.include_guide {
                    let path = request.skills_dir(&root, agent)?.join("taskrail-agent-mcp");
                    plan_skill(&mut plan, path, MCP_SKILL)?;
                }
                let config = agent_mcp_install::prepare(
                    &root,
                    agent,
                    request.scope == Scope::Global,
                    entry,
                )?;
                if installed_configs.contains(&config.path) {
                    return Err(invalid());
                }
                installed_configs.push(config.path.clone());
                plan.push(InstallTarget::Mcp(config));
            }
        }
    }
    // 所有目标通过校验后再写入。遇到写入错误时按逆序撤销已写入的目标。
    let result = commit_plan(&plan);
    if !matches!(result, Err((_, false))) {
        if let Some(profile) = profile {
            profile.keep().map_err(|error| io_error(error.error))?;
        }
    }
    match result {
        Ok(()) if request.mode == InstallMode::Skill => {
            Ok(plan.iter().map(|target| target.path().to_owned()).collect())
        }
        Ok(()) => Ok(installed_configs),
        Err((error, false)) => Err(error),
        Err((error, true)) => Err(FileError::new(
            "SKILL_INSTALL_ROLLBACK",
            format!("安装中断，部分目标仍保留本次配置。{}", error.message),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn batch_shares_connection_and_deduplicates_skill_directories() {
        for mode in [InstallMode::Skill, InstallMode::McpStdio] {
            let root = tempfile::tempdir().unwrap();
            let root = root.path();
            let data = root.join("private");
            let native = std::env::current_exe().unwrap();
            let is_skill = mode == InstallMode::Skill;
            let paths = install(
                InstallRequest {
                    agent: SkillAgent::Codex,
                    additional_agents: vec![SkillAgent::Cursor, SkillAgent::Antigravity],
                    scope: Scope::Project,
                    directory: Some(root.to_owned()),
                    mode,
                    base_url: "https://rail.example".into(),
                    invite_token: None,

                    include_guide: !is_skill,
                },
                root,
                &data,
                &native,
                None,
            )
            .unwrap();
            let mut profiles = fs::read_dir(data.join("agent-connections")).unwrap();
            let profile = profiles.next().unwrap().unwrap().path();
            assert!(profiles.next().is_none());
            if is_skill {
                assert_eq!(paths.len(), 1);
                assert!(paths[0].ends_with(".agents/skills/taskrail-agent"));
                continue;
            }
            assert_eq!(paths.len(), 3);
            assert!(root
                .join(".agents/skills/taskrail-agent-mcp/SKILL.md")
                .is_file());
            let cursor: Value = serde_json::from_slice(&fs::read(&paths[1]).unwrap()).unwrap();
            let agy: Value = serde_json::from_slice(&fs::read(&paths[2]).unwrap()).unwrap();
            assert_eq!(
                cursor["mcpServers"]["taskrail"]["args"][1],
                profile.to_str().unwrap()
            );
            assert_eq!(cursor, agy);
            let codex = fs::read_to_string(&paths[0])
                .unwrap()
                .parse::<toml_edit::DocumentMut>()
                .unwrap();
            assert_eq!(
                codex["mcp_servers"]["taskrail"]["args"]
                    .as_array()
                    .unwrap()
                    .get(1)
                    .unwrap()
                    .as_str(),
                profile.to_str()
            );
        }
    }

    #[test]
    fn batch_rolls_back_existing_host_config_if_later_target_changes() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let existing = r#"{"mcpServers":{"existing":{"command":"local"}}}"#;
        let config_path = root.join(".mcp.json");
        fs::write(&config_path, existing).unwrap();
        let config = agent_mcp_install::prepare(
            root,
            SkillAgent::ClaudeCode,
            false,
            ServerEntry::Http {
                url: "https://rail.example/mcp",
            },
        )
        .unwrap();
        let conflicting = root.join("taskrail-agent");
        let mut plan = vec![InstallTarget::Mcp(config)];
        plan_skill(&mut plan, conflicting.clone(), "skill").unwrap();
        fs::create_dir(&conflicting).unwrap();
        let (error, rollback_failed) = commit_plan(&plan).unwrap_err();
        assert_eq!(error.code, "SKILL_EXISTS");
        assert!(!rollback_failed);
        assert_eq!(fs::read_to_string(config_path).unwrap(), existing);
        assert!(conflicting.is_dir());
    }

    #[test]
    fn installs_skill_in_shared_and_new_harness_directories() {
        for (agent, scope, directory) in [
            (SkillAgent::Shared, Scope::Global, "agents/skills"),
            (SkillAgent::Dsh, Scope::Project, ".dsh/skills"),
            (SkillAgent::Devin, Scope::Project, ".devin/skills"),
            (
                SkillAgent::Antigravity,
                Scope::Global,
                ".gemini/config/skills",
            ),
            (SkillAgent::Antigravity, Scope::Project, ".agents/skills"),
            (SkillAgent::GrokBuild, Scope::Project, ".grok/skills"),
            (SkillAgent::Zcode, Scope::Project, ".zcode/skills"),
        ] {
            let root = tempfile::tempdir().unwrap();
            let root = root.path();
            let installed = install(
                InstallRequest {
                    agent,
                    additional_agents: Vec::new(),
                    directory: (scope == Scope::Project).then(|| root.to_owned()),
                    scope,
                    mode: InstallMode::Skill,
                    base_url: "https://rail.example".into(),
                    invite_token: None,

                    include_guide: false,
                },
                root,
                &root.join("private"),
                &std::env::current_exe().unwrap(),
                None,
            )
            .unwrap()
            .pop()
            .unwrap();
            assert!(installed.ends_with(format!("{directory}/taskrail-agent")));
            assert!(installed.join("SKILL.md").is_file());
        }
    }

    #[test]
    fn installs_skill_and_mcp_independently_with_private_connections() {
        let root = tempfile::tempdir().unwrap();
        let token = format!("tr_inv_{}", "a".repeat(64));
        let root = root.path();
        let app_data = root.join("app");
        let executable = std::env::current_exe().unwrap();
        let make_request = |mode| InstallRequest {
            mode,
            agent: SkillAgent::Codex,
            additional_agents: Vec::new(),
            scope: Scope::Project,
            directory: Some(root.to_owned()),
            base_url: "https://taskrail.example/".into(),
            invite_token: Some(token.clone()),

            include_guide: false,
        };
        let installed = install(
            make_request(InstallMode::Skill),
            root,
            &app_data,
            &executable,
            None,
        )
        .unwrap()
        .pop()
        .unwrap();
        assert!(installed.ends_with(".agents/skills/taskrail-agent"));
        let markdown = fs::read_to_string(installed.join("SKILL.md")).unwrap();
        assert!(!markdown.contains(&token));
        assert!(markdown.contains("--config"));
        assert_eq!(fs::read_dir(installed).unwrap().count(), 1);
        assert!(!root.join(".codex/config.toml").exists());
        let configuration = install(
            make_request(InstallMode::McpStdio),
            root,
            &app_data,
            &executable,
            None,
        )
        .unwrap()
        .pop()
        .unwrap();
        let host = fs::read_to_string(&configuration).unwrap();
        assert!(host.contains("[mcp_servers.taskrail]"));
        assert!(host.contains("mcp"));
        assert!(!host.contains(&token));
        let private = fs::read_dir(app_data.join("agent-connections"))
            .unwrap()
            .map(|entry| {
                serde_json::from_slice::<Value>(&fs::read(entry.unwrap().path()).unwrap()).unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(private.len(), 2);
        assert!(private
            .iter()
            .all(|profile| profile["invite_token"] == token));
        assert!(install(
            make_request(InstallMode::McpStdio),
            root,
            &app_data,
            &executable,
            None
        )
        .is_err());
        assert_eq!(
            fs::read_dir(app_data.join("agent-connections"))
                .unwrap()
                .count(),
            2
        );
        assert_eq!(fs::read_to_string(configuration).unwrap(), host);
    }

    #[test]
    fn oauth_mcp_installs_without_executable_or_invite_and_preserves_other_servers() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let host_file = root.join(".mcp.json");
        fs::write(
            &host_file,
            r#"{"mcpServers":{"another":{"command":"hello"}}}"#,
        )
        .unwrap();
        let private = root.join("private");
        let installed = install(
            InstallRequest {
                agent: SkillAgent::ClaudeCode,
                additional_agents: Vec::new(),
                scope: Scope::Project,
                directory: Some(root.to_owned()),
                base_url: "https://rail.example".into(),
                invite_token: None,
                mode: InstallMode::McpHttp,

                include_guide: true,
            },
            root,
            &private,
            &root.join("unused.exe"),
            None,
        )
        .unwrap()
        .pop()
        .unwrap();
        assert_eq!(installed.file_name().unwrap(), ".mcp.json");
        let config: Value = serde_json::from_slice(&fs::read(host_file).unwrap()).unwrap();
        assert_eq!(
            config["mcpServers"]["taskrail"],
            serde_json::json!({
                "type":"http", "url":"https://rail.example/mcp",
            })
        );
        assert_eq!(config["mcpServers"]["another"]["command"], "hello");
        let guide = root.join(".claude/skills/taskrail-agent-mcp");
        assert_eq!(fs::read_dir(&guide).unwrap().count(), 1);
        assert!(guide.join("SKILL.md").is_file());
        assert!(!root.join(".claude/skills/taskrail-agent").exists());
        assert!(!private.exists());
    }
}
