<script lang="ts">
  /**
   * 文件职责：承载右侧滑出的悬赏任务详情抽屉。
   * 定义范围：展示任务状态与元数据、状态转换动作栏（发布/撤回/编辑/取消）、Markdown+KaTeX 正文与验收条件渲染及状态流转时间线。
   */
  import type { BountyDetail, BountyProject, BountyTask } from '../api/types';
  import { locale, t } from '../i18n';
  import ProjectIcon from '../ProjectIcon.svelte';
  import {
    formatAssigneeConstraint,
    formatBountyPublisher,
    formatBountyStatus,
    formatEventActor,
    formatEventLabel,
    formatServiceError,
    formatTimestamp,
    renderMarkdownAction,
  } from './helpers';

  interface Props {
    detail: BountyDetail;
    project?: BountyProject;
    onClose: () => void;
    onEditDraft: (bounty: BountyTask) => void;
    onPublish: (bountyId: string) => Promise<void>;
    onWithdraw: (bountyId: string) => Promise<void>;
    onCancel: (bountyId: string) => Promise<void>;
  }

  let {
    detail,
    project,
    onClose,
    onEditDraft,
    onPublish,
    onWithdraw,
    onCancel,
  }: Props = $props();

  let busy = $state(false);
  let actionError = $state('');

  const bounty = $derived(detail.bounty);
  const events = $derived(detail.events);

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

  async function runAction(fn: () => Promise<void>): Promise<void> {
    if (busy) return;
    actionError = '';
    busy = true;
    try {
      await fn();
    } catch (error) {
      actionError = formatServiceError(error);
    } finally {
      busy = false;
    }
  }

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

<div
  class="drawer-backdrop"
  role="presentation"
  onclick={event => {
    if (event.target === event.currentTarget) onClose();
  }}
>
  <div
    class="detail-drawer"
    role="dialog"
    aria-modal="true"
    aria-label={bounty.title}
    data-dialog="bounty-detail"
    data-bounty-status={bounty.status}
  >
    <header class="detail-drawer-header">
      <div class="detail-drawer-meta-top">
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
        <button
          type="button"
          class="icon-button"
          aria-label={$t('关闭对话框')}
          onclick={onClose}
        >
          ✕
        </button>
      </div>

      <h2 class="detail-drawer-title">{bounty.title}</h2>

      <div class="detail-drawer-meta-grid">
        <span class="bounty-card-project">
          <ProjectIcon icon={project?.icon ?? undefined} />
          <span class="bounty-card-project-name">{project?.name ?? $t('未命名项目')}</span>
        </span>
        <span
          class="detail-meta-pill"
          data-slot="detail-publisher"
          data-publisher-kind={publisher.isAgent ? 'agent' : 'human'}
        >
          {publisher.label}
        </span>
        <span class="detail-meta-pill">{assigneeLabel}</span>
        {#if bounty.lease_agent_id}
          <span class="detail-meta-pill">
            {$t('执行者: {agent}', { agent: bounty.lease_agent_id })}
          </span>
        {/if}
        <span class="detail-meta-time">
          {$t('更新于 {time}', { time: formatTimestamp(bounty.updated_at || bounty.created_at) })}
        </span>
      </div>

      {#if bounty.status === 'DRAFT' || bounty.status === 'OPEN' || bounty.status === 'WAITING_INFO'}
        <div class="detail-drawer-actions" data-slot="detail-actions">
          {#if bounty.status === 'DRAFT'}
            <button
              type="button"
              data-action="edit-draft"
              disabled={busy}
              onclick={() => onEditDraft(bounty)}
            >
              {$t('编辑草稿')}
            </button>
            <button
              type="button"
              class="primary"
              data-action="publish-bounty"
              disabled={busy}
              onclick={() => void runAction(() => onPublish(bounty.id))}
            >
              {$t('立即发布')}
            </button>
            <button
              type="button"
              class="danger-text"
              data-action="cancel-bounty"
              disabled={busy}
              onclick={() => void runAction(() => onCancel(bounty.id))}
            >
              {$t('丢弃草稿')}
            </button>
          {:else if bounty.status === 'OPEN'}
            <button
              type="button"
              data-action="withdraw-bounty"
              disabled={busy}
              onclick={() => void runAction(() => onWithdraw(bounty.id))}
            >
              {$t('撤回为草稿')}
            </button>
            <button
              type="button"
              class="danger-text"
              data-action="cancel-bounty"
              disabled={busy}
              onclick={() => void runAction(() => onCancel(bounty.id))}
            >
              {$t('取消任务')}
            </button>
          {:else if bounty.status === 'WAITING_INFO'}
            <button
              type="button"
              class="danger-text"
              data-action="cancel-bounty"
              disabled={busy}
              onclick={() => void runAction(() => onCancel(bounty.id))}
            >
              {$t('取消任务')}
            </button>
          {/if}
        </div>
      {/if}

      {#if actionError}
        <div class="drawer-error-banner" role="alert" data-status="detail-action-error">
          <span>{actionError}</span>
        </div>
      {/if}
    </header>

    <div class="detail-drawer-body">
      <section class="detail-section">
        <h3 class="detail-section-title">{$t('任务详情')}</h3>
        <div
          class="bounty-detail-markdown"
          data-slot="detail-description"
          use:renderMarkdownAction={bounty.description || $t('暂无详情描述')}
        ></div>
      </section>

      <section class="detail-section">
        <h3 class="detail-section-title">{$t('验收条件')}</h3>
        <div
          class="bounty-detail-markdown"
          data-slot="detail-criteria"
          use:renderMarkdownAction={bounty.acceptance_criteria || $t('暂无验收条件')}
        ></div>
      </section>

      <section class="detail-section" data-slot="detail-events">
        <h3 class="detail-section-title">{$t('状态流转记录')}</h3>
        {#if events.length === 0}
          <p class="detail-empty-hint">{$t('暂无流转记录')}</p>
        {:else}
          <ol class="detail-timeline">
            {#each events as item (item.id)}
              {@const eventActionLabel = (() => {
                const _lang = $locale;
                return formatEventLabel(item.event_type);
              })()}
              {@const actorLabel = (() => {
                const _lang = $locale;
                return formatEventActor(item.actor, bounty);
              })()}
              {@const transitionLabel = (() => {
                const _lang = $locale;
                return item.from_status
                  ? `${formatBountyStatus(item.from_status)} → ${formatBountyStatus(item.to_status)}`
                  : formatBountyStatus(item.to_status);
              })()}
              <li class="detail-timeline-item" data-event-type={item.event_type}>
                <div class="detail-timeline-main">
                  <span class="detail-timeline-action">{eventActionLabel}</span>
                  <span class="detail-timeline-actor">{actorLabel}</span>
                  <span class="detail-timeline-transition">{transitionLabel}</span>
                </div>
                <time class="detail-timeline-time" datetime={item.created_at}>
                  {formatTimestamp(item.created_at)}
                </time>
              </li>
            {/each}
          </ol>
        {/if}
      </section>
    </div>
  </div>
</div>
