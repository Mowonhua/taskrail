/**
 * 文件职责：提供任务服务客户端端口、HTTP 实现、嵌入式连接信息读取与内存模拟实现。
 * 约束条件：HTTP 客户端不向 /api/v1/health 发送管理员鉴权头；内存实现严格对齐 Rust 仓储状态机与错误码。
 */
import { invoke } from '@tauri-apps/api/core';
import skillTemplate from '../../../assets/skills/taskrail-agent/SKILL.md?raw';
import { parseProjectIcon } from '../project-icons';
import {
  DEFAULT_AUTH_AVATAR_PRESETS,
  validateUserAvatar,
  validateUserNameError,
  validateUserPasswordError,
} from '../auth/avatars';
import {
  ServiceApiError,
  type AbandonBountyInput,
  type AckAgentEventsResult,
  type AdminStatus,
  type AgentChannelStatus,
  type AgentCreateBountyInput,
  type AgentDetail,
  type AgentDraftPolicy,
  type AgentEvent,
  type AgentFilter,
  type AgentInboxResponse,
  type AgentRatingLedgerEntry,
  type AgentSubscriptionsInput,
  type AgentSummary,
  type AgentTier,
  type AgentUpdateDraftInput,
  type AgentWebhookConfig,
  type ApproveArbitrationInput,
  type ArbitrationConfig,
  type AssigneeMode,
  type AuthAvatarPreset,
  type AuthSessionResult,
  type BountyComment,
  type BountyCommentMutationResult,
  type BountyDependencySummary,
  type BountyDetail,
  type BountyDifficulty,
  type BountyEvent,
  type BountyProject,
  type BountyQuery,
  type BountyStatus,
  type BountyTask,
  type CompleteBountyInput,
  type CreateBountyCommentInput,
  type CreatedInviteToken,
  type CreateBountyInput,
  type CreateInviteTokenInput,
  type CreateProjectInput,
  type DeclineBountyInput,
  type DeleteBountyCommentResult,
  type EmbeddedServerInfo,
  type InviteDraftCollaboratorInput,
  type InviteTokenSummary,
  type LeaseHeartbeatInput,
  type LoginUserInput,
  type OnboardingSettings,
  type PublishBountiesBatchInput,
  type QualityGrade,
  type RatingOutcome,
  type RegisterAgentInput,
  type RegisteredAgentResult,
  type RegisterUserInput,
  type RejectArbitrationInput,
  type RequestInfoInput,
  type ServerHealth,
  type SkillDocument,
  type SplitDraftInput,
  type SplitDraftItem,
  type SplitDraftResult,
  type SubmitArbitrationInput,
  type UpdateAgentInput,
  type UpdateAgentWebhookInput,
  type UpdateBountyCommentInput,
  type UpdateBountyInput,
  type UpdateOnboardingInput,
  type UpdateProjectInput,
  type UpdateUserProfileInput,
  type UserAvatar,
  type UserProfile,
} from './types';

const VALID_DIFFICULTIES = new Set<BountyDifficulty>(['C', 'B', 'A', 'S']);
const VALID_ASSIGNEE_MODES = new Set<AssigneeMode>(['any', 'min_tier', 'specific']);
const VALID_AGENT_POLICIES = new Set<AgentDraftPolicy>(['open', 'draft']);
const VALID_QUALITY_GRADES = new Set<QualityGrade>(['perfect', 'flawed']);
const AGENT_CREATE_BOUNTY_ALLOWED_KEYS = new Set([
  'project_id',
  'title',
  'description',
  'acceptance_criteria',
  'difficulty',
  'assignee_mode',
  'assignee_min_tier',
  'assignee_agent_id',
  'require_arbitration',
  'blocked_by',
]);
const INVITE_COLLABORATOR_ALLOWED_KEYS = new Set(['agent_id']);
const AGENT_UPDATE_DRAFT_ALLOWED_KEYS = new Set([
  'title',
  'description',
  'acceptance_criteria',
  'blocked_by',
]);
const SPLIT_DRAFT_ALLOWED_KEYS = new Set(['items']);
const SPLIT_DRAFT_ITEM_ALLOWED_KEYS = new Set([
  'key',
  'title',
  'description',
  'acceptance_criteria',
  'difficulty',
  'assignee_mode',
  'assignee_min_tier',
  'assignee_agent_id',
  'require_arbitration',
  'blocked_by',
]);
const PUBLISH_BATCH_ALLOWED_KEYS = new Set(['bounty_ids']);
const DECLINE_ALLOWED_KEYS = new Set(['reason']);
const HEARTBEAT_ALLOWED_KEYS = new Set(['lease_id']);
const ABANDON_ALLOWED_KEYS = new Set(['lease_id', 'reason']);
const COMPLETE_ALLOWED_KEYS = new Set(['lease_id', 'completion_note', 'completion_cost']);
const CREATE_COMMENT_ALLOWED_KEYS = new Set(['body', 'reply_to_comment_id']);
const UPDATE_COMMENT_ALLOWED_KEYS = new Set(['body']);
const REQUEST_INFO_ALLOWED_KEYS = new Set(['lease_id', 'question']);
const ARBITRATION_CONFIG_ALLOWED_KEYS = new Set(['arbitrator_chain']);
const SUBMIT_ARBITRATION_ALLOWED_KEYS = new Set([
  'lease_id',
  'completion_note',
  'completion_cost',
]);
const APPROVE_ARBITRATION_ALLOWED_KEYS = new Set([
  'quality_grade',
  'calibrated_difficulty',
  'comment',
]);
const REJECT_ARBITRATION_ALLOWED_KEYS = new Set(['comment']);
const CANCELLABLE_STATUSES = new Set<BountyStatus>(['DRAFT', 'OPEN', 'WAITING_INFO']);
const DEFAULT_SKILL_BASE_URL = 'http://127.0.0.1:8420';
const LEASE_TTL_MS = 15 * 60 * 1000;
const LEASE_WARNING_WINDOW_MS = 3 * 60 * 1000;

export interface BountyApiPort {
  health(): Promise<ServerHealth>;
  adminStatus(): Promise<AdminStatus>;
  listProjects(includeArchived?: boolean): Promise<BountyProject[]>;
  createProject(input: CreateProjectInput): Promise<BountyProject>;
  getProject(id: string): Promise<BountyProject>;
  updateProject(id: string, input: UpdateProjectInput): Promise<BountyProject>;
  deleteProject(id: string): Promise<void>;
  listBounties(query?: BountyQuery): Promise<BountyTask[]>;
  getBountyDetail(id: string): Promise<BountyDetail>;
  listBountyComments?(id: string): Promise<BountyComment[]>;
  createBountyComment?(
    id: string,
    input: CreateBountyCommentInput,
  ): Promise<BountyCommentMutationResult>;
  toggleBountyCommentLike?(id: string, commentId: number): Promise<BountyComment>;
  updateBountyComment?(
    id: string,
    commentId: number,
    input: UpdateBountyCommentInput,
  ): Promise<BountyComment>;
  deleteBountyComment?(id: string, commentId: number): Promise<DeleteBountyCommentResult>;
  createBounty(input: CreateBountyInput): Promise<BountyTask>;
  updateBounty(id: string, input: UpdateBountyInput): Promise<BountyTask>;
  publishBounty(id: string): Promise<BountyTask>;
  inviteDraftCollaborator?(
    id: string,
    input: InviteDraftCollaboratorInput,
  ): Promise<BountyTask>;
  publishBountiesBatch?(input: PublishBountiesBatchInput): Promise<BountyTask[]>;
  withdrawBounty(id: string): Promise<BountyTask>;
  cancelBounty(id: string): Promise<BountyTask>;
  /** 将已取消的任务转为草稿，保留任务标识和历史记录。 */
  restoreBountyDraft(id: string): Promise<BountyTask>;
  /** 永久删除已取消任务。仍有后续任务依赖时，以 BOUNTY_HAS_DEPENDENTS 拒绝。 */
  deleteBounty(id: string): Promise<void>;
  releaseLease(id: string): Promise<BountyTask>;
  getArbitrationConfig?(): Promise<ArbitrationConfig>;
  updateArbitrationConfig?(input: ArbitrationConfig): Promise<ArbitrationConfig>;
  approveArbitration?(id: string, input?: ApproveArbitrationInput): Promise<BountyTask>;
  rejectArbitration?(id: string, input: RejectArbitrationInput): Promise<BountyTask>;
  getOnboardingSettings(): Promise<OnboardingSettings>;
  updateOnboardingSettings(input: UpdateOnboardingInput): Promise<OnboardingSettings>;
  listInviteTokens(): Promise<InviteTokenSummary[]>;
  createInviteToken(input?: CreateInviteTokenInput): Promise<CreatedInviteToken>;
  revokeInviteToken(id: string): Promise<InviteTokenSummary>;
  deleteInviteToken(id: string): Promise<void>;
  getSkillMarkdown(options?: { baseUrl?: string; inviteToken?: string }): Promise<SkillDocument>;
  listAgents(filter?: AgentFilter): Promise<AgentSummary[]>;
  getAgentDetail(id: string): Promise<AgentDetail>;
  disableAgent(id: string): Promise<AgentSummary>;
  enableAgent(id: string): Promise<AgentSummary>;
  revokeAgent(id: string): Promise<AgentSummary>;
  agentCreateBounty?(apiKey: string, input: AgentCreateBountyInput): Promise<BountyTask>;
  registerUser?(input: RegisterUserInput): Promise<AuthSessionResult>;
  loginUser?(input: LoginUserInput): Promise<AuthSessionResult>;
  getAuthMe?(token: string): Promise<UserProfile>;
  updateAuthMe?(token: string, input: UpdateUserProfileInput): Promise<UserProfile>;
  logoutUser?(token: string): Promise<void>;
  listAuthAvatars?(): Promise<AuthAvatarPreset[]>;
}

export interface AgentApiPort {
  getPublicSkillMarkdown(options?: {
    inviteToken?: string;
    bearerInviteToken?: string;
  }): Promise<string>;
  registerAgent(
    input: RegisterAgentInput,
    options?: { bearerInviteToken?: string },
  ): Promise<RegisteredAgentResult>;
  getMe(): Promise<AgentSummary>;
  updateMe(input: UpdateAgentInput): Promise<AgentSummary>;
  updateSubscriptions(input: AgentSubscriptionsInput): Promise<AgentSummary>;
  listProjects(): Promise<BountyProject[]>;
  createBounty(input: AgentCreateBountyInput): Promise<BountyTask>;
  updateDraft(id: string, input: AgentUpdateDraftInput): Promise<BountyTask>;
  splitDraft(id: string, input: SplitDraftInput): Promise<SplitDraftResult>;
  listBounties(query?: BountyQuery): Promise<BountyTask[]>;
  getBountyDetail(id: string): Promise<BountyDetail>;
  listComments(id: string): Promise<BountyComment[]>;
  createComment(
    id: string,
    input: CreateBountyCommentInput,
  ): Promise<BountyCommentMutationResult>;
  claimBounty(id: string): Promise<BountyTask>;
  declineBounty(id: string, input?: DeclineBountyInput): Promise<BountyTask>;
  heartbeatBounty(id: string, input: LeaseHeartbeatInput): Promise<BountyTask>;
  requestInfo(id: string, input: RequestInfoInput): Promise<BountyTask>;
  abandonBounty(id: string, input: AbandonBountyInput): Promise<BountyTask>;
  completeBounty(id: string, input: CompleteBountyInput): Promise<BountyTask>;
  submitArbitration(id: string, input: SubmitArbitrationInput): Promise<BountyTask>;
  approveArbitration(id: string, input?: ApproveArbitrationInput): Promise<BountyTask>;
  rejectArbitration(id: string, input: RejectArbitrationInput): Promise<BountyTask>;
  getWebhookConfig(): Promise<AgentWebhookConfig>;
  updateWebhookConfig(input: UpdateAgentWebhookInput): Promise<AgentWebhookConfig>;
  listEvents(options?: { cursor?: number; limit?: number }): Promise<AgentInboxResponse>;
  ackEvents(eventId: number): Promise<AckAgentEventsResult>;
}

export interface MemoryBountyClient extends BountyApiPort {
  registerUser(input: RegisterUserInput): Promise<AuthSessionResult>;
  loginUser(input: LoginUserInput): Promise<AuthSessionResult>;
  getAuthMe(token: string): Promise<UserProfile>;
  updateAuthMe(token: string, input: UpdateUserProfileInput): Promise<UserProfile>;
  logoutUser(token: string): Promise<void>;
  listAuthAvatars(): Promise<AuthAvatarPreset[]>;
  inviteDraftCollaborator(
    id: string,
    input: InviteDraftCollaboratorInput,
  ): Promise<BountyTask>;
  publishBountiesBatch(input: PublishBountiesBatchInput): Promise<BountyTask[]>;
  listBountyComments(id: string): Promise<BountyComment[]>;
  createBountyComment(
    id: string,
    input: CreateBountyCommentInput,
  ): Promise<BountyCommentMutationResult>;
  toggleBountyCommentLike(id: string, commentId: number): Promise<BountyComment>;
  updateBountyComment(
    id: string,
    commentId: number,
    input: UpdateBountyCommentInput,
  ): Promise<BountyComment>;
  deleteBountyComment(id: string, commentId: number): Promise<DeleteBountyCommentResult>;
  getArbitrationConfig(): Promise<ArbitrationConfig>;
  updateArbitrationConfig(input: ArbitrationConfig): Promise<ArbitrationConfig>;
  approveArbitration(id: string, input?: ApproveArbitrationInput): Promise<BountyTask>;
  rejectArbitration(id: string, input: RejectArbitrationInput): Promise<BountyTask>;
  registerAgent(
    input: RegisterAgentInput,
    options?: { bearerInviteToken?: string },
  ): Promise<RegisteredAgentResult>;
  getPublicSkillMarkdown(options?: {
    inviteToken?: string;
    bearerInviteToken?: string;
    baseUrl?: string;
  }): Promise<string>;
  agentGetMe(apiKey: string): Promise<AgentSummary>;
  agentUpdateMe(apiKey: string, input: UpdateAgentInput): Promise<AgentSummary>;
  agentUpdateSubscriptions(
    apiKey: string,
    input: AgentSubscriptionsInput,
  ): Promise<AgentSummary>;
  agentListProjects(apiKey: string): Promise<BountyProject[]>;
  agentCreateBounty(apiKey: string, input: AgentCreateBountyInput): Promise<BountyTask>;
  agentUpdateDraft(
    apiKey: string,
    id: string,
    input: AgentUpdateDraftInput,
  ): Promise<BountyTask>;
  agentSplitDraft(
    apiKey: string,
    id: string,
    input: SplitDraftInput,
  ): Promise<SplitDraftResult>;
  agentListBounties(apiKey: string, query?: BountyQuery): Promise<BountyTask[]>;
  agentGetBountyDetail(apiKey: string, id: string): Promise<BountyDetail>;
  agentListComments(apiKey: string, id: string): Promise<BountyComment[]>;
  agentCreateComment(
    apiKey: string,
    id: string,
    input: CreateBountyCommentInput,
  ): Promise<BountyCommentMutationResult>;
  agentClaimBounty(apiKey: string, id: string): Promise<BountyTask>;
  agentDeclineBounty(
    apiKey: string,
    id: string,
    input?: DeclineBountyInput,
  ): Promise<BountyTask>;
  agentHeartbeatBounty(
    apiKey: string,
    id: string,
    input: LeaseHeartbeatInput,
  ): Promise<BountyTask>;
  agentRequestInfo(
    apiKey: string,
    id: string,
    input: RequestInfoInput,
  ): Promise<BountyTask>;
  agentAbandonBounty(
    apiKey: string,
    id: string,
    input: AbandonBountyInput,
  ): Promise<BountyTask>;
  agentCompleteBounty(
    apiKey: string,
    id: string,
    input: CompleteBountyInput,
  ): Promise<BountyTask>;
  agentSubmitArbitration(
    apiKey: string,
    id: string,
    input: SubmitArbitrationInput,
  ): Promise<BountyTask>;
  agentApproveArbitration(
    apiKey: string,
    id: string,
    input?: ApproveArbitrationInput,
  ): Promise<BountyTask>;
  agentRejectArbitration(
    apiKey: string,
    id: string,
    input: RejectArbitrationInput,
  ): Promise<BountyTask>;
  sweepLeases(nowMs?: number): Promise<void>;
  agentGetWebhookConfig(apiKey: string): Promise<AgentWebhookConfig>;
  agentUpdateWebhookConfig(
    apiKey: string,
    input: UpdateAgentWebhookInput,
  ): Promise<AgentWebhookConfig>;
  agentListEvents(
    apiKey: string,
    options?: { cursor?: number; limit?: number },
  ): Promise<AgentInboxResponse>;
  agentAckEvents(apiKey: string, eventId: number): Promise<AckAgentEventsResult>;
  agentOpenStream(
    apiKey: string,
    listener: (event: AgentEvent) => void,
    options?: { cursor?: number },
  ): Promise<() => void>;
  simulateAgentTier(
    agentId: string,
    tier: AgentTier,
    placementCompleted?: boolean,
  ): Promise<AgentSummary>;
  simulateAgentStream(agentId: string, connected: boolean): Promise<AgentSummary>;
  simulateAgentPush(
    agentId: string,
    options?: { pushedAt?: string | null; recentFailedCount?: number },
  ): Promise<AgentSummary>;
}

/** 校验 http/https 服务地址并去除末尾斜杠，避免路径拼接或 URL 序列化产生额外斜杠。 */
export function normalizeServerUrl(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) {
    throw new Error('Server URL must not be empty');
  }
  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    throw new Error('Server URL must be a valid http:// or https:// URL');
  }
  if ((parsed.protocol !== 'http:' && parsed.protocol !== 'https:') || !parsed.hostname) {
    throw new Error('Server URL must use http:// or https:// protocol');
  }
  return trimmed.replace(/\/+$/, '');
}

export async function fetchEmbeddedServerInfo(): Promise<EmbeddedServerInfo> {
  return await invoke<EmbeddedServerInfo>('get_embedded_server_info');
}

function buildBountyQueryString(query?: BountyQuery): string {
  const params = new URLSearchParams();
  if (query?.project_id !== undefined) {
    params.set('project_id', query.project_id);
  }
  if (query?.status !== undefined) {
    params.set('status', query.status);
  }
  if (query?.q !== undefined && query.q !== '') {
    params.set('q', query.q);
  }
  if (query?.include_drafts) {
    params.set('include_drafts', 'true');
  }
  return params.toString();
}

export class HttpBountyClient implements BountyApiPort {
  readonly baseUrl: string;
  private readonly adminToken: string;
  private readonly fetchImpl: typeof fetch;

  constructor(baseUrl: string, adminToken: string, fetchImpl?: typeof fetch) {
    this.baseUrl = normalizeServerUrl(baseUrl);
    this.adminToken = adminToken;
    this.fetchImpl = fetchImpl
      ? (input, init) => fetchImpl(input, init)
      : (input, init) => globalThis.fetch(input, init);
  }

  async health(): Promise<ServerHealth> {
    return await this.request<ServerHealth>('GET', '/api/v1/health', { auth: false });
  }

  async registerUser(input: RegisterUserInput): Promise<AuthSessionResult> {
    return await this.request<AuthSessionResult>('POST', '/api/v1/auth/register', {
      auth: false,
      body: {
        role: 'user',
        name: input.name,
        password: input.password,
        ...(input.avatar !== undefined ? { avatar: input.avatar } : {}),
      },
    });
  }

  async loginUser(input: LoginUserInput): Promise<AuthSessionResult> {
    return await this.request<AuthSessionResult>('POST', '/api/v1/auth/login', {
      auth: false,
      body: {
        role: 'user',
        name: input.name,
        password: input.password,
      },
    });
  }

  async getAuthMe(token: string): Promise<UserProfile> {
    return await this.request<UserProfile>('GET', '/api/v1/auth/me', {
      bearerToken: token,
    });
  }

  async updateAuthMe(token: string, input: UpdateUserProfileInput): Promise<UserProfile> {
    return await this.request<UserProfile>('PATCH', '/api/v1/auth/me', {
      bearerToken: token,
      body: input,
    });
  }

  async logoutUser(token: string): Promise<void> {
    await this.request<void>('POST', '/api/v1/auth/logout', {
      bearerToken: token,
    });
  }

  async listAuthAvatars(): Promise<AuthAvatarPreset[]> {
    return await this.request<AuthAvatarPreset[]>('GET', '/api/v1/auth/avatars', {
      auth: false,
    });
  }

  async adminStatus(): Promise<AdminStatus> {
    return await this.request<AdminStatus>('GET', '/api/v1/admin/status');
  }

  async listProjects(includeArchived?: boolean): Promise<BountyProject[]> {
    const params = new URLSearchParams();
    if (includeArchived !== undefined) {
      params.set('include_archived', includeArchived ? 'true' : 'false');
    }
    const qs = params.toString();
    return await this.request<BountyProject[]>('GET', qs ? `/api/v1/projects?${qs}` : '/api/v1/projects');
  }

  async createProject(input: CreateProjectInput): Promise<BountyProject> {
    return await this.request<BountyProject>('POST', '/api/v1/projects', { body: input });
  }

  async getProject(id: string): Promise<BountyProject> {
    return await this.request<BountyProject>('GET', `/api/v1/projects/${encodeURIComponent(id)}`);
  }

  async updateProject(id: string, input: UpdateProjectInput): Promise<BountyProject> {
    return await this.request<BountyProject>('PATCH', `/api/v1/projects/${encodeURIComponent(id)}`, {
      body: input,
    });
  }

  async deleteProject(id: string): Promise<void> {
    await this.request<void>('DELETE', `/api/v1/projects/${encodeURIComponent(id)}`);
  }

  async listBounties(query?: BountyQuery): Promise<BountyTask[]> {
    const qs = buildBountyQueryString(query);
    return await this.request<BountyTask[]>('GET', qs ? `/api/v1/bounties?${qs}` : '/api/v1/bounties');
  }

  async getBountyDetail(id: string): Promise<BountyDetail> {
    return await this.request<BountyDetail>('GET', `/api/v1/bounties/${encodeURIComponent(id)}`);
  }

  async listBountyComments(id: string): Promise<BountyComment[]> {
    return await this.request<BountyComment[]>(
      'GET',
      `/api/v1/bounties/${encodeURIComponent(id)}/comments`,
    );
  }

  async createBountyComment(
    id: string,
    input: CreateBountyCommentInput,
  ): Promise<BountyCommentMutationResult> {
    return await this.request<BountyCommentMutationResult>(
      'POST',
      `/api/v1/bounties/${encodeURIComponent(id)}/comments`,
      { body: input },
    );
  }

