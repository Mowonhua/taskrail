//! 文件职责：将窗口材质请求映射到当前调用窗口的原生效果。
//! 定义范围：材质命令参数、平台能力边界和 Tauri 命令适配。

use serde::Deserialize;
use tauri::{Theme, WebviewWindow};

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};

#[cfg(target_os = "windows")]
const KEEP_TRANSPARENCY_SUBCLASS_ID: usize = 1;

#[cfg(target_os = "windows")]
const REFRESH_BACKDROP_SUBCLASS_ID: usize = 2;

#[cfg(target_os = "windows")]
const NO_ERASE_SUBCLASS_ID: usize = 3;

/// 只维持非客户区的激活绘制状态，不改变输入焦点或原生材质。
/// 必须先将原消息交给其余子类：Tao 会从 WM_NCACTIVATE 更新真实焦点，不能把 TRUE 传进该链。
/// 转交后直接更新系统绘制，保留系统 Acrylic 的染色与遮盖程度；最小化和销毁仍遵循系统处理。
#[cfg(target_os = "windows")]
unsafe extern "system" fn keep_transparency_subclass(
    hwnd: HWND,
    message: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
    subclass_id: usize,
    _: usize,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::{
        Shell::{DefSubclassProc, GetWindowSubclass, RemoveWindowSubclass},
        WindowsAndMessaging::{DefWindowProcW, IsIconic, IsWindow, WM_NCACTIVATE, WM_NCDESTROY},
    };

    if message == WM_NCDESTROY {
        // 在原窗口过程销毁 HWND 前移除回调，随后原样转交销毁消息。
        unsafe { RemoveWindowSubclass(hwnd, Some(keep_transparency_subclass), subclass_id) };
        return unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    }
    let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    if message == WM_NCACTIVATE && wparam & 0xffff == 0 {
        let mut reference = 0;
        // 原消息处理可能重入并关闭偏好或销毁窗口；只对仍持有此策略的正常窗口补绘。
        let still_enabled = unsafe {
            IsWindow(hwnd) != 0
                && IsIconic(hwnd) == 0
                && GetWindowSubclass(
                    hwnd,
                    Some(keep_transparency_subclass),
                    subclass_id,
                    &mut reference,
                ) != 0
        };
        if still_enabled {
            // -1 阻止非客户区重绘；直接调用系统过程，不生成 Tao 的激活或焦点事件。
            unsafe { DefWindowProcW(hwnd, WM_NCACTIVATE, 1, -1) };
        }
    }
    result
}

/// 为当前 HWND 安装或移除失焦时保持 Acrylic 的绘制策略；必须在窗口所属主线程调用。
/// 同一回调与 ID 重复安装不会叠加子类，移除时不存在策略视为成功。
/// 只改变系统绘制，原材质的染色和不透明度均由正常 Acrylic 路径管理。
#[cfg(target_os = "windows")]
fn set_keep_transparency_on_blur(hwnd: HWND, enabled: bool) -> Result<(), String> {
    use windows_sys::Win32::UI::{
        Shell::{GetWindowSubclass, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            DefWindowProcW, GetForegroundWindow, IsIconic, IsWindow, WM_NCACTIVATE,
        },
    };

    if unsafe { IsWindow(hwnd) } == 0 {
        return Err("无法更新失焦透明策略：窗口句柄无效。".to_owned());
    }
    if enabled {
        if unsafe {
            SetWindowSubclass(
                hwnd,
                Some(keep_transparency_subclass),
                KEEP_TRANSPARENCY_SUBCLASS_ID,
                0,
            )
        } == 0
        {
            return Err("无法安装失焦透明策略。".to_owned());
        }
    } else {
        let mut reference = 0;
        let installed = unsafe {
            GetWindowSubclass(
                hwnd,
                Some(keep_transparency_subclass),
                KEEP_TRANSPARENCY_SUBCLASS_ID,
                &mut reference,
            )
        } != 0;
        if !installed {
            return Ok(());
        }
        if unsafe {
            RemoveWindowSubclass(
                hwnd,
                Some(keep_transparency_subclass),
                KEEP_TRANSPARENCY_SUBCLASS_ID,
            )
        } == 0
        {
            return Err("无法移除失焦透明策略。".to_owned());
        }
    }
    if unsafe { IsIconic(hwnd) } == 0 {
        // 开启时立即维持激活绘制；关闭后按真实前台窗口恢复系统外观，避免失焦窗口残留透明状态。
        let active = enabled || unsafe { GetForegroundWindow() } == hwnd;
        unsafe { DefWindowProcW(hwnd, WM_NCACTIVATE, usize::from(active), -1) };
    }
    Ok(())
}

