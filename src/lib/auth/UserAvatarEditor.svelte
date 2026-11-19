<script lang="ts">
  /**
   * 文件职责：渲染并编辑用户头像（头像点击上传与相机角标、莫兰迪预制底色圆点）。
   * 约束条件：由注册弹窗与个人设置弹窗共用，选择预制底色可覆盖已上传头像，再次点击当前底色可清空。
   */
  import type { AuthAvatarPreset, UserAvatar as UserAvatarValue } from '../api/types';
  import { formatAuthError } from '../bounty/helpers';
  import { t } from '../i18n';
  import UserAvatar from './UserAvatar.svelte';
  import { DEFAULT_AUTH_AVATAR_PRESETS, fileToUploadAvatar } from './avatars';

  interface Props {
    name: string;
    avatar: UserAvatarValue | null;
    presets: AuthAvatarPreset[];
    presetsError?: string;
    disabled?: boolean;
    errorContext: 'register' | 'profile';
    editorSlot?: string;
    previewSlot?: string;
    uploadInputSlot?: string;
    uploadAction?: string;
    warningSlot?: string;
    onChange: (next: UserAvatarValue | null) => void;
    onError: (message: string) => void;
  }

  let {
    name,
    avatar,
    presets,
    presetsError = '',
    disabled = false,
    errorContext,
    editorSlot = 'profile-avatar-editor',
    previewSlot = 'profile-preview',
    uploadInputSlot = 'avatar-upload-input',
    uploadAction = 'choose-avatar-file',
    warningSlot = 'profile-presets-warning',
    onChange,
    onError,
  }: Props = $props();

  let fileInput = $state<HTMLInputElement>();

  const resolvedPresets = $derived(
    presets.length > 0 ? presets : [...DEFAULT_AUTH_AVATAR_PRESETS],
  );

  async function handleFileChange(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file || disabled) return;
    onError('');
    try {
      onChange(await fileToUploadAvatar(file));
    } catch (cause) {
      onError(formatAuthError(cause, errorContext));
    }
  }
</script>

<div class="avatar-picker-block" data-slot={editorSlot}>
  <input
    bind:this={fileInput}
    type="file"
    class="visually-hidden"
    accept="image/png,image/jpeg,image/webp"
    data-slot={uploadInputSlot}
    onchange={handleFileChange}
  />

  <div class="avatar-picker-trigger-wrap">
    <button
      type="button"
      class="profile-avatar-trigger"
      data-slot={previewSlot}
      data-action={uploadAction}
      aria-label={$t('上传头像')}
      title={$t('上传头像')}
      {disabled}
      onclick={() => fileInput?.click()}
    >
      <UserAvatar {name} {avatar} size="lg" />
      <span class="profile-avatar-overlay" aria-hidden="true">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z" />
          <circle cx="12" cy="13" r="3" />
        </svg>
      </span>
      <span class="avatar-camera-badge" aria-hidden="true">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
          <path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z" />
          <circle cx="12" cy="13" r="3" />
        </svg>
      </span>
    </button>
  </div>

  <div
    class="profile-color-dots"
    role="group"
    aria-label={$t('系统预制头像')}
    data-slot="avatar-preset-grid"
  >
    {#each resolvedPresets as preset (preset.id)}
      {@const active = avatar?.kind === 'preset' && avatar.id === preset.id}
      <button
        type="button"
        class="profile-color-dot"
        class:active
        aria-pressed={active}
        aria-label={$t(preset.label)}
        title={$t(preset.label)}
        data-avatar-preset={preset.id}
        data-avatar-preset-option={preset.id}
        {disabled}
        onclick={() => {
          onChange(active ? null : { kind: 'preset', id: preset.id });
          onError('');
        }}
      >
        {#if active}
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M3.5 8.2 6.5 11.2 12.5 5" />
          </svg>
        {/if}
      </button>
    {/each}
  </div>
</div>

{#if presetsError}
  <p class="composer-hint" data-slot={warningSlot}>
    {$t('未能同步最新预设列表，当前使用内置预设：{error}', { error: presetsError })}
  </p>
{/if}
