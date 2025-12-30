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
// Allow common patterns
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::unused_async)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::significant_drop_in_scrutinee)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::similar_names)]
#![allow(clippy::if_not_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::unit_arg)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::assigning_clones)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::case_sensitive_file_extension_comparisons)]
#![allow(clippy::ignored_unit_patterns)]

// Always available - shared types for client and server
pub mod models;

// Server-only modules
#[cfg(feature = "server")]
pub mod auth;
#[cfg(feature = "server")]
mod cross_node;
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
pub use state::{AppState, DeviceCommand};

/// Default API port.
pub const DEFAULT_PORT: u16 = 8080;

/// API version prefix.
pub const API_VERSION: &str = "v1";
