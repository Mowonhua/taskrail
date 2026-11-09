/** 文件职责：验证正文段首缩进的投影范围与列表文字的列对齐。 */
import { afterEach, expect, it } from 'vitest';
import { EditorController } from './index';

Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
Range.prototype.getBoundingClientRect = () => new DOMRect();
let instance: EditorController;
afterEach(() => { instance?.destroy(); document.body.replaceChildren(); });

const text = [
  '# 章节',
  '',
  '段落一',
  '续行',
  '',
  '- [ ] 任务',
  '',
  '  任务正文',
  '  正文续行',
  '',
  '  ```',
  '  代码',
  '  ```',
  '',
  '  $$',
  '  a=b',
  '  $$',
  '',
  '  > 引用',
  '',
  '- 列表项',
  '',
  '| A | B |',
  '| - | - |',
  '| 1 | 2 |',
].join('\n');

it('只有顶层正文段落的首行带段首缩进，任务正文、列表、标题、引用、公式、代码和表格不受影响', () => {
  instance = new EditorController(document.body, { text, mode: 'todo', onChange: () => {} });
  instance.focusAt(text.length);
  const indented = [...instance.view.contentDOM.querySelectorAll<HTMLElement>('.cm-line.fm-paragraph')];
  expect(indented.map(line => line.textContent)).toEqual(['段落一']);
  expect(instance.view.contentDOM.querySelectorAll('.fm-list-line.fm-paragraph, .fm-heading.fm-paragraph, .fm-quote.fm-paragraph, .fm-code-line.fm-paragraph')).toHaveLength(0);
  expect(instance.text).toBe(text);
});

it('列表层级以两字为步长，任务正文不额外缩进', () => {
  const source = '- [ ] 父任务\n\n  父任务正文\n\n  - [ ] 子任务\n\n    子任务正文';
  instance = new EditorController(document.body, { text: source, mode: 'todo', onChange: () => {} });
  instance.focusAt(source.length);
  const lines = [...instance.view.contentDOM.querySelectorAll<HTMLElement>('.cm-line')];
  const margin = (label: string): string => lines.find(line => line.textContent === label)!.style.marginLeft;
  expect(margin('父任务')).toBe('calc(var(--editor-font-size, 16px) * 0)');
  expect(margin('父任务正文')).toBe('calc(var(--editor-font-size, 16px) * 2)');
  expect(margin('子任务')).toBe('calc(var(--editor-font-size, 16px) * 2)');
  expect(margin('子任务正文')).toBe('calc(var(--editor-font-size, 16px) * 4)');
  expect(instance.text).toBe(source);
});

it('源码视图保留原始对齐，不标记段首缩进', () => {
  instance = new EditorController(document.body, { text, mode: 'todo', onChange: () => {} });
  instance.setMode('source');
  expect(instance.view.contentDOM.querySelectorAll('.fm-paragraph')).toHaveLength(0);
  expect(instance.text).toBe(text);
});
