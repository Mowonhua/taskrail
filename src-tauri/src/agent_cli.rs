//! 文件职责：读取本机 Agent 连接、执行原生命令，并提供 STDIO 传输。
use crate::agent_tools::{Operation, ToolResult, ToolServer, ACTIONS};
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
    connection: std::sync::Arc<Connection>,
}

impl Client {
    fn new(connection: &Connection) -> Result<Self, Box<dyn Error>> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .use_rustls_tls()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http,
            connection: std::sync::Arc::new(Connection {
                base_url: connection.base_url.clone(),
                local_address_file: connection.local_address_file.clone(),
                invite_token: None,
                api_key: connection.api_key.clone(),
            }),
        })
    }

    pub(crate) async fn execute(&self, operation: Operation, registering: bool) -> ToolResult {
        let fail = |code, message: String| json!({"error":{"code":code,"message":message}});
        let base = self
            .connection
            .address()
            .map_err(|_| fail("CONNECTION_FAILED", "无法读取 Taskrail 服务地址".into()))?;
        let url = base
            .join(&operation.path)
            .map_err(|_| fail("INVALID_ARGUMENT", "无效的 Taskrail 请求".into()))?;
        let mut req = self.http.request(operation.method, url);
        if !registering {
            let key =
                self.connection.api_key.as_deref().ok_or_else(|| {
                    fail("UNAUTHORIZED", "请先运行 taskrail-agent register".into())
                })?;
            req = req.bearer_auth(key);
        }
        if let Some(body) = operation.body {
            req = req.json(&body);
        }
        let response = req
            .send()
            .await
            .map_err(|error| fail("CONNECTION_FAILED", error.without_url().to_string()))?;
        let status = response.status();
        if status == reqwest::StatusCode::NO_CONTENT {
            return Ok(Value::Null);
        }
        let value = response
            .json()
            .await
            .map_err(|_| fail("INVALID_RESPONSE", format!("Taskrail 返回 HTTP {status}")))?;
        if status.is_success() {
            Ok(value)
        } else {
            Err(value)
        }
    }
}

fn read_args(path: Option<&Path>) -> Result<Map<String, Value>, Box<dyn Error>> {
    match path {
        Some(path) if path == Path::new("-") => Ok(serde_json::from_reader(io::stdin().lock())?),
        Some(path) => Ok(serde_json::from_reader(fs::File::open(path)?)?),
        None => Ok(Map::new()),
    }
}

fn command_help() {
    println!("taskrail-agent --config FILE register NAME");
    println!("taskrail-agent --config FILE mcp [AGENT_NAME]");
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
            let mut config: Connection = serde_json::from_reader(fs::File::open(&path)?)?;
            if config.api_key.is_none() {
                let name = words.get(1).ok_or("请先运行 taskrail-agent register")?;
                register(&path, name)
                    .await?
                    .map_err(|error| error.to_string())?;
                config = serde_json::from_reader(fs::File::open(&path)?)?;
            }
            let client = Client::new(&config)?;
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
            let operation = action.operation(read_args(json.as_deref())?)?;
            let config = serde_json::from_reader(fs::File::open(path)?)?;
            Client::new(&config)?.execute(operation, false).await
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
async fn register(path: &Path, name: &str) -> Result<ToolResult, Box<dyn Error>> {
    let mut lock_path = path.as_os_str().to_os_string();
    lock_path.push(".register.lock");
    let lock = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    // 多个宿主首次同时启动时，后续进程等待首个进程保存凭证，再重读配置。
    let _lock = tokio::task::spawn_blocking(move || {
        lock.lock()?;
        Ok::<_, std::io::Error>(lock)
    })
    .await??;
    let mut config: Connection = serde_json::from_reader(fs::File::open(path)?)?;
    let client = Client::new(&config)?;
    if config.api_key.is_some() {
        return Ok(client
            .execute(
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
    let value = match client.execute(operation, true).await {
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
