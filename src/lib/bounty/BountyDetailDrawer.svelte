<script lang="ts">
  /**
   * 文件职责：承载右侧滑出的任务详情抽屉。
   * 定义范围：展示任务状态与元数据、状态转换动作栏（发布/撤回/编辑/取消）、Markdown+KaTeX 正文与验收条件渲染及状态流转时间线。
   */
  import { onMount } from 'svelte';
  import type {
    AgentSummary,
    ApproveArbitrationInput,
    BountyDetail,
    BountyDifficulty,
    BountyProject,
    BountyTask,
    QualityGrade,
    RejectArbitrationInput,
  } from '../api/types';
  import { observeAnchoredPopover, positionAnchoredPopover } from '../anchored-popover';
  import { locale, t } from '../i18n';
  import ProjectIcon from '../ProjectIcon.svelte';
  import {
    computeActiveDurationMs,
    formatActiveDuration,
    formatAgentTier,
    formatAssigneeConstraint,
    formatBountyArbitrator,
    formatBountyExecutor,
    formatBountyPublisher,
    formatBountyStatus,
    formatCommentAuthorRole,
    formatCommentKind,
    formatCompletionCost,
    formatDraftCollaborator,
    formatEventActor,
    formatEventLabel,
    formatLeaseCountdown,
    formatQualityGrade,
    formatServiceError,
    formatTimestamp,
    renderMarkdownAction,
  } from './helpers';

  interface Props {
    detail: BountyDetail;
    project?: BountyProject;
    agents?: AgentSummary[];
    allBounties?: BountyTask[];
    onClose: () => void;
    onEditDraft: (bounty: BountyTask) => void;
    onPublish: (bountyId: string) => Promise<void>;
    onWithdraw: (bountyId: string) => Promise<void>;
    onCancel: (bountyId: string) => Promise<void>;
    onReleaseLease: (bountyId: string) => Promise<void>;
    onInviteCollaborator?: (bountyId: string, agentId: string | null) => Promise<void>;
    onSelectBounty?: (bountyId: string) => Promise<void> | void;
    onPublishBatch?: (bountyIds: string[]) => Promise<void>;
    onCreateComment?: (bountyId: string, body: string) => Promise<void>;
    onApproveArbitration?: (bountyId: string, input: ApproveArbitrationInput) => Promise<void>;
    onRejectArbitration?: (bountyId: string, input: RejectArbitrationInput) => Promise<void>;
  }

  let {
    detail,
    project,
    agents = [],
    allBounties = [],
    onClose,
    onEditDraft,
    onPublish,
    onWithdraw,
    onCancel,
    onReleaseLease,
    onInviteCollaborator,
    onSelectBounty,
    onPublishBatch,
    onCreateComment,
    onApproveArbitration,
    onRejectArbitration,
  }: Props = $props();

  const FOCUSABLE_SELECTOR =
    'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  let busy = $state(false);
  let actionError = $state('');
  let nowMs = $state(Date.now());
  let commentDraft = $state('');
  let commentSubmitting = $state(false);
  let commentError = $state('');
  let approveGrade = $state<QualityGrade>('perfect');
  let approveDifficulty = $state<'' | BountyDifficulty>('');
  let approveComment = $state('');
  let rejectComment = $state('');
  let arbitrationError = $state('');
  let selectedCollaboratorId = $state('');
  let collaboratorError = $state('');
  let lastBountyId = '';
  let lastArbitrationStateKey = '';
  let lastCollaboratorKey = '';

  let drawerEl = $state<HTMLDivElement | null>(null);
  let collaboratorHelpButton = $state<HTMLButtonElement | undefined>(undefined);
  let collaboratorHelpBubble = $state<HTMLDivElement | undefined>(undefined);
  let collaboratorHelpOpen = $state(false);
  let returnFocusEl: HTMLElement | null =
    typeof document !== 'undefined' && document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
  let isDestroyed = false;

  const bounty = $derived(detail.bounty);
  const events = $derived(detail.events);
  const comments = $derived(detail.comments ?? []);
  const blockedBySummaries = $derived(bounty.blocked_by_summaries ?? []);
  const blockDownstreams = $derived(detail.blocks ?? []);
  const childBounties = $derived(detail.child_bounties ?? []);

  function getFocusableElements(container: HTMLElement): HTMLElement[] {
    return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
      el => !el.hasAttribute('disabled') && !el.closest('[hidden], [inert]'),
    );
  }

  function isTopmostDialog(): boolean {
    if (isDestroyed || !drawerEl || typeof document === 'undefined') return false;
    const modals = Array.from(
      document.querySelectorAll<HTMLElement>('[role="dialog"][aria-modal="true"]'),
    );
    return modals.length > 0 && modals[modals.length - 1] === drawerEl;
  }

  function focusIntoDrawer(): void {
    if (!drawerEl) return;
    const target = getFocusableElements(drawerEl)[0] ?? drawerEl;
    target.focus();
  }

  function closeCollaboratorHelp(): void {
    collaboratorHelpOpen = false;
    if (
      typeof collaboratorHelpBubble?.hidePopover === 'function' &&
      collaboratorHelpBubble.matches(':popover-open')
    ) {
      collaboratorHelpBubble.hidePopover();
    }
  }

  function toggleCollaboratorHelp(): void {
    if (!collaboratorHelpButton || !collaboratorHelpBubble) return;
    collaboratorHelpOpen = !collaboratorHelpOpen;
    if (!collaboratorHelpOpen) {
      closeCollaboratorHelp();
      return;
    }
    if (
      typeof collaboratorHelpBubble.showPopover === 'function' &&
      !collaboratorHelpBubble.matches(':popover-open')
    ) {
      collaboratorHelpBubble.showPopover();
    }
    positionAnchoredPopover(
      collaboratorHelpButton,
      collaboratorHelpBubble,
      closeCollaboratorHelp,
    );
  }

  onMount(() => {
    if (isTopmostDialog()) {
      const currentFocus = document.activeElement;
      if (!(currentFocus instanceof Node) || !drawerEl?.contains(currentFocus)) {
        focusIntoDrawer();
      }
    }
    const docBody = drawerEl?.ownerDocument?.body ?? null;
    const modalObserver =
      typeof MutationObserver !== 'undefined' && docBody
        ? new MutationObserver(() => {
            if (isDestroyed || !drawerEl || !isTopmostDialog()) return;
            const currentFocus = document.activeElement;
            if (!(currentFocus instanceof Node) || !drawerEl.contains(currentFocus)) {
              focusIntoDrawer();
            }
          })
        : null;
    modalObserver?.observe(docBody!, { childList: true, subtree: true });
    return () => {
      isDestroyed = true;
      modalObserver?.disconnect();
      const opener = returnFocusEl;
      returnFocusEl = null;
      if (opener && opener.isConnected) {
        opener.focus();
      }
    };
  });

  $effect(() => {
    if (bounty.id !== lastBountyId) {
      lastBountyId = bounty.id;
      commentDraft = '';
      commentError = '';
      actionError = '';
    }
    const collabKey = `${bounty.id}:${bounty.draft_collaborator_agent_id ?? ''}`;
    if (collabKey !== lastCollaboratorKey) {
      lastCollaboratorKey = collabKey;
      selectedCollaboratorId = bounty.draft_collaborator_agent_id ?? '';
      collaboratorError = '';
    }
    const arbitrationKey = `${bounty.id}:${bounty.status}:${bounty.rejection_count}`;
    if (arbitrationKey !== lastArbitrationStateKey) {
      lastArbitrationStateKey = arbitrationKey;
      approveGrade = 'perfect';
      approveDifficulty = '';
      approveComment = '';
      rejectComment = '';
      arbitrationError = '';
    }
  });

  $effect(() => {
    void bounty.id;
    void bounty.status;
    void detail;
    if (!drawerEl || !isTopmostDialog()) return;
    const currentFocus = document.activeElement;
    if (!(currentFocus instanceof Node) || !drawerEl.contains(currentFocus)) {
      focusIntoDrawer();
    }
  });

  $effect(() => {
    if (bounty.status !== 'DRAFT') {
      closeCollaboratorHelp();
    }
  });

  $effect(() => {
    if (!collaboratorHelpOpen || !collaboratorHelpButton || !collaboratorHelpBubble) return;
    const stopObserve = observeAnchoredPopover(
      collaboratorHelpButton,
      collaboratorHelpBubble,
      closeCollaboratorHelp,
    );
    const onKeydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        closeCollaboratorHelp();
      }
    };
    document.addEventListener('keydown', onKeydown, true);
    return () => {
      document.removeEventListener('keydown', onKeydown, true);
      stopObserve();
    };
  });

  $effect(() => {
    const needsLiveTimer =
      bounty.status === 'IN_PROGRESS' ||
      (bounty.status === 'PENDING_ARBITRATION' && Boolean(bounty.arbitration_expires_at)) ||
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

  const collaborator = $derived.by(() => {
    const _lang = $locale;
    return formatDraftCollaborator(bounty);
  });

  const activeAgents = $derived(agents.filter(agent => agent.status === 'active'));

  const parentBountySummary = $derived(
    bounty.parent_bounty_id
      ? (allBounties.find(item => item.id === bounty.parent_bounty_id) ?? null)
      : null,
  );

  const completedBlockersCount = $derived(
    blockedBySummaries.filter(item => item.status === 'COMPLETED').length,
  );

  const blockedRemainingCount = $derived.by(() => {
    const incomplete = blockedBySummaries.filter(item => item.status !== 'COMPLETED').length;
    if (incomplete > 0) return incomplete;
    return bounty.is_blocked ? Math.max(1, (bounty.blocked_by ?? []).length) : 0;
  });

  const childDraftIds = $derived(
    childBounties.filter(item => item.status === 'DRAFT').map(item => item.id),
  );

  const showCoCreationSection = $derived(
    bounty.status === 'DRAFT' ||
      Boolean(bounty.draft_collaborator_agent_id) ||
      Boolean(bounty.parent_bounty_id),
  );

  const showDependenciesSection = $derived(
    blockedBySummaries.length > 0 || blockDownstreams.length > 0 || childBounties.length > 0,
  );

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

  const assignmentCountdown = $derived.by(() => {
    const _lang = $locale;
    if (bounty.status !== 'OPEN' || bounty.assignee_mode !== 'specific') return null;
    return formatLeaseCountdown(bounty.assignment_expires_at, nowMs);
  });

  const qualityLabel = $derived.by(() => {
    const _lang = $locale;
    return bounty.quality_grade ? formatQualityGrade(bounty.quality_grade) : null;
  });

  const activeDurationMs = $derived(computeActiveDurationMs(bounty, nowMs));

  const showActiveDuration = $derived(
    bounty.status === 'IN_PROGRESS' ||
      bounty.status === 'WAITING_INFO' ||
      bounty.status === 'PENDING_ARBITRATION' ||
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

  async function submitCollaboratorChange(agentId: string | null): Promise<void> {
    if (!onInviteCollaborator || busy) return;
    collaboratorError = '';
    actionError = '';
    busy = true;
    try {
      await onInviteCollaborator(bounty.id, agentId);
    } catch (error) {
      collaboratorError = formatServiceError(error);
    } finally {
      busy = false;
    }
  }

  async function submitApproveArbitration(): Promise<void> {
    if (!onApproveArbitration || busy) return;
    arbitrationError = '';
    actionError = '';
    busy = true;
    try {
      const trimmedComment = approveComment.trim();
      const input: ApproveArbitrationInput = {
        quality_grade: approveGrade,
        ...(approveDifficulty ? { calibrated_difficulty: approveDifficulty } : {}),
        ...(trimmedComment ? { comment: trimmedComment } : {}),
      };
      await onApproveArbitration(bounty.id, input);
    } catch (error) {
      arbitrationError = formatServiceError(error);
    } finally {
      busy = false;
    }
  }

  async function submitRejectArbitration(): Promise<void> {
    if (!onRejectArbitration || busy) return;
    const trimmed = rejectComment.trim();
    if (!trimmed) {
      arbitrationError = $t('请输入打回原因。');
      return;
    }
    arbitrationError = '';
    actionError = '';
    busy = true;
    try {
      await onRejectArbitration(bounty.id, { comment: trimmed });
      rejectComment = '';
    } catch (error) {
      arbitrationError = formatServiceError(error);
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

  function handleWindowFocusIn(event: FocusEvent): void {
    if (!isTopmostDialog() || !drawerEl) return;
    const target = event.target;
    if (target instanceof Node && drawerEl.contains(target)) return;
    focusIntoDrawer();
  }

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (!isTopmostDialog() || !drawerEl) return;
    if (event.key === 'Tab') {
      const focusables = getFocusableElements(drawerEl);
      if (focusables.length === 0) {
        event.preventDefault();
        drawerEl.focus();
        return;
      }
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      const active = document.activeElement;
      if (!drawerEl.contains(active)) {
        event.preventDefault();
        (event.shiftKey ? last : first).focus();
        return;
      }
      if (event.shiftKey && (active === first || active === drawerEl)) {
        event.preventDefault();
        last.focus();
        return;
      }
      if (!event.shiftKey && active === last) {
        event.preventDefault();
        first.focus();
      }
      return;
    }
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      if (collaboratorHelpOpen) {
        closeCollaboratorHelp();
        return;
      }
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} onfocusin={handleWindowFocusIn} />

<div
  class="drawer-backdrop"
  role="presentation"
  onclick={event => {
    if (event.target === event.currentTarget) onClose();
  }}
>
  <div
    bind:this={drawerEl}
    tabindex="-1"
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
          <span class="status-pill" data-status={bounty.status}>
            {statusLabel}
          </span>
          <span class="difficulty-badge" data-difficulty={bounty.difficulty}>
            {bounty.difficulty}
          </span>
          {#if bounty.status === 'DRAFT' && bounty.draft_needs_human_review}
            <span class="draft-review-pill" data-slot="detail-draft-review">
              {$t('待人工审核')}
            </span>
          {/if}
          {#if bounty.is_blocked}
            <span class="blocked-pill" data-slot="detail-blocked">
              {$t('等待前置 ({count})', { count: blockedRemainingCount })}
            </span>
          {/if}
          {#if bounty.parent_bounty_id}
            <span class="derived-draft-pill" data-slot="detail-derived-draft">
              {$t('派生子单')}
            </span>
          {/if}
          {#if bounty.require_arbitration}
            <span class="arbitration-pill">{$t('仲裁开启')}</span>
          {/if}
          {#if (bounty.rejection_count ?? 0) > 0}
            <span class="rejection-pill" data-slot="detail-rejection-count">
              {$t('已打回 {count}/4 次', { count: bounty.rejection_count })}
            </span>
          {/if}
          {#if bounty.status === 'COMPLETED' && qualityLabel}
            <span
              class="quality-pill"
              data-slot="detail-quality-grade"
              data-quality-grade={bounty.quality_grade}
            >
              {$t('验收等级: {grade}', { grade: qualityLabel })}
            </span>
          {/if}
          {#if bounty.status === 'COMPLETED' && bounty.calibrated_difficulty}
            <span
              class="difficulty-badge"
              data-slot="detail-calibrated-difficulty"
              data-difficulty={bounty.calibrated_difficulty}
            >
              {$t('校准难度 {difficulty}', { difficulty: bounty.calibrated_difficulty })}
            </span>
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

      <div class="detail-drawer-submeta">
        <span class="bounty-card-project">
          <ProjectIcon icon={project?.icon ?? undefined} />
          <span class="bounty-card-project-name">{project?.name ?? $t('未命名项目')}</span>
        </span>
        <span class="detail-meta-sep" aria-hidden="true"></span>
        <span
          class="detail-meta-inline"
          data-slot="detail-publisher"
          data-publisher-kind={publisher.isAgent ? 'agent' : 'human'}
        >
          {publisher.label}
        </span>
        <span class="detail-meta-sep" aria-hidden="true"></span>
        <span class="detail-meta-time">
          {$t('更新于 {time}', { time: formatTimestamp(bounty.updated_at || bounty.created_at) })}
        </span>
      </div>

      <div class="detail-drawer-meta-grid">
        {#if bounty.assignee_mode !== 'any'}
          <span class="detail-meta-pill">{assigneeLabel}</span>
        {/if}
        {#if executor}
          <span
            class="detail-meta-pill"
            data-slot="detail-executor"
            data-executor-tier={executor.tier ?? 'unknown'}
          >
            {$t('执行者: {agent}', { agent: executor.label })}
          </span>
        {/if}
        {#if arbitrator}
          <span
            class="detail-meta-pill bounty-arbitrator-pill"
            data-slot="detail-arbitrator"
            data-arbitrator-tier={arbitrator.tier ?? 'unknown'}
          >
            {$t('裁判: {agent}', { agent: arbitrator.label })}
          </span>
        {:else if bounty.status === 'PENDING_ARBITRATION'}
          <span class="detail-meta-pill bounty-human-fallback-pill" data-slot="detail-human-fallback">
            {$t('人工仲裁兜底')}
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
        {:else if arbitrationCountdown}
          <span
            class="detail-meta-pill bounty-lease-pill"
            data-slot="detail-arbitration-countdown"
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
              class="bounty-detail-markdown is-flat"
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
              class="bounty-detail-markdown is-flat"
              data-slot="detail-completion-note"
              use:renderMarkdownAction={bounty.completion_note.trim()}
            ></div>
          {:else}
            <p class="detail-empty-hint">{$t('执行者未填写完成说明')}</p>
          {/if}
        </section>
      {/if}

      {#if bounty.status === 'PENDING_ARBITRATION' && (onApproveArbitration || onRejectArbitration)}
        <section class="detail-section detail-arbitration-card" data-slot="detail-arbitration-panel">
          <h3 class="detail-section-title">{$t('仲裁验收裁决')}</h3>

          {#if bounty.rejection_count === 3}
            <p class="composer-field-warning" data-slot="detail-circuit-breaker-warning">
              {$t('第 4 次打回将直接终止本轮交付并重新分发')}
            </p>
          {/if}

          {#if arbitrationError}
            <p class="detail-comment-error" role="alert" data-status="arbitration-error">
              {arbitrationError}
            </p>
          {/if}

          {#if onApproveArbitration}
            <div class="detail-arbitration-block" data-slot="arbitration-approve-form">
              <div class="detail-arbitration-grid">
                <div class="project-form-field">
                  <span class="composer-label" id="arbitration-grade-label">{$t('验收等级')}</span>
                  <div
                    class="segmented-group"
                    role="group"
                    aria-labelledby="arbitration-grade-label"
                  >
                    <button
                      type="button"
                      class="segmented-button"
                      class:active={approveGrade === 'perfect'}
                      aria-pressed={approveGrade === 'perfect'}
                      data-quality-grade="perfect"
                      disabled={busy}
                      onclick={() => {
                        approveGrade = 'perfect';
                      }}
                    >
                      {$t('完美交付')}
                    </button>
                    <button
                      type="button"
                      class="segmented-button"
                      class:active={approveGrade === 'flawed'}
                      aria-pressed={approveGrade === 'flawed'}
                      data-quality-grade="flawed"
                      disabled={busy}
                      onclick={() => {
                        approveGrade = 'flawed';
                      }}
                    >
                      {$t('有瑕疵')}
                    </button>
                  </div>
                </div>

                <div class="project-form-field">
                  <label class="composer-label" for="arbitration-calibrated-difficulty">
                    {$t('校准难度')}
                  </label>
                  <select
                    id="arbitration-calibrated-difficulty"
                    class="composer-select"
                    data-slot="arbitration-calibrated-difficulty"
                    disabled={busy}
                    bind:value={approveDifficulty}
                  >
                    <option value="">
                      {$t('不调整（保持 {difficulty}）', { difficulty: bounty.difficulty })}
                    </option>
                    <option value="C">C</option>
                    <option value="B">B</option>
                    <option value="A">A</option>
                    <option value="S">S</option>
                  </select>
                </div>
              </div>

              <div class="project-form-field">
                <label class="composer-label" for="arbitration-approve-comment">
                  {$t('裁决备注（可选）')}
                </label>
                <input
                  id="arbitration-approve-comment"
                  type="text"
                  class="settings-text-input"
                  data-slot="arbitration-approve-comment"
                  placeholder={$t('裁决备注（可选）')}
                  disabled={busy}
                  bind:value={approveComment}
                />
              </div>

              <div class="detail-arbitration-actions">
                <button
                  type="button"
                  class="primary"
                  data-action="approve-arbitration"
                  disabled={busy}
                  onclick={() => void submitApproveArbitration()}
                >
                  {$t('通过验收')}
                </button>
              </div>
            </div>
          {/if}

          {#if onRejectArbitration}
            <div class="detail-arbitration-block detail-arbitration-reject-block" data-slot="arbitration-reject-form">
              <div class="project-form-field">
                <label class="composer-label" for="arbitration-reject-comment">
                  {$t('打回原因（必填）')}
                </label>
                <textarea
                  id="arbitration-reject-comment"
                  class="detail-comment-input"
                  rows="2"
                  data-slot="arbitration-reject-comment"
                  placeholder={$t('请说明需要修改的具体问题…')}
                  disabled={busy}
                  bind:value={rejectComment}
                ></textarea>
              </div>
              <div class="detail-arbitration-actions">
                <button
                  type="button"
                  class="danger-text"
                  data-action="reject-arbitration"
                  disabled={busy || !rejectComment.trim()}
                  onclick={() => void submitRejectArbitration()}
                >
                  {bounty.rejection_count === 3
                    ? $t('第 4 次打回并重开')
                    : $t('打回修改')}
                </button>
              </div>
            </div>
          {/if}
        </section>
      {/if}

      {#if showCoCreationSection}
        <section class="detail-section detail-cocreation-card" data-slot="detail-draft-cocreation">
          <div class="detail-cocreation-header">
            <h3 class="detail-section-title">{$t('草稿共创与派生')}</h3>
            {#if collaborator}
              <span class="bounty-collaborator-pill" data-slot="detail-collaborator">
                {$t('协作者: {agent}', { agent: collaborator.label })}
              </span>
            {/if}
          </div>

          {#if bounty.status === 'DRAFT' && bounty.draft_needs_human_review}
            <p class="detail-review-banner" role="status" data-slot="detail-draft-review-banner">
              {$t('受邀 Agent 已更新该草稿，请审阅修订内容并在确认后发布。')}
            </p>
          {/if}

          {#if bounty.parent_bounty_id}
            <div class="detail-lineage-row" data-slot="detail-parent-link">
              <span class="detail-lineage-label">{$t('来自父草稿')}</span>
              <button
                type="button"
                class="detail-relation-link"
                data-action="open-parent-bounty"
                data-bounty-id={bounty.parent_bounty_id}
                onclick={() => void onSelectBounty?.(bounty.parent_bounty_id!)}
              >
                {parentBountySummary?.title ?? bounty.parent_bounty_id}
              </button>
            </div>
          {/if}

          {#if bounty.status === 'DRAFT' && onInviteCollaborator}
            <div class="detail-collaborator-controls">
              <div class="project-form-field">
                <div class="settings-label-group">
                  <label class="composer-label" for="detail-collaborator-select">
                    {$t('受邀协作者')}
                  </label>
                  <span class="settings-help">
                    <button
                      bind:this={collaboratorHelpButton}
                      type="button"
                      class="settings-help-trigger"
                      data-action="toggle-collaborator-help"
                      aria-label={$t('关于受邀协作者')}
                      aria-expanded={collaboratorHelpOpen}
                      aria-controls="detail-collaborator-help"
                      onclick={toggleCollaboratorHelp}
                    >
                      <svg
                        width="14"
                        height="14"
                        viewBox="0 0 16 16"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.5"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        aria-hidden="true"
                      >
                        <circle cx="8" cy="8" r="6.25" />
                        <path d="M6.4 6.2a1.7 1.7 0 0 1 3.2.7c0 1.1-1.6 1.4-1.6 2.5" />
                        <circle cx="8" cy="11.8" r=".75" fill="currentColor" stroke="none" />
                      </svg>
                    </button>
                    <div
                      bind:this={collaboratorHelpBubble}
                      id="detail-collaborator-help"
                      class="settings-help-popover dropdown"
                      role="note"
                      popover="manual"
                      hidden={!collaboratorHelpOpen}
                      style="position: fixed; margin: 0; bottom: auto; right: auto; box-sizing: border-box;"
                    >
                      <p>{$t('Agent 修订或评论草稿会标记为待人工审核；管理员发表评论将清除该标记并通知协作者。')}</p>
                    </div>
                  </span>
                </div>
                <div class="detail-collaborator-row">
                  <select
                    id="detail-collaborator-select"
                    class="composer-select"
                    data-slot="collaborator-select"
                    disabled={busy}
                    bind:value={selectedCollaboratorId}
                  >
                    <option value="">{$t('选择活跃 Agent…')}</option>
                    {#if bounty.draft_collaborator_agent_id && !activeAgents.some(a => a.id === bounty.draft_collaborator_agent_id)}
                      <option value={bounty.draft_collaborator_agent_id}>
                        {collaborator?.label ?? bounty.draft_collaborator_agent_id}
                      </option>
                    {/if}
                    {#each activeAgents as agent (agent.id)}
                      <option value={agent.id}>
                        {agent.name} ({formatAgentTier(agent.tier)})
                      </option>
                    {/each}
                  </select>
                  <button
                    type="button"
                    class="primary"
                    data-action="invite-collaborator"
                    disabled={busy ||
                      !selectedCollaboratorId ||
                      selectedCollaboratorId === bounty.draft_collaborator_agent_id}
                    onclick={() => void submitCollaboratorChange(selectedCollaboratorId)}
                  >
                    {bounty.draft_collaborator_agent_id ? $t('更换协作者') : $t('邀请协助草稿')}
                  </button>
                  {#if bounty.draft_collaborator_agent_id}
                    <button
                      type="button"
                      class="secondary-action"
                      data-action="clear-collaborator"
                      disabled={busy}
                      onclick={() => void submitCollaboratorChange(null)}
                    >
                      {$t('取消协作')}
                    </button>
                  {/if}
                </div>
              </div>
              {#if collaboratorError}
                <p class="detail-comment-error" role="alert" data-status="collaborator-error">
                  {collaboratorError}
                </p>
              {/if}
            </div>
          {/if}
        </section>
      {/if}

      {#if showDependenciesSection}
        <section class="detail-section detail-dependencies-card" data-slot="detail-dependencies">
          <h3 class="detail-section-title">{$t('依赖与拆单关系')}</h3>

          {#if blockedBySummaries.length > 0}
            <div class="detail-dependency-group" data-slot="detail-blocked-by-list">
              <div class="detail-dependency-group-header">
                <span class="detail-dependency-group-title">
                  {$t('前置依赖 ({done}/{total})', {
                    done: completedBlockersCount,
                    total: blockedBySummaries.length,
                  })}
                </span>
              </div>
              <div class="detail-relation-list">
                {#each blockedBySummaries as dep (dep.id)}
                  {@const depStatusLabel = (() => {
                    const _lang = $locale;
                    return formatBountyStatus(dep.status, dep.status === 'DRAFT');
                  })()}
                  <button
                    type="button"
                    class="detail-relation-item"
                    data-slot="dependency-blocker-item"
                    data-bounty-id={dep.id}
                    data-bounty-status={dep.status}
                    onclick={() => void onSelectBounty?.(dep.id)}
                  >
                    <span class="status-pill" data-status={dep.status}>{depStatusLabel}</span>
                    <span class="detail-relation-title">{dep.title}</span>
                  </button>
                {/each}
              </div>
            </div>
          {/if}

          {#if blockDownstreams.length > 0}
            <div class="detail-dependency-group" data-slot="detail-blocks-list">
              <div class="detail-dependency-group-header">
                <span class="detail-dependency-group-title">
                  {$t('被我阻塞的下游 ({count})', { count: blockDownstreams.length })}
                </span>
              </div>
              <div class="detail-relation-list">
                {#each blockDownstreams as item (item.id)}
                  {@const itemStatusLabel = (() => {
                    const _lang = $locale;
                    return formatBountyStatus(item.status, item.status === 'DRAFT');
                  })()}
                  <button
                    type="button"
                    class="detail-relation-item"
                    data-slot="dependency-downstream-item"
                    data-bounty-id={item.id}
                    data-bounty-status={item.status}
                    onclick={() => void onSelectBounty?.(item.id)}
                  >
                    <span class="status-pill" data-status={item.status}>{itemStatusLabel}</span>
                    <span class="detail-relation-title">{item.title}</span>
                  </button>
                {/each}
              </div>
            </div>
          {/if}

          {#if childBounties.length > 0}
            <div class="detail-dependency-group" data-slot="detail-children-list">
              <div class="detail-dependency-group-header">
                <span class="detail-dependency-group-title">
                  {$t('派生子单 ({count})', { count: childBounties.length })}
                </span>
                {#if childDraftIds.length > 0 && onPublishBatch}
                  <button
                    type="button"
                    class="primary secondary-action"
                    data-action="publish-derived-drafts"
                    disabled={busy}
                    onclick={() => void runAction(() => onPublishBatch!(childDraftIds))}
                  >
                    {$t('发布全部派生草稿 ({count})', { count: childDraftIds.length })}
                  </button>
                {/if}
              </div>
              <div class="detail-relation-list">
                {#each childBounties as child (child.id)}
                  {@const childStatusLabel = (() => {
                    const _lang = $locale;
                    return formatBountyStatus(child.status, child.status === 'DRAFT');
                  })()}
                  <button
                    type="button"
                    class="detail-relation-item"
                    data-slot="dependency-child-item"
                    data-bounty-id={child.id}
                    data-bounty-status={child.status}
                    onclick={() => void onSelectBounty?.(child.id)}
                  >
                    <span class="status-pill" data-status={child.status}>{childStatusLabel}</span>
                    <span class="detail-relation-title">{child.title}</span>
                  </button>
                {/each}
              </div>
            </div>
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
