<script lang="ts">
  /**
   * 文件职责：提供项目图标的选择、搜索、分类、自定义输入和恢复默认操作。
   * 定义范围：表情与 Lucide 数据加载、筛选状态、网格键盘焦点和输入校验；持久化由调用方负责。
   */
  import { untrack } from 'svelte';
  import { locale, t, type Locale } from './i18n';
  import ProjectIcon from './ProjectIcon.svelte';
  import { emojiCategoryFor, loadEmojiDataset, searchEmojis, type EmojiDataset, type EmojiEntry } from './emoji-icons';
  import {
    lucideCategories, lucideCategoryLabel, loadLucideDataset, lucideIconBody, recommendedLucideIcons,
    searchLucideIcons, type LucideDataset,
  } from './lucide-icons';
  import { normalizeProjectEmoji, parseProjectIcon, serializeProjectIcon, type ProjectIcon as IconReference } from './project-icons';

  interface Props {
    projectName: string;
    icon?: string;
    /** 手机端不把初始焦点放进搜索框，避免打开选择器时立即唤起输入法。 */
    mobile: boolean;
    onSelect: (icon: IconReference) => void;
    onReset: () => void;
  }

  type Tab = 'emoji' | 'lucide';
  type EmojiItem = { kind: 'emoji'; key: string; icon: Extract<IconReference, { kind: 'emoji' }>; label: string };
  type LucideItem = { kind: 'lucide'; key: string; icon: Extract<IconReference, { kind: 'lucide' }>; label: string; body: string };
  type PickerItem = EmojiItem | LucideItem;
  type CategoryBadge = { kind: 'emoji'; value: string } | { kind: 'lucide'; body: string };
  interface CategoryView { id: string; label: string; badge: CategoryBadge; items: PickerItem[] }

  let { projectName, icon, mobile, onSelect, onReset }: Props = $props();
  const current = $derived(parseProjectIcon(icon));
  const selectedKey = $derived(current ? serializeProjectIcon(current) : '');
  const recommendedLucideSet = new Set<string>(recommendedLucideIcons);
  // 分类导航使用代表性标识，避免完整分类名称在窄弹窗里挤成横向长列表。
  const emojiCategoryBadges: Record<string, string> = {
    recommended: '⭐', 'smileys-emotion': '😀', 'people-body': '👤', 'animals-nature': '🐻',
    'food-drink': '🍔', 'travel-places': '✈️', activities: '⚽', objects: '💡', symbols: '❤️', flags: '🚩',
  };

  let tab = $state<Tab>(untrack(() => current?.kind === 'lucide' ? 'lucide' : 'emoji'));
  let categoryId = $state('recommended');
  let categoryTouched = $state(false);
  let query = $state('');
  let custom = $state('');
  let error = $state('');
  let roving = $state(0);
  let grid = $state<HTMLDivElement>();
  let categoryRow = $state<HTMLDivElement>();
  let emojiDataset = $state.raw<EmojiDataset | null>(null);
  let emojiLocale = $state<Locale | null>(null);
  let lucideDataset = $state.raw<LucideDataset | null>(null);
  let emojiFailed = $state(false);
  let lucideFailed = $state(false);
  let emojiLoadToken = 0;

  function emojiItem(entry: EmojiEntry): EmojiItem {
    const icon = { kind: 'emoji' as const, value: entry.unicode };
    return { kind: 'emoji', key: serializeProjectIcon(icon), icon, label: entry.label };
  }

  function lucideItem(dataset: LucideDataset, name: string): LucideItem | null {
    const body = lucideIconBody(dataset, name);
    if (!body) return null;
    const icon = { kind: 'lucide' as const, name };
    return { kind: 'lucide', key: serializeProjectIcon(icon), icon, label: name.replace(/-/g, ' '), body };
  }

  async function ensureEmoji(language: Locale): Promise<void> {
    if (emojiDataset && emojiLocale === language) return;
    const token = ++emojiLoadToken;
    emojiDataset = null; emojiLocale = null; emojiFailed = false;
    try {
      const dataset = await loadEmojiDataset(language);
      if (token !== emojiLoadToken || language !== $locale) return;
      emojiDataset = dataset; emojiLocale = language;
      const icon = current;
      if (!categoryTouched && icon?.kind === 'emoji' && !dataset.recommended.some(entry => entry.unicode === icon.value)) {
        categoryId = emojiCategoryFor(dataset, icon.value)?.id ?? 'recommended';
      }
    } catch { if (token === emojiLoadToken) emojiFailed = true; }
  }

  async function ensureLucide(): Promise<void> {
    if (lucideDataset) return;
    lucideFailed = false;
    try {
      const dataset = await loadLucideDataset();
      lucideDataset = dataset;
      const icon = current;
      if (!categoryTouched && icon?.kind === 'lucide' && !recommendedLucideSet.has(icon.name)) {
        categoryId = lucideCategories.find(category => category.icons.includes(icon.name))?.id ?? 'recommended';
      }
    } catch { lucideFailed = true; }
  }

  $effect(() => {
    const language = $locale;
    const activeTab = tab;
    if (activeTab === 'emoji') void untrack(() => ensureEmoji(language));
    else void untrack(() => ensureLucide());
  });

  const emojiCategoryViews = $derived.by<CategoryView[]>(() => {
    const dataset = emojiDataset;
    if (!dataset) return [];
    return [
      { id: 'recommended', label: $t('推荐'), badge: { kind: 'emoji' as const, value: '⭐' }, items: dataset.recommended.map(emojiItem) },
      ...dataset.categories.map(category => ({
        id: category.id, label: category.label,
        badge: { kind: 'emoji' as const, value: emojiCategoryBadges[category.id] ?? category.emojis[0]?.unicode ?? '🏷️' },
        items: category.emojis.map(emojiItem),
      })),
    ];
  });

  const lucideCategoryViews = $derived.by<CategoryView[]>(() => {
    const dataset = lucideDataset;
    if (!dataset) return [];
    const items = (names: readonly string[]) => names
      .map(name => lucideItem(dataset, name))
      .filter((item): item is LucideItem => !!item);
    return [
      { id: 'recommended', label: $t('推荐'), badge: { kind: 'lucide' as const, body: lucideIconBody(dataset, 'sparkles') ?? '' }, items: items(recommendedLucideIcons) },
      ...lucideCategories.map(category => ({
        id: category.id, label: lucideCategoryLabel(category, $locale),
        badge: { kind: 'lucide' as const, body: lucideIconBody(dataset, category.icon) ?? '' },
        items: items(category.icons),
      })),
    ];
  });

  const categories = $derived(tab === 'emoji' ? emojiCategoryViews : lucideCategoryViews);
  const activeCategory = $derived(categories.find(category => category.id === categoryId) ?? categories[0]);
  const activeItems = $derived(activeCategory?.items ?? []);
  const searching = $derived(!!query.trim());
  const searchResult = $derived.by<{ items: PickerItem[]; total: number } | null>(() => {
    const text = query.trim();
    if (!text) return null;
    if (tab === 'emoji') {
      if (!emojiDataset) return { items: [], total: 0 };
      const result = searchEmojis(emojiDataset, text);
      return { items: result.items.map(emojiItem), total: result.total };
    }
    const dataset = lucideDataset;
    if (!dataset) return { items: [], total: 0 };
    const result = searchLucideIcons(dataset, text, $locale);
    const items = result.names
      .map(name => lucideItem(dataset, name))
      .filter((item): item is LucideItem => !!item);
    return { items, total: result.total };
  });
  const visibleItems = $derived(searchResult?.items ?? activeItems);
  const activeFailed = $derived(tab === 'emoji' ? emojiFailed : lucideFailed);
  const loading = $derived(!activeFailed && !(tab === 'emoji' ? emojiDataset : lucideDataset));

  // 分类或数据变化后修正失效的分类选择；空数据时保留推荐分类。
  $effect(() => {
    const list = categories;
    if (list.length && !list.some(category => category.id === categoryId)) categoryId = list[0].id;
  });
  // 当前分类在窄屏可能滚出分类条；打开已有图标或切换页签后把它带回可见范围。
  $effect(() => {
    void categoryId;
    const row = categoryRow;
    row?.querySelector<HTMLElement>('.icon-picker-category[aria-pressed="true"]')?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  });
  // 可见项变化时回到当前图标；搜索框和分类按钮持有焦点，移动网格焦点不会触发这里。
  $effect(() => {
    const items = visibleItems;
    const index = items.findIndex(item => item.key === selectedKey);
    roving = index >= 0 ? index : 0;
    if (grid) grid.scrollTop = 0;
  });

  function focusChoice(index: number): void {
    if (!visibleItems.length) return;
    const next = Math.max(0, Math.min(visibleItems.length - 1, index));
    roving = next;
    grid?.querySelectorAll<HTMLButtonElement>('.icon-choice')[next]?.focus();
  }

  /** 网格按视觉列数移动焦点；列数由当前布局解析，窄屏和桌面共用同一套按键。 */
  function handleChoiceKeydown(event: KeyboardEvent, index: number): void {
    const tracks = grid ? getComputedStyle(grid).gridTemplateColumns.split(' ').filter(Boolean).length : 0;
    const columns = tracks > 0 ? tracks : 8;
    let next: number;
    switch (event.key) {
      case 'ArrowLeft': next = index - 1; break;
      case 'ArrowRight': next = index + 1; break;
      case 'ArrowUp': next = index - columns; break;
      case 'ArrowDown': next = index + columns; break;
      case 'Home': next = 0; break;
      case 'End': next = visibleItems.length - 1; break;
      default: return;
    }
    event.preventDefault();
    focusChoice(next);
  }

  function handleSearchKeydown(event: KeyboardEvent): void {
    if (event.isComposing) return;
    if (event.key === 'Enter' && visibleItems.length === 1) {
      event.preventDefault(); onSelect(visibleItems[0].icon); return;
    }
    if (event.key === 'ArrowDown' && visibleItems.length) {
      event.preventDefault();
      const index = visibleItems.findIndex(item => item.key === selectedKey);
      focusChoice(index >= 0 ? index : 0);
    }
  }

  function selectTab(next: Tab): void {
    if (tab === next) return;
    tab = next; query = ''; categoryId = 'recommended'; categoryTouched = false; error = ''; roving = 0;
  }

  function applyCustom(): void {
    const value = normalizeProjectEmoji(custom);
    if (!value) { error = $t('请输入一个字符或表情。'); return; }
    onSelect({ kind: 'emoji', value });
  }

  function retry(): void {
    if (tab === 'emoji') { emojiDataset = null; emojiFailed = false; void ensureEmoji($locale); }
    else { lucideDataset = null; lucideFailed = false; void ensureLucide(); }
  }
