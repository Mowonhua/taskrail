import { beforeEach, describe, expect, it, vi } from 'vitest';

const native = vi.hoisted(() => ({
  invoke: vi.fn(),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));

import {
  createMemoryBountyClient,
  fetchEmbeddedServerInfo,
  HttpAgentClient,
  HttpBountyClient,
  normalizeServerUrl,
  registerAgent,
} from './bounty-client';
import {
  computeActiveDurationMs,
  formatActiveDuration,
  formatAgentChannelStatus,
  formatAgentDraftPolicy,
  formatAgentStatus,
  formatAgentSubscriptionSummary,
  formatAgentTier,
  formatAssigneeConstraint,
  formatBountyExecutor,
  formatBountyPublisher,
  formatCommentAuthorRole,
  formatCommentKind,
  formatCompletionCost,
  formatEventActor,
  formatEventLabel,
  formatInviteTokenStatus,
  formatLeaseCountdown,
  formatServiceError,
} from '../bounty/helpers';
import { setLocalePreference } from '../i18n';
import {
  ServiceApiError,
  type AgentSummary,
  type BountyProject,
  type BountyTask,
  type InviteTokenSummary,
} from './types';

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function errorResponse(status: number, code: string, message: string): Response {
  return jsonResponse({ error: { code, message } }, status);
}

describe('normalizeServerUrl & fetchEmbeddedServerInfo', () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it('校验并规范化 http/https 服务根地址，去除首尾空白与末尾斜杠', () => {
    expect(normalizeServerUrl('http://127.0.0.1:8420')).toBe('http://127.0.0.1:8420');
    expect(normalizeServerUrl('  https://taskrail.example.com:8443/api///  ')).toBe(
      'https://taskrail.example.com:8443/api',
    );
    for (const bad of ['', '   ', 'ftp://127.0.0.1:8420', 'javascript:alert(1)', 'not-a-url']) {
      expect(() => normalizeServerUrl(bad)).toThrow();
    }
  });

  it('通过 Tauri invoke 读取内嵌服务连接信息', async () => {
    const info = {
      base_url: 'http://127.0.0.1:49152',
      admin_token: 'abc123',
      db_path: 'D:/AppData/taskrail.db',
    };
    native.invoke.mockResolvedValueOnce(info);
    await expect(fetchEmbeddedServerInfo()).resolves.toEqual(info);
    expect(native.invoke).toHaveBeenCalledWith('get_embedded_server_info');
  });
});

describe('HttpBountyClient', () => {
  it('规范化 baseUrl，对健康检查省略 Bearer 头，对其余路由注入 Bearer 头并编码查询参数', async () => {
    const calls: Array<{ url: string; method?: string; headers: Record<string, string>; body?: string }> = [];
    const fetchMock: typeof fetch = async (input, init) => {
      const url = typeof input === 'string' ? input : input.toString();
      const headers = (init?.headers ?? {}) as Record<string, string>;
      calls.push({
        url,
        method: init?.method,
        headers,
        body: typeof init?.body === 'string' ? init.body : undefined,
      });
      if (url.endsWith('/api/v1/health')) {
        return jsonResponse({ ok: true, mode: 'standalone', version: '0.8.8' });
      }
      if (url.endsWith('/api/v1/admin/status')) {
        return jsonResponse({ ok: true, mode: 'standalone', db_path: './taskrail.db', version: '0.8.8' });
      }
      if (url.includes('/api/v1/projects') && init?.method === 'DELETE') {
        return new Response(null, { status: 204 });
      }
      if (url.includes('/api/v1/bounties') && init?.method === 'GET') {
        return jsonResponse([]);
      }
      return jsonResponse([]);
    };

    const client = new HttpBountyClient('http://127.0.0.1:8420///', 'admin-secret', fetchMock);
    expect(client.baseUrl).toBe('http://127.0.0.1:8420');

    const health = await client.health();
    expect(health).toEqual({ ok: true, mode: 'standalone', version: '0.8.8' });
    expect(calls[0].url).toBe('http://127.0.0.1:8420/api/v1/health');
    expect(calls[0].headers.Authorization).toBeUndefined();

    await client.adminStatus();
    expect(calls[1].url).toBe('http://127.0.0.1:8420/api/v1/admin/status');
    expect(calls[1].headers.Authorization).toBe('Bearer admin-secret');

    await client.listProjects(true);
    expect(calls[2].url).toBe('http://127.0.0.1:8420/api/v1/projects?include_archived=true');
    expect(calls[2].headers.Authorization).toBe('Bearer admin-secret');

    await client.listBounties({
      project_id: 'proj-1',
      status: 'DRAFT',
      q: '100% pass_check',
      include_drafts: true,
    });
    const listUrl = new URL(calls[3].url);
    expect(listUrl.pathname).toBe('/api/v1/bounties');
    expect(listUrl.searchParams.get('project_id')).toBe('proj-1');
    expect(listUrl.searchParams.get('status')).toBe('DRAFT');
    expect(listUrl.searchParams.get('q')).toBe('100% pass_check');
    expect(listUrl.searchParams.get('include_drafts')).toBe('true');
    expect(calls[3].headers.Authorization).toBe('Bearer admin-secret');

    await expect(client.deleteProject('proj/1')).resolves.toBeUndefined();
    expect(calls[4].url).toBe('http://127.0.0.1:8420/api/v1/projects/proj%2F1');
    expect(calls[4].method).toBe('DELETE');
    expect(calls[4].headers.Authorization).toBe('Bearer admin-secret');
  });

  it('正确发送项目与悬赏创建、更新和状态转换请求', async () => {
    const calls: Array<{ url: string; method?: string; headers: Record<string, string>; body?: string }> = [];
    const fetchMock: typeof fetch = async (input, init) => {
      calls.push({
        url: String(input),
        method: init?.method,
        headers: (init?.headers ?? {}) as Record<string, string>,
        body: typeof init?.body === 'string' ? init.body : undefined,
      });
      return jsonResponse({ ok: true }, init?.method === 'POST' && String(input).endsWith('/bounties') ? 201 : 200);
    };

    const client = new HttpBountyClient('https://host.example/prefix/', 'tok', fetchMock);
    await client.createProject({ name: 'Taskrail', icon: 'lucide:train-front' });
    await client.getProject('p1');
    await client.updateProject('p1', { icon: null, archived: true });
    await client.createBounty({
      project_id: 'p1',
      title: 'Init',
      difficulty: 'B',
      assignee_mode: 'any',
      status: 'DRAFT',
    });
    await client.getBountyDetail('b1');
    await client.updateBounty('b1', { title: 'Updated', assignee_min_tier: null });
    await client.publishBounty('b1');
    await client.withdrawBounty('b1');
    await client.cancelBounty('b1');

    expect(calls.map(c => `${c.method} ${c.url}`)).toEqual([
      'POST https://host.example/prefix/api/v1/projects',
      'GET https://host.example/prefix/api/v1/projects/p1',
      'PATCH https://host.example/prefix/api/v1/projects/p1',
      'POST https://host.example/prefix/api/v1/bounties',
      'GET https://host.example/prefix/api/v1/bounties/b1',
      'PATCH https://host.example/prefix/api/v1/bounties/b1',
      'POST https://host.example/prefix/api/v1/bounties/b1/publish',
      'POST https://host.example/prefix/api/v1/bounties/b1/withdraw',
      'POST https://host.example/prefix/api/v1/bounties/b1/cancel',
    ]);
    expect(JSON.parse(calls[2].body!)).toEqual({ icon: null, archived: true });
    expect(calls[6].body).toBeUndefined();
    expect(calls[7].body).toBeUndefined();
    expect(calls[8].body).toBeUndefined();
  });

  it('将服务错误信封解析为 ServiceApiError（401、409 抢单冲突与项目删除保护）', async () => {
    const scenarios = [
      { status: 401, code: 'UNAUTHORIZED', message: '需要有效的管理员 Bearer token', call: (c: HttpBountyClient) => c.adminStatus() },
      { status: 409, code: 'BOUNTY_ALREADY_CLAIMED', message: '悬赏已被接取，不能撤回或重复接取', call: (c: HttpBountyClient) => c.withdrawBounty('b1') },
      { status: 409, code: 'PROJECT_HAS_BOUNTIES', message: '项目已有悬赏，不能删除', call: (c: HttpBountyClient) => c.deleteProject('p1') },
    ];

    for (const scenario of scenarios) {
      const client = new HttpBountyClient('http://127.0.0.1:8420', 'bad-token', async () =>
        errorResponse(scenario.status, scenario.code, scenario.message),
      );
      await expect(scenario.call(client)).rejects.toSatisfy(error => {
        expect(error).toBeInstanceOf(ServiceApiError);
        expect((error as ServiceApiError).status).toBe(scenario.status);
        expect((error as ServiceApiError).code).toBe(scenario.code);
        expect((error as ServiceApiError).message).toBe(scenario.message);
        return true;
      });
    }
  });
});

describe('createMemoryBountyClient', () => {
  it('支持项目增删改、归档拦截与关联悬赏删除保护', async () => {
    const client = createMemoryBountyClient();

    const project = await client.createProject({
      name: '核心悬赏项目',
      icon: 'lucide:train-front',
      description: '服务内核',
      workspace_path: 'D:/Workspace/taskrail',
      default_acceptance_criteria: 'npm test 通过',
      agent_draft_policy: 'draft',
      arbitrator_chain: ['arb-1'],
    });
    expect(project.archived).toBe(false);
    expect(project.agent_draft_policy).toBe('draft');
    expect(project.arbitrator_chain).toEqual(['arb-1']);

    await expect(client.createProject({ name: '   ' })).rejects.toMatchObject({
      code: 'VALIDATION_ERROR',
      status: 400,
    });
    await expect(client.createProject({ name: 'BadIcon', icon: 'bad:icon' })).rejects.toMatchObject({
      code: 'VALIDATION_ERROR',
      status: 400,
    });

    const clearedIcon = await client.updateProject(project.id, { icon: null, archived: true });
    expect(clearedIcon.icon).toBeNull();
    expect(clearedIcon.archived).toBe(true);
    expect(await client.listProjects()).toEqual([]);
    expect((await client.listProjects(true)).map(p => p.id)).toEqual([project.id]);

    await expect(
      client.createBounty({
        project_id: project.id,
        title: 'Blocked',
        difficulty: 'B',
        assignee_mode: 'any',
        status: 'DRAFT',
      }),
    ).rejects.toMatchObject({ code: 'PROJECT_ARCHIVED', status: 409 });

    await client.updateProject(project.id, { archived: false });
    const bounty = await client.createBounty({
      project_id: project.id,
      title: '100% pass_check',
      difficulty: 'B',
      assignee_mode: 'any',
      status: 'DRAFT',
    });

    await client.cancelBounty(bounty.id);
    await expect(client.deleteProject(project.id)).rejects.toMatchObject({
      code: 'PROJECT_HAS_BOUNTIES',
      status: 409,
    });

    const emptyProject = await client.createProject({ name: '可删除空项目' });
    await expect(client.deleteProject(emptyProject.id)).resolves.toBeUndefined();
    await expect(client.getProject(emptyProject.id)).rejects.toMatchObject({
      code: 'PROJECT_NOT_FOUND',
      status: 404,
    });
  });

  it('支持草稿创建/编辑/发布、OPEN 撤回为草稿、并发抢单冲突与字面检索', async () => {
    const seedProject: BountyProject = {
      id: 'proj-seed',
      name: 'Seed',
      icon: null,
      description: '',
      workspace_path: '',
      default_acceptance_criteria: '',
      agent_draft_policy: 'open',
      arbitrator_chain: [],
      archived: false,
      created_at: '2026-11-16T10:00:00.000Z',
      updated_at: '2026-11-16T10:00:00.000Z',
    };
    const client = createMemoryBountyClient({ projects: [seedProject] });

    const draft = await client.createBounty({
      project_id: 'proj-seed',
      title: 'Implement %_literal match',
      description: 'Details',
      acceptance_criteria: 'All green',
      difficulty: 'A',
      assignee_mode: 'min_tier',
      assignee_min_tier: 'B',
      status: 'DRAFT',
    });

    // 默认排除草稿，即使传 status=DRAFT 也需要 include_drafts=true
    expect(await client.listBounties({ status: 'DRAFT' })).toEqual([]);
    expect((await client.listBounties({ status: 'DRAFT', include_drafts: true })).map(b => b.id)).toEqual([
      draft.id,
    ]);
    // % 与 _ 按字面子串匹配，不作为 SQL 通配符
    expect(
      await client.listBounties({ q: '%_LITERAL', include_drafts: true }),
    ).toHaveLength(1);
    expect(
      await client.listBounties({ q: 'implement x_literal', include_drafts: true }),
    ).toHaveLength(0);

    // 组合校验失败时整体回滚，不破坏原草稿
    await expect(
      client.updateBounty(draft.id, {
        title: 'Should Rollback',
        assignee_mode: 'specific',
        assignee_agent_id: 'agent-7',
        // 未清空 assignee_min_tier 导致冲突
      }),
    ).rejects.toMatchObject({ code: 'VALIDATION_ERROR', status: 400 });
    expect((await client.getBountyDetail(draft.id)).bounty.title).toBe('Implement %_literal match');

    const edited = await client.updateBounty(draft.id, {
      title: 'Edited Draft',
      assignee_mode: 'specific',
      assignee_min_tier: null,
      assignee_agent_id: 'agent-7',
      require_arbitration: true,
    });
    expect(edited.assignee_mode).toBe('specific');
    expect(edited.assignee_min_tier).toBeNull();
    expect(edited.assignee_agent_id).toBe('agent-7');
    expect(edited.require_arbitration).toBe(true);

    const published = await client.publishBounty(draft.id);
    expect(published.status).toBe('OPEN');
    expect(published.published_at).not.toBeNull();

    // 非草稿不能编辑
    await expect(client.updateBounty(draft.id, { title: 'Illegal Edit' })).rejects.toMatchObject({
      code: 'INVALID_TRANSITION',
      status: 409,
    });

    const withdrawn = await client.withdrawBounty(draft.id);
    expect(withdrawn.status).toBe('DRAFT');
    expect(withdrawn.withdrawn_at).not.toBeNull();
    expect(withdrawn.published_at).toBe(published.published_at);

    await client.publishBounty(draft.id);
    const claimed = await client.simulateClaim(draft.id, 'agent-7');
    expect(claimed.status).toBe('IN_PROGRESS');
    expect(claimed.lease_id).toBe(1);
    expect(claimed.lease_agent_id).toBe('agent-7');
    expect(claimed.lease_expires_at).not.toBeNull();

    // 抢单成功后撤回或重复抢单必须拒绝
    await expect(client.withdrawBounty(draft.id)).rejects.toMatchObject({
      code: 'BOUNTY_ALREADY_CLAIMED',
      status: 409,
    });
    await expect(client.simulateClaim(draft.id, 'agent-8')).rejects.toMatchObject({
      code: 'BOUNTY_ALREADY_CLAIMED',
      status: 409,
    });

    const detail = await client.getBountyDetail(draft.id);
    expect(detail.events.map(e => `${e.event_type}:${e.actor}:${e.from_status}->${e.to_status}`)).toEqual([
      'created:human:null->DRAFT',
      'updated:human:DRAFT->DRAFT',
      'published:human:DRAFT->OPEN',
      'withdrawn:human:OPEN->DRAFT',
      'published:human:DRAFT->OPEN',
      'claimed:agent:agent-7:OPEN->IN_PROGRESS',
    ]);
  });

  it('校验种子悬赏数据与 CANCELLED/WAITING_INFO 状态转换限制', async () => {
    const seedProject: BountyProject = {
      id: 'p1',
      name: 'P1',
      icon: null,
      description: '',
      workspace_path: '',
      default_acceptance_criteria: '',
      agent_draft_policy: 'open',
      arbitrator_chain: [],
      archived: false,
      created_at: '2026-11-16T10:00:00.000Z',
      updated_at: '2026-11-16T10:00:00.000Z',
    };
    const waitingBounty: BountyTask = {
      id: 'b-wait',
      project_id: 'p1',
      title: 'Waiting Info',
      description: '',
      acceptance_criteria: '',
      difficulty: 'C',
      assignee_mode: 'any',
      assignee_min_tier: null,
      assignee_agent_id: null,
      assignee_agent_name: null,
      assignee_agent_tier: null,
      assignment_expires_at: null,
      require_arbitration: false,
      status: 'WAITING_INFO',
      created_by: 'human',
      created_by_agent_id: null,
      created_by_agent_name: null,
      lease_id: 1,
      lease_agent_id: null,
      lease_agent_name: null,
      lease_agent_tier: null,
      lease_expires_at: null,
      active_duration_ms: 0,
      active_started_at: null,
      comment_count: 0,
      rejection_count: 0,
      completion_note: null,
      completion_cost: null,
      published_at: '2026-11-16T10:01:00.000Z',
      withdrawn_at: null,
      cancelled_at: null,
      completed_at: null,
      created_at: '2026-11-16T10:00:00.000Z',
      updated_at: '2026-11-16T10:02:00.000Z',
    };

    const client = createMemoryBountyClient({
      projects: [seedProject],
      bounties: [waitingBounty],
    });
    const cancelled = await client.cancelBounty('b-wait');
    expect(cancelled.status).toBe('CANCELLED');
    expect(cancelled.cancelled_at).not.toBeNull();
    await expect(client.cancelBounty('b-wait')).rejects.toMatchObject({
      code: 'INVALID_TRANSITION',
      status: 409,
    });
  });
});

