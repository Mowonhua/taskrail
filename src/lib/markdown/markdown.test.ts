/**
 * 文件职责：验证独立 Markdown 与 KaTeX 数学公式渲染器的语法、公式排版与 XSS 安全防护边界。
 * 定义范围：GFM 结构、CJK 友好删除线、行内 $...$ 与块级 $$...$$ 公式、未闭合/非法公式降级、协议白名单与原始 HTML 拦截。
 */
import { describe, expect, it } from 'vitest';
import { parser } from '@lezer/markdown';
import {
  markdownExtensions,
  renderMarkdown,
  renderMarkdownToHtml,
  renderMathToHtml,
  safeImageUrl,
  safeLinkUrl,
} from './index';

function render(source: string): HTMLDivElement {
  const container = document.createElement('div');
  container.append(renderMarkdown(source));
  return container;
}

describe('Markdown 与 KaTeX 语法扩展', () => {
  const mdParser = parser.configure(markdownExtensions);

  it('识别块级公式 MathBlock 与行内公式 InlineMath / InlineMathUnclosed，且不误伤代码块与转义美元符', () => {
    const source = '```md\n$not_math$\n```\n\n`$inline_code$` 与 \\$100\n\n行内 $x^2 + y^2 = z^2$ 公式\n\n未闭合 $a + b\n\n$$\n\\int_0^1 x^2 dx\n$$\n';
    const tree = mdParser.parse(source);
    const nodes: { name: string; text: string }[] = [];
    tree.iterate({
      enter: node => {
        if (['InlineMath', 'InlineMathUnclosed', 'MathBlock'].includes(node.name)) {
          nodes.push({ name: node.name, text: source.slice(node.from, node.to) });
        }
      },
    });
    expect(nodes).toEqual([
      { name: 'InlineMath', text: '$x^2 + y^2 = z^2$' },
      { name: 'InlineMathUnclosed', text: '$a + b' },
      { name: 'MathBlock', text: '$$\n\\int_0^1 x^2 dx\n$$' },
    ]);
  });

  it('支持中文标点紧邻的 CJK 友好删除线，同时保持空白边界不误判', () => {
    const html = renderMarkdownToHtml('前缀~~（已废弃方案）~~后缀 与 ~~ 不生效~~');
    expect(html).toContain('<del>（已废弃方案）</del>');
    expect(html).toContain('~~ 不生效~~');
  });
});

