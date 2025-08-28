use crate::{models::*, RemoteUi};
use actix_files::Files;
use actix_web::{get, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_ws::{Message, Session};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    env,
    net::TcpListener,
    sync::{Arc, RwLock},
};
use tauri::{AppHandle, Error, Manager, Url};
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
            for (_key, session) in self.window_connections.drain() {
                tauri::async_runtime::spawn(async move {
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
        tauri::async_runtime::spawn(async move {
            let server = HttpServer::new(move || {
                App::new()
                    .app_data(web::Data::new(app_handle.clone()))
                    .service(remote_ui_active)
                    .route("/remote_ui_ws", web::get().to(Self::ws_handler))
                    .service(Files::new("/", &static_path).index_file("index.html"))
            })
            .listen(listner)
            .unwrap()
            .disable_signals() // important inside Tauri
            .run();

            tokio::select! {
                _ = server => (),
                _ = stop_signal.notified() => {
                    // graceful shutdown

                    println!("Shutting down Actix server...");
                }
            }
        });
        self.is_active = true;

        #[cfg(debug_assertions)]
        {
            let window = self.app.get_webview_window("main").unwrap();
            let current_url = window.url().unwrap();
            let parsed = Url::parse(current_url.as_str()).unwrap();
            let host = parsed.domain().unwrap();
            let scheme = parsed.scheme();
            let window = window.clone();
            let new_url = format!("{}://{}:{}/remote_ui", scheme, host, port);
            // Delay navigation by 5 seconds
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                window.navigate(Url::parse(&new_url).unwrap()).unwrap();
            });
        }
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
                        println!("Closed Connection");
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
}
