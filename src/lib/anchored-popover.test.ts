import { afterEach, describe, expect, it, vi } from 'vitest';
import { observeAnchoredPopover, positionAnchoredPopover } from './anchored-popover';

afterEach(() => { document.body.replaceChildren(); vi.restoreAllMocks(); });

describe('锚点浮层', () => {
  it('内部按下和焦点转移保留菜单，外部按下关闭，清理后不再接收事件', () => {
    const trigger = document.createElement('button');
    const popup = document.createElement('div');
    const option = document.createElement('button'); popup.append(option);
    document.body.append(trigger, popup);
    const hide = vi.fn();
    Object.defineProperty(popup, 'hidePopover', { value: hide });
    vi.spyOn(popup, 'matches').mockImplementation(selector => selector === ':popover-open');
    const close = vi.fn();
    const dispose = observeAnchoredPopover(trigger, popup, close);
    trigger.dispatchEvent(new Event('pointerdown', { bubbles: true }));
    option.dispatchEvent(new Event('pointerdown', { bubbles: true }));
    popup.dispatchEvent(new Event('focusout', { bubbles: true }));
    expect(close).not.toHaveBeenCalled();
    document.body.dispatchEvent(new Event('pointerdown', { bubbles: true }));
    expect(close).toHaveBeenCalledOnce();
    dispose();
    expect(hide).toHaveBeenCalledOnce();
    document.body.dispatchEvent(new Event('pointerdown', { bubbles: true }));
    expect(close).toHaveBeenCalledOnce();
  });

  it('锚点离开可滚动祖先后请求关闭，重新进入后保留菜单', () => {
    const ancestor = document.createElement('div'); ancestor.style.overflowY = 'auto';
    const trigger = document.createElement('button'); const popup = document.createElement('div');
    ancestor.append(trigger); document.body.append(ancestor, popup);
    vi.spyOn(ancestor, 'getBoundingClientRect').mockReturnValue({ top: 100, bottom: 300, left: 0, right: 300 } as DOMRect);
    const bounds = vi.spyOn(trigger, 'getBoundingClientRect');
    bounds.mockReturnValue({ top: 20, bottom: 50, left: 10, right: 100, width: 90 } as DOMRect);
    const close = vi.fn();
    positionAnchoredPopover(trigger, popup, close);
    expect(close).toHaveBeenCalledOnce();
    bounds.mockReturnValue({ top: 120, bottom: 150, left: 10, right: 100, width: 90 } as DOMRect);
    positionAnchoredPopover(trigger, popup, close);
    expect(close).toHaveBeenCalledOnce();
  });

  it('align=start 使用浮层固有宽度并与触发器左缘对齐', () => {
    const trigger = document.createElement('button');
    const popup = document.createElement('div');
    document.body.append(trigger, popup);
    vi.spyOn(trigger, 'getBoundingClientRect').mockReturnValue({
      top: 100,
      bottom: 130,
      left: 40,
      right: 70,
      width: 30,
    } as DOMRect);
    vi.spyOn(popup, 'getBoundingClientRect').mockReturnValue({
      width: 160,
      height: 80,
    } as DOMRect);
    positionAnchoredPopover(trigger, popup, vi.fn(), { align: 'start' });
    expect(popup.style.width).toBe('');
    expect(popup.style.left).toBe('40px');
    expect(popup.style.top).toBe('134px');
  });

  it('align=end 保留浮层固有宽度并与触发器右缘对齐', () => {
    const trigger = document.createElement('button');
    const popup = document.createElement('div');
    document.body.append(trigger, popup);
    vi.spyOn(trigger, 'getBoundingClientRect').mockReturnValue({
      top: 100,
      bottom: 130,
      left: 500,
      right: 530,
      width: 30,
    } as DOMRect);
    vi.spyOn(popup, 'getBoundingClientRect').mockReturnValue({
      width: 160,
      height: 80,
    } as DOMRect);
    positionAnchoredPopover(trigger, popup, vi.fn(), { align: 'end' });
    expect(popup.style.width).toBe('');
    expect(popup.style.left).toBe('370px');
    expect(popup.style.top).toBe('134px');
  });
});
