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

use std::net::SocketAddr;
use std::sync::Arc;

use crate::latency::LatencyReport;
use crate::metering::MeterLevels;
use crate::stream_registry::StreamRegistry;
use crate::subscription::SubscriptionId;
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

    /// Starts a VBAN sender for an outgoing subscription.
    ///
    /// This is called when a remote node requests a subscription and
    /// we need to start sending audio via VBAN.
    ///
    /// # Arguments
    ///
    /// * `subscription_id` - The subscription ID to associate with this sender
    /// * `stream_name` - VBAN stream name
    /// * `destination` - Destination socket address
    /// * `source_buffers` - Buffer indices to read audio from
    /// * `channels` - Number of channels
    ///
    /// # Errors
    ///
    /// Returns an error if the sender cannot be started.
    fn start_vban_sender(
        &self,
        subscription_id: SubscriptionId,
        stream_name: String,
        destination: SocketAddr,
        source_buffers: Vec<usize>,
        channels: u8,
    ) -> RouteResult<()>;

    /// Stops a VBAN sender for a subscription.
    fn stop_vban_sender(&self, subscription_id: SubscriptionId) -> RouteResult<()>;

    /// Ensures an input stream is running for the given device and returns buffer indices.
    ///
    /// This is called when a subscription is created for a device to ensure
    /// audio is being captured from that device.
    ///
    /// # Returns
    ///
    /// Returns the buffer indices for the requested channels, or an error if
    /// the device cannot be started.
    fn ensure_input_stream(&self, device_id: &str, channels: &[u16]) -> RouteResult<Vec<usize>>;

    /// Registers buffer indices for a VBAN stream (receiver side).
    ///
    /// This is called when an outgoing subscription is confirmed and we know
    /// the VBAN stream name the source will use to send audio to us.
    ///
    /// # Arguments
    ///
    /// * `stream_name` - The VBAN stream name from the subscription acknowledgment
    /// * `buffer_indices` - Buffer indices where received audio should be written
    fn register_vban_stream_buffers(&self, stream_name: &str, buffer_indices: Vec<usize>);

    /// Unregisters a VBAN stream (receiver side).
    ///
    /// Call this when a subscription is terminated.
    fn unregister_vban_stream(&self, stream_name: &str);

    /// Allocates buffers for receiving audio from a cross-node route.
    ///
    /// This allocates ring buffers that will be written to by the VBAN receiver
    /// and read from by the output device.
    ///
    /// # Arguments
    ///
    /// * `dest_device` - Destination device ID
    /// * `channels` - Channel indices (1-based) for the destination
    ///
    /// # Returns
    ///
    /// Buffer indices for the allocated buffers.
    fn allocate_receive_buffers(
        &self,
        dest_device: &str,
        channels: &[u16],
    ) -> RouteResult<Vec<usize>>;

    /// Ensures an output stream is running for the given device.
    ///
    /// This is called when a cross-node route targets a local output device,
    /// to ensure the device is playing audio from the routing table.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The output device ID to ensure is running
    ///
    /// # Errors
    ///
    /// Returns an error if the device cannot be started.
    fn ensure_output_stream(&self, device_id: &str) -> RouteResult<()>;

    /// Returns meter levels for all input devices.
    ///
    /// Returns a vector of (device_id, channel_levels) pairs.
    /// The levels are reset after reading.
    fn all_input_meters(&self) -> Vec<(String, Vec<MeterLevels>)>;

    /// Returns meter levels for all output devices.
    ///
    /// Returns a vector of (device_id, channel_levels) pairs.
    /// The levels are reset after reading.
    fn all_output_meters(&self) -> Vec<(String, Vec<MeterLevels>)>;

    /// Returns diagnostic information about available audio devices.
    ///
    /// This is useful for debugging device detection issues.
    fn audio_diagnostics(&self) -> AudioDiagnostics;

    /// Attempts to diagnose why a device stream cannot be started.
    ///
    /// Returns detailed diagnostic information about what went wrong.
    fn diagnose_device(&self, device_id: &str) -> DeviceDiagnostic;
}

/// Audio system diagnostics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AudioDiagnostics {
    /// Available audio hosts (e.g., ASIO, WASAPI).
    pub hosts: Vec<String>,
    /// Input devices visible to cpal.
    pub input_devices: Vec<CpalDeviceInfo>,
    /// Output devices visible to cpal.
    pub output_devices: Vec<CpalDeviceInfo>,
}

/// Information about a cpal device.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CpalDeviceInfo {
    /// Host name (e.g., "ASIO", "WASAPI").
    pub host: String,
    /// Device name as reported by cpal.
    pub name: String,
    /// Whether this device supports input.
    pub has_input: bool,
    /// Whether this device supports output.
    pub has_output: bool,
}

/// Diagnostic information for a specific device.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceDiagnostic {
    /// The device ID being diagnosed.
    pub device_id: String,
    /// Whether the device was found.
    pub found: bool,
    /// Whether an input stream could be started.
    pub input_stream_result: Option<String>,
    /// Whether an output stream could be started.
    pub output_stream_result: Option<String>,
    /// Detailed error message if any operation failed.
    pub error: Option<String>,
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

        fn start_vban_sender(
            &self,
            _subscription_id: SubscriptionId,
            _stream_name: String,
            _destination: SocketAddr,
            _source_buffers: Vec<usize>,
            _channels: u8,
        ) -> RouteResult<()> {
            Ok(())
        }

        fn stop_vban_sender(&self, _subscription_id: SubscriptionId) -> RouteResult<()> {
            Ok(())
        }

        fn ensure_input_stream(
            &self,
            _device_id: &str,
            channels: &[u16],
        ) -> RouteResult<Vec<usize>> {
            // Return mock buffer indices (channel number as index)
            Ok(channels.iter().map(|&ch| ch as usize).collect())
        }

        fn register_vban_stream_buffers(&self, _stream_name: &str, _buffer_indices: Vec<usize>) {
            // Mock: do nothing
        }

        fn unregister_vban_stream(&self, _stream_name: &str) {
            // Mock: do nothing
        }

        fn allocate_receive_buffers(
            &self,
            _dest_device: &str,
            channels: &[u16],
        ) -> RouteResult<Vec<usize>> {
            // Return mock buffer indices
            Ok(channels.iter().map(|&ch| ch as usize).collect())
        }

        fn ensure_output_stream(&self, _device_id: &str) -> RouteResult<()> {
            // Mock: do nothing
            Ok(())
        }

        fn all_input_meters(&self) -> Vec<(String, Vec<MeterLevels>)> {
            Vec::new()
        }

        fn all_output_meters(&self) -> Vec<(String, Vec<MeterLevels>)> {
            Vec::new()
        }

        fn audio_diagnostics(&self) -> AudioDiagnostics {
            AudioDiagnostics {
                hosts: vec!["MockHost".to_string()],
                input_devices: vec![],
                output_devices: vec![],
            }
        }

        fn diagnose_device(&self, device_id: &str) -> DeviceDiagnostic {
            DeviceDiagnostic {
                device_id: device_id.to_string(),
                found: false,
                input_stream_result: None,
                output_stream_result: None,
                error: Some("Mock controller - no real devices".to_string()),
            }
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
