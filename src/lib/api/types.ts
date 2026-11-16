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
  require_arbitration: boolean;
  status: BountyStatus;
  created_by: string;
  lease_id: number;
  lease_agent_id: string | null;
  lease_expires_at: string | null;
  rejection_count: number;
  completion_note: string | null;
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

export interface BountyDetail {
  bounty: BountyTask;
  events: BountyEvent[];
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

export type ServiceErrorCode =
  | 'VALIDATION_ERROR'
  | 'UNAUTHORIZED'
  | 'PROJECT_NOT_FOUND'
  | 'BOUNTY_NOT_FOUND'
  | 'NOT_FOUND'
  | 'METHOD_NOT_ALLOWED'
  | 'PROJECT_HAS_BOUNTIES'
  | 'PROJECT_ARCHIVED'
  | 'INVALID_TRANSITION'
  | 'BOUNTY_ALREADY_CLAIMED'
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
