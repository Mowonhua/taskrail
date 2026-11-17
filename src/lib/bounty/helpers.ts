/**
 * 文件职责：提供悬赏板视图、详情抽屉与表单共用的格式化辅助函数与 Markdown 渲染 Action。
 * 定义范围：状态文案映射、接单限制文案、时间格式化、服务错误消息转换及安全 Markdown/KaTeX DOM 挂载。
 */
import type { Action } from 'svelte/action';
import {
  ServiceApiError,
  type AgentChannelStatus,
  type AgentDraftPolicy,
  type AgentStatus,
  type AgentSummary,
  type AgentTier,
  type BountyCommentAuthorType,
  type BountyCommentKind,
  type BountyProject,
  type BountyStatus,
  type BountyTask,
  type InviteTokenSummary,
  type QualityGrade,
  type RatingOutcome,
} from '../api/types';
import { errorMessage } from '../errors';
import { translate } from '../i18n';
import { renderMarkdown } from '../markdown';

export const renderMarkdownAction: Action<HTMLElement, string> = (node, source) => {
  const update = (next: string) => {
    node.replaceChildren(renderMarkdown(next));
  };
  update(source);
  return { update };
};

export function formatBountyStatus(status: BountyStatus, bracketDraft = false): string {
  switch (status) {
    case 'DRAFT':
      return bracketDraft ? translate('[草稿]') : translate('草稿');
    case 'OPEN':
      return translate('待接单');
    case 'IN_PROGRESS':
      return translate('执行中');
    case 'WAITING_INFO':
      return translate('待回复');
    case 'PENDING_ARBITRATION':
      return translate('待仲裁');
    case 'COMPLETED':
      return translate('已完成');
    case 'CANCELLED':
      return translate('已取消');
  }
}

export function formatAgentStatus(status: AgentStatus): string {
  switch (status) {
    case 'active':
      return translate('正常接入');
    case 'disabled':
      return translate('已停用');
    case 'revoked':
      return translate('已吊销');
  }
}

export function formatAgentChannelStatus(
  agent: Pick<AgentSummary, 'channel_status' | 'stream_connected' | 'webhook_configured'>,
): { kind: AgentChannelStatus; label: string } {
  const kind: AgentChannelStatus =
    agent.stream_connected || agent.channel_status === 'streaming'
      ? 'streaming'
      : agent.webhook_configured || agent.channel_status === 'webhook'
        ? 'webhook'
        : 'offline';
  switch (kind) {
    case 'streaming':
      return { kind: 'streaming', label: translate('在线长连接') };
    case 'webhook':
      return { kind: 'webhook', label: 'Webhook' };
    case 'offline':
      return { kind: 'offline', label: translate('离线') };
  }
}

export function formatAgentTier(tier: AgentTier): string {
  if (tier === 'Unranked') {
    return translate('未定级');
  }
  return translate('{tier} 级', { tier });
}

export function formatInviteTokenStatus(
  token: InviteTokenSummary,
  nowMs = Date.now(),
): { kind: 'active' | 'revoked' | 'expired' | 'exhausted'; label: string } {
  if (token.revoked_at !== null) {
    return { kind: 'revoked', label: translate('已作废') };
  }
  if (token.expires_at !== null) {
    const expiresAtMs = Date.parse(token.expires_at);
    if (!Number.isNaN(expiresAtMs) && expiresAtMs <= nowMs) {
      return { kind: 'expired', label: translate('已过期') };
    }
  }
  if (token.max_uses !== null && token.use_count >= token.max_uses) {
    return { kind: 'exhausted', label: translate('已用尽') };
  }
  return { kind: 'active', label: translate('有效') };
}

