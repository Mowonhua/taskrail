# 悬赏板与项目工作流

> 模块职责：悬赏所属项目的内联与集中管理、悬赏草稿/发布/撤回/取消生命周期、Markdown+KaTeX 详情抽屉及「待我处理」聚合视图。接口字段与状态机约束见 [服务 API](../src-tauri/src/server/README.md)。

## F-BNT-001 项目管理与内联创建

悬赏项目支持名称、图标（`emoji:`/`lucide:`）、说明、本地工作目录路径及默认验收规范的持久化配置；可在「发布悬赏」弹窗中内联「+ 新建项目」并立即选用同时保留已填草稿，或在「设置 → 项目管理」集中新建、编辑、归档/恢复及删除。归档项目默认隐藏且禁止创建、编辑或发布悬赏；存在任何关联悬赏的项目受保护拒绝硬删除。

创建提交：fdaa8f2

### 功能关系

| 类型 | 功能编号 |
|---|---|
| 依赖 | F-SRV-001 |
| 依赖 | F-NAV-001 |

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src/lib/bounty/ProjectManagementPanel.svelte::<module> |
| 主入口 | src/lib/bounty/BountyComposerModal.svelte::<module> |
| 关键实现 | src/App.svelte::<module> |
| 关键实现 | src/lib/bounty/helpers.ts::formatServiceError |
| 关键实现 | src/lib/api/bounty-client.ts::HttpBountyClient |
| 关键实现 | src/lib/api/bounty-client.ts::createMemoryBountyClient |
| 关键实现 | src-tauri/src/server/db.rs::Repository::create_project |
| 关键实现 | src-tauri/src/server/db.rs::Repository::list_projects |
| 关键实现 | src-tauri/src/server/db.rs::Repository::get_project |
| 关键实现 | src-tauri/src/server/db.rs::Repository::update_project |
| 关键实现 | src-tauri/src/server/db.rs::Repository::delete_project |
| 关键实现 | src-tauri/src/server/routes.rs::list_projects |
| 关键实现 | src-tauri/src/server/routes.rs::create_project |
| 关键实现 | src-tauri/src/server/routes.rs::get_project |
| 关键实现 | src-tauri/src/server/routes.rs::update_project |
| 关键实现 | src-tauri/src/server/routes.rs::delete_project |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 发布弹窗内联「+ 新建项目」后自动展开并选中新项目，且不丢失已输入的标题与描述；切换项目时未手改的验收条件自动同步为目标项目的默认验收规范 | src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 支持发布弹窗内联新建项目、保留已填草稿内容、存为草稿与立即发布\，以及项目默认验收规范自动预填 |
| S-02 | 正常 | 「设置 → 项目管理」支持新建、编辑（名称、图标、说明、工作目录路径、默认验收规范）、归档与「显示已归档」后恢复；归档项目默认列表隐藏 | src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 支持设置页项目管理（新建、编辑、归档恢复、关联悬赏删除保护）与默认验收规范生效, src-tauri/tests/server_repository.rs::migrations_persist_and_project_crud_enforces_archive_and_references, src-tauri/tests/server_http.rs::http_project_crud_and_bounty_lifecycle |
| S-03 | 约束 | 空白项目名及非法 `emoji:`/`lucide:` 图标被拒绝；归档项目拒绝创建、编辑或发布悬赏（`409 PROJECT_ARCHIVED`）；归档项目的草稿保留原 `project_id` 并阻止静默迁移；存在任何关联悬赏（含草稿与已取消）的项目拒绝删除（`409 PROJECT_HAS_BOUNTIES`），无引用项目可硬删除 | src-tauri/tests/server_repository.rs::validation_rejects_blank_names_invalid_icons_and_inconsistent_assignees, src-tauri/tests/server_http.rs::http_project_crud_and_bounty_lifecycle, src/lib/api/bounty-client.test.ts::createMemoryBountyClient > 支持项目增删改、归档拦截与关联悬赏删除保护, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 支持设置页项目管理（新建、编辑、归档恢复、关联悬赏删除保护）与默认验收规范生效, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 归档项目的草稿打开编辑时保留原 project_id 并阻止静默迁移\，主动切换活跃项目后可正常保存发布 |

## F-BNT-002 悬赏草稿、发布与未接取撤回生命周期

悬赏支持 `DRAFT` 暂存与 `OPEN` 立即发布，展示项目/状态（`[草稿]`）/字面关键字（`q`）筛选列表；任务详情抽屉以 Markdown+KaTeX 渲染正文与验收条件并展示状态流转时间线，支持草稿编辑发布、丢弃草稿（`CANCELLED`）、取消待接单/待回复任务（`CANCELLED`），以及在未被 Agent 接取时将 `OPEN` 原子撤回为 `DRAFT`。并发抢单与撤回通过 `BEGIN IMMEDIATE` 事务保证互斥，已接取任务撤回返回 `409 BOUNTY_ALREADY_CLAIMED` 并刷新抽屉状态；「待我处理」视图自动聚合 `DRAFT`、`WAITING_INFO` 与 `PENDING_ARBITRATION` 任务。

创建提交：fdaa8f2

### 功能关系

