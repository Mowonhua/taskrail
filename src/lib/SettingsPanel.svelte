<script lang="ts">
  /**
   * 文件职责：承载工作台外观、阅读排版、自定义主题导入导出及工作台扩展分组预留设置。
   * 定义范围：界面语言、主题与明暗模式、桌面窗口失焦保持透明、字体栈与正文排版、主题管理及扩展预留。
   */
  import { composeFontStack, DEFAULT_FONT_STACK, GENERIC_FAMILIES, parseFontStack } from './font-stack';
  import FontSelect from './FontSelect.svelte';
  import { t, type LocalePreference } from './i18n';
  import { observeAnchoredPopover, positionAnchoredPopover } from './anchored-popover';
  import type { Preferences } from './contracts';
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
  }

  let {
    preferences,
    themes,
    activeTheme,
    customThemes,
    desktopMaterialSupported,
    importingTheme,
    themeError,
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
  }: Props = $props();

  let helpButton: HTMLButtonElement;
  let helpBubble: HTMLDivElement;
  let helpOpen = $state(false);
  let customFontDraft = $state('');

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
        bind:value={() => preferences.locale ?? preferences.language ?? 'zh-CN', value => onSelectLanguage(value as LocalePreference)}
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
    <div class="settings-row settings-extension-row">
      <div class="settings-extension-copy">
        <span class="settings-row-label">{$t('本地服务与连接')}</span>
        <span class="settings-extension-desc">{$t('管理本地服务端口与局域网访问监听')}</span>
      </div>
      <span class="settings-tag">{$t('即将推出')}</span>
    </div>
    <div class="settings-row settings-extension-row">
      <div class="settings-extension-copy">
        <span class="settings-row-label">{$t('项目管理与默认图标')}</span>
        <span class="settings-extension-desc">{$t('支持为悬赏所属项目配置 Emoji 或 Lucide 图标标识')}</span>
      </div>
      <span class="settings-tag">{$t('项目标识')}</span>
    </div>
    <div class="settings-row settings-extension-row">
      <div class="settings-extension-copy">
        <span class="settings-row-label">{$t('Agent 准入与协议')}</span>
        <span class="settings-extension-desc">{$t('提供接入说明文档与 Agent 令牌管理入口')}</span>
      </div>
      <span class="settings-tag">{$t('协作接入')}</span>
    </div>
  </section>
</div>