export function formatAgentSubscriptionSummary(
  agent: AgentSummary,
  projectsById: ReadonlyMap<string, BountyProject>,
): string {
  if (agent.subscribe_all_projects) {
    return translate('全部项目');
  }
  if (agent.project_ids.length === 0) {
    return translate('未订阅项目');
  }
  return agent.project_ids
    .map(projectId => projectsById.get(projectId)?.name ?? projectId)
    .join(translate('、'));
}

export function formatAssigneeConstraint(
  bounty: Pick<BountyTask, 'assignee_mode' | 'assignee_min_tier' | 'assignee_agent_id'> &
    Partial<Pick<BountyTask, 'assignee_agent_name'>>,
): string {
  switch (bounty.assignee_mode) {
    case 'any':
      return translate('无要求');
    case 'min_tier':
      return translate('最低 {tier} 级', { tier: bounty.assignee_min_tier ?? 'C' });
    case 'specific':
      return translate('指派: {agent}', {
        agent: bounty.assignee_agent_name || bounty.assignee_agent_id || '',
      });
  }
}

export function formatBountyExecutor(
  bounty: Pick<BountyTask, 'lease_agent_id' | 'lease_agent_name' | 'lease_agent_tier'>,
): { agentId: string; name: string; tier: AgentTier | null; label: string } | null {
  if (!bounty.lease_agent_id) {
    return null;
  }
  const name = bounty.lease_agent_name || bounty.lease_agent_id;
  const tier = bounty.lease_agent_tier ?? null;
  return {
    agentId: bounty.lease_agent_id,
    name,
    tier,
    label: tier ? translate('{name}（{tier}）', { name, tier: formatAgentTier(tier) }) : name,
  };
}

export function formatBountyArbitrator(
  bounty: Pick<BountyTask, 'arbitrator_agent_id' | 'arbitrator_agent_name' | 'arbitrator_agent_tier'>,
): { agentId: string; name: string; tier: AgentTier | null; label: string } | null {
  if (!bounty.arbitrator_agent_id) {
    return null;
  }
  const name = bounty.arbitrator_agent_name || bounty.arbitrator_agent_id;
  const tier = bounty.arbitrator_agent_tier ?? null;
  return {
    agentId: bounty.arbitrator_agent_id,
    name,
    tier,
    label: tier ? translate('{name}（{tier}）', { name, tier: formatAgentTier(tier) }) : name,
  };
}

export function formatDraftCollaborator(
  bounty: Pick<
    BountyTask,
    'draft_collaborator_agent_id' | 'draft_collaborator_agent_name' | 'draft_collaborator_agent_tier'
  >,
): { agentId: string; name: string; tier: AgentTier | null; label: string } | null {
  if (!bounty.draft_collaborator_agent_id) {
    return null;
  }
  const name = bounty.draft_collaborator_agent_name || bounty.draft_collaborator_agent_id;
  const tier = bounty.draft_collaborator_agent_tier ?? null;
  return {
    agentId: bounty.draft_collaborator_agent_id,
    name,
    tier,
    label: tier ? translate('{name}（{tier}）', { name, tier: formatAgentTier(tier) }) : name,
  };
}

export function formatQualityGrade(grade: QualityGrade): string {
  switch (grade) {
    case 'perfect':
      return translate('完美交付');
    case 'flawed':
      return translate('有瑕疵');
  }
}

export interface LeaseCountdownInfo {
  expired: boolean;
  expiringSoon: boolean;
  remainingMs: number;
  label: string;
}

