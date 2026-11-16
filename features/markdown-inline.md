# 行内 Markdown

正文、表格单元格、任务标题与更新说明共用同一套行内语法，按语义排版而不显示语法标记。

## F-INLINE-001 行内语法排版

粗体、斜体、删除线、行内代码、链接和公式在预览中显示排版结果，源码模式保留原文。删除线采用 CJK 友好定界符判定，中文标点按正文参与判断。

创建提交：2cb918f

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src/lib/markdown/syntax.ts::markdownExtensions |
| 关键实现 | src/lib/markdown/cjk.ts::cjkStrikethrough |
| 关键实现 | src/lib/markdown/render.ts::renderMarkdown |
| 关键实现 | src/lib/markdown/render.ts::renderMarkdownToHtml |
| 关键实现 | src/lib/updater/release-notes.ts::renderReleaseNotes |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 删除线闭合标记内侧是中文括号或中文标点、外侧紧接正文时仍识别为删除线，预览隐藏标记并显示删除线样式，源码保持原文 | src/lib/markdown/markdown.test.ts::Markdown 与 KaTeX 语法扩展 > 支持中文标点紧邻的 CJK 友好删除线\，同时保持空白边界不误判, src/lib/updater/release-notes.test.ts::更新说明渲染 > 中文括号紧邻删除线标记时仍然渲染 |
| S-02 | 边界 | 标记内侧为空白、两侧不满足定界符条件或第三个波浪线紧随其后时不构成删除线，标记按原文显示 | src/lib/markdown/markdown.test.ts::Markdown 与 KaTeX 语法扩展 > 支持中文标点紧邻的 CJK 友好删除线\，同时保持空白边界不误判 |
