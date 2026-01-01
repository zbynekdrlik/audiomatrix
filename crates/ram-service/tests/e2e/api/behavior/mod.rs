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
    pub input: usize,
    pub output: usize,
}

/// Response for node info.
#[derive(Debug, Deserialize)]
pub struct NodeInfo {
    pub name: String,
    #[allow(dead_code)]
    pub ip: String,
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
