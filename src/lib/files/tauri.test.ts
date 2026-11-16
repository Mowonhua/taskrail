import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { emptyAppConfig } from '../config-defaults';
import { setLocalePreference } from '../i18n';
import { TauriFilePort } from './tauri';

const native = vi.hoisted(() => ({
  invoke: vi.fn(),
  save: vi.fn(),
  writeTextFile: vi.fn(),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: native.save }));
vi.mock('@tauri-apps/plugin-fs', () => ({ writeTextFile: native.writeTextFile }));

beforeEach(() => {
  vi.resetAllMocks();
  setLocalePreference('en');
});
afterEach(() => {
  setLocalePreference('zh-CN');
});

describe('桌面平台端口边界', () => {
  it('按当前界面语言翻译原生错误并保留错误码', async () => {
    const port = new TauriFilePort();
    native.invoke.mockRejectedValue({
      code: 'LINK_PROTOCOL_BLOCKED',
      message: '仅允许打开网页或电子邮件链接。',
    });
    await expect(port.openExternalLink('javascript:alert(1)')).rejects.toMatchObject({
      code: 'LINK_PROTOCOL_BLOCKED',
      message: 'Only web and email links can be opened.',
    });
    setLocalePreference('zh-CN');
    await expect(port.openExternalLink('javascript:alert(1)')).rejects.toMatchObject({
      code: 'LINK_PROTOCOL_BLOCKED',
      message: '仅允许打开网页或电子邮件链接。',
    });
    expect(native.invoke).toHaveBeenLastCalledWith('open_external_link', {
      url: 'javascript:alert(1)',
    });
  });

  it('校验并保存应用配置，读取时还原经过校验的配置快照', async () => {
    const port = new TauriFilePort();
    const config = emptyAppConfig();
    config.preferences.workspaceSection = 'agents';
    native.invoke.mockResolvedValueOnce(undefined).mockResolvedValueOnce(config);
    await port.saveConfig(config);
    expect(native.invoke).toHaveBeenCalledWith('save_config', { config });
    const loaded = await port.loadConfig();
    expect(loaded?.preferences.workspaceSection).toBe('agents');
  });

  it('使用系统保存对话框与受控原生命令 export_theme_file 导出 JSON 主题模板，取消时不写入', async () => {
    const port = new TauriFilePort();
    native.save.mockResolvedValueOnce(null);
    expect(await port.exportText('{}', 'custom-theme.json')).toBe(false);
    expect(native.invoke).not.toHaveBeenCalled();

    native.save.mockResolvedValueOnce('D:/custom-theme.json');
    native.invoke.mockResolvedValueOnce(undefined);
    expect(await port.exportText('{"version":1}', 'custom-theme.json')).toBe(true);
    expect(native.save).toHaveBeenCalledWith(
      expect.objectContaining({ title: 'Save theme template', defaultPath: 'custom-theme.json' }),
    );
    expect(native.invoke).toHaveBeenCalledWith('export_theme_file', {
      path: 'D:/custom-theme.json',
      text: '{"version":1}',
    });
  });

  it('调用受控原生命令 open_local_document 打开本地文档路径', async () => {
    const port = new TauriFilePort();
    native.invoke.mockResolvedValueOnce(undefined);
    await port.openLocalDocument('D:/docs/guide.md');
    expect(native.invoke).toHaveBeenCalledWith('open_local_document', {
      path: 'D:/docs/guide.md',
    });
  });
});
