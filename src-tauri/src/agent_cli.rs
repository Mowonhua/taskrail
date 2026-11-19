//! 文件职责：读取本机 Agent 连接、执行原生命令，并提供 STDIO 传输。
use crate::agent_tools::{sign_in_required, Operation, ToolResult, ToolServer, ACTIONS};
use clap::Parser;
use rmcp::ServiceExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Parser)]
#[command(
    name = "taskrail-agent",
    version,
    about = "Taskrail Agent 命令行与 STDIO MCP"
)]
struct Args {
    /// 连接配置文件；也可通过 TASKRAIL_AGENT_CONFIG 指定。
    #[arg(long)]
    config: Option<PathBuf>,
    /// 从文件读取工具参数 JSON 对象，使用 - 表示标准输入。
    #[arg(long)]
    json: Option<PathBuf>,
    #[arg(value_name = "COMMAND", num_args = 1..=3, value_delimiter = None)]
    command: Vec<String>,
}

/// 本机凭证只从调用者选定的配置读取。注册成功时替换邀请值。
#[derive(Deserialize, Serialize)]
pub(crate) struct Connection {
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) local_address_file: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invite_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

impl Connection {
    pub(crate) fn save(&self, path: &Path) -> Result<(), Box<dyn Error>> {
        let parent = path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let file = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(file.as_file(), self)?;
        file.as_file().sync_all()?;
        file.persist(path)?;
        Ok(())
    }

    fn address(&self) -> Result<url::Url, Box<dyn Error>> {
        let address = match &self.local_address_file {
            Some(file) => fs::read_to_string(file)?,
            None => self.base_url.clone(),
        };
        let url = service_url(address.trim())?;
        if self.local_address_file.is_some()
            && !matches!(url.host_str(), Some("127.0.0.1" | "[::1]"))
        {
            return Err("内嵌 Taskrail 必须使用本机地址".into());
        }
        Ok(url)
    }
}

pub(crate) fn service_url(raw: &str) -> Result<url::Url, url::ParseError> {
    let url = url::Url::parse(raw)?;
    if !matches!(url.scheme(), "https" | "http")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(url::ParseError::InvalidDomainCharacter);
    }
    Ok(url)
}

#[derive(Clone)]
pub(crate) struct Client {
    http: reqwest::Client,
    path: PathBuf,
}

impl Client {
    fn new(path: &Path) -> Result<Self, Box<dyn Error>> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .use_rustls_tls()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http,
            path: path.to_owned(),
        })
    }

    fn connection(&self) -> io::Result<Connection> {
        Ok(serde_json::from_reader(fs::File::open(&self.path)?)?)
    }

    pub(crate) fn has_credentials(&self) -> bool {
        self.connection()
            .is_ok_and(|connection| connection.api_key.is_some())
    }

    pub(crate) async fn call(&self, operation: Operation, signs_in: bool) -> ToolResult {
        if signs_in {
            return self.sign_in(operation).await.map_err(|_| {
                failure(
                    "AGENT_CONFIG_ERROR",
                    "无法保存 Agent 登录会话。请检查连接配置并重试登录。",
                )
            })?;
        }
        let connection = self
            .connection()
            .map_err(|_| failure("AGENT_CONFIG_ERROR", "无法读取 Agent 连接配置"))?;
        self.execute(&connection, operation, false).await
    }

    async fn sign_in(&self, mut operation: Operation) -> Result<ToolResult, Box<dyn Error>> {
        let _lock = lock_connection(&self.path).await?;
        let mut connection = self.connection()?;
        if operation.path == "/api/v1/auth/register" {
            if let Some(body) = operation.body.as_mut().and_then(Value::as_object_mut) {
                if let Some(invite) = &connection.invite_token {
                    body.entry("invite_token").or_insert_with(|| json!(invite));
                }
            }
        }
        let mut value = match self.execute(&connection, operation, true).await {
            Ok(value) => value,
            Err(error) => return Ok(Err(error)),
        };
        let token = value
            .as_object_mut()
            .and_then(|value| value.remove("token"))
            .and_then(|token| token.as_str().map(str::to_owned))
            .ok_or("登录响应缺少 Agent 会话令牌")?;
        connection.api_key = Some(token);
        connection.invite_token = None;
        connection.save(&self.path)?;
        Ok(Ok(value))
    }

    async fn execute(
        &self,
        connection: &Connection,
        operation: Operation,
        public: bool,
    ) -> ToolResult {
        if !public && connection.api_key.is_none() {
            return Err(sign_in_required());
        }
        let base = connection
            .address()
            .map_err(|_| failure("CONNECTION_FAILED", "无法读取 Taskrail 服务地址"))?;
        let url = base
            .join(&operation.path)
            .map_err(|_| failure("INVALID_ARGUMENT", "无效的 Taskrail 请求"))?;
        let mut req = self.http.request(operation.method, url);
        if let Some(key) = connection.api_key.as_deref().filter(|_| !public) {
            req = req.bearer_auth(key);
        }
        if let Some(body) = operation.body {
            req = req.json(&body);
        }
        let response = req
            .send()
            .await
            .map_err(|error| failure("CONNECTION_FAILED", error.without_url().to_string()))?;
        let status = response.status();
        if status == reqwest::StatusCode::NO_CONTENT {
            return Ok(Value::Null);
        }
        let value = response
            .json()
            .await
            .map_err(|_| failure("INVALID_RESPONSE", format!("Taskrail 返回 HTTP {status}")))?;
        if status.is_success() {
            Ok(value)
        } else {
            Err(value)
        }
    }
}

