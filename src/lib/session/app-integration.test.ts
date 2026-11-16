/**
 * 文件职责：验证悬赏板工作台外壳（App.svelte）的导航切换、主题与窗口材质、语言切换、项目图标选择与配置持久化。
 */
import { mount, tick, unmount } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App from '../../App.svelte';
import { createMemoryBountyClient } from '../api/bounty-client';
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
});
