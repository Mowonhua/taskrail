<script lang="ts">
  /**
   * 文件职责：承载新建任务与草稿创建的居中双栏弹窗外壳。
   * 定义范围：复用 BountyEditorForm 双栏编辑骨架，负责弹窗容器、固定顶栏、Escape 关闭与焦点恢复。
   */
  import { onMount } from 'svelte';
  import type {
    BountyProject,
    BountyTask,
    CreateBountyInput,
    CreateProjectInput,
    UpdateBountyInput,
  } from '../api/types';
  import { t } from '../i18n';
  import BountyEditorForm from './BountyEditorForm.svelte';

  interface Props {
    projects: BountyProject[];
    availableDependencies?: BountyTask[];
    initialProjectId?: string | null;
    editingBounty?: BountyTask | null;
    desktop: boolean;
    mobile: boolean;
    onClose: () => void;
    onCreateProject: (input: CreateProjectInput) => Promise<BountyProject>;
    onCreateBounty: (input: CreateBountyInput) => Promise<BountyTask>;
    onUpdateDraft: (id: string, input: UpdateBountyInput, publishAfter: boolean) => Promise<BountyTask>;
  }

  let {
    projects,
    availableDependencies = [],
    initialProjectId = null,
    editingBounty = null,
    desktop,
    mobile,
    onClose,
    onCreateProject,
    onCreateBounty,
    onUpdateDraft,
  }: Props = $props();

  let editorForm = $state<ReturnType<typeof BountyEditorForm> | undefined>(undefined);
  let returnFocusEl: HTMLElement | null =
    typeof document !== 'undefined' && document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;

  onMount(() => {
    return () => {
      const opener = returnFocusEl;
      returnFocusEl = null;
      if (opener && opener.isConnected && !opener.closest('[hidden], [inert]')) {
        opener.focus();
      }
    };
  });

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (event.key !== 'Escape') return;
    event.preventDefault();
    event.stopPropagation();
    if (editorForm?.handleEscapeKey()) {
      return;
    }
    onClose();
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

<div
  class="detail-modal-backdrop composer-backdrop"
  role="presentation"
  onclick={event => {
    if (event.target === event.currentTarget) onClose();
  }}
>
  <div
    class="detail-modal bounty-composer-dialog"
    role="dialog"
    aria-modal="true"
    aria-label={editingBounty ? $t('编辑草稿') : $t('发布任务')}
    data-dialog="bounty-composer"
  >
    <header class="detail-modal-topbar">
      <div class="detail-modal-breadcrumb">
        <h2 class="composer-topbar-title">{editingBounty ? $t('编辑草稿') : $t('发布任务')}</h2>
      </div>
      <div class="detail-modal-topbar-actions">
        <button
          type="button"
          class="icon-button detail-close-btn"
          aria-label={$t('关闭对话框')}
          onclick={onClose}
        >
          ✕
        </button>
      </div>
    </header>

    <BountyEditorForm
      bind:this={editorForm}
      {projects}
      {availableDependencies}
      {initialProjectId}
      {editingBounty}
      {desktop}
      {mobile}
      onCancel={onClose}
      onSaved={onClose}
      {onCreateProject}
      {onCreateBounty}
      {onUpdateDraft}
    />
  </div>
</div>
