/**
 * 文件职责：验证应用持久化配置的边界与字段契约。
 * 定义范围：项目元数据（含项目图标）、自定义主题、界面/排版/工作台分区偏好与桌面窗口坐标校验。
 */
import { normalizeServerUrl } from './api/bounty-client';
import type { ServiceMode } from './api/types';
import { defaultPreferences, defaultThemeId } from './config-defaults';
import type { AppConfig, Preferences, Project, WindowPlacement, WorkspaceSection } from './contracts';
import { translate } from './i18n';
import type { LocalePreference } from './i18n';
import { parseProjectIcon, serializeProjectIcon } from './project-icons';
import { builtInThemes, validateTheme, type ThemeDefinition, type ThemeMode } from './themes';

const WORKSPACE_SECTIONS = new Set<WorkspaceSection>(['bounties', 'inbox', 'agents', 'settings']);
const SERVICE_MODES = new Set<ServiceMode>(['embedded', 'remote']);

function invalidConfig(): never {
  throw new Error(
    `STATE_CONFIG_INVALID: ${translate('项目配置格式无效，原配置已保留，请修复配置后重新打开应用。')}`,
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value);
}

function finiteNumberInRange(value: unknown, min: number, max: number, fallback: number): number {
  if (value === undefined) return fallback;
  if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max) invalidConfig();
  return value;
}

function booleanOrDefault(value: unknown, fallback: boolean): boolean {
  if (value === undefined) return fallback;
  if (typeof value !== 'boolean') invalidConfig();
  return value;
}

