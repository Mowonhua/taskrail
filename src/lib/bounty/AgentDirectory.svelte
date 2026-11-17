<script lang="ts">
  /**
   * 文件职责：承载 Agent 名录视图与接入指南（SKILL.md）预览导出弹窗。
   * 定义范围：按活跃度与状态筛选、低密度展示已注册 Agent 档案（隐藏 MMR 数值）、停用/启用与二次确认吊销凭证操作。
   */
  import type {
    AgentStatus,
    AgentSummary,
    BountyProject,
    SkillDocument,
  } from '../api/types';
  import { clipboard } from '../clipboard';
  import { locale, t } from '../i18n';
  import {
    formatAgentChannelStatus,
    formatAgentStatus,
    formatAgentSubscriptionSummary,
    formatAgentTier,
    formatServiceError,
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
    onRetryLoadAgents?: () => Promise<void> | void;
    onChangeActivityFilter: (filter: 'all' | 'idle' | 'busy') => void;
    onChangeStatusFilter: (filter: 'ALL' | AgentStatus) => void;
    onDisableAgent: (id: string) => Promise<AgentSummary>;
    onEnableAgent: (id: string) => Promise<AgentSummary>;
    onRevokeAgent: (id: string) => Promise<AgentSummary>;
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
    onRetryLoadAgents,
    onChangeActivityFilter,
    onChangeStatusFilter,
    onDisableAgent,
    onEnableAgent,
    onRevokeAgent,
    onOpenSkillGuide,
    onCloseSkillGuide,
    onLoadSkillMarkdown,
    onExportSkillMarkdown,
    onNavigateToSettings,
    onNotify,
  }: Props = $props();

  let confirmingRevokeId = $state<string | null>(null);
  let busyAgentId = $state<string | null>(null);
  let retryingAgents = $state(false);
  let directoryError = $state('');
  let loadingSkill = $state(false);
  let modalError = $state('');
  let syncedConnectionKey = $state<string | null>(null);
  let skillLoadSeq = 0;
  let agentOpSeq = 0;
  let retrySeq = 0;

  $effect(() => {
    const nextKey = `${serviceEpoch}::${serviceUrl}`;
    if (nextKey !== syncedConnectionKey) {
      syncedConnectionKey = nextKey;
      skillLoadSeq += 1;
      agentOpSeq += 1;
      retrySeq += 1;
      confirmingRevokeId = null;
      busyAgentId = null;
      retryingAgents = false;
      directoryError = '';
      modalError = '';
      loadingSkill = false;
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

  function isSameConnection(epoch: number, connKey: string | null): boolean {
    return epoch === serviceEpoch && connKey === syncedConnectionKey;
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

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape' && skillGuideOpen) {
      event.preventDefault();
      event.stopPropagation();
      onCloseSkillGuide();
    }
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

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

  {#if onRetryLoadAgents}
    <button
      type="button"
      class="secondary-action"
      data-action="refresh-agents"
      disabled={retryingAgents}
      onclick={() => void handleRetryLoadAgents()}
    >
      {$t('刷新')}
    </button>
  {/if}
</div>

{#if agentsLoadError}
  <div
    class="drawer-error-banner directory-load-error-banner"
    role="alert"
    data-status="agent-directory-load-error"
  >
    <span>{$t('Agent 名录加载失败：{error}', { error: agentsLoadError })}</span>
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

          <div class="agent-card-footer">
            {#if agent.status === 'revoked'}
              <span class="status-pill" data-agent-status-pill="revoked">
                {$t('已吊销')}
              </span>
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
