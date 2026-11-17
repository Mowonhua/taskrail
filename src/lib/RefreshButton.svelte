<script lang="ts">
  /**
   * 文件职责：渲染统一的刷新图标按钮，并保证请求期间与最短时长内的旋转动画。
   * 定义范围：按钮外观、禁用状态与旋转计时；刷新请求和错误反馈由调用方处理。
   */
  import { onDestroy } from 'svelte';
  import { t } from './i18n';
  import { createRefreshSpin } from './refresh-spin';

  interface Props {
    /** 供集成测试与外部联动识别的动作标识。 */
    action: string;
    /** 刷新请求进行中时禁用按钮。 */
    busy?: boolean;
    onRefresh: () => Promise<void> | void;
  }

  let { action, busy = false, onRefresh }: Props = $props();
  let spinning = $state(false);
  const spin = createRefreshSpin(value => {
    spinning = value;
  });
  onDestroy(() => spin.stop());

  async function refresh(): Promise<void> {
    if (busy) return;
    const session = spin.begin();
    try {
      await onRefresh();
    } catch {
      /* 错误反馈由调用方展示，这里只负责结束旋转。 */
    } finally {
      spin.settle(session);
    }
  }
</script>

<button
  type="button"
  class="icon-button refresh-button"
  class:refreshing={spinning}
  aria-label={$t('刷新')}
  title={$t('刷新')}
  data-action={action}
  disabled={busy}
  onclick={() => void refresh()}
>
  <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path d="M14 8a6 6 0 1 1-6-6c1.68 0 3.29 .67 4.49 1.83L14 5.33" />
    <path d="M14 2v3.33h-3.33" />
  </svg>
</button>
