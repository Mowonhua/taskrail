# 悬赏服务 API

桌面和 `taskrail-server` 使用同一 Rust 服务内核及 SQLite 数据库。当前接口面向人类管理员；Agent 注册、准入与执行期 HTTP 接口由独立功能提供。

## 连接

桌面启动时监听 `127.0.0.1:0`，数据库为 `app_data_dir()/taskrail.db`。调用 Tauri 命令 `get_embedded_server_info` 获取：

```json
{"base_url":"http://127.0.0.1:49152","admin_token":"64位十六进制会话密钥","db_path":"数据库绝对路径"}
```

端口由系统分配。密钥每次启动重新生成，不落盘。Android 不启动内嵌服务，也不注册此命令。

独立服务启动方式：

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin taskrail-server -- --bind 127.0.0.1:8420 --db ./taskrail.db --admin-token example-token
```

参数可分别使用 `TASKRAIL_BIND`、`TASKRAIL_DB`、`TASKRAIL_ADMIN_TOKEN` 环境变量；命令行覆盖对应环境变量。支持 `--key value` 与 `--key=value`。默认监听 `127.0.0.1:8420`，默认数据库为当前目录的 `taskrail.db`，管理员密钥必填。`--help` 显示用法。Ctrl+C 触发关闭；Unix 也接受 SIGTERM。关闭时停止接入并等待在途请求。

除健康检查外，以下请求都应携带 `Authorization: Bearer <admin_token>`。允许跨域 GET、POST、PATCH、DELETE、OPTIONS 和 Authorization、Content-Type 请求头，不使用 Cookie。JSON 字段名全部为 `snake_case`。

## 路由

所有路径以 `base_url` 为前缀。查询参数布尔值使用 `true` 或 `false`。

| 方法与路径 | 输入 | 成功响应 |
|---|---|---|
| `GET /api/v1/health` | 无，公开 | 200 `{ok:true,mode,version}` |
| `GET /api/v1/admin/status` | 无 | 200 `{ok:true,mode,db_path,version}` |
| `GET /api/v1/projects` | `include_archived=false` | 200 `Project[]` |
| `POST /api/v1/projects` | `CreateProject` | 201 `Project` |
| `GET /api/v1/projects/{id}` | 无 | 200 `Project` |
| `PATCH /api/v1/projects/{id}` | 项目部分更新 | 200 `Project` |
| `DELETE /api/v1/projects/{id}` | 无 | 204，无响应正文 |
| `GET /api/v1/bounties` | 可选 `project_id`、`status`、`q`、`include_drafts=false` | 200 `Bounty[]` |
| `POST /api/v1/bounties` | `CreateBounty` | 201 `Bounty` |
| `GET /api/v1/bounties/{id}` | 无 | 200 `{bounty:Bounty,events:BountyEvent[]}` |
| `PATCH /api/v1/bounties/{id}` | 草稿部分更新 | 200 `Bounty` |
| `POST /api/v1/bounties/{id}/publish` | 无请求正文 | 200 `Bounty` |
| `POST /api/v1/bounties/{id}/withdraw` | 无请求正文 | 200 `Bounty` |
| `POST /api/v1/bounties/{id}/cancel` | 无请求正文 | 200 `Bounty` |

`mode` 为 `embedded` 或 `standalone`。列表直接返回数组，无分页信封。项目按 `created_at,id` 升序；悬赏按 `created_at` 降序、`id` 升序；事件按递增 `id` 排序。`q` 为 id、标题、详情、验收条件的字面子串，ASCII 大小写不敏感；`%`、`_` 不作为通配符。草稿默认从列表排除，即使传 `status=DRAFT` 也须同时传 `include_drafts=true`。管理员持有凭证可直接读取草稿详情。

## 项目 JSON

创建必填 `name`；可选 `icon`（默认 null）、`description`、`workspace_path`、`default_acceptance_criteria`（默认空字符串）、`agent_draft_policy`（`open` 或 `draft`，默认 `open`）、`arbitrator_chain`（字符串数组，默认 `[]`）。

PATCH 可修改上述字段及 `archived`（布尔值）。省略字段保留当前值；`icon:null` 清空图标，其余字段传 null 保留当前值。空白名称被拒绝。图标接受单字素 `emoji:🚆` 或合法名称 `lucide:train-front`；Lucide 名称只含小写 ASCII 字母、数字及分隔非空片段的连字符，最多 64 字符；表情最多 64 个 UTF-16 单元，不能含控制字符，须含可见字符。

`Project` 返回以上全部字段，另含 `id`、`archived`、`created_at`、`updated_at`。归档项目可读取和恢复，不能创建、编辑或发布悬赏。硬删除仅允许完全没有悬赏引用的项目，已取消悬赏也算引用。

数据库将 `arbitrator_chain` 存入 `arbitrator_chain_json`；API 返回解析后的数组。

## 悬赏 JSON

创建示例：

```json
{
  "project_id":"项目 UUID",
  "title":"实现共享内核",
  "description":"Markdown 正文",
  "acceptance_criteria":"所有测试通过",
  "difficulty":"B",
  "assignee_mode":"any",
  "assignee_min_tier":null,
  "assignee_agent_id":null,
  "require_arbitration":false,
  "status":"DRAFT",
  "created_by":"human"
}
```

必填 `project_id`、`title`、`difficulty`、`assignee_mode`、`status`。`description` 和 `acceptance_criteria` 默认空字符串，`require_arbitration` 默认 false，两个限制字段默认 null，`created_by` 默认 `human`。

`difficulty` 和 `assignee_min_tier` 只能为 `C/B/A/S`。

- `assignee_mode=any`：两个限制字段应为 null。
- `assignee_mode=min_tier`：应提供 `assignee_min_tier`，`assignee_agent_id` 应为 null。
- `assignee_mode=specific`：应提供非空白 `assignee_agent_id`，`assignee_min_tier` 应为 null。

标题、发布者不能全为空白。创建只允许 `DRAFT` 或 `OPEN`。目标项目应存在且未归档。

PATCH 只允许修改 `project_id`、`title`、`description`、`acceptance_criteria`、`difficulty`、`assignee_mode`、`assignee_min_tier`、`assignee_agent_id`、`require_arbitration`。仅 DRAFT 可编辑；两个限制字段支持显式 null 清空，省略保留。合并后重新校验限制组合，失败整体回滚。其他字段传 null 保留当前值。所有输入拒绝未知字段。

`Bounty` 返回创建示例中的全部字段，另含：

- `id`：UUID。
- `lease_id`：整数，初始 0，每次成功接取递增。
- `lease_agent_id`、`lease_expires_at`：可空字符串。
- `rejection_count`：整数，初始 0。
- `completion_note`：可空字符串。
- `published_at`、`withdrawn_at`、`cancelled_at`、`completed_at`：可空时间。
- `created_at`、`updated_at`：时间。

时间为 UTC RFC3339，精确到毫秒。完整状态集合为 `DRAFT/OPEN/IN_PROGRESS/WAITING_INFO/PENDING_ARBITRATION/COMPLETED/CANCELLED`。

publish 执行 `DRAFT → OPEN` 并设置 `published_at`；withdraw 只允许无持有人租约的 `OPEN → DRAFT`，设置 `withdrawn_at`；cancel 允许 `DRAFT/OPEN/WAITING_INFO → CANCELLED` 并设置 `cancelled_at`。历史动作时间保留，重复发布更新 `published_at`。

创建记录 `created` 事件；编辑记录 `updated`；其他已实现动作记录 `published`、`withdrawn`、`cancelled` 或 `claimed`。直接创建 OPEN 记录一条 `created` 事件，其 `to_status` 为 OPEN。`BountyEvent` 字段为 `id`（整数）、`bounty_id`、`event_type`、`actor`、`from_status`（创建时 null）、`to_status`、`detail`（JSON 对象）、`created_at`。HTTP 更新和转换的 actor 为 `human`；创建事件使用 `created_by`。数据库将 `detail` 存为 `detail_json`。

Rust 仓储提供 `Repository::try_claim_bounty(id,agent_id)`，原子执行 `OPEN → IN_PROGRESS`，递增 `lease_id`，签发十五分钟租约并记录 `claimed` 事件。此入口尚无 HTTP 路由，身份及段位校验由 Agent 授权层负责。所有写事务采用 `BEGIN IMMEDIATE`，状态变化与事件同事务提交；并发撤回与抢单只有一个能成功。

## 错误

错误响应统一为：

```json
{"error":{"code":"INVALID_TRANSITION","message":"当前状态不允许此操作"}}
```

| HTTP | code | 条件 |
|---|---|---|
| 400 | `VALIDATION_ERROR` | JSON、枚举、查询参数、必填字段、图标或执行者组合不合法 |
| 401 | `UNAUTHORIZED` | 缺少或错误的管理员 Bearer token |
| 404 | `PROJECT_NOT_FOUND` | 项目不存在 |
| 404 | `BOUNTY_NOT_FOUND` | 悬赏不存在 |
| 404 | `NOT_FOUND` | 路由不存在 |
| 405 | `METHOD_NOT_ALLOWED` | 路由不支持该 HTTP 方法 |
| 409 | `PROJECT_HAS_BOUNTIES` | 删除仍有悬赏引用的项目 |
| 409 | `PROJECT_ARCHIVED` | 向归档项目创建、编辑或发布悬赏 |
| 409 | `INVALID_TRANSITION` | 非草稿编辑或不允许的状态转换 |
| 409 | `BOUNTY_ALREADY_CLAIMED` | 悬赏已接取时撤回或重复接取 |
| 500 | `INTERNAL_ERROR` | 数据库或运行时内部错误 |

数据库连接启用 WAL、外键和 5000ms busy timeout；`schema_migrations` 记录已应用版本，拒绝打开高于当前内核支持版本的数据库。
