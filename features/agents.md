# Agent 准入与名录

> 模块职责：外部 Agent 自注册准入开关与邀请令牌管理、接入指南生成与桌面 Skill 与 MCP 接入，以及 Agent 名录管控、项目订阅与 `DRAFT` 草稿隔离。接口字段与错误码见 [服务 API](../src-tauri/src/server/README.md)。

## F-AGT-001 Agent 自注册、准入令牌与接入文档

管理员在「设置 → Agent 准入与协议」控制局域网免审注册开关（`lan_open_registration`，说明收纳入帮助 Popover 保持低密度交互）、签发带使用次数上限与有效期的邀请令牌（`tr_inv_` 前缀，库内仅存 SHA-256 摘要与展示前缀），幂等作废有效令牌或经二次确认永久删除令牌（均不影响已注册 Agent，删除时清除 Agent 对邀请的关联）。服务端与前端按服务根地址及注册策略生成无凭证的 `SKILL.md`。设置页从「接入」打开 Markdown 预览和桌面安装，复制操作位于预览弹窗中。管理员选定有效的邀请令牌，在安装设置中选择目标 Agent 和范围，系统把服务地址和可选邀请令牌写入 Taskrail 私有连接配置。外部 Agent 凭有效邀请令牌或开启的免审策略通过 `/api/v1/agents/register` 自注册并获得唯一一次性 `tr_ag_` 密钥，初始段位统一为 `Unranked`。

服务端与前端内存模式共用 [SKILL.md 模板](../assets/skills/taskrail-agent/SKILL.md)。构建时嵌入模板，并随桌面安装程序分发原生 `taskrail-agent`。在线文档填入服务地址和注册策略。安装后的 Skill 只包含 `SKILL.md`，调用原生命令时使用 Taskrail 写入的连接配置路径。

创建提交：4411188

### 功能关系