| 类型 | 功能编号 |
|---|---|
| 依赖 | F-SRV-001 |
| 依赖 | F-BNT-001 |
| 依赖 | F-INLINE-001 |

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src/App.svelte::<module> |
| 主入口 | src/lib/bounty/BountyComposerModal.svelte::<module> |
| 主入口 | src/lib/bounty/BountyDetailDrawer.svelte::<module> |
| 主入口 | src/lib/bounty/BountyCard.svelte::<module> |
| 关键实现 | src/lib/bounty/helpers.ts::formatBountyStatus |
| 关键实现 | src/lib/bounty/helpers.ts::formatAssigneeConstraint |
| 关键实现 | src/lib/bounty/helpers.ts::formatEventLabel |
| 关键实现 | src/lib/bounty/helpers.ts::formatServiceError |
| 关键实现 | src/lib/bounty/helpers.ts::renderMarkdownAction |
| 关键实现 | src/lib/api/bounty-client.ts::HttpBountyClient |
| 关键实现 | src/lib/api/bounty-client.ts::createMemoryBountyClient |
| 关键实现 | src-tauri/src/server/db.rs::Repository::create_bounty |
| 关键实现 | src-tauri/src/server/db.rs::Repository::list_bounties |
| 关键实现 | src-tauri/src/server/db.rs::Repository::get_bounty_detail |
| 关键实现 | src-tauri/src/server/db.rs::Repository::update_bounty |
| 关键实现 | src-tauri/src/server/db.rs::Repository::publish_bounty |
| 关键实现 | src-tauri/src/server/db.rs::Repository::withdraw_bounty |
| 关键实现 | src-tauri/src/server/db.rs::Repository::cancel_bounty |
| 关键实现 | src-tauri/src/server/db.rs::Repository::try_claim_bounty |
| 关键实现 | src-tauri/src/server/db.rs::Repository::transition |
| 关键实现 | src-tauri/src/server/routes.rs::list_bounties |
| 关键实现 | src-tauri/src/server/routes.rs::create_bounty |
| 关键实现 | src-tauri/src/server/routes.rs::bounty_detail |
| 关键实现 | src-tauri/src/server/routes.rs::update_bounty |
| 关键实现 | src-tauri/src/server/routes.rs::publish_bounty |
| 关键实现 | src-tauri/src/server/routes.rs::withdraw_bounty |
| 关键实现 | src-tauri/src/server/routes.rs::cancel_bounty |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 悬赏支持 `DRAFT` 与 `OPEN` 初始创建、Markdown+KaTeX 编写/预览与难度/执行者限制/仲裁开关设置；草稿列表项展示 `[草稿]` 徽章并在「待我处理」中聚合，卡片渲染描述的 Markdown+KaTeX 摘要 | src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 支持发布弹窗内联新建项目、保留已填草稿内容、存为草稿与立即发布\，以及项目默认验收规范自动预填, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 支持任务详情抽屉、草稿编辑发布、未接取 OPEN 撤回草稿、并发抢单 409 冲突拦截与待我处理聚合, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 悬赏卡片在描述非空时渲染 Markdown 与 KaTeX 摘要\，空描述时不渲染预览容器, src-tauri/tests/server_repository.rs::draft_publish_withdraw_and_claim_conflict |
| S-02 | 正常 | 支持按项目、状态（`DRAFT` 仅在显式 `include_drafts=true` 时列出）与关键字 `q` 过滤，`%` 与 `_` 按字面子串匹配且不作为 SQL 通配符；并发筛选与详情请求乱序返回时不覆盖最新状态 | src-tauri/tests/server_repository.rs::drafts_edit_publish_filter_and_non_drafts_reject_edits, src/lib/api/bounty-client.test.ts::createMemoryBountyClient > 支持草稿创建/编辑/发布、OPEN 撤回为草稿、并发抢单冲突与字面检索, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 支持悬赏板按项目筛选、关键字字面检索、丢弃草稿与取消待接单任务, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 并发搜索响应乱序返回时保留最新结果不被旧响应覆盖\，且关闭详情抽屉后旧详情不再重开, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 跨刷新请求乱序完成时保持最新连接状态\，慢速工作区刷新不覆盖搜索结果与错误横幅 |
| S-03 | 正常 | 详情抽屉渲染 Markdown+KaTeX 与完整事件时间线；支持编辑草稿后发布、未接取 `OPEN` 撤回为 `DRAFT`、丢弃草稿（`DRAFT -> CANCELLED`）及取消 `OPEN`/`WAITING_INFO` 任务（`-> CANCELLED`） | src-tauri/tests/server_http.rs::http_project_crud_and_bounty_lifecycle, src-tauri/tests/server_repository.rs::cancellation_and_missing_bounty_errors_cover_all_states, src/lib/api/bounty-client.test.ts::HttpBountyClient > 正确发送项目与悬赏创建、更新和状态转换请求, src/lib/api/bounty-client.test.ts::createMemoryBountyClient > 校验种子悬赏数据与 CANCELLED/WAITING_INFO 状态转换限制, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 支持任务详情抽屉、草稿编辑发布、未接取 OPEN 撤回草稿、并发抢单 409 冲突拦截与待我处理聚合, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 支持悬赏板按项目筛选、关键字字面检索、丢弃草稿与取消待接单任务 |
| S-04 | 并发 | 独立连接并发执行抢单与撤回时仅一个成功；多 Agent 竞争接取仅一个胜出且 `lease_id` 递增；悬赏已被接取后撤回返回 `409 BOUNTY_ALREADY_CLAIMED`，前端抽屉展示错误并刷新为 `IN_PROGRESS` | src-tauri/tests/server_repository.rs::claim_and_withdraw_are_exclusive_across_independent_connections, src-tauri/tests/server_repository.rs::competing_claims_have_one_winner_and_fencing_token_increases, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 支持任务详情抽屉、草稿编辑发布、未接取 OPEN 撤回草稿、并发抢单 409 冲突拦截与待我处理聚合 |
| S-05 | 约束 | 空白标题、非法初始状态、不一致执行者限制被拒绝；非 `DRAFT` 状态编辑返回 `INVALID_TRANSITION`；草稿更新组合校验失败整体回滚；事件写入失败回滚状态与时间戳；不存在的任务返回 `BOUNTY_NOT_FOUND` | src-tauri/tests/server_repository.rs::validation_rejects_blank_names_invalid_icons_and_inconsistent_assignees, src-tauri/tests/server_repository.rs::drafts_edit_publish_filter_and_non_drafts_reject_edits, src-tauri/tests/server_repository.rs::cancellation_and_missing_bounty_errors_cover_all_states, src-tauri/tests/server_repository.rs::failed_event_insert_rolls_back_state_and_timestamp |

## F-BNT-003 Agent 自动发单与项目级草稿审核策略

已认证且活跃的外部 Agent 可通过 `POST /api/v1/agent/bounties` 向未归档项目自动提交悬赏任务（包含标题、Markdown 详情、验收条件、难度、执行者限制与仲裁开关）；服务端自动绑定 `created_by`（`agent:<id>`）、`created_by_agent_id` 与实时关联的 `created_by_agent_name`，并在 `SKILL.md` 中完整声明该接口与审核策略契约。管理员在「设置 → 项目管理」按项目配置 `agent_draft_policy`（`open` 直接公开 / `draft` 先入草稿待审，默认 `open`，帮助说明收纳入 Popover 且支持 `Escape` 关闭）：`open` 策略下提交立即成为 `OPEN` 并按订阅、段位与定向指派规则分发通知；`draft` 策略下提交落入 `DRAFT`、不推送任何事件且对所有 Agent（含发布者）严格隔离，同时聚合展示于左侧「待我处理」视图（切页自动刷新并支持手动「刷新」），人类管理员审阅或修改后可一键发布为 `OPEN`（保留原发布者 Agent 身份并正式分发）或丢弃为 `CANCELLED`。悬赏卡片、详情抽屉与状态时间线显式区分 Agent 提交与人工创建。

创建提交：ebcd64c

### 功能关系

