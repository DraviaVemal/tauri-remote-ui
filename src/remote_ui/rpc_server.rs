
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

use crate::{models::*, remote_ui::net, RemoteUi};
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
use std::{
    collections::HashMap,
    env,
    future::Future,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tauri::{async_runtime::JoinHandle, AppHandle, Error, Manager, Url, WebviewWindow};
use tokio::{
    net::TcpListener,
    sync::{Mutex, RwLock},
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

    /// The port the remote UI server is currently bound to, or `None` if the
    /// server is not running. Useful when the configuration requested a random
    /// port (`port = None`) and the caller needs to surface the chosen port.
    fn remote_ui_port(&self) -> impl Future<Output = Option<u16>>;
}

/// Implementation of `RemoteUiExt` for Tauri's `AppHandle`.
impl RemoteUiExt for AppHandle {
    /// Start the remote UI server.
    async fn start_remote_ui(&self, remote_ui_config: RemoteUiConfig) -> Result<(), Error> {
        let state = self.state::<Arc<RwLock<RemoteUi>>>();
        let mut guard = state.write().await;
        guard
            .rpc_server
            .start(remote_ui_config)
            .await
            .map_err(Into::into)
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
        remote_ui.rpc_server.is_active()
    }

    async fn remote_ui_port(&self) -> Option<u16> {
        let state = self.state::<Arc<RwLock<RemoteUi>>>();
        let remote_ui = state.read().await;
        remote_ui.rpc_server.bound_port()
    }
}


/// Type alias for window label strings.
type WindowLabel = String;

const VERSION_PREFIX: &str = "version:";

/// WebSocket sink handle for a single connection (sending side).
pub(crate) type WsSink =
    Arc<Mutex<SplitSink<WebSocketStream<TokioIo<Upgraded>>, Message>>>;

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
    ws_window_handle: HashMap<WindowLabel, WsSink>,
    /// Handle to the HTTP server task for aborting on stop.
    http_server_thread: Option<JoinHandle<()>>,
    /// Port the listener was bound to (resolved after start, useful when the
    /// configured port was 0 / `None`).
    bound_port: Option<u16>,
}


impl RpcServer {
    /// Returns whether the server is currently active.
    pub(crate) fn is_active(&self) -> bool {
        self.is_active
    }

    /// The port the listener is bound to, if the server is currently active.
    pub(crate) fn bound_port(&self) -> Option<u16> {
        self.bound_port
    }

    /// The label of the Tauri webview window the remote UI is bound to.
    pub(crate) fn primary_window_label(&self) -> &str {
        self.remote_ui_config.primary_window_label()
    }

    /// Create a new `RpcServer` instance for the given app handle.
    pub(crate) fn new(app: Arc<AppHandle>) -> Self {
        Self {
            app,
            is_active: false,
            remote_ui_config: RemoteUiConfig::default(),
            ws_window_handle: HashMap::new(),
            http_server_thread: None,
            bound_port: None,
        }
    }

    /// Start the remote UI server with the provided configuration.
    /// Returns an error if the server is already running.
    pub(crate) async fn start(
        &mut self,
        remote_ui_config: RemoteUiConfig,
    ) -> crate::Result<()> {
        if self.is_active {
            return Err(crate::Error::ServerAlreadyRunning);
        }
        self.remote_ui_config = remote_ui_config;
        self.spawn_http_server().await
    }

    /// Stop the remote UI server and abort the HTTP server task.
    pub(crate) fn stop(&mut self) {
        if !self.is_active {
            return;
        }
        self.is_active = false;
        self.bound_port = None;
        let label = self.remote_ui_config.primary_window_label().to_owned();
        if let Some(window) = self.app.get_webview_window(&label) {
            if let Err(err) = window.reload() {
                log::error!("Failed to reload webview window '{label}': {err}");
            }
        }
        if let Some(server_handle) = self.http_server_thread.take() {
            server_handle.abort();
        }
        self.ws_window_handle.clear();
    }

