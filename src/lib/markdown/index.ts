/**
 * 文件职责：公开 Markdown 与 KaTeX 数学公式解析及安全渲染模块边界。
 * 定义范围：GFM + CJK 删除线 + 数学公式语法扩展，以及供悬赏详情、验收条件与评论区复用的只读安全渲染器。
 */
export { cjkStrikethrough } from './cjk';
export { markdownExtensions, mathExtension } from './syntax';
export {
  renderMarkdown,
  renderMarkdownToHtml,
  renderMathToHtml,
  safeImageUrl,
  safeLinkUrl,
} from './render';