| 类型 | 功能编号 |
|---|---|
| 依赖 | F-SRV-001 |
| 依赖 | F-BNT-001 |

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src/lib/bounty/AgentOnboardingPanel.svelte::<module> |
| 主入口 | src/App.svelte::<module> |
| 关键实现 | src/lib/SettingsPanel.svelte::<module> |
| 关键实现 | src/lib/anchored-popover.ts::positionAnchoredPopover |
| 关键实现 | src/lib/files/tauri.ts::TauriFilePort.exportText |
| 关键实现 | src-tauri/src/lib.rs::export_markdown_file |
| 关键实现 | src-tauri/src/storage.rs::export_markdown_file |
| 关键实现 | src/lib/bounty/helpers.ts::formatInviteTokenStatus |
| 关键实现 | src/lib/bounty/helpers.ts::formatServiceError |
| 关键实现 | src/lib/api/bounty-client.ts::HttpBountyClient |
| 关键实现 | src/lib/api/bounty-client.ts::HttpAgentClient |
| 关键实现 | src/lib/api/bounty-client.ts::registerAgent |
| 关键实现 | src/lib/api/bounty-client.ts::createMemoryBountyClient |
| 关键实现 | src-tauri/src/server/db/agents.rs::Repository::onboarding |
| 关键实现 | src-tauri/src/server/db/agents.rs::Repository::create_invite_token |
| 关键实现 | src-tauri/src/server/db/agents.rs::Repository::revoke_invite_token |
| 关键实现 | src-tauri/src/server/db/agents.rs::Repository::delete_invite_token |
| 关键实现 | src-tauri/src/server/db/agents.rs::Repository::validate_invite_token |
| 关键实现 | src-tauri/src/server/db/agents.rs::Repository::register_agent |
| 关键实现 | src-tauri/src/server/routes/agents.rs::onboarding |
| 关键实现 | src-tauri/src/server/routes/agents.rs::update_onboarding |
| 关键实现 | src-tauri/src/server/routes/agents.rs::invites |
| 关键实现 | src-tauri/src/server/routes/agents.rs::create_invite |
| 关键实现 | src-tauri/src/server/routes/agents.rs::revoke_invite |
| 关键实现 | src-tauri/src/server/routes/agents.rs::delete_invite |
| 关键实现 | src-tauri/src/server/routes/agents.rs::admin_skill |
| 关键实现 | src-tauri/src/server/routes/agents.rs::public_skill |
| 关键实现 | src-tauri/src/server/routes/agents.rs::register |
| 关键实现 | src-tauri/src/server/routes/agents.rs::skill_content |
| 关键实现 | src/lib/api/bounty-client.ts::renderSkillMarkdown |
| 关键实现 | src/lib/bounty/SkillInstallDialog.svelte::<module> |
| 关键实现 | src/lib/dialog-light-dismiss.ts::lightDismiss |
| 关键实现 | src/lib/files/tauri.ts::TauriFilePort.installAgentSkill |
| 关键实现 | src-tauri/src/lib.rs::install_agent_skill |
| 关键实现 | src-tauri/src/agent_skill.rs::install |
| 关键实现 | src-tauri/src/agent_cli.rs::register |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 设置页支持切换局域网免审注册开关（「局域网免审注册」说明收纳入帮助 Popover（按内容宽度展开并与问号按钮左缘对齐）且展开期间统一监听 `Escape` 以便焦点移至开关等其他控件时仍可关闭）、创建/作废邀请令牌、通过弹窗展示并复制一次性明文 `raw_token`（剪贴板不可用或写入拒绝时向上报错且不误报已复制）；免审切换失败时回滚开关状态并报错；切换服务连接时关闭并清理旧连接的明文令牌弹窗状态与 `SKILL.md` 预览 | src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 支持设置页 Agent 准入与协议（局域网免审开关关闭/开启、邀请令牌生成/复制/作废、SKILL.md 预览与选中令牌安装）, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 支持免审开关失败回滚、服务连接切换清理旧令牌与 SKILL.md 预览\，以及浏览器禁用本地安装, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 剪贴板不可用或写入拒绝时\，复制邀请令牌与复制 SKILL.md 展示错误信息且绝不误报已复制, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 打开局域网免审帮助 Popover 后将焦点移至开关或其他控件\，按 Escape 仍可立即关闭 Popover, src/lib/anchored-popover.test.ts::锚点浮层 > align=start 使用浮层固有宽度并与触发器左缘对齐 |
| S-02 | 正常 | 依据请求地址或显式 `base_url` 生成 `SKILL.md`。提供邀请参数时校验有效性且不消耗次数。文档正文不包含凭证。设置页在弹窗渲染当前连接的 Markdown，复制图标将该份文档的源文本写入剪贴板，成功后显示勾选图标。点击弹窗外部可关闭预览。服务连接或注册策略变更后，打开预览会加载新文档，加载失败时没有复制入口 | src-tauri/tests/server_agents.rs::skill_documents_follow_request_address_and_never_export_admin_credentials, src-tauri/tests/server_config.rs::binary_starts_with_environment_and_exposes_shared_kernel, src/lib/api/bounty-client.test.ts::createMemoryBountyClient (Issue 03 Agent 准入、订阅与隔离) > 支持邀请令牌门控、局域网免审开关、重名拦截与 SKILL.md 动态生成, src/lib/files/tauri.test.ts::桌面平台端口边界 > 使用系统保存对话框与受控原生命令 export_markdown_file 导出 SKILL.md\，取消时不写入且透传翻译 Coded Error, src-tauri/src/storage.rs::tests::markdown_export_preserves_utf8_text_and_replaces_regular_files, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 支持设置页 Agent 准入与协议（局域网免审开关关闭/开启、邀请令牌生成/复制/作废、SKILL.md 预览与选中令牌安装）, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 支持免审开关失败回滚、服务连接切换清理旧令牌与 SKILL.md 预览\，以及浏览器禁用本地安装, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 预览弹窗复制当前 SKILL.md\，免审策略变更后重新加载, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 作废后刷新准入返回 503 时仍禁止选中该令牌安装 |
| S-03 | 并发 | `max_uses=1` 邀请令牌在跨连接并发自注册下严格仅允许 1 个 Agent 成功消耗；后续校验或写入失败的注册全量回滚且不消耗使用次数；跨服务切换期间迟到的 `createInviteToken` 响应按服务代次隔离抛弃，不写入新服务的明文令牌横幅或开启附带开关 | src-tauri/tests/server_agent_repository.rs::one_use_invite_allows_exactly_one_registration_across_connections, src-tauri/tests/server_agent_repository.rs::registration_persists_only_hashes_and_failed_registration_does_not_consume_invites, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 服务 A 延迟返回 createInviteToken 期间切换到服务 B 后\，旧响应不会写入明文令牌或开启附带开关 |
| S-04 | 约束 | 仅存储邀请令牌与 `api_key` 的 SHA-256 摘要及展示前缀；免审关闭缺令牌返回 `401 INVITE_TOKEN_REQUIRED`，作废/过期/耗尽令牌返回 `401 INVITE_TOKEN_INVALID`，重名返回 `409 AGENT_NAME_TAKEN`，非法 `base_url` 或参数返回 `400 VALIDATION_ERROR`；桌面 `export_markdown_file` 通过生成式 ACL 运行且禁用 `plugin:fs\|write_text_file` 权限，仅接受 `.md`/`.markdown`/`.txt` 与不超过 1 MiB 内容，并拒绝备用数据流、无效父路径、目录及现存/悬空符号链接 | src-tauri/tests/server_agent_repository.rs::registration_persists_only_hashes_and_failed_registration_does_not_consume_invites, src-tauri/tests/server_agent_repository.rs::invite_revocation_expiration_and_lan_gate_are_enforced, src/lib/api/bounty-client.test.ts::Issue 03 HTTP 客户端（管理员准入/名录 + Agent 门户） > HttpBountyClient 正确构造准入设置、邀请令牌、SKILL.md 与 Agent 名录管控请求, src/lib/api/bounty-client.test.ts::createMemoryBountyClient (Issue 03 Agent 准入、订阅与隔离) > 支持邀请令牌门控、局域网免审开关、重名拦截与 SKILL.md 动态生成, src/lib/api/bounty-client.test.ts::Issue 03 辅助格式化函数与中英文翻译 > 正确格式化 Agent 状态、段位、邀请令牌状态、订阅摘要与错误消息且不含中点分隔符, src-tauri/src/lib.rs::tests::markdown_export_uses_generated_acl_without_plugin_fs_write_permission, src-tauri/src/storage.rs::tests::markdown_export_rejects_invalid_extensions_without_writing, src-tauri/src/storage.rs::tests::markdown_export_rejects_alternate_data_stream_paths, src-tauri/src/storage.rs::tests::markdown_export_accepts_the_byte_limit_and_rejects_larger_content, src-tauri/src/storage.rs::tests::markdown_export_rejects_missing_parent_and_parent_that_is_a_file, src-tauri/src/storage.rs::tests::markdown_export_rejects_directory_targets, src-tauri/src/storage.rs::tests::markdown_export_rejects_existing_and_dangling_symlinks |
| S-05 | 异常 | 仅准入设置加载失败时不误显假默认关闭状态（展示「状态未知」并禁用免审开关），显式展示加载错误横幅并支持重试加载恢复真实状态；重试加载使用独立请求序号，与创建令牌等操作并发时互不取消且按钮不锁死、明文令牌不丢失；作废令牌后后续准入刷新 503 不回滚本地已作废状态 | src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 仅准入设置或 Agent 列表加载失败时显式展示错误并支持重试恢复\，且不误显假默认值或空状态, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 作废后刷新准入返回 503 时仍禁止选中该令牌安装, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 准入设置或 Agent 名录重试加载与业务操作并发时互不取消\，按钮不锁死且一次性明文令牌不丢失 |
| S-06 | 正常 | 邀请令牌以列对齐的单层列表行展示备注、前缀、状态、使用次数、有效期与「更多操作」菜单入口，宽容器内保持单行并在空间不足时自动换行而非横向滚动，最多显示 3 行且超出部分纵向滚动查看；管理员可在「更多操作」菜单中作废有效令牌或取消、二次确认删除任意状态令牌，删除后原明文不可再用于注册或文档、已注册 Agent 凭证保持有效且邀请关联清空，一次性明文弹窗与相关 `SKILL.md` 缓存同步失效；后续准入刷新失败不回滚本地删除状态 | src-tauri/tests/server_agent_repository.rs::invite_deletion_preserves_agents_and_invalidates_raw_token, src-tauri/tests/server_agents.rs::skill_documents_follow_request_address_and_never_export_admin_credentials, src/lib/api/bounty-client.test.ts::Issue 03 HTTP 客户端（管理员准入/名录 + Agent 门户） > HttpBountyClient 正确构造准入设置、邀请令牌、SKILL.md 与 Agent 名录管控请求, src/lib/api/bounty-client.test.ts::createMemoryBountyClient (Issue 03 Agent 准入、订阅与隔离) > 支持邀请令牌门控、局域网免审开关、重名拦截与 SKILL.md 动态生成, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 邀请令牌需二次确认后删除\，取消不删除且刷新失败仍清理本地明文与缓存 |
| S-07 | 正常 | 桌面设置页从有效令牌行选中安装凭证。选中或悬停时，令牌备注、状态、使用次数和有效期保持可读。在「安装」弹窗选择 Skill 并填写当次安装的服务地址。当前设置会话内生成的令牌可直接使用，历史令牌需粘贴原值。可在 Codex、Claude Code、Cursor、DeepSeek Harness、Devin CLI、Antigravity、Grok Build、ZCode 的项目或全局路径安装 `taskrail-agent`，也可选择共享 Skills（`~/agents/skills`）或自定义目录。Skill 的宿主列表以共享 Skills 开头。宿主选项单行横向滚动，允许一次选择多个。批次内相同的 Skill 目录只写一份。程序只安装 `SKILL.md`，保留已有安装。程序在私有数据目录写入服务地址和可选邀请令牌。原生命令注册成功后保存 Agent 密钥并移除邀请令牌 | src-tauri/src/agent_skill.rs::tests::installs_skill_and_mcp_independently_with_private_connections, src-tauri/src/agent_skill.rs::tests::installs_skill_in_shared_and_new_harness_directories, src-tauri/src/agent_skill.rs::tests::batch_shares_connection_and_deduplicates_skill_directories, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 支持设置页 Agent 准入与协议（局域网免审开关关闭/开启、邀请令牌生成/复制/作废、SKILL.md 预览与选中令牌安装）, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 安装历史邀请令牌时要求粘贴原值并使用选定的全局目标 |

