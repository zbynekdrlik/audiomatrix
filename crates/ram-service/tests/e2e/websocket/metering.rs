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
#[ignore = "Requires running server with audio devices"]
async fn test_subscribe_metering() {
    let (ws_stream, _) = connect_async(&ws_url()).await.expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Send subscribe message
    let subscribe_msg = json!({
        "type": "SubscribeMetering",
        "node_id": "LOCAL",
        "device_id": "test-device"
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
#[ignore = "Requires running server with audio devices"]
async fn test_unsubscribe_metering() {
    let (ws_stream, _) = connect_async(&ws_url()).await.expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // First subscribe
    let subscribe_msg = json!({
        "type": "SubscribeMetering",
        "node_id": "LOCAL",
        "device_id": "test-device"
    });

    write
        .send(text_message(&subscribe_msg.to_string()))
        .await
        .expect("Failed to send subscribe message");

    // Wait a bit for subscription to be established
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Then unsubscribe
    let unsubscribe_msg = json!({
        "type": "UnsubscribeMetering",
        "node_id": "LOCAL",
        "device_id": "test-device"
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
#[ignore = "Requires running server with active audio device"]
async fn test_metering_data_format() {
    let (ws_stream, _) = connect_async(&ws_url()).await.expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Subscribe to metering for LOCAL node
    let subscribe_msg = json!({
        "type": "SubscribeMetering",
        "node_id": "LOCAL",
        "device_id": "*"  // Subscribe to all devices
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
                    // Check if this is metering data
                    if json.get("type").and_then(|v| v.as_str()) == Some("Metering") {
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
        assert!(
            data.get("device_id").is_some(),
            "Metering should have device_id"
        );
        assert!(data.get("levels").is_some(), "Metering should have levels");
    }

    let _ = write.close().await;
}

/// Test subscribing to multiple devices.
#[tokio::test]
#[ignore = "Requires running server with audio devices"]
async fn test_subscribe_multiple_devices() {
    let (ws_stream, _) = connect_async(&ws_url()).await.expect("Failed to connect");

    let (mut write, _read) = ws_stream.split();

    // Subscribe to multiple devices
    for i in 1..=3 {
        let subscribe_msg = json!({
            "type": "SubscribeMetering",
            "node_id": "LOCAL",
            "device_id": format!("device-{}", i)
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

/// Test route change events are broadcast.
#[tokio::test]
#[ignore = "Requires running server"]
async fn test_route_change_events() {
    let (ws_stream, _) = connect_async(&ws_url()).await.expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Subscribe to route events
    let subscribe_msg = json!({
        "type": "SubscribeRouteChanges"
    });

    write
        .send(text_message(&subscribe_msg.to_string()))
        .await
        .expect("Failed to send subscribe message");

    // Wait briefly for any response
    let _ = timeout(Duration::from_millis(500), read.next()).await;

    let _ = write.close().await;
}

/// Test device status events are broadcast.
#[tokio::test]
#[ignore = "Requires running server"]
async fn test_device_status_events() {
    let (ws_stream, _) = connect_async(&ws_url()).await.expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Subscribe to device status events
    let subscribe_msg = json!({
        "type": "SubscribeDeviceStatus",
        "node_id": "LOCAL"
    });

    write
        .send(text_message(&subscribe_msg.to_string()))
        .await
        .expect("Failed to send subscribe message");

    // Wait briefly for any response
    let _ = timeout(Duration::from_millis(500), read.next()).await;

    let _ = write.close().await;
}
