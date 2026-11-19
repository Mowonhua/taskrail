<script lang="ts">
  /**
   * 文件职责：渲染任务板与待处理视图中的单个任务委托卡片。
   * 定义范围：采用“左侧委托单主体 + 右侧等级票根”印刷单据结构，展示状态、仲裁标识、标题摘要、执行/租约信息、所属项目、等宽时间与巨型等级票根。
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
    return formatBountyStatus(bounty.status, false);
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
  const shortCode = $derived(`#${bounty.id.slice(0, 6).toUpperCase()}`);
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
    class:is-selectable={selectable && Boolean(onToggleSelect)}
    onclick={() => onSelect(bounty.id)}
    aria-label={`${bounty.title} (${statusLabel})`}
  >
    <div class="bounty-card-main">
      <div class="bounty-card-header">
        <div class="bounty-card-badges">
          {#if bounty.status !== 'OPEN'}
            <span class="status-pill" data-status={bounty.status}>
              <span class="status-pill-dot" aria-hidden="true"></span>
              <span>{statusLabel}</span>
            </span>
          {/if}
          {#if bounty.status === 'DRAFT' && bounty.draft_needs_human_review}
            <span class="draft-review-pill" data-slot="card-draft-review">
              <span class="status-pill-dot" aria-hidden="true"></span>
              <span>{$t('待人工审核')}</span>
            </span>
          {/if}
          {#if bounty.is_blocked}
            <span class="blocked-pill" data-slot="card-blocked">
              <span class="status-pill-dot" aria-hidden="true"></span>
              <span>{$t('等待前置 ({count})', { count: blockedRemainingCount })}</span>
            </span>
          {/if}
          {#if bounty.parent_bounty_id}
            <span class="derived-draft-pill" data-slot="card-derived-draft">
              <span class="status-pill-dot" aria-hidden="true"></span>
              <span>{$t('派生子单')}</span>
            </span>
          {/if}
          {#if bounty.require_arbitration}
            <span class="arbitration-pill" data-slot="card-arbitration" title={$t('仲裁开启')}>
              <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <path d="M8 2.5v11" />
                <path d="M4.5 13.5h7" />
                <path d="M3 5.5h10" />
                <path d="M3 5.5 1.75 9.25a1.65 1.65 0 0 0 2.5 0L3 5.5Z" />
                <path d="M13 5.5 11.75 9.25a1.65 1.65 0 0 0 2.5 0L13 5.5Z" />
              </svg>
              <span>{$t('仲裁')}</span>
            </span>
          {/if}
          {#if (bounty.rejection_count ?? 0) > 0}
            <span class="rejection-pill" data-slot="card-rejection-count">
              <span class="status-pill-dot" aria-hidden="true"></span>
              <span>{$t('已打回 {count}/4 次', { count: bounty.rejection_count })}</span>
            </span>
          {/if}
          {#if bounty.status === 'COMPLETED' && qualityLabel}
            <span
              class="quality-pill"
              data-slot="card-quality-grade"
              data-quality-grade={bounty.quality_grade}
            >
              <span class="status-pill-dot" aria-hidden="true"></span>
              <span>{qualityLabel}</span>
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
          <span
            class="bounty-comment-pill"
            data-slot="card-comment-count"
            title={$t('评论 {count}', { count: bounty.comment_count })}
            aria-label={$t('评论 {count}', { count: bounty.comment_count })}
          >
            <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" aria-hidden="true">
              <path d="M3 3.5h10v7H7.5L4.5 13v-2.5H3z" />
            </svg>
            <span>{bounty.comment_count}</span>
          </span>
        {/if}
      </div>

      <div class="bounty-card-body">
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

        {#if publisher.isAgent || collaborator || executor || arbitrator || bounty.status === 'PENDING_ARBITRATION' || bounty.status === 'WAITING_INFO' || leaseCountdown || arbitrationCountdown || bounty.assignee_mode === 'specific'}
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
            {#if bounty.assignee_mode === 'specific'}
              <span class="bounty-card-constraint">{assigneeLabel}</span>
            {/if}
          </div>
        {/if}
      </div>

      <div class="bounty-card-meta">
        <span class="bounty-card-project">
          <ProjectIcon icon={project?.icon ?? undefined} />
          <span class="bounty-card-project-name">{project?.name ?? $t('未命名项目')}</span>
        </span>
        {#if timeLabel}
          <time class="bounty-card-time" datetime={bounty.updated_at}>{timeLabel}</time>
        {/if}
      </div>
    </div>

    <div class="bounty-card-stub" data-slot="card-rank-stub">
      <span class="bounty-card-stub-code" data-slot="card-short-id">{shortCode}</span>
      <div class="bounty-card-stub-main">
        <span
          class="bounty-card-stub-frame"
          data-difficulty={bounty.difficulty}
          aria-hidden="true"
        >
          {#if bounty.difficulty === 'C'}
            <svg width="64" height="64" viewBox="0 0 64 64" fill="none" stroke="currentColor">
              <circle cx="32" cy="32" r="27" stroke-width="1.15" />
            </svg>
          {:else if bounty.difficulty === 'B'}
            <svg width="64" height="64" viewBox="0 0 64 64" fill="none" stroke="currentColor">
              <circle cx="32" cy="32" r="27" stroke-width="1.2" />
              <circle cx="32" cy="32" r="22.5" stroke-width="0.8" stroke-dasharray="2 3" stroke-opacity="0.7" />
            </svg>
          {:else if bounty.difficulty === 'A'}
            <svg width="64" height="64" viewBox="0 0 64 64" fill="none" stroke="currentColor">
              <circle cx="32" cy="32" r="27" stroke-width="1.35" />
              <circle cx="32" cy="32" r="22.5" stroke-width="0.95" stroke-dasharray="2.5 2.5" />
            </svg>
          {:else}
            <svg width="64" height="64" viewBox="0 0 64 64" fill="none" stroke="currentColor">
              <circle cx="32" cy="32" r="27" stroke-width="1.6" />
              <circle cx="32" cy="32" r="23" stroke-width="1" stroke-dasharray="2.5 2.5" />
              <circle cx="32" cy="32" r="19.5" stroke-width="0.6" stroke-opacity="0.55" />
            </svg>
          {/if}
        </span>
        <span
          class="bounty-card-stub-rank"
          data-difficulty={bounty.difficulty}
          title={$t('预估难度')}
        >
          {bounty.difficulty}
        </span>
      </div>
      <div class="bounty-card-stub-foot" aria-hidden={bounty.assignee_mode !== 'min_tier'}>
        {#if bounty.assignee_mode === 'min_tier'}
          <span class="bounty-card-stub-tier" data-slot="card-min-tier">
            {$t('门槛 ≥ {tier}', { tier: bounty.assignee_min_tier ?? 'C' })}
          </span>
        {:else}
          <span class="bounty-card-stub-dash" aria-hidden="true"></span>
        {/if}
      </div>
    </div>
  </button>
  <span class="bounty-card-notch is-top" aria-hidden="true"></span>
  <span class="bounty-card-notch is-bottom" aria-hidden="true"></span>
</article>
