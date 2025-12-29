//! API router configuration.

use axum::routing::{delete, get, patch, post, put};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::state::AppState;
use crate::static_files::serve_path;
use crate::websocket::ws_handler;
use crate::API_VERSION;

/// Creates the API router with the given application state.
pub fn create_router_with_state(state: AppState) -> Router {
    let api_routes = Router::new()
        // Health
        .route("/health", get(handlers::health))
        // Nodes
        .route("/nodes", get(handlers::list_nodes))
        .route("/nodes/:id", get(handlers::get_node))
        // Devices
        .route("/nodes/:node_id/devices", get(handlers::list_devices))
        .route(
            "/nodes/:node_id/devices/:device_id",
            get(handlers::get_device).patch(handlers::update_device),
        )
        .route(
            "/nodes/:node_id/devices/:device_id/attach",
            post(handlers::attach_device),
        )
        .route(
            "/nodes/:node_id/devices/:device_id/detach",
            post(handlers::detach_device),
        )
        // Channel labels
        .route(
            "/nodes/:node_id/devices/:device_id/channels",
            get(handlers::get_device_channels).put(handlers::bulk_update_channel_labels),
        )
        .route(
            "/nodes/:node_id/devices/:device_id/channels/:channel",
            patch(handlers::update_channel_label),
        )
        // Virtual devices
        .route(
            "/nodes/:node_id/virtual-devices",
            get(handlers::list_virtual_devices).post(handlers::create_virtual_device),
        )
        .route(
            "/nodes/:node_id/virtual-devices/:device_id",
            patch(handlers::update_virtual_device).delete(handlers::delete_virtual_device),
        )
        // Wave generators (test signal injection)
        .route(
            "/nodes/:node_id/devices/:device_id/generator",
            get(handlers::get_device_generator).post(handlers::set_all_generators),
        )
        .route(
            "/nodes/:node_id/devices/:device_id/channels/:channel/generator",
            post(handlers::set_channel_generator),
        )
        // Routes
        .route("/routes", get(handlers::list_routes))
        .route("/routes", post(handlers::create_route))
        .route("/routes/:id", get(handlers::get_route))
        .route("/routes/:id", put(handlers::update_route))
        .route("/routes/:id", delete(handlers::delete_route))
        .route("/routes/:id/latency", get(handlers::get_route_latency))
        // Streams
        .route("/streams", get(handlers::list_streams))
        .route("/streams/count", get(handlers::get_stream_count))
        // Subscriptions
        .route("/subscriptions", get(handlers::list_subscriptions))
        .route("/subscriptions", post(handlers::create_subscription))
        .route("/subscriptions/stats", get(handlers::get_subscription_stats))
        // WebSocket
        .route("/ws", get(ws_handler))
        .with_state(state);

    Router::new()
        .nest(&format!("/api/{API_VERSION}"), api_routes)
        // Static files and SPA fallback - serves the web UI
        .fallback(serve_path)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
}

/// Creates the API router with default state.
pub fn create_router() -> Router {
    create_router_with_state(AppState::default())
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

    #[tokio::test]
    async fn nodes_endpoint_works() {
        let app = create_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/nodes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn routes_endpoint_works() {
        let app = create_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/routes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn get_node_not_found() {
        let app = create_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/nodes/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_route_not_found() {
        let app = create_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/routes/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
