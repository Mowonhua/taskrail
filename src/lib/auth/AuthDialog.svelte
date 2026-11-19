<script lang="ts">
  /**
   * 文件职责：承载用户账号注册与登录弹窗，支持可选系统预制或上传头像。
   * 约束条件：用户注册不依赖 Agent 邀请令牌；严格校验名称（1–64）与密码（12–128）规则。
   */
  import { untrack } from 'svelte';
  import type {
    AuthAvatarPreset,
    LoginUserInput,
    RegisterUserInput,
    UserAvatar as UserAvatarValue,
  } from '../api/types';
  import { formatAuthError } from '../bounty/helpers';
  import { lightDismiss } from '../dialog-light-dismiss';
  import { t } from '../i18n';
  import UserAvatar from './UserAvatar.svelte';
  import {
    AUTH_NAME_MAX_LENGTH,
    AUTH_PASSWORD_MAX_LENGTH,
    DEFAULT_AUTH_AVATAR_PRESETS,
    fileToUploadAvatar,
    validateUserNameError,
    validateUserPasswordError,
  } from './avatars';

  interface Props {
    initialMode?: 'login' | 'register';
    presets: AuthAvatarPreset[];
    onLoadAvatars?: () => Promise<void>;
    onLogin: (input: LoginUserInput) => Promise<void>;
    onRegister: (input: RegisterUserInput) => Promise<void>;
    onClose: () => void;
  }

  let {
    initialMode = 'login',
    presets,
    onLoadAvatars,
    onLogin,
    onRegister,
    onClose,
  }: Props = $props();

  let dialog = $state<HTMLDialogElement>();
  let mode = $state<'login' | 'register'>(untrack(() => initialMode));
  let name = $state('');
  let password = $state('');
  let selectedAvatar = $state<UserAvatarValue | null>(null);
  let submitting = $state(false);
  let error = $state('');
  let fileInput = $state<HTMLInputElement>();

  const resolvedPresets = $derived(
    presets.length > 0 ? presets : [...DEFAULT_AUTH_AVATAR_PRESETS],
  );

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

  $effect(() => {
    if (mode === 'register') {
      void onLoadAvatars?.();
    }
  });

  function close(): void {
    if (!dialog?.isConnected) return;
    if (typeof dialog.close === 'function') dialog.close();
    else onClose();
  }

  function switchMode(nextMode: 'login' | 'register'): void {
    if (submitting || mode === nextMode) return;
    mode = nextMode;
    error = '';
  }

  async function handleFileChange(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file || submitting) return;
    error = '';
    try {
      selectedAvatar = await fileToUploadAvatar(file);
    } catch (cause) {
      error = formatAuthError(cause, 'register');
    }
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (submitting) return;
    error = '';
    const trimmedName = name.trim();
    if (mode === 'login') {
      if (!trimmedName || !password) {
        error = $t('请输入用户名和密码。');
        return;
      }
      submitting = true;
      try {
        await onLogin({ name: trimmedName, password });
        close();
      } catch (cause) {
        error = formatAuthError(cause, 'login');
      } finally {
        submitting = false;
      }
      return;
    }

    const nameErr = validateUserNameError(name);
    if (nameErr) {
      error = nameErr;
      return;
    }
    const passwordErr = validateUserPasswordError(password);
    if (passwordErr) {
      error = passwordErr;
      return;
    }

    submitting = true;
    try {
      await onRegister({
        name: trimmedName,
        password,
        ...(selectedAvatar !== null ? { avatar: $state.snapshot(selectedAvatar) } : {}),
      });
      close();
    } catch (cause) {
      error = formatAuthError(cause, 'register');
    } finally {
      submitting = false;
    }
  }
</script>

<dialog
  bind:this={dialog}
  use:lightDismiss
  class="modal-dialog auth-dialog"
  aria-labelledby="auth-dialog-title"
  closedby="any"
  data-dialog="user-auth"
  onclose={onClose}
