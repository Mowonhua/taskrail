<script lang="ts">
  import { untrack } from 'svelte';
  import type { InviteTokenSummary } from '../api/types';
  import type { SkillInstallAgent, SkillInstallRequest } from '../contracts';
  import { lightDismiss } from '../dialog-light-dismiss';
  import { t } from '../i18n';
  import { formatServiceError } from './helpers';

  interface Props {
    baseUrl: string;
    token: InviteTokenSummary | null;
    rawToken?: string;
    lanOpen: boolean;
    pickDirectory: () => Promise<string | null>;
    install: (request: SkillInstallRequest) => Promise<void>;
    onClose: () => void;
  }

  const methods = [
    { value: 'skill', title: 'Skill', detail: 'CLI' },
    { value: 'mcp-stdio', title: 'MCP', detail: 'STDIO' },
    { value: 'mcp-http', title: 'MCP', detail: 'Streamable HTTP (OAuth)' },
  ] as const;
  const agents = [
    { value: 'shared', label: '共享 Skills' },
    { value: 'codex', label: 'Codex' },
    { value: 'claude-code', label: 'Claude Code' },
    { value: 'cursor', label: 'Cursor' },
    { value: 'dsh', label: 'DeepSeek Harness' },
    { value: 'devin', label: 'Devin CLI' },
    { value: 'antigravity', label: 'Antigravity' },
    { value: 'grok-build', label: 'Grok Build' },
    { value: 'zcode', label: 'ZCode' },
  ] as const;
  const scopes = [
    { value: 'project', label: '当前项目' },
    { value: 'global', label: '全局' },
    { value: 'custom', label: '自定义 Skills 目录' },
  ] as const;

  let { baseUrl, token, rawToken, lanOpen, pickDirectory, install, onClose }: Props = $props();
  let dialog = $state<HTMLDialogElement>();
  let mode = $state<NonNullable<SkillInstallRequest['mode']>>('skill');
  let selectedAgents = $state<SkillInstallAgent[]>(['codex']);
  let scope = $state<SkillInstallRequest['scope']>('project');
  let serviceAddress = $state(untrack(() => baseUrl));
  let directory = $state('');
  let pastedToken = $state('');
  let agentName = $state('');
  let includeGuide = $state(false);
  let picking = $state(false);
  let installing = $state(false);
  let error = $state('');
  const availableAgents = $derived(agents.filter(option => supportsMethod(option.value, mode)));
  const availableScopes = $derived(scopes.filter(option => supportsScope(mode, option.value)));

  $effect(() => {
    if (!dialog) return;
    if (typeof dialog.showModal === 'function') dialog.showModal();
    else dialog.open = true;
  });

  function close(): void {
    if (!dialog?.isConnected) return;
    if (typeof dialog.close === 'function') dialog.close();
    else onClose();
  }

  function supportsMethod(target: SkillInstallAgent, method: typeof mode): boolean {
    return method === 'skill' || (target !== 'shared' && (target !== 'dsh' || method === 'mcp-stdio'));
  }

  function supportsScope(method: typeof mode, value: typeof scope): boolean {
    if (value === 'custom') return method === 'skill';
    return value === 'global' || selectedAgents.every(target =>
      target !== 'shared' && (target !== 'dsh' || method === 'skill'));
  }

  function normalizeScope(method: typeof mode): void {
    error = '';
    if (!supportsScope(method, scope)) {
      scope = supportsScope(method, 'project') ? 'project' : 'global';
      directory = '';
    }
  }

  function changeMethod(method: typeof mode): void {
    const compatible = selectedAgents.filter(target => supportsMethod(target, method));
    selectedAgents = compatible.length ? compatible : ['codex'];
    normalizeScope(method);
  }

  function toggleAgent(target: SkillInstallAgent, checked: boolean): void {
    selectedAgents = checked ? [...selectedAgents, target] : selectedAgents.filter(agent => agent !== target);
    normalizeScope(mode);
  }

  async function chooseDirectory(): Promise<void> {
    picking = true;
    error = '';
    try {
      directory = (await pickDirectory()) ?? directory;
    } catch (cause) {
      error = formatServiceError(cause);
    } finally {
      picking = false;
    }
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (picking || installing || (scope !== 'custom' && !selectedAgents.length)) return;
    error = '';
    const usesOAuth = mode === 'mcp-http';
    const inviteToken = !usesOAuth && token ? (rawToken ?? pastedToken.trim()) : '';
    if (!usesOAuth && token && (!inviteToken || !inviteToken.startsWith(token.token_prefix))) {
      error = $t('请输入所选邀请令牌的原始值。');
      return;
    }
    if (!usesOAuth && !inviteToken && !lanOpen) {
      error = $t('请先在列表中选中有效的邀请令牌。');
      return;
    }
    if (scope !== 'global' && !directory) {
      error = $t('请选择安装目录。');
      return;
    }
    installing = true;
    const [agent, ...additionalAgents] = scope === 'custom' ? ['codex'] as const : selectedAgents;
    try {
      await install({
        agent,
        ...(additionalAgents.length ? { additionalAgents } : {}),
        scope,
        ...(scope !== 'global' ? { directory } : {}),
        baseUrl: serviceAddress.trim(),
        ...(inviteToken ? { inviteToken } : {}),
        ...(mode !== 'skill' ? { mode, includeGuide } : {}),
        ...(mode === 'mcp-stdio' ? { agentName: agentName.trim() } : {}),
      });
      close();
    } catch (cause) {
      error = formatServiceError(cause);
    } finally {
      installing = false;
    }
  }
