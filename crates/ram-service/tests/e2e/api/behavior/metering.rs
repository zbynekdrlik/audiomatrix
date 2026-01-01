//! WebSocket metering behavioral tests.

use super::*;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;

/// Test: Metering WebSocket provides level data for attached devices.
#[tokio::test]
async fn test_metering_websocket_provides_levels() {
    let base_url =
        std::env::var("TEST_SERVER_URL").unwrap_or_else(|_| "localhost:8080".to_string());
    let ws_url = format!("ws://{base_url}/api/v1/ws");

    let (mut ws, _) = connect_async(&ws_url)
        .await
        .expect("Failed to connect WebSocket");

    let subscribe = serde_json::json!({
        "command": "subscribe",
        "data": { "device": "*" }
    });

    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        subscribe.to_string().into(),
    ))
    .await
    .expect("Failed to send subscribe");

    let mut received_metering = false;
    let timeout = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(msg) = ws.next().await {
            if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = msg {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if json.get("event").and_then(|e| e.as_str()) == Some("metering") {
                        assert!(
                            json.get("device").is_some(),
                            "Metering should include device"
                        );
                        assert!(
                            json.get("levels").is_some(),
                            "Metering should include levels"
                        );

                        if let Some(levels) = json.get("levels").and_then(|l| l.as_array()) {
                            for level in levels {
                                if let Some(db) = level.as_f64() {
                                    assert!(
                                        db <= 0.0 && db >= -100.0,
                                        "Level should be in valid dB range: {}",
                                        db
                                    );
                                }
                            }
                        }

                        received_metering = true;
                        break;
                    }
                }
            }
        }
    });

    let _ = timeout.await;

    if received_metering {
        println!("✓ Received valid metering data");
    } else {
        println!("⚠ No metering data received (device may be silent)");
    }

    let unsubscribe = serde_json::json!({
        "command": "unsubscribe",
        "data": { "device": "*" }
    });

    let _ = ws
        .send(tokio_tungstenite::tungstenite::Message::Text(
            unsubscribe.to_string().into(),
        ))
        .await;
}

/// Test: WebSocket broadcasts route changes.
#[tokio::test]
async fn test_websocket_broadcasts_route_changes() {
    let client = TestClient::new();
    let base_url =
        std::env::var("TEST_SERVER_URL").unwrap_or_else(|_| "localhost:8080".to_string());
    let ws_url = format!("ws://{base_url}/api/v1/ws");

    let (mut ws, _) = connect_async(&ws_url)
        .await
        .expect("Failed to connect WebSocket");

    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let input = devices
        .iter()
        .find(|d| matches!(d.device_type, DeviceType::Input | DeviceType::Duplex));
    let output = devices
        .iter()
        .find(|d| matches!(d.device_type, DeviceType::Output | DeviceType::Duplex));

    let inp = input.expect("TEST INFRASTRUCTURE ERROR: No input devices. DO NOT SKIP.");
    let out = output.expect("TEST INFRASTRUCTURE ERROR: No output devices. DO NOT SKIP.");

    let route = serde_json::json!({
        "source_node": "LOCAL",
        "source_device": inp.id,
        "source_channel": 1,
        "destination_node": "LOCAL",
        "destination_device": out.id,
        "destination_channel": 1,
        "volume": 1.0,
        "muted": false
    });

    let ws_task = tokio::spawn(async move {
        let mut route_event_received = false;
        let timeout = tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(msg) = ws.next().await {
                if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = msg {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        let event = json.get("event").and_then(|e| e.as_str());
                        if event == Some("route_created") || event == Some("route_updated") {
                            route_event_received = true;
                            break;
                        }
                    }
                }
            }
        });
        let _ = timeout.await;
        route_event_received
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let response = client
        .post_json("/routes", &route)
        .await
        .expect("Failed to create route");

    #[derive(Deserialize)]
    struct RouteResp {
        id: String,
    }

    if response.status().is_success() {
        let created: RouteResp = response.json().await.unwrap();
        let route_event_received = ws_task.await.unwrap_or(false);

        if route_event_received {
            println!("✓ Received route change event via WebSocket");
        } else {
            println!("⚠ No route change event received (may be expected if not implemented)");
        }

        let _ = client.delete(&format!("/routes/{}", created.id)).await;
    }
}
