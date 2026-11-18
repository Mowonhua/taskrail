<script lang="ts">
  /**
   * 文件职责：承载 Agents 视图与接入指南（SKILL.md）预览导出弹窗。
   * 定义范围：按活跃度与状态筛选、低密度展示已注册 Agent 档案（隐藏 MMR 数值）、停用/启用与二次确认吊销凭证操作。
   */
  import { tick } from 'svelte';
  import type {
    AgentDetail,
    AgentStatus,
    AgentSummary,
    BountyProject,
    SkillDocument,
  } from '../api/types';
  import { clipboard } from '../clipboard';
  import { locale, t } from '../i18n';
  import RefreshButton from '../RefreshButton.svelte';
  import {
    formatActiveDuration,
    formatAgentChannelStatus,
    formatAgentStatus,
    formatAgentSubscriptionSummary,
    formatAgentTier,
    formatCompletionCost,
    formatEfficiencyPercent,
    formatMmrDelta,
    formatMmrValue,
    formatQualityGrade,
    formatRatingCapReason,
    formatRatingOutcome,
    formatServiceError,
    formatTierGateReason,
    formatTimestamp,
  } from './helpers';

  interface Props {
    agents: AgentSummary[];
    agentsLoaded?: boolean;
    agentsLoadError?: string;
    projectsById: ReadonlyMap<string, BountyProject>;
    activityFilter: 'all' | 'idle' | 'busy';
    statusFilter: 'ALL' | AgentStatus;
    skillGuideOpen?: boolean;
    skillDocument?: SkillDocument | null;
    serviceUrl?: string;
    serviceEpoch?: number;
    refreshingAgents?: boolean;
    onRefreshAgents?: () => Promise<void> | void;
    onRetryLoadAgents?: () => Promise<void> | void;
    onChangeActivityFilter: (filter: 'all' | 'idle' | 'busy') => void;
    onChangeStatusFilter: (filter: 'ALL' | AgentStatus) => void;
    onDisableAgent: (id: string) => Promise<AgentSummary>;
    onEnableAgent: (id: string) => Promise<AgentSummary>;
    onRevokeAgent: (id: string) => Promise<AgentSummary>;
    onLoadAgentDetail?: (id: string) => Promise<AgentDetail>;
    onAgentDetailOpenChange?: (open: boolean) => void;
    onOpenSkillGuide: () => void;
    onCloseSkillGuide: () => void;
    onLoadSkillMarkdown?: (options?: {
      baseUrl?: string;
      inviteToken?: string;
    }) => Promise<SkillDocument>;
    onExportSkillMarkdown?: (doc: SkillDocument) => Promise<boolean>;
    onNavigateToSettings: () => void;
    onNotify?: (message: string) => void;
  }

  let {
    agents,
    agentsLoaded = true,
    agentsLoadError = '',
    projectsById,
    activityFilter,
    statusFilter,
    skillGuideOpen = false,
    skillDocument = null,
    serviceUrl = '',
    serviceEpoch = 0,
    refreshingAgents = false,
    onRefreshAgents,
    onRetryLoadAgents,
    onChangeActivityFilter,
    onChangeStatusFilter,
    onDisableAgent,
    onEnableAgent,
    onRevokeAgent,
    onLoadAgentDetail,
    onAgentDetailOpenChange,
    onOpenSkillGuide,
    onCloseSkillGuide,
    onLoadSkillMarkdown,
    onExportSkillMarkdown,
    onNavigateToSettings,
    onNotify,
  }: Props = $props();

  const FOCUSABLE_SELECTOR =
    'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  let confirmingRevokeId = $state<string | null>(null);
  let busyAgentId = $state<string | null>(null);
  let retryingAgents = $state(false);
  let directoryError = $state('');
  let loadingSkill = $state(false);
  let modalError = $state('');
  let detailAgentId = $state<string | null>(null);
  let agentDetail = $state<AgentDetail | null>(null);
  let loadingDetail = $state(false);
  let detailError = $state('');
  let detailDialogEl = $state<HTMLDivElement | null>(null);
  let returnFocusEl: HTMLElement | null = null;
  let syncedConnectionKey = $state<string | null>(null);
  let skillLoadSeq = 0;
  let agentOpSeq = 0;
  let retrySeq = 0;
  let detailLoadSeq = 0;

  let selectedAgentSummary = $derived(
    detailAgentId
      ? (agents.find(item => item.id === detailAgentId) ?? agentDetail?.agent ?? null)
      : null,
  );

  function getFocusableElements(container: HTMLElement): HTMLElement[] {
    return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
      el => !el.hasAttribute('disabled') && !el.closest('[hidden], [inert]'),
    );
  }

  $effect(() => {
    const nextKey = `${serviceEpoch}::${serviceUrl}`;
    if (nextKey !== syncedConnectionKey) {
      syncedConnectionKey = nextKey;
      skillLoadSeq += 1;
      agentOpSeq += 1;
      retrySeq += 1;
      detailLoadSeq += 1;
      confirmingRevokeId = null;
      busyAgentId = null;
      retryingAgents = false;
      directoryError = '';
      modalError = '';
      loadingSkill = false;
      detailAgentId = null;
      agentDetail = null;
      loadingDetail = false;
      detailError = '';
      returnFocusEl = null;
      onAgentDetailOpenChange?.(false);
    }
  });

  $effect(() => {
    if (confirmingRevokeId && !agents.some(item => item.id === confirmingRevokeId && item.status !== 'revoked')) {
      confirmingRevokeId = null;
    }
  });

  $effect(() => {
    if (!skillGuideOpen) {
      skillLoadSeq += 1;
      loadingSkill = false;
      modalError = '';
    }
  });

  $effect(() => {
    const activeId = detailAgentId;
    const dialog = detailDialogEl;
    void detailError;
    void loadingDetail;
    void agentDetail;
    if (activeId === null || !dialog) return;
    const currentFocus = document.activeElement;
    if (!(currentFocus instanceof Node) || !dialog.contains(currentFocus)) {
      const target = getFocusableElements(dialog)[0] ?? dialog;
      target.focus();
    }
  });

  function isSameConnection(epoch: number, connKey: string | null): boolean {
    return epoch === serviceEpoch && connKey === syncedConnectionKey;
  }

  async function loadAgentDetail(id: string): Promise<void> {
    if (!onLoadAgentDetail) return;
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    const seq = ++detailLoadSeq;
    if (
      detailDialogEl &&
      typeof document !== 'undefined' &&
      document.activeElement instanceof Element &&
      detailDialogEl.contains(document.activeElement) &&
      document.activeElement.closest('[data-status="agent-detail-error"]')
    ) {
      const fallbackFocus =
        getFocusableElements(detailDialogEl).find(
          el => !el.closest('[data-status="agent-detail-error"]'),
        ) ?? detailDialogEl;
      fallbackFocus.focus();
    }
    loadingDetail = true;
    detailError = '';
    try {
      const result = await onLoadAgentDetail(id);
      if (!isSameConnection(epoch, connKey) || seq !== detailLoadSeq || detailAgentId !== id) {
        return;
      }
      agentDetail = result;
    } catch (error) {
      if (isSameConnection(epoch, connKey) && seq === detailLoadSeq && detailAgentId === id) {
        detailError = formatServiceError(error);
      }
    } finally {
      if (isSameConnection(epoch, connKey) && seq === detailLoadSeq) {
        loadingDetail = false;
      }
    }
  }

  function openAgentDetail(id: string): void {
    if (!onLoadAgentDetail) return;
    if (detailAgentId === null && typeof document !== 'undefined') {
      const active = document.activeElement;
      returnFocusEl = active instanceof HTMLElement ? active : null;
    }
    detailAgentId = id;
    agentDetail = null;
    detailError = '';
    onAgentDetailOpenChange?.(true);
    void loadAgentDetail(id);
  }

  function closeAgentDetail(): void {
    const opener = returnFocusEl;
    returnFocusEl = null;
    detailLoadSeq += 1;
    detailAgentId = null;
    agentDetail = null;
    loadingDetail = false;
    detailError = '';
    onAgentDetailOpenChange?.(false);
    if (opener) {
      void tick().then(() => {
        if (opener.isConnected) opener.focus();
      });
    }
  }

  function normalizeCapabilities(capabilities: readonly string[] | null | undefined): string[] {
    if (!Array.isArray(capabilities)) return [];
    return Array.from(new Set(capabilities.map(item => String(item ?? '').trim()).filter(Boolean)));
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

  async function handleRetryLoadAgents(): Promise<void> {
    if (!onRetryLoadAgents || retryingAgents) return;
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    const seq = ++retrySeq;
    retryingAgents = true;
    directoryError = '';
    try {
      await onRetryLoadAgents();
    } catch (error) {
      if (isSameConnection(epoch, connKey) && seq === retrySeq) {
        directoryError = formatServiceError(error);
      }
    } finally {
      if (isSameConnection(epoch, connKey) && seq === retrySeq) {
        retryingAgents = false;
      }
    }
  }

  async function handleDisable(agent: AgentSummary): Promise<void> {
    if (busyAgentId) return;
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    const seq = ++agentOpSeq;
    directoryError = '';
    busyAgentId = agent.id;
    try {
      await onDisableAgent(agent.id);
    } catch (error) {
      if (isSameConnection(epoch, connKey) && seq === agentOpSeq) {
        directoryError = formatServiceError(error);
      }
    } finally {
      if (isSameConnection(epoch, connKey) && seq === agentOpSeq) {
        busyAgentId = null;
      }
    }
  }

  async function handleEnable(agent: AgentSummary): Promise<void> {
    if (busyAgentId) return;
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    const seq = ++agentOpSeq;
    directoryError = '';
    busyAgentId = agent.id;
    try {
      await onEnableAgent(agent.id);
    } catch (error) {
      if (isSameConnection(epoch, connKey) && seq === agentOpSeq) {
        directoryError = formatServiceError(error);
      }
    } finally {
      if (isSameConnection(epoch, connKey) && seq === agentOpSeq) {
        busyAgentId = null;
      }
    }
  }

  async function handleConfirmRevoke(agent: AgentSummary): Promise<void> {
    if (busyAgentId) return;
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    const seq = ++agentOpSeq;
    directoryError = '';
    busyAgentId = agent.id;
    try {
      await onRevokeAgent(agent.id);
      if (isSameConnection(epoch, connKey) && seq === agentOpSeq) {
        confirmingRevokeId = null;
      }
    } catch (error) {
      if (isSameConnection(epoch, connKey) && seq === agentOpSeq) {
        directoryError = formatServiceError(error);
      }
    } finally {
      if (isSameConnection(epoch, connKey) && seq === agentOpSeq) {
        busyAgentId = null;
      }
    }
  }

  async function ensureSkillDocument(): Promise<SkillDocument | null> {
    if (skillDocument) return skillDocument;
    if (!onLoadSkillMarkdown) return null;
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    const seq = ++skillLoadSeq;
    modalError = '';
    loadingSkill = true;
    try {
      const doc = await onLoadSkillMarkdown(serviceUrl ? { baseUrl: serviceUrl } : undefined);
      if (!isSameConnection(epoch, connKey) || seq !== skillLoadSeq || !skillGuideOpen) {
        return null;
      }
      return doc;
    } catch (error) {
      if (isSameConnection(epoch, connKey) && seq === skillLoadSeq && skillGuideOpen) {
        modalError = formatServiceError(error);
      }
      return null;
    } finally {
      if (isSameConnection(epoch, connKey) && seq === skillLoadSeq) {
        loadingSkill = false;
      }
    }
  }

  async function handleCopySkill(): Promise<void> {
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    modalError = '';
    const doc = await ensureSkillDocument();
    if (!doc || !isSameConnection(epoch, connKey) || !skillGuideOpen) return;
    try {
      await copyToClipboard(doc.content);
      if (!isSameConnection(epoch, connKey) || !skillGuideOpen) return;
      onNotify?.($t('SKILL.md 已复制'));
    } catch (error) {
      if (isSameConnection(epoch, connKey) && skillGuideOpen) {
        modalError = formatServiceError(error);
      }
    }
  }

  async function handleExportSkill(): Promise<void> {
    const epoch = serviceEpoch;
    const connKey = syncedConnectionKey;
    modalError = '';
    const doc = await ensureSkillDocument();
    if (!doc || !isSameConnection(epoch, connKey) || !skillGuideOpen) return;
    try {
      if (onExportSkillMarkdown) {
        const exported = await onExportSkillMarkdown(doc);
        if (!isSameConnection(epoch, connKey)) return;
        if (exported) {
          onNotify?.($t('SKILL.md 已导出'));
        }
      } else {
        triggerBlobDownload(doc.content, doc.filename || 'SKILL.md');
        if (!isSameConnection(epoch, connKey)) return;
        onNotify?.($t('SKILL.md 已导出'));
      }
    } catch (error) {
      if (isSameConnection(epoch, connKey) && skillGuideOpen) {
        modalError = formatServiceError(error);
      }
    }
  }

  function handleWindowFocusIn(event: FocusEvent): void {
    if (detailAgentId === null || !detailDialogEl) return;
    const target = event.target;
    if (target instanceof Node && detailDialogEl.contains(target)) return;
    const focusTarget = getFocusableElements(detailDialogEl)[0] ?? detailDialogEl;
    focusTarget.focus();
  }

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (detailAgentId !== null && detailDialogEl && event.key === 'Tab') {
      const focusables = getFocusableElements(detailDialogEl);
      if (focusables.length === 0) {
        event.preventDefault();
        detailDialogEl.focus();
        return;
      }
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      const active = document.activeElement;
      if (!detailDialogEl.contains(active)) {
        event.preventDefault();
        (event.shiftKey ? last : first).focus();
        return;
      }
      if (event.shiftKey && (active === first || active === detailDialogEl)) {
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
    if (event.key !== 'Escape') return;
    if (detailAgentId !== null) {
      event.preventDefault();
      event.stopPropagation();
      closeAgentDetail();
      return;
    }
    if (skillGuideOpen) {
      event.preventDefault();
      event.stopPropagation();
      onCloseSkillGuide();
    }
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} onfocusin={handleWindowFocusIn} />

<div class="workspace-toolbar" data-slot="agents-toolbar">
  <div class="toolbar-filters-group">
    <div class="filter-pills" role="group" aria-label={$t('按活跃度筛选')}>
      {#each [
        { id: 'all', label: $t('全部 Agent') },
        { id: 'idle', label: $t('空闲可接单') },
        { id: 'busy', label: $t('执行任务中') },
      ] as item (item.id)}
        <button
          type="button"
          class="filter-pill"
          class:active={activityFilter === item.id}
          data-agent-activity-filter={item.id}
          onclick={() => onChangeActivityFilter(item.id as 'all' | 'idle' | 'busy')}
        >
          {item.label}
        </button>
      {/each}
    </div>

    <span class="toolbar-divider" aria-hidden="true"></span>

    <div class="filter-pills" role="group" aria-label={$t('按状态筛选')}>
      {#each [
        { id: 'ALL', label: $t('全部') },
        { id: 'active', label: $t('正常接入') },
        { id: 'disabled', label: $t('已停用') },
        { id: 'revoked', label: $t('已吊销') },
      ] as item (item.id)}
        <button
          type="button"
          class="filter-pill"
          class:active={statusFilter === item.id}
          data-agent-status-filter={item.id}
          onclick={() => onChangeStatusFilter(item.id as 'ALL' | AgentStatus)}
        >
          {item.label}
        </button>
      {/each}
    </div>
  </div>

  <div class="toolbar-end-actions">
    <button
      type="button"
      class="secondary-action"
      data-action="open-skill-guide"
      onclick={onOpenSkillGuide}
    >
      {$t('接入指南 (SKILL.md)')}
    </button>
    {#if onRefreshAgents || onRetryLoadAgents}
      <RefreshButton
        action="refresh-agents"
        busy={refreshingAgents || retryingAgents}
        onRefresh={onRefreshAgents ?? handleRetryLoadAgents}
      />
    {/if}
  </div>
</div>

{#if agentsLoadError}
  <div
    class="drawer-error-banner directory-load-error-banner"
    role="alert"
    data-status="agent-directory-load-error"
  >
    <span>{$t('Agent 列表加载失败：{error}', { error: agentsLoadError })}</span>
    {#if onRetryLoadAgents}
      <button
        type="button"
        class="secondary-action"
        data-action="retry-load-agents"
        disabled={retryingAgents}
        onclick={() => void handleRetryLoadAgents()}
      >
        {$t(retryingAgents ? '正在加载…' : '重试加载')}
      </button>
    {/if}
  </div>
{/if}

{#if directoryError}
  <div class="drawer-error-banner" role="alert" data-status="agent-directory-error">
    <span>{directoryError}</span>
  </div>
{/if}

<div class="skeleton-stage" data-slot="agents-region">
  {#if agents.length === 0}
    {#if agentsLoaded && !agentsLoadError}
      <div class="empty-state-card" data-slot="agent-empty-state">
        <h2 class="empty-state-title">{$t('暂无已注册的 Agent')}</h2>
        <p class="empty-state-copy">
          {$t('外部 Agent 可阅读接入说明自助完成注册并获取专属凭证，在此展示能力标签与履约统计。')}
        </p>
        <div class="agent-empty-actions">
          <button
            type="button"
            class="primary"
            data-action="empty-open-skill-guide"
            onclick={onOpenSkillGuide}
          >
            {$t('接入指南 (SKILL.md)')}
          </button>
          <button
            type="button"
            class="secondary-action"
            data-action="empty-open-onboarding-settings"
            onclick={onNavigateToSettings}
          >
            {$t('前往设置')}
          </button>
        </div>
      </div>
    {:else if !agentsLoadError}
      <p class="detail-empty-hint" data-status="agent-directory-loading">{$t('正在加载…')}</p>
    {/if}
  {:else}
    <div class="bounty-card-grid" data-slot="agent-list">
      {#each agents as agent (agent.id)}
        {@const tierLabel = (() => {
          const _lang = $locale;
          return formatAgentTier(agent.tier);
        })()}
        {@const statusLabel = (() => {
          const _lang = $locale;
          return formatAgentStatus(agent.status);
        })()}
        {@const channelInfo = (() => {
          const _lang = $locale;
          return agentsLoadError
            ? { kind: 'unknown' as const, label: $t('状态未知') }
            : formatAgentChannelStatus(agent);
        })()}
        {@const subscriptionSummary = (() => {
          const _lang = $locale;
          return formatAgentSubscriptionSummary(agent, projectsById);
        })()}
        {@const uniqueCapabilities = normalizeCapabilities(agent.capabilities)}
        <article
          class="agent-card"
          data-agent-id={agent.id}
          data-agent-status={agent.status}
          data-agent-tier={agent.tier}
        >
          {#snippet cardBody()}
            <div class="agent-card-header">
              <div class="agent-card-identity">
                <h3 class="agent-card-name">{agent.name}</h3>
                <span
                  class="difficulty-badge"
                  data-agent-tier-badge={agent.tier}
                  title={tierLabel}
                >
                  [{agent.tier}] {tierLabel}
                </span>
              </div>

              <div class="bounty-card-badges">
                <span
                  class="status-pill"
                  data-agent-status-pill={agent.status}
                >
                  {statusLabel}
                </span>
                <span
                  class="status-pill"
                  data-agent-channel={channelInfo.kind}
                >
                  {channelInfo.label}
                </span>
                <span
                  class="detail-meta-pill"
                  data-agent-activity={agent.active_lease_count > 0 ? 'busy' : 'idle'}
                >
                  {agent.active_lease_count > 0
                    ? $t('{count} 个进行中租约', { count: agent.active_lease_count })
                    : $t('空闲')}
                </span>
              </div>
            </div>

            {#if agent.description.trim()}
              <p class="agent-card-description">{agent.description}</p>
            {/if}

            {#if uniqueCapabilities.length > 0}
              <div class="agent-capability-list" aria-label={$t('能力标签')}>
                {#each uniqueCapabilities as cap (cap)}
                  <span class="settings-tag">{cap}</span>
                {/each}
              </div>
            {/if}

            <div class="agent-card-meta">
              <div class="agent-meta-item">
                <span class="agent-meta-label">{$t('订阅范围')}:</span>
                <span class="agent-meta-value" data-slot="agent-subscriptions">
                  {subscriptionSummary}
                </span>
              </div>

              <div class="agent-meta-item">
                <span class="agent-meta-label">{$t('凭证前缀')}:</span>
                <code class="agent-key-prefix">{agent.api_key_prefix}…</code>
              </div>

              <div class="agent-meta-item">
                <span class="agent-meta-label">{$t('最近推送')}:</span>
                <span class="agent-meta-value" data-slot="agent-last-pushed">
                  {agent.last_pushed_at ? formatTimestamp(agent.last_pushed_at) : $t('从未推送')}
                </span>
              </div>

              <div class="agent-meta-item">
                <span class="agent-meta-label">{$t('最近活跃')}:</span>
                <span class="agent-meta-value">
                  {agent.last_seen_at ? formatTimestamp(agent.last_seen_at) : $t('从未活跃')}
                </span>
              </div>

              {#if agent.timeout_count > 0 || agent.abandon_count > 0}
                <div class="agent-meta-item" data-slot="agent-lease-incidents">
                  <span class="agent-meta-label">{$t('租约异常')}:</span>
                  <span class="agent-meta-value">
                    {$t('超时 {timeout} 次 弃单 {abandon} 次', {
                      timeout: agent.timeout_count,
                      abandon: agent.abandon_count,
                    })}
                  </span>
                </div>
              {/if}
            </div>
          {/snippet}

          {#if onLoadAgentDetail}
            <button
              type="button"
              class="agent-card-body-button"
              data-action="open-agent-detail"
              onclick={() => openAgentDetail(agent.id)}
            >
              {@render cardBody()}
            </button>
          {:else}
            <div class="agent-card-body-static">
              {@render cardBody()}
            </div>
          {/if}

          <div class="agent-card-footer">
            {#if agent.status === 'revoked'}
              <div class="project-item-actions">
                {#if onLoadAgentDetail}
                  <button
                    type="button"
                    class="secondary-action"
                    data-action="view-agent-detail"
                    onclick={() => openAgentDetail(agent.id)}
                  >
                    {$t('查看评级详情')}
                  </button>
                {/if}
                <span class="status-pill" data-agent-status-pill="revoked">
                  {$t('已吊销')}
                </span>
              </div>
            {:else if confirmingRevokeId === agent.id}
              <div class="agent-revoke-confirm" role="group" aria-label={$t('吊销凭证')}>
                <span class="agent-revoke-prompt">
                  {$t('确认吊销该 Agent 的凭证？吊销后无法恢复，该 Agent 必须重新注册。')}
                </span>
                <div class="agent-revoke-actions">
                  <button
                    type="button"
                    class="secondary-action"
                    data-action="cancel-revoke-agent"
                    disabled={busyAgentId === agent.id}
                    onclick={() => {
                      confirmingRevokeId = null;
                    }}
                  >
                    {$t('取消')}
                  </button>
                  <button
                    type="button"
                    class="secondary-action danger-text"
                    data-action="confirm-revoke-agent"
                    disabled={busyAgentId === agent.id}
                    onclick={() => void handleConfirmRevoke(agent)}
                  >
                    {$t('确认吊销')}
                  </button>
                </div>
              </div>
            {:else}
              <div class="project-item-actions">
                {#if onLoadAgentDetail}
                  <button
                    type="button"
                    class="secondary-action"
                    data-action="view-agent-detail"
                    onclick={() => openAgentDetail(agent.id)}
                  >
                    {$t('查看评级详情')}
                  </button>
                {/if}
                {#if agent.status === 'active'}
                  <button
                    type="button"
                    class="secondary-action"
                    data-action="disable-agent"
                    disabled={busyAgentId === agent.id}
                    onclick={() => void handleDisable(agent)}
                  >
                    {$t('停用')}
                  </button>
                {:else if agent.status === 'disabled'}
                  <button
                    type="button"
                    class="secondary-action"
                    data-action="enable-agent"
                    disabled={busyAgentId === agent.id}
                    onclick={() => void handleEnable(agent)}
                  >
                    {$t('启用')}
                  </button>
                {/if}
                <button
                  type="button"
                  class="secondary-action danger-text"
                  data-action="revoke-agent"
                  disabled={busyAgentId === agent.id}
                  onclick={() => {
                    confirmingRevokeId = agent.id;
                  }}
                >
                  {$t('吊销凭证')}
                </button>
              </div>
            {/if}
          </div>
        </article>
      {/each}
    </div>
  {/if}
</div>

{#if skillGuideOpen}
  <div
    class="modal-backdrop"
    role="presentation"
    onclick={event => {
      if (event.target === event.currentTarget) onCloseSkillGuide();
    }}
  >
    <div
      class="modal-dialog skill-guide-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={$t('接入文档 (SKILL.md)')}
      data-dialog="skill-guide"
    >
      <div class="modal-header">
        <div class="settings-extension-copy">
          <h2 class="modal-title">{$t('接入文档 (SKILL.md)')}</h2>
          {#if skillDocument?.base_url || serviceUrl}
            <span class="service-url-text">{skillDocument?.base_url || serviceUrl}</span>
          {/if}
        </div>
        <button
          type="button"
          class="icon-button"
          aria-label={$t('关闭对话框')}
          onclick={onCloseSkillGuide}
        >
          ✕
        </button>
      </div>

      {#if modalError}
        <p class="dialog-error settings-error" role="alert">{modalError}</p>
      {/if}

      <div class="skill-guide-body">
        {#if loadingSkill && !skillDocument}
          <p class="detail-empty-hint">{$t('正在加载…')}</p>
        {:else if skillDocument}
          <pre class="skill-md-preview" data-slot="skill-md-preview"><code>{skillDocument.content}</code></pre>
        {/if}
      </div>

      <div class="modal-actions composer-footer">
        <button
          type="button"
          class="secondary-action"
          data-action="jump-to-onboarding-settings"
          onclick={() => {
            onCloseSkillGuide();
            onNavigateToSettings();
          }}
        >
          {$t('前往生成邀请令牌')}
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
  </div>
{/if}

{#if detailAgentId !== null}
  {@const headerTier = agentDetail?.agent.tier ?? selectedAgentSummary?.tier ?? 'Unranked'}
  {@const headerTierLabel = (() => {
    const _lang = $locale;
    return formatAgentTier(headerTier);
  })()}
  <div
    class="modal-backdrop"
    role="presentation"
    onclick={event => {
      if (event.target === event.currentTarget) closeAgentDetail();
    }}
  >
    <div
      bind:this={detailDialogEl}
      tabindex="-1"
      class="modal-dialog agent-detail-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={$t('Agent 评级详情')}
      data-dialog="agent-detail"
      data-agent-id={detailAgentId}
    >
      <div class="modal-header">
        <div class="settings-extension-copy">
          <div class="agent-card-identity">
            <h2 class="modal-title">{$t('Agent 评级详情')}</h2>
            {#if selectedAgentSummary}
              <span class="agent-detail-name">{selectedAgentSummary.name}</span>
            {/if}
            <span class="difficulty-badge" data-slot="agent-detail-header-tier">
              [{headerTier}] {headerTierLabel}
            </span>
          </div>
        </div>
        <button
          type="button"
          class="icon-button"
          aria-label={$t('关闭对话框')}
          onclick={closeAgentDetail}
        >
          ✕
        </button>
      </div>

      {#if detailError}
        <div
          class="drawer-error-banner agent-detail-error-banner"
          role="alert"
          data-status="agent-detail-error"
        >
          <span>{detailError}</span>
          <button
            type="button"
            class="secondary-action"
            data-action="retry-agent-detail"
            disabled={loadingDetail}
            onclick={() => {
              if (detailAgentId) void loadAgentDetail(detailAgentId);
            }}
          >
            {$t(loadingDetail ? '正在加载…' : '重试加载')}
          </button>
        </div>
      {/if}

      <div class="agent-detail-body">
        {#if loadingDetail && !agentDetail}
          <p class="detail-empty-hint" data-status="agent-detail-loading">{$t('正在加载…')}</p>
        {:else if agentDetail}
          {@const detailTierLabel = (() => {
            const _lang = $locale;
            return formatAgentTier(agentDetail.agent.tier);
          })()}
          <div class="agent-detail-stats" data-slot="agent-detail-summary">
            <div class="agent-detail-stat-card is-primary">
              <span class="agent-detail-stat-label">{$t('隐藏 MMR')}</span>
              <strong class="agent-detail-stat-value agent-detail-mmr" data-slot="agent-detail-mmr">
                {formatMmrValue(agentDetail.mmr)}
              </strong>
            </div>
            <div class="agent-detail-stat-card is-primary">
              <span class="agent-detail-stat-label">{$t('当前段位')}</span>
              <strong class="agent-detail-stat-value" data-slot="agent-detail-tier">
                [{agentDetail.agent.tier}] {detailTierLabel}
              </strong>
            </div>
            <div class="agent-detail-stat-card is-primary">
              <span class="agent-detail-stat-label">{$t('定级状态')}</span>
              <strong class="agent-detail-stat-value" data-slot="agent-detail-placement">
                {agentDetail.placement_count >= agentDetail.placement_required
                  ? $t('已完成定级 ({count}/{required})', {
                      count: agentDetail.placement_count,
                      required: agentDetail.placement_required,
                    })
                  : $t('定级进度 {count}/{required}', {
                      count: agentDetail.placement_count,
                      required: agentDetail.placement_required,
                    })}
              </strong>
            </div>
            <div class="agent-detail-stat-card">
              <span class="agent-detail-stat-label">{$t('已完成')}</span>
              <strong class="agent-detail-stat-value" data-slot="agent-detail-completed">
                {agentDetail.completed_count}
              </strong>
            </div>
            <div class="agent-detail-stat-card">
              <span class="agent-detail-stat-label">{$t('失败次数')}</span>
              <strong class="agent-detail-stat-value" data-slot="agent-detail-failed">
                {agentDetail.failed_count}
              </strong>
            </div>
            <div class="agent-detail-stat-card">
              <span class="agent-detail-stat-label">{$t('超时次数')}</span>
              <strong class="agent-detail-stat-value" data-slot="agent-detail-timeout">
                {agentDetail.agent.timeout_count}
              </strong>
            </div>
            <div class="agent-detail-stat-card">
              <span class="agent-detail-stat-label">{$t('弃单次数')}</span>
              <strong class="agent-detail-stat-value" data-slot="agent-detail-abandon">
                {agentDetail.agent.abandon_count}
              </strong>
            </div>
            <div class="agent-detail-stat-card">
              <span class="agent-detail-stat-label">{$t('核验 A/S 胜场')}</span>
              <strong class="agent-detail-stat-value" data-slot="agent-detail-as-wins">
                {agentDetail.verified_as_wins}
              </strong>
            </div>
            <div class="agent-detail-stat-card">
              <span class="agent-detail-stat-label">{$t('核验 S 胜场')}</span>
              <strong class="agent-detail-stat-value" data-slot="agent-detail-s-wins">
                {agentDetail.verified_s_wins}
              </strong>
            </div>
          </div>

          <section class="agent-detail-ledger-section" data-slot="agent-rating-ledger">
            <h3 class="agent-detail-section-title">{$t('评级结算流水')}</h3>
            {#if agentDetail.ledger.length === 0}
              <p class="detail-empty-hint" data-status="agent-ledger-empty">
                {$t('暂无评级结算记录')}
              </p>
            {:else}
              <div class="rating-ledger-list">
                {#each agentDetail.ledger as entry (entry.id)}
                  {@const outcomeLabel = (() => {
                    const _lang = $locale;
                    return formatRatingOutcome(entry.outcome);
                  })()}
                  {@const capReasonLabel = (() => {
                    const _lang = $locale;
                    return formatRatingCapReason(entry.cap_reason);
                  })()}
                  {@const tierGateLabel = (() => {
                    const _lang = $locale;
                    return formatTierGateReason(entry.tier_gate_reason);
                  })()}
                  {@const qualityLabel = (() => {
                    const _lang = $locale;
                    return entry.quality_grade ? formatQualityGrade(entry.quality_grade) : null;
                  })()}
                  {@const tierBeforeLabel = (() => {
                    const _lang = $locale;
                    return formatAgentTier(entry.tier_before);
                  })()}
                  {@const tierAfterLabel = (() => {
                    const _lang = $locale;
                    return formatAgentTier(entry.tier_after);
                  })()}
                  {@const costLabel = formatCompletionCost(entry.completion_cost)}
                  <div
                    class="rating-ledger-entry"
                    data-ledger-id={entry.id}
                    data-outcome={entry.outcome}
                  >
                    <div class="rating-ledger-header">
                      <div class="rating-ledger-heading">
                        <strong class="rating-ledger-title">{entry.bounty_title}</strong>
                        <div class="rating-ledger-badges">
                          <span class="status-pill" data-slot="ledger-outcome">{outcomeLabel}</span>
                          <span class="detail-meta-pill" data-slot="ledger-arbitrated">
                            {entry.arbitrated ? $t('仲裁验收') : $t('直接自结')}
                          </span>
                          <span class="detail-meta-pill" data-slot="ledger-difficulty">
                            {entry.original_difficulty !== entry.effective_difficulty
                              ? $t('难度 {original} 校准为 {effective}', {
                                  original: entry.original_difficulty,
                                  effective: entry.effective_difficulty,
                                })
                              : $t('难度 {difficulty}', { difficulty: entry.effective_difficulty })}
                          </span>
                          {#if qualityLabel}
                            <span
                              class="quality-pill"
                              data-quality-grade={entry.quality_grade}
                              data-slot="ledger-quality-grade"
                            >
                              {qualityLabel}
                            </span>
                          {/if}
                          <span class="detail-meta-pill" data-slot="ledger-k-factor">
                            {entry.is_placement
                              ? $t('定级赛 K={k}', { k: entry.k_factor })
                              : $t('常规赛 K={k}', { k: entry.k_factor })}
                          </span>
                          {#if entry.self_dealt}
                            <span class="status-pill" data-slot="ledger-self-dealt">
                              {$t('自发自接')}
                            </span>
                          {/if}
                        </div>
                      </div>
                      <div class="rating-ledger-delta-box">
                        <strong
                          class="rating-ledger-delta"
                          class:positive={entry.applied_delta > 0}
                          class:negative={entry.applied_delta < 0}
                          data-slot="ledger-delta"
                        >
                          {formatMmrDelta(entry.applied_delta)}
                        </strong>
                        {#if Math.abs(entry.raw_delta - entry.applied_delta) >= 0.05}
                          <span class="rating-ledger-raw-delta" data-slot="ledger-raw-delta">
                            {$t('原始变化 {delta}', { delta: formatMmrDelta(entry.raw_delta) })}
                          </span>
                        {/if}
                      </div>
                    </div>

                    <div class="rating-ledger-metrics">
                      <div class="rating-ledger-metric">
                        <span class="agent-meta-label">{$t('MMR 变动')}:</span>
                        <span class="agent-meta-value" data-slot="ledger-mmr-range">
                          {formatMmrValue(entry.mmr_before)} → {formatMmrValue(entry.mmr_after)}
                        </span>
                      </div>
                      <div class="rating-ledger-metric">
                        <span class="agent-meta-label">{$t('段位变动')}:</span>
                        <span class="agent-meta-value" data-slot="ledger-tier-change">
                          {tierBeforeLabel} → {tierAfterLabel}
                        </span>
                      </div>
                      <div class="rating-ledger-metric">
                        <span class="agent-meta-label">{$t('质量分 Q')}:</span>
                        <span class="agent-meta-value" data-slot="ledger-quality-score">
                          {entry.quality_score.toFixed(2)}
                          {#if entry.rejection_count > 0}
                            ({$t('打回 {count} 次', { count: entry.rejection_count })})
                          {/if}
                        </span>
                      </div>
                      <div class="rating-ledger-metric">
                        <span class="agent-meta-label">{$t('时间效率 φ_time')}:</span>
                        <span class="agent-meta-value" data-slot="ledger-time-efficiency">
                          {formatEfficiencyPercent(entry.time_efficiency)} ({formatActiveDuration(entry.active_duration_ms)})
                        </span>
                      </div>
                      <div class="rating-ledger-metric">
                        <span class="agent-meta-label">{$t('成本效率 φ_cost')}:</span>
                        <span class="agent-meta-value" data-slot="ledger-cost-efficiency">
                          {formatEfficiencyPercent(entry.cost_efficiency)} ({costLabel ?? $t('未申报（中性 50%）')})
                        </span>
                      </div>
                      <div class="rating-ledger-metric">
                        <span class="agent-meta-label">{$t('结算时间')}:</span>
                        <span class="agent-meta-value" data-slot="ledger-created-at">
                          {formatTimestamp(entry.created_at)}
                        </span>
                      </div>
                    </div>

                    {#if capReasonLabel || tierGateLabel}
                      <div class="rating-ledger-flags">
                        {#if capReasonLabel}
                          <span class="detail-meta-pill" data-slot="ledger-cap-reason">
                            {capReasonLabel}
                          </span>
                        {/if}
                        {#if tierGateLabel}
                          <span class="detail-meta-pill" data-slot="ledger-tier-gate">
                            {tierGateLabel}
                          </span>
                        {/if}
                      </div>
                    {/if}
                  </div>
                {/each}
              </div>
            {/if}
          </section>
        {/if}
      </div>
    </div>
  </div>
{/if}