describe('Issue 03 HTTP 客户端（管理员准入/名录 + Agent 门户）', () => {
  it('HttpBountyClient 正确构造准入设置、邀请令牌、SKILL.md 与 Agent 名录管控请求', async () => {
    const calls: Array<{ url: string; method?: string; headers: Record<string, string>; body?: string }> = [];
    const fetchMock: typeof fetch = async (input, init) => {
      calls.push({
        url: String(input),
        method: init?.method,
        headers: (init?.headers ?? {}) as Record<string, string>,
        body: typeof init?.body === 'string' ? init.body : undefined,
      });
      return jsonResponse({});
    };

    const client = new HttpBountyClient('http://127.0.0.1:8420/', 'admin-tok', fetchMock);
    await client.getOnboardingSettings();
    await client.updateOnboardingSettings({ lan_open_registration: true });
    await client.listInviteTokens();
    await client.createInviteToken({ label: 'CI', max_uses: 3, expires_in_hours: 12 });
    await client.revokeInviteToken('tok/1');
    await client.getSkillMarkdown({ baseUrl: 'http://192.168.1.5:8420', inviteToken: 'tr_inv_abc' });
    await client.listAgents({ status: 'active', activity: 'busy' });
    await client.disableAgent('ag/1');
    await client.enableAgent('ag/1');
    await client.revokeAgent('ag/1');

    expect(calls.map(c => `${c.method} ${c.url}`)).toEqual([
      'GET http://127.0.0.1:8420/api/v1/admin/onboarding',
      'PATCH http://127.0.0.1:8420/api/v1/admin/onboarding',
      'GET http://127.0.0.1:8420/api/v1/admin/invite-tokens',
      'POST http://127.0.0.1:8420/api/v1/admin/invite-tokens',
      'POST http://127.0.0.1:8420/api/v1/admin/invite-tokens/tok%2F1/revoke',
      'GET http://127.0.0.1:8420/api/v1/admin/skill.md?base_url=http%3A%2F%2F192.168.1.5%3A8420&invite_token=tr_inv_abc',
      'GET http://127.0.0.1:8420/api/v1/agents?status=active&activity=busy',
      'POST http://127.0.0.1:8420/api/v1/agents/ag%2F1/disable',
      'POST http://127.0.0.1:8420/api/v1/agents/ag%2F1/enable',
      'POST http://127.0.0.1:8420/api/v1/agents/ag%2F1/revoke',
    ]);
    for (const call of calls) {
      expect(call.headers.Authorization).toBe('Bearer admin-tok');
    }
    expect(JSON.parse(calls[1].body!)).toEqual({ lan_open_registration: true });
    expect(JSON.parse(calls[3].body!)).toEqual({ label: 'CI', max_uses: 3, expires_in_hours: 12 });
    expect(calls[4].body).toBeUndefined();
    expect(calls[7].body).toBeUndefined();
  });

  it('HttpAgentClient 与 registerAgent 正确构造公开文档、注册及 Agent 鉴权请求', async () => {
    const calls: Array<{ url: string; method?: string; headers: Record<string, string>; body?: string }> = [];
    const fetchMock: typeof fetch = async (input, init) => {
      const url = String(input);
      calls.push({
        url,
        method: init?.method,
        headers: (init?.headers ?? {}) as Record<string, string>,
        body: typeof init?.body === 'string' ? init.body : undefined,
      });
      if (url.includes('/skill.md')) {
        return new Response('# Taskrail Agent', {
          status: 200,
          headers: { 'Content-Type': 'text/markdown; charset=utf-8' },
        });
      }
      return jsonResponse({});
    };

    await registerAgent(
      'http://127.0.0.1:8420/',
      { name: 'agent-1', subscribe_all_projects: true },
      { bearerInviteToken: 'tr_inv_header', fetchImpl: fetchMock },
    );

    const agentClient = new HttpAgentClient('http://127.0.0.1:8420/', 'tr_ag_secret', fetchMock);
    const md = await agentClient.getPublicSkillMarkdown({ inviteToken: 'tr_inv_q' });
    expect(md).toBe('# Taskrail Agent');
    await agentClient.getMe();
    await agentClient.updateMe({ description: 'Rust & TS', capabilities: ['rust', 'ts'] });
    await agentClient.updateSubscriptions({ subscribe_all_projects: false, project_ids: ['p1'] });
    await agentClient.listProjects();
    await agentClient.listBounties({ project_id: 'p1', status: 'OPEN', q: 'fix%_bug' });
    await agentClient.getBountyDetail('b/1');

    expect(calls.map(c => `${c.method} ${c.url}`)).toEqual([
      'POST http://127.0.0.1:8420/api/v1/agents/register',
      'GET http://127.0.0.1:8420/skill.md?invite_token=tr_inv_q',
      'GET http://127.0.0.1:8420/api/v1/agent/me',
      'PATCH http://127.0.0.1:8420/api/v1/agent/me',
      'PUT http://127.0.0.1:8420/api/v1/agent/me/subscriptions',
      'GET http://127.0.0.1:8420/api/v1/agent/projects',
      'GET http://127.0.0.1:8420/api/v1/agent/bounties?project_id=p1&status=OPEN&q=fix%25_bug',
      'GET http://127.0.0.1:8420/api/v1/agent/bounties/b%2F1',
    ]);
    expect(calls[0].headers.Authorization).toBe('Bearer tr_inv_header');
    expect(JSON.parse(calls[0].body!)).toEqual({ name: 'agent-1', subscribe_all_projects: true });
    expect(calls[1].headers.Authorization).toBeUndefined();
    for (const call of calls.slice(2)) {
      expect(call.headers.Authorization).toBe('Bearer tr_ag_secret');
    }
    expect(JSON.parse(calls[3].body!)).toEqual({ description: 'Rust & TS', capabilities: ['rust', 'ts'] });
    expect(JSON.parse(calls[4].body!)).toEqual({ subscribe_all_projects: false, project_ids: ['p1'] });
  });
});

describe('createMemoryBountyClient (Issue 03 Agent 准入、订阅与隔离)', () => {
  it('支持邀请令牌门控、局域网免审开关、重名拦截与 SKILL.md 动态生成', async () => {
    const client = createMemoryBountyClient();

    // 默认关闭免审
    const initial = await client.getOnboardingSettings();
    expect(initial.lan_open_registration).toBe(false);
    expect(initial.invite_tokens).toEqual([]);

    // 无邀请且免审关闭时拒绝注册
    await expect(client.registerAgent({ name: 'alpha' })).rejects.toMatchObject({
      code: 'INVITE_TOKEN_REQUIRED',
      status: 401,
    });

    // 创建一次性邀请令牌，检查前缀与明文分离
    const createdInvite = await client.createInviteToken({
      label: '单次测试',
      max_uses: 1,
      expires_in_hours: 24,
    });
    expect(createdInvite.raw_token.startsWith('tr_inv_')).toBe(true);
    expect(createdInvite.token.token_prefix).toBe(createdInvite.raw_token.slice(0, 15));
    expect(createdInvite.token.use_count).toBe(0);
    expect(createdInvite.token.max_uses).toBe(1);

    // 导出 SKILL.md 带有效邀请不消耗次数
    const skillWithInvite = await client.getSkillMarkdown({
      baseUrl: 'http://192.168.1.10:8420/',
      inviteToken: createdInvite.raw_token,
    });
    expect(skillWithInvite.filename).toBe('SKILL.md');
    expect(skillWithInvite.base_url).toBe('http://192.168.1.10:8420');
    expect(skillWithInvite.content).toContain(createdInvite.raw_token);
    expect(skillWithInvite.content).toContain('需要有效邀请令牌');
    expect((await client.listInviteTokens())[0].use_count).toBe(0);

    // 非法 base_url 被拒绝
    await expect(client.getSkillMarkdown({ baseUrl: 'http://127.0.0.1:8420/subpath' })).rejects.toMatchObject({
      code: 'VALIDATION_ERROR',
      status: 400,
    });

    // 通过 Bearer 头使用一次性邀请注册成功，返回 Unranked 与一次性 api_key
    const reg1 = await client.registerAgent(
      { name: '  agent-alpha  ', description: 'Primary coder', capabilities: ['rust', 'ts'] },
      { bearerInviteToken: createdInvite.raw_token },
    );
    expect(reg1.api_key.startsWith('tr_ag_')).toBe(true);
    expect(reg1.agent.name).toBe('agent-alpha');
    expect(reg1.agent.tier).toBe('Unranked');
    expect(reg1.agent.placement_completed).toBe(false);
    expect(reg1.agent.subscribe_all_projects).toBe(true);
    expect(reg1.agent.status).toBe('active');
    expect(reg1.agent.invite_token_id).toBe(createdInvite.token.id);
    expect('mmr' in reg1.agent).toBe(false);
    expect('api_key' in reg1.agent).toBe(false);
    expect('api_key_hash' in reg1.agent).toBe(false);

    // 一次性邀请已耗尽，第二次注册失败
    await expect(
      client.registerAgent({ name: 'agent-beta', invite_token: createdInvite.raw_token }),
    ).rejects.toMatchObject({ code: 'INVITE_TOKEN_INVALID', status: 401 });

    // 作废另一个邀请令牌后不可用于文档或注册
    const invite2 = await client.createInviteToken({ label: 'To revoke' });
    const revokedInvite = await client.revokeInviteToken(invite2.token.id);
    expect(revokedInvite.revoked_at).not.toBeNull();
    await expect(
      client.getPublicSkillMarkdown({ inviteToken: invite2.raw_token }),
    ).rejects.toMatchObject({ code: 'INVITE_TOKEN_INVALID', status: 401 });

    // 开启免审注册后可无邀请注册，且公开 SKILL.md 反映策略；但显式传入无效邀请仍被拒绝
    const updatedOnboarding = await client.updateOnboardingSettings({ lan_open_registration: true });
    expect(updatedOnboarding.lan_open_registration).toBe(true);
    const openSkill = await client.getPublicSkillMarkdown();
    expect(openSkill).toContain('已开启局域网免审注册');
    expect(openSkill).toContain('<INVITE_TOKEN>');

    await expect(
      client.registerAgent({ name: 'agent-beta', invite_token: invite2.raw_token }),
    ).rejects.toMatchObject({ code: 'INVITE_TOKEN_INVALID', status: 401 });

    // 重名冲突
    await expect(client.registerAgent({ name: 'agent-alpha' })).rejects.toMatchObject({
      code: 'AGENT_NAME_TAKEN',
      status: 409,
    });

    const reg2 = await client.registerAgent({ name: 'agent-beta' });
    expect(reg2.agent.invite_token_id).toBeNull();
    expect(reg2.agent.tier).toBe('Unranked');
  });

  it('支持订阅替换校验、项目归档拒绝、停用/启用/吊销鉴权拦截与名录活跃度筛选', async () => {
    const client = createMemoryBountyClient({ lan_open_registration: true });
    const p1 = await client.createProject({ name: 'P1' });
    const p2 = await client.createProject({ name: 'P2' });
    const pArchived = await client.createProject({ name: 'Archived' });
    await client.updateProject(pArchived.id, { archived: true });

    const { agent, api_key: apiKey } = await client.registerAgent({
      name: 'sub-agent',
      project_ids: [p2.id, p1.id, p1.id],
    });
    expect(agent.subscribe_all_projects).toBe(false);
    expect(agent.project_ids).toEqual([p1.id, p2.id].sort());

    // subscribe_all_projects: true 与非空 project_ids 冲突
    await expect(
      client.agentUpdateSubscriptions(apiKey, {
        subscribe_all_projects: true,
        project_ids: [p1.id],
      }),
    ).rejects.toMatchObject({ code: 'VALIDATION_ERROR', status: 400 });

    // 订阅归档项目被拒绝并保留原有订阅
    await expect(
      client.agentUpdateSubscriptions(apiKey, {
        subscribe_all_projects: false,
        project_ids: [pArchived.id],
      }),
    ).rejects.toMatchObject({ code: 'PROJECT_ARCHIVED', status: 409 });
    expect((await client.agentGetMe(apiKey)).project_ids).toEqual([p1.id, p2.id].sort());

    // 切换为全部项目与空订阅
    const allSub = await client.agentUpdateSubscriptions(apiKey, { subscribe_all_projects: true });
    expect(allSub.subscribe_all_projects).toBe(true);
    expect(allSub.project_ids).toEqual([]);

    const noneSub = await client.agentUpdateSubscriptions(apiKey, {
      subscribe_all_projects: false,
      project_ids: [],
    });
    expect(noneSub.subscribe_all_projects).toBe(false);
    expect(noneSub.project_ids).toEqual([]);

    // agentListProjects 只返回未归档项目，且按 created_at 升序稳定排列（不依赖随机 UUID）
    expect(p1.created_at < p2.created_at).toBe(true);
    const listedProjects = await client.agentListProjects(apiKey);
    expect(listedProjects.map(p => p.id)).toEqual([p1.id, p2.id]);
    expect(listedProjects.map(p => p.name)).toEqual(['P1', 'P2']);

    // 模拟持有租约使名录识别为 busy
    const openBounty = await client.createBounty({
      project_id: p1.id,
      title: 'Claimable',
      difficulty: 'C',
      assignee_mode: 'any',
      status: 'OPEN',
    });
    await client.simulateClaim(openBounty.id, agent.id);
    expect((await client.listAgents({ activity: 'busy' })).map(a => a.id)).toEqual([agent.id]);
    expect(await client.listAgents({ activity: 'idle' })).toEqual([]);

    // 停用后返回 403 AGENT_DISABLED，启用后恢复原密钥可用
    const disabled = await client.disableAgent(agent.id);
    expect(disabled.status).toBe('disabled');
    await expect(client.agentGetMe(apiKey)).rejects.toMatchObject({
      code: 'AGENT_DISABLED',
      status: 403,
    });
    const enabled = await client.enableAgent(agent.id);
    expect(enabled.status).toBe('active');
    const meAfterEnable = await client.agentUpdateMe(apiKey, {
      description: 'Updated desc',
      capabilities: ['go'],
    });
    expect(meAfterEnable.description).toBe('Updated desc');
    expect(meAfterEnable.capabilities).toEqual(['go']);
    expect(meAfterEnable.last_seen_at).not.toBeNull();

    // 吊销后返回 401 AGENT_REVOKED，不可启用或停用，重复吊销幂等
    const revoked = await client.revokeAgent(agent.id);
    expect(revoked.status).toBe('revoked');
    await expect(client.agentGetMe(apiKey)).rejects.toMatchObject({
      code: 'AGENT_REVOKED',
      status: 401,
    });
    await expect(client.enableAgent(agent.id)).rejects.toMatchObject({
      code: 'INVALID_TRANSITION',
      status: 409,
    });
    await expect(client.disableAgent(agent.id)).rejects.toMatchObject({
      code: 'INVALID_TRANSITION',
      status: 409,
    });
    expect((await client.revokeAgent(agent.id)).status).toBe('revoked');
  });

  it('严格隔离 DRAFT 任务与定向任务可见性', async () => {
    const client = createMemoryBountyClient({ lan_open_registration: true });
    const pSub = await client.createProject({ name: 'Subscribed' });
    const pOther = await client.createProject({ name: 'Unsubscribed' });

    const { agent: a1, api_key: k1 } = await client.registerAgent({
      name: 'agent-1',
      project_ids: [pSub.id],
    });
    const { agent: a2 } = await client.registerAgent({
      name: 'agent-2',
      project_ids: [pSub.id],
    });

    // 1. 订阅项目内的公开悬赏（可见）
    const openSub = await client.createBounty({
      project_id: pSub.id,
      title: 'Public in subscribed',
      difficulty: 'B',
      assignee_mode: 'any',
      status: 'OPEN',
    });
    // 2. 订阅项目内的草稿（不可见，即使指派给自己）
    const draftSelf = await client.createBounty({
      project_id: pSub.id,
      title: 'Draft assigned to self',
      difficulty: 'B',
      assignee_mode: 'specific',
      assignee_agent_id: a1.id,
      status: 'DRAFT',
    });
    // 3. 订阅项目内指派给其他 Agent 的公开悬赏（不可见）
    const openOtherSpecific = await client.createBounty({
      project_id: pSub.id,
      title: 'Assigned to agent-2',
      difficulty: 'B',
      assignee_mode: 'specific',
      assignee_agent_id: a2.id,
      status: 'OPEN',
    });
    // 4. 未订阅项目内的公开悬赏（不可见）
    const openUnsub = await client.createBounty({
      project_id: pOther.id,
      title: 'Public in unsubscribed',
      difficulty: 'B',
      assignee_mode: 'any',
      status: 'OPEN',
    });
    // 5. 未订阅项目内指派给自己的公开悬赏（可见）
    const openUnsubSelf = await client.createBounty({
      project_id: pOther.id,
      title: 'Assigned to self in unsubscribed',
      difficulty: 'B',
      assignee_mode: 'specific',
      assignee_agent_id: a1.id,
      status: 'OPEN',
    });

    const visibleIds = (await client.agentListBounties(k1, { include_drafts: true })).map(b => b.id);
    expect(visibleIds.sort()).toEqual([openSub.id, openUnsubSelf.id].sort());

    // 传 status=DRAFT 也返回空列表
    expect(await client.agentListBounties(k1, { status: 'DRAFT', include_drafts: true })).toEqual([]);

    // 单个草稿、他人定向任务、未订阅公开任务详情均返回 404 BOUNTY_NOT_FOUND
    for (const hiddenId of [draftSelf.id, openOtherSpecific.id, openUnsub.id]) {
      await expect(client.agentGetBountyDetail(k1, hiddenId)).rejects.toMatchObject({
        code: 'BOUNTY_NOT_FOUND',
        status: 404,
      });
    }
    const detail = await client.agentGetBountyDetail(k1, openUnsubSelf.id);
    expect(detail.bounty.id).toBe(openUnsubSelf.id);
  });
});

