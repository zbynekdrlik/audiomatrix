//! WebSocket service for real-time events.
//!
//! Connects to the AudioMatrix service WebSocket endpoint and
//! dispatches events to update application state.

use leptos::prelude::*;
use send_wrapper::SendWrapper;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CloseEvent, ErrorEvent, MessageEvent, WebSocket};

use crate::state::{AppState, ChannelLevel};

/// Inner WebSocket state (not Send+Sync).
struct WsInner {
    socket: RefCell<Option<WebSocket>>,
}

// ============================================================================
// WebSocket Event Types
// ============================================================================

/// Events received from the server.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    /// Metering data update.
    Metering(MeteringUpdate),
    /// Route changed notification.
    RouteChanged(RouteUpdate),
    /// Node status update.
    NodeStatus(NodeStatusUpdate),
    /// Device status update.
    DeviceStatus(DeviceStatusUpdate),
    /// Device attached event.
    DeviceAttached {
        device_id: String,
        device_name: String,
        display_name: Option<String>,
    },
    /// Device detached event.
    DeviceDetached { device_id: String },
    /// Pong response.
    Pong,
    /// Error from server.
    Error { message: String },
}

/// Metering data from server.
#[derive(Debug, Clone, Deserialize)]
pub struct MeteringUpdate {
    pub node: String,
    pub device: String,
    pub direction: String,
    pub levels: Vec<f32>,
    pub peaks: Vec<f32>,
}

/// Route change notification.
#[derive(Debug, Clone, Deserialize)]
pub struct RouteUpdate {
    pub action: String,
    pub route_id: String,
}

/// Node status update.
#[derive(Debug, Clone, Deserialize)]
pub struct NodeStatusUpdate {
    pub node: String,
    pub online: bool,
}

/// Device status update.
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceStatusUpdate {
    pub device: String,
    pub state: String,
}

// ============================================================================
// WebSocket Commands
// ============================================================================

/// Commands sent to the server.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsCommand {
    /// Subscribe to metering for a device.
    SubscribeMetering { node: String, device: String },
    /// Unsubscribe from metering.
    UnsubscribeMetering { node: String, device: String },
    /// Ping for keepalive.
    Ping,
}

// ============================================================================
// WebSocket Service
// ============================================================================

/// WebSocket connection state.
///
/// Uses `SendWrapper` to make the non-Send WebSocket type work with
/// Leptos context in single-threaded WASM environment.
#[derive(Clone)]
pub struct WsService {
    /// The WebSocket connection (wrapped for Send+Sync).
    inner: Rc<SendWrapper<WsInner>>,
    /// Connection status signal.
    pub connected: RwSignal<bool>,
    /// Last error message.
    pub last_error: RwSignal<Option<String>>,
}

// Safety: WsService is only used in single-threaded WASM environment.
// SendWrapper ensures we panic if accessed from wrong thread.
unsafe impl Send for WsService {}
unsafe impl Sync for WsService {}

impl Default for WsService {
    fn default() -> Self {
        Self::new()
    }
}

