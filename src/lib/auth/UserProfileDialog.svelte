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
  import UserAvatarEditor from './UserAvatarEditor.svelte';
  import {
    AUTH_NAME_MAX_LENGTH,
    avatarsEqual,
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
    <UserAvatarEditor
      name={draftName.trim() || profile.name}
      avatar={draftAvatar}
      {presets}
      {presetsError}
      disabled={saving}
      errorContext="profile"
      onChange={next => {
        draftAvatar = next;
      }}
      onError={message => {
        error = message;
      }}
    />

    <div class="project-form-field">
      <label class="composer-label" for="profile-name-input">{$t('显示名称')}</label>
      <input
        id="profile-name-input"
        name="displayName"
        class="settings-text-input"
        bind:value={draftName}
        maxlength={AUTH_NAME_MAX_LENGTH}
        autocomplete="name"
        autocorrect="off"
        autocapitalize="none"
        spellcheck={false}
        enterkeyhint="done"
        placeholder={$t('输入显示名称')}
        disabled={saving}
        required
      />
    </div>

    {#if error}
      <p class="dialog-error" role="alert" data-slot="profile-error">{error}</p>
    {/if}

    <div class="auth-submit-block">
      <button
        type="submit"
        class="primary auth-submit-btn"
        data-action="save-profile"
        disabled={saving}
      >
        {$t(saving ? '正在保存…' : '保存')}
      </button>
    </div>
  </form>
</dialog>