describe('Issue 03 辅助格式化函数与中英文翻译', () => {
  it('正确格式化 Agent 状态、段位、邀请令牌状态、订阅摘要与错误消息且不含中点分隔符', () => {
    setLocalePreference('zh-CN');
    expect(formatAgentStatus('active')).toBe('正常接入');
    expect(formatAgentStatus('disabled')).toBe('已停用');
    expect(formatAgentStatus('revoked')).toBe('已吊销');
    expect(formatAgentTier('Unranked')).toBe('未定级');
    expect(formatAgentTier('A')).toBe('A 级');

    const nowMs = Date.parse('2026-11-17T12:00:00.000Z');
    const baseToken: InviteTokenSummary = {
      id: 't1',
      label: 'Test',
      token_prefix: 'tr_inv_12345678',
      max_uses: 2,
      use_count: 0,
      expires_at: '2026-11-18T12:00:00.000Z',
      revoked_at: null,
      created_at: '2026-11-17T00:00:00.000Z',
    };
    expect(formatInviteTokenStatus(baseToken, nowMs)).toEqual({ kind: 'active', label: '有效' });
    expect(formatInviteTokenStatus({ ...baseToken, revoked_at: '2026-11-17T01:00:00.000Z' }, nowMs)).toEqual({
      kind: 'revoked',
      label: '已作废',
    });
    expect(formatInviteTokenStatus({ ...baseToken, expires_at: '2026-11-17T12:00:00.000Z' }, nowMs)).toEqual({
      kind: 'expired',
      label: '已过期',
    });
    expect(formatInviteTokenStatus({ ...baseToken, use_count: 2 }, nowMs)).toEqual({
      kind: 'exhausted',
      label: '已用尽',
    });

    const p1: BountyProject = {
      id: 'p1',
      name: 'Core',
      icon: null,
      description: '',
      workspace_path: '',
      default_acceptance_criteria: '',
      agent_draft_policy: 'open',
      arbitrator_chain: [],
      archived: false,
      created_at: '2026-11-17T00:00:00.000Z',
      updated_at: '2026-11-17T00:00:00.000Z',
    };
    const p2: BountyProject = { ...p1, id: 'p2', name: 'Web' };
    const projectsById = new Map<string, BountyProject>([
      [p1.id, p1],
      [p2.id, p2],
    ]);
    const baseAgent: AgentSummary = {
      id: 'a1',
      name: 'Agent',
      description: '',
      capabilities: [],
      api_key_prefix: 'tr_ag_123456789',
      tier: 'Unranked',
      placement_completed: false,
      subscribe_all_projects: true,
      project_ids: [],
      status: 'active',
      invite_token_id: null,
      active_lease_count: 0,
      timeout_count: 0,
      abandon_count: 0,
      last_seen_at: null,
      channel_status: 'offline',
      stream_connected: false,
      webhook_configured: false,
      webhook_url: null,
      last_pushed_at: null,
      created_at: '2026-11-17T00:00:00.000Z',
      updated_at: '2026-11-17T00:00:00.000Z',
    };

    expect(formatAgentSubscriptionSummary(baseAgent, projectsById)).toBe('全部项目');
    expect(
      formatAgentSubscriptionSummary({ ...baseAgent, subscribe_all_projects: false, project_ids: [] }, projectsById),
    ).toBe('未订阅项目');
    const summaryZh = formatAgentSubscriptionSummary(
      { ...baseAgent, subscribe_all_projects: false, project_ids: ['p1', 'p2'] },
      projectsById,
    );
    expect(summaryZh).toBe('Core、Web');
    expect(summaryZh).not.toContain('·');

    expect(formatServiceError(new ServiceApiError('INVITE_TOKEN_REQUIRED', 401, 'raw'))).toBe(
      '需要提供注册邀请令牌。',
    );
    expect(formatServiceError(new ServiceApiError('AGENT_NAME_TAKEN', 409, 'raw'))).toBe(
      '该 Agent 名称已被占用，请更换名称。',
    );

    setLocalePreference('en');
    expect(formatAgentStatus('active')).toBe('Active');
    expect(formatAgentTier('Unranked')).toBe('Unranked');
    expect(formatAgentTier('S')).toBe('Tier S');
    expect(
      formatAgentSubscriptionSummary(
        { ...baseAgent, subscribe_all_projects: false, project_ids: ['p1', 'p2'] },
        projectsById,
      ),
    ).toBe('Core, Web');
    expect(formatServiceError(new ServiceApiError('AGENT_DISABLED', 403, 'raw'))).toBe(
      'This Agent is disabled. Enable it before continuing.',
    );
    setLocalePreference('zh-CN');
  });
});

describe('Issue 04 HTTP Agent 客户端（Webhook 配置、Inbox 与 Ack）', () => {
  it('HttpAgentClient 正确构造 Webhook 读写、Inbox 查询参数与 Ack 请求', async () => {
    const calls: Array<{ url: string; method?: string; headers: Record<string, string>; body?: string }> = [];
    const fetchMock: typeof fetch = async (input, init) => {
      calls.push({
        url: String(input),
        method: init?.method,
        headers: (init?.headers ?? {}) as Record<string, string>,
        body: typeof init?.body === 'string' ? init.body : undefined,
      });
      return jsonResponse({});
    };

    const agentClient = new HttpAgentClient('http://127.0.0.1:8420/', 'tr_ag_secret', fetchMock);
    await agentClient.getWebhookConfig();
    await agentClient.updateWebhookConfig({
      webhook_url: 'https://agent.example.com/hook',
      webhook_secret: 'custom-secret',
    });
    await agentClient.updateWebhookConfig({ webhook_url: null });
    await agentClient.listEvents();
    await agentClient.listEvents({ cursor: 0, limit: 50 });
    await agentClient.ackEvents(42);

    expect(calls.map(c => `${c.method} ${c.url}`)).toEqual([
      'GET http://127.0.0.1:8420/api/v1/agent/me/webhook',
      'PUT http://127.0.0.1:8420/api/v1/agent/me/webhook',
      'PUT http://127.0.0.1:8420/api/v1/agent/me/webhook',
      'GET http://127.0.0.1:8420/api/v1/agent/events',
      'GET http://127.0.0.1:8420/api/v1/agent/events?cursor=0&limit=50',
      'POST http://127.0.0.1:8420/api/v1/agent/events/ack',
    ]);
    for (const call of calls) {
      expect(call.headers.Authorization).toBe('Bearer tr_ag_secret');
    }
    expect(JSON.parse(calls[1].body!)).toEqual({
      webhook_url: 'https://agent.example.com/hook',
      webhook_secret: 'custom-secret',
    });
    expect(JSON.parse(calls[2].body!)).toEqual({ webhook_url: null });
    expect(JSON.parse(calls[5].body!)).toEqual({ event_id: 42 });
  });
});

