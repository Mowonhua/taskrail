# 任务编辑

## F-EDIT-001 子任务完成与排序

完成子任务并整理完成组，支持调整同级任务的顺序。

创建提交：-

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src/lib/editor/index.ts::EditorController.toggleTask |
| 主入口 | src/lib/editor/index.ts::EditorController.moveItem |
| 关键实现 | src/lib/markdown/archive.ts::sortCompletedChildren |
| 关键实现 | src/lib/markdown/transactions.ts::moveItemChanges |
| 关键实现 | src/lib/editor/preview.ts::buildPreview |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 完成首个子任务后整理并收起完成组，不产生额外空行；撤销恢复原文 | src/lib/editor/completed-groups.test.ts::完成首个子任务后不出现额外空行 |
| S-02 | 正常 | 将文末子任务上移时保留原有文末换行状态，撤销恢复原顺序 | src/lib/editor/completed-groups.test.ts::末尾子任务上移后不留下空行, src/lib/markdown/markdown.test.ts::structure edits > handles moving EOF without newline and crossing headings between top-level lists |
