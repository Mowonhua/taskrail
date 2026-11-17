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
