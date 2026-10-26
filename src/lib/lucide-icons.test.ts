/** 文件职责：验证 Lucide 数据完整、精选名称有效、别名和中文检索可用。 */
import { describe, expect, it } from 'vitest';
import {
  lucideCategories, lucideIconBody, loadLucideDataset, recommendedLucideIcons, searchLucideIcons,
} from './lucide-icons';
import { parseProjectIcon } from './project-icons';

describe('Lucide 图标数据', () => {
  it('精选分类和常用图标全部存在于本地数据', async () => {
    const dataset = await loadLucideDataset();
    expect(Object.keys(dataset.icons).length).toBeGreaterThan(1500);
    for (const name of recommendedLucideIcons) expect(lucideIconBody(dataset, name), name).toBeTruthy();
    for (const category of lucideCategories) {
      expect(category.icons.length, category.id).toBeGreaterThan(0);
      expect(lucideIconBody(dataset, category.icon), category.icon).toBeTruthy();
      for (const name of category.icons) expect(lucideIconBody(dataset, name), name).toBeTruthy();
    }
    expect(lucideIconBody(dataset, 'not-a-real-icon')).toBeNull();
  });

  it('所有图标名都能写入项目配置', async () => {
    const dataset = await loadLucideDataset();
    for (const name of Object.keys(dataset.icons)) {
      expect(parseProjectIcon(`lucide:${name}`), name).toEqual({ kind: 'lucide', name });
    }
  });

  it('别名解析为同一份 SVG 内容', async () => {
    const dataset = await loadLucideDataset();
    expect(lucideIconBody(dataset, 'home')).toBe(lucideIconBody(dataset, 'house'));
  });

  it('英文名称、别名和中文分类词均可搜索', async () => {
    const dataset = await loadLucideDataset();
    expect(searchLucideIcons(dataset, 'briefcase', 'zh-CN').names[0]).toBe('briefcase');
    expect(searchLucideIcons(dataset, 'home', 'en').names).toContain('house');
    expect(searchLucideIcons(dataset, '工作', 'zh-CN').names).toContain('briefcase');
    expect(searchLucideIcons(dataset, '书', 'zh-CN').names).toContain('book-open');
    expect(searchLucideIcons(dataset, '旅行', 'zh-CN').names).toContain('plane');
    const limited = searchLucideIcons(dataset, 'a', 'en', 5);
    expect(limited.names).toHaveLength(5);
    expect(limited.total).toBeGreaterThan(5);
    expect(searchLucideIcons(dataset, '完全不存在的图标', 'zh-CN')).toEqual({ names: [], total: 0 });
  });
});
