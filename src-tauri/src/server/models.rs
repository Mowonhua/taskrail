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

fn open_policy() -> AgentDraftPolicy {
    AgentDraftPolicy::Open
}
fn human() -> String {
    "human".into()
}

/// 结构职责：可空字段的 PATCH 值；缺省保持原值，显式 null 清空。
#[derive(Debug, Clone)]
pub enum NullablePatch<T> {
    Unset,
    Set(Option<T>),
}
impl<T> Default for NullablePatch<T> {
    fn default() -> Self {
        Self::Unset
    }
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
