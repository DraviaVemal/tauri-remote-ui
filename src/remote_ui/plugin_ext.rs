// MIT License
// Copyright (c) 2025 DraviaVemal
// See LICENSE file in the root directory.

use crate::{RpcServer, WsPayload};
use actix_ws::Session;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use std::sync::{Arc, RwLock};
use tauri::{plugin::PluginApi, AppHandle, Error, Listener, Manager, Runtime};

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

/// Access to the remote-ui APIs.
pub struct RemoteUi {
    pub(crate) app: Arc<AppHandle>,
    pub(crate) rpc_server: RpcServer,
}

impl RemoteUi {
    pub(crate) fn is_rpc_active(&self) -> bool {
        self.rpc_server.get_is_active()
    }

    pub(crate) async fn invoke_rpc(
        &self,
        payload: String,
        mut session: Session,
    ) -> Result<(), Error> {
        let ws_payload: WsPayload = serde_json::from_str(&payload)?;
        let window = self.app.get_webview_window("main").unwrap();
        let req_unique_id = format!("remote-ui::result::{}", &ws_payload.id);
        self.app
            .app_handle()
            .once_any(&req_unique_id, move |handler| {
                // Spawn a new task to send the message asynchronously
                let payload = handler.payload().to_string();
                let id = ws_payload.id;
                tauri::async_runtime::spawn(async move {
                    let _ = session
                        .text(json!({"id":id,"payload":payload}).to_string())
                        .await;
                });
            });
        let js = format!(
            r#"
            window.__TAURI_INTERNALS__.invoke("{}",{},{})
                .then((res) => {{
                        window.__TAURI_INTERNALS__.invoke("plugin:event|emit",{{
                        event:"{}",
                        payload:{{
                            status: "success",
                            payload:res
                        }}
                    }})
                }})
                .catch((err) => {{
                    window.__TAURI_INTERNALS__.invoke("plugin:event|emit",{{
                        event:"{}",
                        payload:{{
                            status: "error",
                            payload:err
                        }}
                    }})
                }});
            "#,
            ws_payload.cmd,
            serde_json::to_string(&ws_payload.args).unwrap(),
            serde_json::to_string(&ws_payload.option).unwrap(),
            &req_unique_id,
            &req_unique_id
        );
        window.eval(js)?;
        Ok(())
    }

    /// Emit message to target window to listen
    pub fn emit<P: Serialize + Clone>(&self, event: &str, payload: P) -> Result<(), Error> {
        if let Some(session) = self.rpc_server.window_connections.get("main") {
            let mut send = session.clone();
            let json = json!({
                "event":event,
                "payload":payload
            })
            .to_string();
            tauri::async_runtime::spawn(async move { send.text(json).await.unwrap() });
        }
        Ok(())
    }
}