impl WsService {
    /// Creates a new WebSocket service.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Rc::new(SendWrapper::new(WsInner {
                socket: RefCell::new(None),
            })),
            connected: RwSignal::new(false),
            last_error: RwSignal::new(None),
        }
    }

    /// Connects to the WebSocket server.
    pub fn connect(&self, app_state: AppState) {
        // Build WebSocket URL from current location
        let window = match web_sys::window() {
            Some(w) => w,
            None => {
                log::error!("No window object available");
                return;
            }
        };

        let location = window.location();
        let protocol = location.protocol().unwrap_or_else(|_| "http:".to_string());
        let host = location.host().unwrap_or_else(|_| "localhost:8080".to_string());

        let ws_protocol = if protocol == "https:" { "wss" } else { "ws" };
        let url = format!("{ws_protocol}://{host}/api/v1/ws");

        log::info!("Connecting to WebSocket: {}", url);

        let ws = match WebSocket::new(&url) {
            Ok(ws) => ws,
            Err(e) => {
                log::error!("Failed to create WebSocket: {:?}", e);
                self.last_error.set(Some("Failed to create WebSocket".into()));
                return;
            }
        };

        // Set binary type to arraybuffer
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        // Clone references for closures
        let inner_ref = self.inner.clone();
        let connected_signal = self.connected;
        let error_signal = self.last_error;

        // onopen handler
        {
            let connected = connected_signal;
            let on_open = Closure::wrap(Box::new(move |_| {
                log::info!("WebSocket connected");
                connected.set(true);
            }) as Box<dyn FnMut(JsValue)>);
            ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
            on_open.forget();
        }

        // onerror handler
        {
            let error_signal = error_signal;
            let on_error = Closure::wrap(Box::new(move |e: ErrorEvent| {
                log::error!("WebSocket error: {:?}", e);
                error_signal.set(Some("WebSocket error".into()));
            }) as Box<dyn FnMut(ErrorEvent)>);
            ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));
            on_error.forget();
        }

        // onclose handler
        {
            let connected = connected_signal;
            let on_close = Closure::wrap(Box::new(move |e: CloseEvent| {
                log::info!("WebSocket closed: code={} reason={}", e.code(), e.reason());
                connected.set(false);
            }) as Box<dyn FnMut(CloseEvent)>);
            ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
            on_close.forget();
        }

        // onmessage handler
        {
            let state = app_state.clone();
            let on_message = Closure::wrap(Box::new(move |e: MessageEvent| {
                if let Some(text) = e.data().as_string() {
                    handle_message(&state, &text);
                }
            }) as Box<dyn FnMut(MessageEvent)>);
            ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
            on_message.forget();
        }

        // Store the WebSocket
        *inner_ref.socket.borrow_mut() = Some(ws);
    }

    /// Sends a command to the server.
    pub fn send(&self, cmd: &WsCommand) {
        if let Some(ws) = self.inner.socket.borrow().as_ref() {
            if ws.ready_state() == WebSocket::OPEN {
                if let Ok(json) = serde_json::to_string(cmd) {
                    if let Err(e) = ws.send_with_str(&json) {
                        log::error!("Failed to send WebSocket message: {:?}", e);
                    }
                }
            }
        }
    }

    /// Subscribes to metering for a specific device.
    pub fn subscribe_metering(&self, node: &str, device: &str) {
        self.send(&WsCommand::SubscribeMetering {
            node: node.to_string(),
            device: device.to_string(),
        });
    }

    /// Unsubscribes from metering for a device.
    pub fn unsubscribe_metering(&self, node: &str, device: &str) {
        self.send(&WsCommand::UnsubscribeMetering {
            node: node.to_string(),
            device: device.to_string(),
        });
    }

    /// Sends a ping to keep the connection alive.
    pub fn ping(&self) {
        self.send(&WsCommand::Ping);
    }

    /// Checks if connected.
    pub fn is_connected(&self) -> bool {
        self.connected.get()
    }

    /// Closes the connection.
    pub fn close(&self) {
        if let Some(ws) = self.inner.socket.borrow().as_ref() {
            let _ = ws.close();
        }
        self.connected.set(false);
    }
}

/// Handles incoming WebSocket messages.
fn handle_message(state: &AppState, text: &str) {
    match serde_json::from_str::<WsEvent>(text) {
        Ok(event) => {
            match event {
                WsEvent::Metering(m) => {
                    // Convert to channel levels
                    let levels: Vec<ChannelLevel> = m
                        .levels
                        .iter()
                        .zip(m.peaks.iter())
                        .map(|(&level, &peak)| ChannelLevel {
                            level_db: level,
                            peak_db: peak,
                        })
                        .collect();

                    let is_input = m.direction == "input";
                    state.update_levels(&m.device, is_input, levels);
                }
                WsEvent::RouteChanged(r) => {
                    log::info!("Route changed: {} - {}", r.action, r.route_id);
                    // Could trigger a route refresh here if needed
                }
                WsEvent::NodeStatus(n) => {
                    log::info!("Node {} online: {}", n.node, n.online);
                    // Could update node status in state
                }
                WsEvent::DeviceStatus(d) => {
                    log::info!("Device {} status: {}", d.device, d.state);
                    // Could update device status in state
                }
                WsEvent::DeviceAttached {
                    device_id,
                    device_name,
                    ..
                } => {
                    log::info!("Device attached: {} ({})", device_name, device_id);
                }
                WsEvent::DeviceDetached { device_id } => {
                    log::info!("Device detached: {}", device_id);
                }
                WsEvent::Pong => {
                    // Keepalive response, nothing to do
                }
                WsEvent::Error { message } => {
                    log::error!("WebSocket server error: {}", message);
                    state.error.set(Some(format!("Server error: {message}")));
                }
            }
        }
        Err(e) => {
            log::warn!("Failed to parse WebSocket message: {} - {}", e, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_command_serialization() {
        let cmd = WsCommand::SubscribeMetering {
            node: "LOCAL".to_string(),
            device: "test".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("subscribe_metering"));
        assert!(json.contains("LOCAL"));
    }

    #[test]
    fn test_ws_event_deserialization() {
        let json = r#"{"type":"pong"}"#;
        let event: WsEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, WsEvent::Pong));
    }

    #[test]
    fn test_metering_event_deserialization() {
        let json = r#"{"type":"metering","node":"LOCAL","device":"test","direction":"input","levels":[-12.0,-18.0],"peaks":[-6.0,-9.0]}"#;
        let event: WsEvent = serde_json::from_str(json).unwrap();
        if let WsEvent::Metering(m) = event {
            assert_eq!(m.device, "test");
            assert_eq!(m.levels.len(), 2);
        } else {
            panic!("Expected Metering event");
        }
    }
}
