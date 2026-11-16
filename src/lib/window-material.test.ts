/** 文件职责：验证原生窗口请求串行、过期结果隔离和错误回退。 */
import { describe, expect, it, vi } from 'vitest';
import { WindowMaterialController } from './window-material';

describe('窗口材质协调', () => {
  it('同一材质切换明暗时仍同步原生主题，避免沿用系统默认染色', async () => {
    const native = vi.fn().mockResolvedValue(true);
    const root = document.createElement('div');
    const controller = new WindowMaterialController(root, native);
    await controller.update('acrylic', 'dark');
    await controller.update('acrylic', 'light');
    await controller.update('acrylic', 'system');
    expect(native.mock.calls).toEqual([['acrylic', 'dark', false], ['acrylic', 'light', false], ['acrylic', 'system', false]]);
    expect(root.dataset.windowTransparent).toBe('true');
  });
  it('失焦透明偏好随请求串行应用，切回普通主题不会残留透明', async () => {
    const root = document.createElement('div');
    let finish!: (value: boolean) => void;
    const native = vi.fn().mockImplementationOnce(() => new Promise<boolean>(resolve => { finish = resolve; })).mockResolvedValue(true);
    const controller = new WindowMaterialController(root, native);
    const first = controller.update('acrylic', 'dark', true);
    await Promise.resolve();
    const next = controller.update('acrylic', 'dark', false);
    const last = controller.update('opaque', 'light', true);
    expect(native).toHaveBeenCalledTimes(1);
    finish(true);
    await first;
    expect(root.dataset.windowTransparent).toBe('false');
    await Promise.all([next, last]);
    expect(native.mock.calls).toEqual([['acrylic', 'dark', true], ['acrylic', 'dark', false], ['opaque', 'light', true]]);
    expect(root.dataset.windowTransparent).toBe('false');
  });
  it('快速切回普通主题时，旧透明请求不得重新打开透明背景', async () => {
    const root = document.createElement('div');
    let finish!: (value: boolean) => void;
    const native = vi.fn().mockImplementationOnce(() => new Promise<boolean>(resolve => { finish = resolve; })).mockResolvedValue(true);
    const controller = new WindowMaterialController(root, native);
    const first = controller.update('transparent');
    await Promise.resolve();
    const last = controller.update('opaque');
    expect(native).toHaveBeenCalledTimes(1);
    finish(true);
    await first;
    expect(root.dataset.windowTransparent).toBe('false');
    await last;
    expect(native.mock.calls.map(call => call[0])).toEqual(['transparent', 'opaque']);
    expect(root.dataset.windowTransparent).toBe('false');
  });
  it('只有原生成功才透明，失败后仍能继续切换', async () => {
    const root = document.createElement('div');
    const native = vi.fn().mockResolvedValueOnce(false).mockRejectedValueOnce(new Error('OS failure')).mockResolvedValue(true);
    const controller = new WindowMaterialController(root, native);
    await controller.update('acrylic');
    expect(root.dataset.windowTransparent).toBe('false');
    await expect(controller.update('blur')).rejects.toThrow('OS failure');
    expect(root.dataset.windowTransparent).toBe('false');
    await controller.update('transparent');
    expect(root.dataset.windowTransparent).toBe('true');
    const reset = controller.update('opaque');
    expect(root.dataset.windowTransparent).toBe('false');
    await reset;
  });
  it('已透明时调整明暗或失焦偏好，在等待原生完成期间不闪回实色', async () => {
    const root = document.createElement('div');
    let finishPending!: (value: boolean) => void;
    const native = vi.fn()
      .mockResolvedValueOnce(true)
      .mockImplementationOnce(() => new Promise<boolean>(resolve => { finishPending = resolve; }));
    const controller = new WindowMaterialController(root, native);
    await controller.update('acrylic', 'dark', false);
    expect(root.dataset.windowTransparent).toBe('true');

    const pending = controller.update('acrylic', 'light', true);
    expect(root.dataset.windowTransparent).toBe('true');
    await Promise.resolve();
    expect(root.dataset.windowTransparent).toBe('true');

    finishPending(true);
    await pending;
    expect(root.dataset.windowTransparent).toBe('true');
  });
  it('已透明后后续切换失败或不支持时回落为实色', async () => {
    const root = document.createElement('div');
    const native = vi.fn()
      .mockResolvedValueOnce(true)
      .mockRejectedValueOnce(new Error('OS failure'))
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce(false);
    const controller = new WindowMaterialController(root, native);
    await controller.update('acrylic', 'dark');
    expect(root.dataset.windowTransparent).toBe('true');

    await expect(controller.update('blur')).rejects.toThrow('OS failure');
    expect(root.dataset.windowTransparent).toBe('false');

    await controller.update('acrylic', 'light');
    expect(root.dataset.windowTransparent).toBe('true');

    await controller.update('blur');
    expect(root.dataset.windowTransparent).toBe('false');
  });
  it('透明状态下触发过渡遮罩会交替启动淡出动画并在结束后清理，切回实色或减少动态效果时不保留遮罩', async () => {
    const root = document.createElement('div');
    const controller = new WindowMaterialController(root, vi.fn().mockResolvedValue(true));

    controller.showBackdropVeil('fade');
    expect(root.dataset.backdropVeil).toBeUndefined();

    await controller.update('acrylic', 'dark');
    controller.showBackdropVeil('hold');
    expect(root.dataset.backdropVeil).toBe('active');

    controller.showBackdropVeil('fade');
    expect(root.dataset.backdropVeil).toBe('fade-a');

    controller.showBackdropVeil('fade');
    expect(root.dataset.backdropVeil).toBe('fade-b');

    const endEvent = new Event('animationend') as Event & { animationName: string };
    Object.defineProperty(endEvent, 'animationName', { value: 'backdrop-veil-fade-b' });
    root.dispatchEvent(endEvent);
    expect(root.dataset.backdropVeil).toBeUndefined();

    controller.showBackdropVeil('fade');
    expect(root.dataset.backdropVeil).toBe('fade-a');
    await controller.update('opaque', 'light');
    expect(root.dataset.backdropVeil).toBeUndefined();

    await controller.update('acrylic', 'light');
    const originalMatchMedia = window.matchMedia;
    vi.stubGlobal('matchMedia', vi.fn().mockReturnValue({ matches: true }));
    try {
      controller.showBackdropVeil('fade');
      expect(root.dataset.backdropVeil).toBeUndefined();
    } finally {
      vi.stubGlobal('matchMedia', originalMatchMedia);
      controller.dispose();
    }
  });
});
