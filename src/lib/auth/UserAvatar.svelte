<script lang="ts">
  /**
   * 文件职责：渲染用户头像（未登录占位、无名轮廓、上传图片、系统预制配色或首字母）。
   * 约束条件：预制 ID（slate/blue/green/amber/purple/rose）通过 data-avatar-preset 在 CSS 中统一定色。
   */
  import type { UserAvatar as UserAvatarValue } from '../api/types';
  import { avatarToDataUrl, getAvatarInitial, isPresetAvatarId } from './avatars';

  interface Props {
    name?: string;
    avatar?: UserAvatarValue | null;
    size?: 'sm' | 'md' | 'lg';
    guest?: boolean;
  }

  let { name = '', avatar = null, size = 'sm', guest = false }: Props = $props();
  let uploadFailed = $state(false);

  const trimmedName = $derived(name.trim());
  const uploadUrl = $derived(avatarToDataUrl(avatar));

  $effect(() => {
    const _url = uploadUrl;
    uploadFailed = false;
  });

  const avatarKind = $derived.by(() => {
    if (guest) return 'guest';
    if (uploadUrl && !uploadFailed) return 'upload';
    if (avatar?.kind === 'preset') return 'preset';
    if (!trimmedName) return 'placeholder';
    return 'initial';
  });

  const presetId = $derived.by(() => {
    if (avatar?.kind === 'preset') {
      return isPresetAvatarId(avatar.id) ? avatar.id : 'slate';
    }
    return guest || !trimmedName ? undefined : 'slate';
  });
</script>

<span
  class={`user-avatar user-avatar-${size}`}
  data-avatar-kind={avatarKind}
  data-avatar-preset={presetId}
  aria-hidden="true"
>
  {#if avatarKind === 'upload' && uploadUrl}
    <img
      src={uploadUrl}
      alt=""
      draggable={false}
      onerror={() => {
        uploadFailed = true;
      }}
    />
  {:else if avatarKind === 'guest' || !trimmedName}
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="8" cy="5.5" r="2.6" />
      <path d="M3 13.4c.8-2.1 2.5-3.2 5-3.2s4.2 1.1 5 3.2" />
    </svg>
  {:else}
    <span class="user-avatar-initial">{getAvatarInitial(trimmedName)}</span>
  {/if}
</span>