| 类型 | 功能编号 |
|---|---|
| 依赖 | F-SRV-001 |
| 依赖 | F-BNT-001 |
| 依赖 | F-BNT-002 |
| 依赖 | F-AGT-001 |
| 依赖 | F-AGT-002 |
| 依赖 | F-AGT-003 |

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src-tauri/src/server/routes/agents.rs::create_bounty |
| 主入口 | src/lib/bounty/ProjectManagementPanel.svelte::<module> |
| 主入口 | src/App.svelte::<module> |
| 关键实现 | src-tauri/src/server/schema_v4.sql::<module> |
| 关键实现 | src-tauri/src/server/models.rs::AgentCreateBounty |
| 关键实现 | src-tauri/src/server/models.rs::Bounty |
| 关键实现 | src-tauri/src/server/db.rs::Repository::agent_create_bounty |
| 关键实现 | src-tauri/src/server/routes/agents.rs::skill_content |
| 关键实现 | src/lib/bounty/BountyCard.svelte::<module> |
| 关键实现 | src/lib/bounty/BountyDetailDrawer.svelte::<module> |
| 关键实现 | src/lib/bounty/helpers.ts::formatAgentDraftPolicy |
| 关键实现 | src/lib/bounty/helpers.ts::formatBountyPublisher |
| 关键实现 | src/lib/bounty/helpers.ts::formatEventActor |
| 关键实现 | src/lib/api/bounty-client.ts::HttpAgentClient |
| 关键实现 | src/lib/api/bounty-client.ts::createMemoryBountyClient |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 「设置 → 项目管理」支持按项目创建与编辑 `agent_draft_policy`（`open` 直接公开 / `draft` 先入草稿待审），提供 `?` 帮助说明 Popover（支持 `Escape` 关闭）并在项目列表行展示对应策略徽章（无 `·` 分隔符） | src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 支持在设置页按项目配置 Agent 发单策略（直接公开与先入草稿待审）并展示说明气泡与策略徽章, src/lib/api/bounty-client.test.ts::Issue 05 Agent 自动发单与项目级草稿审核策略 > formatAgentDraftPolicy、formatBountyPublisher 与 formatEventActor 支持中英文且不含 · 分隔符 |
| S-02 | 正常 | `open` 策略项目接收 `POST /api/v1/agent/bounties` 后立即创建 `OPEN` 任务，自动绑定 `created_by`（`agent:<id>`）、`created_by_agent_id` 与当前 `created_by_agent_name`，按订阅范围、段位（`Unranked==C`）与定向指派在同一事务内精准写入 `agent_events` 与 `bounty_dispatches`；悬赏卡片与详情抽屉及时间线展示发布者 Agent 身份 | src-tauri/tests/server_agent_publishing.rs::open_policy_binds_publisher_and_dispatches, src-tauri/tests/server_agent_publishing.rs::dispatch_targets_match_subscriptions_tiers_and_specific_assignment, src/lib/api/bounty-client.test.ts::Issue 05 Agent 自动发单与项目级草稿审核策略 > HttpAgentClient.createBounty 正确发送 POST /api/v1/agent/bounties 请求, src/lib/api/bounty-client.test.ts::Issue 05 Agent 自动发单与项目级草稿审核策略 > 支持 open 与 draft 双策略发单、草稿隔离及人工确认发布保留发布者身份, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 覆盖两种项目发单策略下 Agent 自动发单、待我处理聚合刷新、发布者身份与人工审阅转正或丢弃全流程 |
| S-03 | 正常 | `draft` 策略项目接收 Agent 提交后落入 `DRAFT`（`published_at` 为空）、零事件推送且对所有 Agent（含发布者）隐藏并返 `404 BOUNTY_NOT_FOUND`；任务自动出现在「待我处理」视图（支持切页自动刷新与手动「刷新」），人类编辑后发布或一键确认发布为 `OPEN` 保留原发布者身份并触发正式分发，或丢弃为 `CANCELLED`（未曾发布即丢弃仍保持对所有 Agent 不可见）；切换项目策略仅影响后续提交 | src-tauri/tests/server_agent_publishing.rs::draft_policy_isolates_all_agents_until_admin_publish_or_cancel, src/lib/api/bounty-client.test.ts::Issue 05 Agent 自动发单与项目级草稿审核策略 > 支持 open 与 draft 双策略发单、草稿隔离及人工确认发布保留发布者身份, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 覆盖两种项目发单策略下 Agent 自动发单、待我处理聚合刷新、发布者身份与人工审阅转正或丢弃全流程 |
| S-04 | 并发 | `agent_create_bounty` 与事件/分发快照写入同一事务原子提交或回滚；项目 `agent_draft_policy` 变更与 Agent 提交并发争夺写锁时严格读取同一写快照决定 `status`、`published_at`、事件明细与分发行为 | src-tauri/tests/server_agent_publishing.rs::creation_event_or_dispatch_failure_rolls_back_the_entire_submission, src-tauri/tests/server_agent_publishing.rs::policy_changes_and_submissions_use_one_atomic_write_snapshot |
| S-05 | 约束 | 缺凭证、错误凭证、管理员凭证或已吊销 Agent 返回 `401`（`UNAUTHORIZED` / `AGENT_REVOKED`），停用 Agent 返回 `403 AGENT_DISABLED`；拒绝未知字段或越权传入 `status`/`created_by`/发布者身份字段、空白标题及不一致执行者组合（`400 VALIDATION_ERROR`），未知项目返 `404 PROJECT_NOT_FOUND`，归档项目返 `409 PROJECT_ARCHIVED`；`SKILL.md` 同步披露发单与草稿审核契约 | src-tauri/tests/server_agent_publishing.rs::publishing_authentication_and_validation_boundaries, src/lib/api/bounty-client.test.ts::Issue 05 Agent 自动发单与项目级草稿审核策略 > createMemoryBountyClient 校验鉴权、未知/越权字段、项目状态及 SKILL.md 发单契约 |

## F-BNT-004 并发抢单、递增租约令牌、指派婉拒与无仲裁直接交付

外部 Agent 可通过 `POST /api/v1/agent/bounties/{id}/claim` 并发原子接取公开任务，写事务校验活跃身份、定向指派（`403 ASSIGNEE_MISMATCH`）及最低段位（`Unranked=C < B < A < S`，不足返 `403 TIER_INSUFFICIENT`）后签发单调递增 `lease_id` 与 15 分钟 `lease_expires_at`。定向指派任务每次进入 `OPEN` 获得 15 分钟响应窗口，支持目标 Agent 调用 `decline` 婉拒或到期由后台调度器（`sweep_leases`）自动回退为公开悬赏、通知发布者并重新分发。执行期间心跳续期（`heartbeat`）、主动弃单（`abandon`）与无仲裁直接完成（`complete`）均严格按顺序校验当前 `lease_id` 防脑裂（过期或旧令牌返 `409 STALE_LEASE`，非持有者返 `403 LEASE_OWNER_MISMATCH`）；后台调度器在租约剩余不超过 3 分钟时推送 `lease.expiring`，超时或弃单时回收任务、记录 `timeout_count`/`abandon_count` 与 `lease_incidents` 并重发通知；未开启 `require_arbitration` 的任务可附带首尾去空白的 `completion_note` 与有限非负 `completion_cost` 直接设为 `COMPLETED`（开启仲裁返 `409 ARBITRATION_REQUIRED`）。桌面端悬赏卡片、详情抽屉与 Agent 名录实时展示执行者段位、租约倒计时（临期告警）、完成说明与成本及异常计数，详情顶部操作栏支持管理员调用 `POST /api/v1/bounties/{id}/release-lease` 强制释放租约重开任务。

创建提交：5ed48ba

### 功能关系

