//! 文件职责：为原生 CLI、STDIO 和 Streamable HTTP 定义同一组 Agent 操作。
use axum::http::Method;
use rmcp::{
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::{RequestContext, RoleServer},
    ErrorData, ServerHandler,
};
use serde_json::{json, Map, Value};

/// 工具名称、参数与路径在此统一生成。服务端仍按既有 Agent API 校验业务输入。
#[derive(Clone, Copy)]
pub(crate) struct Action {
    pub group: &'static str,
    pub command: &'static str,
    name: &'static str,
    description: &'static str,
    method: &'static str,
    path: &'static str,
    fields: &'static [(&'static str, &'static str)],
}

macro_rules! action {
    ($group:literal, $command:literal, $name:literal, $method:literal, $path:literal, $desc:literal $(, $field:literal : $kind:literal)*) => {
        Action { group: $group, command: $command, name: concat!("taskrail_", $name), description: $desc,
            method: $method, path: $path, fields: &[ $( ($field, $kind) ),* ] }
    };
}

pub(crate) const ACTIONS: &[Action] = &[
    action!("account", "register", "register", "POST", "/register", "注册并登录。自选名称和至少 12 字符的密码，按需提供邀请码和头像。", "name":"string!", "password":"string!", "invite_token":"string", "avatar":"avatar"),
    action!("account", "login", "login", "POST", "/login", "使用名称和密码登录，或仅提供已有 Agent 密钥 agent_key。", "name":"string", "password":"string", "agent_key":"string"),
    action!(
        "account",
        "profile",
        "profile",
        "GET",
        "/me",
        "读取账户名称和头像"
    ),
    action!("account", "update", "update_profile", "PATCH", "/me", "修改名称和头像。avatar 为 null 时清除头像。", "name":"string", "avatar":"avatar"),
    action!(
        "agent",
        "me",
        "me",
        "GET",
        "/me",
        "读取当前 Agent 的身份和状态"
    ),
    action!("agent", "update", "update_me", "PATCH", "/me", "修改 Agent 简介和能力", "description":"string", "capabilities":"strings"),
    action!("agent", "subscriptions", "set_subscriptions", "PUT", "/me/subscriptions", "替换项目订阅", "subscribe_all_projects":"boolean", "project_ids":"strings"),
    action!(
        "agent",
        "webhook",
        "get_webhook",
        "GET",
        "/me/webhook",
        "读取 Webhook 配置"
    ),
    action!("agent", "set-webhook", "set_webhook", "PUT", "/me/webhook", "配置 Webhook，传入 null 清除地址", "webhook_url":"nullable!", "webhook_secret":"string"),
    action!(
        "projects",
        "list",
        "list_projects",
        "GET",
        "/projects",
        "列出可订阅的项目及其工作目录"
    ),
    action!("tasks", "list", "list_tasks", "GET", "/bounties", "按项目、状态或文本查询可见任务", "project_id":"string", "status":"string", "q":"string", "include_drafts":"boolean"),
    action!("tasks", "get", "get_task", "GET", "/bounties/{id}", "读取任务详情和最新租约", "task_id":"string!"),
    action!("tasks", "create", "create_task", "POST", "/bounties", "提交任务。项目策略决定初始状态", "project_id":"string!", "title":"string!", "description":"string", "acceptance_criteria":"string", "difficulty":"tier!", "assignee_mode":"mode!", "assignee_agent_id":"string", "assignee_min_tier":"tier", "require_arbitration":"boolean", "blocked_by":"strings"),
    action!("tasks", "update-draft", "update_draft", "PATCH", "/bounties/{id}", "修订受邀协作的草稿", "task_id":"string!", "title":"string", "description":"string", "acceptance_criteria":"string", "blocked_by":"strings"),
    action!("tasks", "split", "split_draft", "POST", "/bounties/{id}/split", "将受邀草稿拆为子草稿。blocked_by 可用 key 引用同批子草稿", "task_id":"string!", "items":"items!"),
    action!("tasks", "claim", "claim_task", "POST", "/bounties/{id}/claim", "认领任务，返回 lease_id 和 lease_expires_at", "task_id":"string!"),
    action!("tasks", "decline", "decline_task", "POST", "/bounties/{id}/decline", "婉拒当前定向指派", "task_id":"string!", "reason":"string"),
    action!("tasks", "heartbeat", "heartbeat", "POST", "/bounties/{id}/heartbeat", "在当前租约到期前续租。STALE_LEASE 时停止用旧租约写入", "task_id":"string!", "lease_id":"integer!"),
    action!("tasks", "abandon", "abandon_task", "POST", "/bounties/{id}/abandon", "放弃任务并释放当前租约", "task_id":"string!", "lease_id":"integer!", "reason":"string"),
    action!("tasks", "complete", "complete_task", "POST", "/bounties/{id}/complete", "直接交付无需仲裁的任务", "task_id":"string!", "lease_id":"integer!", "completion_note":"string", "completion_cost":"number"),
    action!("tasks", "request-info", "request_info", "POST", "/bounties/{id}/request-info", "提问并挂起执行，收到回复后用原租约继续", "task_id":"string!", "lease_id":"integer!", "question":"string!"),
    action!("tasks", "comments", "list_comments", "GET", "/bounties/{id}/comments", "读取任务评论和提问", "task_id":"string!"),
    action!("tasks", "comment", "add_comment", "POST", "/bounties/{id}/comments", "添加评论或以发布者身份回复问题", "task_id":"string!", "body":"string!"),
    action!("tasks", "submit-arbitration", "submit_arbitration", "POST", "/bounties/{id}/submit-arbitration", "提交需仲裁的任务成果", "task_id":"string!", "lease_id":"integer!", "completion_note":"string", "completion_cost":"number"),
    action!("tasks", "approve", "approve_arbitration", "POST", "/bounties/{id}/arbitration/approve", "以当前裁判身份通过交付", "task_id":"string!", "quality_grade":"grade", "calibrated_difficulty":"tier", "comment":"string"),
    action!("tasks", "reject", "reject_arbitration", "POST", "/bounties/{id}/arbitration/reject", "以当前裁判身份打回交付并说明原因", "task_id":"string!", "comment":"string!"),
    action!("events", "list", "list_events", "GET", "/events", "按游标读取事件收件箱。按 event_id 顺序处理后再确认", "cursor":"integer", "limit":"integer"),
    action!("events", "ack", "ack_events", "POST", "/events/ack", "确认指定事件及所有更早的事件", "event_id":"integer!"),
];

