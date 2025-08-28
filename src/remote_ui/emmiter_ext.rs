use crate::RemoteUi;
use serde::Serialize;
use std::sync::{Arc, RwLock};
use tauri::{Emitter, Error, EventTarget, Manager, Runtime, WebviewWindow};

pub trait EmitterExt<R>
where
    R: Runtime,
{
    fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), Error>;
    fn emit_to<I, S>(&self, target: I, event: &str, payload: S) -> Result<(), Error>
    where
        I: Into<EventTarget>,
        S: Serialize + Clone;
}

impl<R> EmitterExt<R> for WebviewWindow<R>
where
    R: Runtime,
{
    fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), Error> {
        let remote_ui = self.state::<Arc<RwLock<RemoteUi>>>();
        if remote_ui.read().unwrap().is_rpc_active() {
            remote_ui.read().unwrap().emit(event, payload)
        } else {
            Emitter::emit(self, event, payload)
        }
    }

    fn emit_to<I, S>(&self, target: I, event: &str, payload: S) -> Result<(), Error>
    where
        I: Into<EventTarget>,
        S: Serialize + Clone,
    {
        let remote_ui = self.state::<Arc<RwLock<RemoteUi>>>();
        if remote_ui.read().unwrap().is_rpc_active() {
            remote_ui
                .read()
                .unwrap()
                .emit_to(target.into(), event, payload)
        } else {
            Emitter::emit_to(self, target, event, payload)
        }
    }
}
