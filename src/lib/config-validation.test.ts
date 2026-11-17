import { describe, expect, it } from 'vitest';
import { defaultPreferences, defaultThemeId } from './config-defaults';
import { validateAppConfig } from './config-validation';

const validProject = { id: 'p1', name: '工作', path: 'D:/work.md' };

describe('应用配置校验', () => {
  it('加载旧别名后仅输出规范字段并可直接保存重载', () => {
    const config = validateAppConfig({ projects: [], preferences: {
      language: 'en', lineWidth: 680, sidebarCollapsed: true, paragraphIndent: true,
    } });
    expect(config?.preferences).toMatchObject({
      locale: 'en', contentWidth: 680, sidebarOpen: false, paragraphIndent: 2,
    });
    for (const alias of ['language', 'lineWidth', 'sidebarCollapsed']) {
      expect(config?.preferences).not.toHaveProperty(alias);
    }
    expect(validateAppConfig(JSON.parse(JSON.stringify(config)))).toEqual(config);
  });

  it('空值保持未初始化状态，最小合法配置补齐默认偏好', () => {
    expect(validateAppConfig(null)).toBeNull();
    expect(validateAppConfig({ projects: [validProject], activeProjectId: 'p1' })).toEqual({
      projects: [validProject],
      activeProjectId: 'p1',
      customThemes: [],
      preferences: { ...defaultPreferences, themeId: defaultThemeId },
    });
  });

  it('保留项目图标引用，拒绝裸表情、空值和未知图标前缀', () => {
    const emojiIcon = `emoji:${String.fromCodePoint(0x1f3af)}`;
    expect(
      validateAppConfig({
        projects: [
          { ...validProject, icon: emojiIcon },
          { id: 'p2', name: '设计', path: 'D:/design.md', icon: 'lucide:briefcase-business' },
        ],
        activeProjectId: 'p1',
      })?.projects,
    ).toEqual([
      { ...validProject, icon: emojiIcon },
      { id: 'p2', name: '设计', path: 'D:/design.md', icon: 'lucide:briefcase-business' },
    ]);

    for (const badIcon of [
      '',
      '   ',
      String.fromCodePoint(0x1f3af),
      'emoji:',
      'emoji:ab',
      'lucide:',
      'lucide:Briefcase',
      'svg:star',
      1,
      null,
    ]) {
      expect(() =>
        validateAppConfig({ projects: [{ ...validProject, icon: badIcon }], activeProjectId: 'p1' }),
      ).toThrow('STATE_CONFIG_INVALID');
    }
  });

  it('校验顶层工作台导航分区（悬赏板、待我处理、Agent 名录、设置）并拒绝未知分区', () => {
    for (const workspaceSection of ['bounties', 'inbox', 'agents', 'settings'] as const) {
      expect(
        validateAppConfig({ projects: [], preferences: { workspaceSection } })?.preferences.workspaceSection,
      ).toBe(workspaceSection);
    }
    for (const workspaceSection of ['unknown', 'canvas', 'sync', 1, null]) {
      expect(() => validateAppConfig({ projects: [], preferences: { workspaceSection } })).toThrow(
        'STATE_CONFIG_INVALID',
      );
    }
  });

  it('保留失焦保持透明与界面语言偏好，缺省关闭失焦透明且拒绝非布尔值', () => {
    expect(validateAppConfig({ projects: [] })?.preferences.keepTransparentOnBlur).toBe(false);
    expect(
      validateAppConfig({
        projects: [],
        preferences: { keepTransparentOnBlur: true, language: 'en' },
      })?.preferences,
    ).toMatchObject({ keepTransparentOnBlur: true, locale: 'en' });
    for (const keepTransparentOnBlur of ['true', 1, null, []]) {
      expect(() =>
        validateAppConfig({ projects: [], preferences: { keepTransparentOnBlur } }),
      ).toThrow('STATE_CONFIG_INVALID');
    }
  });

  it('保留合法字体栈偏好，拒绝空值与样式注入', () => {
    expect(
      validateAppConfig({
        projects: [],
        preferences: {
          fontFamily: '  "LXGW WenKai", "Inter", serif  ',
        },
      })?.preferences.fontFamily,
    ).toBe('"LXGW WenKai", "Inter", serif');
    for (const preferences of [
      { fontFamily: '' },
      { fontFamily: 'Bad;Font' },
      { fontFamily: 'url(https://example.com)' },
      { fontFamily: 123 },
    ]) {
      expect(() => validateAppConfig({ projects: [], preferences })).toThrow('STATE_CONFIG_INVALID');
    }
  });

  it('加载规范字段与旧布尔缩进并忽略遗留顶层字段', () => {
    const legacyConfig = validateAppConfig({
      projects: [validProject],
      activeProjectId: 'p1',
      projectViews: { p1: 'outline' },
      inspiration: { content: 'legacy', updatedAt: '2026-01-01' },
      headingAdd: true,
      canvasPreview: { zoom: 1 },
      sidebarView: 'projects',
      preferences: {
        locale: 'en',
        theme: 'dark',
        contentWidth: 680,
        paragraphIndent: 3,
        sidebarOpen: false,
        defaultProjectIcon: 'lucide:rocket',
      },
      window: { width: 1280, height: 800, x: 40, y: 60, maximized: true },
    });

    expect(legacyConfig).not.toBeNull();
    expect(legacyConfig?.preferences).toMatchObject({
      locale: 'en',
      theme: 'dark',
      contentWidth: 680,
      paragraphIndent: 3,
      sidebarOpen: false,
      defaultProjectIcon: 'lucide:rocket',
    });
    expect(legacyConfig?.window).toEqual({
      width: 1280,
      height: 800,
      x: 40,
      y: 60,
      maximized: true,
    });

    expect(
      validateAppConfig({
        projects: [],
        preferences: { paragraphIndent: true },
      })?.preferences.paragraphIndent,
    ).toBe(2);
    expect(
      validateAppConfig({
        projects: [],
        preferences: { paragraphIndent: false },
      })?.preferences.paragraphIndent,
    ).toBe(0);
    expect(
      validateAppConfig({
        projects: [],
        preferences: { paragraphIndent: 0 },
      })?.preferences.paragraphIndent,
    ).toBe(0);
  });

  it('拒绝损坏的项目结构、重复 ID、悬空活动项目和越界偏好', () => {
    const cases: unknown[] = [
      'not-an-object',
      {},
      { projects: [null] },
      { projects: [{ id: '', name: '工作', path: 'D:/work.md' }] },
      { projects: [validProject, validProject] },
      { projects: [validProject], activeProjectId: 'missing' },
      { projects: [validProject], activeProjectId: 'inspiration' },
      { projects: [validProject], preferences: { fontSize: 99 } },
      { projects: [validProject], preferences: { lineWidth: 100 } },
      { projects: [validProject], preferences: { contentWidth: 100 } },
      { projects: [validProject], preferences: { paragraphIndent: -1 } },
      { projects: [validProject], preferences: { paragraphIndent: 5 } },
      { projects: [validProject], preferences: { language: 'fr' } },
      { projects: [validProject], preferences: { locale: 'fr' } },
      { projects: [validProject], preferences: { defaultProjectIcon: 'bad' } },
      { projects: [validProject], window: { width: 100, height: 600 } },
    ];
    for (const item of cases) {
      expect(() => validateAppConfig(item)).toThrow('STATE_CONFIG_INVALID');
    }
  });

  it('校验悬赏服务连接模式、远程服务地址与管理员令牌', () => {
    expect(validateAppConfig({ projects: [] })?.preferences.serviceMode).toBe('embedded');

    for (const serviceMode of ['embedded', 'remote'] as const) {
      expect(
        validateAppConfig({ projects: [], preferences: { serviceMode } })?.preferences.serviceMode,
      ).toBe(serviceMode);
    }

    expect(
      validateAppConfig({
        projects: [],
        preferences: {
          serviceMode: 'remote',
          remoteServerUrl: '  https://bounty.example.com:8420///  ',
          remoteAdminToken: '  secret-token-123  ',
        },
      })?.preferences,
    ).toMatchObject({
      serviceMode: 'remote',
      remoteServerUrl: 'https://bounty.example.com:8420',
      remoteAdminToken: 'secret-token-123',
    });

    expect(
      validateAppConfig({
        projects: [],
        preferences: {
          remoteServerUrl: 'http://127.0.0.1:8420/subpath/',
          remoteAdminToken: '',
        },
      })?.preferences,
    ).toMatchObject({
      remoteServerUrl: 'http://127.0.0.1:8420/subpath',
      remoteAdminToken: '',
    });

    expect(
      validateAppConfig({
        projects: [],
        preferences: { remoteServerUrl: '   ' },
      })?.preferences.remoteServerUrl,
    ).toBe('');

    for (const badPreferences of [
      { serviceMode: 'standalone' },
      { serviceMode: 123 },
      { serviceMode: null },
      { remoteServerUrl: 'ftp://example.com' },
      { remoteServerUrl: 'javascript:alert(1)' },
      { remoteServerUrl: 'ws://127.0.0.1:8420' },
      { remoteServerUrl: 'not-a-url' },
      { remoteServerUrl: 'http://' },
      { remoteServerUrl: 42 },
      { remoteServerUrl: null },
      { remoteAdminToken: 123 },
      { remoteAdminToken: null },
      { remoteAdminToken: {} },
    ]) {
      expect(() => validateAppConfig({ projects: [], preferences: badPreferences })).toThrow(
        'STATE_CONFIG_INVALID',
      );
    }
  });
});
