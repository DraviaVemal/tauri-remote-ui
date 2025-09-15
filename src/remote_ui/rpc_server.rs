// MIT License
// Copyright (c) 2025 DraviaVemal
// See LICENSE file in the root directory.

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
use tauri::{AppHandle, Error, Manager, Url, WebviewWindow};
use tokio::{
    net::TcpListener,
    sync::{Mutex, RwLock},
};

pub trait RemoteUiExt {
    fn start_remote_ui(
        &self,
        remote_ui_config: RemoteUiConfig,
    ) -> impl Future<Output = Result<(), tauri::Error>>;
    fn stop_remote_ui(&self) -> impl Future<Output = Result<(), tauri::Error>>;
    fn is_remote_ui_running(&self) -> impl Future<Output = bool>;
}

impl RemoteUiExt for AppHandle {
    async fn start_remote_ui(&self, remote_ui_config: RemoteUiConfig) -> Result<(), Error> {
        let remote_ui = self.state::<Arc<RwLock<RemoteUi>>>();
        remote_ui.write().await.rpc_server.start(remote_ui_config)?;
        Ok(())
    }

    async fn stop_remote_ui(&self) -> Result<(), Error> {
        let remote_ui = self.state::<Arc<RwLock<RemoteUi>>>();
        remote_ui.write().await.rpc_server.stop();
        Ok(())
    }

    async fn is_remote_ui_running(&self) -> bool {
        let state = self.state::<Arc<RwLock<RemoteUi>>>();
        let remote_ui = state.read().await;
        remote_ui.rpc_server.get_is_active()
    }
}

type WindowLabel = String;
#[derive(Debug, Clone)]
pub struct RpcServer {
    pub(crate) app: Arc<AppHandle>,
    is_active: bool,
    remote_ui_config: RemoteUiConfig,
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
            ws_window_handle: HashMap::new(),
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
            if let Some(window) = self.app.get_webview_window("main") {
                if let Err(err) = window.reload() {
                    eprintln!("Failed to reload webview window. Err:{err}");
                }
            }
        }
    }

    /// Spawns the Actix HTTP server inside tokio task of tauri
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
        tauri::async_runtime::spawn(async move {
            if let Err(err) = create_hyper_server(origin, port, app_handle).await {
                eprintln!("Failed to create hyper Server for Remote UI plugin. Err:{err}");
            }
        });
        let window = self.app.get_webview_window("main").unwrap();
        if self.remote_ui_config.minimize_app {
            window.minimize()?;
        }
        if !self.remote_ui_config.application_ui {
            let current_url = window.url().unwrap();
            let parsed = Url::parse(current_url.as_str()).unwrap();
            let host = parsed.domain().unwrap();
            let scheme = if parsed.scheme() == "https" {
                "https"
            } else {
                "http"
            };
            let new_url = format!("{}://{}:{}", scheme, host, port);
            self.activate_remote_ui_mode(
                &window,
                &new_url,
                &self.remote_ui_config.custom_blocking_ui,
            )?;
        }
        Ok(())
    }

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
            
            console.info("Remote UI Plugin Activated");
            console.info("Remote UI active at: {}")
        }})();"#,
            html, url
        ))
    }

    pub(crate) fn set_ws_handle(
        &mut self,
        window_label: &str,
        ws_handle: Arc<Mutex<SplitSink<WebSocketStream<TokioIo<Upgraded>>, Message>>>,
    ) -> () {
        self.ws_window_handle
            .insert(window_label.to_owned(), ws_handle);
    }

    pub(crate) fn get_ws_handle(
        &self,
        window_label: &str,
    ) -> Option<&Arc<Mutex<SplitSink<WebSocketStream<TokioIo<Upgraded>>, Message>>>> {
        self.ws_window_handle.get(window_label)
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
                .with_upgrades()
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
        ("GET", "/remote_ui_info") => {
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
        ("GET", "/remote_ui_disconnect") => {
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
        ("GET", "/remote_ui_ws") => {
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
                Err(Error::FailedToReceiveMessage)
            }
        }
        ("GET", path) => wildcard_get_handler(path, app_handle)
            .await
            .map_err(|err| Error::AssetNotFound(format!("File serving failed. {:?}", err))),

        _ => not_found()
            .map_err(|err| Error::AssetNotFound(format!("File serving failed. {:?}", err))),
    }
}

/// Handle a websocket connection.
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
                // Replace overwrite existing handle to maintain reliability on one window like desktop
                remote_ui_mut
                    .rpc_server
                    .set_ws_handle("main", ws_sender.clone());
            }
            while let Some(message_stream) = rx.next().await {
                match message_stream {
                    Ok(message) => match message {
                        Message::Text(msg) => {
                            let remote_ui = app_handle.state::<Arc<RwLock<RemoteUi>>>();
                            let remote_ui_mut = remote_ui.read().await;
                            remote_ui_mut.invoke_rpc(msg.to_string(), ws_sender.clone())?;
                        }
                        Message::Close(_) => {}
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

fn not_found() -> Result<Response<Full<Bytes>>, tauri::http::Error> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Full::new(Bytes::from("Not Found!")))
}
