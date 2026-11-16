/**
 * 文件职责：提供独立的只读 Markdown 与 KaTeX 数学公式安全渲染器。
 * 定义范围：GFM 块级与行内语法、CJK 友好删除线、行内 $...$ 与块级 $$...$$ 数学公式、协议白名单与原始 HTML 拦截。
 */
import 'katex/dist/katex.min.css';
import katex from 'katex';
import { parser } from '@lezer/markdown';
import type { SyntaxNode } from '@lezer/common';
import { translate } from '../i18n';
import { markdownExtensions } from './syntax';

const markdownParser = parser.configure(markdownExtensions);

/** 引用式链接定义表；标签做空白归一并忽略大小写。 */
type LinkReferences = ReadonlyMap<string, { url: string; title: string }>;

/** 行内语法到元素标签的映射；其余节点为语法标记或原始 HTML，不直接输出。 */
const inlineTags: Record<string, string> = {
  Emphasis: 'em',
  StrongEmphasis: 'strong',
  Strikethrough: 'del',
  InlineCode: 'code',
};

const entityCache = new Map<string, string>();

/** 只把已匹配的字符引用交给 DOM 解码；链接目标中的反斜杠转义一并还原。 */
function decodeText(value: string): string {
  return value
    .replace(/\\([\x21-\x2f\x3a-\x40\x5b-\x60\x7b-\x7e])/g, '$1')
    .replace(/&(?:#[xX][\da-fA-F]+|#\d+|[a-zA-Z][a-zA-Z\d]*);/g, entity => {
      const cached = entityCache.get(entity);
      if (cached !== undefined) return cached;
      const decoder = document.createElement('textarea');
      decoder.innerHTML = entity;
      if (entityCache.size >= 128) entityCache.delete(entityCache.keys().next().value!);
      entityCache.set(entity, decoder.value);
      return decoder.value;
    });
}

function normalizeLabel(label: string): string {
  return label.trim().replace(/[ \t\r\n]+/g, ' ').toUpperCase().toLowerCase();
}

/** 链接只放行 http(s) 与非空 mailto，拒绝 javascript:/data:/vbscript:/file: 等不安全协议。 */
export function safeLinkUrl(value: string): string | null {
  const clean = value.trim();
  if (!clean || /[\u0000-\u001f\u007f]/.test(clean)) return null;
  return /^(https?:\/\/|mailto:\S)/i.test(clean) ? clean : null;
}

/** 图片目标放行 http(s) 与常见位图类 data URL，拦截 svg/html data URL 与脚本协议。 */
export function safeImageUrl(value: string): string | null {
  const clean = value.trim();
  if (!clean || /[\u0000-\u001f\u007f]/.test(clean)) return null;
  return /^(https?:\/\/|data:image\/(?:png|jpeg|gif|webp);base64,)/i.test(clean) ? clean : null;
}

/**
 * 函数职责：使用 KaTeX 安全渲染单个数学公式表达式。
 * 安全约束：固定 trust: false、maxExpand: 500，失败时返回错误原因，由调用方以纯文本回退。
 */
export function renderMathToHtml(
  expression: string,
  displayMode = false,
): { ok: true; html: string } | { ok: false; error: string } {
  try {
    const html = katex.renderToString(expression, {
      displayMode,
      throwOnError: true,
      trust: false,
      strict: 'warn',
      maxExpand: 500,
    });
    return { ok: true, html };
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : translate('公式无法排版'),
    };
  }
}

/**
 * 函数职责：把 Markdown 源码（含 GFM 与行内/块级 KaTeX 公式）渲染为安全的只读 DOM 片段。
 * 输出说明：源文本一律经 textNode 写入，原始 HTML 节点不透传，仅 KaTeX 产出的受控数学标记写入公式容器。
 */
export function renderMarkdown(source: string): DocumentFragment {
  const fragment = document.createDocumentFragment();
  const normalized = source.replace(/\r\n?/g, '\n');
  const tree = markdownParser.parse(normalized);
  const references = collectReferences(tree.topNode, normalized);
  for (let node = tree.topNode.firstChild; node; node = node.nextSibling) {
    const rendered = renderBlock(node, normalized, references);
    if (rendered) fragment.append(rendered);
  }
  return fragment;
}

/**
 * 函数职责：把 Markdown 源码渲染为安全的 HTML 字符串。
 * 适用场景：测试断言或只读容器快照序列化。
 */
export function renderMarkdownToHtml(source: string): string {
  const container = document.createElement('div');
  container.append(renderMarkdown(source));
  return container.innerHTML;
}

