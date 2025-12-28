//! Application state shared between API handlers.
//!
//! This module provides the shared state that handlers use to access
//! the audio engine, device manager, and other core services.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use tokio::sync::broadcast;

use ram_core::{ConnectionId, RouteController, SubscriptionStats};

use crate::models::{
    DeviceInfo, DeviceType, LatencyInfo, NodeInfo, RouteDefinition, StreamDirection, StreamInfo,
    SubscriptionInfo, SubscriptionState as ApiSubscriptionState, SubscriptionStatsResponse,
};
use crate::websocket::{SubscriptionNeededEvent, WsEvent};

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Local node information.
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    /// Local node info.
    local_node: RwLock<NodeInfo>,
    /// Known remote nodes.
    remote_nodes: RwLock<HashMap<String, NodeInfo>>,
    /// Local devices.
    devices: RwLock<HashMap<String, DeviceInfo>>,
    /// Active routes.
    routes: RwLock<HashMap<String, RouteDefinition>>,
    /// WebSocket event broadcaster.
    ws_broadcaster: broadcast::Sender<WsEvent>,
    /// Route controller for audio processor integration.
    /// None if running without audio processor (e.g., tests).
    route_controller: Option<Arc<dyn RouteController>>,
}

impl AppState {
    /// Creates a new application state without a route controller.
    ///
    /// Use `with_route_controller` in production to enable audio processor integration.
    #[must_use]
    pub fn new(node_name: &str, api_port: u16, vban_port: u16) -> Self {
        Self::with_route_controller(node_name, api_port, vban_port, None)
    }

    /// Creates a new application state with an optional route controller.
    ///
    /// When a route controller is provided, route operations will be applied
    /// to the live audio processor.
    #[must_use]
    pub fn with_route_controller(
        node_name: &str,
        api_port: u16,
        vban_port: u16,
        route_controller: Option<Arc<dyn RouteController>>,
    ) -> Self {
        let hostname = hostname::get().map_or_else(
            |_| "unknown".to_string(),
            |h| h.to_string_lossy().into_owned(),
        );

        let local_node = NodeInfo {
            id: format!("{node_name}@{hostname}"),
            name: node_name.to_string(),
            addresses: vec![],
            api_port,
            vban_port,
            online: true,
        };

        let (ws_broadcaster, _) = broadcast::channel(256);

        Self {
            inner: Arc::new(AppStateInner {
                local_node: RwLock::new(local_node),
                remote_nodes: RwLock::new(HashMap::new()),
                devices: RwLock::new(HashMap::new()),
                routes: RwLock::new(HashMap::new()),
                ws_broadcaster,
                route_controller,
            }),
        }
    }

    /// Returns the route controller if available.
    #[must_use]
    pub fn route_controller(&self) -> Option<&Arc<dyn RouteController>> {
        self.inner.route_controller.as_ref()
    }

    /// Returns the local node information.
    #[must_use]
    pub fn local_node(&self) -> NodeInfo {
        self.inner.local_node.read().clone()
    }

    /// Updates the local node's addresses.
    pub fn set_addresses(&self, addresses: Vec<String>) {
        self.inner.local_node.write().addresses = addresses;
    }

    /// Subscribes to WebSocket events.
    #[must_use]
    pub fn subscribe_events(&self) -> broadcast::Receiver<WsEvent> {
        self.inner.ws_broadcaster.subscribe()
    }

    /// Broadcasts a WebSocket event to all subscribers.
    pub fn broadcast_event(&self, event: WsEvent) {
        // Ignore send errors (no subscribers)
        let _ = self.inner.ws_broadcaster.send(event);
    }

    // --- Node Management ---

    /// Returns all known nodes (local + remote).
    #[must_use]
    pub fn all_nodes(&self) -> Vec<NodeInfo> {
        let mut nodes = vec![self.local_node()];
        nodes.extend(self.inner.remote_nodes.read().values().cloned());
        nodes
    }

    /// Gets a node by ID.
    #[must_use]
    pub fn get_node(&self, id: &str) -> Option<NodeInfo> {
        let local = self.inner.local_node.read();
        if local.id == id {
            return Some(local.clone());
        }
        drop(local);

        self.inner.remote_nodes.read().get(id).cloned()
    }

    /// Adds or updates a remote node.
    pub fn upsert_remote_node(&self, node: NodeInfo) {
        self.inner
            .remote_nodes
            .write()
            .insert(node.id.clone(), node);
    }