fn property(kind: &str) -> Value {
    match kind {
        "strings" => json!({"type":"array", "items":{"type":"string"}}),
        "avatar" => json!({"oneOf":[
            {"type":"null"},
            {"type":"object","additionalProperties":false,"required":["kind","id"],"properties":{
                "kind":{"const":"preset"}, "id":{"enum":crate::server::db::accounts::PRESETS.iter().map(|(id, _)| id).collect::<Vec<_>>()}
            }},
            {"type":"object","additionalProperties":false,"required":["kind","media_type","data_base64"],"properties":{
                "kind":{"const":"upload"}, "media_type":{"enum":["image/png","image/jpeg","image/webp"]},
                "data_base64":{"type":"string","description":"标准 Base64，图片最多 256 KiB，宽高最多 1024 像素"}
            }}
        ]}),
        "nullable" => json!({"type":["string","null"]}),
        "tier" => json!({"type":"string","enum":["C","B","A","S"]}),
        "mode" => json!({"type":"string","enum":["any","min_tier","specific"]}),
        "grade" => json!({"type":"string","enum":["perfect","flawed"]}),
        "items" => json!({"type":"array","items": {
            "type":"object", "additionalProperties":false,
            "required":["title","difficulty","assignee_mode"],
            "properties":{"key":property("string"),"title":property("string"),
                "description":property("string"),"acceptance_criteria":property("string"),
                "difficulty":property("tier"),"assignee_mode":property("mode"),
                "assignee_min_tier":property("tier"),"assignee_agent_id":property("string"),
                "require_arbitration":property("boolean"),"blocked_by":property("strings")}
        }}),
        _ => json!({"type":kind}),
    }
}

pub(crate) struct Operation {
    pub method: Method,
    pub path: String,
    pub body: Option<Value>,
}

impl Action {
    pub(crate) fn signs_in(self) -> bool {
        self.group == "account" && matches!(self.command, "register" | "login")
    }

