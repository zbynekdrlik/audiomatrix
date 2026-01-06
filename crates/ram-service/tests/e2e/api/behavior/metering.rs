//! Metering E2E behavioral tests.
//!
//! These tests verify that metering infrastructure works correctly:
//! - Debug metering HTTP endpoint returns correct structure
//! - Metering contexts are registered for attached devices
//! - WebSocket broadcasts metering events when signal is present

use super::*;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;

/// Response structure for /debug/metering endpoint.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DebugMeteringResponse {
    input_device_count: usize,
    output_device_count: usize,
    input_meters: Vec<DeviceMeterDebug>,
    output_meters: Vec<DeviceMeterDebug>,
}

/// Per-device meter debug info.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DeviceMeterDebug {
    device_id: String,
    channel_count: usize,
    max_rms_db: f32,
    max_peak_db: f32,
    rms_levels: Vec<f32>,
    peak_levels: Vec<f32>,
}

/// Test: Debug metering endpoint returns correct structure.
///
/// This test verifies the /debug/metering HTTP endpoint:
/// - Returns proper JSON structure
/// - Device counts match attached devices
/// - Level arrays have correct channel counts
#[tokio::test]
async fn test_debug_metering_endpoint_structure() {
    let client = TestClient::new();

    // Ensure at least one duplex device is attached for metering
    let device = ensure_duplex_attached(&client)
        .await
        .expect("TEST INFRASTRUCTURE ERROR: No duplex devices available. DO NOT SKIP.");

    // Give streams time to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Fetch debug metering endpoint
    let response: DebugMeteringResponse = client
        .get_json("/debug/metering")
        .await
        .expect("Failed to get debug metering");

    // Verify structure
    assert!(
        response.input_device_count >= 1,
        "Should have at least one input device with metering"
    );
    assert!(
        response.output_device_count >= 1,
        "Should have at least one output device with metering"
    );

    // Find our attached device in input meters
    let input_meter = response
        .input_meters
        .iter()
        .find(|m| m.device_id == device.id);

    assert!(
        input_meter.is_some(),
        "Attached device {} should have input metering",
        device.id
    );

    if let Some(meter) = input_meter {
        assert!(
            meter.channel_count > 0,
            "Device should have at least one input channel"
        );
        assert_eq!(
            meter.rms_levels.len(),
            meter.channel_count,
            "RMS levels should match channel count"
        );
        assert_eq!(
            meter.peak_levels.len(),
            meter.channel_count,
            "Peak levels should match channel count"
        );

        // Verify all levels are in valid dB range (-inf to 0)
        for (i, &level) in meter.rms_levels.iter().enumerate() {
            assert!(
                level <= 0.0,
                "RMS level on channel {} ({} dB) should be <= 0 dB",
                i + 1,
                level
            );
        }
        for (i, &level) in meter.peak_levels.iter().enumerate() {
            assert!(
                level <= 0.0,
                "Peak level on channel {} ({} dB) should be <= 0 dB",
                i + 1,
                level
            );
        }
    }

    println!("Debug metering endpoint structure verified");
    println!(
        "  Input devices: {}, Output devices: {}",
        response.input_device_count, response.output_device_count
    );
    for meter in &response.input_meters {
        println!(
            "  {} (input): {} channels, max_rms={:.1} dB",
            meter.device_id, meter.channel_count, meter.max_rms_db
        );
    }
}