/// 在透明或原生材质激活期间拦截 GDI 擦除，避免显示、恢复和缩放时父窗口背景刷出不透明闪烁。
#[cfg(target_os = "windows")]
unsafe extern "system" fn no_erase_subclass(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    subclass_id: usize,
    _: usize,
) -> LRESULT {
    use windows_sys::Win32::UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass},
        WindowsAndMessaging::{WM_ERASEBKGND, WM_NCDESTROY},
    };

    if message == WM_NCDESTROY {
        unsafe { RemoveWindowSubclass(hwnd, Some(no_erase_subclass), subclass_id) };
        return unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    }
    if message == WM_ERASEBKGND {
        return 1;
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

/// 为当前 HWND 安装或移除禁用 GDI 背景擦除的子类；必须在窗口所属主线程调用。
#[cfg(target_os = "windows")]
fn set_no_erase_background(hwnd: HWND, enabled: bool) -> Result<(), String> {
    use windows_sys::Win32::UI::{
        Shell::{GetWindowSubclass, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::IsWindow,
    };

    if unsafe { IsWindow(hwnd) } == 0 {
        return Err("无法更新窗口擦除抑制策略：窗口句柄无效。".to_owned());
    }
    let mut reference = 0;
    let installed = unsafe {
        GetWindowSubclass(
            hwnd,
            Some(no_erase_subclass),
            NO_ERASE_SUBCLASS_ID,
            &mut reference,
        )
    } != 0;
    if enabled {
        if installed {
            return Ok(());
        }
        if unsafe { SetWindowSubclass(hwnd, Some(no_erase_subclass), NO_ERASE_SUBCLASS_ID, 0) }
            == 0
        {
            return Err("无法安装窗口擦除抑制策略。".to_owned());
        }
        return Ok(());
    }
    if !installed {
        return Ok(());
    }
    if unsafe { RemoveWindowSubclass(hwnd, Some(no_erase_subclass), NO_ERASE_SUBCLASS_ID) } == 0 {
        return Err("无法移除窗口擦除抑制策略。".to_owned());
    }
    Ok(())
}

/// 函数职责：重建系统 Acrylic 背景样本。
/// 输入说明：hwnd 必须是已应用系统背景材质且已脱离最小化状态的窗口。
/// 输出说明：不支持系统背景的版本会静默失败，保留原有材质。
/// 实现思路：同步切换背景属性以重新采样桌面背景并等待合成帧对齐，避免恢复时透出旧背景或留下透明空档，且不改动客户区玻璃边距以免无边框窗口顶部出现系统标题边框线。
#[cfg(target_os = "windows")]
unsafe fn reset_acrylic_backdrop(hwnd: HWND) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmFlush, DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_NONE,
        DWMSBT_TRANSIENTWINDOW,
    };

    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE as u32,
            &DWMSBT_NONE as *const _ as _,
            4,
        );
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE as u32,
            &DWMSBT_TRANSIENTWINDOW as *const _ as _,
            4,
        );
        let _ = DwmFlush();
    }
}

#[cfg(target_os = "windows")]
struct RefreshBackdropState {
    was_minimized: std::sync::atomic::AtomicBool,
    restore_notified: std::sync::atomic::AtomicBool,
    window: Option<WebviewWindow>,
}

