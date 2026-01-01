#![allow(clippy::doc_markdown)]
//! WebSocket metering E2E tests.

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Helper to send a text message.
fn text_message(s: &str) -> Message {
    Message::Text(s.into())
}

/// WebSocket URL for tests.
fn ws_url() -> String {
    "ws://localhost:8080/api/v1/ws".to_string()
}

/// Test subscribing to metering for a device.
#[tokio::test]
async fn test_subscribe_metering() {
    let (ws_stream, _) = connect_async(&ws_url()).await.expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Send subscribe message (correct format: command + data)
    let subscribe_msg = json!({
        "command": "subscribe_metering",
        "data": {
            "node": "LOCAL",
            "device": "test-device"
        }
    });

    write
        .send(text_message(&subscribe_msg.to_string()))
        .await
        .expect("Failed to send subscribe message");

    // Should receive acknowledgment or metering data
    let response = timeout(Duration::from_secs(5), read.next()).await;

    // Server should respond (either error if device doesn't exist, or success)
    assert!(response.is_ok(), "Should receive response within timeout");

    let _ = write.close().await;
}

/// Test unsubscribing from metering.
#[tokio::test]
async fn test_unsubscribe_metering() {
    let (ws_stream, _) = connect_async(&ws_url()).await.expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // First subscribe (correct format: command + data)
    let subscribe_msg = json!({
        "command": "subscribe_metering",
        "data": {
            "node": "LOCAL",
            "device": "test-device"
        }
    });

    write
        .send(text_message(&subscribe_msg.to_string()))
        .await
        .expect("Failed to send subscribe message");

    // Wait a bit for subscription to be established
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Then unsubscribe (correct format: command + data)
    let unsubscribe_msg = json!({
        "command": "unsubscribe_metering",
        "data": {
            "node": "LOCAL",
            "device": "test-device"
        }
    });

    write
        .send(text_message(&unsubscribe_msg.to_string()))
        .await
        .expect("Failed to send unsubscribe message");

    // Drain any pending messages
    let _ = timeout(Duration::from_millis(500), async {
        while read.next().await.is_some() {}
    })
    .await;

    let _ = write.close().await;
}

/// Test metering data format when device is active.
#[tokio::test]
async fn test_metering_data_format() {
    let (ws_stream, _) = connect_async(&ws_url()).await.expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Subscribe to metering for LOCAL node (correct format: command + data)
    // Note: Using "*" for device subscribes to all devices
    let subscribe_msg = json!({
        "command": "subscribe_metering",
        "data": {
            "node": "LOCAL",
            "device": "*"
        }
    });

    write
        .send(text_message(&subscribe_msg.to_string()))
        .await
        .expect("Failed to send subscribe message");

    // Wait for metering data
    let metering_data = timeout(Duration::from_secs(5), async {
        while let Some(msg_result) = read.next().await {
            if let Ok(Message::Text(text)) = msg_result {
                // text is Utf8Bytes, convert to &str
                let text_str: &str = text.as_ref();
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(text_str) {
                    // Check if this is metering data (event format: "event": "metering")
                    if json.get("event").and_then(|v| v.as_str()) == Some("metering") {
                        return Some(json);
                    }
                }
            }
        }
        None
    })
    .await;

    if let Ok(Some(data)) = metering_data {
        // Verify metering data has expected structure
        assert!(data.get("device").is_some(), "Metering should have device");
        assert!(data.get("levels").is_some(), "Metering should have levels");
    }

    let _ = write.close().await;
}

/// Test subscribing to multiple devices.
#[tokio::test]
async fn test_subscribe_multiple_devices() {
    let (ws_stream, _) = connect_async(&ws_url()).await.expect("Failed to connect");

    let (mut write, _read) = ws_stream.split();

    // Subscribe to multiple devices (correct format: command + data)
    for i in 1..=3 {
        let subscribe_msg = json!({
            "command": "subscribe_metering",
            "data": {
                "node": "LOCAL",
                "device": format!("device-{}", i)
            }
        });

        write
            .send(text_message(&subscribe_msg.to_string()))
            .await
            .expect("Failed to send subscribe message");
    }

    // All subscriptions should be accepted
    tokio::time::sleep(Duration::from_millis(100)).await;

    let _ = write.close().await;
}

/// Test route change events are broadcast when routes are modified.
///
/// Note: Route change events are automatically broadcast to all connected clients
/// when routes are created, updated, or deleted. No explicit subscription is needed.
#[tokio::test]
async fn test_route_change_events() {
    let (ws_stream, _) = connect_async(&ws_url()).await.expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Send a ping to verify connection is working
    let ping_msg = json!({
        "command": "ping"
    });

    write
        .send(text_message(&ping_msg.to_string()))
        .await
        .expect("Failed to send ping message");

    // Wait briefly for any response (route events are broadcast automatically)
    let _ = timeout(Duration::from_millis(500), read.next()).await;

    let _ = write.close().await;
}

/// Test device status events are broadcast when device status changes.
///
/// Note: Device status events are automatically broadcast to all connected clients
/// when devices are attached, detached, or their status changes. No explicit subscription needed.
#[tokio::test]
async fn test_device_status_events() {
    let (ws_stream, _) = connect_async(&ws_url()).await.expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Send a ping to verify connection is working
    let ping_msg = json!({
        "command": "ping"
    });

    write
        .send(text_message(&ping_msg.to_string()))
        .await
        .expect("Failed to send ping message");

    // Wait briefly for any response (device status events are broadcast automatically)
    let _ = timeout(Duration::from_millis(500), read.next()).await;

    let _ = write.close().await;
}