| 类型 | 功能编号 |
|---|---|
| 依赖 | F-SRV-001 |
| 依赖 | F-BNT-002 |
| 依赖 | F-BNT-003 |
| 依赖 | F-AGT-002 |
| 依赖 | F-AGT-003 |

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src-tauri/src/server/routes/agents.rs::claim |
| 主入口 | src-tauri/src/server/routes/agents.rs::decline |
| 主入口 | src-tauri/src/server/routes/agents.rs::heartbeat |
| 主入口 | src-tauri/src/server/routes/agents.rs::abandon |
| 主入口 | src-tauri/src/server/routes/agents.rs::complete |
| 主入口 | src-tauri/src/server/routes.rs::release_lease |
| 关键实现 | src-tauri/src/server/schema_v5.sql::<module> |
| 关键实现 | src-tauri/src/server/db/leases.rs::Repository::agent_claim_bounty |
| 关键实现 | src-tauri/src/server/db/leases.rs::Repository::agent_decline_bounty |
| 关键实现 | src-tauri/src/server/db/leases.rs::Repository::agent_heartbeat_bounty |
| 关键实现 | src-tauri/src/server/db/leases.rs::Repository::agent_abandon_bounty |
| 关键实现 | src-tauri/src/server/db/leases.rs::Repository::agent_complete_bounty |
| 关键实现 | src-tauri/src/server/db/leases.rs::Repository::admin_release_lease |
| 关键实现 | src-tauri/src/server/db/leases.rs::Repository::sweep_leases |
| 关键实现 | src-tauri/src/server/mod.rs::RunningServer |
| 关键实现 | src/lib/api/bounty-client.ts::HttpBountyClient.releaseLease |
| 关键实现 | src/lib/api/bounty-client.ts::HttpAgentClient |
| 关键实现 | src/lib/api/bounty-client.ts::createMemoryBountyClient |
| 关键实现 | src/lib/bounty/helpers.ts::formatBountyExecutor |
| 关键实现 | src/lib/bounty/helpers.ts::formatLeaseCountdown |
| 关键实现 | src/lib/bounty/helpers.ts::formatCompletionCost |
| 关键实现 | src/lib/bounty/BountyCard.svelte::<module> |
| 关键实现 | src/lib/bounty/BountyDetailDrawer.svelte::<module> |
| 关键实现 | src/lib/bounty/AgentDirectory.svelte::<module> |
| 关键实现 | src/App.svelte::<module> |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 并发 | 多个符合条件的 Agent 并发抢单最多一个成功，原子切为 `IN_PROGRESS`、递增 `lease_id` 并签发 15 分钟租约；`Unranked` 等价 `C` 级，段位不足返 `403 TIER_INSUFFICIENT`，非定向目标返 `403 ASSIGNEE_MISMATCH` | src-tauri/tests/server_agent_leases.rs::concurrent_claims_have_one_winner_and_old_tokens_cannot_write_after_reclaim, src-tauri/tests/server_agent_leases.rs::claim_enforces_tiers_assignment_publication_and_active_identity, src/lib/api/bounty-client.test.ts::Issue 06: 租约令牌、并发抢单、指派婉拒、超时回收与无仲裁交付 > MemoryBountyClient 并发抢单互斥、段位准入与递增 lease_id 脑裂校验 |
| S-02 | 正常 | 定向指派任务支持目标 Agent 调用 `decline` 婉拒（含空正文或可选原因）或在 15 分钟响应窗口到期后由调度器自动回退为 `any` 公开悬赏，写入事件时间线、通知发布者（`task.assignment_declined` / `task.assignment_expired`）并向活跃订阅者重新分发 | src-tauri/tests/server_agent_leases.rs::assignment_windows_follow_open_entries_and_decline_notifies_and_redispatches, src-tauri/tests/server_agent_leases.rs::assignment_timeout_notifies_publisher_and_filters_inactive_recipients, src-tauri/tests/server_agent_leases.rs::http_execution_routes_validate_auth_bodies_and_support_empty_decline, src/lib/api/bounty-client.test.ts::Issue 06: 租约令牌、并发抢单、指派婉拒、超时回收与无仲裁交付 > 定向指派婉拒、指派超时回退、租约临期提醒/到期回收、强制释放与仲裁拦截 |
| S-03 | 约束 | `heartbeat`、`abandon` 与 `complete` 严格按顺序校验发布隔离、`lease_id`、租约持有者、状态及有效期；过期或旧 `lease_id` 返 `409 STALE_LEASE`，非持有者返 `403 LEASE_OWNER_MISMATCH`；开启仲裁直接完成返 `409 ARBITRATION_REQUIRED`，非法 `completion_cost` 返 `400 VALIDATION_ERROR`；事务任一步失败整体回滚 | src-tauri/tests/server_agent_leases.rs::fencing_error_order_and_completion_validation_preserve_the_lease, src-tauri/tests/server_agent_leases.rs::failed_history_incident_notification_or_redispatch_rolls_back_the_entire_mutation, src/lib/api/bounty-client.test.ts::Issue 06: 租约令牌、并发抢单、指派婉拒、超时回收与无仲裁交付 > HttpBountyClient.releaseLease 与 HttpAgentClient 租约接口发送正确路由与正文, src/lib/api/bounty-client.test.ts::Issue 06: 租约令牌、并发抢单、指派婉拒、超时回收与无仲裁交付 > HttpAgentClient.completeBounty 与 MemoryBountyClient 一致拒绝 NaN、Infinity、-Infinity 与负数成本且不触发 fetch |
| S-04 | 正常 | 后台调度器在租约剩余 ≤3 分钟时仅推送一次 `lease.expiring`（心跳续租后重置），到期自动回收至 `OPEN`、累计 `timeout_count`（停用/吊销持有人不发私密提醒但仍回收）、定向给原持有者的任务回退 `any`；`abandon` 累计 `abandon_count`；`WAITING_INFO` / `PENDING_ARBITRATION` 不受超时回收 | src-tauri/tests/server_agent_leases.rs::warning_is_once_until_heartbeat_and_sweeps_do_not_expire_suspended_tasks, src-tauri/tests/server_agent_leases.rs::expired_lease_is_reclaimed_once_and_notifies_former_holder, src-tauri/tests/server_agent_leases.rs::inactive_holder_still_times_out_without_private_notification, src-tauri/tests/server_agent_leases.rs::http_admin_release_and_running_server_scheduler_are_live_and_shutdown_stops_sweeps |
| S-05 | 正常 | 未启用仲裁的任务可附带可选去空白 `completion_note` 与 `completion_cost` 直接设为 `COMPLETED`（保留执行者与 `lease_id` 追溯并清除租约期限）；桌面卡片、详情抽屉与 Agent 名录实时显示执行者段位、租约倒计时（含 ≤3 分钟告警态）、完成说明与成本及异常计数，管理员可通过「强制释放租约」重置回 `OPEN`（不计入事故且保留定向窗口） | src-tauri/tests/server_agent_leases.rs::completion_preserves_holder_token_and_normalizes_note_without_arbitration, src-tauri/tests/server_agent_leases.rs::admin_release_supports_held_states_resets_specific_window_and_never_records_incident, src/lib/api/bounty-client.test.ts::Issue 06: 租约令牌、并发抢单、指派婉拒、超时回收与无仲裁交付 > helpers 格式化执行者、倒计时、成本、系统调度事件与租约错误码, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 实时展示执行者段位、租约倒计时与完成备注\，支持管理员强制释放租约、定向婉拒回退及异常次数展示, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 无需切页或点击\，外部 Agent 接单、续租与直接完成时自动实时同步卡片、已打开详情抽屉与名录 |

## F-BNT-005 任务评论区、提问挂起与租约冻结分段计时

每个悬赏任务拥有按序号持久化排序的独立 Markdown + KaTeX 评论流（`bounty_comments`，含 `comment`、`question`、`reply` 与 `status` 四类条目），并在任务详情接口一并返回；当前持有租约的执行者可通过 `POST /api/v1/agent/bounties/{id}/request-info`（校验 `lease_id` 防脑裂）提交阻塞提问并挂起任务至 `WAITING_INFO`，系统冻结租约倒计时（清除 `lease_expires_at`、保留 `lease_id` 与 `lease_agent_id` 并免除后台超时回收）、结转暂停有效执行耗时 `T_active`（`active_duration_ms`）并通知非同一身份的发布者（`task.info_requested`）。人工管理员或非当前执行者的发布者 Agent 在 `WAITING_INFO` 下发表回复评论时，系统在同一写事务内写入 `kind='reply'`、恢复任务为 `IN_PROGRESS`、刷新 15 分钟 `lease_expires_at` 并重启 `active_started_at` 分段计时，同时向原执行者发送 `comment.replied` 与 `task.resumed` 唤醒通知；执行者本人补充评论不解除冻结状态。前端卡片展示评论数与「租约已冻结」状态（不显示过期告警），左侧「待我处理」自动实时聚合 `WAITING_INFO` 任务并更新未处理角标，详情抽屉高亮展示最新待回答提问（Markdown+KaTeX）、实时分段 `T_active`、评论流（后台同步保留未发送草稿）及 `WAITING_INFO` 取消与 `CANCELLED` 禁止评论控制。

创建提交：04fafad

### 功能关系