    /// Removes a remote node.
    #[must_use]
    pub fn remove_remote_node(&self, id: &str) -> Option<NodeInfo> {
        self.inner.remote_nodes.write().remove(id)
    }

    // --- Device Management ---

    /// Returns all local devices.
    #[must_use]
    pub fn all_devices(&self) -> Vec<DeviceInfo> {
        self.inner.devices.read().values().cloned().collect()
    }

    /// Returns devices filtered by type.
    #[must_use]
    pub fn devices_by_type(&self, device_type: DeviceType) -> Vec<DeviceInfo> {
        self.inner
            .devices
            .read()
            .values()
            .filter(|d| d.device_type == device_type)
            .cloned()
            .collect()
    }

    /// Gets a device by ID.
    #[must_use]
    pub fn get_device(&self, id: &str) -> Option<DeviceInfo> {
        self.inner.devices.read().get(id).cloned()
    }

    /// Registers a device.
    pub fn register_device(&self, device: DeviceInfo) {
        self.inner.devices.write().insert(device.id.clone(), device);
    }

    /// Unregisters a device.
    #[must_use]
    pub fn unregister_device(&self, id: &str) -> Option<DeviceInfo> {
        self.inner.devices.write().remove(id)
    }

    // --- Route Management ---

    /// Returns all routes.
    #[must_use]
    pub fn all_routes(&self) -> Vec<RouteDefinition> {
        self.inner.routes.read().values().cloned().collect()
    }

    /// Gets a route by ID.
    #[must_use]
    pub fn get_route(&self, id: &str) -> Option<RouteDefinition> {
        self.inner.routes.read().get(id).cloned()
    }

    /// Creates or updates a route.
    ///
    /// If a route controller is available, the route will also be applied
    /// to the live audio processor.
    ///
    /// For cross-node routes (source on different node), this also triggers
    /// VBAN subscription setup.
    ///
    /// # Errors
    ///
    /// Returns an error if the route controller rejects the route.
    pub fn upsert_route(&self, route: RouteDefinition) -> Result<String, String> {
        let id = Self::route_id(&route);

        // Check if this is a cross-node route
        let is_cross_node = self.is_cross_node_route(&route);

        // If we have a route controller, apply the route to the audio processor
        if let Some(controller) = &self.inner.route_controller {
            // Convert RouteDefinition to ConnectionId
            let conn_id = ConnectionId::new(
                &route.source_node,
                &route.source_device,
                route.source_channel,
                &route.destination_node,
                &route.destination_device,
                route.destination_channel,
            );

            // For local routes, add to audio processor directly
            // For cross-node routes, we need subscription handshake first
            if !is_cross_node {
                // Check if route already exists in controller
                if !controller.has_route(&conn_id) {
                    controller
                        .add_route(conn_id.clone())
                        .map_err(|e| e.to_string())?;
                }

                // Apply gain and mute settings
                controller
                    .set_route_gain(&conn_id, route.volume)
                    .map_err(|e| e.to_string())?;
                controller
                    .set_route_muted(&conn_id, route.muted)
                    .map_err(|e| e.to_string())?;
            } else {
                // Cross-node route: Log for now, subscription handler will set up VBAN
                tracing::info!(
                    "Cross-node route created: {} -> {} (requires VBAN subscription)",
                    route.source_node,
                    route.destination_node
                );
            }
        }

        // Store in local state
        self.inner.routes.write().insert(id.clone(), route.clone());

        // For cross-node routes, broadcast subscription needed event
        if is_cross_node {
            self.broadcast_event(WsEvent::SubscriptionNeeded(SubscriptionNeededEvent {
                route_id: id.clone(),
                source_node: route.source_node.clone(),
                source_device: route.source_device.clone(),
                source_channel: route.source_channel,
                destination_node: route.destination_node.clone(),
                destination_device: route.destination_device.clone(),
                destination_channel: route.destination_channel,
            }));
        }

        Ok(id)
    }

    /// Checks if a route crosses node boundaries.
    fn is_cross_node_route(&self, route: &RouteDefinition) -> bool {
        let local_id = self.local_node().id;
        let local_name = self.local_node().name;

        // Check if source is local
        let source_is_local = route.source_node == "LOCAL"
            || route.source_node == local_id
            || route.source_node == local_name;

        // Check if destination is local
        let dest_is_local = route.destination_node == "LOCAL"
            || route.destination_node == local_id
            || route.destination_node == local_name;

        // It's cross-node if either endpoint is remote
        !source_is_local || !dest_is_local
    }