function stripAngleBrackets(raw: string): string {
  const trimmed = raw.trim();
  return trimmed.startsWith('<') && trimmed.endsWith('>') ? trimmed.slice(1, -1) : raw;
}

function collectReferences(root: SyntaxNode, source: string): LinkReferences {
  const references = new Map<string, { url: string; title: string }>();
  for (let node = root.firstChild; node; node = node.nextSibling) {
    if (node.name !== 'LinkReference') continue;
    const label = node.getChild('LinkLabel');
    const url = node.getChild('URL');
    if (!label || !url) continue;
    const key = normalizeLabel(source.slice(label.from + 1, label.to - 1));
    if (!references.has(key)) {
      const title = node.getChild('LinkTitle');
      references.set(key, {
        url: decodeText(stripAngleBrackets(source.slice(url.from, url.to))),
        title: title ? decodeText(source.slice(title.from + 1, title.to - 1)) : '',
      });
    }
  }
  return references;
}

function linkDestination(node: SyntaxNode, source: string, references: LinkReferences, label: string): string {
  const url = node.getChild('URL');
  if (url) return decodeText(stripAngleBrackets(source.slice(url.from, url.to)));
  const reference = node.getChild('LinkLabel');
  return references.get(normalizeLabel(reference ? source.slice(reference.from + 1, reference.to - 1) : label))?.url ?? '';
}

function element(name: string, content?: Node): HTMLElement {
  const node = document.createElement(name);
  if (content) node.append(content);
  return node;
}

function trimEdges(fragment: DocumentFragment): DocumentFragment {
  const trim = (child: ChildNode | null, side: 'start' | 'end'): void => {
    while (child instanceof Text) {
      const trimmed = side === 'start' ? child.data.replace(/^\s+/, '') : child.data.replace(/\s+$/, '');
      if (trimmed) {
        child.data = trimmed;
        return;
      }
      child.remove();
      child = side === 'start' ? fragment.firstChild : fragment.lastChild;
    }
  };
  trim(fragment.firstChild, 'start');
  trim(fragment.lastChild, 'end');
  return fragment;
}

function renderBlock(node: SyntaxNode, source: string, references: LinkReferences): Node | null {
  const heading = /^(?:ATX|Setext)Heading(\d)$/.exec(node.name);
  if (heading) return element(`h${heading[1]}`, trimEdges(renderInline(node, source, references)));
  if (node.name === 'Paragraph') return element('p', trimEdges(renderInline(node, source, references)));
  if (node.name === 'BulletList' || node.name === 'OrderedList') return renderList(node, source, references);
  if (node.name === 'Blockquote') {
    const quote = element('blockquote');
    for (let child = node.firstChild; child; child = child.nextSibling) {
      const rendered = renderBlock(child, source, references);
      if (rendered) quote.append(rendered);
    }
    return quote;
  }
  if (node.name === 'FencedCode' || node.name === 'CodeBlock') return renderCode(node, source);
  if (node.name === 'MathBlock') return renderMathBlock(node, source);
  if (node.name === 'Table') return renderTable(node, source, references);
  if (node.name === 'HorizontalRule') return document.createElement('hr');
  // LinkReference 属于引用定义，HTMLBlock 属于原始 HTML；两者都不作为可见内容输出。
  return null;
}

function renderMathBlock(node: SyntaxNode, source: string): HTMLElement {
  const raw = source.slice(node.from, node.to);
  const strippedLines = raw
    .split('\n')
    .map(line => line.replace(/^[ \t]*(?:>[ \t]?)*/, ''));
  let closed = false;
  let expression = '';
  if (strippedLines.length === 1) {
    const single = strippedLines[0].trim();
    if (single.length > 3 && single.startsWith('$$') && single.endsWith('$$')) {
      closed = true;
      expression = single.slice(2, -2).trim();
    }
  } else if (strippedLines.length > 1) {
    const first = strippedLines[0].trim();
    const last = strippedLines[strippedLines.length - 1].trim();
    if (first === '$$' && last === '$$') {
      closed = true;
      expression = strippedLines.slice(1, -1).join('\n').trim();
    }
  }
  const block = document.createElement('div');
  block.className = 'fm-math-block';
  block.dataset.mathDisplay = 'block';
  if (!closed) {
    block.classList.add('fm-math-error');
    block.title = translate('公式尚未闭合');
    block.textContent = raw;
    return block;
  }
  if (!expression) {
    block.classList.add('fm-math-empty');
    block.textContent = translate('空公式块');
    return block;
  }
  const rendered = renderMathToHtml(expression, true);
  if (rendered.ok) {
    block.innerHTML = rendered.html;
  } else {
    block.classList.add('fm-math-error');
    block.title = rendered.error;
    block.textContent = expression;
  }
  return block;
}

