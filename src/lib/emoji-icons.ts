/**
 * 文件职责：按界面语言懒加载 Unicode 表情数据，并提供分类和关键词搜索。
 * 定义范围：emojibase 数据解析、分类组织与搜索；不渲染界面，也不涉及配置。
 */
import type { Locale } from './i18n';
import { recommendedEmojis } from './project-icons';

export interface EmojiEntry {
  group?: number;
  hexcode: string;
  label: string;
  order?: number;
  tags?: string[];
  unicode: string;
}

export interface EmojiCategory {
  id: string;
  label: string;
  order: number;
  emojis: EmojiEntry[];
}

export interface EmojiDataset {
  entries: EmojiEntry[];
  categories: EmojiCategory[];
  recommended: EmojiEntry[];
}

export interface EmojiSearchResult {
  items: EmojiEntry[];
  total: number;
}

interface Messages {
  groups: { key: string; message: string; order: number }[];
}

const cache = new Map<Locale, Promise<EmojiDataset>>();

/**
 * 函数职责：加载并缓存当前语言的表情全集。
 * 输入说明：locale 只区分受支持的简体中文和英文；数据包随构建产物本地分发。
 * 输出说明：返回已排序的根表情、分类和推荐集合；加载失败由调用方重试。
 * 实现思路：按语言动态导入 emojibase 的紧凑数据和分类文案，避免启动时加载整表。
 */
export function loadEmojiDataset(locale: Locale): Promise<EmojiDataset> {
  let pending = cache.get(locale);
  if (!pending) {
    pending = load(locale); cache.set(locale, pending);
    // 失败不保留失败 Promise，选择器重试时可以重新发起加载。
    pending.catch(() => cache.delete(locale));
  }
  return pending;
}

async function load(locale: Locale): Promise<EmojiDataset> {
  const [compactModule, messagesModule] = locale === 'en'
    ? await Promise.all([import('emojibase-data/en/compact.json?raw'), import('emojibase-data/en/messages.json?raw')])
    : await Promise.all([import('emojibase-data/zh/compact.json?raw'), import('emojibase-data/zh/messages.json?raw')]);
  const entries = (JSON.parse(compactModule.default) as EmojiEntry[])
    .filter(entry => entry.group !== undefined && entry.group !== 2 && !!entry.unicode)
    .sort((left, right) => (left.order ?? 0) - (right.order ?? 0));
  const messages = JSON.parse(messagesModule.default) as Messages;
  const categories = messages.groups
    .filter(group => entries.some(entry => entry.group === group.order))
    .sort((left, right) => left.order - right.order)
    .map(group => ({
      id: group.key, label: group.message, order: group.order,
      emojis: entries.filter(entry => entry.group === group.order),
    }));
  const byUnicode = new Map(entries.map(entry => [entry.unicode, entry]));
  const recommended = recommendedEmojis
    .map(emoji => byUnicode.get(emoji))
    .filter((entry): entry is EmojiEntry => !!entry);
  return { entries, categories, recommended };
}

/** 当前表情所在分类；自定义输入的新表情可能不在数据集内，由调用方回退到推荐分类。 */
export function emojiCategoryFor(dataset: EmojiDataset, unicode: string): EmojiCategory | undefined {
  return dataset.categories.find(category => category.emojis.some(entry => entry.unicode === unicode));
}

/**
 * 函数职责：在 locale 的名称和标签中搜索表情。
 * 输入说明：query 为用户输入，允许大小写和首尾空白；limit 控制单次渲染上限。
 * 输出说明：按精确名称、名称前缀、名称包含、标签命中排序，并返回截断前总数。
 * 实现思路：名称优先级高于标签，同分保持 Unicode 数据顺序。
 */
export function searchEmojis(dataset: EmojiDataset, query: string, limit = 240): EmojiSearchResult {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return { items: [], total: 0 };
  const ranked: { entry: EmojiEntry; score: number; order: number }[] = [];
  for (const entry of dataset.entries) {
    const label = entry.label.toLowerCase();
    const tags = entry.tags?.map(tag => tag.toLowerCase()) ?? [];
    let score = 0;
    if (label === normalized) score = 4;
    else if (label.startsWith(normalized)) score = 3;
    else if (label.includes(normalized)) score = 2;
    else if (tags.some(tag => tag === normalized)) score = 2;
    else if (tags.some(tag => tag.startsWith(normalized))) score = 1;
    else if (tags.some(tag => tag.includes(normalized))) score = 0.5;
    if (score) ranked.push({ entry, score, order: entry.order ?? 0 });
  }
  ranked.sort((left, right) => right.score - left.score || left.order - right.order);
  return { items: ranked.slice(0, limit).map(item => item.entry), total: ranked.length };
}
