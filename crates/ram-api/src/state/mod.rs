//! Application state shared between API handlers.
//!
//! This module provides the shared state that handlers use to access
//! the audio engine, device manager, and other core services.

mod devices;
mod generators;

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use tokio::sync::broadcast;

use ram_core::{ConnectionId, RouteController, SubscriptionStats};

use crate::cross_node;
use crate::models::{
    ChannelInfo, DeviceInfo, DeviceType, GeneratorStatus, LatencyInfo, NodeInfo, RouteDefinition,
    StreamDirection, StreamInfo, SubscriptionInfo, SubscriptionState as ApiSubscriptionState,
    SubscriptionStatsResponse, WaveformType,
};
use crate::subscription_client::SubscriptionClient;
use crate::websocket::{SubscriptionNeededEvent, WsEvent};

/// Commands for device stream management.
///
/// These commands are sent from the API layer to the service layer
/// to trigger stream start/stop operations for metering.
#[derive(Debug, Clone)]
pub enum DeviceCommand {
    /// Start streams for a device (for metering).
    StartStreams {
        /// Device ID to start streams for.
        device_id: String,
        /// Device type (input, output, duplex).
        device_type: DeviceType,
    },
    /// Stop streams for a device.
    StopStreams {
        /// Device ID to stop streams for.
        device_id: String,
    },
}

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Local node information.
    inner: Arc<AppStateInner>,
}

pub(crate) struct AppStateInner {
    /// Local node info.
    pub local_node: RwLock<NodeInfo>,
    /// Known remote nodes.
    pub remote_nodes: RwLock<HashMap<String, NodeInfo>>,
    /// Local devices.
    pub devices: RwLock<HashMap<String, DeviceInfo>>,
    /// Channel labels by device ID.
    pub channel_labels: RwLock<HashMap<String, HashMap<u16, String>>>,
    /// Active routes.
    pub routes: RwLock<HashMap<String, RouteDefinition>>,
    /// Wave generators by "device_id:channel" key.
    /// NOT persisted (safety - always disabled on restart).
    pub generators: RwLock<HashMap<String, Arc<ram_core::WaveGeneratorConfig>>>,
    /// WebSocket event broadcaster.
    pub ws_broadcaster: broadcast::Sender<WsEvent>,
    /// Device command broadcaster for stream control.
    pub device_commands: broadcast::Sender<DeviceCommand>,
    /// Route controller for audio processor integration.
    /// None if running without audio processor (e.g., tests).
    pub route_controller: Option<Arc<dyn RouteController>>,
    /// Subscription client for cross-node subscriptions.
    pub subscription_client: SubscriptionClient,
    /// Shared HTTP client for remote node requests (connection pooling).
    pub http_client: reqwest::Client,
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
        let (device_commands, _) = broadcast::channel(64);
        let subscription_client = SubscriptionClient::new(vban_port);