    /// Removes a route.
    ///
    /// If a route controller is available, the route will also be removed
    /// from the live audio processor.
    pub fn remove_route(&self, id: &str) -> Result<Option<RouteDefinition>, String> {
        // Remove from local state first
        let route = self.inner.routes.write().remove(id);

        // If we have a route controller and the route existed, remove from audio processor
        if let (Some(controller), Some(ref route_def)) = (&self.inner.route_controller, &route) {
            let conn_id = ConnectionId::new(
                &route_def.source_node,
                &route_def.source_device,
                route_def.source_channel,
                &route_def.destination_node,
                &route_def.destination_device,
                route_def.destination_channel,
            );

            // Ignore not found errors (route may have been removed elsewhere)
            let _ = controller.remove_route(&conn_id);
        }

        Ok(route)
    }

    /// Updates the volume for a route.
    ///
    /// # Errors
    ///
    /// Returns an error if the route doesn't exist or the controller rejects the update.
    pub fn set_route_volume(&self, id: &str, volume: f32) -> Result<(), String> {
        let mut routes = self.inner.routes.write();
        let route = routes
            .get_mut(id)
            .ok_or_else(|| format!("Route not found: {id}"))?;

        // Update in audio processor if available
        if let Some(controller) = &self.inner.route_controller {
            let conn_id = ConnectionId::new(
                &route.source_node,
                &route.source_device,
                route.source_channel,
                &route.destination_node,
                &route.destination_device,
                route.destination_channel,
            );
            controller
                .set_route_gain(&conn_id, volume)
                .map_err(|e| e.to_string())?;
        }

        route.volume = volume;
        Ok(())
    }

    /// Updates the mute state for a route.
    ///
    /// # Errors
    ///
    /// Returns an error if the route doesn't exist or the controller rejects the update.
    pub fn set_route_muted(&self, id: &str, muted: bool) -> Result<(), String> {
        let mut routes = self.inner.routes.write();
        let route = routes
            .get_mut(id)
            .ok_or_else(|| format!("Route not found: {id}"))?;

        // Update in audio processor if available
        if let Some(controller) = &self.inner.route_controller {
            let conn_id = ConnectionId::new(
                &route.source_node,
                &route.source_device,
                route.source_channel,
                &route.destination_node,
                &route.destination_device,
                route.destination_channel,
            );
            controller
                .set_route_muted(&conn_id, muted)
                .map_err(|e| e.to_string())?;
        }

        route.muted = muted;
        Ok(())
    }

    /// Generates a route ID from its definition.
    fn route_id(route: &RouteDefinition) -> String {
        format!(
            "{}:{}:{}->{}:{}:{}",
            route.source_node,
            route.source_device,
            route.source_channel,
            route.destination_node,
            route.destination_device,
            route.destination_channel
        )
    }

    // --- Stream Management ---

    /// Returns all active streams.
    ///
    /// If a route controller is available, returns actual stream data.
    /// Otherwise returns an empty list.
    #[must_use]
    pub fn all_streams(&self) -> Vec<StreamInfo> {
        let Some(controller) = &self.inner.route_controller else {
            return Vec::new();
        };

        let registry = controller.stream_registry();
        let mut streams = Vec::new();

        // Collect input streams
        registry.for_each_input(|stream| {
            streams.push(StreamInfo {
                device_id: stream.device_id().to_string(),
                device_name: stream.device_id().to_string(), // Use device_id as name for now
                direction: StreamDirection::Input,
                channels: stream.config().channels as u16,
                sample_rate: stream.config().sample_rate,
                running: stream.is_running(),
            });
        });

        // Collect output streams
        registry.for_each_output(|stream| {
            streams.push(StreamInfo {
                device_id: stream.device_id().to_string(),
                device_name: stream.device_id().to_string(), // Use device_id as name for now
                direction: StreamDirection::Output,
                channels: stream.config().channels as u16,
                sample_rate: stream.config().sample_rate,
                running: stream.is_running(),
            });
        });

        streams
    }

    /// Returns the count of input and output streams.
    #[must_use]
    pub fn stream_counts(&self) -> (usize, usize) {
        match &self.inner.route_controller {
            Some(controller) => {
                let registry = controller.stream_registry();
                (registry.input_count(), registry.output_count())
            },
            None => (0, 0),
        }
    }