function renderList(node: SyntaxNode, source: string, references: LinkReferences): HTMLElement {
  const list = element(node.name === 'OrderedList' ? 'ol' : 'ul');
  const tight = listIsTight(node, source);
  for (let child = node.firstChild; child; child = child.nextSibling) {
    if (child.name === 'ListItem') list.append(renderListItem(child, source, references, tight));
  }
  return list;
}

function hasBlankLine(source: string, from: number, to: number): boolean {
  return /\n[ \t]*\n/.test(source.slice(from, to));
}

function listIsTight(list: SyntaxNode, source: string): boolean {
  for (let item = list.firstChild; item; item = item.nextSibling) {
    if (item.name !== 'ListItem') continue;
    for (let child = item.firstChild; child; child = child.nextSibling) {
      if (child.name === 'Paragraph' && child.prevSibling && hasBlankLine(source, child.prevSibling.to, child.from)) {
        return false;
      }
    }
    const next = item.nextSibling;
    if (next && hasBlankLine(source, item.to, next.from)) return false;
  }
  return true;
}

function renderListItem(item: SyntaxNode, source: string, references: LinkReferences, tight: boolean): HTMLElement {
  const itemElement = element('li');
  for (let child = item.firstChild; child; child = child.nextSibling) {
    if (child.name === 'ListMark') continue;
    if (child.name === 'Task') {
      const marker = child.getChild('TaskMarker');
      if (marker) {
        const checkbox = document.createElement('input');
        checkbox.type = 'checkbox';
        checkbox.disabled = true;
        checkbox.checked = source.slice(marker.from + 1, marker.to - 1).toLowerCase() === 'x';
        itemElement.append(checkbox, trimEdges(renderInline(child, source, references, marker.to, child.to)));
      } else {
        itemElement.append(renderInline(child, source, references));
      }
      continue;
    }
    if (child.name === 'Paragraph' && tight) {
      itemElement.append(trimEdges(renderInline(child, source, references)));
      continue;
    }
    const rendered = renderBlock(child, source, references);
    if (rendered) itemElement.append(rendered);
  }
  return itemElement;
}