  async toggleBountyCommentLike(id: string, commentId: number): Promise<BountyComment> {
    return await this.request<BountyComment>(
      'POST',
      `/api/v1/bounties/${encodeURIComponent(id)}/comments/${encodeURIComponent(String(commentId))}/like`,
    );
  }

  async updateBountyComment(
    id: string,
    commentId: number,
    input: UpdateBountyCommentInput,
  ): Promise<BountyComment> {
    return await this.request<BountyComment>(
      'PATCH',
      `/api/v1/bounties/${encodeURIComponent(id)}/comments/${encodeURIComponent(String(commentId))}`,
      { body: input },
    );
  }

  async deleteBountyComment(id: string, commentId: number): Promise<DeleteBountyCommentResult> {
    return await this.request<DeleteBountyCommentResult>(
      'DELETE',
      `/api/v1/bounties/${encodeURIComponent(id)}/comments/${encodeURIComponent(String(commentId))}`,
    );
  }

  async createBounty(input: CreateBountyInput): Promise<BountyTask> {
    return await this.request<BountyTask>('POST', '/api/v1/bounties', { body: input });
  }

  async updateBounty(id: string, input: UpdateBountyInput): Promise<BountyTask> {
    return await this.request<BountyTask>('PATCH', `/api/v1/bounties/${encodeURIComponent(id)}`, {
      body: input,
    });
  }

  async publishBounty(id: string): Promise<BountyTask> {
    return await this.request<BountyTask>('POST', `/api/v1/bounties/${encodeURIComponent(id)}/publish`);
  }

  async inviteDraftCollaborator(
    id: string,
    input: InviteDraftCollaboratorInput,
  ): Promise<BountyTask> {
    return await this.request<BountyTask>(
      'POST',
      `/api/v1/bounties/${encodeURIComponent(id)}/collaborator`,
      { body: input },
    );
  }

  async publishBountiesBatch(input: PublishBountiesBatchInput): Promise<BountyTask[]> {
    return await this.request<BountyTask[]>('POST', '/api/v1/bounties/publish-batch', {
      body: input,
    });
  }

  async withdrawBounty(id: string): Promise<BountyTask> {
    return await this.request<BountyTask>('POST', `/api/v1/bounties/${encodeURIComponent(id)}/withdraw`);
  }

  async cancelBounty(id: string): Promise<BountyTask> {
    return await this.request<BountyTask>('POST', `/api/v1/bounties/${encodeURIComponent(id)}/cancel`);
  }

  async restoreBountyDraft(id: string): Promise<BountyTask> {
    return await this.request<BountyTask>('POST', `/api/v1/bounties/${encodeURIComponent(id)}/restore-draft`);
  }

  async deleteBounty(id: string): Promise<void> {
    await this.request<void>('DELETE', `/api/v1/bounties/${encodeURIComponent(id)}`);
  }

  async releaseLease(id: string): Promise<BountyTask> {
    return await this.request<BountyTask>(
      'POST',
      `/api/v1/bounties/${encodeURIComponent(id)}/release-lease`,
    );
  }

  async getArbitrationConfig(): Promise<ArbitrationConfig> {
    return await this.request<ArbitrationConfig>('GET', '/api/v1/admin/arbitration');
  }

  async updateArbitrationConfig(input: ArbitrationConfig): Promise<ArbitrationConfig> {
    return await this.request<ArbitrationConfig>('PATCH', '/api/v1/admin/arbitration', {
      body: input,
    });
  }

  async approveArbitration(id: string, input?: ApproveArbitrationInput): Promise<BountyTask> {
    return await this.request<BountyTask>(
      'POST',
      `/api/v1/bounties/${encodeURIComponent(id)}/arbitration/approve`,
      input !== undefined ? { body: input } : undefined,
    );
  }

  async rejectArbitration(id: string, input: RejectArbitrationInput): Promise<BountyTask> {
    return await this.request<BountyTask>(
      'POST',
      `/api/v1/bounties/${encodeURIComponent(id)}/arbitration/reject`,
      { body: input },
    );
  }

  async getOnboardingSettings(): Promise<OnboardingSettings> {
    return await this.request<OnboardingSettings>('GET', '/api/v1/admin/onboarding');
  }

  async updateOnboardingSettings(input: UpdateOnboardingInput): Promise<OnboardingSettings> {
    return await this.request<OnboardingSettings>('PATCH', '/api/v1/admin/onboarding', {
      body: input,
    });
  }

  async listInviteTokens(): Promise<InviteTokenSummary[]> {
    return await this.request<InviteTokenSummary[]>('GET', '/api/v1/admin/invite-tokens');
  }

  async createInviteToken(input?: CreateInviteTokenInput): Promise<CreatedInviteToken> {
    return await this.request<CreatedInviteToken>('POST', '/api/v1/admin/invite-tokens', {
      body: input ?? {},
    });
  }

  async revokeInviteToken(id: string): Promise<InviteTokenSummary> {
    return await this.request<InviteTokenSummary>(
      'POST',
      `/api/v1/admin/invite-tokens/${encodeURIComponent(id)}/revoke`,
    );
  }

  async deleteInviteToken(id: string): Promise<void> {
    await this.request<void>(
      'DELETE',
      `/api/v1/admin/invite-tokens/${encodeURIComponent(id)}`,
    );
  }

  async getSkillMarkdown(options?: {
    baseUrl?: string;
    inviteToken?: string;
  }): Promise<SkillDocument> {
    const params = new URLSearchParams();
    if (options?.baseUrl !== undefined && options.baseUrl !== '') {
      params.set('base_url', options.baseUrl);
    }
    if (options?.inviteToken !== undefined && options.inviteToken !== '') {
      params.set('invite_token', options.inviteToken);
    }
    const qs = params.toString();
    return await this.request<SkillDocument>(
      'GET',
      qs ? `/api/v1/admin/skill.md?${qs}` : '/api/v1/admin/skill.md',
    );
  }

  async listAgents(filter?: AgentFilter): Promise<AgentSummary[]> {
    const params = new URLSearchParams();
    if (filter?.status !== undefined) {
      params.set('status', filter.status);
    }
    if (filter?.activity !== undefined) {
      params.set('activity', filter.activity);
    }
    const qs = params.toString();
    return await this.request<AgentSummary[]>('GET', qs ? `/api/v1/agents?${qs}` : '/api/v1/agents');
  }

  async getAgentDetail(id: string): Promise<AgentDetail> {
    return await this.request<AgentDetail>('GET', `/api/v1/agents/${encodeURIComponent(id)}`);
  }

  async disableAgent(id: string): Promise<AgentSummary> {
    return await this.request<AgentSummary>(
      'POST',
      `/api/v1/agents/${encodeURIComponent(id)}/disable`,
    );
  }

  async enableAgent(id: string): Promise<AgentSummary> {
    return await this.request<AgentSummary>(
      'POST',
      `/api/v1/agents/${encodeURIComponent(id)}/enable`,
    );
  }

  async revokeAgent(id: string): Promise<AgentSummary> {
    return await this.request<AgentSummary>(
      'POST',
      `/api/v1/agents/${encodeURIComponent(id)}/revoke`,
    );
  }

  private async request<T>(
    method: string,
    path: string,
    options?: { body?: unknown; auth?: boolean; bearerToken?: string },
  ): Promise<T> {
    const headers: Record<string, string> = {};
    if (options?.bearerToken !== undefined) {
      headers.Authorization = `Bearer ${options.bearerToken}`;
    } else if (options?.auth !== false) {
      headers.Authorization = `Bearer ${this.adminToken}`;
    }
    if (options?.body !== undefined) {
      headers['Content-Type'] = 'application/json';
    }

    const response = await this.fetchImpl(`${this.baseUrl}${path}`, {
      method,
      headers,
      ...(options?.body !== undefined ? { body: JSON.stringify(options.body) } : {}),
    });

    if (!response.ok) {
      await throwResponseError(response);
    }

    if (response.status === 204) {
      return undefined as T;
    }

    return (await response.json()) as T;
  }
}

export class HttpAgentClient implements AgentApiPort {
  readonly baseUrl: string;
  private readonly apiKey: string;
  private readonly fetchImpl: typeof fetch;

  constructor(baseUrl: string, apiKey = '', fetchImpl?: typeof fetch) {
    this.baseUrl = normalizeServerUrl(baseUrl);
    this.apiKey = apiKey;
    this.fetchImpl = fetchImpl
      ? (input, init) => fetchImpl(input, init)
      : (input, init) => globalThis.fetch(input, init);
  }

  async getPublicSkillMarkdown(options?: {
    inviteToken?: string;
    bearerInviteToken?: string;
  }): Promise<string> {
    const params = new URLSearchParams();
    if (options?.inviteToken !== undefined && options.inviteToken !== '') {
      params.set('invite_token', options.inviteToken);
    }
    const qs = params.toString();
    const headers: Record<string, string> = {};
    if (options?.bearerInviteToken !== undefined && options.bearerInviteToken !== '') {
      headers.Authorization = `Bearer ${options.bearerInviteToken}`;
    }
    const response = await this.fetchImpl(`${this.baseUrl}${qs ? `/skill.md?${qs}` : '/skill.md'}`, {
      method: 'GET',
      headers,
    });
    if (!response.ok) {
      await throwResponseError(response);
    }
    return await response.text();
  }

  async registerAgent(
    input: RegisterAgentInput,
    options?: { bearerInviteToken?: string },
  ): Promise<RegisteredAgentResult> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    };
    if (options?.bearerInviteToken !== undefined && options.bearerInviteToken !== '') {
      headers.Authorization = `Bearer ${options.bearerInviteToken}`;
    }
    const response = await this.fetchImpl(`${this.baseUrl}/api/v1/agents/register`, {
      method: 'POST',
      headers,
      body: JSON.stringify(input),
    });
    if (!response.ok) {
      await throwResponseError(response);
    }
    return (await response.json()) as RegisteredAgentResult;
  }

  async getMe(): Promise<AgentSummary> {
    return await this.request<AgentSummary>('GET', '/api/v1/agent/me');
  }

  async updateMe(input: UpdateAgentInput): Promise<AgentSummary> {
    return await this.request<AgentSummary>('PATCH', '/api/v1/agent/me', { body: input });
  }

  async updateSubscriptions(input: AgentSubscriptionsInput): Promise<AgentSummary> {
    return await this.request<AgentSummary>('PUT', '/api/v1/agent/me/subscriptions', {
      body: input,
    });
  }

  async listProjects(): Promise<BountyProject[]> {
    return await this.request<BountyProject[]>('GET', '/api/v1/agent/projects');
  }

  async createBounty(input: AgentCreateBountyInput): Promise<BountyTask> {
    return await this.request<BountyTask>('POST', '/api/v1/agent/bounties', { body: input });
  }

  async updateDraft(id: string, input: AgentUpdateDraftInput): Promise<BountyTask> {
    return await this.request<BountyTask>(
      'PATCH',
      `/api/v1/agent/bounties/${encodeURIComponent(id)}`,
      { body: input },
    );
  }

  async splitDraft(id: string, input: SplitDraftInput): Promise<SplitDraftResult> {
    return await this.request<SplitDraftResult>(
      'POST',
      `/api/v1/agent/bounties/${encodeURIComponent(id)}/split`,
      { body: input },
    );
  }

  async listBounties(query?: BountyQuery): Promise<BountyTask[]> {
    const qs = buildBountyQueryString(query);
    return await this.request<BountyTask[]>(
      'GET',
      qs ? `/api/v1/agent/bounties?${qs}` : '/api/v1/agent/bounties',
    );
  }

  async getBountyDetail(id: string): Promise<BountyDetail> {
    return await this.request<BountyDetail>(
      'GET',
      `/api/v1/agent/bounties/${encodeURIComponent(id)}`,
    );
  }

  async listComments(id: string): Promise<BountyComment[]> {
    return await this.request<BountyComment[]>(
      'GET',
      `/api/v1/agent/bounties/${encodeURIComponent(id)}/comments`,
    );
  }

  async createComment(
    id: string,
    input: CreateBountyCommentInput,
  ): Promise<BountyCommentMutationResult> {
    return await this.request<BountyCommentMutationResult>(
      'POST',
      `/api/v1/agent/bounties/${encodeURIComponent(id)}/comments`,
      { body: input },
    );
  }

  async claimBounty(id: string): Promise<BountyTask> {
    return await this.request<BountyTask>(
      'POST',
      `/api/v1/agent/bounties/${encodeURIComponent(id)}/claim`,
    );
  }

  async declineBounty(id: string, input?: DeclineBountyInput): Promise<BountyTask> {
    return await this.request<BountyTask>(
      'POST',
      `/api/v1/agent/bounties/${encodeURIComponent(id)}/decline`,
      input !== undefined ? { body: input } : undefined,
    );
  }

  async heartbeatBounty(id: string, input: LeaseHeartbeatInput): Promise<BountyTask> {
    return await this.request<BountyTask>(
      'POST',
      `/api/v1/agent/bounties/${encodeURIComponent(id)}/heartbeat`,
      { body: input },
    );
  }

  async requestInfo(id: string, input: RequestInfoInput): Promise<BountyTask> {
    if (typeof input?.lease_id === 'number' && !Number.isFinite(input.lease_id)) {
      throw new ServiceApiError('VALIDATION_ERROR', 400, 'lease_id 必须为有限整数');
    }
    return await this.request<BountyTask>(
      'POST',
      `/api/v1/agent/bounties/${encodeURIComponent(id)}/request-info`,
      { body: input },
    );
  }

  async abandonBounty(id: string, input: AbandonBountyInput): Promise<BountyTask> {
    return await this.request<BountyTask>(
      'POST',
      `/api/v1/agent/bounties/${encodeURIComponent(id)}/abandon`,
      { body: input },
    );
  }

  async completeBounty(id: string, input: CompleteBountyInput): Promise<BountyTask> {
    if (
      input.completion_cost !== undefined &&
      input.completion_cost !== null &&
      (typeof input.completion_cost !== 'number' ||
        !Number.isFinite(input.completion_cost) ||
        input.completion_cost < 0)
    ) {
      throw new ServiceApiError('VALIDATION_ERROR', 400, 'completion_cost 必须为有限非负数');
    }
    return await this.request<BountyTask>(
      'POST',
      `/api/v1/agent/bounties/${encodeURIComponent(id)}/complete`,
      { body: input },
    );
  }

  async submitArbitration(id: string, input: SubmitArbitrationInput): Promise<BountyTask> {
    if (
      input.completion_cost !== undefined &&
      input.completion_cost !== null &&
      (typeof input.completion_cost !== 'number' ||
        !Number.isFinite(input.completion_cost) ||
        input.completion_cost < 0)
    ) {
      throw new ServiceApiError('VALIDATION_ERROR', 400, 'completion_cost 必须为有限非负数');
    }
    return await this.request<BountyTask>(
      'POST',
      `/api/v1/agent/bounties/${encodeURIComponent(id)}/submit-arbitration`,
      { body: input },
    );
  }

  async approveArbitration(id: string, input?: ApproveArbitrationInput): Promise<BountyTask> {
    return await this.request<BountyTask>(
      'POST',
      `/api/v1/agent/bounties/${encodeURIComponent(id)}/arbitration/approve`,
      { body: input ?? {} },
    );
  }

  async rejectArbitration(id: string, input: RejectArbitrationInput): Promise<BountyTask> {
    return await this.request<BountyTask>(
      'POST',
      `/api/v1/agent/bounties/${encodeURIComponent(id)}/arbitration/reject`,
      { body: input },
    );
  }

  async getWebhookConfig(): Promise<AgentWebhookConfig> {
    return await this.request<AgentWebhookConfig>('GET', '/api/v1/agent/me/webhook');
  }

  async updateWebhookConfig(input: UpdateAgentWebhookInput): Promise<AgentWebhookConfig> {
    return await this.request<AgentWebhookConfig>('PUT', '/api/v1/agent/me/webhook', {
      body: input,
    });
  }

  async listEvents(options?: { cursor?: number; limit?: number }): Promise<AgentInboxResponse> {
    const params = new URLSearchParams();
    if (options?.cursor !== undefined) {
      params.set('cursor', String(options.cursor));
    }
    if (options?.limit !== undefined) {
      params.set('limit', String(options.limit));
    }
    const qs = params.toString();
    return await this.request<AgentInboxResponse>(
      'GET',
      qs ? `/api/v1/agent/events?${qs}` : '/api/v1/agent/events',
    );
  }

  async ackEvents(eventId: number): Promise<AckAgentEventsResult> {
    return await this.request<AckAgentEventsResult>('POST', '/api/v1/agent/events/ack', {
      body: { event_id: eventId },
    });
  }

  private async request<T>(method: string, path: string, options?: { body?: unknown }): Promise<T> {
    const headers: Record<string, string> = {
      Authorization: `Bearer ${this.apiKey}`,
    };
    if (options?.body !== undefined) {
      headers['Content-Type'] = 'application/json';
    }
    const response = await this.fetchImpl(`${this.baseUrl}${path}`, {
      method,
      headers,
      ...(options?.body !== undefined ? { body: JSON.stringify(options.body) } : {}),
    });
    if (!response.ok) {
      await throwResponseError(response);
    }
    return (await response.json()) as T;
  }
}

export async function registerAgent(
  baseUrl: string,
  input: RegisterAgentInput,
  options?: { bearerInviteToken?: string; fetchImpl?: typeof fetch },
): Promise<RegisteredAgentResult> {
  return await new HttpAgentClient(baseUrl, '', options?.fetchImpl).registerAgent(input, {
    bearerInviteToken: options?.bearerInviteToken,
  });
}

async function throwResponseError(response: Response): Promise<never> {
  const rawText = await response.text();
  if (rawText) {
    try {
      const parsed = JSON.parse(rawText) as {
        error?: { code?: unknown; message?: unknown };
      };
      if (
        parsed &&
        typeof parsed === 'object' &&
        parsed.error &&
        typeof parsed.error === 'object' &&
        typeof parsed.error.code === 'string' &&
        typeof parsed.error.message === 'string'
      ) {
        throw new ServiceApiError(parsed.error.code, response.status, parsed.error.message);
      }
    } catch (err) {
      if (err instanceof ServiceApiError) {
        throw err;
      }
    }
  }
  const fallbackCode = response.status === 401 ? 'UNAUTHORIZED' : 'INTERNAL_ERROR';
  const fallbackMessage = rawText || response.statusText || `HTTP ${response.status}`;
  throw new ServiceApiError(fallbackCode, response.status, fallbackMessage);
}


function asciiLower(text: string): string {
  return text.replace(/[A-Z]/g, ch => ch.toLowerCase());
}

function clone<T>(value: T): T {
  try {
    return structuredClone(value);
  } catch {
    return JSON.parse(JSON.stringify(value)) as T;
  }
}

function randomHex64(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
}

function credentialPrefix(raw: string): string {
  return Array.from(raw).slice(0, 15).join('');
}

function validateSkillBaseUrl(raw: string): string {
  const trimmed = raw.trim();
  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    throw new ServiceApiError('VALIDATION_ERROR', 400, 'base_url 必须为 HTTP(S) 服务根地址');
  }
  if (
    (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') ||
    !parsed.hostname ||
    parsed.username !== '' ||
    parsed.password !== '' ||
    parsed.search !== '' ||
    parsed.hash !== '' ||
    parsed.pathname !== '/'
  ) {
    throw new ServiceApiError('VALIDATION_ERROR', 400, 'base_url 必须为 HTTP(S) 服务根地址');
  }
  return parsed.toString().replace(/\/+$/, '');
}

function resolveSubscriptionInput(
  subscribeAllProjects?: boolean,
  projectIds?: string[],
): { subscribeAll: boolean; projectIds: string[] } {
  if (subscribeAllProjects === true && Array.isArray(projectIds) && projectIds.length > 0) {
    throw new ServiceApiError('VALIDATION_ERROR', 400, '全局订阅不能同时指定 project_ids');
  }
  const subscribeAll =
    subscribeAllProjects === true || (projectIds === undefined && subscribeAllProjects !== false);
  const deduped = Array.from(new Set(projectIds ?? [])).sort();
  return { subscribeAll, projectIds: deduped };
}

function renderSkillMarkdown(
  baseUrl: string,
  lanOpenRegistration: boolean,
): string {
  const policy = lanOpenRegistration
    ? '已开启局域网免审注册；可省略 invite_token。显式提供邀请时仍须有效。'
    : '需要有效邀请令牌；请向管理员获取 invite_token。';
  // 最后填入服务地址，避免将地址中的占位符当成模板内容。
  return skillTemplate
    .replaceAll('{{registration_policy}}', () => policy)
    .replaceAll('{{cli}}', () => 'taskrail-agent')
    .replaceAll('{{base_url}}', () => baseUrl);
}

function validateProjectFields(name: string, icon: string | null, policy: AgentDraftPolicy): void {
  if (typeof name !== 'string' || !name.trim()) {
    throw new ServiceApiError('VALIDATION_ERROR', 400, '项目名称不能为空');
  }
  if (icon !== null && !parseProjectIcon(icon)) {
    throw new ServiceApiError('VALIDATION_ERROR', 400, 'icon 必须为单字素 emoji: 或合法 lucide: 名称');
  }
  if (!VALID_AGENT_POLICIES.has(policy)) {
    throw new ServiceApiError('VALIDATION_ERROR', 400, 'agent_draft_policy 必须为 open 或 draft');
  }
}

function validateBountyFields(
  title: string,
  difficulty: BountyDifficulty,
  mode: AssigneeMode,
  minTier: BountyDifficulty | null,
  agentId: string | null,
): void {
  if (typeof title !== 'string' || !title.trim()) {
    throw new ServiceApiError('VALIDATION_ERROR', 400, '任务标题不能为空');
  }
  if (!VALID_DIFFICULTIES.has(difficulty)) {
    throw new ServiceApiError('VALIDATION_ERROR', 400, 'difficulty 必须为 C、B、A 或 S');
  }
  if (!VALID_ASSIGNEE_MODES.has(mode)) {
    throw new ServiceApiError('VALIDATION_ERROR', 400, 'assignee_mode 不合法');
  }
  if (minTier !== null && !VALID_DIFFICULTIES.has(minTier)) {
    throw new ServiceApiError('VALIDATION_ERROR', 400, 'assignee_min_tier 必须为 C、B、A 或 S');
  }

  const validCombination =
    mode === 'any'
      ? minTier === null && agentId === null
      : mode === 'min_tier'
        ? minTier !== null && agentId === null
        : minTier === null && typeof agentId === 'string' && agentId.trim().length > 0;

  if (!validCombination) {
    throw new ServiceApiError('VALIDATION_ERROR', 400, '执行者限制字段组合不一致');
  }
}

interface StoredInviteToken {
  summary: InviteTokenSummary;
  rawToken: string;
}

interface StoredAgentEvent {
  event: AgentEvent;
  lastError: string | null;
}

