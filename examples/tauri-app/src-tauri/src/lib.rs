use serde_json::json;
use std::sync::{Arc, RwLock};
use tauri::{AppHandle, Manager};
use tauri_remote_ui::{EmitterExt, RemoteUiConfig, RemoteUiExt};

#[tauri::command]
async fn enable_server(app: AppHandle) -> String {
    match app
        .start_remote_ui(RemoteUiConfig::default().set_port(Some(9090)))
        .await
    {
        Ok(()) => format!("Server Started."),
        Err(err) => format!("Server Error {:?}", err),
    }
}

#[tauri::command]
async fn disable_server(app: AppHandle) -> String {
    match app.stop_remote_ui().await {
        Ok(()) => format!("Server Stoped"),
        Err(err) => format!("Server Error {:?}", err),
    }
}

#[tauri::command]
fn exit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
async fn increment(app: AppHandle) {
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
        .await
        .unwrap();
}

#[tauri::command]
async fn decrement(app: AppHandle) {
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
        .await
        .unwrap();
}

pub struct Counter {
    pub now: i32,
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_remote_ui::init())
        .invoke_handler(tauri::generate_handler![
            increment,
            decrement,
            enable_server,
            disable_server,
            exit_app,
        ])
        .setup(|app| {
            app.manage(Arc::new(RwLock::new(Counter { now: 0 })));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
