/**
 * 文件职责：懒加载 Lucide 图标数据，并提供精选分类、中文检索词与 SVG 内容。
 * 定义范围：Iconify 数据解析、别名解析与搜索；不渲染 Svelte 组件。
 */
import type { Locale } from './i18n';

export interface LucideDataset {
  icons: Record<string, { body: string }>;
  aliases: Record<string, { parent: string }>;
  aliasesByParent: Map<string, string[]>;
}

export interface LucideCategory {
  id: string;
  labels: Record<Locale, string>;
  keywords: Record<Locale, string[]>;
  /** 分类导航使用的小图标名称，同时必须存在于本地数据中。 */
  icon: string;
  icons: readonly string[];
}

export interface LucideSearchResult {
  names: string[];
  total: number;
}

/** 常用图标作为图标页签的默认入口，覆盖项目、任务、阅读、开发和生活场景。 */
export const recommendedLucideIcons = [
  'briefcase', 'target', 'folder-kanban', 'book-open',
  'code', 'terminal', 'house', 'heart-pulse',
  'plane', 'palette', 'wallet', 'calendar',
  'star', 'coffee', 'leaf', 'camera',
  'music', 'lightbulb', 'check', 'map',
  'globe', 'flag', 'bookmark', 'sparkles',
] as const;

/**
 * 分类只用于浏览入口，不限制搜索范围；完整 Lucide 集合始终可按英文名称或别名检索。
 * 约束条件：所有名称必须存在于当前 @iconify-json/lucide 数据中，由单元测试守护。
 */
export const lucideCategories: readonly LucideCategory[] = [
  {
    id: 'work',
    labels: { 'zh-CN': '工作', en: 'Work' },
    icon: 'briefcase',
    keywords: {
      'zh-CN': ['工作', '办公', '项目', '任务', '计划', '进度', '会议', '管理'],
      en: ['work', 'office', 'project', 'task', 'plan', 'progress', 'meeting', 'management'],
    },
    icons: ['briefcase', 'briefcase-business', 'target', 'chart-no-axes-column-increasing', 'calendar-check', 'folder-kanban', 'clipboard-check', 'presentation', 'handshake', 'badge-check'],
  },
  {
    id: 'development',
    labels: { 'zh-CN': '开发', en: 'Development' },
    icon: 'code',
    keywords: {
      'zh-CN': ['开发', '代码', '编程', '程序', '技术', '调试', '接口', '数据库', '服务器', '部署'],
      en: ['development', 'code', 'programming', 'developer', 'debug', 'api', 'database', 'server', 'deploy'],
    },
    icons: ['code', 'code-xml', 'terminal', 'bug', 'git-branch', 'database', 'server', 'box', 'package', 'rocket'],
  },
  {
    id: 'learning',
    labels: { 'zh-CN': '学习', en: 'Learning' },
    icon: 'book-open',
    keywords: {
      'zh-CN': ['学习', '阅读', '书', '课程', '知识', '研究', '学校', '笔记', '语言'],
      en: ['learn', 'read', 'book', 'course', 'knowledge', 'research', 'school', 'notes', 'language'],
    },
    icons: ['book-open', 'book', 'graduation-cap', 'brain', 'pencil', 'pen-line', 'notebook-pen', 'library', 'languages', 'school'],
  },
  {
    id: 'life',
    labels: { 'zh-CN': '生活', en: 'Life' },
    icon: 'house',
    keywords: {
      'zh-CN': ['生活', '家', '家居', '购物', '饮食', '植物', '日常'],
      en: ['life', 'home', 'shopping', 'food', 'plant', 'daily'],
    },
    icons: ['house', 'coffee', 'shopping-cart', 'utensils', 'leaf', 'sun', 'moon', 'sofa', 'lightbulb', 'sparkles'],
  },
  {
    id: 'health',
    labels: { 'zh-CN': '健康', en: 'Health' },
    icon: 'heart-pulse',
    keywords: {
      'zh-CN': ['健康', '运动', '医疗', '身体', '饮食', '睡眠'],
      en: ['health', 'fitness', 'medical', 'body', 'diet', 'sleep'],
    },
    icons: ['heart-pulse', 'dumbbell', 'salad', 'bed-double', 'activity', 'stethoscope', 'pill', 'apple', 'bike', 'smile'],
  },
  {
    id: 'travel',
    labels: { 'zh-CN': '出行', en: 'Travel' },
    icon: 'plane',
    keywords: {
      'zh-CN': ['旅行', '出行', '交通', '地图', '地点', '路线'],
      en: ['travel', 'trip', 'transport', 'map', 'place', 'route'],
    },
    icons: ['plane', 'map', 'luggage', 'compass', 'car-front', 'train-front', 'bike', 'globe', 'mountain', 'map-pin'],
  },
  {
    id: 'creative',
    labels: { 'zh-CN': '创意', en: 'Creative' },
    icon: 'palette',
    keywords: {
      'zh-CN': ['创意', '设计', '艺术', '摄影', '音乐', '图片', '绘画'],
      en: ['creative', 'design', 'art', 'photo', 'music', 'image', 'draw'],
    },
    icons: ['palette', 'brush', 'music', 'camera', 'film', 'pen-tool', 'scissors', 'image', 'shapes', 'wand-sparkles'],
  },
  {
    id: 'finance',
    labels: { 'zh-CN': '财务', en: 'Finance' },
    icon: 'wallet',
    keywords: {
      'zh-CN': ['财务', '钱', '预算', '账单', '支付', '银行卡', '投资'],
      en: ['finance', 'money', 'budget', 'bill', 'payment', 'card', 'investment'],
    },
    icons: ['wallet', 'banknote', 'credit-card', 'receipt', 'piggy-bank', 'calculator', 'coins', 'chart-line', 'landmark', 'badge-dollar-sign'],
  },
  {
    id: 'time',
    labels: { 'zh-CN': '时间', en: 'Time' },
    icon: 'clock',
    keywords: {
      'zh-CN': ['时间', '日历', '日期', '日程', '闹钟', '计划'],
      en: ['time', 'calendar', 'date', 'schedule', 'alarm', 'plan'],
    },
    icons: ['calendar', 'calendar-days', 'clock', 'timer', 'alarm-clock', 'hourglass', 'history', 'repeat', 'watch', 'sunrise'],
  },
  {
    id: 'more',
    labels: { 'zh-CN': '其他', en: 'More' },
    icon: 'star',
    keywords: {
      'zh-CN': ['其他', '收藏', '标签', '链接', '安全', '标记'],
      en: ['more', 'star', 'tag', 'link', 'security', 'bookmark'],
    },
    icons: ['star', 'flag', 'bookmark', 'tag', 'link', 'lock', 'key', 'shield', 'globe-2', 'hash'],
  },
];

