<script lang="ts">
  /**
   * 文件职责：渲染任务板与待处理视图中的单个任务卡片。
   * 定义范围：低密度展示难度徽章、所属项目、状态标签、标题、执行者限制、仲裁标识与更新时间。
   */
  import type { BountyProject, BountyTask } from '../api/types';
  import { locale, t } from '../i18n';
  import ProjectIcon from '../ProjectIcon.svelte';
  import {
    formatAssigneeConstraint,
    formatBountyArbitrator,
    formatBountyExecutor,
    formatBountyPublisher,
    formatBountyStatus,
    formatCompletionCost,
    formatDraftCollaborator,
    formatLeaseCountdown,
    formatQualityGrade,
    formatTimestamp,
    renderMarkdownAction,
  } from './helpers';

  interface Props {
    bounty: BountyTask;
    project?: BountyProject;
    selectable?: boolean;
    selected?: boolean;
    onSelect: (bountyId: string) => void;
    onToggleSelect?: (bountyId: string) => void;
  }

  let {
    bounty,
    project,
    selectable = false,
    selected = false,
    onSelect,
    onToggleSelect,
  }: Props = $props();

  let nowMs = $state(Date.now());

  $effect(() => {
    const activeExpires =
      bounty.status === 'IN_PROGRESS'
        ? bounty.lease_expires_at
        : bounty.status === 'PENDING_ARBITRATION'
          ? bounty.arbitration_expires_at
          : null;
    nowMs = Date.now();
    if (!activeExpires) return;
    const timer = setInterval(() => {
      nowMs = Date.now();
    }, 1000);
    return () => clearInterval(timer);
  });

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

  const collaborator = $derived.by(() => {
    const _lang = $locale;
    return formatDraftCollaborator(bounty);
  });

  const executor = $derived.by(() => {
    const _lang = $locale;
    return formatBountyExecutor(bounty);
  });

  const arbitrator = $derived.by(() => {
    const _lang = $locale;
    return formatBountyArbitrator(bounty);
  });

  const leaseCountdown = $derived.by(() => {
    const _lang = $locale;
    if (bounty.status !== 'IN_PROGRESS') return null;
    return formatLeaseCountdown(bounty.lease_expires_at, nowMs);
  });

  const arbitrationCountdown = $derived.by(() => {
    const _lang = $locale;
    if (bounty.status !== 'PENDING_ARBITRATION') return null;
    return formatLeaseCountdown(bounty.arbitration_expires_at, nowMs);
  });

  const qualityLabel = $derived.by(() => {
    const _lang = $locale;
    return bounty.quality_grade ? formatQualityGrade(bounty.quality_grade) : null;
  });

  const completionCostLabel = $derived.by(() => {
    const _lang = $locale;
    return formatCompletionCost(bounty.completion_cost);
  });

  const hasCompletionSummary = $derived(
    Boolean(bounty.completion_note?.trim()) || completionCostLabel !== null,
  );

  const blockedRemainingCount = $derived.by(() => {
    const summaries = bounty.blocked_by_summaries ?? [];
    const incomplete = summaries.filter(item => item.status !== 'COMPLETED').length;
    if (incomplete > 0) return incomplete;
    return bounty.is_blocked ? Math.max(1, (bounty.blocked_by ?? []).length) : 0;
  });

  const timeLabel = $derived(formatTimestamp(bounty.updated_at || bounty.created_at));
</script>

<article
  class="bounty-card"
  class:is-selected={selected}
  class:is-selectable={selectable && Boolean(onToggleSelect)}
  data-bounty-id={bounty.id}
  data-bounty-status={bounty.status}
  data-bounty-difficulty={bounty.difficulty}
