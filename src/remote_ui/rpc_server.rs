// MIT License
// Copyright (c) 2025 DraviaVemal
// See LICENSE file in the root directory.

use crate::{models::*, RemoteUi};
use actix_web::{
    get, http::KeepAlive, web, App, HttpRequest, HttpResponse, HttpServer, Responder,
    Result as ActixResult,
};
use actix_ws::{Message, Session};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    env,
    net::TcpListener,
    sync::{Arc, RwLock},
    time::Duration,
};
use tauri::{async_runtime, AppHandle, Error, Manager, Url, WebviewWindow};
use tokio::sync::Notify;

pub trait RemoteUiExt {
    fn start_remote_ui(&self, remote_ui_config: RemoteUiConfig) -> Result<(String, String), Error>;
    fn stop_remote_ui(&self) -> Result<(), Error>;
}

impl RemoteUiExt for AppHandle {
    fn start_remote_ui(&self, remote_ui_config: RemoteUiConfig) -> Result<(String, String), Error> {
        let remote_ui = self.state::<Arc<RwLock<RemoteUi>>>();
        let (origin, port) = remote_ui
            .write()
            .unwrap()
            .rpc_server
            .start(remote_ui_config)?;
        Ok((origin, port))
    }

    fn stop_remote_ui(&self) -> Result<(), Error> {
        let remote_ui = self.state::<Arc<RwLock<RemoteUi>>>();
        remote_ui.write().unwrap().rpc_server.stop();
        Ok(())
    }
}

#[get("/remote_ui")]
async fn remote_ui_active(app_handle: web::Data<Arc<AppHandle>>) -> impl Responder {
    let app = app_handle.state::<Arc<RwLock<RemoteUi>>>();
    let remote_ui_config = app.read().unwrap().rpc_server.remote_ui_config.clone();
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
    HttpResponse::Ok().body(serde_json::to_string_pretty(&value).unwrap())
}

/// Handler for all wildcard GET routes: serve file from disk, then embedded, else 404
async fn wildcard_get_handler(
    req: HttpRequest,
    app_handle: web::Data<Arc<AppHandle>>,
) -> ActixResult<HttpResponse> {
    // If the path ends with a slash or has no file extension, serve index.html
    let mut file_path = req.path().trim_start_matches('/').to_string();
    file_path = if file_path.ends_with('/') || !file_path.contains('.') {
        format!("{}/index.html", &file_path.trim_end_matches('/'))
    } else {
        file_path
    };
    #[cfg(debug_assertions)]
    {
        let remote_state = app_handle.state::<Arc<RwLock<RemoteUi>>>();
        let remote_ui = remote_state.read().unwrap();
        if let Some(static_path) = remote_ui.rpc_server.remote_ui_config.bundle_path.as_ref() {
            let file_path = format!("{}/{}", static_path, file_path);
            if let Ok(bytes) = std::fs::read(&file_path) {
                let content_type = mime_guess::from_path(&file_path).first_or_octet_stream();
                return Ok(HttpResponse::Ok()
                    .content_type(content_type.to_string())
                    .body(bytes));
            }
        }
    }
    #[cfg(not(debug_assertions))] // Release Mode Serve from handle assert
    {
        let content_type = mime_guess::from_path(&file_path).first_or_octet_stream();
        if let Some(assert) = app_handle.asset_resolver().get(file_path) {
            return Ok(HttpResponse::Ok()
                .content_type(content_type.to_string())
                .body(assert.bytes));
        }
    }
    Ok(HttpResponse::NotFound().body("File not found"))
}

pub struct RpcServer {
    pub(crate) app: Arc<AppHandle>,
    is_active: bool,
    stop_signal: Arc<Notify>,
    remote_ui_config: RemoteUiConfig,
    pub(crate) window_connections: HashMap<String, Session>,
}

impl RpcServer {
    pub(crate) fn get_is_active(&self) -> bool {
        self.is_active
    }

    pub(crate) fn new(app: Arc<AppHandle>) -> Self {
        Self {
            app,
            is_active: false,
            stop_signal: Arc::new(Notify::new()),
            remote_ui_config: RemoteUiConfig::default(),
            window_connections: HashMap::new(),
        }
    }

