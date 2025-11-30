
//! Remote UI RPC Server implementation for Tauri applications.
//!
//! This module provides the `RpcServer` struct and related traits for managing a remote UI server,
//! including HTTP and WebSocket handling, lifecycle management, and integration with Tauri's state system.
//!
//! # Features
//! - Start/stop remote UI server
//! - Serve static and embedded assets
//! - WebSocket communication for RPC
//! - Customizable UI activation and disconnect flows
//!
//! # License
//! AGPL-3.0-only License
//! Copyright (c) 2025 DraviaVemal
//! See LICENSE file in the root directory.

use crate::{models::*, RemoteUi};
use futures::{stream::SplitSink, SinkExt, StreamExt};
use http_body_util::Full;
use hyper::{
    body::{Bytes, Incoming},
    server::conn::http1,
    service::service_fn,
    upgrade::Upgraded,
    Request, Response, StatusCode,
};
use hyper_tungstenite::{tungstenite::Message, HyperWebsocket, WebSocketStream};
use hyper_util::rt::TokioIo;
use std::{collections::HashMap, env, future::Future, sync::Arc};
use tauri::{async_runtime::JoinHandle, AppHandle, Error, Manager, Url, WebviewWindow};
use tokio::{
    net::TcpListener,
    sync::{oneshot, Mutex, RwLock},
};


/// Extension trait for Tauri's `AppHandle` to manage the Remote UI server lifecycle.
pub trait RemoteUiExt {
    /// Start the remote UI server with the given configuration.
    fn start_remote_ui(
        &self,
        remote_ui_config: RemoteUiConfig,
    ) -> impl Future<Output = Result<(), tauri::Error>>;

    /// Stop the remote UI server if running.
    fn stop_remote_ui(&self) -> impl Future<Output = Result<(), tauri::Error>>;

    /// Check if the remote UI server is currently active.
    fn is_remote_ui_running(&self) -> impl Future<Output = bool>;
}

/// Implementation of `RemoteUiExt` for Tauri's `AppHandle`.
impl RemoteUiExt for AppHandle {
    /// Start the remote UI server.
    async fn start_remote_ui(&self, remote_ui_config: RemoteUiConfig) -> Result<(), Error> {
        let remote_ui = self.state::<Arc<RwLock<RemoteUi>>>();
        remote_ui.write().await.rpc_server.start(remote_ui_config)?;
        Ok(())
    }

    /// Stop the remote UI server.
    async fn stop_remote_ui(&self) -> Result<(), Error> {
        let remote_ui = self.state::<Arc<RwLock<RemoteUi>>>();
        remote_ui.write().await.rpc_server.stop();
        Ok(())
    }

    /// Check if the remote UI server is running.
    async fn is_remote_ui_running(&self) -> bool {
        let state = self.state::<Arc<RwLock<RemoteUi>>>();
        let remote_ui = state.read().await;
        remote_ui.rpc_server.get_is_active()
    }
}


/// Type alias for window label strings.
type WindowLabel = String;

/// The main Remote UI RPC server struct.
///
/// Manages HTTP/WebSocket server lifecycle, window handles, and configuration.
#[derive(Debug)]
pub struct RpcServer {
    /// Reference to the Tauri application handle.
    pub(crate) app: Arc<AppHandle>,
    /// Indicates if the server is currently active.
    is_active: bool,
    /// Configuration for the remote UI server.
    remote_ui_config: RemoteUiConfig,
    /// Map of window labels to WebSocket handles.
    ws_window_handle: HashMap<
        WindowLabel,
        Arc<
            Mutex<
                futures::stream::SplitSink<
                    hyper_tungstenite::WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>,
                    Message,
                >,
            >,
        >,
    >,
    /// Handle to the HTTP server thread for aborting.
    http_server_thread: Option<JoinHandle<()>>,
}


impl RpcServer {
    /// Returns whether the server is currently active.
    pub(crate) fn get_is_active(&self) -> bool {
        self.is_active
    }

    /// Create a new `RpcServer` instance for the given app handle.
    pub(crate) fn new(app: Arc<AppHandle>) -> Self {
        Self {
            app,
            is_active: false,
            remote_ui_config: RemoteUiConfig::default(),
            ws_window_handle: HashMap::new(),
            http_server_thread: None,
        }
    }

    /// Start the remote UI server with the provided configuration.
    /// Returns an error if the server is already running.
    pub(crate) fn start(&mut self, remote_ui_config: RemoteUiConfig) -> Result<(), Error> {
        if self.is_active {
            Err(Error::PluginInitialization(
                "tauri-remote-ui".to_owned(),
                "Server Already Running".to_owned(),
            ))
        } else {
            self.remote_ui_config = remote_ui_config.clone();
            self.spawn_http_server()
        }
    }

