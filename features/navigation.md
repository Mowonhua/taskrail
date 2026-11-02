# 项目导航

> 模块职责：项目列表的标识、切换与排序。

## F-NAV-001 项目图标

每个项目可保存 Unicode 表情、Lucide 线性图标或单个字符，在左侧项目列表区分项目。图标属于本机界面配置，不写入 Markdown；没有自定义图标时显示通用文档标识。选择器按“表情/图标”分页：表情提供完整 Unicode 分类和按界面语言搜索，图标提供精选 Lucide 分类并可检索完整集合的名称、别名和中文词；两组数据懒加载，支持网格方向键、自定义输入和恢复默认。

创建提交：afa4a29

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src/lib/ProjectIcon.svelte::<module> |
| 关键实现 | src/lib/ProjectIconPicker.svelte::<module> |
| 关键实现 | src/lib/project-icons.ts::parseProjectIcon |
| 关键实现 | src/lib/project-icons.ts::serializeProjectIcon |
| 关键实现 | src/lib/emoji-icons.ts::loadEmojiDataset |
| 关键实现 | src/lib/emoji-icons.ts::searchEmojis |
| 关键实现 | src/lib/lucide-icons.ts::loadLucideDataset |
| 关键实现 | src/lib/lucide-icons.ts::searchLucideIcons |
| 关键实现 | src/App.svelte::openIconPicker |
| 关键实现 | src/App.svelte::saveProjectIcon |
| 关键实现 | src/lib/config-validation.ts::validateAppConfig |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 项目表情图标以 `emoji:表情` 保存到本机配置并在重启后恢复；从项目菜单编辑其他项目不切换当前正文 | src/lib/session/app-integration.test.ts::项目图标 > 默认显示通用标识\，右键其他项目可选择图标且不切换当前项目 |
| S-02 | 正常 | 无自定义图标时显示通用文档标识；表情页签可搜索推荐之外的完整 Unicode 数据，中文名称和标签参与匹配；可输入单个自定义字符或恢复默认，多字素输入留在对话框并提示 | src/lib/session/app-integration.test.ts::项目图标 > 搜索可找到推荐之外的表情并写回配置, src/lib/session/app-integration.test.ts::项目图标 > 可从更多菜单自定义图标、拒绝多字素输入并恢复默认, src/lib/emoji-icons.test.ts::表情数据 > 中文名称和标签可搜索\，结果优先精确名称 |
| S-03 | 正常 | 图标页签提供精选 Lucide 分类，完整集合可按英文名称、别名和中文分类词检索；选中后保存 `lucide:名称`，侧栏渲染跟随主题的线性图标，重启后恢复 | src/lib/session/app-integration.test.ts::项目图标 > 可切换到 Lucide 图标、搜索并持久化\，重启后恢复同一图标, src/lib/lucide-icons.test.ts::Lucide 图标数据 > 英文名称、别名和中文分类词均可搜索, src/lib/lucide-icons.test.ts::Lucide 图标数据 > 精选分类和常用图标全部存在于本地数据 |
| S-04 | 约束 | 配置边界只接受 `emoji:表情` 或 `lucide:名称`，拒绝裸表情、空值、多字素、非法图标名和非字符串，使配置整体失效；当前数据包的全部图标名都满足该规则 | src/lib/config-validation.test.ts::配置边界 > 项目图标只接受 emoji 和 lucide 前缀\，拒绝裸表情、多字素与非法名称, src/lib/project-icons.test.ts::项目图标引用 > 拒绝裸表情、空值、多个字素、非法图标名和非字符串, src/lib/lucide-icons.test.ts::Lucide 图标数据 > 所有图标名都能写入项目配置 |

## F-NAV-002 项目阅读位置恢复

切换项目时保存各自的视图模式、光标、折叠与滚动位置，返回项目后恢复阅读位置。

创建提交：-

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src/App.svelte::openProject |
| 主入口 | src/App.svelte::openInspiration |
| 关键实现 | src/lib/editor/index.ts::EditorController.restoreState |
| 关键实现 | src/lib/editor/index.ts::EditorController.applyUIState |
| 关键实现 | src/lib/editor/index.ts::EditorController.restoreScrollTop |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 桌面与手机在待办、归档和源码视图切换项目后恢复原滚动位置，连续切换不累积下移；覆盖顶部、靠近顶部、中部与底部，原位置超出重建后的可滚动范围时停在底部 | tests/browser-project-scroll.ts::verify, scripts/test-project-scroll.mjs::<module> |
| S-02 | 并发 | 用户主动滚动或快速切换到另一个项目后，旧的延迟恢复失效，不覆盖用户或新项目的滚动位置 | tests/browser-project-scroll.ts::verifyCancellation, scripts/test-project-scroll.mjs::<module> |