describe('createMemoryBountyClient (Issue 04 双通道推送与持久化事件收件箱)', () => {
  it('支持注册与更新 Webhook 配置、一次性密钥返回、URL/密钥校验及通道状态更新', async () => {
    const client = createMemoryBountyClient({ lan_open_registration: true });

    // 非法 webhook_url 与空白 webhook_secret 均被拒绝
    await expect(
      client.registerAgent({ name: 'bad-url', webhook_url: 'ftp://example.com/hook' }),
    ).rejects.toMatchObject({ code: 'VALIDATION_ERROR', status: 400 });
    await expect(
      client.registerAgent({ name: 'bad-space', webhook_url: ' https://example.com/hook' }),
    ).rejects.toMatchObject({ code: 'VALIDATION_ERROR', status: 400 });
    await expect(
      client.registerAgent({ name: 'bad-sec', webhook_url: 'https://example.com/hook', webhook_secret: '   ' }),
    ).rejects.toMatchObject({ code: 'VALIDATION_ERROR', status: 400 });

    // 注册配置 webhook_url 且省略密钥时自动生成 tr_whsec_... 并仅返回一次
    const regHook = await client.registerAgent({
      name: 'hook-agent',
      webhook_url: 'https://agent.example.com/events',
    });
    expect(regHook.webhook_secret?.startsWith('tr_whsec_')).toBe(true);
    expect(regHook.agent.channel_status).toBe('webhook');
    expect(regHook.agent.stream_connected).toBe(false);
    expect(regHook.agent.webhook_configured).toBe(true);
    expect(regHook.agent.webhook_url).toBe('https://agent.example.com/events');
    expect(regHook.agent.last_pushed_at).toBeNull();
    expect('webhook_secret' in regHook.agent).toBe(false);

    // 读取配置不回显已有密钥
    const hookConfig = await client.agentGetWebhookConfig(regHook.api_key);
    expect(hookConfig).toEqual({
      webhook_url: 'https://agent.example.com/events',
      webhook_configured: true,
      has_secret: true,
      last_pushed_at: null,
      recent_failed_count: 0,
    });
    expect('webhook_secret' in hookConfig).toBe(false);

    // 仅更换 URL 时保留旧密钥且不再返回明文
    const replacedUrl = await client.agentUpdateWebhookConfig(regHook.api_key, {
      webhook_url: 'http://127.0.0.1:9001/hook',
    });
    expect(replacedUrl.webhook_url).toBe('http://127.0.0.1:9001/hook');
    expect(replacedUrl.has_secret).toBe(true);
    expect(replacedUrl.webhook_secret).toBeUndefined();

    // 显式更换密钥时返回一次新密钥
    const rotatedSecret = await client.agentUpdateWebhookConfig(regHook.api_key, {
      webhook_url: 'http://127.0.0.1:9001/hook',
      webhook_secret: 'my-explicit-secret',
    });
    expect(rotatedSecret.webhook_secret).toBe('my-explicit-secret');

    // 清空 webhook_url 同时清除密钥并使通道回到 offline
    const cleared = await client.agentUpdateWebhookConfig(regHook.api_key, { webhook_url: null });
    expect(cleared.webhook_url).toBeNull();
    expect(cleared.webhook_configured).toBe(false);
    expect(cleared.has_secret).toBe(false);
    expect(cleared.webhook_secret).toBeUndefined();
    expect((await client.agentGetMe(regHook.api_key)).channel_status).toBe('offline');
  });

  it('精准分发：Unranked/C/B 段位映射、指定指派、订阅匹配与撤回仅通知已接收目标', async () => {
    const client = createMemoryBountyClient({ lan_open_registration: true });
    const p1 = await client.createProject({ name: 'Project A' });
    const p2 = await client.createProject({ name: 'Project B' });

    const unrankedSub1 = await client.registerAgent({
      name: 'unranked-p1',
      project_ids: [p1.id],
    });
    const tierBSub1 = await client.registerAgent({
      name: 'tier-b-p1',
      project_ids: [p1.id],
    });
    await client.simulateAgentTier(tierBSub1.agent.id, 'B', true);

    const unsubAgent = await client.registerAgent({
      name: 'unsub-p2',
      project_ids: [p2.id],
    });
    const disabledAgent = await client.registerAgent({
      name: 'disabled-all',
      subscribe_all_projects: true,
    });
    await client.disableAgent(disabledAgent.agent.id);

    // 1. DRAFT 不触发分发
    const draft = await client.createBounty({
      project_id: p1.id,
      title: 'Secret Draft Title',
      description: 'Secret Draft Body',
      difficulty: 'C',
      assignee_mode: 'min_tier',
      assignee_min_tier: 'C',
      status: 'DRAFT',
    });
    expect((await client.agentListEvents(unrankedSub1.api_key)).events).toEqual([]);

    // 2. min_tier = C 时，Unranked 与 B 均收到 task.published；未订阅 p1 与已停用 Agent 收不到
    await client.publishBounty(draft.id);
    const inboxUnranked1 = await client.agentListEvents(unrankedSub1.api_key);
    const inboxB1 = await client.agentListEvents(tierBSub1.api_key);
    expect(inboxUnranked1.events.map(e => e.event_type)).toEqual(['task.published']);
    expect(inboxUnranked1.events[0].bounty_id).toBe(draft.id);
    expect(inboxUnranked1.events[0].project_id).toBe(p1.id);
    expect(inboxUnranked1.events[0].data.title).toBe('Secret Draft Title');
    expect(inboxB1.events.map(e => e.event_type)).toEqual(['task.published']);
    expect((await client.agentListEvents(unsubAgent.api_key)).events).toEqual([]);

    // 3. min_tier = B 直接创建为 OPEN 时，Unranked 收不到，B 级收到
    const openMinB = await client.createBounty({
      project_id: p1.id,
      title: 'Need B',
      difficulty: 'B',
      assignee_mode: 'min_tier',
      assignee_min_tier: 'B',
      status: 'OPEN',
    });
    expect((await client.agentListEvents(unrankedSub1.api_key)).events).toHaveLength(1);
    const inboxB2 = await client.agentListEvents(tierBSub1.api_key);
    expect(inboxB2.events.map(e => e.bounty_id)).toEqual([draft.id, openMinB.id]);

    // 4. specific 指派：即使目标 Agent 未订阅项目也收到 task.assigned，其余订阅者收不到
    const specificBounty = await client.createBounty({
      project_id: p1.id,
      title: 'Direct Assignment',
      difficulty: 'A',
      assignee_mode: 'specific',
      assignee_agent_id: unsubAgent.agent.id,
      status: 'OPEN',
    });
    const inboxUnsub = await client.agentListEvents(unsubAgent.api_key);
    expect(inboxUnsub.events.map(e => `${e.event_type}:${e.bounty_id}`)).toEqual([
      `task.assigned:${specificBounty.id}`,
    ]);
    expect((await client.agentListEvents(unrankedSub1.api_key)).events).toHaveLength(1);
    expect((await client.agentListEvents(tierBSub1.api_key)).events).toHaveLength(2);

    // 5. 撤回 draft：变更订阅不影响撤回通知，且不泄露草稿正文；同时清理分发记录
    await client.agentUpdateSubscriptions(unrankedSub1.api_key, {
      subscribe_all_projects: false,
      project_ids: [],
    });
    // 让 unsubAgent 此时订阅 p1，也不应收到上一轮未通知任务的撤回事件
    await client.agentUpdateSubscriptions(unsubAgent.api_key, {
      subscribe_all_projects: false,
      project_ids: [p1.id],
    });
    await client.withdrawBounty(draft.id);

    const afterWithdrawUnranked = await client.agentListEvents(unrankedSub1.api_key);
    expect(afterWithdrawUnranked.events.map(e => e.event_type)).toEqual([
      'task.published',
      'task.withdrawn',
    ]);
    expect(afterWithdrawUnranked.events[1].data).toEqual({
      bounty_id: draft.id,
      project_id: p1.id,
    });
    expect(afterWithdrawUnranked.events[1].data.title).toBeUndefined();

    const afterWithdrawB = await client.agentListEvents(tierBSub1.api_key);
    expect(afterWithdrawB.events.map(e => `${e.event_type}:${e.bounty_id}`)).toEqual([
      `task.published:${draft.id}`,
      `task.published:${openMinB.id}`,
      `task.withdrawn:${draft.id}`,
    ]);
    expect((await client.agentListEvents(unsubAgent.api_key)).events).toHaveLength(1);

    // 再次发布 draft：按最新订阅重新计算目标（unranked-p1 已取消订阅收不到，unsub-p2 新订阅收到）
    await client.publishBounty(draft.id);
    expect((await client.agentListEvents(unrankedSub1.api_key)).events).toHaveLength(2);
    expect((await client.agentListEvents(unsubAgent.api_key)).events.map(e => e.event_type)).toEqual([
      'task.assigned',
      'task.published',
    ]);
  });

  it('支持游标补拉、分页、Ack 钳位不回退与实时流连接/last_pushed_at 更新', async () => {
    const client = createMemoryBountyClient({ lan_open_registration: true });
    const p1 = await client.createProject({ name: 'StreamProj' });
    const reg = await client.registerAgent({
      name: 'stream-agent',
      webhook_url: 'https://agent.example.com/hook',
    });

    // 初始空收件箱与参数校验
    const empty = await client.agentListEvents(reg.api_key);
    expect(empty).toEqual({
      events: [],
      cursor: 0,
      acked_event_id: 0,
      latest_event_id: 0,
      has_more: false,
    });
    await expect(client.agentListEvents(reg.api_key, { cursor: -1 })).rejects.toMatchObject({
      code: 'VALIDATION_ERROR',
      status: 400,
    });
    await expect(client.agentListEvents(reg.api_key, { limit: 0 })).rejects.toMatchObject({
      code: 'VALIDATION_ERROR',
      status: 400,
    });
    await expect(client.agentAckEvents(reg.api_key, -1)).rejects.toMatchObject({
      code: 'VALIDATION_ERROR',
      status: 400,
    });

    // 发布 3 个任务产生 3 条事件
    await client.createBounty({ project_id: p1.id, title: 'T1', difficulty: 'C', assignee_mode: 'any', status: 'OPEN' });
    await client.createBounty({ project_id: p1.id, title: 'T2', difficulty: 'C', assignee_mode: 'any', status: 'OPEN' });
    await client.createBounty({ project_id: p1.id, title: 'T3', difficulty: 'C', assignee_mode: 'any', status: 'OPEN' });

    const page1 = await client.agentListEvents(reg.api_key, { limit: 2 });
    expect(page1.events).toHaveLength(2);
    expect(page1.has_more).toBe(true);
    expect(page1.cursor).toBe(page1.events[1].event_id);
    expect(page1.acked_event_id).toBe(0);
    expect(page1.latest_event_id).toBeGreaterThan(page1.cursor);

    const page2 = await client.agentListEvents(reg.api_key, { cursor: page1.cursor, limit: 2 });
    expect(page2.events).toHaveLength(1);
    expect(page2.has_more).toBe(false);
    expect(page2.cursor).toBe(page2.latest_event_id);

    // 空页保留请求 cursor
    const emptyTail = await client.agentListEvents(reg.api_key, { cursor: page2.latest_event_id });
    expect(emptyTail.events).toEqual([]);
    expect(emptyTail.cursor).toBe(page2.latest_event_id);

    // Ack 第 1 条，再尝试回退 Ack 0 不回退；尝试超前 Ack 99999 钳位至自身最大 event_id
    const ack1 = await client.agentAckEvents(reg.api_key, page1.events[0].event_id);
    expect(ack1.acked_event_id).toBe(page1.events[0].event_id);
    const ackRegress = await client.agentAckEvents(reg.api_key, 0);
    expect(ackRegress.acked_event_id).toBe(page1.events[0].event_id);

    // 建立内存实时流：立即补发 acked_event_id 之后的积压事件，切换为 streaming，并更新 last_pushed_at
    const received: number[] = [];
    const unsubscribe = await client.agentOpenStream(reg.api_key, ev => {
      received.push(ev.event_id);
    });
    expect(received).toEqual([page1.events[1].event_id, page2.events[0].event_id]);

    const streamingMe = await client.agentGetMe(reg.api_key);
    expect(streamingMe.channel_status).toBe('streaming');
    expect(streamingMe.stream_connected).toBe(true);
    expect(streamingMe.webhook_configured).toBe(true);
    expect(streamingMe.last_pushed_at).not.toBeNull();

    // 实时发布第 4 个任务直接推送给流监听者
    await client.createBounty({ project_id: p1.id, title: 'T4', difficulty: 'C', assignee_mode: 'any', status: 'OPEN' });
    expect(received).toHaveLength(3);

    const ackClamped = await client.agentAckEvents(reg.api_key, 999_999);
    expect(ackClamped.acked_event_id).toBe(received[2]);

    // 关闭流连接后恢复为 webhook（因仍配置了 webhook_url）
    unsubscribe();
    const afterClose = await client.agentGetMe(reg.api_key);
    expect(afterClose.channel_status).toBe('webhook');
    expect(afterClose.stream_connected).toBe(false);

    // 模拟流与推送辅助方法覆盖
    const simStream = await client.simulateAgentStream(reg.agent.id, true);
    expect(simStream.channel_status).toBe('streaming');
    await client.disableAgent(reg.agent.id);
    const disabledSummary = (await client.listAgents())[0];
    expect(disabledSummary.stream_connected).toBe(false);
    expect(disabledSummary.channel_status).toBe('webhook');

    const pushed = await client.simulateAgentPush(reg.agent.id, {
      pushedAt: '2026-11-17T08:00:00.000Z',
      recentFailedCount: 2,
    });
    expect(pushed.last_pushed_at).toBe('2026-11-17T08:00:00.000Z');
  });
});

describe('Issue 04 通道状态格式化与中英文翻译', () => {
  it('formatAgentChannelStatus 正确输出中英文状态且不含 · 分隔符', () => {
    setLocalePreference('zh-CN');
    const streamingZh = formatAgentChannelStatus({
      channel_status: 'streaming',
      stream_connected: true,
      webhook_configured: false,
    });
    const webhookZh = formatAgentChannelStatus({
      channel_status: 'webhook',
      stream_connected: false,
      webhook_configured: true,
    });
    const offlineZh = formatAgentChannelStatus({
      channel_status: 'offline',
      stream_connected: false,
      webhook_configured: false,
    });
    expect(streamingZh).toEqual({ kind: 'streaming', label: '在线长连接' });
    expect(webhookZh).toEqual({ kind: 'webhook', label: 'Webhook' });
    expect(offlineZh).toEqual({ kind: 'offline', label: '离线' });

    setLocalePreference('en');
    const streamingEn = formatAgentChannelStatus({
      channel_status: 'streaming',
      stream_connected: true,
      webhook_configured: true,
    });
    const webhookEn = formatAgentChannelStatus({
      channel_status: 'webhook',
      stream_connected: false,
      webhook_configured: true,
    });
    const offlineEn = formatAgentChannelStatus({
      channel_status: 'offline',
      stream_connected: false,
      webhook_configured: false,
    });
    expect(streamingEn).toEqual({ kind: 'streaming', label: 'Live stream' });
    expect(webhookEn).toEqual({ kind: 'webhook', label: 'Webhook' });
    expect(offlineEn).toEqual({ kind: 'offline', label: 'Offline' });

    for (const entry of [streamingZh, webhookZh, offlineZh, streamingEn, webhookEn, offlineEn]) {
      expect(entry.label).not.toContain('·');
    }
    setLocalePreference('zh-CN');
  });
});