describe('Markdown 与 KaTeX 渲染器', () => {
  it('渲染 GFM 标题、任务列表、代码块、引用和对齐表格', () => {
    const container = render(
      '# 任务验收条件\n\n- [x] 通过 `cargo test`\n- [ ] 补充边界测试\n\n> 保持原子写入\n\n```rust\nfn main() {}\n```\n\n| 阶段 | 状态 |\n| :--- | ---: |\n| 冒烟 | 通过 |\n',
    );
    expect(container.querySelector('h1')?.textContent).toBe('任务验收条件');
    const checkboxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
    expect(checkboxes).toHaveLength(2);
    expect(checkboxes[0].checked).toBe(true);
    expect(checkboxes[0].disabled).toBe(true);
    expect(checkboxes[1].checked).toBe(false);
    expect(container.querySelector('blockquote p')?.textContent).toBe('保持原子写入');
    const code = container.querySelector('pre code') as HTMLElement | null;
    expect(code?.textContent).toBe('fn main() {}');
    expect(code?.dataset.language).toBe('rust');
    expect(container.querySelectorAll('table th')).toHaveLength(2);
    expect(container.querySelectorAll('table td')).toHaveLength(2);
  });

  it('正确渲染行内 $...$、多行/单行块级 $$...$$ 与引用/列表容器内的 KaTeX 数学公式', () => {
    const container = render(
      [
        '复杂度为 $O(n \\log n)$，满足：',
        '',
        '$$',
        '\\sum_{i=1}^{n} i = \\frac{n(n+1)}{2}',
        '$$',
        '',
        '$$x^2 + 1$$',
        '',
        '> $$',
        '> a^2 + b^2 = c^2',
        '> $$',
        '',
        '- 列表项：',
        '  $$',
        '  e^{i\\pi} + 1 = 0',
        '  $$',
      ].join('\n'),
    );
    const inlineMath = container.querySelector<HTMLElement>('.fm-math-inline');
    const blockMaths = container.querySelectorAll<HTMLElement>('.fm-math-block');
    expect(inlineMath).not.toBeNull();
    expect(inlineMath?.dataset.mathDisplay).toBe('inline');
    expect(inlineMath?.querySelector('.katex')).not.toBeNull();
    expect(blockMaths).toHaveLength(4);
    for (const blockMath of blockMaths) {
      expect(blockMath.dataset.mathDisplay).toBe('block');
      expect(blockMath.classList.contains('fm-math-error')).toBe(false);
      expect(blockMath.querySelector('.katex-display')).not.toBeNull();
    }
    expect(container.querySelector('blockquote .fm-math-block .katex-display')).not.toBeNull();
  });

  it('支持尖括号包裹的链接目标 [x](<https://example.com>) 正常解析为安全外链', () => {
    const container = render('[尖括号链接](<https://example.com/path(1)>) 与 [引用链接][ref]\n\n[ref]: <https://example.com/ref>');
    const links = [...container.querySelectorAll('a')].map(a => a.getAttribute('href'));
    expect(links).toEqual(['https://example.com/path(1)', 'https://example.com/ref']);
  });

  it('未闭合公式或非法 KaTeX 语法安全回退为纯文本错误提示，不抛异常也不注入 HTML', () => {
    const container = render(
      '未闭合 $x + 1\n\n非法行内 $\\invalidCommandXYZ{1}$ 结束\n\n$$\n\\unclosedBlock\n',
    );
    const errors = container.querySelectorAll<HTMLElement>('.fm-math-error');
    expect(errors.length).toBeGreaterThanOrEqual(3);
    expect(errors[0].textContent).toBe('$x + 1');
    expect(errors[1].textContent).toBe('$\\invalidCommandXYZ{1}$');
    expect(errors[2].textContent).toContain('\\unclosedBlock');
  });

  it('拦截原始 HTML、脚本注入与危险协议链接/图片（XSS 防护）', () => {
    const malicious = [
      '<script>window.__xss = true;</script>',
      '<img src="x" onerror="alert(1)">',
      '正文内嵌 <svg onload="alert(1)"></svg> 标签',
      '[危险脚本](javascript:alert(1))',
      '[危险数据](data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==)',
      '[本地文件](file:///C:/Windows/System32/cmd.exe)',
      '![恶意图片](javascript:alert(1))',
      '![SVG数据](data:image/svg+xml;utf8,<svg onload="alert(1)"/>)',
      '[安全外链](https://example.com/bounty/1)',
      '[安全邮件](mailto:agent@example.com)',
      '![安全PNG](data:image/png;base64,AAAA)',
    ].join('\n\n');

    const container = render(malicious);
    expect(container.querySelector('script')).toBeNull();
    expect(container.querySelector('svg')).toBeNull();
    expect(container.innerHTML).not.toContain('onerror');
    expect(container.innerHTML).not.toContain('onload');
    expect(container.innerHTML).not.toContain('javascript:');
    expect(container.innerHTML).not.toContain('data:text/html');
    expect(container.innerHTML).not.toContain('file:///');

    const links = [...container.querySelectorAll('a')].map(a => a.getAttribute('href'));
    expect(links).toEqual(['https://example.com/bounty/1', 'mailto:agent@example.com']);

    const images = [...container.querySelectorAll('img')].map(img => img.getAttribute('src'));
    expect(images).toEqual(['data:image/png;base64,AAAA']);

    expect(safeLinkUrl('https://example.com')).toBe('https://example.com');
    expect(safeLinkUrl('javascript:alert(1)')).toBeNull();
    expect(safeImageUrl('https://example.com/pic.webp')).toBe('https://example.com/pic.webp');
    expect(safeImageUrl('data:image/svg+xml;base64,AAAA')).toBeNull();
  });

  it('KaTeX 禁用 trust 宏与危险命令（如 \\href{javascript:...} 不生成可执行链接）', () => {
    const hrefResult = renderMathToHtml('\\href{javascript:alert(1)}{click}', false);
    if (hrefResult.ok) {
      const box = document.createElement('div');
      box.innerHTML = hrefResult.html;
      expect(box.querySelector('a')).toBeNull();
    } else {
      expect(hrefResult.error).toBeTruthy();
    }
  });
});
