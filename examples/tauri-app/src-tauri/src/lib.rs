use serde_json::json;
use std::sync::{Arc, RwLock};
use tauri::{AppHandle, Manager};
use tauri_remote_ui::{EmitterExt, RemoteUiConfig, RemoteUiExt};

#[tauri::command]
fn enable_server(app: AppHandle) -> String {
    match app.start_remote_ui(RemoteUiConfig::default().set_port(Some(9090))) {
        Ok((origin, port)) => format!("Server Started at {origin}:{port}"),
        Err(err) => format!("Server Error {:?}", err),
    }
}

#[tauri::command]
fn increment(app: AppHandle) {
    app.state::<Arc<RwLock<Counter>>>().write().unwrap().now += 1;
    let counter = app.state::<Arc<RwLock<Counter>>>().read().unwrap().now;
    app.get_webview_window("main")
        .unwrap()
        .emit(
            "counter",
            json!({
            "result":counter
            }),
        )
        .unwrap();
}

#[tauri::command]
fn decrement(app: AppHandle) {
    app.state::<Arc<RwLock<Counter>>>().write().unwrap().now -= 1;
    let counter = app.state::<Arc<RwLock<Counter>>>().read().unwrap().now;
    app.get_webview_window("main")
        .unwrap()
        .emit(
            "counter",
            json!({
            "result":counter
            }),
        )
        .unwrap();
}

#[tauri::command]
fn disable_server(app: AppHandle) -> String {
    match app.stop_remote_ui() {
        Ok(()) => format!("Server Stoped"),
        Err(err) => format!("Server Error {:?}", err),
    }
}

#[tauri::command]
fn exit_app(app: AppHandle) {
    app.exit(0);
}

pub struct Counter {
    pub now: u32,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            increment,
            decrement,
            enable_server,
            disable_server,
            exit_app,
        ])
        .plugin(tauri_remote_ui::init())
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                use tauri::Manager;
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            app.manage(Arc::new(RwLock::new(Counter { now: 0 })));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
