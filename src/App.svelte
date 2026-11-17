<script lang="ts">
  /**
   * 文件职责：承载 Taskrail 悬赏板工作台外壳（左侧四入口紧凑导航 + 右侧多视图主工作区 + 详情抽屉与发布弹窗）。
   * 定义范围：顶层导航切换、同构悬赏服务连接与筛选、项目管理、主题与原生窗口材质协调、配置持久化及应用更新面板。
   */
  import { onDestroy, onMount, tick, untrack } from 'svelte';
  import { getVersion } from '@tauri-apps/api/app';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import {
    createMemoryBountyClient,
    fetchEmbeddedServerInfo,
    HttpBountyClient,
    type BountyApiPort,
  } from './lib/api/bounty-client';
  import type {
    AgentActivity,
    AgentFilter,
    AgentStatus,
    AgentSummary,
    BountyDetail,
    BountyProject,
    BountyQuery,
    BountyStatus,
    BountyTask,
    CreatedInviteToken,
    CreateBountyInput,
    CreateInviteTokenInput,
    CreateProjectInput,
    InviteTokenSummary,
    OnboardingSettings,
    ServiceMode,
    SkillDocument,
    UpdateBountyInput,
    UpdateProjectInput,
  } from './lib/api/types';
  import AgentDirectory from './lib/bounty/AgentDirectory.svelte';
  import BountyCard from './lib/bounty/BountyCard.svelte';
  import BountyComposerModal from './lib/bounty/BountyComposerModal.svelte';
  import BountyDetailDrawer from './lib/bounty/BountyDetailDrawer.svelte';
  import { formatServiceError, renderMarkdownAction } from './lib/bounty/helpers';
  import { BrowserFilePort } from './lib/browser-files';
  import { defaultPreferences, defaultThemeId, emptyAppConfig } from './lib/config-defaults';
  import type {
    AppConfig,
    FilePort,
    Preferences,
    Project,
    WindowPlacement,
    WorkspaceSection,
  } from './lib/contracts';
  import { errorMessage } from './lib/errors';
  import { AndroidFilePort } from './lib/files/android';
  import { TauriFilePort } from './lib/files/tauri';
  import { locale, setLocalePreference, t, type LocalePreference } from './lib/i18n';
  import { registerDrawerGestures } from './lib/mobile-navigation';
  import { platformCapabilities } from './lib/platform';
  import {
    parseProjectIcon,
    serializeProjectIcon,
    type ProjectIcon as ProjectIconValue,
  } from './lib/project-icons';
  import ProjectIcon from './lib/ProjectIcon.svelte';
  import ProjectIconPicker from './lib/ProjectIconPicker.svelte';
  import SettingsPanel from './lib/SettingsPanel.svelte';
  import {
    applyTheme,
    bootThemeKey,
    builtInThemes,
    parseTheme,
    type ThemeDefinition,
    type ThemeMode,
  } from './lib/themes';
  import UpdatePanel from './lib/UpdatePanel.svelte';
  import { AndroidUpdatePort } from './lib/updater/android';
  import type { UpdatePort, UpdateStatus } from './lib/updater/contracts';
  import { TauriUpdatePort } from './lib/updater/tauri';
  import { UpdateCoordinator } from './lib/updater/update-coordinator';
  import { WindowMaterialController, type WindowMaterial } from './lib/window-material';
  import WindowControls from './lib/WindowControls.svelte';

  type ApplyWindowMaterial = (
    material: WindowMaterial,
    theme: 'light' | 'dark' | 'system',
    keepTransparentOnBlur: boolean,
  ) => Promise<boolean>;

  interface Props {
    files?: FilePort;
    updater?: UpdatePort;
    setWindowMaterial?: ApplyWindowMaterial;
    bountyApi?: BountyApiPort;
    onready?: () => void;
  }

  let {
    files: injectedFiles,
    updater: injectedUpdater,
    setWindowMaterial,
    bountyApi: injectedBountyApi,
    onready,
  }: Props = $props();

  const capabilities = platformCapabilities();
  const initialFiles = untrack(() => injectedFiles);
  const initialUpdater = untrack(() => injectedUpdater);
  const initialSetWindowMaterial = untrack(() => setWindowMaterial);
  const initialBountyApi = untrack(() => injectedBountyApi);

  const files: FilePort =
    initialFiles ??
    (capabilities.desktop
      ? new TauriFilePort()
      : capabilities.android
        ? new AndroidFilePort()
        : new BrowserFilePort());
  const updateSupported = Boolean(initialUpdater) || capabilities.native;
  const updatePort: UpdatePort =
    initialUpdater ??
    (capabilities.desktop
      ? new TauriUpdatePort()
      : capabilities.android
        ? new AndroidUpdatePort()
        : {
            check: async () => null,
          });

  const applyNativeWindowMaterial: ApplyWindowMaterial =
    initialSetWindowMaterial ??
    (capabilities.desktop
      ? (material, theme, keepTransparentOnBlur) =>
          invoke<boolean>('set_window_material', { material, theme, keepTransparentOnBlur })
      : async () => false);

  const fallbackMemoryBountyClient = createMemoryBountyClient();
  let activeBountyApi = $state.raw<BountyApiPort>(initialBountyApi ?? fallbackMemoryBountyClient);
  let serviceConnected = $state(true);
  let serviceUrl = $state('');
  let serviceEpoch = $state(0);
  let serviceConnectionError = $state('');

  let loading = $state(true);
  let booted = $state(false);
  let configResolved = $state(false);
  let configLoadFailed = $state(false);
  let configLoadError = $state('');
  let projects = $state<Project[]>([]);
  let activeProjectId = $state<string | null>(null);
  let customThemes = $state<ThemeDefinition[]>([]);
  let preferences = $state<Preferences>({ ...defaultPreferences });
  let windowPlacement = $state<WindowPlacement | undefined>(undefined);
  let activeSection = $state<WorkspaceSection>(defaultPreferences.workspaceSection ?? 'bounties');

  // 悬赏板、项目与 Agent 准入/名录状态
  let allBountyProjects = $state<BountyProject[]>([]);
  let showArchivedProjects = $state(false);
  let bountyProjectFilter = $state<string>('ALL');
  let bountyStatusFilter = $state<'ALL' | BountyStatus>('ALL');
  let bountySearchQuery = $state('');
  let bounties = $state<BountyTask[]>([]);
  let actionableBounties = $state<BountyTask[]>([]);
  let inboxFilter = $state<'ALL' | 'DRAFT' | 'WAITING_INFO' | 'PENDING_ARBITRATION'>('ALL');

  let onboardingSettings = $state<OnboardingSettings>({
    lan_open_registration: false,
    invite_tokens: [],
  });
  let onboardingLoaded = $state(false);
  let onboardingLoadError = $state('');
  let agents = $state<AgentSummary[]>([]);
  let agentsLoaded = $state(false);
  let agentsLoadError = $state('');
  let agentActivityFilter = $state<'all' | AgentActivity>('all');
  let agentStatusFilter = $state<'ALL' | AgentStatus>('ALL');
  let skillGuideOpen = $state(false);
  let skillDocument = $state<SkillDocument | null>(null);

  let composerOpen = $state(false);
  let editingBounty = $state<BountyTask | null>(null);
  let activeDetail = $state<BountyDetail | null>(null);

  let iconPickerOpen = $state(false);
  let updateDialogOpen = $state(false);
  let mobileSidebarOpen = $state(false);

  let themeError = $state('');
  let importingTheme = $state(false);
  let configSaveFailed = $state(false);
  let toast = $state('');
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  let currentVersion = $state('0.8.8');
  let updateStatus = $state<UpdateStatus>({ kind: 'idle' });
  let systemDark = $state(
    typeof window !== 'undefined' && typeof window.matchMedia === 'function'
      ? window.matchMedia('(prefers-color-scheme: dark)').matches
      : false,
  );

  const windowMaterial =
    typeof document !== 'undefined'
      ? new WindowMaterialController(document.documentElement, applyNativeWindowMaterial)
      : null;

  const allThemes = $derived([...builtInThemes, ...customThemes]);
  const activeTheme = $derived(
    allThemes.find(item => item.id === preferences.themeId) ?? builtInThemes[0],
  );
  const desktopMaterialSupported =
    Boolean(initialSetWindowMaterial) || (capabilities.desktop && !files.isBrowser && !files.isMobile);

  const activeBountyProjects = $derived(allBountyProjects.filter(project => !project.archived));
  const settingsBountyProjects = $derived(
    showArchivedProjects ? allBountyProjects : activeBountyProjects,
  );
  const bountyProjectsById = $derived(
    new Map(allBountyProjects.map(project => [project.id, project])),
  );

  const activeProject = $derived(
    projects.length > 0
      ? (projects.find(project => project.id === activeProjectId) ?? projects[0])
      : null,
  );
  const selectedBountyProject = $derived(
    bountyProjectFilter !== 'ALL' ? (bountyProjectsById.get(bountyProjectFilter) ?? null) : null,
  );
  const currentProjectIcon = $derived(
    activeProject
      ? activeProject.icon
      : selectedBountyProject?.icon
        ? selectedBountyProject.icon
        : preferences.defaultProjectIcon,
  );
  const sidebarCollapsed = $derived(
    preferences.sidebarOpen !== undefined
      ? !preferences.sidebarOpen
      : Boolean(preferences.sidebarCollapsed),
  );

  const visibleInboxBounties = $derived(
    actionableBounties.filter(item =>
      inboxFilter === 'ALL' ? true : item.status === inboxFilter,
    ),
  );

  function resolveWindowMaterial(
    theme: ThemeDefinition,
    mode: ThemeMode,
    dark: boolean,
  ): WindowMaterial {
    if (theme.monochrome) return 'opaque';
    const resolvedMode = mode === 'system' ? (dark ? 'dark' : 'light') : mode;
    const candidate = theme.appearance?.[resolvedMode]?.['window-material'];
    return candidate === 'acrylic' || candidate === 'blur' || candidate === 'transparent'
      ? candidate
      : 'opaque';
  }

  function themeSupportsBlur(theme: ThemeDefinition): boolean {
    if (theme.monochrome) return false;
    const lightMaterial = theme.appearance?.light['window-material'];
    const darkMaterial = theme.appearance?.dark['window-material'];
    return (
      lightMaterial === 'acrylic' ||
      lightMaterial === 'blur' ||
      lightMaterial === 'transparent' ||
      darkMaterial === 'acrylic' ||
      darkMaterial === 'blur' ||
      darkMaterial === 'transparent'
    );
  }

  function createThemeTemplate(source: ThemeDefinition): ThemeDefinition {
    const candidateId = `${source.id}-copy`.slice(0, 64);
    return {
      ...source,
      id: candidateId,
      name: `${source.name} Copy`,
      light: { ...source.light },
      dark: { ...source.dark },
      ...(source.appearance
        ? {
            appearance: {
              light: { ...source.appearance.light },
              dark: { ...source.appearance.dark },
            },
          }
        : {}),
    };
  }

  const sectionMeta = $derived.by(() => {
    const _lang = $locale;
    switch (activeSection) {
      case 'bounties':
        return {
          title: $t('悬赏板'),
          subtitle: $t('悬赏大厅与项目工作流'),
          description: $t('集中浏览公开悬赏、筛选项目与状态流转，跟进 Agent 认领、交付与验收。'),
        };
      case 'inbox':
        return {
          title: $t('待我处理'),
          subtitle: $t('待补充信息与验收裁决'),
          description: $t('聚合需要人工介入或我方响应的阻塞悬赏、待验收交付与超时预警。'),
        };
      case 'agents':
        return {
          title: $t('Agent 名录'),
          subtitle: $t('已注册 Agent 与能力档案'),
          description: $t('查看接入悬赏板的外部 Agent 身份、擅长领域、当前租约负载与历史交付记录。'),
        };
      case 'settings':
        return {
          title: $t('设置'),
          subtitle: $t('工作台外观、排版与扩展配置'),
          description: $t('管理界面语言、内置与自制主题、原生玻璃材质、阅读排版及工作台扩展分组。'),
        };
    }
  });

  const sampleCriteriaMarkdown = $derived(
    $locale === 'en'
      ? '- [x] Passes `cargo test` & `npm test`\n- [ ] Complexity bound $O(n \\log n)$ verified'
      : '- [x] 通过 `cargo test` 与 `npm test`\n- [ ] 验证复杂度上界 $O(n \\log n)$',
  );

  function showToast(message: string): void {
    toast = message;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toast = '';
    }, 4000);
  }

  function snapshotConfig(preferencesPatch?: Partial<Preferences>): AppConfig {
    return $state.snapshot({
      projects,
      activeProjectId,
      customThemes,
      preferences: {
        ...preferences,
        ...(preferencesPatch ?? {}),
        workspaceSection: activeSection,
      },
      ...(windowPlacement ? { window: windowPlacement } : {}),
    });
  }

  let lastConfigSaveError = '';
  let configSaveChain: Promise<void> = Promise.resolve();
  async function persistConfig(preferencesPatch?: Partial<Preferences>): Promise<boolean> {
    if (configLoadFailed) {
      lastConfigSaveError = configLoadError || $t('配置尚未加载成功，无法保存配置。');
      return false;
    }
    const task = configSaveChain.then(async () => {
      const config = snapshotConfig(preferencesPatch);
      await files.saveConfig(config);
      if (preferencesPatch) {
        preferences = { ...preferences, ...preferencesPatch };
      }
    });
    configSaveChain = task.catch(() => {});
    try {
      await task;
      configSaveFailed = false;
      lastConfigSaveError = '';
      return true;
    } catch (error) {
      configSaveFailed = true;
      lastConfigSaveError = errorMessage(error);
      showToast($t('配置保存失败：{error}', { error: lastConfigSaveError }));
      return false;
    }
  }

  const REALTIME_SYNC_INTERVAL_MS = 2000;
  let realtimeSyncTimer: ReturnType<typeof setInterval> | null = null;
  let realtimeSyncInFlight = false;
  let componentDisposed = false;
  let bountyListInFlightCount = 0;
  let agentListInFlightCount = 0;
  let detailInFlightCount = 0;

  let bountyListRequestSeq = 0;
  let workspaceRequestSeq = 0;
  let detailRequestSeq = 0;
  let agentListRequestSeq = 0;
  let onboardingRequestSeq = 0;
  let skillDocRequestSeq = 0;
  let serviceReconnectSeq = 0;
  let connectionStatusSeq = 0;

  function buildBountyQuery(): BountyQuery {
    const query: BountyQuery = {};
    if (bountyProjectFilter !== 'ALL') {
      query.project_id = bountyProjectFilter;
    }
    if (bountyStatusFilter !== 'ALL') {
      query.status = bountyStatusFilter;
    }
    const trimmedQ = bountySearchQuery.trim();
    if (trimmedQ) {
      query.q = trimmedQ;
    }
    if (bountyStatusFilter === 'ALL' || bountyStatusFilter === 'DRAFT') {
      query.include_drafts = true;
    }
    return query;
  }

  function buildAgentFilter(): AgentFilter {
    const filter: AgentFilter = {};
    if (agentStatusFilter !== 'ALL') {
      filter.status = agentStatusFilter;
    }
    if (agentActivityFilter !== 'all') {
      filter.activity = agentActivityFilter;
    }
    return filter;
  }

  async function refreshBountiesOnly(): Promise<void> {
    const statusSeq = ++connectionStatusSeq;
    const listSeq = ++bountyListRequestSeq;
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    const query = buildBountyQuery();
    bountyListInFlightCount += 1;
    try {
      const [filteredList, fullList] = await Promise.all([
        api.listBounties(query),
        api.listBounties({ include_drafts: true }),
      ]);
      if (serviceSeq !== serviceReconnectSeq || api !== activeBountyApi) {
        return;
      }
      if (listSeq === bountyListRequestSeq) {
        bounties = filteredList;
        actionableBounties = fullList.filter(
          item =>
            item.status === 'DRAFT' ||
            item.status === 'WAITING_INFO' ||
            item.status === 'PENDING_ARBITRATION',
        );
      }
      if (statusSeq === connectionStatusSeq) {
        serviceConnected = true;
        serviceConnectionError = '';
      }
    } catch (error) {
      if (
        statusSeq === connectionStatusSeq &&
        serviceSeq === serviceReconnectSeq &&
        api === activeBountyApi
      ) {
        serviceConnected = false;
        serviceConnectionError = formatServiceError(error);
      }
    } finally {
      bountyListInFlightCount -= 1;
    }
  }

  function normalizeOnboardingSettings(raw: unknown): OnboardingSettings {
    if (raw && typeof raw === 'object') {
      const candidate = raw as Partial<OnboardingSettings>;
      return {
        lan_open_registration: Boolean(candidate.lan_open_registration),
        invite_tokens: Array.isArray(candidate.invite_tokens) ? candidate.invite_tokens : [],
      };
    }
    return { lan_open_registration: false, invite_tokens: [] };
  }

  async function refreshAgentsOnly(): Promise<void> {
    const agentsSeq = ++agentListRequestSeq;
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    const filter = buildAgentFilter();
    agentListInFlightCount += 1;
    try {
      const list = await api.listAgents(filter);
      if (
        agentsSeq !== agentListRequestSeq ||
        serviceSeq !== serviceReconnectSeq ||
        api !== activeBountyApi
      ) {
        return;
      }
      agents = Array.isArray(list) ? list : [];
      agentsLoaded = true;
      agentsLoadError = '';
    } catch (error) {
      if (
        agentsSeq === agentListRequestSeq &&
        serviceSeq === serviceReconnectSeq &&
        api === activeBountyApi
      ) {
        agentsLoadError = formatServiceError(error);
      }
    } finally {
      agentListInFlightCount -= 1;
    }
  }

  async function refreshOnboarding(): Promise<OnboardingSettings | null> {
    const onbSeq = ++onboardingRequestSeq;
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    try {
      const settings = normalizeOnboardingSettings(await api.getOnboardingSettings());
      if (
        onbSeq !== onboardingRequestSeq ||
        serviceSeq !== serviceReconnectSeq ||
        api !== activeBountyApi
      ) {
        return null;
      }
      onboardingSettings = settings;
      onboardingLoaded = true;
      onboardingLoadError = '';
      return settings;
    } catch (error) {
      if (
        onbSeq === onboardingRequestSeq &&
        serviceSeq === serviceReconnectSeq &&
        api === activeBountyApi
      ) {
        onboardingLoadError = formatServiceError(error);
      }
      return null;
    }
  }

  async function loadSkillMarkdown(options?: {
    baseUrl?: string;
    inviteToken?: string;
  }): Promise<SkillDocument> {
    const skillSeq = ++skillDocRequestSeq;
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    const resolvedBaseUrl = options?.baseUrl?.trim() || serviceUrl.trim() || undefined;
    try {
      const doc = await api.getSkillMarkdown({
        ...(resolvedBaseUrl ? { baseUrl: resolvedBaseUrl } : {}),
        ...(options?.inviteToken ? { inviteToken: options.inviteToken } : {}),
      });
      if (
        skillSeq === skillDocRequestSeq &&
        serviceSeq === serviceReconnectSeq &&
        api === activeBountyApi
      ) {
        skillDocument = doc;
      }
      return doc;
    } catch (error) {
      if (
        skillSeq === skillDocRequestSeq &&
        serviceSeq === serviceReconnectSeq &&
        api === activeBountyApi
      ) {
        skillDocument = null;
      }
      throw error;
    }
  }

  async function refreshBountyWorkspace(
    throwOnError = false,
    inheritedStatusSeq?: number,
  ): Promise<void> {
    const workspaceSeq = ++workspaceRequestSeq;
    const listSeq = ++bountyListRequestSeq;
    const agentsSeq = ++agentListRequestSeq;
    const onbSeq = ++onboardingRequestSeq;
    const statusSeq = inheritedStatusSeq ?? ++connectionStatusSeq;
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    const query = buildBountyQuery();
    const agentFilterQuery = buildAgentFilter();
    bountyListInFlightCount += 1;
    agentListInFlightCount += 1;
    try {
      const [projectsList, filteredList, fullList, onboardingSettled, agentsSettled] =
        await Promise.all([
          api.listProjects(true),
          api.listBounties(query),
          api.listBounties({ include_drafts: true }),
          typeof api.getOnboardingSettings === 'function'
            ? Promise.allSettled([api.getOnboardingSettings()]).then(([result]) => result)
            : Promise.resolve({
                status: 'fulfilled' as const,
                value: { lan_open_registration: false, invite_tokens: [] } satisfies OnboardingSettings,
              }),
          typeof api.listAgents === 'function'
            ? Promise.allSettled([api.listAgents(agentFilterQuery)]).then(([result]) => result)
            : Promise.resolve({
                status: 'fulfilled' as const,
                value: [] satisfies AgentSummary[],
              }),
        ]);
      if (
        workspaceSeq !== workspaceRequestSeq ||
        serviceSeq !== serviceReconnectSeq ||
        api !== activeBountyApi
      ) {
        return;
      }
      allBountyProjects = projectsList;
      if (
        bountyProjectFilter !== 'ALL' &&
        !projectsList.some(p => p.id === bountyProjectFilter && !p.archived)
      ) {
        bountyProjectFilter = 'ALL';
      }
      if (listSeq === bountyListRequestSeq) {
        bounties = filteredList;
        actionableBounties = fullList.filter(
          item =>
            item.status === 'DRAFT' ||
            item.status === 'WAITING_INFO' ||
            item.status === 'PENDING_ARBITRATION',
        );
      }
      if (
        onbSeq === onboardingRequestSeq &&
        serviceSeq === serviceReconnectSeq &&
        api === activeBountyApi
      ) {
        if (onboardingSettled.status === 'fulfilled') {
          onboardingSettings = normalizeOnboardingSettings(onboardingSettled.value);
          onboardingLoaded = true;
          onboardingLoadError = '';
        } else {
          onboardingLoadError = formatServiceError(onboardingSettled.reason);
        }
      }
      if (
        agentsSeq === agentListRequestSeq &&
        serviceSeq === serviceReconnectSeq &&
        api === activeBountyApi
      ) {
        if (agentsSettled.status === 'fulfilled') {
          agents = Array.isArray(agentsSettled.value) ? agentsSettled.value : [];
          agentsLoaded = true;
          agentsLoadError = '';
        } else {
          agentsLoadError = formatServiceError(agentsSettled.reason);
        }
      }
      if (statusSeq === connectionStatusSeq) {
        serviceConnected = true;
        serviceConnectionError = '';
      }
    } catch (error) {
      if (
        statusSeq === connectionStatusSeq &&
        workspaceSeq === workspaceRequestSeq &&
        serviceSeq === serviceReconnectSeq &&
        api === activeBountyApi
      ) {
        serviceConnected = false;
        serviceConnectionError = formatServiceError(error);
      }
      if (throwOnError) throw error;
    } finally {
      bountyListInFlightCount -= 1;
      agentListInFlightCount -= 1;
    }
  }

  /**
   * 后台静默同步当前悬赏列表、Agent 名录与已打开详情抽屉，响应外部 Agent 抢单、续租与交付状态变更。
   * 仅在页面可见、服务已连接且无重叠后台轮询时执行；某子项已有前台请求在途时跳过该子项，防止与前台操作竞争序列号。
   */
  async function syncRealtimeWorkspace(): Promise<void> {
    if (
      componentDisposed ||
      loading ||
      !serviceConnected ||
      realtimeSyncInFlight ||
      (typeof document !== 'undefined' && document.visibilityState === 'hidden')
    ) {
      return;
    }
    realtimeSyncInFlight = true;
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    const tasks: Promise<void>[] = [];

    if (bountyListInFlightCount === 0) {
      const listSeq = ++bountyListRequestSeq;
      const query = buildBountyQuery();
      tasks.push(
        Promise.all([
          api.listBounties(query),
          api.listBounties({ include_drafts: true }),
        ]).then(([filteredList, fullList]) => {
          if (
            componentDisposed ||
            serviceSeq !== serviceReconnectSeq ||
            api !== activeBountyApi ||
            listSeq !== bountyListRequestSeq
          ) {
            return;
          }
          bounties = filteredList;
          actionableBounties = fullList.filter(
            item =>
              item.status === 'DRAFT' ||
              item.status === 'WAITING_INFO' ||
              item.status === 'PENDING_ARBITRATION',
          );
        }),
      );
    }

    if (
      agentListInFlightCount === 0 &&
      typeof api.listAgents === 'function' &&
      !agentsLoadError
    ) {
      const agentsSeq = ++agentListRequestSeq;
      const agentFilter = buildAgentFilter();
      tasks.push(
        api.listAgents(agentFilter).then(list => {
          if (
            componentDisposed ||
            serviceSeq !== serviceReconnectSeq ||
            api !== activeBountyApi ||
            agentsSeq !== agentListRequestSeq ||
            agentsLoadError
          ) {
            return;
          }
          agents = Array.isArray(list) ? list : [];
          agentsLoaded = true;
        }),
      );
    }

    if (detailInFlightCount === 0 && activeDetail !== null) {
      const detailBountyId = activeDetail.bounty.id;
      const detailSeq = ++detailRequestSeq;
      tasks.push(
        api.getBountyDetail(detailBountyId).then(refreshedDetail => {
          if (
            componentDisposed ||
            serviceSeq !== serviceReconnectSeq ||
            api !== activeBountyApi ||
            detailSeq !== detailRequestSeq ||
            activeDetail?.bounty.id !== detailBountyId
          ) {
            return;
          }
          activeDetail = refreshedDetail;
        }),
      );
    }

    try {
      await Promise.allSettled(tasks);
    } finally {
      realtimeSyncInFlight = false;
    }
  }

  async function resolveBountyApiInstance(
    prefs: Preferences,
    preferInjected: boolean,
  ): Promise<{ api: BountyApiPort; url: string }> {
    if (preferInjected && initialBountyApi) {
      return { api: initialBountyApi, url: '' };
    }
    if (prefs.serviceMode === 'remote' && prefs.remoteServerUrl?.trim()) {
      const client = new HttpBountyClient(
        prefs.remoteServerUrl,
        prefs.remoteAdminToken ?? '',
      );
      return { api: client, url: client.baseUrl };
    }
    if (initialBountyApi) {
      return { api: initialBountyApi, url: '' };
    }
    if (capabilities.desktop) {
      const info = await fetchEmbeddedServerInfo();
      const client = new HttpBountyClient(info.base_url, info.admin_token);
      return { api: client, url: client.baseUrl };
    }
    return { api: fallbackMemoryBountyClient, url: '' };
  }

  async function reconnectBountyService(preferInjected = false, throwOnError = false): Promise<void> {
    const serviceSeq = ++serviceReconnectSeq;
    let statusSeq = ++connectionStatusSeq;
    bountyListInFlightCount += 1;
    agentListInFlightCount += 1;
    detailInFlightCount += 1;
    try {
      const resolved = await resolveBountyApiInstance(preferences, preferInjected);
      if (serviceSeq !== serviceReconnectSeq) {
        return;
      }
      const switchingClient = resolved.api !== activeBountyApi || resolved.url !== serviceUrl;
      activeBountyApi = resolved.api;
      serviceUrl = resolved.url;
      skillDocument = null;
      skillDocRequestSeq += 1;
      if (switchingClient) {
        serviceEpoch += 1;
        bountyListRequestSeq += 1;
        workspaceRequestSeq += 1;
        detailRequestSeq += 1;
        agentListRequestSeq += 1;
        onboardingRequestSeq += 1;
        allBountyProjects = [];
        bounties = [];
        actionableBounties = [];
        onboardingSettings = { lan_open_registration: false, invite_tokens: [] };
        onboardingLoaded = false;
        onboardingLoadError = '';
        agents = [];
        agentsLoaded = false;
        agentsLoadError = '';
        activeDetail = null;
      }
      statusSeq = ++connectionStatusSeq;
      await refreshBountyWorkspace(true, statusSeq);
    } catch (error) {
      if (serviceSeq === serviceReconnectSeq && statusSeq === connectionStatusSeq) {
        serviceConnected = false;
        serviceConnectionError = formatServiceError(error);
      }
      if (throwOnError) throw error;
    } finally {
      bountyListInFlightCount -= 1;
      agentListInFlightCount -= 1;
      detailInFlightCount -= 1;
    }
  }

  async function handleSaveServiceConnection(
    mode: ServiceMode,
    remoteServerUrl: string,
    remoteAdminToken: string,
  ): Promise<void> {
    const persisted = await persistConfig({
      serviceMode: mode,
      remoteServerUrl,
      remoteAdminToken,
    });
    if (!persisted) {
      throw new Error(lastConfigSaveError || $t('配置保存失败，未应用服务连接更改。'));
    }
    await reconnectBountyService(false, true);
  }

  async function handleCreateBountyProject(input: CreateProjectInput): Promise<BountyProject> {
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    const created = await api.createProject(input);
    if (serviceSeq === serviceReconnectSeq && api === activeBountyApi) {
      await refreshBountyWorkspace();
    }
    return created;
  }

  async function handleUpdateBountyProject(
    id: string,
    input: UpdateProjectInput,
  ): Promise<BountyProject> {
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    const updated = await api.updateProject(id, input);
    if (serviceSeq === serviceReconnectSeq && api === activeBountyApi) {
      await refreshBountyWorkspace();
    }
    return updated;
  }

  async function handleDeleteBountyProject(id: string): Promise<void> {
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    await api.deleteProject(id);
    if (serviceSeq === serviceReconnectSeq && api === activeBountyApi) {
      await refreshBountyWorkspace();
    }
  }

  async function handleToggleLanRegistration(enabled: boolean): Promise<OnboardingSettings> {
    const onbSeq = ++onboardingRequestSeq;
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    const updated = normalizeOnboardingSettings(
      await api.updateOnboardingSettings({ lan_open_registration: enabled }),
    );
    if (
      onbSeq === onboardingRequestSeq &&
      serviceSeq === serviceReconnectSeq &&
      api === activeBountyApi
    ) {
      onboardingSettings = updated;
      onboardingLoaded = true;
      onboardingLoadError = '';
      skillDocument = null;
      skillDocRequestSeq += 1;
    }
    return updated;
  }

  async function handleCreateInviteToken(
    input?: CreateInviteTokenInput,
  ): Promise<CreatedInviteToken> {
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    const created = await api.createInviteToken(input);
    if (serviceSeq === serviceReconnectSeq && api === activeBountyApi) {
      const existingTokens = Array.isArray(onboardingSettings.invite_tokens)
        ? onboardingSettings.invite_tokens
        : [];
      const exists = existingTokens.some(item => item.id === created.token.id);
      onboardingSettings = {
        ...onboardingSettings,
        invite_tokens: exists
          ? existingTokens.map(item => (item.id === created.token.id ? created.token : item))
          : [created.token, ...existingTokens],
      };
      try {
        await refreshOnboarding();
      } catch {
        // refreshOnboarding 内部已记录 onboardingLoadError，此处兜底避免吞掉已成功签发的明文令牌
      }
    }
    return created;
  }

  async function handleRevokeInviteToken(id: string): Promise<InviteTokenSummary> {
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    const revoked = await api.revokeInviteToken(id);
    if (serviceSeq === serviceReconnectSeq && api === activeBountyApi) {
      const existingTokens = Array.isArray(onboardingSettings.invite_tokens)
        ? onboardingSettings.invite_tokens
        : [];
      const exists = existingTokens.some(item => item.id === id || item.id === revoked.id);
      onboardingSettings = {
        ...onboardingSettings,
        invite_tokens: exists
          ? existingTokens.map(item => (item.id === id || item.id === revoked.id ? revoked : item))
          : [revoked, ...existingTokens],
      };
      skillDocument = null;
      skillDocRequestSeq += 1;
      try {
        await refreshOnboarding();
      } catch {
        // refreshOnboarding 内部已记录 onboardingLoadError，不回滚本地已作废状态也不对外抛错
      }
    }
    return revoked;
  }

  async function handleExportSkillMarkdown(doc: SkillDocument): Promise<boolean> {
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    const filename = doc.filename || 'SKILL.md';
    if (files.exportText) {
      const exported = await files.exportText(doc.content, filename);
      if (serviceSeq !== serviceReconnectSeq || api !== activeBountyApi) {
        return false;
      }
      return exported;
    }
    if (typeof URL !== 'undefined' && typeof URL.createObjectURL === 'function') {
      const url = URL.createObjectURL(
        new Blob([doc.content], { type: 'text/markdown;charset=utf-8' }),
      );
      const link = document.createElement('a');
      link.href = url;
      link.download = filename;
      link.click();
      URL.revokeObjectURL(url);
      return serviceSeq === serviceReconnectSeq && api === activeBountyApi;
    }
    return false;
  }

  async function handleOpenSkillGuide(): Promise<void> {
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    skillDocument = null;
    skillGuideOpen = true;
    try {
      await loadSkillMarkdown();
    } catch (error) {
      if (skillGuideOpen && serviceSeq === serviceReconnectSeq && api === activeBountyApi) {
        showToast(formatServiceError(error));
      }
    }
  }

  async function handleDisableAgent(id: string): Promise<AgentSummary> {
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    agentListInFlightCount += 1;
    try {
      const updated = await api.disableAgent(id);
      if (serviceSeq === serviceReconnectSeq && api === activeBountyApi) {
        await refreshAgentsOnly();
        if (serviceSeq === serviceReconnectSeq && api === activeBountyApi) {
          showToast($t('已停用 Agent'));
        }
      }
      return updated;
    } finally {
      agentListInFlightCount -= 1;
    }
  }

  async function handleEnableAgent(id: string): Promise<AgentSummary> {
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    agentListInFlightCount += 1;
    try {
      const updated = await api.enableAgent(id);
      if (serviceSeq === serviceReconnectSeq && api === activeBountyApi) {
        await refreshAgentsOnly();
        if (serviceSeq === serviceReconnectSeq && api === activeBountyApi) {
          showToast($t('已启用 Agent'));
        }
      }
      return updated;
    } finally {
      agentListInFlightCount -= 1;
    }
  }

  async function handleRevokeAgent(id: string): Promise<AgentSummary> {
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    agentListInFlightCount += 1;
    try {
      const updated = await api.revokeAgent(id);
      if (serviceSeq === serviceReconnectSeq && api === activeBountyApi) {
        await refreshAgentsOnly();
        if (serviceSeq === serviceReconnectSeq && api === activeBountyApi) {
          showToast($t('已吊销 Agent'));
        }
      }
      return updated;
    } finally {
      agentListInFlightCount -= 1;
    }
  }

  async function handleCreateBounty(input: CreateBountyInput): Promise<BountyTask> {
    const created = await activeBountyApi.createBounty(input);
    await refreshBountyWorkspace();
    showToast(created.status === 'OPEN' ? $t('悬赏已发布') : $t('草稿已保存'));
    return created;
  }

  async function handleUpdateDraft(
    id: string,
    input: UpdateBountyInput,
    publishAfter: boolean,
  ): Promise<BountyTask> {
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    detailInFlightCount += 1;
    try {
      let updated = await api.updateBounty(id, input);
      if (publishAfter) {
        updated = await api.publishBounty(id);
      }
      await refreshBountyWorkspace();
      if (
        activeDetail &&
        activeDetail.bounty.id === id &&
        serviceSeq === serviceReconnectSeq &&
        api === activeBountyApi
      ) {
        const detailSeq = ++detailRequestSeq;
        const refreshedDetail = await api.getBountyDetail(id);
        if (
          detailSeq === detailRequestSeq &&
          serviceSeq === serviceReconnectSeq &&
          api === activeBountyApi &&
          activeDetail?.bounty.id === id
        ) {
          activeDetail = refreshedDetail;
        }
      }
      showToast(publishAfter ? $t('悬赏已发布') : $t('草稿已保存'));
      return updated;
    } finally {
      detailInFlightCount -= 1;
    }
  }

  async function openBountyDetail(bountyId: string): Promise<void> {
    const detailSeq = ++detailRequestSeq;
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    detailInFlightCount += 1;
    try {
      const detail = await api.getBountyDetail(bountyId);
      if (
        detailSeq !== detailRequestSeq ||
        serviceSeq !== serviceReconnectSeq ||
        api !== activeBountyApi
      ) {
        return;
      }
      activeDetail = detail;
    } catch (error) {
      if (
        detailSeq !== detailRequestSeq ||
        serviceSeq !== serviceReconnectSeq ||
        api !== activeBountyApi
      ) {
        return;
      }
      showToast(formatServiceError(error));
    } finally {
      detailInFlightCount -= 1;
    }
  }

  async function handlePublishFromDrawer(bountyId: string): Promise<void> {
    const detailSeq = ++detailRequestSeq;
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    detailInFlightCount += 1;
    try {
      await api.publishBounty(bountyId);
      const refreshedDetail = await api.getBountyDetail(bountyId);
      if (
        detailSeq === detailRequestSeq &&
        serviceSeq === serviceReconnectSeq &&
        api === activeBountyApi
      ) {
        activeDetail = refreshedDetail;
      }
      await refreshBountyWorkspace();
      showToast($t('悬赏已发布'));
    } finally {
      detailInFlightCount -= 1;
    }
  }

  async function handleWithdrawFromDrawer(bountyId: string): Promise<void> {
    const detailSeq = ++detailRequestSeq;
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    detailInFlightCount += 1;
    try {
      await api.withdrawBounty(bountyId);
      const refreshedDetail = await api.getBountyDetail(bountyId);
      if (
        detailSeq === detailRequestSeq &&
        serviceSeq === serviceReconnectSeq &&
        api === activeBountyApi
      ) {
        activeDetail = refreshedDetail;
      }
      await refreshBountyWorkspace();
      showToast($t('已撤回为草稿'));
    } catch (error) {
      showToast(formatServiceError(error));
      try {
        const refreshedDetail = await api.getBountyDetail(bountyId);
        if (
          detailSeq === detailRequestSeq &&
          serviceSeq === serviceReconnectSeq &&
          api === activeBountyApi
        ) {
          activeDetail = refreshedDetail;
        }
      } catch {
        /* 忽略刷新详情失败 */
      }
      await refreshBountyWorkspace();
      throw error;
    } finally {
      detailInFlightCount -= 1;
    }
  }

  async function handleCancelFromDrawer(bountyId: string): Promise<void> {
    const detailSeq = ++detailRequestSeq;
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    detailInFlightCount += 1;
    try {
      await api.cancelBounty(bountyId);
      const refreshedDetail = await api.getBountyDetail(bountyId);
      if (
        detailSeq === detailRequestSeq &&
        serviceSeq === serviceReconnectSeq &&
        api === activeBountyApi
      ) {
        activeDetail = refreshedDetail;
      }
      await refreshBountyWorkspace();
      showToast($t('任务已取消'));
    } finally {
      detailInFlightCount -= 1;
    }
  }

  async function handleReleaseLeaseFromDrawer(bountyId: string): Promise<void> {
    const detailSeq = ++detailRequestSeq;
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    detailInFlightCount += 1;
    try {
      await api.releaseLease(bountyId);
      const refreshedDetail = await api.getBountyDetail(bountyId);
      if (
        detailSeq === detailRequestSeq &&
        serviceSeq === serviceReconnectSeq &&
        api === activeBountyApi
      ) {
        activeDetail = refreshedDetail;
      }
      await refreshBountyWorkspace();
      showToast($t('已释放租约并重新开放悬赏'));
    } catch (error) {
      showToast(formatServiceError(error));
      try {
        const refreshedDetail = await api.getBountyDetail(bountyId);
        if (
          detailSeq === detailRequestSeq &&
          serviceSeq === serviceReconnectSeq &&
          api === activeBountyApi
        ) {
          activeDetail = refreshedDetail;
        }
      } catch {
        /* 忽略刷新详情失败 */
      }
      await refreshBountyWorkspace();
      throw error;
    } finally {
      detailInFlightCount -= 1;
    }
  }

  async function handleCreateCommentFromDrawer(bountyId: string, body: string): Promise<void> {
    const detailSeq = ++detailRequestSeq;
    const serviceSeq = serviceReconnectSeq;
    const api = activeBountyApi;
    if (typeof api.createBountyComment !== 'function') {
      throw new Error('Comment API is unavailable');
    }
    const previousStatus = activeDetail?.bounty.status ?? null;
    detailInFlightCount += 1;
    try {
      const result = await api.createBountyComment(bountyId, { body });
      const refreshedDetail = await api.getBountyDetail(bountyId);
      if (
        detailSeq === detailRequestSeq &&
        serviceSeq === serviceReconnectSeq &&
        api === activeBountyApi &&
        activeDetail?.bounty.id === bountyId
      ) {
        activeDetail = refreshedDetail;
      }
      await refreshBountyWorkspace();
      const resumed = previousStatus === 'WAITING_INFO' && result.bounty.status === 'IN_PROGRESS';
      showToast(resumed ? $t('已回复并恢复任务') : $t('评论已发送'));
    } catch (error) {
      showToast(formatServiceError(error));
      throw error;
    } finally {
      detailInFlightCount -= 1;
    }
  }

  function openCreateComposer(): void {
    editingBounty = null;
    composerOpen = true;
  }

  function openEditDraftComposer(bounty: BountyTask): void {
    editingBounty = bounty;
    composerOpen = true;
  }

  async function applyVisualPreferences(): Promise<void> {
    if (typeof document === 'undefined') return;
    const root = document.documentElement;
    applyTheme(root, activeTheme, preferences.theme, systemDark);
    try {
      localStorage.setItem(
        bootThemeKey,
        JSON.stringify({ definition: activeTheme, mode: preferences.theme }),
      );
    } catch {
      /* 忽略无存储权限环境 */
    }
    if (preferences.fontFamily) root.style.setProperty('--user-font-body', preferences.fontFamily);
    else root.style.removeProperty('--user-font-body');
    const widthPx = preferences.contentWidth ?? preferences.lineWidth ?? defaultPreferences.contentWidth;
    const indentChars =
      typeof preferences.paragraphIndent === 'number'
        ? preferences.paragraphIndent
        : preferences.paragraphIndent
          ? 2
          : 0;
    root.style.setProperty('--font-size', `${preferences.fontSize}px`);
    root.style.setProperty('--content-width', `${widthPx}px`);
    root.style.setProperty('--line-width', `${widthPx}px`);
    root.style.setProperty('--paragraph-indent', indentChars > 0 ? `${indentChars}em` : '0em');

    if (desktopMaterialSupported && windowMaterial) {
      const material = resolveWindowMaterial(activeTheme, preferences.theme, systemDark);
      const keepOnBlur =
        themeSupportsBlur(activeTheme) && Boolean(preferences.keepTransparentOnBlur);
      try {
        await windowMaterial.update(material, preferences.theme, keepOnBlur);
      } catch (error) {
        showToast($t('窗口材质应用失败：{error}', { error: errorMessage(error) }));
      }
    } else {
      root.dataset.windowTransparent = 'false';
    }
  }

  $effect(() => {
    const _resolved = configResolved;
    const _theme = activeTheme;
    const _mode = preferences.theme;
    const _dark = systemDark;
    const _blur = preferences.keepTransparentOnBlur;
    const _fontFamily = preferences.fontFamily;
    const _fontSize = preferences.fontSize;
    const _contentWidth = preferences.contentWidth;
    const _lineWidth = preferences.lineWidth;
    const _indent = preferences.paragraphIndent;
    if (!_resolved) return;
    void applyVisualPreferences();
  });

  function waitNextPaintFrames(): Promise<void> {
    return new Promise<void>(resolve => {
      let settled = false;
      const done = () => {
        if (settled) return;
        settled = true;
        resolve();
      };
      const timer = setTimeout(done, 48);
      if (typeof requestAnimationFrame === 'function') {
        requestAnimationFrame(() => {
          requestAnimationFrame(() => {
            clearTimeout(timer);
            done();
          });
        });
      }
    });
  }

  function applyLoadedConfig(loaded: AppConfig | null): void {
    const config = loaded ?? emptyAppConfig();
    projects = config.projects;
    activeProjectId =
      config.activeProjectId ?? (config.projects.length > 0 ? config.projects[0].id : null);
    customThemes = config.customThemes ?? [];
    const mergedPrefs = { ...defaultPreferences, ...config.preferences };
    const resolvedLocale = mergedPrefs.locale ?? mergedPrefs.language ?? 'zh-CN';
    const resolvedWidth = mergedPrefs.contentWidth ?? mergedPrefs.lineWidth ?? defaultPreferences.contentWidth;
    const resolvedIndent =
      typeof mergedPrefs.paragraphIndent === 'number'
        ? mergedPrefs.paragraphIndent
        : mergedPrefs.paragraphIndent
          ? 2
          : 0;
    const resolvedSidebarOpen =
      mergedPrefs.sidebarOpen !== undefined
        ? mergedPrefs.sidebarOpen
        : mergedPrefs.sidebarCollapsed !== undefined
          ? !mergedPrefs.sidebarCollapsed
          : true;
    let resolvedDefaultIcon = mergedPrefs.defaultProjectIcon;
    if (resolvedDefaultIcon) {
      const parsed = parseProjectIcon(resolvedDefaultIcon);
      resolvedDefaultIcon = parsed ? serializeProjectIcon(parsed) : undefined;
    }
    preferences = {
      ...mergedPrefs,
      locale: resolvedLocale,
      language: resolvedLocale,
      contentWidth: resolvedWidth,
      lineWidth: resolvedWidth,
      paragraphIndent: resolvedIndent,
      sidebarOpen: resolvedSidebarOpen,
      sidebarCollapsed: !resolvedSidebarOpen,
      ...(resolvedDefaultIcon !== undefined ? { defaultProjectIcon: resolvedDefaultIcon } : {}),
    };
    windowPlacement = config.window ? { ...config.window } : undefined;
    activeSection = preferences.workspaceSection ?? 'bounties';
    setLocalePreference(resolvedLocale);
  }

  async function retryLoadConfig(): Promise<void> {
    try {
      const loaded = await files.loadConfig();
      configLoadFailed = false;
      configLoadError = '';
      applyLoadedConfig(loaded);
      await applyVisualPreferences();
      void reconnectBountyService(true);
    } catch (error) {
      configLoadFailed = true;
      configLoadError = errorMessage(error);
      showToast(configLoadError);
    }
  }

  function selectSection(section: WorkspaceSection): void {
    activeSection = section;
    preferences = { ...preferences, workspaceSection: section };
    mobileSidebarOpen = false;
    if (serviceConnected) {
      if (section === 'bounties' || section === 'inbox') {
        void refreshBountiesOnly();
      } else if (section === 'agents') {
        void refreshAgentsOnly();
      }
    }
    void persistConfig();
  }

  function selectProject(projectId: string): void {
    activeProjectId = projectId;
    void persistConfig();
  }

  function toggleSidebar(collapsed: boolean): void {
    preferences = { ...preferences, sidebarOpen: !collapsed, sidebarCollapsed: collapsed };
    if (!collapsed && files.isMobile) mobileSidebarOpen = true;
    else if (collapsed && files.isMobile) mobileSidebarOpen = false;
    void persistConfig();
  }

  function handleSelectLanguage(language: LocalePreference): void {
    setLocalePreference(language);
    preferences = { ...preferences, locale: language, language };
    void persistConfig();
  }

  function handleSelectThemeId(themeId: string): void {
    themeError = '';
    preferences = { ...preferences, themeId };
    void persistConfig();
  }

  function handleSelectThemeMode(theme: ThemeMode): void {
    preferences = { ...preferences, theme };
    void persistConfig();
  }

  function handleToggleKeepTransparentOnBlur(enabled: boolean): void {
    preferences = { ...preferences, keepTransparentOnBlur: enabled };
    void persistConfig();
  }

  function handleChangeFontFamily(fontFamily: string): void {
    preferences = { ...preferences, fontFamily };
    void persistConfig();
  }

  function handleChangeFontSize(fontSize: number): void {
    preferences = { ...preferences, fontSize: Math.min(20, Math.max(14, fontSize)) };
    void persistConfig();
  }

  function handleChangeLineWidth(lineWidth: number): void {
    const clamped = Math.min(900, Math.max(600, lineWidth));
    preferences = { ...preferences, contentWidth: clamped, lineWidth: clamped };
    void persistConfig();
  }

  function handleChangeParagraphIndent(indent: number): void {
    const clamped = Math.min(4, Math.max(0, Math.round(indent)));
    preferences = { ...preferences, paragraphIndent: clamped };
    void persistConfig();
  }

  async function handleImportThemeFile(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file || importingTheme) return;
    themeError = '';
    if (file.size > 65536) {
      themeError = $t('THEME_INVALID: 主题文件不能超过 64 KiB。');
      input.value = '';
      return;
    }
    importingTheme = true;
    try {
      const text = await file.text();
      const parsed = parseTheme(text);
      if (allThemes.some(item => item.id === parsed.id)) {
        themeError = $t('THEME_DUPLICATE: 此主题 ID 已存在，请修改文件中的 id 或先移除已有主题。');
        return;
      }
      customThemes = [...customThemes, parsed];
      preferences = { ...preferences, themeId: parsed.id };
      await persistConfig();
    } catch (error) {
      themeError = errorMessage(error);
    } finally {
      input.value = '';
      importingTheme = false;
    }
  }

  async function handleExportThemeTemplate(): Promise<void> {
    themeError = '';
    const template = createThemeTemplate(activeTheme);
    const text = `${JSON.stringify(template, null, 2)}\n`;
    try {
      if (files.exportText) {
        const saved = await files.exportText(text, `${template.id}.json`);
        if (saved) showToast($t('主题模板已保存'));
      }
    } catch (error) {
      showToast($t('主题模板保存失败：{error}', { error: errorMessage(error) }));
    }
  }

  function handleRemoveActiveTheme(): void {
    if (!customThemes.some(item => item.id === activeTheme.id)) return;
    const removedId = activeTheme.id;
    customThemes = customThemes.filter(item => item.id !== removedId);
    preferences = { ...preferences, themeId: defaultThemeId };
    themeError = '';
    void persistConfig();
  }

  function handlePickProjectIcon(icon: ProjectIconValue): void {
    const serialized = serializeProjectIcon(icon);
    if (activeProject) {
      const targetId = activeProject.id;
      projects = projects.map(project =>
        project.id === targetId ? { ...project, icon: serialized } : project,
      );
    } else {
      preferences = { ...preferences, defaultProjectIcon: serialized };
    }
    if (selectedBountyProject) {
      void handleUpdateBountyProject(selectedBountyProject.id, { icon: serialized });
    }
    void persistConfig();
    iconPickerOpen = false;
  }

  function handleResetProjectIcon(): void {
    if (activeProject) {
      const targetId = activeProject.id;
      projects = projects.map(project => {
        if (project.id !== targetId) return project;
        const { icon: _removed, ...rest } = project;
        return rest;
      });
    } else {
      const { defaultProjectIcon: _removed, ...rest } = preferences;
      preferences = rest;
    }
    if (selectedBountyProject) {
      void handleUpdateBountyProject(selectedBountyProject.id, { icon: null });
    }
    void persistConfig();
    iconPickerOpen = false;
  }

  async function openExternalLink(url: string): Promise<void> {
    try {
      if (files.openExternalLink) {
        await files.openExternalLink(url);
      } else {
        window.open(url, '_blank', 'noopener,noreferrer');
      }
    } catch (error) {
      showToast(errorMessage(error));
    }
  }

  const updateCoordinator = new UpdateCoordinator({
    port: updatePort,
    autoDownload: true,
    onStatus: (status: UpdateStatus) => {
      updateStatus = status;
      if (status.kind === 'ready' && status.version && !updateDialogOpen) {
        showToast($t('Taskrail {version} 已下载，可在更新面板安装。', { version: status.version }));
      }
    },
    beforeInstall: async () => {
      await configSaveChain;
      return !configSaveFailed && !configLoadFailed;
    },
  });

  function handleUpdatePreferences(autoCheck: boolean, autoDownload: boolean): void {
    preferences = { ...preferences, autoCheckUpdates: autoCheck, autoDownloadUpdates: autoDownload };
    updateCoordinator.setAutoDownload(autoDownload);
    void persistConfig();
  }

  onMount(() => {
    let disposed = false;
    componentDisposed = false;
    const cleanups: Array<() => void> = [];

    if (realtimeSyncTimer) clearInterval(realtimeSyncTimer);
    realtimeSyncTimer = setInterval(() => {
      void syncRealtimeWorkspace();
    }, REALTIME_SYNC_INTERVAL_MS);
    cleanups.push(() => {
      if (realtimeSyncTimer) {
        clearInterval(realtimeSyncTimer);
        realtimeSyncTimer = null;
      }
    });

    if (typeof window !== 'undefined' && typeof window.matchMedia === 'function') {
      const media = window.matchMedia('(prefers-color-scheme: dark)');
      const onMediaChange = (event: MediaQueryListEvent) => {
        systemDark = event.matches;
      };
      media.addEventListener?.('change', onMediaChange);
      cleanups.push(() => media.removeEventListener?.('change', onMediaChange));
    }

    if (files.isMobile) {
      cleanups.push(
        registerDrawerGestures({
          isOpen: () => mobileSidebarOpen,
          blocked: () =>
            iconPickerOpen ||
            updateDialogOpen ||
            composerOpen ||
            skillGuideOpen ||
            activeDetail !== null,
          setOpen: open => {
            mobileSidebarOpen = open;
            preferences = { ...preferences, sidebarOpen: open, sidebarCollapsed: !open };
          },
        }),
      );
    }

    if (capabilities.desktop) {
      try {
        const win = getCurrentWindow();
        if (typeof win?.onCloseRequested === 'function') {
          let closing = false;
          void win
            .onCloseRequested(async event => {
              if (closing) return;
              event.preventDefault();
              closing = true;
              try {
                if (!configLoadFailed) {
                  const maximized = await win.isMaximized();
                  if (maximized) {
                    windowPlacement = {
                      ...(windowPlacement ?? {}),
                      maximized: true,
                    };
                  } else {
                    const [pos, size] = await Promise.all([win.outerPosition(), win.innerSize()]);
                    windowPlacement = {
                      width: size.width,
                      height: size.height,
                      x: pos.x,
                      y: pos.y,
                      maximized: false,
                    };
                  }
                  await persistConfig();
                }
              } catch {
                /* 忽略窗口坐标读取或保存异常，继续关闭流程 */
              } finally {
                await win.destroy();
              }
            })
            .then(unlisten => {
              if (disposed) unlisten();
              else cleanups.push(unlisten);
            })
            .catch(() => {});
        }
      } catch {
        /* 非桌面或测试环境忽略 */
      }
      try {
        void listen('taskrail:window-restored', () => {
          if (!disposed) windowMaterial?.showBackdropVeil('fade');
        })
          .then(unlisten => {
            if (disposed) unlisten();
            else cleanups.push(unlisten);
          })
          .catch(() => {});
      } catch {
        /* 非桌面或测试环境忽略 */
      }
    }

    void (async () => {
      if (capabilities.native) {
        try {
          currentVersion = await getVersion();
        } catch {
          /* 测试或浏览器环境下保留默认版本号 */
        }
      }

      try {
        const loaded = await files.loadConfig();
        if (disposed) return;
        configLoadFailed = false;
        configLoadError = '';
        applyLoadedConfig(loaded);
      } catch (error) {
        if (!disposed) {
          configLoadFailed = true;
          configLoadError = errorMessage(error);
          setLocalePreference(defaultPreferences.locale ?? 'zh-CN');
          showToast(configLoadError);
        }
      }

      if (disposed) return;
      loading = false;
      void reconnectBountyService(true);
      await applyVisualPreferences();
      if (disposed) return;
      configResolved = true;
      booted = true;
      windowMaterial?.showBackdropVeil('hold');
      await tick();
      await waitNextPaintFrames();
      if (disposed) return;
      onready?.();
      windowMaterial?.showBackdropVeil('fade');

      if (!disposed && updateSupported) {
        updateCoordinator.setAutoDownload(preferences.autoDownloadUpdates ?? true);
        if (preferences.autoCheckUpdates ?? true) {
          void updateCoordinator.check();
        }
      }
    })();

    return () => {
      disposed = true;
      componentDisposed = true;
      for (const cleanup of cleanups) cleanup();
    };
  });

  onDestroy(() => {
    componentDisposed = true;
    if (realtimeSyncTimer) {
      clearInterval(realtimeSyncTimer);
      realtimeSyncTimer = null;
    }
    if (toastTimer) clearTimeout(toastTimer);
    windowMaterial?.dispose();
    updateCoordinator.dispose();
  });
