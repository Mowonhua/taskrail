import { beforeEach, describe, expect, it, vi } from 'vitest';

const native = vi.hoisted(() => ({ invoke: vi.fn(), write: vi.fn(), save: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: native.save }));
vi.mock('@tauri-apps/plugin-fs', () => ({ writeTextFile: native.write }));

import { AndroidFilePort } from './android';

beforeEach(() => {
  vi.resetAllMocks();
});

describe('移动端平台端口边界', () => {
  it('导出使用用户选择的 URI，取消时不写入', async () => {
    const files = new AndroidFilePort();
    expect(files.isMobile).toBe(true);
    native.save.mockResolvedValue(null);
    expect(await files.exportText('{"version":1}', 'custom-theme.json')).toBe(false);
    expect(native.write).not.toHaveBeenCalled();

    native.save.mockResolvedValue('content://provider/export');
    expect(await files.exportText('{"version":1}', 'custom-theme.json')).toBe(true);
    expect(native.write).toHaveBeenCalledWith('content://provider/export', '{"version":1}');
  });
});
