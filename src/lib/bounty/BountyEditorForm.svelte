<script lang="ts">
  /**
   * 文件职责：共享的新建任务与草稿编辑双栏表单骨架（左写内容 + 右配参数 + 固定底栏）。
   * 定义范围：供发布弹窗（BountyComposerModal）与详情弹窗原地编辑（BountyDetailModal）复用，统一排版、校验与交互。
   */
  import { onMount, tick, untrack } from 'svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import type {
    AssigneeMode,
    BountyDependencySummary,
    BountyDifficulty,
    BountyProject,
    BountyTask,
    CreateBountyInput,
    CreateProjectInput,
    UpdateBountyInput,
  } from '../api/types';
  import { observeAnchoredPopover, positionAnchoredPopover } from '../anchored-popover';
  import { locale, t } from '../i18n';
  import { serializeProjectIcon, type ProjectIcon as ProjectIconValue } from '../project-icons';
  import ProjectIcon from '../ProjectIcon.svelte';
  import ProjectIconPicker from '../ProjectIconPicker.svelte';
  import { formatBountyStatus, formatServiceError, renderMarkdownAction } from './helpers';

  interface Props {
    projects: BountyProject[];
    availableDependencies?: BountyTask[];
    initialProjectId?: string | null;
    editingBounty?: BountyTask | null;
    desktop: boolean;
    mobile: boolean;
    onCancel: () => void;
    onSaved: () => void;
    onCreateProject: (input: CreateProjectInput) => Promise<BountyProject>;
    onCreateBounty?: (input: CreateBountyInput) => Promise<BountyTask>;
    onUpdateDraft?: (id: string, input: UpdateBountyInput, publishAfter: boolean) => Promise<BountyTask>;
  }

  let {
    projects,
    availableDependencies = [],
    initialProjectId = null,
    editingBounty = null,
    desktop,
    mobile,
    onCancel,
    onSaved,
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
  let blockedBy = $state<string[]>(untrack(() => [...(editingBounty?.blocked_by ?? [])]));

  const dependencyCandidates = $derived.by((): BountyDependencySummary[] => {
    const byId = new Map<string, BountyDependencySummary>();
    for (const item of availableDependencies) {
      if (item.id === editingBounty?.id) continue;
      if (item.status !== 'CANCELLED' || blockedBy.includes(item.id)) {
        byId.set(item.id, {
          id: item.id,
          title: item.title,
          status: item.status,
        });
      }
    }
    for (const id of blockedBy) {
      if (id === editingBounty?.id || byId.has(id)) continue;
      const summary = editingBounty?.blocked_by_summaries?.find(item => item.id === id);
      byId.set(
        id,
        summary
          ? { id: summary.id, title: summary.title, status: summary.status }
          : { id, title: id, status: 'CANCELLED' },
      );
    }
    return Array.from(byId.values());
  });

  const selectedDependencies = $derived.by((): BountyDependencySummary[] => {
    const byId = new Map(dependencyCandidates.map(item => [item.id, item]));
    return blockedBy
      .map(id => byId.get(id))
      .filter((item): item is BountyDependencySummary => Boolean(item));
  });

  let depSearchQuery = $state('');
  const filteredDependencyCandidates = $derived.by((): BountyDependencySummary[] => {
    const q = depSearchQuery.trim().toLowerCase();
    if (!q) return dependencyCandidates;
    return dependencyCandidates.filter(
      item => item.title.toLowerCase().includes(q) || item.id.toLowerCase().includes(q),
    );
  });

  let titleInputEl = $state<HTMLInputElement | undefined>(undefined);
  let projectTriggerEl = $state<HTMLButtonElement | undefined>(undefined);
  let projectMenuEl = $state<HTMLDivElement | undefined>(undefined);
  let projectMenuOpen = $state(false);
  let projectActiveIndex = $state(-1);
  let depHelpButton = $state<HTMLButtonElement | undefined>(undefined);
  let depHelpBubble = $state<HTMLDivElement | undefined>(undefined);
  let depHelpOpen = $state(false);
  let depPickerTrigger = $state<HTMLButtonElement | undefined>(undefined);
  let depPickerEl = $state<HTMLDivElement | undefined>(undefined);
  let depSearchInput = $state<HTMLInputElement | undefined>(undefined);
  let depPickerOpen = $state(false);

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

  interface ProjectSelectOption {
    value: string;
    label: string;
    icon: string | null;
    isCreateNew?: boolean;
  }

  const projectOptions = $derived.by<ProjectSelectOption[]>(() => {
    const _lang = $locale;
    const list: ProjectSelectOption[] = [];
    if (editingOriginalProject && editingOriginalProject.archived) {
      list.push({
        value: editingOriginalProject.id,
        label: $t('{name}（已归档）', { name: editingOriginalProject.name }),
        icon: editingOriginalProject.icon,
      });
    } else if (editingBounty && !editingOriginalProject && selectedProjectId) {
      list.push({
        value: selectedProjectId,
        label: selectedProjectId,
        icon: null,
      });
    }
    for (const p of activeProjects) {
      list.push({
        value: p.id,
        label: p.name,
        icon: p.icon,
      });
    }
    list.push({
      value: '__create_new__',
      label: `+ ${$t('新建项目')}`,
      icon: null,
      isCreateNew: true,
    });
    return list;
  });

  const selectedProjectLabel = $derived.by(() => {
    const _lang = $locale;
    const matched = projectOptions.find(opt => opt.value === selectedProjectId && !opt.isCreateNew);
    if (matched) return matched.label;
    return activeProjects.length === 0 ? $t('暂无可用项目，请在下方新建项目') : '';
  });

  function closeProjectMenu(): void {
    projectMenuOpen = false;
    if (
      typeof projectMenuEl?.hidePopover === 'function' &&
      projectMenuEl.matches(':popover-open')
    ) {
      projectMenuEl.hidePopover();
    }
  }

  async function openProjectMenu(initialIndex?: number): Promise<void> {
    if (projectOptions.length === 0) return;
    const selectedIdx = projectOptions.findIndex(opt => opt.value === selectedProjectId);
    projectActiveIndex = initialIndex ?? Math.max(0, selectedIdx);
    projectMenuOpen = true;
    await tick();
    if (!projectMenuOpen || !projectMenuEl?.isConnected) return;
    if (
      typeof projectMenuEl.showPopover === 'function' &&
      !projectMenuEl.matches(':popover-open')
    ) {
      projectMenuEl.showPopover();
    }
    if (projectTriggerEl) {
      positionAnchoredPopover(projectTriggerEl, projectMenuEl, closeProjectMenu);
    }
  }

  function commitProjectOption(index: number): void {
    const opt = projectOptions[index];
    if (!opt) return;
    closeProjectMenu();
    handleSelectProject(opt.value);
    projectTriggerEl?.focus();
  }

  function handleProjectTriggerKeydown(event: KeyboardEvent): void {
    if (event.isComposing) return;
    if (event.key === 'Tab') {
      closeProjectMenu();
      return;
    }
    if (event.key === 'Escape' && projectMenuOpen) {
      event.preventDefault();
      event.stopPropagation();
      closeProjectMenu();
      return;
    }
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
      event.preventDefault();
      if (!projectMenuOpen) {
        const idx =
          event.key === 'Home'
            ? 0
            : event.key === 'End'
              ? projectOptions.length - 1
              : undefined;
        void openProjectMenu(idx);
        return;
      }
      if (event.key === 'Home') projectActiveIndex = 0;
      else if (event.key === 'End') projectActiveIndex = projectOptions.length - 1;
      else {
        projectActiveIndex = Math.max(
          0,
          Math.min(
            projectOptions.length - 1,
            projectActiveIndex + (event.key === 'ArrowDown' ? 1 : -1),
          ),
        );
      }
      return;
    }
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      if (projectMenuOpen) commitProjectOption(projectActiveIndex);
      else void openProjectMenu();
    }
  }

  $effect(() => {
    if (!projectMenuOpen || !projectTriggerEl || !projectMenuEl) return;
    return observeAnchoredPopover(projectTriggerEl, projectMenuEl, closeProjectMenu);
  });

  onMount(() => {
    void tick().then(() => {
      if (titleInputEl && !inlineProjectOpen) {
        titleInputEl.focus();
      }
    });
    return () => {
      closeProjectMenu();
      closeDepHelp();
    };
  });

  function closeDepHelp(): void {
    depHelpOpen = false;
    if (
      typeof depHelpBubble?.hidePopover === 'function' &&
      depHelpBubble.matches(':popover-open')
    ) {
      depHelpBubble.hidePopover();
    }
  }

  function toggleDepHelp(): void {
    if (!depHelpButton || !depHelpBubble) return;
    depHelpOpen = !depHelpOpen;
    if (!depHelpOpen) {
      closeDepHelp();
      return;
    }
    if (
      typeof depHelpBubble.showPopover === 'function' &&
      !depHelpBubble.matches(':popover-open')
    ) {
      depHelpBubble.showPopover();
    }
    positionAnchoredPopover(depHelpButton, depHelpBubble, closeDepHelp, { align: 'start' });
  }

  $effect(() => {
    if (!depHelpOpen || !depHelpButton || !depHelpBubble) return;
    const stopObserve = observeAnchoredPopover(depHelpButton, depHelpBubble, closeDepHelp, {
      align: 'start',
    });
    const onKeydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        closeDepHelp();
      }
    };
    document.addEventListener('keydown', onKeydown, true);
    return () => {
      document.removeEventListener('keydown', onKeydown, true);
      stopObserve();
    };
  });

  $effect(() => {
    if (!depPickerOpen) return;
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (
        target &&
        !depPickerEl?.contains(target) &&
        !depPickerTrigger?.contains(target)
      ) {
        depPickerOpen = false;
      }
    };
    document.addEventListener('pointerdown', handlePointerDown, true);
    return () => document.removeEventListener('pointerdown', handlePointerDown, true);
  });

  export function handleEscapeKey(): boolean {
    if (projectMenuOpen) {
      closeProjectMenu();
      projectTriggerEl?.focus();
      return true;
    }
    if (depHelpOpen) {
      closeDepHelp();
      return true;
    }
    if (depPickerOpen) {
      depPickerOpen = false;
      depPickerTrigger?.focus();
      return true;
    }
    if (inlineIconPickerOpen) {
      inlineIconPickerOpen = false;
      return true;
    }
    return false;
  }

  function toggleDependency(depId: string): void {
    formError = '';
    if (blockedBy.includes(depId)) {
      blockedBy = blockedBy.filter(id => id !== depId);
    } else {
      blockedBy = [...blockedBy, depId];
    }
  }

  async function toggleDepPicker(): Promise<void> {
    depPickerOpen = !depPickerOpen;
    if (depPickerOpen) {
      await tick();
      depSearchInput?.focus();
    }
  }

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
    blocked_by: string[];
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
      formError = $t('请输入任务标题。');
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
      blocked_by: [...blockedBy],
    };
  }

  async function handleSubmit(targetStatus: 'DRAFT' | 'OPEN'): Promise<void> {
    if (submitting) return;
    formError = '';
    const fields = validateAndBuildFields();
    if (!fields) return;

    submitting = true;
    try {
      if (editingBounty && onUpdateDraft) {
        await onUpdateDraft(editingBounty.id, fields, targetStatus === 'OPEN');
      } else if (onCreateBounty) {
        await onCreateBounty({
          ...fields,
          status: targetStatus,
        });
      }
      onSaved();
    } catch (error) {
      formError = formatServiceError(error);
    } finally {
      submitting = false;
    }
  }
