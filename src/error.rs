
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
///
/// Wraps IO errors and can be extended for other error kinds.
#[derive(Debug, thiserror::Error)]
pub enum Error {
  /// IO error variant, wraps `std::io::Error`.
  #[error(transparent)]
  Io(#[from] std::io::Error),
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
