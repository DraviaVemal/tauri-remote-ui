use crate::{models::*, RemoteUi};
use actix_files::Files;
use actix_web::{get, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_ws::{Message, Session};
use futures_util::StreamExt;
use std::{
    collections::HashMap,
    net::TcpListener,
    sync::{Arc, RwLock},
};
use tauri::{AppHandle, Error, Manager};
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
    HttpResponse::Ok().json(remote_ui_config)
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
            self.spawn_http_server(self.stop_signal.clone(), &remote_ui_config)
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
    fn spawn_http_server(
        &mut self,
        stop_signal: Arc<Notify>,
        remote_ui_config: &RemoteUiConfig,
    ) -> Result<(String, String), Error> {
        let origin: &str = remote_ui_config.get_allowed_origin().into();
        let static_path = remote_ui_config
            .get_bundle_path()
            .unwrap_or("./static".to_owned());
        let listner = if let Some(port) = remote_ui_config.get_port() {
            TcpListener::bind((origin, port))?
        } else {
            TcpListener::bind((origin, 0))?
        };
        let port = listner.local_addr()?.port().to_string();
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
