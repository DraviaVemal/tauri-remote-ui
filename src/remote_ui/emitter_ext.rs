//! Emitter Extension for Tauri Remote UI
//!
//! This module provides the `EmitterExt` trait for extending Tauri's event emission capabilities,
//! integrating with the remote UI plugin and supporting both standard and remote event flows.
//!
//! # License
//! AGPL-3.0-only License
//! Copyright (c) 2025 DraviaVemal
//! See LICENSE file in the root directory.

use crate::RemoteUi;
use serde::Serialize;
use std::{future::Future, sync::Arc};
use tauri::{Emitter, Error, EventTarget, Manager, Runtime, WebviewWindow};
use tokio::sync::RwLock;

/// Extension trait for event emission in Tauri Remote UI.
///
/// Provides async and filter-based event emission methods, integrating with the remote UI plugin.
pub trait EmitterExt<R>
where
    R: Runtime,
{
    /// Emit an event to the window and, if remote UI is active, also over WebSocket.
    fn emit<S: Serialize + Clone>(
        &self,
        event: &str,
        payload: S,
    ) -> impl Future<Output = Result<(), Error>>;

    /// Emit an event to a specific target.
    fn emit_to<I, S>(&self, target: I, event: &str, payload: S) -> Result<(), Error>
    where
        I: Into<EventTarget>,
        S: Serialize + Clone;

    /// Emit an event with a string payload.
    fn emit_str(&self, event: &str, payload: String) -> Result<(), Error>;

    /// Emit an event with a string payload to a specific target.
    fn emit_str_to<I>(&self, target: I, event: &str, payload: String) -> Result<(), Error>
    where
        I: Into<EventTarget>;

    /// Emit an event to all targets passing the filter.
    fn emit_filter<S, F>(&self, event: &str, payload: S, filter: F) -> Result<(), Error>
    where
        S: Serialize + Clone,
        F: Fn(&EventTarget) -> bool;

    /// Emit an event with a string payload to all targets passing the filter.
    fn emit_str_filter<F>(&self, event: &str, payload: String, filter: F) -> Result<(), Error>
    where
        F: Fn(&EventTarget) -> bool;
}

impl<R> EmitterExt<R> for WebviewWindow<R>
where
    R: Runtime,
{
    /// Emit an event to the window and, if remote UI is active, also over WebSocket.
    ///
    /// This method first checks if the remote UI RPC is active, and if so, emits the event via WebSocket.
    /// It then emits the event using Tauri's standard event system.
    async fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), Error> {
        let remote_ui = self.state::<Arc<RwLock<RemoteUi>>>();
        {
            let guard = remote_ui.read().await;
            if guard.is_rpc_active() {
                guard.emit(event, payload.clone()).await?;
            }
        }
        Emitter::emit(self, event, payload)?;
        Ok(())
    }

    /// Emit an event to a specific target using Tauri's event system.
    fn emit_to<I, S>(&self, target: I, event: &str, payload: S) -> Result<(), Error>
    where
        I: Into<EventTarget>,
        S: Serialize + Clone,
    {
        Emitter::emit_to(self, target, event, payload)
    }

    /// Emit an event with a string payload using Tauri's event system.
    fn emit_str(&self, event: &str, payload: String) -> Result<(), Error> {
        Emitter::emit_str(self, event, payload)
    }

    /// Emit an event with a string payload to a specific target using Tauri's event system.
    fn emit_str_to<I>(&self, target: I, event: &str, payload: String) -> Result<(), Error>
    where
        I: Into<EventTarget>,
    {
        Emitter::emit_str_to(self, target, event, payload)
    }

    /// Emit an event to all targets passing the filter using Tauri's event system.
    fn emit_filter<S, F>(&self, event: &str, payload: S, filter: F) -> Result<(), Error>
    where
        S: Serialize + Clone,
        F: Fn(&EventTarget) -> bool,
    {
        Emitter::emit_filter(self, event, payload, filter)
    }

    /// Emit an event with a string payload to all targets passing the filter using Tauri's event system.
    fn emit_str_filter<F>(&self, event: &str, payload: String, filter: F) -> Result<(), Error>
    where
        F: Fn(&EventTarget) -> bool,
    {
        Emitter::emit_str_filter(self, event, payload, filter)
    }
}
