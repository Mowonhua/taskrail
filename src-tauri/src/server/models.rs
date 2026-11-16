//! 文件职责：定义仓储和 HTTP 共用的数据契约；JSON 字段统一使用 snake_case。
use serde::{Deserialize, Deserializer, Serialize};

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $name { $(#[serde(rename = $value)] $variant),+ }
        impl $name {
            pub fn as_str(self) -> &'static str { match self { $(Self::$variant => $value),+ } }
        }
    };
}

string_enum!(Tier { C => "C", B => "B", A => "A", S => "S" });
string_enum!(AssigneeMode { Any => "any", MinTier => "min_tier", Specific => "specific" });
string_enum!(BountyStatus {
    Draft => "DRAFT", Open => "OPEN", InProgress => "IN_PROGRESS",
    WaitingInfo => "WAITING_INFO", PendingArbitration => "PENDING_ARBITRATION",
    Completed => "COMPLETED", Cancelled => "CANCELLED"
});
string_enum!(AgentDraftPolicy { Open => "open", Draft => "draft" });
string_enum!(AgentTier { Unranked => "Unranked", C => "C", B => "B", A => "A", S => "S" });
string_enum!(AgentStatus { Active => "active", Disabled => "disabled", Revoked => "revoked" });
string_enum!(AgentActivity { Idle => "idle", Busy => "busy" });

/// 结构职责：邀请令牌的公开摘要；永远不包含摘要密钥或明文凭证。
#[derive(Debug, Serialize, Deserialize)]
pub struct InviteTokenSummary {
    pub id: String,
    pub label: String,
    pub token_prefix: String,
    pub max_uses: Option<i64>,
    pub use_count: i64,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
    pub created_at: String,
}

/// 结构职责：创建邀请；次数须为正整数，有效小时数须为有限正数。
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CreateInviteToken {
    pub label: String,
    pub max_uses: Option<i64>,
    pub expires_in_hours: Option<f64>,
}

/// 结构职责：仅创建时返回明文邀请；后续列表仅返回 token 摘要。
#[derive(Serialize)]
pub struct CreatedInviteToken {
    pub token: InviteTokenSummary,
    pub raw_token: String,
}

/// 结构职责：管理员查看的准入配置及邀请摘要。
#[derive(Serialize)]
pub struct Onboarding {
    pub lan_open_registration: bool,
    pub invite_tokens: Vec<InviteTokenSummary>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateOnboarding {
    pub lan_open_registration: bool,
}

/// 结构职责：Agent 注册输入；名称去空白后限 1 至 64 个 Unicode 字符。
/// 约束条件：全局订阅不能同时指定非空项目列表；缺省订阅全部项目。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterAgent {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub invite_token: Option<String>,
    pub subscribe_all_projects: Option<bool>,
    pub project_ids: Option<Vec<String>>,
    pub webhook_url: Option<String>,
    pub webhook_secret: Option<String>,
}

/// 结构职责：管理员名录及 Agent 自身使用的公开资料；不包含 MMR 和凭证摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub api_key_prefix: String,
    pub tier: AgentTier,
    pub placement_completed: bool,
    pub subscribe_all_projects: bool,
    pub project_ids: Vec<String>,
    pub status: AgentStatus,
    pub invite_token_id: Option<String>,
    pub active_lease_count: i64,
    pub last_seen_at: Option<String>,
    pub channel_status: String,
    pub stream_connected: bool,
    pub webhook_configured: bool,
    pub webhook_url: Option<String>,
    pub last_pushed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 结构职责：注册成功的一次性凭证；后续读取不能恢复 api_key。
#[derive(Serialize)]
pub struct RegisteredAgent {
    pub agent: AgentSummary,
    pub api_key: String,
    /// 注册配置回调时仅此返回签名密钥；名录及自身资料永不包含它。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_secret: Option<String>,
}

/// 结构职责：Agent 可自行更新的资料；不允许修改名称、凭证或评级。
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UpdateAgent {
    pub description: Option<String>,
    pub capabilities: Option<Vec<String>>,
}

/// 结构职责：完整替换项目订阅；空对象恢复全局订阅，false 加空列表表示不订阅。
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentSubscriptions {
    pub subscribe_all_projects: Option<bool>,
    pub project_ids: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentFilter {
    pub status: Option<AgentStatus>,
    pub activity: Option<AgentActivity>,
}

fn open_policy() -> AgentDraftPolicy {
    AgentDraftPolicy::Open
}
fn human() -> String {
    "human".into()
}

/// 结构职责：可空字段的 PATCH 值；缺省保持原值，显式 null 清空。
#[derive(Debug, Clone, Default)]
pub enum NullablePatch<T> {
    #[default]
    Unset,
    Set(Option<T>),
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for NullablePatch<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Option::deserialize(d).map(Self::Set)
    }
}
impl<T> NullablePatch<T> {
    pub(crate) fn apply(self, target: &mut Option<T>) {
        if let Self::Set(value) = self {
            *target = value;
        }
    }
}

