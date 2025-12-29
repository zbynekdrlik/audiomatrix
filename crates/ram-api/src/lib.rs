//! RAM API - REST and WebSocket API for `AudioMatrix`
//!
//! This crate provides the HTTP API for controlling `AudioMatrix`:
//! - REST endpoints for configuration and control
//! - WebSocket for real-time events and metering
//! - Shared application state management
//!
//! # Features
//!
//! - `server` (default): Full server implementation with Axum
//! - `client`: Only models for WASM clients (no server dependencies)

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

// Always available - shared types for client and server
pub mod models;

// Server-only modules
#[cfg(feature = "server")]
pub mod auth;
#[cfg(feature = "server")]
pub mod error;
#[cfg(feature = "server")]
pub mod handlers;
#[cfg(feature = "server")]
pub mod router;
#[cfg(feature = "server")]
pub mod state;
#[cfg(feature = "server")]
pub mod static_files;
#[cfg(feature = "server")]
pub mod subscription_client;
#[cfg(feature = "server")]
pub mod websocket;

#[cfg(feature = "server")]
pub use auth::{Auth, AuthContext, AuthManager, Permission, SecurityConfig, SecurityMode};
#[cfg(feature = "server")]
pub use error::{Error, Result};
#[cfg(feature = "server")]
pub use router::create_router;
#[cfg(feature = "server")]
pub use state::AppState;

/// Default API port.
pub const DEFAULT_PORT: u16 = 8080;

/// API version prefix.
pub const API_VERSION: &str = "v1";
