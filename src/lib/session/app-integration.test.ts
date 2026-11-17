/**
 * 文件职责：验证悬赏板工作台外壳（App.svelte）的导航切换、主题与窗口材质、语言切换、项目图标选择与配置持久化。
 */
import { mount, tick, unmount } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App from '../../App.svelte';
import { createMemoryBountyClient } from '../api/bounty-client';
import { formatTimestamp } from '../bounty/helpers';
import { clipboard } from '../clipboard';
import { emptyAppConfig } from '../config-defaults';
import type { AppConfig, FilePort } from '../contracts';
import { setLocalePreference } from '../i18n';
import { bootThemeKey, builtInThemes, paletteKeys } from '../themes';

async function flush(): Promise<void> {
  for (let i = 0; i < 6; i += 1) {
    await Promise.resolve();
    await tick();
  }
}

function createMemoryPort(initial?: Partial<AppConfig>, isBrowser = false): {
  port: FilePort;
  saved: AppConfig[];
  exported: Array<{ text: string; filename: string }>;
} {
  const saved: AppConfig[] = [];
  const exported: Array<{ text: string; filename: string }> = [];
  let state: AppConfig = {
    ...emptyAppConfig(),
    ...initial,
    preferences: {
      ...emptyAppConfig().preferences,
      ...(initial?.preferences ?? {}),
    },
  };
  const port: FilePort = {
    isBrowser,
    async loadConfig() {
      return structuredClone(state);
    },
    async saveConfig(config) {
      state = structuredClone(config);
      saved.push(structuredClone(config));
    },
    async exportText(text, filename) {
      exported.push({ text, filename });
      return true;
    },
    async openExternalLink() {},
  };
  return { port, saved, exported };
}

async function selectComboboxOption(container: HTMLElement, triggerId: string, optionValue: string): Promise<void> {
  const trigger = container.querySelector<HTMLButtonElement>(`#${triggerId}`);
  expect(trigger).not.toBeNull();
  trigger!.click();
  await flush();
  const listbox = container.querySelector<HTMLElement>(`#${triggerId}-listbox`);
  const option = listbox?.querySelector<HTMLButtonElement>(`[data-value="${optionValue}"]`);
  expect(option).not.toBeNull();
  option!.click();
  await flush();
}

