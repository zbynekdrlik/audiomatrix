//! RAM UI - Leptos-based Web Interface for AudioMatrix
//!
//! This crate provides a reactive web interface for the AudioMatrix
//! audio routing system. It uses Leptos for client-side rendering
//! and shares types with ram-api for type-safe API calls.
//!
//! # Architecture
//!
//! ```text
//! Browser
//!   └── Leptos App (WASM)
//!         ├── Components (routing_matrix, device_card, meter)
//!         ├── API Client (uses ram-api::models)
//!         └── WebSocket (real-time metering)
//! ```

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod api;
pub mod app;
pub mod components;
pub mod state;

use wasm_bindgen::prelude::*;

/// Initialize the application.
///
/// This is called from JavaScript/index.html to mount the Leptos app.
#[wasm_bindgen(start)]
pub fn main() {
    // Set up panic hook for better error messages
    console_error_panic_hook::set_once();

    // Initialize logging
    let _ = console_log::init_with_level(log::Level::Debug);

    log::info!("AudioMatrix UI starting...");

    // Mount the Leptos app
    leptos::mount::mount_to_body(app::App);

    log::info!("AudioMatrix UI mounted");
}