/// 维护 Acrylic 背景刷新子类：最小化期间保留已有材质不被提前拆除，从最小化恢复可见时同步重建样本并通知前端平滑过渡。
/// 销毁前移除回调并释放状态。
#[cfg(target_os = "windows")]
unsafe extern "system" fn refresh_backdrop_subclass(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    subclass_id: usize,
    ref_data: usize,
) -> LRESULT {
    use std::sync::atomic::Ordering;
    use tauri::Emitter;
    use windows_sys::Win32::UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass},
        WindowsAndMessaging::{
            IsIconic, SC_MAXIMIZE, SC_RESTORE, SIZE_MAXIMIZED, SIZE_MINIMIZED, SIZE_RESTORED,
            WM_NCDESTROY, WM_SIZE, WM_SYSCOMMAND,
        },
    };

    if message == WM_NCDESTROY {
        let removed =
            unsafe { RemoveWindowSubclass(hwnd, Some(refresh_backdrop_subclass), subclass_id) };
        if removed != 0 && ref_data != 0 {
            drop(unsafe { Box::from_raw(ref_data as *mut RefreshBackdropState) });
        }
        return unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    }
    if message == WM_SYSCOMMAND && ref_data != 0 {
        let command = (wparam & 0xfff0) as u32;
        if (command == SC_RESTORE || command == SC_MAXIMIZE) && unsafe { IsIconic(hwnd) } != 0 {
            let state = unsafe { &*(ref_data as *const RefreshBackdropState) };
            if !state.restore_notified.swap(true, Ordering::Relaxed) {
                if let Some(window) = state.window.as_ref() {
                    let _ = window.emit("taskrail:window-restored", ());
                }
            }
        }
    }
    let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    if message == WM_SIZE && ref_data != 0 {
        let state = unsafe { &*(ref_data as *const RefreshBackdropState) };
        if wparam == SIZE_MINIMIZED as WPARAM {
            // 最小化时不撤销系统背景，避免后续系统恢复动画期缺少已合成材质。
            state.was_minimized.store(true, Ordering::Relaxed);
            state.restore_notified.store(false, Ordering::Relaxed);
        } else if (wparam == SIZE_RESTORED as WPARAM || wparam == SIZE_MAXIMIZED as WPARAM)
            && state.was_minimized.swap(false, Ordering::Relaxed)
        {
            if !state.restore_notified.swap(true, Ordering::Relaxed) {
                if let Some(window) = state.window.as_ref() {
                    let _ = window.emit("taskrail:window-restored", ());
                }
            }
            unsafe { reset_acrylic_backdrop(hwnd) };
        }
    }
    result
}

/// 为当前 HWND 安装或移除 Acrylic 背景刷新策略；必须在窗口所属主线程调用。
/// 同一回调与 ID 重复安装不会叠加子类，移除时不存在策略视为成功。
#[cfg(target_os = "windows")]
fn set_refresh_backdrop(
    hwnd: HWND,
    enabled: bool,
    window: Option<WebviewWindow>,
) -> Result<(), String> {
    use std::sync::atomic::AtomicBool;
    use windows_sys::Win32::UI::{
        Shell::{GetWindowSubclass, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::IsWindow,
    };

    if unsafe { IsWindow(hwnd) } == 0 {
        return Err("无法更新窗口背景刷新：窗口句柄无效。".to_owned());
    }
    let mut reference = 0;
    let installed = unsafe {
        GetWindowSubclass(
            hwnd,
            Some(refresh_backdrop_subclass),
            REFRESH_BACKDROP_SUBCLASS_ID,
            &mut reference,
        )
    } != 0;
    if enabled {
        if installed {
            return Ok(());
        }
        let state = Box::into_raw(Box::new(RefreshBackdropState {
            was_minimized: AtomicBool::new(false),
            restore_notified: AtomicBool::new(false),
            window,
        }));
        if unsafe {
            SetWindowSubclass(
                hwnd,
                Some(refresh_backdrop_subclass),
                REFRESH_BACKDROP_SUBCLASS_ID,
                state as usize,
            )
        } == 0
        {
            drop(unsafe { Box::from_raw(state) });
            return Err("无法安装窗口背景刷新策略。".to_owned());
        }
        return Ok(());
    }
    if !installed {
        return Ok(());
    }
    if unsafe {
        RemoveWindowSubclass(
            hwnd,
            Some(refresh_backdrop_subclass),
            REFRESH_BACKDROP_SUBCLASS_ID,
        )
    } == 0
    {
        return Err("无法移除窗口背景刷新策略。".to_owned());
    }
    if reference != 0 {
        drop(unsafe { Box::from_raw(reference as *mut RefreshBackdropState) });
    }
    Ok(())
}

/// 结构职责：限定前端可请求的窗口背景材质。
/// 字段说明：Opaque 与 Transparent 都清除原生效果，由前端分别绘制实色或透明背景；Blur 与 Acrylic 请求桌面内容模糊。
/// 约束条件：仅接受小写枚举值，不携带窗口标签或主题身份。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowMaterial {
    Opaque,
    Transparent,
    Blur,
    Acrylic,
}