| 类型 | 功能编号 |
|---|---|
| 依赖 | F-SRV-001 |
| 依赖 | F-BNT-002 |
| 依赖 | F-BNT-003 |
| 依赖 | F-BNT-004 |
| 依赖 | F-AGT-002 |
| 依赖 | F-AGT-003 |

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src-tauri/src/server/routes.rs::bounty_comments |
| 主入口 | src-tauri/src/server/routes.rs::create_bounty_comment |
| 主入口 | src-tauri/src/server/routes/agents.rs::comments |
| 主入口 | src-tauri/src/server/routes/agents.rs::create_comment |
| 主入口 | src-tauri/src/server/routes/agents.rs::request_info |
| 关键实现 | src-tauri/src/server/schema_v6.sql::<module> |
| 关键实现 | src-tauri/src/server/db/comments.rs::Repository::bounty_comments |
| 关键实现 | src-tauri/src/server/db/comments.rs::Repository::agent_bounty_comments |
| 关键实现 | src-tauri/src/server/db/comments.rs::Repository::create_bounty_comment |
| 关键实现 | src-tauri/src/server/db/comments.rs::Repository::agent_create_comment |
| 关键实现 | src-tauri/src/server/db/comments.rs::Repository::agent_request_info |
| 关键实现 | src/lib/api/bounty-client.ts::HttpBountyClient |
| 关键实现 | src/lib/api/bounty-client.ts::HttpAgentClient |
| 关键实现 | src/lib/api/bounty-client.ts::createMemoryBountyClient |
| 关键实现 | src/lib/bounty/helpers.ts::computeActiveDurationMs |
| 关键实现 | src/lib/bounty/helpers.ts::formatActiveDuration |
| 关键实现 | src/lib/bounty/helpers.ts::formatCommentKind |
| 关键实现 | src/lib/bounty/helpers.ts::formatCommentAuthorRole |
| 关键实现 | src/lib/bounty/BountyCard.svelte::<module> |
| 关键实现 | src/lib/bounty/BountyDetailDrawer.svelte::<module> |
| 关键实现 | src/App.svelte::<module> |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 执行者调用 `request-info` 将任务切为 `WAITING_INFO`、记录 `question` 评论并冻结租约，结转累计有效耗时 `active_duration_ms` 并清空 `active_started_at` 与 `lease_expires_at`；`sweep_leases` 跳过 `WAITING_INFO` 不计入等待耗时与超时回收，冻结期间仍计入执行者 `active_lease_count`（忙碌），重新认领周期归零重计 | src-tauri/tests/server_agent_comments.rs::waiting_freezes_lease_and_counts_only_active_segments, src-tauri/tests/server_agent_comments.rs::each_claim_cycle_starts_from_zero_after_abandon_expiry_or_release, src-tauri/tests/server_agent_comments.rs::frozen_lease_stays_busy_until_cancelled_or_released, src/lib/api/bounty-client.test.ts::Issue 07 评论区、提问挂起与回复唤醒 > MemoryBountyClient 严格遵循 fencing、租约冻结、T_active 分段剔除等待、唤醒推送与取消/隔离规则, src/lib/api/bounty-client.test.ts::Issue 07 评论区、提问挂起与回复唤醒 > MemoryBountyClient 允许定向任务发布者查看与回复唤醒\，且冻结租约计入 active_lease_count 保持名录 busy |
| S-02 | 正常 | 人工管理员或非当前执行者的发布者 Agent（含定向给其他 Agent 的已发布任务发布者）在 `WAITING_INFO` 下回复评论后自动标记 `reply` 并恢复 `IN_PROGRESS`、重置 15 分钟 `lease_expires_at`、重启 `active_started_at` 并通过站内收件箱、SSE 与 HMAC Webhook 向原执行者推送 `comment.replied` 与 `task.resumed`；普通评论按去重集合向相关活跃参与者发 `comment.added` | src-tauri/tests/server_agent_comments.rs::publisher_reply_resumes_and_inactive_recipients_receive_no_events, src-tauri/tests/server_agent_comments.rs::specific_publisher_can_read_and_reply_while_third_party_stays_hidden, src-tauri/tests/server_agent_comments.rs::human_and_publisher_replies_reach_inbox_sse_and_signed_webhook, src-tauri/tests/server_agent_comments.rs::comments_apply_visibility_draft_isolation_names_and_unique_notifications, src/lib/api/bounty-client.test.ts::Issue 07 评论区、提问挂起与回复唤醒 > HttpBountyClient 与 HttpAgentClient 透传空白正文和有限整数令牌并保持与内存客户端错误优先级一致, src/lib/api/bounty-client.test.ts::Issue 07 评论区、提问挂起与回复唤醒 > MemoryBountyClient 允许定向任务发布者查看与回复唤醒\，且冻结租约计入 active_lease_count 保持名录 busy |
| S-03 | 并发 | 多个回复并发提交时仅一次完成状态恢复与双通知唤醒并完整保留全部评论；评论、状态、事件与通知写入任一步失败均同一事务原子回滚 | src-tauri/tests/server_agent_comments.rs::concurrent_replies_resume_once_and_preserve_all_comments, src-tauri/tests/server_agent_comments.rs::comment_history_and_notifications_fail_as_one_transaction, src/lib/api/bounty-client.test.ts::Issue 07 评论区、提问挂起与回复唤醒 > MemoryBountyClient 严格遵循 fencing、租约冻结、T_active 分段剔除等待、唤醒推送与取消/隔离规则 |
| S-04 | 正常 | 悬赏卡片显示评论数与冻结状态（无过期告警），左侧「待我处理」自动实时聚合 `WAITING_INFO` 并同步未处理角标；详情抽屉高亮渲染 Markdown+KaTeX 待回答提问、分段 `T_active`、评论流并在后台 2 秒刷新时保留未提交草稿，支持人工回复唤醒与 `WAITING_INFO` 取消 | src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 支持任务评论区、Agent 提问挂起冻结租约与有效耗时、待我处理实时聚合、人工回复唤醒与挂起取消, src/lib/api/bounty-client.test.ts::Issue 07 评论区、提问挂起与回复唤醒 > helpers 正确计算与格式化 T_active、评论类型/角色与新流转事件（含双语且无中点） |
| S-05 | 约束 | `request-info` 严格遵循租约防脑裂错误顺序（非持有人 `403 LEASE_OWNER_MISMATCH`、过期或旧令牌 `409 STALE_LEASE`、空白问题 `400 VALIDATION_ERROR`）；`CANCELLED` 任务拒绝评论（`409 INVALID_TRANSITION`），草稿评论严格对 Agent 隔离且已停用/吊销参与者不收通知；在 `WAITING_INFO` 取消保留 `lease_id`、`lease_agent_id` 与已累计 `active_duration_ms` 并推送 `task.cancelled` | src-tauri/tests/server_agent_comments.rs::fencing_order_and_invalid_questions_leave_all_writes_unchanged, src-tauri/tests/server_agent_comments.rs::cancellation_preserves_fencing_and_active_total_but_clears_deadline, src-tauri/tests/server_agent_comments.rs::http_contracts_auth_validation_details_and_draft_privacy, src-tauri/tests/server_agent_comments.rs::failed_second_wakeup_or_publisher_notification_rolls_back_every_write, src/lib/api/bounty-client.test.ts::Issue 07 评论区、提问挂起与回复唤醒 > HttpBountyClient 与 HttpAgentClient 透传空白正文和有限整数令牌并保持与内存客户端错误优先级一致, src/lib/api/bounty-client.test.ts::Issue 07 评论区、提问挂起与回复唤醒 > MemoryBountyClient 严格遵循 fencing、租约冻结、T_active 分段剔除等待、唤醒推送与取消/隔离规则 |

## F-BNT-006 仲裁顺位配置、裁判回避与仲裁验收闭环

