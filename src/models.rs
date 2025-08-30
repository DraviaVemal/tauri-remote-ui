// MIT License
// Copyright (c) 2025 DraviaVemal
// See LICENSE file in the root directory.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OriginType {
    Localhost,
    Direct,
    Any,
}

impl From<OriginType> for &str {
    fn from(value: OriginType) -> Self {
        match value {
            OriginType::Localhost => "127.0.0.1",
            OriginType::Direct => "::",
            _ => "0.0.0.0",
        }
    }
}

#[derive(Serialize)]
pub struct RemoteUiEvent<P> {
    pub event_name: String,
    pub window_label: Option<String>,
    pub payload: P,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmitRequest {
    pub value: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmitResponse {
    pub value: Option<String>,
}

/// Describe this struct.
/// # Fields
/// - `allowed_origin` (`Vec<String>`) - Allowed orgin
/// - `port` (`Option<u16>`) - Set None for random port and value for specific port to use
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteUiConfig {
    pub(crate) allowed_origin: OriginType,
    pub(crate) port: Option<u16>,
    pub(crate) bundle_path: Option<String>,
    pub(crate) custom_blocking_ui: Option<String>,
}

impl Default for RemoteUiConfig {
    fn default() -> Self {
        RemoteUiConfig {
            allowed_origin: OriginType::Localhost,
            port: None,
            bundle_path: None,
            custom_blocking_ui: None,
        }
    }
}

impl RemoteUiConfig {
    pub fn set_allowed_origin(mut self, allowed_origin: OriginType) -> RemoteUiConfig {
        self.allowed_origin = allowed_origin;
        self
    }

    pub fn set_port(mut self, port: Option<u16>) -> RemoteUiConfig {
        self.port = port;
        self
    }

    pub fn set_bundle_path(mut self, bundle_path: Option<String>) -> RemoteUiConfig {
        self.bundle_path = bundle_path;
        self
    }

    /// Inject standardized HTML, CSS, and JavaScript to allow customization of the UI blocking message during a remote session
    /// Pass %URL% where URL will be updated and %URL_INFO% for info path
    pub fn set_custom_blocking_ui(mut self, bundle_path: Option<String>) -> RemoteUiConfig {
        self.bundle_path = bundle_path;
        self
    }

    pub fn get_allowed_origin(&self) -> OriginType {
        self.allowed_origin.clone()
    }

    pub fn get_port(&self) -> Option<u16> {
        self.port.clone()
    }

    pub fn get_bundle_path(&self) -> Option<String> {
        self.bundle_path.clone()
    }
}

// Structure representing the payload of an RPC invoke request
#[derive(Debug, Deserialize)]
pub struct WsPayload {
    pub id: usize,
    pub cmd: String,
    pub args: Option<Value>,
    pub option: Option<Value>,
}

#[derive(Serialize, Deserialize)]
pub(crate) enum RpcResponseStatus {
    Success,
    Error,
    Invalid,
}

impl From<RpcResponseStatus> for &str {
    fn from(value: RpcResponseStatus) -> Self {
        match value {
            RpcResponseStatus::Success => "success",
            RpcResponseStatus::Error => "error",
            _ => "invalid",
        }
    }
}

impl From<&str> for RpcResponseStatus {
    fn from(value: &str) -> Self {
        match value {
            "success" => RpcResponseStatus::Success,
            "error" => RpcResponseStatus::Error,
            _ => RpcResponseStatus::Invalid,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct RpcResult {
    pub(crate) status: RpcResponseStatus,
    pub(crate) data: Value,
}