function renderCode(node: SyntaxNode, source: string): HTMLElement | null {
  const lines = source.slice(node.from, node.to).split('\n');
  let language = '';
  if (node.name === 'FencedCode') {
    const info = node.getChild('CodeInfo');
    if (info) language = source.slice(info.from, info.to).trim();
    lines.shift();
    if (lines.length && /^(`{3,}|~{3,})[ \t]*$/.test(lines[lines.length - 1].trim())) lines.pop();
  } else {
    for (let index = 0; index < lines.length; index++) lines[index] = lines[index].replace(/^ {1,4}/, '');
  }
  while (lines.length && !lines[0].trim()) lines.shift();
  while (lines.length && !lines[lines.length - 1].trim()) lines.pop();
  if (!lines.length) return null;
  const code = element('code', document.createTextNode(lines.join('\n')));
  if (language) code.dataset.language = language;
  return element('pre', code);
}

function renderTable(node: SyntaxNode, source: string, references: LinkReferences): HTMLElement {
  const columnsOf = (row: SyntaxNode): (SyntaxNode | null)[] => {
    const cells: (SyntaxNode | null)[] = [];
    let pending: SyntaxNode | null = null;
    for (let child = row.firstChild; child; child = child.nextSibling) {
      if (child.name === 'TableCell') pending = child;
      if (child.name === 'TableDelimiter' && child.from !== row.from) {
        cells.push(pending);
        pending = null;
      }
    }
    if (pending || row.lastChild?.name !== 'TableDelimiter') cells.push(pending);
    return cells;
  };
  const header = node.getChild('TableHeader');
  const columns = header ? columnsOf(header).length : 0;
  const table = element('table');
  if (!columns || !header) return table;
  const delimiter = node.getChild('TableDelimiter');
  const alignment = delimiter
    ? source
        .slice(delimiter.from, delimiter.to)
        .split('|')
        .map(part => part.trim())
        .filter(Boolean)
        .map(part =>
          part.startsWith(':') && part.endsWith(':')
            ? 'center'
            : part.endsWith(':')
              ? 'right'
              : part.startsWith(':')
                ? 'left'
                : '',
        )
    : [];
  const fill = (row: SyntaxNode, name: 'th' | 'td', parent: HTMLElement): void => {
    const cells = columnsOf(row);
    const rowElement = element('tr');
    for (let column = 0; column < columns; column++) {
      const cellElement = element(name);
      const cell = cells[column];
      if (cell) cellElement.append(trimEdges(renderInline(cell, source, references)));
      if (alignment[column]) cellElement.style.textAlign = alignment[column];
      rowElement.append(cellElement);
    }
    parent.append(rowElement);
  };
  const head = element('thead');
  fill(header, 'th', head);
  table.append(head);
  const body = element('tbody');
  for (let child = node.firstChild; child; child = child.nextSibling) {
    if (child.name === 'TableRow') fill(child, 'td', body);
  }
  if (body.hasChildNodes()) table.append(body);
  return table;
}

function renderInlineMath(raw: string, closed: boolean): HTMLElement {
  const span = document.createElement('span');
  span.className = 'fm-math-inline';
  span.dataset.mathDisplay = 'inline';
  if (!closed) {
    span.classList.add('fm-math-error');
    span.title = translate('公式尚未闭合，请补充 $');
    span.textContent = raw;
    return span;
  }
  const expression = raw.slice(1, -1).trim();
  const rendered = renderMathToHtml(expression, false);
  if (rendered.ok) {
    span.innerHTML = rendered.html;
  } else {
    span.classList.add('fm-math-error');
    span.title = rendered.error;
    span.textContent = raw;
  }
  return span;
}

function renderInline(
  parent: SyntaxNode,
  source: string,
  references: LinkReferences,
  from = parent.from,
  to = parent.to,
): DocumentFragment {
  const fragment = document.createDocumentFragment();
  const end = Math.min(to, parent.to);
  let position = Math.max(from, parent.from);
  for (let child = parent.firstChild; child; child = child.nextSibling) {
    if (child.to <= position) continue;
    if (child.from >= end) break;
    if (child.from > position) fragment.append(document.createTextNode(source.slice(position, child.from)));
    const raw = source.slice(child.from, child.to);
    const tag = inlineTags[child.name];
    if (tag) {
      fragment.append(element(tag, renderInline(child, source, references)));
    } else if (child.name === 'InlineMath') {
      fragment.append(renderInlineMath(raw, true));
    } else if (child.name === 'InlineMathUnclosed') {
      fragment.append(renderInlineMath(raw, false));
    } else if (child.name === 'Link' || child.name === 'Image' || child.name === 'Autolink' || child.name === 'URL') {
      fragment.append(renderLinkish(child, source, references));
    } else if (child.name === 'Escape') {
      fragment.append(document.createTextNode(raw.slice(1)));
    } else if (child.name === 'Entity') {
      fragment.append(document.createTextNode(decodeText(raw)));
    } else if (child.name === 'HardBreak') {
      fragment.append(document.createElement('br'));
    }
    position = child.to;
  }
  if (position < end) fragment.append(document.createTextNode(source.slice(position, end)));
  return fragment;
}

function renderLinkish(node: SyntaxNode, source: string, references: LinkReferences): Node {
  if (node.name === 'URL') return renderAutoLink(source.slice(node.from, node.to));
  if (node.name === 'Autolink') {
    const url = node.getChild('URL');
    return renderAutoLink(url ? source.slice(url.from, url.to) : source.slice(node.from, node.to));
  }
  const opening = node.firstChild;
  let closing = opening?.nextSibling ?? null;
  while (closing && !(closing.name === 'LinkMark' && source.slice(closing.from, closing.to) === ']')) {
    closing = closing.nextSibling;
  }
  if (!opening || !closing) return document.createTextNode(source.slice(node.from, node.to));
  const label = renderInline(node, source, references, opening.to, closing.from);
  const destination = linkDestination(node, source, references, source.slice(opening.to, closing.from));
  if (node.name === 'Image') {
    const clean = safeImageUrl(destination);
    if (!clean) return label;
    const image = document.createElement('img');
    image.src = clean;
    image.alt = source.slice(opening.to, closing.from);
    image.loading = 'lazy';
    return image;
  }
  const clean = safeLinkUrl(destination);
  if (!clean) return label;
  const link = element('a', label);
  link.setAttribute('href', clean);
  link.setAttribute('target', '_blank');
  link.setAttribute('rel', 'noopener noreferrer');
  return link;
}

function renderAutoLink(label: string): Node {
  const destination = /^www\./i.test(label)
    ? `http://${label}`
    : !/^[a-z][a-z\d+.-]*:/i.test(label) && /^[^\s@]+@[^\s@]+$/.test(label)
      ? `mailto:${label}`
      : label;
  const clean = safeLinkUrl(destination);
  if (!clean) return document.createTextNode(label);
  const link = element('a', document.createTextNode(label));
  link.setAttribute('href', clean);
  link.setAttribute('target', '_blank');
  link.setAttribute('rel', 'noopener noreferrer');
  return link;
}
