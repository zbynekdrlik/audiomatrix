//! `AudioMatrix` Service
//!
//! Main entry point for the `AudioMatrix` audio routing system.

use std::net::SocketAddr;

use anyhow::Result;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("AudioMatrix v{}", env!("CARGO_PKG_VERSION"));
    info!("Starting service...");

    // Create API router
    let app = ram_api::create_router();

    // Bind to address
    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    info!("API server listening on {addr}");

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
    }
}
