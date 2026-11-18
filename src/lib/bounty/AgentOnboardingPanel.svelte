<script lang="ts">
  /**
   * 文件职责：在设置页面集中管理外部 Agent 准入与协议（局域网免审注册开关、邀请令牌生成、作废与删除、SKILL.md 在线预览与导出）。
   * 定义范围：免审状态切换、一次性明文令牌横幅展示、邀请令牌状态徽章及动态 SKILL.md 预览复制下载。
   */
  import { tick } from 'svelte';
  import type {
    CreatedInviteToken,
    CreateInviteTokenInput,
    InviteTokenSummary,
    OnboardingSettings,
    SkillDocument,
  } from '../api/types';
  import { observeAnchoredPopover, positionAnchoredPopover } from '../anchored-popover';
  import { clipboard } from '../clipboard';
  import { locale, t } from '../i18n';
  import {
    formatInviteTokenStatus,
    formatServiceError,
    formatTimestamp,
  } from './helpers';

  interface Props {
    onboarding: OnboardingSettings;
    onboardingLoaded?: boolean;
    onboardingLoadError?: string;
    serviceUrl?: string;
    serviceEpoch?: number;
    skillDocument?: SkillDocument | null;
    onRetryLoadOnboarding?: () => Promise<OnboardingSettings | null | void> | void;
    onToggleLanRegistration: (enabled: boolean) => Promise<OnboardingSettings>;
    onCreateInviteToken: (input?: CreateInviteTokenInput) => Promise<CreatedInviteToken>;
    onRevokeInviteToken: (id: string) => Promise<InviteTokenSummary>;
    onDeleteInviteToken: (id: string) => Promise<void>;
    onLoadSkillMarkdown: (options?: {
      baseUrl?: string;
      inviteToken?: string;
    }) => Promise<SkillDocument>;
    onExportSkillMarkdown?: (doc: SkillDocument) => Promise<boolean>;
    onNotify?: (message: string) => void;
  }

  let {
    onboarding,
    onboardingLoaded = true,
    onboardingLoadError = '',
    serviceUrl = '',
    serviceEpoch = 0,
    skillDocument: _skillDocument = null,
    onRetryLoadOnboarding,
    onToggleLanRegistration,
    onCreateInviteToken,
    onRevokeInviteToken,
    onDeleteInviteToken,
    onLoadSkillMarkdown,
    onExportSkillMarkdown,
    onNotify,
  }: Props = $props();

  let lanHelpButton: HTMLButtonElement;
  let lanHelpBubble: HTMLDivElement;
  let lanHelpOpen = $state(false);

  let labelDraft = $state('');
  let maxUsesDraft = $state('');
  let expiresHoursDraft = $state('');
  let baseUrlDraft = $state('');
  let attachLatestToken = $state(false);
  let latestCreatedToken = $state<CreatedInviteToken | null>(null);
  let createdTokenDialog = $state<HTMLDialogElement | undefined>(undefined);
  let createdTokenDialogOpen = $state(false);
  let createdTokenCopied = $state(false);
  let createdDialogPressedOutside = false;
  let loadedSkill = $state<{ doc: SkillDocument; contextKey: string } | null>(null);

  let busy = $state(false);
  let creatingToken = $state(false);
  let loadingSkill = $state(false);
  let retryingOnboarding = $state(false);
  let panelError = $state('');
  let syncedConnectionKey = $state<string | null>(null);
  let revokedTokenIds = $state(new Set<string>());
  let openMenuTokenId = $state<string | null>(null);
  let confirmingDeleteTokenId = $state<string | null>(null);
  let skillLoadSeq = 0;
  let opSeq = 0;
  let retrySeq = 0;

  function closeLanHelp(): void {
    lanHelpOpen = false;
    if (
      typeof lanHelpBubble?.hidePopover === 'function' &&
      lanHelpBubble.matches(':popover-open')
    ) {
      lanHelpBubble.hidePopover();
    }
  }

  function closeTokenMenu(options: { restoreFocus?: boolean } = {}): void {
    const closingId = openMenuTokenId;
    openMenuTokenId = null;
    confirmingDeleteTokenId = null;
    if (closingId) {
      const popup = document.getElementById(`invite-token-menu-${closingId}`);
      if (typeof popup?.hidePopover === 'function' && popup.matches(':popover-open')) {
        popup.hidePopover();
      }
      if (options.restoreFocus) {
        document.getElementById(`invite-token-menu-trigger-${closingId}`)?.focus();
      }
    }
  }

  async function openTokenMenu(tokenId: string, focusFirstItem = false): Promise<void> {
    if (busy) return;
    closeLanHelp();
    if (openMenuTokenId && openMenuTokenId !== tokenId) {
      closeTokenMenu();
    }
    confirmingDeleteTokenId = null;
    openMenuTokenId = tokenId;
    await tick();
    if (openMenuTokenId !== tokenId) return;
    const trigger = document.getElementById(`invite-token-menu-trigger-${tokenId}`);
    const popup = document.getElementById(`invite-token-menu-${tokenId}`);
    if (!trigger || !popup || !popup.isConnected) return;
    if (typeof popup.showPopover === 'function' && !popup.matches(':popover-open')) {
      popup.showPopover();
    }
    positionAnchoredPopover(trigger, popup, () => closeTokenMenu(), { align: 'end' });
    if (focusFirstItem) {
      popup.querySelector<HTMLButtonElement>('[role="menuitem"]:not(:disabled)')?.focus();
    }
  }

  function toggleTokenMenu(tokenId: string, event: MouseEvent): void {
    if (openMenuTokenId === tokenId) {
      closeTokenMenu();
      return;
    }
    void openTokenMenu(tokenId, event.detail === 0);
  }

  function handleMenuTriggerKeydown(event: KeyboardEvent, tokenId: string): void {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      void openTokenMenu(tokenId, true);
    }
  }

  function handleTokenMenuKeydown(event: KeyboardEvent, tokenId: string): void {
    if (event.key === 'Tab') {
      closeTokenMenu();
      return;
    }
    if (confirmingDeleteTokenId === tokenId) return;
    if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
    const popup = event.currentTarget as HTMLElement;
    const items = [...popup.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)')];
    if (items.length === 0) return;
    const current = items.indexOf(document.activeElement as HTMLButtonElement);
    const next =
      event.key === 'Home'
        ? 0
        : event.key === 'End'
          ? items.length - 1
          : current < 0
            ? 0
            : (current + (event.key === 'ArrowUp' ? -1 : 1) + items.length) % items.length;
    event.preventDefault();
    items[next].focus();
  }

  async function openDeleteConfirmInMenu(tokenId: string): Promise<void> {
    confirmingDeleteTokenId = tokenId;
    await tick();
    if (openMenuTokenId !== tokenId || confirmingDeleteTokenId !== tokenId) return;
    const trigger = document.getElementById(`invite-token-menu-trigger-${tokenId}`);
    const popup = document.getElementById(`invite-token-menu-${tokenId}`);
    if (trigger && popup) {
      positionAnchoredPopover(trigger, popup, () => closeTokenMenu(), { align: 'end' });
      popup.querySelector<HTMLButtonElement>('[data-action="cancel-delete-invite-token"]')?.focus();
    }
  }

  function closeCreatedTokenDialog(): void {
    createdTokenDialogOpen = false;
    panelError = '';
    if (createdTokenDialog?.open && typeof createdTokenDialog.close === 'function') {
      createdTokenDialog.close();
    }
  }

  function isOutsideDialog(event: PointerEvent, dialog: HTMLDialogElement): boolean {
    const r = dialog.getBoundingClientRect();
    return (
      event.clientX < r.left ||
      event.clientX > r.right ||
      event.clientY < r.top ||
      event.clientY > r.bottom
    );
  }

  function handleCreatedDialogPointerDown(event: PointerEvent): void {
    if (!createdTokenDialog || 'closedBy' in HTMLDialogElement.prototype) return;
    createdDialogPressedOutside = isOutsideDialog(event, createdTokenDialog);
  }

  function handleCreatedDialogPointerUp(event: PointerEvent): void {
    if (!createdTokenDialog || 'closedBy' in HTMLDialogElement.prototype) return;
    if (createdDialogPressedOutside && isOutsideDialog(event, createdTokenDialog)) {
      closeCreatedTokenDialog();
    }
    createdDialogPressedOutside = false;
  }

  $effect(() => {
    const dialog = createdTokenDialog;
    if (!createdTokenDialogOpen || !dialog) return;
    if (typeof dialog.showModal === 'function') {
      if (!dialog.open) {
        dialog.showModal();
        dialog.querySelector<HTMLButtonElement>('[data-action="copy-invite-token"]')?.focus();
      }
      return () => {
        if (dialog.open) {
          dialog.close();
        }
      };
    }
    dialog.setAttribute('open', '');
    const onKeydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        closeCreatedTokenDialog();
      }
    };
    document.addEventListener('keydown', onKeydown, true);
    return () => {
      document.removeEventListener('keydown', onKeydown, true);
      dialog.removeAttribute('open');
    };
  });

  function toggleLanHelp(): void {
    lanHelpOpen = !lanHelpOpen;
    if (!lanHelpOpen) {
      closeLanHelp();
      return;
    }
    closeTokenMenu();
    if (
      typeof lanHelpBubble?.showPopover === 'function' &&
      !lanHelpBubble.matches(':popover-open')
    ) {
      lanHelpBubble.showPopover();
    }
    positionAnchoredPopover(lanHelpButton, lanHelpBubble, closeLanHelp);
  }

  $effect(() => {
    const activeId = openMenuTokenId;
    if (!activeId) return;
    const trigger = document.getElementById(`invite-token-menu-trigger-${activeId}`);
    const popup = document.getElementById(`invite-token-menu-${activeId}`);
    if (!trigger || !popup) return;
    const stopObserve = observeAnchoredPopover(trigger, popup, () => closeTokenMenu(), {
      align: 'end',
    });
    const onKeydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        closeTokenMenu({ restoreFocus: true });
      }
    };
    const onFocusin = (event: FocusEvent) => {
      const target = event.target as Node | null;
      if (target && !trigger.contains(target) && !popup.contains(target)) {
        closeTokenMenu();
      }
    };
    document.addEventListener('keydown', onKeydown, true);
    document.addEventListener('focusin', onFocusin, true);
    return () => {
      document.removeEventListener('keydown', onKeydown, true);
      document.removeEventListener('focusin', onFocusin, true);
      stopObserve();
    };
  });

  $effect(() => {
    if (!lanHelpOpen) return;
    const stopObserve = observeAnchoredPopover(lanHelpButton, lanHelpBubble, closeLanHelp);
    const onKeydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        closeLanHelp();
      }
    };
    document.addEventListener('keydown', onKeydown, true);
    return () => {
      document.removeEventListener('keydown', onKeydown, true);
      stopObserve();
    };
  });

  $effect(() => {
    const nextKey = `${serviceEpoch}::${serviceUrl}`;
    if (nextKey !== syncedConnectionKey) {
      syncedConnectionKey = nextKey;
      skillLoadSeq += 1;
      opSeq += 1;
      retrySeq += 1;
      closeLanHelp();
      closeTokenMenu();
      createdTokenDialogOpen = false;
      createdTokenCopied = false;
      busy = false;
      creatingToken = false;
      loadingSkill = false;
      retryingOnboarding = false;
      baseUrlDraft = serviceUrl;
      labelDraft = '';
      maxUsesDraft = '';
      expiresHoursDraft = '';
      latestCreatedToken = null;
      attachLatestToken = false;
      revokedTokenIds = new Set();
      confirmingDeleteTokenId = null;
      loadedSkill = null;
      panelError = '';
    }
  });

  const inviteTokens = $derived(
    Array.isArray(onboarding?.invite_tokens) ? onboarding.invite_tokens : [],
  );

  const latestTokenStillValid = $derived.by(() => {
    const _lang = $locale;
    if (!latestCreatedToken) return false;
    if (revokedTokenIds.has(latestCreatedToken.token.id)) return false;
    if (latestCreatedToken.token.revoked_at != null) return false;
    const found = inviteTokens.find(item => item.id === latestCreatedToken!.token.id);
    if (found?.revoked_at != null) return false;
    const target = found ?? latestCreatedToken.token;
    return formatInviteTokenStatus(target).kind === 'active';
  });

  function buildSkillContextKey(
    epoch: number,
    connKey: string,
    baseUrl: string,
    inviteToken: string,
    lanOpen: boolean,
  ): string {
    return JSON.stringify([epoch, connKey, baseUrl, inviteToken, lanOpen]);
  }

  const effectiveBaseUrl = $derived(baseUrlDraft.trim() || serviceUrl.trim());
  const effectiveToken = $derived(
    attachLatestToken && latestCreatedToken && latestTokenStillValid
      ? latestCreatedToken.raw_token
      : '',
  );
  const currentSkillContext = $derived(
    buildSkillContextKey(
      serviceEpoch,
      syncedConnectionKey ?? '',
      effectiveBaseUrl,
      effectiveToken,
      Boolean(onboarding?.lan_open_registration),
    ),
  );
  const freshSkillDoc = $derived(
    loadedSkill && loadedSkill.contextKey === currentSkillContext ? loadedSkill.doc : null,
  );

  $effect(() => {
    if (loadedSkill && loadedSkill.contextKey !== currentSkillContext) {
      loadedSkill = null;
    }
  });

  function isSameConnection(epoch: number, connKey: string | null): boolean {
    return epoch === serviceEpoch && connKey === syncedConnectionKey;
  }

  function showFeedback(message: string): void {
    onNotify?.(message);
  }

  async function copyToClipboard(text: string): Promise<void> {
    await clipboard.writeText(text);
  }

  function triggerBlobDownload(content: string, filename: string): void {
    if (typeof URL === 'undefined' || typeof URL.createObjectURL !== 'function') return;
    const blob = new Blob([content], { type: 'text/markdown;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = filename || 'SKILL.md';
    link.click();
    URL.revokeObjectURL(url);
  }

  async function handleRetryLoadOnboarding(): Promise<void> {
    if (!onRetryLoadOnboarding || retryingOnboarding) return;
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    const seq = ++retrySeq;
    retryingOnboarding = true;
    panelError = '';
    try {
      await onRetryLoadOnboarding();
    } catch (error) {
      if (isSameConnection(epoch, connKey) && seq === retrySeq) {
        panelError = formatServiceError(error);
      }
    } finally {
      if (isSameConnection(epoch, connKey) && seq === retrySeq) {
        retryingOnboarding = false;
      }
    }
  }

  async function handleToggleLan(input: HTMLInputElement): Promise<void> {
    if (busy || !onboardingLoaded) {
      input.checked = onboardingLoaded ? onboarding.lan_open_registration : false;
      return;
    }
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    const seq = ++opSeq;
    const targetChecked = input.checked;
    panelError = '';
    busy = true;
    try {
      const updated = await onToggleLanRegistration(targetChecked);
      if (!isSameConnection(epoch, connKey) || seq !== opSeq) return;
      input.checked = updated.lan_open_registration;
    } catch (error) {
      if (!isSameConnection(epoch, connKey) || seq !== opSeq) return;
      input.checked = onboarding.lan_open_registration;
      panelError = formatServiceError(error);
    } finally {
      if (isSameConnection(epoch, connKey) && seq === opSeq) {
        busy = false;
      }
    }
  }

  async function handleCreateToken(): Promise<void> {
    if (busy || creatingToken) return;
    panelError = '';

    const trimmedLabel = String(labelDraft ?? '').trim();
    const trimmedMaxUses = String(maxUsesDraft ?? '').trim();
    const trimmedExpires = String(expiresHoursDraft ?? '').trim();

    let maxUsesValue: number | null = null;
    if (trimmedMaxUses !== '') {
      const parsed = Number(trimmedMaxUses);
      if (!Number.isInteger(parsed) || parsed <= 0) {
        panelError = $t('最大使用次数必须为正整数。');
        return;
      }
      maxUsesValue = parsed;
    }

    let expiresInHoursValue: number | null = null;
    if (trimmedExpires !== '') {
      const parsed = Number(trimmedExpires);
      if (!Number.isFinite(parsed) || parsed <= 0) {
        panelError = $t('有效期小时数必须为大于 0 的数字。');
        return;
      }
      expiresInHoursValue = parsed;
    }

    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    const seq = ++opSeq;
    creatingToken = true;
    busy = true;
    try {
      const created = await onCreateInviteToken({
        ...(trimmedLabel ? { label: trimmedLabel } : {}),
        max_uses: maxUsesValue,
        expires_in_hours: expiresInHoursValue,
      });
      if (!isSameConnection(epoch, connKey) || seq !== opSeq) return;
      latestCreatedToken = created;
      createdTokenCopied = false;
      createdTokenDialogOpen = true;
      confirmingDeleteTokenId = null;
      attachLatestToken = true;
      labelDraft = '';
      maxUsesDraft = '';
      expiresHoursDraft = '';
    } catch (error) {
      if (!isSameConnection(epoch, connKey) || seq !== opSeq) return;
      closeTokenMenu();
      panelError = formatServiceError(error);
    } finally {
      if (isSameConnection(epoch, connKey) && seq === opSeq) {
        creatingToken = false;
        busy = false;
      }
    }
  }

  async function handleRevokeToken(id: string): Promise<void> {
    if (busy) return;
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    const seq = ++opSeq;
    panelError = '';
    busy = true;
    try {
      const revoked = await onRevokeInviteToken(id);
      if (!isSameConnection(epoch, connKey)) return;
      revokedTokenIds = new Set([...revokedTokenIds, id]);
      closeTokenMenu();
      loadedSkill = null;
      skillLoadSeq += 1;
      loadingSkill = false;
      if (latestCreatedToken && latestCreatedToken.token.id === id) {
        latestCreatedToken = {
          ...latestCreatedToken,
          token: revoked,
        };
        createdTokenDialogOpen = false;
        attachLatestToken = false;
      }
      if (seq === opSeq) {
        showFeedback($t('已作废邀请令牌'));
      }
    } catch (error) {
      if (!isSameConnection(epoch, connKey) || seq !== opSeq) return;
      closeTokenMenu();
      panelError = formatServiceError(error);
    } finally {
      if (isSameConnection(epoch, connKey) && seq === opSeq) {
        busy = false;
      }
    }
  }

  async function handleDeleteToken(id: string): Promise<void> {
    if (busy) return;
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    const seq = ++opSeq;
    panelError = '';
    busy = true;
    try {
      await onDeleteInviteToken(id);
      if (!isSameConnection(epoch, connKey)) return;
      closeTokenMenu();
      loadedSkill = null;
      skillLoadSeq += 1;
      loadingSkill = false;
      if (latestCreatedToken?.token.id === id) {
        latestCreatedToken = null;
        createdTokenDialogOpen = false;
        createdTokenCopied = false;
        attachLatestToken = false;
      }
      if (seq === opSeq) {
        showFeedback($t('已删除邀请令牌'));
      }
    } catch (error) {
      if (!isSameConnection(epoch, connKey) || seq !== opSeq) return;
      closeTokenMenu();
      panelError = formatServiceError(error);
    } finally {
      if (isSameConnection(epoch, connKey) && seq === opSeq) {
        busy = false;
      }
    }
  }

  async function handleCopyRawToken(): Promise<void> {
    if (!latestCreatedToken) return;
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    const rawToken = latestCreatedToken.raw_token;
    panelError = '';
    try {
      await copyToClipboard(rawToken);
      if (!isSameConnection(epoch, connKey)) return;
      createdTokenCopied = true;
    } catch (error) {
      if (!isSameConnection(epoch, connKey)) return;
      panelError = formatServiceError(error);
    }
  }

  async function resolveSkillDoc(explicitRawToken?: string): Promise<SkillDocument | null> {
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    const seq = ++skillLoadSeq;
    const requestBaseUrl = baseUrlDraft.trim() || serviceUrl.trim();
    const requestToken =
      explicitRawToken !== undefined
        ? explicitRawToken
        : attachLatestToken && latestCreatedToken && latestTokenStillValid
          ? latestCreatedToken.raw_token
          : '';
    const requestLanOpen = Boolean(onboarding?.lan_open_registration);
    const requestContextKey = buildSkillContextKey(
      epoch,
      connKey ?? '',
      requestBaseUrl,
      requestToken,
      requestLanOpen,
    );
    panelError = '';
    loadingSkill = true;
    try {
      const doc = await onLoadSkillMarkdown({
        ...(requestBaseUrl ? { baseUrl: requestBaseUrl } : {}),
        ...(requestToken ? { inviteToken: requestToken } : {}),
      });
      if (
        !isSameConnection(epoch, connKey) ||
        seq !== skillLoadSeq ||
        requestContextKey !== currentSkillContext
      ) {
        return null;
      }
      loadedSkill = { doc, contextKey: requestContextKey };
      return doc;
    } catch (error) {
      if (isSameConnection(epoch, connKey) && seq === skillLoadSeq) {
        panelError = formatServiceError(error);
      }
      return null;
    } finally {
      if (isSameConnection(epoch, connKey) && seq === skillLoadSeq) {
        loadingSkill = false;
      }
    }
  }

  async function handlePreviewSkill(): Promise<void> {
    await resolveSkillDoc();
  }

  async function handleCopySkill(): Promise<void> {
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    const targetContextKey = currentSkillContext;
    panelError = '';
    const doc = freshSkillDoc ?? (await resolveSkillDoc());
    if (!doc || !isSameConnection(epoch, connKey) || targetContextKey !== currentSkillContext) {
      return;
    }
    try {
      await copyToClipboard(doc.content);
      if (!isSameConnection(epoch, connKey)) return;
      showFeedback($t('SKILL.md 已复制'));
    } catch (error) {
      if (!isSameConnection(epoch, connKey)) return;
      panelError = formatServiceError(error);
    }
  }

  async function handleExportSkill(): Promise<void> {
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    const targetContextKey = currentSkillContext;
    panelError = '';
    const doc = freshSkillDoc ?? (await resolveSkillDoc());
    if (!doc || !isSameConnection(epoch, connKey) || targetContextKey !== currentSkillContext) {
      return;
    }
    try {
      if (onExportSkillMarkdown) {
        const exported = await onExportSkillMarkdown(doc);
        if (!isSameConnection(epoch, connKey)) return;
        if (exported) {
          showFeedback($t('SKILL.md 已导出'));
        }
      } else {
        triggerBlobDownload(doc.content, doc.filename || 'SKILL.md');
        if (!isSameConnection(epoch, connKey)) return;
        showFeedback($t('SKILL.md 已导出'));
      }
    } catch (error) {
      if (!isSameConnection(epoch, connKey)) return;
      panelError = formatServiceError(error);
    }
  }
</script>

<div class="agent-onboarding-panel" data-section="agent-onboarding">
  <div class="agent-onboarding-header">
    <div class="settings-extension-copy">
      <span class="settings-row-label">{$t('Agent 准入与协议')}</span>
      <span class="settings-extension-desc">
        {$t('提供接入说明文档与 Agent 令牌管理入口')}
      </span>
    </div>
  </div>

  <div class="onboarding-lan-row" data-slot="lan-registration-row">
    <div class="settings-label-group">
      <label class="settings-row-label" for="lan-open-registration-switch">
        {$t('局域网免审注册')}
      </label>
      <span class="settings-help">
        <button
          bind:this={lanHelpButton}
          type="button"
          class="settings-help-trigger"
          data-action="toggle-lan-registration-help"
          aria-label={$t('关于局域网免审注册')}
          aria-expanded={lanHelpOpen}
          aria-controls="lan-open-registration-help"
          onclick={toggleLanHelp}
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
          bind:this={lanHelpBubble}
          id="lan-open-registration-help"
          class="settings-help-popover dropdown"
          role="note"
          popover="manual"
          hidden={!lanHelpOpen}
          style="position: fixed; margin: 0; bottom: auto; right: auto; box-sizing: border-box;"
        >
          <p>{$t('开启后，外部 Agent 无需邀请令牌即可直接自注册；关闭时必须提供有效邀请令牌。')}</p>
        </div>
      </span>
    </div>
    <div class="inline-project-actions">
      {#if !onboardingLoaded}
        <span class="status-pill" data-status="onboarding-unknown">
          {$t(onboardingLoadError ? '状态未知' : '正在加载…')}
        </span>
      {/if}
      <span class="settings-switch">
        <input
          id="lan-open-registration-switch"
          type="checkbox"
          role="switch"
          data-action="toggle-lan-registration"
          aria-label={$t('局域网免审注册')}
          checked={onboardingLoaded ? onboarding.lan_open_registration : false}
          disabled={busy || !onboardingLoaded}
          aria-disabled={!onboardingLoaded ? 'true' : undefined}
          onchange={event => void handleToggleLan(event.currentTarget)}
        />
        <span class="settings-switch-track" aria-hidden="true">
          <span class="settings-switch-thumb"></span>
        </span>
      </span>
    </div>
  </div>

  {#if onboardingLoadError}
    <div
      class="drawer-error-banner onboarding-load-error-banner"
      role="alert"
      data-status="onboarding-load-error"
    >
      <span>
        {onboardingLoaded
          ? $t('准入设置刷新失败：{error}', { error: onboardingLoadError })
          : $t('准入设置加载失败，免审状态未知：{error}', { error: onboardingLoadError })}
      </span>
      {#if onRetryLoadOnboarding}
        <button
          type="button"
          class="secondary-action"
          data-action="retry-load-onboarding"
          disabled={retryingOnboarding}
          onclick={() => void handleRetryLoadOnboarding()}
        >
          {$t(retryingOnboarding ? '正在加载…' : '重试加载')}
        </button>
      {/if}
    </div>
  {/if}

  {#if panelError && !createdTokenDialogOpen}
    <p class="dialog-error settings-error" role="alert" data-status="onboarding-error">
      {panelError}
    </p>
  {/if}

  <div class="onboarding-block invite-token-manager" data-slot="invite-token-manager">
    <h4 class="invite-token-section-title">{$t('邀请令牌')}</h4>

    <div class="invite-token-form" data-slot="invite-token-form">
      <div class="project-form-field invite-token-field-label">
        <label class="composer-label" for="invite-token-label-input">{$t('令牌备注')}</label>
        <input
          id="invite-token-label-input"
          type="text"
          class="settings-text-input"
          aria-label={$t('令牌备注')}
          placeholder={$t('例如：本地 Codex 测试')}
          bind:value={labelDraft}
        />
      </div>

      <div class="project-form-field invite-token-field-uses">
        <label class="composer-label" for="invite-token-max-uses-input">{$t('最大使用次数')}</label>
        <input
          id="invite-token-max-uses-input"
          type="number"
          min="1"
          step="1"
          class="settings-text-input"
          aria-label={$t('最大使用次数')}
          placeholder={$t('不限次数')}
          bind:value={maxUsesDraft}
        />
      </div>

      <div class="project-form-field invite-token-field-expiry">
        <label class="composer-label" for="invite-token-expires-input">{$t('有效期（小时）')}</label>
        <input
          id="invite-token-expires-input"
          type="number"
          min="1"
          step="1"
          class="settings-text-input"
          aria-label={$t('有效期（小时）')}
          placeholder={$t('永不过期')}
          bind:value={expiresHoursDraft}
        />
      </div>

      <button
        type="button"
        class="primary secondary-action invite-token-create"
        data-action="create-invite-token"
        disabled={busy || creatingToken}
        onclick={() => void handleCreateToken()}
      >
        {$t(creatingToken ? '正在生成…' : '生成令牌')}
      </button>
    </div>

    {#if !onboardingLoaded && onboardingLoadError}
      <p class="project-list-empty" data-slot="invite-token-list">
        {$t('邀请令牌列表加载失败，请重试加载。')}
      </p>
    {:else if inviteTokens.length === 0}
      <p class="project-list-empty" data-slot="invite-token-list">
        {$t('暂无邀请令牌')}
      </p>
    {:else}
      <!-- 单层列表：宽容器对齐列，窄容器自动双行换行，不产生横向滚动。 -->
      <div class="invite-token-table">
        <div class="invite-token-head" aria-hidden="true">
          <span>{$t('令牌')}</span>
          <span>{$t('状态')}</span>
          <span>{$t('使用次数')}</span>
          <span>{$t('有效期')}</span>
          <span class="invite-token-head-action"></span>
        </div>
        <ul class="invite-token-list" data-slot="invite-token-list">
          {#each inviteTokens as token (token.id)}
            {@const statusInfo = (() => {
              const _lang = $locale;
              if (revokedTokenIds.has(token.id) && token.revoked_at === null) {
                return { kind: 'revoked' as const, label: $t('已作废') };
              }
              return formatInviteTokenStatus(token);
            })()}
            {@const isMenuOpen = openMenuTokenId === token.id}
            <li
              class="invite-token-row"
              data-invite-token-id={token.id}
              data-invite-token-status={statusInfo.kind}
            >
              <div class="invite-token-identity">
                {#if token.label}
                  <span class="invite-token-label" title={token.label}>{token.label}</span>
                  <code class="invite-token-prefix">{token.token_prefix}…</code>
                {:else}
                  <code class="invite-token-prefix invite-token-prefix-primary">
                    {token.token_prefix}…
                  </code>
                {/if}
              </div>
              <div class="invite-token-status-cell">
                <span class="status-pill" data-token-status={statusInfo.kind}>
                  {statusInfo.label}
                </span>
              </div>
              <span class="invite-token-usage">
                {token.max_uses !== null
                  ? $t('已使用 {used} / {max} 次', {
                      used: token.use_count,
                      max: token.max_uses,
                    })
                  : $t('已使用 {used} 次', { used: token.use_count })}
              </span>
              <span class="invite-token-expiry">
                {token.expires_at
                  ? $t('过期于 {time}', { time: formatTimestamp(token.expires_at) })
                  : $t('永不过期')}
              </span>
              <div class="invite-token-actions">
                <button
                  type="button"
                  class="icon-button invite-token-menu-trigger"
                  id={`invite-token-menu-trigger-${token.id}`}
                  data-action="toggle-invite-token-menu"
                  aria-label={`${$t('更多操作')} ${token.label || token.token_prefix}`}
                  aria-haspopup="menu"
                  aria-expanded={isMenuOpen}
                  aria-controls={`invite-token-menu-${token.id}`}
                  disabled={busy}
                  onclick={event => toggleTokenMenu(token.id, event)}
                  onkeydown={event => handleMenuTriggerKeydown(event, token.id)}
                >
                  <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                    <circle cx="3.25" cy="8" r="1.25" />
                    <circle cx="8" cy="8" r="1.25" />
                    <circle cx="12.75" cy="8" r="1.25" />
                  </svg>
                </button>
                <div
                  id={`invite-token-menu-${token.id}`}
                  class="invite-token-menu dropdown"
                  role={confirmingDeleteTokenId === token.id ? 'alertdialog' : 'menu'}
                  aria-labelledby={`invite-token-menu-trigger-${token.id}`}
                  popover="manual"
                  hidden={!isMenuOpen}
                  style="position: fixed; margin: 0; bottom: auto; right: auto; box-sizing: border-box;"
                  onkeydown={event => handleTokenMenuKeydown(event, token.id)}
                >
                  {#if confirmingDeleteTokenId === token.id}
                    <div class="invite-token-menu-confirm">
                      <p class="invite-token-menu-confirm-text">{$t('确认永久删除该令牌？')}</p>
                      <div class="invite-token-menu-confirm-actions">
                        <button
                          type="button"
                          class="secondary-action"
                          data-action="cancel-delete-invite-token"
                          disabled={busy}
                          onclick={() => closeTokenMenu({ restoreFocus: true })}
                        >
                          {$t('取消')}
                        </button>
                        <button
                          type="button"
                          class="secondary-action danger-text"
                          data-action="confirm-delete-invite-token"
                          disabled={busy}
                          aria-label={`${$t('确认删除')} ${token.label || token.token_prefix}`}
                          onclick={() => void handleDeleteToken(token.id)}
                        >
                          {$t(busy ? '正在删除…' : '确认删除')}
                        </button>
                      </div>
                    </div>
                  {:else}
                    {#if statusInfo.kind === 'active'}
                      <button
                        type="button"
                        class="invite-token-menu-item"
                        role="menuitem"
                        data-action="revoke-invite-token"
                        disabled={busy}
                        aria-label={`${$t('作废邀请令牌')} ${token.label || token.token_prefix}`}
                        onpointerdown={event => event.preventDefault()}
                        onclick={() => void handleRevokeToken(token.id)}
                      >
                        {$t('作废')}
                      </button>
                      <hr class="invite-token-menu-sep" />
                    {/if}
                    <button
                      type="button"
                      class="invite-token-menu-item danger-text"
                      role="menuitem"
                      data-action="delete-invite-token"
                      disabled={busy}
                      aria-label={`${$t('删除邀请令牌')} ${token.label || token.token_prefix}`}
                      onpointerdown={event => event.preventDefault()}
                      onclick={() => void openDeleteConfirmInMenu(token.id)}
                    >
                      {$t('删除')}
                    </button>
                  {/if}
                </div>
              </div>
            </li>
          {/each}
        </ul>
      </div>
    {/if}
  </div>

  <div class="onboarding-block" data-slot="skill-md-section">
    <div class="skill-md-header">
      <div class="settings-extension-copy">
        <span class="project-form-title">{$t('接入文档 (SKILL.md)')}</span>
        {#if serviceUrl || freshSkillDoc?.base_url}
          <span class="service-url-text">
            {freshSkillDoc?.base_url || serviceUrl}
          </span>
        {/if}
      </div>

      <div class="skill-md-actions">
        <button
          type="button"
          class="secondary-action"
          data-action="preview-skill-md"
          disabled={loadingSkill}
          onclick={() => void handlePreviewSkill()}
        >
          {$t('预览 SKILL.md')}
        </button>
        <button
          type="button"
          class="secondary-action"
          data-action="copy-skill-md"
          disabled={loadingSkill}
          onclick={() => void handleCopySkill()}
        >
          {$t('复制 SKILL.md')}
        </button>
        <button
          type="button"
          class="primary secondary-action"
          data-action="export-skill-md"
          disabled={loadingSkill}
          onclick={() => void handleExportSkill()}
        >
          {$t('导出 SKILL.md')}
        </button>
      </div>
    </div>

    <div class="skill-md-options">
      <div class="project-form-field skill-url-field">
        <label class="composer-label" for="skill-base-url-input">{$t('服务地址覆盖')}</label>
        <input
          id="skill-base-url-input"
          type="url"
          class="settings-text-input"
          aria-label={$t('服务地址覆盖')}
          placeholder={serviceUrl || 'http://127.0.0.1:8420'}
          bind:value={baseUrlDraft}
        />
      </div>

      {#if latestCreatedToken && latestTokenStillValid}
        <label class="archive-switch-label" for="attach-invite-token-switch">
          <span>{$t('附带邀请令牌')} ({latestCreatedToken.token.token_prefix}…)</span>
          <span class="settings-switch">
            <input
              id="attach-invite-token-switch"
              type="checkbox"
              role="switch"
              aria-label={$t('附带邀请令牌')}
              bind:checked={attachLatestToken}
            />
            <span class="settings-switch-track" aria-hidden="true">
              <span class="settings-switch-thumb"></span>
            </span>
          </span>
        </label>
      {/if}
    </div>

    {#if freshSkillDoc}
      <pre class="skill-md-preview" data-slot="skill-md-preview"><code>{freshSkillDoc.content}</code></pre>
    {/if}
  </div>

  {#if createdTokenDialogOpen && latestCreatedToken}
    <dialog
      bind:this={createdTokenDialog}
      class="modal-dialog created-token-dialog"
      aria-labelledby="created-invite-dialog-title"
      closedby="any"
      data-slot="created-invite-raw-token"
      onpointerdown={handleCreatedDialogPointerDown}
      onpointerup={handleCreatedDialogPointerUp}
      oncancel={event => {
        event.preventDefault();
        closeCreatedTokenDialog();
      }}
      onclose={() => {
        createdTokenDialogOpen = false;
      }}
    >
      <div class="modal-header created-token-header">
        <h2 id="created-invite-dialog-title" class="modal-title">{$t('邀请令牌已生成')}</h2>
        <button
          type="button"
          class="icon-button"
          data-action="close-created-invite-dialog"
          aria-label={$t('关闭对话框')}
          onclick={closeCreatedTokenDialog}
        >
          <svg
            width="14"
            height="14"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.6"
            stroke-linecap="round"
            aria-hidden="true"
          >
            <path d="M4 4l8 8M12 4l-8 8" />
          </svg>
        </button>
      </div>

      <p class="created-token-desc">{$t('明文令牌仅显示一次，请立即复制保存。')}</p>

      <div class="created-token-box">
        <code class="created-token-code">{latestCreatedToken.raw_token}</code>
      </div>

      <dl class="created-token-meta">
        {#if latestCreatedToken.token.label}
          <div class="created-token-meta-item">
            <dt>{$t('令牌备注')}</dt>
            <dd>{latestCreatedToken.token.label}</dd>
          </div>
        {/if}
        <div class="created-token-meta-item">
          <dt>{$t('使用上限')}</dt>
          <dd>
            {latestCreatedToken.token.max_uses !== null
              ? $t('{max} 次', { max: latestCreatedToken.token.max_uses })
              : $t('不限次数')}
          </dd>
        </div>
        <div class="created-token-meta-item">
          <dt>{$t('有效期')}</dt>
          <dd>
            {latestCreatedToken.token.expires_at
              ? formatTimestamp(latestCreatedToken.token.expires_at)
              : $t('永不过期')}
          </dd>
        </div>
      </dl>

      {#if panelError}
        <p class="dialog-error settings-error" role="alert" data-status="onboarding-error">
          {panelError}
        </p>
      {/if}

      <div class="modal-actions created-token-actions">
        <button
          type="button"
          class="secondary-action"
          data-action="dismiss-created-invite-dialog"
          onclick={closeCreatedTokenDialog}
        >
          {$t('完成')}
        </button>
        <button
          type="button"
          class="primary secondary-action"
          data-action="copy-invite-token"
          onclick={() => void handleCopyRawToken()}
        >
          {#if createdTokenCopied}
            <svg
              width="14"
              height="14"
              viewBox="0 0 16 16"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path d="M3.5 8.5 6.5 11.5 12.5 4.5" />
            </svg>
            {$t('已复制令牌')}
          {:else}
            {$t('复制令牌')}
          {/if}
        </button>
      </div>
    </dialog>
  {/if}
</div>
