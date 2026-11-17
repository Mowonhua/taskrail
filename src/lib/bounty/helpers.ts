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
  type BountyProject,
  type BountyStatus,
  type BountyTask,
  type InviteTokenSummary,
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

export function formatAssigneeConstraint(bounty: Pick<BountyTask, 'assignee_mode' | 'assignee_min_tier' | 'assignee_agent_id'>): string {
  switch (bounty.assignee_mode) {
    case 'any':
      return translate('无要求');
    case 'min_tier':
      return translate('最低 {tier} 级', { tier: bounty.assignee_min_tier ?? 'C' });
    case 'specific':
      return translate('指派: {agent}', { agent: bounty.assignee_agent_id ?? '' });
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
  bounty?: Pick<BountyTask, 'created_by' | 'created_by_agent_id' | 'created_by_agent_name'>,
): string {
  if (actor === 'human') {
    return translate('管理员');
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
