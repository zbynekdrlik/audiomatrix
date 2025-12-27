//! RAM API - REST and WebSocket API for `AudioMatrix`
//!
//! This crate provides the HTTP API for controlling `AudioMatrix`:
//! - REST endpoints for configuration and control
//! - WebSocket for real-time events and metering
//! - Shared application state management

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod auth;
pub mod error;
pub mod handlers;
pub mod models;
pub mod router;
pub mod state;
pub mod websocket;

pub use auth::{Auth, AuthContext, AuthManager, Permission, SecurityConfig, SecurityMode};
pub use error::{Error, Result};
pub use router::create_router;
pub use state::AppState;

/// Default API port.
pub const DEFAULT_PORT: u16 = 8080;

/// API version prefix.
pub const API_VERSION: &str = "v1";