interface StoredAgent {
  summary: AgentSummary;
  apiKey: string;
  webhookSecret: string | null;
  ackedEventId: number;
  inbox: StoredAgentEvent[];
  streamListeners: Map<number, (event: AgentEvent) => void>;
  simulatedStreamConnected: boolean;
  simulatedRecentFailedCount: number;
  mmr: number;
  settledCount: number;
  completedCount: number;
  failedCount: number;
  verifiedAsWins: number;
  verifiedSWins: number;
  ledger: AgentRatingLedgerEntry[];
}

function tierRank(tier: AgentTier | BountyDifficulty): number {
  switch (tier) {
    case 'S':
      return 4;
    case 'A':
      return 3;
    case 'B':
      return 2;
    case 'C':
    case 'Unranked':
      return 1;
  }
}

function ratingBenchmark(difficulty: BountyDifficulty): [number, number, number, number | null] {
  switch (difficulty) {
    case 'C':
      return [1000.0, 600_000.0, 1.0, 1199.0];
    case 'B':
      return [1350.0, 1_200_000.0, 3.0, 1499.0];
    case 'A':
      return [1650.0, 1_800_000.0, 8.0, 1799.0];
    case 'S':
      return [1950.0, 2_700_000.0, 15.0, null];
  }
}

function assignedTier(
  mmr: number,
  settled: number,
  asWins: number,
  sWins: number,
): AgentTier {
  if (settled < 5) {
    return 'Unranked';
  }
  if (mmr >= 1800.0 && sWins >= 5 && asWins >= 3) {
    return 'S';
  }
  if (mmr >= 1500.0 && asWins >= 3) {
    return 'A';
  }
  if (mmr >= 1200.0) {
    return 'B';
  }
  return 'C';
}

function resolveWebhookValues(
  url: string | null | undefined,
  secret: string | null | undefined,
  oldSecret: string | null,
): { webhookUrl: string | null; webhookSecret: string | null } {
  if (secret !== undefined && secret !== null) {
    if (typeof secret !== 'string' || secret.trim().length === 0) {
      throw new ServiceApiError('VALIDATION_ERROR', 400, 'webhook_secret 不能为空白');
    }
  }
  if (url === undefined || url === null) {
    return { webhookUrl: null, webhookSecret: null };
  }
  if (typeof url !== 'string' || url.length === 0 || /\s/.test(url)) {
    throw new ServiceApiError('VALIDATION_ERROR', 400, 'webhook_url 必须为 HTTP(S) URL');
  }
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    throw new ServiceApiError('VALIDATION_ERROR', 400, 'webhook_url 必须为 HTTP(S) URL');
  }
  if ((parsed.protocol !== 'http:' && parsed.protocol !== 'https:') || !parsed.hostname) {
    throw new ServiceApiError('VALIDATION_ERROR', 400, 'webhook_url 必须为 HTTP(S) URL');
  }
  const resolvedSecret = (secret ?? oldSecret) ?? `tr_whsec_${randomHex64()}`;
  return { webhookUrl: url, webhookSecret: resolvedSecret };
}