    /// Bind the TCP listener and spawn the connection-accept loop. Records the
    /// bound port on `self` so callers can read it via [`Self::bound_port`].
    pub(crate) async fn spawn_http_server(&mut self) -> crate::Result<()> {
        let origin: &str = self.remote_ui_config.allowed_origin().bind_address();
        let dist_path = if let Some(frontend_path) =
            self.app.config().build.frontend_dist.as_ref()
        {
            if Url::parse(&frontend_path.to_string()).is_ok() {
                return Err(crate::Error::InvalidFrontendDist);
            }
            frontend_path.to_string()
        } else {
            "../dist".to_owned()
        };
        let static_path = self
            .remote_ui_config
            .bundle_path()
            .map(str::to_owned)
            .unwrap_or(dist_path);
        self.remote_ui_config.bundle_path = Some(static_path);

        let port = self.remote_ui_config.port().unwrap_or(0);
        let listener = TcpListener::bind((origin, port)).await?;
        let actual_port = listener.local_addr()?.port();
        self.bound_port = Some(actual_port);
        self.remote_ui_config.port = Some(actual_port);
        log::info!("Tauri Remote UI listening on {origin}:{actual_port}");

        let scope = self.remote_ui_config.allowed_origin();
        match scope {
            OriginType::Localhost => {
                log::info!("Origin scope: Localhost — peer filter: loopback only");
            }
            OriginType::Any => {
                log::warn!(
                    "Origin scope: Any — peer filter DISABLED, any host that can route to this machine can connect"
                );
            }
            OriginType::Subnet => {
                let subnets = net::trusted_subnet_descriptions();
                if subnets.is_empty() {
                    log::warn!(
                        "Origin scope: Subnet — but no bounded local subnets were detected; only loopback will be accepted"
                    );
                } else {
                    log::info!(
                        "Origin scope: Subnet — trusted networks (peers outside these will get 403): {}",
                        subnets.join(", ")
                    );
                }
            }
        }

        let app_handle = self.app.clone();
        self.is_active = true;
        let handle = tauri::async_runtime::spawn(async move {
            if let Err(err) = run_hyper_server(listener, app_handle).await {
                log::error!("Hyper server for Remote UI exited with error: {err}");
            }
        });
        self.http_server_thread = Some(handle);

        let window_label = self.remote_ui_config.primary_window_label().to_owned();
        let window = self.app.get_webview_window(&window_label).ok_or_else(|| {
            crate::Error::PrimaryWindowNotFound(window_label.clone())
        })?;
        if self.remote_ui_config.minimize_app {
            window.minimize().map_err(crate::Error::Tauri)?;
        }
        if !self.remote_ui_config.application_ui {
            let origin = self.remote_ui_config.allowed_origin();
            let urls = build_reachable_urls(origin, actual_port);
            log::info!(
                "Tauri Remote UI reachable at: {}",
                urls.join(", ")
            );
            let primary_url = urls
                .first()
                .cloned()
                .unwrap_or_else(|| format!("http://127.0.0.1:{actual_port}"));
            self.activate_remote_ui_mode(
                &window,
                &primary_url,
                &urls,
                &self.remote_ui_config.custom_blocking_ui,
            )
            .map_err(crate::Error::Tauri)?;
        }
        Ok(())
    }

    /// Activate the remote UI mode in the given window, replacing its DOM with
    /// custom or default HTML. `urls` is the full list of addresses the server
    /// is reachable on; `primary_url` is the canonical one used for the info
    /// page link and for the JS `console.info` notice.
    pub(crate) fn activate_remote_ui_mode(
        &self,
        window: &WebviewWindow,
        primary_url: &str,
        urls: &[String],
        custom_html: &Option<String>,
    ) -> Result<(), Error> {
        let urls_list = render_urls_list(urls);
        let urls_csv = urls.join(", ");
        let info_url = format!("{primary_url}/remote_ui_info");
        let html = if let Some(custom_html) = custom_html {
            custom_html
                .replace("%URLS%", &urls_csv)
                .replace("%URLS_LIST%", &urls_list)
                .replace("%URL_INFO%", &info_url)
        } else {
            include_str!("default.html")
                .replace("%URLS%", &urls_csv)
                .replace("%URLS_LIST%", &urls_list)
                .replace("%URL_INFO%", &info_url)
        };
        // Replace DOM content with HTML string.
        window.eval(format!(
            r#"(function() {{
            document.body.innerHTML = `{html}`;
            document.body.style.margin = '0';
            document.body.style.padding = '0';
            document.documentElement.style.height = '100%';
            document.body.style.height = '100%';
            console.info("Tauri-Remote-UI : Remote UI Plugin Activated");
            console.info("Tauri-Remote-UI : Reachable at", {urls});
        }})();"#,
            html = html,
            urls = serde_json::to_string(urls).unwrap_or_else(|_| "[]".to_owned()),
        ))
    }

    /// Set the WebSocket handle for a given window label.
    pub(crate) fn set_ws_handle(&mut self, window_label: &str, ws_handle: WsSink) {
        self.ws_window_handle
            .insert(window_label.to_owned(), ws_handle);
    }

    /// Get the WebSocket handle for a given window label, if present.
    pub(crate) fn get_ws_handle(&self, window_label: &str) -> Option<&WsSink> {
        self.ws_window_handle.get(window_label)
    }
}