</script>

<div class="composer-columns">
  <div class="composer-main">
    <div class="composer-title-wrap">
      <input
        bind:this={titleInputEl}
        id="bounty-title-input"
        type="text"
        class="composer-title-input"
        aria-label={$t('任务标题')}
        placeholder={$t('输入任务标题…')}
        bind:value={title}
      />
    </div>

    <div class="composer-field composer-writing-section">
      <div class="composer-field-header">
        <label class="composer-label" for="bounty-description-input">{$t('任务详情')}</label>
        <div class="composer-mode-tabs" role="group" aria-label={$t('任务详情视图')}>
          <button
            type="button"
            class="composer-mode-tab"
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
            class="composer-mode-tab"
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
          rows="6"
          aria-label={$t('任务详情')}
          placeholder={$t('详细描述任务背景、目标与约束…')}
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

    <div class="composer-field composer-writing-section">
      <div class="composer-field-header">
        <label class="composer-label" for="bounty-criteria-input">{$t('验收条件')}</label>
        <div class="composer-mode-tabs" role="group" aria-label={$t('验收条件视图')}>
          <button
            type="button"
            class="composer-mode-tab"
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
            class="composer-mode-tab"
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
          rows="5"
          aria-label={$t('验收条件')}
          placeholder={$t('列出明确的测试命令与验收标准…')}
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

    <div class="composer-writing-hint">
      <span>Markdown / KaTeX</span>
    </div>
  </div>

  <aside class="composer-sidebar">
    <div class="composer-field">
      <div class="composer-field-header">
        <label class="composer-label" id="bounty-project-label" for="bounty-project-select">{$t('所属项目')}</label>
        <button
          type="button"
          class="composer-link-action"
          data-action="toggle-inline-project"
          onclick={() => {
            inlineProjectOpen = !inlineProjectOpen;
            inlineProjectError = '';
          }}
        >
          + {$t('新建项目')}
        </button>
      </div>

      <div class="composer-project-select-wrap">
        <button
          bind:this={projectTriggerEl}
          id="bounty-project-select"
          type="button"
          class="composer-project-trigger"
          role="combobox"
          data-value={selectedProjectId}
          aria-labelledby="bounty-project-label"
          aria-haspopup="listbox"
          aria-expanded={projectMenuOpen}
          aria-controls="bounty-project-listbox"
          aria-activedescendant={projectMenuOpen && projectActiveIndex >= 0
            ? `bounty-project-option-${projectActiveIndex}`
            : undefined}
          onclick={() => (projectMenuOpen ? closeProjectMenu() : void openProjectMenu())}
          onkeydown={handleProjectTriggerKeydown}
        >
          <span class="composer-project-trigger-main">
            <span class="composer-project-field-icon">
              <ProjectIcon icon={selectedProject?.icon ?? undefined} />
            </span>
            <span class="composer-project-trigger-label">{selectedProjectLabel}</span>
          </span>
          <svg
            class="composer-project-chevron"
            width="14"
            height="14"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="m4 6 4 4 4-4" />
          </svg>
        </button>

        <div
          bind:this={projectMenuEl}
          id="bounty-project-listbox"
          class="theme-select-popup dropdown composer-project-popup"
          role="listbox"
          aria-labelledby="bounty-project-label"
          popover="manual"
          hidden={!projectMenuOpen}
          style="position: fixed; margin: 0; bottom: auto; right: auto; box-sizing: border-box;"
        >
          {#each projectOptions as option, index (option.value)}
            <button
              id={`bounty-project-option-${index}`}
              type="button"
              role="option"
              tabindex="-1"
              class="theme-select-option composer-project-option"
              class:is-create-new={option.isCreateNew}
              aria-selected={!option.isCreateNew && option.value === selectedProjectId}
              data-active={index === projectActiveIndex}
              data-value={option.value}
              onpointermove={() => {
                projectActiveIndex = index;
              }}
              onpointerdown={event => event.preventDefault()}
              onclick={() => commitProjectOption(index)}
            >
              <span class="composer-project-option-main">
                {#if !option.isCreateNew}
                  <ProjectIcon icon={option.icon ?? undefined} />
                {/if}
                <span>{option.label}</span>
              </span>
              {#if !option.isCreateNew}
                <span class="theme-select-check" aria-hidden="true">
                  {option.value === selectedProjectId ? '✓' : ''}
                </span>
              {/if}
            </button>
          {/each}
        </div>
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
              class="inline-project-icon-button"
              data-action="open-inline-icon-picker"
              aria-label={$t('选择项目图标')}
              title={$t('选择项目图标')}
              onclick={() => {
                inlineIconPickerOpen = true;
              }}
            >
              <ProjectIcon icon={inlineProjectIcon ?? undefined} />
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
              class="composer-cancel-btn"
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
      <span class="composer-label">{$t('预估难度')}</span>
      <div class="segmented-group composer-tier-group" role="group" aria-label={$t('预估难度')}>
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
      <div class="segmented-group composer-assignee-group" role="group" aria-label={$t('执行者限制')}>
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
          {$t('指定')}
        </button>
      </div>

      {#if assigneeMode === 'min_tier'}
        <div class="composer-min-tier-row">
          <span class="composer-sublabel">{$t('最低段位要求')}</span>
          <div
            class="segmented-group segmented-compact"
            role="group"
            aria-label={$t('最低段位要求')}
          >
            {#each ['C', 'B', 'A', 'S'] as const as tier (tier)}
              <button
                type="button"
                class="segmented-button"
                class:active={assigneeMinTier === tier}
                aria-pressed={assigneeMinTier === tier}
                data-min-tier={tier}
                onclick={() => {
                  assigneeMinTier = tier;
                }}
              >
                {tier}
              </button>
            {/each}
          </div>
        </div>
      {:else if assigneeMode === 'specific'}
        <input
          id="bounty-assignee-agent-input"
          type="text"
          class="settings-text-input composer-compact-input"
          aria-label={$t('指定 Agent 标识')}
          placeholder={$t('例如：agent-core-1')}
          bind:value={assigneeAgentId}
        />
      {/if}
    </div>

    <div class="composer-field composer-dep-field" data-slot="composer-blocked-by">
      <div class="composer-field-header">
        <div class="settings-label-group">
          <span class="composer-label" id="composer-blocked-by-label">{$t('前置依赖任务')}</span>
          <span class="settings-help">
            <button
              bind:this={depHelpButton}
              type="button"
              class="settings-help-trigger"
              data-action="toggle-dependencies-help"
              aria-label={$t('关于前置依赖任务')}
              aria-expanded={depHelpOpen}
              aria-controls="composer-blocked-by-help"
              onclick={toggleDepHelp}
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 16 16"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
                aria-hidden="true"
              >
                <circle cx="8" cy="8" r="6.25" />
                <path d="M6.4 6.2a1.7 1.7 0 0 1 3.2.7c0 1.1-1.6 1.4-1.6 2.5" />
                <circle cx="8" cy="11.8" r=".75" fill="currentColor" stroke="none" />
              </svg>
            </button>
            <div
              bind:this={depHelpBubble}
              id="composer-blocked-by-help"
              class="settings-help-popover dropdown"
              role="note"
              popover="manual"
              hidden={!depHelpOpen}
              style="position: fixed; margin: 0; bottom: auto; right: auto; box-sizing: border-box;"
            >
              <p>{$t('选中任务全部完成后，当前任务才会向 Agent 开放抢单；保存时自动检测循环依赖。')}</p>
            </div>
          </span>
        </div>
        {#if blockedBy.length > 0}
          <span class="composer-meta-badge">{$t('已选 {count} 项', { count: blockedBy.length })}</span>
        {/if}
      </div>

      {#if dependencyCandidates.length === 0}
        <p class="composer-empty-hint">{$t('暂无可选前置任务')}</p>
      {:else}
        <div class="composer-dep-picker-wrap">
          <button
            bind:this={depPickerTrigger}
            type="button"
            class="composer-dep-trigger"
            data-action="toggle-dep-picker"
            aria-expanded={depPickerOpen}
            aria-controls="composer-dep-dropdown"
            onclick={() => void toggleDepPicker()}
          >
            <span>+ {$t('添加前置任务')}</span>
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="m3 4.5 3 3 3-3" />
            </svg>
          </button>

          <div
            bind:this={depPickerEl}
            id="composer-dep-dropdown"
            class="composer-dep-dropdown"
            role="group"
            aria-labelledby="composer-blocked-by-label"
            hidden={!depPickerOpen}
          >
            <input
              bind:this={depSearchInput}
              type="search"
              class="composer-dep-search"
              data-slot="dep-search-input"
              aria-label={$t('搜索前置任务')}
              placeholder={$t('搜索任务标题或编号…')}
              bind:value={depSearchQuery}
            />
            <div class="composer-dep-options">
              {#if filteredDependencyCandidates.length === 0}
                <p class="composer-dep-no-match">{$t('没有匹配的任务')}</p>
              {:else}
                {#each filteredDependencyCandidates as candidate (candidate.id)}
                  {@const candidateStatusLabel = (() => {
                    const _lang = $locale;
                    return formatBountyStatus(candidate.status, candidate.status === 'DRAFT');
                  })()}
                  <label
                    class="composer-dependency-option"
                    class:is-cancelled={candidate.status === 'CANCELLED'}
                    data-dependency-id={candidate.id}
                    data-dependency-status={candidate.status}
                  >
                    <input
                      type="checkbox"
                      checked={blockedBy.includes(candidate.id)}
                      onchange={() => toggleDependency(candidate.id)}
                    />
                    <span class="composer-dependency-title">{candidate.title}</span>
                    <span class="status-pill" data-status={candidate.status}>
                      {candidateStatusLabel}
                    </span>
                  </label>
                {/each}
              {/if}
            </div>
          </div>
        </div>

        {#if selectedDependencies.length > 0}
          <ul class="composer-selected-deps" data-slot="selected-dependencies-list">
            {#each selectedDependencies as dep (dep.id)}
              {@const depStatusLabel = (() => {
                const _lang = $locale;
                return formatBountyStatus(dep.status, dep.status === 'DRAFT');
              })()}
              <li
                class="composer-selected-dep-row"
                class:is-cancelled={dep.status === 'CANCELLED'}
                data-selected-dependency-id={dep.id}
                data-dependency-status={dep.status}
              >
                <span class="composer-selected-dep-title">{dep.title}</span>
                <span class="status-pill" data-status={dep.status}>{depStatusLabel}</span>
                <button
                  type="button"
                  class="composer-selected-dep-remove"
                  data-action="remove-dependency"
                  data-dependency-id={dep.id}
                  aria-label={$t('移除')}
                  onclick={() => toggleDependency(dep.id)}
                >
                  ✕
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      {/if}
    </div>

    <div class="composer-switch-row composer-arbitration-row">
      <label class="composer-label" for="bounty-require-arbitration">{$t('开启仲裁验收')}</label>
      <label class="settings-switch">
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
      </label>
    </div>

    {#if formError}
      <p class="dialog-error" role="alert" data-status="composer-error">{formError}</p>
    {/if}
  </aside>
</div>

<div class="modal-actions composer-footer">
  <button type="button" class="composer-cancel-btn" onclick={onCancel}>{$t('取消')}</button>
  <button
    type="button"
    class="composer-draft-btn"
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
    {editingBounty ? $t('发布任务') : $t('立即发布')}
  </button>
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
      <button
        type="button"
        class="icon-button icon-picker-close"
        aria-label={$t('关闭对话框')}
        onclick={() => {
          inlineIconPickerOpen = false;
        }}
      >
        ✕
      </button>
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