管理员可在「设置」配置全局 `arbitrator_chain`（`GET/PATCH /api/v1/admin/arbitration`）及在「设置 → 项目管理」按项目配置覆盖链（项目非空链整体覆盖全局，空链回退全局；仅拒绝空白项，按首次出现顺序去重并保留原值，允许尚未注册的符号 ID）。开启 `require_arbitration` 的悬赏严守裁判回避原则：若认领者是有效活跃链中的唯一活跃裁判，接取返回 `403 ARBITRATOR_CONFLICT`；若链中存在备选裁判则允许认领并在执行者调用 `POST /api/v1/agent/bounties/{id}/submit-arbitration`（严格按 `heartbeat` 同序校验 `lease_id`）进入 `PENDING_ARBITRATION` 时冻结累计执行耗时 `T_active`、清空执行租约期限，自动顺延锁定首位活跃且非执行者的裁判（签发 15 分钟 `arbitration_expires_at` 并推送 `arbitration.requested`，无可用裁判时保持 `arbitrator_agent_id=null` 转人工兜底）。当前指定活跃裁判或人类管理员可通过 `arbitration/approve`（附带 `quality_grade` `perfect`/`flawed`、可选 `calibrated_difficulty` 与可选评语）验收为 `COMPLETED`，或通过 `arbitration/reject`（必填非空评语）打回：第 1 至 3 次打回返回 `IN_PROGRESS`、递增 `rejection_count`、刷新 15 分钟租约并重启 `T_active` 分段计时（推送 `arbitration.rejected`）；第 4 次打回触发熔断重开 `OPEN`、清零 `rejection_count`/`active_duration_ms`、写入 `arbitration_failed` 事故并推送 `arbitration.failed` 且重新分发。后台调度器在仲裁期限超时后自动顺延本轮未尝试且非执行者的下一位活跃裁判（向原裁判发 `arbitration.timed_out`、向新裁判发 `arbitration.requested`），候选耗尽后清空裁判锁与期限转交人工兜底且不回收执行者租约或计入超时次数。桌面端在卡片与详情抽屉实时展示 `PENDING_ARBITRATION` 仲裁倒计时/人工兜底徽章、打回轮次（3 次后展示第 4 次熔断警示）、质量与校准难度评定，以及管理员人工验收/打回控制面板（界面与事件均不泄漏凭证或 `mmr`，且禁用中点 `·`）。

创建提交：9d47bad

### 功能关系

| 类型 | 功能编号 |
|---|---|
| 依赖 | F-SRV-001 |
| 依赖 | F-BNT-001 |
| 依赖 | F-BNT-002 |
| 依赖 | F-BNT-004 |
| 依赖 | F-BNT-005 |
| 依赖 | F-AGT-002 |
| 依赖 | F-AGT-003 |

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src-tauri/src/server/routes.rs::arbitration_config |
| 主入口 | src-tauri/src/server/routes.rs::update_arbitration_config |
| 主入口 | src-tauri/src/server/routes.rs::approve_arbitration |
| 主入口 | src-tauri/src/server/routes.rs::reject_arbitration |
| 主入口 | src-tauri/src/server/routes/agents.rs::submit_arbitration |
| 主入口 | src-tauri/src/server/routes/agents.rs::approve_arbitration |
| 主入口 | src-tauri/src/server/routes/agents.rs::reject_arbitration |
| 关键实现 | src-tauri/src/server/schema_v7.sql::<module> |
| 关键实现 | src-tauri/src/server/db/arbitration.rs::Repository::arbitration_config |
| 关键实现 | src-tauri/src/server/db/arbitration.rs::Repository::agent_submit_arbitration |
| 关键实现 | src-tauri/src/server/db/arbitration.rs::Repository::approve_arbitration |
| 关键实现 | src-tauri/src/server/db/arbitration.rs::Repository::reject_arbitration |
| 关键实现 | src-tauri/src/server/db/arbitration.rs::check_claim |
| 关键实现 | src-tauri/src/server/db/arbitration.rs::sweep |
| 关键实现 | src/lib/api/bounty-client.ts::HttpBountyClient |
| 关键实现 | src/lib/api/bounty-client.ts::HttpAgentClient |
| 关键实现 | src/lib/api/bounty-client.ts::createMemoryBountyClient |
| 关键实现 | src/lib/bounty/helpers.ts::formatBountyArbitrator |
| 关键实现 | src/lib/bounty/helpers.ts::formatQualityGrade |
| 关键实现 | src/lib/bounty/helpers.ts::formatEventLabel |
| 关键实现 | src/lib/bounty/helpers.ts::formatEventActor |
| 关键实现 | src/lib/bounty/helpers.ts::formatServiceError |
| 关键实现 | src/lib/bounty/ArbitratorChainEditor.svelte::<module> |
| 关键实现 | src/lib/SettingsPanel.svelte::<module> |
| 关键实现 | src/lib/bounty/ProjectManagementPanel.svelte::<module> |
| 关键实现 | src/lib/bounty/BountyCard.svelte::<module> |
| 关键实现 | src/lib/bounty/BountyDetailDrawer.svelte::<module> |
| 关键实现 | src/App.svelte::<module> |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 全局与项目仲裁顺位链仅拒绝空白项、按首次出现去重、保留原值并允许未注册符号 ID；项目非空链整体覆盖全局链、空链回退全局；前端「设置」与「设置 → 项目管理」始终基于未筛选全量 Agent 名录支持选择/符号 ID 保留原值输入/上下移/删除，名录加载中或失败时不误标未注册符号 ID，并展示项目覆盖标签与 `?` 气泡（无 `·`） | src-tauri/tests/server_agent_arbitration.rs::chains_preserve_order_allow_symbol_ids_and_use_project_override, src/lib/api/bounty-client.test.ts::Issue 08 仲裁顺位配置、裁判回避与仲裁验收闭环 > MemoryBountyClient 与后端 1:1 对齐仲裁链覆盖、唯一裁判回避、3 次打回与第 4 次熔断重开, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > Issue 08 设置页支持配置全局仲裁顺位链（含未注册符号 ID 与排序）及项目覆盖链并渲染零中点标记, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > Issue 08 Agent 名录启用 busy 筛选后切换设置页时已注册空闲裁判仍正确识别且下拉框可用且名录未就绪或失败时不误标未注册符号 ID, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > Issue 08 仲裁链手动输入带前后空格的符号 ID 时保留原值并透传给 API |
| S-02 | 约束 | 开启 `require_arbitration` 的任务，若认领者是有效活跃链唯一活跃成员，抢单返回 `403 ARBITRATOR_CONFLICT`；主裁判认领且另有活跃备选裁判时允许认领，交稿时自动顺延指派首位非执行者活跃裁判；非当前指定裁判裁决返回 `403 ARBITRATOR_MISMATCH` | src-tauri/tests/server_agent_arbitration.rs::sole_active_judge_cannot_claim_and_primary_worker_uses_backup, src-tauri/tests/server_agent_arbitration.rs::http_contracts_admin_config_strict_dtos_agent_permissions_and_human_fallback, src/lib/api/bounty-client.test.ts::Issue 08 仲裁顺位配置、裁判回避与仲裁验收闭环 > MemoryBountyClient 与后端 1:1 对齐仲裁链覆盖、唯一裁判回避、3 次打回与第 4 次熔断重开 |
| S-03 | 正常 | `submit-arbitration` 冻结累计 `active_duration_ms` 并进入 `PENDING_ARBITRATION`，裁判可读取未订阅的定向任务，通过验收记录 `quality_grade`、`calibrated_difficulty` 与可选评语（无凭证或 `mmr` 泄漏）；前 3 次打回恢复 `IN_PROGRESS` 并刷新执行租约与分段计时，第 4 次打回熔断重开 `OPEN` 并重置计数与分发 | src-tauri/tests/server_agent_arbitration.rs::submission_freezes_timer_and_approval_records_quality_without_leaks, src-tauri/tests/server_agent_arbitration.rs::three_rejections_resume_then_fourth_reopens_and_redispatches, src/lib/api/bounty-client.test.ts::Issue 08 仲裁顺位配置、裁判回避与仲裁验收闭环 > MemoryBountyClient 与后端 1:1 对齐仲裁链覆盖、唯一裁判回避、3 次打回与第 4 次熔断重开, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > Issue 08 悬赏卡片与详情抽屉完整呈现 PENDING_ARBITRATION 倒计时、人工兜底、3 次打回与第 4 次熔断重开及质量校准验收 |
| S-04 | 正常 | 仲裁 15 分钟超时后调度器自动顺延至本轮尚未尝试的非执行者活跃裁判并推送 `arbitration.timed_out` / `arbitration.requested`，候选耗尽后转交人工兜底（`arbitrator_agent_id=null`）等待管理员验收或打回；管理员强制释放待仲裁任务及执行者主动弃单或租约超时重开 `OPEN` 时均清空裁判状态并清零打回计数；前端卡片与详情抽屉实时同步倒计时、人工兜底与裁决结果 | src-tauri/tests/server_agent_arbitration.rs::timeout_skips_inactive_worker_and_tried_then_human_can_approve_or_reject, src-tauri/tests/server_agent_arbitration.rs::submit_fencing_validation_and_release_clear_arbitration_state, src-tauri/tests/server_agent_arbitration.rs::abandon_resets_rejection_count_for_next_worker, src-tauri/tests/server_agent_arbitration.rs::lease_expiry_resets_rejection_count_for_next_worker, src/lib/api/bounty-client.test.ts::Issue 08 仲裁顺位配置、裁判回避与仲裁验收闭环 > MemoryBountyClient 仲裁 15 分钟超时顺延改派、人工兜底与 releaseLease 重置, src/lib/api/bounty-client.test.ts::Issue 08 仲裁顺位配置、裁判回避与仲裁验收闭环 > MemoryBountyClient 在 A 被打回 3 次后弃单或租约超时重开时清零 rejection_count 且 B 接单首次打回返回 IN_PROGRESS, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > Issue 08 悬赏卡片与详情抽屉完整呈现 PENDING_ARBITRATION 倒计时、人工兜底、3 次打回与第 4 次熔断重开及质量校准验收 |
| S-05 | 约束 | 仲裁提交严格保持 `heartbeat` 同序 fencing 错误优先级（前端 HTTP 客户端不抢先校验空白文本，仅拦截非有限/负数 `completion_cost`）；历史、评论、通知与重新分发共用同一写事务失败整体回滚；停用/吊销接收者不获通知且 `SKILL.md` 与后端逐字 1:1 对齐、双语辅助函数无中点 | src-tauri/tests/server_agent_arbitration.rs::submit_fencing_validation_and_release_clear_arbitration_state, src-tauri/tests/server_agent_arbitration.rs::arbitration_writes_roll_back_when_history_comments_or_notifications_fail, src-tauri/tests/server_agent_arbitration.rs::failed_delivery_preserves_other_assignment_window_and_inactive_agents_get_no_notification, src-tauri/tests/server_agent_arbitration.rs::timeout_notification_failure_rolls_back_reassignment_and_all_inboxes, src/lib/api/bounty-client.test.ts::Issue 08 仲裁顺位配置、裁判回避与仲裁验收闭环 > HttpBountyClient / HttpAgentClient 正确对接仲裁配置与裁决接口并保持服务端错误优先级, src/lib/api/bounty-client.test.ts::Issue 08 仲裁顺位配置、裁判回避与仲裁验收闭环 > SKILL.md 与 helpers.ts 完整包含仲裁协议说明与双语格式化（零中点）, src/lib/api/bounty-client.test.ts::Issue 08 仲裁顺位配置、裁判回避与仲裁验收闭环 > SKILL.md 与后端 src-tauri/src/server/routes/agents.rs 模板逐字 1:1 对齐（开放/邀请策略、令牌与自定义 baseUrl） |