/// 结构职责：项目创建输入；名称和图标内容在仓储边界校验。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateProject {
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub workspace_path: String,
    #[serde(default)]
    pub default_acceptance_criteria: String,
    #[serde(default = "open_policy")]
    pub agent_draft_policy: AgentDraftPolicy,
    #[serde(default)]
    pub arbitrator_chain: Vec<String>,
}

/// 结构职责：项目部分更新；icon 支持 null，其余字段 null 不改变现值。
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UpdateProject {
    pub name: Option<String>,
    pub icon: NullablePatch<String>,
    pub description: Option<String>,
    pub workspace_path: Option<String>,
    pub default_acceptance_criteria: Option<String>,
    pub agent_draft_policy: Option<AgentDraftPolicy>,
    pub arbitrator_chain: Option<Vec<String>>,
    pub archived: Option<bool>,
}

/// 结构职责：项目持久化快照；arbitrator_chain 在 SQLite 中保存为 JSON 数组。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub description: String,
    pub workspace_path: String,
    pub default_acceptance_criteria: String,
    pub agent_draft_policy: AgentDraftPolicy,
    pub arbitrator_chain: Vec<String>,
    pub archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 结构职责：人工创建悬赏的内容；仅允许 DRAFT 或 OPEN 初始状态。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBounty {
    pub project_id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub acceptance_criteria: String,
    pub difficulty: Tier,
    pub assignee_mode: AssigneeMode,
    #[serde(default)]
    pub assignee_min_tier: Option<Tier>,
    #[serde(default)]
    pub assignee_agent_id: Option<String>,
    #[serde(default)]
    pub require_arbitration: bool,
    pub status: BountyStatus,
    #[serde(default = "human")]
    pub created_by: String,
}

/// 结构职责：仅草稿可更新的内容；状态、发布者和租约不得由 PATCH 修改。
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UpdateBounty {
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub difficulty: Option<Tier>,
    pub assignee_mode: Option<AssigneeMode>,
    pub assignee_min_tier: NullablePatch<Tier>,
    pub assignee_agent_id: NullablePatch<String>,
    pub require_arbitration: Option<bool>,
}

/// 结构职责：悬赏持久化快照；lease_id 只递增，时间为 UTC RFC3339。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bounty {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: String,
    pub difficulty: Tier,
    pub assignee_mode: AssigneeMode,
    pub assignee_min_tier: Option<Tier>,
    pub assignee_agent_id: Option<String>,
    pub require_arbitration: bool,
    pub status: BountyStatus,
    pub created_by: String,
    pub lease_id: i64,
    pub lease_agent_id: Option<String>,
    pub lease_expires_at: Option<String>,
    pub rejection_count: i64,
    pub completion_note: Option<String>,
    pub published_at: Option<String>,
    pub withdrawn_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 结构职责：悬赏检索条件；include_drafts 默认 false，即使 status=DRAFT 也先排除草稿。
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BountyFilter {
    pub project_id: Option<String>,
    pub status: Option<BountyStatus>,
    pub q: Option<String>,
    pub include_drafts: bool,
}

/// 结构职责：与悬赏写操作原子提交的事件；按 id 递增读取。
#[derive(Debug, Serialize, Deserialize)]
pub struct BountyEvent {
    pub id: i64,
    pub bounty_id: String,
    pub event_type: String,
    pub actor: String,
    pub from_status: Option<BountyStatus>,
    pub to_status: BountyStatus,
    pub detail: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BountyDetail {
    pub bounty: Bounty,
    pub events: Vec<BountyEvent>,
}

/// 结构职责：提供给桌面前端的当前会话连接信息；token 不写入磁盘或日志。
#[derive(Debug, Clone, Serialize)]
pub struct EmbeddedServerInfo {
    pub base_url: String,
    pub admin_token: String,
    pub db_path: String,
}

/// 结构职责：目标 Agent 独占的不可变事件快照，不包含服务端凭证或评级内部分数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEvent {
    pub event_id: i64,
    pub event_type: String,
    pub bounty_id: Option<String>,
    pub project_id: Option<String>,
    pub created_at: String,
    pub data: serde_json::Value,
}

/// 结构职责：按 Agent 隔离的收件箱分页；cursor 为本页末项，空页保留请求游标。
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentInbox {
    pub events: Vec<AgentEvent>,
    pub cursor: i64,
    pub acked_event_id: i64,
    pub latest_event_id: i64,
    pub has_more: bool,
}

/// 结构职责：完整替换回调地址；null 清除地址与密钥，省略密钥保留已有值。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateWebhook {
    #[serde(deserialize_with = "Option::deserialize")]
    pub webhook_url: Option<String>,
    pub webhook_secret: Option<String>,
}

/// 结构职责：回调公开配置；密钥仅在显式更新或新生成时返回一次。
#[derive(Debug, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub webhook_url: Option<String>,
    pub webhook_configured: bool,
    pub has_secret: bool,
    pub last_pushed_at: Option<String>,
    pub recent_failed_count: i64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub webhook_secret: Option<String>,
}