/// 函数职责：为发起命令的窗口提交材质变更，不操作其他窗口。
/// 输入说明：window 由 Tauri 注入；material 限制为四种合法值，theme 为显式 light、dark 或跟随系统的 null；keep_transparent_on_blur 缺省为 false，仅影响 Acrylic。
/// 输出说明：true 表示原生调用完成，false 表示平台或版本不支持；调用错误返回可显示的字符串。
/// 实现思路：将原生调用送到主线程并异步等待结果，不把入队成功作为应用成功。
#[tauri::command]
pub async fn set_window_material(
    window: WebviewWindow,
    material: WindowMaterial,
    theme: Option<Theme>,
    keep_transparent_on_blur: Option<bool>,
) -> Result<bool, String> {
    let (sender, mut receiver) = tauri::async_runtime::channel(1);
    let target = window.clone();
    window
        .run_on_main_thread(move || {
            let result = apply_material(
                &target,
                material,
                theme,
                keep_transparent_on_blur.unwrap_or(false),
            );
            // 容量为一且仅发送一次；前端已关闭时接收端可能释放，无需重试原生操作。
            let _ = sender.try_send(result);
        })
        .map_err(|error| format!("无法调度窗口材质变更：{error}"))?;
    receiver
        .recv()
        .await
        .ok_or_else(|| "窗口材质变更未返回结果。".to_owned())?
}

/// 结构职责：区分 Acrylic 在两类 Windows 上的原生实现。
/// 字段说明：Blur 保留透明模糊但不使用旧版 Acrylic；SystemBackdrop 使用系统的背景材质属性。
/// 约束条件：只由系统版本决定，不依赖主题身份或用户偏好。
#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AcrylicEffect {
    Blur,
    SystemBackdrop,
}

/// Windows 11 22H2 起提供系统背景属性；更早的系统只能使用逐帧采样的旧版 Acrylic。
#[cfg(target_os = "windows")]
const SYSTEM_BACKDROP_MIN_BUILD: u32 = 22523;

/// 函数职责：按系统 build 选择 Acrylic 的原生实现。
/// 输入说明：build 为当前系统的构建号。
/// 输出说明：支持系统背景属性时返回 SystemBackdrop，否则返回 Blur。
/// 实现思路：旧版 Acrylic 在 Windows 10 上拖动和缩放时逐帧采样，明显迟滞且松手后仍在补帧，改用同等透明的系统 Blur 规避；Windows 11 不受影响。
#[cfg(target_os = "windows")]
fn acrylic_effect(build: u32) -> AcrylicEffect {
    if build >= SYSTEM_BACKDROP_MIN_BUILD {
        AcrylicEffect::SystemBackdrop
    } else {
        AcrylicEffect::Blur
    }
}