</script>

<div
  class="app-shell"
  class:booted
  class:desktop-window={capabilities.desktop}
  class:sidebar-collapsed={sidebarCollapsed}
  class:mobile-sidebar-open={mobileSidebarOpen}
>
  {#if files.isMobile && mobileSidebarOpen && !sidebarCollapsed}
    <button
      type="button"
      class="sidebar-backdrop"
      aria-label={$t('收起导航栏')}
      onclick={() => toggleSidebar(true)}
    ></button>
  {/if}

  <aside class="sidebar" aria-label={$t('工作台导航')}>
    <div class="sidebar-header">
      <div class="brand-lockup">
        <span class="brand-mark" aria-hidden="true">
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <rect x="2" y="2.5" width="12" height="11" rx="2.5" />
            <path d="M5.2 6h5.6M5.2 9.5h3.5" />
          </svg>
        </span>
        <div class="brand-copy">
          <span class="brand-title">Taskrail</span>
          <span class="brand-subtitle">{$t('Agent 悬赏工作台')}</span>
        </div>
      </div>
      <button
        type="button"
        class="icon-button sidebar-collapse-btn"
        aria-label={$t('收起导航栏')}
        title={$t('收起导航栏')}
        onclick={() => toggleSidebar(true)}
      >
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <rect x="2" y="2.5" width="12" height="11" rx="2" />
          <path d="M6 2.5v11M10.5 6 8.5 8l2 2" />
        </svg>
      </button>
    </div>

    <nav class="workspace-nav" aria-label={$t('工作台导航')}>
      <button
        type="button"
        class="workspace-nav-item"
        class:active={activeSection === 'bounties'}
        aria-current={activeSection === 'bounties' ? 'page' : undefined}
        data-nav-section="bounties"
        onclick={() => selectSection('bounties')}
      >
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <rect x="2" y="2.5" width="12" height="11" rx="2" />
          <path d="M5.5 6h5M5.5 9h5M5.5 11.5h3" />
        </svg>
        <span class="workspace-nav-label">{$t('悬赏板')}</span>
      </button>

      <button
        type="button"
        class="workspace-nav-item"
        class:active={activeSection === 'inbox'}
        aria-current={activeSection === 'inbox' ? 'page' : undefined}
        data-nav-section="inbox"
        onclick={() => selectSection('inbox')}
      >
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M2.5 9.5h3.2l1.1 1.8h2.4l1.1-1.8h3.2" />
          <rect x="2.2" y="3" width="11.6" height="10" rx="2" />
        </svg>
        <span class="workspace-nav-label">{$t('待我处理')}</span>
        {#if actionableBounties.length > 0}
          <span class="workspace-nav-badge" data-slot="inbox-count">
            {actionableBounties.length}
          </span>
        {/if}
      </button>

      <button
        type="button"
        class="workspace-nav-item"
        class:active={activeSection === 'agents'}
        aria-current={activeSection === 'agents' ? 'page' : undefined}
        data-nav-section="agents"
        onclick={() => selectSection('agents')}
      >
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <rect x="3" y="4" width="10" height="8.5" rx="2.2" />
          <path d="M8 2v2M5.8 7.8h.01M10.2 7.8h.01M6 10.2h4" />
        </svg>
        <span class="workspace-nav-label">{$t('Agent 名录')}</span>
      </button>

      <button
        type="button"
        class="workspace-nav-item"
        class:active={activeSection === 'settings'}
        aria-current={activeSection === 'settings' ? 'page' : undefined}
        data-nav-section="settings"
        onclick={() => selectSection('settings')}
      >
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="8" cy="8" r="2.2" />
          <path d="M8 2.2v1.4M8 12.4v1.4M2.2 8h1.4M12.4 8h1.4M3.9 3.9l1 1M11.1 11.1l1 1M12.1 3.9l-1 1M4.9 11.1l-1 1" />
        </svg>
        <span class="workspace-nav-label">{$t('设置')}</span>
      </button>
    </nav>
  </aside>

  <main class="workspace">
    <header class="topbar" data-tauri-drag-region={capabilities.desktop ? true : undefined}>
      <div class="topbar-start">
        {#if sidebarCollapsed || files.isMobile}
          <button
            type="button"
            class="icon-button sidebar-expand-btn"
            aria-label={$t('展开导航栏')}
            title={$t('展开导航栏')}
            onclick={() => toggleSidebar(false)}
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <rect x="2" y="2.5" width="12" height="11" rx="2" />
              <path d="M6 2.5v11M8.5 6l2 2-2 2" />
            </svg>
          </button>
        {/if}

        <div class="topbar-heading" data-tauri-drag-region={capabilities.desktop ? true : undefined}>
          <span class="topbar-title">{sectionMeta.title}</span>
          <span class="topbar-divider" aria-hidden="true">/</span>
          <span class="topbar-subtitle">{sectionMeta.subtitle}</span>
        </div>
      </div>

      <div class="topbar-actions">
        {#if updateStatus.kind === 'available' || updateStatus.kind === 'downloading' || updateStatus.kind === 'ready'}
          <button
            type="button"
            class="update-badge"
            onclick={() => {
              updateDialogOpen = true;
            }}
          >
            {#if updateStatus.kind === 'ready'}
              {$t('更新已就绪')}
            {:else if updateStatus.kind === 'downloading'}
              {$t('正在下载更新…')}
            {:else}
              {$t('发现新版本')}
            {/if}
          </button>
        {/if}

        <button
          type="button"
          class="icon-button"
          aria-label={$t('应用更新')}
          title={$t('应用更新')}
          onclick={() => {
            updateDialogOpen = true;
          }}
        >
          <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M8 2.5v7.2M5.2 6.9 8 9.7l2.8-2.8M3 12.8h10" />
          </svg>
        </button>

        {#if capabilities.desktop}
          <WindowControls onerror={message => showToast(message)} />
        {/if}
      </div>
    </header>

    {#if configLoadFailed}
      <div class="status-banner" role="alert" data-status="config-load-failed">
        <span>{configLoadError || $t('项目配置格式无效，原配置已保留，请修复配置后重新打开应用。')}</span>
        <button type="button" onclick={() => void retryLoadConfig()}>{$t('重试加载配置')}</button>
      </div>
    {:else if configSaveFailed}
      <div class="status-banner" role="alert" data-status="config-save-failed">
        <span>{$t('配置保存失败：{error}', { error: $t('未保存') })}</span>
        <button type="button" onclick={() => void persistConfig()}>{$t('重试配置保存')}</button>
      </div>
    {:else if !serviceConnected && serviceConnectionError}
      <div class="status-banner" role="alert" data-status="service-connection-failed">
        <span>{$t('悬赏服务连接失败：{error}', { error: serviceConnectionError })}</span>
        <div class="status-banner-actions">
          <button
            type="button"
            data-action="retry-service-connection"
            onclick={() => void reconnectBountyService()}
          >
            {$t('重试连接')}
          </button>
          <button
            type="button"
            data-action="open-service-settings"
            onclick={() => selectSection('settings')}
          >
            {$t('前往设置')}
          </button>
        </div>
      </div>
    {/if}

    <div class="workspace-body" data-active-section={activeSection}>
      {#if loading}
        <div class="workspace-loading" role="status">
          <p>{$t('正在加载…')}</p>
        </div>
      {:else if activeSection === 'bounties'}
        <section class="workspace-view" aria-labelledby="view-heading-bounties" data-view="bounties">
          <header class="view-hero">
            <div class="view-hero-copy">
              <h1 id="view-heading-bounties" class="view-title">{$t('悬赏板')}</h1>
              <p class="view-description">{sectionMeta.description}</p>
            </div>
            <div class="view-hero-actions">
              {#if projects.length > 1}
                <div class="filter-pills" role="group" aria-label={$t('切换项目')}>
                  {#each projects as project (project.id)}
                    <button
                      type="button"
                      class="filter-pill"
                      class:active={activeProject?.id === project.id}
                      data-project-id={project.id}
                      onclick={() => selectProject(project.id)}
                    >
                      <ProjectIcon icon={project.icon} />
                      <span>{project.name}</span>
                    </button>
                  {/each}
                </div>
              {/if}
              <button
                type="button"
                class="project-badge-button"
                aria-label={$t('设置项目图标')}
                title={$t('设置项目图标')}
                onclick={() => {
                  iconPickerOpen = true;
                }}
              >
                <ProjectIcon icon={currentProjectIcon} />
                <span>{activeProject?.name ?? selectedBountyProject?.name ?? $t('全部项目')}</span>
              </button>
              <button
                type="button"
                class="primary"
                data-action="open-composer"
                title={$t('发布悬赏')}
                onclick={openCreateComposer}
              >
                + {$t('发布悬赏')}
              </button>
            </div>
          </header>

          <div class="workspace-toolbar" data-slot="bounty-toolbar">
            <div class="toolbar-filters-group">
              <select
                id="bounty-project-filter"
                class="toolbar-project-select"
                data-filter="project"
                aria-label={$t('按项目筛选')}
                value={bountyProjectFilter}
                onchange={event => {
                  bountyProjectFilter = (event.currentTarget as HTMLSelectElement).value;
                  void refreshBountiesOnly();
                }}
              >
                <option value="ALL">{$t('全部项目')}</option>
                {#each activeBountyProjects as project (project.id)}
                  <option value={project.id}>{project.name}</option>
                {/each}
              </select>

              <div class="filter-pills" role="group" aria-label={$t('全部状态')}>
                {#each [
                  { id: 'ALL', label: $t('全部状态') },
                  { id: 'DRAFT', label: $t('[草稿]') },
                  { id: 'OPEN', label: $t('待接单') },
                  { id: 'IN_PROGRESS', label: $t('执行中') },
                  { id: 'WAITING_INFO', label: $t('待回复') },
                  { id: 'PENDING_ARBITRATION', label: $t('待仲裁') },
                  { id: 'COMPLETED', label: $t('已完成') },
                  { id: 'CANCELLED', label: $t('已取消') },
                ] as item (item.id)}
                  <button
                    type="button"
                    class="filter-pill"
                    class:active={bountyStatusFilter === item.id}
                    data-status-filter={item.id}
                    onclick={() => {
                      bountyStatusFilter = item.id as typeof bountyStatusFilter;
                      void refreshBountiesOnly();
                    }}
                  >
                    {item.label}
                  </button>
                {/each}
              </div>
            </div>

            <div class="toolbar-search">
              <input
                type="search"
                class="toolbar-search-input"
                data-filter="search"
                aria-label={$t('搜索悬赏标题、编号或标签…')}
                placeholder={$t('搜索悬赏标题、编号或标签…')}
                value={bountySearchQuery}
                oninput={event => {
                  bountySearchQuery = (event.currentTarget as HTMLInputElement).value;
                  void refreshBountiesOnly();
                }}
              />
            </div>
          </div>

          <div class="skeleton-stage" data-slot="bounty-list-region">
            {#if bounties.length === 0}
              <div class="empty-state-card">
                <div class="empty-state-icon" aria-hidden="true">
                  <ProjectIcon icon={currentProjectIcon} large />
                </div>
                <h2 class="empty-state-title">{$t('暂无悬赏任务')}</h2>
                <p class="empty-state-copy">
                  {$t('发布新悬赏后，将在这里按项目与状态筛选浏览任务详情、验收条件和交付记录。')}
                </p>
                <div class="markdown-preview-sample" use:renderMarkdownAction={sampleCriteriaMarkdown}></div>
                <button
                  type="button"
                  class="primary"
                  data-action="empty-post-bounty"
                  onclick={openCreateComposer}
                >
                  + {$t('发布悬赏')}
                </button>
              </div>
            {:else}
              <div class="bounty-card-grid" data-slot="bounty-card-grid">
                {#each bounties as bounty (bounty.id)}
                  <BountyCard
                    {bounty}
                    project={bountyProjectsById.get(bounty.project_id)}
                    onSelect={id => void openBountyDetail(id)}
                  />
                {/each}
              </div>
            {/if}
          </div>
        </section>
      {:else if activeSection === 'inbox'}
        <section class="workspace-view" aria-labelledby="view-heading-inbox" data-view="inbox">
          <header class="view-hero">
            <div class="view-hero-copy">
              <h1 id="view-heading-inbox" class="view-title">{$t('待我处理')}</h1>
              <p class="view-description">{sectionMeta.description}</p>
            </div>
          </header>

          <div class="workspace-toolbar" data-slot="inbox-toolbar">
            <div class="filter-pills" role="group" aria-label={$t('待我处理')}>
              {#each [
                { id: 'ALL', label: $t('全部待办') },
                { id: 'DRAFT', label: $t('待发布草稿') },
                { id: 'WAITING_INFO', label: $t('待回复') },
                { id: 'PENDING_ARBITRATION', label: $t('待仲裁') },
              ] as item (item.id)}
                <button
                  type="button"
                  class="filter-pill"
                  class:active={inboxFilter === item.id}
                  data-inbox-filter={item.id}
                  onclick={() => {
                    inboxFilter = item.id as typeof inboxFilter;
                  }}
                >
                  {item.label}
                </button>
              {/each}
            </div>

            <button
              type="button"
              class="secondary-action"
              data-action="refresh-inbox"
              onclick={() => void refreshBountiesOnly()}
            >
              {$t('刷新')}
            </button>
          </div>

          <div class="skeleton-stage" data-slot="inbox-region">
            {#if visibleInboxBounties.length === 0}
              <div class="empty-state-card">
                <h2 class="empty-state-title">{$t('当前没有待处理事项')}</h2>
                <p class="empty-state-copy">
                  {$t('当 Agent 请求补充信息（租约计时暂停）或提交交付等待验收时，相关任务会自动汇聚到此视图。')}
                </p>
              </div>
            {:else}
              <div class="bounty-card-grid" data-slot="inbox-card-grid">
                {#each visibleInboxBounties as bounty (bounty.id)}
                  <BountyCard
                    {bounty}
                    project={bountyProjectsById.get(bounty.project_id)}
                    onSelect={id => void openBountyDetail(id)}
                  />
                {/each}
              </div>
            {/if}
          </div>
        </section>
      {:else if activeSection === 'agents'}
        <section class="workspace-view" aria-labelledby="view-heading-agents" data-view="agents">
          <header class="view-hero">
            <div class="view-hero-copy">
              <h1 id="view-heading-agents" class="view-title">{$t('Agent 名录')}</h1>
              <p class="view-description">{sectionMeta.description}</p>
            </div>
            <div class="view-hero-actions">
              <button
                type="button"
                class="secondary-action"
                data-action="open-skill-guide"
                onclick={() => void handleOpenSkillGuide()}
              >
                {$t('接入指南 (SKILL.md)')}
              </button>
            </div>
          </header>

          <AgentDirectory
            {agents}
            {agentsLoaded}
            {agentsLoadError}
            projectsById={bountyProjectsById}
            activityFilter={agentActivityFilter}
            statusFilter={agentStatusFilter}
            {skillGuideOpen}
            {skillDocument}
            {serviceUrl}
            {serviceEpoch}
            onRetryLoadAgents={refreshAgentsOnly}
            onChangeActivityFilter={next => {
              agentActivityFilter = next;
              void refreshAgentsOnly();
            }}
            onChangeStatusFilter={next => {
              agentStatusFilter = next;
              void refreshAgentsOnly();
            }}
            onDisableAgent={handleDisableAgent}
            onEnableAgent={handleEnableAgent}
            onRevokeAgent={handleRevokeAgent}
            onOpenSkillGuide={() => void handleOpenSkillGuide()}
            onCloseSkillGuide={() => {
              skillDocRequestSeq += 1;
              skillGuideOpen = false;
            }}
            onLoadSkillMarkdown={loadSkillMarkdown}
            onExportSkillMarkdown={handleExportSkillMarkdown}
            onNavigateToSettings={() => selectSection('settings')}
            onNotify={showToast}
          />
        </section>
      {:else if activeSection === 'settings'}
        <section class="workspace-view workspace-settings-view" aria-labelledby="view-heading-settings" data-view="settings">
          <header class="view-hero">
            <div class="view-hero-copy">
              <h1 id="view-heading-settings" class="view-title">{$t('设置')}</h1>
              <p class="view-description">{sectionMeta.description}</p>
            </div>
          </header>

          <div class="workspace-settings-surface">
            <SettingsPanel
              {preferences}
              themes={allThemes}
              {activeTheme}
              {customThemes}
              {desktopMaterialSupported}
              {importingTheme}
              {themeError}
              {serviceConnected}
              {serviceUrl}
              {serviceEpoch}
              {serviceConnectionError}
              bountyProjects={settingsBountyProjects}
              {showArchivedProjects}
              {onboardingSettings}
              {onboardingLoaded}
              {onboardingLoadError}
              {skillDocument}
              desktop={capabilities.desktop}
              mobile={Boolean(files.isMobile)}
              onSelectLanguage={handleSelectLanguage}
              onSelectThemeId={handleSelectThemeId}
              onSelectThemeMode={handleSelectThemeMode}
              onToggleKeepTransparentOnBlur={handleToggleKeepTransparentOnBlur}
              onChangeFontFamily={handleChangeFontFamily}
              onChangeFontSize={handleChangeFontSize}
              onChangeLineWidth={handleChangeLineWidth}
              onChangeParagraphIndent={handleChangeParagraphIndent}
              onImportThemeFile={handleImportThemeFile}
              onExportThemeTemplate={handleExportThemeTemplate}
              onRemoveActiveTheme={handleRemoveActiveTheme}
              onSaveServiceConnection={handleSaveServiceConnection}
              onToggleShowArchivedProjects={show => {
                showArchivedProjects = show;
              }}
              onCreateBountyProject={handleCreateBountyProject}
              onUpdateBountyProject={handleUpdateBountyProject}
              onDeleteBountyProject={handleDeleteBountyProject}
              onRetryLoadOnboarding={refreshOnboarding}
              onToggleLanRegistration={handleToggleLanRegistration}
              onCreateInviteToken={handleCreateInviteToken}
              onRevokeInviteToken={handleRevokeInviteToken}
              onLoadSkillMarkdown={loadSkillMarkdown}
              onExportSkillMarkdown={handleExportSkillMarkdown}
              onNotify={showToast}
            />
          </div>
        </section>
      {/if}
    </div>
  </main>

  {#if activeDetail}
    <BountyDetailDrawer
      detail={activeDetail}
      project={bountyProjectsById.get(activeDetail.bounty.project_id)}
      onClose={() => {
        detailRequestSeq += 1;
        activeDetail = null;
      }}
      onEditDraft={openEditDraftComposer}
      onPublish={handlePublishFromDrawer}
      onWithdraw={handleWithdrawFromDrawer}
      onCancel={handleCancelFromDrawer}
      onReleaseLease={handleReleaseLeaseFromDrawer}
      onCreateComment={handleCreateCommentFromDrawer}
    />
  {/if}

  {#if composerOpen}
    <BountyComposerModal
      projects={allBountyProjects}
      initialProjectId={bountyProjectFilter !== 'ALL' ? bountyProjectFilter : null}
      {editingBounty}
      desktop={capabilities.desktop}
      mobile={Boolean(files.isMobile)}
      onClose={() => {
        composerOpen = false;
        editingBounty = null;
      }}
      onCreateProject={handleCreateBountyProject}
      onCreateBounty={handleCreateBounty}
      onUpdateDraft={handleUpdateDraft}
    />
  {/if}

  {#if iconPickerOpen}
    <div
      class="modal-backdrop"
      role="presentation"
      onclick={event => {
        if (event.target === event.currentTarget) iconPickerOpen = false;
      }}
    >
      <div class="modal-dialog icon-picker-dialog" role="dialog" aria-modal="true" aria-label={$t('设置项目图标')}>
        <div class="modal-header">
          <h2 class="modal-title">{$t('设置项目图标')}</h2>
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
          projectName={activeProject?.name ?? selectedBountyProject?.name ?? $t('全部项目')}
          icon={currentProjectIcon}
          mobile={Boolean(files.isMobile)}
          onSelect={handlePickProjectIcon}
          onReset={handleResetProjectIcon}
        />
      </div>
    </div>
  {/if}

  {#if updateDialogOpen}
    <div
      class="modal-backdrop"
      role="presentation"
      onclick={event => {
        if (event.target === event.currentTarget) updateDialogOpen = false;
      }}
    >
      <div class="modal-dialog update-dialog" role="dialog" aria-modal="true" aria-label={$t('应用更新')}>
        <div class="modal-header">
          <h2 class="modal-title">{$t('应用更新')}</h2>
          <button
            type="button"
            class="icon-button"
            aria-label={$t('关闭对话框')}
            onclick={() => {
              updateDialogOpen = false;
            }}
          >
            ✕
          </button>
        </div>
        <UpdatePanel
          supported={updateSupported}
          mobile={Boolean(files.isMobile)}
          {currentVersion}
          status={updateStatus}
          autoCheck={preferences.autoCheckUpdates ?? true}
          autoDownload={preferences.autoDownloadUpdates ?? true}
          onCheck={() => void updateCoordinator.check()}
          onDownload={() => void updateCoordinator.download()}
          onInstall={() => void updateCoordinator.install()}
          onRetry={() => void updateCoordinator.retry()}
          onPreferences={handleUpdatePreferences}
          onOpenLink={url => void openExternalLink(url)}
        />
      </div>
    </div>
  {/if}

  {#if toast}
    <div class="toast-bar" role="status" aria-live="polite">
      <span>{toast}</span>
      <button
        type="button"
        class="icon-button toast-dismiss"
        aria-label={$t('关闭提示')}
        onclick={() => {
          toast = '';
        }}
      >
        ✕
      </button>
    </div>
  {/if}
</div>
