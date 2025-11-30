
//! Tauri Remote UI Plugin Library
//!
//! This crate provides the main entry point and exports for the remote UI plugin for Tauri applications.
//! It exposes plugin initialization, error types, configuration models, and remote UI APIs.
//!
//! # License
//! AGPL-3.0-only License
//! Copyright (c) 2025 DraviaVemal
//! See LICENSE file in the root directory.

use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Wry,
};


/// Re-export all public models for convenience.
pub use models::*;

pub use error::{Error, Result};
pub use remote_ui::*;

mod error;
mod models;
/// Remote UI module containing plugin logic and APIs.
pub mod remote_ui;


/// Initializes the remote-ui Tauri plugin.
///
/// This function should be called from your Tauri application's plugin registration.
/// It sets up the remote UI state and exposes plugin APIs.
///
/// # Example
/// ```rust
/// tauri::Builder::default()
///     .plugin(tauri_remote_ui::init())
///     .run(tauri::generate_context!())
///     .expect("error while running tauri application");
/// ```
pub fn init() -> TauriPlugin<Wry> {
    Builder::new("remote-ui")
        .setup(|app, api| {
            let remote_ui = remote_ui::init(app, api)?;
            app.manage(remote_ui);
            Ok(())
        })
        .build()
}