>
  {#if selectable && onToggleSelect}
    <label class="bounty-card-select" title={$t('选择草稿')}>
      <input
        type="checkbox"
        data-action="toggle-select-draft"
        data-bounty-id={bounty.id}
        checked={selected}
        aria-label={$t('选择草稿 {title}', { title: bounty.title })}
        onchange={() => onToggleSelect(bounty.id)}
      />
    </label>
  {/if}
  <button
    type="button"
    class="bounty-card-button"
    onclick={() => onSelect(bounty.id)}
    aria-label={`${bounty.title} (${statusLabel})`}
  >
    <div class="bounty-card-header">
      <div class="bounty-card-badges">
        {#if bounty.status !== 'OPEN'}
          <span class="status-pill" data-status={bounty.status}>
            {statusLabel}
          </span>
        {/if}
        <span class="difficulty-badge" data-difficulty={bounty.difficulty} title={$t('预估难度')}>
          {bounty.difficulty}
        </span>
        {#if bounty.status === 'DRAFT' && bounty.draft_needs_human_review}
          <span class="draft-review-pill" data-slot="card-draft-review">
            {$t('待人工审核')}
          </span>
        {/if}
        {#if bounty.is_blocked}
          <span class="blocked-pill" data-slot="card-blocked">
            {$t('等待前置 ({count})', { count: blockedRemainingCount })}
          </span>
        {/if}
        {#if bounty.parent_bounty_id}
          <span class="derived-draft-pill" data-slot="card-derived-draft">
            {$t('派生子单')}
          </span>
        {/if}
        {#if bounty.require_arbitration}
          <span class="arbitration-pill">{$t('仲裁开启')}</span>
        {/if}
        {#if (bounty.rejection_count ?? 0) > 0}
          <span class="rejection-pill" data-slot="card-rejection-count">
            {$t('已打回 {count}/4 次', { count: bounty.rejection_count })}
          </span>
        {/if}
        {#if bounty.status === 'COMPLETED' && qualityLabel}
          <span
            class="quality-pill"
            data-slot="card-quality-grade"
            data-quality-grade={bounty.quality_grade}
          >
            {qualityLabel}
          </span>
        {/if}
        {#if bounty.status === 'COMPLETED' && bounty.calibrated_difficulty}
          <span
            class="difficulty-badge"
            data-slot="card-calibrated-difficulty"
            data-difficulty={bounty.calibrated_difficulty}
          >
            {$t('校准难度 {difficulty}', { difficulty: bounty.calibrated_difficulty })}
          </span>
        {/if}
      </div>
      {#if (bounty.comment_count ?? 0) > 0}
        <span class="bounty-comment-pill" data-slot="card-comment-count">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" aria-hidden="true">
            <path d="M3 3.5h10v7H7.5L4.5 13v-2.5H3z" />
          </svg>
          {$t('评论 {count}', { count: bounty.comment_count })}
        </span>
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

    {#if hasCompletionSummary}
      <div class="bounty-card-completion" data-slot="card-completion">
        <div class="bounty-card-completion-header">
          <span class="bounty-card-completion-label">{$t('完成说明')}</span>
          {#if completionCostLabel !== null}
            <span class="bounty-card-completion-cost" data-slot="card-completion-cost">
              {$t('成本 {cost}', { cost: completionCostLabel })}
            </span>
          {/if}
        </div>
        {#if bounty.completion_note?.trim()}
          <p class="bounty-card-completion-note" data-slot="card-completion-note">
            {bounty.completion_note.trim()}
          </p>
        {/if}
      </div>
    {/if}

    {#if publisher.isAgent || collaborator || executor || arbitrator || bounty.status === 'PENDING_ARBITRATION' || bounty.status === 'WAITING_INFO' || leaseCountdown || arbitrationCountdown || bounty.assignee_mode !== 'any'}
      <div class="bounty-card-people">
        {#if publisher.isAgent}
          <span class="bounty-publisher-pill" data-publisher-kind="agent">
            {publisher.label}
          </span>
        {/if}
        {#if collaborator}
          <span class="bounty-collaborator-pill" data-slot="card-collaborator">
            {$t('协作者: {agent}', { agent: collaborator.label })}
          </span>
        {/if}
        {#if executor}
          <span
            class="bounty-executor-pill"
            data-slot="card-executor"
            data-executor-tier={executor.tier ?? 'unknown'}
          >
            {$t('执行者: {agent}', { agent: executor.label })}
          </span>
        {/if}
        {#if arbitrator}
          <span
            class="bounty-arbitrator-pill"
            data-slot="card-arbitrator"
            data-arbitrator-tier={arbitrator.tier ?? 'unknown'}
          >
            {$t('裁判: {agent}', { agent: arbitrator.label })}
          </span>
        {:else if bounty.status === 'PENDING_ARBITRATION'}
          <span class="bounty-human-fallback-pill" data-slot="card-human-fallback">
            {$t('人工仲裁兜底')}
          </span>
        {/if}
        {#if bounty.status === 'WAITING_INFO'}
          <span
            class="bounty-frozen-pill"
            data-slot="card-lease-frozen"
            data-lease-state="frozen"
          >
            {$t('租约已冻结（等待回复）')}
          </span>
        {:else if leaseCountdown}
          <span
            class="bounty-lease-pill"
            data-slot="card-lease-countdown"
            data-lease-state={leaseCountdown.expired
              ? 'expired'
              : leaseCountdown.expiringSoon
                ? 'expiring'
                : 'active'}
          >
            {leaseCountdown.expired
              ? leaseCountdown.label
              : $t('租约剩余 {countdown}', { countdown: leaseCountdown.label })}
          </span>
        {:else if arbitrationCountdown}
          <span
            class="bounty-lease-pill"
            data-slot="card-arbitration-countdown"
            data-lease-state={arbitrationCountdown.expired
              ? 'expired'
              : arbitrationCountdown.expiringSoon
                ? 'expiring'
                : 'active'}
          >
            {arbitrationCountdown.expired
              ? $t('仲裁已超时待顺延')
              : $t('仲裁剩余 {countdown}', { countdown: arbitrationCountdown.label })}
          </span>
        {/if}
        {#if bounty.assignee_mode !== 'any'}
          <span class="bounty-card-constraint">{assigneeLabel}</span>
        {/if}
      </div>
    {/if}

    <div class="bounty-card-meta">
      <span class="bounty-card-project">
        <ProjectIcon icon={project?.icon ?? undefined} />
        <span class="bounty-card-project-name">{project?.name ?? $t('未命名项目')}</span>
      </span>
      {#if timeLabel}
        <time class="bounty-card-time" datetime={bounty.updated_at}>{timeLabel}</time>
      {/if}
    </div>
  </button>
</article>
