# 服务内核

> 模块职责：桌面内嵌与独立进程共用的 SQLite + Axum 同构服务内核、管理员 Bearer 鉴权及前端运行模式切换与配置校验。接口字段与错误码见 [服务 API](../src-tauri/src/server/README.md)。

## F-SRV-001 同构服务内核与运行模式

桌面应用启动回环临时端口的 `embedded` 内嵌服务并向前端提供会话随机密钥与连接信息；无头二进制 `taskrail-server` 通过命令行参数或环境变量以 `standalone` 模式运行同一内核。数据库按当前结构初始化并启用 WAL、外键与等待超时；除公开健康检查外，管理接口均校验管理员 Bearer token 并支持跨域预检。前端通过 `HttpBountyClient` 对接服务，在「设置 → 本地服务与连接」支持个人内嵌模式与团队远端模式的测试连接、配置持久化及运行时切换。

创建提交：fdaa8f2

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src-tauri/src/server/mod.rs::start |
| 主入口 | src-tauri/src/bin/taskrail-server.rs::main |
| 主入口 | src-tauri/src/lib.rs::get_embedded_server_info |
| 关键实现 | src-tauri/src/lib.rs::run |
| 关键实现 | src-tauri/src/server/mod.rs::router |
| 关键实现 | src-tauri/src/server/mod.rs::random_admin_token |
| 关键实现 | src-tauri/src/server/mod.rs::RunningServer |
| 关键实现 | src-tauri/src/server/config.rs::parse_config |
| 关键实现 | src-tauri/src/server/db.rs::Repository::open |
| 关键实现 | src-tauri/src/server/schema.sql::<module> |
| 关键实现 | src-tauri/src/server/routes.rs::router |
| 关键实现 | src-tauri/src/server/routes.rs::authorize |
| 关键实现 | src-tauri/src/server/routes.rs::health |
| 关键实现 | src-tauri/src/server/routes.rs::admin_status |
| 关键实现 | src-tauri/src/server/error.rs::ApiError |
| 关键实现 | src/lib/api/bounty-client.ts::HttpBountyClient |
| 关键实现 | src/lib/api/bounty-client.ts::fetchEmbeddedServerInfo |
| 关键实现 | src/lib/api/bounty-client.ts::normalizeServerUrl |
| 关键实现 | src/lib/config-validation.ts::validateAppConfig |
| 关键实现 | src/App.svelte::<module> |
| 关键实现 | src/lib/SettingsPanel.svelte::<module> |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 空库通过 `schema.sql` 在同一事务内初始化完整结构和默认配置，失败时整体回滚；重复打开数据库保留项目、任务、Agent、配置与事件历史，不创建版本记录表；连接启用 WAL、外键约束和 5000ms busy timeout | src-tauri/tests/server_repository.rs::initialization_persists_and_project_crud_enforces_archive_and_references, src-tauri/tests/server_repository.rs::current_schema_enforces_comment_incident_arbitration_and_dependency_constraints, src-tauri/tests/server_agent_events.rs::event_storage_preserves_data_on_reopen_and_never_reuses_ids, src-tauri/tests/server_agent_publishing.rs::publisher_identity_defaults_and_references_survive_reopen, src-tauri/src/server/db.rs::tests::initialization_failure_rolls_back_schema, src-tauri/src/server/db.rs::tests::connections_enable_foreign_keys_wal_and_busy_timeout |
| S-03 | 正常 | 无头入口支持 `--help`、缺少密钥以非零退出码结束；CLI 参数覆盖环境变量，缺省使用默认地址与数据库文件；重启保留数据并更换密钥后旧密钥失效 | src-tauri/tests/server_config.rs::cli_overrides_environment_and_preserves_paths, src-tauri/tests/server_config.rs::env_fallbacks_and_defaults_are_deterministic, src-tauri/tests/server_config.rs::invalid_or_ambiguous_config_is_rejected, src-tauri/tests/server_config.rs::binary_help_and_missing_token_exit_codes, src-tauri/tests/server_config.rs::binary_starts_with_environment_and_exposes_shared_kernel, src-tauri/tests/server_http.rs::server_restart_preserves_data_changes_token_and_releases_port |
| S-04 | 约束 | `/api/v1/health` 公开访问；管理接口缺少或携带错误凭证返回 401 `UNAUTHORIZED`；跨域预检与 401 均携带 CORS 头；非法 JSON/参数返回 400 信封，存储失败返回 500 而不泄露 SQL 细节 | src-tauri/tests/server_http.rs::http_auth_cors_health_and_validation, src-tauri/tests/server_http.rs::server_restart_preserves_data_changes_token_and_releases_port |
| S-05 | 正常 | 前端 `HttpBountyClient` 规范化 `http://`/`https://` 服务根地址、对健康检查省略鉴权头、为管理请求注入 Bearer token 并将错误信封解析为 `ServiceApiError`；可通过 `get_embedded_server_info` 获取内嵌连接信息 | src/lib/api/bounty-client.test.ts::normalizeServerUrl & fetchEmbeddedServerInfo > 校验并规范化 http/https 服务根地址\，去除首尾空白与末尾斜杠, src/lib/api/bounty-client.test.ts::normalizeServerUrl & fetchEmbeddedServerInfo > 通过 Tauri invoke 读取内嵌服务连接信息, src/lib/api/bounty-client.test.ts::HttpBountyClient > 规范化 baseUrl\，对健康检查省略 Bearer 头\，对其余路由注入 Bearer 头并编码查询参数, src/lib/api/bounty-client.test.ts::HttpBountyClient > 将服务错误信封解析为 ServiceApiError（401、409 抢单冲突与项目删除保护） |
| S-06 | 正常 | 应用配置校验与持久化保留 `serviceMode`（`embedded`/`remote`）、规范化 `remoteServerUrl` 与 `remoteAdminToken`，并拒绝非 `http`/`https` 协议或非法模式值 | src/lib/config-validation.test.ts::应用配置校验 > 校验任务服务连接模式、远程服务地址与管理员令牌 |
| S-07 | 正常 | 设置页切换团队远端模式可测试连接、保存并重连且重载远端项目与任务，也可切回个人内嵌模式；配置持久化失败时回滚配置并阻止重连；初始连接失败时不阻塞窗口就绪并展示可重试横幅 | src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 支持切换团队远端模式、测试连接、持久化远端配置并从远端服务重载项目与任务, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 初始服务连接失败时不阻塞窗口就绪（onready）\，并展示可重试的非阻塞连接错误横幅, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 配置保存失败时不连接远端服务、回滚配置变更并展示错误反馈\，绝不显示成功提示, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 保存远端配置挂起期间并发导航保存不会将未确认的 remote 配置写入磁盘, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 跨刷新请求乱序完成时保持最新连接状态\，慢速工作区刷新不覆盖搜索结果与错误横幅 |
| S-08 | 约束 | 配置加载时将 `language`、`lineWidth`、`sidebarCollapsed` 转换为 `locale`、`contentWidth`、`sidebarOpen`；运行时、默认值和保存结果仅使用规范字段，旧布尔缩进在加载时转换为字符数，保存重载保持偏好 | src/lib/config-validation.test.ts::应用配置校验 > 加载旧别名后仅输出规范字段并可直接保存重载, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 支持中英文语言切换与项目图标选择器交互 |
| S-09 | 正常 | 打开旧库时将 MCP 授权表升级为可延后关联 Agent 的结构，已有授权的访问令牌和刷新令牌继续有效 | src-tauri/src/server/db/oauth.rs::tests::upgrading_oauth_tables_preserves_existing_agent_grants |

