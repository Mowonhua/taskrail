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
  formatAgentChannelStatus,
  formatAgentStatus,
  formatAgentSubscriptionSummary,
  formatAgentTier,
  formatInviteTokenStatus,
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
      'claimed:agent-7:OPEN->IN_PROGRESS',
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
      require_arbitration: false,
      status: 'WAITING_INFO',
      created_by: 'human',
      lease_id: 1,
      lease_agent_id: null,
      lease_expires_at: null,
      rejection_count: 0,
      completion_note: null,
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


