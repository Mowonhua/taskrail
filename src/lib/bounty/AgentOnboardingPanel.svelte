<script lang="ts">
  /**
   * 文件职责：在设置页面集中管理外部 Agent 准入与协议（局域网免审注册开关、邀请令牌生成与作废、SKILL.md 在线预览与导出）。
   * 定义范围：免审状态切换、一次性明文令牌横幅展示、邀请令牌状态徽章及动态 SKILL.md 预览复制下载。
   */
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
  let loadedSkill = $state<{ doc: SkillDocument; contextKey: string } | null>(null);

  let busy = $state(false);
  let creatingToken = $state(false);
  let loadingSkill = $state(false);
  let retryingOnboarding = $state(false);
  let panelError = $state('');
  let feedbackMessage = $state('');
  let syncedConnectionKey = $state<string | null>(null);
  let revokedTokenIds = $state(new Set<string>());
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

  function toggleLanHelp(): void {
    lanHelpOpen = !lanHelpOpen;
    if (!lanHelpOpen) {
      closeLanHelp();
      return;
    }
    if (
      typeof lanHelpBubble?.showPopover === 'function' &&
      !lanHelpBubble.matches(':popover-open')
    ) {
      lanHelpBubble.showPopover();
    }
    positionAnchoredPopover(lanHelpButton, lanHelpBubble, closeLanHelp);
  }

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
      loadedSkill = null;
      panelError = '';
      feedbackMessage = '';
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
    feedbackMessage = message;
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
    feedbackMessage = '';
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
    feedbackMessage = '';

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
      attachLatestToken = true;
      labelDraft = '';
      maxUsesDraft = '';
      expiresHoursDraft = '';
    } catch (error) {
      if (!isSameConnection(epoch, connKey) || seq !== opSeq) return;
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
    feedbackMessage = '';
    busy = true;
    try {
      const revoked = await onRevokeInviteToken(id);
      if (!isSameConnection(epoch, connKey)) return;
      revokedTokenIds = new Set([...revokedTokenIds, id]);
      loadedSkill = null;
      skillLoadSeq += 1;
      loadingSkill = false;
      if (latestCreatedToken && latestCreatedToken.token.id === id) {
        latestCreatedToken = {
          ...latestCreatedToken,
          token: revoked,
        };
        attachLatestToken = false;
      }
      if (seq === opSeq) {
        showFeedback($t('已作废邀请令牌'));
      }
    } catch (error) {
      if (!isSameConnection(epoch, connKey) || seq !== opSeq) return;
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
    feedbackMessage = '';
    try {
      await copyToClipboard(rawToken);
      if (!isSameConnection(epoch, connKey)) return;
      showFeedback($t('令牌已复制'));
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

  async function handlePreviewWithCreatedToken(): Promise<void> {
    if (!latestCreatedToken || !latestTokenStillValid) return;
    attachLatestToken = true;
    await resolveSkillDoc(latestCreatedToken.raw_token);
  }

  async function handlePreviewSkill(): Promise<void> {
    await resolveSkillDoc();
  }

  async function handleCopySkill(): Promise<void> {
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    const targetContextKey = currentSkillContext;
    panelError = '';
    feedbackMessage = '';
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
    feedbackMessage = '';
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

  {#if panelError}
    <p class="dialog-error settings-error" role="alert" data-status="onboarding-error">
      {panelError}
    </p>
  {/if}

  {#if feedbackMessage}
    <p class="service-feedback-success" role="status" data-status="onboarding-feedback">
      {feedbackMessage}
    </p>
  {/if}

  <div class="onboarding-block" data-slot="invite-token-manager">
    <h4 class="project-form-title">{$t('邀请令牌')}</h4>

    <div class="invite-token-form" data-slot="invite-token-form">
      <div class="invite-token-inputs">
        <div class="project-form-field">
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

        <div class="project-form-field">
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

        <div class="project-form-field">
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
      </div>

      <div class="inline-project-actions">
        <button
          type="button"
          class="primary secondary-action"
          data-action="create-invite-token"
          disabled={busy || creatingToken}
          onclick={() => void handleCreateToken()}
        >
          {$t(creatingToken ? '正在生成…' : '生成令牌')}
        </button>
      </div>
    </div>

    {#if latestCreatedToken}
      <div
        class="created-invite-banner"
        role="status"
        data-slot="created-invite-raw-token"
      >
        <div class="created-invite-copy">
          <span class="created-invite-hint">{$t('明文令牌仅显示一次，请立即复制保存：')}</span>
          <code class="created-invite-code">{latestCreatedToken.raw_token}</code>
        </div>
        <div class="created-invite-actions">
          <button
            type="button"
            class="secondary-action"
            data-action="copy-invite-token"
            onclick={() => void handleCopyRawToken()}
          >
            {$t('复制令牌')}
          </button>
          {#if latestTokenStillValid}
            <button
              type="button"
              class="secondary-action"
              data-action="preview-skill-with-token"
              disabled={loadingSkill}
              onclick={() => void handlePreviewWithCreatedToken()}
            >
              {$t('用此令牌预览 SKILL.md')}
            </button>
          {/if}
        </div>
      </div>
    {/if}

    {#if !onboardingLoaded && onboardingLoadError}
      <p class="project-list-empty" data-slot="invite-token-list">
        {$t('邀请令牌列表加载失败，请重试加载。')}
      </p>
    {:else if inviteTokens.length === 0}
      <p class="project-list-empty" data-slot="invite-token-list">
        {$t('暂无邀请令牌')}
      </p>
    {:else}
      <ul class="invite-token-list" data-slot="invite-token-list">
        {#each inviteTokens as token (token.id)}
          {@const statusInfo = (() => {
            const _lang = $locale;
            if (revokedTokenIds.has(token.id) && token.revoked_at === null) {
              return { kind: 'revoked' as const, label: $t('已作废') };
            }
            return formatInviteTokenStatus(token);
          })()}
          <li
            class="invite-token-item"
            data-invite-token-id={token.id}
            data-invite-token-status={statusInfo.kind}
          >
            <div class="invite-token-main">
              <div class="invite-token-title-row">
                <code class="invite-token-prefix">{token.token_prefix}…</code>
                {#if token.label}
                  <span class="invite-token-label">{token.label}</span>
                {/if}
                <span
                  class="status-pill"
                  data-token-status={statusInfo.kind}
                >
                  {statusInfo.label}
                </span>
              </div>
              <div class="invite-token-meta">
                <span>
                  {token.max_uses !== null
                    ? $t('已使用 {used} / {max} 次', {
                        used: token.use_count,
                        max: token.max_uses,
                      })
                    : $t('已使用 {used} 次', { used: token.use_count })}
                </span>
                <span>
                  {token.expires_at
                    ? $t('过期于 {time}', { time: formatTimestamp(token.expires_at) })
                    : $t('永不过期')}
                </span>
              </div>
            </div>

            {#if statusInfo.kind === 'active'}
              <div class="project-item-actions">
                <button
                  type="button"
                  class="secondary-action danger-text"
                  data-action="revoke-invite-token"
                  disabled={busy}
                  aria-label={`${$t('作废邀请令牌')} ${token.label || token.token_prefix}`}
                  onclick={() => void handleRevokeToken(token.id)}
                >
                  {$t('作废')}
                </button>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
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
</div>
