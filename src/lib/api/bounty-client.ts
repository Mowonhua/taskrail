/**
 * 文件职责：提供悬赏服务客户端端口、HTTP 实现、嵌入式连接信息读取与内存模拟实现。
 * 约束条件：HTTP 客户端不向 /api/v1/health 发送管理员鉴权头；内存实现严格对齐 Rust 仓储状态机与错误码。
 */
import { invoke } from '@tauri-apps/api/core';
import { parseProjectIcon } from '../project-icons';
import {
  ServiceApiError,
  type AdminStatus,
  type AgentDraftPolicy,
  type AssigneeMode,
  type BountyDetail,
  type BountyDifficulty,
  type BountyEvent,
  type BountyProject,
  type BountyQuery,
  type BountyStatus,
  type BountyTask,
  type CreateBountyInput,
  type CreateProjectInput,
  type EmbeddedServerInfo,
  type ServerHealth,
  type UpdateBountyInput,
  type UpdateProjectInput,
} from './types';

const VALID_DIFFICULTIES = new Set<BountyDifficulty>(['C', 'B', 'A', 'S']);
const VALID_ASSIGNEE_MODES = new Set<AssigneeMode>(['any', 'min_tier', 'specific']);
const VALID_AGENT_POLICIES = new Set<AgentDraftPolicy>(['open', 'draft']);
const CANCELLABLE_STATUSES = new Set<BountyStatus>(['DRAFT', 'OPEN', 'WAITING_INFO']);

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
  createBounty(input: CreateBountyInput): Promise<BountyTask>;
  updateBounty(id: string, input: UpdateBountyInput): Promise<BountyTask>;
  publishBounty(id: string): Promise<BountyTask>;
  withdrawBounty(id: string): Promise<BountyTask>;
  cancelBounty(id: string): Promise<BountyTask>;
}