/** 精确中文词提高常见检索的命中优先级；分类关键词仍负责返回同组图标。 */
const chineseAliases: Readonly<Record<string, readonly string[]>> = {
  '工作': ['briefcase', 'briefcase-business'],
  '办公': ['briefcase-business', 'presentation'],
  '项目': ['folder-kanban', 'target'],
  '任务': ['clipboard-check', 'badge-check'],
  '计划': ['calendar-check', 'notebook-pen'],
  '书': ['book', 'book-open', 'library'],
  '读书': ['book-open', 'library'],
  '代码': ['code', 'code-xml', 'terminal'],
  '编程': ['code', 'terminal', 'bug'],
  '数据库': ['database'],
  '服务器': ['server'],
  '设计': ['palette', 'pen-tool', 'brush'],
  '图片': ['image', 'camera'],
  '音乐': ['music'],
  '摄影': ['camera'],
  '咖啡': ['coffee'],
  '家': ['house'],
  '购物': ['shopping-cart'],
  '旅行': ['plane', 'luggage', 'map'],
  '飞机': ['plane'],
  '地图': ['map', 'map-pin'],
  '健康': ['heart-pulse', 'activity', 'stethoscope'],
  '运动': ['dumbbell', 'bike', 'activity'],
  '钱': ['wallet', 'banknote', 'coins'],
  '财务': ['wallet', 'credit-card', 'calculator'],
  '日历': ['calendar', 'calendar-days'],
  '时间': ['clock', 'timer', 'hourglass'],
  '提醒': ['alarm-clock'],
  '安全': ['shield', 'lock', 'key'],
  '收藏': ['star', 'bookmark'],
  '标签': ['tag'],
  '链接': ['link'],
  '灵感': ['lightbulb', 'sparkles'],
  '目标': ['target'],
  '进度': ['chart-no-axes-column-increasing', 'chart-line'],
  '数据': ['database', 'chart-line'],
  '阅读': ['book-open', 'library'],
  '学习': ['graduation-cap', 'brain'],
};