export function createMemoryBountyClient(seed?: {
  projects?: BountyProject[];
  bounties?: BountyTask[];
  lan_open_registration?: boolean;
  arbitrator_chain?: string[];
}): MemoryBountyClient {
  function normalizeArbitratorChain(raw: unknown): string[] {
    if (!Array.isArray(raw) || raw.some(item => typeof item !== 'string')) {
      throw new ServiceApiError('VALIDATION_ERROR', 400, 'arbitrator_chain 必须为字符串数组');
    }
    const result: string[] = [];
    for (const id of raw) {
      if (!id.trim()) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'arbitrator_chain 不能包含空白项');
      }
      if (!result.includes(id)) {
        result.push(id);
      }
    }
    return result;
  }

  const projects = new Map<string, BountyProject>();
  const bounties = new Map<string, BountyTask>();
  const dependenciesByBounty = new Map<string, string[]>();
  const eventsByBounty = new Map<string, BountyEvent[]>();
  const commentsByBounty = new Map<string, BountyComment[]>();
  const inviteTokensById = new Map<string, StoredInviteToken>();
  const inviteTokensByRaw = new Map<string, StoredInviteToken>();
  const agentsById = new Map<string, StoredAgent>();
  const agentsByApiKey = new Map<string, StoredAgent>();
  interface StoredUserAccount {
    id: string;
    role: 'user';
    name: string;
    password: string;
    avatar: UserAvatar | null;
  }
  const userAccountsById = new Map<string, StoredUserAccount>();
  const userSessionsByToken = new Map<string, string>();
  const bountyDispatches = new Map<string, Set<string>>();
  const leaseWarnedIdByBounty = new Map<string, number>();
  const arbitrationTriedIdsByBounty = new Map<string, string[]>();
  let lanOpenRegistration = seed?.lan_open_registration ?? false;
  let globalArbitratorChain: string[] =
    seed?.arbitrator_chain !== undefined ? normalizeArbitratorChain(seed.arbitrator_chain) : [];
  let nextEventId = 1;
  let nextCommentId = 1;
  let nextAgentEventId = 1;
  let nextStreamConnectionId = 1;
  let nextLedgerId = 1;
  let lastTimestampMs = 0;

  function toUserProfile(account: StoredUserAccount): UserProfile {
    return clone({
      id: account.id,
      role: account.role,
      name: account.name,
      avatar: account.avatar,
    });
  }

  function validateAccountName(raw: unknown): string {
    if (typeof raw !== 'string') {
      throw new ServiceApiError('VALIDATION_ERROR', 400, '名称须为 1 至 64 个字符');
    }
    const err = validateUserNameError(raw);
    if (err) {
      throw new ServiceApiError('VALIDATION_ERROR', 400, err);
    }
    return raw.trim();
  }

  function validateAccountPassword(raw: unknown): string {
    if (typeof raw !== 'string') {
      throw new ServiceApiError('VALIDATION_ERROR', 400, '密码须为 12 至 128 个字符');
    }
    const err = validateUserPasswordError(raw);
    if (err) {
      throw new ServiceApiError('VALIDATION_ERROR', 400, err);
    }
    return raw;
  }

  function validateAccountAvatar(raw: unknown): UserAvatar | null {
    if (raw === null) {
      return null;
    }
    if (!raw || typeof raw !== 'object') {
      throw new ServiceApiError('VALIDATION_ERROR', 400, '头像格式无效');
    }
    const err = validateUserAvatar(raw as UserAvatar);
    if (err) {
      throw new ServiceApiError('VALIDATION_ERROR', 400, err);
    }
    return clone(raw as UserAvatar);
  }

  function ensureAccountNameAvailable(name: string, ownId: string | null): void {
    for (const account of userAccountsById.values()) {
      if (account.id !== ownId && account.name === name) {
        throw new ServiceApiError('NAME_TAKEN', 409, '名称已被使用');
      }
    }
    for (const agent of agentsById.values()) {
      if (agent.summary.id !== ownId && agent.summary.name === name) {
        throw new ServiceApiError('NAME_TAKEN', 409, '名称已被使用');
      }
    }
  }

  function authenticateUserSession(token: string): StoredUserAccount {
    if (typeof token !== 'string' || !token) {
      throw new ServiceApiError('UNAUTHORIZED', 401, '请先注册或登录');
    }
    const accountId = userSessionsByToken.get(token);
    const account = accountId ? userAccountsById.get(accountId) : undefined;
    if (!account) {
      throw new ServiceApiError('UNAUTHORIZED', 401, '请先注册或登录');
    }
    return account;
  }

  function withTransaction<T>(fn: () => T): T {
    const snapBounties = clone(bounties);
    const snapDeps = clone(dependenciesByBounty);
    const snapEvents = clone(eventsByBounty);
    const snapComments = clone(commentsByBounty);
    const snapDispatches = clone(bountyDispatches);
    const snapWarned = clone(leaseWarnedIdByBounty);
    const snapArbitrationTried = clone(arbitrationTriedIdsByBounty);
    const snapAgentsInbox = new Map<
      string,
      { inbox: StoredAgentEvent[]; last_pushed_at: string | null }
    >();
    for (const [id, agent] of agentsById.entries()) {
      snapAgentsInbox.set(id, {
        inbox: clone(agent.inbox),
        last_pushed_at: agent.summary.last_pushed_at,
      });
    }
    const snapNextEventId = nextEventId;
    const snapNextCommentId = nextCommentId;
    const snapNextAgentEventId = nextAgentEventId;

    const restoreMap = <K, V>(target: Map<K, V>, snapshot: Map<K, V>) => {
      target.clear();
      for (const [k, v] of snapshot.entries()) {
        target.set(k, v);
      }
    };

    try {
      return fn();
    } catch (error) {
      restoreMap(bounties, snapBounties);
      restoreMap(dependenciesByBounty, snapDeps);
      restoreMap(eventsByBounty, snapEvents);
      restoreMap(commentsByBounty, snapComments);
      restoreMap(bountyDispatches, snapDispatches);
      restoreMap(leaseWarnedIdByBounty, snapWarned);
      restoreMap(arbitrationTriedIdsByBounty, snapArbitrationTried);
      for (const [id, saved] of snapAgentsInbox.entries()) {
        const agent = agentsById.get(id);
        if (agent) {
          agent.inbox = saved.inbox;
          agent.summary.last_pushed_at = saved.last_pushed_at;
        }
      }
      nextEventId = snapNextEventId;
      nextCommentId = snapNextCommentId;
      nextAgentEventId = snapNextAgentEventId;
      throw error;
    }
  }

  function calculateRating(
    bounty: BountyTask,
    agent: StoredAgent,
    outcome: RatingOutcome,
    arbitrated: boolean,
    timestamp: string,
  ): AgentRatingLedgerEntry {
    const completed = outcome === 'completed';
    const effectiveDifficulty: BountyDifficulty = arbitrated
      ? (bounty.calibrated_difficulty ?? bounty.difficulty)
      : bounty.difficulty;
    const [target, duration, cost, diffCap] = ratingBenchmark(effectiveDifficulty);
    const isPlacement = agent.settledCount < 5;
    const kFactor = isPlacement ? 80.0 : 32.0;
    const expectedScore = 1.0 / (1.0 + Math.pow(10.0, (target - agent.mmr) / 400.0));
    let qualityScore = 0.0;
    let timeEfficiency = 0.0;
    let costEfficiency = 0.0;
    if (completed) {
      const qBase =
        arbitrated && bounty.quality_grade === 'perfect'
          ? 1.0
          : arbitrated && bounty.quality_grade === 'flawed'
            ? 0.85
            : 0.9;
      qualityScore = qBase * Math.pow(0.85, Math.max(0, bounty.rejection_count));
      timeEfficiency = 1.0 / (1.0 + Math.max(0, bounty.active_duration_ms) / duration);
      costEfficiency =
        bounty.completion_cost === null || bounty.completion_cost === undefined
          ? 0.5
          : 1.0 / (1.0 + Math.max(0.0, bounty.completion_cost) / cost);
    }
    const actualScore = qualityScore * (0.75 + 0.15 * timeEfficiency + 0.10 * costEfficiency);
    const rawDelta = kFactor * (actualScore - expectedScore);
    const selfDealt = bounty.created_by_agent_id === agent.summary.id;
    let mmrAfter: number;
    let capReason: string | null = null;
    if (rawDelta <= 0.0) {
      mmrAfter = Math.max(0.0, agent.mmr + rawDelta);
    } else if (selfDealt) {
      mmrAfter = agent.mmr;
      capReason = 'self_deal';
    } else {
      let ceiling: [number, string] | null = null;
      if (diffCap !== null && !arbitrated && diffCap <= 1350.0) {
        ceiling = [diffCap, 'difficulty_cap'];
      } else if (!arbitrated) {
        ceiling = [1350.0, 'unarbitrated_cap'];
      } else if (diffCap !== null) {
        ceiling = [diffCap, 'difficulty_cap'];
      }
      if (ceiling !== null && agent.mmr >= ceiling[0]) {
        mmrAfter = agent.mmr;
        capReason = ceiling[1];
      } else if (ceiling !== null && agent.mmr + rawDelta > ceiling[0]) {
        mmrAfter = ceiling[0];
        capReason = ceiling[1];
      } else {
        mmrAfter = agent.mmr + rawDelta;
      }
    }
    const verified = completed && arbitrated;
    const asWins =
      agent.verifiedAsWins +
      (verified && (effectiveDifficulty === 'A' || effectiveDifficulty === 'S') ? 1 : 0);
    const sWins = agent.verifiedSWins + (verified && effectiveDifficulty === 'S' ? 1 : 0);
    const settled = agent.settledCount + 1;
    const tierAfter = assignedTier(mmrAfter, settled, asWins, sWins);
    let tierGateReason: string | null = null;
    if (settled >= 5 && mmrAfter >= 1500.0 && asWins < 3) {
      tierGateReason = 'needs_as_wins';
    } else if (settled >= 5 && mmrAfter >= 1800.0 && sWins < 5) {
      tierGateReason = 'needs_s_wins';
    }
    const appliedDelta = rawDelta > 0.0 && capReason === null ? rawDelta : mmrAfter - agent.mmr;
    return {
      id: nextLedgerId++,
      agent_id: agent.summary.id,
      bounty_id: bounty.id,
      bounty_title: bounty.title,
      project_id: bounty.project_id,
      lease_id: bounty.lease_id,
      outcome,
      arbitrated,
      self_dealt: selfDealt,
      original_difficulty: bounty.difficulty,
      effective_difficulty: effectiveDifficulty,
      quality_grade: arbitrated ? (bounty.quality_grade ?? null) : null,
      rejection_count: bounty.rejection_count,
      quality_score: qualityScore,
      active_duration_ms: bounty.active_duration_ms,
      time_efficiency: timeEfficiency,
      completion_cost: bounty.completion_cost,
      cost_efficiency: costEfficiency,
      actual_score: actualScore,
      expected_score: expectedScore,
      k_factor: kFactor,
      is_placement: isPlacement,
      raw_delta: rawDelta,
      applied_delta: appliedDelta,
      mmr_before: agent.mmr,
      mmr_after: mmrAfter,
      tier_before: agent.summary.tier,
      tier_after: tierAfter,
      cap_reason: capReason,
      tier_gate_reason: tierGateReason,
      created_at: timestamp,
    };
  }

  // 停用与吊销不免除已持有租约的结算责任，结算只要求身份记录存在。
  function requireRatingAgent(bounty: BountyTask): StoredAgent {
    const agent = bounty.lease_agent_id ? agentsById.get(bounty.lease_agent_id) : undefined;
    if (!agent) throw new ServiceApiError('INTERNAL_ERROR', 500, '执行租约缺少有效的 Agent 身份');
    return agent;
  }

  function settleRating(
    bounty: BountyTask,
    agent: StoredAgent,
    outcome: RatingOutcome,
    arbitrated: boolean,
    timestamp: string,
  ): void {
    const entry = calculateRating(bounty, agent, outcome, arbitrated, timestamp);
    const completed = outcome === 'completed';
    const verified = completed && arbitrated;
    agent.mmr = entry.mmr_after;
    agent.summary.tier = entry.tier_after;
    agent.settledCount += 1;
    agent.summary.placement_completed = agent.settledCount >= 5;
    agent.completedCount += completed ? 1 : 0;
    agent.failedCount += !completed ? 1 : 0;
    agent.verifiedAsWins +=
      verified && (entry.effective_difficulty === 'A' || entry.effective_difficulty === 'S')
        ? 1
        : 0;
    agent.verifiedSWins += verified && entry.effective_difficulty === 'S' ? 1 : 0;
    agent.summary.updated_at = timestamp;
    agent.ledger.push(entry);
  }

  function nowIso(): string {
    const currentMs = Date.now();
    lastTimestampMs = currentMs > lastTimestampMs ? currentMs : lastTimestampMs + 1;
    return new Date(lastTimestampMs).toISOString();
  }

  function deadlineIso(baseMs = Date.now()): string {
    return new Date(baseMs + LEASE_TTL_MS).toISOString();
  }

  function hydrateBounty(raw: BountyTask): BountyTask {
    const snapshot = clone(raw);
    if (snapshot.created_by_agent_id !== null) {
      const publisher = agentsById.get(snapshot.created_by_agent_id);
      if (publisher) {
        snapshot.created_by_agent_name = publisher.summary.name;
      }
    } else {
      snapshot.created_by_agent_name = null;
    }

    snapshot.draft_collaborator_agent_id = snapshot.draft_collaborator_agent_id ?? null;
    if (snapshot.draft_collaborator_agent_id !== null) {
      const collaborator = agentsById.get(snapshot.draft_collaborator_agent_id);
      if (collaborator) {
        snapshot.draft_collaborator_agent_name = collaborator.summary.name;
        snapshot.draft_collaborator_agent_tier = collaborator.summary.tier;
      } else {
        snapshot.draft_collaborator_agent_name = null;
        snapshot.draft_collaborator_agent_tier = null;
      }
    } else {
      snapshot.draft_collaborator_agent_name = null;
      snapshot.draft_collaborator_agent_tier = null;
    }
    snapshot.draft_needs_human_review = Boolean(snapshot.draft_needs_human_review);
    snapshot.parent_bounty_id = snapshot.parent_bounty_id ?? null;

    if (snapshot.lease_agent_id !== null) {
      const holder = agentsById.get(snapshot.lease_agent_id);
      if (holder) {
        snapshot.lease_agent_name = holder.summary.name;
        snapshot.lease_agent_tier = holder.summary.tier;
      }
    } else {
      snapshot.lease_agent_name = null;
      snapshot.lease_agent_tier = null;
    }

    if (snapshot.assignee_agent_id !== null) {
      const assignee = agentsById.get(snapshot.assignee_agent_id);
      if (assignee) {
        snapshot.assignee_agent_name = assignee.summary.name;
        snapshot.assignee_agent_tier = assignee.summary.tier;
      }
    } else {
      snapshot.assignee_agent_name = null;
      snapshot.assignee_agent_tier = null;
    }

    snapshot.arbitrator_agent_id = snapshot.arbitrator_agent_id ?? null;
    snapshot.arbitrator_agent_name = snapshot.arbitrator_agent_name ?? null;
    snapshot.arbitrator_agent_tier = snapshot.arbitrator_agent_tier ?? null;
    if (snapshot.arbitrator_agent_id !== null) {
      const arbitrator = agentsById.get(snapshot.arbitrator_agent_id);
      if (arbitrator) {
        snapshot.arbitrator_agent_name = arbitrator.summary.name;
        snapshot.arbitrator_agent_tier = arbitrator.summary.tier;
      }
    } else {
      snapshot.arbitrator_agent_name = null;
      snapshot.arbitrator_agent_tier = null;
    }
    snapshot.arbitration_expires_at = snapshot.arbitration_expires_at ?? null;
    snapshot.quality_grade = snapshot.quality_grade ?? null;
    snapshot.calibrated_difficulty = snapshot.calibrated_difficulty ?? null;
    snapshot.active_duration_ms = snapshot.active_duration_ms ?? 0;
    snapshot.active_started_at = snapshot.active_started_at ?? null;
    snapshot.comment_count = (commentsByBounty.get(snapshot.id) ?? []).length;

    const depIds = dependenciesByBounty.get(snapshot.id) ?? [];
    const summaries: BountyDependencySummary[] = [];
    for (const depId of depIds) {
      const depBounty = bounties.get(depId);
      if (depBounty) {
        summaries.push({
          id: depBounty.id,
          title: depBounty.title,
          status: depBounty.status,
        });
      }
    }
    snapshot.blocked_by_summaries = summaries;
    snapshot.blocked_by = summaries.map(item => item.id);
    snapshot.is_blocked = summaries.some(item => item.status !== 'COMPLETED');
    return snapshot;
  }

  function listHydratedComments(id: string): BountyComment[] {
    const rawComments = commentsByBounty.get(id) ?? [];
    const byId = new Map<number, BountyComment>();
    const hydrated = rawComments.map(comment => {
      const copy = clone(comment);
      if (copy.author_type === 'agent' && copy.author_id) {
        const agent = agentsById.get(copy.author_id);
        if (agent) {
          copy.author_name = agent.summary.name;
        }
      }
      copy.parent_id = copy.parent_id ?? null;
      copy.reply_to_comment_id = copy.reply_to_comment_id ?? null;
      copy.reply_to_author_name = copy.reply_to_author_name ?? null;
      copy.like_count = copy.like_count ?? 0;
      copy.liked = copy.liked ?? false;
      copy.updated_at = copy.updated_at ?? null;
      byId.set(copy.id, copy);
      return copy;
    });
    for (const copy of hydrated) {
      if (copy.reply_to_comment_id != null) {
        const target = byId.get(copy.reply_to_comment_id);
        if (target) {
          copy.reply_to_author_name = target.author_name;
        }
      }
    }
    return hydrated.sort((a, b) => a.id - b.id);
  }

  for (const project of seed?.projects ?? []) {
    const copied = clone(project);
    copied.arbitrator_chain = normalizeArbitratorChain(copied.arbitrator_chain ?? []);
    projects.set(copied.id, copied);
  }
  for (const bounty of seed?.bounties ?? []) {
    const seedBlockedBy = Array.isArray(bounty.blocked_by) ? [...bounty.blocked_by] : [];
    if (seedBlockedBy.length > 0) {
      dependenciesByBounty.set(bounty.id, seedBlockedBy);
    }
    bounties.set(bounty.id, {
      ...clone(bounty),
      assignee_agent_name: bounty.assignee_agent_name ?? null,
      assignee_agent_tier: bounty.assignee_agent_tier ?? null,
      assignment_expires_at: bounty.assignment_expires_at ?? null,
      created_by_agent_id: bounty.created_by_agent_id ?? null,
      created_by_agent_name: bounty.created_by_agent_name ?? null,
      draft_collaborator_agent_id: bounty.draft_collaborator_agent_id ?? null,
      draft_collaborator_agent_name: bounty.draft_collaborator_agent_name ?? null,
      draft_collaborator_agent_tier: bounty.draft_collaborator_agent_tier ?? null,
      draft_needs_human_review: bounty.draft_needs_human_review ?? false,
      parent_bounty_id: bounty.parent_bounty_id ?? null,
      blocked_by: seedBlockedBy,
      blocked_by_summaries: Array.isArray(bounty.blocked_by_summaries)
        ? clone(bounty.blocked_by_summaries)
        : [],
      is_blocked: bounty.is_blocked ?? false,
      lease_agent_name: bounty.lease_agent_name ?? null,
      lease_agent_tier: bounty.lease_agent_tier ?? null,
      arbitrator_agent_id: bounty.arbitrator_agent_id ?? null,
      arbitrator_agent_name: bounty.arbitrator_agent_name ?? null,
      arbitrator_agent_tier: bounty.arbitrator_agent_tier ?? null,
      arbitration_expires_at: bounty.arbitration_expires_at ?? null,
      quality_grade: bounty.quality_grade ?? null,
      calibrated_difficulty: bounty.calibrated_difficulty ?? null,
      active_duration_ms: bounty.active_duration_ms ?? 0,
      active_started_at: bounty.active_started_at ?? null,
      comment_count: bounty.comment_count ?? 0,
      completion_cost: bounty.completion_cost ?? null,
    });
    eventsByBounty.set(bounty.id, []);
    commentsByBounty.set(bounty.id, []);
    arbitrationTriedIdsByBounty.set(bounty.id, []);
  }

  function normalizeDependencyIds(raw: unknown): string[] {
    if (!Array.isArray(raw) || raw.some(item => typeof item !== 'string')) {
      throw new ServiceApiError('VALIDATION_ERROR', 400, 'blocked_by 必须为字符串数组');
    }
    const seen = new Set<string>();
    const result: string[] = [];
    for (const id of raw) {
      if (!id.trim()) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, '任务 ID 不能包含空白项');
      }
      if (!seen.has(id)) {
        seen.add(id);
        result.push(id);
      }
    }
    return result;
  }

  function hasDependencyCycle(): boolean {
    for (const root of dependenciesByBounty.keys()) {
      const visited = new Set<string>();
      const queue = [...(dependenciesByBounty.get(root) ?? [])];
      while (queue.length > 0) {
        const node = queue.shift()!;
        if (node === root) {
          return true;
        }
        if (visited.has(node)) continue;
        visited.add(node);
        for (const next of dependenciesByBounty.get(node) ?? []) {
          queue.push(next);
        }
      }
    }
    return false;
  }

  function replaceDependencies(id: string, rawIds: unknown, agentId: string | null): void {
    const ids = normalizeDependencyIds(rawIds);
    for (const target of ids) {
      if (target === id) {
        throw new ServiceApiError(
          'DEPENDENCY_CYCLE',
          409,
          '存在自依赖或循环依赖，违反有向无环图约束',
        );
      }
      const dep = requireBounty(target);
      if (agentId !== null) {
        const agent = agentsById.get(agentId);
        if (!agent || !isBountyVisibleToAgent(agent.summary, dep)) {
          throw new ServiceApiError('BOUNTY_NOT_FOUND', 404, '任务不存在');
        }
      }
      if (dep.status === 'CANCELLED') {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
    }
    const prev = dependenciesByBounty.get(id);
    if (ids.length > 0) {
      dependenciesByBounty.set(id, ids);
    } else {
      dependenciesByBounty.delete(id);
    }
    if (hasDependencyCycle()) {
      if (prev !== undefined) {
        dependenciesByBounty.set(id, prev);
      } else {
        dependenciesByBounty.delete(id);
      }
      throw new ServiceApiError(
        'DEPENDENCY_CYCLE',
        409,
        '存在自依赖或循环依赖，违反有向无环图约束',
      );
    }
  }

  function relatedSummaries(id: string, children: boolean): BountyDependencySummary[] {
    return Array.from(bounties.values())
      .filter(candidate =>
        children
          ? candidate.parent_bounty_id === id
          : (dependenciesByBounty.get(candidate.id) ?? []).includes(id),
      )
      .sort((a, b) => a.created_at.localeCompare(b.created_at) || a.id.localeCompare(b.id))
      .map(candidate => ({
        id: candidate.id,
        title: candidate.title,
        status: candidate.status,
      }));
  }

  function redactBountyForAgent(bounty: BountyTask, agentId: string): BountyTask {
    const copy = clone(bounty);
    const agent = agentsById.get(agentId);
    for (const summary of copy.blocked_by_summaries) {
      const raw = bounties.get(summary.id);
      if (!raw || !agent || !isBountyVisibleToAgent(agent.summary, raw)) {
        summary.title = '[草稿前置任务]';
      }
    }
    return copy;
  }

  function redactPayloadForAgent(
    data: Record<string, unknown>,
    agentId: string,
  ): Record<string, unknown> {
    const copy = clone(data);
    const maybeBounty =
      copy.bounty && typeof copy.bounty === 'object' && !Array.isArray(copy.bounty)
        ? (copy.bounty as Record<string, unknown>)
        : copy;
    if (Array.isArray(maybeBounty.blocked_by_summaries)) {
      const redacted = redactBountyForAgent(maybeBounty as unknown as BountyTask, agentId);
      maybeBounty.blocked_by_summaries = redacted.blocked_by_summaries;
    }
    return copy;
  }

  function requireProject(id: string): BountyProject {
    const project = projects.get(id);
    if (!project) {
      throw new ServiceApiError('PROJECT_NOT_FOUND', 404, '项目不存在');
    }
    return project;
  }

  function requireActiveProject(id: string): BountyProject {
    const project = requireProject(id);
    if (project.archived) {
      throw new ServiceApiError('PROJECT_ARCHIVED', 409, '归档项目不能创建、编辑或发布任务');
    }
    return project;
  }

  function requireBounty(id: string): BountyTask {
    const bounty = bounties.get(id);
    if (!bounty) {
      throw new ServiceApiError('BOUNTY_NOT_FOUND', 404, '任务不存在');
    }
    return bounty;
  }

  function requirePublishedBounty(id: string): BountyTask {
    const bounty = requireBounty(id);
    if (bounty.status === 'DRAFT' || bounty.published_at === null) {
      throw new ServiceApiError('BOUNTY_NOT_FOUND', 404, '任务不存在');
    }
    return bounty;
  }

  function recordEvent(
    bounty: BountyTask,
    eventType: string,
    actor: string,
    fromStatus: BountyStatus | null,
    detail: Record<string, unknown> = {},
  ): void {
    const list = eventsByBounty.get(bounty.id) ?? [];
    list.push({
      id: nextEventId++,
      bounty_id: bounty.id,
      event_type: eventType,
      actor,
      from_status: fromStatus,
      to_status: bounty.status,
      detail: clone(detail),
      created_at: bounty.updated_at,
    });
    eventsByBounty.set(bounty.id, list);
  }

  function sortedInviteTokens(): InviteTokenSummary[] {
    return Array.from(inviteTokensById.values())
      .map(entry => clone(entry.summary))
      .sort((a, b) => b.created_at.localeCompare(a.created_at) || b.id.localeCompare(a.id));
  }

  function validateInviteToken(raw: string, nowMs = Date.now()): StoredInviteToken {
    const entry = inviteTokensByRaw.get(raw);
    if (!entry) {
      throw new ServiceApiError('INVITE_TOKEN_INVALID', 401, '邀请令牌无效、已过期或已耗尽');
    }
    const { summary } = entry;
    if (summary.revoked_at !== null) {
      throw new ServiceApiError('INVITE_TOKEN_INVALID', 401, '邀请令牌无效、已过期或已耗尽');
    }
    if (summary.expires_at !== null) {
      const expiresAtMs = Date.parse(summary.expires_at);
      if (Number.isNaN(expiresAtMs) || expiresAtMs <= nowMs) {
        throw new ServiceApiError('INVITE_TOKEN_INVALID', 401, '邀请令牌无效、已过期或已耗尽');
      }
    }
    if (summary.max_uses !== null && summary.use_count >= summary.max_uses) {
      throw new ServiceApiError('INVITE_TOKEN_INVALID', 401, '邀请令牌无效、已过期或已耗尽');
    }
    return entry;
  }

  function computeActiveLeaseCount(agentId: string, nowMs = Date.now()): number {
    let count = 0;
    for (const bounty of bounties.values()) {
      if (bounty.lease_agent_id !== agentId) continue;
      if (
        (bounty.status === 'IN_PROGRESS' &&
          bounty.lease_expires_at !== null &&
          Date.parse(bounty.lease_expires_at) > nowMs) ||
        bounty.status === 'WAITING_INFO' ||
        bounty.status === 'PENDING_ARBITRATION'
      ) {
        count += 1;
      }
    }
    return count;
  }

  function toAgentSummary(entry: StoredAgent): AgentSummary {
    const summary = clone(entry.summary);
    summary.active_lease_count = computeActiveLeaseCount(summary.id);
    const streamConnected =
      entry.summary.status === 'active' &&
      (entry.streamListeners.size > 0 || entry.simulatedStreamConnected);
    const webhookConfigured = summary.webhook_url !== null;
    const channelStatus: AgentChannelStatus = streamConnected
      ? 'streaming'
      : webhookConfigured
        ? 'webhook'
        : 'offline';
    summary.stream_connected = streamConnected;
    summary.webhook_configured = webhookConfigured;
    summary.channel_status = channelStatus;
    return summary;
  }

  function toWebhookConfig(entry: StoredAgent): AgentWebhookConfig {
    const cutoffMs = Date.now() - 24 * 3_600_000;
    const failedInInbox = entry.inbox.filter(
      item => item.lastError !== null && Date.parse(item.event.created_at) >= cutoffMs,
    ).length;
    return {
      webhook_url: entry.summary.webhook_url,
      webhook_configured: entry.summary.webhook_url !== null,
      has_secret: entry.webhookSecret !== null,
      last_pushed_at: entry.summary.last_pushed_at,
      recent_failed_count: failedInInbox + entry.simulatedRecentFailedCount,
    };
  }

  function disconnectAgentStreams(entry: StoredAgent): void {
    entry.streamListeners.clear();
    entry.simulatedStreamConnected = false;
  }

  function pushAgentEvent(entry: StoredAgent, event: AgentEvent): void {
    entry.inbox.push({ event, lastError: null });
    if (entry.streamListeners.size > 0) {
      entry.summary.last_pushed_at = nowIso();
      for (const listener of entry.streamListeners.values()) {
        listener(clone(event));
      }
    }
  }

  function notifyActiveAgent(
    agentId: string | null,
    bounty: BountyTask,
    eventType: string,
    data: Record<string, unknown>,
  ): void {
    if (!agentId) return;
    const entry = agentsById.get(agentId);
    if (!entry || entry.summary.status !== 'active') return;
    pushAgentEvent(entry, {
      event_id: nextAgentEventId++,
      event_type: eventType,
      bounty_id: bounty.id,
      project_id: bounty.project_id,
      created_at: nowIso(),
      data: redactPayloadForAgent(data, agentId),
    });
  }

  function dispatchBountyEvents(bounty: BountyTask, withdrawn: boolean): void {
    const timestamp = nowIso();
    if (withdrawn) {
      const targetIds = Array.from(bountyDispatches.get(bounty.id) ?? []).sort((a, b) =>
        a < b ? -1 : a > b ? 1 : 0,
      );
      for (const agentId of targetIds) {
        const entry = agentsById.get(agentId);
        if (!entry || entry.summary.status !== 'active') continue;
        pushAgentEvent(entry, {
          event_id: nextAgentEventId++,
          event_type: 'task.withdrawn',
          bounty_id: bounty.id,
          project_id: bounty.project_id,
          created_at: timestamp,
          data: {
            bounty_id: bounty.id,
            project_id: bounty.project_id,
          },
        });
      }
      bountyDispatches.delete(bounty.id);
      return;
    }

    if (bounty.status !== 'OPEN') {
      throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
    }

    const targets = Array.from(agentsById.values())
      .filter(entry => {
        if (entry.summary.status !== 'active') return false;
        if (bounty.assignee_mode === 'specific') {
          return entry.summary.id === bounty.assignee_agent_id;
        }
        const subscribed =
          entry.summary.subscribe_all_projects ||
          entry.summary.project_ids.includes(bounty.project_id);
        if (!subscribed) return false;
        if (bounty.assignee_mode === 'min_tier') {
          return tierRank(entry.summary.tier) >= tierRank(bounty.assignee_min_tier ?? 'C');
        }
        return true;
      })
      .sort((a, b) =>
        a.summary.id < b.summary.id ? -1 : a.summary.id > b.summary.id ? 1 : 0,
      );

    const eventType = bounty.assignee_mode === 'specific' ? 'task.assigned' : 'task.published';
    const data = hydrateBounty(bounty) as unknown as Record<string, unknown>;
    let dispatched = bountyDispatches.get(bounty.id);
    if (!dispatched) {
      dispatched = new Set<string>();
      bountyDispatches.set(bounty.id, dispatched);
    }

    for (const entry of targets) {
      pushAgentEvent(entry, {
        event_id: nextAgentEventId++,
        event_type: eventType,
        bounty_id: bounty.id,
        project_id: bounty.project_id,
        created_at: timestamp,
        data: redactPayloadForAgent(data, entry.summary.id),
      });
      dispatched.add(entry.summary.id);
    }
  }

  function dispatchReady(id: string, timestamp: string): BountyTask {
    const raw = requireBounty(id);
    if (raw.status !== 'OPEN') {
      throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
    }
    let hydrated = hydrateBounty(raw);
    if (!hydrated.is_blocked) {
      if (raw.assignee_mode === 'specific') {
        const updated: BountyTask = {
          ...clone(raw),
          assignment_expires_at: deadlineIso(),
          updated_at: timestamp,
        };
        bounties.set(id, updated);
        hydrated = hydrateBounty(updated);
      }
      dispatchBountyEvents(hydrated, false);
    }
    return hydrated;
  }

  function unlockDependents(completedId: string, timestamp: string): void {
    const downstream = relatedSummaries(completedId, false);
    for (const summary of downstream) {
      if (summary.status !== 'OPEN') continue;
      const raw = bounties.get(summary.id);
      if (!raw) continue;
      const hydrated = hydrateBounty(raw);
      if (hydrated.is_blocked) continue;
      const updated: BountyTask = {
        ...clone(raw),
        assignment_expires_at: raw.assignee_mode === 'specific' ? deadlineIso() : null,
        updated_at: timestamp,
      };
      bounties.set(raw.id, updated);
      const unlocked = hydrateBounty(updated);
      recordEvent(unlocked, 'dependency_unlocked', 'system', 'OPEN', {
        unlocked_by_bounty_id: completedId,
      });
      dispatchBountyEvents(unlocked, false);
    }
  }

  function leaseDetail(bounty: BountyTask): Record<string, unknown> {
    const hydrated = hydrateBounty(bounty);
    return {
      lease_id: hydrated.lease_id,
      lease_agent_id: hydrated.lease_agent_id,
      lease_agent_name: hydrated.lease_agent_name,
      lease_agent_tier: hydrated.lease_agent_tier,
      lease_expires_at: hydrated.lease_expires_at,
    };
  }

  function validateJsonObjectKeys(input: unknown, allowedKeys: ReadonlySet<string>): Record<string, unknown> {
    if (!input || typeof input !== 'object' || Array.isArray(input)) {
      throw new ServiceApiError('VALIDATION_ERROR', 400, '请求正文必须为 JSON 对象');
    }
    const record = input as Record<string, unknown>;
    for (const key of Object.keys(record)) {
      if (!allowedKeys.has(key)) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, `不允许传入未知字段: ${key}`);
      }
    }
    return record;
  }

  function requireFencedBounty(
    id: string,
    agentId: string,
    leaseId: number,
    nowMs = Date.now(),
  ): BountyTask {
    const bounty = requirePublishedBounty(id);
    if (bounty.lease_id !== leaseId || bounty.lease_agent_id === null) {
      throw new ServiceApiError('STALE_LEASE', 409, '租约令牌已失效或租约已到期');
    }
    if (bounty.lease_agent_id !== agentId) {
      throw new ServiceApiError('LEASE_OWNER_MISMATCH', 403, 'Agent 不是租约持有者');
    }
    if (bounty.status !== 'IN_PROGRESS') {
      throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
    }
    if (bounty.lease_expires_at === null || Date.parse(bounty.lease_expires_at) <= nowMs) {
      throw new ServiceApiError('STALE_LEASE', 409, '租约令牌已失效或租约已到期');
    }
    return bounty;
  }

  function settleActive(bounty: BountyTask, timestamp: string): number {
    let elapsed = 0;
    if (bounty.status === 'IN_PROGRESS' && bounty.active_started_at !== null) {
      const startMs = Date.parse(bounty.active_started_at);
      const endMs = Date.parse(timestamp);
      if (!Number.isNaN(startMs) && !Number.isNaN(endMs)) {
        elapsed = Math.max(0, endMs - startMs);
      }
    }
    const total = (bounty.active_duration_ms ?? 0) + elapsed;
    const stored = bounties.get(bounty.id);
    if (stored) {
      stored.active_duration_ms = total;
      stored.active_started_at = null;
    }
    return total;
  }

  function commentPayload(bounty: BountyTask, comment: BountyComment): Record<string, unknown> {
    return {
      bounty: clone(bounty),
      comment: clone(comment),
      comment_id: comment.id,
      body: comment.body,
      lease_id: bounty.lease_id,
      lease_expires_at: bounty.lease_expires_at,
    };
  }

  function addComment(
    bounty: BountyTask,
    authorId: string | null,
    authorName: string,
    body: string,
    replyToCommentId: number | null = null,
  ): BountyCommentMutationResult {
    if (bounty.status === 'CANCELLED') {
      throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
    }
    const trimmedBody = body.trim();
    if (!trimmedBody) {
      throw new ServiceApiError('VALIDATION_ERROR', 400, '评论或问题正文不能为空');
    }
    const existingComments = commentsByBounty.get(bounty.id) ?? [];
    let parentId: number | null = null;
    let targetCommentId: number | null = null;
    let replyToAuthorName: string | null = null;
    if (replyToCommentId !== null) {
      const target = listHydratedComments(bounty.id).find(item => item.id === replyToCommentId);
      if (!target) {
        throw new ServiceApiError('COMMENT_NOT_FOUND', 404, '目标评论不存在');
      }
      parentId = target.parent_id ?? target.id;
      targetCommentId = target.id;
      replyToAuthorName = target.author_name;
    }
    const resume =
      bounty.status === 'WAITING_INFO' &&
      (authorId === null ||
        (authorId === bounty.created_by_agent_id && authorId !== bounty.lease_agent_id));
    const timestamp = nowIso();
    const kind: BountyComment['kind'] = resume ? 'reply' : 'comment';
    const comment: BountyComment = {
      id: nextCommentId++,
      bounty_id: bounty.id,
      author_type: authorId !== null ? 'agent' : 'human',
      author_id: authorId,
      author_name: authorName,
      kind,
      body: trimmedBody,
      parent_id: parentId,
      reply_to_comment_id: targetCommentId,
      reply_to_author_name: replyToAuthorName,
      like_count: 0,
      liked: false,
      updated_at: null,
      created_at: timestamp,
    };
    existingComments.push(comment);
    commentsByBounty.set(bounty.id, existingComments);

    const updated: BountyTask =
      bounty.status === 'DRAFT'
        ? {
            ...clone(bounty),
            draft_needs_human_review: authorId !== null,
            updated_at: timestamp,
          }
        : resume
          ? {
              ...clone(bounty),
              status: 'IN_PROGRESS',
              lease_expires_at: deadlineIso(Date.parse(timestamp)),
              active_started_at: timestamp,
              updated_at: timestamp,
            }
          : {
              ...clone(bounty),
              updated_at: timestamp,
            };
    bounties.set(bounty.id, updated);
    if (resume) {
      leaseWarnedIdByBounty.set(bounty.id, 0);
    }
    const hydrated = hydrateBounty(updated);
    const actor = authorId !== null ? `agent:${authorId}` : 'human';
    const detail = resume
      ? {
          lease_id: hydrated.lease_id,
          lease_agent_id: hydrated.lease_agent_id,
          comment_id: comment.id,
          lease_expires_at: hydrated.lease_expires_at,
        }
      : { comment_id: comment.id };
    recordEvent(hydrated, resume ? 'resumed' : 'commented', actor, bounty.status, detail);
    const data = commentPayload(hydrated, comment);
    if (resume) {
      notifyActiveAgent(hydrated.lease_agent_id, hydrated, 'comment.replied', data);
      notifyActiveAgent(hydrated.lease_agent_id, hydrated, 'task.resumed', data);
    } else if (hydrated.status === 'DRAFT') {
      if (authorId === null) {
        notifyActiveAgent(
          hydrated.draft_collaborator_agent_id,
          hydrated,
          'comment.added',
          data,
        );
      }
    } else if (hydrated.published_at !== null) {
      const targets = new Set<string>();
      for (const candidate of [hydrated.lease_agent_id, hydrated.created_by_agent_id]) {
        if (candidate !== null && candidate !== authorId) {
          targets.add(candidate);
        }
      }
      for (const target of targets) {
        notifyActiveAgent(target, hydrated, 'comment.added', data);
      }
    }
    const hydratedComment =
      listHydratedComments(bounty.id).find(item => item.id === comment.id) ?? clone(comment);
    const returnedBounty =
      authorId !== null ? redactBountyForAgent(hydrated, authorId) : hydrated;
    return {
      bounty: returnedBounty,
      comment: hydratedComment,
    };
  }

  function reopenFromLease(bounty: BountyTask, timestamp: string): BountyTask {
    const fallback =
      bounty.assignee_mode === 'specific' && bounty.assignee_agent_id === bounty.lease_agent_id;
    const updated: BountyTask = {
      ...clone(bounty),
      status: 'OPEN',
      active_duration_ms: 0,
      active_started_at: null,
      lease_agent_id: null,
      lease_agent_name: null,
      lease_agent_tier: null,
      lease_expires_at: null,
      arbitrator_agent_id: null,
      arbitrator_agent_name: null,
      arbitrator_agent_tier: null,
      arbitration_expires_at: null,
      rejection_count: 0,
      completion_note: null,
      completion_cost: null,
      assignee_mode: fallback ? 'any' : bounty.assignee_mode,
      assignee_agent_id: fallback ? null : bounty.assignee_agent_id,
      assignee_agent_name: fallback ? null : bounty.assignee_agent_name,
      assignee_agent_tier: fallback ? null : bounty.assignee_agent_tier,
      assignee_min_tier: fallback ? null : bounty.assignee_min_tier,
      assignment_expires_at: fallback ? null : bounty.assignment_expires_at,
      updated_at: timestamp,
    };
    bounties.set(updated.id, updated);
    leaseWarnedIdByBounty.set(updated.id, 0);
    arbitrationTriedIdsByBounty.set(updated.id, []);
    return hydrateBounty(updated);
  }

  function requireStoredAgent(id: string): StoredAgent {
    const entry = agentsById.get(id);
    if (!entry) {
      throw new ServiceApiError('AGENT_NOT_FOUND', 404, 'Agent 不存在');
    }
    return entry;
  }

  function authenticateAgent(apiKey: string): StoredAgent {
    const entry = agentsByApiKey.get(apiKey);
    if (!entry) {
      throw new ServiceApiError('UNAUTHORIZED', 401, '需要有效的 Agent Bearer api_key');
    }
    if (entry.summary.status === 'disabled') {
      throw new ServiceApiError('AGENT_DISABLED', 403, 'Agent 已停用');
    }
    if (entry.summary.status === 'revoked') {
      throw new ServiceApiError('AGENT_REVOKED', 401, 'Agent 凭证已吊销');
    }
    entry.summary.last_seen_at = nowIso();
    return entry;
  }

  function resolveArbitrationCandidates(bounty: BountyTask): string[] {
    const project = requireProject(bounty.project_id);
    const rawChain =
      project.arbitrator_chain.length > 0 ? project.arbitrator_chain : globalArbitratorChain;
    const active: string[] = [];
    for (const id of normalizeArbitratorChain(rawChain)) {
      const entry = agentsById.get(id);
      if (entry && entry.summary.status === 'active') {
        active.push(id);
      }
    }
    return active;
  }

  function parseApproveArbitrationInput(input?: ApproveArbitrationInput): {
    quality_grade: QualityGrade;
    calibrated_difficulty: BountyDifficulty | null;
    comment: string | null;
  } {
    if (input === undefined) {
      return {
        quality_grade: 'perfect',
        calibrated_difficulty: null,
        comment: null,
      };
    }
    const record = validateJsonObjectKeys(input, APPROVE_ARBITRATION_ALLOWED_KEYS);
    if (record.quality_grade !== undefined && record.quality_grade !== null) {
      if (
        typeof record.quality_grade !== 'string' ||
        !VALID_QUALITY_GRADES.has(record.quality_grade as QualityGrade)
      ) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'quality_grade 必须为 perfect 或 flawed');
      }
    }
    if (record.calibrated_difficulty !== undefined && record.calibrated_difficulty !== null) {
      if (
        typeof record.calibrated_difficulty !== 'string' ||
        !VALID_DIFFICULTIES.has(record.calibrated_difficulty as BountyDifficulty)
      ) {
        throw new ServiceApiError(
          'VALIDATION_ERROR',
          400,
          'calibrated_difficulty 必须为 C、B、A 或 S',
        );
      }
    }
    if (
      record.comment !== undefined &&
      record.comment !== null &&
      typeof record.comment !== 'string'
    ) {
      throw new ServiceApiError('VALIDATION_ERROR', 400, 'comment 必须为字符串');
    }
    const trimmedComment =
      typeof record.comment === 'string' ? record.comment.trim() : '';
    return {
      quality_grade:
        (record.quality_grade as QualityGrade | null | undefined) ?? 'perfect',
      calibrated_difficulty:
        (record.calibrated_difficulty as BountyDifficulty | null | undefined) ?? null,
      comment: trimmedComment.length > 0 ? trimmedComment : null,
    };
  }

  function approveArbitrationInternal(
    id: string,
    agentId: string | null,
    input?: ApproveArbitrationInput,
  ): BountyTask {
    const parsed = parseApproveArbitrationInput(input);
    const existing = requireBounty(id);
    if (existing.status !== 'PENDING_ARBITRATION') {
      throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
    }
    let actor = 'human';
    let authorName = 'human';
    if (agentId !== null) {
      const entry = requireStoredAgent(agentId);
      if (
        existing.arbitrator_agent_id !== agentId ||
        existing.lease_agent_id === agentId
      ) {
        throw new ServiceApiError(
          'ARBITRATOR_MISMATCH',
          403,
          'Agent 不是当前指定裁判或与执行者身份冲突',
        );
      }
      actor = `agent:${agentId}`;
      authorName = entry.summary.name;
    }

    const executor = requireRatingAgent(existing);
    const timestamp = nowIso();
    let commentId: number | null = null;
    if (parsed.comment !== null) {
      const comment: BountyComment = {
        id: nextCommentId++,
        bounty_id: id,
        author_type: agentId !== null ? 'agent' : 'human',
        author_id: agentId,
        author_name: authorName,
        kind: 'comment',
        body: parsed.comment,
        created_at: timestamp,
      };
      const list = commentsByBounty.get(id) ?? [];
      list.push(comment);
      commentsByBounty.set(id, list);
      commentId = comment.id;
    }

    const updated: BountyTask = {
      ...clone(existing),
      status: 'COMPLETED',
      completed_at: timestamp,
      quality_grade: parsed.quality_grade,
      calibrated_difficulty: parsed.calibrated_difficulty,
      arbitration_expires_at: null,
      lease_expires_at: null,
      updated_at: timestamp,
    };
    bounties.set(id, updated);
    settleRating(updated, executor, 'completed', true, timestamp);
    const hydrated = hydrateBounty(updated);
    const detail = {
      lease_id: existing.lease_id,
      quality_grade: parsed.quality_grade,
      calibrated_difficulty: parsed.calibrated_difficulty,
      comment_id: commentId,
    };
    recordEvent(hydrated, 'arbitration_approved', actor, existing.status, detail);
    notifyActiveAgent(existing.lease_agent_id, hydrated, 'arbitration.approved', {
      ...detail,
      bounty: hydrated,
    });
    unlockDependents(id, timestamp);
    return hydrateBounty(updated);
  }

  function rejectArbitrationInternal(
    id: string,
    agentId: string | null,
    input: RejectArbitrationInput,
  ): BountyTask {
    const record = validateJsonObjectKeys(input, REJECT_ARBITRATION_ALLOWED_KEYS);
    if (typeof record.comment !== 'string') {
      throw new ServiceApiError('VALIDATION_ERROR', 400, 'comment 必须为字符串');
    }
    const existing = requireBounty(id);
    if (existing.status !== 'PENDING_ARBITRATION') {
      throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
    }
    let actor = 'human';
    let authorName = 'human';
    if (agentId !== null) {
      const entry = requireStoredAgent(agentId);
      if (
        existing.arbitrator_agent_id !== agentId ||
        existing.lease_agent_id === agentId
      ) {
        throw new ServiceApiError(
          'ARBITRATOR_MISMATCH',
          403,
          'Agent 不是当前指定裁判或与执行者身份冲突',
        );
      }
      actor = `agent:${agentId}`;
      authorName = entry.summary.name;
    }

    const body = record.comment.trim();
    if (!body) {
      throw new ServiceApiError('VALIDATION_ERROR', 400, '打回评论不能为空');
    }

    const executor = requireRatingAgent(existing);
    const timestamp = nowIso();
    const comment: BountyComment = {
      id: nextCommentId++,
      bounty_id: id,
      author_type: agentId !== null ? 'agent' : 'human',
      author_id: agentId,
      author_name: authorName,
      kind: 'comment',
      body,
      created_at: timestamp,
    };
    const list = commentsByBounty.get(id) ?? [];
    list.push(comment);
    commentsByBounty.set(id, list);

    const next = existing.rejection_count + 1;
    const failed = next >= 4;
    leaseWarnedIdByBounty.set(id, 0);
    arbitrationTriedIdsByBounty.set(id, []);

    let updated: BountyTask;
    if (failed) {
      const fallback =
        existing.assignee_mode === 'specific' &&
        existing.assignee_agent_id === existing.lease_agent_id;
      const assignmentExpires =
        existing.assignee_mode === 'specific' && !fallback ? deadlineIso() : null;
      updated = {
        ...clone(existing),
        status: 'OPEN',
        rejection_count: 0,
        lease_agent_id: null,
        lease_agent_name: null,
        lease_agent_tier: null,
        lease_expires_at: null,
        arbitrator_agent_id: null,
        arbitrator_agent_name: null,
        arbitrator_agent_tier: null,
        arbitration_expires_at: null,
        completion_note: null,
        completion_cost: null,
        active_duration_ms: 0,
        active_started_at: null,
        assignee_mode: fallback ? 'any' : existing.assignee_mode,
        assignee_agent_id: fallback ? null : existing.assignee_agent_id,
        assignee_agent_name: fallback ? null : existing.assignee_agent_name,
        assignee_agent_tier: fallback ? null : existing.assignee_agent_tier,
        assignee_min_tier: fallback ? null : existing.assignee_min_tier,
        assignment_expires_at: assignmentExpires,
        updated_at: timestamp,
      };
    } else {
      updated = {
        ...clone(existing),
        status: 'IN_PROGRESS',
        rejection_count: next,
        lease_expires_at: deadlineIso(),
        active_started_at: timestamp,
        arbitrator_agent_id: null,
        arbitrator_agent_name: null,
        arbitrator_agent_tier: null,
        arbitration_expires_at: null,
        updated_at: timestamp,
      };
    }

    if (failed) {
      settleRating({ ...clone(existing), rejection_count: next }, executor, 'arbitration_failed', true, timestamp);
    }

    bounties.set(id, updated);
    const hydrated = hydrateBounty(updated);
    const detail = {
      failed_agent_id: failed ? existing.lease_agent_id : null,
      lease_id: existing.lease_id,
      rejection_count: next,
      active_duration_ms: existing.active_duration_ms,
      comment_id: comment.id,
      comment: body,
    };
    recordEvent(
      hydrated,
      failed ? 'arbitration_failed' : 'arbitration_rejected',
      actor,
      existing.status,
      detail,
    );
    notifyActiveAgent(
      existing.lease_agent_id,
      hydrated,
      failed ? 'arbitration.failed' : 'arbitration.rejected',
      { ...detail, bounty: hydrated },
    );
    if (failed) {
      dispatchBountyEvents(hydrated, false);
    }
    return hydrated;
  }

  function isBountyVisibleToAgent(agent: AgentSummary, bounty: BountyTask): boolean {
    if (bounty.status === 'DRAFT') {
      return bounty.draft_collaborator_agent_id === agent.id;
    }
    if (bounty.published_at === null) return false;
    if (
      bounty.created_by_agent_id === agent.id ||
      bounty.assignee_agent_id === agent.id ||
      bounty.lease_agent_id === agent.id ||
      bounty.arbitrator_agent_id === agent.id
    ) {
      return true;
    }
    if (bounty.assignee_mode === 'specific') {
      return false;
    }
    return agent.subscribe_all_projects || agent.project_ids.includes(bounty.project_id);
  }

  function filterBountiesInternal(query?: BountyQuery): BountyTask[] {
    const includeDrafts = query?.include_drafts === true;
    const needle = query?.q !== undefined && query.q !== '' ? asciiLower(query.q) : null;

    return Array.from(bounties.values())
      .filter(bounty => {
        if (!includeDrafts && bounty.status === 'DRAFT') return false;
        if (query?.project_id !== undefined && bounty.project_id !== query.project_id) return false;
        if (query?.status !== undefined && bounty.status !== query.status) return false;
        if (needle !== null) {
          const matched =
            asciiLower(bounty.id).includes(needle) ||
            asciiLower(bounty.title).includes(needle) ||
            asciiLower(bounty.description).includes(needle) ||
            asciiLower(bounty.acceptance_criteria).includes(needle);
          if (!matched) return false;
        }
        return true;
      })
      .sort((a, b) => b.created_at.localeCompare(a.created_at) || a.id.localeCompare(b.id))
      .map(hydrateBounty);
  }

  return {
    async health(): Promise<ServerHealth> {
      return { ok: true, mode: 'embedded', version: '0.8.8' };
    },

    async adminStatus(): Promise<AdminStatus> {
      return { ok: true, mode: 'embedded', db_path: ':memory:', version: '0.8.8' };
    },

    async registerUser(input: RegisterUserInput): Promise<AuthSessionResult> {
      const name = validateAccountName(input?.name);
      const password = validateAccountPassword(input?.password);
      const avatar = input?.avatar !== undefined ? validateAccountAvatar(input.avatar) : null;
      ensureAccountNameAvailable(name, null);
      const account: StoredUserAccount = {
        id: crypto.randomUUID(),
        role: 'user',
        name,
        password,
        avatar,
      };
      userAccountsById.set(account.id, account);
      const token = `tr_ac_${randomHex64()}`;
      userSessionsByToken.set(token, account.id);
      return {
        token,
        profile: toUserProfile(account),
      };
    },

    async loginUser(input: LoginUserInput): Promise<AuthSessionResult> {
      if (typeof input?.name !== 'string' || typeof input?.password !== 'string') {
        throw new ServiceApiError('INVALID_CREDENTIALS', 401, '名称或凭证不正确');
      }
      const trimmedName = input.name.trim();
      const nameErr = validateUserNameError(trimmedName);
      const passwordErr = validateUserPasswordError(input.password);
      if (nameErr || passwordErr) {
        throw new ServiceApiError('INVALID_CREDENTIALS', 401, '名称或凭证不正确');
      }
      let matched: StoredUserAccount | undefined;
      for (const account of userAccountsById.values()) {
        if (account.name === trimmedName && account.password === input.password) {
          matched = account;
          break;
        }
      }
      if (!matched) {
        throw new ServiceApiError('INVALID_CREDENTIALS', 401, '名称或凭证不正确');
      }
      const token = `tr_ac_${randomHex64()}`;
      userSessionsByToken.set(token, matched.id);
      return {
        token,
        profile: toUserProfile(matched),
      };
    },

    async getAuthMe(token: string): Promise<UserProfile> {
      return toUserProfile(authenticateUserSession(token));
    },

    async updateAuthMe(token: string, input: UpdateUserProfileInput): Promise<UserProfile> {
      const account = authenticateUserSession(token);
      const nextName =
        input?.name !== undefined ? validateAccountName(input.name) : undefined;
      const nextAvatar =
        input?.avatar !== undefined ? validateAccountAvatar(input.avatar) : undefined;
      if (nextName !== undefined) {
        ensureAccountNameAvailable(nextName, account.id);
      }
      if (nextName !== undefined) {
        account.name = nextName;
      }
      if (nextAvatar !== undefined) {
        account.avatar = nextAvatar;
      }
      return toUserProfile(account);
    },

    async logoutUser(token: string): Promise<void> {
      authenticateUserSession(token);
      userSessionsByToken.delete(token);
    },

    async listAuthAvatars(): Promise<AuthAvatarPreset[]> {
      return clone([...DEFAULT_AUTH_AVATAR_PRESETS]);
    },

    async listProjects(includeArchived = false): Promise<BountyProject[]> {
      return Array.from(projects.values())
        .filter(project => includeArchived || !project.archived)
        .sort((a, b) => a.created_at.localeCompare(b.created_at) || a.id.localeCompare(b.id))
        .map(clone);
    },

    async createProject(input: CreateProjectInput): Promise<BountyProject> {
      const icon = input.icon ?? null;
      const policy = input.agent_draft_policy ?? 'open';
      validateProjectFields(input.name, icon, policy);
      const arbitratorChain =
        input.arbitrator_chain !== undefined
          ? normalizeArbitratorChain(input.arbitrator_chain)
          : [];
      const timestamp = nowIso();
      const project: BountyProject = {
        id: crypto.randomUUID(),
        name: input.name,
        icon,
        description: input.description ?? '',
        workspace_path: input.workspace_path ?? '',
        default_acceptance_criteria: input.default_acceptance_criteria ?? '',
        agent_draft_policy: policy,
        arbitrator_chain: arbitratorChain,
        archived: false,
        created_at: timestamp,
        updated_at: timestamp,
      };
      projects.set(project.id, project);
      return clone(project);
    },

    async getProject(id: string): Promise<BountyProject> {
      return clone(requireProject(id));
    },

    async updateProject(id: string, input: UpdateProjectInput): Promise<BountyProject> {
      const existing = requireProject(id);
      const nextChain =
        input.arbitrator_chain !== undefined && input.arbitrator_chain !== null
          ? normalizeArbitratorChain(input.arbitrator_chain)
          : existing.arbitrator_chain;
      const candidate: BountyProject = {
        ...clone(existing),
        ...(input.name !== undefined && input.name !== null ? { name: input.name } : {}),
        ...(input.icon !== undefined ? { icon: input.icon } : {}),
        ...(input.description !== undefined && input.description !== null
          ? { description: input.description }
          : {}),
        ...(input.workspace_path !== undefined && input.workspace_path !== null
          ? { workspace_path: input.workspace_path }
          : {}),
        ...(input.default_acceptance_criteria !== undefined &&
        input.default_acceptance_criteria !== null
          ? { default_acceptance_criteria: input.default_acceptance_criteria }
          : {}),
        ...(input.agent_draft_policy !== undefined && input.agent_draft_policy !== null
          ? { agent_draft_policy: input.agent_draft_policy }
          : {}),
        arbitrator_chain: [...nextChain],
        ...(input.archived !== undefined && input.archived !== null
          ? { archived: input.archived }
          : {}),
        updated_at: nowIso(),
      };
      validateProjectFields(candidate.name, candidate.icon, candidate.agent_draft_policy);
      projects.set(id, candidate);
      return clone(candidate);
    },

    async deleteProject(id: string): Promise<void> {
      requireProject(id);
      for (const bounty of bounties.values()) {
        if (bounty.project_id === id) {
          throw new ServiceApiError('PROJECT_HAS_BOUNTIES', 409, '项目已有任务，不能删除');
        }
      }
      projects.delete(id);
      for (const entry of agentsById.values()) {
        if (entry.summary.project_ids.includes(id)) {
          entry.summary.project_ids = entry.summary.project_ids.filter(pid => pid !== id);
        }
      }
    },

    async listBounties(query?: BountyQuery): Promise<BountyTask[]> {
      return filterBountiesInternal(query);
    },

    async getBountyDetail(id: string): Promise<BountyDetail> {
      const bounty = requireBounty(id);
      const events = [...(eventsByBounty.get(id) ?? [])].sort((a, b) => a.id - b.id);
      return {
        bounty: hydrateBounty(bounty),
        events: clone(events),
        comments: listHydratedComments(id),
        blocks: relatedSummaries(id, false),
        child_bounties: relatedSummaries(id, true),
      };
    },

    async listBountyComments(id: string): Promise<BountyComment[]> {
      requireBounty(id);
      return listHydratedComments(id);
    },

    async createBountyComment(
      id: string,
      input: CreateBountyCommentInput,
    ): Promise<BountyCommentMutationResult> {
      const record = validateJsonObjectKeys(input, CREATE_COMMENT_ALLOWED_KEYS);
      if (typeof record.body !== 'string') {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'body 必须为字符串');
      }
      const replyToId =
        record.reply_to_comment_id === undefined || record.reply_to_comment_id === null
          ? null
          : typeof record.reply_to_comment_id === 'number' &&
              Number.isSafeInteger(record.reply_to_comment_id)
            ? record.reply_to_comment_id
            : (() => {
                throw new ServiceApiError('VALIDATION_ERROR', 400, 'reply_to_comment_id 必须为整数');
              })();
      const existing = requireBounty(id);
      return addComment(existing, null, 'human', record.body, replyToId);
    },

    async toggleBountyCommentLike(id: string, commentId: number): Promise<BountyComment> {
      requireBounty(id);
      const list = commentsByBounty.get(id) ?? [];
      const target = list.find(item => item.id === commentId);
      if (!target) {
        throw new ServiceApiError('COMMENT_NOT_FOUND', 404, '目标评论不存在');
      }
      target.liked = !(target.liked ?? false);
      target.like_count = target.liked
        ? (target.like_count ?? 0) + 1
        : Math.max(0, (target.like_count ?? 0) - 1);
      return listHydratedComments(id).find(item => item.id === commentId)!;
    },

    async updateBountyComment(
      id: string,
      commentId: number,
      input: UpdateBountyCommentInput,
    ): Promise<BountyComment> {
      const record = validateJsonObjectKeys(input, UPDATE_COMMENT_ALLOWED_KEYS);
      if (typeof record.body !== 'string') {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'body 必须为字符串');
      }
      const existing = requireBounty(id);
      if (existing.status === 'CANCELLED') {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
      const trimmed = record.body.trim();
      if (!trimmed) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, '评论或问题正文不能为空');
      }
      const list = commentsByBounty.get(id) ?? [];
      const target = list.find(item => item.id === commentId);
      if (!target) {
        throw new ServiceApiError('COMMENT_NOT_FOUND', 404, '目标评论不存在');
      }
      target.body = trimmed;
      target.updated_at = nowIso();
      return listHydratedComments(id).find(item => item.id === commentId)!;
    },

    async deleteBountyComment(id: string, commentId: number): Promise<DeleteBountyCommentResult> {
      const existing = requireBounty(id);
      if (existing.status === 'CANCELLED') {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
      const list = commentsByBounty.get(id) ?? [];
      const target = list.find(item => item.id === commentId);
      if (!target) {
        throw new ServiceApiError('COMMENT_NOT_FOUND', 404, '目标评论不存在');
      }
      const deletedIds = [
        commentId,
        ...list.filter(item => item.parent_id === commentId).map(item => item.id),
      ];
      const deletedSet = new Set(deletedIds);
      commentsByBounty.set(
        id,
        list.filter(item => !deletedSet.has(item.id)),
      );
      return {
        bounty: hydrateBounty(existing),
        deleted_ids: deletedIds,
      };
    },

    async createBounty(input: CreateBountyInput): Promise<BountyTask> {
      if (input.status !== 'DRAFT' && input.status !== 'OPEN') {
        throw new ServiceApiError('VALIDATION_ERROR', 400, '初始状态必须为 DRAFT 或 OPEN');
      }
      const createdBy = input.created_by ?? 'human';
      if (typeof createdBy !== 'string' || !createdBy.trim()) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'created_by 不能为空');
      }
      const minTier = input.assignee_min_tier ?? null;
      const agentId = input.assignee_agent_id ?? null;
      validateBountyFields(input.title, input.difficulty, input.assignee_mode, minTier, agentId);
      requireActiveProject(input.project_id);

      return withTransaction(() => {
        const timestamp = nowIso();
        const bounty: BountyTask = {
          id: crypto.randomUUID(),
          project_id: input.project_id,
          title: input.title,
          description: input.description ?? '',
          acceptance_criteria: input.acceptance_criteria ?? '',
          difficulty: input.difficulty,
          assignee_mode: input.assignee_mode,
          assignee_min_tier: minTier,
          assignee_agent_id: agentId,
          assignee_agent_name: null,
          assignee_agent_tier: null,
          assignment_expires_at: null,
          require_arbitration: input.require_arbitration ?? false,
          status: input.status,
          created_by: createdBy,
          created_by_agent_id: null,
          created_by_agent_name: null,
          draft_collaborator_agent_id: null,
          draft_collaborator_agent_name: null,
          draft_collaborator_agent_tier: null,
          draft_needs_human_review: false,
          parent_bounty_id: null,
          blocked_by: [],
          blocked_by_summaries: [],
          is_blocked: false,
          lease_id: 0,
          lease_agent_id: null,
          lease_agent_name: null,
          lease_agent_tier: null,
          lease_expires_at: null,
          arbitrator_agent_id: null,
          arbitrator_agent_name: null,
          arbitrator_agent_tier: null,
          arbitration_expires_at: null,
          quality_grade: null,
          calibrated_difficulty: null,
          active_duration_ms: 0,
          active_started_at: null,
          comment_count: 0,
          rejection_count: 0,
          completion_note: null,
          completion_cost: null,
          published_at: input.status === 'OPEN' ? timestamp : null,
          withdrawn_at: null,
          cancelled_at: null,
          completed_at: null,
          created_at: timestamp,
          updated_at: timestamp,
        };
        bounties.set(bounty.id, bounty);
        eventsByBounty.set(bounty.id, []);
        commentsByBounty.set(bounty.id, []);
        arbitrationTriedIdsByBounty.set(bounty.id, []);
        if (input.blocked_by !== undefined) {
          replaceDependencies(bounty.id, input.blocked_by, null);
        }
        let hydrated = hydrateBounty(bounty);
        recordEvent(hydrated, 'created', hydrated.created_by, null);
        if (hydrated.status === 'OPEN') {
          hydrated = dispatchReady(bounty.id, timestamp);
        }
        return hydrated;
      });
    },

    async updateBounty(id: string, input: UpdateBountyInput): Promise<BountyTask> {
      return withTransaction(() => {
        const existing = requireBounty(id);
        if (existing.status !== 'DRAFT') {
          throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
        }
        const timestamp = nowIso();
        const candidate: BountyTask = {
          ...clone(existing),
          ...(input.project_id !== undefined && input.project_id !== null
            ? { project_id: input.project_id }
            : {}),
          ...(input.title !== undefined && input.title !== null ? { title: input.title } : {}),
          ...(input.description !== undefined && input.description !== null
            ? { description: input.description }
            : {}),
          ...(input.acceptance_criteria !== undefined && input.acceptance_criteria !== null
            ? { acceptance_criteria: input.acceptance_criteria }
            : {}),
          ...(input.difficulty !== undefined && input.difficulty !== null
            ? { difficulty: input.difficulty }
            : {}),
          ...(input.assignee_mode !== undefined && input.assignee_mode !== null
            ? { assignee_mode: input.assignee_mode }
            : {}),
          ...(input.assignee_min_tier !== undefined
            ? { assignee_min_tier: input.assignee_min_tier }
            : {}),
          ...(input.assignee_agent_id !== undefined
            ? { assignee_agent_id: input.assignee_agent_id }
            : {}),
          ...(input.require_arbitration !== undefined && input.require_arbitration !== null
            ? { require_arbitration: input.require_arbitration }
            : {}),
          updated_at: timestamp,
        };
        requireActiveProject(candidate.project_id);
        validateBountyFields(
          candidate.title,
          candidate.difficulty,
          candidate.assignee_mode,
          candidate.assignee_min_tier,
          candidate.assignee_agent_id,
        );
        bounties.set(id, candidate);
        if (input.blocked_by !== undefined) {
          replaceDependencies(id, input.blocked_by, null);
        }
        const hydrated = hydrateBounty(candidate);
        recordEvent(hydrated, 'updated', 'human', 'DRAFT');
        return hydrated;
      });
    },

    async publishBounty(id: string): Promise<BountyTask> {
      const existing = requireBounty(id);
      if (existing.status !== 'DRAFT') {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
      requireActiveProject(existing.project_id);
      const timestamp = nowIso();
      const updated: BountyTask = {
        ...clone(existing),
        status: 'OPEN',
        published_at: timestamp,
        draft_needs_human_review: false,
        assignment_expires_at: null,
        updated_at: timestamp,
      };
      bounties.set(id, updated);
      const hydrated = dispatchReady(id, timestamp);
      recordEvent(hydrated, 'published', 'human', 'DRAFT');
      return hydrated;
    },

    async inviteDraftCollaborator(
      id: string,
      input: InviteDraftCollaboratorInput,
    ): Promise<BountyTask> {
      const record = validateJsonObjectKeys(input, INVITE_COLLABORATOR_ALLOWED_KEYS);
      if (
        record.agent_id !== undefined &&
        record.agent_id !== null &&
        typeof record.agent_id !== 'string'
      ) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'agent_id 必须为字符串或 null');
      }
      const rawAgentId = (record.agent_id as string | null | undefined) ?? null;
      const existing = requireBounty(id);
      if (existing.status !== 'DRAFT') {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
      let agentName: string | null = null;
      if (rawAgentId !== null) {
        if (!rawAgentId.trim()) {
          throw new ServiceApiError('VALIDATION_ERROR', 400, 'agent_id 不能为空白');
        }
        const targetAgent = requireStoredAgent(rawAgentId);
        if (targetAgent.summary.status !== 'active') {
          throw new ServiceApiError(
            'AGENT_INACTIVE',
            409,
            '只能邀请活跃状态的 Agent 协助草稿',
          );
        }
        agentName = targetAgent.summary.name;
      }
      const timestamp = nowIso();
      const updated: BountyTask = {
        ...clone(existing),
        draft_collaborator_agent_id: rawAgentId,
        updated_at: timestamp,
      };
      bounties.set(id, updated);
      const hydrated = hydrateBounty(updated);
      if (rawAgentId !== null) {
        recordEvent(hydrated, 'draft_invited', 'human', 'DRAFT', {
          collaborator_agent_id: rawAgentId,
          collaborator_agent_name: agentName,
        });
        notifyActiveAgent(rawAgentId, hydrated, 'draft.invited', { bounty: hydrated });
      } else {
        recordEvent(hydrated, 'draft_collaborator_cleared', 'human', 'DRAFT', {});
      }
      return hydrated;
    },

    async publishBountiesBatch(input: PublishBountiesBatchInput): Promise<BountyTask[]> {
      const record = validateJsonObjectKeys(input, PUBLISH_BATCH_ALLOWED_KEYS);
      const ids = normalizeDependencyIds(record.bounty_ids);
      if (ids.length === 0) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'bounty_ids 不能为空');
      }
      return withTransaction(() => {
        const timestamp = nowIso();
        for (const id of ids) {
          const bounty = requireBounty(id);
          if (bounty.status !== 'DRAFT') {
            throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
          }
          requireActiveProject(bounty.project_id);
        }
        for (const id of ids) {
          const current = requireBounty(id);
          bounties.set(id, {
            ...clone(current),
            status: 'OPEN',
            published_at: timestamp,
            draft_needs_human_review: false,
            assignment_expires_at: null,
            updated_at: timestamp,
          });
        }
        const published: BountyTask[] = [];
        for (const id of ids) {
          const bounty = dispatchReady(id, timestamp);
          recordEvent(bounty, 'published', 'human', 'DRAFT', {});
          published.push(bounty);
        }
        return published;
      });
    },

    async withdrawBounty(id: string): Promise<BountyTask> {
      const existing = requireBounty(id);
      if (existing.status === 'IN_PROGRESS' || existing.lease_agent_id !== null) {
        throw new ServiceApiError('BOUNTY_ALREADY_CLAIMED', 409, '任务已被接取，不能撤回或重复接取');
      }
      if (existing.status !== 'OPEN') {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
      const timestamp = nowIso();
      const updated: BountyTask = {
        ...clone(existing),
        status: 'DRAFT',
        withdrawn_at: timestamp,
        draft_needs_human_review: false,
        assignment_expires_at: null,
        updated_at: timestamp,
      };
      bounties.set(id, updated);
      const hydrated = hydrateBounty(updated);
      dispatchBountyEvents(hydrated, true);
      recordEvent(hydrated, 'withdrawn', 'human', 'OPEN');
      return hydrated;
    },

    async cancelBounty(id: string): Promise<BountyTask> {
      const existing = requireBounty(id);
      if (!CANCELLABLE_STATUSES.has(existing.status)) {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
      const timestamp = nowIso();
      const updated: BountyTask = {
        ...clone(existing),
        status: 'CANCELLED',
        active_started_at: null,
        lease_expires_at: null,
        cancelled_at: timestamp,
        draft_needs_human_review: false,
        assignment_expires_at: null,
        updated_at: timestamp,
      };
      bounties.set(id, updated);
      leaseWarnedIdByBounty.set(id, 0);
      const hydrated = hydrateBounty(updated);
      if (existing.status === 'WAITING_INFO') {
        notifyActiveAgent(existing.lease_agent_id, hydrated, 'task.cancelled', {
          bounty: hydrated,
          lease_id: existing.lease_id,
          lease_agent_id: existing.lease_agent_id,
        });
      }
      recordEvent(hydrated, 'cancelled', 'human', existing.status);
      return hydrated;
    },

    async restoreBountyDraft(id: string): Promise<BountyTask> {
      return withTransaction(() => {
        const existing = requireBounty(id);
        if (existing.status !== 'CANCELLED') {
          throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
        }
        // 清除上一轮公开和接取的状态，保留 lease_id 供下一次接取递增。
        const updated: BountyTask = {
          ...existing,
          status: 'DRAFT',
          published_at: null,
          withdrawn_at: null,
          cancelled_at: null,
          completed_at: null,
          assignment_expires_at: null,
          lease_agent_id: null,
          lease_expires_at: null,
          active_duration_ms: 0,
          active_started_at: null,
          rejection_count: 0,
          arbitrator_agent_id: null,
          arbitration_expires_at: null,
          completion_note: null,
          completion_cost: null,
          quality_grade: null,
          calibrated_difficulty: null,
          draft_collaborator_agent_id: null,
          draft_needs_human_review: false,
          updated_at: nowIso(),
        };
        bounties.set(id, updated);
        leaseWarnedIdByBounty.delete(id);
        arbitrationTriedIdsByBounty.delete(id);
        bountyDispatches.delete(id);
        const hydrated = hydrateBounty(updated);
        recordEvent(hydrated, 'restored_to_draft', 'human', existing.status);
        return hydrated;
      });
    },

    async deleteBounty(id: string): Promise<void> {
      const existing = requireBounty(id);
      if (existing.status !== 'CANCELLED') {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
      if (relatedSummaries(id, false).length > 0) {
        throw new ServiceApiError('BOUNTY_HAS_DEPENDENTS', 409, '该任务仍是其他任务的前置任务');
      }

      // 与 SQLite 外键行为一致。派生任务独立保留，任务名下的明细随任务删除。
      for (const child of bounties.values()) {
        if (child.parent_bounty_id === id) child.parent_bounty_id = null;
      }
      for (const agent of agentsById.values()) {
        agent.ledger = agent.ledger.filter(entry => entry.bounty_id !== id);
      }
      bounties.delete(id);
      dependenciesByBounty.delete(id);
      eventsByBounty.delete(id);
      commentsByBounty.delete(id);
      bountyDispatches.delete(id);
      leaseWarnedIdByBounty.delete(id);
      arbitrationTriedIdsByBounty.delete(id);
    },

    async releaseLease(id: string): Promise<BountyTask> {
      const existing = requireBounty(id);
      const releasable =
        existing.status === 'IN_PROGRESS' ||
        ((existing.status === 'WAITING_INFO' || existing.status === 'PENDING_ARBITRATION') &&
          existing.lease_agent_id !== null);
      if (!releasable) {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
      const hydratedOld = hydrateBounty(existing);
      const timestamp = nowIso();
      const activeDurationMs = settleActive(existing, timestamp);
      const updated: BountyTask = {
        ...clone(existing),
        status: 'OPEN',
        active_duration_ms: 0,
        active_started_at: null,
        lease_agent_id: null,
        lease_agent_name: null,
        lease_agent_tier: null,
        lease_expires_at: null,
        arbitrator_agent_id: null,
        arbitrator_agent_name: null,
        arbitrator_agent_tier: null,
        arbitration_expires_at: null,
        rejection_count: 0,
        completion_note: null,
        completion_cost: null,
        assignment_expires_at: existing.assignee_mode === 'specific' ? deadlineIso() : null,
        updated_at: timestamp,
      };
      bounties.set(id, updated);
      leaseWarnedIdByBounty.set(id, 0);
      arbitrationTriedIdsByBounty.set(id, []);
      const hydrated = hydrateBounty(updated);
      const detail = {
        lease_id: hydratedOld.lease_id,
        lease_agent_id: hydratedOld.lease_agent_id,
        lease_agent_name: hydratedOld.lease_agent_name,
        active_duration_ms: activeDurationMs,
      };
      recordEvent(hydrated, 'lease_released', 'human', existing.status, detail);
      notifyActiveAgent(hydratedOld.lease_agent_id, hydrated, 'lease.released', detail);
      dispatchBountyEvents(hydrated, false);
      return hydrated;
    },

    async getArbitrationConfig(): Promise<ArbitrationConfig> {
      return { arbitrator_chain: [...globalArbitratorChain] };
    },

    async updateArbitrationConfig(input: ArbitrationConfig): Promise<ArbitrationConfig> {
      const record = validateJsonObjectKeys(input, ARBITRATION_CONFIG_ALLOWED_KEYS);
      globalArbitratorChain = normalizeArbitratorChain(record.arbitrator_chain);
      return { arbitrator_chain: [...globalArbitratorChain] };
    },

    async approveArbitration(id: string, input?: ApproveArbitrationInput): Promise<BountyTask> {
      return approveArbitrationInternal(id, null, input);
    },

    async rejectArbitration(id: string, input: RejectArbitrationInput): Promise<BountyTask> {
      return rejectArbitrationInternal(id, null, input);
    },

    async getOnboardingSettings(): Promise<OnboardingSettings> {
      return {
        lan_open_registration: lanOpenRegistration,
        invite_tokens: sortedInviteTokens(),
      };
    },

    async updateOnboardingSettings(input: UpdateOnboardingInput): Promise<OnboardingSettings> {
      if (typeof input?.lan_open_registration !== 'boolean') {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'lan_open_registration 必须为布尔值');
      }
      lanOpenRegistration = input.lan_open_registration;
      return {
        lan_open_registration: lanOpenRegistration,
        invite_tokens: sortedInviteTokens(),
      };
    },

    async listInviteTokens(): Promise<InviteTokenSummary[]> {
      return sortedInviteTokens();
    },

    async createInviteToken(input?: CreateInviteTokenInput): Promise<CreatedInviteToken> {
      const label = input?.label ?? '';
      if (typeof label !== 'string') {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'label 必须为字符串');
      }
      const maxUses = input?.max_uses ?? null;
      if (maxUses !== null && (typeof maxUses !== 'number' || !Number.isInteger(maxUses) || maxUses < 1)) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'max_uses 必须至少为 1');
      }
      const expiresInHours = input?.expires_in_hours ?? null;
      let expiresAt: string | null = null;
      if (expiresInHours !== null) {
        if (typeof expiresInHours !== 'number' || !Number.isFinite(expiresInHours) || expiresInHours <= 0) {
          throw new ServiceApiError('VALIDATION_ERROR', 400, 'expires_in_hours 必须为有效正数');
        }
        const millis = expiresInHours * 3_600_000;
        if (!Number.isFinite(millis) || millis >= Number.MAX_SAFE_INTEGER) {
          throw new ServiceApiError('VALIDATION_ERROR', 400, '过期时间超出范围');
        }
        expiresAt = new Date(Date.now() + Math.max(1, Math.trunc(millis))).toISOString();
      }

      const rawToken = `tr_inv_${randomHex64()}`;
      const summary: InviteTokenSummary = {
        id: crypto.randomUUID(),
        label,
        token_prefix: credentialPrefix(rawToken),
        max_uses: maxUses,
        use_count: 0,
        expires_at: expiresAt,
        revoked_at: null,
        created_at: nowIso(),
      };
      const stored: StoredInviteToken = { summary, rawToken };
      inviteTokensById.set(summary.id, stored);
      inviteTokensByRaw.set(rawToken, stored);
      return {
        token: clone(summary),
        raw_token: rawToken,
      };
    },

    async revokeInviteToken(id: string): Promise<InviteTokenSummary> {
      const stored = inviteTokensById.get(id);
      if (!stored) {
        throw new ServiceApiError('INVITE_TOKEN_NOT_FOUND', 404, '邀请令牌不存在');
      }
      if (stored.summary.revoked_at === null) {
        stored.summary.revoked_at = nowIso();
      }
      return clone(stored.summary);
    },

    async deleteInviteToken(id: string): Promise<void> {
      const stored = inviteTokensById.get(id);
      if (!stored) {
        throw new ServiceApiError('INVITE_TOKEN_NOT_FOUND', 404, '邀请令牌不存在');
      }
      inviteTokensById.delete(id);
      inviteTokensByRaw.delete(stored.rawToken);
      for (const agent of agentsById.values()) {
        if (agent.summary.invite_token_id === id) {
          agent.summary.invite_token_id = null;
        }
      }
    },

    async getSkillMarkdown(options?: {
      baseUrl?: string;
      inviteToken?: string;
    }): Promise<SkillDocument> {
      const baseUrl =
        options?.baseUrl !== undefined && options.baseUrl.trim() !== ''
          ? validateSkillBaseUrl(options.baseUrl)
          : DEFAULT_SKILL_BASE_URL;
      const invite =
        options?.inviteToken !== undefined && options.inviteToken.trim() !== ''
          ? options.inviteToken
          : null;
      if (invite !== null) {
        validateInviteToken(invite);
      }
      return {
        base_url: baseUrl,
        content: renderSkillMarkdown(baseUrl, lanOpenRegistration),
        filename: 'SKILL.md',
      };
    },

    async getPublicSkillMarkdown(options?: {
      inviteToken?: string;
      bearerInviteToken?: string;
      baseUrl?: string;
    }): Promise<string> {
      const baseUrl =
        options?.baseUrl !== undefined && options.baseUrl.trim() !== ''
          ? validateSkillBaseUrl(options.baseUrl)
          : DEFAULT_SKILL_BASE_URL;
      const inviteFromQuery =
        options?.inviteToken !== undefined && options.inviteToken.trim() !== ''
          ? options.inviteToken
          : null;
      const inviteFromBearer =
        options?.bearerInviteToken !== undefined && options.bearerInviteToken.trim() !== ''
          ? options.bearerInviteToken
          : null;
      const invite = inviteFromQuery ?? inviteFromBearer;
      if (invite !== null) {
        validateInviteToken(invite);
      }
      return renderSkillMarkdown(baseUrl, lanOpenRegistration);
    },

    async listAgents(filter?: AgentFilter): Promise<AgentSummary[]> {
      return Array.from(agentsById.values())
        .sort(
          (a, b) =>
            b.summary.created_at.localeCompare(a.summary.created_at) ||
            b.summary.id.localeCompare(a.summary.id),
        )
        .map(toAgentSummary)
        .filter(agent => {
          if (filter?.status !== undefined && agent.status !== filter.status) {
            return false;
          }
          if (filter?.activity === 'idle' && agent.active_lease_count !== 0) {
            return false;
          }
          if (filter?.activity === 'busy' && agent.active_lease_count === 0) {
            return false;
          }
          return true;
        });
    },

    async getAgentDetail(id: string): Promise<AgentDetail> {
      const entry = requireStoredAgent(id);
      return {
        agent: toAgentSummary(entry),
        mmr: entry.mmr,
        placement_count: Math.min(entry.settledCount, 5),
        placement_required: 5,
        completed_count: entry.completedCount,
        failed_count: entry.failedCount,
        verified_as_wins: entry.verifiedAsWins,
        verified_s_wins: entry.verifiedSWins,
        ledger: [...entry.ledger].sort((a, b) => b.id - a.id).map(clone),
      };
    },

    async disableAgent(id: string): Promise<AgentSummary> {
      const entry = requireStoredAgent(id);
      if (entry.summary.status === 'revoked') {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
      entry.summary.status = 'disabled';
      entry.summary.updated_at = nowIso();
      disconnectAgentStreams(entry);
      return toAgentSummary(entry);
    },

    async enableAgent(id: string): Promise<AgentSummary> {
      const entry = requireStoredAgent(id);
      if (entry.summary.status === 'revoked') {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
      entry.summary.status = 'active';
      entry.summary.updated_at = nowIso();
      return toAgentSummary(entry);
    },

    async revokeAgent(id: string): Promise<AgentSummary> {
      const entry = requireStoredAgent(id);
      entry.summary.status = 'revoked';
      entry.summary.updated_at = nowIso();
      disconnectAgentStreams(entry);
      return toAgentSummary(entry);
    },

    async registerAgent(
      input: RegisterAgentInput,
      options?: { bearerInviteToken?: string },
    ): Promise<RegisteredAgentResult> {
      if (typeof input?.name !== 'string') {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'name 去除首尾空白后须为 1 至 64 字符');
      }
      const trimmedName = input.name.trim();
      const nameLength = Array.from(trimmedName).length;
      if (nameLength < 1 || nameLength > 64) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'name 去除首尾空白后须为 1 至 64 字符');
      }
      const description = input.description ?? '';
      if (typeof description !== 'string') {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'description 必须为字符串');
      }
      const capabilities = input.capabilities ?? [];
      if (!Array.isArray(capabilities) || capabilities.some(item => typeof item !== 'string')) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'capabilities 必须为字符串数组');
      }

      const { subscribeAll, projectIds } = resolveSubscriptionInput(
        input.subscribe_all_projects,
        input.project_ids,
      );
      const { webhookUrl, webhookSecret } = resolveWebhookValues(
        input.webhook_url,
        input.webhook_secret,
        null,
      );

      const bodyInvite =
        typeof input.invite_token === 'string' && input.invite_token.trim() !== ''
          ? input.invite_token
          : null;
      const bearerInvite =
        typeof options?.bearerInviteToken === 'string' && options.bearerInviteToken.trim() !== ''
          ? options.bearerInviteToken
          : null;
      const rawInvite = bodyInvite ?? bearerInvite;

      let inviteEntry: StoredInviteToken | null = null;
      if (rawInvite !== null) {
        inviteEntry = validateInviteToken(rawInvite);
      } else if (!lanOpenRegistration) {
        throw new ServiceApiError('INVITE_TOKEN_REQUIRED', 401, '需要注册邀请令牌');
      }

      for (const existing of agentsById.values()) {
        if (existing.summary.name === trimmedName) {
          throw new ServiceApiError('AGENT_NAME_TAKEN', 409, 'Agent 名称已被使用');
        }
      }

      for (const projectId of projectIds) {
        requireActiveProject(projectId);
      }

      if (inviteEntry !== null) {
        inviteEntry.summary.use_count += 1;
      }

      const apiKey = `tr_ag_${randomHex64()}`;
      const timestamp = nowIso();
      const summary: AgentSummary = {
        id: crypto.randomUUID(),
        name: trimmedName,
        description,
        capabilities: [...capabilities],
        api_key_prefix: credentialPrefix(apiKey),
        tier: 'Unranked',
        placement_completed: false,
        subscribe_all_projects: subscribeAll,
        project_ids: [...projectIds],
        status: 'active',
        invite_token_id: inviteEntry?.summary.id ?? null,
        active_lease_count: 0,
        timeout_count: 0,
        abandon_count: 0,
        last_seen_at: null,
        channel_status: webhookUrl !== null ? 'webhook' : 'offline',
        stream_connected: false,
        webhook_configured: webhookUrl !== null,
        webhook_url: webhookUrl,
        last_pushed_at: null,
        created_at: timestamp,
        updated_at: timestamp,
      };
      const stored: StoredAgent = {
        summary,
        apiKey,
        webhookSecret,
        ackedEventId: 0,
        inbox: [],
        streamListeners: new Map(),
        simulatedStreamConnected: false,
        simulatedRecentFailedCount: 0,
        mmr: 1000.0,
        settledCount: 0,
        completedCount: 0,
        failedCount: 0,
        verifiedAsWins: 0,
        verifiedSWins: 0,
        ledger: [],
      };
      agentsById.set(summary.id, stored);
      agentsByApiKey.set(apiKey, stored);

      const result: RegisteredAgentResult = {
        agent: toAgentSummary(stored),
        api_key: apiKey,
      };
      if (webhookSecret !== null) {
        result.webhook_secret = webhookSecret;
      }
      return result;
    },

    async agentGetMe(apiKey: string): Promise<AgentSummary> {
      const entry = authenticateAgent(apiKey);
      return toAgentSummary(entry);
    },

    async agentUpdateMe(apiKey: string, input: UpdateAgentInput): Promise<AgentSummary> {
      const entry = authenticateAgent(apiKey);
      if (input.description !== undefined) {
        if (typeof input.description !== 'string') {
          throw new ServiceApiError('VALIDATION_ERROR', 400, 'description 必须为字符串');
        }
        entry.summary.description = input.description;
      }
      if (input.capabilities !== undefined) {
        if (
          !Array.isArray(input.capabilities) ||
          input.capabilities.some(item => typeof item !== 'string')
        ) {
          throw new ServiceApiError('VALIDATION_ERROR', 400, 'capabilities 必须为字符串数组');
        }
        entry.summary.capabilities = [...input.capabilities];
      }
      entry.summary.updated_at = nowIso();
      return toAgentSummary(entry);
    },

    async agentUpdateSubscriptions(
      apiKey: string,
      input: AgentSubscriptionsInput,
    ): Promise<AgentSummary> {
      const entry = authenticateAgent(apiKey);
      const { subscribeAll, projectIds } = resolveSubscriptionInput(
        input.subscribe_all_projects,
        input.project_ids,
      );
      for (const projectId of projectIds) {
        requireActiveProject(projectId);
      }
      entry.summary.subscribe_all_projects = subscribeAll;
      entry.summary.project_ids = [...projectIds];
      entry.summary.updated_at = nowIso();
      return toAgentSummary(entry);
    },

    async agentListProjects(apiKey: string): Promise<BountyProject[]> {
      authenticateAgent(apiKey);
      return Array.from(projects.values())
        .filter(project => !project.archived)
        .sort((a, b) => a.created_at.localeCompare(b.created_at) || a.id.localeCompare(b.id))
        .map(clone);
    },

    async agentCreateBounty(apiKey: string, input: AgentCreateBountyInput): Promise<BountyTask> {
      const entry = authenticateAgent(apiKey);
      const record = validateJsonObjectKeys(input, AGENT_CREATE_BOUNTY_ALLOWED_KEYS);
      if (typeof record.project_id !== 'string') {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'project_id 必须为字符串');
      }
      if (record.description !== undefined && typeof record.description !== 'string') {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'description 必须为字符串');
      }
      if (
        record.acceptance_criteria !== undefined &&
        typeof record.acceptance_criteria !== 'string'
      ) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'acceptance_criteria 必须为字符串');
      }
      if (
        record.require_arbitration !== undefined &&
        typeof record.require_arbitration !== 'boolean'
      ) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'require_arbitration 必须为布尔值');
      }

      const title = record.title as string;
      const difficulty = record.difficulty as BountyDifficulty;
      const assigneeMode = record.assignee_mode as AssigneeMode;
      const minTier = (record.assignee_min_tier as BountyDifficulty | null | undefined) ?? null;
      const agentId = (record.assignee_agent_id as string | null | undefined) ?? null;
      validateBountyFields(title, difficulty, assigneeMode, minTier, agentId);
      const project = requireActiveProject(record.project_id);

      return withTransaction(() => {
        const timestamp = nowIso();
        const status: BountyStatus = project.agent_draft_policy === 'open' ? 'OPEN' : 'DRAFT';
        const actor = `agent:${entry.summary.id}`;
        const bounty: BountyTask = {
          id: crypto.randomUUID(),
          project_id: record.project_id as string,
          title,
          description: (record.description as string | undefined) ?? '',
          acceptance_criteria: (record.acceptance_criteria as string | undefined) ?? '',
          difficulty,
          assignee_mode: assigneeMode,
          assignee_min_tier: minTier,
          assignee_agent_id: agentId,
          assignee_agent_name: null,
          assignee_agent_tier: null,
          assignment_expires_at: null,
          require_arbitration: (record.require_arbitration as boolean | undefined) ?? false,
          status,
          created_by: actor,
          created_by_agent_id: entry.summary.id,
          created_by_agent_name: entry.summary.name,
          draft_collaborator_agent_id: null,
          draft_collaborator_agent_name: null,
          draft_collaborator_agent_tier: null,
          draft_needs_human_review: false,
          parent_bounty_id: null,
          blocked_by: [],
          blocked_by_summaries: [],
          is_blocked: false,
          lease_id: 0,
          lease_agent_id: null,
          lease_agent_name: null,
          lease_agent_tier: null,
          lease_expires_at: null,
          arbitrator_agent_id: null,
          arbitrator_agent_name: null,
          arbitrator_agent_tier: null,
          arbitration_expires_at: null,
          quality_grade: null,
          calibrated_difficulty: null,
          active_duration_ms: 0,
          active_started_at: null,
          comment_count: 0,
          rejection_count: 0,
          completion_note: null,
          completion_cost: null,
          published_at: status === 'OPEN' ? timestamp : null,
          withdrawn_at: null,
          cancelled_at: null,
          completed_at: null,
          created_at: timestamp,
          updated_at: timestamp,
        };
        bounties.set(bounty.id, bounty);
        eventsByBounty.set(bounty.id, []);
        commentsByBounty.set(bounty.id, []);
        arbitrationTriedIdsByBounty.set(bounty.id, []);
        if (record.blocked_by !== undefined) {
          replaceDependencies(bounty.id, record.blocked_by, entry.summary.id);
        }
        let hydrated = hydrateBounty(bounty);
        recordEvent(hydrated, 'created', actor, null, {
          publisher_agent_id: entry.summary.id,
          publisher_agent_name: entry.summary.name,
          policy: project.agent_draft_policy,
        });
        if (status === 'OPEN') {
          hydrated = dispatchReady(bounty.id, timestamp);
        }
        return redactBountyForAgent(hydrated, entry.summary.id);
      });
    },

    async agentUpdateDraft(
      apiKey: string,
      id: string,
      input: AgentUpdateDraftInput,
    ): Promise<BountyTask> {
      const entry = authenticateAgent(apiKey);
      const record = validateJsonObjectKeys(input, AGENT_UPDATE_DRAFT_ALLOWED_KEYS);
      if (
        record.title !== undefined &&
        record.title !== null &&
        typeof record.title !== 'string'
      ) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'title 必须为字符串');
      }
      if (
        record.description !== undefined &&
        record.description !== null &&
        typeof record.description !== 'string'
      ) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'description 必须为字符串');
      }
      if (
        record.acceptance_criteria !== undefined &&
        record.acceptance_criteria !== null &&
        typeof record.acceptance_criteria !== 'string'
      ) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'acceptance_criteria 必须为字符串');
      }

      const existing = requireBounty(id);
      if (existing.draft_collaborator_agent_id !== entry.summary.id) {
        throw new ServiceApiError('BOUNTY_NOT_FOUND', 404, '任务不存在');
      }
      if (existing.status !== 'DRAFT') {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }

      const nextTitle =
        typeof record.title === 'string' ? record.title : existing.title;
      const nextDescription =
        typeof record.description === 'string' ? record.description : existing.description;
      const nextAcceptance =
        typeof record.acceptance_criteria === 'string'
          ? record.acceptance_criteria
          : existing.acceptance_criteria;
      if (!nextTitle.trim()) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, '任务标题不能为空');
      }

      return withTransaction(() => {
        const timestamp = nowIso();
        if (record.blocked_by !== undefined && record.blocked_by !== null) {
          replaceDependencies(id, record.blocked_by, entry.summary.id);
        }
        const updated: BountyTask = {
          ...clone(existing),
          title: nextTitle,
          description: nextDescription,
          acceptance_criteria: nextAcceptance,
          draft_needs_human_review: true,
          updated_at: timestamp,
        };
        bounties.set(id, updated);
        const hydrated = hydrateBounty(updated);
        recordEvent(hydrated, 'draft_revised', `agent:${entry.summary.id}`, 'DRAFT', {});
        return redactBountyForAgent(hydrated, entry.summary.id);
      });
    },

    async agentSplitDraft(
      apiKey: string,
      id: string,
      input: SplitDraftInput,
    ): Promise<SplitDraftResult> {
      const entry = authenticateAgent(apiKey);
      const record = validateJsonObjectKeys(input, SPLIT_DRAFT_ALLOWED_KEYS);
      if (!Array.isArray(record.items)) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'items 必须为数组');
      }
      const existing = requireBounty(id);
      if (existing.draft_collaborator_agent_id !== entry.summary.id) {
        throw new ServiceApiError('BOUNTY_NOT_FOUND', 404, '任务不存在');
      }
      if (existing.status !== 'DRAFT') {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
      const rawItems = record.items as unknown[];
      if (rawItems.length === 0) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'items 不能为空');
      }
      requireActiveProject(existing.project_id);

      return withTransaction(() => {
        const timestamp = nowIso();
        const actor = `agent:${entry.summary.id}`;
        const childIds = rawItems.map(() => crypto.randomUUID());
        const keyMap = new Map<string, string>();
        const parsedItems: SplitDraftItem[] = [];

        for (let i = 0; i < rawItems.length; i++) {
          const itemRecord = validateJsonObjectKeys(
            rawItems[i],
            SPLIT_DRAFT_ITEM_ALLOWED_KEYS,
          );
          if (typeof itemRecord.title !== 'string') {
            throw new ServiceApiError('VALIDATION_ERROR', 400, 'title 必须为字符串');
          }
          if (
            itemRecord.description !== undefined &&
            typeof itemRecord.description !== 'string'
          ) {
            throw new ServiceApiError('VALIDATION_ERROR', 400, 'description 必须为字符串');
          }
          if (
            itemRecord.acceptance_criteria !== undefined &&
            typeof itemRecord.acceptance_criteria !== 'string'
          ) {
            throw new ServiceApiError('VALIDATION_ERROR', 400, 'acceptance_criteria 必须为字符串');
          }
          if (
            itemRecord.require_arbitration !== undefined &&
            typeof itemRecord.require_arbitration !== 'boolean'
          ) {
            throw new ServiceApiError('VALIDATION_ERROR', 400, 'require_arbitration 必须为布尔值');
          }
          if (
            itemRecord.key !== undefined &&
            itemRecord.key !== null &&
            typeof itemRecord.key !== 'string'
          ) {
            throw new ServiceApiError('VALIDATION_ERROR', 400, 'key 必须为字符串');
          }
          if (typeof itemRecord.difficulty !== 'string') {
            throw new ServiceApiError('VALIDATION_ERROR', 400, 'difficulty 必须为 C、B、A 或 S');
          }
          if (typeof itemRecord.assignee_mode !== 'string') {
            throw new ServiceApiError('VALIDATION_ERROR', 400, 'assignee_mode 不合法');
          }
          if (
            itemRecord.assignee_min_tier !== undefined &&
            itemRecord.assignee_min_tier !== null &&
            typeof itemRecord.assignee_min_tier !== 'string'
          ) {
            throw new ServiceApiError('VALIDATION_ERROR', 400, 'assignee_min_tier 必须为 C、B、A 或 S');
          }
          if (
            itemRecord.assignee_agent_id !== undefined &&
            itemRecord.assignee_agent_id !== null &&
            typeof itemRecord.assignee_agent_id !== 'string'
          ) {
            throw new ServiceApiError('VALIDATION_ERROR', 400, 'assignee_agent_id 必须为字符串或 null');
          }

          const title = itemRecord.title;
          const difficulty = itemRecord.difficulty as BountyDifficulty;
          const assigneeMode = itemRecord.assignee_mode as AssigneeMode;
          const minTier =
            (itemRecord.assignee_min_tier as BountyDifficulty | null | undefined) ?? null;
          const agentId = (itemRecord.assignee_agent_id as string | null | undefined) ?? null;
          validateBountyFields(title, difficulty, assigneeMode, minTier, agentId);

          if (typeof itemRecord.key === 'string') {
            const trimmedKey = itemRecord.key.trim();
            if (!trimmedKey || keyMap.has(trimmedKey)) {
              throw new ServiceApiError('VALIDATION_ERROR', 400, 'key 去空白后须非空且批内唯一');
            }
            if (bounties.has(trimmedKey)) {
              throw new ServiceApiError('VALIDATION_ERROR', 400, 'key 不能与现有任务 ID 冲突');
            }
            keyMap.set(trimmedKey, childIds[i]);
          }

          if (
            itemRecord.blocked_by !== undefined &&
            (!Array.isArray(itemRecord.blocked_by) ||
              itemRecord.blocked_by.some(dep => typeof dep !== 'string'))
          ) {
            throw new ServiceApiError('VALIDATION_ERROR', 400, 'blocked_by 必须为字符串数组');
          }

          parsedItems.push({
            key: (itemRecord.key as string | null | undefined) ?? null,
            title,
            description: (itemRecord.description as string | undefined) ?? '',
            acceptance_criteria:
              (itemRecord.acceptance_criteria as string | undefined) ?? '',
            difficulty,
            assignee_mode: assigneeMode,
            assignee_min_tier: minTier,
            assignee_agent_id: agentId,
            require_arbitration: (itemRecord.require_arbitration as boolean | undefined) ?? false,
            blocked_by: (itemRecord.blocked_by as string[] | undefined) ?? [],
          });
        }

        for (let i = 0; i < parsedItems.length; i++) {
          const item = parsedItems[i];
          const childId = childIds[i];
          const child: BountyTask = {
            id: childId,
            project_id: existing.project_id,
            title: item.title,
            description: item.description ?? '',
            acceptance_criteria: item.acceptance_criteria ?? '',
            difficulty: item.difficulty,
            assignee_mode: item.assignee_mode,
            assignee_min_tier: item.assignee_min_tier ?? null,
            assignee_agent_id: item.assignee_agent_id ?? null,
            assignee_agent_name: null,
            assignee_agent_tier: null,
            assignment_expires_at: null,
            require_arbitration: item.require_arbitration ?? false,
            status: 'DRAFT',
            created_by: actor,
            created_by_agent_id: entry.summary.id,
            created_by_agent_name: entry.summary.name,
            draft_collaborator_agent_id: entry.summary.id,
            draft_collaborator_agent_name: entry.summary.name,
            draft_collaborator_agent_tier: entry.summary.tier,
            draft_needs_human_review: true,
            parent_bounty_id: id,
            blocked_by: [],
            blocked_by_summaries: [],
            is_blocked: false,
            lease_id: 0,
            lease_agent_id: null,
            lease_agent_name: null,
            lease_agent_tier: null,
            lease_expires_at: null,
            arbitrator_agent_id: null,
            arbitrator_agent_name: null,
            arbitrator_agent_tier: null,
            arbitration_expires_at: null,
            quality_grade: null,
            calibrated_difficulty: null,
            active_duration_ms: 0,
            active_started_at: null,
            comment_count: 0,
            rejection_count: 0,
            completion_note: null,
            completion_cost: null,
            published_at: null,
            withdrawn_at: null,
            cancelled_at: null,
            completed_at: null,
            created_at: timestamp,
            updated_at: timestamp,
          };
          bounties.set(childId, child);
          eventsByBounty.set(childId, []);
          commentsByBounty.set(childId, []);
          arbitrationTriedIdsByBounty.set(childId, []);
        }

        for (let i = 0; i < parsedItems.length; i++) {
          const item = parsedItems[i];
          const childId = childIds[i];
          const resolvedEdges = (item.blocked_by ?? []).map(
            edge => keyMap.get(edge.trim()) ?? edge,
          );
          replaceDependencies(childId, resolvedEdges, entry.summary.id);
        }

        const children: BountyTask[] = [];
        for (const childId of childIds) {
          const child = hydrateBounty(requireBounty(childId));
          recordEvent(child, 'created', actor, null, { parent_bounty_id: id });
          children.push(redactBountyForAgent(child, entry.summary.id));
        }

        const updatedParentRaw: BountyTask = {
          ...clone(requireBounty(id)),
          draft_needs_human_review: true,
          updated_at: timestamp,
        };
        bounties.set(id, updatedParentRaw);
        const updatedParent = hydrateBounty(updatedParentRaw);
        recordEvent(updatedParent, 'draft_split', actor, 'DRAFT', {
          child_bounty_ids: childIds,
        });
        return {
          parent: redactBountyForAgent(updatedParent, entry.summary.id),
          children,
        };
      });
    },

    async agentListBounties(apiKey: string, query?: BountyQuery): Promise<BountyTask[]> {
      const entry = authenticateAgent(apiKey);
      const safeQuery: BountyQuery = {
        ...query,
        include_drafts: true,
      };
      return filterBountiesInternal(safeQuery)
        .filter(bounty => isBountyVisibleToAgent(entry.summary, bounty))
        .map(bounty => redactBountyForAgent(bounty, entry.summary.id));
    },

    async agentGetBountyDetail(apiKey: string, id: string): Promise<BountyDetail> {
      const entry = authenticateAgent(apiKey);
      const bounty = requireBounty(id);
      if (!isBountyVisibleToAgent(entry.summary, bounty)) {
        throw new ServiceApiError('BOUNTY_NOT_FOUND', 404, '任务不存在');
      }
      const events = [...(eventsByBounty.get(id) ?? [])].sort((a, b) => a.id - b.id);
      const blocks = relatedSummaries(id, false).filter(summary => {
        const raw = bounties.get(summary.id);
        return raw !== undefined && isBountyVisibleToAgent(entry.summary, raw);
      });
      const child_bounties = relatedSummaries(id, true).filter(summary => {
        const raw = bounties.get(summary.id);
        return raw !== undefined && isBountyVisibleToAgent(entry.summary, raw);
      });
      return {
        bounty: redactBountyForAgent(hydrateBounty(bounty), entry.summary.id),
        events: clone(events),
        comments: listHydratedComments(id),
        blocks,
        child_bounties,
      };
    },

    async agentListComments(apiKey: string, id: string): Promise<BountyComment[]> {
      const entry = authenticateAgent(apiKey);
      const bounty = requireBounty(id);
      if (!isBountyVisibleToAgent(entry.summary, bounty)) {
        throw new ServiceApiError('BOUNTY_NOT_FOUND', 404, '任务不存在');
      }
      return listHydratedComments(id);
    },

    async agentCreateComment(
      apiKey: string,
      id: string,
      input: CreateBountyCommentInput,
    ): Promise<BountyCommentMutationResult> {
      const entry = authenticateAgent(apiKey);
      const record = validateJsonObjectKeys(input, CREATE_COMMENT_ALLOWED_KEYS);
      if (typeof record.body !== 'string') {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'body 必须为字符串');
      }
      const replyToId =
        record.reply_to_comment_id === undefined || record.reply_to_comment_id === null
          ? null
          : typeof record.reply_to_comment_id === 'number' &&
              Number.isSafeInteger(record.reply_to_comment_id)
            ? record.reply_to_comment_id
            : (() => {
                throw new ServiceApiError('VALIDATION_ERROR', 400, 'reply_to_comment_id 必须为整数');
              })();
      const bounty = requireBounty(id);
      if (!isBountyVisibleToAgent(entry.summary, bounty)) {
        throw new ServiceApiError('BOUNTY_NOT_FOUND', 404, '任务不存在');
      }
      return addComment(bounty, entry.summary.id, entry.summary.name, record.body, replyToId);
    },

    async agentClaimBounty(apiKey: string, id: string): Promise<BountyTask> {
      const entry = authenticateAgent(apiKey);
      const existing = requirePublishedBounty(id);
      if (existing.status === 'IN_PROGRESS' || existing.lease_agent_id !== null) {
        throw new ServiceApiError('BOUNTY_ALREADY_CLAIMED', 409, '任务已被接取，不能撤回或重复接取');
      }
      if (existing.status !== 'OPEN') {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
      if (hydrateBounty(existing).is_blocked) {
        throw new ServiceApiError(
          'BOUNTY_BLOCKED',
          409,
          '前置依赖任务尚未全部完成，暂不可抢单',
        );
      }
      if (
        existing.assignee_mode === 'specific' &&
        existing.assignee_agent_id !== entry.summary.id
      ) {
        throw new ServiceApiError('ASSIGNEE_MISMATCH', 403, 'Agent 不是被指派者');
      }
      if (
        existing.assignee_mode === 'min_tier' &&
        tierRank(entry.summary.tier) < tierRank(existing.assignee_min_tier ?? 'C')
      ) {
        throw new ServiceApiError('TIER_INSUFFICIENT', 403, 'Agent 段位不足');
      }
      if (existing.require_arbitration) {
        const active = resolveArbitrationCandidates(existing);
        if (active.length === 1 && active[0] === entry.summary.id) {
          throw new ServiceApiError(
            'ARBITRATOR_CONFLICT',
            403,
            '唯一仲裁者不得接取开启仲裁的任务；请增配裁判或由发布者调整为人工/无仲裁',
          );
        }
      }
      const timestamp = nowIso();
      const updated: BountyTask = {
        ...clone(existing),
        status: 'IN_PROGRESS',
        active_duration_ms: 0,
        active_started_at: timestamp,
        lease_id: existing.lease_id + 1,
        lease_agent_id: entry.summary.id,
        lease_expires_at: deadlineIso(),
        assignment_expires_at: null,
        rejection_count: 0,
        completion_note: null,
        completion_cost: null,
        arbitrator_agent_id: null,
        arbitrator_agent_name: null,
        arbitrator_agent_tier: null,
        arbitration_expires_at: null,
        updated_at: timestamp,
      };
      bounties.set(id, updated);
      leaseWarnedIdByBounty.set(id, 0);
      arbitrationTriedIdsByBounty.set(id, []);
      const hydrated = hydrateBounty(updated);
      recordEvent(hydrated, 'claimed', `agent:${entry.summary.id}`, 'OPEN', leaseDetail(hydrated));
      return hydrated;
    },

    async agentDeclineBounty(
      apiKey: string,
      id: string,
      input?: DeclineBountyInput,
    ): Promise<BountyTask> {
      const entry = authenticateAgent(apiKey);
      let reason: string | null = null;
      if (input !== undefined) {
        const record = validateJsonObjectKeys(input, DECLINE_ALLOWED_KEYS);
        if (record.reason !== undefined && record.reason !== null) {
          if (typeof record.reason !== 'string') {
            throw new ServiceApiError('VALIDATION_ERROR', 400, 'reason 必须为字符串');
          }
          reason = record.reason;
        }
      }
      const existing = requirePublishedBounty(id);
      if (existing.assignee_mode !== 'specific' || existing.status !== 'OPEN') {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
      if (existing.assignee_agent_id !== entry.summary.id) {
        throw new ServiceApiError('ASSIGNEE_MISMATCH', 403, 'Agent 不是被指派者');
      }
      if (hydrateBounty(existing).is_blocked) {
        throw new ServiceApiError(
          'BOUNTY_BLOCKED',
          409,
          '前置依赖任务尚未全部完成，暂不可抢单',
        );
      }
      const timestamp = nowIso();
      const updated: BountyTask = {
        ...clone(existing),
        assignee_mode: 'any',
        assignee_agent_id: null,
        assignee_agent_name: null,
        assignee_agent_tier: null,
        assignee_min_tier: null,
        assignment_expires_at: null,
        updated_at: timestamp,
      };
      bounties.set(id, updated);
      const hydrated = hydrateBounty(updated);
      const detail = {
        declined_by_agent_id: entry.summary.id,
        declined_by_agent_name: entry.summary.name,
        reason,
      };
      recordEvent(hydrated, 'declined', `agent:${entry.summary.id}`, existing.status, detail);
      notifyActiveAgent(
        hydrated.created_by_agent_id,
        hydrated,
        'task.assignment_declined',
        detail,
      );
      dispatchBountyEvents(hydrated, false);
      return hydrated;
    },

    async agentHeartbeatBounty(
      apiKey: string,
      id: string,
      input: LeaseHeartbeatInput,
    ): Promise<BountyTask> {
      const entry = authenticateAgent(apiKey);
      const record = validateJsonObjectKeys(input, HEARTBEAT_ALLOWED_KEYS);
      if (typeof record.lease_id !== 'number' || !Number.isSafeInteger(record.lease_id)) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'lease_id 必须为整数');
      }
      const existing = requireFencedBounty(id, entry.summary.id, record.lease_id);
      const timestamp = nowIso();
      const updated: BountyTask = {
        ...clone(existing),
        lease_expires_at: deadlineIso(),
        updated_at: timestamp,
      };
      bounties.set(id, updated);
      leaseWarnedIdByBounty.set(id, 0);
      return hydrateBounty(updated);
    },

    async agentRequestInfo(
      apiKey: string,
      id: string,
      input: RequestInfoInput,
    ): Promise<BountyTask> {
      const entry = authenticateAgent(apiKey);
      const record = validateJsonObjectKeys(input, REQUEST_INFO_ALLOWED_KEYS);
      if (typeof record.lease_id !== 'number' || !Number.isSafeInteger(record.lease_id)) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'lease_id 必须为整数');
      }
      if (typeof record.question !== 'string') {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'question 必须为字符串');
      }
      const existing = requireFencedBounty(id, entry.summary.id, record.lease_id);
      const body = record.question.trim();
      if (!body) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, '评论或问题正文不能为空');
      }
      const timestamp = nowIso();
      const activeDurationMs = settleActive(existing, timestamp);
      const updated: BountyTask = {
        ...clone(existing),
        status: 'WAITING_INFO',
        active_duration_ms: activeDurationMs,
        active_started_at: null,
        lease_expires_at: null,
        updated_at: timestamp,
      };
      bounties.set(id, updated);
      leaseWarnedIdByBounty.set(id, 0);
      const comment: BountyComment = {
        id: nextCommentId++,
        bounty_id: id,
        author_type: 'agent',
        author_id: entry.summary.id,
        author_name: entry.summary.name,
        kind: 'question',
        body,
        created_at: timestamp,
      };
      const existingComments = commentsByBounty.get(id) ?? [];
      existingComments.push(comment);
      commentsByBounty.set(id, existingComments);

      const hydrated = hydrateBounty(updated);
      recordEvent(hydrated, 'info_requested', `agent:${entry.summary.id}`, existing.status, {
        lease_id: hydrated.lease_id,
        comment_id: comment.id,
        question: body,
      });
      if (hydrated.created_by_agent_id !== entry.summary.id) {
        notifyActiveAgent(
          hydrated.created_by_agent_id,
          hydrated,
          'task.info_requested',
          commentPayload(hydrated, comment),
        );
      }
      return hydrated;
    },

    async agentAbandonBounty(
      apiKey: string,
      id: string,
      input: AbandonBountyInput,
    ): Promise<BountyTask> {
      const entry = authenticateAgent(apiKey);
      const record = validateJsonObjectKeys(input, ABANDON_ALLOWED_KEYS);
      if (typeof record.lease_id !== 'number' || !Number.isSafeInteger(record.lease_id)) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'lease_id 必须为整数');
      }
      let reason: string | null = null;
      if (record.reason !== undefined && record.reason !== null) {
        if (typeof record.reason !== 'string') {
          throw new ServiceApiError('VALIDATION_ERROR', 400, 'reason 必须为字符串');
        }
        reason = record.reason;
      }
      const existing = requireFencedBounty(id, entry.summary.id, record.lease_id);
      const timestamp = nowIso();
      const activeDurationMs = settleActive(existing, timestamp);
      settleRating(
        {
          ...clone(existing),
          active_duration_ms: activeDurationMs,
        },
        entry,
        'abandon',
        existing.require_arbitration,
        timestamp,
      );
      const updated = reopenFromLease(existing, timestamp);
      entry.summary.abandon_count += 1;
      entry.summary.updated_at = timestamp;
      recordEvent(updated, 'abandoned', `agent:${entry.summary.id}`, existing.status, {
        lease_id: existing.lease_id,
        lease_agent_id: entry.summary.id,
        reason,
        active_duration_ms: activeDurationMs,
      });
      dispatchBountyEvents(updated, false);
      return updated;
    },

    async agentCompleteBounty(
      apiKey: string,
      id: string,
      input: CompleteBountyInput,
    ): Promise<BountyTask> {
      const entry = authenticateAgent(apiKey);
      const record = validateJsonObjectKeys(input, COMPLETE_ALLOWED_KEYS);
      if (typeof record.lease_id !== 'number' || !Number.isSafeInteger(record.lease_id)) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'lease_id 必须为整数');
      }
      if (
        record.completion_note !== undefined &&
        record.completion_note !== null &&
        typeof record.completion_note !== 'string'
      ) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'completion_note 必须为字符串');
      }
      if (
        record.completion_cost !== undefined &&
        record.completion_cost !== null &&
        typeof record.completion_cost !== 'number'
      ) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'completion_cost 必须为数字');
      }
      const existing = requireFencedBounty(id, entry.summary.id, record.lease_id);
      if (existing.require_arbitration) {
        throw new ServiceApiError('ARBITRATION_REQUIRED', 409, '任务需要仲裁，不能直接完成');
      }
      const rawCost = record.completion_cost ?? null;
      if (rawCost !== null && (!Number.isFinite(rawCost) || rawCost < 0)) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'completion_cost 必须为有限非负数');
      }
      const rawNote =
        typeof record.completion_note === 'string' ? record.completion_note.trim() : '';
      const completionNote = rawNote.length > 0 ? rawNote : null;
      const timestamp = nowIso();
      const activeDurationMs = settleActive(existing, timestamp);
      const updated: BountyTask = {
        ...clone(existing),
        status: 'COMPLETED',
        active_duration_ms: activeDurationMs,
        active_started_at: null,
        completion_note: completionNote,
        completion_cost: rawCost,
        completed_at: timestamp,
        lease_expires_at: null,
        updated_at: timestamp,
      };
      bounties.set(id, updated);
      leaseWarnedIdByBounty.set(id, 0);
      settleRating(updated, entry, 'completed', false, timestamp);
      const hydrated = hydrateBounty(updated);
      recordEvent(hydrated, 'completed', `agent:${entry.summary.id}`, existing.status, {
        lease_id: hydrated.lease_id,
        lease_agent_id: entry.summary.id,
        lease_agent_name: hydrated.lease_agent_name,
        completion_note: hydrated.completion_note,
        completion_cost: hydrated.completion_cost,
      });
      unlockDependents(id, timestamp);
      return hydrated;
    },

    async agentSubmitArbitration(
      apiKey: string,
      id: string,
      input: SubmitArbitrationInput,
    ): Promise<BountyTask> {
      const entry = authenticateAgent(apiKey);
      const record = validateJsonObjectKeys(input, SUBMIT_ARBITRATION_ALLOWED_KEYS);
      if (typeof record.lease_id !== 'number' || !Number.isSafeInteger(record.lease_id)) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'lease_id 必须为整数');
      }
      if (
        record.completion_note !== undefined &&
        record.completion_note !== null &&
        typeof record.completion_note !== 'string'
      ) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'completion_note 必须为字符串');
      }
      if (
        record.completion_cost !== undefined &&
        record.completion_cost !== null &&
        typeof record.completion_cost !== 'number'
      ) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'completion_cost 必须为数字');
      }
      const existing = requireFencedBounty(id, entry.summary.id, record.lease_id);
      if (!existing.require_arbitration) {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
      const rawCost = record.completion_cost ?? null;
      if (rawCost !== null && (!Number.isFinite(rawCost) || rawCost < 0)) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'completion_cost 必须为有限非负数');
      }
      const rawNote =
        typeof record.completion_note === 'string' ? record.completion_note.trim() : '';
      const completionNote = rawNote.length > 0 ? rawNote : null;
      const timestamp = nowIso();
      const activeDurationMs = settleActive(existing, timestamp);
      const judge =
        resolveArbitrationCandidates(existing).find(candidateId => candidateId !== entry.summary.id) ??
        null;
      const expires = judge ? deadlineIso(Date.parse(timestamp)) : null;
      const tried = judge ? [judge] : [];
      const updated: BountyTask = {
        ...clone(existing),
        status: 'PENDING_ARBITRATION',
        active_duration_ms: activeDurationMs,
        active_started_at: null,
        lease_expires_at: null,
        completion_note: completionNote,
        completion_cost: rawCost,
        arbitrator_agent_id: judge,
        arbitration_expires_at: expires,
        updated_at: timestamp,
      };
      bounties.set(id, updated);
      leaseWarnedIdByBounty.set(id, 0);
      arbitrationTriedIdsByBounty.set(id, tried);
      const hydrated = hydrateBounty(updated);
      const detail = {
        lease_id: hydrated.lease_id,
        arbitrator_agent_id: judge,
        arbitration_expires_at: expires,
      };
      recordEvent(
        hydrated,
        'arbitration_requested',
        `agent:${entry.summary.id}`,
        existing.status,
        detail,
      );
      notifyActiveAgent(judge, hydrated, 'arbitration.requested', {
        ...detail,
        bounty: hydrated,
      });
      return hydrated;
    },

    async agentApproveArbitration(
      apiKey: string,
      id: string,
      input?: ApproveArbitrationInput,
    ): Promise<BountyTask> {
      const entry = authenticateAgent(apiKey);
      return approveArbitrationInternal(id, entry.summary.id, input);
    },

    async agentRejectArbitration(
      apiKey: string,
      id: string,
      input: RejectArbitrationInput,
    ): Promise<BountyTask> {
      const entry = authenticateAgent(apiKey);
      return rejectArbitrationInternal(id, entry.summary.id, input);
    },

    async sweepLeases(nowMs = Date.now()): Promise<void> {
      const warningUntilMs = nowMs + LEASE_WARNING_WINDOW_MS;
      const candidates = Array.from(bounties.values())
        .filter(bounty => {
          if (bounty.status === 'OPEN' && bounty.assignee_mode === 'specific') {
            return (
              bounty.assignment_expires_at !== null &&
              Date.parse(bounty.assignment_expires_at) <= nowMs
            );
          }
          if (bounty.status === 'IN_PROGRESS' && bounty.lease_expires_at !== null) {
            const expiresMs = Date.parse(bounty.lease_expires_at);
            if (expiresMs <= nowMs) return true;
            if (
              expiresMs <= warningUntilMs &&
              (leaseWarnedIdByBounty.get(bounty.id) ?? 0) < bounty.lease_id
            ) {
              return true;
            }
          }
          return false;
        })
        .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));

      for (const bounty of candidates) {
        const timestamp = nowIso();
        if (bounty.status === 'OPEN') {
          const oldAssignee = hydrateBounty(bounty);
          const updated: BountyTask = {
            ...clone(bounty),
            assignee_mode: 'any',
            assignee_agent_id: null,
            assignee_agent_name: null,
            assignee_agent_tier: null,
            assignee_min_tier: null,
            assignment_expires_at: null,
            updated_at: timestamp,
          };
          bounties.set(bounty.id, updated);
          const hydrated = hydrateBounty(updated);
          const detail = {
            assignee_agent_id: oldAssignee.assignee_agent_id,
            assignee_agent_name: oldAssignee.assignee_agent_name,
          };
          recordEvent(hydrated, 'assignment_expired', 'system', bounty.status, detail);
          notifyActiveAgent(
            oldAssignee.created_by_agent_id,
            hydrated,
            'task.assignment_expired',
            detail,
          );
          dispatchBountyEvents(hydrated, false);
        } else if (
          bounty.lease_expires_at !== null &&
          Date.parse(bounty.lease_expires_at) <= nowMs
        ) {
          const holder = requireRatingAgent(bounty);
          const oldLease = hydrateBounty(bounty);
          const activeDurationMs = settleActive(bounty, timestamp);
          settleRating(
            {
              ...clone(bounty),
              active_duration_ms: activeDurationMs,
            },
            holder,
            'timeout',
            bounty.require_arbitration,
            timestamp,
          );
          const updated = reopenFromLease(bounty, timestamp);
          holder.summary.timeout_count += 1;
          holder.summary.updated_at = timestamp;
          const detail = {
            ...leaseDetail(oldLease),
            active_duration_ms: activeDurationMs,
          };
          recordEvent(updated, 'lease_expired', 'system', bounty.status, detail);
          notifyActiveAgent(oldLease.lease_agent_id, updated, 'lease.expired', detail);
          dispatchBountyEvents(updated, false);
        } else {
          leaseWarnedIdByBounty.set(bounty.id, bounty.lease_id);
          const hydrated = hydrateBounty(bounty);
          notifyActiveAgent(
            hydrated.lease_agent_id,
            hydrated,
            'lease.expiring',
            leaseDetail(hydrated),
          );
        }
      }

      const pendingArbitrations = Array.from(bounties.values())
        .filter(
          bounty =>
            bounty.status === 'PENDING_ARBITRATION' &&
            bounty.arbitrator_agent_id !== null &&
            bounty.arbitration_expires_at !== null &&
            Date.parse(bounty.arbitration_expires_at) <= nowMs,
        )
        .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));

      for (const bounty of pendingArbitrations) {
        const timestamp = nowIso();
        const previousJudgeId = bounty.arbitrator_agent_id;
        const tried = [...(arbitrationTriedIdsByBounty.get(bounty.id) ?? [])];
        const nextJudge =
          resolveArbitrationCandidates(bounty).find(
            candidateId =>
              candidateId !== bounty.lease_agent_id &&
              !tried.includes(candidateId) &&
              candidateId !== previousJudgeId,
          ) ?? null;
        if (previousJudgeId !== null && !tried.includes(previousJudgeId)) {
          tried.push(previousJudgeId);
        }
        if (nextJudge !== null && !tried.includes(nextJudge)) {
          tried.push(nextJudge);
        }
        const expires = nextJudge !== null ? deadlineIso(nowMs) : null;
        const updated: BountyTask = {
          ...clone(bounty),
          arbitrator_agent_id: nextJudge,
          arbitrator_agent_name: null,
          arbitrator_agent_tier: null,
          arbitration_expires_at: expires,
          updated_at: timestamp,
        };
        bounties.set(bounty.id, updated);
        arbitrationTriedIdsByBounty.set(bounty.id, tried);
        const hydrated = hydrateBounty(updated);
        const detail = {
          lease_id: bounty.lease_id,
          previous_arbitrator_agent_id: previousJudgeId,
          arbitrator_agent_id: nextJudge,
          arbitration_expires_at: expires,
        };
        recordEvent(
          hydrated,
          nextJudge !== null ? 'arbitration_reassigned' : 'arbitration_human_fallback',
          'system',
          bounty.status,
          detail,
        );
        notifyActiveAgent(previousJudgeId, hydrated, 'arbitration.timed_out', {
          ...detail,
          bounty: hydrated,
        });
        notifyActiveAgent(nextJudge, hydrated, 'arbitration.requested', {
          ...detail,
          bounty: hydrated,
        });
      }
    },

    async agentGetWebhookConfig(apiKey: string): Promise<AgentWebhookConfig> {
      const entry = authenticateAgent(apiKey);
      return toWebhookConfig(entry);
    },

    async agentUpdateWebhookConfig(
      apiKey: string,
      input: UpdateAgentWebhookInput,
    ): Promise<AgentWebhookConfig> {
      const entry = authenticateAgent(apiKey);
      if (!input || input.webhook_url === undefined) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'webhook_url 为必填字段');
      }
      const oldSecret = entry.webhookSecret;
      const reveal =
        input.webhook_url !== null &&
        ((input.webhook_secret !== undefined && input.webhook_secret !== null) || oldSecret === null);
      const { webhookUrl, webhookSecret } = resolveWebhookValues(
        input.webhook_url,
        input.webhook_secret,
        oldSecret,
      );
      entry.summary.webhook_url = webhookUrl;
      entry.webhookSecret = webhookSecret;
      entry.summary.updated_at = nowIso();
      const config = toWebhookConfig(entry);
      if (reveal && webhookSecret !== null) {
        config.webhook_secret = webhookSecret;
      }
      return config;
    },

    async agentListEvents(
      apiKey: string,
      options?: { cursor?: number; limit?: number },
    ): Promise<AgentInboxResponse> {
      const rawLimit = options?.limit ?? 100;
      if (typeof rawLimit !== 'number' || !Number.isInteger(rawLimit) || rawLimit <= 0) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'cursor 必须非负，limit 必须大于 0');
      }
      if (
        options?.cursor !== undefined &&
        (typeof options.cursor !== 'number' ||
          !Number.isInteger(options.cursor) ||
          options.cursor < 0)
      ) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'cursor 必须非负，limit 必须大于 0');
      }
      const entry = authenticateAgent(apiKey);
      const effectiveCursor = options?.cursor ?? entry.ackedEventId;
      const limit = Math.min(rawLimit, 500);
      const latestEventId =
        entry.inbox.length > 0 ? entry.inbox[entry.inbox.length - 1].event.event_id : 0;
      const matching = entry.inbox
        .map(item => item.event)
        .filter(ev => ev.event_id > effectiveCursor);
      const hasMore = matching.length > limit;
      const events = clone(matching.slice(0, limit));
      const cursor = events.length > 0 ? events[events.length - 1].event_id : effectiveCursor;
      return {
        events,
        cursor,
        acked_event_id: entry.ackedEventId,
        latest_event_id: latestEventId,
        has_more: hasMore,
      };
    },

    async agentAckEvents(apiKey: string, eventId: number): Promise<AckAgentEventsResult> {
      if (typeof eventId !== 'number' || !Number.isInteger(eventId) || eventId < 0) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'event_id 必须非负');
      }
      const entry = authenticateAgent(apiKey);
      const latestEventId =
        entry.inbox.length > 0 ? entry.inbox[entry.inbox.length - 1].event.event_id : 0;
      entry.ackedEventId = Math.max(entry.ackedEventId, Math.min(eventId, latestEventId));
      return {
        acked_event_id: entry.ackedEventId,
      };
    },

    async agentOpenStream(
      apiKey: string,
      listener: (event: AgentEvent) => void,
      options?: { cursor?: number },
    ): Promise<() => void> {
      if (
        options?.cursor !== undefined &&
        (typeof options.cursor !== 'number' ||
          !Number.isInteger(options.cursor) ||
          options.cursor < 0)
      ) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'cursor 必须非负');
      }
      const entry = authenticateAgent(apiKey);
      const connectionId = nextStreamConnectionId++;
      entry.streamListeners.set(connectionId, listener);
      const effectiveCursor = options?.cursor ?? entry.ackedEventId;
      const backlog = entry.inbox
        .map(item => item.event)
        .filter(ev => ev.event_id > effectiveCursor);
      if (backlog.length > 0) {
        entry.summary.last_pushed_at = nowIso();
        for (const ev of backlog) {
          listener(clone(ev));
        }
      }
      return () => {
        entry.streamListeners.delete(connectionId);
      };
    },

    async simulateAgentTier(
      agentId: string,
      tier: AgentTier,
      placementCompleted = true,
    ): Promise<AgentSummary> {
      const entry = requireStoredAgent(agentId);
      entry.summary.tier = tier;
      entry.summary.placement_completed = placementCompleted;
      if (!placementCompleted || tier === 'Unranked') {
        entry.settledCount = Math.min(entry.settledCount, 4);
        entry.mmr = 1000.0;
        entry.verifiedAsWins = 0;
        entry.verifiedSWins = 0;
      } else {
        entry.settledCount = Math.max(entry.settledCount, 5);
        switch (tier) {
          case 'C':
            entry.mmr = Math.min(entry.mmr, 1100.0);
            break;
          case 'B':
            entry.mmr = Math.max(entry.mmr, 1300.0);
            break;
          case 'A':
            entry.mmr = Math.max(entry.mmr, 1600.0);
            entry.verifiedAsWins = Math.max(entry.verifiedAsWins, 3);
            break;
          case 'S':
            entry.mmr = Math.max(entry.mmr, 1900.0);
            entry.verifiedAsWins = Math.max(entry.verifiedAsWins, 5);
            entry.verifiedSWins = Math.max(entry.verifiedSWins, 5);
            break;
        }
      }
      entry.summary.updated_at = nowIso();
      return toAgentSummary(entry);
    },

    async simulateAgentStream(agentId: string, connected: boolean): Promise<AgentSummary> {
      const entry = requireStoredAgent(agentId);
      if (connected && entry.summary.status !== 'active') {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
      entry.simulatedStreamConnected = connected;
      return toAgentSummary(entry);
    },

    async simulateAgentPush(
      agentId: string,
      options?: { pushedAt?: string | null; recentFailedCount?: number },
    ): Promise<AgentSummary> {
      const entry = requireStoredAgent(agentId);
      if (options?.pushedAt !== undefined) {
        entry.summary.last_pushed_at = options.pushedAt;
      } else if (options?.recentFailedCount === undefined) {
        entry.summary.last_pushed_at = nowIso();
      }
      if (options?.recentFailedCount !== undefined) {
        entry.simulatedRecentFailedCount = Math.max(0, options.recentFailedCount);
      }
      return toAgentSummary(entry);
    },
  };
}
