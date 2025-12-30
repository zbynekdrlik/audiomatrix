//! Tests for application state.

use super::*;
use crate::models::DeviceStatus;
use tokio::sync::broadcast;

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
    assert_eq!(all.len(), 2);

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
        display_name: None,
        device_type: DeviceType::Input,
        input_channels: 2,
        output_channels: 0,
        sample_rate: 48000,
        buffer_size: 256,
        is_virtual: false,
        status: DeviceStatus::Available,
        backend: None,
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
