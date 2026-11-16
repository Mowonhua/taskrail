/** 文件职责：挂载桌面与浏览器共享的应用界面，并在首帧样式就绪后显示桌面窗口。 */
import { mount } from 'svelte';
import { platformCapabilities } from './lib/platform';
import { getCurrentWindow } from '@tauri-apps/api/window';
import App from './App.svelte';
import './app.css';
import { applyTheme, bootThemeKey, builtInThemes, parseTheme, type ThemeDefinition, type ThemeMode } from './lib/themes';

/**
 * 配置异步恢复前从启动缓存取首帧配色；缓存由 App 在主题生效时回写。
 * 缓存缺失、损坏或未通过主题校验时回退内置默认主题，避免 CSS 维护另一份默认主题。
 */
function bootTheme(): { theme: ThemeDefinition; mode: ThemeMode } {
  try {
    const cached = localStorage.getItem(bootThemeKey);
    if (cached) {
      const value = JSON.parse(cached) as { definition?: unknown; mode?: unknown };
      if (value.mode === 'light' || value.mode === 'dark' || value.mode === 'system')
        return { theme: parseTheme(JSON.stringify(value.definition)), mode: value.mode };
    }
  } catch { /* 缓存不可信时按无缓存处理。 */ }
  return { theme: builtInThemes[0], mode: 'system' };
}

const boot = bootTheme();
applyTheme(document.documentElement, boot.theme, boot.mode, window.matchMedia('(prefers-color-scheme: dark)').matches);
let windowShown = false;
const showDesktopWindow = () => {
  if (windowShown || !platformCapabilities().desktop) return;
  windowShown = true;
  void getCurrentWindow().show().then(() => getCurrentWindow().setFocus());
};

mount(App, {
  target: document.getElementById('app')!,
  props: platformCapabilities().desktop ? { onready: showDesktopWindow } : {},
});

// 桌面窗口以 visible:false 创建，等待首帧主题、原生材质和界面内容就绪后再显示，避免从透明底闪到半透明；同时保留兜底超时避免初始化异常导致窗口长期不可见。
if (platformCapabilities().desktop) setTimeout(showDesktopWindow, 1500);