## F-AGT-002 Agent 名录管控、项目订阅与草稿隔离

管理员在「Agents」按活跃度（全部 / 空闲可接单 / 执行任务中）与状态（全部 / 正常接入 / 已停用 / 已吊销）筛选查看 Agent 档案、能力标签、订阅项目范围与段位徽章（不暴露隐藏 MMR 且无中点分隔符），可执行停用（`403 AGENT_DISABLED`）、启用与二次确认不可逆吊销凭证（`401 AGENT_REVOKED`），并通过工作区下方的「接入」按钮跳转至设置页的 Agent 准入与协议区域。已认证 Agent 可自助维护自身描述与能力标签、整体替换订阅范围（仅限未归档项目）并拉取未归档项目清单及订阅可见的公开任务，系统严格禁止向 Agent 暴露任何 `DRAFT` 草稿及他人定向/未订阅公开任务。

创建提交：4411188

### 功能关系

| 类型 | 功能编号 |
|---|---|
| 依赖 | F-SRV-001 |
| 依赖 | F-BNT-001 |
| 依赖 | F-BNT-002 |
| 依赖 | F-AGT-001 |

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src/lib/bounty/AgentDirectory.svelte::<module> |
| 主入口 | src/App.svelte::<module> |
| 关键实现 | src/lib/bounty/helpers.ts::formatAgentStatus |
| 关键实现 | src/lib/bounty/helpers.ts::formatAgentTier |
| 关键实现 | src/lib/bounty/helpers.ts::formatAgentSubscriptionSummary |
| 关键实现 | src/lib/api/bounty-client.ts::HttpBountyClient |
| 关键实现 | src/lib/api/bounty-client.ts::HttpAgentClient |
| 关键实现 | src/lib/api/bounty-client.ts::createMemoryBountyClient |
| 关键实现 | src-tauri/src/server/db/agents.rs::Repository::authenticate_agent |
| 关键实现 | src-tauri/src/server/db/agents.rs::Repository::list_agents |
| 关键实现 | src-tauri/src/server/db/agents.rs::Repository::set_agent_status |
| 关键实现 | src-tauri/src/server/db/agents.rs::Repository::update_agent |
| 关键实现 | src-tauri/src/server/db/agents.rs::Repository::update_agent_subscriptions |
| 关键实现 | src-tauri/src/server/db/agents.rs::Repository::agent_projects |
| 关键实现 | src-tauri/src/server/db/agents.rs::Repository::agent_bounties |
| 关键实现 | src-tauri/src/server/db/agents.rs::Repository::agent_bounty_detail |
| 关键实现 | src-tauri/src/server/routes/agents.rs::Agent |
| 关键实现 | src-tauri/src/server/routes/agents.rs::agents |
| 关键实现 | src-tauri/src/server/routes/agents.rs::disable |
| 关键实现 | src-tauri/src/server/routes/agents.rs::enable |
| 关键实现 | src-tauri/src/server/routes/agents.rs::revoke |
| 关键实现 | src-tauri/src/server/routes/agents.rs::me |
| 关键实现 | src-tauri/src/server/routes/agents.rs::update_me |
| 关键实现 | src-tauri/src/server/routes/agents.rs::update_subscriptions |
| 关键实现 | src-tauri/src/server/routes/agents.rs::projects |
| 关键实现 | src-tauri/src/server/routes/agents.rs::bounties |
| 关键实现 | src-tauri/src/server/routes/agents.rs::bounty |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | Agents 视图展示段位、接入状态、订阅范围与当前租约数（不暴露 MMR 且无中点分隔符），对重复能力标签去重渲染不触发 `each_key_duplicate`，支持按活跃度（`idle` / `busy`）与状态筛选、停用与启用、取消或二次确认吊销凭证（吊销后操作入口消失），空态展示唯一的「接入」按钮并跳转设置页的 Agent 准入与协议区域 | src-tauri/tests/server_agents.rs::onboarding_validation_subscriptions_and_directory_filters, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 支持 Agent 名录筛选、订阅范围与段位展示（不暴露 MMR）、停用启用与二次确认吊销凭证, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > Agent 携带重复能力标签时正常去重渲染且不触发 each_key_duplicate, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > Agents 空态「接入」按钮跳转到设置页 Agent 准入与协议位置 |
| S-02 | 并发 | 快速切换 Agent 活跃度或状态筛选时按请求序号抛弃迟到响应，不覆盖最新名录结果 | src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > Agent 名录并发切换筛选时忽略迟到响应 |
| S-03 | 正常 | Agent 凭 `api_key` 可读写自身档案（同步刷新 `last_seen_at`）、区分「全部订阅」「指定项目」与「空订阅」整体替换订阅，并按 `created_at` 稳定顺序拉取未归档项目列表 | src-tauri/tests/server_agent_repository.rs::subscription_defaults_and_empty_selections_remain_distinct, src-tauri/tests/server_agents.rs::onboarding_validation_subscriptions_and_directory_filters, src/lib/api/bounty-client.test.ts::Issue 03 HTTP 客户端（管理员准入/名录 + Agent 门户） > HttpAgentClient 与 registerAgent 正确构造公开文档、注册及 Agent 鉴权请求, src/lib/api/bounty-client.test.ts::createMemoryBountyClient (Issue 03 Agent 准入、订阅与隔离) > 支持订阅替换校验、项目归档拒绝、停用/启用/吊销鉴权拦截与名录活跃度筛选 |
| S-04 | 约束 | 严格隔离 `DRAFT` 任务（含指派给自身的草稿）及未订阅或指派给他人的任务，单条详情统一返回 `404 BOUNTY_NOT_FOUND`；停用返 `403 AGENT_DISABLED`、吊销返 `401 AGENT_REVOKED` 且终态禁止重新启用/停用；管理员凭证不可调用 Agent 门户且无效订阅或归档项目（`409 PROJECT_ARCHIVED`）保留原有订阅 | src-tauri/tests/server_agents.rs::registration_credentials_and_draft_isolation, src-tauri/tests/server_agents.rs::onboarding_validation_subscriptions_and_directory_filters, src/lib/api/bounty-client.test.ts::createMemoryBountyClient (Issue 03 Agent 准入、订阅与隔离) > 支持订阅替换校验、项目归档拒绝、停用/启用/吊销鉴权拦截与名录活跃度筛选, src/lib/api/bounty-client.test.ts::createMemoryBountyClient (Issue 03 Agent 准入、订阅与隔离) > 严格隔离 DRAFT 任务与定向任务可见性 |
| S-05 | 异常 | 仅 Agent 列表加载失败时显式展示加载错误并支持重试恢复，不误显「暂无已注册的 Agent」空状态；重试加载使用独立请求序号，与停用/启用/吊销操作并发时互不取消且不锁死操作按钮 | src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 仅准入设置或 Agent 列表加载失败时显式展示错误并支持重试恢复\，且不误显假默认值或空状态, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 准入设置或 Agent 名录重试加载与业务操作并发时互不取消\，按钮不锁死且一次性明文令牌不丢失 |

