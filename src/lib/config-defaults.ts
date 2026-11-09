/** 文件职责：提供各平台共用的配置初始偏好。 */
import { DEFAULT_FONT_STACK } from './font-stack';
import type { Preferences } from './contracts';

export const defaultPreferences = { theme: 'system' as const, themeId: 'paper', fontFamily: DEFAULT_FONT_STACK, fontSize: 16, contentWidth: 800, paragraphIndent: 2, autoCheckUpdates: true, autoDownloadUpdates: true, keepTransparentOnBlur: false, headingAdd: false, canvasPreview: 'compact' as const } satisfies Preferences;
