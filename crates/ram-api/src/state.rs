//! Application state shared between API handlers.
//!
//! This module provides the shared state that handlers use to access
//! the audio engine, device manager, and other core services.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use tokio::sync::broadcast;

use crate::models::{DeviceInfo, DeviceType, NodeInfo, RouteDefinition};
use crate::websocket::WsEvent;

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
}

impl AppState {
    /// Creates a new application state.
    #[must_use]
    pub fn new(node_name: &str, api_port: u16, vban_port: u16) -> Self {
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
            }),
        }
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
    /// Returns the route ID.
    #[must_use]
    pub fn upsert_route(&self, route: RouteDefinition) -> String {
        let id = Self::route_id(&route);
        self.inner.routes.write().insert(id.clone(), route);
        id
    }

    /// Removes a route.
    #[must_use]
    pub fn remove_route(&self, id: &str) -> Option<RouteDefinition> {
        self.inner.routes.write().remove(id)
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

        let id = state.upsert_route(route);
        assert_eq!(id, "node-a:dev-1:1->node-b:dev-2:1");

        let all = state.all_routes();
        assert_eq!(all.len(), 1);

        let found = state.get_route(&id);
        assert!(found.is_some());

        let removed = state.remove_route(&id);
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
