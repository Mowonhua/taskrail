# 阅读排版

> 模块职责：正文的字体、字号、宽度与段落缩进等本机阅读参数。

## F-READ-001 正文段首缩进

正文段落的首行按设置缩进，缺省为两个字符，0 表示关闭；缩进以正文字号为单位，随字号变化，范围限制在 0 到 4 个字符之间，即时写入 `--paragraph-indent` 样式变量并随本机配置持久化。

创建提交：408d01e

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src/App.svelte::applyVisualPreferences |
| 关键实现 | src/App.svelte::handleChangeParagraphIndent |
| 关键实现 | src/lib/SettingsPanel.svelte::<module> |
| 关键实现 | src/lib/config-defaults.ts::defaultPreferences |
| 关键实现 | src/lib/config-validation.ts::validateAppConfig |
| 关键实现 | src/app.css::<module> |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 正文段落缩进变量 `--paragraph-indent` 随设置值以 `em` 为单位生效，0 时输出 `0em` | src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 正文段首缩进默认两字符\，调整后即时写入样式变量并持久化 |
| S-02 | 正常 | 设置中的段首缩进可在 0 到 4 个字符之间调整并即时应用，随配置持久化 | src/lib/session/app-integration.test.ts::悬赏板工作台外壳集成测试 > 正文段首缩进默认两字符\，调整后即时写入样式变量并持久化 |
| S-03 | 约束 | 偏好缺省为两个字符；兼容旧配置的布尔与数值字段，0 表示关闭，负数、非有限数值和非数字使配置整体失效并保留原文件 | src/lib/config-validation.test.ts::应用配置校验 > 加载规范字段与旧布尔缩进并忽略遗留顶层字段, src/lib/config-validation.test.ts::应用配置校验 > 拒绝损坏的项目结构、重复 ID、悬空活动项目和越界偏好 |
