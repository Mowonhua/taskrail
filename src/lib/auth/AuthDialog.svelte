<script lang="ts">
  /**
   * 文件职责：承载用户账号登录与注册弹窗，支持可选莫兰迪系统预制头像或上传头像。
   * 约束条件：用户注册不依赖 Agent 邀请令牌；严格校验名称（1–64）与密码（12–128）规则。
   */
  import { untrack } from 'svelte';
  import appIcon from '../../../src-tauri/icons/icon.png';
  import type {
    AuthAvatarPreset,
    LoginUserInput,
    RegisterUserInput,
    UserAvatar as UserAvatarValue,
  } from '../api/types';
  import { formatAuthError } from '../bounty/helpers';
  import { lightDismiss } from '../dialog-light-dismiss';
  import { t } from '../i18n';
  import UserAvatarEditor from './UserAvatarEditor.svelte';
  import {
    AUTH_NAME_MAX_LENGTH,
    AUTH_PASSWORD_MAX_LENGTH,
    AUTH_PASSWORD_MIN_LENGTH,
    validateUserNameError,
    validateUserPasswordError,
  } from './avatars';

  interface Props {
    initialMode?: 'login' | 'register';
    presets: AuthAvatarPreset[];
    presetsError?: string;
    onLoadAvatars?: () => Promise<void>;
    onLogin: (input: LoginUserInput) => Promise<void>;
    onRegister: (input: RegisterUserInput) => Promise<void>;
    onClose: () => void;
  }

  let {
    initialMode = 'login',
    presets,
    presetsError = '',
    onLoadAvatars,
    onLogin,
    onRegister,
    onClose,
  }: Props = $props();

  let dialog = $state<HTMLDialogElement>();
  let mode = $state<'login' | 'register'>(untrack(() => initialMode));
  let name = $state('');
  let password = $state('');
  let showPassword = $state(false);
  let passwordTouched = $state(false);
  let selectedAvatar = $state<UserAvatarValue | null>(null);
  let submitting = $state(false);
  let error = $state('');

  const showPasswordHint = $derived(
    !error &&
      mode === 'register' &&
      passwordTouched &&
      password.length > 0 &&
      Array.from(password).length < AUTH_PASSWORD_MIN_LENGTH,
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
    showPassword = false;
    passwordTouched = false;
    error = '';
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
    <div class="auth-dialog-brand">
      <img src={appIcon} width="22" height="22" alt="" draggable={false} />
      <h2 id="auth-dialog-title" class="modal-title">
        {$t(mode === 'login' ? '登录 Taskrail' : '创建账号')}
      </h2>
    </div>
    <button
      type="button"
      class="icon-button"
      aria-label={$t('关闭对话框')}
      onclick={close}
    >
      ✕
    </button>
  </header>

  <form class="auth-dialog-form" onsubmit={submit}>
    {#if mode === 'register'}
      <UserAvatarEditor
        name={name.trim()}
        avatar={selectedAvatar}
        {presets}
        {presetsError}
        disabled={submitting}
        errorContext="register"
        editorSlot="auth-register-avatar"
        previewSlot="auth-avatar-preview"
        uploadInputSlot="auth-avatar-upload-input"
        uploadAction="auth-upload-avatar"
        warningSlot="auth-presets-warning"
        onChange={next => {
          selectedAvatar = next;
        }}
        onError={message => {
          error = message;
        }}
      />
    {/if}

    <div class="project-form-field">
      <label class="composer-label" for="auth-name-input">{$t('用户名')}</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        id="auth-name-input"
        name="username"
        class="settings-text-input"
        bind:value={name}
        maxlength={AUTH_NAME_MAX_LENGTH}
        autocomplete="username"
        autocorrect="off"
        autocapitalize="none"
        spellcheck={false}
        enterkeyhint="next"
        autofocus
        disabled={submitting}
        placeholder={$t('输入用户名')}
      />
    </div>

    <div class="project-form-field">
      <label class="composer-label" for="auth-password-input">{$t('密码')}</label>
      <div class="auth-password-wrap">
        <input
          id="auth-password-input"
          name="password"
          type={showPassword ? 'text' : 'password'}
          class="settings-text-input auth-password-input"
          bind:value={password}
          maxlength={AUTH_PASSWORD_MAX_LENGTH}
          autocomplete={mode === 'login' ? 'current-password' : 'new-password'}
          enterkeyhint="done"
          disabled={submitting}
          placeholder={mode === 'register' ? $t('输入密码（至少 12 位）') : $t('输入密码')}
          onblur={() => {
            if (password.length > 0) passwordTouched = true;
          }}
        />
        <button
          type="button"
          class="icon-button auth-password-toggle"
          aria-pressed={showPassword}
          aria-label={$t(showPassword ? '隐藏密码' : '显示密码')}
          title={$t(showPassword ? '隐藏密码' : '显示密码')}
          disabled={submitting}
          onclick={() => {
            showPassword = !showPassword;
          }}
        >
          {#if showPassword}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M10.733 5.076a10.744 10.744 0 0 1 11.205 6.575 1 1 0 0 1 0 .696 10.747 10.747 0 0 1-1.444 2.49" />
              <path d="M14.084 14.158a3 3 0 0 1-4.242-4.242" />
              <path d="M17.479 17.499a10.75 10.75 0 0 1-15.417-5.151 1 1 0 0 1 0-.696 10.75 10.75 0 0 1 4.446-5.143" />
              <path d="m2 2 20 20" />
            </svg>
          {:else}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0" />
              <circle cx="12" cy="12" r="3" />
            </svg>
          {/if}
        </button>
      </div>
      {#if showPasswordHint}
        <p class="auth-field-hint" data-slot="auth-password-hint">
          {$t('密码至少需要 12 个字符')}
        </p>
      {/if}
    </div>

    {#if error}
      <p class="dialog-error" role="alert" data-slot="auth-error">{error}</p>
    {/if}

    <div class="auth-submit-block">
      <button
        type="submit"
        class="primary auth-submit-btn"
        data-action="confirm-auth"
        disabled={submitting}
      >
        {$t(
          submitting
            ? mode === 'login'
              ? '正在登录…'
              : '正在创建…'
            : mode === 'login'
              ? '登录'
              : '创建账号',
        )}
      </button>

      <p class="auth-mode-footer">
        <span>{$t(mode === 'login' ? '还没有账号？' : '已有账号？')}</span>
        <button
          type="button"
          class="auth-mode-switch-link"
          data-auth-tab={mode === 'login' ? 'register' : 'login'}
          disabled={submitting}
          onclick={() => switchMode(mode === 'login' ? 'register' : 'login')}
        >
          {$t(mode === 'login' ? '去注册' : '去登录')}
        </button>
      </p>
    </div>
  </form>
</dialog>