    /// Stop the remote UI server and abort the HTTP server thread.
    pub(crate) fn stop(&mut self) {
        if self.is_active {
            self.is_active = false;
            if let Some(window) = self.app.get_webview_window("main") {
                if let Err(err) = window.reload() {
                    eprintln!("Failed to reload webview window. Err:{err}");
                }
            }
            if let Some(server_handle) = self.http_server_thread.as_ref() {
                server_handle.abort();
            }
        }
    }

    /// Spawn the HTTP server for remote UI inside a Tokio task.
    /// Handles asset serving, WebSocket upgrades, and UI activation.
    pub(crate) fn spawn_http_server(&mut self) -> Result<(), Error> {
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
        // Spawn the HTTP server and store the JoinHandle so we can abort it later
        // TODO Dynamic Port Map to UI
        let (tx, mut _rx) = oneshot::channel::<u16>();
        let handle = tauri::async_runtime::spawn(async move {
            if let Err(err) = create_hyper_server(origin, port, app_handle, tx).await {
                eprintln!("Failed to create hyper Server for Remote UI plugin. Err:{err}");
            }
        });
        self.http_server_thread = Some(handle);
        // TODO Update for custom name
        let window = self.app.get_webview_window("main").unwrap();
        if self.remote_ui_config.minimize_app {
            window.minimize()?;
        }
        if !self.remote_ui_config.application_ui {
            let current_url = window.url().unwrap();
            let parsed = Url::parse(current_url.as_str()).unwrap();
            let host = parsed.domain().unwrap();
            let new_url = format!("http://{}:{}", host, port);
            self.activate_remote_ui_mode(
                &window,
                &new_url,
                &self.remote_ui_config.custom_blocking_ui,
            )?;
        }
        Ok(())
    }

    /// Activate the remote UI mode in the given window, replacing its DOM with custom or default HTML.
    pub(crate) fn activate_remote_ui_mode(
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
                .replace("%URL_INFO%", &format!("{}/remote_ui_info", url))
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
            
            console.info("Tauri-Remote-UI : Remote UI Plugin Activated");
            console.info("Tauri-Remote-UI : Remote UI active at: {}")
        }})();"#,
            html, url
        ))
    }

    /// Set the WebSocket handle for a given window label.
    pub(crate) fn set_ws_handle(
        &mut self,
        window_label: &str,
        ws_handle: Arc<Mutex<SplitSink<WebSocketStream<TokioIo<Upgraded>>, Message>>>,
    ) -> () {
        self.ws_window_handle
            .insert(window_label.to_owned(), ws_handle);
    }

    /// Get the WebSocket handle for a given window label, if present.
    pub(crate) fn get_ws_handle(
        &self,
        window_label: &str,
    ) -> Option<&Arc<Mutex<SplitSink<WebSocketStream<TokioIo<Upgraded>>, Message>>>> {
        self.ws_window_handle.get(window_label)
    }
}


/// Create and run the Hyper HTTP server for remote UI.
/// Handles incoming connections, upgrades, and request routing.
async fn create_hyper_server(
    origin: &str,
    port: u16,
    app_handle: Arc<AppHandle>,
    _tx: oneshot::Sender<u16>,
) -> Result<(), Error> {
    let listener = TcpListener::bind((origin, port)).await?;
    let actual_port = listener.local_addr()?.port();
    println!("Listening on {}:{}", origin, actual_port);
    // tx.send(actual_port).map_err(|err| { ... })
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
                .with_upgrades()
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
    Ok(())
}