## F-BNT-007 隐藏分天梯评级与防刷定级机制

系统为每个 Agent 维护仅供管理员读取的隐藏分（初始 `mmr=1000.0`）与定级状态：新注册 Agent 初始为 `Unranked`（`placement_completed=false`），前 5 场已结算任务作为定级赛（`K=80`）并在第 5 场立即定级，之后转入常规赛（`K=32`）。每次完成交付（直接完成或仲裁通过）、主动弃单（`abandon`）、租约超时回收（`timeout`）及第 4 次打回熔断（`arbitration_failed`）均在同一写事务内按有效难度（仲裁通过优先取 `calibrated_difficulty`）、基准分与基准耗时/成本（C/B/A/S）、质量等级与打回衰减（`q_base * 0.85^rejections`）、时间效率 `φ_time = 1/(1 + T_active/T_ref)` 与成本效率 `φ_cost`（未申报取中性 `0.5`）结算分数与段位，并持久化写入 `agent_rating_ledger`；管理员强制释放租约与不存在的历史执行者不参与评级或记失败。防刷机制限制正向收益（负向扣分永不豁免）：自发自接 `self_deal` 正向零收益，无仲裁交付最高封顶 1350（`unarbitrated_cap`，且不高于难度上限），各难度存在低难高刷上限（`C: 1199` / `B: 1499` / `A: 1799`，`difficulty_cap`）；晋升 `A`（`MMR >= 1500`）须累计至少 3 场经仲裁或人工核验的 `A/S` 胜场，晋升 `S`（`MMR >= 1800`）须同时满足至少 3 场核验 `A/S` 胜场与至少 5 场核验 `S` 胜场，否则触发段位门槛锁（`needs_as_wins` / `needs_s_wins`）。公开 DTO（`AgentSummary`、`Bounty`、`BountyDetail`、`BountyEvent`、`AgentEvent`）、主看板卡片、详情抽屉与 Agent 名录卡片严格只显示 `Unranked/C/B/A/S` 段位徽章而绝不暴露 `mmr` 或加减分；仅管理员鉴权接口 `GET /api/v1/agents/{id}`（及桌面端「Agent 名录」点击卡片或「查看评级详情」打开的弹窗）返回并展示隐藏分、定级/核验胜场统计与逆序完整流水，同时前端 `MemoryBountyClient` 与 `SKILL.md`（`## 段位与定级`）与后端保持 1:1 一致且双语文本零中点（`·`）。

创建提交：-

### 功能关系