## F-SRV-002 用户与 Agent 账户

用户和 Agent 共用注册、登录、会话和资料接口。账户角色决定是否需要 Agent 邀请以及会话能访问哪些接口。

创建提交：2d42887

### 功能关系

| 类型 | 功能编号 |
|---|---|
| 依赖 | F-AGT-001 |

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src-tauri/src/server/routes/accounts.rs::router |
| 主入口 | src/App.svelte::<module> |
| 主入口 | src/lib/auth/AuthDialog.svelte::<module> |
| 主入口 | src/lib/auth/UserProfileDialog.svelte::<module> |
| 关键实现 | src/lib/api/bounty-client.ts::HttpBountyClient |
| 关键实现 | src-tauri/src/server/db/accounts.rs::Repository::register_account |
| 关键实现 | src-tauri/src/server/db/accounts.rs::Repository::login_account |
| 关键实现 | src-tauri/src/server/db/accounts.rs::Repository::update_account_profile |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 注册或登录返回当前角色的账户资料和会话。Agent 注册应用邀请规则。旧 Agent 可用原密钥登录原身份 | src-tauri/tests/server_accounts.rs::shared_accounts_validate_names_profiles_and_session_roles |
| S-02 | 约束 | 用户与 Agent 共用名称空间。重名返回 `409 NAME_TAKEN`，注册失败不消耗邀请，改名失败保留原资料 | src-tauri/tests/server_accounts.rs::shared_accounts_validate_names_profiles_and_session_roles |
| S-03 | 正常 | 账户可更改名称、选择预制头像、上传 PNG、JPEG 或 WebP，或清除头像。服务校验文件内容和大小 | src-tauri/tests/server_accounts.rs::shared_accounts_validate_names_profiles_and_session_roles |
| S-04 | 约束 | 用户会话只能访问自己的账户资料。退出使当前会话失效。停用或吊销 Agent 禁止其会话访问业务接口 | src-tauri/tests/server_accounts.rs::shared_accounts_validate_names_profiles_and_session_roles |
| S-05 | 正常 | 用户通过侧栏底部入口注册或登录。登录后，侧栏显示名称和头像。点击打开菜单，可进入个人设置修改名称或头像，也可退出登录 | src/lib/session/app-integration.test.ts::用户账号注册、登录、个人设置与服务隔离集成测试 > 侧栏底部悬浮用户卡片支持注册、登录、个人设置（预设/上传/清除头像与改名）、错误提示与退出登录 |
| S-06 | 约束 | 桌面按当前连接的服务隔离用户会话。切换服务时恢复对应会话。服务拒绝会话时清除登录状态 | src/lib/session/app-integration.test.ts::用户账号注册、登录、个人设置与服务隔离集成测试 > 用户登录凭证按服务隔离且独立于管理员凭证\，切换服务互不串话并在 401 时清理失效会话 |
