/**
 * 文件职责：封装 Tauri 桌面环境的配置持久化、主题与 Markdown 文档导出与外链打开通道。
 * 定义范围：仅调用保留的原生命令（load_config / save_config / export_theme_file / export_markdown_file / open_external_link / open_local_document）与插件对话框。
 */
import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import { clonePersistableConfig } from '../config-defaults';
import { validateAppConfig } from '../config-validation';
import type { AppConfig, FilePort, SkillInstallRequest } from '../contracts';
import { translate } from '../i18n';

const patterns: readonly { match: RegExp; key: string; names: readonly string[] }[] = [
  {
    match: /^(.*) 原始数据已完整保留在备份：(.+)。$/,
    key: '{reason} 原始数据已完整保留在备份：{path}。',
    names: ['reason', 'path'],
  },
];

function isCodedError(error: unknown): error is { code: string; message: string } {
  return (
    !!error &&
    typeof error === 'object' &&
    'code' in error &&
    typeof (error as { code: unknown }).code === 'string' &&
    'message' in error &&
    typeof (error as { message: unknown }).message === 'string'
  );
}

function localizeBackendMessage(message: string): string {
  const direct = translate(message);
  if (direct !== message) return direct;
  for (const { match, key, names } of patterns) {
    const groups = match.exec(message);
    if (!groups) continue;
    const parameters = Object.fromEntries(
      names.map((name, index) => [
        name,
        name === 'reason' ? localizeBackendMessage(groups[index + 1]) : groups[index + 1],
      ]),
    );
    return translate(key, parameters);
  }
  return message;
}

async function call<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    if (isCodedError(error)) {
      throw Object.assign(new Error(localizeBackendMessage(error.message)), { code: error.code });
    }
    throw error;
  }
}

export class TauriFilePort implements FilePort {
  isBrowser = false;

  async loadConfig(): Promise<AppConfig | null> {
    return validateAppConfig(await call<AppConfig | null>('load_config'));
  }

  async saveConfig(config: AppConfig): Promise<void> {
    const validated = validateAppConfig(clonePersistableConfig(config));
    if (!validated) throw new Error('STATE_CONFIG_INVALID');
    await call('save_config', { config: validated });
  }

  async exportText(text: string, filename: string): Promise<boolean> {
    const extension = filename.endsWith('.json') ? 'json' : 'md';
    const picked = await save({
      title: extension === 'json' ? translate('保存主题模板') : translate('另存副本'),
      defaultPath: filename,
      filters: [
        extension === 'json'
          ? { name: translate('JSON 主题'), extensions: ['json'] }
          : { name: 'Markdown', extensions: ['md', 'markdown', 'txt'] },
      ],
    });
    if (!picked || Array.isArray(picked)) return false;
    await call(extension === 'json' ? 'export_theme_file' : 'export_markdown_file', {
      path: picked,
      text,
    });
    return true;
  }

  async openExternalLink(url: string): Promise<void> {
    await call('open_external_link', { url });
  }

  async pickSkillDirectory(): Promise<string | null> {
    const target = await open({ directory: true });
    return typeof target === 'string' ? target : null;
  }

  async installAgentSkill(request: SkillInstallRequest): Promise<string[]> {
    return call('install_agent_skill', { request });
  }

  async openLocalDocument(path: string): Promise<void> {
    await call('open_local_document', { path });
  }
}