    pub(crate) fn tool(self) -> Tool {
        let mut required = Vec::new();
        let properties: Map<String, Value> = self
            .fields
            .iter()
            .map(|&(field, ty)| {
                let ty = if let Some(ty) = ty.strip_suffix('!') {
                    required.push(field);
                    ty
                } else {
                    ty
                };
                (field.to_owned(), property(ty))
            })
            .collect();
        let schema = json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
        Tool::new(
            self.name,
            self.description,
            schema.as_object().unwrap().clone(),
        )
    }

    /// URL 路径只来自固定操作与校验后的任务 ID，参数无法选中其他 HTTP 路由。
    pub(crate) fn operation(self, mut args: Map<String, Value>) -> Result<Operation, String> {
        for &(name, kind) in self.fields {
            if kind.ends_with('!') && !args.contains_key(name) {
                return Err(format!("缺少参数 {name}"));
            }
        }
        if let Some(name) = args
            .keys()
            .find(|name| !self.fields.iter().any(|(field, _)| field == name))
        {
            return Err(format!("未知参数 {name}"));
        }
        let namespace = if self.group == "account" {
            "auth"
        } else {
            "agent"
        };
        let mut path = format!("/api/v1/{namespace}{}", self.path);
        if self.signs_in() {
            args.insert("role".into(), json!("agent"));
        }
        if let Some(id) = args.remove("task_id") {
            let id = id
                .as_str()
                .filter(|id| uuid::Uuid::parse_str(id).is_ok())
                .ok_or("task_id 必须是任务 UUID")?;
            path = path.replace("{id}", id);
        }
        let method = self.method.parse::<Method>().expect("固定工具 HTTP 方法");
        let body = if method == Method::GET {
            let mut query = url::form_urlencoded::Serializer::new(String::new());
            for (key, value) in args {
                match value {
                    Value::String(text) => query.append_pair(&key, &text),
                    Value::Bool(_) | Value::Number(_) => {
                        query.append_pair(&key, &value.to_string())
                    }
                    _ => return Err(format!("参数 {key} 必须是标量")),
                };
            }
            let query = query.finish();
            if !query.is_empty() {
                path.push('?');
                path.push_str(&query);
            }
            None
        } else if self.command == "claim" {
            None
        } else {
            Some(Value::Object(args))
        };
        Ok(Operation { method, path, body })
    }
}

pub(crate) type ToolResult = Result<Value, Value>;

pub(crate) fn sign_in_required() -> Value {
    json!({"error":{"code":"UNAUTHORIZED","message":"请先注册或登录"}})
}

/// HTTP MCP 只传递已验证的身份。CLI 与 STDIO 使用其本地连接配置访问相同的 API。
#[derive(Clone)]
pub(crate) enum ToolServer {
    Stdio(crate::agent_cli::Client),
    Http(axum::Router),
}

impl ServerHandler for ToolServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("taskrail", env!("CARGO_PKG_VERSION")))
            .with_instructions("首次使用请调用 taskrail_register 或 taskrail_login。认领后保存 lease_id 并在到期前续租。遇到 STALE_LEASE 时重读任务。按 event_id 顺序处理收件箱，再确认已处理的最后一个事件。")
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let mut result = ListToolsResult::default();
        result.tools = ACTIONS.iter().map(|action| action.tool()).collect();
        Ok(result)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let spec = ACTIONS
            .iter()
            .find(|action| action.name == request.name)
            .ok_or_else(|| ErrorData::invalid_params("未知 Taskrail 工具", None))?;
        let identity = context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<crate::server::McpIdentity>());
        let authorized = match self {
            Self::Stdio(client) => client.has_credentials(),
            Self::Http(_) => identity.is_some_and(|identity| identity.agent.is_some()),
        };
        let result = if !spec.signs_in() && !authorized {
            Err(sign_in_required())
        } else {
            match spec.operation(request.arguments.unwrap_or_default()) {
                Err(message) => Err(json!({"error":{"code":"INVALID_ARGUMENT","message":message}})),
                Ok(operation) => match self {
                    Self::Stdio(client) => client.call(operation, spec.signs_in()).await,
                    Self::Http(router) => {
                        let identity = identity.ok_or_else(|| {
                            ErrorData::internal_error("缺少已验证的 Agent 身份", None)
                        })?;
                        identity.execute(router.clone(), operation).await
                    }
                },
            }
        };
        Ok(match result {
            Ok(value) => CallToolResult::structured(json!({"data":value})),
            Err(error) => CallToolResult::structured_error(error),
        }
        .into())
    }
}
