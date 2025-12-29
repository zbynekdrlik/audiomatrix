//! `AudioMatrix` Service
//!
//! Main entry point for the `AudioMatrix` audio routing system.
//!
//! This service coordinates all components:
//! - Audio device management
//! - VBAN network audio
//! - REST/WebSocket API
//! - Service discovery (mDNS)

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
// Allow common patterns in service code
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
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::significant_drop_in_scrutinee)]
#![allow(clippy::await_holding_lock)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::unused_async)]
#![allow(clippy::similar_names)]
#![allow(clippy::if_not_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::float_cmp)]
#![allow(clippy::let_underscore_must_use)]
#![allow(clippy::blocks_in_conditions)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::wildcard_imports)]
#![allow(dead_code)] // Known dead code per ARCHITECTURE.md technical debt
#![allow(clippy::items_after_statements)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::single_match_else)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::missing_fields_in_debug)]

use std::path::PathBuf;

use anyhow::Result;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod audio_processor;
mod config;
mod route_manager;
mod service;
#[cfg(windows)]
mod tray;
mod vban_manager;

use config::ServiceConfig;
use service::AudioMatrixService;

/// Command line arguments.
struct Args {
    /// Path to configuration file.
    config: Option<PathBuf>,
    /// Override node name.
    node_name: Option<String>,
    /// Override API port.
    api_port: Option<u16>,
    /// Override VBAN port.
    vban_port: Option<u16>,
}

fn parse_args() -> Args {
    let mut args = Args {
        config: None,
        node_name: None,
        api_port: None,
        vban_port: None,
    };

    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-c" | "--config" => {
                args.config = iter.next().map(PathBuf::from);
            },
            "-n" | "--name" => {
                args.node_name = iter.next();
            },
            "-p" | "--port" => {
                args.api_port = iter.next().and_then(|s| s.parse().ok());
            },
            "--vban-port" => {
                args.vban_port = iter.next().and_then(|s| s.parse().ok());
            },
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            },
            "-V" | "--version" => {
                println!("AudioMatrix v{}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            },
            _ => {
                eprintln!("Unknown argument: {arg}");
                eprintln!("Use --help for usage information");
                std::process::exit(1);
            },
        }
    }

    args
}

fn print_help() {
    println!(
        "AudioMatrix v{} - Professional Audio Routing System

USAGE:
    audiomatrix [OPTIONS]

OPTIONS:
    -c, --config <FILE>     Configuration file path
    -n, --name <NAME>       Node name (default: hostname)
    -p, --port <PORT>       API server port (default: 8080)
        --vban-port <PORT>  VBAN port (default: 6980)
    -h, --help              Print help information
    -V, --version           Print version information

ENVIRONMENT VARIABLES:
    AUDIOMATRIX_CONFIG      Configuration file path
    AUDIOMATRIX_NODE_NAME   Node name
    AUDIOMATRIX_API_PORT    API server port
    AUDIOMATRIX_VBAN_PORT   VBAN port
    RUST_LOG                Log level (trace, debug, info, warn, error)

EXAMPLES:
    audiomatrix                          Start with default settings
    audiomatrix -n Studio                Start with node name 'Studio'
    audiomatrix -c /etc/audiomatrix.json Start with config file
    audiomatrix -p 9000 --vban-port 7000 Start with custom ports
",
        env!("CARGO_PKG_VERSION")
    );
}

fn load_config(args: &Args) -> Result<ServiceConfig> {
    // Try config file from args, then environment, then default
    let config_path = args
        .config
        .clone()
        .or_else(|| std::env::var("AUDIOMATRIX_CONFIG").ok().map(PathBuf::from));

    let mut config = if let Some(path) = config_path {
        if path.exists() {
            info!("Loading configuration from {}", path.display());
            ServiceConfig::from_file(&path)?
        } else {
            info!("Config file not found, using defaults");
            ServiceConfig::default()
        }
    } else {
        ServiceConfig::default()
    };

    // Apply command line overrides
    if let Some(name) = &args.node_name {
        config.node_name.clone_from(name);
    } else if let Ok(name) = std::env::var("AUDIOMATRIX_NODE_NAME") {
        config.node_name = name;
    }

    if let Some(port) = args.api_port {
        config.api.port = port;
    } else if let Ok(port) = std::env::var("AUDIOMATRIX_API_PORT") {
        if let Ok(p) = port.parse() {
            config.api.port = p;
        }
    }

    if let Some(port) = args.vban_port {
        config.vban.port = port;
    } else if let Ok(port) = std::env::var("AUDIOMATRIX_VBAN_PORT") {
        if let Ok(p) = port.parse() {
            config.vban.port = p;
        }
    }

    Ok(config)
}

fn setup_logging(config: &ServiceConfig) {
    let level = match config.logging.level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "warn" | "warning" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(config.logging.include_target)
        .with_file(config.logging.include_location)
        .with_line_number(config.logging.include_location)
        .finish();

    if tracing::subscriber::set_global_default(subscriber).is_err() {
        eprintln!("Warning: Failed to set logging subscriber");
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args();
    let config = load_config(&args)?;

    setup_logging(&config);

    info!("AudioMatrix v{}", env!("CARGO_PKG_VERSION"));
    info!("Node: {}", config.node_name);

    // Create and run service
    let mut service = AudioMatrixService::new(config.clone());

    // Set up Ctrl+C handler
    let shutdown = service.shutdown_signal();
    let ctrl_c_shutdown = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            info!("Received Ctrl+C, initiating shutdown...");
            ctrl_c_shutdown.shutdown();
        }
    });

    // Start system tray on Windows
    #[cfg(windows)]
    let _tray_handle = {
        let tray_shutdown = shutdown.clone();
        let api_port = config.api.port;

        std::thread::spawn(move || {
            tray::run_tray(tray_shutdown, api_port);
        })
    };

    // Run service
    service.run().await?;

    info!("AudioMatrix shutdown complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_set() {
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
    }

    #[test]
    fn config_defaults() {
        let config = ServiceConfig::default();
        assert_eq!(config.api.port, 8080);
        assert_eq!(config.vban.port, 6980);
    }

    #[test]
    fn parse_args_empty() {
        // This would test arg parsing but requires mocking env::args
        // For now, just ensure the function exists
    }
}