/// 函数职责：在主线程同步窗口明暗，按目标材质切换原生效果与辅助策略。
/// 输入说明：window 必须是调用命令所属窗口，theme 保留用户的明暗偏好，None 表示跟随系统；keep_transparent_on_blur 为 Acrylic 的失焦策略；调用方必须保证主线程执行。
/// 输出说明：不支持目标材质返回 false；原生调用失败时清除效果并返回错误。
/// 实现思路：保留同类原生材质以避免明暗或失焦偏好切换时闪断；其他平台仅支持无原生效果模式。
fn apply_material(
    window: &WebviewWindow,
    material: WindowMaterial,
    theme: Option<Theme>,
    keep_transparent_on_blur: bool,
) -> Result<bool, String> {
    // 主线程的 set_theme 同步更新原生明暗后再应用材质，避免浅色网页沿用深色 Acrylic 底色。
    // 原生主题会同步 WebView 的 prefers-color-scheme；跟随系统必须传 None 解除显式覆盖，不能传已解析的明暗值。
    window
        .set_theme(theme)
        .map_err(|error| format!("无法同步窗口明暗：{error}"))?;
    #[cfg(target_os = "windows")]
    {
        let effect = acrylic_effect(windows_version::OsVersion::current().build);
        match material {
            WindowMaterial::Opaque => {
                clear_material(window)?;
                Ok(true)
            }
            WindowMaterial::Transparent => {
                clear_material(window)?;
                let hwnd = window
                    .hwnd()
                    .map_err(|error| format!("无法获取窗口材质句柄：{error}"))?;
                set_no_erase_background(hwnd.0, true)?;
                Ok(true)
            }
            WindowMaterial::Blur => {
                clear_acrylic_layer(window)?;
                match window_vibrancy::apply_blur(window, None) {
                    Ok(()) => {
                        let result = window
                            .hwnd()
                            .map_err(|error| format!("无法获取窗口材质句柄：{error}"))
                            .and_then(|hwnd| set_no_erase_background(hwnd.0, true));
                        if let Err(error) = result {
                            clear_material(window)?;
                            return Err(error);
                        }
                        Ok(true)
                    }
                    Err(error) => {
                        clear_material(window)?;
                        match error {
                            window_vibrancy::Error::UnsupportedPlatform(_)
                            | window_vibrancy::Error::UnsupportedPlatformVersion(_) => Ok(false),
                            _ => Err(format!("无法应用窗口材质：{error}")),
                        }
                    }
                }
            }
            WindowMaterial::Acrylic => match effect {
                AcrylicEffect::Blur => {
                    clear_acrylic_layer(window)?;
                    match window_vibrancy::apply_blur(window, None) {
                        Ok(()) => {
                            let result = window
                                .hwnd()
                                .map_err(|error| format!("无法获取窗口材质句柄：{error}"))
                                .and_then(|hwnd| {
                                    set_no_erase_background(hwnd.0, true)?;
                                    set_keep_transparency_on_blur(
                                        hwnd.0,
                                        keep_transparent_on_blur,
                                    )
                                });
                            if let Err(error) = result {
                                clear_material(window)?;
                                return Err(error);
                            }
                            Ok(true)
                        }
                        Err(error) => {
                            clear_material(window)?;
                            match error {
                                window_vibrancy::Error::UnsupportedPlatform(_)
                                | window_vibrancy::Error::UnsupportedPlatformVersion(_) => {
                                    Ok(false)
                                }
                                _ => Err(format!("无法应用窗口材质：{error}")),
                            }
                        }
                    }
                }
                AcrylicEffect::SystemBackdrop => {
                    clear_blur_layer(window)?;
                    let hwnd = window
                        .hwnd()
                        .map_err(|error| format!("无法获取窗口材质句柄：{error}"))?;
                    match window_vibrancy::apply_acrylic(window, None) {
                        Ok(()) => {
                            let result = set_no_erase_background(hwnd.0, true)
                                .and_then(|()| {
                                    set_refresh_backdrop(hwnd.0, true, Some(window.clone()))
                                })
                                .and_then(|()| {
                                    set_keep_transparency_on_blur(
                                        hwnd.0,
                                        keep_transparent_on_blur,
                                    )
                                });
                            if let Err(error) = result {
                                clear_material(window)?;
                                return Err(error);
                            }
                            Ok(true)
                        }
                        Err(error) => {
                            clear_material(window)?;
                            match error {
                                window_vibrancy::Error::UnsupportedPlatform(_)
                                | window_vibrancy::Error::UnsupportedPlatformVersion(_) => {
                                    Ok(false)
                                }
                                _ => Err(format!("无法应用窗口材质：{error}")),
                            }
                        }
                    }
                }
            },
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (window, keep_transparent_on_blur);
        Ok(matches!(
            material,
            WindowMaterial::Opaque | WindowMaterial::Transparent
        ))
    }
}

/// 仅清除旧版 Blur，保留 SystemBackdrop 以避免同类切换时闪烁。
#[cfg(target_os = "windows")]
fn clear_blur_layer(window: &WebviewWindow) -> Result<(), String> {
    match window_vibrancy::clear_blur(window) {
        Ok(())
        | Err(window_vibrancy::Error::UnsupportedPlatform(_))
        | Err(window_vibrancy::Error::UnsupportedPlatformVersion(_)) => Ok(()),
        Err(error) => Err(format!("无法清除窗口材质：{error}")),
    }
}

/// 清除系统 Acrylic 与辅助刷新策略，用于切到 Blur。
#[cfg(target_os = "windows")]
fn clear_acrylic_layer(window: &WebviewWindow) -> Result<(), String> {
    let hwnd = window
        .hwnd()
        .map_err(|error| format!("无法获取窗口材质句柄：{error}"))?;
    let refresh_result = set_refresh_backdrop(hwnd.0, false, None);
    let transparency_result = set_keep_transparency_on_blur(hwnd.0, false);
    let acrylic_result = window_vibrancy::clear_acrylic(window);
    refresh_result?;
    transparency_result?;
    match acrylic_result {
        Ok(())
        | Err(window_vibrancy::Error::UnsupportedPlatform(_))
        | Err(window_vibrancy::Error::UnsupportedPlatformVersion(_)) => Ok(()),
        Err(error) => Err(format!("无法清除窗口材质：{error}")),
    }
}

/// 函数职责：移除当前窗口可能残留的 Blur、Acrylic 与全部辅助子类策略。
/// 输入说明：调用方必须位于主线程。
/// 输出说明：无法支持的效果视为无需清除；其他原生错误保留为字符串。
/// 实现思路：先移除所有辅助子类，再清除 Blur 和 Acrylic；全部先发出清理请求，避免一种失败阻止另一种清理。
#[cfg(target_os = "windows")]
fn clear_material(window: &WebviewWindow) -> Result<(), String> {
    let hwnd = window
        .hwnd()
        .map_err(|error| format!("无法获取窗口材质句柄：{error}"))?;
    let erase_result = set_no_erase_background(hwnd.0, false);
    let refresh_result = set_refresh_backdrop(hwnd.0, false, None);
    let transparency_result = set_keep_transparency_on_blur(hwnd.0, false);
    let results = [
        window_vibrancy::clear_blur(window),
        window_vibrancy::clear_acrylic(window),
    ];
    erase_result?;
    refresh_result?;
    transparency_result?;
    for result in results {
        match result {
            Ok(())
            | Err(window_vibrancy::Error::UnsupportedPlatform(_))
            | Err(window_vibrancy::Error::UnsupportedPlatformVersion(_)) => {}
            Err(error) => return Err(format!("无法清除窗口材质：{error}")),
        }
    }
    Ok(())
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use windows_sys::Win32::{
        Foundation::{LPARAM, LRESULT, WPARAM},
        Graphics::Dwm::{
            DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_NONE,
            DWMSBT_TRANSIENTWINDOW,
        },
        UI::{
            Shell::{DefSubclassProc, GetWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, GetForegroundWindow, SendMessageW, SIZE_MINIMIZED,
                SIZE_RESTORED, WM_ACTIVATE, WM_ERASEBKGND, WM_NCACTIVATE, WM_NCDESTROY, WM_SIZE,
                WS_OVERLAPPED,
            },
        },
    };

    struct TestWindow(HWND);

    impl TestWindow {
        fn new() -> Self {
            // STATIC 是系统预注册类；隐藏窗口仅在创建线程使用，不争抢用户的前台焦点。
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    class.as_ptr(),
                    WS_OVERLAPPED,
                    0,
                    0,
                    32,
                    32,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            assert!(!hwnd.is_null(), "无法创建测试窗口");
            Self(hwnd)
        }
    }

    impl Drop for TestWindow {
        fn drop(&mut self) {
            // 即使断言失败也释放测试 HWND；测试不触碰用户正在使用的窗口。
            if !self.0.is_null() {
                unsafe { DestroyWindow(self.0) };
            }
        }
    }

    #[derive(Default)]
    struct MessageObserver {
        messages: Vec<(u32, WPARAM, LPARAM)>,
        transparency_removed_before_destroy: bool,
        no_erase_removed_before_destroy: bool,
        refresh_removed_before_destroy: bool,
    }

    unsafe extern "system" fn observe_messages(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _: usize,
        reference: usize,
    ) -> LRESULT {
        // Box 保证观察数据地址在子类存在期间稳定；回调不抛出断言，避免 panic 穿过原生 ABI。
        let observer = unsafe { &mut *(reference as *mut MessageObserver) };
        if matches!(message, WM_NCACTIVATE | WM_ACTIVATE) {
            observer.messages.push((message, wparam, lparam));
        }
        if message == WM_NCDESTROY {
            let mut reference = 0;
            observer.transparency_removed_before_destroy = unsafe {
                GetWindowSubclass(
                    hwnd,
                    Some(keep_transparency_subclass),
                    KEEP_TRANSPARENCY_SUBCLASS_ID,
                    &mut reference,
                )
            } == 0;
            observer.no_erase_removed_before_destroy = unsafe {
                GetWindowSubclass(
                    hwnd,
                    Some(no_erase_subclass),
                    NO_ERASE_SUBCLASS_ID,
                    &mut reference,
                )
            } == 0;
            observer.refresh_removed_before_destroy = unsafe {
                GetWindowSubclass(
                    hwnd,
                    Some(refresh_backdrop_subclass),
                    REFRESH_BACKDROP_SUBCLASS_ID,
                    &mut reference,
                )
            } == 0;
        }
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }

    fn install_observer(window: &TestWindow, observer: &mut MessageObserver) {
        assert_ne!(
            unsafe {
                SetWindowSubclass(
                    window.0,
                    Some(observe_messages),
                    1,
                    observer as *mut _ as usize,
                )
            },
            0
        );
    }

    fn transparency_installed(window: &TestWindow) -> bool {
        let mut reference = 0;
        (unsafe {
            GetWindowSubclass(
                window.0,
                Some(keep_transparency_subclass),
                KEEP_TRANSPARENCY_SUBCLASS_ID,
                &mut reference,
            )
        }) != 0
    }

    fn refresh_backdrop_installed(window: &TestWindow) -> bool {
        let mut reference = 0;
        (unsafe {
            GetWindowSubclass(
                window.0,
                Some(refresh_backdrop_subclass),
                REFRESH_BACKDROP_SUBCLASS_ID,
                &mut reference,
            )
        }) != 0
    }

    fn no_erase_installed(window: &TestWindow) -> bool {
        let mut reference = 0;
        (unsafe {
            GetWindowSubclass(
                window.0,
                Some(no_erase_subclass),
                NO_ERASE_SUBCLASS_ID,
                &mut reference,
            )
        }) != 0
    }

    fn read_system_backdrop(window: &TestWindow) -> Option<i32> {
        let mut value = DWMSBT_NONE;
        let result = unsafe {
            DwmGetWindowAttribute(
                window.0,
                DWMWA_SYSTEMBACKDROP_TYPE as u32,
                &mut value as *mut _ as _,
                4,
            )
        };
        (result >= 0).then_some(value)
    }

    #[test]
    fn acrylic_effect_degrades_to_blur_before_system_backdrop() {
        // Windows 10 各版本只能使用旧版 Acrylic；Windows 11 21H2 也早于系统背景属性。
        assert_eq!(acrylic_effect(19045), AcrylicEffect::Blur);
        assert_eq!(acrylic_effect(22000), AcrylicEffect::Blur);
        // 22H2 起使用系统背景属性，拖动和缩放不再受旧版采样影响。
        assert_eq!(acrylic_effect(22523), AcrylicEffect::SystemBackdrop);
        assert_eq!(acrylic_effect(22631), AcrylicEffect::SystemBackdrop);
    }

    #[test]
    fn keep_transparency_rejects_an_invalid_window() {
        assert!(set_keep_transparency_on_blur(std::ptr::null_mut(), true).is_err());
        assert!(set_keep_transparency_on_blur(std::ptr::null_mut(), false).is_err());
    }

    #[test]
    fn keep_transparency_installs_and_removes_idempotently() {
        let window = TestWindow::new();
        assert!(!transparency_installed(&window));
        assert!(set_keep_transparency_on_blur(window.0, false).is_ok());
        assert!(set_keep_transparency_on_blur(window.0, true).is_ok());
        assert!(set_keep_transparency_on_blur(window.0, true).is_ok());
        assert!(transparency_installed(&window));
        assert!(set_keep_transparency_on_blur(window.0, false).is_ok());
        assert!(set_keep_transparency_on_blur(window.0, false).is_ok());
        assert!(!transparency_installed(&window));
    }

    #[test]
    fn keep_transparency_preserves_activation_messages_and_foreground_window() {
        let mut observer = Box::<MessageObserver>::default();
        let window = TestWindow::new();
        install_observer(&window, &mut observer);
        let foreground = unsafe { GetForegroundWindow() };
        assert!(set_keep_transparency_on_blur(window.0, true).is_ok());
        let expected = [
            (WM_NCACTIVATE, 0, -1),
            (WM_ACTIVATE, 0x10000, 0),
            (WM_NCACTIVATE, 1, 0),
        ];
        for (message, wparam, lparam) in expected {
            unsafe { SendMessageW(window.0, message, wparam, lparam) };
        }
        assert_eq!(observer.messages, expected);
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
        assert!(set_keep_transparency_on_blur(window.0, false).is_ok());
        assert_eq!(
            observer.messages, expected,
            "安装与移除只更新绘制，不制造焦点事件"
        );
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    }

    #[test]
    fn native_subclasses_are_removed_before_native_window_destruction() {
        let mut observer = Box::<MessageObserver>::default();
        let mut window = TestWindow::new();
        install_observer(&window, &mut observer);
        assert!(set_keep_transparency_on_blur(window.0, true).is_ok());
        assert!(set_no_erase_background(window.0, true).is_ok());
        assert!(set_refresh_backdrop(window.0, true, None).is_ok());
        assert_ne!(unsafe { DestroyWindow(window.0) }, 0);
        window.0 = std::ptr::null_mut();
        assert!(observer.transparency_removed_before_destroy);
        assert!(observer.no_erase_removed_before_destroy);
        assert!(observer.refresh_removed_before_destroy);
    }

    #[test]
    fn no_erase_background_installs_removes_and_suppresses_erase() {
        assert!(set_no_erase_background(std::ptr::null_mut(), true).is_err());
        assert!(set_no_erase_background(std::ptr::null_mut(), false).is_err());
        let window = TestWindow::new();
        assert!(!no_erase_installed(&window));
        assert!(set_no_erase_background(window.0, false).is_ok());
        assert!(set_no_erase_background(window.0, true).is_ok());
        assert!(set_no_erase_background(window.0, true).is_ok());
        assert!(no_erase_installed(&window));
        assert_eq!(unsafe { SendMessageW(window.0, WM_ERASEBKGND, 0, 0) }, 1);
        assert!(set_no_erase_background(window.0, false).is_ok());
        assert!(set_no_erase_background(window.0, false).is_ok());
        assert!(!no_erase_installed(&window));
    }

    #[test]
    fn backdrop_refresh_installs_and_removes_idempotently() {
        let window = TestWindow::new();
        assert!(!refresh_backdrop_installed(&window));
        assert!(set_refresh_backdrop(window.0, false, None).is_ok());
        assert!(set_refresh_backdrop(window.0, true, None).is_ok());
        assert!(set_refresh_backdrop(window.0, true, None).is_ok());
        assert!(refresh_backdrop_installed(&window));
        assert!(set_refresh_backdrop(window.0, false, None).is_ok());
        assert!(set_refresh_backdrop(window.0, false, None).is_ok());
        assert!(!refresh_backdrop_installed(&window));
    }

    #[test]
    fn backdrop_refresh_rebuilds_acrylic_after_minimize() {
        let window = TestWindow::new();
        let none = DWMSBT_NONE;
        if unsafe {
            DwmSetWindowAttribute(
                window.0,
                DWMWA_SYSTEMBACKDROP_TYPE as u32,
                &none as *const _ as _,
                4,
            )
        } < 0
        {
            // Windows 10 使用旧的 Acrylic 路径，没有需要验证的系统背景属性。
            return;
        }
        assert!(set_refresh_backdrop(window.0, true, None).is_ok());
        // 未经历最小化的普通恢复不应多余重建。
        unsafe { SendMessageW(window.0, WM_SIZE, SIZE_RESTORED as WPARAM, 0) };
        if let Some(value) = read_system_backdrop(&window) {
            assert_eq!(value, DWMSBT_NONE);
        }

        // 最小化期间保留原有系统背景，不提前拆除导致恢复过渡透明。
        let transient = DWMSBT_TRANSIENTWINDOW;
        unsafe {
            DwmSetWindowAttribute(
                window.0,
                DWMWA_SYSTEMBACKDROP_TYPE as u32,
                &transient as *const _ as _,
                4,
            )
        };
        unsafe { SendMessageW(window.0, WM_SIZE, SIZE_MINIMIZED as WPARAM, 0) };
        if let Some(value) = read_system_backdrop(&window) {
            assert_eq!(value, DWMSBT_TRANSIENTWINDOW);
        }

        // 模拟系统丢失/清空背景后从最小化恢复，子类应同步重建为 DWMSBT_TRANSIENTWINDOW。
        unsafe {
            DwmSetWindowAttribute(
                window.0,
                DWMWA_SYSTEMBACKDROP_TYPE as u32,
                &none as *const _ as _,
                4,
            )
        };
        unsafe { SendMessageW(window.0, WM_SIZE, SIZE_RESTORED as WPARAM, 0) };
        if let Some(value) = read_system_backdrop(&window) {
            assert_eq!(value, DWMSBT_TRANSIENTWINDOW);
        }
    }
}
