<script lang="ts">
  /**
   * 文件职责：提供全局与项目级仲裁者顺位列表的可复用编辑器组件。
   * 定义范围：支持从已注册 Agent 列表选用或手工输入未注册符号 ID，进行追加、上移、下移与移除操作，并保留原值与顺序。
   */
  import type { AgentSummary } from '../api/types';
  import { locale, t } from '../i18n';
  import { formatAgentStatus, formatAgentTier } from './helpers';

  interface Props {
    chain: string[];
    agents?: AgentSummary[];
    agentsLoaded?: boolean;
    agentsLoadError?: string;
    scope: 'global' | 'project';
    emptyText: string;
    disabled?: boolean;
    onChange: (nextChain: string[]) => void;
  }

  let {
    chain,
    agents = [],
    agentsLoaded = true,
    agentsLoadError = '',
    scope,
    emptyText,
    disabled = false,
    onChange,
  }: Props = $props();

  let selectedAgentId = $state('');
  let manualIdInput = $state('');

  const agentsById = $derived(new Map(agents.map(agent => [agent.id, agent])));

  const availableAgents = $derived(
    agents.filter(agent => !chain.includes(agent.id)),
  );

  function addId(rawId: string): boolean {
    if (!rawId.trim()) return false;
    if (chain.includes(rawId)) return false;
    onChange([...chain, rawId]);
    return true;
  }

  function handleAddSelected(): void {
    if (disabled || !selectedAgentId) return;
    if (addId(selectedAgentId)) {
      selectedAgentId = '';
    }
  }

  function handleAddManual(): void {
    if (disabled) return;
    if (addId(manualIdInput)) {
      manualIdInput = '';
    }
  }

  function moveItem(index: number, delta: -1 | 1): void {
    if (disabled) return;
    const target = index + delta;
    if (target < 0 || target >= chain.length) return;
    const next = [...chain];
    const [moved] = next.splice(index, 1);
    next.splice(target, 0, moved);
    onChange(next);
  }

  function removeItem(index: number): void {
    if (disabled) return;
    onChange(chain.filter((_, idx) => idx !== index));
  }
</script>

<div class="arbitrator-chain-editor" data-chain-scope={scope}>
  {#if chain.length === 0}
    <p class="detail-empty-hint arbitrator-chain-empty" data-slot="arbitrator-chain-empty">
      {emptyText}
    </p>
  {:else}
    <ol class="arbitrator-chain-list" data-slot="arbitrator-chain-list">
      {#each chain as entryId, index (entryId)}
        {@const matchedAgent = agentsById.get(entryId)}
        {@const statusLabel = (() => {
          const _lang = $locale;
          return matchedAgent ? formatAgentStatus(matchedAgent.status) : null;
        })()}
        {@const tierLabel = (() => {
          const _lang = $locale;
          return matchedAgent ? formatAgentTier(matchedAgent.tier) : null;
        })()}
        <li
          class="arbitrator-chain-item"
          data-arbitrator-entry={entryId}
          data-arbitrator-index={index}
        >
          <div class="arbitrator-chain-identity">
            <span class="settings-tag arbitrator-order-tag">
              {$t('第 {order} 顺位', { order: index + 1 })}
            </span>
            {#if matchedAgent}
              <span class="arbitrator-entry-name">{matchedAgent.name}</span>
              <code class="agent-key-prefix">{matchedAgent.id}</code>
              {#if tierLabel}
                <span class="settings-tag">{tierLabel}</span>
              {/if}
              {#if matchedAgent.status !== 'active' && statusLabel}
                <span class="status-pill" data-agent-status-pill={matchedAgent.status}>
                  {statusLabel}
                </span>
              {/if}
            {:else if agentsLoadError}
              <code class="agent-key-prefix">{entryId}</code>
              <span class="settings-tag" data-arbitrator-identity="error">
                {$t('Agent 列表加载失败')}
              </span>
            {:else if !agentsLoaded}
              <code class="agent-key-prefix">{entryId}</code>
              <span class="settings-tag" data-arbitrator-identity="loading">
                {$t('正在加载 Agent 列表…')}
              </span>
            {:else}
              <code class="agent-key-prefix">{entryId}</code>
              <span class="settings-tag" data-arbitrator-identity="symbol">
                {$t('未注册符号 ID')}
              </span>
            {/if}
          </div>

          <div class="arbitrator-chain-controls">
            <button
              type="button"
              class="secondary-action"
              data-action="move-arbitrator-up"
              aria-label={$t('上移')}
              disabled={disabled || index === 0}
              onclick={() => moveItem(index, -1)}
            >
              ↑
            </button>
            <button
              type="button"
              class="secondary-action"
              data-action="move-arbitrator-down"
              aria-label={$t('下移')}
              disabled={disabled || index === chain.length - 1}
              onclick={() => moveItem(index, 1)}
            >
              ↓
            </button>
            <button
              type="button"
              class="secondary-action danger-text"
              data-action="remove-arbitrator"
              disabled={disabled}
              onclick={() => removeItem(index)}
            >
              {$t('移除')}
            </button>
          </div>
        </li>
      {/each}
    </ol>
  {/if}

  <div class="arbitrator-chain-adder">
    {#if agents.length > 0}
      <div class="arbitrator-add-row">
        <select
          class="composer-select"
          data-slot="arbitrator-agent-select"
          aria-label={$t('选择已注册 Agent…')}
          disabled={disabled || availableAgents.length === 0}
          bind:value={selectedAgentId}
        >
          <option value="">{$t('选择已注册 Agent…')}</option>
          {#each availableAgents as agent (agent.id)}
            <option value={agent.id}>
              {agent.name} ({agent.id})
            </option>
          {/each}
        </select>
        <button
          type="button"
          class="secondary-action"
          data-action="add-selected-arbitrator"
          disabled={disabled || !selectedAgentId}
          onclick={handleAddSelected}
        >
          {$t('添加选定 Agent')}
        </button>
      </div>
    {/if}

    <div class="arbitrator-add-row">
      <input
        type="text"
        class="settings-text-input"
        data-slot="arbitrator-id-input"
        aria-label={$t('或输入 Agent / 符号 ID')}
        placeholder={$t('或输入 Agent / 符号 ID')}
        disabled={disabled}
        bind:value={manualIdInput}
        onkeydown={event => {
          if (event.key === 'Enter') {
            event.preventDefault();
            handleAddManual();
          }
        }}
      />
      <button
        type="button"
        class="secondary-action"
        data-action="add-manual-arbitrator"
        disabled={disabled || !manualIdInput.trim()}
        onclick={handleAddManual}
      >
        {$t('添加 ID')}
      </button>
    </div>
  </div>
</div>
