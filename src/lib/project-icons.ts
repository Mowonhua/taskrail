/**
 * 文件职责：定义项目图标的引用格式、推荐表情与解析规则。
 * 定义范围：Unicode 表情与 Lucide 图标引用的规范化和序列化；不加载图标数据，也不渲染。
 */

/** 结构职责：区分可直接显示的表情字素与需要图标数据的 Lucide 名称。 */
export type ProjectIcon =
  | { kind: 'emoji'; value: string }
  | { kind: 'lucide'; name: string };

/** 推荐表情覆盖常见项目主题，作为选择器的默认入口；真实名称和标签来自本地表情数据。 */
export const recommendedEmojis = [
  '💼', '🎯', '📊', '📈', '🗂️', '📁', '📝', '✅',
  '💻', '🐛', '⚙️', '🧪', '🚀', '🗺️', '🔧', '📦',
  '📚', '📖', '✏️', '🎓', '🧠', '🔬', '💡', '🧩',
  '🏠', '🌱', '☕', '🛒', '🧺', '🎨', '✨', '🎬',
  '🎵', '📷', '🏃', '🧘', '🥗', '💤', '💪', '🩺',
  '🗓️', '📌', '✈️', '🧳', '⛰️', '🏆', '💰', '💬',
] as const;

const EMOJI_PREFIX = 'emoji:';
const LUCIDE_PREFIX = 'lucide:';
/** Iconify 名称只含小写字母、数字和连字符；长度上限防止手工配置写入异常长值。 */
const LUCIDE_NAME = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const MAX_LUCIDE_NAME_LENGTH = 64;

const graphemeSegmenter = typeof Intl.Segmenter === 'function'
  ? new Intl.Segmenter(undefined, { granularity: 'grapheme' })
  : null;

/**
 * 函数职责：把自定义输入规范为单个表情或字符字素。
 * 输入说明：值来自自定义输入；允许首尾空白、组合表情和任意单字素。
 * 输出说明：返回截取后的单个字素；空值、多个字素、控制字符或超长序列返回 null。
 * 实现思路：按字素而非码点切分，保证肤色、旗帜和 ZWJ 组合表情不会拆散；拒绝没有可见字形的值。
 */
export function normalizeProjectEmoji(value: unknown): string | null {
  if (typeof value !== 'string') return null;
  const text = value.trim();
  if (!text) return null;
  const parts = graphemeSegmenter
    ? Array.from(graphemeSegmenter.segment(text), part => part.segment)
    : Array.from(text);
  if (parts.length !== 1) return null;
  const emoji = parts[0];
  // 组合表情可以包含格式字符，但不能含换行和控制字符；上限防止手工配置塞入超长字素。
  if (emoji.length > 64 || /[\p{Cc}\p{Zl}\p{Zp}]/u.test(emoji)) return null;
  if (!/[\p{L}\p{N}\p{P}\p{S}]/u.test(emoji)) return null;
  return emoji;
}

/**
 * 函数职责：把配置中的图标值解析为表情或 Lucide 引用。
 * 输入说明：配置只接受 `emoji:表情` 或 `lucide:名称`，两类图标共用同一前缀格式。
 * 输出说明：形状合法返回引用；裸表情、空值、多字素、非法名称或非字符串返回 null。
 */
export function parseProjectIcon(value: unknown): ProjectIcon | null {
  if (typeof value !== 'string') return null;
  const text = value.trim();
  if (text.startsWith(EMOJI_PREFIX)) {
    const emoji = normalizeProjectEmoji(text.slice(EMOJI_PREFIX.length));
    return emoji ? { kind: 'emoji', value: emoji } : null;
  }
  if (text.startsWith(LUCIDE_PREFIX)) {
    const name = text.slice(LUCIDE_PREFIX.length);
    return name.length <= MAX_LUCIDE_NAME_LENGTH && LUCIDE_NAME.test(name) ? { kind: 'lucide', name } : null;
  }
  return null;
}

/** 表情和图标都写入来源前缀，选择器和配置校验共用同一序列化结果作为稳定键。 */
export function serializeProjectIcon(icon: ProjectIcon): string {
  return icon.kind === 'emoji' ? `${EMOJI_PREFIX}${icon.value}` : `${LUCIDE_PREFIX}${icon.name}`;
}
