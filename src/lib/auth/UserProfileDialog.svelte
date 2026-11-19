<script lang="ts">
  /**
   * 文件职责：承载已登录用户的个人设置弹窗，支持更新显示名称与头像（系统预制、上传或清除）。
   * 约束条件：沿用当前服务连接与登录 Bearer 凭证，不修改工作台管理员权限。
   */
  import { untrack } from 'svelte';
  import type {
    AuthAvatarPreset,
    UpdateUserProfileInput,
    UserAvatar as UserAvatarValue,
    UserProfile,
  } from '../api/types';
  import { formatAuthError } from '../bounty/helpers';
  import { lightDismiss } from '../dialog-light-dismiss';
  import { t } from '../i18n';
  import UserAvatar from './UserAvatar.svelte';
  import {
    AUTH_NAME_MAX_LENGTH,
    DEFAULT_AUTH_AVATAR_PRESETS,
    avatarsEqual,
    fileToUploadAvatar,
    validateUserNameError,
  } from './avatars';

  interface Props {
    profile: UserProfile;
    presets: AuthAvatarPreset[];
    presetsError?: string;
    onLoadAvatars?: () => Promise<void>;
    onSave: (input: UpdateUserProfileInput) => Promise<void>;
    onClose: () => void;
  }

  let {
    profile,
    presets,
    presetsError = '',
    onLoadAvatars,
    onSave,
    onClose,
  }: Props = $props();

  let dialog = $state<HTMLDialogElement>();
  let draftName = $state(untrack(() => profile.name));
  let draftAvatar = $state<UserAvatarValue | null>(
    untrack(() => (profile.avatar ? { ...profile.avatar } : null)),
  );
  let saving = $state(false);
  let error = $state('');
  let fileInput = $state<HTMLInputElement>();

  const resolvedPresets = $derived(
    presets.length > 0 ? presets : [...DEFAULT_AUTH_AVATAR_PRESETS],
  );

  $effect(() => {
    void onLoadAvatars?.();
  });

  $effect(() => {
    const el = dialog;
    if (!el) return;
    if (typeof el.showModal === 'function') {
      if (!el.open) el.showModal();
      return;
    }
    el.setAttribute('open', '');
    const onKeydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        close();
      }
    };
    document.addEventListener('keydown', onKeydown, true);
    return () => {
      document.removeEventListener('keydown', onKeydown, true);
      el.removeAttribute('open');
    };
  });

  function close(): void {
    if (!dialog?.isConnected) return;
    if (typeof dialog.close === 'function') dialog.close();
    else onClose();
  }

  async function handleFileChange(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file || saving) return;
    error = '';
    try {
      draftAvatar = await fileToUploadAvatar(file);
    } catch (cause) {
      error = formatAuthError(cause, 'profile');
    }
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (saving) return;
    error = '';

    const nameErr = validateUserNameError(draftName);
    if (nameErr) {
      error = nameErr;
      return;
    }
    const trimmedName = draftName.trim();
    const nameChanged = trimmedName !== profile.name;
    const avatarChanged = !avatarsEqual(draftAvatar, profile.avatar);
    if (!nameChanged && !avatarChanged) {
      close();
      return;
    }

    saving = true;
    try {
      await onSave({
        ...(nameChanged ? { name: trimmedName } : {}),
        ...(avatarChanged ? { avatar: $state.snapshot(draftAvatar) } : {}),
      });
      close();
    } catch (cause) {
      error = formatAuthError(cause, 'profile');
    } finally {
      saving = false;
    }
  }
</script>

<dialog
  bind:this={dialog}
  use:lightDismiss
  class="modal-dialog profile-dialog"
  aria-labelledby="profile-dialog-title"
  closedby="any"
  data-dialog="user-profile"
  onclose={onClose}
>
  <header class="modal-header profile-dialog-header">
    <h2 id="profile-dialog-title" class="modal-title">{$t('个人设置')}</h2>
    <button
      type="button"
      class="icon-button"
      aria-label={$t('关闭对话框')}
      onclick={close}
    >
      ✕
    </button>
  </header>

  <form class="profile-dialog-form" onsubmit={submit}>
    <div class="profile-avatar-row" data-slot="profile-avatar-editor">
      <input
        bind:this={fileInput}
        type="file"
        class="visually-hidden"
        accept="image/png,image/jpeg,image/webp"
        data-slot="avatar-upload-input"
        onchange={handleFileChange}
      />
      <button
        type="button"
        class="profile-avatar-trigger"
        data-slot="profile-preview"
        aria-label={$t('上传头像')}
        title={$t('上传头像')}
        disabled={saving}
        onclick={() => fileInput?.click()}
      >
        <UserAvatar name={draftName.trim() || profile.name} avatar={draftAvatar} size="lg" />
        <span class="profile-avatar-overlay" aria-hidden="true">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z" />
            <circle cx="12" cy="13" r="3" />
          </svg>
        </span>
      </button>

      <div class="profile-avatar-controls">
        <div class="profile-avatar-buttons">
          <button
            type="button"
            class="secondary-action"
            data-action="choose-avatar-file"
            disabled={saving}
            onclick={() => fileInput?.click()}
          >
            {$t('上传头像')}
          </button>
          {#if draftAvatar?.kind === 'upload'}
            <button
              type="button"
              class="secondary-action ghost-action"
              data-action="clear-avatar"
              disabled={saving}
              onclick={() => {
                draftAvatar = null;
                error = '';
              }}
            >
              {$t('移除')}
            </button>
          {/if}
        </div>

        <div class="profile-color-row">
          <span class="profile-color-label">{$t('默认底色')}</span>
          <div
            class="profile-color-dots"
            role="group"
            aria-label={$t('系统预制头像')}
            data-slot="avatar-preset-grid"
          >
            {#each resolvedPresets as preset (preset.id)}
              {@const active = draftAvatar?.kind === 'preset' && draftAvatar.id === preset.id}
              <button
                type="button"
                class="profile-color-dot"
                class:active
                aria-pressed={active}
                aria-label={$t(preset.label)}
                title={$t(preset.label)}
                data-avatar-preset={preset.id}
                data-avatar-preset-option={preset.id}
                disabled={saving}
                onclick={() => {
                  draftAvatar = { kind: 'preset', id: preset.id };
                  error = '';
                }}
              ></button>
            {/each}
          </div>
        </div>
      </div>
    </div>

    {#if presetsError}
      <p class="composer-hint" data-slot="profile-presets-warning">
        {$t('未能同步最新预设列表，当前使用内置预设：{error}', { error: presetsError })}
      </p>
    {/if}

    <div class="project-form-field">
      <label class="composer-label" for="profile-name-input">{$t('显示名称')}</label>
      <input
        id="profile-name-input"
        class="settings-text-input"
        bind:value={draftName}
        maxlength={AUTH_NAME_MAX_LENGTH}
        disabled={saving}
        required
      />
    </div>

    {#if error}
      <p class="dialog-error" role="alert" data-slot="profile-error">{error}</p>
    {/if}

    <footer class="modal-actions profile-dialog-actions">
      <button
        type="button"
        class="secondary-action ghost-action"
        data-action="cancel-profile"
        disabled={saving}
        onclick={close}
      >
        {$t('取消')}
      </button>
      <button
        type="submit"
        class="primary secondary-action"
        data-action="save-profile"
        disabled={saving}
      >
        {$t(saving ? '正在保存…' : '保存')}
      </button>
    </footer>
  </form>
</dialog>
