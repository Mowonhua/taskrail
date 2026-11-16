/**
 * 文件职责：定义前端工作台外壳的数据契约与平台端口接口。
 * 定义范围：工作台顶层导航分区、项目元数据、界面与排版偏好、窗口位置、应用配置及平台持久化端口。
 */
import type { LocalePreference } from './i18n';
import type { ThemeDefinition, ThemeMode } from './themes';

/** 结构职责：定义悬赏板工作台左侧紧凑导航的四个顶层入口。 */
export type WorkspaceSection = 'bounties' | 'inbox' | 'agents' | 'settings';

/** 结构职责：表示工作台关联的项目元数据与可选图标配置。 */
export interface Project {
  id: string;
  name: string;
  path: string;
  /** 缺省使用文件夹图标；格式为 emoji:<字素> 或 lucide:<kebab-case 名称>。 */
  icon?: string;
  archived?: boolean;
}

/** 结构职责：保存用户界面、主题、窗口材质与阅读排版偏好。 */
export interface Preferences {
  /** 缺省保持简体中文；system 在启动与系统语言变化时解析为受支持语言。 */
  locale?: LocalePreference;
  /** 兼容旧版或别名持久化字段。 */
  language?: LocalePreference;
  theme: ThemeMode;
  themeId?: string;
  /** 仅桌面玻璃材质主题使用；缺省为 false，保留 Windows 失焦回落实色的默认行为。 */
  keepTransparentOnBlur?: boolean;
  fontFamily: string;
  fontSize: number;
  contentWidth: number;
  /** 兼容读取/写回行宽别名字段。 */
  lineWidth?: number;
  /** 段首缩进字符数（0–4，0 表示关闭；兼容旧版布尔值）。 */
  paragraphIndent: number;
  sidebarOpen?: boolean;
  /** 兼容读取/写回侧栏折叠别名字段。 */
  sidebarCollapsed?: boolean;
  /** 启动或切换时激活的顶层工作台分区，缺省为悬赏板（bounties）。 */
  workspaceSection?: WorkspaceSection;
  /** 无关联项目时使用的默认项目图标（格式同 Project.icon）。 */
  defaultProjectIcon?: string;
  autoCheckUpdates?: boolean;
  autoDownloadUpdates?: boolean;
}

/** 结构职责：保存桌面窗口尺寸与坐标状态。 */
export interface WindowPlacement {
  width?: number;
  height?: number;
  x?: number;
  y?: number;
  maximized?: boolean;
}

/** 结构职责：应用持久化配置根结构。 */
export interface AppConfig {
  projects: Project[];
  activeProjectId: string | null;
  customThemes?: ThemeDefinition[];
  preferences: Preferences;
  window?: WindowPlacement;
}

/**
 * 接口职责：隔离桌面、移动端与浏览器预览的配置持久化、文本导出与外链打开能力。
 * 约束条件：不绑定已废弃的单文件读写、文件监听、草稿恢复或局域网同步命令。
 */
export interface FilePort {
  isBrowser: boolean;
  isMobile?: boolean;
  loadConfig(): Promise<AppConfig | null>;
  saveConfig(config: AppConfig): Promise<void>;
  exportText?(text: string, filename: string): Promise<boolean>;
  openExternalLink?(url: string): Promise<void>;
  openLocalDocument?(path: string): Promise<void>;
}
