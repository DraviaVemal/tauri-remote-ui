// MIT License
// Copyright (c) 2025 DraviaVemal
// See LICENSE file in the root directory.

use crate::{models::*, RemoteUi};
use futures_util::StreamExt;
use http_body_util::Full;
use hyper::{
    body::{Bytes, Incoming},
    server::conn::http1,
    service::service_fn,
    Request, Response, StatusCode,
};
use hyper_tungstenite::{tungstenite::Message, HyperWebsocket};
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use std::{env, sync::Arc};
use tauri::{async_runtime::block_on, AppHandle, Error, Manager, Url, WebviewWindow};
use tokio::{
    net::TcpListener,
    sync::{Mutex, RwLock},
};

pub trait RemoteUiExt {
    fn start_remote_ui(&self, remote_ui_config: RemoteUiConfig) -> Result<(), Error>;
    fn stop_remote_ui(&self) -> Result<(), Error>;
}

impl RemoteUiExt for AppHandle {
    fn start_remote_ui(&self, remote_ui_config: RemoteUiConfig) -> Result<(), Error> {
        let remote_ui = self.state::<Arc<RwLock<RemoteUi>>>();
        println!("Server Start");
        block_on(async { remote_ui.write().await.rpc_server.start(remote_ui_config) })
    }

    fn stop_remote_ui(&self) -> Result<(), Error> {
        let remote_ui = self.state::<Arc<RwLock<RemoteUi>>>();
        block_on(async {
            remote_ui.write().await.rpc_server.stop();
        });
        println!("Server Stop");
        Ok(())
    }
}

async fn create_hyper_server(
    origin: &str,
    port: u16,
    app_handle: Arc<AppHandle>,
) -> Result<(), Error> {
    let listener = TcpListener::bind((origin, port)).await?;
    loop {
        let remote_ui = app_handle.state::<Arc<RwLock<RemoteUi>>>();
        if !remote_ui.read().await.rpc_server.get_is_active() {
            break;
        }
        let (stream, _) = listener.accept().await?;

        let io = TokioIo::new(stream);
        let req_app_handle = app_handle.clone();

        tauri::async_runtime::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(
                    io,
                    service_fn(move |req| handle_request(req, req_app_handle.clone())),
                )
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
    Ok(())
}

async fn handle_request(
    request: Request<Incoming>,
    app_handle: Arc<AppHandle>,
) -> Result<Response<Full<Bytes>>, Error> {
    let path = request.uri().path().to_string();

    match (request.method().as_str(), path.as_str()) {
        ("GET", "/remote_ui") => {
            let app = app_handle.state::<Arc<RwLock<RemoteUi>>>();
            let remote_ui_config = app.read().await.rpc_server.remote_ui_config.clone();
            let mut value = serde_json::to_value(&remote_ui_config).unwrap();
            if let Value::Object(ref mut map) = value {
                map.insert("Plugin".to_string(), json!("tauri-remote-ui"));
                map.insert(
                    "Status".to_string(),
                    json!("This Window is Dis-Connected as another one opened"),
                );
                // Get Tauri app version
                let app_version = app_handle.package_info().version.to_string();
                map.insert("app_version".to_string(), json!(app_version));
                // Get plugin version from Cargo.toml env var if set at build time
                map.insert(
                    "plugin_version".to_string(),
                    json!(env!("CARGO_PKG_VERSION")),
                );
            }
            let resp = serde_json::to_string(&value)?;
            Ok(Response::new(Full::new(Bytes::from(resp))))
        }

        ("GET", "/remote_ui_ws") => {
            if hyper_tungstenite::is_upgrade_request(&request) {
                match hyper_tungstenite::upgrade(request, None) {
                    Ok((response, websocket)) => {
                        let state = Arc::clone(&app_handle);
                        tokio::spawn(async move {
                            if let Err(e) = serve_websocket(websocket, state).await {
                                println!("WebSocket error: {:?}", e);
                            }
                        });
                        Ok(response)
                    }
                    Err(e) => {
                        println!("WebSocket upgrade error: {}", e);
                        Ok(Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body(Full::new(Bytes::from("WebSocket upgrade failed")))
                            .unwrap())
                    }
                }
            } else {
                println!("Not WS Upgrade Request");
                Ok(Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Full::new(Bytes::from("Expected WebSocket request")))
                    .unwrap())
            }
        }

        ("GET", path) => wildcard_get_handler(path, app_handle)
            .await
            .map_err(|err| Error::AssetNotFound(format!("File serving failed. {:?}", err))),

        _ => not_found()
            .map_err(|err| Error::AssetNotFound(format!("File serving failed. {:?}", err))),
    }
}

fn not_found() -> Result<Response<Full<Bytes>>, tauri::http::Error> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Full::new(Bytes::from("Not Found!")))
}

/// Handle a websocket connection.
async fn serve_websocket(
    websocket: HyperWebsocket,
    app_handle: Arc<AppHandle>,
) -> Result<(), Error> {
    match websocket.await {
        Ok(ws_stream) => {
            let (tx, mut rx) = ws_stream.split();
            let ws_sender = Arc::new(Mutex::new(tx));
            while let Some(message) = rx.next().await {
                match message.unwrap() {
                    Message::Text(msg) => {
                        let remote_ui = app_handle.state::<Arc<RwLock<RemoteUi>>>();
                        let socket_handle = ws_sender.clone();
                        let _ = remote_ui
                            .read()
                            .await
                            .invoke_rpc(msg.to_string(), socket_handle)
                            .await;
                    }
                    _ => {
                        println!("Unhandled ws data!")
                    }
                }
            }
            Ok(())
        }
        Err(err) => {
            println!("Socket stream upgrade failed {:?}", err);
            Err(Error::FailedToReceiveMessage)
        }
    }
}

