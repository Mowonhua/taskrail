//! 文件职责：连接桌面外壳命令与配置存储。
//! 定义范围：Tauri 启动入口和平台命令适配。
#[cfg(not(target_os = "android"))]
mod desktop_update;
mod external_link;
#[cfg(target_os = "windows")]
mod single_instance;
mod storage;
mod system_fonts;
mod window_material;
#[cfg(not(target_os = "android"))]
mod window_state;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use storage::FileError;
use tauri::{Manager, State};
use tauri_plugin_opener::OpenerExt;

/// 配置读写共用同一把锁，且仅在后台阻塞线程持锁，避免并发保存交错提交。
#[derive(Default)]
struct FileState {
    gate: Arc<Mutex<()>>,
}

/// 文件 IO 与等待互斥锁都在阻塞线程池中执行，避免阻塞 WebView 输入事件。
async fn disk<T: Send + 'static>(
    gate: Arc<Mutex<()>>,
    operation: impl FnOnce() -> Result<T, FileError> + Send + 'static,
) -> Result<T, FileError> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = gate
            .lock()
            .map_err(|_| FileError::new("FILE_INTERNAL", "文件锁不可用。"))?;
        operation()
    })
    .await
    .map_err(|error| FileError::new("FILE_INTERNAL", error.to_string()))?
}

fn state_path(app: &tauri::AppHandle, name: &str) -> Result<PathBuf, FileError> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join(name))
        .map_err(|error| FileError::new("STATE_PATH", error.to_string()))
}

#[tauri::command]
async fn open_external_link(url: String, app: tauri::AppHandle) -> Result<(), FileError> {
    let validated = external_link::validated_url(&url)?;
    tauri::async_runtime::spawn_blocking(move || {
        app.opener()
            .open_url(validated, None::<&str>)
            .map_err(|error| FileError::new("LINK_OPEN", error.to_string()))
    })
    .await
    .map_err(|error| FileError::new("LINK_OPEN", error.to_string()))?
}

/// 本地资源打开独立于正文持久化；文件存在性与类型校验在后台线程执行。
#[tauri::command]
async fn open_local_document(path: String, app: tauri::AppHandle) -> Result<(), FileError> {
    tauri::async_runtime::spawn_blocking(move || {
        let validated = external_link::validated_local_document(Path::new(&path))?;
        app.opener()
            .open_path(validated, None::<&str>)
            .map_err(|error| FileError::new("LINK_OPEN", error.to_string()))
    })
    .await
    .map_err(|error| FileError::new("LINK_OPEN", error.to_string()))?
}

#[tauri::command]
async fn load_config(
    app: tauri::AppHandle,
    state: State<'_, FileState>,
) -> Result<Option<Value>, FileError> {
    let path = state_path(&app, "config.json")?;
    disk(state.gate.clone(), move || storage::load_json(&path)).await
}

#[tauri::command]
async fn save_config(
    config: Value,
    app: tauri::AppHandle,
    state: State<'_, FileState>,
) -> Result<(), FileError> {
    let path = state_path(&app, "config.json")?;
    disk(state.gate.clone(), move || {
        storage::save_json(&path, &config)
    })
    .await
}

/// 导出仅开放受限 JSON 写入；输入校验、文件 IO 和提交均在共用文件锁内执行。
#[tauri::command]
async fn export_theme_file(
    path: String,
    text: String,
    state: State<'_, FileState>,
) -> Result<(), FileError> {
    disk(state.gate.clone(), move || {
        storage::export_theme_file(Path::new(&path), &text)
    })
    .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "windows")]
    let _instance = match single_instance::acquire().expect("无法仲裁 Taskrail 启动实例") {
        Some(instance) => instance,
        None => return,
    };
    let mut context = tauri::generate_context!();
    // 插件注册时读取配置；必须在运行前选择通道，正式版不能收到预发布更新。
    let channel = if context.package_info().version.pre.is_empty() {
        "stable"
    } else {
        "preview"
    };
    context
        .config_mut()
        .plugins
        .0
        .get_mut("updater")
        .expect("更新配置缺失")["endpoints"] = serde_json::json!([format!(
        "https://raw.githubusercontent.com/Mowonhua/taskrail/updates/{channel}.json"
    )]);
    #[cfg(target_os = "windows")]
    let window_key = _instance.window_key();
    // 互斥锁已先于 Tauri 初始化获取，并保持到事件循环退出。
    let builder = tauri::Builder::default().setup(move |app| {
        #[cfg(target_os = "windows")]
        single_instance::mark_window(app, &window_key)?;
        // 窗口此时尚未显示，恢复上次几何不会露出移动过程。
        #[cfg(not(target_os = "android"))]
        window_state::restore(app);
        #[cfg(target_os = "android")]
        app.handle().plugin(tauri_plugin_taskrail_mobile::init())?;
        Ok(())
    });
    #[cfg(not(target_os = "android"))]
    let builder = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(desktop_update::DesktopUpdateState::default());
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_fs::init())
        .manage(FileState::default())
        .invoke_handler(tauri::generate_handler![
            open_external_link,
            open_local_document,
            load_config,
            save_config,
            export_theme_file,
            #[cfg(not(target_os = "android"))] desktop_update::desktop_update_check,
            #[cfg(not(target_os = "android"))] desktop_update::desktop_update_download,
            #[cfg(not(target_os = "android"))] desktop_update::desktop_update_install,
            #[cfg(not(target_os = "android"))] desktop_update::desktop_update_close,
            system_fonts::list_system_fonts,
            window_material::set_window_material
        ])
        .run(context)
        .expect("无法启动 Taskrail 桌面窗口");
}