describe('Issue 05 Agent 自动发单与项目级草稿审核策略', () => {
  it('HttpAgentClient.createBounty 正确发送 POST /api/v1/agent/bounties 请求', async () => {
    const calls: Array<{ url: string; method?: string; headers: Record<string, string>; body?: string }> = [];
    const fetchMock: typeof fetch = async (input, init) => {
      calls.push({
        url: String(input),
        method: init?.method,
        headers: (init?.headers ?? {}) as Record<string, string>,
        body: typeof init?.body === 'string' ? init.body : undefined,
      });
      return jsonResponse(
        {
          id: 'b-agent-1',
          project_id: 'p1',
          title: 'Fix CI flakiness',
          description: 'Logs attached',
          acceptance_criteria: 'npm test passes',
          difficulty: 'B',
          assignee_mode: 'min_tier',
          assignee_min_tier: 'C',
          assignee_agent_id: null,
          require_arbitration: true,
          status: 'DRAFT',
          created_by: 'agent:ag-1',
          created_by_agent_id: 'ag-1',
          created_by_agent_name: 'inspector-bot',
          lease_id: 0,
          lease_agent_id: null,
          lease_expires_at: null,
          active_duration_ms: 0,
          active_started_at: null,
          comment_count: 0,
          rejection_count: 0,
          completion_note: null,
          published_at: null,
          withdrawn_at: null,
          cancelled_at: null,
          completed_at: null,
          created_at: '2026-11-17T03:00:00.000Z',
          updated_at: '2026-11-17T03:00:00.000Z',
        },
        201,
      );
    };

    const agentClient = new HttpAgentClient('http://127.0.0.1:8420/', 'tr_ag_publisher', fetchMock);
    const created = await agentClient.createBounty({
      project_id: 'p1',
      title: 'Fix CI flakiness',
      description: 'Logs attached',
      acceptance_criteria: 'npm test passes',
      difficulty: 'B',
      assignee_mode: 'min_tier',
      assignee_min_tier: 'C',
      require_arbitration: true,
    });

    expect(calls).toHaveLength(1);
    expect(calls[0].method).toBe('POST');
    expect(calls[0].url).toBe('http://127.0.0.1:8420/api/v1/agent/bounties');
    expect(calls[0].headers.Authorization).toBe('Bearer tr_ag_publisher');
    expect(calls[0].headers['Content-Type']).toBe('application/json');
    expect(JSON.parse(calls[0].body!)).toEqual({
      project_id: 'p1',
      title: 'Fix CI flakiness',
      description: 'Logs attached',
      acceptance_criteria: 'npm test passes',
      difficulty: 'B',
      assignee_mode: 'min_tier',
      assignee_min_tier: 'C',
      require_arbitration: true,
    });
    expect(created.created_by_agent_id).toBe('ag-1');
    expect(created.created_by_agent_name).toBe('inspector-bot');
    expect(created.status).toBe('DRAFT');
  });

  it('createMemoryBountyClient 校验鉴权、未知/越权字段、项目状态及 SKILL.md 发单契约', async () => {
    const client = createMemoryBountyClient({ lan_open_registration: true });
    const activeProject = await client.createProject({ name: 'Active Project' });
    const archivedProject = await client.createProject({ name: 'Archived Project' });
    await client.updateProject(archivedProject.id, { archived: true });

    const publisher = await client.registerAgent({ name: 'scout-agent' });
    const disabled = await client.registerAgent({ name: 'disabled-agent' });
    const revoked = await client.registerAgent({ name: 'revoked-agent' });
    await client.disableAgent(disabled.agent.id);
    await client.revokeAgent(revoked.agent.id);

    // 鉴权与状态拦截
    await expect(
      client.agentCreateBounty('tr_ag_unknown', {
        project_id: activeProject.id,
        title: 'Task',
        difficulty: 'C',
        assignee_mode: 'any',
      }),
    ).rejects.toMatchObject({ code: 'UNAUTHORIZED', status: 401 });

    await expect(
      client.agentCreateBounty(disabled.api_key, {
        project_id: activeProject.id,
        title: 'Task',
        difficulty: 'C',
        assignee_mode: 'any',
      }),
    ).rejects.toMatchObject({ code: 'AGENT_DISABLED', status: 403 });

    await expect(
      client.agentCreateBounty(revoked.api_key, {
        project_id: activeProject.id,
        title: 'Task',
        difficulty: 'C',
        assignee_mode: 'any',
      }),
    ).rejects.toMatchObject({ code: 'AGENT_REVOKED', status: 401 });

    // 越权字段（status / created_by / created_by_agent_id）被拒绝
    for (const extra of [
      { status: 'OPEN' },
      { created_by: 'human' },
      { created_by_agent_id: 'fake' },
    ]) {
      await expect(
        client.agentCreateBounty(publisher.api_key, {
          project_id: activeProject.id,
          title: 'Forbidden extra',
          difficulty: 'C',
          assignee_mode: 'any',
          ...extra,
        } as never),
      ).rejects.toMatchObject({ code: 'VALIDATION_ERROR', status: 400 });
    }

    // 标题空白、执行者组合不合法、未知项目与归档项目拦截
    await expect(
      client.agentCreateBounty(publisher.api_key, {
        project_id: activeProject.id,
        title: '   ',
        difficulty: 'C',
        assignee_mode: 'any',
      }),
    ).rejects.toMatchObject({ code: 'VALIDATION_ERROR', status: 400 });

    await expect(
      client.agentCreateBounty(publisher.api_key, {
        project_id: activeProject.id,
        title: 'Bad assignee',
        difficulty: 'C',
        assignee_mode: 'min_tier',
      }),
    ).rejects.toMatchObject({ code: 'VALIDATION_ERROR', status: 400 });

    await expect(
      client.agentCreateBounty(publisher.api_key, {
        project_id: 'missing-project',
        title: 'Unknown project',
        difficulty: 'C',
        assignee_mode: 'any',
      }),
    ).rejects.toMatchObject({ code: 'PROJECT_NOT_FOUND', status: 404 });

    await expect(
      client.agentCreateBounty(publisher.api_key, {
        project_id: archivedProject.id,
        title: 'Archived project',
        difficulty: 'C',
        assignee_mode: 'any',
      }),
    ).rejects.toMatchObject({ code: 'PROJECT_ARCHIVED', status: 409 });

    // SKILL.md 包含 /api/v1/agent/bounties 与 agent_draft_policy 说明
    const skillDoc = await client.getSkillMarkdown({ baseUrl: 'http://127.0.0.1:8420' });
    expect(skillDoc.content).toContain('POST http://127.0.0.1:8420/api/v1/agent/bounties');
    expect(skillDoc.content).toContain('agent_draft_policy');
  });

  it('支持 open 与 draft 双策略发单、草稿隔离及人工确认发布保留发布者身份', async () => {
    const client = createMemoryBountyClient({ lan_open_registration: true });
    const openProj = await client.createProject({
      name: 'Open Policy Project',
      agent_draft_policy: 'open',
    });
    const draftProj = await client.createProject({
      name: 'Draft Policy Project',
      agent_draft_policy: 'draft',
    });

    const publisher = await client.registerAgent({
      name: 'feedback-watcher',
      subscribe_all_projects: true,
    });
    const worker = await client.registerAgent({
      name: 'worker-bot',
      subscribe_all_projects: true,
    });

    const streamedToWorker: string[] = [];
    const closeWorkerStream = await client.agentOpenStream(worker.api_key, ev => {
      streamedToWorker.push(`${ev.event_type}:${ev.bounty_id}`);
    });

    // 1. open 策略：立即 OPEN、published_at 非空、写入身份字段与 created 事件并触发订阅分发
    const openTask = await client.agentCreateBounty(publisher.api_key, {
      project_id: openProj.id,
      title: 'Fix login redirect',
      description: 'Reported by monitor',
      acceptance_criteria: 'E2E passes',
      difficulty: 'B',
      assignee_mode: 'any',
    });
    expect(openTask.status).toBe('OPEN');
    expect(openTask.published_at).not.toBeNull();
    expect(openTask.created_by).toBe(`agent:${publisher.agent.id}`);
    expect(openTask.created_by_agent_id).toBe(publisher.agent.id);
    expect(openTask.created_by_agent_name).toBe('feedback-watcher');

    const openDetail = await client.getBountyDetail(openTask.id);
    expect(openDetail.events).toHaveLength(1);
    expect(openDetail.events[0]).toMatchObject({
      event_type: 'created',
      actor: `agent:${publisher.agent.id}`,
      from_status: null,
      to_status: 'OPEN',
      detail: {
        publisher_agent_id: publisher.agent.id,
        publisher_agent_name: 'feedback-watcher',
        policy: 'open',
      },
    });
    expect(streamedToWorker).toEqual([`task.published:${openTask.id}`]);
    const workerInboxAfterOpen = await client.agentListEvents(worker.api_key);
    expect(workerInboxAfterOpen.events.map(e => e.bounty_id)).toEqual([openTask.id]);

    // 2. draft 策略：落入 DRAFT、published_at 为 null、绝不分发事件且对所有 Agent（含发布者）隔离
    const draftTask = await client.agentCreateBounty(publisher.api_key, {
      project_id: draftProj.id,
      title: 'Needs human triage',
      description: 'Draft body',
      difficulty: 'A',
      assignee_mode: 'specific',
      assignee_agent_id: worker.agent.id,
      require_arbitration: true,
    });
    expect(draftTask.status).toBe('DRAFT');
    expect(draftTask.published_at).toBeNull();
    expect(draftTask.created_by).toBe(`agent:${publisher.agent.id}`);
    expect(draftTask.created_by_agent_id).toBe(publisher.agent.id);
    expect(draftTask.created_by_agent_name).toBe('feedback-watcher');

    const draftDetailAdmin = await client.getBountyDetail(draftTask.id);
    expect(draftDetailAdmin.events[0]).toMatchObject({
      event_type: 'created',
      actor: `agent:${publisher.agent.id}`,
      from_status: null,
      to_status: 'DRAFT',
      detail: {
        publisher_agent_id: publisher.agent.id,
        publisher_agent_name: 'feedback-watcher',
        policy: 'draft',
      },
    });

    // 无任何推送或收件箱事件
    expect(streamedToWorker).toEqual([`task.published:${openTask.id}`]);
    expect((await client.agentListEvents(worker.api_key)).events).toHaveLength(1);
    expect((await client.agentListEvents(publisher.api_key)).events).toHaveLength(1);

    // 包括发布者在内的所有 Agent 不可见 DRAFT
    for (const key of [publisher.api_key, worker.api_key]) {
      const visibleIds = (await client.agentListBounties(key, { include_drafts: true })).map(b => b.id);
      expect(visibleIds).toEqual([openTask.id]);
      await expect(client.agentGetBountyDetail(key, draftTask.id)).rejects.toMatchObject({
        code: 'BOUNTY_NOT_FOUND',
        status: 404,
      });
    }

    // 管理员编辑并确认发布后转为 OPEN，保留原发布者身份并触发定向指派分发
    const edited = await client.updateBounty(draftTask.id, { title: 'Triaged & Ready' });
    expect(edited.created_by).toBe(`agent:${publisher.agent.id}`);
    expect(edited.created_by_agent_id).toBe(publisher.agent.id);
    expect(edited.created_by_agent_name).toBe('feedback-watcher');

    const published = await client.publishBounty(draftTask.id);
    expect(published.status).toBe('OPEN');
    expect(published.published_at).not.toBeNull();
    expect(published.created_by).toBe(`agent:${publisher.agent.id}`);
    expect(published.created_by_agent_id).toBe(publisher.agent.id);
    expect(published.created_by_agent_name).toBe('feedback-watcher');

    expect(streamedToWorker).toEqual([
      `task.published:${openTask.id}`,
      `task.assigned:${draftTask.id}`,
    ]);
    expect((await client.agentGetBountyDetail(worker.api_key, draftTask.id)).bounty.title).toBe(
      'Triaged & Ready',
    );

    const discardedDraft = await client.agentCreateBounty(publisher.api_key, {
      project_id: draftProj.id,
      title: 'Private discarded draft',
      description: 'UNAPPROVED_PRIVATE_BODY',
      difficulty: 'C',
      assignee_mode: 'any',
    });
    const cancelledDraft = await client.cancelBounty(discardedDraft.id);
    expect(cancelledDraft.status).toBe('CANCELLED');
    expect(cancelledDraft.published_at).toBeNull();
    expect(cancelledDraft.created_by_agent_id).toBe(publisher.agent.id);
    for (const key of [publisher.api_key, worker.api_key]) {
      const visible = await client.agentListBounties(key, {
        status: 'CANCELLED',
        include_drafts: true,
      });
      expect(visible.some(b => b.id === discardedDraft.id)).toBe(false);
      await expect(client.agentGetBountyDetail(key, discardedDraft.id)).rejects.toMatchObject({
        code: 'BOUNTY_NOT_FOUND',
        status: 404,
      });
    }

    const humanTask = await client.createBounty({
      project_id: openProj.id,
      title: 'Human created',
      difficulty: 'C',
      assignee_mode: 'any',
      status: 'OPEN',
    });
    expect(humanTask.created_by).toBe('human');
    expect(humanTask.created_by_agent_id).toBeNull();
    expect(humanTask.created_by_agent_name).toBeNull();

    closeWorkerStream();
  });

  it('formatAgentDraftPolicy、formatBountyPublisher 与 formatEventActor 支持中英文且不含 · 分隔符', () => {
    const agentBounty = {
      created_by: 'agent:ag-1',
      created_by_agent_id: 'ag-1',
      created_by_agent_name: 'Scout',
    };
    const agentFallbackIdBounty = {
      created_by: 'agent:ag-2',
      created_by_agent_id: 'ag-2',
      created_by_agent_name: null,
    };
    const agentPrefixOnlyBounty = {
      created_by: 'agent:legacy-bot',
      created_by_agent_id: null,
      created_by_agent_name: null,
    };
    const humanBounty = {
      created_by: 'human',
      created_by_agent_id: null,
      created_by_agent_name: null,
    };

    setLocalePreference('zh-CN');
    expect(formatAgentDraftPolicy('open')).toBe('直接公开');
    expect(formatAgentDraftPolicy('draft')).toBe('先入草稿待审');
    expect(formatBountyPublisher(agentBounty)).toEqual({
      isAgent: true,
      label: 'Agent 提交: Scout',
    });
    expect(formatBountyPublisher(agentFallbackIdBounty)).toEqual({
      isAgent: true,
      label: 'Agent 提交: ag-2',
    });
    expect(formatBountyPublisher(agentPrefixOnlyBounty)).toEqual({
      isAgent: true,
      label: 'Agent 提交: legacy-bot',
    });
    expect(formatBountyPublisher(humanBounty)).toEqual({
      isAgent: false,
      label: '人工创建',
    });
    expect(formatEventActor('human', agentBounty)).toBe('管理员');
    expect(formatEventActor('agent:ag-1', agentBounty)).toBe('Agent: Scout');
    expect(formatEventActor('ag-1', agentBounty)).toBe('Agent: Scout');
    expect(formatEventActor('agent:ag-2', agentFallbackIdBounty)).toBe('Agent: ag-2');

    setLocalePreference('en');
    expect(formatAgentDraftPolicy('open')).toBe('Publish directly');
    expect(formatAgentDraftPolicy('draft')).toBe('Hold as draft for review');
    expect(formatBountyPublisher(agentBounty)).toEqual({
      isAgent: true,
      label: 'Submitted by Agent: Scout',
    });
    expect(formatBountyPublisher(humanBounty)).toEqual({
      isAgent: false,
      label: 'Created manually',
    });
    expect(formatEventActor('human', agentBounty)).toBe('Admin');
    expect(formatEventActor('agent:ag-1', agentBounty)).toBe('Agent: Scout');

    for (const text of [
      formatAgentDraftPolicy('open'),
      formatAgentDraftPolicy('draft'),
      formatBountyPublisher(agentBounty).label,
      formatBountyPublisher(humanBounty).label,
      formatEventActor('human'),
      formatEventActor('agent:ag-1', agentBounty),
    ]) {
      expect(text).not.toContain('·');
    }
    setLocalePreference('zh-CN');
  });
});

