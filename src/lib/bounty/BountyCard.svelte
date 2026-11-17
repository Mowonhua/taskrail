<script lang="ts">
  /**
   * 文件职责：渲染悬赏板与待处理视图中的单个悬赏任务卡片。
   * 定义范围：低密度展示难度徽章、所属项目、状态标签、标题、执行者限制、仲裁标识与更新时间。
   */
  import type { BountyProject, BountyTask } from '../api/types';
  import { locale, t } from '../i18n';
  import ProjectIcon from '../ProjectIcon.svelte';
  import {
    formatAssigneeConstraint,
    formatBountyPublisher,
    formatBountyStatus,
    formatTimestamp,
    renderMarkdownAction,
  } from './helpers';

  interface Props {
    bounty: BountyTask;
    project?: BountyProject;
    onSelect: (bountyId: string) => void;
  }

  let { bounty, project, onSelect }: Props = $props();

  const statusLabel = $derived.by(() => {
    const _lang = $locale;
    return formatBountyStatus(bounty.status, bounty.status === 'DRAFT');
  });

  const assigneeLabel = $derived.by(() => {
    const _lang = $locale;
    return formatAssigneeConstraint(bounty);
  });

  const publisher = $derived.by(() => {
    const _lang = $locale;
    return formatBountyPublisher(bounty);
  });

  const timeLabel = $derived(formatTimestamp(bounty.updated_at || bounty.created_at));
</script>

<article
  class="bounty-card"
  data-bounty-id={bounty.id}
  data-bounty-status={bounty.status}
  data-bounty-difficulty={bounty.difficulty}
>
  <button
    type="button"
    class="bounty-card-button"
    onclick={() => onSelect(bounty.id)}
    aria-label={`${bounty.title} (${statusLabel})`}
  >
    <div class="bounty-card-header">
      <div class="bounty-card-badges">
        <span class="difficulty-badge" data-difficulty={bounty.difficulty}>
          [{bounty.difficulty}]
        </span>
        <span class="status-pill" data-status={bounty.status}>
          {statusLabel}
        </span>
        {#if bounty.require_arbitration}
          <span class="arbitration-pill">{$t('仲裁开启')}</span>
        {/if}
      </div>
      {#if timeLabel}
        <time class="bounty-card-time" datetime={bounty.updated_at}>{timeLabel}</time>
      {/if}
    </div>

    <h3 class="bounty-card-title">{bounty.title}</h3>

    {#if bounty.description.trim()}
      <div
        class="bounty-card-preview"
        data-slot="bounty-card-preview"
        use:renderMarkdownAction={bounty.description}
      ></div>
    {/if}

    <div class="bounty-card-meta">
      <span class="bounty-card-project">
        <ProjectIcon icon={project?.icon ?? undefined} />
        <span class="bounty-card-project-name">{project?.name ?? $t('未命名项目')}</span>
      </span>

      <div class="bounty-card-meta-end">
        {#if publisher.isAgent}
          <span class="bounty-publisher-pill" data-publisher-kind="agent">
            {publisher.label}
          </span>
        {/if}
        <span class="bounty-card-constraint">{assigneeLabel}</span>
      </div>
    </div>
  </button>
</article>