    pub(crate) fn start(
        &mut self,
        remote_ui_config: RemoteUiConfig,
    ) -> Result<(String, String), Error> {
        if self.is_active {
            Err(Error::IllegalEventName("Server Already Running".to_owned()))
        } else {
            self.remote_ui_config = remote_ui_config.clone();
            self.spawn_http_server(self.stop_signal.clone())
        }
    }

    pub(crate) fn stop(&mut self) {
        if self.is_active {
            self.is_active = false;
            self.stop_signal.notify_one();
            let window = self.app.get_webview_window("main").unwrap();
            window.reload().unwrap();
            for (_key, session) in self.window_connections.drain() {
                async_runtime::spawn(async move {
                    let _ = session.close(None).await;
                });
            }
        }
    }

    /// Spawns the Actix HTTP server inside tokio task of tauri
    fn spawn_http_server(&mut self, stop_signal: Arc<Notify>) -> Result<(String, String), Error> {
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
        let listner = if let Some(port) = self.remote_ui_config.get_port() {
            TcpListener::bind((origin, port))?
        } else {
            TcpListener::bind((origin, 0))?
        };
        let port = listner.local_addr()?.port().to_string();
        self.remote_ui_config.port = Some(port.parse::<u16>().unwrap());
        self.remote_ui_config.bundle_path = Some(static_path.clone());
        let app_handle = self.app.clone();
        async_runtime::spawn(async move {
            let server = HttpServer::new(move || {
                App::new()
                    .app_data(web::Data::new(app_handle.clone()))
                    .service(remote_ui_active)
                    .route("/remote_ui_ws", web::get().to(Self::ws_handler))
                    .default_service(web::get().to(wildcard_get_handler))
            })
            .listen(listner)
            .unwrap()
            .keep_alive(KeepAlive::Disabled)
            .shutdown_timeout(1)
            .client_disconnect_timeout(Duration::from_millis(1))
            .disable_signals()
            .run();

            let server_handle = server.handle();

            println!("Going into select");
            tokio::select! {
                _ = server => (),
                _ = stop_signal.notified() => {
                    async_runtime::spawn(async move {
                        server_handle.stop(true).await;
                        println!("Server Stopped");
                    });
                    println!("Shutting down Remote UI server...");
                }
            }
            println!("Crossed select");
        });
        self.is_active = true;
        let window = self.app.get_webview_window("main").unwrap();
        let current_url = window.url().unwrap();
        let parsed = Url::parse(current_url.as_str()).unwrap();
        let host = parsed.domain().unwrap();
        let scheme = parsed.scheme();
        let new_url = format!("{}://{}:{}", scheme, host, port);
        self.activate_remote_ui_mode(&window, &new_url, &self.remote_ui_config.custom_blocking_ui)
            .unwrap();
        Ok((origin.to_owned(), port))
    }

    async fn ws_handler(
        req: HttpRequest,
        body: web::Payload,
        app_handle: web::Data<Arc<AppHandle>>,
    ) -> Result<impl Responder, actix_web::Error> {
        let (response, mut session, mut msg_stream) = actix_ws::handle(&req, body)?;
        // spawn task to process messages
        if let Some(existing_connection) = app_handle
            .app_handle()
            .state::<Arc<RwLock<RemoteUi>>>()
            .read()
            .unwrap()
            .rpc_server
            .window_connections
            .get("main")
        {
            let _ = existing_connection.clone().close(None).await;
        }
        app_handle
            .app_handle()
            .state::<Arc<RwLock<RemoteUi>>>()
            .write()
            .unwrap()
            .rpc_server
            .window_connections
            .insert("main".to_owned(), session.clone());
        actix_web::rt::spawn(async move {
            while let Some(Ok(msg)) = msg_stream.next().await {
                // echo message back
                match msg {
                    Message::Ping(bytes) => session.pong(&bytes).await.unwrap(),
                    Message::Text(text) => {
                        let remote_ui = app_handle.state::<Arc<RwLock<RemoteUi>>>();
                        let _ = remote_ui
                            .read()
                            .unwrap()
                            .invoke_rpc(text.to_string(), session.clone())
                            .await;
                    }
                    Message::Close(reason) => {
                        let _ = session.close(reason).await;
                        break;
                    }
                    _ => {
                        println!("unknown msg");
                    }
                }
            }
        });
        Ok(response)
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