        // Create shared HTTP client with connection pooling for remote node requests
        let http_client = reqwest::Client::builder()
            .pool_max_idle_per_host(4)
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            inner: Arc::new(AppStateInner {
                local_node: RwLock::new(local_node),
                remote_nodes: RwLock::new(HashMap::new()),
                devices: RwLock::new(HashMap::new()),
                channel_labels: RwLock::new(HashMap::new()),
                routes: RwLock::new(HashMap::new()),
                generators: RwLock::new(HashMap::new()),
                ws_broadcaster,
                device_commands,
                route_controller,
                subscription_client,
                http_client,
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

    /// Subscribes to device commands for stream control.
    #[must_use]
    pub fn subscribe_device_commands(&self) -> broadcast::Receiver<DeviceCommand> {
        self.inner.device_commands.subscribe()
    }

    /// Sends a device command.
    fn send_device_command(&self, command: DeviceCommand) {
        // Ignore send errors (no subscribers)
        let _ = self.inner.device_commands.send(command);
    }

    /// Returns the shared HTTP client for remote node requests.
    #[must_use]
    pub fn http_client(&self) -> &reqwest::Client {
        &self.inner.http_client
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

    /// Gets a remote node by ID or name.
    #[must_use]
    pub fn get_remote_node(&self, node_ref: &str) -> Option<NodeInfo> {
        let nodes = self.inner.remote_nodes.read();

        // Try exact ID match first
        if let Some(node) = nodes.get(node_ref) {
            return Some(node.clone());
        }

        // Try matching by name or partial ID
        for node in nodes.values() {
            if node.name == node_ref || node.id.starts_with(node_ref) {
                return Some(node.clone());
            }
            // Handle format "name@address"
            if node_ref.contains('@') {
                if let Some(name) = node_ref.split('@').next() {
                    if node.name == name {
                        return Some(node.clone());
                    }
                }
            }
        }

        None
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

    // --- Device Management (delegated to devices module) ---

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

    /// Attaches a device for routing.
    ///
    /// This also sends a command to start streams for the device,
    /// enabling metering even when no routes are connected.
    pub fn attach_device(
        &self,
        id: &str,
        display_name: Option<String>,
    ) -> Result<DeviceInfo, String> {
        let result = devices::attach_device(&self.inner.devices, id, display_name)?;

        // Send command to start streams for metering
        self.send_device_command(DeviceCommand::StartStreams {
            device_id: result.id.clone(),
            device_type: result.device_type,
        });

        Ok(result)
    }

    /// Detaches a device from routing.
    ///
    /// This also sends a command to stop streams for the device.
    pub fn detach_device(&self, id: &str) -> Result<DeviceInfo, String> {
        let result = devices::detach_device(&self.inner.devices, id)?;

        // Send command to stop streams
        self.send_device_command(DeviceCommand::StopStreams {
            device_id: result.id.clone(),
        });

        Ok(result)
    }

    /// Updates device settings.
    pub fn update_device(
        &self,
        id: &str,
        display_name: Option<String>,
        sample_rate: Option<u32>,
        buffer_size: Option<u32>,
    ) -> Result<DeviceInfo, String> {
        devices::update_device(
            &self.inner.devices,
            id,
            display_name,
            sample_rate,
            buffer_size,
        )
    }

    /// Gets channel information for a device.
    #[must_use]
    pub fn get_device_channels(&self, device_id: &str) -> Option<Vec<ChannelInfo>> {
        devices::get_device_channels(&self.inner.devices, &self.inner.channel_labels, device_id)
    }

    /// Sets a channel label.
    pub fn set_channel_label(
        &self,
        device_id: &str,
        channel: u16,
        label: String,
    ) -> Result<ChannelInfo, String> {
        devices::set_channel_label(
            &self.inner.devices,
            &self.inner.channel_labels,
            device_id,
            channel,
            label,
        )
    }

    /// Sets multiple channel labels at once.
    pub fn set_channel_labels(
        &self,
        device_id: &str,
        labels_map: HashMap<u16, String>,
    ) -> Result<Vec<ChannelInfo>, String> {
        devices::set_channel_labels(
            &self.inner.devices,
            &self.inner.channel_labels,
            device_id,
            labels_map,
        )
    }

    // --- Virtual Device Management (delegated to devices module) ---

    /// Lists all virtual devices.
    #[must_use]
    pub fn list_virtual_devices(&self) -> Vec<DeviceInfo> {
        self.inner
            .devices
            .read()
            .values()
            .filter(|d| d.is_virtual)
            .cloned()
            .collect()
    }

    /// Creates a virtual device.
    #[allow(clippy::too_many_arguments)]
    pub fn create_virtual_device(
        &self,
        name: String,
        input_channels: u16,
        output_channels: u16,
        sample_rate: u32,
        buffer_size: u32,
    ) -> Result<DeviceInfo, String> {
        devices::create_virtual_device(
            &self.inner.devices,
            name,
            input_channels,
            output_channels,
            sample_rate,
            buffer_size,
        )
    }

    /// Updates a virtual device.
    #[allow(clippy::too_many_arguments)]
    pub fn update_virtual_device(
        &self,
        id: &str,
        name: Option<String>,
        input_channels: Option<u16>,
        output_channels: Option<u16>,
        sample_rate: Option<u32>,
        buffer_size: Option<u32>,
    ) -> Result<DeviceInfo, String> {
        devices::update_virtual_device(
            &self.inner.devices,
            id,
            name,
            input_channels,
            output_channels,
            sample_rate,
            buffer_size,
        )
    }

    /// Deletes a virtual device.
    pub fn delete_virtual_device(&self, id: &str) -> Result<DeviceInfo, String> {
        devices::delete_virtual_device(&self.inner.devices, id)
    }

    // --- Wave Generator Management (delegated to generators module) ---

    /// Get generator status for all output channels of a device.
    #[must_use]
    pub fn get_generator_status(&self, device_id: &str) -> Vec<GeneratorStatus> {
        generators::get_generator_status(&self.inner.generators, &self.inner.devices, device_id)
    }

    /// Set generator for a specific channel.
    pub fn set_channel_generator(
        &self,
        device_id: &str,
        channel: u16,
        enabled: bool,
        waveform: WaveformType,
        frequency: u32,
        level_db: f32,
    ) {
        generators::set_channel_generator(
            &self.inner.generators,
            device_id,
            channel,
            enabled,
            waveform,
            frequency,
            level_db,
        );
    }

    /// Set generator for all output channels of a device.
    pub fn set_all_generators(
        &self,
        device_id: &str,
        enabled: bool,
        waveform: WaveformType,
        frequency: u32,
        level_db: f32,
    ) {
        generators::set_all_generators(
            &self.inner.generators,
            &self.inner.devices,
            device_id,
            enabled,
            waveform,
            frequency,
            level_db,
        );
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
    pub fn upsert_route(&self, route: RouteDefinition) -> Result<String, String> {
        let id = Self::route_id(&route);
        let is_cross_node = self.is_cross_node_route(&route);

        // If we have a route controller, apply the route to the audio processor
        if let Some(controller) = &self.inner.route_controller {
            let conn_id = ConnectionId::new(
                &route.source_node,
                &route.source_device,
                route.source_channel,
                &route.destination_node,
                &route.destination_device,
                route.destination_channel,
            );

            if !is_cross_node {
                if !controller.has_route(&conn_id) {
                    controller
                        .add_route(conn_id.clone())
                        .map_err(|e| e.to_string())?;
                }
                controller
                    .set_route_gain(&conn_id, route.volume)
                    .map_err(|e| e.to_string())?;
                controller
                    .set_route_muted(&conn_id, route.muted)
                    .map_err(|e| e.to_string())?;
            } else {
                tracing::info!(
                    "Cross-node route created: {} -> {} (requires VBAN subscription)",
                    route.source_node,
                    route.destination_node
                );
            }
        }

        self.inner.routes.write().insert(id.clone(), route.clone());

        // For cross-node routes, broadcast subscription needed event and initiate subscription
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

            let source_is_local = self.is_local_node(&route.source_node);
            let dest_is_local = self.is_local_node(&route.destination_node);

            tracing::debug!(
                "Cross-node routing decision: source_is_local={}, dest_is_local={}",
                source_is_local,
                dest_is_local
            );

            if !source_is_local {
                if let Some(source_node) = self.get_remote_node(&route.source_node) {
                    cross_node::initiate_subscription(
                        self.inner.subscription_client.clone(),
                        self.local_node(),
                        route.clone(),
                        source_node,
                        self.inner.route_controller.clone(),
                    );
                } else {
                    tracing::warn!(
                        "Cannot initiate subscription: source node not found: {}",
                        route.source_node
                    );
                }
            } else if !dest_is_local {
                if let Some(dest_node) = self.get_remote_node(&route.destination_node) {
                    cross_node::forward_route_to_destination(
                        self.local_node(),
                        route.clone(),
                        dest_node,
                    );
                } else {
                    tracing::warn!(
                        "Cannot forward route: destination node not found: {}",
                        route.destination_node
                    );
                }
            }
        }

        Ok(id)
    }

    /// Checks if a node identifier refers to the local node.
    ///
    /// Accepts "LOCAL", "local", the node's ID, or the node's name (case-insensitive for "local").
    #[must_use]
    pub fn is_local_node(&self, node_id: &str) -> bool {
        let local = self.local_node();
        let node_id_lower = node_id.to_lowercase();
        node_id_lower == "local" || node_id == local.id || node_id == local.name
    }

    /// Checks if a route crosses node boundaries.
    fn is_cross_node_route(&self, route: &RouteDefinition) -> bool {
        let local_id = self.local_node().id;
        let local_name = self.local_node().name;

        let source_is_local = route.source_node == "LOCAL"
            || route.source_node == local_id
            || route.source_node == local_name;

        let dest_is_local = route.destination_node == "LOCAL"
            || route.destination_node == local_id
            || route.destination_node == local_name;

        !source_is_local || !dest_is_local
    }

    /// Removes a route.
    pub fn remove_route(&self, id: &str) -> Result<Option<RouteDefinition>, String> {
        let route = self.inner.routes.write().remove(id);

        if let (Some(controller), Some(ref route_def)) = (&self.inner.route_controller, &route) {
            let conn_id = ConnectionId::new(
                &route_def.source_node,
                &route_def.source_device,
                route_def.source_channel,
                &route_def.destination_node,
                &route_def.destination_device,
                route_def.destination_channel,
            );
            let _ = controller.remove_route(&conn_id);
        }

        Ok(route)
    }

    /// Updates the volume for a route.
    pub fn set_route_volume(&self, id: &str, volume: f32) -> Result<(), String> {
        let mut routes = self.inner.routes.write();
        let route = routes
            .get_mut(id)
            .ok_or_else(|| format!("Route not found: {id}"))?;

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
    pub fn set_route_muted(&self, id: &str, muted: bool) -> Result<(), String> {
        let mut routes = self.inner.routes.write();
        let route = routes
            .get_mut(id)
            .ok_or_else(|| format!("Route not found: {id}"))?;

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
    #[must_use]
    pub fn all_streams(&self) -> Vec<StreamInfo> {
        let Some(controller) = &self.inner.route_controller else {
            return Vec::new();
        };

        let registry = controller.stream_registry();
        let mut streams = Vec::new();

        registry.for_each_input(|stream| {
            streams.push(StreamInfo {
                device_id: stream.device_id().to_string(),
                device_name: stream.device_id().to_string(),
                direction: StreamDirection::Input,
                channels: stream.config().channels as u16,
                sample_rate: stream.config().sample_rate,
                running: stream.is_running(),
            });
        });

        registry.for_each_output(|stream| {
            streams.push(StreamInfo {
                device_id: stream.device_id().to_string(),
                device_name: stream.device_id().to_string(),
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
    #[must_use]
    pub fn all_subscriptions(&self) -> Vec<SubscriptionInfo> {
        let Some(controller) = &self.inner.route_controller else {
            return Vec::new();
        };

        let manager = controller.subscription_manager();
        let mut subscriptions = Vec::new();

        let convert_state = |state: ram_core::SubscriptionState| -> ApiSubscriptionState {
            match state {
                ram_core::SubscriptionState::Pending => ApiSubscriptionState::Pending,
                ram_core::SubscriptionState::Active => ApiSubscriptionState::Active,
                ram_core::SubscriptionState::Failed
                | ram_core::SubscriptionState::Paused
                | ram_core::SubscriptionState::Closing => ApiSubscriptionState::Failed,
            }
        };

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
    #[must_use]
    pub fn get_route_latency(&self, id: &str) -> Option<LatencyInfo> {
        let route = self.get_route(id)?;

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

        let is_local = route.source_node == "LOCAL" && route.destination_node == "LOCAL";

        if is_local {
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
            Some(LatencyInfo {
                route_id: id.to_string(),
                input_buffer_ms: 5.33,
                ring_buffer_ms: 42.67,
                output_buffer_ms: 5.33,
                network_ms: 11.17,
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
mod tests;
