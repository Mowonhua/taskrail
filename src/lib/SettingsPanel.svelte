<script lang="ts">
  /**
   * 文件职责：承载工作台外观、阅读排版、自定义主题导入导出、同构服务连接模式及项目管理设置。
   * 定义范围：界面语言、主题与明暗模式、桌面窗口失焦保持透明、字体栈与正文排版、主题管理、本地/远端服务连接与项目管理。
   */
  import { HttpBountyClient, normalizeServerUrl } from './api/bounty-client';
  import type {
    AgentSummary,
    ArbitrationConfig,
    BountyProject,
    CreatedInviteToken,
    CreateInviteTokenInput,
    CreateProjectInput,
    InviteTokenSummary,
    OnboardingSettings,
    ServiceMode,
    SkillDocument,
    UpdateProjectInput,
  } from './api/types';
  import AgentOnboardingPanel from './bounty/AgentOnboardingPanel.svelte';
  import ArbitratorChainEditor from './bounty/ArbitratorChainEditor.svelte';
  import { formatServiceError } from './bounty/helpers';
  import ProjectManagementPanel from './bounty/ProjectManagementPanel.svelte';
  import { composeFontStack, DEFAULT_FONT_STACK, GENERIC_FAMILIES, parseFontStack } from './font-stack';
  import FontSelect from './FontSelect.svelte';
  import { t, type LocalePreference } from './i18n';
  import { observeAnchoredPopover, positionAnchoredPopover } from './anchored-popover';
  import type { Preferences, SkillInstallRequest } from './contracts';
  import type { ThemeDefinition, ThemeMode } from './themes';
  import ThemeSelect from './ThemeSelect.svelte';

  interface Props {
    preferences: Preferences;
    themes: ThemeDefinition[];
    activeTheme: ThemeDefinition;
    customThemes: ThemeDefinition[];
    desktopMaterialSupported: boolean;
    importingTheme: boolean;
    themeError: string;
    serviceConnected?: boolean;
    serviceUrl?: string;
    serviceEpoch?: number;
    serviceConnectionError?: string;
    bountyProjects?: BountyProject[];
    agents?: AgentSummary[];
    agentsLoaded?: boolean;
    agentsLoadError?: string;
    showArchivedProjects?: boolean;
    onboardingSettings?: OnboardingSettings;
    onboardingLoaded?: boolean;
    onboardingLoadError?: string;
    arbitrationConfig?: ArbitrationConfig;
    arbitrationConfigLoaded?: boolean;
    arbitrationConfigLoadError?: string;
    skillDocument?: SkillDocument | null;
    desktop?: boolean;
    mobile?: boolean;
    onSelectLanguage: (language: LocalePreference) => void;
    onSelectThemeId: (id: string) => void;
    onSelectThemeMode: (mode: ThemeMode) => void;
    onToggleKeepTransparentOnBlur: (enabled: boolean) => void;
    onChangeFontFamily: (fontFamily: string) => void;
    onChangeFontSize: (fontSize: number) => void;
    onChangeLineWidth: (lineWidth: number) => void;
    onChangeParagraphIndent: (indent: number) => void;
    onImportThemeFile: (event: Event) => void;
    onExportThemeTemplate: () => void;
    onRemoveActiveTheme: () => void;
    onSaveServiceConnection?: (
      mode: ServiceMode,
      remoteServerUrl: string,
      remoteAdminToken: string,
    ) => Promise<void>;
    onToggleShowArchivedProjects?: (show: boolean) => Promise<void> | void;
    onCreateBountyProject?: (input: CreateProjectInput) => Promise<BountyProject>;
    onUpdateBountyProject?: (id: string, input: UpdateProjectInput) => Promise<BountyProject>;
    onDeleteBountyProject?: (id: string) => Promise<void>;
    onRetryLoadArbitrationConfig?: () => Promise<ArbitrationConfig | null | void> | void;
    onUpdateArbitrationConfig?: (chain: string[]) => Promise<ArbitrationConfig>;
    onRetryLoadOnboarding?: () => Promise<OnboardingSettings | null | void> | void;
    onToggleLanRegistration?: (enabled: boolean) => Promise<OnboardingSettings>;
    onCreateInviteToken?: (input?: CreateInviteTokenInput) => Promise<CreatedInviteToken>;
    onRevokeInviteToken?: (id: string) => Promise<InviteTokenSummary>;
    onDeleteInviteToken?: (id: string) => Promise<void>;
    onLoadSkillMarkdown?: (options?: {
      baseUrl?: string;
      inviteToken?: string;
    }) => Promise<SkillDocument>;
    onPickSkillDirectory?: () => Promise<string | null>;
    onInstallSkill?: (request: SkillInstallRequest) => Promise<string[]>;
    onNotify?: (message: string) => void;
  }

  let {
    preferences,
    themes,
    activeTheme,
    customThemes,
    desktopMaterialSupported,
    importingTheme,
    themeError,
    serviceConnected = true,
    serviceUrl = '',
    serviceEpoch = 0,
    serviceConnectionError = '',
    bountyProjects = [],
    agents = [],
    agentsLoaded = true,
    agentsLoadError = '',
    showArchivedProjects = false,
    onboardingSettings = { lan_open_registration: false, invite_tokens: [] },
    onboardingLoaded = true,
    onboardingLoadError = '',
    arbitrationConfig = { arbitrator_chain: [] },
    arbitrationConfigLoaded = true,
    arbitrationConfigLoadError = '',
    skillDocument = null,
    desktop = false,
    mobile = false,
    onSelectLanguage,
    onSelectThemeId,
    onSelectThemeMode,
    onToggleKeepTransparentOnBlur,
    onChangeFontFamily,
    onChangeFontSize,
    onChangeLineWidth,
    onChangeParagraphIndent,
    onImportThemeFile,
    onExportThemeTemplate,
    onRemoveActiveTheme,
    onSaveServiceConnection,
    onToggleShowArchivedProjects = () => {},
    onCreateBountyProject = async () => {
      throw new Error('Not configured');
    },
    onUpdateBountyProject = async () => {
      throw new Error('Not configured');
    },
    onDeleteBountyProject = async () => {
      throw new Error('Not configured');
    },
    onRetryLoadArbitrationConfig,
    onUpdateArbitrationConfig = async () => {
      throw new Error('Not configured');
    },
    onRetryLoadOnboarding,
    onToggleLanRegistration = async () => {
      throw new Error('Not configured');
    },
    onCreateInviteToken = async () => {
      throw new Error('Not configured');
    },
    onRevokeInviteToken = async () => {
      throw new Error('Not configured');
    },
    onDeleteInviteToken = async () => {
      throw new Error('Not configured');
    },
    onLoadSkillMarkdown = async () => {
      throw new Error('Not configured');
    },
    onPickSkillDirectory,
    onInstallSkill,
    onNotify,
  }: Props = $props();

  let helpButton: HTMLButtonElement;
  let helpBubble: HTMLDivElement;
  let helpOpen = $state(false);
  let customFontDraft = $state('');

  let serviceModeDraft = $state<ServiceMode>('embedded');
  let remoteUrlDraft = $state('');
  let remoteTokenDraft = $state('');
  let testingConnection = $state(false);
  let savingConnection = $state(false);
  let connectionFeedback = $state<{ kind: 'success' | 'error'; message: string } | null>(null);

  let arbitrationChainDraft = $state<string[]>([]);
  let savingArbitrationChain = $state(false);
  let arbitrationFeedback = $state<{ kind: 'success' | 'error'; message: string } | null>(null);
  let lastSyncedArbitrationKey = '';
  let arbitrationHelpButton = $state<HTMLButtonElement | undefined>(undefined);
  let arbitrationHelpBubble = $state<HTMLDivElement | undefined>(undefined);
  let arbitrationHelpOpen = $state(false);

  function closeArbitrationHelp(): void {
    arbitrationHelpOpen = false;
    if (
      typeof arbitrationHelpBubble?.hidePopover === 'function' &&
      arbitrationHelpBubble.matches(':popover-open')
    ) {
      arbitrationHelpBubble.hidePopover();
    }
  }

  function toggleArbitrationHelp(): void {
    if (!arbitrationHelpButton || !arbitrationHelpBubble) return;
    arbitrationHelpOpen = !arbitrationHelpOpen;
    if (!arbitrationHelpOpen) {
      closeArbitrationHelp();
      return;
    }
    if (
      typeof arbitrationHelpBubble.showPopover === 'function' &&
      !arbitrationHelpBubble.matches(':popover-open')
    ) {
      arbitrationHelpBubble.showPopover();
    }
    positionAnchoredPopover(arbitrationHelpButton, arbitrationHelpBubble, closeArbitrationHelp);
  }

  $effect(() => {
    if (!arbitrationHelpOpen || !arbitrationHelpButton || !arbitrationHelpBubble) return;
    const stopObserve = observeAnchoredPopover(
      arbitrationHelpButton,
      arbitrationHelpBubble,
      closeArbitrationHelp,
    );
    const onKeydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        closeArbitrationHelp();
      }
    };
    document.addEventListener('keydown', onKeydown, true);
    return () => {
      document.removeEventListener('keydown', onKeydown, true);
      stopObserve();
    };
  });

  $effect(() => {
    const incomingChain = arbitrationConfig?.arbitrator_chain ?? [];
    const syncKey = `${serviceEpoch}:${JSON.stringify(incomingChain)}`;
    if (syncKey !== lastSyncedArbitrationKey) {
      lastSyncedArbitrationKey = syncKey;
      arbitrationChainDraft = [...incomingChain];
      arbitrationFeedback = null;
    }
  });

  async function handleSaveGlobalArbitrationChain(): Promise<void> {
    if (savingArbitrationChain) return;
    arbitrationFeedback = null;
    if (!arbitrationConfigLoaded) {
      arbitrationFeedback = {
        kind: 'error',
        message: $t('全局仲裁配置尚未加载完成，无法保存。'),
      };
      return;
    }
    savingArbitrationChain = true;
    try {
      const updated = await onUpdateArbitrationConfig(arbitrationChainDraft);
      const nextChain = [...(updated?.arbitrator_chain ?? arbitrationChainDraft)];
      lastSyncedArbitrationKey = `${serviceEpoch}:${JSON.stringify(nextChain)}`;
      arbitrationChainDraft = nextChain;
      const msg = $t('已保存全局仲裁顺位');
      arbitrationFeedback = { kind: 'success', message: msg };
      onNotify?.(msg);
    } catch (error) {
      arbitrationFeedback = { kind: 'error', message: formatServiceError(error) };
    } finally {
      savingArbitrationChain = false;
    }
  }

  $effect(() => {
    serviceModeDraft = preferences.serviceMode ?? 'embedded';
    remoteUrlDraft = preferences.remoteServerUrl ?? '';
    remoteTokenDraft = preferences.remoteAdminToken ?? '';
  });

  function themeSupportsBlur(theme: ThemeDefinition): boolean {
    if (theme.monochrome) return false;
    const lightMaterial = theme.appearance?.light['window-material'];
    const darkMaterial = theme.appearance?.dark['window-material'];
    return (
      lightMaterial === 'acrylic' ||
      lightMaterial === 'blur' ||
      lightMaterial === 'transparent' ||
      darkMaterial === 'acrylic' ||
      darkMaterial === 'blur' ||
      darkMaterial === 'transparent'
    );
  }

  const fontSlots = $derived(parseFontStack(preferences.fontFamily));
  const genericFontOptions = $derived([
    { value: '', label: $t('无') },
    ...GENERIC_FAMILIES.map(family => ({ value: family, label: family })),
  ]);

  const hasCustomFontSetting = $derived(preferences.fontFamily !== DEFAULT_FONT_STACK);
  const supportsBlurTransparency = $derived(desktopMaterialSupported && themeSupportsBlur(activeTheme));
  const keepTransparentChecked = $derived(
    supportsBlurTransparency && Boolean(preferences.keepTransparentOnBlur),
  );
  const isCustomActiveTheme = $derived(customThemes.some(item => item.id === activeTheme.id));

  const languageOptions = $derived([
    { value: 'zh-CN', label: '简体中文' },
    { value: 'en', label: 'English' },
    { value: 'system', label: $t('跟随系统') },
  ]);

  const themeOptions = $derived(
    themes.map(theme => ({
      value: theme.id,
      label: customThemes.some(item => item.id === theme.id)
        ? $t('{name}（自制）', { name: theme.name })
        : $t(theme.name),
    })),
  );

  const themeModeOptions = $derived([
    { value: 'system', label: $t('跟随系统') },
    { value: 'light', label: $t('浅色') },
    { value: 'dark', label: $t('深色') },
  ]);

  function updateFontSlot(slot: 'primary' | 'fallback' | 'generic', value: string | null): void {
    const named = [...fontSlots.named];
    let generic = fontSlots.generic;
    if (slot === 'primary') {
      if (value) named[0] = value;
      else named.shift();
    } else if (slot === 'fallback') {
      if (value) {
        if (named.length === 0) named.push(value);
        else named[1] = value;
      } else if (named.length > 1) {
        named.splice(1, 1);
      }
    } else {
      generic = value || null;
    }
    const next = composeFontStack(named, generic) || DEFAULT_FONT_STACK;
    onChangeFontFamily(next);
  }

  function closeHelp() {
    helpOpen = false;
    if (typeof helpBubble?.hidePopover === 'function' && helpBubble.matches(':popover-open')) {
      helpBubble.hidePopover();
    }
  }

  function toggleHelp() {
    helpOpen = !helpOpen;
    if (!helpOpen) {
      closeHelp();
      return;
    }
    if (typeof helpBubble?.showPopover === 'function' && !helpBubble.matches(':popover-open')) {
      helpBubble.showPopover();
    }
    positionAnchoredPopover(helpButton, helpBubble, closeHelp);
  }

  $effect(() => {
    customFontDraft = preferences.fontFamily;
  });

  function commitCustomFontDraft() {
    const trimmed = customFontDraft.trim();
    if (!trimmed || /[;{}<>]|url\s*\(/i.test(trimmed)) {
      customFontDraft = preferences.fontFamily;
      return;
    }
    onChangeFontFamily(trimmed);
  }

  $effect(() => {
    if (!helpOpen) return;
    return observeAnchoredPopover(helpButton, helpBubble, closeHelp);
  });

  async function handleSelectServiceMode(nextMode: ServiceMode): Promise<void> {
    serviceModeDraft = nextMode;
    connectionFeedback = null;
    if (nextMode === 'embedded' && (preferences.serviceMode ?? 'embedded') !== 'embedded') {
      savingConnection = true;
      try {
        await onSaveServiceConnection?.('embedded', remoteUrlDraft.trim(), remoteTokenDraft.trim());
      } catch (error) {
        serviceModeDraft = preferences.serviceMode ?? 'embedded';
        connectionFeedback = { kind: 'error', message: formatServiceError(error) };
      } finally {
        savingConnection = false;
      }
    }
  }

  async function handleTestRemoteConnection(): Promise<void> {
    if (testingConnection) return;
    connectionFeedback = null;
    let normalizedUrl: string;
    try {
      normalizedUrl = normalizeServerUrl(remoteUrlDraft);
    } catch {
      connectionFeedback = {
        kind: 'error',
        message: $t('请输入合法的 http:// 或 https:// 远端服务地址。'),
      };
      return;
    }
    testingConnection = true;
    try {
      const client = new HttpBountyClient(normalizedUrl, remoteTokenDraft.trim());
      const [health, status] = await Promise.all([client.health(), client.adminStatus()]);
      connectionFeedback = {
        kind: 'success',
        message: $t('连接成功：{mode} (v{version})', {
          mode: status.mode || health.mode,
          version: status.version || health.version,
        }),
      };
    } catch (error) {
      connectionFeedback = {
        kind: 'error',
        message: formatServiceError(error),
      };
    } finally {
      testingConnection = false;
    }
  }

  async function handleSaveAndReconnect(): Promise<void> {
    if (savingConnection) return;
    connectionFeedback = null;
    if (serviceModeDraft === 'remote') {
      let normalizedUrl: string;
      try {
        normalizedUrl = normalizeServerUrl(remoteUrlDraft);
      } catch {
        connectionFeedback = {
          kind: 'error',
          message: $t('请输入合法的 http:// 或 https:// 远端服务地址。'),
        };
        return;
      }
      savingConnection = true;
      try {
        await onSaveServiceConnection?.('remote', normalizedUrl, remoteTokenDraft.trim());
        connectionFeedback = {
          kind: 'success',
          message: $t('已保存并连接到远端服务。'),
        };
      } catch (error) {
        serviceModeDraft = preferences.serviceMode ?? 'embedded';
        connectionFeedback = {
          kind: 'error',
          message: formatServiceError(error),
        };
      } finally {
        savingConnection = false;
      }
    } else {
      savingConnection = true;
      try {
        await onSaveServiceConnection?.('embedded', remoteUrlDraft.trim(), remoteTokenDraft.trim());
        connectionFeedback = {
          kind: 'success',
          message: $t('已切换回个人内嵌模式。'),
        };
      } catch (error) {
        serviceModeDraft = preferences.serviceMode ?? 'embedded';
        connectionFeedback = {
          kind: 'error',
          message: formatServiceError(error),
        };
      } finally {
        savingConnection = false;
      }
    }
  }
</script>

<div class="settings-panel">
  <section class="settings-section" aria-labelledby="settings-interface-heading">
    <h3 class="settings-section-title" id="settings-interface-heading">{$t('界面')}</h3>

    <div class="settings-row">
      <ThemeSelect
        id="language-select"
        inline
        label={$t('语言')}
        options={languageOptions}
        bind:value={() => preferences.locale, value => onSelectLanguage(value as LocalePreference)}
      />
    </div>

    <div class="settings-row">
      <ThemeSelect
        id="theme-id-select"
        inline
        label={$t('主题')}
        options={themeOptions}
        bind:value={() => activeTheme.id, value => onSelectThemeId(value)}
      />
    </div>

    <div class="settings-row">
      <ThemeSelect
        id="theme-mode-select"
        inline
        label={$t('明暗模式')}
        options={themeModeOptions}
        bind:value={() => preferences.theme, value => onSelectThemeMode(value as ThemeMode)}
      />
    </div>

    <div class="settings-row" class:settings-row-disabled={!supportsBlurTransparency}>
      <div class="settings-label-group">
        <label class="settings-row-label" for="keep-transparent-switch">{$t('窗口失焦时保持透明')}</label>
        {#if !desktopMaterialSupported}
          <span class="settings-tag">{$t('仅桌面版')}</span>
        {:else if !themeSupportsBlur(activeTheme)}
          <span class="settings-tag">{$t('仅玻璃主题')}</span>
        {/if}
        <span class="settings-help">
          <button
            bind:this={helpButton}
            type="button"
            class="settings-help-trigger"
            aria-label={$t('关于窗口透明')}
            aria-expanded={helpOpen}
            aria-controls="window-transparent-help"
            onclick={toggleHelp}
            onkeydown={event => {
              if (event.key === 'Escape' && helpOpen) {
                event.preventDefault();
                event.stopPropagation();
                closeHelp();
              }
            }}
          >
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <circle cx="8" cy="8" r="6.25" />
              <path d="M6.4 6.2a1.7 1.7 0 0 1 3.2.7c0 1.1-1.6 1.4-1.6 2.5" />
              <circle cx="8" cy="11.8" r=".75" fill="currentColor" stroke="none" />
            </svg>
          </button>
          <div
            bind:this={helpBubble}
            id="window-transparent-help"
            class="settings-help-popover dropdown"
            role="note"
            popover="manual"
            hidden={!helpOpen}
            style="position: fixed; margin: 0; bottom: auto; right: auto; box-sizing: border-box;"
          >
            <p>{$t('开启后，窗口失去焦点仍保持透明模糊；关闭时使用系统默认效果。')}</p>
            {#if !desktopMaterialSupported}
              <p>{$t('浏览器预览不支持原生窗口透明，请在 Windows 桌面版使用。')}</p>
            {:else if !themeSupportsBlur(activeTheme)}
              <p>{$t('请选择支持玻璃窗口效果的主题，例如液态玻璃或磨砂玻璃。')}</p>
            {/if}
          </div>
        </span>
      </div>
      <span class="settings-switch">
        <input
          id="keep-transparent-switch"
          type="checkbox"
          role="switch"
          aria-label={$t('窗口失焦时保持透明')}
          checked={keepTransparentChecked}
          disabled={!supportsBlurTransparency}
          onchange={event => onToggleKeepTransparentOnBlur(event.currentTarget.checked)}
        />
        <span class="settings-switch-track" aria-hidden="true">
          <span class="settings-switch-thumb"></span>
        </span>
      </span>
    </div>
  </section>

  <section class="settings-section" aria-labelledby="settings-document-heading">
    <h3 class="settings-section-title" id="settings-document-heading">{$t('正文')}</h3>

    <div class="settings-row">
      <FontSelect
        id="font-primary-select"
        inline
        allowEmpty
        label={$t('正文字体')}
        bind:value={() => fontSlots.named[0] ?? null, value => updateFontSlot('primary', value)}
      />
    </div>

    <div class="settings-row">
      <FontSelect
        id="font-fallback-select"
        inline
        allowEmpty
        label={$t('备用字体')}
        bind:value={() => fontSlots.named[1] ?? null, value => updateFontSlot('fallback', value)}
      />
    </div>

    <div class="settings-row">
      <ThemeSelect
        id="font-generic-select"
        inline
        label={$t('兜底字体')}
        options={genericFontOptions}
        bind:value={() => fontSlots.generic ?? '', value => updateFontSlot('generic', value || null)}
      />
    </div>

    <div class="settings-row">
      <label class="settings-row-label" for="font-custom-input">{$t('手动输入字体')}</label>
      <div class="settings-custom-font">
        <input
          id="font-custom-input"
          class="settings-text-input"
          type="text"
          aria-label={$t('手动输入字体')}
          placeholder={DEFAULT_FONT_STACK}
          bind:value={customFontDraft}
          onchange={commitCustomFontDraft}
          onkeydown={event => {
            if (event.key === 'Enter') {
              event.preventDefault();
              commitCustomFontDraft();
            }
          }}
        />
        {#if hasCustomFontSetting}
          <button
            type="button"
            class="settings-text-action"
            onclick={() => onChangeFontFamily(DEFAULT_FONT_STACK)}
          >
            {$t('恢复默认')}
          </button>
        {/if}
      </div>
    </div>

    <div class="settings-row">
      <label class="settings-row-label" for="font-size-range">{$t('字号')}</label>
      <div class="settings-slider-group">
        <input
          id="font-size-range"
          type="range"
          min="14"
          max="20"
          aria-label={$t('字号')}
          value={preferences.fontSize}
          oninput={event => onChangeFontSize(Number((event.currentTarget as HTMLInputElement).value))}
        />
        <output for="font-size-range">{preferences.fontSize}px</output>
      </div>
    </div>

    <div class="settings-row">
      <label class="settings-row-label" for="line-width-range">{$t('正文宽度')}</label>
      <div class="settings-slider-group">
        <input
          id="line-width-range"
          type="range"
          min="600"
          max="900"
          step="20"
          aria-label={$t('正文宽度')}
          value={preferences.contentWidth}
          oninput={event => onChangeLineWidth(Number((event.currentTarget as HTMLInputElement).value))}
        />
        <output for="line-width-range">{preferences.contentWidth}px</output>
      </div>
    </div>

    <div class="settings-row">
      <label class="settings-row-label" for="settings-paragraph-indent">{$t('段首缩进')}</label>
      <div class="settings-slider-group">
        <input
          id="settings-paragraph-indent"
          type="range"
          min="0"
          max="4"
          step="1"
          aria-label={$t('段首缩进')}
          value={preferences.paragraphIndent}
          oninput={event => onChangeParagraphIndent(Number((event.currentTarget as HTMLInputElement).value))}
        />
        <output for="settings-paragraph-indent">
          {preferences.paragraphIndent > 0 ? `${preferences.paragraphIndent} ${$t('字符')}` : $t('关闭')}
        </output>
      </div>
    </div>
  </section>

  <details class="settings-advanced" open={Boolean(themeError) || isCustomActiveTheme}>
    <summary class="settings-advanced-summary">
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="m6 4 4 4-4 4" />
      </svg>
      <span>{$t('主题管理')}</span>
    </summary>
    <div class="settings-advanced-body">
      <div class="settings-row">
        <span class="settings-row-label">{$t('导入与导出')}</span>
        <div class="settings-inline-actions">
          <label class="button-like" class:disabled={importingTheme}>
            <input
              type="file"
              accept=".json,application/json"
              aria-label={$t('导入主题文件')}
              disabled={importingTheme}
              onchange={onImportThemeFile}
            />
            {$t(importingTheme ? '正在导入…' : '导入主题')}
          </label>
          <button type="button" onclick={onExportThemeTemplate}>{$t('下载主题模板')}</button>
          {#if isCustomActiveTheme}
            <button type="button" class="danger-text" onclick={onRemoveActiveTheme}>{$t('移除主题')}</button>
          {/if}
        </div>
      </div>
      {#if themeError}
        <p class="dialog-error settings-error" role="alert">{themeError}</p>
      {/if}
    </div>
  </details>

  <section
    class="settings-section settings-extension-section"
    aria-labelledby="settings-extensions-heading"
    data-extension-group="workspace"
  >
    <h3 class="settings-section-title" id="settings-extensions-heading">{$t('工作台与服务扩展')}</h3>

    <div class="service-connection-card" data-section="service-connection">
      <div class="service-connection-header">
        <div class="settings-extension-copy">
          <span class="settings-row-label">{$t('本地服务与连接')}</span>
          <span class="settings-extension-desc">
            {$t('切换个人内嵌服务或接入团队远端独立守护进程')}
          </span>
        </div>

        <div class="segmented-group" role="group" aria-label={$t('本地服务与连接')}>
          <button
            type="button"
            class="segmented-button"
            class:active={serviceModeDraft === 'embedded'}
            aria-pressed={serviceModeDraft === 'embedded'}
            data-service-mode="embedded"
            onclick={() => void handleSelectServiceMode('embedded')}
          >
            {$t('个人内嵌模式')}
          </button>
          <button
            type="button"
            class="segmented-button"
            class:active={serviceModeDraft === 'remote'}
            aria-pressed={serviceModeDraft === 'remote'}
            data-service-mode="remote"
            onclick={() => void handleSelectServiceMode('remote')}
          >
            {$t('团队远端模式')}
          </button>
        </div>
      </div>

      {#if serviceModeDraft === 'embedded'}
        <div class="service-status-row" data-slot="embedded-service-status">
          <div class="service-status-meta">
            <span
              class="status-pill"
              data-status={serviceConnected ? 'COMPLETED' : 'CANCELLED'}
            >
              {serviceConnected ? $t('已连接') : $t('未连接')}
            </span>
            <span class="service-url-text">
              {serviceUrl || $t('本地内嵌服务内核')}
            </span>
          </div>
          {#if !serviceConnected}
            <button
              type="button"
              class="secondary-action"
              data-action="reconnect-embedded"
              disabled={savingConnection}
              onclick={() => void handleSaveAndReconnect()}
            >
              {$t('重新连接')}
            </button>
          {/if}
        </div>
        {#if serviceConnectionError && !connectionFeedback}
          <p class="dialog-error settings-error" role="alert">{serviceConnectionError}</p>
        {/if}
      {:else}
        <div class="remote-service-form" data-slot="remote-service-form">
          <div class="project-form-field">
            <label class="composer-label" for="remote-server-url-input">{$t('远端服务地址')}</label>
            <input
              id="remote-server-url-input"
              type="url"
              class="settings-text-input"
              aria-label={$t('远端服务地址')}
              placeholder="http://127.0.0.1:8420"
              bind:value={remoteUrlDraft}
            />
          </div>

          <div class="project-form-field">
            <label class="composer-label" for="remote-admin-token-input">{$t('管理员凭证')}</label>
            <input
              id="remote-admin-token-input"
              type="password"
              class="settings-text-input"
              aria-label={$t('管理员凭证')}
              placeholder={$t('输入服务端管理员 Bearer Token')}
              bind:value={remoteTokenDraft}
            />
          </div>

          <div class="inline-project-actions">
            <button
              type="button"
              class="secondary-action"
              data-action="test-connection"
              disabled={testingConnection || savingConnection}
              onclick={() => void handleTestRemoteConnection()}
            >
              {$t(testingConnection ? '正在测试…' : '测试连接')}
            </button>
            <button
              type="button"
              class="primary secondary-action"
              data-action="save-reconnect"
              disabled={testingConnection || savingConnection}
              onclick={() => void handleSaveAndReconnect()}
            >
              {$t(savingConnection ? '正在连接…' : '保存并重连')}
            </button>
          </div>
        </div>
      {/if}

      {#if connectionFeedback}
        <p
          class={connectionFeedback.kind === 'error' ? 'dialog-error settings-error' : 'service-feedback-success'}
          role={connectionFeedback.kind === 'error' ? 'alert' : 'status'}
          data-status="connection-feedback"
        >
          {connectionFeedback.message}
        </p>
      {/if}
    </div>

    <ProjectManagementPanel
      projects={bountyProjects}
      {agents}
      {agentsLoaded}
      {agentsLoadError}
      showArchived={showArchivedProjects}
      {desktop}
      {mobile}
      onToggleShowArchived={onToggleShowArchivedProjects}
      onCreateProject={onCreateBountyProject}
      onUpdateProject={onUpdateBountyProject}
      onDeleteProject={onDeleteBountyProject}
    />

    <div class="service-connection-card" data-section="arbitration-config">
      <div class="service-connection-header">
        <div class="settings-label-group">
          <span class="settings-row-label">{$t('仲裁者顺位配置')}</span>
          <span class="settings-help">
            <button
              bind:this={arbitrationHelpButton}
              type="button"
              class="settings-help-trigger"
              data-action="toggle-arbitration-help"
              aria-label={$t('关于仲裁者顺位')}
              aria-expanded={arbitrationHelpOpen}
              aria-controls="global-arbitration-help"
              onclick={toggleArbitrationHelp}
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
              bind:this={arbitrationHelpBubble}
              id="global-arbitration-help"
              class="settings-help-popover dropdown"
              role="note"
              popover="manual"
              hidden={!arbitrationHelpOpen}
              style="position: fixed; margin: 0; bottom: auto; right: auto; box-sizing: border-box;"
            >
              <p>
                {$t('首位处于 active 且非执行者的候选人将接手 15 分钟仲裁窗，超时后顺延至下一顺位；无人可用时转交人工仲裁兜底。')}
              </p>
              <p>
                {$t('全局链允许预填尚未注册的符号 ID；项目配置非空链时整体覆盖全局链，留空则沿用全局链。')}
              </p>
            </div>
          </span>
        </div>

        <button
          type="button"
          class="primary secondary-action"
          data-action="save-global-arbitration-chain"
          disabled={savingArbitrationChain || !arbitrationConfigLoaded}
          onclick={() => void handleSaveGlobalArbitrationChain()}
        >
          {savingArbitrationChain ? $t('正在保存…') : $t('保存全局顺位')}
        </button>
      </div>

      {#if arbitrationConfigLoadError}
        <div
          class="drawer-error-banner onboarding-load-error-banner"
          role="alert"
          data-status="arbitration-load-error"
        >
          <span>{$t('加载仲裁配置失败：{error}', { error: arbitrationConfigLoadError })}</span>
          {#if onRetryLoadArbitrationConfig}
            <button
              type="button"
              class="secondary-action"
              data-action="retry-load-arbitration"
              onclick={() => void onRetryLoadArbitrationConfig()}
            >
              {$t('重试加载')}
            </button>
          {/if}
        </div>
      {/if}

      <ArbitratorChainEditor
        scope="global"
        chain={arbitrationChainDraft}
        {agents}
        {agentsLoaded}
        {agentsLoadError}
        disabled={savingArbitrationChain || !arbitrationConfigLoaded}
        emptyText={$t('未配置顺位（直接由人工仲裁兜底）')}
        onChange={next => {
          arbitrationChainDraft = next;
          arbitrationFeedback = null;
        }}
      />

      {#if arbitrationFeedback}
        <p
          class={arbitrationFeedback.kind === 'error' ? 'dialog-error settings-error' : 'service-feedback-success'}
          role={arbitrationFeedback.kind === 'error' ? 'alert' : 'status'}
          data-status="arbitration-feedback"
        >
          {arbitrationFeedback.message}
        </p>
      {/if}
    </div>

    <AgentOnboardingPanel
      onboarding={onboardingSettings}
      {onboardingLoaded}
      {onboardingLoadError}
      {serviceUrl}
      {serviceEpoch}
      {skillDocument}
      {onRetryLoadOnboarding}
      {onToggleLanRegistration}
      {onCreateInviteToken}
      {onRevokeInviteToken}
      {onDeleteInviteToken}
      {onLoadSkillMarkdown}
      {onPickSkillDirectory}
      {onInstallSkill}
      {onNotify}
    />
  </section>
</div>