/// Handler for all wildcard GET routes: serve file from disk, then embedded, else 404
async fn wildcard_get_handler(
    path: &str,
    app_handle: Arc<AppHandle>,
) -> Result<Response<Full<Bytes>>, tauri::http::Error> {
    // If the path ends with a slash or has no file extension, serve index.html
    let mut file_path = path.trim_start_matches('/').to_string();
    file_path = if file_path.ends_with('/') || !file_path.contains('.') {
        format!("{}/index.html", &file_path.trim_end_matches('/'))
    } else {
        file_path
    };
    #[cfg(debug_assertions)]
    {
        let remote_state = app_handle.state::<Arc<RwLock<RemoteUi>>>();
        let remote_ui = remote_state.read().await;
        if let Some(static_path) = remote_ui.rpc_server.remote_ui_config.bundle_path.as_ref() {
            let file_path = format!("{}/{}", static_path, file_path);
            if let Ok(bytes) = std::fs::read(&file_path) {
                let content_type = mime_guess::from_path(&file_path).first_or_octet_stream();
                return Response::builder()
                    .header("Content-Type", content_type.to_string())
                    .body(Full::new(Bytes::from(bytes)));
            }
        }
    }
    #[cfg(not(debug_assertions))] // Release Mode Serve from handle assert
    {
        let content_type = mime_guess::from_path(&file_path).first_or_octet_stream();
        if let Some(assert) = app_handle.asset_resolver().get(file_path) {
            return Response::builder()
                .header("Content-Type", content_type.to_string())
                .body(Full::new(Bytes::from(assert.bytes)));
        }
    }
    not_found()
}

#[derive(Debug, Clone)]
pub struct RpcServer {
    pub(crate) app: Arc<AppHandle>,
    is_active: bool,
    remote_ui_config: RemoteUiConfig,
}

impl RpcServer {
    pub(crate) fn get_is_active(&self) -> bool {
        self.is_active
    }

    pub(crate) fn new(app: Arc<AppHandle>) -> Self {
        Self {
            app,
            is_active: false,
            remote_ui_config: RemoteUiConfig::default(),
        }
    }

    pub(crate) fn start(&mut self, remote_ui_config: RemoteUiConfig) -> Result<(), Error> {
        if self.is_active {
            Err(Error::IllegalEventName("Server Already Running".to_owned()))
        } else {
            self.remote_ui_config = remote_ui_config.clone();
            self.spawn_http_server()
        }
    }

    pub(crate) fn stop(&mut self) {
        if self.is_active {
            self.is_active = false;
            let window = self.app.get_webview_window("main").unwrap();
            window.reload().unwrap();
        }
    }

    /// Spawns the Actix HTTP server inside tokio task of tauri
    fn spawn_http_server(&mut self) -> Result<(), Error> {
        let origin: &str = self.remote_ui_config.get_allowed_origin().into();
        let dist_path = if let Some(frontend_path) = self.app.config().build.frontend_dist.as_ref()
        {
            if Url::parse(&frontend_path.to_string()).is_ok() {
                return Err(Error::UnknownPath);
            } else {
                frontend_path.to_string()
            }
        } else {
            "../dist".to_owned()
        };
        let static_path = self.remote_ui_config.get_bundle_path().unwrap_or(dist_path);
        self.remote_ui_config.bundle_path = Some(static_path.clone());
        let app_handle = self.app.clone();
        let port = self.remote_ui_config.get_port().unwrap_or_default();
        self.is_active = true;
        tauri::async_runtime::spawn(async move {
            create_hyper_server(origin, port, app_handle).await.unwrap();
        });
        let window = self.app.get_webview_window("main").unwrap();
        let current_url = window.url().unwrap();
        let parsed = Url::parse(current_url.as_str()).unwrap();
        let host = parsed.domain().unwrap();
        let scheme = parsed.scheme();
        let new_url = format!("{}://{}:{}", scheme, host, port);
        self.activate_remote_ui_mode(&window, &new_url, &self.remote_ui_config.custom_blocking_ui)
            .unwrap();
        Ok(())
    }

    pub fn activate_remote_ui_mode(
        &self,
        window: &WebviewWindow,
        url: &str,
        custom_html: &Option<String>,
    ) -> Result<(), Error> {
        let html = if let Some(custom_html) = custom_html {
            custom_html
        } else {
            &include_str!("default.html")
                .replace("%URL%", url)
                .replace("%URL_INFO%", &format!("{}/remote_ui", url))
        };
        // Save current URL and replace DOM content with HTML string
        window.eval(&format!(
            r#"(function() {{
            // Replace entire body content with our HTML
            document.body.innerHTML = `{}`;
            
            // Apply styles to html/body to ensure full coverage
            document.body.style.margin = '0';
            document.body.style.padding = '0';
            document.documentElement.style.height = '100%';
            document.body.style.height = '100%';
            
            console.info("Remote UI Plugin Activated");
            console.info("Remote UI active at: {}")
        }})();"#,
            html, url
        ))
    }
}
