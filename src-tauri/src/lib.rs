//! 文件职责：连接桌面外壳命令与配置存储。
//! 定义范围：Tauri 启动入口和平台命令适配。
#[cfg(not(target_os = "android"))]
mod desktop_update;
mod external_link;
#[cfg(not(target_os = "android"))]
pub mod server;
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

/// 服务句柄存活至应用退出；关闭时先停止接入，再等待在途事务完成。
#[cfg(not(target_os = "android"))]
struct EmbeddedServerState {
    info: server::models::EmbeddedServerInfo,
    handle: Mutex<Option<server::RunningServer>>,
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
fn get_embedded_server_info(
    state: State<'_, EmbeddedServerState>,
) -> server::models::EmbeddedServerInfo {
    state.info.clone()
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

/// Markdown 导出的路径校验、文件 IO 与原子提交共用配置文件锁。
#[tauri::command]
async fn export_markdown_file(
    path: String,
    text: String,
    state: State<'_, FileState>,
) -> Result<(), FileError> {
    disk(state.gate.clone(), move || {
        storage::export_markdown_file(Path::new(&path), &text)
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
        #[cfg(not(target_os = "android"))]
        {
            let config = server::ServerConfig {
                bind: "127.0.0.1:0".parse()?,
                db_path: app.path().app_data_dir()?.join("taskrail.db"),
                admin_token: server::random_admin_token()?,
                mode: server::ServerMode::Embedded,
            };
            let handle = tauri::async_runtime::block_on(server::start(config))?;
            app.manage(EmbeddedServerState {
                info: handle.info.clone(),
                handle: Mutex::new(Some(handle)),
            });
        }
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
            export_markdown_file,
            #[cfg(not(target_os = "android"))]
            get_embedded_server_info,
            #[cfg(not(target_os = "android"))]
            desktop_update::desktop_update_check,
            #[cfg(not(target_os = "android"))]
            desktop_update::desktop_update_download,
            #[cfg(not(target_os = "android"))]
            desktop_update::desktop_update_install,
            #[cfg(not(target_os = "android"))]
            desktop_update::desktop_update_close,
            system_fonts::list_system_fonts,
            window_material::set_window_material
        ])
        .build(context)
        .expect("无法启动 Taskrail 桌面窗口")
        .run(|app, event| {
            #[cfg(not(target_os = "android"))]
            if matches!(event, tauri::RunEvent::Exit) {
                let state = app.state::<EmbeddedServerState>();
                let handle = state.handle.lock().expect("服务状态锁不可用").take();
                if let Some(handle) = handle {
                    if let Err(error) = tauri::async_runtime::block_on(handle.shutdown()) {
                        eprintln!("{error}");
                    }
                }
            }
            #[cfg(target_os = "android")]
            let _ = (app, event);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_os = "android"))]
    #[test]
    fn markdown_export_uses_generated_acl_without_plugin_fs_write_permission() {
        let capabilities: Value =
            serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/capabilities.json")))
                .unwrap();
        let manifests: Value = serde_json::from_str(include_str!(concat!(
            env!("OUT_DIR"),
            "/acl-manifests.json"
        )))
        .unwrap();
        assert!(manifests.get("__app-acl__").is_none());
        assert!(capabilities["desktop"]["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|permission| !permission.to_string().contains("fs:")));

        // 使用构建生成的真实配置与 ACL；仅替换窗口运行时，不添加测试专用授权。
        let app = tauri::test::mock_builder()
            .manage(FileState::default())
            .plugin(tauri_plugin_fs::init())
            .invoke_handler(tauri::generate_handler![export_markdown_file])
            .build(tauri::generate_context!())
            .unwrap();
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("SKILL.md");
        let invoke = |cmd: &str, body: Value| {
            tauri::test::get_ipc_response(
                &webview,
                tauri::webview::InvokeRequest {
                    cmd: cmd.into(),
                    callback: tauri::ipc::CallbackFn(0),
                    error: tauri::ipc::CallbackFn(1),
                    url: if cfg!(windows) {
                        "http://tauri.localhost"
                    } else {
                        "tauri://localhost"
                    }
                    .parse()
                    .unwrap(),
                    body: tauri::ipc::InvokeBody::Json(body),
                    headers: Default::default(),
                    invoke_key: tauri::test::INVOKE_KEY.into(),
                },
            )
        };
        let text = "# 接入指南\n";
        assert!(invoke(
            "export_markdown_file",
            serde_json::json!({"path":path,"text":text})
        )
        .is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);

        let denied = invoke(
            "plugin:fs|write_text_file",
            serde_json::json!({"path":path,"text":"uncontrolled"}),
        )
        .unwrap_err();
        assert!(denied.as_str().unwrap().contains("not allowed"), "{denied}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);

        let invalid = invoke(
            "export_markdown_file",
            serde_json::json!({"path":directory.path().join("SKILL.exe"),"text":text}),
        )
        .unwrap_err();
        assert_eq!(invalid["code"], "MARKDOWN_EXPORT_PATH");
        assert!(invalid["message"].as_str().unwrap().contains("扩展名"));
    }
}
