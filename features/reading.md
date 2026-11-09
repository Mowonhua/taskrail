# 阅读排版

> 模块职责：正文的字体、字号、宽度与段落缩进等本机阅读参数。

## F-READ-001 正文段首缩进

正文段落的首行按设置缩进，缺省为两个字符，0 表示关闭；缩进以正文字号为单位，随字号变化。只有不属于列表项的顶层段落参与缩进。列表标记列占两个字符宽：文字列与两字缩进的首行起点对齐，标记停在文字前半字空隙处，其余留白落在列首；任务下方的正文、标题、引用、公式、代码和表格保持各自的结构排版，Markdown 源文不变。

创建提交：-

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src/lib/editor/preview.ts::buildPreview |
| 关键实现 | src/lib/editor/editor.css::<module> |
| 关键实现 | src/lib/SettingsPanel.svelte::<module> |
| 关键实现 | src/lib/config-defaults.ts::defaultPreferences |
| 关键实现 | src/lib/config-validation.ts::validateAppConfig |
| 关键实现 | src/App.svelte::<module> |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 待办与归档预览中，顶层正文段落的起始行按设置缩进，段落内的续行保持左端对齐；任务下方的正文、任务行、列表项、标题、引用、公式、代码和表格不受影响。列表层级以两字为步长，列表文字与两字缩进的首行起点对齐，标记与文字保持半字空隙。源码视图保留原始对齐，投影不写入 Markdown | src/lib/editor/paragraph-indent.test.ts::只有顶层正文段落的首行带段首缩进\，任务正文、列表、标题、引用、公式、代码和表格不受影响, src/lib/editor/paragraph-indent.test.ts::列表层级以两字为步长\，任务正文不额外缩进, src/lib/editor/paragraph-indent.test.ts::源码视图保留原始对齐\，不标记段首缩进 |
| S-02 | 正常 | 设置中的段首缩进可在 0 到 4 个字符之间调整并即时应用，随配置持久化，重启后恢复同一档位；正文文件保持不变 | src/lib/session/app-integration.test.ts::正文段首缩进 > 默认两字符\，修改后立即写入阅读样式并持久化\，重启恢复且不改写正文 |
| S-03 | 约束 | 偏好缺省为两个字符；缺少该字段的旧配置继续有效，0 表示关闭，负数、非有限数值和非数字使配置整体失效并保留原文件 | src/lib/config-validation.test.ts::配置边界 > 段首缩进兼容旧配置缺省\，0 表示关闭并拒绝负数、非有限数值和非数字 |