export interface MemoryBountyClient extends BountyApiPort {
  simulateClaim(id: string, agentId: string): Promise<BountyTask>;
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
    const qs = params.toString();
    return await this.request<BountyTask[]>('GET', qs ? `/api/v1/bounties?${qs}` : '/api/v1/bounties');
  }

  async getBountyDetail(id: string): Promise<BountyDetail> {
    return await this.request<BountyDetail>('GET', `/api/v1/bounties/${encodeURIComponent(id)}`);
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

  async withdrawBounty(id: string): Promise<BountyTask> {
    return await this.request<BountyTask>('POST', `/api/v1/bounties/${encodeURIComponent(id)}/withdraw`);
  }

  async cancelBounty(id: string): Promise<BountyTask> {
    return await this.request<BountyTask>('POST', `/api/v1/bounties/${encodeURIComponent(id)}/cancel`);
  }

  private async request<T>(
    method: string,
    path: string,
    options?: { body?: unknown; auth?: boolean },
  ): Promise<T> {
    const headers: Record<string, string> = {};
    if (options?.auth !== false) {
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

function nowIso(): string {
  return new Date().toISOString();
}

function asciiLower(text: string): string {
  return text.replace(/[A-Z]/g, ch => ch.toLowerCase());
}

function clone<T>(value: T): T {
  return structuredClone(value);
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
    throw new ServiceApiError('VALIDATION_ERROR', 400, '悬赏标题不能为空');
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

export function createMemoryBountyClient(seed?: {
  projects?: BountyProject[];
  bounties?: BountyTask[];
}): MemoryBountyClient {
  const projects = new Map<string, BountyProject>();
  const bounties = new Map<string, BountyTask>();
  const eventsByBounty = new Map<string, BountyEvent[]>();
  let nextEventId = 1;

  for (const project of seed?.projects ?? []) {
    projects.set(project.id, clone(project));
  }
  for (const bounty of seed?.bounties ?? []) {
    bounties.set(bounty.id, clone(bounty));
    eventsByBounty.set(bounty.id, []);
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
      throw new ServiceApiError('PROJECT_ARCHIVED', 409, '归档项目不能创建、编辑或发布悬赏');
    }
    return project;
  }

  function requireBounty(id: string): BountyTask {
    const bounty = bounties.get(id);
    if (!bounty) {
      throw new ServiceApiError('BOUNTY_NOT_FOUND', 404, '悬赏不存在');
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

  return {
    async health(): Promise<ServerHealth> {
      return { ok: true, mode: 'embedded', version: '0.8.8' };
    },

    async adminStatus(): Promise<AdminStatus> {
      return { ok: true, mode: 'embedded', db_path: ':memory:', version: '0.8.8' };
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
      const timestamp = nowIso();
      const project: BountyProject = {
        id: crypto.randomUUID(),
        name: input.name,
        icon,
        description: input.description ?? '',
        workspace_path: input.workspace_path ?? '',
        default_acceptance_criteria: input.default_acceptance_criteria ?? '',
        agent_draft_policy: policy,
        arbitrator_chain: [...(input.arbitrator_chain ?? [])],
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
        ...(input.arbitrator_chain !== undefined && input.arbitrator_chain !== null
          ? { arbitrator_chain: [...input.arbitrator_chain] }
          : {}),
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
          throw new ServiceApiError('PROJECT_HAS_BOUNTIES', 409, '项目已有悬赏，不能删除');
        }
      }
      projects.delete(id);
    },

    async listBounties(query?: BountyQuery): Promise<BountyTask[]> {
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
        .map(clone);
    },

    async getBountyDetail(id: string): Promise<BountyDetail> {
      const bounty = requireBounty(id);
      const events = [...(eventsByBounty.get(id) ?? [])].sort((a, b) => a.id - b.id);
      return {
        bounty: clone(bounty),
        events: clone(events),
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
        require_arbitration: input.require_arbitration ?? false,
        status: input.status,
        created_by: createdBy,
        lease_id: 0,
        lease_agent_id: null,
        lease_expires_at: null,
        rejection_count: 0,
        completion_note: null,
        published_at: input.status === 'OPEN' ? timestamp : null,
        withdrawn_at: null,
        cancelled_at: null,
        completed_at: null,
        created_at: timestamp,
        updated_at: timestamp,
      };
      bounties.set(bounty.id, bounty);
      eventsByBounty.set(bounty.id, []);
      recordEvent(bounty, 'created', bounty.created_by, null);
      return clone(bounty);
    },

    async updateBounty(id: string, input: UpdateBountyInput): Promise<BountyTask> {
      const existing = requireBounty(id);
      if (existing.status !== 'DRAFT') {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
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
        updated_at: nowIso(),
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
      recordEvent(candidate, 'updated', 'human', 'DRAFT');
      return clone(candidate);
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
        updated_at: timestamp,
      };
      bounties.set(id, updated);
      recordEvent(updated, 'published', 'human', 'DRAFT');
      return clone(updated);
    },

    async withdrawBounty(id: string): Promise<BountyTask> {
      const existing = requireBounty(id);
      if (existing.status === 'IN_PROGRESS' || existing.lease_agent_id !== null) {
        throw new ServiceApiError('BOUNTY_ALREADY_CLAIMED', 409, '悬赏已被接取，不能撤回或重复接取');
      }
      if (existing.status !== 'OPEN') {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
      const timestamp = nowIso();
      const updated: BountyTask = {
        ...clone(existing),
        status: 'DRAFT',
        withdrawn_at: timestamp,
        updated_at: timestamp,
      };
      bounties.set(id, updated);
      recordEvent(updated, 'withdrawn', 'human', 'OPEN');
      return clone(updated);
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
        cancelled_at: timestamp,
        updated_at: timestamp,
      };
      bounties.set(id, updated);
      recordEvent(updated, 'cancelled', 'human', existing.status);
      return clone(updated);
    },

    async simulateClaim(id: string, agentId: string): Promise<BountyTask> {
      if (typeof agentId !== 'string' || !agentId.trim()) {
        throw new ServiceApiError('VALIDATION_ERROR', 400, 'agent_id 不能为空');
      }
      const existing = requireBounty(id);
      if (existing.status === 'IN_PROGRESS' || existing.lease_agent_id !== null) {
        throw new ServiceApiError('BOUNTY_ALREADY_CLAIMED', 409, '悬赏已被接取，不能撤回或重复接取');
      }
      if (existing.status !== 'OPEN') {
        throw new ServiceApiError('INVALID_TRANSITION', 409, '当前状态不允许此操作');
      }
      const timestamp = nowIso();
      const expiresAt = new Date(Date.now() + 15 * 60 * 1000).toISOString();
      const updated: BountyTask = {
        ...clone(existing),
        status: 'IN_PROGRESS',
        lease_id: existing.lease_id + 1,
        lease_agent_id: agentId,
        lease_expires_at: expiresAt,
        updated_at: timestamp,
      };
      bounties.set(id, updated);
      recordEvent(updated, 'claimed', agentId, 'OPEN', {
        lease_id: updated.lease_id,
        lease_agent_id: updated.lease_agent_id,
        lease_expires_at: updated.lease_expires_at,
      });
      return clone(updated);
    },
  };
}
