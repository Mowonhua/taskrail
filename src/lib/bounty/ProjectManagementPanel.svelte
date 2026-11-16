<script lang="ts">
  /**
   * 文件职责：在设置页面集中管理悬赏项目（创建、编辑、图标配置、工作目录选择、默认验收规范、归档恢复与无引用删除）。
   * 定义范围：列表展示、显示已归档开关、表单交互与 PROJECT_HAS_BOUNTIES 错误提示。
   */
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import type {
    BountyProject,
    CreateProjectInput,
    UpdateProjectInput,
  } from '../api/types';
  import { t } from '../i18n';
  import { serializeProjectIcon, type ProjectIcon as ProjectIconValue } from '../project-icons';
  import ProjectIcon from '../ProjectIcon.svelte';
  import ProjectIconPicker from '../ProjectIconPicker.svelte';
  import { formatServiceError } from './helpers';

  interface Props {
    projects: BountyProject[];
    showArchived: boolean;
    desktop: boolean;
    mobile: boolean;
    onToggleShowArchived: (show: boolean) => Promise<void> | void;
    onCreateProject: (input: CreateProjectInput) => Promise<BountyProject>;
    onUpdateProject: (id: string, input: UpdateProjectInput) => Promise<BountyProject>;
    onDeleteProject: (id: string) => Promise<void>;
  }

  let {
    projects,
    showArchived,
    desktop,
    mobile,
    onToggleShowArchived,
    onCreateProject,
    onUpdateProject,
    onDeleteProject,
  }: Props = $props();

  let formMode = $state<'closed' | 'create' | 'edit'>('closed');
  let editingId = $state<string | null>(null);
  let formName = $state('');
  let formIcon = $state<string | null>(null);
  let formDescription = $state('');
  let formWorkspacePath = $state('');
  let formDefaultCriteria = $state('');

  let iconPickerOpen = $state(false);
  let busy = $state(false);
  let panelError = $state('');

  function openCreateForm(): void {
    formMode = 'create';
    editingId = null;
    formName = '';
    formIcon = null;
    formDescription = '';
    formWorkspacePath = '';
    formDefaultCriteria = '';
    panelError = '';
  }

  function openEditForm(project: BountyProject): void {
    formMode = 'edit';
    editingId = project.id;
    formName = project.name;
    formIcon = project.icon;
    formDescription = project.description;
    formWorkspacePath = project.workspace_path;
    formDefaultCriteria = project.default_acceptance_criteria;
    panelError = '';
  }

  function closeForm(): void {
    formMode = 'closed';
    editingId = null;
    panelError = '';
  }

  async function handleBrowseWorkspace(): Promise<void> {
    if (!desktop) return;
    try {
      const picked = await openDialog({
        directory: true,
        multiple: false,
      });
      if (typeof picked === 'string' && picked.trim()) {
        formWorkspacePath = picked.trim();
      }
    } catch (error) {
      panelError = formatServiceError(error);
    }
  }

  async function handleSubmitForm(): Promise<void> {
    if (busy) return;
    const trimmedName = formName.trim();
    if (!trimmedName) {
      panelError = $t('请输入项目名称。');
      return;
    }
    panelError = '';
    busy = true;
    try {
      if (formMode === 'create') {
        await onCreateProject({
          name: trimmedName,
          icon: formIcon,
          description: formDescription.trim(),
          workspace_path: formWorkspacePath.trim(),
          default_acceptance_criteria: formDefaultCriteria,
        });
      } else if (formMode === 'edit' && editingId) {
        await onUpdateProject(editingId, {
          name: trimmedName,
          icon: formIcon,
          description: formDescription.trim(),
          workspace_path: formWorkspacePath.trim(),
          default_acceptance_criteria: formDefaultCriteria,
        });
      }
      closeForm();
    } catch (error) {
      panelError = formatServiceError(error);
    } finally {
      busy = false;
    }
  }

  async function handleToggleArchive(project: BountyProject): Promise<void> {
    if (busy) return;
    panelError = '';
    busy = true;
    try {
      await onUpdateProject(project.id, { archived: !project.archived });
    } catch (error) {
      panelError = formatServiceError(error);
    } finally {
      busy = false;
    }
  }

  async function handleDelete(project: BountyProject): Promise<void> {
    if (busy) return;
    panelError = '';
    busy = true;
    try {
      await onDeleteProject(project.id);
      if (editingId === project.id) closeForm();
    } catch (error) {
      panelError = formatServiceError(error);
    } finally {
      busy = false;
    }
  }
</script>

