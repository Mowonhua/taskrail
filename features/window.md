# 桌面窗口

> 模块职责：Windows 窗口的原生材质与显示过渡。

## F-WIN-001 Windows 窗口材质与显示过渡

窗口保留整窗透明与系统 Acrylic，不禁用 DWM 过渡，沿用系统默认的打开与最小化缩放动画；透明与系统材质期间抑制 GDI 背景擦除并保持最小化期间不拆除背景，从最小化恢复时同步重建 Acrylic 样本；桌面端等待首帧配置与原生材质落定后再显示窗口，且跳过网页透明度淡入。

创建提交：-

### 实现

| 角色 | 代码符号 |
|---|---|
| 主入口 | src-tauri/src/lib.rs::run |
| 关键实现 | src-tauri/src/window_material.rs::no_erase_subclass |
| 关键实现 | src-tauri/src/window_material.rs::refresh_backdrop_subclass |
| 关键实现 | src-tauri/src/window_material.rs::apply_material |
| 关键实现 | src-tauri/src/window_material.rs::set_window_material |
| 关键实现 | src/lib/window-material.ts::WindowMaterialController |
| 关键实现 | src/App.svelte |
| 关键实现 | src/main.ts |

### 行为场景

| 编号 | 类型 | 场景与预期 | 测试 |
|---|---|---|---|
| S-01 | 约束 | 主窗口启动与运行期间保持系统默认 DWM 过渡动画，不设置 `DWMWA_TRANSITIONS_FORCEDISABLED` | - |
| S-02 | 正常 | 最小化期间保留原有系统背景不提前拆除，从最小化恢复可见时同步重建系统 Acrylic 背景样本 | src-tauri/src/window_material.rs::tests::backdrop_refresh_rebuilds_acrylic_after_minimize |
| S-03 | 正常 | 透明、Blur 与 Acrylic 材质激活期间抑制 `WM_ERASEBKGND` 背景擦除，并在窗口销毁前移除所有辅助子类 | src-tauri/src/window_material.rs::tests::no_erase_background_installs_removes_and_suppresses_erase |
| S-04 | 兼容 | 系统早于 Windows 11 22H2 时，Acrylic 请求使用同等透明的系统 Blur，窗口仍透明且失焦策略保留；支持系统背景属性的版本继续使用 Acrylic | src-tauri/src/window_material.rs::tests::acrylic_effect_degrades_to_blur_before_system_backdrop |
| S-05 | 正常 | 桌面端以透明主题启动时不先触发 `opaque` 材质调用，待首帧材质与内容就绪后触发 `onready` 显示窗口，且不叠加 CSS 启动透明度淡入 | src/lib/session/app-integration.test.ts |
| S-06 | 边界 | 已处于透明状态时切换明暗模式或失焦偏好，在等待原生调用期间保持网页透明不闪回实色；后续失败或不支持时回落为实色 | src/lib/window-material.test.ts |
