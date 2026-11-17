/**
 * 文件职责：定义悬赏服务 HTTP API 与嵌入式连接的数据契约与错误类型。
 * 约束条件：字段命名与 src-tauri/src/server/models.rs 及 README.md 保持 snake_case 一致。
 */

export type ServiceMode = 'embedded' | 'remote';
export type ServerMode = 'embedded' | 'standalone';

export interface EmbeddedServerInfo {
  base_url: string;
  admin_token: string;
  db_path: string;
}

export interface ServerHealth {
  ok: boolean;
  mode: ServerMode;
  version: string;
}

export interface AdminStatus {
  ok: boolean;
  mode: ServerMode;
  db_path: string;
  version: string;
}

export type AgentDraftPolicy = 'open' | 'draft';

export interface BountyProject {
  id: string;
  name: string;
  icon: string | null;
  description: string;
  workspace_path: string;
  default_acceptance_criteria: string;
  agent_draft_policy: AgentDraftPolicy;
  arbitrator_chain: string[];
  archived: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreateProjectInput {
  name: string;
  icon?: string | null;
  description?: string;
  workspace_path?: string;
  default_acceptance_criteria?: string;
  agent_draft_policy?: AgentDraftPolicy;
  arbitrator_chain?: string[];
}

export interface UpdateProjectInput {
  name?: string;
  icon?: string | null;
  description?: string;
  workspace_path?: string;
  default_acceptance_criteria?: string;
  agent_draft_policy?: AgentDraftPolicy;
  arbitrator_chain?: string[];
  archived?: boolean;
}

export type BountyDifficulty = 'C' | 'B' | 'A' | 'S';

export type BountyStatus =
  | 'DRAFT'
  | 'OPEN'
  | 'IN_PROGRESS'
  | 'WAITING_INFO'
  | 'PENDING_ARBITRATION'
  | 'COMPLETED'
  | 'CANCELLED';

export type AssigneeMode = 'any' | 'min_tier' | 'specific';

export interface BountyTask {
  id: string;
  project_id: string;
  title: string;
  description: string;
  acceptance_criteria: string;
  difficulty: BountyDifficulty;
  assignee_mode: AssigneeMode;
  assignee_min_tier: BountyDifficulty | null;
  assignee_agent_id: string | null;
  assignee_agent_name: string | null;
  assignee_agent_tier: AgentTier | null;
  assignment_expires_at: string | null;
  require_arbitration: boolean;
  status: BountyStatus;
  created_by: string;
  created_by_agent_id: string | null;
  created_by_agent_name: string | null;
  lease_id: number;
  lease_agent_id: string | null;
  lease_agent_name: string | null;
  lease_agent_tier: AgentTier | null;
  lease_expires_at: string | null;
  rejection_count: number;
  completion_note: string | null;
  completion_cost: number | null;
  active_duration_ms: number;
  active_started_at: string | null;
  comment_count: number;
  published_at: string | null;
  withdrawn_at: string | null;
  cancelled_at: string | null;
  completed_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface BountyEvent {
  id: number;
  bounty_id: string;
  event_type: string;
  actor: string;
  from_status: BountyStatus | null;
  to_status: BountyStatus;
  detail: Record<string, unknown>;
  created_at: string;
}

export type BountyCommentAuthorType = 'human' | 'agent' | 'system';
export type BountyCommentKind = 'comment' | 'question' | 'reply' | 'status';

export interface BountyComment {
  id: number;
  bounty_id: string;
  author_type: BountyCommentAuthorType;
  author_id: string | null;
  author_name: string;
  kind: BountyCommentKind;
  body: string;
  created_at: string;
}

export interface CreateBountyCommentInput {
  body: string;
}

export interface RequestInfoInput {
  lease_id: number;
  question: string;
}

export interface BountyCommentMutationResult {
  bounty: BountyTask;
  comment: BountyComment;
}

export interface BountyDetail {
  bounty: BountyTask;
  events: BountyEvent[];
  comments: BountyComment[];
}

export interface CreateBountyInput {
  project_id: string;
  title: string;
  description?: string;
  acceptance_criteria?: string;
  difficulty: BountyDifficulty;
  assignee_mode: AssigneeMode;
  assignee_min_tier?: BountyDifficulty | null;
  assignee_agent_id?: string | null;
  require_arbitration?: boolean;
  status: 'DRAFT' | 'OPEN';
  created_by?: string;
}

export interface AgentCreateBountyInput {
  project_id: string;
  title: string;
  description?: string;
  acceptance_criteria?: string;
  difficulty: BountyDifficulty;
  assignee_mode: AssigneeMode;
  assignee_min_tier?: BountyDifficulty | null;
  assignee_agent_id?: string | null;
  require_arbitration?: boolean;
}

export interface UpdateBountyInput {
  project_id?: string;
  title?: string;
  description?: string;
  acceptance_criteria?: string;
  difficulty?: BountyDifficulty;
  assignee_mode?: AssigneeMode;
  assignee_min_tier?: BountyDifficulty | null;
  assignee_agent_id?: string | null;
  require_arbitration?: boolean;
}

export interface BountyQuery {
  project_id?: string;
  status?: BountyStatus;
  q?: string;
  include_drafts?: boolean;
}

export interface DeclineBountyInput {
  reason?: string | null;
}

export interface LeaseHeartbeatInput {
  lease_id: number;
}

export interface AbandonBountyInput {
  lease_id: number;
  reason?: string | null;
}

export interface CompleteBountyInput {
  lease_id: number;
  completion_note?: string | null;
  completion_cost?: number | null;
}

export type AgentTier = 'Unranked' | 'C' | 'B' | 'A' | 'S';
export type AgentStatus = 'active' | 'disabled' | 'revoked';
export type AgentActivity = 'idle' | 'busy';
export type AgentChannelStatus = 'streaming' | 'webhook' | 'offline';

export interface InviteTokenSummary {
  id: string;
  label: string;
  token_prefix: string;
  max_uses: number | null;
  use_count: number;
  expires_at: string | null;
  revoked_at: string | null;
  created_at: string;
}

export interface CreateInviteTokenInput {
  label?: string;
  max_uses?: number | null;
  expires_in_hours?: number | null;
}

export interface CreatedInviteToken {
  token: InviteTokenSummary;
  raw_token: string;
}

export interface OnboardingSettings {
  lan_open_registration: boolean;
  invite_tokens: InviteTokenSummary[];
}

export interface UpdateOnboardingInput {
  lan_open_registration: boolean;
}

export interface SkillDocument {
  base_url: string;
  content: string;
  filename: string;
}

export interface AgentSummary {
  id: string;
  name: string;
  description: string;
  capabilities: string[];
  api_key_prefix: string;
  tier: AgentTier;
  placement_completed: boolean;
  subscribe_all_projects: boolean;
  project_ids: string[];
  status: AgentStatus;
  invite_token_id: string | null;
  active_lease_count: number;
  timeout_count: number;
  abandon_count: number;
  last_seen_at: string | null;
  channel_status: AgentChannelStatus;
  stream_connected: boolean;
  webhook_configured: boolean;
  webhook_url: string | null;
  last_pushed_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface AgentFilter {
  status?: AgentStatus;
  activity?: AgentActivity;
}

export interface RegisterAgentInput {
  name: string;
  description?: string;
  capabilities?: string[];
  invite_token?: string | null;
  subscribe_all_projects?: boolean;
  project_ids?: string[];
  webhook_url?: string | null;
  webhook_secret?: string | null;
}

export interface RegisteredAgentResult {
  agent: AgentSummary;
  api_key: string;
  webhook_secret?: string;
}

export interface UpdateAgentInput {
  description?: string;
  capabilities?: string[];
}

export interface AgentSubscriptionsInput {
  subscribe_all_projects?: boolean;
  project_ids?: string[];
}

export type AgentEventType =
  | 'task.published'
  | 'task.assigned'
  | 'task.withdrawn'
  | 'task.assignment_declined'
  | 'task.assignment_expired'
  | 'lease.expiring'
  | 'lease.expired'
  | 'lease.released'
  | 'task.info_requested'
  | 'comment.added'
  | 'comment.replied'
  | 'task.resumed'
  | 'task.cancelled'
  | string;

export interface AgentEvent {
  event_id: number;
  event_type: AgentEventType;
  bounty_id: string | null;
  project_id: string | null;
  created_at: string;
  data: Record<string, unknown>;
}

export interface AgentInboxResponse {
  events: AgentEvent[];
  cursor: number;
  acked_event_id: number;
  latest_event_id: number;
  has_more: boolean;
}

export interface AgentWebhookConfig {
  webhook_url: string | null;
  webhook_configured: boolean;
  has_secret: boolean;
  last_pushed_at: string | null;
  recent_failed_count: number;
  webhook_secret?: string;
}

export interface UpdateAgentWebhookInput {
  webhook_url: string | null;
  webhook_secret?: string | null;
}

export interface AckAgentEventsResult {
  acked_event_id: number;
}

export type ServiceErrorCode =
  | 'VALIDATION_ERROR'
  | 'UNAUTHORIZED'
  | 'INVITE_TOKEN_REQUIRED'
  | 'INVITE_TOKEN_INVALID'
  | 'AGENT_REVOKED'
  | 'AGENT_DISABLED'
  | 'ASSIGNEE_MISMATCH'
  | 'TIER_INSUFFICIENT'
  | 'LEASE_OWNER_MISMATCH'
  | 'PROJECT_NOT_FOUND'
  | 'BOUNTY_NOT_FOUND'
  | 'AGENT_NOT_FOUND'
  | 'INVITE_TOKEN_NOT_FOUND'
  | 'NOT_FOUND'
  | 'METHOD_NOT_ALLOWED'
  | 'PROJECT_HAS_BOUNTIES'
  | 'PROJECT_ARCHIVED'
  | 'INVALID_TRANSITION'
  | 'AGENT_NAME_TAKEN'
  | 'BOUNTY_ALREADY_CLAIMED'
  | 'STALE_LEASE'
  | 'ARBITRATION_REQUIRED'
  | 'INTERNAL_ERROR';

export class ServiceApiError extends Error {
  readonly code: string;
  readonly status: number;

  constructor(code: string, status: number, message: string) {
    super(message);
    this.name = 'ServiceApiError';
    this.code = code;
    this.status = status;
  }
}