/// Run the Hyper accept loop for the already-bound listener. The loop exits
/// when the server is marked inactive (the task is also `.abort()`ed by
/// [`RpcServer::stop`], whichever happens first).
async fn run_hyper_server(listener: TcpListener, app_handle: Arc<AppHandle>) -> std::io::Result<()> {
    loop {
        {
            let remote_ui = app_handle.state::<Arc<RwLock<RemoteUi>>>();
            if !remote_ui.read().await.rpc_server.is_active() {
                break;
            }
        }
        let (stream, peer_addr) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let req_app_handle = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(
                    io,
                    service_fn(move |req| {
                        handle_request(req, req_app_handle.clone(), peer_addr)
                    }),
                )
                .with_upgrades()
                .await
            {
                log::warn!("Error serving Remote UI connection: {err:?}");
            }
        });
    }
    Ok(())
}

/// Build the list of `http://<ip>:<port>` URLs the server is reachable on.
fn build_reachable_urls(origin: OriginType, port: u16) -> Vec<String> {
    let mut urls: Vec<String> = net::reachable_addresses(origin)
        .into_iter()
        .map(|ip| format_url(ip, port))
        .collect();
    urls.dedup();
    urls
}

fn format_url(ip: IpAddr, port: u16) -> String {
    match ip {
        IpAddr::V4(_) => format!("http://{ip}:{port}"),
        IpAddr::V6(_) => format!("http://[{ip}]:{port}"),
    }
}

/// Render a list of URLs as `<li><a href="...">...</a></li>` items.
fn render_urls_list(urls: &[String]) -> String {
    urls.iter()
        .map(|u| {
            let escaped = html_escape(u);
            format!("<li><a href=\"{escaped}\" target=\"_blank\">{escaped}</a></li>")
        })
        .collect::<Vec<_>>()
        .join("")
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}


/// Handle incoming HTTP requests for the remote UI server.
/// Routes requests to keep-alive, info, WebSocket, disconnect, or asset serving endpoints.
async fn handle_request(
    request: Request<Incoming>,
    app_handle: Arc<AppHandle>,
    peer_addr: SocketAddr,
) -> Result<Response<Full<Bytes>>, Error> {
    // Origin scope filter: reject early if the peer is outside the allowed scope.
    {
        let remote_ui = app_handle.state::<Arc<RwLock<RemoteUi>>>();
        let origin = remote_ui
            .read()
            .await
            .rpc_server
            .remote_ui_config
            .allowed_origin();
        if !net::peer_allowed(origin, peer_addr.ip()) {
            log::warn!(
                "Remote UI: rejected {peer_addr} with 403 (scope: {origin:?})"
            );
            return Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Full::new(Bytes::from("Forbidden")))
                .map_err(|err| {
                    Error::AssetNotFound(format!("Failed to build forbidden response: {err}"))
                });
        }
    }
    let path = request.uri().path().to_string();
    match (request.method().as_str(), path.as_str()) {
        ("GET", "/keep_alive") => {
            // Respond to keep-alive checks
            let remote_ui = app_handle.state::<Arc<RwLock<RemoteUi>>>();
            if remote_ui.read().await.rpc_server.is_active() {
                let response = Response::builder()
                    .header("Content-Type", "text/plain; charset=UTF-8".to_owned())
                    .body(Full::new(Bytes::from("alive")))
                    .map_err(|err| {
                        Error::AssetNotFound(format!("Failed to respond to keep alive. Err:{err}"))
                    })?;
                Ok(response)
            } else {
                not_found()
                    .map_err(|err| Error::AssetNotFound(format!("Keep alive failed. {err:?}")))
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
                    .map_err(|err| Error::AssetNotFound(format!("File serving failed. {err:?}")))
            } else {
                let app = app_handle.state::<Arc<RwLock<RemoteUi>>>();
                let remote_ui_config = app.read().await.rpc_server.remote_ui_config.clone();
                let info_html = include_str!("information.html")
                    .replace(
                        "%ORIGIN_SCOPE%",
                        remote_ui_config.allowed_origin().bind_address(),
                    )
                    .replace(
                        "%PORT%",
                        &remote_ui_config.port().unwrap_or_default().to_string(),
                    )
                    .replace("%PLUGIN_VERSION%", env!("CARGO_PKG_VERSION"))
                    .replace(
                        "%APP_VERSION%",
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
                                log::warn!("WebSocket session error: {e:?}");
                            }
                        });
                        Ok(response)
                    }
                    Err(e) => {
                        log::warn!("WebSocket upgrade error: {e}");
                        let response = Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body(Full::new(Bytes::from("WebSocket upgrade failed")))
                            .map_err(|err| {
                                Error::AssetNotFound(format!(
                                    "Failed to build WS upgrade failure response: {err}"
                                ))
                            })?;
                        Ok(response)
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
            .map_err(|err| Error::AssetNotFound(format!("File serving failed. {err:?}"))),

        _ => not_found()
            .map_err(|err| Error::AssetNotFound(format!("File serving failed. {err:?}"))),
    }
}