>
  <header class="modal-header auth-dialog-header">
    <h2 id="auth-dialog-title" class="modal-title">
      {$t(mode === 'login' ? '登录账号' : '注册账号')}
    </h2>
    <button
      type="button"
      class="icon-button"
      aria-label={$t('关闭对话框')}
      onclick={close}
    >
      ✕
    </button>
  </header>

  <div class="segmented-group auth-mode-tabs" role="tablist" aria-label={$t('账号操作')}>
    <button
      type="button"
      role="tab"
      class="segmented-button"
      class:active={mode === 'login'}
      aria-selected={mode === 'login'}
      data-auth-tab="login"
      disabled={submitting}
      onclick={() => switchMode('login')}
    >
      {$t('登录')}
    </button>
    <button
      type="button"
      role="tab"
      class="segmented-button"
      class:active={mode === 'register'}
      aria-selected={mode === 'register'}
      data-auth-tab="register"
      disabled={submitting}
      onclick={() => switchMode('register')}
    >
      {$t('注册')}
    </button>
  </div>

  <form class="auth-dialog-form" onsubmit={submit}>
    <div class="project-form-field">
      <label class="composer-label" for="auth-name-input">{$t('用户名')}</label>
      <input
        id="auth-name-input"
        class="settings-text-input"
        bind:value={name}
        maxlength={AUTH_NAME_MAX_LENGTH}
        autocomplete="username"
        disabled={submitting}
        placeholder={$t('1 至 64 个字符')}
      />
    </div>

    <div class="project-form-field">
      <label class="composer-label" for="auth-password-input">{$t('密码')}</label>
      <input
        id="auth-password-input"
        type="password"
        class="settings-text-input"
        bind:value={password}
        maxlength={AUTH_PASSWORD_MAX_LENGTH}
        autocomplete={mode === 'login' ? 'current-password' : 'new-password'}
        disabled={submitting}
        placeholder={mode === 'register' ? $t('12 至 128 个字符') : ''}
      />
    </div>

    {#if mode === 'register'}
      <div class="auth-avatar-section" data-slot="auth-register-avatar">
        <div class="auth-avatar-preview-row">
          <UserAvatar name={name || '?'} avatar={selectedAvatar} size="md" />
          <div class="auth-avatar-preview-copy">
            <span class="composer-label">{$t('头像（可选）')}</span>
            <span class="composer-hint">{$t('可选择预设颜色，或上传不超过 256 KiB 的 PNG/JPEG/WebP。')}</span>
          </div>
        </div>

        <div class="avatar-preset-grid" role="group" aria-label={$t('系统预制头像')}>
          {#each resolvedPresets as preset (preset.id)}
            {@const active = selectedAvatar?.kind === 'preset' && selectedAvatar.id === preset.id}
            <button
              type="button"
              class="avatar-preset-option"
              class:active
              aria-pressed={active}
              data-avatar-preset-option={preset.id}
              disabled={submitting}
              onclick={() => {
                selectedAvatar = { kind: 'preset', id: preset.id };
                error = '';
              }}
            >
              <UserAvatar name={name || preset.label} avatar={{ kind: 'preset', id: preset.id }} size="sm" />
              <span>{$t(preset.label)}</span>
            </button>
          {/each}
        </div>

        <div class="avatar-upload-actions">
          <input
            bind:this={fileInput}
            type="file"
            class="visually-hidden"
            accept="image/png,image/jpeg,image/webp"
            data-slot="auth-avatar-upload-input"
            onchange={handleFileChange}
          />
          <button
            type="button"
            class="secondary-action"
            data-action="auth-upload-avatar"
            disabled={submitting}
            onclick={() => fileInput?.click()}
          >
            {$t('上传头像')}
          </button>
          {#if selectedAvatar !== null}
            <button
              type="button"
              class="secondary-action"
              data-action="auth-clear-avatar"
              disabled={submitting}
              onclick={() => {
                selectedAvatar = null;
                error = '';
              }}
            >
              {$t('清除头像')}
            </button>
          {/if}
        </div>
      </div>
    {/if}

    {#if error}
      <p class="dialog-error" role="alert" data-slot="auth-error">{error}</p>
    {/if}

    <footer class="modal-actions auth-dialog-actions">
      <button
        type="button"
        class="secondary-action"
        data-action="cancel-auth"
        disabled={submitting}
        onclick={close}
      >
        {$t('取消')}
      </button>
      <button
        type="submit"
        class="primary"
        data-action="confirm-auth"
        disabled={submitting}
      >
        {$t(
          submitting
            ? mode === 'login'
              ? '正在登录…'
              : '正在注册…'
            : mode === 'login'
              ? '登录'
              : '注册',
        )}
      </button>
    </footer>
  </form>
</dialog>
