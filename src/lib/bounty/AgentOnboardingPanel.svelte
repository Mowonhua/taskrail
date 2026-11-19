<script lang="ts">
  /**
   * 在设置中管理 Agent 准入、邀请令牌和接入安装。
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
  import type { SkillInstallRequest } from '../contracts';
  import { lightDismiss } from '../dialog-light-dismiss';
  import SkillInstallDialog from './SkillInstallDialog.svelte';
  import { locale, t } from '../i18n';
  import {
    formatInviteTokenStatus,
    formatServiceError,
    formatTimestamp,
    renderMarkdownAction,
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
    onPickSkillDirectory?: () => Promise<string | null>;
    onInstallSkill?: (request: SkillInstallRequest) => Promise<string[]>;
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
    onPickSkillDirectory,
    onInstallSkill,
    onNotify,
  }: Props = $props();

  let lanHelpButton: HTMLButtonElement;
  let lanHelpBubble: HTMLDivElement;
  let lanHelpOpen = $state(false);

  let labelDraft = $state('');
  let maxUsesDraft = $state('');
  let expiresHoursDraft = $state('');
  let selectedTokenId = $state<string | null>(null);
  let installOpen = $state(false);
  let installPending = $state(false);
  // 服务端只保存摘要。明文在当前连接的设置会话内保留，供本地安装使用。
  const sessionInviteTokens = new Map<string, string>();
  let latestCreatedToken = $state<CreatedInviteToken | null>(null);
  let createdTokenDialog = $state<HTMLDialogElement | undefined>(undefined);
  let createdTokenDialogOpen = $state(false);
  let createdTokenCopied = $state(false);
  let loadedSkill = $state<{ doc: SkillDocument; contextKey: string } | null>(null);
  let skillPreviewDialog: HTMLDialogElement;
  let skillPreviewOpen = $state(false);
  let skillPreviewError = $state('');
  let skillPreviewCopied = $state(false);
  let skillPreviewCopying = $state(false);

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
      labelDraft = '';
      maxUsesDraft = '';
      expiresHoursDraft = '';
      latestCreatedToken = null;
      selectedTokenId = null;
      sessionInviteTokens.clear();
      installOpen = false;
      revokedTokenIds = new Set();
      confirmingDeleteTokenId = null;
      loadedSkill = null;
      panelError = '';
    }
  });

  const inviteTokens = $derived(
    Array.isArray(onboarding?.invite_tokens) ? onboarding.invite_tokens : [],
  );

  const selectedInstallToken = $derived(
    inviteTokens.find(token => token.id === selectedTokenId &&
      !revokedTokenIds.has(token.id) && formatInviteTokenStatus(token).kind === 'active') ?? null,
  );

  $effect(() => {
    if (selectedTokenId && !selectedInstallToken) {
      selectedTokenId = null;
      installOpen = false;
    }
  });

  const currentSkillContext = $derived(
    JSON.stringify([serviceEpoch, serviceUrl, onboarding.lan_open_registration]),
  );
  const freshSkillDoc = $derived(
    loadedSkill && loadedSkill.contextKey === currentSkillContext ? loadedSkill.doc : null,
  );

  $effect(() => {
    if (loadedSkill && loadedSkill.contextKey !== currentSkillContext) {
      loadedSkill = null;
    }
    if (skillPreviewOpen && !freshSkillDoc) {
      closeSkillPreview();
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
      sessionInviteTokens.set(created.token.id, created.raw_token);
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
      sessionInviteTokens.delete(id);
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
      sessionInviteTokens.delete(id);
      if (selectedTokenId === id) selectedTokenId = null;
      closeTokenMenu();
      loadedSkill = null;
      skillLoadSeq += 1;
      loadingSkill = false;
      if (latestCreatedToken?.token.id === id) {
        latestCreatedToken = null;
        createdTokenDialogOpen = false;
        createdTokenCopied = false;
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

  async function resolveSkillDoc(): Promise<SkillDocument | null> {
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    const seq = ++skillLoadSeq;
    const requestBaseUrl = serviceUrl.trim();
    const requestContextKey = currentSkillContext;
    panelError = '';
    loadingSkill = true;
    try {
      const doc = await onLoadSkillMarkdown({
        ...(requestBaseUrl ? { baseUrl: requestBaseUrl } : {}),
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
    if (loadingSkill || skillPreviewOpen) return;
    closeLanHelp();
    const doc = await resolveSkillDoc();
    if (!doc || !freshSkillDoc || !skillPreviewDialog?.isConnected) return;
    skillPreviewError = '';
    skillPreviewCopied = false;
    skillPreviewCopying = false;
    if (typeof skillPreviewDialog.showModal === 'function') skillPreviewDialog.showModal();
    else skillPreviewDialog.open = true;
    skillPreviewOpen = true;
  }

  function closeSkillPreview(): void {
    skillPreviewOpen = false;
    if (skillPreviewDialog?.open) {
      if (typeof skillPreviewDialog.close === 'function') skillPreviewDialog.close();
      else skillPreviewDialog.open = false;
    }
  }

  async function handleCopySkill(): Promise<void> {
    const doc = freshSkillDoc;
    if (!doc || !skillPreviewOpen || skillPreviewCopying) return;
    const isCurrentPreview = () => skillPreviewOpen && skillPreviewDialog.open && doc === freshSkillDoc;
    skillPreviewCopying = true;
    skillPreviewError = '';
    skillPreviewCopied = false;
    try {
      await copyToClipboard(doc.content);
      if (isCurrentPreview()) skillPreviewCopied = true;
    } catch (error) {
      if (isCurrentPreview()) skillPreviewError = formatServiceError(error);
    } finally {
      if (isCurrentPreview()) skillPreviewCopying = false;
    }
  }

  async function handleInstallSkill(request: SkillInstallRequest): Promise<void> {
    if (request.inviteToken && (
      !selectedInstallToken || formatInviteTokenStatus(selectedInstallToken).kind !== 'active'
    )) {
      throw new Error($t('请先在列表中选中有效的邀请令牌。'));
    }
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    installPending = true;
    try {
      const paths = await onInstallSkill!(request);
      if (isSameConnection(epoch, connKey)) {
        const mcp = request.mode?.startsWith('mcp-');
        showFeedback(paths.length === 1
          ? $t(mcp ? 'MCP 已配置到 {path}' : 'Skill 已安装到 {path}', { path: paths[0] })
          : $t(mcp ? '已配置 {count} 处 MCP' : '已安装 {count} 处 Skill', { count: paths.length }));
      }
    } catch (error) {
      if (!installOpen && isSameConnection(epoch, connKey)) showFeedback(formatServiceError(error));
      throw error;
    } finally {
      installPending = false;
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
              class:is-selected={selectedTokenId === token.id}
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
              {#if statusInfo.kind === 'active' && onInstallSkill}
                <button
                  type="button"
                  class="invite-token-select"
                  aria-pressed={selectedTokenId === token.id}
                  aria-label={`${$t('选中邀请令牌')} ${token.label || token.token_prefix}`}
                  data-action="select-invite-token"
                  onclick={() => { selectedTokenId = selectedTokenId === token.id ? null : token.id; }}
                ></button>
              {/if}
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
        <span class="project-form-title">{$t('接入')}</span>
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
          aria-haspopup="dialog"
          aria-disabled={loadingSkill}
          onclick={() => void handlePreviewSkill()}
        >
          {$t('预览 SKILL.md')}
        </button>
        <button
          type="button"
          class="primary secondary-action"
          data-action="install-skill"
          aria-haspopup="dialog"
          disabled={!onInstallSkill || !onPickSkillDirectory || installPending}
          onclick={() => {
            closeLanHelp();
            installOpen = true;
          }}
        >
          {$t('安装')}
        </button>
      </div>
    </div>
  </div>

  {#if installOpen && onInstallSkill && onPickSkillDirectory}
    <SkillInstallDialog
      baseUrl={serviceUrl.trim() || freshSkillDoc?.base_url || ''}
      token={selectedInstallToken}
      rawToken={selectedInstallToken ? sessionInviteTokens.get(selectedInstallToken.id) : undefined}
      lanOpen={onboarding.lan_open_registration}
      pickDirectory={onPickSkillDirectory}
      install={handleInstallSkill}
      onClose={() => { installOpen = false; }}
    />
  {/if}

  <dialog
    bind:this={skillPreviewDialog}
    use:lightDismiss
    class="modal-dialog skill-preview-dialog"
    aria-labelledby="skill-preview-title"
    closedby="any"
    data-dialog="skill-preview"
    onclose={() => {
      if (!skillPreviewDialog.open) skillPreviewOpen = false;
    }}
  >
    <div class="modal-header">
      <h2 id="skill-preview-title" class="modal-title">{$t('接入文档 (SKILL.md)')}</h2>
      <button
        type="button"
        class="icon-button"
        aria-label={$t('关闭对话框')}
        data-action="close-skill-preview"
        onclick={closeSkillPreview}
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor"
          stroke-width="1.6" stroke-linecap="round" aria-hidden="true">
          <path d="M4 4l8 8M12 4l-8 8" />
        </svg>
      </button>
    </div>
    {#if skillPreviewOpen && freshSkillDoc}
      <!-- YAML 头用于识别 Skill，复制时保留完整原文。 -->
      <div
        class="bounty-markdown-preview skill-preview-body"
        data-slot="skill-md-preview"
        use:renderMarkdownAction={freshSkillDoc.content.replace(/^---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)/, '')}
      ></div>
      {#if skillPreviewError}
        <p class="dialog-error" role="alert" data-status="onboarding-error">{skillPreviewError}</p>
      {/if}
      <footer class="modal-actions">
        <span class="visually-hidden" role="status">{skillPreviewCopied ? $t('SKILL.md 已复制') : ''}</span>
        <button
          type="button"
          class="icon-button"
          data-action="copy-skill-md"
          aria-label={$t('复制 SKILL.md')}
          title={$t(skillPreviewCopied ? 'SKILL.md 已复制' : '复制 SKILL.md')}
          aria-disabled={skillPreviewCopying}
          onclick={() => void handleCopySkill()}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor"
            stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            {#if skillPreviewCopied}
              <path d="m5 12 4 4L19 6" />
            {:else}
              <rect x="8" y="8" width="13" height="13" rx="2" />
              <path d="M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3" />
            {/if}
          </svg>
        </button>
      </footer>
    {/if}
  </dialog>

  {#if createdTokenDialogOpen && latestCreatedToken}
    <dialog
      bind:this={createdTokenDialog}
      use:lightDismiss
      class="modal-dialog created-token-dialog"
      aria-labelledby="created-invite-dialog-title"
      closedby="any"
      data-slot="created-invite-raw-token"
      oncancel={event => {
        event.preventDefault();
        closeCreatedTokenDialog();
      }}
      onclose={closeCreatedTokenDialog}
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
