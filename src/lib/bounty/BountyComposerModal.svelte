<script lang="ts">
  /**
   * 文件职责：承载新建悬赏与编辑草稿（DRAFT）的模态抽屉表单。
   * 定义范围：项目选择与内联新建项目、标题、Markdown 详情与验收条件（编写/预览切换）、难度、执行者限制与仲裁开关。
   */
  import { untrack } from 'svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import type {
    AssigneeMode,
    BountyDifficulty,
    BountyProject,
    BountyTask,
    CreateBountyInput,
    CreateProjectInput,
    UpdateBountyInput,
  } from '../api/types';
  import { t } from '../i18n';
  import { serializeProjectIcon, type ProjectIcon as ProjectIconValue } from '../project-icons';
  import ProjectIcon from '../ProjectIcon.svelte';
  import ProjectIconPicker from '../ProjectIconPicker.svelte';
  import { formatServiceError, renderMarkdownAction } from './helpers';

  interface Props {
    projects: BountyProject[];
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
    initialProjectId = null,
    editingBounty = null,
    desktop,
    mobile,
    onClose,
    onCreateProject,
    onCreateBounty,
    onUpdateDraft,
  }: Props = $props();

  const activeProjects = $derived(projects.filter(project => !project.archived));
  const editingOriginalProject = $derived(
    editingBounty ? (projects.find(project => project.id === editingBounty.project_id) ?? null) : null,
  );

  const initialSelectedProject = untrack(() => {
    if (editingBounty) {
      return projects.find(p => p.id === editingBounty.project_id) ?? null;
    }
    if (initialProjectId) {
      const matched = activeProjects.find(p => p.id === initialProjectId);
      if (matched) return matched;
    }
    return activeProjects[0] ?? null;
  });

  let selectedProjectId = $state(
    untrack(() => (editingBounty ? editingBounty.project_id : (initialSelectedProject?.id ?? ''))),
  );
  let title = $state(untrack(() => editingBounty?.title ?? ''));
  let description = $state(untrack(() => editingBounty?.description ?? ''));
  let lastAutoCriteria = $state(
    untrack(() => (editingBounty ? '' : (initialSelectedProject?.default_acceptance_criteria ?? ''))),
  );
  let acceptanceCriteria = $state(
    untrack(() =>
      editingBounty
        ? editingBounty.acceptance_criteria
        : (initialSelectedProject?.default_acceptance_criteria ?? ''),
    ),
  );
  let difficulty = $state<BountyDifficulty>(untrack(() => editingBounty?.difficulty ?? 'B'));
  let assigneeMode = $state<AssigneeMode>(untrack(() => editingBounty?.assignee_mode ?? 'any'));
  let assigneeMinTier = $state<BountyDifficulty>(
    untrack(() => editingBounty?.assignee_min_tier ?? 'B'),
  );
  let assigneeAgentId = $state(untrack(() => editingBounty?.assignee_agent_id ?? ''));
  let requireArbitration = $state(untrack(() => editingBounty?.require_arbitration ?? false));

  let descriptionTab = $state<'write' | 'preview'>('write');
  let criteriaTab = $state<'write' | 'preview'>('write');

  // 内联新建项目状态
  let inlineProjectOpen = $state(untrack(() => !editingBounty && activeProjects.length === 0));
  let inlineProjectName = $state('');
  let inlineProjectIcon = $state<string | null>(null);
  let inlineProjectWorkspace = $state('');
  let inlineIconPickerOpen = $state(false);
  let inlineProjectSubmitting = $state(false);
  let inlineProjectError = $state('');

  let submitting = $state(false);
  let formError = $state('');

  const selectedProject = $derived(
    projects.find(project => project.id === selectedProjectId) ?? null,
  );

  function handleSelectProject(nextProjectId: string, selectEl?: HTMLSelectElement): void {
    if (nextProjectId === '__create_new__') {
      inlineProjectOpen = true;
      inlineProjectError = '';
      if (selectEl) selectEl.value = selectedProjectId;
      return;
    }
    selectedProjectId = nextProjectId;
    formError = '';
    const targetProject = activeProjects.find(p => p.id === nextProjectId);
    if (!editingBounty && targetProject) {
      if (acceptanceCriteria.trim() === '' || acceptanceCriteria === lastAutoCriteria) {
        acceptanceCriteria = targetProject.default_acceptance_criteria;
        lastAutoCriteria = targetProject.default_acceptance_criteria;
      }
    }
  }

  async function handleBrowseInlineWorkspace(): Promise<void> {
    if (!desktop) return;
    try {
      const picked = await openDialog({
        directory: true,
        multiple: false,
      });
      if (typeof picked === 'string' && picked.trim()) {
        inlineProjectWorkspace = picked.trim();
      }
    } catch (error) {
      inlineProjectError = formatServiceError(error);
    }
  }

  async function handleCreateInlineProject(): Promise<void> {
    const trimmedName = inlineProjectName.trim();
    if (!trimmedName) {
      inlineProjectError = $t('请输入项目名称。');
      return;
    }
    inlineProjectError = '';
    inlineProjectSubmitting = true;
    try {
      const created = await onCreateProject({
        name: trimmedName,
        icon: inlineProjectIcon,
        workspace_path: inlineProjectWorkspace.trim(),
      });
      selectedProjectId = created.id;
      formError = '';
      if (!editingBounty && (acceptanceCriteria.trim() === '' || acceptanceCriteria === lastAutoCriteria)) {
        acceptanceCriteria = created.default_acceptance_criteria;
        lastAutoCriteria = created.default_acceptance_criteria;
      }
      inlineProjectName = '';
      inlineProjectIcon = null;
      inlineProjectWorkspace = '';
      inlineProjectOpen = false;
    } catch (error) {
      inlineProjectError = formatServiceError(error);
    } finally {
      inlineProjectSubmitting = false;
    }
  }

  function validateAndBuildFields(): {
    project_id: string;
    title: string;
    description: string;
    acceptance_criteria: string;
    difficulty: BountyDifficulty;
    assignee_mode: AssigneeMode;
    assignee_min_tier: BountyDifficulty | null;
    assignee_agent_id: string | null;
    require_arbitration: boolean;
  } | null {
    if (!selectedProjectId) {
      formError = $t('请先选择或新建一个有效项目。');
      return null;
    }
    const matchedProject = projects.find(p => p.id === selectedProjectId);
    if (matchedProject?.archived) {
      formError = $t('所属项目已归档，无法直接保存或发布。请在设置中恢复该项目或切换为其他活跃项目。');
      return null;
    }
    if (!matchedProject || !activeProjects.some(p => p.id === selectedProjectId)) {
      formError = $t('请先选择或新建一个有效项目。');
      return null;
    }
    const trimmedTitle = title.trim();
    if (!trimmedTitle) {
      formError = $t('请输入悬赏标题。');
      return null;
    }
    if (assigneeMode === 'specific' && !assigneeAgentId.trim()) {
      formError = $t('请输入指定的执行者 Agent 标识。');
      return null;
    }
    return {
      project_id: selectedProjectId,
      title: trimmedTitle,
      description,
      acceptance_criteria: acceptanceCriteria,
      difficulty,
      assignee_mode: assigneeMode,
      assignee_min_tier: assigneeMode === 'min_tier' ? assigneeMinTier : null,
      assignee_agent_id: assigneeMode === 'specific' ? assigneeAgentId.trim() : null,
      require_arbitration: requireArbitration,
    };
  }

  async function handleSubmit(targetStatus: 'DRAFT' | 'OPEN'): Promise<void> {
    if (submitting) return;
    formError = '';
    const fields = validateAndBuildFields();
    if (!fields) return;

    submitting = true;
    try {
      if (editingBounty) {
        await onUpdateDraft(editingBounty.id, fields, targetStatus === 'OPEN');
      } else {
        await onCreateBounty({
          ...fields,
          status: targetStatus,
        });
      }
      onClose();
    } catch (error) {
      formError = formatServiceError(error);
    } finally {
      submitting = false;
    }
  }

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (event.key !== 'Escape') return;
    event.preventDefault();
    event.stopPropagation();
    if (inlineIconPickerOpen) {
      inlineIconPickerOpen = false;
      return;
    }
    onClose();
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

