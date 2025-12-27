//! WebSocket handler for real-time events.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

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

/// Error update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorUpdate {
    /// Error code.
    pub code: String,
    /// Error message.
    pub message: String,
}

/// WebSocket upgrade handler.
pub async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

/// Handle WebSocket connection.
async fn handle_socket(mut socket: WebSocket) {
    debug!("WebSocket client connected");

    // Send initial connection acknowledgment
    let ack = WsEvent::NodeStatus(NodeStatusUpdate {
        node: "LOCAL".into(),
        online: true,
    });

    if let Ok(json) = serde_json::to_string(&ack) {
        if socket.send(Message::Text(json)).await.is_err() {
            return;
        }
    }

    // Handle incoming messages
    while let Some(msg) = socket.recv().await {
        match msg {
            Ok(Message::Text(text)) => {
                debug!("Received: {text}");
                // TODO: Handle client commands
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_event_serializes() {
        let event = WsEvent::Metering(MeteringUpdate {
            node: "host1".into(),
            device: "Device A".into(),
            levels: vec![-12.0, -15.0],
        });

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("metering"));
    }
}
