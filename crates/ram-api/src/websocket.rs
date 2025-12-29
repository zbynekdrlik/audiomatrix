//! WebSocket handler for real-time events.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{debug, warn};

use crate::state::AppState;

/// WebSocket event types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WsEvent {
    /// Metering update.
    #[serde(rename = "metering")]
    Metering(MeteringUpdate),
    /// Route changed.
    #[serde(rename = "route_changed")]
    RouteChanged(RouteUpdate),
    /// Node discovered/lost.
    #[serde(rename = "node_status")]
    NodeStatus(NodeStatusUpdate),
    /// Device state changed.
    #[serde(rename = "device_status")]
    DeviceStatus(DeviceStatusUpdate),
    /// Subscription needed for cross-node route.
    #[serde(rename = "subscription_needed")]
    SubscriptionNeeded(SubscriptionNeededEvent),
    /// Error occurred.
    #[serde(rename = "error")]
    Error(ErrorUpdate),
}

/// Metering update data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeteringUpdate {
    /// Node identifier.
    pub node: String,
    /// Device identifier.
    pub device: String,
    /// Channel levels (dBFS).
    pub levels: Vec<f32>,
}

/// Route update data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteUpdate {
    /// Action (added/removed/modified).
    pub action: String,
    /// Route identifier.
    pub route_id: String,
}

/// Node status update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatusUpdate {
    /// Node identifier.
    pub node: String,
    /// Whether node is online.
    pub online: bool,
}

/// Device status update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatusUpdate {
    /// Device identifier.
    pub device: String,
    /// Device state (idle, running, error, disconnected).
    pub state: String,
}

/// Error update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorUpdate {
    /// Error code.
    pub code: String,
    /// Error message.
    pub message: String,
}

/// Subscription needed event for cross-node routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionNeededEvent {
    /// Route identifier.
    pub route_id: String,
    /// Source node identifier.
    pub source_node: String,
    /// Source device identifier.
    pub source_device: String,
    /// Source channel (1-based).
    pub source_channel: u16,
    /// Destination node identifier.
    pub destination_node: String,
    /// Destination device identifier.
    pub destination_device: String,
    /// Destination channel (1-based).
    pub destination_channel: u16,
}

/// WebSocket command from client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", content = "data")]
pub enum WsCommand {
    /// Subscribe to metering for a device.
    #[serde(rename = "subscribe_metering")]
    SubscribeMetering { node: String, device: String },
    /// Unsubscribe from metering.
    #[serde(rename = "unsubscribe_metering")]
    UnsubscribeMetering { node: String, device: String },
    /// Ping for keepalive.
    #[serde(rename = "ping")]
    Ping,
}

/// WebSocket upgrade handler.
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Handle WebSocket connection.
async fn handle_socket(socket: WebSocket, state: AppState) {
    debug!("WebSocket client connected");

    let (mut sender, mut receiver) = socket.split();

    // Subscribe to broadcast events
    let mut event_rx = state.subscribe_events();

    // Send initial connection acknowledgment
    let ack = WsEvent::NodeStatus(NodeStatusUpdate {
        node: state.local_node().id,
        online: true,
    });

    if let Ok(json) = serde_json::to_string(&ack) {
        if sender.send(Message::Text(json)).await.is_err() {
            return;
        }
    }

    // Spawn task to forward broadcast events to this client
    let send_task = tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    if let Ok(json) = serde_json::to_string(&event) {
                        if sender.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                },
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("WebSocket client lagged by {n} messages");
                },
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Handle incoming messages
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                debug!("Received: {text}");
                if let Ok(cmd) = serde_json::from_str::<WsCommand>(&text) {
                    handle_command(cmd);
                }
            },
            Ok(Message::Close(_)) => {
                debug!("Client disconnected");
                break;
            },
            Err(e) => {
                warn!("WebSocket error: {e}");
                break;
            },
            _ => {},
        }
    }

    // Clean up
    send_task.abort();
    debug!("WebSocket handler finished");
}

/// Handle a WebSocket command from client.
fn handle_command(cmd: WsCommand) {
    match cmd {
        WsCommand::SubscribeMetering { node, device } => {
            debug!("Subscribe metering: {node}/{device}");
            // Note: Metering broadcast is implemented in service.rs and sends all meters
            // to all clients at 30 Hz. Per-device filtering is a future optimization.
            // For now, clients receive all meter updates and can filter locally.
        },
        WsCommand::UnsubscribeMetering { node, device } => {
            debug!("Unsubscribe metering: {node}/{device}");
            // Note: Currently all meters are broadcast to all clients.
            // Per-device subscription filtering is a future optimization.
        },
        WsCommand::Ping => {
            debug!("Ping received");
            // Pong is handled automatically by axum
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_event_metering_serializes() {
        let event = WsEvent::Metering(MeteringUpdate {
            node: "host1".into(),
            device: "Device A".into(),
            levels: vec![-12.0, -15.0],
        });

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"metering\""));
        assert!(json.contains("\"node\":\"host1\""));
    }

    #[test]
    fn ws_event_route_changed_serializes() {
        let event = WsEvent::RouteChanged(RouteUpdate {
            action: "added".into(),
            route_id: "route-1".into(),
        });

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"route_changed\""));
        assert!(json.contains("\"action\":\"added\""));
    }

    #[test]
    fn ws_event_node_status_serializes() {
        let event = WsEvent::NodeStatus(NodeStatusUpdate {
            node: "node-1".into(),
            online: true,
        });

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"node_status\""));
        assert!(json.contains("\"online\":true"));
    }

    #[test]
    fn ws_event_device_status_serializes() {
        let event = WsEvent::DeviceStatus(DeviceStatusUpdate {
            device: "device-1".into(),
            state: "running".into(),
        });

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"device_status\""));
        assert!(json.contains("\"state\":\"running\""));
    }

    #[test]
    fn ws_event_error_serializes() {
        let event = WsEvent::Error(ErrorUpdate {
            code: "DEVICE_ERROR".into(),
            message: "Device disconnected".into(),
        });

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("\"code\":\"DEVICE_ERROR\""));
    }

    #[test]
    fn ws_command_subscribe_deserializes() {
        let json =
            r#"{"command":"subscribe_metering","data":{"node":"node-1","device":"device-1"}}"#;
        let cmd: WsCommand = serde_json::from_str(json).unwrap();

        match cmd {
            WsCommand::SubscribeMetering { node, device } => {
                assert_eq!(node, "node-1");
                assert_eq!(device, "device-1");
            },
            _ => panic!("Wrong command type"),
        }
    }

    #[test]
    fn ws_command_unsubscribe_deserializes() {
        let json =
            r#"{"command":"unsubscribe_metering","data":{"node":"node-1","device":"device-1"}}"#;
        let cmd: WsCommand = serde_json::from_str(json).unwrap();

        match cmd {
            WsCommand::UnsubscribeMetering { node, device } => {
                assert_eq!(node, "node-1");
                assert_eq!(device, "device-1");
            },
            _ => panic!("Wrong command type"),
        }
    }

    #[test]
    fn ws_command_ping_deserializes() {
        let json = r#"{"command":"ping"}"#;
        let cmd: WsCommand = serde_json::from_str(json).unwrap();

        assert!(matches!(cmd, WsCommand::Ping));
    }
}
