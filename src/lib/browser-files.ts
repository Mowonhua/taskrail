/**
 * 文件职责：为浏览器预览与测试环境提供基于 localStorage 的配置持久化与文件导出回退实现。
 * 定义范围：配置读取、校验保存与浏览器 Blob 下载。
 */
import { clonePersistableConfig } from './config-defaults';
import { validateAppConfig } from './config-validation';
import type { AppConfig, FilePort } from './contracts';

const CONFIG_KEY = 'taskrail:config';

export class BrowserFilePort implements FilePort {
  isBrowser = true;

  async loadConfig(): Promise<AppConfig | null> {
    const raw = localStorage.getItem(CONFIG_KEY);
    return raw ? validateAppConfig(JSON.parse(raw)) : null;
  }

  async saveConfig(config: AppConfig): Promise<void> {
    const validated = validateAppConfig(clonePersistableConfig(config));
    if (!validated) throw new Error('STATE_CONFIG_INVALID');
    localStorage.setItem(CONFIG_KEY, JSON.stringify(validated));
  }

  async exportText(text: string, filename: string): Promise<boolean> {
    const mime = filename.endsWith('.json')
      ? 'application/json;charset=utf-8'
      : 'text/markdown;charset=utf-8';
    const url = URL.createObjectURL(new Blob([text], { type: mime }));
    const link = document.createElement('a');
    link.href = url;
    link.download = filename;
    link.click();
    URL.revokeObjectURL(url);
    return true;
  }

  async openExternalLink(url: string): Promise<void> {
    window.open(url, '_blank', 'noopener,noreferrer');
  }
}
