//! Route controller trait for API integration.
//!
//! This module defines the `RouteController` trait that abstracts audio route
//! management. The trait allows the API layer (ram-api) to manage routes without
//! depending on the service layer (ram-service).
//!
//! # Architecture
//!
//! ```text
//! ram-api (AppState) --uses--> RouteController trait (ram-core)
//!                                      ^
//!                                      |
//!                              implements
//!                                      |
//! ram-service (AudioProcessor) --------+
//! ```

use std::sync::Arc;

use crate::latency::LatencyReport;
use crate::stream_registry::StreamRegistry;
use crate::subscription_manager::SubscriptionManager;
use crate::ConnectionId;

/// Result type for route operations.
pub type RouteResult<T> = std::result::Result<T, RouteError>;

/// Errors that can occur during route operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RouteError {
    /// No buffer available in the pool.
    #[error("no buffer available")]
    NoBufferAvailable,

    /// Route not found.
    #[error("route not found: {0}")]
    NotFound(String),

    /// Invalid route configuration.
    #[error("invalid route: {0}")]
    Invalid(String),

    /// Internal error.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Trait for managing audio routes.
///
/// This trait is implemented by `AudioProcessor` and used by the API layer
/// to manage routes without creating a circular dependency.
pub trait RouteController: Send + Sync {
    /// Adds a route to the routing matrix.
    ///
    /// Returns the buffer index allocated for this route.
    fn add_route(&self, conn_id: ConnectionId) -> RouteResult<usize>;

    /// Removes a route from the routing matrix.
    fn remove_route(&self, conn_id: &ConnectionId) -> RouteResult<()>;

    /// Sets the gain for a route (0.0 to 1.0+).
    fn set_route_gain(&self, conn_id: &ConnectionId, gain: f32) -> RouteResult<()>;

    /// Sets the mute state for a route.
    fn set_route_muted(&self, conn_id: &ConnectionId, muted: bool) -> RouteResult<()>;

    /// Checks if a route exists.
    fn has_route(&self, conn_id: &ConnectionId) -> bool;

    /// Calculates expected latency for a route.
    fn calculate_latency(&self, conn_id: &ConnectionId) -> LatencyReport;

    /// Returns the stream registry for querying active streams.
    fn stream_registry(&self) -> &Arc<StreamRegistry>;

    /// Returns the subscription manager for cross-node routing.
    fn subscription_manager(&self) -> &Arc<SubscriptionManager>;

    /// Returns the local node name.
    fn node_name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock implementation for testing
    struct MockRouteController {
        node: String,
        registry: Arc<StreamRegistry>,
        subscriptions: Arc<SubscriptionManager>,
    }

    impl MockRouteController {
        fn new() -> Self {
            Self {
                node: "test-node".to_string(),
                registry: Arc::new(StreamRegistry::new()),
                subscriptions: Arc::new(SubscriptionManager::with_defaults(
                    "test-node".to_string(),
                )),
            }
        }
    }

    impl RouteController for MockRouteController {
        fn add_route(&self, _conn_id: ConnectionId) -> RouteResult<usize> {
            Ok(0)
        }

        fn remove_route(&self, _conn_id: &ConnectionId) -> RouteResult<()> {
            Ok(())
        }

        fn set_route_gain(&self, _conn_id: &ConnectionId, _gain: f32) -> RouteResult<()> {
            Ok(())
        }

        fn set_route_muted(&self, _conn_id: &ConnectionId, _muted: bool) -> RouteResult<()> {
            Ok(())
        }

        fn has_route(&self, _conn_id: &ConnectionId) -> bool {
            false
        }

        fn calculate_latency(&self, _conn_id: &ConnectionId) -> LatencyReport {
            LatencyReport::default()
        }

        fn stream_registry(&self) -> &Arc<StreamRegistry> {
            &self.registry
        }

        fn subscription_manager(&self) -> &Arc<SubscriptionManager> {
            &self.subscriptions
        }

        fn node_name(&self) -> &str {
            &self.node
        }
    }

    #[test]
    fn mock_controller_works() {
        let controller = MockRouteController::new();
        let conn_id = ConnectionId::new("LOCAL", "dev", 1, "LOCAL", "out", 1);

        assert!(controller.add_route(conn_id.clone()).is_ok());
        assert!(!controller.has_route(&conn_id));
    }
}