<div class="project-management-panel" data-section="project-management">
  <div class="project-management-header">
    <div class="settings-extension-copy">
      <span class="settings-row-label">{$t('项目管理与默认图标')}</span>
    </div>

    <div class="project-management-toolbar">
      <label class="archive-switch-label" for="show-archived-projects-switch">
        <span>{$t('显示已归档')}</span>
        <span class="settings-switch">
          <input
            id="show-archived-projects-switch"
            type="checkbox"
            role="switch"
            aria-label={$t('显示已归档')}
            checked={showArchived}
            onchange={event => void onToggleShowArchived(event.currentTarget.checked)}
          />
          <span class="settings-switch-track" aria-hidden="true">
            <span class="settings-switch-thumb"></span>
          </span>
        </span>
      </label>

      <button
        type="button"
        class="secondary-action"
        data-action="new-project"
        onclick={openCreateForm}
      >
        + {$t('新建项目')}
      </button>
    </div>
  </div>

  {#if panelError}
    <p class="dialog-error settings-error" role="alert" data-status="project-panel-error">
      {panelError}
    </p>
  {/if}

  {#if formMode !== 'closed'}
    <div class="project-form-card" data-slot="project-form">
      <h4 class="project-form-title">
        {formMode === 'create' ? $t('新建项目') : $t('编辑项目')}
      </h4>

      <div class="project-form-row">
        <button
          type="button"
          class="project-badge-button"
          data-action="open-project-form-icon-picker"
          aria-label={$t('选择项目图标')}
          onclick={() => {
            iconPickerOpen = true;
          }}
        >
          <ProjectIcon icon={formIcon ?? undefined} />
          <span>{$t('选择图标')}</span>
        </button>

        {#if formIcon}
          <button
            type="button"
            class="secondary-action"
            data-action="clear-project-form-icon"
            onclick={() => {
              formIcon = null;
            }}
          >
            {$t('清除图标')}
          </button>
        {/if}

        <input
          id="project-form-name"
          type="text"
          class="settings-text-input"
          aria-label={$t('项目名称')}
          placeholder={$t('项目名称')}
          bind:value={formName}
        />
      </div>

      <div class="project-form-field">
        <label class="composer-label" for="project-form-description">{$t('项目说明')}</label>
        <input
          id="project-form-description"
          type="text"
          class="settings-text-input"
          aria-label={$t('项目说明')}
          placeholder={$t('简要说明项目范围与协作定位（可选）')}
          bind:value={formDescription}
        />
      </div>

      <div class="project-form-field">
        <label class="composer-label" for="project-form-workspace">{$t('工作目录路径')}</label>
        <div class="inline-project-row">
          <input
            id="project-form-workspace"
            type="text"
            class="settings-text-input"
            aria-label={$t('工作目录路径')}
            placeholder={$t('例如：D:/Workspace/taskrail')}
            bind:value={formWorkspacePath}
          />
          {#if desktop}
            <button
              type="button"
              class="secondary-action"
              data-action="browse-project-workspace"
              onclick={() => void handleBrowseWorkspace()}
            >
              {$t('浏览…')}
            </button>
          {/if}
        </div>
      </div>

      <div class="project-form-field">
        <label class="composer-label" for="project-form-criteria">{$t('默认验收规范')}</label>
        <textarea
          id="project-form-criteria"
          class="composer-textarea"
          rows="3"
          aria-label={$t('默认验收规范')}
          placeholder={$t('新建该项目下的悬赏时自动预填的验收条件（支持 Markdown 与 KaTeX）')}
          bind:value={formDefaultCriteria}
        ></textarea>
      </div>

      <div class="inline-project-actions">
        <button
          type="button"
          class="secondary-action"
          data-action="cancel-project-form"
          onclick={closeForm}
        >
          {$t('取消')}
        </button>
        <button
          type="button"
          class="primary secondary-action"
          data-action="submit-project-form"
          disabled={busy}
          onclick={() => void handleSubmitForm()}
        >
          {$t('保存项目')}
        </button>
      </div>
    </div>
  {/if}

  {#if projects.length === 0}
    <p class="project-list-empty" data-slot="project-list-empty">
      {$t('暂无项目，点击「+ 新建项目」创建第一个悬赏项目。')}
    </p>
  {:else}
    <ul class="project-management-list" data-slot="project-list">
      {#each projects as project (project.id)}
        <li
          class="project-management-item"
          class:archived={project.archived}
          data-project-row={project.id}
        >
          <div class="project-item-main">
            <ProjectIcon icon={project.icon ?? undefined} />
            <div class="project-item-copy">
              <div class="project-item-title-row">
                <span class="project-item-name">{project.name}</span>
                {#if project.archived}
                  <span class="settings-tag" data-tag="archived">{$t('已归档')}</span>
                {/if}
              </div>
              {#if project.workspace_path}
                <span class="project-item-path">{project.workspace_path}</span>
              {/if}
              {#if project.description}
                <span class="project-item-desc">{project.description}</span>
              {/if}
            </div>
          </div>

          <div class="project-item-actions">
            <button
              type="button"
              class="secondary-action"
              data-action="edit-project"
              disabled={busy}
              onclick={() => openEditForm(project)}
            >
              {$t('编辑')}
            </button>
            <button
              type="button"
              class="secondary-action"
              data-action="toggle-archive-project"
              disabled={busy}
              onclick={() => void handleToggleArchive(project)}
            >
              {project.archived ? $t('恢复') : $t('归档')}
            </button>
            <button
              type="button"
              class="secondary-action danger-text"
              data-action="delete-project"
              disabled={busy}
              onclick={() => void handleDelete(project)}
            >
              {$t('删除')}
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}

  {#if iconPickerOpen}
    <div
      class="modal-backdrop"
      role="presentation"
      onclick={event => {
        if (event.target === event.currentTarget) iconPickerOpen = false;
      }}
    >
      <div
        class="modal-dialog icon-picker-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={$t('选择项目图标')}
      >
        <div class="modal-header">
          <h2 class="modal-title">{$t('选择项目图标')}</h2>
          <button
            type="button"
            class="icon-button"
            aria-label={$t('关闭对话框')}
            onclick={() => {
              iconPickerOpen = false;
            }}
          >
            ✕
          </button>
        </div>
        <ProjectIconPicker
          projectName={formName.trim() || $t('新项目')}
          icon={formIcon ?? undefined}
          {mobile}
          onSelect={(icon: ProjectIconValue) => {
            formIcon = serializeProjectIcon(icon);
            iconPickerOpen = false;
          }}
          onReset={() => {
            formIcon = null;
            iconPickerOpen = false;
          }}
        />
      </div>
    </div>
  {/if}
</div>
