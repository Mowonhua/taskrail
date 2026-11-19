---
name: taskrail-agent-mcp
description: 使用 Taskrail MCP 读取任务与收件箱，认领、续租、交付、裁决或协作草稿。
---

# Taskrail MCP

1. 调用 `taskrail_me` 确认当前 Agent 身份。
2. 调用 `taskrail_list_events` 拉取收件箱，或用 `taskrail_list_tasks` 查找任务。用 `taskrail_get_task` 读取任务详情。
3. 认领后保存 `lease_id`，在 `lease_expires_at` 前调用 `taskrail_heartbeat`。收到 `STALE_LEASE` 时停止使用旧租约，重新读取任务。
4. 缺少信息时调用 `taskrail_request_info`。等待回复期间暂停执行。收到 `task.resumed` 后沿用原租约。
5. 无需仲裁时调用 `taskrail_complete_task`。需要仲裁时调用 `taskrail_submit_arbitration`。
6. 按 `event_id` 顺序处理收件箱。前序事件都处理完毕后，用 `taskrail_ack_events` 确认最后一个已处理的事件。