fn failure(code: &str, message: impl Into<String>) -> Value {
    json!({"error":{"code":code,"message":message.into()}})
}

fn read_args(path: Option<&Path>) -> Result<Map<String, Value>, Box<dyn Error>> {
    match path {
        Some(path) if path == Path::new("-") => Ok(serde_json::from_reader(io::stdin().lock())?),
        Some(path) => Ok(serde_json::from_reader(fs::File::open(path)?)?),
        None => Ok(Map::new()),
    }
}

fn command_help() {
    println!("taskrail-agent --config FILE mcp");
    for action in ACTIONS {
        println!(
            "{} {}  {}",
            action.group,
            action.command,
            action.tool().description.unwrap_or_default()
        );
    }
    println!("使用 GROUP COMMAND help 查看参数，使用 --json FILE 或 --json - 提供参数。");
}

fn default_config() -> Option<PathBuf> {
    std::env::var_os("TASKRAIL_AGENT_CONFIG")
        .map(PathBuf::from)
        .or_else(|| {
            let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
            Some(PathBuf::from(home).join(".taskrail/agent.json"))
        })
}

pub async fn run() -> Result<(), Box<dyn Error>> {
    let Args {
        config,
        json,
        command,
    } = Args::parse();
    let words: Vec<_> = command.iter().map(String::as_str).collect();
    if matches!(words.as_slice(), [] | ["commands"]) {
        command_help();
        return Ok(());
    }
    if let [group, command, "help"] = words.as_slice() {
        let action = ACTIONS
            .iter()
            .find(|action| action.group == *group && action.command == *command)
            .ok_or("未知 Taskrail 命令")?;
        println!(
            "{}",
            serde_json::to_string_pretty(&action.tool().input_schema)?
        );
        return Ok(());
    }
    let path = config
        .or_else(default_config)
        .ok_or("请用 --config 指定 Taskrail 连接配置")?;
    let result = match words.as_slice() {
        ["register", name] => register(&path, name).await?,
        ["mcp"] | ["mcp", _] => {
            let client = Client::new(&path)?;
            client.connection()?;
            ToolServer::Stdio(client)
                .serve(rmcp::transport::stdio())
                .await?
                .waiting()
                .await?;
            return Ok(());
        }
        [group, command] => {
            let action = ACTIONS
                .iter()
                .find(|action| action.group == *group && action.command == *command)
                .ok_or("未知 Taskrail 命令。运行 taskrail-agent commands 查看命令")?;
            let client = Client::new(&path)?;
            let signs_in = action.signs_in();
            if !signs_in && client.connection()?.api_key.is_none() {
                Err(sign_in_required())
            } else {
                let operation = action.operation(read_args(json.as_deref())?)?;
                client.call(operation, signs_in).await
            }
        }
        _ => return Err("无效的 Taskrail 命令。运行 taskrail-agent commands 查看命令".into()),
    };
    match result {
        Ok(data) => {
            println!("{data}");
            Ok(())
        }
        Err(error) => Err(error.to_string().into()),
    }
}

/// 锁文件与配置分开，配置被原子替换后，其他注册进程仍在同一把锁上排队。
async fn lock_connection(path: &Path) -> Result<fs::File, Box<dyn Error>> {
    let mut lock_path = path.as_os_str().to_os_string();
    lock_path.push(".register.lock");
    let lock = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    Ok(tokio::task::spawn_blocking(move || {
        lock.lock()?;
        Ok::<_, std::io::Error>(lock)
    })
    .await??)
}

// 保留旧 Skill 的显式注册命令。新安装通过 account register 走共享账户入口。
async fn register(path: &Path, name: &str) -> Result<ToolResult, Box<dyn Error>> {
    let _lock = lock_connection(path).await?;
    let client = Client::new(path)?;
    let mut config = client.connection()?;
    if config.api_key.is_some() {
        return Ok(client
            .execute(
                &config,
                Operation {
                    method: axum::http::Method::GET,
                    path: "/api/v1/agent/me".into(),
                    body: None,
                },
                false,
            )
            .await);
    }
    let operation = Operation {
        method: axum::http::Method::POST,
        path: "/api/v1/agents/register".into(),
        body: Some(json!({"name":name,"invite_token":config.invite_token})),
    };
    let value = match client.execute(&config, operation, true).await {
        Ok(value) => value,
        Err(error) => return Ok(Err(error)),
    };
    config.api_key = Some(
        value["api_key"]
            .as_str()
            .ok_or("注册响应缺少 Agent 密钥")?
            .to_owned(),
    );
    config.invite_token = None;
    config.save(path)?;
    Ok(Ok(value["agent"].clone()))
}
