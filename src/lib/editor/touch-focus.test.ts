/** 文件职责：验证手机触摸滑动不聚焦正文、轻点保留系统聚焦，并保留长按选字。 */
import { afterEach, expect, it } from 'vitest';
import { EditorController } from './index';

let instance: EditorController;
afterEach(() => { instance?.destroy(); document.body.replaceChildren(); });
function editor(text: string, nativeSelection = true) {
  instance = new EditorController(document.body, { text, mode: 'todo', nativeSelection, onChange: () => {} });
}
function touch(type: 'touchstart' | 'touchmove' | 'touchend', x: number, y: number) {
  const touches = type === 'touchend' ? [] : [{ clientX: x, clientY: y }] as unknown as Touch[];
  instance.view.contentDOM.dispatchEvent(new TouchEvent(type, { bubbles: true, cancelable: true, touches }));
}

it('手机快速滑动撤销正文聚焦，轻点保持系统聚焦', () => {
  editor('- [ ] 任务\n');
  const content = instance.view.contentDOM;
  touch('touchstart', 100, 100);
  // 模拟 Android WebView 按住正文时的默认聚焦。
  content.focus();
  touch('touchend', 100, 100);
  expect(document.activeElement).toBe(content);

  content.blur();
  touch('touchstart', 100, 100);
  content.focus();
  touch('touchmove', 160, 100);
  expect(document.activeElement).not.toBe(content);
});

it('滑动撤销聚焦后恢复滑动前的插入点', () => {
  editor('正文\n\n- [ ] 任务\n');
  instance.focusAt(0);
  const before = instance.state.selection;
  instance.view.contentDOM.blur();
  touch('touchstart', 100, 100);
  instance.view.contentDOM.focus();
  instance.view.dispatch({ selection: { anchor: 3 } });
  touch('touchmove', 160, 100);
  expect(document.activeElement).not.toBe(instance.view.contentDOM);
  expect(instance.state.selection.eq(before)).toBe(true);
});

it('抬手后才补上聚焦的滑动同样撤销聚焦', async () => {
  editor('- [ ] 任务\n');
  const content = instance.view.contentDOM;
  touch('touchstart', 100, 100);
  touch('touchmove', 160, 100);
  // WebView 也可能在抬手时才把焦点交给正文。
  content.focus();
  touch('touchend', 160, 100);
  await new Promise(resolve => requestAnimationFrame(() => resolve(undefined)));
  expect(document.activeElement).not.toBe(content);
});

it('已聚焦时滑动滚动保持聚焦', () => {
  editor('- [ ] 任务\n');
  instance.focusAt(0);
  touch('touchstart', 100, 100);
  touch('touchmove', 160, 100);
  expect(document.activeElement).toBe(instance.view.contentDOM);
});

it('长按后的拖动保留系统选字', async () => {
  editor('- [ ] 任务\n');
  const content = instance.view.contentDOM;
  touch('touchstart', 100, 100);
  content.focus();
  await new Promise(resolve => setTimeout(resolve, 300));
  touch('touchmove', 160, 100);
  expect(document.activeElement).toBe(content);
});

it('桌面触摸不启用手机聚焦防护', () => {
  editor('- [ ] 任务\n', false);
  const content = instance.view.contentDOM;
  touch('touchstart', 100, 100);
  content.focus();
  touch('touchmove', 160, 100);
  expect(document.activeElement).toBe(content);
});
