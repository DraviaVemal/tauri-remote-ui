// AGPL-3.0-only License
// Copyright (c) 2025 DraviaVemal
// See LICENSE file in the root directory.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Access scope the remote UI HTTP/WebSocket server allows connections from.
///
/// This describes *who* may connect, not which IP family the listener uses —
/// the server always binds to an address sufficient to satisfy the scope and
/// applies a peer-address allow-list at request time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OriginType {
    /// Only the local machine (loopback) can connect. Binds to `127.0.0.1`.
    /// Most secure default.
    Localhost,
    /// Only hosts on the same local subnet(s) as this machine can connect.
    /// Binds to `0.0.0.0` and rejects peers whose address is not within any
    /// configured local interface subnet.
    Subnet,
    /// Any host that can route to this machine may connect. Binds to
    /// `0.0.0.0` with no peer filtering. Use with care.
    Any,
}

impl OriginType {
    /// The address the TCP listener binds to for this scope.
    pub fn bind_address(self) -> &'static str {
        match self {
            OriginType::Localhost => "127.0.0.1",
            OriginType::Subnet | OriginType::Any => "0.0.0.0",
        }
    }
}

impl From<OriginType> for &'static str {
    fn from(value: OriginType) -> Self {
        value.bind_address()
    }
}

/// Configuration for the remote UI server.
///
/// Build with [`RemoteUiConfig::default`] and chain the `set_*` / `enable_*` /
/// `disable_*` builder methods to customise behaviour before passing the value
/// to [`crate::RemoteUiExt::start_remote_ui`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteUiConfig {
    pub(crate) application_ui: bool,
    pub(crate) allowed_origin: OriginType,
    pub(crate) port: Option<u16>,
    pub(crate) bundle_path: Option<String>,
    pub(crate) primary_window_label: String,
    pub(crate) minimize_app: bool,
    pub(crate) enable_info_url: bool,
    pub(crate) custom_blocking_ui: Option<String>,
    pub(crate) custom_disconnect_ui: Option<String>,
}

impl Default for RemoteUiConfig {
    fn default() -> Self {
        RemoteUiConfig {
            application_ui: false,
            allowed_origin: OriginType::Localhost,
            port: None,
            bundle_path: None,
            primary_window_label: "main".to_owned(),
            enable_info_url: true,
            minimize_app: false,
            custom_disconnect_ui: None,
            custom_blocking_ui: None,
        }
    }
}

impl RemoteUiConfig {
    /// Keep the original Tauri UI active and let it navigate to the same path
    /// as the remote UI, instead of replacing it with a blocking screen.
    pub fn enable_application_ui(mut self) -> Self {
        self.application_ui = true;
        self
    }

    /// Minimize the host Tauri window when the remote UI server starts.
    pub fn minimize_app(mut self) -> Self {
        self.minimize_app = true;
        self
    }

    /// Disable the `/remote_ui_info` endpoint. After calling this, requests to
    /// that path will respond with `404 Not Found`.
    pub fn disable_info_url(mut self) -> Self {
        self.enable_info_url = false;
        self
    }

    /// Set the network origin that the remote UI server binds to.
    pub fn set_allowed_origin(mut self, allowed_origin: OriginType) -> Self {
        self.allowed_origin = allowed_origin;
        self
    }

    /// Set the TCP port to listen on. Pass `None` (the default) to let the OS
    /// pick a free port — the chosen port is then surfaced via
    /// [`crate::RemoteUiExt::remote_ui_port`].
    pub fn set_port(mut self, port: Option<u16>) -> Self {
        self.port = port;
        self
    }

    /// Override the static bundle path to serve assets from. Defaults to the
    /// Tauri-configured `frontend_dist`, falling back to `../dist`.
    pub fn set_bundle_path(mut self, bundle_path: Option<String>) -> Self {
        self.bundle_path = bundle_path;
        self
    }

    /// Inject a custom HTML/CSS/JS payload to be displayed on the host Tauri
    /// window while a remote session is active. The following placeholders are
    /// substituted before the HTML is injected:
    ///
    /// - `%URLS%` — comma-separated list of every URL the server is reachable on.
    /// - `%URLS_LIST%` — pre-rendered `<li><a href="…">…</a></li>` items for
    ///   embedding inside a `<ul>` / `<ol>`.
    /// - `%URL_INFO%` — the `/remote_ui_info` URL.
    pub fn set_custom_blocking_ui(mut self, custom_blocking_ui: Option<String>) -> Self {
        self.custom_blocking_ui = custom_blocking_ui;
        self
    }

    /// Inject a custom HTML/CSS/JS payload to be displayed in the remote
    /// browser tab when the connection is closed.
    pub fn set_custom_disconnect_ui(mut self, custom_disconnect_ui: Option<String>) -> Self {
        self.custom_disconnect_ui = custom_disconnect_ui;
        self
    }

    /// Override the label of the Tauri webview window that the remote UI
    /// controls. Defaults to `"main"`.
    pub fn set_primary_window_label(mut self, label: impl Into<String>) -> Self {
        self.primary_window_label = label.into();
        self
    }

    /// The configured network origin.
    pub fn allowed_origin(&self) -> OriginType {
        self.allowed_origin
    }

    /// The configured port, if any.
    pub fn port(&self) -> Option<u16> {
        self.port
    }

    /// The configured bundle path, if any.
    pub fn bundle_path(&self) -> Option<&str> {
        self.bundle_path.as_deref()
    }

    /// The configured primary window label.
    pub fn primary_window_label(&self) -> &str {
        &self.primary_window_label
    }
}

/// WebSocket payload describing an RPC invoke request from the remote UI.
#[derive(Debug, Deserialize)]
pub struct WsPayload {
    pub id: usize,
    pub cmd: String,
    pub args: Option<Value>,
    pub option: Option<Value>,
}

/// Status discriminator on the response payload sent back to the remote UI.
///
/// Wire format is the lowercase string of each variant (`"success"` or
/// `"error"`). The matching TypeScript constant lives in
/// `npm-package/src/socket.ts` so both sides share a single source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RpcStatus {
    /// The invoked command resolved successfully.
    Success,
    /// The invoked command rejected or threw.
    Error,
}

impl RpcStatus {
    /// The lowercase wire-format string for this status.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Error => "error",
        }
    }
}