interface RawLucideData {
  icons: Record<string, { body: string }>;
  aliases?: Record<string, { parent: string }>;
}

let cache: Promise<LucideDataset> | null = null;

/**
 * 函数职责：加载并缓存完整 Lucide 图标数据。
 * 输入说明：无；数据包随构建产物本地分发，只在打开图标页签或渲染已选图标时加载。
 * 输出说明：返回图标、别名和按父级归组的别名索引；失败后清除缓存以便重试。
 */
export function loadLucideDataset(): Promise<LucideDataset> {
  if (!cache) {
    cache = import('@iconify-json/lucide/icons.json?raw').then(module => {
      const raw = JSON.parse(module.default) as RawLucideData;
      const aliases = raw.aliases ?? {};
      const aliasesByParent = new Map<string, string[]>();
      for (const [alias, target] of Object.entries(aliases)) {
        if (!raw.icons[target.parent]) continue;
        const list = aliasesByParent.get(target.parent) ?? [];
        list.push(alias); aliasesByParent.set(target.parent, list);
      }
      return { icons: raw.icons, aliases, aliasesByParent };
    }).catch(error => { cache = null; throw error; });
  }
  return cache;
}

/** 解析别名链并返回可直接嵌入 SVG 的路径内容；未知名称返回 null。 */
export function lucideIconBody(dataset: LucideDataset, name: string): string | null {
  const seen = new Set<string>();
  let current = name;
  while (!dataset.icons[current]) {
    const alias = dataset.aliases[current];
    if (!alias || seen.has(current)) return null;
    seen.add(current); current = alias.parent;
  }
  return dataset.icons[current]?.body ?? null;
}

export function lucideCategoryLabel(category: LucideCategory, locale: Locale): string {
  return category.labels[locale] ?? category.labels.en;
}

/**
 * 函数职责：搜索完整 Lucide 集合，包括未列入精选分类的图标。
 * 输入说明：query 可为英文名称、英文别名或已登记的中文词；locale 决定分类关键词语言。
 * 输出说明：按名称/别名精确度优先返回规范图标名，分类命中和中文词命中随后合并去重。
 * 实现思路：直接遍历完整名称表，读取按父级归组的别名；最后叠加分类关键词和中文别名。
 */
export function searchLucideIcons(dataset: LucideDataset, query: string, locale: Locale, limit = 240): LucideSearchResult {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return { names: [], total: 0 };
  const scores = new Map<string, number>();
  const bump = (name: string, score: number) => {
    if (scores.get(name) === undefined || score > scores.get(name)!) scores.set(name, score);
  };
  for (const name of Object.keys(dataset.icons)) {
    if (name === normalized) bump(name, 6);
    else if (name.startsWith(normalized)) bump(name, 5);
    else if (name.includes(normalized)) bump(name, 3);
    for (const alias of dataset.aliasesByParent.get(name) ?? []) {
      if (alias === normalized) bump(name, 6);
      else if (alias.startsWith(normalized)) bump(name, 5);
      else if (alias.includes(normalized)) bump(name, 3);
    }
  }
  for (const [label, names] of Object.entries(chineseAliases)) {
    if (label.includes(normalized) || normalized.includes(label)) for (const name of names) bump(name, 4);
  }
  for (const category of lucideCategories) {
    const label = lucideCategoryLabel(category, locale).toLowerCase();
    const keywords = category.keywords[locale] ?? [];
    if (label.includes(normalized) || keywords.some(keyword => keyword.toLowerCase().includes(normalized))) {
      for (const name of category.icons) bump(name, 2);
    }
  }
  const ranked = [...scores.entries()]
    .filter(([name]) => !!dataset.icons[name])
    .sort((left, right) => right[1] - left[1] || left[0].localeCompare(right[0]));
  return { names: ranked.slice(0, limit).map(([name]) => name), total: ranked.length };
}
