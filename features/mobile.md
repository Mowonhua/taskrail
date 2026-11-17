# 移动编辑

> 模块职责：Android 平台隔离、系统文档导出、触屏抽屉导航与应用更新。

## F-MOB-001 Android 平台隔离与适配

手机以 `app.taskrail.mobile` 作为应用标识，不调用桌面窗口控制、桌面更新插件与系统字体扫描；支持通过系统文档选择器导出文本及视口手势区抽屉导航。

创建提交：39021a9

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src/lib/files/android.ts::AndroidFilePort |
| 关键实现 | src/lib/platform.ts::platformCapabilities |
| 关键实现 | src/lib/mobile-navigation.ts::registerDrawerGestures |
| 关键实现 | src/App.svelte::<module> |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 约束 | Android 应用标识为 app.taskrail.mobile，不调用桌面窗口控制、桌面更新插件和系统字体扫描 | src/lib/platform.test.ts::Android 原生环境不具备桌面窗口能力 |
| S-02 | 正常 | 导出使用用户选择的系统文档 URI 写入文本，用户取消选择时不写入 | src/lib/files/android.test.ts::移动端平台端口边界 > 导出使用用户选择的 URI\，取消时不写入 |
| S-06 | 约束 | 手机滚动条不占布局空间，全部主题及明暗模式保持相同布局 | - |
| S-12 | 正常 | 视口正中宽 50%、高 75% 的手势区内右滑展开导航，左滑收起；左侧边缘、左右两侧与顶部横滑不触发，纵向滚动也不触发 | - |

## F-MOB-002 Android 应用更新

启动时从 `Mowonhua/taskrail` 检查并按偏好下载较新的稳定版 APK，校验安装包身份，保存配置后交给 Android 系统确认安装。

创建提交：e6e2b91

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src/lib/updater/android.ts::AndroidUpdatePort |
| 关键实现 | src/lib/updater/update-coordinator.ts::UpdateCoordinator |
| 关键实现 | src-tauri/plugins/mobile/android/src/main/java/AndroidUpdater.kt::AndroidUpdater |
| 关键实现 | src-tauri/plugins/mobile/android/src/main/java/AndroidUpdater.kt::AndroidUpdateRules |
| 关键实现 | scripts/release.mjs::prepareRelease |
| 关键实现 | scripts/release-policy.mjs::verifyReleaseFiles |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 正常 | 只选择当前版本之后带 arm64 APK 的最高稳定版，忽略草稿、预发布和其他仓库资产 | src-tauri/plugins/mobile/android/src/test/java/AndroidUpdateRulesTest.kt::AndroidUpdateRulesTest |
| S-02 | 正常 | 默认自动下载但不自动安装，下载成功和失败均释放监听 | src/lib/updater/android.test.ts::下载成功和失败都移除进度监听\，只有原生校验成功才完成下载, src/lib/updater/update-coordinator.test.ts::更新检查与后台下载 > 默认自动下载并保留累计进度\，但从不自动安装 |
| S-03 | 约束 | 下载包的大小、包名、发布版本、递增版本号和签名须全部校验通过 | - |
| S-04 | 正常 | 未授权安装时打开系统授权页，返回应用后可重试；系统安装器接管后解除保存锁并保留候选 | src/lib/updater/android.test.ts::未知来源授权不足时保留候选允许重试\，系统安装器接管后应用继续运行, src/lib/updater/update-coordinator.test.ts::更新检查与后台下载 > 系统安装器接管后解除保存锁并保留安装包\，取消安装后可重新发起 |
| S-05 | 异常 | 配置保存失败时不调用系统安装器 | src/lib/updater/update-coordinator.test.ts::安装前保存与失败恢复 > 保存未成功时保留安装包并禁止原生安装：refused |
| S-06 | 正常 | 没有可安装的新版本时结束检查且不创建候选 | src/lib/updater/android.test.ts::原生没有新版本时不创建下载会话 |
| S-07 | 约束 | 标签发布必须包含同版本签名 arm64 APK，缺失或摘要不符时禁止公开版本；重跑不覆盖已公开资源 | scripts/release.test.mjs::<module>, scripts/release-policy.test.mjs::<module>, scripts/release-github.test.mjs::<module> |
| S-08 | 异常 | 传输连接中断时重新建立连接并从已写入字节续传；服务端忽略范围请求或资源变化时从头重写 | - |
| S-09 | 异常 | 重试耗尽后界面显示统一的网络中断提示，不暴露平台英文异常 | src/lib/updater/android.test.ts::传输中断的原始英文异常被替换为可重试的网络提示 |