## F-AGT-003 双通道推送（WebSocket/SSE + Webhook）与持久化事件收件箱

任务发布（`OPEN`）与未接取撤回（`OPEN -> DRAFT`）在同一数据库写事务内按订阅范围、段位门槛（`Unranked` 等价 `C` 级）与定向指派规则精准分发至目标 Agent 的持久化收件箱（`agent_events`）及本轮分发快照（`bounty_dispatches`），撤回仅通知曾收到本轮分发的活跃 Agent 且不携带草稿正文。Agent 可通过 WebSocket（`/api/v1/agent/ws`）与 SSE（`/api/v1/agent/stream`）实时长连接（按 `cursor`/`Last-Event-ID` 补拉、断开自动释放连接登记并更新 `last_pushed_at`），或配置带 HMAC-SHA256 签名与指数退避重试的 Webhook 回调接收推送，亦可随时分页补拉收件箱并单调推进 `acked_event_id` 游标；工作台「Agents」低密度展示通道状态（`在线长连接` / `Webhook` / `离线`，刷新失败时显示 `状态未知`）与最近推送时间，支持手动「刷新」与切页自动刷新（与停用/启用/吊销操作互不锁死）。

创建提交：505811b

### 功能关系

| 类型 | 功能编号 |
|---|---|
| 依赖 | F-SRV-001 |
| 依赖 | F-BNT-001 |
| 依赖 | F-BNT-002 |
| 依赖 | F-AGT-001 |
| 依赖 | F-AGT-002 |

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src-tauri/src/server/routes/events.rs::router |
| 主入口 | src-tauri/src/server/push.rs::dispatch_webhooks |
| 主入口 | src/lib/bounty/AgentDirectory.svelte::<module> |
| 主入口 | src/App.svelte::<module> |
| 关键实现 | src-tauri/src/server/schema.sql::<module> |
| 关键实现 | src-tauri/src/server/mod.rs::start |
| 关键实现 | src-tauri/src/server/mod.rs::RunningServer |
| 关键实现 | src-tauri/src/server/models.rs::AgentSummary |
| 关键实现 | src-tauri/src/server/models.rs::AgentEvent |
| 关键实现 | src-tauri/src/server/models.rs::AgentInbox |
| 关键实现 | src-tauri/src/server/models.rs::UpdateWebhook |
| 关键实现 | src-tauri/src/server/models.rs::WebhookConfig |
| 关键实现 | src-tauri/src/server/db/events.rs::dispatch |
| 关键实现 | src-tauri/src/server/db/events.rs::webhook_values |
| 关键实现 | src-tauri/src/server/db/events.rs::WebhookDelivery |
| 关键实现 | src-tauri/src/server/db/events.rs::Repository::agent_events |
| 关键实现 | src-tauri/src/server/db/events.rs::Repository::ack_agent_events |
| 关键实现 | src-tauri/src/server/db/events.rs::Repository::webhook_config |
| 关键实现 | src-tauri/src/server/db/events.rs::Repository::update_webhook |
| 关键实现 | src-tauri/src/server/db/events.rs::Repository::connect_stream |
| 关键实现 | src-tauri/src/server/db/events.rs::Repository::mark_stream_pushed |
| 关键实现 | src-tauri/src/server/db/events.rs::Repository::due_webhooks |
| 关键实现 | src-tauri/src/server/db/events.rs::Repository::webhook_current_values |
| 关键实现 | src-tauri/src/server/db/events.rs::Repository::finish_webhook |
| 关键实现 | src-tauri/src/server/db/agents.rs::Repository::agent_summary |
| 关键实现 | src-tauri/src/server/push.rs::Hub |
| 关键实现 | src-tauri/src/server/push.rs::StreamGuard |
| 关键实现 | src-tauri/src/server/push.rs::signature |
| 关键实现 | src-tauri/src/server/push.rs::build_webhook_client |
| 关键实现 | src-tauri/src/server/routes/events.rs::events |
| 关键实现 | src-tauri/src/server/routes/events.rs::ack |
| 关键实现 | src-tauri/src/server/routes/events.rs::webhook |
| 关键实现 | src-tauri/src/server/routes/events.rs::update_webhook |
| 关键实现 | src-tauri/src/server/routes/events.rs::sse |
| 关键实现 | src-tauri/src/server/routes/events.rs::ws |
| 关键实现 | src-tauri/src/server/routes/events.rs::EventStream |
| 关键实现 | src-tauri/src/server/routes/events.rs::websocket |
| 关键实现 | src/lib/bounty/helpers.ts::formatAgentChannelStatus |
| 关键实现 | src/lib/api/bounty-client.ts::HttpBountyClient |
| 关键实现 | src/lib/api/bounty-client.ts::HttpAgentClient |
| 关键实现 | src/lib/api/bounty-client.ts::createMemoryBountyClient |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 精准订阅与 `Unranked == C` 分发：发布或直接创建 `OPEN` 任务在同一写事务内写入目标 Agent 收件箱与 `bounty_dispatches` 快照；公开/最低段位任务仅匹配 `active` 且已订阅对应项目的 Agent（`Unranked` 按 `C` 级参与段位筛选），`specific` 定向任务仅向指定 `active` 目标推送 `task.assigned`；事务失败时任务与事件一并回滚，重新发布按最新订阅与状态重选接收目标 | src-tauri/tests/server_agent_events.rs::dispatch_subscription_tiers_assignment_withdrawal_and_ack, src-tauri/tests/server_agent_events.rs::dispatch_and_withdrawal_roll_back_together_and_republish_reselects_recipients, src/lib/api/bounty-client.test.ts::createMemoryBountyClient (Issue 04 双通道推送与持久化事件收件箱) > 精准分发：Unranked/C/B 段位映射、指定指派、订阅匹配与撤回仅通知已接收目标, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 工作台发布与撤回任务联动精准事件分发、Unranked==C 段位匹配、Inbox 游标补拉与 Ack 推进 |
| S-02 | 正常 | `task.withdrawn` 精准撤回：`OPEN -> DRAFT` 撤回仅向本轮 `bounty_dispatches` 中仍为 `active` 的原接收者发送 `task.withdrawn`（载荷仅含 `bounty_id` 与 `project_id`，绝不泄露草稿正文），并清空本轮分发快照 | src-tauri/tests/server_agent_events.rs::dispatch_subscription_tiers_assignment_withdrawal_and_ack, src-tauri/tests/server_agent_events.rs::dispatch_and_withdrawal_roll_back_together_and_republish_reselects_recipients, src/lib/api/bounty-client.test.ts::createMemoryBountyClient (Issue 04 双通道推送与持久化事件收件箱) > 精准分发：Unranked/C/B 段位映射、指定指派、订阅匹配与撤回仅通知已接收目标, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 工作台发布与撤回任务联动精准事件分发、Unranked==C 段位匹配、Inbox 游标补拉与 Ack 推进 |
| S-03 | 正常 | 持久化事件收件箱游标与 Ack：Agent 按 `cursor` 与 `limit` 顺序分页补拉自身事件（跨页不重不漏、严格隔离他人事件、省略 `cursor` 时从 `acked_event_id` 起读且读取不推进 Ack），通过 HTTP Ack 接口或 WebSocket `ack` 消息提交 `event_id` 时钳位到自身最大事件号且单调永不回退；若发现存在前序失败或断线间隙，先从已确认游标经 Inbox 补齐处理后再累计 Ack；非法负数游标、零 `limit` 或非法 Ack 载荷返回 `400 VALIDATION_ERROR` | src-tauri/tests/server_agent_events.rs::dispatch_subscription_tiers_assignment_withdrawal_and_ack, src-tauri/tests/server_agent_events.rs::inbox_http_isolation_validation_and_webhook_configuration, src-tauri/tests/server_agent_events.rs::large_backlog_crosses_page_boundary_without_duplicates_and_releases_last_connection, src-tauri/tests/server_agent_events.rs::webhook_failed_head_releases_successor_and_inbox_recovers_gap_before_cumulative_ack, src/lib/api/bounty-client.test.ts::Issue 04 HTTP Agent 客户端（Webhook 配置、Inbox 与 Ack） > HttpAgentClient 正确构造 Webhook 读写、Inbox 查询参数与 Ack 请求, src/lib/api/bounty-client.test.ts::createMemoryBountyClient (Issue 04 双通道推送与持久化事件收件箱) > 支持游标补拉、分页、Ack 钳位不回退与实时流连接/last_pushed_at 更新, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 工作台发布与撤回任务联动精准事件分发、Unranked==C 段位匹配、Inbox 游标补拉与 Ack 推进 |
| S-04 | 正常 | SSE/WebSocket 断线重连补拉与名录通道状态：SSE（`cursor`/`Last-Event-ID`）与 WebSocket 在数据库写锁内先登记 `Hub` 连接再按游标连续补发积压与新事件，写出事件后更新 `last_pushed_at`，断开、停用或吊销立即释放连接计数并中断在途流；工作台「Agents」展示 `在线长连接` / `Webhook` / `离线` 与最近推送时间（无中点分隔符，不泄露 `webhook_secret`/`api_key_hash`/`mmr`），支持手动「刷新」及切回 `agents` 自动刷新（刷新失败显示 `状态未知` 不误展旧在线状态，切回 `agents` 时仍会自动重试拉取并在恢复后清空错误与更新最新通道状态，切换连接清理错误，且并发刷新与停用互不锁死 `busyAgentId`） | src-tauri/tests/server_agent_events.rs::websocket_sse_reconnect_ack_connection_counts_and_revocation, src-tauri/tests/server_agent_events.rs::large_backlog_crosses_page_boundary_without_duplicates_and_releases_last_connection, src/lib/api/bounty-client.test.ts::createMemoryBountyClient (Issue 04 双通道推送与持久化事件收件箱) > 支持游标补拉、分页、Ack 钳位不回退与实时流连接/last_pushed_at 更新, src/lib/api/bounty-client.test.ts::Issue 04 通道状态格式化与中英文翻译 > formatAgentChannelStatus 正确输出中英文状态且不含中点分隔符, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > Agent 名录展示连接通道状态与最近推送时间\，支持手动刷新、切页自动刷新、失败不误显旧在线状态且并发停用不锁死 |
| S-05 | 约束 | Webhook HMAC-SHA256 签名、单 Agent 队头保序与指数退避重试：注册或更新 Webhook 校验 HTTP(S) URL 与非空白密钥，清空 URL 同时清除密钥，明文 `webhook_secret` 仅在新生成或显式更新时返回一次；`server::start` 预安装 rustls 提供者并校验 HTTP 客户端创建（失败则明确拒绝启动），独立进程与内嵌服务共用 `dispatch_webhooks`（禁用重定向、携带 `X-Taskrail-Signature`）；同一 Agent 按最小 `pending` `event_id` 队头保序投递（队头重试退避期间暂缓该 Agent 后续事件而不阻塞其他 Agent 或批容量），失败按 100/200/400/800ms 最多重试 5 次转 `failed` 后释放后续事件投递并允许经 Inbox 补齐，Agent 停用/吊销/换绑时立即停止投递，服务关闭取消在途回调并在重启后继续投递持久化积压 | src-tauri/tests/server_agent_events.rs::inbox_http_isolation_validation_and_webhook_configuration, src-tauri/tests/server_agent_events.rs::standalone_binary_delivers_signed_webhook_without_preinstalled_crypto_provider, src-tauri/tests/server_agent_events.rs::signed_webhooks_retry_then_succeed_and_stop_for_disabled_revoked_agents, src-tauri/tests/server_agent_events.rs::webhook_retry_preserves_fifo_without_blocking_other_agents_or_batch_capacity, src-tauri/tests/server_agent_events.rs::webhook_exhaustion_is_persistent, src-tauri/tests/server_agent_events.rs::webhook_failed_head_releases_successor_and_inbox_recovers_gap_before_cumulative_ack, src-tauri/tests/server_agent_events.rs::shutdown_cancels_inflight_webhook_and_restart_delivers_persistent_backlog, src/lib/api/bounty-client.test.ts::createMemoryBountyClient (Issue 04 双通道推送与持久化事件收件箱) > 支持注册与更新 Webhook 配置、一次性密钥返回、URL/密钥校验及通道状态更新 |

