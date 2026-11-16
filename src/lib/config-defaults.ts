/**
 * 文件职责：提供工作台默认偏好、空应用配置与持久化拷贝辅助函数。
 * 定义范围：默认主题/排版/侧栏偏好与配置深拷贝。
 */
import type { AppConfig, Preferences } from './contracts';
import { DEFAULT_FONT_STACK } from './font-stack';
import { builtInThemes } from './themes';

export const defaultThemeId = builtInThemes[0].id;

export const defaultPreferences: Preferences = {
  locale: 'zh-CN',
  language: 'zh-CN',
  theme: 'system',
  themeId: defaultThemeId,
  keepTransparentOnBlur: false,
  fontFamily: DEFAULT_FONT_STACK,
  fontSize: 16,
  contentWidth: 800,
  lineWidth: 800,
  paragraphIndent: 2,
  sidebarOpen: true,
  sidebarCollapsed: false,
  workspaceSection: 'bounties',
  autoCheckUpdates: true,
  autoDownloadUpdates: true,
};

export function emptyAppConfig(): AppConfig {
  return {
    projects: [],
    activeProjectId: null,
    customThemes: [],
    preferences: { ...defaultPreferences },
  };
}

export function clonePersistableConfig(config: AppConfig): AppConfig {
  return {
    projects: config.projects.map(project => ({ ...project })),
    activeProjectId: config.activeProjectId,
    customThemes: (config.customThemes ?? []).map(theme => ({
      ...theme,
      light: { ...theme.light },
      dark: { ...theme.dark },
      ...(theme.appearance
        ? {
            appearance: {
              light: { ...theme.appearance.light },
              dark: { ...theme.appearance.dark },
            },
          }
        : {}),
    })),
    preferences: { ...config.preferences },
    window: config.window ? { ...config.window } : undefined,
  };
}