/// Handle a WebSocket connection for remote UI RPC.
/// Manages ping/pong, message routing, and connection lifecycle.
async fn ws_handle(websocket: HyperWebsocket, app_handle: Arc<AppHandle>) -> Result<(), Error> {
    match websocket.await {
        Ok(ws_stream) => {
            let (tx, mut rx) = ws_stream.split();
            let ws_sender = Arc::new(Mutex::new(tx));
            let primary_label;
            // Replace any existing handle for the primary window with this new one.
            {
                let remote_ui = app_handle.state::<Arc<RwLock<RemoteUi>>>();
                let mut remote_ui_mut = remote_ui.write().await;
                primary_label = remote_ui_mut
                    .rpc_server
                    .primary_window_label()
                    .to_owned();
                if let Some(existing_handle) =
                    remote_ui_mut.rpc_server.get_ws_handle(&primary_label)
                {
                    if let Err(err) = existing_handle.lock().await.close().await {
                        log::warn!("Failed to close existing socket connection: {err}");
                    }
                }
                remote_ui_mut
                    .rpc_server
                    .set_ws_handle(&primary_label, ws_sender.clone());
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
                                    log::warn!("Failed to send pong: {err}");
                                }
                            } else if let Some(client_version) = msg.strip_prefix(VERSION_PREFIX) {
                                let server_version = env!("CARGO_PKG_VERSION");
                                if client_version != server_version {
                                    log::warn!(
                                        "Tauri Remote UI version mismatch — frontend npm package is '{client_version}', host crate is '{server_version}'. Behavior is undefined; align both to the same release."
                                    );
                                }
                                let reply = format!("{VERSION_PREFIX}{server_version}");
                                if let Err(err) = ws_sender
                                    .lock()
                                    .await
                                    .send(Message::Text(reply.into()))
                                    .await
                                {
                                    log::warn!("Failed to send version reply: {err}");
                                }
                            } else {
                                let remote_ui = app_handle.state::<Arc<RwLock<RemoteUi>>>();
                                let remote_ui_mut = remote_ui.read().await;
                                remote_ui_mut.invoke_rpc(msg.as_ref(), ws_sender.clone())?;
                            }
                        }
                        Message::Close(_) => {
                            log::debug!("Remote UI socket closed by peer");
                        }
                        _ => {
                            log::trace!("Unhandled WS data frame");
                        }
                    },
                    Err(err) => {
                        log::warn!("Message read failed: {err}");
                    }
                }
            }
            Ok(())
        }
        Err(err) => {
            log::warn!("Socket stream upgrade failed: {err:?}");
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
            let file_path = urlencoding::decode(&format!("{static_path}/{file_path}"))
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
    #[cfg(not(debug_assertions))] // Release mode: serve from the embedded asset resolver.
    {
        let content_type = mime_guess::from_path(&file_path).first_or_octet_stream();
        if let Some(asset) = app_handle.asset_resolver().get(file_path) {
            return Response::builder()
                .header("Content-Type", content_type.to_string())
                .body(Full::new(Bytes::from(asset.bytes)));
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