export function formatLeaseCountdown(
  expiresAt: string | null | undefined,
  nowMs = Date.now(),
): LeaseCountdownInfo | null {
  if (!expiresAt) {
    return null;
  }
  const expiresAtMs = Date.parse(expiresAt);
  if (Number.isNaN(expiresAtMs)) {
    return null;
  }
  const remainingMs = Math.max(0, expiresAtMs - nowMs);
  if (expiresAtMs <= nowMs) {
    return {
      expired: true,
      expiringSoon: true,
      remainingMs: 0,
      label: translate('已超时待回收'),
    };
  }
  const totalSeconds = Math.ceil(remainingMs / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const pad = (n: number) => String(n).padStart(2, '0');
  const label =
    hours > 0 ? `${hours}:${pad(minutes)}:${pad(seconds)}` : `${pad(minutes)}:${pad(seconds)}`;
  return {
    expired: false,
    expiringSoon: remainingMs <= 3 * 60 * 1000,
    remainingMs,
    label,
  };
}

export function computeActiveDurationMs(
  bounty: Pick<BountyTask, 'status' | 'active_duration_ms' | 'active_started_at'>,
  nowMs = Date.now(),
): number {
  const base = Math.max(0, Number.isFinite(bounty.active_duration_ms) ? bounty.active_duration_ms : 0);
  if (bounty.status === 'IN_PROGRESS' && bounty.active_started_at) {
    const startMs = Date.parse(bounty.active_started_at);
    if (!Number.isNaN(startMs)) {
      return base + Math.max(0, nowMs - startMs);
    }
  }
  return base;
}

export function formatActiveDuration(durationMs: number): string {
  const totalSeconds = Math.max(0, Math.floor(durationMs / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const pad = (n: number) => String(n).padStart(2, '0');
  return hours > 0
    ? `${hours}:${pad(minutes)}:${pad(seconds)}`
    : `${pad(minutes)}:${pad(seconds)}`;
}

export function formatCompletionCost(cost: number | null | undefined): string | null {
  if (cost === null || cost === undefined || !Number.isFinite(cost) || cost < 0) {
    return null;
  }
  return Number.isInteger(cost) ? String(cost) : Number(cost.toFixed(4)).toString();
}

export function formatCommentKind(kind: BountyCommentKind): string {
  switch (kind) {
    case 'question':
      return translate('提问阻塞');
    case 'reply':
      return translate('恢复回复');
    case 'status':
      return translate('状态记录');
    case 'comment':
    default:
      return translate('评论');
  }
}

export function formatCommentAuthorRole(authorType: BountyCommentAuthorType): string {
  switch (authorType) {
    case 'human':
      return translate('人类管理员');
    case 'agent':
      return 'Agent';
    case 'system':
      return translate('系统');
  }
}

export function formatEventLabel(eventType: string): string {
  switch (eventType) {
    case 'created':
      return translate('创建悬赏');
    case 'updated':
      return translate('更新草稿');
    case 'published':
      return translate('发布悬赏');
    case 'withdrawn':
      return translate('撤回为草稿');
    case 'cancelled':
      return translate('取消任务');
    case 'claimed':
      return translate('认领任务');
    case 'declined':
      return translate('婉拒指派');
    case 'abandoned':
      return translate('主动弃单');
    case 'completed':
      return translate('完成交付');
    case 'info_requested':
      return translate('提问挂起');
    case 'resumed':
      return translate('回复并恢复');
    case 'commented':
      return translate('发表评论');
    case 'lease_released':
      return translate('释放租约');
    case 'lease_expired':
      return translate('租约超时回收');
    case 'assignment_expired':
      return translate('指派超时回退');
    case 'arbitration_requested':
      return translate('提交仲裁');
    case 'arbitration_approved':
      return translate('仲裁通过');
    case 'arbitration_rejected':
      return translate('仲裁打回');
    case 'arbitration_failed':
      return translate('多次打回重开');
    case 'arbitration_reassigned':
      return translate('仲裁顺延改派');
    case 'arbitration_human_fallback':
      return translate('仲裁转人工兜底');
    case 'draft_invited':
      return translate('邀请草稿协作');
    case 'draft_collaborator_cleared':
      return translate('取消草稿协作');
    case 'draft_revised':
      return translate('协同修订草稿');
    case 'draft_split':
      return translate('派生拆分草稿');
    case 'dependency_unlocked':
      return translate('前置依赖解锁');
    default:
      return eventType;
  }
}

export function formatAgentDraftPolicy(policy: AgentDraftPolicy): string {
  switch (policy) {
    case 'open':
      return translate('直接公开');
    case 'draft':
      return translate('先入草稿待审');
  }
}

export function formatBountyPublisher(
  bounty: Pick<BountyTask, 'created_by' | 'created_by_agent_id' | 'created_by_agent_name'>,
): { isAgent: boolean; label: string } {
  if (bounty.created_by_agent_id || bounty.created_by.startsWith('agent:')) {
    const name =
      bounty.created_by_agent_name ||
      bounty.created_by_agent_id ||
      bounty.created_by.slice(6);
    return {
      isAgent: true,
      label: translate('Agent 提交: {name}', { name }),
    };
  }
  return {
    isAgent: false,
    label: translate('人工创建'),
  };
}

export function formatEventActor(
  actor: string,
  bounty?: Partial<
    Pick<
      BountyTask,
      | 'created_by'
      | 'created_by_agent_id'
      | 'created_by_agent_name'
      | 'lease_agent_id'
      | 'lease_agent_name'
      | 'assignee_agent_id'
      | 'assignee_agent_name'
      | 'arbitrator_agent_id'
      | 'arbitrator_agent_name'
    >
  >,
): string {
  if (actor === 'human') {
    return translate('管理员');
  }
  if (actor === 'system') {
    return translate('系统调度');
  }
  if (
    bounty?.lease_agent_id &&
    (actor === `agent:${bounty.lease_agent_id}` || actor === bounty.lease_agent_id)
  ) {
    return translate('Agent: {name}', {
      name: bounty.lease_agent_name || bounty.lease_agent_id,
    });
  }
  if (
    bounty?.arbitrator_agent_id &&
    (actor === `agent:${bounty.arbitrator_agent_id}` || actor === bounty.arbitrator_agent_id)
  ) {
    return translate('Agent: {name}', {
      name: bounty.arbitrator_agent_name || bounty.arbitrator_agent_id,
    });
  }
  if (
    bounty?.assignee_agent_id &&
    (actor === `agent:${bounty.assignee_agent_id}` || actor === bounty.assignee_agent_id)
  ) {
    return translate('Agent: {name}', {
      name: bounty.assignee_agent_name || bounty.assignee_agent_id,
    });
  }
  if (
    bounty?.created_by_agent_id &&
    (actor === `agent:${bounty.created_by_agent_id}` || actor === bounty.created_by_agent_id)
  ) {
    return translate('Agent: {name}', {
      name: bounty.created_by_agent_name || bounty.created_by_agent_id,
    });
  }
  if (actor.startsWith('agent:')) {
    return translate('Agent: {name}', { name: actor.slice(6) });
  }
  return actor;
}

export function formatTimestamp(iso: string | null | undefined): string {
  if (!iso) return '';
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

export function formatServiceError(error: unknown): string {
  if (error instanceof ServiceApiError) {
    switch (error.code) {
      case 'BOUNTY_ALREADY_CLAIMED':
        return translate('悬赏已被 Agent 接取，无法撤回为草稿。');
      case 'INVALID_TRANSITION':
        return translate('当前任务状态已变更，无法执行该操作。');
      case 'PROJECT_HAS_BOUNTIES':
        return translate('该项目下已有悬赏记录，无法直接删除，建议将其归档。');
      case 'PROJECT_ARCHIVED':
        return translate('归档项目不能创建、编辑或发布悬赏。');
      case 'PROJECT_NOT_FOUND':
        return translate('所属项目不存在。');
      case 'BOUNTY_NOT_FOUND':
        return translate('未找到该悬赏任务。');
      case 'AGENT_NOT_FOUND':
        return translate('未找到指定的 Agent。');
      case 'INVITE_TOKEN_NOT_FOUND':
        return translate('未找到指定的邀请令牌。');
      case 'INVITE_TOKEN_REQUIRED':
        return translate('需要提供注册邀请令牌。');
      case 'INVITE_TOKEN_INVALID':
        return translate('邀请令牌无效、已作废、已过期或已用尽。');
      case 'AGENT_REVOKED':
        return translate('该 Agent 凭证已被吊销，无法继续访问。');
      case 'AGENT_DISABLED':
        return translate('该 Agent 已被停用，请先启用后再试。');
      case 'AGENT_NAME_TAKEN':
        return translate('该 Agent 名称已被占用，请更换名称。');
      case 'ASSIGNEE_MISMATCH':
        return translate('仅被指派的 Agent 可执行此操作。');
      case 'TIER_INSUFFICIENT':
        return translate('当前 Agent 段位未达到该悬赏的最低接取要求。');
      case 'LEASE_OWNER_MISMATCH':
        return translate('当前租约不属于该 Agent，无法写入。');
      case 'STALE_LEASE':
        return translate('租约令牌已过期或租约已失效，请重新获取最新任务状态。');
      case 'ARBITRATION_REQUIRED':
        return translate('该任务已开启仲裁验收，不能直接标记为完成。');
      case 'ARBITRATOR_CONFLICT':
        return translate('唯一仲裁者不得接取开启仲裁的任务，请增配裁判或改由人工/无仲裁执行。');
      case 'ARBITRATOR_MISMATCH':
        return translate('仅当前指定的活跃仲裁者可执行该裁决操作。');
      case 'AGENT_INACTIVE':
        return translate('只能邀请活跃状态的 Agent 协助草稿。');
      case 'DEPENDENCY_CYCLE':
        return translate('存在自依赖或循环依赖，无法保存依赖关系。');
      case 'BOUNTY_BLOCKED':
        return translate('前置依赖任务尚未全部完成，暂不可抢单。');
      case 'UNAUTHORIZED':
        return translate('管理员凭证无效或未授权（401）。');
      case 'VALIDATION_ERROR':
        return error.message || translate('提交的字段内容未通过校验。');
      default:
        return error.message || `HTTP ${error.status}`;
    }
  }
  return errorMessage(error);
}

export function formatRatingOutcome(outcome: RatingOutcome): string {
  switch (outcome) {
    case 'completed':
      return translate('完成交付');
    case 'arbitration_failed':
      return translate('仲裁打回失败');
    case 'timeout':
      return translate('租约超时');
    case 'abandon':
      return translate('主动弃单');
  }
}

export function formatRatingCapReason(reason: string | null | undefined): string | null {
  if (!reason) return null;
  switch (reason) {
    case 'self_deal':
      return translate('自发自接封顶');
    case 'unarbitrated_cap':
      return translate('无仲裁自结上限');
    case 'difficulty_cap':
      return translate('难度收益上限');
    default:
      return reason;
  }
}

export function formatTierGateReason(reason: string | null | undefined): string | null {
  if (!reason) return null;
  switch (reason) {
    case 'needs_as_wins':
      return translate('待累计 3 场核验 A/S 胜场');
    case 'needs_s_wins':
      return translate('待累计 5 场核验 S 胜场');
    default:
      return reason;
  }
}

export function formatEfficiencyPercent(value: number): string {
  if (!Number.isFinite(value)) return '0%';
  const clamped = Math.min(1, Math.max(0, value));
  const pct = Math.round(clamped * 1000) / 10;
  return `${Number.isInteger(pct) ? String(pct) : pct.toFixed(1)}%`;
}

export function formatMmrValue(mmr: number): string {
  if (!Number.isFinite(mmr)) return '0.0';
  return Math.max(0, mmr).toFixed(1);
}

export function formatMmrDelta(delta: number): string {
  if (!Number.isFinite(delta) || Math.abs(delta) < 0.05) return '0.0';
  return delta > 0 ? `+${delta.toFixed(1)}` : delta.toFixed(1);
}
