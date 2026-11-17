<script lang="ts">
  /**
   * 文件职责：承载右侧滑出的悬赏任务详情抽屉。
   * 定义范围：展示任务状态与元数据、状态转换动作栏（发布/撤回/编辑/取消）、Markdown+KaTeX 正文与验收条件渲染及状态流转时间线。
   */
  import type { BountyDetail, BountyProject, BountyTask } from '../api/types';
  import { locale, t } from '../i18n';
  import ProjectIcon from '../ProjectIcon.svelte';
  import {
    computeActiveDurationMs,
    formatActiveDuration,
    formatAssigneeConstraint,
    formatBountyExecutor,
    formatBountyPublisher,
    formatBountyStatus,
    formatCommentAuthorRole,
    formatCommentKind,
    formatCompletionCost,
    formatEventActor,
    formatEventLabel,
    formatLeaseCountdown,
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
    onReleaseLease: (bountyId: string) => Promise<void>;
    onCreateComment?: (bountyId: string, body: string) => Promise<void>;
  }

  let {
    detail,
    project,
    onClose,
    onEditDraft,
    onPublish,
    onWithdraw,
    onCancel,
    onReleaseLease,
    onCreateComment,
  }: Props = $props();

  let busy = $state(false);
  let actionError = $state('');
  let nowMs = $state(Date.now());
  let commentDraft = $state('');
  let commentSubmitting = $state(false);
  let commentError = $state('');
  let lastBountyId = '';

  const bounty = $derived(detail.bounty);
  const events = $derived(detail.events);
  const comments = $derived(detail.comments ?? []);

  $effect(() => {
    if (bounty.id !== lastBountyId) {
      lastBountyId = bounty.id;
      commentDraft = '';
      commentError = '';
    }
  });

  $effect(() => {
    const needsLiveTimer =
      bounty.status === 'IN_PROGRESS' ||
      (bounty.status === 'OPEN' &&
        bounty.assignee_mode === 'specific' &&
        Boolean(bounty.assignment_expires_at));
    nowMs = Date.now();
    if (!needsLiveTimer) return;
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

  const executor = $derived.by(() => {
    const _lang = $locale;
    return formatBountyExecutor(bounty);
  });

  const leaseCountdown = $derived.by(() => {
    const _lang = $locale;
    if (bounty.status !== 'IN_PROGRESS') return null;
    return formatLeaseCountdown(bounty.lease_expires_at, nowMs);
  });

  const assignmentCountdown = $derived.by(() => {
    const _lang = $locale;
    if (bounty.status !== 'OPEN' || bounty.assignee_mode !== 'specific') return null;
    return formatLeaseCountdown(bounty.assignment_expires_at, nowMs);
  });

  const activeDurationMs = $derived(computeActiveDurationMs(bounty, nowMs));

  const showActiveDuration = $derived(
    bounty.status === 'IN_PROGRESS' ||
      bounty.status === 'WAITING_INFO' ||
      activeDurationMs > 0,
  );

  const activeDurationLabel = $derived(formatActiveDuration(activeDurationMs));

  const pendingQuestion = $derived(
    bounty.status === 'WAITING_INFO'
      ? ([...comments].reverse().find(item => item.kind === 'question') ?? null)
      : null,
  );

  const completionCostLabel = $derived.by(() => {
    const _lang = $locale;
    return formatCompletionCost(bounty.completion_cost);
  });

  const hasCompletionSection = $derived(
    bounty.status === 'COMPLETED' ||
      Boolean(bounty.completion_note?.trim()) ||
      completionCostLabel !== null,
  );

  const canReleaseLease = $derived(
    bounty.status === 'IN_PROGRESS' ||
      ((bounty.status === 'WAITING_INFO' || bounty.status === 'PENDING_ARBITRATION') &&
        bounty.lease_agent_id !== null),
  );

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

  async function submitComment(): Promise<void> {
    if (!onCreateComment || commentSubmitting) return;
    const trimmed = commentDraft.trim();
    if (!trimmed) {
      commentError = $t('请输入评论内容。');
      return;
    }
    commentError = '';
    commentSubmitting = true;
    try {
      await onCreateComment(bounty.id, trimmed);
      commentDraft = '';
    } catch (error) {
      commentError = formatServiceError(error);
    } finally {
      commentSubmitting = false;
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
        {#if executor}
          <span
            class="detail-meta-pill"
            data-slot="detail-executor"
            data-executor-tier={executor.tier ?? 'unknown'}
          >
            {$t('执行者: {agent}', { agent: executor.label })}
          </span>
        {/if}
        {#if bounty.status === 'WAITING_INFO'}
          <span
            class="detail-meta-pill bounty-frozen-pill"
            data-slot="detail-lease-frozen"
            data-lease-state="frozen"
          >
            {$t('租约已冻结（等待回复）')}
          </span>
        {:else if leaseCountdown}
          <span
            class="detail-meta-pill bounty-lease-pill"
            data-slot="detail-lease-countdown"
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
        {:else if assignmentCountdown}
          <span
            class="detail-meta-pill bounty-lease-pill"
            data-slot="detail-assignment-countdown"
            data-lease-state={assignmentCountdown.expired
              ? 'expired'
              : assignmentCountdown.expiringSoon
                ? 'expiring'
                : 'active'}
          >
            {assignmentCountdown.expired
              ? $t('指派窗口已超时待回退')
              : $t('指派响应剩余 {countdown}', { countdown: assignmentCountdown.label })}
          </span>
        {/if}
        {#if showActiveDuration}
          <span
            class="detail-meta-pill detail-active-timer"
            data-slot="detail-active-duration"
            data-timer-state={bounty.status === 'WAITING_INFO'
              ? 'frozen'
              : bounty.status === 'IN_PROGRESS'
                ? 'running'
                : 'settled'}
          >
            {bounty.status === 'WAITING_INFO'
              ? $t('有效耗时已冻结 {duration}', { duration: activeDurationLabel })
              : $t('有效耗时 {duration}', { duration: activeDurationLabel })}
          </span>
        {/if}
        <span class="detail-meta-time">
          {$t('更新于 {time}', { time: formatTimestamp(bounty.updated_at || bounty.created_at) })}
        </span>
      </div>

      {#if bounty.status === 'DRAFT' || bounty.status === 'OPEN' || bounty.status === 'WAITING_INFO' || canReleaseLease}
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
          {:else}
            {#if canReleaseLease}
              <button
                type="button"
                class="danger-text"
                data-action="release-lease"
                disabled={busy}
                onclick={() => void runAction(() => onReleaseLease(bounty.id))}
              >
                {$t('强制释放租约')}
              </button>
            {/if}
            {#if bounty.status === 'WAITING_INFO'}
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
      {#if bounty.status === 'WAITING_INFO'}
        <section class="detail-section detail-pending-question" data-slot="detail-pending-question">
          <div class="detail-pending-question-header">
            <h3 class="detail-section-title">
              {pendingQuestion
                ? $t('待回答提问（{author}）', { author: pendingQuestion.author_name })
                : $t('待回答问题')}
            </h3>
            {#if pendingQuestion}
              <time class="detail-timeline-time" datetime={pendingQuestion.created_at}>
                {formatTimestamp(pendingQuestion.created_at)}
              </time>
            {/if}
          </div>
          {#if pendingQuestion}
            <div
              class="bounty-detail-markdown"
              data-slot="detail-pending-question-body"
              use:renderMarkdownAction={pendingQuestion.body}
            ></div>
          {/if}
        </section>
      {/if}

      {#if hasCompletionSection}
        <section class="detail-section detail-completion-card" data-slot="detail-completion">
          <div class="detail-completion-header">
            <h3 class="detail-section-title">{$t('完成说明')}</h3>
            {#if completionCostLabel !== null}
              <span class="detail-meta-pill" data-slot="detail-completion-cost">
                {$t('成本 {cost}', { cost: completionCostLabel })}
              </span>
            {/if}
          </div>
          {#if bounty.completion_note?.trim()}
            <div
              class="bounty-detail-markdown"
              data-slot="detail-completion-note"
              use:renderMarkdownAction={bounty.completion_note.trim()}
            ></div>
          {:else}
            <p class="detail-empty-hint">{$t('执行者未填写完成说明')}</p>
          {/if}
        </section>
      {/if}

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

      <section class="detail-section detail-comments-section" data-slot="detail-comments">
        <h3 class="detail-section-title">
          {$t('评论与讨论 ({count})', { count: comments.length })}
        </h3>
        {#if comments.length === 0}
          <p class="detail-empty-hint">{$t('暂无评论或澄清记录')}</p>
        {:else}
          <ol class="detail-comments-list" data-slot="detail-comments-list">
            {#each comments as comment (comment.id)}
              {@const kindLabel = (() => {
                const _lang = $locale;
                return formatCommentKind(comment.kind);
              })()}
              {@const roleLabel = (() => {
                const _lang = $locale;
                return formatCommentAuthorRole(comment.author_type);
              })()}
              <li
                class="detail-comment-item"
                class:is-question={comment.kind === 'question'}
                class:is-reply={comment.kind === 'reply'}
                data-comment-id={comment.id}
                data-comment-kind={comment.kind}
                data-author-type={comment.author_type}
              >
                <div class="detail-comment-header">
                  <div class="detail-comment-author">
                    <span class="detail-comment-author-name">{comment.author_name}</span>
                    <span class="detail-comment-role">{roleLabel}</span>
                    <span class="detail-comment-kind" data-kind={comment.kind}>{kindLabel}</span>
                  </div>
                  <time class="detail-timeline-time" datetime={comment.created_at}>
                    {formatTimestamp(comment.created_at)}
                  </time>
                </div>
                <div
                  class="bounty-detail-markdown detail-comment-body"
                  data-slot="comment-body"
                  use:renderMarkdownAction={comment.body}
                ></div>
              </li>
            {/each}
          </ol>
        {/if}

        {#if onCreateComment}
          {#if bounty.status === 'CANCELLED'}
            <p class="detail-comment-disabled" data-slot="comment-disabled-notice">
              {$t('任务已取消，无法继续发表评论')}
            </p>
          {:else}
            <form
              class="detail-comment-composer"
              data-slot="comment-composer"
              onsubmit={event => {
                event.preventDefault();
                void submitComment();
              }}
            >
              <textarea
                class="detail-comment-input"
                data-slot="comment-input"
                rows="3"
                aria-label={$t('评论内容')}
                placeholder={bounty.status === 'WAITING_INFO'
                  ? $t('补充回复以恢复任务执行…')
                  : $t('发表评论（支持 Markdown 与 KaTeX 公式）…')}
                bind:value={commentDraft}
                disabled={commentSubmitting}
                onkeydown={event => {
                  if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
                    event.preventDefault();
                    void submitComment();
                  }
                }}
              ></textarea>
              {#if commentError}
                <p class="detail-comment-error" role="alert" data-status="comment-error">
                  {commentError}
                </p>
              {/if}
              <div class="detail-comment-actions">
                <button
                  type="submit"
                  class="primary"
                  data-action="submit-comment"
                  title={bounty.status === 'WAITING_INFO'
                    ? $t('回复后任务将自动恢复为执行中，并刷新 15 分钟租约。')
                    : undefined}
                  disabled={commentSubmitting || !commentDraft.trim()}
                >
                  {commentSubmitting
                    ? $t('正在发送…')
                    : bounty.status === 'WAITING_INFO'
                      ? $t('回复并恢复执行')
                      : $t('发送评论')}
                </button>
              </div>
            </form>
          {/if}
        {/if}
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