describe('Issue 06: 租约令牌、并发抢单、指派婉拒、超时回收与无仲裁交付', () => {
  it('HttpBountyClient.releaseLease 与 HttpAgentClient 租约接口发送正确路由与正文', async () => {
    const calls: Array<{ url: string; method?: string; headers: Record<string, string>; body?: string }> = [];
    const fetchMock: typeof fetch = async (input, init) => {
      calls.push({
        url: String(input),
        method: init?.method,
        headers: (init?.headers ?? {}) as Record<string, string>,
        body: typeof init?.body === 'string' ? init.body : undefined,
      });
      return jsonResponse({ id: 'b1' }, 200);
    };

    const adminClient = new HttpBountyClient('http://127.0.0.1:8420/', 'admin-tok', fetchMock);
    await adminClient.releaseLease('b/1');

    const agentClient = new HttpAgentClient('http://127.0.0.1:8420/', 'tr_ag_secret', fetchMock);
    await agentClient.claimBounty('b/1');
    await agentClient.declineBounty('b/1');
    await agentClient.declineBounty('b/1', { reason: '超出能力范围' });
    await agentClient.heartbeatBounty('b/1', { lease_id: 2 });
    await agentClient.abandonBounty('b/1', { lease_id: 2, reason: '依赖缺失' });
    await agentClient.completeBounty('b/1', {
      lease_id: 2,
      completion_note: '测试全部通过',
      completion_cost: 1.25,
    });

    expect(calls.map(c => `${c.method} ${c.url}`)).toEqual([
      'POST http://127.0.0.1:8420/api/v1/bounties/b%2F1/release-lease',
      'POST http://127.0.0.1:8420/api/v1/agent/bounties/b%2F1/claim',
      'POST http://127.0.0.1:8420/api/v1/agent/bounties/b%2F1/decline',
      'POST http://127.0.0.1:8420/api/v1/agent/bounties/b%2F1/decline',
      'POST http://127.0.0.1:8420/api/v1/agent/bounties/b%2F1/heartbeat',
      'POST http://127.0.0.1:8420/api/v1/agent/bounties/b%2F1/abandon',
      'POST http://127.0.0.1:8420/api/v1/agent/bounties/b%2F1/complete',
    ]);
    expect(calls[0].headers.Authorization).toBe('Bearer admin-tok');
    expect(calls[0].body).toBeUndefined();
    expect(calls[1].headers.Authorization).toBe('Bearer tr_ag_secret');
    expect(calls[1].body).toBeUndefined();
    expect(calls[2].body).toBeUndefined();
    expect(JSON.parse(calls[3].body!)).toEqual({ reason: '超出能力范围' });
    expect(JSON.parse(calls[4].body!)).toEqual({ lease_id: 2 });
    expect(JSON.parse(calls[5].body!)).toEqual({ lease_id: 2, reason: '依赖缺失' });
    expect(JSON.parse(calls[6].body!)).toEqual({
      lease_id: 2,
      completion_note: '测试全部通过',
      completion_cost: 1.25,
    });
  });

  it('HttpAgentClient.completeBounty 与 MemoryBountyClient 一致拒绝 NaN、Infinity、-Infinity 与负数成本且不触发 fetch', async () => {
    const fetchCalls: Array<{ url: string; body?: string }> = [];
    const fetchMock: typeof fetch = async (input, init) => {
      fetchCalls.push({
        url: String(input),
        body: typeof init?.body === 'string' ? init.body : undefined,
      });
      return jsonResponse({ id: 'b-http', status: 'COMPLETED' }, 200);
    };
    const httpAgent = new HttpAgentClient('http://127.0.0.1:8420', 'tr_ag_secret', fetchMock);

    const memoryClient = createMemoryBountyClient({ lan_open_registration: true });
    const project = await memoryClient.createProject({ name: 'Cost Validation' });
    const agent = await memoryClient.registerAgent({ name: 'cost-checker' });
    const task = await memoryClient.createBounty({
      project_id: project.id,
      title: 'Cost check task',
      difficulty: 'C',
      assignee_mode: 'any',
      status: 'OPEN',
    });
    const claimed = await memoryClient.agentClaimBounty(agent.api_key, task.id);

    for (const invalidCost of [Number.NaN, Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY, -0.01, -5]) {
      await expect(
        httpAgent.completeBounty('b-http', {
          lease_id: 1,
          completion_cost: invalidCost,
        }),
      ).rejects.toMatchObject({
        code: 'VALIDATION_ERROR',
        status: 400,
      });
      await expect(
        memoryClient.agentCompleteBounty(agent.api_key, task.id, {
          lease_id: claimed.lease_id,
          completion_cost: invalidCost,
        }),
      ).rejects.toMatchObject({
        code: 'VALIDATION_ERROR',
        status: 400,
      });
    }
    expect(fetchCalls).toHaveLength(0);
    expect((await memoryClient.getBountyDetail(task.id)).bounty.status).toBe('IN_PROGRESS');

    // 有效非负有限值（0、正数）与省略/null 成本在两端均正常通过
    await httpAgent.completeBounty('b-http', { lease_id: 1, completion_cost: 0 });
    await httpAgent.completeBounty('b-http', { lease_id: 1, completion_cost: 2.5 });
    await httpAgent.completeBounty('b-http', { lease_id: 1 });
    await httpAgent.completeBounty('b-http', { lease_id: 1, completion_cost: null });
    expect(fetchCalls).toHaveLength(4);
    expect(JSON.parse(fetchCalls[0].body!)).toEqual({ lease_id: 1, completion_cost: 0 });
    expect(JSON.parse(fetchCalls[1].body!)).toEqual({ lease_id: 1, completion_cost: 2.5 });
    expect(JSON.parse(fetchCalls[2].body!)).toEqual({ lease_id: 1 });
    expect(JSON.parse(fetchCalls[3].body!)).toEqual({ lease_id: 1, completion_cost: null });

    const completedZero = await memoryClient.agentCompleteBounty(agent.api_key, task.id, {
      lease_id: claimed.lease_id,
      completion_cost: 0,
    });
    expect(completedZero.status).toBe('COMPLETED');
    expect(completedZero.completion_cost).toBe(0);

    const taskOmitted = await memoryClient.createBounty({
      project_id: project.id,
      title: 'Omitted cost task',
      difficulty: 'C',
      assignee_mode: 'any',
      status: 'OPEN',
    });
    const claimedOmitted = await memoryClient.agentClaimBounty(agent.api_key, taskOmitted.id);
    const completedOmitted = await memoryClient.agentCompleteBounty(
      agent.api_key,
      taskOmitted.id,
      { lease_id: claimedOmitted.lease_id },
    );
    expect(completedOmitted.status).toBe('COMPLETED');
    expect(completedOmitted.completion_cost).toBeNull();
  });

  it('MemoryBountyClient 并发抢单互斥、段位准入与递增 lease_id 脑裂校验', async () => {
    const client = createMemoryBountyClient({ lan_open_registration: true });
    const project = await client.createProject({ name: 'Lease Core' });
    const rookie = await client.registerAgent({ name: 'rookie-agent' });
    const senior = await client.registerAgent({ name: 'senior-agent' });
    await client.simulateAgentTier(senior.agent.id, 'B');

    const minTierTask = await client.createBounty({
      project_id: project.id,
      title: 'Tier B task',
      difficulty: 'B',
      assignee_mode: 'min_tier',
      assignee_min_tier: 'B',
      status: 'OPEN',
    });

    await expect(client.agentClaimBounty(rookie.api_key, minTierTask.id)).rejects.toMatchObject({
      code: 'TIER_INSUFFICIENT',
      status: 403,
    });

    const claimed = await client.agentClaimBounty(senior.api_key, minTierTask.id);
    expect(claimed.status).toBe('IN_PROGRESS');
    expect(claimed.lease_id).toBe(1);
    expect(claimed.lease_agent_id).toBe(senior.agent.id);
    expect(claimed.lease_agent_name).toBe('senior-agent');
    expect(claimed.lease_agent_tier).toBe('B');
    expect(claimed.lease_expires_at).not.toBeNull();

    await expect(client.agentClaimBounty(senior.api_key, minTierTask.id)).rejects.toMatchObject({
      code: 'BOUNTY_ALREADY_CLAIMED',
      status: 409,
    });

    await expect(
      client.agentHeartbeatBounty(rookie.api_key, minTierTask.id, { lease_id: 1 }),
    ).rejects.toMatchObject({ code: 'LEASE_OWNER_MISMATCH', status: 403 });

    await expect(
      client.agentCompleteBounty(senior.api_key, minTierTask.id, { lease_id: 0 }),
    ).rejects.toMatchObject({ code: 'STALE_LEASE', status: 409 });

    const abandoned = await client.agentAbandonBounty(senior.api_key, minTierTask.id, {
      lease_id: 1,
      reason: '需要交接',
    });
    expect(abandoned.status).toBe('OPEN');
    expect(abandoned.lease_id).toBe(1);
    expect(abandoned.lease_agent_id).toBeNull();
    expect((await client.agentGetMe(senior.api_key)).abandon_count).toBe(1);

    const reclaimed = await client.agentClaimBounty(senior.api_key, minTierTask.id);
    expect(reclaimed.lease_id).toBe(2);
    await expect(
      client.agentHeartbeatBounty(senior.api_key, minTierTask.id, { lease_id: 1 }),
    ).rejects.toMatchObject({ code: 'STALE_LEASE', status: 409 });

    const completed = await client.agentCompleteBounty(senior.api_key, minTierTask.id, {
      lease_id: 2,
      completion_note: '  已通过全量回归  ',
      completion_cost: 3.5,
    });
    expect(completed.status).toBe('COMPLETED');
    expect(completed.lease_id).toBe(2);
    expect(completed.lease_agent_id).toBe(senior.agent.id);
    expect(completed.lease_agent_name).toBe('senior-agent');
    expect(completed.lease_agent_tier).toBe('B');
    expect(completed.lease_expires_at).toBeNull();
    expect(completed.completion_note).toBe('已通过全量回归');
    expect(completed.completion_cost).toBe(3.5);
    expect(completed.completed_at).not.toBeNull();
  });

  it('定向指派婉拒、指派超时回退、租约临期提醒/到期回收、强制释放与仲裁拦截', async () => {
    const client = createMemoryBountyClient({ lan_open_registration: true });
    const project = await client.createProject({ name: 'Lease Flow' });
    const publisher = await client.registerAgent({ name: 'pub-bot' });
    const target = await client.registerAgent({ name: 'target-bot' });
    const bystander = await client.registerAgent({ name: 'bystander-bot' });

    const specificTask = await client.agentCreateBounty(publisher.api_key, {
      project_id: project.id,
      title: 'Specific task',
      difficulty: 'C',
      assignee_mode: 'specific',
      assignee_agent_id: target.agent.id,
    });
    expect(specificTask.assignment_expires_at).not.toBeNull();
    expect(specificTask.assignee_agent_name).toBe('target-bot');

    await expect(client.agentClaimBounty(bystander.api_key, specificTask.id)).rejects.toMatchObject({
      code: 'ASSIGNEE_MISMATCH',
      status: 403,
    });

    const declined = await client.agentDeclineBounty(target.api_key, specificTask.id, {
      reason: '正在处理其他任务',
    });
    expect(declined.status).toBe('OPEN');
    expect(declined.assignee_mode).toBe('any');
    expect(declined.assignee_agent_id).toBeNull();
    expect(declined.assignment_expires_at).toBeNull();

    const pubEventsAfterDecline = await client.agentListEvents(publisher.api_key);
    expect(pubEventsAfterDecline.events.some(e => e.event_type === 'task.assignment_declined')).toBe(
      true,
    );

    const claimedByBystander = await client.agentClaimBounty(bystander.api_key, specificTask.id);
    const expiresMs = Date.parse(claimedByBystander.lease_expires_at!);

    await client.sweepLeases(expiresMs - 2 * 60 * 1000);
    await client.sweepLeases(expiresMs - 60 * 1000);
    const expiringEvents = (await client.agentListEvents(bystander.api_key)).events.filter(
      e => e.event_type === 'lease.expiring' && e.bounty_id === specificTask.id,
    );
    expect(expiringEvents).toHaveLength(1);

    await client.sweepLeases(expiresMs + 1000);
    const afterTimeout = await client.getBountyDetail(specificTask.id);
    expect(afterTimeout.bounty.status).toBe('OPEN');
    expect(afterTimeout.bounty.lease_agent_id).toBeNull();
    expect(afterTimeout.events.some(e => e.event_type === 'lease_expired' && e.actor === 'system')).toBe(
      true,
    );
    expect((await client.agentGetMe(bystander.api_key)).timeout_count).toBe(1);
    expect(
      (await client.agentListEvents(bystander.api_key)).events.some(
        e => e.event_type === 'lease.expired',
      ),
    ).toBe(true);

    const timedAssignment = await client.agentCreateBounty(publisher.api_key, {
      project_id: project.id,
      title: 'Timed assignment',
      difficulty: 'C',
      assignee_mode: 'specific',
      assignee_agent_id: target.agent.id,
    });
    const assignmentExpiresMs = Date.parse(timedAssignment.assignment_expires_at!);
    await client.sweepLeases(assignmentExpiresMs + 1000);
    const fallbackTask = await client.getBountyDetail(timedAssignment.id);
    expect(fallbackTask.bounty.assignee_mode).toBe('any');
    expect(fallbackTask.events.some(e => e.event_type === 'assignment_expired')).toBe(true);
    expect(
      (await client.agentListEvents(publisher.api_key)).events.some(
        e => e.event_type === 'task.assignment_expired',
      ),
    ).toBe(true);

    const arbTask = await client.createBounty({
      project_id: project.id,
      title: 'Arbitrated task',
      difficulty: 'C',
      assignee_mode: 'specific',
      assignee_agent_id: target.agent.id,
      require_arbitration: true,
      status: 'OPEN',
    });
    const arbClaimed = await client.agentClaimBounty(target.api_key, arbTask.id);
    await expect(
      client.agentCompleteBounty(target.api_key, arbTask.id, { lease_id: arbClaimed.lease_id }),
    ).rejects.toMatchObject({ code: 'ARBITRATION_REQUIRED', status: 409 });

    const released = await client.releaseLease(arbTask.id);
    expect(released.status).toBe('OPEN');
    expect(released.lease_agent_id).toBeNull();
    expect(released.assignee_mode).toBe('specific');
    expect(released.assignee_agent_id).toBe(target.agent.id);
    expect(released.assignment_expires_at).not.toBeNull();
    expect((await client.agentGetMe(target.api_key)).timeout_count).toBe(0);
    expect((await client.agentGetMe(target.api_key)).abandon_count).toBe(0);
    expect(
      (await client.agentListEvents(target.api_key)).events.some(
        e => e.event_type === 'lease.released',
      ),
    ).toBe(true);
  });

  it('helpers 格式化执行者、倒计时、成本、系统调度事件与租约错误码', () => {
    setLocalePreference('zh-CN');
    expect(
      formatBountyExecutor({
        lease_agent_id: 'ag-1',
        lease_agent_name: 'Runner',
        lease_agent_tier: 'A',
      }),
    ).toEqual({
      agentId: 'ag-1',
      name: 'Runner',
      tier: 'A',
      label: 'Runner（A 级）',
    });
    expect(
      formatBountyExecutor({
        lease_agent_id: null,
        lease_agent_name: null,
        lease_agent_tier: null,
      }),
    ).toBeNull();

    const baseNow = Date.parse('2026-11-17T10:00:00.000Z');
    expect(formatLeaseCountdown('2026-11-17T10:15:00.000Z', baseNow)).toEqual({
      expired: false,
      expiringSoon: false,
      remainingMs: 15 * 60 * 1000,
      label: '15:00',
    });
    expect(formatLeaseCountdown('2026-11-17T10:02:05.000Z', baseNow)).toEqual({
      expired: false,
      expiringSoon: true,
      remainingMs: 125_000,
      label: '02:05',
    });
    expect(formatLeaseCountdown('2026-11-17T09:59:59.000Z', baseNow)).toEqual({
      expired: true,
      expiringSoon: true,
      remainingMs: 0,
      label: '已超时待回收',
    });

    expect(formatCompletionCost(null)).toBeNull();
    expect(formatCompletionCost(0)).toBe('0');
    expect(formatCompletionCost(2.5)).toBe('2.5');

    expect(
      formatAssigneeConstraint({
        assignee_mode: 'specific',
        assignee_min_tier: null,
        assignee_agent_id: 'ag-9',
        assignee_agent_name: 'TargetBot',
      }),
    ).toBe('指派: TargetBot');

    expect(formatEventLabel('declined')).toBe('婉拒指派');
    expect(formatEventLabel('lease_released')).toBe('释放租约');
    expect(formatEventLabel('lease_expired')).toBe('租约超时回收');
    expect(formatEventActor('system')).toBe('系统调度');
    expect(
      formatEventActor('agent:ag-1', {
        lease_agent_id: 'ag-1',
        lease_agent_name: 'Runner',
      }),
    ).toBe('Agent: Runner');

    expect(formatServiceError(new ServiceApiError('STALE_LEASE', 409, 'raw'))).toBe(
      '租约令牌已过期或租约已失效，请重新获取最新任务状态。',
    );
    expect(formatServiceError(new ServiceApiError('ARBITRATION_REQUIRED', 409, 'raw'))).toBe(
      '该任务已开启仲裁验收，不能直接标记为完成。',
    );

    setLocalePreference('en');
    expect(
      formatBountyExecutor({
        lease_agent_id: 'ag-1',
        lease_agent_name: 'Runner',
        lease_agent_tier: 'Unranked',
      })?.label,
    ).toBe('Runner (Unranked)');
    expect(formatLeaseCountdown('2026-11-17T09:59:59.000Z', baseNow)?.label).toBe(
      'Expired, awaiting reclaim',
    );
    expect(formatEventActor('system')).toBe('System scheduler');
    expect(formatEventLabel('completed')).toBe('Completed delivery');
    expect(formatServiceError(new ServiceApiError('TIER_INSUFFICIENT', 403, 'raw'))).toBe(
      "This Agent's tier does not meet the minimum requirement for this bounty.",
    );

    for (const sample of [
      formatBountyExecutor({
        lease_agent_id: 'ag-1',
        lease_agent_name: 'Runner',
        lease_agent_tier: 'A',
      })!.label,
      formatEventActor('system'),
      formatEventLabel('lease_released'),
      formatServiceError(new ServiceApiError('STALE_LEASE', 409, 'raw')),
    ]) {
      expect(sample).not.toContain('·');
    }
    setLocalePreference('zh-CN');
  });
});

