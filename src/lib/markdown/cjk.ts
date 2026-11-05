/**
 * 文件职责：按 CJK 友好规则解析删除线定界符。
 * 定义范围：相邻字符分类、定界符两侧判定及替换 GFM 删除线解析器；不修改原文和坐标。
 * 依据：CommonMark 的 CJK 友好修订，中日韩标点按正文处理，不再阻断行内语法。
 * https://github.com/tats-u/markdown-cjk-friendly/blob/main/specification.md
 * 约束条件：只改删除线；强调与加粗仍按 CommonMark 语义解析。
 */
import type { InlineContext, MarkdownConfig } from '@lezer/markdown';

/** 相邻字符类别；CJK 字符与 CJK 标点都算正文，只有非 CJK 标点参与定界符判定。 */
interface CharacterClass { space: boolean; cjk: boolean; nonCjkPunctuation: boolean }

/** CommonMark 的 Unicode 空白包含制表符、换行、换页、回车和 Zs 类，与 JS 的 \s 不完全一致。 */
const whitespace = /[\t\n\f\r\p{Zs}]/u;
/** CommonMark 的 Unicode 标点字符包含 P（标点）与 S（符号）两个类别。 */
const punctuation = /[\p{P}\p{S}]/u;

/**
 * 规范定义的 CJK 字符范围：东亚宽度 W/F/H 且非默认表情呈现，或文字属性为 Hangul。
 * 取自同一规范的参考实现（Unicode 18）；未分配码位的东亚宽度按 W 处理。
 */
const cjkRanges: readonly (readonly [number, number])[] = [
  [0x1100, 0x11ff], [0x20a9, 0x20a9], [0x2329, 0x232a], [0x2630, 0x2637], [0x268a, 0x268f],
  [0x2e80, 0x2e99], [0x2e9b, 0x2ef3], [0x2f00, 0x2fd5], [0x2ff0, 0x303e], [0x3041, 0x3096],
  [0x3099, 0x30ff], [0x3105, 0x312f], [0x3131, 0x318e], [0x3190, 0x31e5], [0x31ef, 0x321e],
  [0x3220, 0x3247], [0x3250, 0xa48c], [0xa490, 0xa4c6], [0xa960, 0xa97c], [0xac00, 0xd7a3],
  [0xd7b0, 0xd7c6], [0xd7cb, 0xd7fb], [0xf900, 0xfaff], [0xfe10, 0xfe19], [0xfe30, 0xfe52],
  [0xfe54, 0xfe66], [0xfe68, 0xfe6b], [0xff01, 0xffbe], [0xffc2, 0xffc7], [0xffca, 0xffcf],
  [0xffd2, 0xffd7], [0xffda, 0xffdc], [0xffe0, 0xffe6], [0xffe8, 0xffee], [0x16fe0, 0x16fe4],
  [0x16ff0, 0x16ff6], [0x17000, 0x18cda], [0x18cff, 0x18d20], [0x18d80, 0x18df2], [0x18e00, 0x19191],
  [0x191a0, 0x191d2], [0x1aff0, 0x1aff3], [0x1aff5, 0x1affb], [0x1affd, 0x1affe], [0x1b000, 0x1b128],
  [0x1b132, 0x1b132], [0x1b150, 0x1b152], [0x1b155, 0x1b155], [0x1b164, 0x1b168], [0x1b170, 0x1b2fb],
  [0x1d300, 0x1d356], [0x1d360, 0x1d376], [0x1f1ae, 0x1f1ae], [0x1f200, 0x1f200], [0x1f202, 0x1f202],
  [0x1f210, 0x1f219], [0x1f21b, 0x1f22e], [0x1f230, 0x1f231], [0x1f237, 0x1f237], [0x1f23b, 0x1f23b],
  [0x1f240, 0x1f248], [0x1f260, 0x1f265], [0x1f7da, 0x1f7da], [0x20000, 0x3fffd],
];

function isCjk(code: number): boolean {
  let low = 0; let high = cjkRanges.length - 1;
  while (low <= high) {
    const middle = (low + high) >>> 1, [from, to] = cjkRanges[middle];
    if (code < from) high = middle - 1;
    else if (code > to) low = middle + 1;
    else return true;
  }
  return false;
}

/** 表意文字变体选择符本身不是独立字符，判定时按 CJK 处理。 */
const isIdeographicVariationSelector = (code: number): boolean => code >= 0xe0100 && code <= 0xe01ef;
/** 非表情变体选择符跟随前一主字符，判定时跳过并回看主字符。 */
const isNonEmojiVariationSelector = (code: number): boolean => code >= 0xfe00 && code <= 0xfe0e;

