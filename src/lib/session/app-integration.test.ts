/**
 * 文件职责：验证悬赏板工作台外壳（App.svelte）的导航切换、主题与窗口材质、语言切换、项目图标选择与配置持久化。
 */
import { mount, tick, unmount } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App from '../../App.svelte';
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
});
