//! RAM API - REST and WebSocket API for `AudioMatrix`
//!
//! This crate provides the HTTP API for controlling `AudioMatrix`:
//! - REST endpoints for configuration and control
//! - WebSocket for real-time events and metering

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod error;
pub mod handlers;
pub mod models;
pub mod router;
pub mod websocket;

pub use error::{Error, Result};
pub use router::create_router;

/// Default API port.
pub const DEFAULT_PORT: u16 = 8080;

/// API version prefix.
pub const API_VERSION: &str = "v1";