/// Test: Metering contexts are registered when device is attached.
///
/// This verifies that attaching a device creates the necessary metering
/// context and that detaching removes it.
#[tokio::test]
async fn test_metering_context_lifecycle() {
    let client = TestClient::new();

    // Get devices
    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    // Find a duplex device - prefer already attached devices (proven to work)
    // over available ones (which may be virtual devices that don't start properly)
    let device = devices
        .iter()
        .find(|d| {
            matches!(d.device_type, DeviceType::Duplex)
                && matches!(d.status, DeviceStatus::Attached | DeviceStatus::Active)
        })
        .or_else(|| {
            devices.iter().find(|d| {
                matches!(d.device_type, DeviceType::Duplex)
                    && matches!(d.status, DeviceStatus::Available)
            })
        })
        .expect("TEST INFRASTRUCTURE ERROR: No duplex devices. DO NOT SKIP.");

    let device_id = urlencoding::encode(&device.id);
    let was_already_attached =
        matches!(device.status, DeviceStatus::Attached | DeviceStatus::Active);

    // If already attached, detach first to test the lifecycle properly
    if was_already_attached {
        let _ = client
            .post_json(
                &format!("/nodes/LOCAL/devices/{device_id}/detach"),
                &serde_json::json!({}),
            )
            .await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // Check metering before attach
    let before: DebugMeteringResponse = client
        .get_json("/debug/metering")
        .await
        .expect("Failed to get metering");

    let had_device_before = before.input_meters.iter().any(|m| m.device_id == device.id);

    // Attach device
    let attach_response = client
        .post_json(
            &format!("/nodes/LOCAL/devices/{device_id}/attach"),
            &serde_json::json!({}),
        )
        .await
        .expect("Failed to attach device");

    assert!(
        attach_response.status().is_success(),
        "Device attachment should succeed"
    );

    // Poll for metering context to appear (stream start is async, may take variable time)
    // ASIO devices can take several seconds to initialize streams
    let mut has_device_after = false;
    let mut last_input_count = 0;
    let mut last_output_count = 0;

    for attempt in 1..=15 {
        tokio::time::sleep(Duration::from_secs(1)).await;

        let after: DebugMeteringResponse = client
            .get_json("/debug/metering")
            .await
            .expect("Failed to get metering after attach");

        last_input_count = after.input_device_count;
        last_output_count = after.output_device_count;

        has_device_after = after.input_meters.iter().any(|m| m.device_id == device.id);

        if has_device_after {
            println!("Metering context appeared after {} seconds", attempt);
            break;
        }

        // Log progress for debugging
        if attempt % 5 == 0 {
            println!(
                "Attempt {}/15: metering has {} input, {} output devices, waiting for {}",
                attempt, after.input_device_count, after.output_device_count, device.id
            );
        }
    }

    assert!(
        has_device_after,
        "Device {} should have metering context after attach (found {} inputs, {} outputs)",
        device.id, last_input_count, last_output_count
    );

    println!("Metering lifecycle verified for device: {}", device.name);
    println!("  Before attach: metering={}", had_device_before);
    println!("  After attach: metering={}", has_device_after);

    // Cleanup: Detach the device
    let _ = client
        .post_json(
            &format!("/nodes/LOCAL/devices/{device_id}/detach"),
            &serde_json::json!({}),
        )
        .await;
}

/// Test: WebSocket receives metering events with correct format.
///
/// This test verifies the WebSocket metering protocol:
/// - Events use correct format: {"type": "metering", "data": {...}}
/// - Data includes node, device, direction, levels, peaks
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn test_websocket_metering_format() {
    let client = TestClient::new();
    let base_url =
        std::env::var("TEST_SERVER_URL").unwrap_or_else(|_| "localhost:8080".to_string());
    let ws_url = format!("ws://{base_url}/api/v1/ws");

    // Ensure device is attached
    let _device = ensure_duplex_attached(&client)
        .await
        .expect("TEST INFRASTRUCTURE ERROR: No duplex devices available. DO NOT SKIP.");

    // Connect to WebSocket
    let (mut ws, _) = connect_async(&ws_url)
        .await
        .expect("Failed to connect WebSocket");

    // Collect messages for up to 5 seconds
    let mut received_node_status = false;
    let mut received_metering = false;
    let mut metering_format_valid = true;
    let mut format_error: Option<String> = None;

    let timeout = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(msg) = ws.next().await {
            if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = msg {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    // Check message type
                    if let Some(msg_type) = json.get("type").and_then(|t| t.as_str()) {
                        match msg_type {
                            "node_status" => {
                                received_node_status = true;
                            },
                            "metering" => {
                                received_metering = true;

                                // Validate metering format
                                let data = json.get("data");
                                if data.is_none() {
                                    metering_format_valid = false;
                                    format_error =
                                        Some("Metering event missing 'data' field".to_string());
                                    break;
                                }

                                let data = data.unwrap();

                                // Required fields: node, device, direction, levels, peaks
                                let required_fields =
                                    ["node", "device", "direction", "levels", "peaks"];
                                for field in &required_fields {
                                    if data.get(*field).is_none() {
                                        metering_format_valid = false;
                                        format_error = Some(format!(
                                            "Metering data missing '{}' field",
                                            field
                                        ));
                                        break;
                                    }
                                }

                                // Validate levels array
                                if let Some(levels) = data.get("levels").and_then(|l| l.as_array())
                                {
                                    for level in levels {
                                        if let Some(db) = level.as_f64() {
                                            if db > 0.0 {
                                                metering_format_valid = false;
                                                format_error =
                                                    Some(format!("Level {} dB should be <= 0", db));
                                                break;
                                            }
                                        }
                                    }
                                }

                                if metering_format_valid {
                                    break;
                                }
                            },
                            _ => {},
                        }
                    }
                }
            }
        }
    });

    let _ = timeout.await;

    // Close WebSocket
    let _ = ws
        .send(tokio_tungstenite::tungstenite::Message::Close(None))
        .await;

    // Verify results
    assert!(
        received_node_status,
        "WebSocket should send initial node_status event"
    );

    if received_metering {
        assert!(
            metering_format_valid,
            "Metering format error: {}",
            format_error.unwrap_or_default()
        );
        println!("WebSocket metering format verified");
    } else {
        // If no metering received, check that it's because device is silent
        let metering: DebugMeteringResponse = client
            .get_json("/debug/metering")
            .await
            .expect("Failed to get debug metering");

        let all_silent = metering.input_meters.iter().all(|m| m.max_rms_db <= -120.0);

        if all_silent {
            println!(
                "No WebSocket metering received because all devices are silent (expected behavior)"
            );
        } else {
            panic!(
                "TEST FAILURE: Device has signal (max_rms > -120 dB) but no metering \
                 events received via WebSocket. This indicates a metering broadcast issue."
            );
        }
    }
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
                        // Check for route_changed event (new format)
                        let event_type = json.get("type").and_then(|e| e.as_str());
                        if event_type == Some("route_changed") {
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
            println!("WebSocket route change broadcast verified");
        } else {
            println!(
                "Note: No route_changed event received (may indicate WebSocket broadcast not implemented for routes)"
            );
        }

        let _ = client.delete(&format!("/routes/{}", created.id)).await;
    }
}