</script>

<div class="project-icon-picker">
  <h2 id="dialog-title">{$t('项目图标')}</h2>
  <div class="icon-picker-current">
    <ProjectIcon {icon} large/>
    <span class="icon-picker-project" title={projectName}>{projectName}</span>
  </div>

  <div class="icon-picker-toolbar">
    <div class="icon-picker-tabs" role="group" aria-label={$t('图标类型')}>
      <button type="button" class="icon-picker-tab" aria-pressed={tab === 'emoji'}
        data-autofocus={mobile && tab === 'emoji' ? 'true' : undefined}
        onclick={() => selectTab('emoji')}>{$t('表情')}</button>
      <button type="button" class="icon-picker-tab" aria-pressed={tab === 'lucide'}
        data-autofocus={mobile && tab === 'lucide' ? 'true' : undefined}
        onclick={() => selectTab('lucide')}>{$t('图标')}</button>
    </div>
    <input class="icon-picker-search" type="text" autocomplete="off" spellcheck="false"
      data-autofocus={!mobile ? 'true' : undefined}
      aria-label={tab === 'emoji' ? $t('搜索表情') : $t('搜索图标')}
      placeholder={tab === 'emoji' ? $t('搜索表情') : $t('搜索图标')}
      bind:value={query} onkeydown={handleSearchKeydown}/>
  </div>

  {#if !searching && categories.length > 0}
    <div class="icon-picker-categories" bind:this={categoryRow} role="group" aria-label={$t('分类')}>
      {#each categories as category (category.id)}
        <button type="button" class="icon-picker-category" aria-pressed={category.id === categoryId}
          aria-label={category.label} title={category.label}
          onclick={() => { categoryId = category.id; categoryTouched = true; }}>
          {#if category.badge.kind === 'emoji'}
            <span class="icon-picker-category-emoji">{category.badge.value}</span>
          {:else}
            <svg class="icon-picker-category-svg" width="15" height="15" viewBox="0 0 24 24" aria-hidden="true">{@html category.badge.body}</svg>
          {/if}
        </button>
      {/each}
    </div>
  {/if}

  <div class="icon-picker-viewport">
    {#if loading}
      <p class="icon-picker-state" role="status">{$t('正在加载…')}</p>
    {:else if activeFailed}
      <div class="icon-picker-state" role="alert">
        <span>{$t('图标数据加载失败。')}</span>
        <button type="button" class="icon-picker-retry" onclick={retry}>{$t('重试')}</button>
      </div>
    {:else if visibleItems.length === 0}
      <p class="icon-picker-state">{$t(searching ? (tab === 'emoji' ? '没有匹配的表情' : '没有匹配的图标') : '暂无图标')}</p>
    {:else}
      <div class="icon-picker-grid" bind:this={grid} role="group" aria-label={tab === 'emoji' ? $t('表情') : $t('图标')}>
        {#each visibleItems as item, index (item.key)}
          <button type="button" class="icon-choice" class:icon-choice-active={item.key === selectedKey}
            tabindex={index === roving ? 0 : -1}
            aria-pressed={item.key === selectedKey}
            aria-label={item.label}
            title={item.label}
            onfocus={() => roving = index}
            onkeydown={event => handleChoiceKeydown(event, index)}
            onclick={() => onSelect(item.icon)}>
            {#if item.kind === 'emoji'}
              <span class="icon-choice-emoji">{item.icon.value}</span>
            {:else}
              <svg class="icon-choice-lucide" width="19" height="19" viewBox="0 0 24 24" aria-hidden="true">{@html item.body}</svg>
            {/if}
          </button>
        {/each}
      </div>
    {/if}
  </div>

  {#if searching && searchResult && searchResult.total > visibleItems.length}
    <p class="icon-picker-note">{$t('显示前 {count} 个结果', { count: visibleItems.length })}</p>
  {/if}

  {#if tab === 'emoji'}
    <div class="icon-custom-row">
      <input class="icon-custom-input" aria-label={$t('自定义图标')} placeholder={$t('输入一个字符或表情')} bind:value={custom}
        oninput={() => { if (error) error = ''; }}
        onkeydown={event => { if (event.key === 'Enter' && !event.isComposing) { event.preventDefault(); applyCustom(); } }}/>
      <button type="button" class="icon-custom-apply" disabled={!custom.trim()} onclick={applyCustom}>{$t('使用此图标')}</button>
    </div>
  {/if}
  {#if error}<p class="dialog-error" role="alert">{error}</p>{/if}
  {#if current}<button type="button" class="icon-reset" onclick={onReset}>{$t('恢复默认图标')}</button>{/if}
</div>