describe('Issue 07 评论区、提问挂起与回复唤醒', () => {
  it('HttpBountyClient 与 HttpAgentClient 透传空白正文和有限整数令牌并保持与内存客户端错误优先级一致', async () => {
    const calls: Array<{ url: string; method?: string; headers: Record<string, string>; body?: string }> = [];
    const sampleComment = {
      id: 1,
      bounty_id: 'b-7',
      author_type: 'agent',
      author_id: 'ag-exec',
      author_name: 'worker',
      kind: 'question',
      body: '请确认上界 $O(n \\log n)$ 约束。',
      created_at: '2026-11-17T12:01:00.000Z',
    };
    const sampleBounty = {
      id: 'b-7',
      project_id: 'p1',
      title: 'Issue 07 Task',
      description: '',
      acceptance_criteria: '',
      difficulty: 'B',
      assignee_mode: 'any',
      assignee_min_tier: null,
      assignee_agent_id: null,
      require_arbitration: false,
      status: 'WAITING_INFO',
      created_by: 'human',
      created_by_agent_id: null,
      created_by_agent_name: null,
      lease_id: 1,
      lease_agent_id: 'ag-exec',
      lease_agent_name: 'worker',
      lease_agent_tier: 'A',
      lease_expires_at: null,
      active_duration_ms: 12000,
      active_started_at: null,
      comment_count: 1,
      rejection_count: 0,
      completion_note: null,
      completion_cost: null,
      published_at: '2026-11-17T12:00:00.000Z',
      withdrawn_at: null,
      cancelled_at: null,
      completed_at: null,
      created_at: '2026-11-17T12:00:00.000Z',
      updated_at: '2026-11-17T12:01:00.000Z',
    };

    const fetchMock: typeof fetch = async (input, init) => {
      const url = String(input);
      const bodyStr = typeof init?.body === 'string' ? init.body : undefined;
      calls.push({
        url,
        method: init?.method,
        headers: (init?.headers ?? {}) as Record<string, string>,
        body: bodyStr,
      });
      if (url.includes('/bounties/b-cancelled/comments') && init?.method === 'POST') {
        return jsonResponse(
          { error: { code: 'INVALID_TRANSITION', message: '当前状态不允许此操作' } },
          409,
        );
      }
      if (url.includes('/request-info')) {
        const parsed = bodyStr ? (JSON.parse(bodyStr) as { lease_id?: number }) : {};
        if (parsed.lease_id !== 1) {
          return jsonResponse(
            { error: { code: 'STALE_LEASE', message: '租约令牌已失效或租约已到期' } },
            409,
          );
        }
        return jsonResponse(sampleBounty);
      }
      if (url.endsWith('/comments') && init?.method === 'POST') {
        return jsonResponse(
          { bounty: { ...sampleBounty, status: 'IN_PROGRESS' }, comment: sampleComment },
          201,
        );
      }
      return jsonResponse([sampleComment]);
    };

    const admin = new HttpBountyClient('http://127.0.0.1:8420/', 'admin-tok', fetchMock);
    const agent = new HttpAgentClient('http://127.0.0.1:8420/', 'tr_ag_worker', fetchMock);

    // 仅拦截会被 JSON.stringify 破坏为 null 的非有限数字 lease_id（不触发网络）
    for (const invalidLease of [Number.NaN, Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY]) {
      await expect(
        agent.requestInfo('b-7', { lease_id: invalidLease, question: 'valid?' }),
      ).rejects.toMatchObject({
        code: 'VALIDATION_ERROR',
        status: 400,
      });
    }
    expect(calls).toHaveLength(0);

    // 与 MemoryBountyClient 一致：错误 lease_id + 空白 question 必须到达服务端并透传 409 STALE_LEASE
    const mem = createMemoryBountyClient({ lan_open_registration: true });
    const proj = await mem.createProject({ name: 'Parity Project' });
    const worker = await mem.registerAgent({ name: 'parity-worker' });
    const activeTask = await mem.createBounty({
      project_id: proj.id,
      title: 'Active Task',
      difficulty: 'C',
      assignee_mode: 'any',
      status: 'OPEN',
    });
    const claimed = await mem.agentClaimBounty(worker.api_key, activeTask.id);
    const cancelledTask = await mem.createBounty({
      project_id: proj.id,
      title: 'Cancelled Task',
      difficulty: 'C',
      assignee_mode: 'any',
      status: 'OPEN',
    });
    await mem.cancelBounty(cancelledTask.id);

    for (const staleLeaseId of [0, claimed.lease_id + 99]) {
      await expect(
        agent.requestInfo('b-7', { lease_id: staleLeaseId, question: '   ' }),
      ).rejects.toMatchObject({
        code: 'STALE_LEASE',
        status: 409,
      });
      await expect(
        mem.agentRequestInfo(worker.api_key, activeTask.id, {
          lease_id: staleLeaseId,
          question: '   ',
        }),
      ).rejects.toMatchObject({
        code: 'STALE_LEASE',
        status: 409,
      });
    }

    // 与 MemoryBountyClient 一致：CANCELLED 任务 + 空白 body 必须到达服务端并透传 409 INVALID_TRANSITION
    await expect(
      admin.createBountyComment('b-cancelled', { body: '   ' }),
    ).rejects.toMatchObject({
      code: 'INVALID_TRANSITION',
      status: 409,
    });
    await expect(
      agent.createComment('b-cancelled', { body: '\n\t' }),
    ).rejects.toMatchObject({
      code: 'INVALID_TRANSITION',
      status: 409,
    });
    await expect(
      mem.createBountyComment(cancelledTask.id, { body: '   ' }),
    ).rejects.toMatchObject({
      code: 'INVALID_TRANSITION',
      status: 409,
    });
    await expect(
      mem.agentCreateComment(worker.api_key, cancelledTask.id, { body: '\n\t' }),
    ).rejects.toMatchObject({
      code: 'INVALID_TRANSITION',
      status: 409,
    });

    const errorCallsCount = calls.length;
    expect(errorCallsCount).toBe(4);
    expect(JSON.parse(calls[0].body!)).toEqual({ lease_id: 0, question: '   ' });
    expect(JSON.parse(calls[1].body!)).toEqual({ lease_id: 100, question: '   ' });
    expect(JSON.parse(calls[2].body!)).toEqual({ body: '   ' });
    expect(JSON.parse(calls[3].body!)).toEqual({ body: '\n\t' });

    const adminList = await admin.listBountyComments('b-7');
    expect(adminList).toHaveLength(1);
    const adminCreated = await admin.createBountyComment('b-7', { body: '已补充约束说明' });
    expect(adminCreated.bounty.status).toBe('IN_PROGRESS');

    const agentList = await agent.listComments('b-7');
    expect(agentList).toHaveLength(1);
    await agent.createComment('b-7', { body: '收到，继续执行' });
    const reqInfoResult = await agent.requestInfo('b-7', { lease_id: 1, question: '确认输入范围？' });
    expect(reqInfoResult.status).toBe('WAITING_INFO');

    expect(calls.slice(errorCallsCount).map(c => `${c.method} ${c.url}`)).toEqual([
      'GET http://127.0.0.1:8420/api/v1/bounties/b-7/comments',
      'POST http://127.0.0.1:8420/api/v1/bounties/b-7/comments',
      'GET http://127.0.0.1:8420/api/v1/agent/bounties/b-7/comments',
      'POST http://127.0.0.1:8420/api/v1/agent/bounties/b-7/comments',
      'POST http://127.0.0.1:8420/api/v1/agent/bounties/b-7/request-info',
    ]);
  });

  it('MemoryBountyClient 允许定向任务发布者查看与回复唤醒，且冻结租约计入 active_lease_count 保持名录 busy', async () => {
    const client = createMemoryBountyClient({ lan_open_registration: true });
    const openProj = await client.createProject({ name: 'Specific Open', agent_draft_policy: 'open' });
    const draftProj = await client.createProject({ name: 'Specific Draft', agent_draft_policy: 'draft' });

    // 发布者 A 不订阅任何项目，仍应对自己发布的已公开定向任务具备可见性与回复唤醒权
    const agentA = await client.registerAgent({
      name: 'publisher-A',
      subscribe_all_projects: false,
      project_ids: [],
    });
    const agentB = await client.registerAgent({ name: 'assignee-B' });
    const agentC = await client.registerAgent({ name: 'outsider-C' });

    const specific = await client.agentCreateBounty(agentA.api_key, {
      project_id: openProj.id,
      title: 'A 定向指派给 B 的任务',
      difficulty: 'B',
      assignee_mode: 'specific',
      assignee_agent_id: agentB.agent.id,
    });

    // 第三方 C 对定向任务仍不可见
    expect((await client.agentListBounties(agentC.api_key)).some(item => item.id === specific.id)).toBe(false);
    await expect(client.agentGetBountyDetail(agentC.api_key, specific.id)).rejects.toMatchObject({
      code: 'BOUNTY_NOT_FOUND',
      status: 404,
    });
    await expect(client.agentListComments(agentC.api_key, specific.id)).rejects.toMatchObject({
      code: 'BOUNTY_NOT_FOUND',
      status: 404,
    });
    await expect(
      client.agentCreateComment(agentC.api_key, specific.id, { body: '越权评论' }),
    ).rejects.toMatchObject({
      code: 'BOUNTY_NOT_FOUND',
      status: 404,
    });

    // B 接单后提问挂起，租约冻结但名录中仍计为 busy (active_lease_count = 1)
    const claimed = await client.agentClaimBounty(agentB.api_key, specific.id);
    expect((await client.agentGetMe(agentB.api_key)).active_lease_count).toBe(1);

    const waiting = await client.agentRequestInfo(agentB.api_key, specific.id, {
      lease_id: claimed.lease_id,
      question: '请 A 确认定向任务的接口字段？',
    });
    expect(waiting.status).toBe('WAITING_INFO');
    expect(waiting.lease_expires_at).toBeNull();
    expect((await client.agentGetMe(agentB.api_key)).active_lease_count).toBe(1);

    const busyAgents = await client.listAgents({ activity: 'busy' });
    const idleAgents = await client.listAgents({ activity: 'idle' });
    expect(busyAgents.map(item => item.id)).toEqual([agentB.agent.id]);
    expect(busyAgents[0].active_lease_count).toBe(1);
    expect(idleAgents.some(item => item.id === agentB.agent.id)).toBe(false);

    // 发布者 A 在内存端可查看定向任务详情、读取提问并回复唤醒 B
    expect((await client.agentListBounties(agentA.api_key)).some(item => item.id === specific.id)).toBe(true);
    const detailForA = await client.agentGetBountyDetail(agentA.api_key, specific.id);
    expect(detailForA.comments).toHaveLength(1);
    const commentsForA = await client.agentListComments(agentA.api_key, specific.id);
    expect(commentsForA[0].body).toBe('请 A 确认定向任务的接口字段？');

    const bEventsBeforeReply = (await client.agentListEvents(agentB.api_key)).events.length;
    const replyRes = await client.agentCreateComment(agentA.api_key, specific.id, {
      body: '字段固定为 snake_case',
    });
    expect(replyRes.comment.kind).toBe('reply');
    expect(replyRes.bounty.status).toBe('IN_PROGRESS');
    expect(replyRes.bounty.lease_expires_at).not.toBeNull();

    const bEventsAfterReply = (await client.agentListEvents(agentB.api_key)).events.slice(
      bEventsBeforeReply,
    );
    expect(bEventsAfterReply.map(e => e.event_type)).toEqual(['comment.replied', 'task.resumed']);

    // DRAFT（含 A 自身创建的草稿）与未发布即取消的草稿继续对 A 严格隔离
    const ownDraft = await client.agentCreateBounty(agentA.api_key, {
      project_id: draftProj.id,
      title: 'A 自身的定向草稿',
      difficulty: 'C',
      assignee_mode: 'specific',
      assignee_agent_id: agentB.agent.id,
    });
    expect(ownDraft.status).toBe('DRAFT');
    await expect(client.agentGetBountyDetail(agentA.api_key, ownDraft.id)).rejects.toMatchObject({
      code: 'BOUNTY_NOT_FOUND',
      status: 404,
    });
    await expect(client.agentListComments(agentA.api_key, ownDraft.id)).rejects.toMatchObject({
      code: 'BOUNTY_NOT_FOUND',
      status: 404,
    });
    await client.cancelBounty(ownDraft.id);
    await expect(client.agentGetBountyDetail(agentA.api_key, ownDraft.id)).rejects.toMatchObject({
      code: 'BOUNTY_NOT_FOUND',
      status: 404,
    });
    const skill = await client.getSkillMarkdown();
    expect(skill.content).toContain('发布者可读取自己定向给其他 Agent 的任务');
    expect(skill.content).toContain('发布者可读取并评论自己已发布的定向任务');
    expect(skill.content).not.toContain('指定其他 Agent 的任务不可见');

    // PENDING_ARBITRATION 且 lease_expires_at=null 同样计入 active_lease_count 并保持 busy
    const fixedAgentId = '00000000-0000-4000-8000-0000000000a1';
    const arbSeed = createMemoryBountyClient({
      lan_open_registration: true,
      projects: [
        {
          id: 'p-arb',
          name: 'Arb Project',
          icon: null,
          description: '',
          workspace_path: '',
          default_acceptance_criteria: '',
          agent_draft_policy: 'open',
          arbitrator_chain: [],
          archived: false,
          created_at: '2026-11-17T00:00:00.000Z',
          updated_at: '2026-11-17T00:00:00.000Z',
        },
      ],
      bounties: [
        {
          id: 'b-arb',
          project_id: 'p-arb',
          title: '待仲裁冻结租约',
          description: '',
          acceptance_criteria: '',
          difficulty: 'A',
          assignee_mode: 'any',
          assignee_min_tier: null,
          assignee_agent_id: null,
          assignee_agent_name: null,
          assignee_agent_tier: null,
          assignment_expires_at: null,
          require_arbitration: true,
          status: 'PENDING_ARBITRATION',
          created_by: 'human',
          created_by_agent_id: null,
          created_by_agent_name: null,
          lease_id: 1,
          lease_agent_id: fixedAgentId,
          lease_agent_name: null,
          lease_agent_tier: null,
          lease_expires_at: null,
          active_duration_ms: 5000,
          active_started_at: null,
          comment_count: 0,
          rejection_count: 0,
          completion_note: null,
          completion_cost: null,
          published_at: '2026-11-17T00:00:00.000Z',
          withdrawn_at: null,
          cancelled_at: null,
          completed_at: null,
          created_at: '2026-11-17T00:00:00.000Z',
          updated_at: '2026-11-17T00:01:00.000Z',
        },
      ],
    });
    const uuidSpy = vi.spyOn(crypto, 'randomUUID').mockReturnValueOnce(fixedAgentId);
    try {
      const arbWorker = await arbSeed.registerAgent({ name: 'arb-worker' });
      expect(arbWorker.agent.active_lease_count).toBe(1);
      expect((await arbSeed.listAgents({ activity: 'busy' })).map(item => item.id)).toEqual([
        fixedAgentId,
      ]);
      expect(await arbSeed.listAgents({ activity: 'idle' })).toHaveLength(0);
    } finally {
      uuidSpy.mockRestore();
    }
  });

  it('MemoryBountyClient 严格遵循 fencing、租约冻结、T_active 分段剔除等待、唤醒推送与取消/隔离规则', async () => {
    vi.useFakeTimers({
      toFake: ['Date', 'setTimeout', 'clearTimeout', 'setInterval', 'clearInterval'],
    });
    try {
      const startMs = Date.parse('2026-11-17T10:00:00.000Z');
      vi.setSystemTime(startMs);

      const client = createMemoryBountyClient({ lan_open_registration: true });
      const project = await client.createProject({ name: 'Issue 07 Project', agent_draft_policy: 'open' });
      const publisher = await client.registerAgent({ name: 'publisher-bot' });
      const executor = await client.registerAgent({ name: 'executor-bot' });
      const outsider = await client.registerAgent({ name: 'outsider-bot' });

      // 发布者发起公开任务
      const bounty = await client.agentCreateBounty(publisher.api_key, {
        project_id: project.id,
        title: '分段计时与挂起任务',
        difficulty: 'B',
        assignee_mode: 'any',
      });
      expect(bounty.active_duration_ms).toBe(0);
      expect(bounty.active_started_at).toBeNull();
      expect(bounty.comment_count).toBe(0);

      // 草稿评论对 Agent 不可见，且不向 Agent 推送通知
      const draftProj = await client.createProject({ name: 'Draft Only', agent_draft_policy: 'draft' });
      const draftTask = await client.agentCreateBounty(publisher.api_key, {
        project_id: draftProj.id,
        title: 'Private Draft',
        difficulty: 'C',
        assignee_mode: 'any',
      });
      await client.createBountyComment(draftTask.id, { body: '管理员内部备注' });
      await expect(client.agentListComments(publisher.api_key, draftTask.id)).rejects.toMatchObject({
        code: 'BOUNTY_NOT_FOUND',
        status: 404,
      });
      expect(
        (await client.agentListEvents(publisher.api_key)).events.some(
          e => e.event_type === 'comment.added',
        ),
      ).toBe(false);

      // 接单进入 IN_PROGRESS（推进 100ms 使单调时钟基线对齐当前 FakeClock）
      vi.advanceTimersByTime(100);
      const claimed = await client.agentClaimBounty(executor.api_key, bounty.id);
      expect(claimed.status).toBe('IN_PROGRESS');
      expect(claimed.active_duration_ms).toBe(0);
      expect(claimed.active_started_at).not.toBeNull();

      // request-info fencing 顺序校验先于正文校验
      await expect(
        client.agentRequestInfo(executor.api_key, bounty.id, {
          lease_id: claimed.lease_id + 1,
          question: '   ',
        }),
      ).rejects.toMatchObject({ code: 'STALE_LEASE', status: 409 });
      await expect(
        client.agentRequestInfo(outsider.api_key, bounty.id, {
          lease_id: claimed.lease_id,
          question: '   ',
        }),
      ).rejects.toMatchObject({ code: 'LEASE_OWNER_MISMATCH', status: 403 });
      await expect(
        client.agentRequestInfo(executor.api_key, bounty.id, {
          lease_id: claimed.lease_id,
          question: '   ',
        }),
      ).rejects.toMatchObject({ code: 'VALIDATION_ERROR', status: 400 });

      // 执行 5 分钟后提问挂起 -> T_active = 300_000ms，租约冻结（lease_expires_at=null）
      vi.advanceTimersByTime(5 * 60 * 1000);
      const waiting = await client.agentRequestInfo(executor.api_key, bounty.id, {
        lease_id: claimed.lease_id,
        question: '请确认边界公式 $f(x) = x^2$',
      });
      expect(waiting.status).toBe('WAITING_INFO');
      expect(waiting.lease_id).toBe(claimed.lease_id);
      expect(waiting.lease_agent_id).toBe(executor.agent.id);
      expect(waiting.lease_expires_at).toBeNull();
      expect(waiting.active_started_at).toBeNull();
      expect(waiting.active_duration_ms).toBe(5 * 60 * 1000);
      expect(waiting.comment_count).toBe(1);

      // 发布者收到 task.info_requested
      const pubEventsAfterReq = await client.agentListEvents(publisher.api_key);
      expect(pubEventsAfterReq.events.some(e => e.event_type === 'task.info_requested')).toBe(true);

      // 等待 2 小时并多次调用 sweepLeases：WAITING_INFO 不超时回收，T_active 保持冻结
      vi.advanceTimersByTime(2 * 60 * 60 * 1000);
      await client.sweepLeases();
      const stillWaiting = (await client.getBountyDetail(bounty.id)).bounty;
      expect(stillWaiting.status).toBe('WAITING_INFO');
      expect(stillWaiting.active_duration_ms).toBe(5 * 60 * 1000);
      expect((await client.agentGetMe(executor.api_key)).timeout_count).toBe(0);

      // 执行者自己在 WAITING_INFO 期间补充评论：只追加 comment，不触发恢复或自唤醒
      const execSelfComment = await client.agentCreateComment(executor.api_key, bounty.id, {
        body: '补充日志片段',
      });
      expect(execSelfComment.bounty.status).toBe('WAITING_INFO');
      expect(execSelfComment.comment.kind).toBe('comment');

      // 发布者（非当前执行者）回复后：自动恢复为 IN_PROGRESS，刷新 15 分钟租约，推送 comment.replied 与 task.resumed
      const execEventsBeforeReply = (await client.agentListEvents(executor.api_key)).events.length;
      vi.advanceTimersByTime(100);
      const resumedByPub = await client.agentCreateComment(publisher.api_key, bounty.id, {
        body: '已确认，按闭区间处理。',
      });
      expect(resumedByPub.comment.kind).toBe('reply');
      expect(resumedByPub.bounty.status).toBe('IN_PROGRESS');
      expect(resumedByPub.bounty.lease_expires_at).not.toBeNull();
      expect(resumedByPub.bounty.active_started_at).not.toBeNull();
      expect(resumedByPub.bounty.active_duration_ms).toBe(5 * 60 * 1000);
      expect(resumedByPub.bounty.comment_count).toBe(3);

      const execEventsAfterReply = (await client.agentListEvents(executor.api_key)).events.slice(
        execEventsBeforeReply,
      );
      expect(execEventsAfterReply.map(e => e.event_type)).toEqual([
        'comment.replied',
        'task.resumed',
      ]);

      // 再执行 3 分钟后完成交付：总有效耗时 = 5 + 3 = 8 分钟（剔除 2 小时等待）
      vi.advanceTimersByTime(3 * 60 * 1000);
      const completed = await client.agentCompleteBounty(executor.api_key, bounty.id, {
        lease_id: claimed.lease_id,
        completion_note: '已完成',
      });
      expect(completed.status).toBe('COMPLETED');
      expect(completed.active_duration_ms).toBe(8 * 60 * 1000);
      expect(completed.active_started_at).toBeNull();

      // 验证新一轮接单会重置 T_active，及 WAITING_INFO 取消与 CANCELLED 禁止评论
      const task2 = await client.createBounty({
        project_id: project.id,
        title: '第二任务：弃单清零与挂起取消',
        difficulty: 'C',
        assignee_mode: 'any',
        status: 'OPEN',
      });
      vi.advanceTimersByTime(100);
      const c2 = await client.agentClaimBounty(executor.api_key, task2.id);
      vi.advanceTimersByTime(60_000);
      await client.agentAbandonBounty(executor.api_key, task2.id, { lease_id: c2.lease_id });
      vi.advanceTimersByTime(100);
      const c3 = await client.agentClaimBounty(executor.api_key, task2.id);
      expect(c3.active_duration_ms).toBe(0);

      vi.advanceTimersByTime(45_000);
      const waiting2 = await client.agentRequestInfo(executor.api_key, task2.id, {
        lease_id: c3.lease_id,
        question: '确认需求是否取消？',
      });
      expect(waiting2.active_duration_ms).toBe(45_000);

      // 人类管理员取消 WAITING_INFO 任务 -> CANCELLED，推送 task.cancelled，保留 T_active 并禁止评论
      const cancelled = await client.cancelBounty(task2.id);
      expect(cancelled.status).toBe('CANCELLED');
      expect(cancelled.active_duration_ms).toBe(45_000);
      expect(cancelled.active_started_at).toBeNull();
      expect(cancelled.lease_expires_at).toBeNull();
      expect(
        (await client.agentListEvents(executor.api_key)).events.some(
          e => e.bounty_id === task2.id && e.event_type === 'task.cancelled',
        ),
      ).toBe(true);

      await expect(
        client.createBountyComment(task2.id, { body: '已取消后评论' }),
      ).rejects.toMatchObject({ code: 'INVALID_TRANSITION', status: 409 });
      await expect(
        client.agentCreateComment(executor.api_key, task2.id, { body: 'Agent 也不允许评论' }),
      ).rejects.toMatchObject({ code: 'INVALID_TRANSITION', status: 409 });
    } finally {
      vi.useRealTimers();
    }
  });

  it('helpers 正确计算与格式化 T_active、评论类型/角色与新流转事件（含双语且无中点）', () => {
    setLocalePreference('zh-CN');
    const nowMs = Date.parse('2026-11-17T10:05:30.000Z');
    expect(
      computeActiveDurationMs(
        {
          status: 'IN_PROGRESS',
          active_duration_ms: 60_000,
          active_started_at: '2026-11-17T10:05:00.000Z',
        },
        nowMs,
      ),
    ).toBe(90_000);
    expect(
      computeActiveDurationMs(
        {
          status: 'WAITING_INFO',
          active_duration_ms: 60_000,
          active_started_at: '2026-11-17T10:05:00.000Z',
        },
        nowMs,
      ),
    ).toBe(60_000);
    expect(formatActiveDuration(90_000)).toBe('01:30');
    expect(formatActiveDuration(3_665_000)).toBe('1:01:05');

    expect(formatCommentKind('question')).toBe('提问阻塞');
    expect(formatCommentKind('reply')).toBe('恢复回复');
    expect(formatCommentAuthorRole('human')).toBe('人类管理员');
    expect(formatCommentAuthorRole('agent')).toBe('Agent');
    expect(formatEventLabel('info_requested')).toBe('提问挂起');
    expect(formatEventLabel('resumed')).toBe('回复并恢复');
    expect(formatEventLabel('commented')).toBe('发表评论');

    setLocalePreference('en');
    expect(formatCommentKind('question')).toBe('Question');
    expect(formatCommentKind('reply')).toBe('Reply (resumed)');
    expect(formatCommentAuthorRole('human')).toBe('Admin');
    expect(formatEventLabel('info_requested')).toBe('Requested info');
    expect(formatEventLabel('resumed')).toBe('Replied & resumed');
    expect(formatEventLabel('commented')).toBe('Commented');

    for (const label of [
      formatCommentKind('question'),
      formatCommentKind('reply'),
      formatCommentAuthorRole('human'),
      formatEventLabel('info_requested'),
      formatEventLabel('resumed'),
    ]) {
      expect(label).not.toContain('·');
    }
    setLocalePreference('zh-CN');
  });
});