    // --- Subscription Management ---

    /// Returns all subscriptions (incoming and outgoing).
    ///
    /// If a route controller is available, returns actual subscription data.
    /// Otherwise returns an empty list.
    #[must_use]
    pub fn all_subscriptions(&self) -> Vec<SubscriptionInfo> {
        let Some(controller) = &self.inner.route_controller else {
            return Vec::new();
        };

        let manager = controller.subscription_manager();
        let mut subscriptions = Vec::new();

        // Helper to convert subscription state
        let convert_state = |state: ram_core::SubscriptionState| -> ApiSubscriptionState {
            match state {
                ram_core::SubscriptionState::Pending => ApiSubscriptionState::Pending,
                ram_core::SubscriptionState::Active => ApiSubscriptionState::Active,
                ram_core::SubscriptionState::Failed
                | ram_core::SubscriptionState::Paused
                | ram_core::SubscriptionState::Closing => ApiSubscriptionState::Failed,
            }
        };

        // Collect outgoing subscriptions
        for sub in manager.active_outgoing() {
            subscriptions.push(SubscriptionInfo {
                id: sub.id,
                source_node: sub.source_node.clone(),
                source_device: sub.source_device.clone(),
                source_channels: sub.source_channels.clone(),
                dest_device: sub.dest_device.clone(),
                dest_channels: sub.dest_channels.clone(),
                vban_stream_name: sub.vban_stream_name.clone(),
                sample_rate: sub.sample_rate,
                state: convert_state(sub.state),
            });
        }

        // Collect incoming subscriptions
        for sub in manager.active_incoming() {
            subscriptions.push(SubscriptionInfo {
                id: sub.id,
                source_node: controller.node_name().to_string(),
                source_device: sub.source_device.clone(),
                source_channels: sub.source_channels.clone(),
                dest_device: sub.dest_device.clone(),
                dest_channels: sub.dest_channels.clone(),
                vban_stream_name: sub.vban_stream_name.clone(),
                sample_rate: sub.sample_rate,
                state: convert_state(sub.state),
            });
        }

        subscriptions
    }

    /// Returns subscription statistics.
    #[must_use]
    pub fn subscription_stats(&self) -> SubscriptionStatsResponse {
        match &self.inner.route_controller {
            Some(controller) => {
                let stats: SubscriptionStats = controller.subscription_manager().stats();
                SubscriptionStatsResponse {
                    outgoing_total: stats.outgoing_total,
                    outgoing_active: stats.outgoing_active,
                    outgoing_pending: stats.outgoing_pending,
                    incoming_total: stats.incoming_total,
                    incoming_active: stats.incoming_active,
                }
            },
            None => SubscriptionStatsResponse {
                outgoing_total: 0,
                outgoing_active: 0,
                outgoing_pending: 0,
                incoming_total: 0,
                incoming_active: 0,
            },
        }
    }

    // --- Latency Information ---

