/** 文件职责：验证表情数据加载、分类和中文搜索命中。 */
import { describe, expect, it } from 'vitest';
import { emojiCategoryFor, loadEmojiDataset, searchEmojis } from './emoji-icons';
import { recommendedEmojis } from './project-icons';

describe('表情数据', () => {
  it('中文数据包含推荐表情、分类和本地化标签', async () => {
    const dataset = await loadEmojiDataset('zh-CN');
    expect(dataset.entries.length).toBeGreaterThan(1500);
    expect(dataset.categories.map(category => category.label)).toContain('物品');
    expect(dataset.categories.map(category => category.id)).not.toContain('component');
    expect(dataset.recommended).toHaveLength(recommendedEmojis.length);
    expect(dataset.entries.find(entry => entry.unicode === '💼')?.label).toBe('公文包');
    expect(emojiCategoryFor(dataset, '💼')?.id).toBe('objects');
  });

  it('中文名称和标签可搜索，结果优先精确名称', async () => {
    const dataset = await loadEmojiDataset('zh-CN');
    expect(searchEmojis(dataset, '公文包').items[0]?.unicode).toBe('💼');
    expect(searchEmojis(dataset, '阅读').items.some(item => item.unicode === '📚')).toBe(true);
    expect(searchEmojis(dataset, '不存在的表情')).toEqual({ items: [], total: 0 });
  });

  it('英文搜索按当前语言名称命中并返回截断前总数', async () => {
    const dataset = await loadEmojiDataset('en');
    expect(searchEmojis(dataset, 'briefcase').items[0]?.unicode).toBe('💼');
    const result = searchEmojis(dataset, 'a', 3);
    expect(result.items).toHaveLength(3);
    expect(result.total).toBeGreaterThan(3);
  });
});
