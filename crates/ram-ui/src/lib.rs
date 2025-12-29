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
// Allow common patterns in UI code
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::similar_names)]
#![allow(clippy::if_not_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::unused_async)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::single_match_else)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::float_cmp)]
#![allow(clippy::option_map_or_none)]

pub mod api;
pub mod app;
pub mod components;
pub mod services;
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
