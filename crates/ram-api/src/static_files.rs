//! Static file serving for embedded UI assets.
//!
//! The web UI is built with Trunk and embedded into the binary using rust-embed.
//! This module provides handlers to serve the static files and implements
//! SPA routing (fallback to index.html for client-side routes).

use axum::body::Body;
use axum::http::{header, Request, Response, StatusCode, Uri};
use axum::response::IntoResponse;
use rust_embed::Embed;

/// Embedded UI assets from the Trunk build output.
/// The path is relative to the ram-api crate root.
#[derive(Embed)]
#[folder = "../ram-ui/dist/"]
struct UiAssets;

/// Serve a static file or fall back to index.html for SPA routing.
pub async fn static_handler(req: Request<Body>) -> impl IntoResponse {
    let path = req.uri().path().trim_start_matches('/');

    // Try to serve the requested file
    if let Some(response) = serve_file(path) {
        return response;
    }

    // For SPA routing: serve index.html for any non-file path
    // This allows the client-side router to handle the route
    if !path.contains('.') || path.is_empty() {
        if let Some(response) = serve_file("index.html") {
            return response;
        }
    }

    // File not found
    not_found()
}

/// Serve a static file from embedded assets.
fn serve_file(path: &str) -> Option<Response<Body>> {
    let asset = UiAssets::get(path)?;

    let content_type = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();

    let body = Body::from(asset.data.into_owned());

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, cache_control(path))
        .body(body)
        .ok()
}

/// Get cache control header based on file type.
fn cache_control(path: &str) -> &'static str {
    // Hash-based filenames are immutable, cache forever
    if path.contains("-") && (path.ends_with(".js") || path.ends_with(".wasm") || path.ends_with(".css")) {
        "public, max-age=31536000, immutable"
    } else if path == "index.html" {
        // HTML should be revalidated
        "no-cache"
    } else {
        // Default: 1 hour cache
        "public, max-age=3600"
    }
}

/// Return a 404 response.
fn not_found() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from("Not Found"))
        .unwrap()
}

/// Index handler that serves the main SPA page.
pub async fn index_handler() -> impl IntoResponse {
    serve_file("index.html").unwrap_or_else(not_found)
}

/// Get URI path from request.
pub async fn serve_path(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    // Try to serve the requested file
    if let Some(response) = serve_file(path) {
        return response;
    }

    // SPA fallback
    if !path.contains('.') {
        if let Some(response) = serve_file("index.html") {
            return response;
        }
    }

    not_found()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_control_hashed_js() {
        let control = cache_control("ram-ui-abc123.js");
        assert!(control.contains("immutable"));
    }

    #[test]
    fn test_cache_control_html() {
        let control = cache_control("index.html");
        assert_eq!(control, "no-cache");
    }

    #[test]
    fn test_cache_control_default() {
        let control = cache_control("favicon.ico");
        assert!(control.contains("3600"));
    }
}
