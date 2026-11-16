import { beforeEach, describe, expect, it, vi } from 'vitest';

const native = vi.hoisted(() => ({
  invoke: vi.fn(),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));

import {
  createMemoryBountyClient,
  fetchEmbeddedServerInfo,
  HttpBountyClient,
  normalizeServerUrl,
} from './bounty-client';
import { ServiceApiError, type BountyProject, type BountyTask } from './types';

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