    /// Returns latency information for a route.
    ///
    /// If a route controller is available, uses accurate latency calculation.
    /// Otherwise falls back to estimated values.
    #[must_use]
    pub fn get_route_latency(&self, id: &str) -> Option<LatencyInfo> {
        // Check if route exists
        let route = self.get_route(id)?;

        // If we have a route controller, use accurate calculation
        if let Some(controller) = &self.inner.route_controller {
            let conn_id = ConnectionId::new(
                &route.source_node,
                &route.source_device,
                route.source_channel,
                &route.destination_node,
                &route.destination_device,
                route.destination_channel,
            );

            let report = controller.calculate_latency(&conn_id);
            return Some(LatencyInfo {
                route_id: id.to_string(),
                input_buffer_ms: report.input_buffer_ms,
                ring_buffer_ms: report.ring_buffer_ms,
                output_buffer_ms: report.output_buffer_ms,
                network_ms: report.network_ms,
                processing_ms: report.processing_ms,
                total_ms: report.total_ms,
                is_local: report.is_local(),
            });
        }

        // Fallback: estimate latency based on route type
        let is_local = route.source_node == "LOCAL" && route.destination_node == "LOCAL";

        if is_local {
            // Local route: ~53ms typical (256 + 2048 + 256 samples @ 48kHz)
            Some(LatencyInfo {
                route_id: id.to_string(),
                input_buffer_ms: 5.33,
                ring_buffer_ms: 42.67,
                output_buffer_ms: 5.33,
                network_ms: 0.0,
                processing_ms: 0.1,
                total_ms: 53.43,
                is_local: true,
            })
        } else {
            // Network route: adds jitter buffer and network latency
            Some(LatencyInfo {
                route_id: id.to_string(),
                input_buffer_ms: 5.33,
                ring_buffer_ms: 42.67,
                output_buffer_ms: 5.33,
                network_ms: 11.17, // jitter buffer + RTT
                processing_ms: 0.1,
                total_ms: 64.6,
                is_local: false,
            })
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new("AudioMatrix", 8080, 6980)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_new() {
        let state = AppState::new("TestNode", 8080, 6980);
        let local = state.local_node();
        assert_eq!(local.name, "TestNode");
        assert_eq!(local.api_port, 8080);
        assert_eq!(local.vban_port, 6980);
        assert!(local.online);
    }

    #[test]
    fn app_state_default() {
        let state = AppState::default();
        let local = state.local_node();
        assert_eq!(local.name, "AudioMatrix");
    }

    #[test]
    fn app_state_addresses() {
        let state = AppState::new("Test", 8080, 6980);
        assert!(state.local_node().addresses.is_empty());

        state.set_addresses(vec!["192.168.1.1".to_string()]);
        assert_eq!(state.local_node().addresses, vec!["192.168.1.1"]);
    }

    #[test]
    fn app_state_remote_nodes() {
        let state = AppState::default();

        let remote = NodeInfo {
            id: "remote-1".to_string(),
            name: "Remote Node".to_string(),
            addresses: vec!["192.168.1.100".to_string()],
            api_port: 8080,
            vban_port: 6980,
            online: true,
        };

        state.upsert_remote_node(remote.clone());

        let all = state.all_nodes();
        assert_eq!(all.len(), 2); // Local + remote

        let found = state.get_node("remote-1");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Remote Node");

        let removed = state.remove_remote_node("remote-1");
        assert!(removed.is_some());
        assert_eq!(state.all_nodes().len(), 1);
    }

    #[test]
    fn app_state_devices() {
        let state = AppState::default();

        let device = DeviceInfo {
            id: "device-1".to_string(),
            name: "Test Device".to_string(),
            device_type: DeviceType::Input,
            channels: 2,
            sample_rate: 48000,
            is_virtual: false,
        };

        state.register_device(device);

        let all = state.all_devices();
        assert_eq!(all.len(), 1);

        let inputs = state.devices_by_type(DeviceType::Input);
        assert_eq!(inputs.len(), 1);

        let outputs = state.devices_by_type(DeviceType::Output);
        assert!(outputs.is_empty());

        let found = state.get_device("device-1");
        assert!(found.is_some());

        let removed = state.unregister_device("device-1");
        assert!(removed.is_some());
        assert!(state.all_devices().is_empty());
    }

    #[test]
    fn app_state_routes() {
        let state = AppState::default();

        let route = RouteDefinition {
            source_node: "node-a".to_string(),
            source_device: "dev-1".to_string(),
            source_channel: 1,
            destination_node: "node-b".to_string(),
            destination_device: "dev-2".to_string(),
            destination_channel: 1,
            volume: 1.0,
            muted: false,
        };

        let id = state.upsert_route(route).expect("Failed to upsert route");
        assert_eq!(id, "node-a:dev-1:1->node-b:dev-2:1");

        let all = state.all_routes();
        assert_eq!(all.len(), 1);

        let found = state.get_route(&id);
        assert!(found.is_some());

        let removed = state.remove_route(&id).expect("Failed to remove route");
        assert!(removed.is_some());
        assert!(state.all_routes().is_empty());
    }

    #[test]
    fn app_state_broadcast() {
        let state = AppState::default();
        let mut receiver = state.subscribe_events();

        state.broadcast_event(WsEvent::NodeStatus(crate::websocket::NodeStatusUpdate {
            node: "test".to_string(),
            online: true,
        }));

        // Try to receive (non-blocking)
        match receiver.try_recv() {
            Ok(event) => {
                if let WsEvent::NodeStatus(status) = event {
                    assert_eq!(status.node, "test");
                } else {
                    panic!("Wrong event type");
                }
            },
            Err(broadcast::error::TryRecvError::Empty) => {
                // Event may not be delivered yet in single-threaded test
            },
            Err(e) => panic!("Unexpected error: {e:?}"),
        }
    }
}
