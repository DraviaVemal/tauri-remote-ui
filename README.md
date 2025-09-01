# Tauri Remote UI

**Tauri Remote UI** is a plugin that allows you to expose your Tauri application's UI to any web browser, enabling seamless remote interaction for development and end-to-end testing. The plugin bridges your native app and commercial browsers, letting you use standard web automation tools for testing and debugging—without modifying your app's logic.

## Features

- **Remote UI Exposure:** Interact with your Tauri app from any browser.
- **Seamless Development:** Enable Development debug attachment for fronend debugging.
- **Seamless E2E Testing:** Use existing web automation/testing tools.
- **Automatic Transport Switching:** IPC for WebView, WebSocket for browsers—handled transparently.
- **Customizable Security:** Control and secure remote access as needed.
- TODO - **Zero App Changes:** No additional changes required to your app after plugin setup.
- **Future Compatibility For Test Migration:** When [CEF-RS](https://github.com/cef-rs/cef) becomes available, the same E2E tests (e.g., written with Playwright or similar tools that use the Chromium debug port) will work seamlessly in debug mode, ensuring long-term support for modern testing workflows.

## Completed Features

### Javascript
- **api/core** - `invoke`
- **api/event** - `listen`
- **api/app**
  - `defaultWindowIcon`,`fetchDataStoreIdentifiers`,`getBundleType`,
  - `getIdentifier`,`getName`,`getTauriVersion`,
  - `getVersion`,`hide`,`removeDataStore`,
  - `setDockVisibility`,`setTheme`,`show`

### Rust
- `emit` - Emit method is updated to handle in this plugin.

## Operation Flow

- **WebView:** Uses IPC for communication between frontend and backend.
- **Commercial Browser:** Uses WebSocket (WS) for remote frontend-backend communication.
- **Automatic Switching:** The Rust backend plugin and npm frontend wrapper handle transport selection automatically.
- **Security:** The exposure of the web app can be secured and customized by the end user.

## Usage

1. **Install the Rust plugin** in your Tauri project `cargo add tauri-remote-ui`.
2. **Initialize the Rust plugin** 
```rust
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
```
3. **Replace Emitter trait**
```rust
use tauri::Emitter;
webview_window.emit(data)
```
To
```rust
use tauri_remote_ui::EmitterExt;
webview_window.emit(data).await
```
4. **Start/Stop Server**
```rust
async fn enable_server(app: AppHandle) -> String {
    match app.start_remote_ui(RemoteUiConfig::default().set_port(Some(9090))).await {
        Ok(()) => format!("Server Started."),
        Err(err) => format!("Server Error {:?}", err),
    }
}
async fn disable_server(app: AppHandle) -> String {
    match app.stop_remote_ui().await {
        Ok(()) => format!("Server Stoped"),
        Err(err) => format!("Server Error {:?}", err),
    }
}
```
5. **Install the NPM plugin** in your frontend `npm i tauri-remote-ui`.
6. **Replace the NPM package** 
```typescript
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
```
To
```typescript
import { invoke } from "tauri-remote-ui/api/core";
import { listen } from "tauri-remote-ui/api/event";
```
7. **Development WebSocket Proxy** `/remote_ui_ws` proxy remote_ui url ws to your dev server like vite.
8. **Enable Source Map and update lauch.json setup in vscode to debug frontend**

## Plugin Development

- Build Rust: `cargo build`
- Build JS: `cd guest-js && pnpm build`
- Example app: See `examples/tauri-app/`

## License

MIT
