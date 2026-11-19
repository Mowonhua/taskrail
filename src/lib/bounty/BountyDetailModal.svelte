<script lang="ts">
  /**
   * 文件职责：承载居中的任务详情弹窗（固定顶栏 + 左侧阅读与评论区 + 右侧属性与流转时间轴）。
   * 定义范围：展示任务属性、状态切换与生命周期操作、Markdown+KaTeX 正文与验收清单、两级楼中楼评论区及状态流转时间轴。
   */
  import { onMount, tick } from 'svelte';
  import type {
    AgentSummary,
    ApproveArbitrationInput,
    BountyComment,
    BountyDetail,
    BountyDifficulty,
    BountyEvent,
    BountyProject,
    BountyTask,
    CreateProjectInput,
    QualityGrade,
    RejectArbitrationInput,
    UpdateBountyInput,
    UserProfile,
  } from '../api/types';
  import { observeAnchoredPopover, positionAnchoredPopover } from '../anchored-popover';
  import { locale, t } from '../i18n';
  import ProjectIcon from '../ProjectIcon.svelte';
  import UserAvatar from '../auth/UserAvatar.svelte';
  import BountyEditorForm from './BountyEditorForm.svelte';
  import {
    computeActiveDurationMs,
    formatAcceptanceCriteriaMarkdown,
    formatActiveDuration,
    formatAgentTier,
    formatAssigneeConstraint,
    formatBountyArbitrator,
    formatBountyExecutor,
    formatBountyStatus,
    formatCompactTimestamp,
    formatCompletionCost,
    formatDraftCollaborator,
    formatEventActor,
    formatEventLabel,
    formatLeaseCountdown,
    formatQualityGrade,
    formatServiceError,
    renderMarkdownAction,
  } from './helpers';

  interface Props {
    detail: BountyDetail;
    project?: BountyProject;
    projects?: BountyProject[];
    agents?: AgentSummary[];
    allBounties?: BountyTask[];
    currentUser?: UserProfile | null;
    desktop?: boolean;
    mobile?: boolean;
    onClose: () => void;
    onEditDraft?: (bounty: BountyTask) => void;
    onCreateProject?: (input: CreateProjectInput) => Promise<BountyProject>;
    onUpdateDraft?: (id: string, input: UpdateBountyInput, publishAfter: boolean) => Promise<BountyTask>;
    onPublish: (bountyId: string) => Promise<void>;
    onWithdraw: (bountyId: string) => Promise<void>;
    onCancel: (bountyId: string) => Promise<void>;
    onRestoreDraft: (bountyId: string) => Promise<void>;
    onDelete: (bounty: BountyTask) => Promise<void>;
    onReleaseLease: (bountyId: string) => Promise<void>;
    onInviteCollaborator?: (bountyId: string, agentId: string | null) => Promise<void>;
    onSelectBounty?: (bountyId: string) => Promise<void> | void;
    onPublishBatch?: (bountyIds: string[]) => Promise<void>;
    onCreateComment?: (
      bountyId: string,
      body: string,
      replyToCommentId?: number | null,
    ) => Promise<void>;
    onToggleCommentLike?: (bountyId: string, commentId: number) => Promise<void>;
    onUpdateComment?: (bountyId: string, commentId: number, body: string) => Promise<void>;
    onDeleteComment?: (bountyId: string, commentId: number) => Promise<void>;
    onApproveArbitration?: (bountyId: string, input: ApproveArbitrationInput) => Promise<void>;
    onRejectArbitration?: (bountyId: string, input: RejectArbitrationInput) => Promise<void>;
  }

  interface CommentThread {
    root: BountyComment;
    replies: BountyComment[];
  }

  interface ReplyTarget {
    rootId: number;
    commentId: number;
    authorName: string;
  }

  let {
    detail,
    project,
    projects = [],
    agents = [],
    allBounties = [],
    currentUser = null,
    desktop = false,
    mobile = false,
    onClose,
    onEditDraft,
    onCreateProject,
    onUpdateDraft,
    onPublish,
    onWithdraw,
    onCancel,
    onRestoreDraft,
    onDelete,
    onReleaseLease,
    onInviteCollaborator,
    onSelectBounty,
    onPublishBatch,
    onCreateComment,
    onToggleCommentLike,
    onUpdateComment,
    onDeleteComment,
    onApproveArbitration,
    onRejectArbitration,
  }: Props = $props();

  const FOCUSABLE_SELECTOR =
    'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';
  const REPLY_COLLAPSE_THRESHOLD = 3;
  const REPLY_PREVIEW_COUNT = 2;

  let busy = $state(false);
  let actionError = $state('');
  let nowMs = $state(Date.now());
  let commentDraft = $state('');
  let commentSubmitting = $state(false);
  let commentError = $state('');
  let composerFocused = $state(false);

  let replyTarget = $state<ReplyTarget | null>(null);
  let replyDraft = $state('');
  let replySubmitting = $state(false);
  let replyError = $state('');
  let replyInputEl = $state<HTMLTextAreaElement | undefined>(undefined);

  let editingCommentId = $state<number | null>(null);
  let editingDraft = $state('');
  let editingSubmitting = $state(false);
  let editingError = $state('');
  let likingCommentIds = $state<Record<number, boolean>>({});
  let expandedThreads = $state<Record<number, boolean>>({});

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

  let modalEl = $state<HTMLDivElement | null>(null);
  let editDraftButton = $state<HTMLButtonElement | undefined>(undefined);
  let editorForm = $state<ReturnType<typeof BountyEditorForm> | undefined>(undefined);
  let isEditingDraft = $state(false);
  let moreMenuButton = $state<HTMLButtonElement | undefined>(undefined);
  let moreMenuEl = $state<HTMLDivElement | undefined>(undefined);
  let moreMenuOpen = $state(false);
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

  const composerExpanded = $derived(
    composerFocused || commentDraft.trim().length > 0 || commentSubmitting || Boolean(commentError),
  );

  // 过滤未改变状态的普通评论记录，避免在状态流转时间轴中产生“草稿 → 草稿”噪音。
  const visibleEvents = $derived(
    events.filter(item => !(item.event_type === 'commented' && item.from_status === item.to_status)),
  );

  const commentThreads = $derived.by<CommentThread[]>(() => {
    const byId = new Map<number, BountyComment>();
    for (const item of comments) {
      byId.set(item.id, item);
    }
    const threads: CommentThread[] = [];
    const repliesMap = new Map<number, BountyComment[]>();
    for (const item of comments) {
      if (item.parent_id && byId.has(item.parent_id)) {
        const list = repliesMap.get(item.parent_id) ?? [];
        list.push(item);
        repliesMap.set(item.parent_id, list);
      } else {
        threads.push({ root: item, replies: [] });
      }
    }
    for (const thread of threads) {
      thread.replies = repliesMap.get(thread.root.id) ?? [];
    }
    return threads;
  });

  const formattedAcceptanceCriteria = $derived(
    bounty.acceptance_criteria.trim()
      ? formatAcceptanceCriteriaMarkdown(bounty.acceptance_criteria, bounty.status === 'COMPLETED')
      : '',
  );

  function getFocusableElements(container: HTMLElement): HTMLElement[] {
    return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
      el => !el.hasAttribute('disabled') && !el.closest('[hidden], [inert]'),
    );
  }

  function isTopmostDialog(): boolean {
    if (isDestroyed || !modalEl || typeof document === 'undefined') return false;
    // 永久删除确认框在浏览器顶层，打开时由它接管焦点和 Escape。
    if (document.querySelector('dialog[data-dialog="delete-bounty"][open]')) return false;
    const modals = Array.from(
      document.querySelectorAll<HTMLElement>('[role="dialog"][aria-modal="true"]'),
    );
    return modals.length > 0 && modals[modals.length - 1] === modalEl;
  }

  function focusIntoModal(): void {
    if (!modalEl) return;
    const target = getFocusableElements(modalEl)[0] ?? modalEl;
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
      { align: 'start' },
    );
  }

  onMount(() => {
    if (isTopmostDialog()) {
      const currentFocus = document.activeElement;
      if (!(currentFocus instanceof Node) || !modalEl?.contains(currentFocus)) {
        focusIntoModal();
      }
    }
    const docBody = modalEl?.ownerDocument?.body ?? null;
    const modalObserver =
      typeof MutationObserver !== 'undefined' && docBody
        ? new MutationObserver(() => {
            if (isDestroyed || !modalEl || !isTopmostDialog()) return;
            const currentFocus = document.activeElement;
            if (!(currentFocus instanceof Node) || !modalEl.contains(currentFocus)) {
              focusIntoModal();
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
      composerFocused = false;
      replyTarget = null;
      replyDraft = '';
      replyError = '';
      editingCommentId = null;
      editingDraft = '';
      editingError = '';
      expandedThreads = {};
      moreMenuOpen = false;
      isEditingDraft = false;
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
    if (!modalEl || !isTopmostDialog()) return;
    const currentFocus = document.activeElement;
    if (!(currentFocus instanceof Node) || !modalEl.contains(currentFocus)) {
      focusIntoModal();
    }
  });

  $effect(() => {
    if (bounty.status !== 'DRAFT') {
      closeCollaboratorHelp();
      isEditingDraft = false;
    }
    if (bounty.status === 'CANCELLED') {
      replyTarget = null;
      editingCommentId = null;
    }
  });

  $effect(() => {
    if (!collaboratorHelpOpen || !collaboratorHelpButton || !collaboratorHelpBubble) return;
    const stopObserve = observeAnchoredPopover(
      collaboratorHelpButton,
      collaboratorHelpBubble,
      closeCollaboratorHelp,
      { align: 'start' },
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
    if (!moreMenuOpen) return;
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (
        moreMenuOpen &&
        target &&
        !moreMenuEl?.contains(target) &&
        !moreMenuButton?.contains(target)
      ) {
        moreMenuOpen = false;
      }
    };
    document.addEventListener('pointerdown', handlePointerDown, true);
    return () => document.removeEventListener('pointerdown', handlePointerDown, true);
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

  const creatorName = $derived(
    detail.creator?.name ?? bounty.created_by_agent_name ?? bounty.created_by_agent_id,
  );

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

  const hasMoreMenuActions = $derived(
    bounty.status === 'DRAFT' ||
      bounty.status === 'OPEN' ||
      bounty.status === 'WAITING_INFO' ||
      bounty.status === 'CANCELLED' ||
      canReleaseLease,
  );

  function enterDraftEdit(): void {
    if (bounty.status !== 'DRAFT') return;
    if (onUpdateDraft && onCreateProject) {
      moreMenuOpen = false;
      isEditingDraft = true;
      return;
    }
    onEditDraft?.(bounty);
  }

  async function exitDraftEdit(): Promise<void> {
    isEditingDraft = false;
    await tick();
    editDraftButton?.focus();
  }

  function displayCommentAuthor(comment: BountyComment): string {
    if (comment.author_type === 'human' && comment.author_name === 'human') {
      return currentUser?.name || $t('管理员');
    }
    return comment.author_name;
  }

  function displayReplyTargetAuthor(comment: BountyComment): string {
    const raw = comment.reply_to_author_name;
    if (!raw || raw === 'human') {
      return currentUser?.name || $t('管理员');
    }
    return raw;
  }

  function displayTimelineActor(item: BountyEvent): string {
    if (item.actor === 'human' && currentUser?.name) {
      return currentUser.name;
    }
    return formatEventActor(item.actor, bounty);
  }

  function isCurrentHumanComment(comment: BountyComment): boolean {
    if (comment.author_type !== 'human') return false;
    if (!currentUser) return comment.author_name === 'human';
    return comment.author_name === 'human' || comment.author_id === currentUser.id;
  }

  async function runAction(fn: () => Promise<void>): Promise<void> {
    if (busy) return;
    actionError = '';
    moreMenuOpen = false;
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
      await onCreateComment(bounty.id, trimmed, null);
      commentDraft = '';
      composerFocused = false;
    } catch (error) {
      commentError = formatServiceError(error);
    } finally {
      commentSubmitting = false;
    }
  }

  async function openReplyBox(rootId: number, comment: BountyComment): Promise<void> {
    const authorName = displayCommentAuthor(comment);
    if (replyTarget?.commentId === comment.id) {
      replyTarget = null;
      replyDraft = '';
      replyError = '';
      return;
    }
    editingCommentId = null;
    editingError = '';
    replyTarget = { rootId, commentId: comment.id, authorName };
    replyDraft = '';
    replyError = '';
    await tick();
    replyInputEl?.focus();
  }

  async function submitReply(): Promise<void> {
    if (!onCreateComment || !replyTarget || replySubmitting) return;
    const trimmed = replyDraft.trim();
    if (!trimmed) {
      replyError = $t('请输入评论内容。');
      return;
    }
    replyError = '';
    replySubmitting = true;
    const rootId = replyTarget.rootId;
    const targetId = replyTarget.commentId;
    try {
      await onCreateComment(bounty.id, trimmed, targetId);
      replyDraft = '';
      replyTarget = null;
      expandedThreads = { ...expandedThreads, [rootId]: true };
    } catch (error) {
      replyError = formatServiceError(error);
    } finally {
      replySubmitting = false;
    }
  }

  async function handleToggleLike(commentId: number): Promise<void> {
    if (!onToggleCommentLike || likingCommentIds[commentId]) return;
    likingCommentIds = { ...likingCommentIds, [commentId]: true };
    try {
      await onToggleCommentLike(bounty.id, commentId);
    } catch (error) {
      commentError = formatServiceError(error);
    } finally {
      const next = { ...likingCommentIds };
      delete next[commentId];
      likingCommentIds = next;
    }
  }

  function startEditingComment(comment: BountyComment): void {
    replyTarget = null;
    replyError = '';
    editingCommentId = comment.id;
    editingDraft = comment.body;
    editingError = '';
  }

  async function submitEditComment(commentId: number): Promise<void> {
    if (!onUpdateComment || editingSubmitting) return;
    const trimmed = editingDraft.trim();
    if (!trimmed) {
      editingError = $t('请输入评论内容。');
      return;
    }
    editingError = '';
    editingSubmitting = true;
    try {
      await onUpdateComment(bounty.id, commentId, trimmed);
      editingCommentId = null;
      editingDraft = '';
    } catch (error) {
      editingError = formatServiceError(error);
    } finally {
      editingSubmitting = false;
    }
  }

  async function handleDeleteComment(commentId: number): Promise<void> {
    if (!onDeleteComment || busy) return;
    commentError = '';
    try {
      await onDeleteComment(bounty.id, commentId);
      if (editingCommentId === commentId) {
        editingCommentId = null;
      }
      if (replyTarget?.commentId === commentId || replyTarget?.rootId === commentId) {
        replyTarget = null;
      }
    } catch (error) {
      commentError = formatServiceError(error);
    }
  }

  function handleWindowFocusIn(event: FocusEvent): void {
    if (!isTopmostDialog() || !modalEl) return;
    const target = event.target;
    if (target instanceof Node && modalEl.contains(target)) return;
    focusIntoModal();
  }

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (!isTopmostDialog() || !modalEl) return;
    if (event.key === 'Tab') {
      const focusables = getFocusableElements(modalEl);
      if (focusables.length === 0) {
        event.preventDefault();
        modalEl.focus();
        return;
      }
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      const active = document.activeElement;
      if (!modalEl.contains(active)) {
        event.preventDefault();
        (event.shiftKey ? last : first).focus();
        return;
      }
      if (event.shiftKey && (active === first || active === modalEl)) {
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
      if (isEditingDraft) {
        if (editorForm?.handleEscapeKey()) {
          return;
        }
        void exitDraftEdit();
        return;
      }
      if (collaboratorHelpOpen) {
        closeCollaboratorHelp();
        return;
      }
      if (moreMenuOpen) {
        moreMenuOpen = false;
        moreMenuButton?.focus();
        return;
      }
      if (editingCommentId !== null) {
        editingCommentId = null;
        editingError = '';
        return;
      }
      if (replyTarget !== null) {
        replyTarget = null;
        replyError = '';
        return;
      }
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} onfocusin={handleWindowFocusIn} />

<div
  class="detail-modal-backdrop"
  role="presentation"
  onclick={event => {
    if (event.target === event.currentTarget) onClose();
  }}
>
  <div
    bind:this={modalEl}
    tabindex="-1"
    class="detail-modal"
    role="dialog"
    aria-modal="true"
    aria-label={bounty.title}
    data-dialog="bounty-detail"
    data-bounty-status={bounty.status}
  >
    <header class="detail-modal-topbar">
      <div class="detail-modal-breadcrumb">
        <span class="detail-modal-project">
          <ProjectIcon icon={project?.icon ?? undefined} />
          <span class="detail-modal-project-name">{project?.name ?? $t('未命名项目')}</span>
        </span>
        <span class="detail-modal-crumb-sep" aria-hidden="true">/</span>
        <span class="detail-modal-bounty-id" title={bounty.id}>
          {isEditingDraft ? $t('编辑草稿') : `#${bounty.id.slice(0, 8)}`}
        </span>
      </div>

      <div class="detail-modal-topbar-actions" data-slot="detail-actions">
        {#if !isEditingDraft}
          {#if bounty.status === 'DRAFT'}
            <button
              bind:this={editDraftButton}
              type="button"
              class="detail-topbar-btn"
              data-action="edit-draft"
              disabled={busy}
              onclick={enterDraftEdit}
            >
              {$t('编辑草稿')}
            </button>
            <button
              type="button"
              class="primary detail-topbar-btn"
              data-action="publish-bounty"
              disabled={busy}
              onclick={() => void runAction(() => onPublish(bounty.id))}
            >
              {$t('立即发布')}
            </button>
          {:else if bounty.status === 'OPEN'}
            <button
              type="button"
              class="detail-topbar-btn"
              data-action="withdraw-bounty"
              disabled={busy}
              onclick={() => void runAction(() => onWithdraw(bounty.id))}
            >
              {$t('撤回为草稿')}
            </button>
          {:else if bounty.status === 'CANCELLED'}
            <button
              type="button"
              class="detail-topbar-btn"
              data-action="restore-draft"
              disabled={busy}
              onclick={() => void runAction(() => onRestoreDraft(bounty.id))}
            >
              {$t('转为草稿')}
            </button>
          {/if}

          {#if hasMoreMenuActions}
            <div class="detail-more-wrap">
              <button
                bind:this={moreMenuButton}
                type="button"
                class="icon-button detail-more-btn"
                data-action="toggle-detail-more"
                aria-label={$t('更多操作')}
                aria-haspopup="menu"
                aria-expanded={moreMenuOpen}
                aria-controls="detail-more-menu"
                disabled={busy}
                onclick={() => {
                  moreMenuOpen = !moreMenuOpen;
                }}
              >
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                <circle cx="3.2" cy="8" r="1.25" />
                <circle cx="8" cy="8" r="1.25" />
                <circle cx="12.8" cy="8" r="1.25" />
              </svg>
            </button>
            <div
              bind:this={moreMenuEl}
              id="detail-more-menu"
              class="detail-more-menu"
              role="menu"
              hidden={!moreMenuOpen}
            >
              {#if bounty.status === 'DRAFT'}
                <button
                  type="button"
                  role="menuitem"
                  class="detail-menu-item danger-text"
                  data-action="cancel-bounty"
                  disabled={busy}
                  onclick={() => void runAction(() => onCancel(bounty.id))}
                >
                  {$t('丢弃草稿')}
                </button>
              {:else if bounty.status === 'OPEN'}
                <button
                  type="button"
                  role="menuitem"
                  class="detail-menu-item danger-text"
                  data-action="cancel-bounty"
                  disabled={busy}
                  onclick={() => void runAction(() => onCancel(bounty.id))}
                >
                  {$t('取消任务')}
                </button>
              {:else if bounty.status === 'CANCELLED'}
                <button
                  type="button"
                  role="menuitem"
                  class="detail-menu-item danger-text"
                  data-action="delete-bounty"
                  disabled={busy}
                  onclick={() => {
                    moreMenuOpen = false;
                    void onDelete(bounty);
                  }}
                >
                  {$t('删除')}
                </button>
              {:else}
                {#if canReleaseLease}
                  <button
                    type="button"
                    role="menuitem"
                    class="detail-menu-item danger-text"
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
                    role="menuitem"
                    class="detail-menu-item danger-text"
                    data-action="cancel-bounty"
                    disabled={busy}
                    onclick={() => void runAction(() => onCancel(bounty.id))}
                  >
                    {$t('取消任务')}
                  </button>
                {/if}
              {/if}
            </div>
          </div>
        {/if}
        {/if}

        <button
          type="button"
          class="icon-button detail-close-btn"
          aria-label={$t('关闭对话框')}
          onclick={onClose}
        >
          ✕
        </button>
      </div>
    </header>

    {#if isEditingDraft && onCreateProject && onUpdateDraft}
      <BountyEditorForm
        bind:this={editorForm}
        {projects}
        availableDependencies={allBounties}
        editingBounty={bounty}
        {desktop}
        {mobile}
        onCancel={() => void exitDraftEdit()}
        onSaved={() => void exitDraftEdit()}
        {onCreateProject}
        {onUpdateDraft}
      />
    {:else}
    <div class="detail-modal-columns">
      <div class="detail-modal-main">
        {#if actionError}
          <div class="drawer-error-banner" role="alert" data-status="detail-action-error">
            <span>{actionError}</span>
          </div>
        {/if}

        <h2 class="detail-drawer-title">{bounty.title}</h2>

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
                  {formatCompactTimestamp(pendingQuestion.created_at)}
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
                    class="detail-arbitration-textarea"
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

        <section class="detail-section detail-description-section">
          {#if bounty.description.trim()}
            <div
              class="bounty-detail-markdown is-flat"
              data-slot="detail-description"
              use:renderMarkdownAction={bounty.description}
            ></div>
          {:else}
            <p class="detail-empty-hint" data-slot="detail-description">{$t('暂无详情描述')}</p>
          {/if}
        </section>

        <section class="detail-section detail-criteria-section">
          <h3 class="detail-section-title detail-criteria-heading">{$t('验收条件')}</h3>
          {#if formattedAcceptanceCriteria}
            <div
              class="bounty-detail-markdown is-flat"
              data-slot="detail-criteria"
              use:renderMarkdownAction={formattedAcceptanceCriteria}
            ></div>
          {:else}
            <p class="detail-empty-hint" data-slot="detail-criteria">{$t('暂无验收条件')}</p>
          {/if}
        </section>

        {#if showCoCreationSection}
          <section class="detail-section detail-flat-subsection" data-slot="detail-draft-cocreation">
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
                      class="secondary-action"
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
          <section class="detail-section detail-flat-subsection" data-slot="detail-dependencies">
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
                      class="secondary-action"
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

        <section class="detail-section detail-comments-section" data-slot="detail-comments">
          <div class="detail-comments-title-row">
            <h3 class="detail-comments-heading">
              <span>{$t('评论')}</span>
              <span class="detail-comments-count" data-slot="detail-comments-count">{comments.length}</span>
            </h3>
          </div>

          {#if onCreateComment}
            {#if bounty.status === 'CANCELLED'}
              <p class="detail-comment-disabled" data-slot="comment-disabled-notice">
                {$t('任务已取消，无法继续发表评论')}
              </p>
            {:else}
              <div class="detail-comment-composer-row">
                <UserAvatar
                  name={currentUser?.name ?? ''}
                  avatar={currentUser?.avatar ?? null}
                  size="sm"
                  guest={!currentUser}
                />
                <form
                  class="detail-comment-composer"
                  class:is-expanded={composerExpanded}
                  data-slot="comment-composer"
                  onsubmit={event => {
                    event.preventDefault();
                    void submitComment();
                  }}
                >
                  <textarea
                    class="detail-comment-input"
                    data-slot="comment-input"
                    rows={composerExpanded ? 3 : 1}
                    aria-label={$t('评论内容')}
                    placeholder={bounty.status === 'WAITING_INFO'
                      ? $t('补充回复以恢复任务执行…')
                      : $t('写下你的评论…')}
                    bind:value={commentDraft}
                    disabled={commentSubmitting}
                    onfocus={() => {
                      composerFocused = true;
                    }}
                    onblur={() => {
                      composerFocused = false;
                    }}
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
                  <div class="detail-comment-actions" hidden={!composerExpanded}>
                    <span class="detail-comment-format-hint" title={$t('支持 Markdown 与 KaTeX 公式')}>
                      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                        <rect x="1.5" y="3" width="13" height="10" rx="2" />
                        <path d="M4 10V6l2 2.4L8 6v4" />
                        <path d="M10.5 6.5v3m0 0-1.2-1.2m1.2 1.2 1.2-1.2" />
                      </svg>
                      <span>Markdown / KaTeX</span>
                    </span>
                    <button
                      type="submit"
                      class="primary detail-comment-submit"
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
                          : $t('发送')}
                    </button>
                  </div>
                </form>
              </div>
            {/if}
          {/if}

          {#if commentThreads.length === 0}
            <p class="detail-empty-hint">{$t('暂无评论或澄清记录')}</p>
          {:else}
            <ol class="detail-comments-list" data-slot="detail-comments-list">
              {#each commentThreads as thread (thread.root.id)}
                {@const comment = thread.root}
                {@const replies = thread.replies}
                {@const hasMoreReplies = replies.length > REPLY_COLLAPSE_THRESHOLD}
                {@const isExpanded = Boolean(expandedThreads[comment.id])}
                {@const visibleReplies =
                  hasMoreReplies && !isExpanded ? replies.slice(0, REPLY_PREVIEW_COUNT) : replies}
                <li
                  class="detail-comment-item"
                  class:is-question={comment.kind === 'question'}
                  class:is-reply={comment.kind === 'reply'}
                  data-comment-id={comment.id}
                  data-comment-kind={comment.kind}
                  data-author-type={comment.author_type}
                >
                  <div class="detail-comment-avatar">
                    {#if comment.author_type === 'agent'}
                      <span class="detail-agent-avatar" aria-hidden="true">
                        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                          <rect x="2.5" y="4" width="11" height="8.5" rx="2.5" />
                          <path d="M8 2v2" />
                          <circle cx="6" cy="8.2" r="0.8" fill="currentColor" stroke="none" />
                          <circle cx="10" cy="8.2" r="0.8" fill="currentColor" stroke="none" />
                        </svg>
                      </span>
                    {:else}
                      <UserAvatar
                        name={displayCommentAuthor(comment)}
                        avatar={isCurrentHumanComment(comment) ? (currentUser?.avatar ?? null) : null}
                        size="sm"
                      />
                    {/if}
                  </div>

                  <div class="detail-comment-content">
                    <div class="detail-comment-header">
                      <span class="detail-comment-author-name">{displayCommentAuthor(comment)}</span>
                      {#if comment.author_type === 'agent'}
                        <span class="detail-comment-role" data-role="agent">Agent</span>
                      {:else if comment.author_type === 'human'}
                        <span class="detail-comment-role" data-role="human">{$t('管理员')}</span>
                      {/if}
                      {#if comment.kind === 'question'}
                        <span class="detail-comment-kind" data-kind="question">{$t('提问阻塞')}</span>
                      {:else if comment.kind === 'reply'}
                        <span class="detail-comment-kind" data-kind="reply">{$t('恢复回复')}</span>
                      {/if}
                    </div>

                    {#if editingCommentId === comment.id}
                      <form
                        class="detail-comment-inline-edit"
                        onsubmit={event => {
                          event.preventDefault();
                          void submitEditComment(comment.id);
                        }}
                      >
                        <textarea
                          class="detail-comment-input is-inline"
                          data-slot="edit-comment-input"
                          rows="2"
                          bind:value={editingDraft}
                          disabled={editingSubmitting}
                        ></textarea>
                        {#if editingError}
                          <p class="detail-comment-error" role="alert">{editingError}</p>
                        {/if}
                        <div class="detail-inline-edit-actions">
                          <button
                            type="button"
                            class="detail-text-action"
                            disabled={editingSubmitting}
                            onclick={() => {
                              editingCommentId = null;
                              editingError = '';
                            }}
                          >
                            {$t('取消')}
                          </button>
                          <button
                            type="submit"
                            class="primary detail-inline-submit"
                            data-action="save-comment-edit"
                            disabled={editingSubmitting || !editingDraft.trim()}
                          >
                            {$t('保存')}
                          </button>
                        </div>
                      </form>
                    {:else}
                      <div
                        class="bounty-detail-markdown is-flat detail-comment-body"
                        data-slot="comment-body"
                        use:renderMarkdownAction={comment.body}
                      ></div>
                    {/if}

                    <div class="detail-comment-footer">
                      <div class="detail-comment-meta-actions">
                        <time class="detail-comment-time" datetime={comment.created_at}>
                          {formatCompactTimestamp(comment.created_at)}
                        </time>
                        {#if comment.updated_at}
                          <span class="detail-comment-edited">{$t('已编辑')}</span>
                        {/if}
                        {#if onToggleCommentLike}
                          <button
                            type="button"
                            class="detail-comment-action-btn"
                            class:is-liked={Boolean(comment.liked)}
                            data-action="like-comment"
                            aria-pressed={Boolean(comment.liked)}
                            disabled={Boolean(likingCommentIds[comment.id])}
                            onclick={() => void handleToggleLike(comment.id)}
                          >
                            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                              <path d="M5.5 7.5V13.5H3a1 1 0 0 1-1-1V8.5a1 1 0 0 1 1-1h2.5Zm0 0 2-4.5a1.2 1.2 0 0 1 2.2.7V6.5h2.6a1.2 1.2 0 0 1 1.18 1.44l-.75 4a1.5 1.5 0 0 1-1.48 1.22H5.5" />
                            </svg>
                            <span>{(comment.like_count ?? 0) > 0 ? comment.like_count : $t('赞')}</span>
                          </button>
                        {/if}
                        {#if onCreateComment && bounty.status !== 'CANCELLED'}
                          <button
                            type="button"
                            class="detail-comment-action-btn"
                            data-action="reply-comment"
                            onclick={() => void openReplyBox(comment.id, comment)}
                          >
                            {$t('回复')}
                          </button>
                        {/if}
                      </div>

                      {#if bounty.status !== 'CANCELLED' && (onUpdateComment || onDeleteComment)}
                        <div class="detail-comment-hover-actions">
                          {#if onUpdateComment && comment.author_type === 'human'}
                            <button
                              type="button"
                              class="detail-comment-action-btn"
                              data-action="edit-comment"
                              onclick={() => startEditingComment(comment)}
                            >
                              {$t('编辑')}
                            </button>
                          {/if}
                          {#if onDeleteComment}
                            <button
                              type="button"
                              class="detail-comment-action-btn is-danger"
                              data-action="delete-comment"
                              onclick={() => void handleDeleteComment(comment.id)}
                            >
                              {$t('删除')}
                            </button>
                          {/if}
                        </div>
                      {/if}
                    </div>

                    {#if replies.length > 0}
                      <ol class="detail-comment-replies" data-slot="comment-replies">
                        {#each visibleReplies as reply (reply.id)}
                          <li
                            class="detail-subreply-item"
                            data-comment-id={reply.id}
                            data-comment-kind={reply.kind}
                            data-author-type={reply.author_type}
                          >
                            <div class="detail-subreply-avatar">
                              {#if reply.author_type === 'agent'}
                                <span class="detail-agent-avatar is-xs" aria-hidden="true">
                                  <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                                    <rect x="2.5" y="4" width="11" height="8.5" rx="2.5" />
                                    <path d="M8 2v2" />
                                    <circle cx="6" cy="8.2" r="0.8" fill="currentColor" stroke="none" />
                                    <circle cx="10" cy="8.2" r="0.8" fill="currentColor" stroke="none" />
                                  </svg>
                                </span>
                              {:else}
                                <UserAvatar
                                  name={displayCommentAuthor(reply)}
                                  avatar={isCurrentHumanComment(reply) ? (currentUser?.avatar ?? null) : null}
                                  size="sm"
                                />
                              {/if}
                            </div>

                            <div class="detail-subreply-content">
                              <div class="detail-subreply-head">
                                <span class="detail-comment-author-name">{displayCommentAuthor(reply)}</span>
                                {#if reply.author_type === 'agent'}
                                  <span class="detail-comment-role" data-role="agent">Agent</span>
                                {/if}
                                {#if reply.reply_to_author_name}
                                  <span class="detail-reply-target">
                                    {$t('回复')}
                                    <span class="detail-reply-mention">@{displayReplyTargetAuthor(reply)}</span>
                                  </span>
                                {/if}
                                {#if reply.kind === 'reply'}
                                  <span class="detail-comment-kind" data-kind="reply">{$t('恢复回复')}</span>
                                {/if}
                              </div>

                              {#if editingCommentId === reply.id}
                                <form
                                  class="detail-comment-inline-edit"
                                  onsubmit={event => {
                                    event.preventDefault();
                                    void submitEditComment(reply.id);
                                  }}
                                >
                                  <textarea
                                    class="detail-comment-input is-inline"
                                    data-slot="edit-comment-input"
                                    rows="2"
                                    bind:value={editingDraft}
                                    disabled={editingSubmitting}
                                  ></textarea>
                                  {#if editingError}
                                    <p class="detail-comment-error" role="alert">{editingError}</p>
                                  {/if}
                                  <div class="detail-inline-edit-actions">
                                    <button
                                      type="button"
                                      class="detail-text-action"
                                      disabled={editingSubmitting}
                                      onclick={() => {
                                        editingCommentId = null;
                                        editingError = '';
                                      }}
                                    >
                                      {$t('取消')}
                                    </button>
                                    <button
                                      type="submit"
                                      class="primary detail-inline-submit"
                                      data-action="save-comment-edit"
                                      disabled={editingSubmitting || !editingDraft.trim()}
                                    >
                                      {$t('保存')}
                                    </button>
                                  </div>
                                </form>
                              {:else}
                                <div
                                  class="bounty-detail-markdown is-flat detail-comment-body detail-subreply-body"
                                  data-slot="comment-body"
                                  use:renderMarkdownAction={reply.body}
                                ></div>
                              {/if}

                              <div class="detail-comment-footer">
                                <div class="detail-comment-meta-actions">
                                  <time class="detail-comment-time" datetime={reply.created_at}>
                                    {formatCompactTimestamp(reply.created_at)}
                                  </time>
                                  {#if reply.updated_at}
                                    <span class="detail-comment-edited">{$t('已编辑')}</span>
                                  {/if}
                                  {#if onToggleCommentLike}
                                    <button
                                      type="button"
                                      class="detail-comment-action-btn"
                                      class:is-liked={Boolean(reply.liked)}
                                      data-action="like-comment"
                                      aria-pressed={Boolean(reply.liked)}
                                      disabled={Boolean(likingCommentIds[reply.id])}
                                      onclick={() => void handleToggleLike(reply.id)}
                                    >
                                      <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                        <path d="M5.5 7.5V13.5H3a1 1 0 0 1-1-1V8.5a1 1 0 0 1 1-1h2.5Zm0 0 2-4.5a1.2 1.2 0 0 1 2.2.7V6.5h2.6a1.2 1.2 0 0 1 1.18 1.44l-.75 4a1.5 1.5 0 0 1-1.48 1.22H5.5" />
                                      </svg>
                                      <span>{(reply.like_count ?? 0) > 0 ? reply.like_count : $t('赞')}</span>
                                    </button>
                                  {/if}
                                  {#if onCreateComment && bounty.status !== 'CANCELLED'}
                                    <button
                                      type="button"
                                      class="detail-comment-action-btn"
                                      data-action="reply-comment"
                                      onclick={() => void openReplyBox(comment.id, reply)}
                                    >
                                      {$t('回复')}
                                    </button>
                                  {/if}
                                </div>

                                {#if bounty.status !== 'CANCELLED' && (onUpdateComment || onDeleteComment)}
                                  <div class="detail-comment-hover-actions">
                                    {#if onUpdateComment && reply.author_type === 'human'}
                                      <button
                                        type="button"
                                        class="detail-comment-action-btn"
                                        data-action="edit-comment"
                                        onclick={() => startEditingComment(reply)}
                                      >
                                        {$t('编辑')}
                                      </button>
                                    {/if}
                                    {#if onDeleteComment}
                                      <button
                                        type="button"
                                        class="detail-comment-action-btn is-danger"
                                        data-action="delete-comment"
                                        onclick={() => void handleDeleteComment(reply.id)}
                                      >
                                        {$t('删除')}
                                      </button>
                                    {/if}
                                  </div>
                                {/if}
                              </div>
                            </div>
                          </li>
                        {/each}

                        {#if hasMoreReplies}
                          <li class="detail-replies-toggle-row">
                            <button
                              type="button"
                              class="detail-replies-toggle"
                              data-action={isExpanded ? 'collapse-replies' : 'expand-replies'}
                              onclick={() => {
                                expandedThreads = {
                                  ...expandedThreads,
                                  [comment.id]: !isExpanded,
                                };
                              }}
                            >
                              {isExpanded
                                ? $t('收起回复')
                                : $t('共 {count} 条回复，点击展开', { count: replies.length })}
                            </button>
                          </li>
                        {/if}
                      </ol>
                    {/if}

                    {#if replyTarget?.rootId === comment.id && onCreateComment && bounty.status !== 'CANCELLED'}
                      <div class="detail-inline-reply-box" data-slot="inline-reply-composer">
                        <div class="detail-subreply-avatar">
                          <UserAvatar
                            name={currentUser?.name ?? ''}
                            avatar={currentUser?.avatar ?? null}
                            size="sm"
                            guest={!currentUser}
                          />
                        </div>
                        <form
                          class="detail-inline-reply-form"
                          onsubmit={event => {
                            event.preventDefault();
                            void submitReply();
                          }}
                        >
                          <textarea
                            bind:this={replyInputEl}
                            class="detail-comment-input is-inline"
                            data-slot="reply-input"
                            rows="2"
                            aria-label={$t('回复 @{name}', { name: replyTarget.authorName })}
                            placeholder={$t('回复 @{name}…', { name: replyTarget.authorName })}
                            bind:value={replyDraft}
                            disabled={replySubmitting}
                            onkeydown={event => {
                              if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
                                event.preventDefault();
                                void submitReply();
                              }
                            }}
                          ></textarea>
                          {#if replyError}
                            <p class="detail-comment-error" role="alert">{replyError}</p>
                          {/if}
                          <div class="detail-inline-edit-actions">
                            <button
                              type="button"
                              class="detail-text-action"
                              disabled={replySubmitting}
                              onclick={() => {
                                replyTarget = null;
                                replyDraft = '';
                                replyError = '';
                              }}
                            >
                              {$t('取消')}
                            </button>
                            <button
                              type="submit"
                              class="primary detail-inline-submit"
                              data-action="submit-reply"
                              disabled={replySubmitting || !replyDraft.trim()}
                            >
                              {replySubmitting ? $t('正在发送…') : $t('发送')}
                            </button>
                          </div>
                        </form>
                      </div>
                    {/if}
                  </div>
                </li>
              {/each}
            </ol>
          {/if}
        </section>
      </div>

      <aside class="detail-modal-sidebar">
        <div class="detail-attr-grid" data-slot="detail-attributes">
          <div class="detail-attr-row">
            <span class="detail-attr-label">{$t('状态')}</span>
            <div class="detail-attr-value" data-slot="detail-status-control">
              <span class="status-pill" data-status={bounty.status}>{statusLabel}</span>
            </div>
          </div>

          <div class="detail-attr-row">
            <span class="detail-attr-label">{$t('等级')}</span>
            <div class="detail-attr-value">
              <span class="difficulty-badge" data-difficulty={bounty.difficulty}>
                {bounty.difficulty}
              </span>
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
          </div>

          <div class="detail-attr-row">
            <span class="detail-attr-label">{$t('项目')}</span>
            <div class="detail-attr-value detail-attr-project">
              <ProjectIcon icon={project?.icon ?? undefined} />
              <span>{project?.name ?? $t('未命名项目')}</span>
            </div>
          </div>

          <div class="detail-attr-row">
            <span class="detail-attr-label">{$t('创建者')}</span>
            <div class="detail-attr-value detail-attr-creator" data-slot="detail-creator">
              <UserAvatar name={creatorName ?? ''} avatar={detail.creator?.avatar} />
              <span class="detail-creator-name">{creatorName ?? $t('未知创建者')}</span>
            </div>
          </div>

          {#if bounty.assignee_mode !== 'any'}
            <div class="detail-attr-row">
              <span class="detail-attr-label">{$t('指派')}</span>
              <div class="detail-attr-value">
                <span class="detail-attr-text">{assigneeLabel}</span>
              </div>
            </div>
          {/if}

          {#if executor}
            <div class="detail-attr-row">
              <span class="detail-attr-label">{$t('执行')}</span>
              <div class="detail-attr-value">
                <span
                  class="detail-meta-pill"
                  data-slot="detail-executor"
                  data-executor-tier={executor.tier ?? 'unknown'}
                >
                  {$t('执行者: {agent}', { agent: executor.label })}
                </span>
              </div>
            </div>
          {/if}

          {#if arbitrator || bounty.status === 'PENDING_ARBITRATION'}
            <div class="detail-attr-row">
              <span class="detail-attr-label">{$t('裁判')}</span>
              <div class="detail-attr-value">
                {#if arbitrator}
                  <span
                    class="detail-meta-pill bounty-arbitrator-pill"
                    data-slot="detail-arbitrator"
                    data-arbitrator-tier={arbitrator.tier ?? 'unknown'}
                  >
                    {$t('裁判: {agent}', { agent: arbitrator.label })}
                  </span>
                {:else}
                  <span class="detail-meta-pill bounty-human-fallback-pill" data-slot="detail-human-fallback">
                    {$t('人工仲裁兜底')}
                  </span>
                {/if}
              </div>
            </div>
          {/if}

          {#if bounty.status === 'WAITING_INFO' || leaseCountdown || arbitrationCountdown || assignmentCountdown}
            <div class="detail-attr-row">
              <span class="detail-attr-label">{$t('租约')}</span>
              <div class="detail-attr-value">
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
              </div>
            </div>
          {/if}

          {#if showActiveDuration}
            <div class="detail-attr-row">
              <span class="detail-attr-label">{$t('耗时')}</span>
              <div class="detail-attr-value">
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
              </div>
            </div>
          {/if}

          <div class="detail-attr-row">
            <span class="detail-attr-label">{$t('更新')}</span>
            <div class="detail-attr-value">
              <span class="detail-meta-time">
                {formatCompactTimestamp(bounty.updated_at || bounty.created_at)}
              </span>
            </div>
          </div>

          {#if (bounty.status === 'DRAFT' && bounty.draft_needs_human_review) || bounty.is_blocked || bounty.parent_bounty_id || bounty.require_arbitration || (bounty.rejection_count ?? 0) > 0 || (bounty.status === 'COMPLETED' && qualityLabel)}
            <div class="detail-attr-tags">
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
            </div>
          {/if}
        </div>

        <section class="detail-events-section" data-slot="detail-events">
          <h3 class="detail-section-title">{$t('状态流转')}</h3>
          {#if visibleEvents.length === 0}
            <p class="detail-empty-hint">{$t('暂无流转记录')}</p>
          {:else}
            <ol class="detail-timeline">
              {#each visibleEvents as item (item.id)}
                {@const eventActionLabel = (() => {
                  const _lang = $locale;
                  return formatEventLabel(item.event_type);
                })()}
                {@const actorLabel = (() => {
                  const _lang = $locale;
                  return displayTimelineActor(item);
                })()}
                {@const transitionLabel = (() => {
                  const _lang = $locale;
                  return item.from_status
                    ? `${formatBountyStatus(item.from_status)} → ${formatBountyStatus(item.to_status)}`
                    : formatBountyStatus(item.to_status);
                })()}
                <li class="detail-timeline-item" data-event-type={item.event_type}>
                  <div class="detail-timeline-main">
                    <span class="detail-timeline-actor">{actorLabel}</span>
                    <span class="detail-timeline-action">{eventActionLabel}</span>
                  </div>
                  {#if item.from_status && item.from_status !== item.to_status}
                    <div class="detail-timeline-transition">{transitionLabel}</div>
                  {/if}
                  <time class="detail-timeline-time" datetime={item.created_at}>
                    {formatCompactTimestamp(item.created_at)}
                  </time>
                </li>
              {/each}
            </ol>
          {/if}
        </section>
      </aside>
    </div>
    {/if}
  </div>
</div>
