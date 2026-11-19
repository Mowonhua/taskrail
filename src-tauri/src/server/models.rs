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
// 验收质量档位，供交付结算读取。
string_enum!(QualityGrade { Perfect => "perfect", Flawed => "flawed" });
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

string_enum!(RatingOutcome { Completed => "completed", ArbitrationFailed => "arbitration_failed", Timeout => "timeout", Abandon => "abandon" });

/// 结构职责：仅管理员详情使用的结算快照；不得嵌入公开资料、任务或事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRatingLedgerEntry {
    pub id: i64,
    pub agent_id: String,
    pub bounty_id: String,
    pub bounty_title: String,
    pub project_id: String,
    pub lease_id: i64,
    pub outcome: RatingOutcome,
    pub arbitrated: bool,
    pub self_dealt: bool,
    pub original_difficulty: Tier,
    pub effective_difficulty: Tier,
    pub quality_grade: Option<QualityGrade>,
    pub rejection_count: i64,
    pub quality_score: f64,
    pub active_duration_ms: i64,
    pub time_efficiency: f64,
    pub completion_cost: Option<f64>,
    pub cost_efficiency: f64,
    pub actual_score: f64,
    pub expected_score: f64,
    pub k_factor: f64,
    pub is_placement: bool,
    pub raw_delta: f64,
    pub applied_delta: f64,
    pub mmr_before: f64,
    pub mmr_after: f64,
    pub tier_before: AgentTier,
    pub tier_after: AgentTier,
    pub cap_reason: Option<String>,
    pub tier_gate_reason: Option<String>,
    pub created_at: String,
}

/// 结构职责：管理员专用评级详情；公开资料继续使用 AgentSummary。
/// 约束条件：进度、统计和倒序流水来自同一个数据库读快照；进度最多为 5。
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentDetail {
    pub agent: AgentSummary,
    pub mmr: f64,
    pub placement_count: i64,
    pub placement_required: i64,
    pub completed_count: i64,
    pub failed_count: i64,
    pub verified_as_wins: i64,
    pub verified_s_wins: i64,
    pub ledger: Vec<AgentRatingLedgerEntry>,
}

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
    pub timeout_count: i64,
    pub abandon_count: i64,
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

/// 结构职责：人工创建任务的内容；仅允许 DRAFT 或 OPEN 初始状态。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBounty {
    #[serde(default)]
    pub blocked_by: Vec<String>,
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

/// 结构职责：Agent 提交的任务内容；发布者由认证身份绑定，初始状态由项目策略决定。
/// 约束条件：拒绝状态、发布者等额外字段；执行者限制组合与人工创建采用相同校验。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCreateBounty {
    #[serde(default)]
    pub blocked_by: Vec<String>,
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
}

/// 结构职责：仅草稿可更新的内容；状态、发布者和租约不得由 PATCH 修改。
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UpdateBounty {
    pub blocked_by: Option<Vec<String>>,
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

/// 结构职责：任务持久化快照；lease_id 只递增，时间为 UTC RFC3339。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bounty {
    /// 只有当前受邀身份可读取 DRAFT；名称和段位关联读取当前公开资料。
    pub draft_collaborator_agent_id: Option<String>,
    pub draft_collaborator_agent_name: Option<String>,
    pub draft_collaborator_agent_tier: Option<AgentTier>,
    /// 受邀者评论、修订或拆单置 true；人类草稿评论及发布、撤回、取消清零。
    pub draft_needs_human_review: bool,
    /// 拆单时绑定父草稿；父任务删除时由数据库置空。
    pub parent_bounty_id: Option<String>,
    /// 依赖按声明顺序返回；Agent 视角对不可见草稿的摘要标题脱敏。
    pub blocked_by: Vec<String>,
    pub blocked_by_summaries: Vec<BountyDependencySummary>,
    /// 任意前置任务尚未完成即阻塞，与当前任务状态无关。
    pub is_blocked: bool,
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: String,
    pub difficulty: Tier,
    pub assignee_mode: AssigneeMode,
    pub assignee_min_tier: Option<Tier>,
    pub assignee_agent_id: Option<String>,
    /// 由指派身份关联读取的当前公开资料；历史未知身份保持 None。
    pub assignee_agent_name: Option<String>,
    pub assignee_agent_tier: Option<AgentTier>,
    pub assignment_expires_at: Option<String>,
    pub require_arbitration: bool,
    pub status: BountyStatus,
    pub created_by: String,
    /// Agent 发布时绑定的身份；人工创建为 None，编辑和状态流转均保留该值。
    pub created_by_agent_id: Option<String>,
    /// 由发布者外键关联读取的当前名称，不包含 Agent 凭证或评分。
    pub created_by_agent_name: Option<String>,
    pub lease_id: i64,
    pub lease_agent_id: Option<String>,
    /// 由租约身份关联读取的当前公开资料；完成后保留执行者身份。
    pub lease_agent_name: Option<String>,
    pub lease_agent_tier: Option<AgentTier>,
    pub lease_expires_at: Option<String>,
    /// 已结转的执行毫秒数；当前段由 active_started_at 起算，等待回复期间不增长。
    pub active_duration_ms: i64,
    pub active_started_at: Option<String>,
    pub comment_count: i64,
    pub rejection_count: i64,
    pub arbitrator_agent_id: Option<String>,
    /// 仅关联读取裁判当前公开资料，不序列化凭证或内部评分。
    pub arbitrator_agent_name: Option<String>,
    pub arbitrator_agent_tier: Option<AgentTier>,
    pub arbitration_expires_at: Option<String>,
    pub quality_grade: Option<QualityGrade>,
    pub calibrated_difficulty: Option<Tier>,
    pub completion_note: Option<String>,
    pub completion_cost: Option<f64>,
    pub published_at: Option<String>,
    pub withdrawn_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 结构职责：被指派者婉拒任务时可选的原因；HTTP 允许省略整个正文。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclineBounty {
    pub reason: Option<String>,
}

/// 结构职责：续租请求；仅当前持有者在租约到期前可使用当前令牌。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseHeartbeat {
    pub lease_id: i64,
}