<div
  class="modal-backdrop composer-backdrop"
  role="presentation"
  onclick={event => {
    if (event.target === event.currentTarget) onClose();
  }}
>
  <div
    class="modal-dialog bounty-composer-dialog"
    role="dialog"
    aria-modal="true"
    aria-label={editingBounty ? $t('编辑草稿') : $t('发布悬赏')}
    data-dialog="bounty-composer"
  >
    <div class="modal-header">
      <h2 class="modal-title">{editingBounty ? $t('编辑草稿') : $t('发布悬赏')}</h2>
      <button
        type="button"
        class="icon-button"
        aria-label={$t('关闭对话框')}
        onclick={onClose}
      >
        ✕
      </button>
    </div>

    <div class="composer-body">
      <div class="composer-field">
        <div class="composer-field-header">
          <label class="composer-label" for="bounty-project-select">{$t('所属项目')}</label>
          <button
            type="button"
            class="secondary-action"
            data-action="toggle-inline-project"
            onclick={() => {
              inlineProjectOpen = !inlineProjectOpen;
              inlineProjectError = '';
            }}
          >
            + {$t('新建项目')}
          </button>
        </div>

        <div class="composer-project-select-row">
          {#if selectedProject}
            <span class="composer-selected-project-icon">
              <ProjectIcon icon={selectedProject.icon ?? undefined} />
            </span>
          {/if}
          <select
            id="bounty-project-select"
            class="composer-select"
            aria-label={$t('所属项目')}
            value={selectedProjectId}
            onchange={event => {
              const selectEl = event.currentTarget as HTMLSelectElement;
              handleSelectProject(selectEl.value, selectEl);
            }}
          >
            {#if editingOriginalProject && editingOriginalProject.archived}
              <option value={editingOriginalProject.id}>
                {$t('{name}（已归档）', { name: editingOriginalProject.name })}
              </option>
            {:else if editingBounty && !editingOriginalProject && selectedProjectId}
              <option value={selectedProjectId}>{selectedProjectId}</option>
            {/if}
            {#if activeProjects.length === 0 && !(editingOriginalProject && editingOriginalProject.archived)}
              <option value="" disabled>{$t('暂无可用项目，请在下方新建项目')}</option>
            {:else}
              {#each activeProjects as project (project.id)}
                <option value={project.id}>{project.name}</option>
              {/each}
            {/if}
            <option value="__create_new__">+ {$t('新建项目')}</option>
          </select>
        </div>

        {#if selectedProject?.archived}
          <p class="composer-field-warning" role="status" data-status="archived-project-warning">
            {$t('当前草稿所属项目已归档，不能直接保存或发布。请在设置中恢复该项目，或手动切换至其他活跃项目。')}
          </p>
        {/if}

        {#if inlineProjectOpen}
          <div class="inline-project-card" data-slot="inline-project-creator">
            <div class="inline-project-header">
              <span class="inline-project-title">{$t('快速新建项目')}</span>
            </div>
            <div class="inline-project-row">
              <button
                type="button"
                class="project-badge-button"
                data-action="open-inline-icon-picker"
                aria-label={$t('选择项目图标')}
                onclick={() => {
                  inlineIconPickerOpen = true;
                }}
              >
                <ProjectIcon icon={inlineProjectIcon ?? undefined} />
                <span>{$t('选择图标')}</span>
              </button>
              <input
                id="inline-project-name"
                type="text"
                class="settings-text-input"
                aria-label={$t('项目名称')}
                placeholder={$t('项目名称')}
                bind:value={inlineProjectName}
              />
            </div>

            <div class="inline-project-row">
              <input
                id="inline-project-workspace"
                type="text"
                class="settings-text-input"
                aria-label={$t('工作目录路径')}
                placeholder={$t('关联本地工作目录路径（可选）')}
                bind:value={inlineProjectWorkspace}
              />
              {#if desktop}
                <button
                  type="button"
                  class="secondary-action"
                  data-action="browse-inline-workspace"
                  onclick={() => void handleBrowseInlineWorkspace()}
                >
                  {$t('浏览…')}
                </button>
              {/if}
            </div>

            {#if inlineProjectError}
              <p class="dialog-error" role="alert">{inlineProjectError}</p>
            {/if}

            <div class="inline-project-actions">
              <button
                type="button"
                class="secondary-action"
                onclick={() => {
                  inlineProjectOpen = false;
                  inlineProjectError = '';
                }}
              >
                {$t('取消')}
              </button>
              <button
                type="button"
                class="primary secondary-action"
                data-action="create-inline-project"
                disabled={inlineProjectSubmitting}
                onclick={() => void handleCreateInlineProject()}
              >
                {$t('创建并选用')}
              </button>
            </div>
          </div>
        {/if}
      </div>

      <div class="composer-field">
        <label class="composer-label" for="bounty-title-input">{$t('悬赏标题')}</label>
        <input
          id="bounty-title-input"
          type="text"
          class="settings-text-input"
          aria-label={$t('悬赏标题')}
          placeholder={$t('清晰描述需要完成的目标')}
          bind:value={title}
        />
      </div>

      <div class="composer-field">
        <div class="composer-field-header">
          <label class="composer-label" for="bounty-description-input">{$t('任务详情')}</label>
          <div class="filter-pills" role="group" aria-label={$t('任务详情视图')}>
            <button
              type="button"
              class="filter-pill"
              class:active={descriptionTab === 'write'}
              data-tab="description-write"
              onclick={() => {
                descriptionTab = 'write';
              }}
            >
              {$t('编写')}
            </button>
            <button
              type="button"
              class="filter-pill"
              class:active={descriptionTab === 'preview'}
              data-tab="description-preview"
              onclick={() => {
                descriptionTab = 'preview';
              }}
            >
              {$t('预览')}
            </button>
          </div>
        </div>
        {#if descriptionTab === 'write'}
          <textarea
            id="bounty-description-input"
            class="composer-textarea"
            rows="4"
            aria-label={$t('任务详情')}
            placeholder={$t('支持 Markdown 与数学公式…')}
            bind:value={description}
          ></textarea>
        {:else}
          <div
            class="bounty-markdown-preview"
            data-slot="composer-description-preview"
            use:renderMarkdownAction={description || $t('暂无内容')}
          ></div>
        {/if}
      </div>

      <div class="composer-field">
        <div class="composer-field-header">
          <label class="composer-label" for="bounty-criteria-input">{$t('验收条件')}</label>
          <div class="filter-pills" role="group" aria-label={$t('验收条件视图')}>
            <button
              type="button"
              class="filter-pill"
              class:active={criteriaTab === 'write'}
              data-tab="criteria-write"
              onclick={() => {
                criteriaTab = 'write';
              }}
            >
              {$t('编写')}
            </button>
            <button
              type="button"
              class="filter-pill"
              class:active={criteriaTab === 'preview'}
              data-tab="criteria-preview"
              onclick={() => {
                criteriaTab = 'preview';
              }}
            >
              {$t('预览')}
            </button>
          </div>
        </div>
        {#if criteriaTab === 'write'}
          <textarea
            id="bounty-criteria-input"
            class="composer-textarea"
            rows="4"
            aria-label={$t('验收条件')}
            placeholder={$t('列出明确的测试命令与验收标准（支持 Markdown 与 KaTeX 公式）…')}
            bind:value={acceptanceCriteria}
          ></textarea>
        {:else}
          <div
            class="bounty-markdown-preview"
            data-slot="composer-criteria-preview"
            use:renderMarkdownAction={acceptanceCriteria || $t('暂无内容')}
          ></div>
        {/if}
      </div>

      <div class="composer-grid-row">
        <div class="composer-field">
          <span class="composer-label">{$t('预估难度')}</span>
          <div class="segmented-group" role="group" aria-label={$t('预估难度')}>
            {#each ['C', 'B', 'A', 'S'] as const as tier (tier)}
              <button
                type="button"
                class="segmented-button"
                class:active={difficulty === tier}
                aria-pressed={difficulty === tier}
                data-difficulty={tier}
                onclick={() => {
                  difficulty = tier;
                }}
              >
                {tier}
              </button>
            {/each}
          </div>
        </div>

        <div class="composer-field">
          <span class="composer-label">{$t('执行者限制')}</span>
          <div class="segmented-group" role="group" aria-label={$t('执行者限制')}>
            <button
              type="button"
              class="segmented-button"
              class:active={assigneeMode === 'any'}
              aria-pressed={assigneeMode === 'any'}
              data-assignee-mode="any"
              onclick={() => {
                assigneeMode = 'any';
              }}
            >
              {$t('无要求')}
            </button>
            <button
              type="button"
              class="segmented-button"
              class:active={assigneeMode === 'min_tier'}
              aria-pressed={assigneeMode === 'min_tier'}
              data-assignee-mode="min_tier"
              onclick={() => {
                assigneeMode = 'min_tier';
              }}
            >
              {$t('最低段位')}
            </button>
            <button
              type="button"
              class="segmented-button"
              class:active={assigneeMode === 'specific'}
              aria-pressed={assigneeMode === 'specific'}
              data-assignee-mode="specific"
              onclick={() => {
                assigneeMode = 'specific';
              }}
            >
              {$t('指定执行者')}
            </button>
          </div>
        </div>
      </div>

      {#if assigneeMode === 'min_tier'}
        <div class="composer-field">
          <label class="composer-label" for="bounty-min-tier-select">{$t('最低段位要求')}</label>
          <select
            id="bounty-min-tier-select"
            class="composer-select"
            aria-label={$t('最低段位要求')}
            bind:value={assigneeMinTier}
          >
            {#each ['C', 'B', 'A', 'S'] as const as tier (tier)}
              <option value={tier}>{tier}</option>
            {/each}
          </select>
        </div>
      {:else if assigneeMode === 'specific'}
        <div class="composer-field">
          <label class="composer-label" for="bounty-assignee-agent-input">{$t('指定 Agent 标识')}</label>
          <input
            id="bounty-assignee-agent-input"
            type="text"
            class="settings-text-input"
            aria-label={$t('指定 Agent 标识')}
            placeholder={$t('例如：agent-core-1')}
            bind:value={assigneeAgentId}
          />
        </div>
      {/if}

      <div class="composer-switch-row">
        <label class="composer-label" for="bounty-require-arbitration">{$t('开启仲裁验收')}</label>
        <span class="settings-switch">
          <input
            id="bounty-require-arbitration"
            type="checkbox"
            role="switch"
            aria-label={$t('开启仲裁验收')}
            bind:checked={requireArbitration}
          />
          <span class="settings-switch-track" aria-hidden="true">
            <span class="settings-switch-thumb"></span>
          </span>
        </span>
      </div>

      {#if formError}
        <p class="dialog-error" role="alert" data-status="composer-error">{formError}</p>
      {/if}
    </div>

    <div class="modal-actions composer-footer">
      <button type="button" onclick={onClose}>{$t('取消')}</button>
      <button
        type="button"
        data-action="save-draft"
        disabled={submitting}
        onclick={() => void handleSubmit('DRAFT')}
      >
        {editingBounty ? $t('保存草稿') : $t('存为草稿')}
      </button>
      <button
        type="button"
        class="primary"
        data-action="publish-now"
        disabled={submitting}
        onclick={() => void handleSubmit('OPEN')}
      >
        {editingBounty ? $t('发布悬赏') : $t('立即发布')}
      </button>
    </div>
  </div>

  {#if inlineIconPickerOpen}
    <div
      class="modal-backdrop"
      role="presentation"
      onclick={event => {
        if (event.target === event.currentTarget) inlineIconPickerOpen = false;
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
              inlineIconPickerOpen = false;
            }}
          >
            ✕
          </button>
        </div>
        <ProjectIconPicker
          projectName={inlineProjectName.trim() || $t('新项目')}
          icon={inlineProjectIcon ?? undefined}
          {mobile}
          onSelect={(icon: ProjectIconValue) => {
            inlineProjectIcon = serializeProjectIcon(icon);
            inlineIconPickerOpen = false;
          }}
          onReset={() => {
            inlineProjectIcon = null;
            inlineIconPickerOpen = false;
          }}
        />
      </div>
    </div>
  {/if}
</div>