describe('悬赏板工作台外壳集成测试', () => {
  let target: HTMLDivElement;
  let instance: ReturnType<typeof mount> | null = null;
  const storageMap = new Map<string, string>();

  beforeEach(() => {
    storageMap.clear();
    if (typeof globalThis.localStorage?.clear !== 'function') {
      const mockStorage: Storage = {
        get length() {
          return storageMap.size;
        },
        clear: () => storageMap.clear(),
        getItem: (key: string) => storageMap.get(key) ?? null,
        key: (index: number) => [...storageMap.keys()][index] ?? null,
        removeItem: (key: string) => {
          storageMap.delete(key);
        },
        setItem: (key: string, value: string) => {
          storageMap.set(key, String(value));
        },
      };
      Object.defineProperty(globalThis, 'localStorage', { value: mockStorage, configurable: true });
      Object.defineProperty(window, 'localStorage', { value: mockStorage, configurable: true });
    } else {
      globalThis.localStorage.clear();
    }
    if (typeof Element.prototype.scrollIntoView !== 'function') {
      Element.prototype.scrollIntoView = vi.fn();
    }
    setLocalePreference('zh-CN');
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
    if (instance) {
      unmount(instance);
      instance = null;
    }
    delete document.documentElement.dataset.windowTransparent;
    delete document.documentElement.dataset.backdropVeil;
    delete document.documentElement.dataset.theme;
    delete document.documentElement.dataset.corners;
    delete document.documentElement.dataset.monochrome;
    document.documentElement.removeAttribute('style');
    target?.remove();
    setLocalePreference('zh-CN');
  });

  it('左侧导航仅包含 4 个顶层入口（悬赏板、待我处理、Agent 名录、设置），支持视图切换与侧栏收起展开', async () => {
    const { port, saved } = createMemoryPort({
      projects: [{ id: 'p1', name: '核心引擎项目', path: 'D:/engine.md', icon: 'emoji:🎯' }],
      activeProjectId: 'p1',
    });
    const onready = vi.fn();
    instance = mount(App, { target, props: { files: port, onready } });
    await flush();

    await vi.waitFor(() => expect(onready).toHaveBeenCalledTimes(1));

    const navItems = [...target.querySelectorAll<HTMLButtonElement>('.workspace-nav .workspace-nav-item')];
    expect(navItems.map(button => button.textContent?.trim())).toEqual([
      '悬赏板',
      '待我处理',
      'Agent 名录',
      '设置',
    ]);
    // 侧栏中部不展示项目列表
    const sidebar = target.querySelector('.sidebar');
    expect(sidebar?.textContent).not.toContain('核心引擎项目');

    // 默认在悬赏板视图，包含内置 Markdown + KaTeX 验收条件示例渲染
    expect(target.querySelector('[data-view="bounties"]')).not.toBeNull();
    expect(target.querySelector('.markdown-preview-sample .katex')).not.toBeNull();

    // 切换到 待我处理
    navItems[1].click();
    await flush();
    expect(target.querySelector('[data-view="inbox"]')).not.toBeNull();
    expect(saved.at(-1)?.preferences.workspaceSection).toBe('inbox');

    // 切换到 Agent 名录
    navItems[2].click();
    await flush();
    expect(target.querySelector('[data-view="agents"]')).not.toBeNull();
    expect(saved.at(-1)?.preferences.workspaceSection).toBe('agents');

    // 切换到 设置，直接承载 SettingsPanel 及扩展分组预留
    navItems[3].click();
    await flush();
    expect(target.querySelector('[data-view="settings"]')).not.toBeNull();
    expect(target.querySelector('.settings-panel')).not.toBeNull();
    expect(target.querySelector('[data-extension-group="workspace"]')).not.toBeNull();
    expect(saved.at(-1)?.preferences.workspaceSection).toBe('settings');

    // 收起与展开导航栏
    const collapseBtn = target.querySelector<HTMLButtonElement>('.sidebar-collapse-btn');
    expect(collapseBtn).not.toBeNull();
    collapseBtn!.click();
    await flush();
    expect(target.querySelector('.app-shell')?.classList.contains('sidebar-collapsed')).toBe(true);
    expect(saved.at(-1)?.preferences.sidebarCollapsed).toBe(true);

    const expandBtn = target.querySelector<HTMLButtonElement>('.sidebar-expand-btn');
    expect(expandBtn).not.toBeNull();
    expandBtn!.click();
    await flush();
    expect(target.querySelector('.app-shell')?.classList.contains('sidebar-collapsed')).toBe(false);
    expect(saved.at(-1)?.preferences.sidebarCollapsed).toBe(false);
  });

  it('保留 5 套内置主题、自定义主题导入/导出/移除、明暗模式、bootThemeKey 与桌面窗口材质控制', async () => {
    const { port, saved, exported } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'settings' },
    });
    const setWindowMaterial = vi.fn().mockResolvedValue(true);
    instance = mount(App, {
      target,
      props: { files: port, setWindowMaterial },
    });
    await flush();

    // 切换到液态玻璃主题
    await selectComboboxOption(target, 'theme-id-select', 'liquid-glass');
    expect(saved.at(-1)?.preferences.themeId).toBe('liquid-glass');
    expect(setWindowMaterial).toHaveBeenLastCalledWith('acrylic', 'system', false);
    expect(document.documentElement.dataset.windowTransparent).toBe('true');

    // 开启窗口失焦时保持透明
    const blurSwitch = target.querySelector<HTMLInputElement>('#keep-transparent-switch');
    expect(blurSwitch?.disabled).toBe(false);
    blurSwitch!.click();
    await flush();
    expect(saved.at(-1)?.preferences.keepTransparentOnBlur).toBe(true);
    expect(setWindowMaterial).toHaveBeenLastCalledWith('acrylic', 'system', true);

    // 切换深色模式并验证 bootThemeKey 写入
    await selectComboboxOption(target, 'theme-mode-select', 'dark');
    expect(saved.at(-1)?.preferences.theme).toBe('dark');
    expect(document.documentElement.style.colorScheme).toBe('dark');
    const cachedBoot = JSON.parse(localStorage.getItem(bootThemeKey) ?? '{}');
    expect(cachedBoot.mode).toBe('dark');
    expect(cachedBoot.definition?.id).toBe('liquid-glass');

    // 导出主题模板
    const buttons = [...target.querySelectorAll<HTMLButtonElement>('.settings-inline-actions button')];
    const exportBtn = buttons.find(btn => btn.textContent?.includes('下载主题模板'));
    expect(exportBtn).not.toBeNull();
    exportBtn!.click();
    await flush();
    expect(exported).toHaveLength(1);
    expect(exported[0].filename).toBe('liquid-glass-copy.json');

    // 导入自定义 JSON 主题并移除
    const palette = (color: string) => Object.fromEntries(paletteKeys.map(key => [key, color]));
    const customTheme = {
      version: 1,
      id: 'bounty-custom',
      name: '悬赏青',
      light: palette('#F5F7FA'),
      dark: palette('#12161F'),
    };
    const fileInput = target.querySelector<HTMLInputElement>('input[type="file"]');
    expect(fileInput).not.toBeNull();
    const content = JSON.stringify(customTheme);
    const themeFile = new File([content], 'bounty-custom.json', {
      type: 'application/json',
    });
    Object.defineProperty(themeFile, 'text', { value: async () => content });
    Object.defineProperty(fileInput!, 'files', { value: [themeFile], configurable: true });
    fileInput!.dispatchEvent(new Event('change', { bubbles: true }));
    await vi.waitFor(() => expect(saved.at(-1)?.preferences.themeId).toBe('bounty-custom'));

    expect(saved.at(-1)?.customThemes?.map(t => t.id)).toEqual(['bounty-custom']);
    expect(document.documentElement.style.getPropertyValue('--canvas')).toBe('#12161F');

    const removeBtn = [...target.querySelectorAll<HTMLButtonElement>('.settings-inline-actions button')].find(
      btn => btn.textContent?.includes('移除主题'),
    );
    expect(removeBtn).not.toBeNull();
    removeBtn!.click();
    await flush();
    expect(saved.at(-1)?.customThemes).toEqual([]);
    expect(saved.at(-1)?.preferences.themeId).toBe(builtInThemes[0].id);
  });

  it('支持中英文语言切换与项目图标选择器交互', async () => {
    const { port, saved } = createMemoryPort({
      projects: [{ id: 'p1', name: 'Taskrail Core', path: 'D:/core.md' }],
      activeProjectId: 'p1',
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'bounties' },
    });
    instance = mount(App, { target, props: { files: port } });
    await flush();

    // 打开项目图标选择器并选择推荐表情
    const iconBadgeBtn = target.querySelector<HTMLButtonElement>('.project-badge-button');
    expect(iconBadgeBtn).not.toBeNull();
    iconBadgeBtn!.click();
    await flush();

    const dialog = target.querySelector('[role="dialog"]');
    expect(dialog).not.toBeNull();
    await vi.waitFor(() => {
      expect(dialog!.querySelector<HTMLButtonElement>('.icon-choice')).not.toBeNull();
    });
    const emojiChoice = dialog!.querySelector<HTMLButtonElement>('.icon-choice');
    emojiChoice!.click();
    await flush();

    expect(target.querySelector('[role="dialog"]')).toBeNull();
    expect(saved.at(-1)?.projects[0].icon).toMatch(/^(emoji|lucide):/);

    // 切换到设置页并切换语言为 English
    const settingsNav = target.querySelector<HTMLButtonElement>('[data-nav-section="settings"]');
    settingsNav!.click();
    await flush();

    await selectComboboxOption(target, 'language-select', 'en');
    expect(saved.at(-1)?.preferences.language).toBe('en');
    const labels = [...target.querySelectorAll('.workspace-nav .workspace-nav-item')].map(el =>
      el.textContent?.trim(),
    );
    expect(labels).toEqual(['Bounty Board', 'Inbox', 'Agent Directory', 'Settings']);
  });

  it('透明主题启动时不先发 opaque 请求，等待原生材质与内容就绪后才触发 onready 并执行遮罩淡出', async () => {
    let resolveMaterial!: (supported: boolean) => void;
    let materialCalls = 0;
    const setWindowMaterial = vi.fn().mockImplementation(() => {
      materialCalls += 1;
      if (materialCalls === 1) {
        return new Promise<boolean>(resolve => {
          resolveMaterial = resolve;
        });
      }
      return Promise.resolve(true);
    });
    const { port } = createMemoryPort({
      preferences: {
        ...emptyAppConfig().preferences,
        themeId: 'frosted-glass',
        theme: 'dark',
      },
    });
    let readySnapshot: {
      transparent?: string;
      booted: boolean;
      veil?: string;
    } | null = null;
    const onready = vi.fn(() => {
      readySnapshot = {
        transparent: document.documentElement.dataset.windowTransparent,
        booted: Boolean(target.querySelector('.app-shell.booted')),
        veil: document.documentElement.dataset.backdropVeil,
      };
    });

    instance = mount(App, {
      target,
      props: { files: port, setWindowMaterial, onready },
    });
    await flush();

    expect(setWindowMaterial).toHaveBeenCalledTimes(1);
    expect(setWindowMaterial).toHaveBeenCalledWith('acrylic', 'dark', false);
    expect(setWindowMaterial.mock.calls.some(([material]) => material === 'opaque')).toBe(false);
    expect(onready).not.toHaveBeenCalled();

    resolveMaterial(true);
    await vi.waitFor(() => expect(onready).toHaveBeenCalledTimes(1));

    expect(readySnapshot).toEqual({
      transparent: 'true',
      booted: true,
      veil: 'active',
    });
    expect(document.documentElement.dataset.backdropVeil).toBe('fade-a');
    expect(setWindowMaterial.mock.calls.some(([material]) => material === 'opaque')).toBe(false);
  });

  it('配置加载失败时不覆盖原配置并显示错误横幅，重试加载成功后恢复正常保存', async () => {
    const recoveredConfig: AppConfig = {
      ...emptyAppConfig(),
      projects: [{ id: 'p1', name: '恢复项目', path: 'D:/recovered.md' }],
      activeProjectId: 'p1',
    };
    const loadConfig = vi
      .fn<FilePort['loadConfig']>()
      .mockRejectedValueOnce(new Error('STATE_CONFIG_INVALID: 配置损坏'))
      .mockResolvedValueOnce(structuredClone(recoveredConfig));
    const saveConfig = vi.fn<FilePort['saveConfig']>().mockResolvedValue(undefined);
    const port: FilePort = {
      isBrowser: false,
      loadConfig,
      saveConfig,
    };

    instance = mount(App, { target, props: { files: port } });
    await flush();

    const banner = target.querySelector<HTMLElement>('[data-status="config-load-failed"]');
    expect(banner).not.toBeNull();
    expect(banner?.textContent).toContain('STATE_CONFIG_INVALID');

    // 尝试切换导航与收起侧栏，不得覆盖写回配置
    target.querySelector<HTMLButtonElement>('[data-nav-section="inbox"]')?.click();
    target.querySelector<HTMLButtonElement>('.sidebar-collapse-btn')?.click();
    await flush();
    expect(saveConfig).not.toHaveBeenCalled();

    // 重试加载配置并恢复
    const retryBtn = banner!.querySelector<HTMLButtonElement>('button');
    expect(retryBtn).not.toBeNull();
    retryBtn!.click();
    await flush();

    expect(target.querySelector('[data-status="config-load-failed"]')).toBeNull();
    expect(target.querySelector('.project-badge-button')?.textContent).toContain('恢复项目');

    target.querySelector<HTMLButtonElement>('[data-nav-section="settings"]')?.click();
    await flush();
    expect(saveConfig).toHaveBeenCalledTimes(1);
    expect(saveConfig.mock.calls[0][0].projects[0].name).toBe('恢复项目');
  });

  it('多项目切换后修改项目图标仅更新当前选中的目标项目', async () => {
    const { port, saved } = createMemoryPort({
      projects: [
        { id: 'p1', name: '项目一', path: 'D:/p1.md', icon: 'emoji:🎯' },
        { id: 'p2', name: '项目二', path: 'D:/p2.md' },
      ],
      activeProjectId: 'p1',
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'bounties' },
    });
    instance = mount(App, { target, props: { files: port } });
    await flush();

    const p2Btn = target.querySelector<HTMLButtonElement>('[data-project-id="p2"]');
    expect(p2Btn).not.toBeNull();
    p2Btn!.click();
    await flush();
    expect(saved.at(-1)?.activeProjectId).toBe('p2');

    target.querySelector<HTMLButtonElement>('.project-badge-button')!.click();
    await flush();
    const dialog = target.querySelector('[role="dialog"]');
    await vi.waitFor(() => {
      expect(dialog?.querySelector<HTMLButtonElement>('.icon-choice')).not.toBeNull();
    });
    dialog!.querySelector<HTMLButtonElement>('.icon-choice')!.click();
    await flush();

    const latest = saved.at(-1)!;
    expect(latest.projects[0].icon).toBe('emoji:🎯');
    expect(latest.projects[1].icon).toMatch(/^(emoji|lucide):/);
  });

  it('正文段首缩进默认两字符，调整后即时写入样式变量并持久化', async () => {
    const { port, saved } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'settings' },
    });
    instance = mount(App, { target, props: { files: port } });
    await flush();

    const indentInput = target.querySelector<HTMLInputElement>('#settings-paragraph-indent');
    expect(indentInput).not.toBeNull();
    expect(indentInput!.value).toBe('2');
    expect(document.documentElement.style.getPropertyValue('--paragraph-indent')).toBe('2em');

    indentInput!.value = '4';
    indentInput!.dispatchEvent(new Event('input', { bubbles: true }));
    await flush();
    expect(document.documentElement.style.getPropertyValue('--paragraph-indent')).toBe('4em');
    expect(saved.at(-1)?.preferences.paragraphIndent).toBe(4);

    indentInput!.value = '0';
    indentInput!.dispatchEvent(new Event('input', { bubbles: true }));
    await flush();
    expect(document.documentElement.style.getPropertyValue('--paragraph-indent')).toBe('0em');
    expect(saved.at(-1)?.preferences.paragraphIndent).toBe(0);
  });

  it('支持发布弹窗内联新建项目、保留已填草稿内容、存为草稿与立即发布，以及项目默认验收规范自动预填', async () => {
    const { port } = createMemoryPort();
    const bountyApi = createMemoryBountyClient();
    const seededProject = await bountyApi.createProject({
      name: '预设引擎项目',
      icon: 'emoji:🚀',
      default_acceptance_criteria: '- [ ] 默认规范：$O(1)$ 校验通过',
    });

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    // 打开「+ 发布悬赏」弹窗
    const openComposerBtn = target.querySelector<HTMLButtonElement>('[data-action="open-composer"]');
    expect(openComposerBtn).not.toBeNull();
    openComposerBtn!.click();
    await flush();

    const composer = target.querySelector<HTMLElement>('[data-dialog="bounty-composer"]');
    expect(composer).not.toBeNull();

    // 初始自动选中第一个活跃项目并预填默认验收规范
    const criteriaInput = composer!.querySelector<HTMLTextAreaElement>('#bounty-criteria-input')!;
    expect(criteriaInput.value).toBe('- [ ] 默认规范：$O(1)$ 校验通过');

    // 先填写标题和详情正文
    const titleInput = composer!.querySelector<HTMLInputElement>('#bounty-title-input')!;
    titleInput.value = '实现同构服务并发租约校验';
    titleInput.dispatchEvent(new Event('input', { bubbles: true }));

    const descInput = composer!.querySelector<HTMLTextAreaElement>('#bounty-description-input')!;
    descInput.value = '确保并发抢单下 $\\Delta R$ 与租约令牌严格单调递增。';
    descInput.dispatchEvent(new Event('input', { bubbles: true }));

    // 展开内联「+ 新建项目」并创建新项目，验证先前输入的标题与正文不丢失
    composer!.querySelector<HTMLButtonElement>('[data-action="toggle-inline-project"]')!.click();
    await flush();

    const inlineCreator = composer!.querySelector<HTMLElement>('[data-slot="inline-project-creator"]');
    expect(inlineCreator).not.toBeNull();
    const inlineNameInput = inlineCreator!.querySelector<HTMLInputElement>('#inline-project-name')!;
    inlineNameInput.value = '随手新建项目';
    inlineNameInput.dispatchEvent(new Event('input', { bubbles: true }));
    const inlineWorkspaceInput = inlineCreator!.querySelector<HTMLInputElement>('#inline-project-workspace')!;
    inlineWorkspaceInput.value = 'D:/Workspace/new-proj';
    inlineWorkspaceInput.dispatchEvent(new Event('input', { bubbles: true }));

    inlineCreator!.querySelector<HTMLButtonElement>('[data-action="create-inline-project"]')!.click();
    await flush();

    // 内联卡片收起，新项目被立即选中，且已填标题与描述完好保留
    expect(composer!.querySelector('[data-slot="inline-project-creator"]')).toBeNull();
    const projectSelect = composer!.querySelector<HTMLSelectElement>('#bounty-project-select')!;
    const createdProjects = await bountyApi.listProjects();
    const newProject = createdProjects.find(p => p.name === '随手新建项目')!;
    expect(newProject).toBeDefined();
    expect(projectSelect.value).toBe(newProject.id);
    expect(titleInput.value).toBe('实现同构服务并发租约校验');
    expect(descInput.value).toBe('确保并发抢单下 $\\Delta R$ 与租约令牌严格单调递增。');

    // 切回预设项目时，因验收条件尚未被人工修改，自动恢复为预设项目的默认验收规范
    projectSelect.value = seededProject.id;
    projectSelect.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    expect(criteriaInput.value).toBe('- [ ] 默认规范：$O(1)$ 校验通过');

    // 预览 Markdown + KaTeX
    composer!.querySelector<HTMLButtonElement>('[data-tab="description-preview"]')!.click();
    composer!.querySelector<HTMLButtonElement>('[data-tab="criteria-preview"]')!.click();
    await flush();
    expect(composer!.querySelector('[data-slot="composer-description-preview"] .katex')).not.toBeNull();
    expect(composer!.querySelector('[data-slot="composer-criteria-preview"] .katex')).not.toBeNull();

    // 设置难度 A、最低段位 B、开启仲裁验收，并存为草稿
    composer!.querySelector<HTMLButtonElement>('[data-difficulty="A"]')!.click();
    composer!.querySelector<HTMLButtonElement>('[data-assignee-mode="min_tier"]')!.click();
    await flush();
    const minTierSelect = composer!.querySelector<HTMLSelectElement>('#bounty-min-tier-select')!;
    minTierSelect.value = 'B';
    minTierSelect.dispatchEvent(new Event('change', { bubbles: true }));
    composer!.querySelector<HTMLInputElement>('#bounty-require-arbitration')!.click();
    await flush();

    composer!.querySelector<HTMLButtonElement>('[data-action="save-draft"]')!.click();
    await flush();

    expect(target.querySelector('[data-dialog="bounty-composer"]')).toBeNull();
    const cards = [...target.querySelectorAll<HTMLElement>('.bounty-card')];
    expect(cards).toHaveLength(1);
    expect(cards[0].dataset.bountyStatus).toBe('DRAFT');
    expect(cards[0].dataset.bountyDifficulty).toBe('A');
    expect(cards[0].textContent).toContain('[草稿]');
    expect(cards[0].textContent).toContain('最低 B 级');
    expect(cards[0].textContent).toContain('仲裁开启');
  });

  it('支持任务详情抽屉、草稿编辑发布、未接取 OPEN 撤回草稿、并发抢单 409 冲突拦截与待我处理聚合', async () => {
    const { port } = createMemoryPort();
    const bountyApi = createMemoryBountyClient();
    const proj = await bountyApi.createProject({
      name: 'Core Kernel',
      icon: 'lucide:cpu',
    });
    const draft = await bountyApi.createBounty({
      project_id: proj.id,
      title: '优化 SQLite WAL 事务吞吐',
      description: '验证事务吞吐满足 $T \\le 5\\text{ms}$',
      acceptance_criteria: '通过 `cargo test`',
      difficulty: 'S',
      assignee_mode: 'any',
      require_arbitration: true,
      status: 'DRAFT',
    });

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    // 按状态筛选：[草稿] 可见，切换 待接单 时隐藏草稿
    target.querySelector<HTMLButtonElement>('[data-status-filter="OPEN"]')!.click();
    await flush();
    expect(target.querySelectorAll('.bounty-card')).toHaveLength(0);

    target.querySelector<HTMLButtonElement>('[data-status-filter="DRAFT"]')!.click();
    await flush();
    expect(target.querySelectorAll('.bounty-card')).toHaveLength(1);

    // 切换到「待我处理」视图，草稿自动汇聚在待办中
    target.querySelector<HTMLButtonElement>('[data-nav-section="inbox"]')!.click();
    await flush();
    expect(target.querySelectorAll('[data-slot="inbox-card-grid"] .bounty-card')).toHaveLength(1);

    // 切回悬赏板并点击卡片打开详情抽屉
    target.querySelector<HTMLButtonElement>('[data-nav-section="bounties"]')!.click();
    await flush();
    target.querySelector<HTMLButtonElement>('.bounty-card-button')!.click();
    await flush();

    let drawer = target.querySelector<HTMLElement>('[data-dialog="bounty-detail"]');
    expect(drawer).not.toBeNull();
    expect(drawer!.querySelector('[data-slot="detail-description"] .katex')).not.toBeNull();

    // 点击「编辑草稿」，修改标题并直接点击「发布悬赏」
    drawer!.querySelector<HTMLButtonElement>('[data-action="edit-draft"]')!.click();
    await flush();
    const composer = target.querySelector<HTMLElement>('[data-dialog="bounty-composer"]')!;
    const titleInput = composer.querySelector<HTMLInputElement>('#bounty-title-input')!;
    titleInput.value = '优化 SQLite WAL 事务吞吐（已定稿）';
    titleInput.dispatchEvent(new Event('input', { bubbles: true }));
    composer.querySelector<HTMLButtonElement>('[data-action="publish-now"]')!.click();
    await flush();

    // 切回全部状态筛选，验证已转为 OPEN
    target.querySelector<HTMLButtonElement>('[data-status-filter="ALL"]')!.click();
    await flush();
    drawer = target.querySelector<HTMLElement>('[data-dialog="bounty-detail"]')!;
    expect(drawer.dataset.bountyStatus).toBe('OPEN');
    expect(drawer.textContent).toContain('优化 SQLite WAL 事务吞吐（已定稿）');

    // 未被接取时点击「撤回为草稿」成功退回 DRAFT
    drawer.querySelector<HTMLButtonElement>('[data-action="withdraw-bounty"]')!.click();
    await flush();
    expect(drawer.dataset.bountyStatus).toBe('DRAFT');

    // 再次点击「立即发布」转为 OPEN
    drawer.querySelector<HTMLButtonElement>('[data-action="publish-bounty"]')!.click();
    await flush();
    expect(drawer.dataset.bountyStatus).toBe('OPEN');

    // 模拟外部 Agent 抢单成功后，人类点击「撤回为草稿」触发 409 BOUNTY_ALREADY_CLAIMED
    await bountyApi.simulateClaim(draft.id, 'agent-fast-1');
    drawer.querySelector<HTMLButtonElement>('[data-action="withdraw-bounty"]')!.click();
    await flush();

    const errorBanner = drawer.querySelector<HTMLElement>('[data-status="detail-action-error"]');
    expect(errorBanner).not.toBeNull();
    expect(errorBanner!.textContent).toContain('已被 Agent 接取');
    // 抽屉与列表状态自动刷新为 IN_PROGRESS 并显示认领者
    expect(drawer.dataset.bountyStatus).toBe('IN_PROGRESS');
    expect(drawer.textContent).toContain('agent-fast-1');
  });

  it('支持设置页项目管理（新建、编辑、归档恢复、关联悬赏删除保护）与默认验收规范生效', async () => {
    const { port } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'settings' },
    });
    const bountyApi = createMemoryBountyClient();
    const pWithBounty = await bountyApi.createProject({ name: '含任务项目' });
    await bountyApi.createBounty({
      project_id: pWithBounty.id,
      title: '关联任务',
      difficulty: 'C',
      assignee_mode: 'any',
      status: 'DRAFT',
    });

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    const projectSection = target.querySelector<HTMLElement>('[data-section="project-management"]')!;
    expect(projectSection).not.toBeNull();

    // 尝试删除已有悬赏的项目，提示 PROJECT_HAS_BOUNTIES 错误
    const firstRow = projectSection.querySelector<HTMLElement>(`[data-project-row="${pWithBounty.id}"]`)!;
    firstRow.querySelector<HTMLButtonElement>('[data-action="delete-project"]')!.click();
    await flush();
    const panelError = projectSection.querySelector<HTMLElement>('[data-status="project-panel-error"]');
    expect(panelError).not.toBeNull();
    expect(panelError!.textContent).toContain('已有悬赏记录');

    // 编辑已有项目（名称、说明、工作目录路径、默认验收规范），验证持久化到 bountyApi 与 DOM
    firstRow.querySelector<HTMLButtonElement>('[data-action="edit-project"]')!.click();
    await flush();
    const editNameInput = projectSection.querySelector<HTMLInputElement>('#project-form-name')!;
    editNameInput.value = '核心内核（已更新）';
    editNameInput.dispatchEvent(new Event('input', { bubbles: true }));
    const editDescInput = projectSection.querySelector<HTMLInputElement>('#project-form-description')!;
    editDescInput.value = '负责 SQLite 与 Axum 同构内核';
    editDescInput.dispatchEvent(new Event('input', { bubbles: true }));
    const editWorkspaceInput = projectSection.querySelector<HTMLInputElement>('#project-form-workspace')!;
    editWorkspaceInput.value = 'D:/Workspace/taskrail';
    editWorkspaceInput.dispatchEvent(new Event('input', { bubbles: true }));
    const editCriteriaInput = projectSection.querySelector<HTMLTextAreaElement>('#project-form-criteria')!;
    editCriteriaInput.value = '- [ ] 通过 `cargo test` 与 `npm test`';
    editCriteriaInput.dispatchEvent(new Event('input', { bubbles: true }));
    projectSection.querySelector<HTMLButtonElement>('[data-action="submit-project-form"]')!.click();
    await flush();

    const persistedProject = await bountyApi.getProject(pWithBounty.id);
    expect(persistedProject).toMatchObject({
      name: '核心内核（已更新）',
      description: '负责 SQLite 与 Axum 同构内核',
      workspace_path: 'D:/Workspace/taskrail',
      default_acceptance_criteria: '- [ ] 通过 `cargo test` 与 `npm test`',
    });
    const updatedRow = projectSection.querySelector<HTMLElement>(`[data-project-row="${pWithBounty.id}"]`)!;
    expect(updatedRow.textContent).toContain('核心内核（已更新）');
    expect(updatedRow.textContent).toContain('D:/Workspace/taskrail');

    // 在设置中新建一个空项目、归档、开启「显示已归档」后恢复，再重新归档并删除
    projectSection.querySelector<HTMLButtonElement>('[data-action="new-project"]')!.click();
    await flush();
    const nameInput = projectSection.querySelector<HTMLInputElement>('#project-form-name')!;
    nameInput.value = '临时项目';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    projectSection.querySelector<HTMLButtonElement>('[data-action="submit-project-form"]')!.click();
    await flush();

    const allProjects = await bountyApi.listProjects(true);
    const tempProject = allProjects.find(p => p.name === '临时项目')!;
    expect(tempProject).toBeDefined();

    const tempRow = projectSection.querySelector<HTMLElement>(`[data-project-row="${tempProject.id}"]`)!;
    tempRow.querySelector<HTMLButtonElement>('[data-action="toggle-archive-project"]')!.click();
    await flush();
    // 默认不显示归档项目
    expect(projectSection.querySelector(`[data-project-row="${tempProject.id}"]`)).toBeNull();

    // 开启「显示已归档」并点击「恢复」
    const archivedSwitch = projectSection.querySelector<HTMLInputElement>('#show-archived-projects-switch')!;
    archivedSwitch.click();
    await flush();
    const archivedRow = projectSection.querySelector<HTMLElement>(`[data-project-row="${tempProject.id}"]`)!;
    expect(archivedRow).not.toBeNull();
    expect(archivedRow.querySelector('[data-tag="archived"]')).not.toBeNull();

    archivedRow.querySelector<HTMLButtonElement>('[data-action="toggle-archive-project"]')!.click();
    await flush();
    expect((await bountyApi.getProject(tempProject.id)).archived).toBe(false);

    // 关闭「显示已归档」，已恢复项目仍可见，随后直接删除
    archivedSwitch.click();
    await flush();
    const restoredRow = projectSection.querySelector<HTMLElement>(`[data-project-row="${tempProject.id}"]`)!;
    expect(restoredRow).not.toBeNull();
    expect(restoredRow.querySelector('[data-tag="archived"]')).toBeNull();

    restoredRow.querySelector<HTMLButtonElement>('[data-action="delete-project"]')!.click();
    await flush();
    expect(projectSection.querySelector(`[data-project-row="${tempProject.id}"]`)).toBeNull();
  });

  it('支持切换团队远端模式、测试连接、持久化远端配置并从远端服务重载项目与悬赏', async () => {
    const { port, saved } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'settings' },
    });
    const localApi = createMemoryBountyClient();
    await localApi.createProject({ name: '本地项目' });

    const remoteProject = {
      id: 'remote-proj-1',
      name: '远端同步项目',
      icon: 'lucide:server',
      description: '来自独立进程',
      workspace_path: '/srv/taskrail',
      default_acceptance_criteria: '',
      agent_draft_policy: 'open' as const,
      arbitrator_chain: [],
      archived: false,
      created_at: '2026-11-16T12:00:00.000Z',
      updated_at: '2026-11-16T12:00:00.000Z',
    };
    const remoteBounty = {
      id: 'remote-bounty-1',
      project_id: 'remote-proj-1',
      title: '远端悬赏任务',
      description: '远端任务描述',
      acceptance_criteria: '远端验收标准',
      difficulty: 'A' as const,
      assignee_mode: 'any' as const,
      assignee_min_tier: null,
      assignee_agent_id: null,
      require_arbitration: false,
      status: 'OPEN' as const,
      created_by: 'human',
      lease_id: 0,
      lease_agent_id: null,
      lease_expires_at: null,
      rejection_count: 0,
      completion_note: null,
      published_at: '2026-11-16T12:05:00.000Z',
      withdrawn_at: null,
      cancelled_at: null,
      completed_at: null,
      created_at: '2026-11-16T12:00:00.000Z',
      updated_at: '2026-11-16T12:05:00.000Z',
    };

    const fetchCalls: Array<{ url: string; auth?: string }> = [];
    vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
      const url = typeof input === 'string' ? input : input.toString();
      const headers = (init?.headers ?? {}) as Record<string, string>;
      fetchCalls.push({ url, auth: headers.Authorization });

      if (url === 'https://remote.taskrail.test/api/v1/health') {
        return new Response(JSON.stringify({ ok: true, mode: 'standalone', version: '0.8.8' }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      if (headers.Authorization !== 'Bearer remote-secret-token') {
        return new Response(
          JSON.stringify({ error: { code: 'UNAUTHORIZED', message: '需要有效的管理员 Bearer token' } }),
          { status: 401, headers: { 'Content-Type': 'application/json' } },
        );
      }
      if (url === 'https://remote.taskrail.test/api/v1/admin/status') {
        return new Response(
          JSON.stringify({ ok: true, mode: 'standalone', db_path: '/var/lib/taskrail.db', version: '0.8.8' }),
          { status: 200, headers: { 'Content-Type': 'application/json' } },
        );
      }
      if (url.startsWith('https://remote.taskrail.test/api/v1/projects')) {
        return new Response(JSON.stringify([remoteProject]), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      if (url.startsWith('https://remote.taskrail.test/api/v1/bounties')) {
        return new Response(JSON.stringify([remoteBounty]), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      return new Response(JSON.stringify({ error: { code: 'NOT_FOUND', message: 'not found' } }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' },
      });
    });

    instance = mount(App, { target, props: { files: port, bountyApi: localApi } });
    await flush();

    const connSection = target.querySelector<HTMLElement>('[data-section="service-connection"]')!;
    expect(connSection).not.toBeNull();

    // 切换到团队远端模式，输入 URL（带尾斜杠测试规范化）与管理员凭证
    connSection.querySelector<HTMLButtonElement>('[data-service-mode="remote"]')!.click();
    await flush();

    const urlInput = connSection.querySelector<HTMLInputElement>('#remote-server-url-input')!;
    const tokenInput = connSection.querySelector<HTMLInputElement>('#remote-admin-token-input')!;
    urlInput.value = 'https://remote.taskrail.test///';
    urlInput.dispatchEvent(new Event('input', { bubbles: true }));
    tokenInput.value = 'remote-secret-token';
    tokenInput.dispatchEvent(new Event('input', { bubbles: true }));
    await flush();

    // 点击测试连接
    connSection.querySelector<HTMLButtonElement>('[data-action="test-connection"]')!.click();
    await vi.waitFor(() => {
      const testFeedback = connSection.querySelector<HTMLElement>('[data-status="connection-feedback"]');
      expect(testFeedback?.textContent).toContain('standalone');
      expect(testFeedback?.textContent).toContain('0.8.8');
    });
    expect(fetchCalls.find(c => c.url.endsWith('/health'))?.auth).toBeUndefined();
    expect(fetchCalls.find(c => c.url.endsWith('/admin/status'))?.auth).toBe('Bearer remote-secret-token');

    // 点击保存并重连，持久化配置并从远端服务重新拉取项目与悬赏
    connSection.querySelector<HTMLButtonElement>('[data-action="save-reconnect"]')!.click();
    await vi.waitFor(() => {
      expect(connSection.querySelector('[data-status="connection-feedback"]')?.textContent).toContain(
        '已保存并连接到远端服务',
      );
    });

    const persistedPrefs = saved.at(-1)?.preferences;
    expect(persistedPrefs).toMatchObject({
      serviceMode: 'remote',
      remoteServerUrl: 'https://remote.taskrail.test',
      remoteAdminToken: 'remote-secret-token',
    });
    expect(target.querySelector('[data-section="project-management"]')?.textContent).toContain('远端同步项目');

    // 切回悬赏板，确认展示远端服务返回的悬赏任务
    target.querySelector<HTMLButtonElement>('[data-nav-section="bounties"]')!.click();
    await flush();
    expect(target.querySelector('.bounty-card')?.textContent).toContain('远端悬赏任务');

    // 切回设置页并切换回个人内嵌模式，确认恢复本地项目列表
    target.querySelector<HTMLButtonElement>('[data-nav-section="settings"]')!.click();
    await flush();
    target.querySelector<HTMLButtonElement>('[data-service-mode="embedded"]')!.click();
    await vi.waitFor(() => {
      expect(saved.at(-1)?.preferences.serviceMode).toBe('embedded');
      expect(target.querySelector('[data-section="project-management"]')?.textContent).toContain('本地项目');
    });
  });

  it('支持悬赏板按项目筛选、关键字字面检索、丢弃草稿与取消待接单任务', async () => {
    const { port } = createMemoryPort();
    const bountyApi = createMemoryBountyClient();
    const projA = await bountyApi.createProject({ name: 'Alpha 引擎' });
    const projB = await bountyApi.createProject({ name: 'Beta 工具链' });

    const draftA = await bountyApi.createBounty({
      project_id: projA.id,
      title: '100%_WAL 草稿任务',
      description: '验证百分号与下划线按字面匹配',
      difficulty: 'B',
      assignee_mode: 'any',
      status: 'DRAFT',
    });
    const openB = await bountyApi.createBounty({
      project_id: projB.id,
      title: '构建 CI 发布流水线',
      description: '仅属于 Beta 项目',
      difficulty: 'A',
      assignee_mode: 'any',
      status: 'OPEN',
    });

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    expect(target.querySelectorAll('.bounty-card')).toHaveLength(2);

    // 按项目筛选：仅保留 Beta 项目
    const projectFilter = target.querySelector<HTMLSelectElement>('#bounty-project-filter')!;
    projectFilter.value = projB.id;
    projectFilter.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    let cards = [...target.querySelectorAll<HTMLElement>('.bounty-card')];
    expect(cards).toHaveLength(1);
    expect(cards[0].dataset.bountyId).toBe(openB.id);

    // 恢复全部项目后按关键字字面搜索（包含 %_ 通配符字符）
    projectFilter.value = 'ALL';
    projectFilter.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();

    const searchInput = target.querySelector<HTMLInputElement>('[data-filter="search"]')!;
    searchInput.value = '100%_wal';
    searchInput.dispatchEvent(new Event('input', { bubbles: true }));
    await flush();
    cards = [...target.querySelectorAll<HTMLElement>('.bounty-card')];
    expect(cards).toHaveLength(1);
    expect(cards[0].dataset.bountyId).toBe(draftA.id);

    // 清空搜索词
    searchInput.value = '';
    searchInput.dispatchEvent(new Event('input', { bubbles: true }));
    await flush();
    expect(target.querySelectorAll('.bounty-card')).toHaveLength(2);

    // 打开 DRAFT 详情抽屉并点击「丢弃草稿」转为 CANCELLED
    target
      .querySelector<HTMLButtonElement>(`[data-bounty-id="${draftA.id}"] .bounty-card-button`)!
      .click();
    await flush();
    let drawer = target.querySelector<HTMLElement>('[data-dialog="bounty-detail"]')!;
    drawer.querySelector<HTMLButtonElement>('[data-action="cancel-bounty"]')!.click();
    await flush();
    expect(drawer.dataset.bountyStatus).toBe('CANCELLED');
    expect((await bountyApi.getBountyDetail(draftA.id)).bounty.status).toBe('CANCELLED');
    drawer.querySelector<HTMLButtonElement>('button[aria-label="关闭对话框"]')!.click();
    await flush();

    // 打开 OPEN 详情抽屉并点击「取消任务」转为 CANCELLED
    target
      .querySelector<HTMLButtonElement>(`[data-bounty-id="${openB.id}"] .bounty-card-button`)!
      .click();
    await flush();
    drawer = target.querySelector<HTMLElement>('[data-dialog="bounty-detail"]')!;
    drawer.querySelector<HTMLButtonElement>('[data-action="cancel-bounty"]')!.click();
    await flush();
    expect(drawer.dataset.bountyStatus).toBe('CANCELLED');
    expect((await bountyApi.getBountyDetail(openB.id)).bounty.status).toBe('CANCELLED');
    drawer.querySelector<HTMLButtonElement>('button[aria-label="关闭对话框"]')!.click();
    await flush();

    // 切换到「已取消」状态筛选，验证两条任务均可检索到
    target.querySelector<HTMLButtonElement>('[data-status-filter="CANCELLED"]')!.click();
    await flush();
    expect(target.querySelectorAll('.bounty-card')).toHaveLength(2);
  });

  it('初始服务连接失败时不阻塞窗口就绪（onready），并展示可重试的非阻塞连接错误横幅', async () => {
    const { port } = createMemoryPort();
    const failingApi = createMemoryBountyClient();
    vi.spyOn(failingApi, 'listProjects').mockRejectedValueOnce(new Error('ECONNREFUSED 127.0.0.1'));
    const onready = vi.fn();

    instance = mount(App, {
      target,
      props: { files: port, bountyApi: failingApi, onready },
    });
    await flush();

    await vi.waitFor(() => expect(onready).toHaveBeenCalledTimes(1));
    const banner = target.querySelector<HTMLElement>('[data-status="service-connection-failed"]');
    expect(banner).not.toBeNull();
    expect(banner!.textContent).toContain('ECONNREFUSED');

    // 点击重试连接后恢复正常
    banner!.querySelector<HTMLButtonElement>('[data-action="retry-service-connection"]')!.click();
    await flush();
    expect(target.querySelector('[data-status="service-connection-failed"]')).toBeNull();
  });

  it('归档项目的草稿打开编辑时保留原 project_id 并阻止静默迁移，主动切换活跃项目后可正常保存发布', async () => {
    const { port } = createMemoryPort();
    const bountyApi = createMemoryBountyClient();
    const archivedProj = await bountyApi.createProject({ name: '已归档旧项目', icon: 'emoji:📦' });
    const activeProj = await bountyApi.createProject({ name: '活跃目标项目', icon: 'emoji:⚡' });
    const draft = await bountyApi.createBounty({
      project_id: archivedProj.id,
      title: '归档项目下的历史草稿',
      description: '不可静默迁移到其他项目',
      difficulty: 'B',
      assignee_mode: 'any',
      status: 'DRAFT',
    });
    await bountyApi.updateProject(archivedProj.id, { archived: true });

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    // 打开详情抽屉并点击「编辑草稿」
    target
      .querySelector<HTMLButtonElement>(`[data-bounty-id="${draft.id}"] .bounty-card-button`)!
      .click();
    await flush();
    target.querySelector<HTMLButtonElement>('[data-action="edit-draft"]')!.click();
    await flush();

    const composer = target.querySelector<HTMLElement>('[data-dialog="bounty-composer"]')!;
    const projectSelect = composer.querySelector<HTMLSelectElement>('#bounty-project-select')!;
    expect(projectSelect.value).toBe(archivedProj.id);
    expect(projectSelect.selectedOptions[0]?.textContent).toContain('已归档旧项目');
    expect(projectSelect.selectedOptions[0]?.textContent).toContain('已归档');
    expect(composer.querySelector('[data-status="archived-project-warning"]')).not.toBeNull();
    expect(composer.querySelector('[data-slot="inline-project-creator"]')).toBeNull();

    // 直接保存草稿或发布必须被阻止，且原 project_id 未被静默改写
    composer.querySelector<HTMLButtonElement>('[data-action="save-draft"]')!.click();
    await flush();
    expect(composer.querySelector('[data-status="composer-error"]')?.textContent).toContain('已归档');
    expect((await bountyApi.getBountyDetail(draft.id)).bounty.project_id).toBe(archivedProj.id);

    composer.querySelector<HTMLButtonElement>('[data-action="publish-now"]')!.click();
    await flush();
    expect(composer.querySelector('[data-status="composer-error"]')?.textContent).toContain('已归档');
    expect((await bountyApi.getBountyDetail(draft.id)).bounty.status).toBe('DRAFT');

    // 用户主动切换至活跃项目后可正常保存并发布
    projectSelect.value = activeProj.id;
    projectSelect.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    expect(composer.querySelector('[data-status="archived-project-warning"]')).toBeNull();

    composer.querySelector<HTMLButtonElement>('[data-action="publish-now"]')!.click();
    await flush();
    expect(target.querySelector('[data-dialog="bounty-composer"]')).toBeNull();
    const updated = (await bountyApi.getBountyDetail(draft.id)).bounty;
    expect(updated.project_id).toBe(activeProj.id);
    expect(updated.status).toBe('OPEN');
  });

  it('并发搜索响应乱序返回时保留最新结果不被旧响应覆盖，且关闭详情抽屉后旧详情不再重开', async () => {
    const { port } = createMemoryPort();
    const bountyApi = createMemoryBountyClient();
    const proj = await bountyApi.createProject({ name: '并发竞态测试项目' });
    const olderBounty = await bountyApi.createBounty({
      project_id: proj.id,
      title: 'older 慢速查询命中',
      difficulty: 'C',
      assignee_mode: 'any',
      status: 'OPEN',
    });
    const newerBounty = await bountyApi.createBounty({
      project_id: proj.id,
      title: 'newer 快速查询命中',
      difficulty: 'A',
      assignee_mode: 'any',
      status: 'OPEN',
    });
    const draftForDrawer = await bountyApi.createBounty({
      project_id: proj.id,
      title: '草稿发布刷新挂起测试',
      difficulty: 'B',
      assignee_mode: 'any',
      status: 'DRAFT',
    });

    const originalListBounties = bountyApi.listBounties.bind(bountyApi);
    let resolveOlder!: (tasks: typeof olderBounty[]) => void;
    vi.spyOn(bountyApi, 'listBounties').mockImplementation(query => {
      if (query?.q === 'older') {
        return new Promise(resolve => {
          resolveOlder = resolve;
        });
      }
      return originalListBounties(query);
    });

    const originalGetDetail = bountyApi.getBountyDetail.bind(bountyApi);
    let resolveSlowDetail!: (detail: Awaited<ReturnType<typeof originalGetDetail>>) => void;
    let slowDetailTargetId: string | null = null;
    vi.spyOn(bountyApi, 'getBountyDetail').mockImplementation(id => {
      if (slowDetailTargetId === id) {
        slowDetailTargetId = null;
        return new Promise(resolve => {
          resolveSlowDetail = resolve;
        });
      }
      return originalGetDetail(id);
    });

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();
    expect(target.querySelectorAll('.bounty-card')).toHaveLength(3);

    // 先触发慢查询 older，再立即触发快查询 newer
    const searchInput = target.querySelector<HTMLInputElement>('[data-filter="search"]')!;
    searchInput.value = 'older';
    searchInput.dispatchEvent(new Event('input', { bubbles: true }));
    searchInput.value = 'newer';
    searchInput.dispatchEvent(new Event('input', { bubbles: true }));
    await flush();

    let cards = [...target.querySelectorAll<HTMLElement>('.bounty-card')];
    expect(cards).toHaveLength(1);
    expect(cards[0].dataset.bountyId).toBe(newerBounty.id);

    // 迟到的 older 响应返回后，不得覆盖 newer 结果
    resolveOlder([olderBounty]);
    await flush();
    cards = [...target.querySelectorAll<HTMLElement>('.bounty-card')];
    expect(cards).toHaveLength(1);
    expect(cards[0].dataset.bountyId).toBe(newerBounty.id);

    // 恢复全部列表，验证先点慢详情 A、再点快详情 B 时，迟到的 A 不覆盖 B
    searchInput.value = '';
    searchInput.dispatchEvent(new Event('input', { bubbles: true }));
    await flush();
    expect(target.querySelectorAll('.bounty-card')).toHaveLength(3);

    slowDetailTargetId = olderBounty.id;
    target
      .querySelector<HTMLButtonElement>(`[data-bounty-id="${olderBounty.id}"] .bounty-card-button`)!
      .click();
    await vi.waitFor(() => expect(resolveSlowDetail).toBeTypeOf('function'));
    target
      .querySelector<HTMLButtonElement>(`[data-bounty-id="${newerBounty.id}"] .bounty-card-button`)!
      .click();
    await flush();

    let drawer = target.querySelector<HTMLElement>('[data-dialog="bounty-detail"]')!;
    expect(drawer).not.toBeNull();
    expect(drawer.querySelector('.detail-drawer-title')?.textContent).toBe(newerBounty.title);

    resolveSlowDetail(await originalGetDetail(olderBounty.id));
    await flush();
    drawer = target.querySelector<HTMLElement>('[data-dialog="bounty-detail"]')!;
    expect(drawer).not.toBeNull();
    expect(drawer.querySelector('.detail-drawer-title')?.textContent).toBe(newerBounty.title);

    drawer.querySelector<HTMLButtonElement>('button[aria-label="关闭对话框"]')!.click();
    await flush();
    expect(target.querySelector('[data-dialog="bounty-detail"]')).toBeNull();

    // 在已打开的详情抽屉中点击「立即发布」且随后的 getBountyDetail 刷新尚在挂起时，
    // 直接点击关闭抽屉，随后该挂起的 getBountyDetail 返回时抽屉保持关闭状态（null）
    target
      .querySelector<HTMLButtonElement>(`[data-bounty-id="${draftForDrawer.id}"] .bounty-card-button`)!
      .click();
    await flush();
    drawer = target.querySelector<HTMLElement>('[data-dialog="bounty-detail"]')!;
    expect(drawer).not.toBeNull();
    expect(drawer.dataset.bountyStatus).toBe('DRAFT');

    let resolvePublishRefresh!: (detail: Awaited<ReturnType<typeof originalGetDetail>>) => void;
    slowDetailTargetId = draftForDrawer.id;
    vi.spyOn(bountyApi, 'getBountyDetail').mockImplementationOnce(id => {
      expect(id).toBe(draftForDrawer.id);
      return new Promise(resolve => {
        resolvePublishRefresh = resolve;
      });
    });

    drawer.querySelector<HTMLButtonElement>('[data-action="publish-bounty"]')!.click();
    await vi.waitFor(() => expect(resolvePublishRefresh).toBeTypeOf('function'));

    drawer.querySelector<HTMLButtonElement>('button[aria-label="关闭对话框"]')!.click();
    await flush();
    expect(target.querySelector('[data-dialog="bounty-detail"]')).toBeNull();

    resolvePublishRefresh(await originalGetDetail(draftForDrawer.id));
    await flush();
    expect(target.querySelector('[data-dialog="bounty-detail"]')).toBeNull();
  });

  it('配置保存失败时不连接远端服务、回滚配置变更并展示错误反馈，绝不显示成功提示', async () => {
    const saveConfig = vi
      .fn<FilePort['saveConfig']>()
      .mockRejectedValue(new Error('DISK_FULL: 无法写入配置'));
    const baseConfig: AppConfig = {
      ...emptyAppConfig(),
      preferences: {
        ...emptyAppConfig().preferences,
        workspaceSection: 'settings',
        serviceMode: 'embedded',
      },
    };
    const port: FilePort = {
      isBrowser: false,
      loadConfig: vi.fn<FilePort['loadConfig']>().mockResolvedValue(structuredClone(baseConfig)),
      saveConfig,
    };
    const localApi = createMemoryBountyClient();
    await localApi.createProject({ name: '本地保留项目' });
    const fetchSpy = vi.spyOn(globalThis, 'fetch');

    instance = mount(App, { target, props: { files: port, bountyApi: localApi } });
    await flush();

    const connSection = target.querySelector<HTMLElement>('[data-section="service-connection"]')!;
    connSection.querySelector<HTMLButtonElement>('[data-service-mode="remote"]')!.click();
    await flush();

    const urlInput = connSection.querySelector<HTMLInputElement>('#remote-server-url-input')!;
    const tokenInput = connSection.querySelector<HTMLInputElement>('#remote-admin-token-input')!;
    urlInput.value = 'https://remote.taskrail.test';
    urlInput.dispatchEvent(new Event('input', { bubbles: true }));
    tokenInput.value = 'secret-token';
    tokenInput.dispatchEvent(new Event('input', { bubbles: true }));
    await flush();

    connSection.querySelector<HTMLButtonElement>('[data-action="save-reconnect"]')!.click();
    await flush();

    // 不得发起远端网络请求，不得显示成功提示
    expect(fetchSpy).not.toHaveBeenCalled();
    const feedback = connSection.querySelector<HTMLElement>('[data-status="connection-feedback"]');
    expect(feedback).not.toBeNull();
    expect(feedback!.getAttribute('role')).toBe('alert');
    expect(feedback!.textContent).toContain('DISK_FULL');
    expect(feedback!.textContent).not.toContain('已保存并连接到远端服务');
    expect(target.querySelector('[data-section="project-management"]')?.textContent).toContain(
      '本地保留项目',
    );
    expect(
      connSection.querySelector<HTMLButtonElement>('[data-service-mode="embedded"]')?.getAttribute('aria-pressed'),
    ).toBe('true');

    // 验证从 remote 模式切回 embedded 模式但 saveConfig 失败时，同样回滚草稿模式并显示错误
    unmount(instance);
    instance = null;
    const remoteConfig: AppConfig = {
      ...emptyAppConfig(),
      preferences: {
        ...emptyAppConfig().preferences,
        workspaceSection: 'settings',
        serviceMode: 'remote',
        remoteServerUrl: 'https://remote.taskrail.test',
        remoteAdminToken: 'secret-token',
      },
    };
    const remotePort: FilePort = {
      isBrowser: false,
      loadConfig: vi.fn<FilePort['loadConfig']>().mockResolvedValue(structuredClone(remoteConfig)),
      saveConfig: vi.fn<FilePort['saveConfig']>().mockRejectedValue(new Error('READONLY_FS')),
    };
    fetchSpy.mockImplementation(async input => {
      const url = typeof input === 'string' ? input : input.toString();
      if (url.endsWith('/projects?include_archived=true')) {
        return new Response(JSON.stringify([]), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      if (url.includes('/bounties')) {
        return new Response(JSON.stringify([]), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      return new Response(JSON.stringify({ ok: true }), { status: 200 });
    });

    instance = mount(App, { target, props: { files: remotePort } });
    await flush();

    const remoteConnSection = target.querySelector<HTMLElement>('[data-section="service-connection"]')!;
    remoteConnSection.querySelector<HTMLButtonElement>('[data-service-mode="embedded"]')!.click();
    await flush();

    const switchFeedback = remoteConnSection.querySelector<HTMLElement>('[data-status="connection-feedback"]');
    expect(switchFeedback).not.toBeNull();
    expect(switchFeedback!.getAttribute('role')).toBe('alert');
    expect(switchFeedback!.textContent).toContain('READONLY_FS');
    expect(switchFeedback!.textContent).not.toContain('已切换回个人内嵌模式');
    expect(
      remoteConnSection
        .querySelector<HTMLButtonElement>('[data-service-mode="remote"]')
        ?.getAttribute('aria-pressed'),
    ).toBe('true');
  });

  it('保存远端配置挂起期间并发导航保存不会将未确认的 remote 配置写入磁盘', async () => {
    const savedConfigs: AppConfig[] = [];
    let rejectFirstSave!: (error: Error) => void;
    let saveAttempts = 0;
    const saveConfig = vi.fn<FilePort['saveConfig']>().mockImplementation(config => {
      saveAttempts += 1;
      if (saveAttempts === 1) {
        return new Promise((_, reject) => {
          rejectFirstSave = reject;
        });
      }
      savedConfigs.push(structuredClone(config));
      return Promise.resolve();
    });
    const baseConfig: AppConfig = {
      ...emptyAppConfig(),
      preferences: {
        ...emptyAppConfig().preferences,
        workspaceSection: 'settings',
        serviceMode: 'embedded',
      },
    };
    const port: FilePort = {
      isBrowser: false,
      loadConfig: vi.fn<FilePort['loadConfig']>().mockResolvedValue(structuredClone(baseConfig)),
      saveConfig,
    };
    const localApi = createMemoryBountyClient();
    const fetchSpy = vi.spyOn(globalThis, 'fetch');

    instance = mount(App, { target, props: { files: port, bountyApi: localApi } });
    await flush();

    const connSection = target.querySelector<HTMLElement>('[data-section="service-connection"]')!;
    connSection.querySelector<HTMLButtonElement>('[data-service-mode="remote"]')!.click();
    await flush();

    const urlInput = connSection.querySelector<HTMLInputElement>('#remote-server-url-input')!;
    const tokenInput = connSection.querySelector<HTMLInputElement>('#remote-admin-token-input')!;
    urlInput.value = 'https://remote.taskrail.test';
    urlInput.dispatchEvent(new Event('input', { bubbles: true }));
    tokenInput.value = 'secret-token';
    tokenInput.dispatchEvent(new Event('input', { bubbles: true }));
    await flush();

    // 第一次保存挂起时切换导航触发第二次保存
    connSection.querySelector<HTMLButtonElement>('[data-action="save-reconnect"]')!.click();
    await vi.waitFor(() => expect(saveConfig).toHaveBeenCalledTimes(1));

    target.querySelector<HTMLButtonElement>('[data-nav-section="bounties"]')!.click();
    await flush();

    // 第一次保存失败，第二次保存随后成功
    rejectFirstSave(new Error('DISK_FULL_CONCURRENT'));
    await flush();

    expect(saveConfig).toHaveBeenCalledTimes(2);
    expect(savedConfigs).toHaveLength(1);
    expect(savedConfigs[0].preferences.serviceMode).toBe('embedded');
    expect(savedConfigs[0].preferences.workspaceSection).toBe('bounties');
    expect(savedConfigs[0].preferences.remoteServerUrl ?? '').toBe('');
    expect(fetchSpy).not.toHaveBeenCalled();

    const toastBar = target.querySelector<HTMLElement>('.toast-bar');
    expect(toastBar).not.toBeNull();
    expect(toastBar!.textContent).toContain('DISK_FULL_CONCURRENT');

    // 切回设置页，服务连接模式仍回滚在个人内嵌模式
    target.querySelector<HTMLButtonElement>('[data-nav-section="settings"]')!.click();
    await flush();
    expect(
      target
        .querySelector<HTMLButtonElement>('[data-service-mode="embedded"]')
        ?.getAttribute('aria-pressed'),
    ).toBe('true');
    expect(target.querySelector('[data-slot="remote-service-form"]')).toBeNull();
  });

  it('跨刷新请求乱序完成时保持最新连接状态，慢速工作区刷新不覆盖搜索结果与错误横幅', async () => {
    const { port } = createMemoryPort();
    const bountyApi = createMemoryBountyClient();
    const proj = await bountyApi.createProject({ name: '跨刷新状态保护项目' });
    await bountyApi.createBounty({
      project_id: proj.id,
      title: 'ok-search 正常任务',
      difficulty: 'B',
      assignee_mode: 'any',
      status: 'OPEN',
    });

    const originalListProjects = bountyApi.listProjects.bind(bountyApi);
    const originalListBounties = bountyApi.listBounties.bind(bountyApi);

    let resolveFirstProjects!: (projects: Awaited<ReturnType<typeof originalListProjects>>) => void;
    let rejectRetryProjects!: (error: Error) => void;
    let projectsCall = 0;

    vi.spyOn(bountyApi, 'listProjects').mockImplementation(includeArchived => {
      projectsCall += 1;
      if (projectsCall === 1) {
        return new Promise(resolve => {
          resolveFirstProjects = resolve;
        });
      }
      if (projectsCall === 2) {
        return new Promise((_, reject) => {
          rejectRetryProjects = reject;
        });
      }
      return originalListProjects(includeArchived);
    });

    vi.spyOn(bountyApi, 'listBounties').mockImplementation(query => {
      if (query?.q === 'fail-search') {
        return Promise.reject(new Error('SEARCH_FAILED'));
      }
      return originalListBounties(query);
    });

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await vi.waitFor(() => expect(resolveFirstProjects).toBeTypeOf('function'));

    // 场景 1：慢速 listProjects 挂起期间触发较新搜索并失败，随后慢速工作区成功返回不得清除错误横幅
    const searchInput = target.querySelector<HTMLInputElement>('[data-filter="search"]')!;
    searchInput.value = 'fail-search';
    searchInput.dispatchEvent(new Event('input', { bubbles: true }));
    await flush();

    let banner = target.querySelector<HTMLElement>('[data-status="service-connection-failed"]');
    expect(banner).not.toBeNull();
    expect(banner!.textContent).toContain('SEARCH_FAILED');

    resolveFirstProjects(await originalListProjects(true));
    await flush();

    banner = target.querySelector<HTMLElement>('[data-status="service-connection-failed"]');
    expect(banner).not.toBeNull();
    expect(banner!.textContent).toContain('SEARCH_FAILED');

    // 场景 2：点击重试触发慢速工作区刷新，挂起期间触发较新搜索成功清除横幅，随后慢速工作区失败不得覆盖回错误横幅
    banner!.querySelector<HTMLButtonElement>('[data-action="retry-service-connection"]')!.click();
    await vi.waitFor(() => expect(rejectRetryProjects).toBeTypeOf('function'));

    searchInput.value = 'ok-search';
    searchInput.dispatchEvent(new Event('input', { bubbles: true }));
    await flush();

    expect(target.querySelector('[data-status="service-connection-failed"]')).toBeNull();
    expect(target.querySelectorAll('.bounty-card')).toHaveLength(1);

    rejectRetryProjects(new Error('LATE_WORKSPACE_FAILURE'));
    await flush();

    expect(target.querySelector('[data-status="service-connection-failed"]')).toBeNull();
    expect(target.querySelectorAll('.bounty-card')).toHaveLength(1);
  });

  it('悬赏卡片在描述非空时渲染 Markdown 与 KaTeX 摘要，空描述时不渲染预览容器', async () => {
    const { port } = createMemoryPort();
    const bountyApi = createMemoryBountyClient();
    const proj = await bountyApi.createProject({ name: '卡片预览项目' });
    const withDesc = await bountyApi.createBounty({
      project_id: proj.id,
      title: '带公式描述的悬赏',
      description: '验证 **加粗摘要**、行内代码 `wal_mode` 与公式 $E = mc^2$',
      difficulty: 'A',
      assignee_mode: 'any',
      status: 'OPEN',
    });
    const emptyDesc = await bountyApi.createBounty({
      project_id: proj.id,
      title: '无描述悬赏',
      description: '   ',
      difficulty: 'C',
      assignee_mode: 'any',
      status: 'OPEN',
    });

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    const withDescCard = target.querySelector<HTMLElement>(`[data-bounty-id="${withDesc.id}"]`)!;
    const preview = withDescCard.querySelector<HTMLElement>('[data-slot="bounty-card-preview"]');
    expect(preview).not.toBeNull();
    expect(preview!.querySelector('strong')?.textContent).toBe('加粗摘要');
    expect(preview!.querySelector('code')?.textContent).toBe('wal_mode');
    expect(preview!.querySelector('.katex')).not.toBeNull();

    const emptyDescCard = target.querySelector<HTMLElement>(`[data-bounty-id="${emptyDesc.id}"]`)!;
    expect(emptyDescCard.querySelector('[data-slot="bounty-card-preview"]')).toBeNull();
  });

  it('支持设置页 Agent 准入与协议（局域网免审开关关闭/开启、邀请令牌生成/复制/作废、附带令牌开关与 SKILL.md 预览导出）', async () => {
    const copySpy = vi.spyOn(clipboard, 'writeText').mockResolvedValue(undefined);
    const { port, exported } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'settings' },
    });
    const bountyApi = createMemoryBountyClient();

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    const onboardingSection = target.querySelector<HTMLElement>('[data-section="agent-onboarding"]')!;
    expect(onboardingSection).not.toBeNull();

    // 局域网免审注册默认不常驻展示长段说明，点击信息按钮可展开/收起 Popover 并支持 Escape 关闭
    const lanRow = onboardingSection.querySelector<HTMLElement>('[data-slot="lan-registration-row"]')!;
    expect(lanRow.querySelector('.settings-extension-desc')).toBeNull();
    const lanHelpBtn = lanRow.querySelector<HTMLButtonElement>('[data-action="toggle-lan-registration-help"]')!;
    const lanHelpPopover = lanRow.querySelector<HTMLElement>('#lan-open-registration-help')!;
    expect(lanHelpBtn).not.toBeNull();
    expect(lanHelpBtn.getAttribute('aria-expanded')).toBe('false');
    expect(lanHelpPopover.hidden).toBe(true);

    lanHelpBtn.click();
    await flush();
    expect(lanHelpBtn.getAttribute('aria-expanded')).toBe('true');
    expect(lanHelpPopover.hidden).toBe(false);
    expect(lanHelpPopover.textContent).toContain(
      '开启后，外部 Agent 无需邀请令牌即可直接自注册；关闭时必须提供有效邀请令牌。',
    );

    lanHelpBtn.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await flush();
    expect(lanHelpBtn.getAttribute('aria-expanded')).toBe('false');
    expect(lanHelpPopover.hidden).toBe(true);

    // 免审关闭状态下预览 SKILL.md，确认包含令牌占位符且要求邀请令牌
    onboardingSection.querySelector<HTMLButtonElement>('[data-action="preview-skill-md"]')!.click();
    await flush();
    const initialPreview = onboardingSection.querySelector<HTMLElement>('[data-slot="skill-md-preview"]')!;
    expect(initialPreview).not.toBeNull();
    expect(initialPreview.textContent).toContain('<INVITE_TOKEN>');
    expect(initialPreview.textContent).toContain('需要有效邀请令牌');

    // 切换局域网免审注册开关：关闭 -> 开启
    const lanSwitch = onboardingSection.querySelector<HTMLInputElement>(
      '[data-action="toggle-lan-registration"]',
    )!;
    expect(lanSwitch.checked).toBe(false);
    lanSwitch.click();
    await flush();
    expect(lanSwitch.checked).toBe(true);
    expect((await bountyApi.getOnboardingSettings()).lan_open_registration).toBe(true);

    // 生成一次性邀请令牌
    const labelInput = onboardingSection.querySelector<HTMLInputElement>('#invite-token-label-input')!;
    const maxUsesInput = onboardingSection.querySelector<HTMLInputElement>('#invite-token-max-uses-input')!;
    const expiresInput = onboardingSection.querySelector<HTMLInputElement>('#invite-token-expires-input')!;
    labelInput.value = '本地 Codex 接入';
    labelInput.dispatchEvent(new Event('input', { bubbles: true }));
    maxUsesInput.value = '2';
    maxUsesInput.dispatchEvent(new Event('input', { bubbles: true }));
    expiresInput.value = '24';
    expiresInput.dispatchEvent(new Event('input', { bubbles: true }));

    onboardingSection.querySelector<HTMLButtonElement>('[data-action="create-invite-token"]')!.click();
    await flush();

    const rawBanner = onboardingSection.querySelector<HTMLElement>('[data-slot="created-invite-raw-token"]');
    expect(rawBanner).not.toBeNull();
    const rawTokenText = rawBanner!.querySelector('code')?.textContent?.trim() ?? '';
    expect(rawTokenText.startsWith('tr_inv_')).toBe(true);

    // 复制一次性明文令牌
    rawBanner!.querySelector<HTMLButtonElement>('[data-action="copy-invite-token"]')!.click();
    await flush();
    expect(copySpy).toHaveBeenCalledWith(rawTokenText);
    expect(onboardingSection.querySelector('[data-status="onboarding-feedback"]')?.textContent).toContain(
      '令牌已复制',
    );

    // 邀请令牌列表展示备注、前缀与有效状态
    const tokenList = onboardingSection.querySelector<HTMLElement>('[data-slot="invite-token-list"]')!;
    expect(tokenList.textContent).toContain('本地 Codex 接入');
    expect(tokenList.textContent).toContain('有效');

    // 使用刚生成的令牌预览 SKILL.md（免审已开启 + 内嵌真实令牌）
    rawBanner!.querySelector<HTMLButtonElement>('[data-action="preview-skill-with-token"]')!.click();
    await flush();
    const skillPreview = onboardingSection.querySelector<HTMLElement>('[data-slot="skill-md-preview"]')!;
    expect(skillPreview).not.toBeNull();
    expect(skillPreview.textContent).toContain(rawTokenText);
    expect(skillPreview.textContent).toContain('已开启局域网免审注册');

    // 复制与导出 SKILL.md
    onboardingSection.querySelector<HTMLButtonElement>('[data-action="copy-skill-md"]')!.click();
    await flush();
    expect(copySpy).toHaveBeenLastCalledWith(skillPreview.textContent ?? '');

    onboardingSection.querySelector<HTMLButtonElement>('[data-action="export-skill-md"]')!.click();
    await flush();
    expect(exported.at(-1)?.filename).toBe('SKILL.md');
    expect(exported.at(-1)?.text).toContain(rawTokenText);

    // 关闭「附带邀请令牌」开关后重新预览，恢复 <INVITE_TOKEN> 占位符
    const attachSwitch = onboardingSection.querySelector<HTMLInputElement>('#attach-invite-token-switch')!;
    expect(attachSwitch.checked).toBe(true);
    attachSwitch.click();
    await flush();
    expect(attachSwitch.checked).toBe(false);
    onboardingSection.querySelector<HTMLButtonElement>('[data-action="preview-skill-md"]')!.click();
    await flush();
    expect(onboardingSection.querySelector('[data-slot="skill-md-preview"]')?.textContent).toContain(
      '<INVITE_TOKEN>',
    );

    // 作废邀请令牌后状态变为已作废，且隐藏「用此令牌预览」与附带开关
    tokenList.querySelector<HTMLButtonElement>('[data-action="revoke-invite-token"]')!.click();
    await flush();
    expect(tokenList.textContent).toContain('已作废');
    expect(tokenList.querySelector('[data-action="revoke-invite-token"]')).toBeNull();
    expect(onboardingSection.querySelector('[data-action="preview-skill-with-token"]')).toBeNull();
    expect(onboardingSection.querySelector('#attach-invite-token-switch')).toBeNull();

    // 切换局域网免审注册开关：开启 -> 关闭
    lanSwitch.click();
    await flush();
    expect(lanSwitch.checked).toBe(false);
    expect((await bountyApi.getOnboardingSettings()).lan_open_registration).toBe(false);
  });

  it('支持 Agent 名录筛选、订阅范围与段位展示（不暴露 MMR）、停用启用与二次确认吊销凭证及 SKILL.md 接入指南弹窗', async () => {
    const { port } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'agents' },
    });
    const bountyApi = createMemoryBountyClient({ lan_open_registration: true });
    const projA = await bountyApi.createProject({ name: 'Kernel 项目' });
    const projB = await bountyApi.createProject({ name: 'UI 项目' });

    const registeredIdle = await bountyApi.registerAgent({
      name: 'codex-idle',
      description: '专注前端与单测修复',
      capabilities: ['svelte', 'vitest'],
      subscribe_all_projects: false,
      project_ids: [projA.id, projB.id],
    });
    const agentIdle = registeredIdle.agent;
    const { agent: agentBusy } = await bountyApi.registerAgent({
      name: 'rust-worker',
      description: '专注 Rust 并发内核',
      capabilities: ['rust', 'sqlite'],
      subscribe_all_projects: true,
    });
    const openTask = await bountyApi.createBounty({
      project_id: projA.id,
      title: '并发压测任务',
      difficulty: 'B',
      assignee_mode: 'any',
      status: 'OPEN',
    });
    await bountyApi.simulateClaim(openTask.id, agentBusy.id);

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    const agentList = target.querySelector<HTMLElement>('[data-slot="agent-list"]')!;
    expect(agentList).not.toBeNull();
    expect(agentList.querySelectorAll('[data-agent-id]')).toHaveLength(2);
    // 严禁暴露隐藏 MMR 数值（如 1000）且不包含中点分隔符
    expect(agentList.textContent).not.toContain('1000');
    expect(agentList.textContent).not.toContain('·');

    const idleCard = agentList.querySelector<HTMLElement>(`[data-agent-id="${agentIdle.id}"]`)!;
    expect(idleCard.dataset.agentTier).toBe('Unranked');
    expect(idleCard.textContent).toContain('[Unranked] 未定级');
    expect(idleCard.textContent).toContain('正常接入');
    expect(idleCard.textContent).toContain('空闲');
    expect(idleCard.textContent).toContain('Kernel 项目');
    expect(idleCard.textContent).toContain('UI 项目');
    expect(idleCard.textContent).toContain('svelte');

    const busyCard = agentList.querySelector<HTMLElement>(`[data-agent-id="${agentBusy.id}"]`)!;
    expect(busyCard.textContent).toContain('全部项目');
    expect(busyCard.textContent).toContain('1 个进行中租约');

    // 按活跃度筛选：空闲可接单 / 执行任务中 / 全部 Agent
    target.querySelector<HTMLButtonElement>('[data-agent-activity-filter="idle"]')!.click();
    await flush();
    expect(target.querySelectorAll('[data-agent-id]')).toHaveLength(1);
    expect(target.querySelector<HTMLElement>('[data-agent-id]')?.dataset.agentId).toBe(agentIdle.id);

    target.querySelector<HTMLButtonElement>('[data-agent-activity-filter="busy"]')!.click();
    await flush();
    expect(target.querySelectorAll('[data-agent-id]')).toHaveLength(1);
    expect(target.querySelector<HTMLElement>('[data-agent-id]')?.dataset.agentId).toBe(agentBusy.id);

    target.querySelector<HTMLButtonElement>('[data-agent-activity-filter="all"]')!.click();
    await flush();
    expect(target.querySelectorAll('[data-agent-id]')).toHaveLength(2);

    // 停用与重新启用 Agent
    target
      .querySelector<HTMLElement>(`[data-agent-id="${agentIdle.id}"]`)!
      .querySelector<HTMLButtonElement>('[data-action="disable-agent"]')!
      .click();
    await flush();
    const disabledCard = target.querySelector<HTMLElement>(`[data-agent-id="${agentIdle.id}"]`)!;
    expect(disabledCard.dataset.agentStatus).toBe('disabled');
    expect(disabledCard.textContent).toContain('已停用');
    await expect(bountyApi.agentGetMe(registeredIdle.api_key)).rejects.toMatchObject({
      code: 'AGENT_DISABLED',
    });

    // 按状态筛选：已停用
    target.querySelector<HTMLButtonElement>('[data-agent-status-filter="disabled"]')!.click();
    await flush();
    expect(target.querySelectorAll('[data-agent-id]')).toHaveLength(1);

    // 恢复全部状态并重新启用
    target.querySelector<HTMLButtonElement>('[data-agent-status-filter="ALL"]')!.click();
    await flush();
    target
      .querySelector<HTMLElement>(`[data-agent-id="${agentIdle.id}"]`)!
      .querySelector<HTMLButtonElement>('[data-action="enable-agent"]')!
      .click();
    await flush();
    expect(
      target.querySelector<HTMLElement>(`[data-agent-id="${agentIdle.id}"]`)?.dataset.agentStatus,
    ).toBe('active');
    expect((await bountyApi.agentGetMe(registeredIdle.api_key)).status).toBe('active');

    // 吊销凭证支持取消二次确认，确认后不可恢复
    const cardBeforeRevoke = target.querySelector<HTMLElement>(`[data-agent-id="${agentIdle.id}"]`)!;
    cardBeforeRevoke.querySelector<HTMLButtonElement>('[data-action="revoke-agent"]')!.click();
    await flush();
    cardBeforeRevoke.querySelector<HTMLButtonElement>('[data-action="cancel-revoke-agent"]')!.click();
    await flush();
    expect(cardBeforeRevoke.dataset.agentStatus).toBe('active');
    expect(cardBeforeRevoke.querySelector('[data-action="disable-agent"]')).not.toBeNull();

    cardBeforeRevoke.querySelector<HTMLButtonElement>('[data-action="revoke-agent"]')!.click();
    await flush();
    const confirmBtn = cardBeforeRevoke.querySelector<HTMLButtonElement>(
      '[data-action="confirm-revoke-agent"]',
    );
    expect(confirmBtn).not.toBeNull();
    confirmBtn!.click();
    await flush();

    const revokedCard = target.querySelector<HTMLElement>(`[data-agent-id="${agentIdle.id}"]`)!;
    expect(revokedCard.dataset.agentStatus).toBe('revoked');
    expect(revokedCard.querySelector('[data-action="enable-agent"]')).toBeNull();
    expect(revokedCard.querySelector('[data-action="disable-agent"]')).toBeNull();
    expect(revokedCard.querySelector('[data-action="revoke-agent"]')).toBeNull();
    await expect(bountyApi.agentGetMe(registeredIdle.api_key)).rejects.toMatchObject({
      code: 'AGENT_REVOKED',
    });

    // 打开右上角「接入指南 (SKILL.md)」弹窗并可一键跳转到设置
    target.querySelector<HTMLButtonElement>('[data-action="open-skill-guide"]')!.click();
    await flush();
    const guideDialog = target.querySelector<HTMLElement>('[data-dialog="skill-guide"]');
    expect(guideDialog).not.toBeNull();
    expect(guideDialog!.querySelector('[data-slot="skill-md-preview"]')?.textContent).toContain(
      '# Taskrail Agent',
    );
    guideDialog!.querySelector<HTMLButtonElement>('[data-action="jump-to-onboarding-settings"]')!.click();
    await flush();
    expect(target.querySelector('[data-view="settings"]')).not.toBeNull();
  });

  it('Agent 名录并发切换筛选时忽略迟到响应，关闭 SKILL.md 弹窗后不弹出过期错误提示', async () => {
    const { port } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'agents' },
    });
    const baseApi = createMemoryBountyClient({ lan_open_registration: true });
    const proj = await baseApi.createProject({ name: 'Race 项目' });
    const { agent: agentIdle } = await baseApi.registerAgent({
      name: 'idle-agent',
      capabilities: ['ts'],
    });
    const { agent: agentBusy } = await baseApi.registerAgent({
      name: 'busy-agent',
      capabilities: ['rust'],
    });
    const task = await baseApi.createBounty({
      project_id: proj.id,
      title: '占用任务',
      difficulty: 'C',
      assignee_mode: 'any',
      status: 'OPEN',
    });
    await baseApi.simulateClaim(task.id, agentBusy.id);

    const pending: { resolveBusy?: () => void; rejectSkill?: (error: Error) => void } = {};
    const bountyApi = {
      ...baseApi,
      async listAgents(filter?: Parameters<typeof baseApi.listAgents>[0]) {
        const result = await baseApi.listAgents(filter);
        if (filter?.activity === 'busy') {
          await new Promise<void>(resolve => {
            pending.resolveBusy = resolve;
          });
        }
        return result;
      },
      async getSkillMarkdown() {
        await new Promise<never>((_resolve, reject) => {
          pending.rejectSkill = reject;
        });
        throw new Error('unreachable');
      },
    };

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    // 先触发慢速 busy 筛选，未返回前立即切换到 idle 筛选
    target.querySelector<HTMLButtonElement>('[data-agent-activity-filter="busy"]')!.click();
    await flush();
    target.querySelector<HTMLButtonElement>('[data-agent-activity-filter="idle"]')!.click();
    await flush();

    expect(target.querySelectorAll('[data-agent-id]')).toHaveLength(1);
    expect(target.querySelector<HTMLElement>('[data-agent-id]')?.dataset.agentId).toBe(agentIdle.id);

    // 迟到的 busy 响应落地后不得覆盖当前 idle 名录
    pending.resolveBusy?.();
    await flush();
    expect(target.querySelectorAll('[data-agent-id]')).toHaveLength(1);
    expect(target.querySelector<HTMLElement>('[data-agent-id]')?.dataset.agentId).toBe(agentIdle.id);
    expect(target.textContent).not.toContain(agentBusy.name);

    // 打开接入指南弹窗后立即关闭，迟到的 SKILL.md 加载失败不应触发过期 toast
    target.querySelector<HTMLButtonElement>('[data-action="open-skill-guide"]')!.click();
    await flush();
    const closeBtn = target.querySelector<HTMLButtonElement>('[data-dialog="skill-guide"] .icon-button');
    expect(closeBtn).not.toBeNull();
    closeBtn!.click();
    await flush();
    pending.rejectSkill?.(new Error('late skill failure'));
    await flush();
    expect(target.querySelector('[data-dialog="skill-guide"]')).toBeNull();
    expect(target.querySelector('.toast-banner')).toBeNull();
  });

  it('支持免审开关失败回滚、服务连接切换清理旧令牌与 SKILL.md 预览，以及浏览器 Blob 导出回退', async () => {
    const baseApi = createMemoryBountyClient();
    const { port: desktopPort } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'settings' },
    });

    let failLanOnce = true;
    const apiWithLanFailure = {
      ...baseApi,
      async updateOnboardingSettings(input: { lan_open_registration: boolean }) {
        if (failLanOnce) {
          failLanOnce = false;
          throw new Error('网络异常，切换失败');
        }
        return await baseApi.updateOnboardingSettings(input);
      },
    };

    const fetchSpy = vi.fn(async () => {
      return new Response(JSON.stringify({ code: 'UNAUTHORIZED', message: 'Invalid admin token' }), {
        status: 401,
        headers: { 'Content-Type': 'application/json' },
      });
    });
    vi.stubGlobal('fetch', fetchSpy);

    instance = mount(App, { target, props: { files: desktopPort, bountyApi: apiWithLanFailure } });
    await flush();

    const onboardingSection = target.querySelector<HTMLElement>('[data-section="agent-onboarding"]')!;
    const lanSwitch = onboardingSection.querySelector<HTMLInputElement>(
      '[data-action="toggle-lan-registration"]',
    )!;

    // 切换免审失败时 DOM checked 状态应立即回滚为 false 并展示错误提示
    expect(lanSwitch.checked).toBe(false);
    lanSwitch.click();
    await flush();
    expect(lanSwitch.checked).toBe(false);
    expect(onboardingSection.querySelector('[data-status="onboarding-error"]')?.textContent).toContain(
      '网络异常，切换失败',
    );

    // 先生成令牌并预览 SKILL.md
    onboardingSection.querySelector<HTMLButtonElement>('[data-action="create-invite-token"]')!.click();
    await flush();
    expect(onboardingSection.querySelector('[data-slot="created-invite-raw-token"]')).not.toBeNull();
    onboardingSection.querySelector<HTMLButtonElement>('[data-action="preview-skill-md"]')!.click();
    await flush();
    expect(onboardingSection.querySelector('[data-slot="skill-md-preview"]')).not.toBeNull();

    // 切换到远端失败节点后，旧连接的明文令牌横幅与 SKILL.md 预览必须被清理，不得跨服残留
    target.querySelector<HTMLButtonElement>('[data-service-mode="remote"]')!.click();
    await flush();
    const urlInput = target.querySelector<HTMLInputElement>('#remote-server-url-input')!;
    const tokenInput = target.querySelector<HTMLInputElement>('#remote-admin-token-input')!;
    urlInput.value = 'http://127.0.0.1:9999';
    urlInput.dispatchEvent(new Event('input', { bubbles: true }));
    tokenInput.value = 'wrong-token';
    tokenInput.dispatchEvent(new Event('input', { bubbles: true }));
    target.querySelector<HTMLButtonElement>('[data-action="save-reconnect"]')!.click();
    await flush();

    expect(onboardingSection.querySelector('[data-slot="created-invite-raw-token"]')).toBeNull();
    expect(onboardingSection.querySelector('[data-slot="skill-md-preview"]')).toBeNull();

    unmount(instance);
    instance = null;
    vi.unstubAllGlobals();

    // 验证浏览器无 exportText 端口时，导出 SKILL.md 走 Blob 下载回退路径
    const { port: basePort } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'settings' },
    });
    const browserFallbackPort: FilePort = {
      isBrowser: true,
      loadConfig: basePort.loadConfig,
      saveConfig: basePort.saveConfig,
      openExternalLink: basePort.openExternalLink,
    };
    const createObjectUrlSpy = vi.fn(() => 'blob:mock-skill-url');
    const revokeObjectUrlSpy = vi.fn();
    Object.defineProperty(URL, 'createObjectURL', {
      value: createObjectUrlSpy,
      configurable: true,
      writable: true,
    });
    Object.defineProperty(URL, 'revokeObjectURL', {
      value: revokeObjectUrlSpy,
      configurable: true,
      writable: true,
    });
    const anchorClickSpy = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});

    instance = mount(App, {
      target,
      props: { files: browserFallbackPort, bountyApi: createMemoryBountyClient() },
    });
    await flush();

    target
      .querySelector<HTMLElement>('[data-section="agent-onboarding"]')!
      .querySelector<HTMLButtonElement>('[data-action="export-skill-md"]')!
      .click();
    await flush();

    expect(createObjectUrlSpy).toHaveBeenCalledTimes(1);
    expect(anchorClickSpy).toHaveBeenCalledTimes(1);
    expect(revokeObjectUrlSpy).toHaveBeenCalledWith('blob:mock-skill-url');
    expect(
      target.querySelector('[data-section="agent-onboarding"] [data-status="onboarding-feedback"]')
        ?.textContent,
    ).toContain('SKILL.md 已导出');
  });

  it('修改服务地址覆盖、关闭附带令牌或作废令牌后复制/导出 SKILL.md 必重新拉取最新参数且失败时不误报成功', async () => {
    const copySpy = vi.spyOn(clipboard, 'writeText').mockResolvedValue(undefined);
    const { port, exported } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'settings' },
    });
    const bountyApi = createMemoryBountyClient();
    const getSkillSpy = vi.spyOn(bountyApi, 'getSkillMarkdown');

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    const onboardingSection = target.querySelector<HTMLElement>('[data-section="agent-onboarding"]')!;

    // 生成令牌并预览 SKILL.md
    onboardingSection.querySelector<HTMLButtonElement>('[data-action="create-invite-token"]')!.click();
    await flush();
    const rawToken = onboardingSection
      .querySelector<HTMLElement>('[data-slot="created-invite-raw-token"] code')
      ?.textContent?.trim()!;
    expect(rawToken.startsWith('tr_inv_')).toBe(true);

    onboardingSection
      .querySelector<HTMLButtonElement>('[data-action="preview-skill-with-token"]')!
      .click();
    await flush();
    expect(onboardingSection.querySelector('[data-slot="skill-md-preview"]')?.textContent).toContain(
      rawToken,
    );
    const callsAfterPreview = getSkillSpy.mock.calls.length;

    // 修改服务地址覆盖后，旧预览立即失效，但不盲目在输入时发请求；直接点击「复制 SKILL.md」时按新地址重新生成
    const baseUrlInput = onboardingSection.querySelector<HTMLInputElement>('#skill-base-url-input')!;
    baseUrlInput.value = 'http://192.168.10.20:9000';
    baseUrlInput.dispatchEvent(new Event('input', { bubbles: true }));
    await flush();
    expect(onboardingSection.querySelector('[data-slot="skill-md-preview"]')).toBeNull();
    expect(getSkillSpy).toHaveBeenCalledTimes(callsAfterPreview);

    onboardingSection.querySelector<HTMLButtonElement>('[data-action="copy-skill-md"]')!.click();
    await flush();
    expect(getSkillSpy).toHaveBeenCalledTimes(callsAfterPreview + 1);
    expect(getSkillSpy).toHaveBeenLastCalledWith({
      baseUrl: 'http://192.168.10.20:9000',
      inviteToken: rawToken,
    });
    expect(copySpy.mock.calls.at(-1)?.[0]).toContain('http://192.168.10.20:9000');
    expect(copySpy.mock.calls.at(-1)?.[0]).toContain(rawToken);

    // 关闭「附带邀请令牌」开关后旧预览清除，直接点击「导出 SKILL.md」不携带旧令牌
    const attachSwitch = onboardingSection.querySelector<HTMLInputElement>('#attach-invite-token-switch')!;
    attachSwitch.click();
    await flush();
    expect(onboardingSection.querySelector('[data-slot="skill-md-preview"]')).toBeNull();

    onboardingSection.querySelector<HTMLButtonElement>('[data-action="export-skill-md"]')!.click();
    await flush();
    expect(getSkillSpy).toHaveBeenLastCalledWith({
      baseUrl: 'http://192.168.10.20:9000',
    });
    expect(exported.at(-1)?.text).toContain('http://192.168.10.20:9000');
    expect(exported.at(-1)?.text).toContain('<INVITE_TOKEN>');
    expect(exported.at(-1)?.text).not.toContain(rawToken);

    // 重新开启附带令牌并预览，然后作废令牌，直接点击「复制 SKILL.md」不得携带已作废令牌
    attachSwitch.click();
    await flush();
    onboardingSection.querySelector<HTMLButtonElement>('[data-action="preview-skill-md"]')!.click();
    await flush();
    expect(onboardingSection.querySelector('[data-slot="skill-md-preview"]')?.textContent).toContain(
      rawToken,
    );

    onboardingSection
      .querySelector<HTMLButtonElement>('[data-action="revoke-invite-token"]')!
      .click();
    await flush();
    expect(onboardingSection.querySelector('[data-slot="skill-md-preview"]')).toBeNull();

    baseUrlInput.value = 'http://127.0.0.1:8420';
    baseUrlInput.dispatchEvent(new Event('input', { bubbles: true }));
    await flush();
    onboardingSection.querySelector<HTMLButtonElement>('[data-action="copy-skill-md"]')!.click();
    await flush();
    expect(copySpy.mock.calls.at(-1)?.[0]).toContain('http://127.0.0.1:8420');
    expect(copySpy.mock.calls.at(-1)?.[0]).not.toContain(rawToken);

    // 切换免审开关使预览失效后，若重新加载失败则中止复制且不提示成功
    onboardingSection
      .querySelector<HTMLInputElement>('[data-action="toggle-lan-registration"]')!
      .click();
    await flush();
    expect(onboardingSection.querySelector('[data-slot="skill-md-preview"]')).toBeNull();

    const copyCallsBeforeFail = copySpy.mock.calls.length;
    getSkillSpy.mockRejectedValueOnce(new Error('SKILL_LOAD_FAILED'));
    onboardingSection.querySelector<HTMLButtonElement>('[data-action="copy-skill-md"]')!.click();
    await flush();
    expect(copySpy).toHaveBeenCalledTimes(copyCallsBeforeFail);
    expect(onboardingSection.querySelector('[data-status="onboarding-error"]')?.textContent).toContain(
      'SKILL_LOAD_FAILED',
    );
    expect(onboardingSection.querySelector('[data-status="onboarding-feedback"]')).toBeNull();
  });

  it('服务 A 延迟返回 createInviteToken 期间切换到服务 B 后，旧响应不会写入明文令牌或开启附带开关', async () => {
    const { port } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'settings' },
    });
    const serviceAApi = createMemoryBountyClient();
    const realCreateInvite = serviceAApi.createInviteToken.bind(serviceAApi);
    let resolveDelayedCreate!: (created: Awaited<ReturnType<typeof realCreateInvite>>) => void;
    let delayedTokenPromise!: Promise<Awaited<ReturnType<typeof realCreateInvite>>>;
    vi.spyOn(serviceAApi, 'createInviteToken').mockImplementation(input => {
      delayedTokenPromise = realCreateInvite(input);
      return new Promise(resolve => {
        resolveDelayedCreate = resolve;
      });
    });

    vi.spyOn(globalThis, 'fetch').mockImplementation(async input => {
      const url = typeof input === 'string' ? input : input.toString();
      if (url.startsWith('https://service-b.taskrail.test/api/v1/admin/onboarding')) {
        return new Response(
          JSON.stringify({ lan_open_registration: true, invite_tokens: [] }),
          { status: 200, headers: { 'Content-Type': 'application/json' } },
        );
      }
      if (
        url.startsWith('https://service-b.taskrail.test/api/v1/projects') ||
        url.startsWith('https://service-b.taskrail.test/api/v1/bounties') ||
        url.startsWith('https://service-b.taskrail.test/api/v1/agents')
      ) {
        return new Response(JSON.stringify([]), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      return new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      });
    });

    instance = mount(App, { target, props: { files: port, bountyApi: serviceAApi } });
    await flush();

    const onboardingSection = target.querySelector<HTMLElement>('[data-section="agent-onboarding"]')!;
    onboardingSection.querySelector<HTMLButtonElement>('[data-action="create-invite-token"]')!.click();
    await vi.waitFor(() => expect(resolveDelayedCreate).toBeTypeOf('function'));

    // 切换到服务 B
    target.querySelector<HTMLButtonElement>('[data-service-mode="remote"]')!.click();
    await flush();
    const urlInput = target.querySelector<HTMLInputElement>('#remote-server-url-input')!;
    const tokenInput = target.querySelector<HTMLInputElement>('#remote-admin-token-input')!;
    urlInput.value = 'https://service-b.taskrail.test';
    urlInput.dispatchEvent(new Event('input', { bubbles: true }));
    tokenInput.value = 'service-b-admin';
    tokenInput.dispatchEvent(new Event('input', { bubbles: true }));
    target.querySelector<HTMLButtonElement>('[data-action="save-reconnect"]')!.click();

    await vi.waitFor(() => {
      expect(
        target.querySelector<HTMLInputElement>('[data-action="toggle-lan-registration"]')?.checked,
      ).toBe(true);
    });
    expect(
      onboardingSection.querySelector<HTMLButtonElement>('[data-action="create-invite-token"]')
        ?.disabled,
    ).toBe(false);

    // 放行服务 A 延迟响应后，绝不能污染服务 B 的明文令牌横幅或附带开关
    const createdFromA = await delayedTokenPromise;
    resolveDelayedCreate(createdFromA);
    await flush();

    expect(onboardingSection.querySelector('[data-slot="created-invite-raw-token"]')).toBeNull();
    expect(onboardingSection.querySelector('#attach-invite-token-switch')).toBeNull();
    expect(onboardingSection.textContent).not.toContain(createdFromA.raw_token);
    expect(onboardingSection.querySelector('[data-status="onboarding-error"]')).toBeNull();
  });

  it('仅准入设置或 Agent 列表加载失败时显式展示错误并支持重试恢复，且不误显假默认值或空状态', async () => {
    const { port } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'settings' },
    });
    const bountyApi = createMemoryBountyClient({ lan_open_registration: true });
    await bountyApi.createProject({ name: '正常项目' });
    await bountyApi.registerAgent({
      name: 'alpha-worker',
      capabilities: ['rust'],
    });

    const originalGetOnboarding = bountyApi.getOnboardingSettings.bind(bountyApi);
    const originalListAgents = bountyApi.listAgents.bind(bountyApi);
    vi.spyOn(bountyApi, 'getOnboardingSettings')
      .mockRejectedValueOnce(new Error('ONBOARDING_503'))
      .mockImplementation(() => originalGetOnboarding());
    vi.spyOn(bountyApi, 'listAgents')
      .mockRejectedValueOnce(new Error('AGENTS_503'))
      .mockRejectedValueOnce(new Error('AGENTS_503'))
      .mockImplementation(filter => originalListAgents(filter));

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    // 其余工作区接口正常，不触发全局断连横幅
    expect(target.querySelector('[data-status="service-connection-failed"]')).toBeNull();

    // 准入面板显示加载错误与状态未知，并禁用免审开关
    const onboardingSection = target.querySelector<HTMLElement>('[data-section="agent-onboarding"]')!;
    const loadError = onboardingSection.querySelector<HTMLElement>(
      '[data-status="onboarding-load-error"]',
    );
    expect(loadError).not.toBeNull();
    expect(loadError!.textContent).toContain('ONBOARDING_503');
    expect(onboardingSection.querySelector('[data-status="onboarding-unknown"]')).not.toBeNull();
    const lanSwitch = onboardingSection.querySelector<HTMLInputElement>(
      '[data-action="toggle-lan-registration"]',
    )!;
    expect(lanSwitch.disabled).toBe(true);
    expect(lanSwitch.checked).toBe(false);

    // 点击重试加载后恢复真实的开启状态
    loadError!.querySelector<HTMLButtonElement>('[data-action="retry-load-onboarding"]')!.click();
    await flush();
    expect(onboardingSection.querySelector('[data-status="onboarding-load-error"]')).toBeNull();
    expect(onboardingSection.querySelector('[data-status="onboarding-unknown"]')).toBeNull();
    expect(lanSwitch.disabled).toBe(false);
    expect(lanSwitch.checked).toBe(true);

    // 切换到 Agent 名录：展示加载错误且不误显「暂无已注册的 Agent」
    target.querySelector<HTMLButtonElement>('[data-nav-section="agents"]')!.click();
    await flush();

    const dirError = target.querySelector<HTMLElement>('[data-status="agent-directory-load-error"]');
    expect(dirError).not.toBeNull();
    expect(dirError!.textContent).toContain('AGENTS_503');
    expect(target.querySelector('[data-slot="agent-empty-state"]')).toBeNull();

    // 点击重试加载后恢复真实名录
    dirError!.querySelector<HTMLButtonElement>('[data-action="retry-load-agents"]')!.click();
    await flush();
    expect(target.querySelector('[data-status="agent-directory-load-error"]')).toBeNull();
    expect(target.querySelector('[data-slot="agent-list"]')?.textContent).toContain('alpha-worker');
  });

  it('Agent 携带重复能力标签时正常去重渲染且不触发 each_key_duplicate', async () => {
    const { port } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'agents' },
    });
    const bountyApi = createMemoryBountyClient({ lan_open_registration: true });
    const { agent } = await bountyApi.registerAgent({
      name: 'dup-cap-agent',
      capabilities: ['rust', 'rust', '  rust ', ''],
    });

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    const card = target.querySelector<HTMLElement>(`[data-agent-id="${agent.id}"]`)!;
    expect(card).not.toBeNull();
    const tags = [...card.querySelectorAll<HTMLElement>('.agent-capability-list .settings-tag')].map(
      el => el.textContent?.trim(),
    );
    expect(tags).toEqual(['rust']);
  });

  it('作废令牌后即使后续准入刷新返回 503，仍保持作废状态且不复用含旧令牌的 SKILL.md 缓存', async () => {
    const copySpy = vi.spyOn(clipboard, 'writeText').mockResolvedValue(undefined);
    const { port, exported } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'settings' },
    });
    const bountyApi = createMemoryBountyClient();
    const originalGetOnboarding = bountyApi.getOnboardingSettings.bind(bountyApi);
    const getSkillSpy = vi.spyOn(bountyApi, 'getSkillMarkdown');

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    const onboardingSection = target.querySelector<HTMLElement>('[data-section="agent-onboarding"]')!;

    // 生成令牌并预览含明文令牌的 SKILL.md
    onboardingSection.querySelector<HTMLButtonElement>('[data-action="create-invite-token"]')!.click();
    await flush();
    const rawToken = onboardingSection
      .querySelector<HTMLElement>('[data-slot="created-invite-raw-token"] code')
      ?.textContent?.trim()!;
    expect(rawToken.startsWith('tr_inv_')).toBe(true);

    onboardingSection
      .querySelector<HTMLButtonElement>('[data-action="preview-skill-with-token"]')!
      .click();
    await flush();
    expect(onboardingSection.querySelector('[data-slot="skill-md-preview"]')?.textContent).toContain(
      rawToken,
    );

    // 令作废后的 getOnboardingSettings 刷新返回 503
    vi.spyOn(bountyApi, 'getOnboardingSettings')
      .mockRejectedValueOnce(new Error('ONBOARDING_REFRESH_503'))
      .mockImplementation(() => originalGetOnboarding());

    const callsBeforeRevoke = getSkillSpy.mock.calls.length;
    onboardingSection
      .querySelector<HTMLButtonElement>('[data-action="revoke-invite-token"]')!
      .click();
    await flush();

    // 刷新 503 仅展示加载/刷新错误横幅，但作废操作本身成功且不回滚本地作废状态
    expect(
      onboardingSection.querySelector('[data-status="onboarding-load-error"]')?.textContent,
    ).toContain('ONBOARDING_REFRESH_503');
    expect(onboardingSection.querySelector('[data-status="onboarding-error"]')).toBeNull();
    expect(onboardingSection.querySelector('[data-status="onboarding-feedback"]')?.textContent).toContain(
      '已作废邀请令牌',
    );
    expect(
      onboardingSection.querySelector<HTMLElement>('[data-invite-token-status]')?.dataset
        .inviteTokenStatus,
    ).toBe('revoked');
    expect(onboardingSection.querySelector('#attach-invite-token-switch')).toBeNull();
    expect(onboardingSection.querySelector('[data-action="preview-skill-with-token"]')).toBeNull();
    expect(onboardingSection.querySelector('[data-slot="skill-md-preview"]')).toBeNull();

    // 点击复制 SKILL.md 必须重新拉取不含已作废令牌的新文档
    onboardingSection.querySelector<HTMLButtonElement>('[data-action="copy-skill-md"]')!.click();
    await flush();
    expect(getSkillSpy).toHaveBeenCalledTimes(callsBeforeRevoke + 1);
    expect(getSkillSpy.mock.calls.at(-1)?.[0]).not.toHaveProperty('inviteToken');
    expect(copySpy.mock.calls.at(-1)?.[0]).not.toContain(rawToken);
    expect(copySpy.mock.calls.at(-1)?.[0]).toContain('<INVITE_TOKEN>');

    // 点击导出 SKILL.md 同样不得包含已作废令牌
    onboardingSection.querySelector<HTMLButtonElement>('[data-action="export-skill-md"]')!.click();
    await flush();
    expect(exported.at(-1)?.text).not.toContain(rawToken);
    expect(exported.at(-1)?.text).toContain('<INVITE_TOKEN>');
  });

  it('准入设置或 Agent 名录重试加载与业务操作并发时互不取消，按钮不锁死且一次性明文令牌不丢失', async () => {
    const { port } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'settings' },
    });
    const bountyApi = createMemoryBountyClient({ lan_open_registration: true });
    await bountyApi.createProject({ name: '并发重试项目' });
    const { agent: targetAgent } = await bountyApi.registerAgent({
      name: 'concurrent-retry-agent',
      capabilities: ['ts'],
    });

    const originalGetOnboarding = bountyApi.getOnboardingSettings.bind(bountyApi);
    const originalCreateInvite = bountyApi.createInviteToken.bind(bountyApi);
    const originalListAgents = bountyApi.listAgents.bind(bountyApi);
    const originalDisableAgent = bountyApi.disableAgent.bind(bountyApi);

    let resolvePendingCreate!: (
      value: Awaited<ReturnType<typeof originalCreateInvite>>,
    ) => void;
    let pendingCreateResult!: Promise<Awaited<ReturnType<typeof originalCreateInvite>>>;

    vi.spyOn(bountyApi, 'getOnboardingSettings')
      .mockRejectedValueOnce(new Error('ONBOARDING_INIT_503'))
      .mockImplementation(() => originalGetOnboarding());
    vi.spyOn(bountyApi, 'createInviteToken').mockImplementation(input => {
      pendingCreateResult = originalCreateInvite(input);
      return new Promise(resolve => {
        resolvePendingCreate = resolve;
      });
    });

    let failAgentsNext = false;
    let resolvePendingDisable!: (
      value: Awaited<ReturnType<typeof originalDisableAgent>>,
    ) => void;
    let pendingDisableResult!: Promise<Awaited<ReturnType<typeof originalDisableAgent>>>;
    vi.spyOn(bountyApi, 'listAgents').mockImplementation(filter => {
      if (failAgentsNext) {
        failAgentsNext = false;
        return Promise.reject(new Error('AGENTS_REFRESH_503'));
      }
      return originalListAgents(filter);
    });
    vi.spyOn(bountyApi, 'disableAgent').mockImplementation(id => {
      pendingDisableResult = originalDisableAgent(id);
      return new Promise(resolve => {
        resolvePendingDisable = resolve;
      });
    });

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    // 1. 准入设置首次 503：点击「生成令牌」（挂起）同时点击「重试加载」
    const onboardingSection = target.querySelector<HTMLElement>('[data-section="agent-onboarding"]')!;
    expect(onboardingSection.querySelector('[data-status="onboarding-load-error"]')).not.toBeNull();

    const createBtn = onboardingSection.querySelector<HTMLButtonElement>(
      '[data-action="create-invite-token"]',
    )!;
    createBtn.click();
    await vi.waitFor(() => expect(resolvePendingCreate).toBeTypeOf('function'));
    expect(createBtn.disabled).toBe(true);
    expect(createBtn.textContent).toContain('正在生成…');

    onboardingSection
      .querySelector<HTMLButtonElement>('[data-action="retry-load-onboarding"]')!
      .click();
    await flush();
    expect(onboardingSection.querySelector('[data-status="onboarding-load-error"]')).toBeNull();

    // 放行挂起的生成响应后，按钮必须退出忙碌态，且一次性明文令牌正常展示
    const createdToken = await pendingCreateResult;
    resolvePendingCreate(createdToken);
    await flush();

    expect(createBtn.disabled).toBe(false);
    expect(createBtn.textContent).toContain('生成令牌');
    expect(
      onboardingSection.querySelector('[data-slot="created-invite-raw-token"] code')?.textContent,
    ).toContain(createdToken.raw_token);

    // 2. Agent 名录：触发一次筛选 503 展示重试横幅后，点击停用（挂起）同时点击「重试加载」
    target.querySelector<HTMLButtonElement>('[data-nav-section="agents"]')!.click();
    await flush();

    failAgentsNext = true;
    target.querySelector<HTMLButtonElement>('[data-agent-status-filter="ALL"]')!.click();
    await flush();
    expect(target.querySelector('[data-status="agent-directory-load-error"]')).not.toBeNull();

    const agentCard = target.querySelector<HTMLElement>(`[data-agent-id="${targetAgent.id}"]`)!;
    const disableBtn = agentCard.querySelector<HTMLButtonElement>('[data-action="disable-agent"]')!;
    disableBtn.click();
    await vi.waitFor(() => expect(resolvePendingDisable).toBeTypeOf('function'));
    expect(disableBtn.disabled).toBe(true);

    target.querySelector<HTMLButtonElement>('[data-action="retry-load-agents"]')!.click();
    await flush();
    expect(target.querySelector('[data-status="agent-directory-load-error"]')).toBeNull();

    resolvePendingDisable(await pendingDisableResult);
    await flush();

    const updatedCard = target.querySelector<HTMLElement>(`[data-agent-id="${targetAgent.id}"]`)!;
    expect(updatedCard.dataset.agentStatus).toBe('disabled');
    const enableBtn = updatedCard.querySelector<HTMLButtonElement>('[data-action="enable-agent"]')!;
    expect(enableBtn).not.toBeNull();
    expect(enableBtn.disabled).toBe(false);
  });

  it('剪贴板不可用或写入拒绝时，复制邀请令牌与复制 SKILL.md 展示错误信息且绝不误报已复制', async () => {
    const { port } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'settings' },
    });
    const bountyApi = createMemoryBountyClient();
    const originalClipboardDescriptor = Object.getOwnPropertyDescriptor(navigator, 'clipboard');

    try {
      instance = mount(App, { target, props: { files: port, bountyApi } });
      await flush();

      const onboardingSection = target.querySelector<HTMLElement>(
        '[data-section="agent-onboarding"]',
      )!;
      onboardingSection
        .querySelector<HTMLButtonElement>('[data-action="create-invite-token"]')!
        .click();
      await flush();

      // 场景 1：navigator.clipboard 不可用
      Object.defineProperty(navigator, 'clipboard', {
        value: undefined,
        configurable: true,
      });

      onboardingSection
        .querySelector<HTMLButtonElement>('[data-action="copy-invite-token"]')!
        .click();
      await flush();
      expect(
        onboardingSection.querySelector('[data-status="onboarding-error"]')?.textContent,
      ).toContain('Clipboard text writing is unavailable');
      expect(onboardingSection.querySelector('[data-status="onboarding-feedback"]')).toBeNull();
      expect(target.querySelector('.toast-bar')).toBeNull();

      onboardingSection
        .querySelector<HTMLButtonElement>('[data-action="copy-skill-md"]')!
        .click();
      await flush();
      expect(
        onboardingSection.querySelector('[data-status="onboarding-error"]')?.textContent,
      ).toContain('Clipboard text writing is unavailable');
      expect(onboardingSection.querySelector('[data-status="onboarding-feedback"]')).toBeNull();
      expect(target.querySelector('.toast-bar')).toBeNull();

      // 场景 2：Agent 名录接入指南弹窗中 navigator.clipboard.writeText 拒绝
      Object.defineProperty(navigator, 'clipboard', {
        value: { writeText: vi.fn().mockRejectedValue(new Error('CLIPBOARD_WRITE_DENIED')) },
        configurable: true,
      });

      target.querySelector<HTMLButtonElement>('[data-nav-section="agents"]')!.click();
      await flush();
      target.querySelector<HTMLButtonElement>('[data-action="open-skill-guide"]')!.click();
      await flush();

      const guideDialog = target.querySelector<HTMLElement>('[data-dialog="skill-guide"]')!;
      expect(guideDialog).not.toBeNull();
      guideDialog.querySelector<HTMLButtonElement>('[data-action="copy-skill-md"]')!.click();
      await flush();

      expect(guideDialog.querySelector('[role="alert"]')?.textContent).toContain(
        'CLIPBOARD_WRITE_DENIED',
      );
      expect(target.querySelector('.toast-bar')).toBeNull();
    } finally {
      if (originalClipboardDescriptor) {
        Object.defineProperty(navigator, 'clipboard', originalClipboardDescriptor);
      } else {
        delete (navigator as { clipboard?: unknown }).clipboard;
      }
    }
  });

  it('打开局域网免审帮助 Popover 后将焦点移至开关或其他控件，按 Escape 仍可立即关闭 Popover', async () => {
    const { port } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'settings' },
    });
    const bountyApi = createMemoryBountyClient();

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    const onboardingSection = target.querySelector<HTMLElement>('[data-section="agent-onboarding"]')!;
    const lanHelpBtn = onboardingSection.querySelector<HTMLButtonElement>(
      '[data-action="toggle-lan-registration-help"]',
    )!;
    const lanHelpPopover = onboardingSection.querySelector<HTMLElement>(
      '#lan-open-registration-help',
    )!;
    const lanSwitch = onboardingSection.querySelector<HTMLInputElement>(
      '#lan-open-registration-switch',
    )!;

    lanHelpBtn.click();
    await flush();
    expect(lanHelpBtn.getAttribute('aria-expanded')).toBe('true');
    expect(lanHelpPopover.hidden).toBe(false);

    // 模拟 Tab 切换焦点到开关上后按下 Escape
    lanSwitch.focus();
    lanSwitch.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await flush();

    expect(lanHelpBtn.getAttribute('aria-expanded')).toBe('false');
    expect(lanHelpPopover.hidden).toBe(true);
  });

  it('Agent 名录展示连接通道状态与最近推送时间，支持手动刷新、切页自动刷新、失败不误显旧在线状态且并发停用不锁死', async () => {
    const { port } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'agents' },
    });
    const bountyApi = createMemoryBountyClient({ lan_open_registration: true });
    await bountyApi.createProject({ name: '通道监测项目' });

    const streamReg = await bountyApi.registerAgent({
      name: 'stream-worker',
      capabilities: ['rust'],
    });
    const hookReg = await bountyApi.registerAgent({
      name: 'webhook-worker',
      capabilities: ['ts'],
      webhook_url: 'https://agent.example.test/webhook',
    });
    const offlineReg = await bountyApi.registerAgent({
      name: 'offline-worker',
      capabilities: ['svelte'],
    });

    const streamPushedAt = '2026-11-17T08:05:00.000Z';
    const hookPushedAt = '2026-11-17T09:15:00.000Z';
    const unsubStream1 = await bountyApi.agentOpenStream(streamReg.api_key, () => {});
    await bountyApi.simulateAgentPush(streamReg.agent.id, { pushedAt: streamPushedAt });
    await bountyApi.simulateAgentPush(hookReg.agent.id, { pushedAt: hookPushedAt });

    const originalListAgents = bountyApi.listAgents.bind(bountyApi);
    const originalDisableAgent = bountyApi.disableAgent.bind(bountyApi);

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    const listEl = target.querySelector<HTMLElement>('[data-slot="agent-list"]')!;
    expect(listEl).not.toBeNull();
    expect(listEl.textContent).not.toContain('·');
    expect(listEl.textContent).not.toContain('webhook_secret');
    expect(listEl.textContent).not.toContain('api_key_hash');
    expect(listEl.textContent?.toLowerCase()).not.toContain('mmr');
    if (hookReg.webhook_secret) {
      expect(listEl.textContent).not.toContain(hookReg.webhook_secret);
    }

    const streamCard = target.querySelector<HTMLElement>(`[data-agent-id="${streamReg.agent.id}"]`)!;
    const hookCard = target.querySelector<HTMLElement>(`[data-agent-id="${hookReg.agent.id}"]`)!;
    const offlineCard = target.querySelector<HTMLElement>(`[data-agent-id="${offlineReg.agent.id}"]`)!;

    expect(streamCard.querySelector<HTMLElement>('[data-agent-channel]')?.dataset.agentChannel).toBe('streaming');
    expect(streamCard.querySelector('[data-agent-channel]')?.textContent?.trim()).toBe('在线长连接');
    expect(streamCard.querySelector('[data-slot="agent-last-pushed"]')?.textContent?.trim()).toBe(
      formatTimestamp(streamPushedAt),
    );

    expect(hookCard.querySelector<HTMLElement>('[data-agent-channel]')?.dataset.agentChannel).toBe('webhook');
    expect(hookCard.querySelector('[data-agent-channel]')?.textContent?.trim()).toBe('Webhook');
    expect(hookCard.querySelector('[data-slot="agent-last-pushed"]')?.textContent?.trim()).toBe(
      formatTimestamp(hookPushedAt),
    );

    expect(offlineCard.querySelector<HTMLElement>('[data-agent-channel]')?.dataset.agentChannel).toBe('offline');
    expect(offlineCard.querySelector('[data-agent-channel]')?.textContent?.trim()).toBe('离线');
    expect(offlineCard.querySelector('[data-slot="agent-last-pushed"]')?.textContent?.trim()).toBe('从未推送');

    // 断开长连接并记录推送失败后点击「刷新」，名录立即更新为 offline
    unsubStream1();
    await bountyApi.simulateAgentPush(streamReg.agent.id, { recentFailedCount: 1 });
    target.querySelector<HTMLButtonElement>('[data-action="refresh-agents"]')!.click();
    await flush();
    expect(
      target.querySelector<HTMLElement>(`[data-agent-id="${streamReg.agent.id}"] [data-agent-channel]`)?.dataset
        .agentChannel,
    ).toBe('offline');
    expect(
      target.querySelector(`[data-agent-id="${streamReg.agent.id}"] [data-agent-channel]`)?.textContent?.trim(),
    ).toBe('离线');

    // 重新建立长连接，切到悬赏板再切回 Agent 名录时自动拉取最新在线通道状态
    await bountyApi.agentOpenStream(streamReg.api_key, () => {});
    target.querySelector<HTMLButtonElement>('[data-nav-section="bounties"]')!.click();
    await flush();
    target.querySelector<HTMLButtonElement>('[data-nav-section="agents"]')!.click();
    await flush();
    expect(
      target.querySelector<HTMLElement>(`[data-agent-id="${streamReg.agent.id}"] [data-agent-channel]`)?.dataset
        .agentChannel,
    ).toBe('streaming');

    // 在其他页签期间停用长连接 Agent，再切回 agents 自动刷新后变为 offline
    target.querySelector<HTMLButtonElement>('[data-nav-section="bounties"]')!.click();
    await flush();
    await bountyApi.disableAgent(streamReg.agent.id);
    target.querySelector<HTMLButtonElement>('[data-nav-section="agents"]')!.click();
    await flush();
    expect(
      target.querySelector<HTMLElement>(`[data-agent-id="${streamReg.agent.id}"]`)?.dataset.agentStatus,
    ).toBe('disabled');
    expect(
      target.querySelector<HTMLElement>(`[data-agent-id="${streamReg.agent.id}"] [data-agent-channel]`)?.dataset
        .agentChannel,
    ).toBe('offline');

    // 重新启用并连回长连接；随后模拟手动刷新失败（503），不得继续误展「在线长连接」旧在线状态
    await bountyApi.enableAgent(streamReg.agent.id);
    await bountyApi.agentOpenStream(streamReg.api_key, () => {});
    target.querySelector<HTMLButtonElement>('[data-action="refresh-agents"]')!.click();
    await flush();
    expect(
      target.querySelector<HTMLElement>(`[data-agent-id="${streamReg.agent.id}"] [data-agent-channel]`)?.dataset
        .agentChannel,
    ).toBe('streaming');

    const listSpy = vi.spyOn(bountyApi, 'listAgents');
    listSpy.mockRejectedValueOnce(new Error('AGENTS_CHANNEL_503'));
    target.querySelector<HTMLButtonElement>('[data-action="refresh-agents"]')!.click();
    await flush();

    expect(target.querySelector('[data-status="agent-directory-load-error"]')?.textContent).toContain(
      'AGENTS_CHANNEL_503',
    );
    const channelAfterError = target.querySelector<HTMLElement>(
      `[data-agent-id="${streamReg.agent.id}"] [data-agent-channel]`,
    )!;
    expect(channelAfterError.dataset.agentChannel).toBe('unknown');
    expect(channelAfterError.textContent?.trim()).toBe('状态未知');

    // 点击「刷新」恢复；随后再次触发 503，验证切去悬赏板再切回名录会自动重试并在服务恢复后清空错误横幅与「状态未知」
    target.querySelector<HTMLButtonElement>('[data-action="refresh-agents"]')!.click();
    await flush();
    expect(target.querySelector('[data-status="agent-directory-load-error"]')).toBeNull();
    expect(
      target.querySelector<HTMLElement>(`[data-agent-id="${streamReg.agent.id}"] [data-agent-channel]`)?.dataset
        .agentChannel,
    ).toBe('streaming');

    listSpy.mockRejectedValueOnce(new Error('AGENTS_REENTER_503'));
    target.querySelector<HTMLButtonElement>('[data-action="refresh-agents"]')!.click();
    await flush();
    expect(target.querySelector('[data-status="agent-directory-load-error"]')?.textContent).toContain(
      'AGENTS_REENTER_503',
    );
    expect(
      target.querySelector<HTMLElement>(`[data-agent-id="${streamReg.agent.id}"] [data-agent-channel]`)?.dataset
        .agentChannel,
    ).toBe('unknown');

    const callsBeforeSwitch = listSpy.mock.calls.length;
    listSpy.mockRejectedValueOnce(new Error('AGENTS_REENTER_RETRY_503'));
    target.querySelector<HTMLButtonElement>('[data-nav-section="bounties"]')!.click();
    await flush();
    target.querySelector<HTMLButtonElement>('[data-nav-section="agents"]')!.click();
    await flush();
    expect(listSpy.mock.calls.length).toBe(callsBeforeSwitch + 1);
    expect(target.querySelector('[data-status="agent-directory-load-error"]')?.textContent).toContain(
      'AGENTS_REENTER_RETRY_503',
    );
    expect(
      target.querySelector<HTMLElement>(`[data-agent-id="${streamReg.agent.id}"] [data-agent-channel]`)?.dataset
        .agentChannel,
    ).toBe('unknown');

    const recoveredPushedAt = '2026-11-17T10:30:00.000Z';
    await bountyApi.simulateAgentPush(streamReg.agent.id, { pushedAt: recoveredPushedAt });
    target.querySelector<HTMLButtonElement>('[data-nav-section="bounties"]')!.click();
    await flush();
    target.querySelector<HTMLButtonElement>('[data-nav-section="agents"]')!.click();
    await flush();
    expect(listSpy.mock.calls.length).toBe(callsBeforeSwitch + 2);
    expect(target.querySelector('[data-status="agent-directory-load-error"]')).toBeNull();
    const recoveredChannel = target.querySelector<HTMLElement>(
      `[data-agent-id="${streamReg.agent.id}"] [data-agent-channel]`,
    )!;
    expect(recoveredChannel.dataset.agentChannel).toBe('streaming');
    expect(recoveredChannel.textContent?.trim()).toBe('在线长连接');
    expect(
      target
        .querySelector(`[data-agent-id="${streamReg.agent.id}"] [data-slot="agent-last-pushed"]`)
        ?.textContent?.trim(),
    ).toBe(formatTimestamp(recoveredPushedAt));

    // 存在 agentsLoadError 时切回名录触发的自动重试也受 agentListRequestSeq 乱序保护
    listSpy.mockRejectedValueOnce(new Error('AGENTS_RACE_503'));
    target.querySelector<HTMLButtonElement>('[data-action="refresh-agents"]')!.click();
    await flush();
    expect(target.querySelector('[data-status="agent-directory-load-error"]')?.textContent).toContain(
      'AGENTS_RACE_503',
    );

    let resolveStaleAutoRefresh!: (agents: Awaited<ReturnType<typeof originalListAgents>>) => void;
    listSpy.mockImplementationOnce(
      () =>
        new Promise(resolve => {
          resolveStaleAutoRefresh = resolve;
        }),
    );
    target.querySelector<HTMLButtonElement>('[data-nav-section="bounties"]')!.click();
    await flush();
    target.querySelector<HTMLButtonElement>('[data-nav-section="agents"]')!.click();
    await vi.waitFor(() => expect(resolveStaleAutoRefresh).toBeTypeOf('function'));

    // 较新的手动刷新先完成并恢复正常状态，随后迟到的自动刷新结果不得覆盖最新名录
    target.querySelector<HTMLButtonElement>('[data-action="refresh-agents"]')!.click();
    await flush();
    expect(target.querySelector('[data-status="agent-directory-load-error"]')).toBeNull();
    expect(
      target.querySelector<HTMLElement>(`[data-agent-id="${streamReg.agent.id}"] [data-agent-channel]`)?.dataset
        .agentChannel,
    ).toBe('streaming');

    resolveStaleAutoRefresh([]);
    await flush();
    expect(target.querySelector('[data-status="agent-directory-load-error"]')).toBeNull();
    expect(
      target.querySelector<HTMLElement>(`[data-agent-id="${streamReg.agent.id}"] [data-agent-channel]`)?.dataset
        .agentChannel,
    ).toBe('streaming');

    let resolveSlowRefresh!: (agents: Awaited<ReturnType<typeof originalListAgents>>) => void;
    let resolvePendingDisable!: (agent: Awaited<ReturnType<typeof originalDisableAgent>>) => void;
    let pendingDisableResult!: Promise<Awaited<ReturnType<typeof originalDisableAgent>>>;

    listSpy.mockImplementationOnce(filter => {
      const result = originalListAgents(filter);
      return new Promise(resolve => {
        void result.then(data => {
          resolveSlowRefresh = () => resolve(data);
        });
      });
    });
    vi.spyOn(bountyApi, 'disableAgent').mockImplementationOnce(id => {
      pendingDisableResult = originalDisableAgent(id);
      return new Promise(resolve => {
        resolvePendingDisable = resolve;
      });
    });

    const refreshBtn = target.querySelector<HTMLButtonElement>('[data-action="refresh-agents"]')!;
    const disableBtn = target.querySelector<HTMLButtonElement>(
      `[data-agent-id="${streamReg.agent.id}"] [data-action="disable-agent"]`,
    )!;
    refreshBtn.click();
    await vi.waitFor(() => expect(resolveSlowRefresh).toBeTypeOf('function'));
    expect(refreshBtn.disabled).toBe(true);
    expect(disableBtn.disabled).toBe(false);

    disableBtn.click();
    await vi.waitFor(() => expect(resolvePendingDisable).toBeTypeOf('function'));
    expect(disableBtn.disabled).toBe(true);

    // 慢刷新先完成不应提前解锁或干扰正在进行的 disableAgent，也不应让 busyAgentId 卡死
    resolveSlowRefresh([]);
    await flush();
    expect(refreshBtn.disabled).toBe(false);
    expect(disableBtn.disabled).toBe(true);

    resolvePendingDisable(await pendingDisableResult);
    await flush();
    const finalStreamCard = target.querySelector<HTMLElement>(`[data-agent-id="${streamReg.agent.id}"]`)!;
    expect(finalStreamCard.dataset.agentStatus).toBe('disabled');
    expect(finalStreamCard.querySelector<HTMLElement>('[data-agent-channel]')?.dataset.agentChannel).toBe('offline');
    const enableBtn = finalStreamCard.querySelector<HTMLButtonElement>('[data-action="enable-agent"]')!;
    expect(enableBtn).not.toBeNull();
    expect(enableBtn.disabled).toBe(false);

    // 存在 agentsLoadError 时切换服务连接（serviceEpoch 递增），必须清理错误横幅与旧名录状态
    listSpy.mockRejectedValueOnce(new Error('AGENTS_SWITCH_503'));
    refreshBtn.click();
    await flush();
    expect(target.querySelector('[data-status="agent-directory-load-error"]')?.textContent).toContain(
      'AGENTS_SWITCH_503',
    );

    vi.spyOn(globalThis, 'fetch').mockImplementation(async input => {
      const url = typeof input === 'string' ? input : input.toString();
      if (url.startsWith('https://service-b-agents.taskrail.test/api/v1/admin/onboarding')) {
        return new Response(
          JSON.stringify({ lan_open_registration: false, invite_tokens: [] }),
          { status: 200, headers: { 'Content-Type': 'application/json' } },
        );
      }
      if (
        url.startsWith('https://service-b-agents.taskrail.test/api/v1/projects') ||
        url.startsWith('https://service-b-agents.taskrail.test/api/v1/bounties') ||
        url.startsWith('https://service-b-agents.taskrail.test/api/v1/agents')
      ) {
        return new Response(JSON.stringify([]), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      return new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      });
    });

    target.querySelector<HTMLButtonElement>('[data-nav-section="settings"]')!.click();
    await flush();
    target.querySelector<HTMLButtonElement>('[data-service-mode="remote"]')!.click();
    await flush();
    const remoteUrlInput = target.querySelector<HTMLInputElement>('#remote-server-url-input')!;
    const remoteTokenInput = target.querySelector<HTMLInputElement>('#remote-admin-token-input')!;
    remoteUrlInput.value = 'https://service-b-agents.taskrail.test';
    remoteUrlInput.dispatchEvent(new Event('input', { bubbles: true }));
    remoteTokenInput.value = 'service-b-admin';
    remoteTokenInput.dispatchEvent(new Event('input', { bubbles: true }));
    target.querySelector<HTMLButtonElement>('[data-action="save-reconnect"]')!.click();
    await flush();

    target.querySelector<HTMLButtonElement>('[data-nav-section="agents"]')!.click();
    await flush();
    expect(target.querySelector('[data-status="agent-directory-load-error"]')).toBeNull();
    expect(target.querySelector('[data-slot="agent-empty-state"]')).not.toBeNull();
  });

  it('工作台发布与撤回悬赏联动精准事件分发、Unranked==C 段位匹配、Inbox 游标补拉与 Ack 推进', async () => {
    const { port } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'bounties' },
    });
    const bountyApi = createMemoryBountyClient({ lan_open_registration: true });
    const projAlpha = await bountyApi.createProject({ name: '分发主项目' });
    const projOther = await bountyApi.createProject({ name: '隔离项目' });

    const unrankedReg = await bountyApi.registerAgent({
      name: 'unranked-agent',
      subscribe_all_projects: false,
      project_ids: [projAlpha.id],
    });
    const tierBReg = await bountyApi.registerAgent({
      name: 'tier-b-agent',
      subscribe_all_projects: false,
      project_ids: [projAlpha.id],
    });
    await bountyApi.simulateAgentTier(tierBReg.agent.id, 'B', true);
    const otherReg = await bountyApi.registerAgent({
      name: 'other-proj-agent',
      subscribe_all_projects: false,
      project_ids: [projOther.id],
    });
    await bountyApi.simulateAgentTier(otherReg.agent.id, 'S', true);

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    async function publishFromComposer(options: {
      title: string;
      mode: 'any' | 'min_tier' | 'specific';
      minTier?: 'C' | 'B' | 'A' | 'S';
      assigneeAgentId?: string;
    }): Promise<void> {
      target.querySelector<HTMLButtonElement>('[data-action="open-composer"]')!.click();
      await flush();
      const composer = target.querySelector<HTMLElement>('[data-dialog="bounty-composer"]')!;
      const projectSelect = composer.querySelector<HTMLSelectElement>('#bounty-project-select')!;
      projectSelect.value = projAlpha.id;
      projectSelect.dispatchEvent(new Event('change', { bubbles: true }));
      const titleInput = composer.querySelector<HTMLInputElement>('#bounty-title-input')!;
      titleInput.value = options.title;
      titleInput.dispatchEvent(new Event('input', { bubbles: true }));
      composer.querySelector<HTMLButtonElement>(`[data-assignee-mode="${options.mode}"]`)!.click();
      await flush();
      if (options.mode === 'min_tier' && options.minTier) {
        const minTierSelect = composer.querySelector<HTMLSelectElement>('#bounty-min-tier-select')!;
        minTierSelect.value = options.minTier;
        minTierSelect.dispatchEvent(new Event('change', { bubbles: true }));
      } else if (options.mode === 'specific' && options.assigneeAgentId) {
        const agentInput = composer.querySelector<HTMLInputElement>('#bounty-assignee-agent-input')!;
        agentInput.value = options.assigneeAgentId;
        agentInput.dispatchEvent(new Event('input', { bubbles: true }));
      }
      await flush();
      composer.querySelector<HTMLButtonElement>('[data-action="publish-now"]')!.click();
      await flush();
      expect(target.querySelector('[data-dialog="bounty-composer"]')).toBeNull();
    }

    // 1. 发布 any、min_tier=B、min_tier=C 与 specific 任务
    await publishFromComposer({ title: '公开任意段位任务', mode: 'any' });
    await publishFromComposer({ title: 'B 级门槛任务', mode: 'min_tier', minTier: 'B' });
    await publishFromComposer({ title: 'C 级门槛任务', mode: 'min_tier', minTier: 'C' });
    await publishFromComposer({
      title: '定向指派任务',
      mode: 'specific',
      assigneeAgentId: otherReg.agent.id,
    });

    const unrankedInboxBeforeWithdraw = await bountyApi.agentListEvents(unrankedReg.api_key, { cursor: 0 });
    expect(unrankedInboxBeforeWithdraw.events.map(ev => ev.event_type)).toEqual([
      'task.published',
      'task.published',
    ]);
    expect(unrankedInboxBeforeWithdraw.events.map(ev => ev.data.title)).toEqual([
      '公开任意段位任务',
      'C 级门槛任务',
    ]);

    const tierBInboxBeforeWithdraw = await bountyApi.agentListEvents(tierBReg.api_key, { cursor: 0 });
    expect(tierBInboxBeforeWithdraw.events.map(ev => ev.data.title)).toEqual([
      '公开任意段位任务',
      'B 级门槛任务',
      'C 级门槛任务',
    ]);

    const otherInboxBeforeWithdraw = await bountyApi.agentListEvents(otherReg.api_key, { cursor: 0 });
    expect(otherInboxBeforeWithdraw.events).toHaveLength(1);
    expect(otherInboxBeforeWithdraw.events[0].event_type).toBe('task.assigned');
    expect(otherInboxBeforeWithdraw.events[0].data.title).toBe('定向指派任务');

    // 2. 通过工作台详情抽屉撤回「B 级门槛任务」（OPEN -> DRAFT）
    const allBounties = await bountyApi.listBounties({ include_drafts: true });
    const tierBBounty = allBounties.find(b => b.title === 'B 级门槛任务')!;
    expect(tierBBounty).toBeDefined();

    target
      .querySelector<HTMLButtonElement>(`[data-bounty-id="${tierBBounty.id}"] .bounty-card-button`)!
      .click();
    await flush();
    const drawer = target.querySelector<HTMLElement>('[data-dialog="bounty-detail"]')!;
    expect(drawer.dataset.bountyStatus).toBe('OPEN');
    drawer.querySelector<HTMLButtonElement>('[data-action="withdraw-bounty"]')!.click();
    await flush();
    expect(drawer.dataset.bountyStatus).toBe('DRAFT');

    // 仅曾收到该任务的 tierBReg 收到 task.withdrawn（且不携带 DRAFT 敏感正文）
    const unrankedInboxAfter = await bountyApi.agentListEvents(unrankedReg.api_key, { cursor: 0 });
    expect(unrankedInboxAfter.events).toHaveLength(2);
    expect(unrankedInboxAfter.events.some(ev => ev.event_type === 'task.withdrawn')).toBe(false);

    const otherInboxAfter = await bountyApi.agentListEvents(otherReg.api_key, { cursor: 0 });
    expect(otherInboxAfter.events).toHaveLength(1);

    const tierBInboxAfter = await bountyApi.agentListEvents(tierBReg.api_key, { cursor: 0 });
    expect(tierBInboxAfter.events).toHaveLength(4);
    const withdrawnEvent = tierBInboxAfter.events[3];
    expect(withdrawnEvent.event_type).toBe('task.withdrawn');
    expect(withdrawnEvent.bounty_id).toBe(tierBBounty.id);
    expect(withdrawnEvent.data).toEqual({
      bounty_id: tierBBounty.id,
      project_id: projAlpha.id,
    });

    // 3. 验证 Inbox cursor 分页补拉与 Ack 游标单调推进
    const page1 = await bountyApi.agentListEvents(unrankedReg.api_key, { cursor: 0, limit: 1 });
    expect(page1.events).toHaveLength(1);
    expect(page1.has_more).toBe(true);
    expect(page1.acked_event_id).toBe(0);

    const page2 = await bountyApi.agentListEvents(unrankedReg.api_key, {
      cursor: page1.cursor,
      limit: 10,
    });
    expect(page2.events).toHaveLength(1);
    expect(page2.has_more).toBe(false);
    expect(page2.events[0].event_id).toBeGreaterThan(page1.events[0].event_id);

    const ackLatest = await bountyApi.agentAckEvents(unrankedReg.api_key, page2.events[0].event_id);
    expect(ackLatest.acked_event_id).toBe(page2.events[0].event_id);

    // Ack 不回退
    const ackRegress = await bountyApi.agentAckEvents(unrankedReg.api_key, page1.events[0].event_id);
    expect(ackRegress.acked_event_id).toBe(page2.events[0].event_id);

    // 省略 cursor 时从 acked_event_id 之后拉取
    const afterAck = await bountyApi.agentListEvents(unrankedReg.api_key);
    expect(afterAck.events).toEqual([]);
    expect(afterAck.acked_event_id).toBe(page2.events[0].event_id);
  });

  it('支持在设置页按项目配置 Agent 发单策略（直接公开与先入草稿待审）并展示说明气泡与策略徽章', async () => {
    const { port } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'settings' },
    });
    const bountyApi = createMemoryBountyClient();

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    const pmPanel = target.querySelector<HTMLElement>('[data-section="project-management"]')!;
    expect(pmPanel).not.toBeNull();

    // 1. 新建「先入草稿待审」项目并测试 ? 帮助 Popover 与 Escape 关闭
    pmPanel.querySelector<HTMLButtonElement>('[data-action="new-project"]')!.click();
    await flush();

    const helpBtn = pmPanel.querySelector<HTMLButtonElement>(
      '[data-action="toggle-agent-policy-help"]',
    )!;
    const helpPopover = pmPanel.querySelector<HTMLElement>('#project-agent-policy-help')!;
    expect(helpBtn.getAttribute('aria-expanded')).toBe('false');
    expect(helpPopover.hidden).toBe(true);

    helpBtn.click();
    await flush();
    expect(helpBtn.getAttribute('aria-expanded')).toBe('true');
    expect(helpPopover.hidden).toBe(false);
    expect(helpPopover.textContent).toContain('直接公开');
    expect(helpPopover.textContent).toContain('先入草稿待审');

    const draftToggle = pmPanel.querySelector<HTMLButtonElement>('[data-agent-policy="draft"]')!;
    const openToggle = pmPanel.querySelector<HTMLButtonElement>('[data-agent-policy="open"]')!;
    expect(openToggle.getAttribute('aria-pressed')).toBe('true');
    expect(draftToggle.getAttribute('aria-pressed')).toBe('false');

    // 焦点移到策略开关上按 Escape 仍可关闭说明气泡
    draftToggle.focus();
    draftToggle.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await flush();
    expect(helpBtn.getAttribute('aria-expanded')).toBe('false');
    expect(helpPopover.hidden).toBe(true);

    const nameInput = pmPanel.querySelector<HTMLInputElement>('#project-form-name')!;
    nameInput.value = '先审后发项目';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    draftToggle.click();
    await flush();
    expect(draftToggle.getAttribute('aria-pressed')).toBe('true');

    pmPanel.querySelector<HTMLButtonElement>('[data-action="submit-project-form"]')!.click();
    await flush();

    const projectsAfterCreate = await bountyApi.listProjects(true);
    const createdDraftProject = projectsAfterCreate.find(p => p.name === '先审后发项目')!;
    expect(createdDraftProject).toBeDefined();
    expect(createdDraftProject.agent_draft_policy).toBe('draft');

    const row = pmPanel.querySelector<HTMLElement>(
      `[data-project-row="${createdDraftProject.id}"]`,
    )!;
    const policyTag = row.querySelector<HTMLElement>('[data-tag="agent-policy"]')!;
    expect(policyTag.dataset.policyValue).toBe('draft');
    expect(policyTag.textContent?.trim()).toBe('先入草稿待审');
    expect(row.textContent).not.toContain('·');

    // 2. 编辑项目切换为「直接公开」并校验更新回显
    row.querySelector<HTMLButtonElement>('[data-action="edit-project"]')!.click();
    await flush();
    expect(
      pmPanel
        .querySelector<HTMLButtonElement>('[data-agent-policy="draft"]')
        ?.getAttribute('aria-pressed'),
    ).toBe('true');

    pmPanel.querySelector<HTMLButtonElement>('[data-agent-policy="open"]')!.click();
    await flush();
    pmPanel.querySelector<HTMLButtonElement>('[data-action="submit-project-form"]')!.click();
    await flush();

    const updatedProject = await bountyApi.getProject(createdDraftProject.id);
    expect(updatedProject.agent_draft_policy).toBe('open');
    const updatedTag = pmPanel.querySelector<HTMLElement>(
      `[data-project-row="${createdDraftProject.id}"] [data-tag="agent-policy"]`,
    )!;
    expect(updatedTag.dataset.policyValue).toBe('open');
    expect(updatedTag.textContent?.trim()).toBe('直接公开');
  });

  it('覆盖两种项目发单策略下 Agent 自动发单、待我处理聚合刷新、发布者身份与人工审阅转正或丢弃全流程', async () => {
    const { port } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'bounties' },
    });
    const bountyApi = createMemoryBountyClient({ lan_open_registration: true });
    const openProject = await bountyApi.createProject({
      name: '直接公开项目',
      agent_draft_policy: 'open',
    });
    const draftProject = await bountyApi.createProject({
      name: '草稿待审项目',
      agent_draft_policy: 'draft',
    });

    const publisherReg = await bountyApi.registerAgent({
      name: '巡检机器人',
      subscribe_all_projects: false,
      project_ids: [openProject.id, draftProject.id],
    });
    const workerReg = await bountyApi.registerAgent({
      name: '执行助手',
      subscribe_all_projects: false,
      project_ids: [openProject.id, draftProject.id],
    });
    const outsiderReg = await bountyApi.registerAgent({
      name: '旁观者',
      subscribe_all_projects: false,
      project_ids: [],
    });

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    // 先由管理员创建一条人工悬赏，作为对比基线
    target.querySelector<HTMLButtonElement>('[data-action="open-composer"]')!.click();
    await flush();
    const composer0 = target.querySelector<HTMLElement>('[data-dialog="bounty-composer"]')!;
    const title0 = composer0.querySelector<HTMLInputElement>('#bounty-title-input')!;
    title0.value = '人工基线悬赏';
    title0.dispatchEvent(new Event('input', { bubbles: true }));
    composer0.querySelector<HTMLButtonElement>('[data-action="publish-now"]')!.click();
    await flush();

    const humanBounty = (await bountyApi.listBounties()).find(b => b.title === '人工基线悬赏')!;
    expect(humanBounty).toBeDefined();
    const humanCard = target.querySelector<HTMLElement>(`[data-bounty-id="${humanBounty.id}"]`)!;
    expect(humanCard.querySelector('[data-publisher-kind="agent"]')).toBeNull();

    // 1. 「直接公开」策略：Agent 提交立即成为 OPEN、记录发布者身份、推送事件并在悬赏板展示
    const openBounty = await bountyApi.agentCreateBounty(publisherReg.api_key, {
      project_id: openProject.id,
      title: '巡检发现的公开缺陷',
      description: '复现步骤：\n\n- 检查日志',
      acceptance_criteria: '- [ ] 补充回归测试',
      difficulty: 'A',
      assignee_mode: 'any',
      require_arbitration: true,
    });
    expect(openBounty.status).toBe('OPEN');
    expect(openBounty.created_by).toBe(`agent:${publisherReg.agent.id}`);
    expect(openBounty.created_by_agent_id).toBe(publisherReg.agent.id);
    expect(openBounty.created_by_agent_name).toBe('巡检机器人');

    // worker 收到人类基线 + Agent 直接公开共 2 条事件；outsider 为 0
    const workerEventsAfterOpen = await bountyApi.agentListEvents(workerReg.api_key, { cursor: 0 });
    expect(workerEventsAfterOpen.events.map(ev => ev.data.title)).toEqual([
      '人工基线悬赏',
      '巡检发现的公开缺陷',
    ]);
    expect(
      (await bountyApi.agentListEvents(outsiderReg.api_key, { cursor: 0 })).events,
    ).toHaveLength(0);

    // 切换页面再回到悬赏板触发自动刷新，验证卡片与详情抽屉均展示发布者 Agent 身份与时间线
    target.querySelector<HTMLButtonElement>('[data-nav-section="agents"]')!.click();
    await flush();
    target.querySelector<HTMLButtonElement>('[data-nav-section="bounties"]')!.click();
    await flush();

    const openCard = target.querySelector<HTMLElement>(`[data-bounty-id="${openBounty.id}"]`)!;
    expect(openCard).not.toBeNull();
    expect(openCard.dataset.bountyStatus).toBe('OPEN');
    expect(openCard.textContent).not.toContain('·');
    const openPublisherBadge = openCard.querySelector<HTMLElement>(
      '[data-publisher-kind="agent"]',
    )!;
    expect(openPublisherBadge.textContent?.trim()).toBe('Agent 提交: 巡检机器人');

    openCard.querySelector<HTMLButtonElement>('.bounty-card-button')!.click();
    await flush();
    const openDrawer = target.querySelector<HTMLElement>('[data-dialog="bounty-detail"]')!;
    expect(openDrawer.querySelector('[data-slot="detail-publisher"]')?.textContent?.trim()).toBe(
      'Agent 提交: 巡检机器人',
    );
    expect(
      openDrawer.querySelector('.detail-timeline-item[data-event-type="created"] .detail-timeline-actor')
        ?.textContent?.trim(),
    ).toBe('Agent: 巡检机器人');
    expect(openDrawer.textContent).not.toContain('api_key');
    expect(openDrawer.textContent).not.toContain('webhook_secret');
    expect(openDrawer.textContent?.toLowerCase()).not.toContain('mmr');
    openDrawer.querySelector<HTMLButtonElement>('[aria-label="关闭对话框"]')!.click();
    await flush();

    // 2. 「先入草稿待审」策略：Agent 提交落入 DRAFT，不推送任何事件且对所有 Agent 隔离
    const draftEditPublish = await bountyApi.agentCreateBounty(publisherReg.api_key, {
      project_id: draftProject.id,
      title: '待审草稿一：编辑后转正',
      description: '需要人工补充验收要求',
      acceptance_criteria: '- [ ] 初步排查',
      difficulty: 'B',
      assignee_mode: 'any',
      require_arbitration: false,
    });
    const draftDirectPublish = await bountyApi.agentCreateBounty(publisherReg.api_key, {
      project_id: draftProject.id,
      title: '待审草稿二：一键确认发布',
      difficulty: 'C',
      assignee_mode: 'any',
    });
    const draftDiscard = await bountyApi.agentCreateBounty(publisherReg.api_key, {
      project_id: draftProject.id,
      title: '待审草稿三：重复问题丢弃',
      difficulty: 'C',
      assignee_mode: 'any',
    });

    for (const draft of [draftEditPublish, draftDirectPublish, draftDiscard]) {
      expect(draft.status).toBe('DRAFT');
      expect(draft.published_at).toBeNull();
      expect(draft.created_by_agent_id).toBe(publisherReg.agent.id);
    }

    // 所有 Agent（包含发布者自身）均不可读 DRAFT，且事件数为 0 新增
    expect(
      (await bountyApi.agentListBounties(publisherReg.api_key, { include_drafts: true })).some(
        b => b.status === 'DRAFT',
      ),
    ).toBe(false);
    await expect(
      bountyApi.agentGetBountyDetail(publisherReg.api_key, draftEditPublish.id),
    ).rejects.toMatchObject({ code: 'BOUNTY_NOT_FOUND', status: 404 });
    expect(
      (await bountyApi.agentListEvents(workerReg.api_key, { cursor: 0 })).events,
    ).toHaveLength(2);

    // 切入「待我处理」自动刷新聚合展示全部 3 条 Agent 草稿
    target.querySelector<HTMLButtonElement>('[data-nav-section="inbox"]')!.click();
    await flush();
    const inboxView = target.querySelector<HTMLElement>('[data-view="inbox"]')!;
    expect(inboxView).not.toBeNull();

    // 在待我处理视图内新提交一条草稿后点击手动「刷新」按钮也能立即拉取
    const draftLate = await bountyApi.agentCreateBounty(publisherReg.api_key, {
      project_id: draftProject.id,
      title: '待审草稿四：刷新拉取',
      difficulty: 'C',
      assignee_mode: 'any',
    });
    inboxView.querySelector<HTMLButtonElement>('[data-action="refresh-inbox"]')!.click();
    await flush();

    inboxView.querySelector<HTMLButtonElement>('[data-inbox-filter="DRAFT"]')!.click();
    await flush();

    for (const draft of [draftEditPublish, draftDirectPublish, draftDiscard, draftLate]) {
      const card = inboxView.querySelector<HTMLElement>(`[data-bounty-id="${draft.id}"]`)!;
      expect(card).not.toBeNull();
      expect(card.dataset.bountyStatus).toBe('DRAFT');
      expect(card.querySelector('[data-publisher-kind="agent"]')?.textContent?.trim()).toBe(
        'Agent 提交: 巡检机器人',
      );
    }

    // 3a. 审阅草稿一 -> 点击「编辑草稿」修改并发布为 OPEN，保留发布者身份且触发分发
    inboxView
      .querySelector<HTMLButtonElement>(`[data-bounty-id="${draftEditPublish.id}"] .bounty-card-button`)!
      .click();
    await flush();
    const detail1 = target.querySelector<HTMLElement>('[data-dialog="bounty-detail"]')!;
    detail1.querySelector<HTMLButtonElement>('[data-action="edit-draft"]')!.click();
    await flush();

    const editComposer = target.querySelector<HTMLElement>('[data-dialog="bounty-composer"]')!;
    const editTitleInput = editComposer.querySelector<HTMLInputElement>('#bounty-title-input')!;
    editTitleInput.value = '人工核验转正悬赏';
    editTitleInput.dispatchEvent(new Event('input', { bubbles: true }));
    editComposer.querySelector<HTMLButtonElement>('[data-action="publish-now"]')!.click();
    await flush();

    const publishedDetail1 = await bountyApi.getBountyDetail(draftEditPublish.id);
    expect(publishedDetail1.bounty.status).toBe('OPEN');
    expect(publishedDetail1.bounty.title).toBe('人工核验转正悬赏');
    expect(publishedDetail1.bounty.created_by_agent_id).toBe(publisherReg.agent.id);
    expect(publishedDetail1.bounty.created_by_agent_name).toBe('巡检机器人');
    expect(publishedDetail1.events.map(ev => ev.actor)).toEqual([
      `agent:${publisherReg.agent.id}`,
      'human',
      'human',
    ]);
    expect(inboxView.querySelector(`[data-bounty-id="${draftEditPublish.id}"]`)).toBeNull();

    // 3b. 审阅草稿二 -> 在详情抽屉一键「立即发布」转正为 OPEN
    inboxView
      .querySelector<HTMLButtonElement>(`[data-bounty-id="${draftDirectPublish.id}"] .bounty-card-button`)!
      .click();
    await flush();
    const detail2 = target.querySelector<HTMLElement>('[data-dialog="bounty-detail"]')!;
    detail2.querySelector<HTMLButtonElement>('[data-action="publish-bounty"]')!.click();
    await flush();
    expect(detail2.dataset.bountyStatus).toBe('OPEN');
    expect(detail2.querySelector('[data-slot="detail-publisher"]')?.textContent?.trim()).toBe(
      'Agent 提交: 巡检机器人',
    );
    detail2.querySelector<HTMLButtonElement>('[aria-label="关闭对话框"]')!.click();
    await flush();
    expect(inboxView.querySelector(`[data-bounty-id="${draftDirectPublish.id}"]`)).toBeNull();

    // 3c. 审阅草稿三 -> 在详情抽屉「丢弃草稿」转为 CANCELLED，绝不向任何 Agent 推送且继续保持对所有 Agent 不可见
    inboxView
      .querySelector<HTMLButtonElement>(`[data-bounty-id="${draftDiscard.id}"] .bounty-card-button`)!
      .click();
    await flush();
    const detail3 = target.querySelector<HTMLElement>('[data-dialog="bounty-detail"]')!;
    detail3.querySelector<HTMLButtonElement>('[data-action="cancel-bounty"]')!.click();
    await flush();
    expect(detail3.dataset.bountyStatus).toBe('CANCELLED');
    detail3.querySelector<HTMLButtonElement>('[aria-label="关闭对话框"]')!.click();
    await flush();
    expect(inboxView.querySelector(`[data-bounty-id="${draftDiscard.id}"]`)).toBeNull();

    for (const agentKey of [publisherReg.api_key, workerReg.api_key]) {
      const visibleAfterDiscard = await bountyApi.agentListBounties(agentKey, {
        status: 'CANCELLED',
        include_drafts: true,
      });
      expect(visibleAfterDiscard.some(b => b.id === draftDiscard.id)).toBe(false);
      await expect(bountyApi.agentGetBountyDetail(agentKey, draftDiscard.id)).rejects.toMatchObject({
        code: 'BOUNTY_NOT_FOUND',
        status: 404,
      });
    }

    const workerEventsFinal = await bountyApi.agentListEvents(workerReg.api_key, { cursor: 0 });
    expect(workerEventsFinal.events.map(ev => ev.data.title)).toEqual([
      '人工基线悬赏',
      '巡检发现的公开缺陷',
      '人工核验转正悬赏',
      '待审草稿二：一键确认发布',
    ]);
    expect(workerEventsFinal.events.some(ev => ev.bounty_id === draftDiscard.id)).toBe(false);
    expect(workerEventsFinal.events.some(ev => ev.bounty_id === draftLate.id)).toBe(false);

    // 切换策略回 open 不隐式公开既有未处理的 draftLate
    await bountyApi.updateProject(draftProject.id, { agent_draft_policy: 'open' });
    expect((await bountyApi.getBountyDetail(draftLate.id)).bounty.status).toBe('DRAFT');
  });

  it('实时展示执行者段位、租约倒计时与完成备注，支持管理员强制释放租约、定向婉拒回退及异常次数展示', async () => {
    vi.useFakeTimers({
      toFake: ['Date', 'setInterval', 'clearInterval'],
    });
    const baseTime = new Date('2026-11-17T10:00:00.000Z');
    vi.setSystemTime(baseTime);

    const { port } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'bounties' },
    });
    const bountyApi = createMemoryBountyClient({ lan_open_registration: true });
    const project = await bountyApi.createProject({
      name: '租约交付项目',
      agent_draft_policy: 'open',
    });

    const alpha = await bountyApi.registerAgent({ name: '执行工兵 Alpha' });
    const beta = await bountyApi.registerAgent({ name: '执行工兵 Beta' });
    await bountyApi.simulateAgentTier(alpha.agent.id, 'A');
    await bountyApi.simulateAgentTier(beta.agent.id, 'B');

    const leaseTask = await bountyApi.createBounty({
      project_id: project.id,
      title: '并发抢单与租约倒计时任务',
      description: '验证卡片与详情抽屉实时展示执行者与倒计时',
      acceptance_criteria: '- [ ] 通过测试',
      difficulty: 'B',
      assignee_mode: 'min_tier',
      assignee_min_tier: 'B',
      require_arbitration: false,
      status: 'OPEN',
    });

    const specificTask = await bountyApi.createBounty({
      project_id: project.id,
      title: '定向指派与无仲裁直接交付任务',
      description: '先由定向 Agent 婉拒，再由接单者完成交付',
      acceptance_criteria: '- [ ] 交付完成说明',
      difficulty: 'C',
      assignee_mode: 'specific',
      assignee_agent_id: alpha.agent.id,
      require_arbitration: false,
      status: 'OPEN',
    });

    const claimed = await bountyApi.agentClaimBounty(alpha.api_key, leaseTask.id);
    expect(claimed.lease_id).toBe(1);

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    // 1. 悬赏板卡片实时展示执行者名称+段位与租约剩余时间倒计时（无 · 分隔符）
    const activeCard = target.querySelector<HTMLElement>(`[data-bounty-id="${leaseTask.id}"]`)!;
    expect(activeCard).not.toBeNull();
    expect(activeCard.textContent).not.toContain('·');
    expect(activeCard.querySelector('[data-slot="card-executor"]')?.textContent?.trim()).toBe(
      '执行者: 执行工兵 Alpha（A 级）',
    );
    const cardCountdown = activeCard.querySelector<HTMLElement>(
      '[data-slot="card-lease-countdown"]',
    )!;
    expect(cardCountdown.dataset.leaseState).toBe('active');
    expect(cardCountdown.textContent?.trim()).toBe('租约剩余 15:00');

    // 推进 2 秒，验证卡片倒计时实时跳动
    vi.advanceTimersByTime(2000);
    await flush();
    expect(
      activeCard.querySelector('[data-slot="card-lease-countdown"]')?.textContent?.trim(),
    ).toBe('租约剩余 14:58');

    // 打开详情抽屉，验证执行者段位、倒计时（含 <=3 分钟临期告警态）及强制释放按钮
    activeCard.querySelector<HTMLButtonElement>('.bounty-card-button')!.click();
    await flush();

    const drawer = target.querySelector<HTMLElement>('[data-dialog="bounty-detail"]')!;
    expect(drawer.querySelector('[data-slot="detail-executor"]')?.textContent?.trim()).toBe(
      '执行者: 执行工兵 Alpha（A 级）',
    );
    expect(drawer.querySelector('[data-slot="detail-lease-countdown"]')?.textContent?.trim()).toBe(
      '租约剩余 14:58',
    );
    expect(drawer.querySelector('[data-action="release-lease"]')).not.toBeNull();

    // 推进至剩余 2 分 30 秒，验证卡片与抽屉均切为 expiring 态
    vi.advanceTimersByTime(12 * 60 * 1000 + 28 * 1000);
    await flush();
    const drawerCountdown = drawer.querySelector<HTMLElement>(
      '[data-slot="detail-lease-countdown"]',
    )!;
    expect(drawerCountdown.dataset.leaseState).toBe('expiring');
    expect(drawerCountdown.textContent?.trim()).toBe('租约剩余 02:30');
    expect(
      activeCard.querySelector<HTMLElement>('[data-slot="card-lease-countdown"]')?.dataset
        .leaseState,
    ).toBe('expiring');

    // 2. 管理员点击「强制释放租约」，任务重置回 OPEN、清理执行者与倒计时并写入时间线
    drawer.querySelector<HTMLButtonElement>('[data-action="release-lease"]')!.click();
    await flush();

    expect(drawer.dataset.bountyStatus).toBe('OPEN');
    expect(drawer.querySelector('[data-slot="detail-executor"]')).toBeNull();
    expect(drawer.querySelector('[data-slot="detail-lease-countdown"]')).toBeNull();
    expect(drawer.querySelector('[data-action="release-lease"]')).toBeNull();
    expect(
      drawer.querySelector(
        '.detail-timeline-item[data-event-type="lease_released"] .detail-timeline-action',
      )?.textContent?.trim(),
    ).toBe('释放租约');
    expect(
      drawer.querySelector(
        '.detail-timeline-item[data-event-type="lease_released"] .detail-timeline-actor',
      )?.textContent?.trim(),
    ).toBe('管理员');

    drawer.querySelector<HTMLButtonElement>('[aria-label="关闭对话框"]')!.click();
    await flush();

    const reopenedCard = target.querySelector<HTMLElement>(`[data-bounty-id="${leaseTask.id}"]`)!;
    expect(reopenedCard.dataset.bountyStatus).toBe('OPEN');
    expect(reopenedCard.querySelector('[data-slot="card-executor"]')).toBeNull();
    expect(reopenedCard.querySelector('[data-slot="card-lease-countdown"]')).toBeNull();

    // 3. 定向指派任务：详情抽屉展示响应倒计时，婉拒后回退公开并由 Beta 接单直接完成交付
    const specificCard = target.querySelector<HTMLElement>(
      `[data-bounty-id="${specificTask.id}"]`,
    )!;
    specificCard.querySelector<HTMLButtonElement>('.bounty-card-button')!.click();
    await flush();
    const specificDrawer = target.querySelector<HTMLElement>('[data-dialog="bounty-detail"]')!;
    expect(
      specificDrawer.querySelector('[data-slot="detail-assignment-countdown"]')?.textContent?.trim(),
    ).toContain('指派响应剩余');
    specificDrawer.querySelector<HTMLButtonElement>('[aria-label="关闭对话框"]')!.click();
    await flush();

    await bountyApi.agentDeclineBounty(alpha.api_key, specificTask.id, {
      reason: '当前队列繁忙',
    });
    const betaClaimed = await bountyApi.agentClaimBounty(beta.api_key, specificTask.id);
    await bountyApi.agentCompleteBounty(beta.api_key, specificTask.id, {
      lease_id: betaClaimed.lease_id,
      completion_note: '  已修复并补齐端到端测试  ',
      completion_cost: 1.75,
    });

    // 4. 制造一次超时回收与一次主动弃单，验证 Agent 名录展示异常计数且工作台刷新后展示交付说明
    const timeoutClaim = await bountyApi.agentClaimBounty(beta.api_key, leaseTask.id);
    await bountyApi.sweepLeases(Date.parse(timeoutClaim.lease_expires_at!) + 1000);
    const abandonClaim = await bountyApi.agentClaimBounty(beta.api_key, leaseTask.id);
    await bountyApi.agentAbandonBounty(beta.api_key, leaseTask.id, {
      lease_id: abandonClaim.lease_id,
      reason: '需切换分支',
    });

    target.querySelector<HTMLButtonElement>('[data-nav-section="agents"]')!.click();
    await flush();
    const betaCard = target.querySelector<HTMLElement>(`[data-agent-id="${beta.agent.id}"]`)!;
    expect(
      betaCard.querySelector('[data-slot="agent-lease-incidents"]')?.textContent?.trim(),
    ).toContain('超时 1 次 弃单 1 次');
    expect(betaCard.textContent).not.toContain('·');

    // 切回悬赏板验证已完成卡片与详情抽屉的交付说明、资源成本、保留的执行者段位及系统调度时间线
    target.querySelector<HTMLButtonElement>('[data-nav-section="bounties"]')!.click();
    await flush();

    const completedCard = target.querySelector<HTMLElement>(
      `[data-bounty-id="${specificTask.id}"]`,
    )!;
    expect(completedCard.dataset.bountyStatus).toBe('COMPLETED');
    expect(completedCard.querySelector('[data-slot="card-executor"]')?.textContent?.trim()).toBe(
      '执行者: 执行工兵 Beta（B 级）',
    );
    expect(completedCard.querySelector('[data-slot="card-lease-countdown"]')).toBeNull();
    expect(
      completedCard.querySelector('[data-slot="card-completion-note"]')?.textContent?.trim(),
    ).toBe('已修复并补齐端到端测试');
    expect(
      completedCard.querySelector('[data-slot="card-completion-cost"]')?.textContent?.trim(),
    ).toBe('成本 1.75');

    completedCard.querySelector<HTMLButtonElement>('.bounty-card-button')!.click();
    await flush();
    const completedDrawer = target.querySelector<HTMLElement>('[data-dialog="bounty-detail"]')!;
    expect(completedDrawer.querySelector('[data-action="release-lease"]')).toBeNull();
    expect(
      completedDrawer.querySelector('[data-slot="detail-executor"]')?.textContent?.trim(),
    ).toBe('执行者: 执行工兵 Beta（B 级）');
    expect(
      completedDrawer.querySelector('[data-slot="detail-completion-note"]')?.textContent?.trim(),
    ).toBe('已修复并补齐端到端测试');
    expect(
      completedDrawer.querySelector('[data-slot="detail-completion-cost"]')?.textContent?.trim(),
    ).toBe('成本 1.75');
    expect(
      completedDrawer.querySelector(
        '.detail-timeline-item[data-event-type="declined"] .detail-timeline-action',
      )?.textContent?.trim(),
    ).toBe('婉拒指派');
    expect(
      completedDrawer.querySelector(
        '.detail-timeline-item[data-event-type="completed"] .detail-timeline-action',
      )?.textContent?.trim(),
    ).toBe('完成交付');
    expect(completedDrawer.textContent).not.toContain('·');
    expect(completedDrawer.textContent?.toLowerCase()).not.toContain('mmr');
  });

  it('无需切页或点击，外部 Agent 接单、续租与直接完成时自动实时同步卡片、已打开详情抽屉与名录', async () => {
    vi.useFakeTimers({
      toFake: ['Date', 'setInterval', 'clearInterval'],
    });
    const baseTime = new Date('2026-11-17T11:00:00.000Z');
    vi.setSystemTime(baseTime);

    const { port } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'bounties' },
    });
    const bountyApi = createMemoryBountyClient({ lan_open_registration: true });
    const project = await bountyApi.createProject({
      name: '实时同步项目',
      agent_draft_policy: 'open',
    });

    const alpha = await bountyApi.registerAgent({ name: '实时工兵 Alpha' });
    await bountyApi.simulateAgentTier(alpha.agent.id, 'A');

    const syncTask = await bountyApi.createBounty({
      project_id: project.id,
      title: '外部 Agent 全流程实时同步任务',
      description: '不切页验证接单、续租与直接完成自动同步',
      acceptance_criteria: '- [ ] 自动同步状态与备注',
      difficulty: 'A',
      assignee_mode: 'any',
      require_arbitration: false,
      status: 'OPEN',
    });
    const directoryTask = await bountyApi.createBounty({
      project_id: project.id,
      title: '名录负载与异常实时同步任务',
      difficulty: 'B',
      assignee_mode: 'any',
      require_arbitration: false,
      status: 'OPEN',
    });

    instance = mount(App, { target, props: { files: port, bountyApi } });
    await flush();

    // 先打开任务详情抽屉，此时卡片与详情均为 OPEN，后续全程不切页、不点击重新打开
    const card = target.querySelector<HTMLElement>(`[data-bounty-id="${syncTask.id}"]`)!;
    expect(card.dataset.bountyStatus).toBe('OPEN');
    expect(card.querySelector('[data-slot="card-executor"]')).toBeNull();
    expect(card.querySelector('[data-slot="card-lease-countdown"]')).toBeNull();

    card.querySelector<HTMLButtonElement>('.bounty-card-button')!.click();
    await flush();

    const drawer = target.querySelector<HTMLElement>('[data-dialog="bounty-detail"]')!;
    expect(drawer).not.toBeNull();
    expect(drawer.dataset.bountyStatus).toBe('OPEN');
    expect(drawer.querySelector('[data-slot="detail-executor"]')).toBeNull();
    expect(drawer.querySelector('[data-slot="detail-lease-countdown"]')).toBeNull();

    // 1. 外部 Agent 接单 -> 推进时间后卡片与已打开详情抽屉自动变为 IN_PROGRESS 并显示执行者段位与倒计时
    const claimed = await bountyApi.agentClaimBounty(alpha.api_key, syncTask.id);
    expect(claimed.lease_id).toBe(1);

    await vi.advanceTimersByTimeAsync(2000);
    await flush();

    expect(card.dataset.bountyStatus).toBe('IN_PROGRESS');
    expect(card.querySelector('[data-slot="card-executor"]')?.textContent?.trim()).toBe(
      '执行者: 实时工兵 Alpha（A 级）',
    );
    expect(card.querySelector('[data-slot="card-lease-countdown"]')?.textContent?.trim()).toBe(
      '租约剩余 14:58',
    );

    expect(drawer.dataset.bountyStatus).toBe('IN_PROGRESS');
    expect(drawer.querySelector('[data-slot="detail-executor"]')?.textContent?.trim()).toBe(
      '执行者: 实时工兵 Alpha（A 级）',
    );
    expect(drawer.querySelector('[data-slot="detail-lease-countdown"]')?.textContent?.trim()).toBe(
      '租约剩余 14:58',
    );
    expect(drawer.querySelector('[data-action="release-lease"]')).not.toBeNull();
    expect(
      drawer.querySelector('.detail-timeline-item[data-event-type="claimed"]'),
    ).not.toBeNull();

    // 2. 推进 5 分钟后外部 Agent 心跳续租 -> 自动刷新延长后的 lease_expires_at 倒计时
    await vi.advanceTimersByTimeAsync(5 * 60 * 1000);
    await flush();
    expect(card.querySelector('[data-slot="card-lease-countdown"]')?.textContent?.trim()).toBe(
      '租约剩余 09:58',
    );
    expect(drawer.querySelector('[data-slot="detail-lease-countdown"]')?.textContent?.trim()).toBe(
      '租约剩余 09:58',
    );

    await bountyApi.agentHeartbeatBounty(alpha.api_key, syncTask.id, {
      lease_id: claimed.lease_id,
    });
    await vi.advanceTimersByTimeAsync(2000);
    await flush();

    expect(card.querySelector('[data-slot="card-lease-countdown"]')?.textContent?.trim()).toBe(
      '租约剩余 14:58',
    );
    expect(drawer.querySelector('[data-slot="detail-lease-countdown"]')?.textContent?.trim()).toBe(
      '租约剩余 14:58',
    );

    // 3. 外部 Agent 直接完成并附带 completion_note -> 自动刷新为 COMPLETED 并展示完成说明备注
    await bountyApi.agentCompleteBounty(alpha.api_key, syncTask.id, {
      lease_id: claimed.lease_id,
      completion_note: '  外部自动同步完成备注  ',
      completion_cost: 2.5,
    });
    await vi.advanceTimersByTimeAsync(2000);
    await flush();

    expect(card.dataset.bountyStatus).toBe('COMPLETED');
    expect(card.querySelector('[data-slot="card-executor"]')?.textContent?.trim()).toBe(
      '执行者: 实时工兵 Alpha（A 级）',
    );
    expect(card.querySelector('[data-slot="card-lease-countdown"]')).toBeNull();
    expect(card.querySelector('[data-slot="card-completion-note"]')?.textContent?.trim()).toBe(
      '外部自动同步完成备注',
    );
    expect(card.querySelector('[data-slot="card-completion-cost"]')?.textContent?.trim()).toBe(
      '成本 2.5',
    );

    expect(drawer.dataset.bountyStatus).toBe('COMPLETED');
    expect(drawer.querySelector('[data-action="release-lease"]')).toBeNull();
    expect(drawer.querySelector('[data-slot="detail-executor"]')?.textContent?.trim()).toBe(
      '执行者: 实时工兵 Alpha（A 级）',
    );
    expect(drawer.querySelector('[data-slot="detail-lease-countdown"]')).toBeNull();
    expect(drawer.querySelector('[data-slot="detail-completion-note"]')?.textContent?.trim()).toBe(
      '外部自动同步完成备注',
    );
    expect(drawer.querySelector('[data-slot="detail-completion-cost"]')?.textContent?.trim()).toBe(
      '成本 2.5',
    );
    expect(
      drawer.querySelector('.detail-timeline-item[data-event-type="completed"]'),
    ).not.toBeNull();

    // 4. 在 Agent 名录视图下同样无需切页或点击，自动同步外部 Agent 的活跃租约数与异常计数
    unmount(instance);
    instance = null;
    const { port: agentsPort } = createMemoryPort({
      preferences: { ...emptyAppConfig().preferences, workspaceSection: 'agents' },
    });
    instance = mount(App, { target, props: { files: agentsPort, bountyApi } });
    await flush();

    const alphaCard = target.querySelector<HTMLElement>(`[data-agent-id="${alpha.agent.id}"]`)!;
    expect(alphaCard).not.toBeNull();
    expect(alphaCard.querySelector<HTMLElement>('[data-agent-activity]')?.dataset.agentActivity).toBe(
      'idle',
    );
    expect(alphaCard.querySelector('[data-slot="agent-lease-incidents"]')).toBeNull();

    const dirClaim = await bountyApi.agentClaimBounty(alpha.api_key, directoryTask.id);
    await vi.advanceTimersByTimeAsync(2000);
    await flush();

    expect(alphaCard.querySelector<HTMLElement>('[data-agent-activity]')?.dataset.agentActivity).toBe(
      'busy',
    );
    expect(alphaCard.querySelector('[data-agent-activity]')?.textContent?.trim()).toBe(
      '1 个进行中租约',
    );

    await bountyApi.agentAbandonBounty(alpha.api_key, directoryTask.id, {
      lease_id: dirClaim.lease_id,
      reason: '外部释放',
    });
    await vi.advanceTimersByTimeAsync(2000);
    await flush();

    expect(alphaCard.querySelector<HTMLElement>('[data-agent-activity]')?.dataset.agentActivity).toBe(
      'idle',
    );
    expect(
      alphaCard.querySelector('[data-slot="agent-lease-incidents"]')?.textContent?.trim(),
    ).toContain('超时 0 次 弃单 1 次');
  });
});