/** 返回文档位置处的完整码点；越界返回 -1，代理对按一个字符读取。 */
function codePointAt(cx: InlineContext, position: number): number {
  if (position < cx.offset || position >= cx.end) return -1;
  const unit = cx.char(position);
  if (unit >= 0xd800 && unit <= 0xdbff) {
    const low = cx.char(position + 1);
    if (low >= 0xdc00 && low <= 0xdfff) return 0x10000 + ((unit - 0xd800) << 10) + (low - 0xdc00);
  }
  return unit;
}

/** 返回文档位置前一个完整码点；位于行内片段起点时返回 -1。 */
function codePointBefore(cx: InlineContext, position: number): number {
  const unit = codePointAt(cx, position - 1);
  if (unit >= 0xdc00 && unit <= 0xdfff) {
    const high = codePointAt(cx, position - 2);
    if (high >= 0xd800 && high <= 0xdbff) return 0x10000 + ((high - 0xd800) << 10) + (unit - 0xdc00);
  }
  return unit;
}

/** 解析位置之前的有效字符：跳过紧随的非表情变体选择符，越界视为行首空白。 */
function precedingCharacter(cx: InlineContext, position: number): CharacterClass {
  let code = codePointBefore(cx, position);
  while (isNonEmojiVariationSelector(code)) code = codePointBefore(cx, position -= 1);
  return classify(code);
}

/** 解析位置之后的有效字符：跳过紧随的非表情变体选择符，越界视为行尾空白。 */
function followingCharacter(cx: InlineContext, position: number): CharacterClass {
  let code = codePointAt(cx, position);
  while (isNonEmojiVariationSelector(code)) code = codePointAt(cx, position += 1);
  return classify(code);
}

function classify(code: number): CharacterClass {
  if (code < 0) return { space: true, cjk: false, nonCjkPunctuation: false };
  if (isIdeographicVariationSelector(code)) return { space: false, cjk: true, nonCjkPunctuation: false };
  const char = String.fromCodePoint(code);
  if (whitespace.test(char)) return { space: true, cjk: false, nonCjkPunctuation: false };
  const cjk = isCjk(code);
  return { space: false, cjk, nonCjkPunctuation: !cjk && punctuation.test(char) };
}

/**
 * 函数职责：判定 from..to 处的定界符能否开启、闭合删除线。
 * 输入说明：from..to 是完整定界符范围；行首行尾按 Unicode 空白参与判定。
 * 输出说明：返回两侧能力，调用方原样交给 addDelimiter，不改写文档。
 * 实现思路：左侧要求定界符后不是空白或非 CJK 标点，右侧要求定界符前不是空白或非 CJK 标点；
 * 相邻一侧是非 CJK 标点时，另一侧是空白、非 CJK 标点或 CJK 字符即可成立。
 */
function delimiterSides(cx: InlineContext, from: number, to: number): { open: boolean; close: boolean } {
  const before = precedingCharacter(cx, from);
  const after = followingCharacter(cx, to);
  const left = !after.space && (!after.nonCjkPunctuation || before.space || before.nonCjkPunctuation || before.cjk);
  const right = !before.space && (!before.nonCjkPunctuation || after.space || after.nonCjkPunctuation || after.cjk);
  return { open: left, close: right };
}

/**
 * 结构职责：替换 GFM 删除线解析器，改用 CJK 友好的两侧判定。
 * 约束条件：节点名与 GFM 相同，因此必须排在 GFM 之后配置；解析器按名称原位替换。
 */
const delimiter = { resolve: 'Strikethrough', mark: 'StrikethroughMark' };
const strikethrough: MarkdownConfig = {
  parseInline: [{
    name: 'Strikethrough',
    parse(cx, next, pos) {
      // 与 GFM 一致：只接受恰好两个波浪线；第三个紧随其后时该段不是定界符。
      if (next !== 126 || cx.char(pos + 1) !== 126 || cx.char(pos + 2) === 126) return -1;
      const { open, close } = delimiterSides(cx, pos, pos + 2);
      return cx.addDelimiter(delimiter, pos, pos + 2, open, close);
    },
  }],
};

/**
 * 函数职责：提供中日韩正文可用的删除线解析扩展。
 * 输出说明：返回可直接并入 MarkdownExtension 的配置，须与 GFM 一起使用。
 */
export function cjkStrikethrough(): MarkdownConfig { return strikethrough; }
