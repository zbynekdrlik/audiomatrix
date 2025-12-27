//! API router configuration.

use axum::routing::get;
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::websocket::ws_handler;
use crate::API_VERSION;

/// Creates the API router.
pub fn create_router() -> Router {
    let api_routes = Router::new()
        // Health
        .route("/health", get(handlers::health))
        // Nodes
        .route("/nodes", get(handlers::list_nodes))
        .route("/nodes/{id}", get(handlers::get_node))
        // Devices
        .route("/nodes/{node_id}/devices", get(handlers::list_devices))
        .route(
            "/nodes/{node_id}/devices/{device_id}",
            get(handlers::get_device),
        )
        // WebSocket
        .route("/ws", get(ws_handler));

    Router::new()
        .nest(&format!("/api/{API_VERSION}"), api_routes)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_endpoint_works() {
        let app = create_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