/// 结构职责：主动弃单请求；过期令牌不能释放新一轮租约。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbandonBounty {
    pub lease_id: i64,
    pub reason: Option<String>,
}

/// 结构职责：无需仲裁的直接交付；成本须为有限非负数，空白说明归一为 None。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteBounty {
    pub lease_id: i64,
    pub completion_note: Option<String>,
    pub completion_cost: Option<f64>,
}

/// 结构职责：任务检索条件；include_drafts 默认 false，即使 status=DRAFT 也先排除草稿。
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BountyFilter {
    pub project_id: Option<String>,
    pub status: Option<BountyStatus>,
    pub q: Option<String>,
    pub include_drafts: bool,
}

/// 结构职责：与任务写操作原子提交的事件；按 id 递增读取。
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
    /// 下游按 created_at、id 排序；Agent 视角隐藏不可见草稿。
    pub blocks: Vec<BountyDependencySummary>,
    /// 派生任务按 created_at、id 排序；Agent 视角仅返回有权查看的记录。
    pub child_bounties: Vec<BountyDependencySummary>,
    pub bounty: Bounty,
    pub events: Vec<BountyEvent>,
    pub comments: Vec<BountyComment>,
}

/// 结构职责：依赖和派生关系的最小公开摘要；不包含正文或 Agent 内部资料。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BountyDependencySummary {
    pub id: String,
    pub title: String,
    pub status: BountyStatus,
}

/// 结构职责：管理员替换草稿协作者；null 或省略 agent_id 取消邀请。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InviteDraftCollaboratorInput {
    pub agent_id: Option<String>,
}

/// 结构职责：受邀协作者可修订的草稿内容；依赖整体替换。
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentUpdateDraftInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub blocked_by: Option<Vec<String>>,
}

/// 结构职责：一个派生子草稿；key 去空白后须非空且批内唯一，可供兄弟项引用。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitDraftItem {
    pub key: Option<String>,
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
    #[serde(default)]
    pub blocked_by: Vec<String>,
}

/// 结构职责：非空拆单批次；全部子草稿、依赖边和父草稿历史原子提交。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitDraftInput {
    pub items: Vec<SplitDraftItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SplitDraftResult {
    pub parent: Bounty,
    pub children: Vec<Bounty>,
}

/// 结构职责：管理员原子发布草稿集合；ID 拒绝空白并按首次出现顺序去重。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishBountiesBatchInput {
    pub bounty_ids: Vec<String>,
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

/// 结构职责：评论作者身份由认证上下文确定；读取 Agent 当前名称，缺失时回退写入快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BountyComment {
    pub id: i64,
    pub bounty_id: String,
    pub author_type: String,
    pub author_id: Option<String>,
    pub author_name: String,
    pub kind: String,
    pub body: String,
    #[serde(default)]
    pub parent_id: Option<i64>,
    #[serde(default)]
    pub reply_to_comment_id: Option<i64>,
    #[serde(default)]
    pub reply_to_author_name: Option<String>,
    #[serde(default)]
    pub like_count: i64,
    #[serde(default)]
    pub liked: bool,
    #[serde(default)]
    pub updated_at: Option<String>,
    pub created_at: String,
}

/// 结构职责：评论输入；服务确定作者与类型，正文去除首尾空白后应非空。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBountyComment {
    pub body: String,
    #[serde(default)]
    pub reply_to_comment_id: Option<i64>,
}

/// 结构职责：编辑评论正文；去除首尾空白后必须非空。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateBountyComment {
    pub body: String,
}

/// 结构职责：删除评论结果；返回最新任务快照和被删除的评论 ID 列表。
#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteBountyCommentResult {
    pub bounty: Bounty,
    pub deleted_ids: Vec<i64>,
}

/// 结构职责：当前租约持有者的提问；先校验令牌，再校验非空问题正文。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestInfoInput {
    pub lease_id: i64,
    pub question: String,
}

/// 结构职责：评论写入结果；任务快照包含本条评论计数以及回复恢复后的租约。
#[derive(Debug, Serialize, Deserialize)]
pub struct BountyCommentResult {
    pub bounty: Bounty,
    pub comment: BountyComment,
}

/// 结构职责：整体替换全局仲裁顺位；拒绝空白项，保留首次出现的 ID 顺序。
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArbitrationConfig {
    pub arbitrator_chain: Vec<String>,
}

/// 结构职责：持有者凭当前租约提交仲裁；成本须有限非负，空白说明保存为 None。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitArbitrationInput {
    pub lease_id: i64,
    pub completion_note: Option<String>,
    pub completion_cost: Option<f64>,
}

/// 结构职责：裁判验收结果；质量缺省 perfect，可选校准难度与评论。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApproveArbitrationInput {
    pub quality_grade: Option<QualityGrade>,
    pub calibrated_difficulty: Option<Tier>,
    pub comment: Option<String>,
}

/// 结构职责：打回裁决；正文去除首尾空白后必须非空。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RejectArbitrationInput {
    pub comment: String,
}