/// Handle incoming HTTP requests for the remote UI server.
/// Routes requests to keep-alive, info, WebSocket, disconnect, or asset serving endpoints.
async fn handle_request(
    request: Request<Incoming>,
    app_handle: Arc<AppHandle>,
) -> Result<Response<Full<Bytes>>, Error> {
    let path = request.uri().path().to_string();
    match (request.method().as_str(), path.as_str()) {
        ("GET", "/keep_alive") => {
            // Respond to keep-alive checks
            let remote_ui = app_handle.state::<Arc<RwLock<RemoteUi>>>();
            if remote_ui.read().await.rpc_server.get_is_active() {
                let response = Response::builder()
                    .header("Content-Type", "text/plain; charset=UTF-8".to_owned())
                    .body(Full::new(Bytes::from("alive")))
                    .map_err(|err| {
                        Error::AssetNotFound(format!("Failed to respond to keep alive. Err:{err}"))
                    })?;
                Ok(response)
            } else {
                not_found()
                    .map_err(|err| Error::AssetNotFound(format!("Keep alive failed. {:?}", err)))
            }
        }
        ("GET", "/remote_ui_info") => {
            // Serve remote UI info page
            let remote_ui = app_handle.state::<Arc<RwLock<RemoteUi>>>();
            if !remote_ui
                .read()
                .await
                .rpc_server
                .remote_ui_config
                .enable_info_url
            {
                not_found()
                    .map_err(|err| Error::AssetNotFound(format!("File serving failed. {:?}", err)))
            } else {
                let app = app_handle.state::<Arc<RwLock<RemoteUi>>>();
                let remote_ui_config = app.read().await.rpc_server.remote_ui_config.clone();
                let info_html = include_str!("information.html")
                    .replace(
                        "%ORIGIN_SCOPE%",
                        remote_ui_config.get_allowed_origin().into(),
                    )
                    .replace(
                        "%PORT%",
                        &remote_ui_config.get_port().unwrap_or_default().to_string(),
                    )
                    .replace("%PLUGIN_VERSION%", env!("CARGO_PKG_VERSION"))
                    .replace(
                        "%APP_VESION%",
                        &app_handle.package_info().version.to_string(),
                    );
                let response = Response::builder()
                    .header("Content-Type", "text/html; charset=UTF-8".to_owned())
                    .body(Full::new(Bytes::from(info_html)))
                    .map_err(|err| {
                        Error::AssetNotFound(format!("Failed to Load Info Page. Err:{err}"))
                    })?;
                Ok(response)
            }
        }
        ("GET", "/remote_ui_ws") => {
            // Handle WebSocket upgrade requests
            if hyper_tungstenite::is_upgrade_request(&request) {
                match hyper_tungstenite::upgrade(request, None) {
                    Ok((response, websocket)) => {
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = ws_handle(websocket, Arc::clone(&app_handle)).await {
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
                Err(Error::PluginInitialization(
                    "tauri-remote-ui".to_owned(),
                    "Failed to Upgrade WS RPC".to_owned(),
                ))
            }
        }
        ("GET", "/remote_ui_disconnect") => {
            // Serve disconnect page
            let remote_ui = app_handle.state::<Arc<RwLock<RemoteUi>>>();
            let redirect_html = if let Some(redirect_html) = remote_ui
                .read()
                .await
                .rpc_server
                .remote_ui_config
                .custom_disconnect_ui
                .as_ref()
            {
                redirect_html.to_string()
            } else {
                include_str!("redirect.html").to_string()
            };

            let response = Response::builder()
                .header("Content-Type", "text/html; charset=UTF-8".to_owned())
                .body(Full::new(Bytes::from(redirect_html)))
                .map_err(|err| {
                    Error::AssetNotFound(format!("Failed to Load Disconnect Page. Err:{err}"))
                })?;
            Ok(response)
        }
        ("GET", path) => wildcard_get_handler(path, app_handle)
            .await
            .map_err(|err| Error::AssetNotFound(format!("File serving failed. {:?}", err))),

        _ => not_found()
            .map_err(|err| Error::AssetNotFound(format!("File serving failed. {:?}", err))),
    }
}


/// Handle a WebSocket connection for remote UI RPC.
/// Manages ping/pong, message routing, and connection lifecycle.
async fn ws_handle(websocket: HyperWebsocket, app_handle: Arc<AppHandle>) -> Result<(), Error> {
    match websocket.await {
        Ok(ws_stream) => {
            let (tx, mut rx) = ws_stream.split();
            let ws_sender = Arc::new(Mutex::new(tx));
            // Internal Closer to handle RemoteUI Lock handling
            {
                let remote_ui = app_handle.state::<Arc<RwLock<RemoteUi>>>();
                let mut remote_ui_mut = remote_ui.write().await;
                if let Some(exitin_handle) = remote_ui_mut.rpc_server.get_ws_handle("main") {
                    // Close connection of existing window
                    if let Err(err) = exitin_handle.lock().await.close().await {
                        eprintln!("Failed to close Socket Connection. Err: {err}");
                    };
                }
                // Replace/overwrite existing handle to maintain reliability on one window like desktop
                remote_ui_mut
                    .rpc_server
                    .set_ws_handle("main", ws_sender.clone());
            }
            while let Some(message_stream) = rx.next().await {
                match message_stream {
                    Ok(message) => match message {
                        Message::Text(msg) => {
                            if msg == "ping" {
                                if let Err(err) = ws_sender
                                    .lock()
                                    .await
                                    .send(Message::Text("pong".into()))
                                    .await
                                {
                                    eprintln!("Failed Pong Err:{err}")
                                }
                            } else {
                                let remote_ui = app_handle.state::<Arc<RwLock<RemoteUi>>>();
                                let remote_ui_mut = remote_ui.read().await;
                                remote_ui_mut.invoke_rpc(msg.to_string(), ws_sender.clone())?;
                            }
                        }
                        Message::Close(_) => {
                            println!("Server Socket Closed")
                        }
                        _ => {
                            println!("Unhandled ws data!")
                        }
                    },
                    Err(err) => {
                        eprintln!("Message read Failed. Err:{err}")
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


/// Handler for all wildcard GET routes: serve file from disk (debug), then embedded (release), else 404.
/// Used for static asset serving in the remote UI server.
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
            let file_path = urlencoding::decode(&format!("{}/{}", static_path, file_path))
                .unwrap_or_default()
                .to_string();
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


/// Helper to return a 404 Not Found HTTP response.
fn not_found() -> Result<Response<Full<Bytes>>, tauri::http::Error> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Full::new(Bytes::from("Not Found!")))
}