function validateFontFamily(value: unknown): string {
  if (value === undefined) return defaultPreferences.fontFamily;
  if (typeof value !== 'string') invalidConfig();
  const trimmed = value.trim();
  if (!trimmed || trimmed.length > 512 || /[;{}<>]|url\s*\(/i.test(trimmed)) invalidConfig();
  return trimmed;
}

function validateProject(value: unknown): Project {
  if (!isRecord(value)) invalidConfig();
  const { id, name, path, icon, archived } = value;
  if (typeof id !== 'string' || !id.trim()) invalidConfig();
  if (typeof name !== 'string' || !name.trim()) invalidConfig();
  if (typeof path !== 'string' || !path.trim()) invalidConfig();
  if (archived !== undefined && typeof archived !== 'boolean') invalidConfig();
  let normalizedIcon: Project['icon'];
  if (icon !== undefined) {
    const parsedIcon = parseProjectIcon(icon);
    if (!parsedIcon) invalidConfig();
    normalizedIcon = serializeProjectIcon(parsedIcon);
  }
  return {
    id,
    name: name.trim(),
    path: path.trim(),
    ...(normalizedIcon !== undefined ? { icon: normalizedIcon } : {}),
    ...(archived !== undefined ? { archived } : {}),
  };
}

function validateCustomThemes(value: unknown): ThemeDefinition[] {
  if (value === undefined) return [];
  if (!Array.isArray(value)) invalidConfig();
  const seen = new Set(builtInThemes.map(theme => theme.id));
  return value.map(item => {
    let theme: ThemeDefinition;
    try {
      theme = validateTheme(item);
    } catch {
      invalidConfig();
    }
    if (seen.has(theme.id)) invalidConfig();
    seen.add(theme.id);
    return theme;
  });
}

function validateParagraphIndent(value: unknown, fallback: number): number {
  if (value === undefined) return fallback;
  if (typeof value === 'boolean') return value ? 2 : 0;
  return finiteNumberInRange(value, 0, 4, fallback);
}

function validateLocalePreference(value: Record<string, unknown>): LocalePreference {
  const hasLocale = value.locale !== undefined;
  const hasLanguage = value.language !== undefined;
  if (hasLocale && value.locale !== 'zh-CN' && value.locale !== 'en' && value.locale !== 'system') {
    invalidConfig();
  }
  if (hasLanguage && value.language !== 'zh-CN' && value.language !== 'en' && value.language !== 'system') {
    invalidConfig();
  }
  const localeVal = value.locale as LocalePreference | undefined;
  const languageVal = value.language as LocalePreference | undefined;
  const fallback = defaultPreferences.locale ?? 'zh-CN';
  if (localeVal !== undefined && languageVal !== undefined) {
    if (localeVal !== fallback) return localeVal;
    if (languageVal !== fallback) return languageVal;
    return localeVal;
  }
  return localeVal ?? languageVal ?? fallback;
}

function validateContentWidth(value: Record<string, unknown>): number {
  const fallback = defaultPreferences.contentWidth;
  const hasContentWidth = value.contentWidth !== undefined;
  const hasLineWidth = value.lineWidth !== undefined;
  const contentWidthVal = hasContentWidth
    ? finiteNumberInRange(value.contentWidth, 560, 960, fallback)
    : undefined;
  const lineWidthVal = hasLineWidth ? finiteNumberInRange(value.lineWidth, 560, 960, fallback) : undefined;
  if (contentWidthVal !== undefined && lineWidthVal !== undefined) {
    if (contentWidthVal !== fallback) return contentWidthVal;
    if (lineWidthVal !== fallback) return lineWidthVal;
    return contentWidthVal;
  }
  return contentWidthVal ?? lineWidthVal ?? fallback;
}

function validateSidebarOpen(value: Record<string, unknown>): boolean {
  const hasSidebarOpen = value.sidebarOpen !== undefined;
  const hasSidebarCollapsed = value.sidebarCollapsed !== undefined;
  if (hasSidebarOpen && typeof value.sidebarOpen !== 'boolean') invalidConfig();
  if (hasSidebarCollapsed && typeof value.sidebarCollapsed !== 'boolean') invalidConfig();
  const openVal = value.sidebarOpen as boolean | undefined;
  const collapsedVal = value.sidebarCollapsed as boolean | undefined;
  if (openVal !== undefined && collapsedVal !== undefined) {
    if (openVal === false || collapsedVal === true) return false;
    return true;
  }
  if (openVal !== undefined) return openVal;
  if (collapsedVal !== undefined) return !collapsedVal;
  return defaultPreferences.sidebarOpen ?? true;
}

export function validatePreferences(value: unknown, customThemes: ThemeDefinition[] = []): Preferences {
  if (value === undefined) return { ...defaultPreferences };
  if (!isRecord(value)) invalidConfig();
  const locale = validateLocalePreference(value);
  const theme = value.theme ?? defaultPreferences.theme;
  if (theme !== 'light' && theme !== 'dark' && theme !== 'system') invalidConfig();
  const themeIds = new Set([...builtInThemes, ...customThemes].map(item => item.id));
  const themeId = value.themeId ?? defaultThemeId;
  if (typeof themeId !== 'string' || !themeIds.has(themeId)) invalidConfig();

  const workspaceSectionValue =
    value.workspaceSection === undefined
      ? (defaultPreferences.workspaceSection ?? 'bounties')
      : value.workspaceSection;
  if (
    typeof workspaceSectionValue !== 'string' ||
    !WORKSPACE_SECTIONS.has(workspaceSectionValue as WorkspaceSection)
  ) {
    invalidConfig();
  }

  const serviceModeValue =
    value.serviceMode === undefined ? (defaultPreferences.serviceMode ?? 'embedded') : value.serviceMode;
  if (typeof serviceModeValue !== 'string' || !SERVICE_MODES.has(serviceModeValue as ServiceMode)) {
    invalidConfig();
  }

  let remoteServerUrl: string | undefined;
  if (value.remoteServerUrl !== undefined) {
    if (typeof value.remoteServerUrl !== 'string') invalidConfig();
    const trimmed = value.remoteServerUrl.trim();
    if (trimmed === '') {
      remoteServerUrl = '';
    } else {
      try {
        remoteServerUrl = normalizeServerUrl(trimmed);
      } catch {
        invalidConfig();
      }
    }
  }

  let remoteAdminToken: string | undefined;
  if (value.remoteAdminToken !== undefined) {
    if (typeof value.remoteAdminToken !== 'string') invalidConfig();
    remoteAdminToken = value.remoteAdminToken.trim();
  }

  let defaultProjectIcon: string | undefined;
  if (value.defaultProjectIcon !== undefined) {
    const parsedIcon = parseProjectIcon(value.defaultProjectIcon);
    if (!parsedIcon) invalidConfig();
    defaultProjectIcon = serializeProjectIcon(parsedIcon);
  }

  const contentWidth = validateContentWidth(value);
  const paragraphIndent = validateParagraphIndent(value.paragraphIndent, defaultPreferences.paragraphIndent);
  const sidebarOpen = validateSidebarOpen(value);

  return {
    locale,
    language: locale,
    theme: theme as ThemeMode,
    themeId,
    keepTransparentOnBlur: booleanOrDefault(
      value.keepTransparentOnBlur,
      defaultPreferences.keepTransparentOnBlur ?? false,
    ),
    fontFamily: validateFontFamily(value.fontFamily),
    fontSize: finiteNumberInRange(value.fontSize, 13, 22, defaultPreferences.fontSize),
    contentWidth,
    lineWidth: contentWidth,
    paragraphIndent,
    sidebarOpen,
    sidebarCollapsed: !sidebarOpen,
    workspaceSection: workspaceSectionValue as WorkspaceSection,
    ...(defaultProjectIcon !== undefined ? { defaultProjectIcon } : {}),
    serviceMode: serviceModeValue as ServiceMode,
    ...(remoteServerUrl !== undefined ? { remoteServerUrl } : {}),
    ...(remoteAdminToken !== undefined ? { remoteAdminToken } : {}),
    autoCheckUpdates: booleanOrDefault(value.autoCheckUpdates, defaultPreferences.autoCheckUpdates ?? true),
    autoDownloadUpdates: booleanOrDefault(
      value.autoDownloadUpdates,
      defaultPreferences.autoDownloadUpdates ?? true,
    ),
  };
}

function validateWindowPlacement(value: unknown): WindowPlacement | undefined {
  if (value === undefined) return undefined;
  if (!isRecord(value)) invalidConfig();
  const width = value.width === undefined ? undefined : finiteNumberInRange(value.width, 320, 10000, 1180);
  const height = value.height === undefined ? undefined : finiteNumberInRange(value.height, 240, 10000, 780);
  const x = value.x === undefined ? undefined : finiteNumberInRange(value.x, -32000, 32000, 0);
  const y = value.y === undefined ? undefined : finiteNumberInRange(value.y, -32000, 32000, 0);
  const maximized = value.maximized === undefined ? undefined : booleanOrDefault(value.maximized, false);
  return {
    ...(width !== undefined ? { width } : {}),
    ...(height !== undefined ? { height } : {}),
    ...(x !== undefined ? { x } : {}),
    ...(y !== undefined ? { y } : {}),
    ...(maximized !== undefined ? { maximized } : {}),
  };
}

export function validateAppConfig(value: unknown): AppConfig | null {
  if (value === null || value === undefined) return null;
  if (!isRecord(value) || !Array.isArray(value.projects)) invalidConfig();
  // 容忍旧单文件版本遗留顶层字段（projectViews, inspiration, headingAdd, canvasPreview, sidebarView）并忽略
  const projects = value.projects.map(validateProject);
  const ids = new Set<string>();
  for (const project of projects) {
    if (ids.has(project.id)) invalidConfig();
    ids.add(project.id);
  }
  const activeProjectId = value.activeProjectId ?? null;
  if (activeProjectId !== null && (typeof activeProjectId !== 'string' || !ids.has(activeProjectId))) {
    invalidConfig();
  }
  const customThemes = validateCustomThemes(value.customThemes);
  const window = validateWindowPlacement(value.window);
  return {
    projects,
    activeProjectId,
    customThemes,
    preferences: validatePreferences(value.preferences, customThemes),
    ...(window ? { window } : {}),
  };
}
