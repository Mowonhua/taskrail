<script lang="ts">
  /**
   * 文件职责：渲染侧栏与选择器中的项目图标。
   * 定义范围：表情字素、Lucide 矢量图标与通用文档回退；不处理选择交互或持久化。
   */
  import { parseProjectIcon } from './project-icons';
  import { lucideIconBody, loadLucideDataset } from './lucide-icons';

  interface Props {
    /** 已持久化的项目图标引用；缺省或无法识别时显示通用文档标识。 */
    icon?: string;
    /** 选择器预览使用更大的图标位，侧栏保持与导航图标同尺寸。 */
    large?: boolean;
  }

  let { icon, large = false }: Props = $props();
  const parsed = $derived(parseProjectIcon(icon));
  let body = $state('');
  let loaded = $state(false);

  $effect(() => {
    const current = parsed;
    body = ''; loaded = false;
    if (current?.kind !== 'lucide') return;
    let cancelled = false;
    void loadLucideDataset()
      .then(dataset => {
        if (cancelled) return;
        body = lucideIconBody(dataset, current.name) ?? '';
        loaded = true;
      })
      .catch(() => { if (!cancelled) loaded = true; });
    return () => { cancelled = true; };
  });

  const size = $derived(large ? 22 : 17);
</script>

{#snippet defaultIcon()}
  <!-- 折边画成直角折线：它与外轮廓的斜切边共用端点，改用弧线会与斜切边大面积重叠，在放大时出现双层描边。 -->
  <svg width={size} height={size} viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
    <path d="M11 3H6a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V8z"/>
    <path d="M11 3v3.8a1.2 1.2 0 0 0 1.2 1.2h3.8"/>
  </svg>
{/snippet}

<span class="project-icon" class:project-icon-large={large} class:project-icon-lucide={parsed?.kind === 'lucide'} aria-hidden="true">
  {#if parsed?.kind === 'emoji'}
    <span class="project-icon-glyph">{parsed.value}</span>
  {:else if parsed?.kind === 'lucide'}
    {#if body}
      <svg class="project-icon-svg" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">{@html body}</svg>
    {:else if loaded}
      {@render defaultIcon()}
    {:else}
      <span class="project-icon-pending"></span>
    {/if}
  {:else}
    {@render defaultIcon()}
  {/if}
</span>