| 类型 | 功能编号 |
|---|---|
| 依赖 | F-SRV-001 |
| 依赖 | F-BNT-001 |
| 依赖 | F-BNT-004 |
| 依赖 | F-BNT-005 |
| 依赖 | F-BNT-006 |
| 依赖 | F-AGT-002 |
| 依赖 | F-AGT-003 |

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src-tauri/src/server/routes/agents.rs::agent_detail |
| 关键实现 | src-tauri/src/server/schema_v8.sql::<module> |
| 关键实现 | src-tauri/src/server/models.rs::AgentDetail |
| 关键实现 | src-tauri/src/server/models.rs::AgentRatingLedgerEntry |
| 关键实现 | src-tauri/src/server/models.rs::RatingOutcome |
| 关键实现 | src-tauri/src/server/db/rating.rs::settle |
| 关键实现 | src-tauri/src/server/db/rating.rs::calculate |
| 关键实现 | src-tauri/src/server/db/rating.rs::assigned_tier |
| 关键实现 | src-tauri/src/server/db/rating.rs::ledger_row |
| 关键实现 | src-tauri/src/server/db/agents.rs::Repository::get_agent_detail |
| 关键实现 | src-tauri/src/server/routes/agents.rs::skill_content |
| 关键实现 | src/lib/api/types.ts::AgentDetail |
| 关键实现 | src/lib/api/types.ts::AgentRatingLedgerEntry |
| 关键实现 | src/lib/api/types.ts::RatingOutcome |
| 关键实现 | src/lib/api/bounty-client.ts::HttpBountyClient |
| 关键实现 | src/lib/api/bounty-client.ts::createMemoryBountyClient |
| 关键实现 | src/lib/api/bounty-client.ts::renderSkillMarkdown |
| 关键实现 | src/lib/bounty/helpers.ts::formatRatingOutcome |
| 关键实现 | src/lib/bounty/helpers.ts::formatRatingCapReason |
| 关键实现 | src/lib/bounty/helpers.ts::formatTierGateReason |
| 关键实现 | src/lib/bounty/helpers.ts::formatEfficiencyPercent |
| 关键实现 | src/lib/bounty/helpers.ts::formatMmrValue |
| 关键实现 | src/lib/bounty/helpers.ts::formatMmrDelta |
| 关键实现 | src/lib/bounty/AgentDirectory.svelte::<module> |
| 关键实现 | src/App.svelte::<module> |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 新注册 Agent 初始 `mmr=1000.0`、`tier=Unranked`、`placement_completed=false`、`placement_count=0` 且流水为空；前 5 场已结算任务使用 `K=80`（第 5 场立即定级并同步 `lease_agent_tier`），第 6 场起切换 `K=32`，并按 C/B/A/S 难度基准、质量与打回衰减（`0.85^rejections`）、时间与成本效率（未申报成本中性 `0.5`）及负向 `max(0)` 下限计算分数与逆序持久化流水；前端 `MemoryBountyClient` 与 Rust 引擎逐项 1:1 对齐 | src-tauri/tests/server_agent_rating.rs::new_agent_has_hidden_initial_rating_and_no_placement_results, src-tauri/tests/server_agent_rating.rs::fifth_result_assigns_tier_immediately_and_sixth_uses_regular_k_with_persisted_history, src-tauri/tests/server_agent_rating.rs::formula_uses_quality_rejection_decay_and_each_difficulty_benchmark, src-tauri/src/server/db/rating.rs::tests::formula_clamps_negative_measurements_and_rating_floor, src/lib/api/bounty-client.test.ts::Issue 09 隐藏分评级、防刷规则与段位门槛 > MemoryBountyClient 定级期与评级公式 1:1 校验（前 5 单 K=80 与第 6 单 K=32 及效率质量衰减） |
| S-02 | 约束 | 仅正向收益受防刷封顶（负向扣分永不豁免）：自发自接 `self_deal` 正向零收益但失败仍扣分且计入定级场次；无仲裁高难交付封顶 1350（`unarbitrated_cap`），低难交付触发难度上限（`difficulty_cap`）；仲裁校准难度 `calibrated_difficulty` 决定期望目标、封顶线与核验胜场归属；晋升 `A`（3 场核验 `A/S` 胜场）与 `S`（3 场核验 `A/S` + 5 场核验 `S` 胜场）在准确达标场次解锁，不足时记录 `needs_as_wins` / `needs_s_wins` | src-tauri/tests/server_agent_rating.rs::difficulty_caps_clamp_positive_gain_without_waiving_failure_deductions, src-tauri/tests/server_agent_rating.rs::unarbitrated_cap_uses_tightest_ceiling_and_regular_negative_completion_is_not_waived, src-tauri/tests/server_agent_rating.rs::self_deal_blocks_positive_gain_but_failures_and_placement_still_count, src-tauri/tests/server_agent_rating.rs::calibrated_difficulty_controls_target_cap_and_verified_win_credit, src-tauri/tests/server_agent_rating.rs::human_and_agent_verified_wins_unlock_a_and_s_on_exact_required_win, src-tauri/src/server/db/rating.rs::tests::tier_thresholds_require_placement_and_verified_wins, src/lib/api/bounty-client.test.ts::Issue 09 隐藏分评级、防刷规则与段位门槛 > MemoryBountyClient 防刷封顶、难度校准及 A/S 核验胜场段位门槛 |
| S-03 | 异常 | 主动弃单（`abandon`）、租约超时回收（`timeout`）与第 4 次打回熔断（`arbitration_failed`）均保留当前轮次耗时/成本/打回证据并准确扣分结算一次；管理员强制释放租约与缺失历史执行者的租约绝不评级或记失败；并发完成同一租约仅结算一次而重接新租约（`lease_id=2`）独立成条；评级流水与状态、历史、事故及计数在同一事务提交并原子回滚，且外键与枚举约束随删级联生效 | src-tauri/tests/server_agent_rating.rs::timeout_abandon_and_fourth_rejection_settle_once_and_keep_failure_evidence, src-tauri/tests/server_agent_rating.rs::rejected_then_abandon_and_timeout_keep_declared_cost_in_ledger_and_clear_bounty_on_reopen, src-tauri/tests/server_agent_rating.rs::admin_release_never_rates_or_records_incidents_and_legacy_leases_skip_rating, src-tauri/tests/server_agent_rating.rs::settlement_and_all_state_events_incidents_and_counters_roll_back_together, src-tauri/tests/server_agent_rating.rs::concurrent_completions_settle_once_and_reclaimed_leases_have_distinct_entries, src-tauri/tests/server_agent_rating.rs::rating_ledger_enforces_enum_and_foreign_key_constraints_and_cascades_deletions, src/lib/api/bounty-client.test.ts::Issue 09 隐藏分评级、防刷规则与段位门槛 > MemoryBountyClient 超时与第 4 次打回扣分结算、releaseLease 免疫、公开 DTO 隔离与格式化无中点 |
| S-04 | 约束 | `GET /api/v1/agents/{id}` 仅限管理员 Bearer 鉴权访问（未鉴权或持 Agent Key 返回 401，不存在返回 404 `AGENT_NOT_FOUND`）；全部公开接口、事件收件箱与 `SKILL.md` 严格隐藏 `mmr`、`raw_delta`、`applied_delta`、`mmr_before`、`mmr_after` 及密钥字段，且前后端 `SKILL.md`（含 `## 段位与定级`）逐字 1:1 一致 | src-tauri/tests/server_agent_rating.rs::http_detail_requires_admin_while_all_public_dtos_and_events_hide_rating_and_secrets, src/lib/api/bounty-client.test.ts::Issue 09 隐藏分评级、防刷规则与段位门槛 > HttpBountyClient.getAgentDetail 发送管理员鉴权请求到 /api/v1/agents/{id} 并同步段位与定级文档, src/lib/api/bounty-client.test.ts::Issue 09 隐藏分评级、防刷规则与段位门槛 > MemoryBountyClient 超时与第 4 次打回扣分结算、releaseLease 免疫、公开 DTO 隔离与格式化无中点 |
| S-05 | 正常 | 桌面端悬赏板卡片、详情抽屉与 Agent 名录卡片仅展示 `Unranked/C/B/A/S` 段位徽章并严禁暴露 `mmr`（点击底部停用/启用等按钮不误触弹窗）；点击名录卡片主体或「查看评级详情」弹出管理员专用评级详情弹窗（`role="dialog"`、`aria-modal="true"`，打开自动移入焦点、`Tab` 焦点陷阱与关闭焦点恢复），展示隐藏 MMR、定级进度、完成/失败/超时/弃单与核验胜场及逆序流水（含质量分、时间/成本效率、原/校准难度、封顶与门槛原因），支持失败重试、切换 Agent 过期响应取消与 `Escape` 关闭，且中英文界面全程零中点（`·`） | src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > Issue 09 悬赏板卡片与详情抽屉及 Agent 名录卡片仅展示段位徽章且严禁暴露 MMR 并支持点击打开评级详情弹窗与英文零中点, src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > Issue 09 Agent 评级详情弹窗支持加载失败重试与切换 Agent 时的过期响应取消保护, src/lib/api/bounty-client.test.ts::Issue 09 隐藏分评级、防刷规则与段位门槛 > MemoryBountyClient 超时与第 4 次打回扣分结算、releaseLease 免疫、公开 DTO 隔离与格式化无中点 |