## F-AGT-004 原生 Agent 工具与 MCP 接入

Taskrail 通过原生命令、STDIO MCP 和 Streamable HTTP MCP 提供 Agent 的任务、收件箱、订阅与交付操作。三个入口沿用现有 Agent 权限与任务规则。桌面设置通过同一个弹窗选择安装 Skill、STDIO MCP 或 Streamable HTTP MCP。安装 MCP 时可附带简短 Skill。

创建提交：461974c

### 功能关系

| 类型 | 功能编号 |
|---|---|
| 依赖 | F-AGT-001 |
| 依赖 | F-AGT-002 |
| 依赖 | F-AGT-003 |
| 依赖 | F-BNT-004 |
| 依赖 | F-SRV-001 |

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src-tauri/src/bin/taskrail-agent.rs::main |
| 主入口 | src-tauri/src/server/routes/mcp.rs::router |
| 主入口 | src/lib/bounty/SkillInstallDialog.svelte::<module> |
| 关键实现 | src-tauri/src/agent_tools.rs::ACTIONS |
| 关键实现 | src-tauri/src/agent_mcp_install.rs::install |
| 关键实现 | src-tauri/src/server/db/oauth.rs::Repository::oauth_consent |
| 关键实现 | src-tauri/src/server/db/oauth.rs::Repository::oauth_exchange |
| 关键实现 | src-tauri/src/server/db/oauth.rs::Repository::oauth_identity |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 原生命令从本机连接配置注册并保存 Agent 密钥。CLI 可查询任务，STDIO 与 Streamable HTTP 提供同一工具清单，并以同一 Agent 身份读取服务数据 | src-tauri/tests/agent_mcp.rs::cli_stdio_and_oauth_http_share_agent_tools_and_permissions |
| S-02 | 正常 | 管理员可在同一弹窗选择 Skill 或任一种 MCP 安装方式。HTTP 模式只在宿主配置写入服务地址，由浏览器完成 OAuth。管理员可为多个宿主一次安装同一种 MCP。批次内的 STDIO 宿主共用私有连接配置和 Agent 凭证。同时启动时，注册进程等待首个进程保存凭证。Codex、Claude Code、Cursor、Devin CLI、Antigravity、Grok Build 和 ZCode 支持项目与全局范围的 STDIO 和 HTTP。DeepSeek Harness 支持全局 STDIO。安装先校验所有目标。写入失败时撤销本次已写入的目标，保留其他程序同时修改的文件。配置写入后，宿主继续加载原有的其他 MCP 服务器。点击弹窗外部可关闭安装配置 | src-tauri/src/agent_skill.rs::tests::installs_skill_and_mcp_independently_with_private_connections, src-tauri/src/agent_skill.rs::tests::oauth_mcp_installs_without_executable_or_invite_and_preserves_other_servers, src-tauri/src/agent_mcp_install.rs::tests::writes_native_mcp_entries_for_new_harnesses, src-tauri/src/agent_mcp_install.rs::tests::preserves_zcode_compatibility_servers_on_first_native_install, src-tauri/src/agent_mcp_install.rs::tests::adds_dsh_overlay_without_replacing_comments_tags_or_existing_rows, src-tauri/src/agent_skill.rs::tests::batch_shares_connection_and_deduplicates_skill_directories, src-tauri/src/agent_skill.rs::tests::batch_rolls_back_existing_host_config_if_later_target_changes, src-tauri/tests/agent_mcp.rs::cli_stdio_and_oauth_http_share_agent_tools_and_permissions, src/lib/session/app-integration.test.ts::任务板工作台外壳集成测试 > 可以单独配置 OAuth MCP 并选择是否附带简短 Skill |
| S-03 | 约束 | HTTP MCP 通过发现文档公布 OAuth 端点。授权代码绑定客户端、回调地址、目标资源和 PKCE，只能成功交换一次。刷新令牌轮换。吊销 Agent 后，先前的访问令牌失效 | src-tauri/tests/agent_mcp.rs::cli_stdio_and_oauth_http_share_agent_tools_and_permissions |
| S-04 | 约束 | 不同路径下的桌面实例分别维护本机地址。上一次的端口可用时，同一安装重启后继续使用该端口 | src-tauri/src/lib.rs::tests::embedded_mcp_addresses_are_isolated_by_installation_and_reused |
