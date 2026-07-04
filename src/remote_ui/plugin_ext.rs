//! Remote UI Plugin Extension for Tauri
//!
//! This module provides the main plugin state, initialization, and core APIs for the remote UI system.
//! It enables RPC invocation and event emission over WebSocket for Tauri applications.
//!
//! # License
//! AGPL-3.0-only License
//! Copyright (c) 2025 DraviaVemal
//! See LICENSE file in the root directory.

use crate::{RpcServer, RpcStatus, WsPayload};
use futures::{stream::SplitSink, SinkExt};
use hyper::upgrade::Upgraded;
use hyper_tungstenite::{tungstenite::Message, WebSocketStream};
use hyper_util::rt::TokioIo;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use std::sync::Arc;
use tauri::{plugin::PluginApi, AppHandle, Error, Listener, Manager, Runtime};
use tokio::sync::{Mutex, RwLock};

/// Initialize the remote UI plugin state for Tauri.
///
/// This function sets up the shared state for the plugin, including the RPC server and app handle.
/// It should be called from the plugin setup code.
pub fn init<R, C>(app: &AppHandle, _api: PluginApi<R, C>) -> crate::Result<Arc<RwLock<RemoteUi>>>
where
    C: DeserializeOwned,
    R: Runtime,
{
    let app_handle = Arc::new(app.clone());
    let remote_ui = Arc::new(RwLock::new(RemoteUi {
        app: app_handle.clone(),
        rpc_server: RpcServer::new(app_handle),
    }));
    Ok(remote_ui)
}

/// Main plugin state for remote UI APIs.
///
/// Holds references to the Tauri app and the RPC server for remote UI communication.
#[derive(Debug)]
pub struct RemoteUi {
    /// Reference to the Tauri application handle.
    pub(crate) app: Arc<AppHandle>,
    /// The RPC server instance for remote UI.
    pub(crate) rpc_server: RpcServer,
}

impl RemoteUi {
    /// Returns whether the remote UI RPC server is currently active.
    pub(crate) fn is_rpc_active(&self) -> bool {
        self.rpc_server.is_active()
    }

    /// Invoke an RPC command from a WebSocket payload.
    ///
    /// This method deserializes the payload, executes the command in the Tauri window,
    /// and sends the result back over WebSocket.
    pub(crate) fn invoke_rpc(
        &self,
        payload: &str,
        session: Arc<Mutex<SplitSink<WebSocketStream<TokioIo<Upgraded>>, Message>>>,
    ) -> Result<(), Error> {
        let ws_payload: WsPayload = serde_json::from_str(payload).map_err(|err| {
            Error::PluginInitialization(
                "tauri-remote-ui".to_owned(),
                format!("Failed to parse WS payload. Err: {err}"),
            )
        })?;
        let window_label = self.rpc_server.primary_window_label().to_owned();
        let window = self.app.get_webview_window(&window_label).ok_or_else(|| {
            Error::AssetNotFound(format!("Webview window '{window_label}' not found",))
        })?;
        let req_unique_id = format!("remote-ui::result::{}", &ws_payload.id);
        self.app
            .app_handle()
            .once_any(&req_unique_id, move |handler| {
                // Spawn a new task to send the message asynchronously
                let payload = handler.payload().to_string();
                let id = ws_payload.id;
                tauri::async_runtime::spawn(async move {
                    if let Err(err) = session
                        .lock()
                        .await
                        .send(Message::text(
                            json!({"id":id,"payload":payload}).to_string(),
                        ))
                        .await
                    {
                        log::error!("WS send message failed: {err}");
                    }
                });
            });
        // JSON-encode every interpolated input so untrusted strings from the
        // socket cannot escape the JS string context inside `window.eval`.
        let cmd_json = serde_json::to_string(&ws_payload.cmd).map_err(|err| {
            Error::PluginInitialization(
                "tauri-remote-ui".to_owned(),
                format!("Failed to serialize cmd: {err}"),
            )
        })?;
        let args_json = serde_json::to_string(&ws_payload.args).map_err(|err| {
            Error::PluginInitialization(
                "tauri-remote-ui".to_owned(),
                format!("Failed to serialize args: {err}"),
            )
        })?;
        let opts_json = serde_json::to_string(&ws_payload.option).map_err(|err| {
            Error::PluginInitialization(
                "tauri-remote-ui".to_owned(),
                format!("Failed to serialize options: {err}"),
            )
        })?;
        let event_json = serde_json::to_string(&req_unique_id).map_err(|err| {
            Error::PluginInitialization(
                "tauri-remote-ui".to_owned(),
                format!("Failed to serialize event id: {err}"),
            )
        })?;
        let js = format!(
            r#"
            window.__TAURI_INTERNALS__.invoke({cmd}, {args}, {opts})
                .then((res) => {{
                    window.__TAURI_INTERNALS__.invoke("plugin:event|emit", {{
                        event: {ev},
                        payload: {{ status: "{success}", payload: res }}
                    }});
                }})
                .catch((err) => {{
                    window.__TAURI_INTERNALS__.invoke("plugin:event|emit", {{
                        event: {ev},
                        payload: {{ status: "{error}", payload: err }}
                    }});
                }});
            "#,
            cmd = cmd_json,
            args = args_json,
            opts = opts_json,
            ev = event_json,
            success = RpcStatus::Success.as_str(),
            error = RpcStatus::Error.as_str(),
        );
        window.eval(js)?;
        Ok(())
    }

    /// Emit a message to the target window over WebSocket.
    ///
    /// This method serializes the event and payload and sends it to the primary window session if available.
    pub async fn emit<P: Serialize + Clone>(&self, event: &str, payload: P) -> Result<(), Error> {
        let label = self.rpc_server.primary_window_label();
        if let Some(session) = self.rpc_server.get_ws_handle(label) {
            let json = json!({
                "event":event,
                "payload":payload
            })
            .to_string();
            session
                .lock()
                .await
                .send(Message::text(json))
                .await
                .map_err(|err| {
                    Error::PluginInitialization(
                        "tauri-remote-ui".to_owned(),
                        format!("Failed to send WS message. Err: {err}"),
                    )
                })?;
        }
        Ok(())
    }
}
