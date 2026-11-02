/** 用真实布局检查跨项目复用编辑视图后，阅读位置不会逐次向下漂移。 */
import { EditorController } from '../src/lib/editor';
import type { ViewMode } from '../src/lib/editor/types';

const tasks = (done: boolean) => Array.from({ length: 100 }, (_, index) => `- [${done ? 'x' : ' '}] ${done ? '归档' : '待办'} ${index}\n\n  说明段落 ${index}`).join('\n\n');
const text = `# 项目\n\n${tasks(false)}\n\n# 归档\n\n${tasks(true)}\n`;
const mobile = new URLSearchParams(location.search).has('mobile');
const editor = new EditorController(document.querySelector<HTMLElement>('#editor')!, { text, mode: 'todo', onChange: () => {}, nativeSelection: mobile });
async function settled(): Promise<void> {
  for (let frame = 0; frame < 16; frame++) await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
}

async function verify(mode: ViewMode, fraction: number): Promise<Record<string, unknown>> {
  editor.restoreState(editor.createState(text, mode), { mode, cursor: 0, scrollTop: 0, folded: [] });
  if (!mobile) editor.view.focus();
  editor.normalizeArchive();
  await settled();
  const scroll = editor.view.scrollDOM;
  scroll.scrollTop = (scroll.scrollHeight - scroll.clientHeight) * fraction;
  await settled();
  const before = scroll.scrollTop;
  const positions: number[] = [];
  const limits: number[] = [];
  // 每轮都重新保存读取位置，模拟应用的 captureUI，暴露累积偏移。
  for (let count = 0; count < 3; count++) {
    const state = editor.state;
    const ui = editor.getUIState();
    editor.restoreState(editor.createState('- [ ] 另一个项目\n', 'todo'), { mode: 'todo', cursor: 0, scrollTop: 0, folded: [] });
    await settled();
    editor.restoreState(state, ui);
    if (!mobile) editor.view.focus();
    editor.normalizeArchive();
    await settled();
    positions.push(scroll.scrollTop);
    limits.push(scroll.scrollHeight - scroll.clientHeight);
  }
  // 重建后真实行高可能修正文档总高度；原位置超过新范围时应停在可滚动的底部。
  return { mode, fraction, before, positions, limits, pass: positions.every((position, index) => Math.abs(position - Math.min(before, limits[index])) <= 1) };
}

/** 恢复尚未落定时，用户滚动或切到另一项目必须使旧任务失效。 */
async function verifyCancellation(): Promise<Record<string, unknown>> {
  const mode = 'todo';
  const ui = { mode, cursor: 0, scrollTop: 2000, folded: [] } as const;
  const state = editor.createState(text, mode);
  editor.restoreState(state, { ...ui, folded: [] });
  await settled();
  editor.setUIState({ ...ui, folded: [] });
  editor.view.scrollDOM.dispatchEvent(new WheelEvent('wheel'));
  editor.view.scrollDOM.scrollTop = 500;
  await settled();
  const afterWheel = editor.view.scrollDOM.scrollTop;
  editor.restoreState(state, { ...ui, folded: [] });
  editor.restoreState(editor.createState(text, mode), { ...ui, scrollTop: 800, folded: [] });
  await settled();
  const afterSwitch = editor.view.scrollDOM.scrollTop;
  return { name: '主动滚动与快速切换取消旧恢复', afterWheel, afterSwitch, pass: Math.abs(afterWheel - 500) <= 1 && Math.abs(afterSwitch - 800) <= 1 };
}

document.querySelector('#verify')!.addEventListener('click', async () => {
  const scenarios = [];
  for (const mode of ['todo', 'archive', 'source'] as const) {
    for (const fraction of [0, 0.1, 0.5, 1]) scenarios.push(await verify(mode, fraction));
  }
  scenarios.push(await verifyCancellation());
  document.querySelector('#result')!.textContent = JSON.stringify({ pass: scenarios.every(scenario => scenario.pass), scenarios });
});