</script>

<dialog
  bind:this={dialog}
  use:lightDismiss
  class="modal-dialog skill-install-dialog"
  aria-labelledby="skill-install-title"
  closedby="any"
  data-dialog="skill-install"
  onclose={onClose}
>
  <header class="modal-header">
    <h2 id="skill-install-title" class="modal-title">{$t('安装')}</h2>
  </header>
  <form class="skill-install-form" onsubmit={submit}>
    <div class="skill-install-content">
      <fieldset class="skill-install-group" disabled={picking || installing}>
        <legend class="composer-label">{$t('安装方式')}</legend>
        <div class="skill-install-methods">
          {#each methods as method (method.value)}
            <label class="skill-install-method">
              <input type="radio" name="skill-install-method" value={method.value}
                bind:group={mode} onchange={() => changeMethod(method.value)} />
              <span class="skill-install-method-label">
                <strong>{method.title}</strong>
                <span>{method.detail}</span>
              </span>
            </label>
          {/each}
        </div>
      </fieldset>
      <div class="skill-install-target">
        {#if scope !== 'custom'}
          <fieldset class="skill-install-group" disabled={picking || installing}>
            <legend class="composer-label">{$t('安装到')}</legend>
            <div class="skill-install-choices skill-install-hosts">
              {#each availableAgents as option (option.value)}
                <label class="skill-install-choice">
                  <input type="checkbox" name="skill-install-agent" value={option.value}
                    checked={selectedAgents.includes(option.value)}
                    onchange={event => toggleAgent(option.value, event.currentTarget.checked)} />
                  <span>{$t(option.label)}</span>
                </label>
              {/each}
            </div>
          </fieldset>
        {/if}
        <fieldset class="skill-install-group" disabled={picking || installing}>
          <legend class="composer-label">{$t('安装范围')}</legend>
          <div class="skill-install-choices">
            {#each availableScopes as option (option.value)}
              <label class="skill-install-choice">
                <input type="radio" name="skill-install-scope" value={option.value}
                  bind:group={scope} onchange={() => { directory = ''; error = ''; }} />
                <span>{$t(option.label)}</span>
              </label>
            {/each}
          </div>
        </fieldset>
        {#if scope !== 'global'}
          <div class="project-form-field">
            <label class="composer-label" for="skill-install-directory">{$t(scope === 'project' ? '项目目录' : '安装目录')}</label>
            <div class="skill-install-directory">
              <input id="skill-install-directory" class="settings-text-input" value={directory} readonly />
              <button type="button" class="secondary-action" onclick={chooseDirectory} disabled={picking || installing}>
                {$t('选择目录')}
              </button>
            </div>
          </div>
        {/if}
      </div>
      <div class="skill-install-connection" class:has-agent-name={mode === 'mcp-stdio'}>
        <div class="project-form-field">
          <label class="composer-label" for="skill-install-url">{$t('服务地址')}</label>
          <input id="skill-install-url" type="url" class="settings-text-input"
            bind:value={serviceAddress} disabled={installing} required />
        </div>
        {#if mode === 'mcp-stdio'}
          <div class="project-form-field">
            <label class="composer-label" for="skill-install-agent-name">{$t('Agent 名称')}</label>
            <input id="skill-install-agent-name" class="settings-text-input" bind:value={agentName}
              required maxlength="64" disabled={installing} />
          </div>
        {/if}
      </div>
      {#if mode !== 'mcp-http' && token}
        <div class="project-form-field">
          {#if rawToken}
            <span class="composer-label">{$t('邀请令牌')}</span>
            <span class="skill-install-token-label">{token.label || token.token_prefix}</span>
          {:else}
            <label class="composer-label" for="skill-install-token">{$t('邀请令牌')} ({token.label || token.token_prefix})</label>
            <input id="skill-install-token" class="settings-text-input" type="password"
              bind:value={pastedToken} disabled={installing} autocomplete="off" />
          {/if}
        </div>
      {/if}
      {#if mode !== 'skill'}
        <label class="skill-install-choice">
          <input type="checkbox" name="skill-install-include-guide" bind:checked={includeGuide} disabled={installing} />
          <span>{$t('附带简短 Skill')}</span>
        </label>
      {/if}
      {#if error}
        <p class="dialog-error" role="alert">{error}</p>
      {/if}
    </div>
    <footer class="modal-actions skill-install-actions">
      <button type="button" class="secondary-action" onclick={close}>{$t('取消')}</button>
    <button type="submit" class="primary" data-action="confirm-install-skill"
      disabled={picking || installing || (scope !== 'custom' && !selectedAgents.length)}>
        {$t(installing ? '正在安装…' : '安装')}
      </button>
    </footer>
  </form>
</dialog>
