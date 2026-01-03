#![allow(clippy::doc_markdown)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::range_plus_one)]
#![allow(clippy::manual_range_contains)]
//! Behavioral E2E tests - STRICT, NO SKIPPING.
//!
//! These tests verify that API operations have their intended EFFECT,
//! not just that they return success responses.
//!
//! IMPORTANT: These tests are MANDATORY and must NEVER be skipped.
//! If test infrastructure is unavailable, tests MUST FAIL to alert
//! the team that the CI environment needs fixing.

mod attach_detach;
mod cross_node;
mod generators;
mod metering;
mod persistence;
mod routes;
mod sample_rate;

use ram_api::models::{DeviceInfo, DeviceStatus, DeviceType, StreamInfo};
use serde::Deserialize;
use std::time::Duration;

use crate::e2e::TestClient;

/// Macro to enforce test infrastructure availability.
/// Tests MUST fail if required conditions are not met - NO SKIPPING.
#[macro_export]
macro_rules! require {
    ($condition:expr, $msg:expr) => {
        if !$condition {
            panic!(
                "TEST INFRASTRUCTURE ERROR: {}. \
                 Fix the test environment - DO NOT SKIP TESTS.",
                $msg
            );
        }
    };
}

/// Response for stream counts endpoint.
#[derive(Debug, Deserialize)]
pub struct StreamCounts {
    pub input_streams: usize,
    pub output_streams: usize,
}

/// Response for node info.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub addresses: Vec<String>,
    pub api_port: u16,
    pub vban_port: u16,
    pub online: bool,
}

/// Helper to wait for a condition with timeout.
#[allow(dead_code)]
pub async fn wait_for<F, Fut>(timeout_secs: u64, check: F) -> bool
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(timeout_secs) {
        if check().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

/// Ensure at least one device of the specified type is attached.
/// Returns the device info after attaching if necessary.
/// This makes tests self-sufficient by handling the attachment prerequisite.
pub async fn ensure_attached_device(
    client: &TestClient,
    device_type_filter: impl Fn(&DeviceInfo) -> bool,
) -> DeviceInfo {
    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    // First try to find an already attached device
    if let Some(attached) = devices.iter().find(|d| {
        device_type_filter(d) && matches!(d.status, DeviceStatus::Attached | DeviceStatus::Active)
    }) {
        return attached.clone();
    }

    // No attached device found, attach an available one
    let available = devices
        .iter()
        .find(|d| device_type_filter(d) && matches!(d.status, DeviceStatus::Available))
        .expect(
            "TEST INFRASTRUCTURE ERROR: No devices available to attach. \
             Ensure stagebox1 has ASIO devices configured. DO NOT SKIP.",
        );

    let device_id = urlencoding::encode(&available.id);
    let response = client
        .post_json(
            &format!("/nodes/LOCAL/devices/{device_id}/attach"),
            &serde_json::json!({}),
        )
        .await
        .expect("Failed to attach device");

    assert!(
        response.status().is_success(),
        "Device attachment should succeed"
    );

    // Wait for device to be attached
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Fetch updated device info
    client
        .get_json(&format!("/nodes/LOCAL/devices/{device_id}"))
        .await
        .expect("Failed to get device info after attach")
}

/// Ensure an input device is attached.
pub async fn ensure_input_attached(client: &TestClient) -> DeviceInfo {
    ensure_attached_device(client, |d| {
        matches!(d.device_type, DeviceType::Input | DeviceType::Duplex)
    })
    .await
}

/// Ensure an output device is attached.
pub async fn ensure_output_attached(client: &TestClient) -> DeviceInfo {
    ensure_attached_device(client, |d| {
        matches!(d.device_type, DeviceType::Output | DeviceType::Duplex)
    })
    .await
}

/// Ensure a duplex device is attached.
/// Returns None if no duplex devices are available.
pub async fn ensure_duplex_attached(client: &TestClient) -> Option<DeviceInfo> {
    let devices: Vec<DeviceInfo> = match client.get_json("/nodes/LOCAL/devices").await {
        Ok(d) => d,
        Err(e) => {
            eprintln!("DUPLEX_DEBUG: Failed to get devices: {e}");
            return None;
        },
    };

    // Debug: Print all devices and their types
    eprintln!("DUPLEX_DEBUG: Found {} devices:", devices.len());
    for d in &devices {
        eprintln!(
            "  - {} ({:?}) type={:?} status={:?}",
            d.name, d.id, d.device_type, d.status
        );
    }

    // First try to find an already attached duplex device
    if let Some(attached) = devices.iter().find(|d| {
        matches!(d.device_type, DeviceType::Duplex)
            && matches!(d.status, DeviceStatus::Attached | DeviceStatus::Active)
    }) {
        eprintln!("DUPLEX_DEBUG: Found attached duplex: {}", attached.name);
        return Some(attached.clone());
    }

    // Try to attach an available duplex device
    let Some(available) = devices.iter().find(|d| {
        matches!(d.device_type, DeviceType::Duplex) && matches!(d.status, DeviceStatus::Available)
    }) else {
        eprintln!("DUPLEX_DEBUG: No duplex devices found!");
        return None;
    };

    let device_id = urlencoding::encode(&available.id);
    let response = client
        .post_json(
            &format!("/nodes/LOCAL/devices/{device_id}/attach"),
            &serde_json::json!({}),
        )
        .await
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    client
        .get_json(&format!("/nodes/LOCAL/devices/{device_id}"))
        .await
        .ok()
}
