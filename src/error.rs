//! Error handling utilities for tauri-remote-ui.
//!
//! This module defines a custom error type and result alias for unified error management across the project.
//!
//! # License
//! AGPL-3.0-only License
//! Copyright (c) 2025 DraviaVemal
//! See LICENSE file in the root directory.

use serde::{ser::Serializer, Serialize};

/// Convenient result type alias using the custom `Error` type.
pub type Result<T> = std::result::Result<T, Error>;

/// Unified error type for tauri-remote-ui.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Wraps `std::io::Error` (network, filesystem, etc.).
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Wraps `tauri::Error` from the underlying Tauri runtime.
    #[error(transparent)]
    Tauri(#[from] tauri::Error),

    /// Wraps `serde_json::Error` from JSON (de)serialization.
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// Wraps `hyper::http::Error` from HTTP response building.
    #[error(transparent)]
    Http(#[from] hyper::http::Error),

    /// The remote UI server is already running.
    #[error("Remote UI server is already running")]
    ServerAlreadyRunning,

    /// The configured primary webview window could not be found.
    #[error("Primary webview window '{0}' not found")]
    PrimaryWindowNotFound(String),

    /// The Tauri `frontend_dist` is configured as a remote URL, which is unsupported.
    #[error("frontend_dist is configured as a remote URL; a local path is required")]
    InvalidFrontendDist,

    /// A WebSocket transport error occurred.
    #[error("WebSocket error: {0}")]
    WebSocket(String),

    /// A generic plugin error with a contextual message.
    #[error("Plugin error: {0}")]
    Plugin(String),
}

/// Serialize the error as a string for use in APIs and logging.
impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}

/// Convert a crate-level `Error` into a `tauri::Error` so it can be returned
/// from public extension trait methods that mirror Tauri's signatures.
impl From<Error> for tauri::Error {
    fn from(err: Error) -> Self {
        match err {
            Error::Tauri(e) => e,
            other => {
                tauri::Error::PluginInitialization("tauri-remote-ui".to_owned(), other.to_string())
            }
        }
    }
}
