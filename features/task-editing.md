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
| S-01 | 正常 | 完成非末尾子任务后整理并收起完成组，保留条目间及列表外的分隔，不在父任务下方增加空行；撤销恢复原文 | src/lib/editor/completed-groups.test.ts::完成首个子任务后不出现额外空行, src/lib/editor/completed-groups.test.ts::完成松散子任务不在归档前叠加空行 "\n", src/lib/editor/completed-groups.test.ts::完成松散子任务不在归档前叠加空行 "\n  ", src/lib/markdown/markdown.test.ts::structure edits > 松散子任务完成和移到末尾保留 CRLF 分隔与定位 |
| S-02 | 正常 | 将文末子任务上移或将其他子任务拖到末尾时，保留列表内外分隔及文末换行状态，光标跟随原条目；撤销恢复原顺序 | src/lib/editor/completed-groups.test.ts::末尾子任务上移后不留下空行, src/lib/editor/completed-groups.test.ts::子任务拖到末尾保留原有行分隔 "", src/lib/editor/completed-groups.test.ts::松散子任务拖到末尾保留列表外分隔 "\n", src/lib/markdown/markdown.test.ts::structure edits > 松散子任务完成和移到末尾保留 CRLF 分隔与定位, src/lib/markdown/markdown.test.ts::structure edits > handles moving EOF without newline and crossing headings between top-level lists |
| S-03 | 正常 | 完成子任务、展开完成组并取消完成后，将恢复项从末尾拖回上方，源文与可见行恢复原状，父任务下方不增加空行；光标、撤销和重做保持一致 | src/lib/editor/completed-groups.test.ts::完成恢复后拖回上方不新增空行：松散列表无尾换行, src/lib/editor/completed-groups.test.ts::完成恢复后拖回上方不新增空行：松散列表有尾换行, src/lib/editor/completed-groups.test.ts::完成恢复后拖回上方不新增空行：松散列表后接归档, src/lib/editor/completed-groups.test.ts::完成恢复后拖回上方不新增空行：松散列表空行含缩进, src/lib/markdown/markdown.test.ts::structural edge cases > 松散列表末项上移保留 CRLF 尾部分隔 "" |
