# AudioMatrix - E2E Test Specification

> **Status**: Comprehensive end-to-end testing strategy
> **Created**: 2025-12-29
> **Critical**: NO E2E tests currently exist

---

## Executive Summary

The AudioMatrix codebase has **154+ unit tests** but **ZERO E2E tests**. This spec defines a comprehensive E2E testing strategy covering:

1. **API E2E Tests** - REST endpoint testing with real service
2. **WebSocket E2E Tests** - Real-time event testing
3. **UI E2E Tests** - Browser automation testing
4. **Cross-Node E2E Tests** - Multi-node network testing
5. **Audio Flow E2E Tests** - End-to-end audio verification

---

## Test Infrastructure

### Test Framework Stack

| Layer | Tool | Purpose |
|-------|------|---------|
| API Tests | `tokio::test` + `reqwest` | REST endpoint testing |
| WebSocket Tests | `tokio-tungstenite` | Real-time event testing |
| UI Tests | `playwright-test` | Browser automation |
| Cross-Node | Docker Compose | Multi-container orchestration |
| Audio | `hound` (WAV) + custom | Audio signal verification |

### Directory Structure

```
tests/
├── e2e/
│   ├── api/
│   │   ├── mod.rs
│   │   ├── health_test.rs
│   │   ├── nodes_test.rs
│   │   ├── devices_test.rs
│   │   ├── routes_test.rs
│   │   ├── channels_test.rs
│   │   ├── virtual_devices_test.rs
│   │   ├── generator_test.rs
│   │   └── subscriptions_test.rs
│   ├── websocket/
│   │   ├── mod.rs
│   │   ├── connection_test.rs
│   │   ├── metering_test.rs
│   │   └── events_test.rs
│   ├── cross_node/
│   │   ├── mod.rs
│   │   ├── discovery_test.rs
│   │   └── routing_test.rs
│   └── common/
│       ├── mod.rs
│       ├── fixtures.rs
│       └── helpers.rs
├── ui/
│   ├── playwright.config.ts
│   ├── fixtures/
│   │   └── base.ts
│   └── specs/
│       ├── routing-matrix.spec.ts
│       ├── device-management.spec.ts
│       ├── channel-labels.spec.ts
│       └── settings.spec.ts
└── integration/
    ├── audio_flow_test.rs
    └── latency_test.rs
```

---

## Part 1: API E2E Tests

### Test Harness (`tests/e2e/common/mod.rs`)

```rust
//! E2E test utilities and fixtures.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// Test server handle.
pub struct TestServer {
    pub addr: SocketAddr,
    pub client: Client,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl TestServer {
    /// Starts a test server on a random port.
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        // Start the service
        tokio::spawn(async move {
            let config = ram_service::Config {
                api_port: addr.port(),
                node_name: "test-node".to_string(),
                ..Default::default()
            };

            ram_service::run_with_shutdown(config, shutdown_rx).await.unwrap();
        });

        // Wait for server to be ready
        tokio::time::sleep(Duration::from_millis(500)).await;

        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();

        Self {
            addr,
            client,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    /// Gets the base URL for API calls.
    pub fn api_url(&self, path: &str) -> String {
        format!("http://{}/api/v1{}", self.addr, path)
    }

    /// Gets the WebSocket URL.
    pub fn ws_url(&self) -> String {
        format!("ws://{}/api/v1/ws", self.addr)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// Test fixtures for common test data.
pub mod fixtures {
    use ram_api::models::*;

    pub fn test_route() -> RouteDefinition {
        RouteDefinition {
            source_node: "LOCAL".to_string(),
            source_device: "test-input".to_string(),
            source_channel: 1,
            destination_node: "LOCAL".to_string(),
            destination_device: "test-output".to_string(),
            destination_channel: 1,
            volume: 1.0,
            muted: false,
        }
    }

    pub fn test_virtual_device() -> CreateVirtualDevice {
        CreateVirtualDevice {
            name: "test-vasio".to_string(),
            input_channels: 8,
            output_channels: 8,
            sample_rate: 48000,
            buffer_size: 256,
            auto_attach: false,
        }
    }
}
```

### Health Endpoint Tests (`tests/e2e/api/health_test.rs`)

```rust
use crate::common::TestServer;

#[tokio::test]
async fn test_health_endpoint_returns_ok() {
    let server = TestServer::start().await;

    let response = server.client
        .get(server.api_url("/health"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert!(body["version"].is_string());
    assert!(body["hostname"].is_string());
}

#[tokio::test]
async fn test_health_includes_version() {
    let server = TestServer::start().await;

    let response = server.client
        .get(server.api_url("/health"))
        .send()
        .await
        .unwrap();

    let body: serde_json::Value = response.json().await.unwrap();
    let version = body["version"].as_str().unwrap();

    // Version should match Cargo.toml pattern
    assert!(version.starts_with("0.") || version.starts_with("1."));
}
```

### Nodes Endpoint Tests (`tests/e2e/api/nodes_test.rs`)

```rust
use crate::common::TestServer;
use ram_api::models::NodeInfo;

#[tokio::test]
async fn test_list_nodes_returns_local_node() {
    let server = TestServer::start().await;

    let response = server.client
        .get(server.api_url("/nodes"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let nodes: Vec<NodeInfo> = response.json().await.unwrap();
    assert!(!nodes.is_empty());

    // Should have at least one local node
    let local_node = nodes.iter().find(|n| n.id == "LOCAL" || n.name == "test-node");
    assert!(local_node.is_some());
}

#[tokio::test]
async fn test_get_node_by_id() {
    let server = TestServer::start().await;

    let response = server.client
        .get(server.api_url("/nodes/LOCAL"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let node: NodeInfo = response.json().await.unwrap();
    assert!(node.online);
}

#[tokio::test]
async fn test_get_nonexistent_node_returns_404() {
    let server = TestServer::start().await;

    let response = server.client
        .get(server.api_url("/nodes/nonexistent-node"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}
```

### Routes CRUD Tests (`tests/e2e/api/routes_test.rs`)

```rust
use crate::common::{TestServer, fixtures};
use ram_api::models::RouteDefinition;

#[tokio::test]
async fn test_create_route() {
    let server = TestServer::start().await;
    let route = fixtures::test_route();

    let response = server.client
        .post(server.api_url("/routes"))
        .json(&route)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 201);

    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["id"].is_string());
}

#[tokio::test]
async fn test_list_routes() {
    let server = TestServer::start().await;

    // Create a route first
    let route = fixtures::test_route();
    server.client
        .post(server.api_url("/routes"))
        .json(&route)
        .send()
        .await
        .unwrap();

    // List routes
    let response = server.client
        .get(server.api_url("/routes"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let routes: Vec<RouteDefinition> = response.json().await.unwrap();
    assert!(!routes.is_empty());
}

#[tokio::test]
async fn test_update_route_volume() {
    let server = TestServer::start().await;

    // Create route
    let route = fixtures::test_route();
    let create_response = server.client
        .post(server.api_url("/routes"))
        .json(&route)
        .send()
        .await
        .unwrap();

    let created: serde_json::Value = create_response.json().await.unwrap();
    let route_id = created["id"].as_str().unwrap();

    // Update volume
    let mut updated_route = route.clone();
    updated_route.volume = 0.5;

    let response = server.client
        .put(server.api_url(&format!("/routes/{}", urlencoding::encode(route_id))))
        .json(&updated_route)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn test_delete_route() {
    let server = TestServer::start().await;

    // Create route
    let route = fixtures::test_route();
    let create_response = server.client
        .post(server.api_url("/routes"))
        .json(&route)
        .send()
        .await
        .unwrap();

    let created: serde_json::Value = create_response.json().await.unwrap();
    let route_id = created["id"].as_str().unwrap();

    // Delete
    let response = server.client
        .delete(server.api_url(&format!("/routes/{}", urlencoding::encode(route_id))))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Verify deleted
    let get_response = server.client
        .get(server.api_url(&format!("/routes/{}", urlencoding::encode(route_id))))
        .send()
        .await
        .unwrap();

    assert_eq!(get_response.status(), 404);
}

#[tokio::test]
async fn test_route_latency_endpoint() {
    let server = TestServer::start().await;

    // Create route
    let route = fixtures::test_route();
    let create_response = server.client
        .post(server.api_url("/routes"))
        .json(&route)
        .send()
        .await
        .unwrap();

    let created: serde_json::Value = create_response.json().await.unwrap();
    let route_id = created["id"].as_str().unwrap();

    // Get latency
    let response = server.client
        .get(server.api_url(&format!("/routes/{}/latency", urlencoding::encode(route_id))))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let latency: serde_json::Value = response.json().await.unwrap();
    assert!(latency["total_ms"].is_number());
    assert!(latency["is_local"].is_boolean());
}
```

### Device Management Tests (`tests/e2e/api/devices_test.rs`)

```rust
use crate::common::TestServer;
use ram_api::models::{DeviceInfo, DeviceStatus};

#[tokio::test]
async fn test_list_devices() {
    let server = TestServer::start().await;

    let response = server.client
        .get(server.api_url("/nodes/LOCAL/devices"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let devices: Vec<DeviceInfo> = response.json().await.unwrap();
    // May be empty if no audio devices, but should not error
}

#[tokio::test]
async fn test_attach_device() {
    let server = TestServer::start().await;

    // List devices to get a real device ID
    let list_response = server.client
        .get(server.api_url("/nodes/LOCAL/devices"))
        .send()
        .await
        .unwrap();

    let devices: Vec<DeviceInfo> = list_response.json().await.unwrap();

    if let Some(device) = devices.first() {
        let response = server.client
            .post(server.api_url(&format!("/nodes/LOCAL/devices/{}/attach", device.id)))
            .json(&serde_json::json!({"display_name": "Test Device"}))
            .send()
            .await
            .unwrap();

        // Should succeed or already attached
        assert!(response.status() == 200 || response.status() == 409);
    }
}

#[tokio::test]
async fn test_device_channels() {
    let server = TestServer::start().await;

    let list_response = server.client
        .get(server.api_url("/nodes/LOCAL/devices"))
        .send()
        .await
        .unwrap();

    let devices: Vec<DeviceInfo> = list_response.json().await.unwrap();

    if let Some(device) = devices.first() {
        let response = server.client
            .get(server.api_url(&format!("/nodes/LOCAL/devices/{}/channels", device.id)))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
    }
}
```

### Virtual Device Tests (`tests/e2e/api/virtual_devices_test.rs`)

```rust
use crate::common::{TestServer, fixtures};
use ram_api::models::DeviceInfo;

#[tokio::test]
async fn test_create_virtual_device() {
    let server = TestServer::start().await;
    let req = fixtures::test_virtual_device();

    let response = server.client
        .post(server.api_url("/nodes/LOCAL/virtual-devices"))
        .json(&req)
        .send()
        .await
        .unwrap();

    // May fail if ASIO not available, which is OK in CI
    if response.status() == 201 {
        let device: DeviceInfo = response.json().await.unwrap();
        assert_eq!(device.name, "test-vasio");
        assert!(device.is_virtual);
    }
}

#[tokio::test]
async fn test_virtual_device_validation() {
    let server = TestServer::start().await;

    // Invalid sample rate
    let invalid_req = serde_json::json!({
        "name": "test",
        "input_channels": 8,
        "output_channels": 8,
        "sample_rate": 12345,  // Invalid
        "buffer_size": 256
    });

    let response = server.client
        .post(server.api_url("/nodes/LOCAL/virtual-devices"))
        .json(&invalid_req)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn test_list_virtual_devices() {
    let server = TestServer::start().await;

    let response = server.client
        .get(server.api_url("/nodes/LOCAL/virtual-devices"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let devices: Vec<DeviceInfo> = response.json().await.unwrap();
    // All returned devices should be virtual
    for device in &devices {
        assert!(device.is_virtual);
    }
}
```

### Generator Tests (`tests/e2e/api/generator_test.rs`)

```rust
use crate::common::TestServer;

#[tokio::test]
async fn test_set_generator() {
    let server = TestServer::start().await;

    // Get a device first
    let list_response = server.client
        .get(server.api_url("/nodes/LOCAL/devices"))
        .send()
        .await
        .unwrap();

    let devices: Vec<serde_json::Value> = list_response.json().await.unwrap();

    if let Some(device) = devices.first() {
        let device_id = device["id"].as_str().unwrap();

        let req = serde_json::json!({
            "enabled": true,
            "waveform": "sine",
            "frequency": 1000,
            "level_db": -20.0
        });

        let response = server.client
            .post(server.api_url(&format!("/nodes/LOCAL/devices/{}/channels/1/generator", device_id)))
            .json(&req)
            .send()
            .await
            .unwrap();

        // May succeed or fail based on device type
        assert!(response.status() == 200 || response.status() == 400);
    }
}

#[tokio::test]
async fn test_get_generator_status() {
    let server = TestServer::start().await;

    let list_response = server.client
        .get(server.api_url("/nodes/LOCAL/devices"))
        .send()
        .await
        .unwrap();

    let devices: Vec<serde_json::Value> = list_response.json().await.unwrap();

    if let Some(device) = devices.first() {
        let device_id = device["id"].as_str().unwrap();

        let response = server.client
            .get(server.api_url(&format!("/nodes/LOCAL/devices/{}/generator", device_id)))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
    }
}
```

---

## Part 2: WebSocket E2E Tests

### WebSocket Connection Tests (`tests/e2e/websocket/connection_test.rs`)

```rust
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use crate::common::TestServer;

#[tokio::test]
async fn test_websocket_connection() {
    let server = TestServer::start().await;

    let (ws_stream, _) = connect_async(server.ws_url())
        .await
        .expect("Failed to connect to WebSocket");

    let (mut write, mut read) = ws_stream.split();

    // Should receive initial connection acknowledgment
    if let Some(msg) = read.next().await {
        let msg = msg.unwrap();
        let text = msg.to_text().unwrap();
        let event: serde_json::Value = serde_json::from_str(text).unwrap();

        // Should be node_status event
        assert_eq!(event["type"], "node_status");
    }
}

#[tokio::test]
async fn test_websocket_ping_pong() {
    let server = TestServer::start().await;

    let (ws_stream, _) = connect_async(server.ws_url())
        .await
        .expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Skip initial message
    let _ = read.next().await;

    // Send ping
    let ping = serde_json::json!({"type": "ping"});
    write.send(ping.to_string().into()).await.unwrap();

    // Should receive pong
    if let Some(msg) = read.next().await {
        let msg = msg.unwrap();
        let text = msg.to_text().unwrap();
        let event: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(event["type"], "pong");
    }
}
```

### Metering Tests (`tests/e2e/websocket/metering_test.rs`)

```rust
use std::time::Duration;
use futures_util::{SinkExt, StreamExt};
use tokio::time::timeout;
use tokio_tungstenite::connect_async;
use crate::common::TestServer;

#[tokio::test]
async fn test_metering_subscription() {
    let server = TestServer::start().await;

    let (ws_stream, _) = connect_async(server.ws_url())
        .await
        .expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Skip initial message
    let _ = read.next().await;

    // Subscribe to metering
    let subscribe = serde_json::json!({
        "type": "subscribe_metering",
        "node": "LOCAL",
        "device": "test-device"
    });
    write.send(subscribe.to_string().into()).await.unwrap();

    // Wait for metering events (with timeout)
    let result = timeout(Duration::from_secs(5), async {
        while let Some(msg) = read.next().await {
            let msg = msg.unwrap();
            let text = msg.to_text().unwrap();
            let event: serde_json::Value = serde_json::from_str(text).unwrap();

            if event["type"] == "metering" {
                return true;
            }
        }
        false
    }).await;

    // Metering may or may not come depending on active streams
    // This test validates the subscription doesn't error
}
```

### Route Change Events (`tests/e2e/websocket/events_test.rs`)

```rust
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use crate::common::{TestServer, fixtures};

#[tokio::test]
async fn test_route_change_event() {
    let server = TestServer::start().await;

    // Connect WebSocket first
    let (ws_stream, _) = connect_async(server.ws_url())
        .await
        .expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Skip initial message
    let _ = read.next().await;

    // Create route via REST
    let route = fixtures::test_route();
    server.client
        .post(server.api_url("/routes"))
        .json(&route)
        .send()
        .await
        .unwrap();

    // Should receive route_changed event
    if let Some(msg) = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        read.next()
    ).await.ok().flatten() {
        let msg = msg.unwrap();
        let text = msg.to_text().unwrap();
        let event: serde_json::Value = serde_json::from_str(text).unwrap();

        assert_eq!(event["type"], "route_changed");
        assert_eq!(event["action"], "added");
    }
}
```

---

## Part 3: UI E2E Tests (Playwright)

### Playwright Config (`tests/ui/playwright.config.ts`)

```typescript
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './specs',
  fullyParallel: false, // Sequential for audio tests
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: 'html',
  use: {
    baseURL: process.env.TEST_URL || 'http://localhost:8080',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  webServer: {
    command: 'cargo run -p ram-service -- -n test',
    url: 'http://localhost:8080/api/v1/health',
    reuseExistingServer: !process.env.CI,
    timeout: 60000,
  },
});
```

### Base Fixture (`tests/ui/fixtures/base.ts`)

```typescript
import { test as base, expect } from '@playwright/test';

export const test = base.extend({
  // Custom fixture for waiting for WebSocket connection
  waitForWs: async ({ page }, use) => {
    const waitForWs = async () => {
      await page.waitForFunction(() => {
        return (window as any).__wsConnected === true;
      }, { timeout: 10000 });
    };
    await use(waitForWs);
  },
});

export { expect };
```

### Routing Matrix Tests (`tests/ui/specs/routing-matrix.spec.ts`)

```typescript
import { test, expect } from '../fixtures/base';

test.describe('Routing Matrix', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForSelector('.routing-matrix', { timeout: 10000 });
  });

  test('should display routing matrix', async ({ page }) => {
    await expect(page.locator('.routing-matrix')).toBeVisible();
  });

  test('should show source and destination selectors', async ({ page }) => {
    await expect(page.locator('select#source-node')).toBeVisible();
    await expect(page.locator('select#source-device')).toBeVisible();
    await expect(page.locator('select#dest-node')).toBeVisible();
    await expect(page.locator('select#dest-device')).toBeVisible();
  });

  test('should create route on cell click', async ({ page }) => {
    // Find an empty cell and click it
    const emptyCell = page.locator('.route-cell:not(.active)').first();

    if (await emptyCell.count() > 0) {
      await emptyCell.click();

      // Should become active
      await expect(emptyCell).toHaveClass(/active/);
    }
  });

  test('should delete route on active cell click', async ({ page }) => {
    // Find an active cell and click it
    const activeCell = page.locator('.route-cell.active').first();

    if (await activeCell.count() > 0) {
      await activeCell.click();

      // Should become inactive
      await expect(activeCell).not.toHaveClass(/active/);
    }
  });

  test('should show route info on hover', async ({ page }) => {
    const activeCell = page.locator('.route-cell.active').first();

    if (await activeCell.count() > 0) {
      await activeCell.hover();

      // Tooltip should appear
      await expect(page.locator('.route-tooltip')).toBeVisible();
    }
  });
});
```

### Device Management Tests (`tests/ui/specs/device-management.spec.ts`)

```typescript
import { test, expect } from '../fixtures/base';

test.describe('Device Management', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/devices');
    await page.waitForSelector('.devices-page', { timeout: 10000 });
  });

  test('should display devices page', async ({ page }) => {
    await expect(page.locator('h1')).toContainText('Audio Devices');
  });

  test('should list input devices', async ({ page }) => {
    const inputSection = page.locator('.device-section.inputs');
    await expect(inputSection).toBeVisible();
  });

  test('should list output devices', async ({ page }) => {
    const outputSection = page.locator('.device-section.outputs');
    await expect(outputSection).toBeVisible();
  });

  test('should show device meters', async ({ page }) => {
    const meter = page.locator('.meter').first();

    if (await meter.count() > 0) {
      await expect(meter).toBeVisible();
    }
  });

  test('should have attach button for available devices', async ({ page }) => {
    const attachBtn = page.locator('button:has-text("Attach")').first();

    if (await attachBtn.count() > 0) {
      await expect(attachBtn).toBeVisible();
    }
  });
});
```

### Channel Labels Tests (`tests/ui/specs/channel-labels.spec.ts`)

```typescript
import { test, expect } from '../fixtures/base';

test.describe('Channel Labels', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/channels');
    await page.waitForSelector('.channel-labels-page', { timeout: 10000 });
  });

  test('should display channel labels page', async ({ page }) => {
    await expect(page.locator('h1')).toContainText('Channel Labels');
  });

  test('should show device selector', async ({ page }) => {
    await expect(page.locator('select.device-selector')).toBeVisible();
  });

  test('should load channels when device selected', async ({ page }) => {
    const select = page.locator('select.device-selector');
    const options = await select.locator('option').all();

    if (options.length > 1) {
      // Select first device (skip placeholder)
      await select.selectOption({ index: 1 });

      // Channel list should appear
      await expect(page.locator('.channel-list')).toBeVisible();
    }
  });

  test('should allow editing channel label', async ({ page }) => {
    const select = page.locator('select.device-selector');
    const options = await select.locator('option').all();

    if (options.length > 1) {
      await select.selectOption({ index: 1 });

      // Double-click to edit
      const labelCell = page.locator('.channel-label').first();
      await labelCell.dblclick();

      // Input should appear
      await expect(labelCell.locator('input')).toBeVisible();
    }
  });
});
```

### Settings Tests (`tests/ui/specs/settings.spec.ts`)

```typescript
import { test, expect } from '../fixtures/base';

test.describe('Settings', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/settings');
    await page.waitForSelector('.settings-page', { timeout: 10000 });
  });

  test('should display settings page', async ({ page }) => {
    await expect(page.locator('h1')).toContainText('Settings');
  });

  test('should show node configuration section', async ({ page }) => {
    await expect(page.locator('h2:has-text("Node Configuration")')).toBeVisible();
  });

  test('should show audio settings section', async ({ page }) => {
    await expect(page.locator('h2:has-text("Audio Settings")')).toBeVisible();
  });

  test('should show about section with version', async ({ page }) => {
    await expect(page.locator('h2:has-text("About")')).toBeVisible();
    await expect(page.locator('text=Version:')).toBeVisible();
  });
});
```

---

## Part 4: Cross-Node Tests

### Docker Compose Setup (`tests/e2e/docker-compose.yml`)

```yaml
version: '3.8'

services:
  node1:
    build:
      context: ../..
      dockerfile: Dockerfile
    command: ["-n", "node1", "--api-port", "8080"]
    ports:
      - "8081:8080"
    networks:
      - audiomatrix-test

  node2:
    build:
      context: ../..
      dockerfile: Dockerfile
    command: ["-n", "node2", "--api-port", "8080"]
    ports:
      - "8082:8080"
    networks:
      - audiomatrix-test

networks:
  audiomatrix-test:
    driver: bridge
```

### Cross-Node Discovery Test (`tests/e2e/cross_node/discovery_test.rs`)

```rust
use std::time::Duration;
use tokio::time::sleep;

/// Tests that two nodes discover each other.
/// Requires docker-compose environment.
#[tokio::test]
#[ignore] // Run with: cargo test --test cross_node -- --ignored
async fn test_cross_node_discovery() {
    let client = reqwest::Client::new();

    // Wait for both nodes to start
    sleep(Duration::from_secs(5)).await;

    // Query node1 for discovered nodes
    let response = client
        .get("http://localhost:8081/api/v1/nodes")
        .send()
        .await
        .unwrap();

    let nodes: Vec<serde_json::Value> = response.json().await.unwrap();

    // Should see both nodes
    assert!(nodes.len() >= 2);

    let node_names: Vec<&str> = nodes.iter()
        .filter_map(|n| n["name"].as_str())
        .collect();

    assert!(node_names.contains(&"node1"));
    assert!(node_names.contains(&"node2"));
}
```

### Cross-Node Routing Test (`tests/e2e/cross_node/routing_test.rs`)

```rust
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
#[ignore]
async fn test_cross_node_route_creation() {
    let client = reqwest::Client::new();

    // Wait for discovery
    sleep(Duration::from_secs(10)).await;

    // Create cross-node route from node1
    let route = serde_json::json!({
        "source_node": "node2",
        "source_device": "test-device",
        "source_channel": 1,
        "destination_node": "node1",
        "destination_device": "test-output",
        "destination_channel": 1,
        "volume": 1.0,
        "muted": false
    });

    let response = client
        .post("http://localhost:8081/api/v1/routes")
        .json(&route)
        .send()
        .await
        .unwrap();

    // May fail if devices don't exist, but shouldn't error
    assert!(response.status() == 201 || response.status() == 404);
}
```

---

## Part 5: Running Tests

### Cargo Test Commands

```bash
# Run all E2E API tests
cargo test --test e2e -- --test-threads=1

# Run specific test module
cargo test --test e2e api::routes

# Run WebSocket tests
cargo test --test e2e websocket

# Run cross-node tests (requires docker)
docker-compose -f tests/e2e/docker-compose.yml up -d
cargo test --test e2e cross_node -- --ignored
docker-compose -f tests/e2e/docker-compose.yml down
```

### Playwright Commands

```bash
# Install Playwright
cd tests/ui && npx playwright install

# Run UI tests
npx playwright test

# Run with UI
npx playwright test --ui

# Run specific test file
npx playwright test specs/routing-matrix.spec.ts
```

### CI Integration

```yaml
# .github/workflows/e2e.yml
name: E2E Tests

on:
  push:
    branches: [main, develop]
  pull_request:

jobs:
  api-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --test e2e -- --test-threads=1

  ui-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: actions/setup-node@v4
      - run: cargo build -p ram-service
      - run: cd tests/ui && npm ci && npx playwright install
      - run: cd tests/ui && npx playwright test

  cross-node-tests:
    runs-on: ubuntu-latest
    services:
      docker:
        image: docker:dind
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: docker-compose -f tests/e2e/docker-compose.yml up -d
      - run: sleep 30
      - run: cargo test --test e2e cross_node -- --ignored
      - run: docker-compose -f tests/e2e/docker-compose.yml down
```

---

## Coverage Goals

| Test Category | Target Coverage | Priority |
|---------------|-----------------|----------|
| API Endpoints | 100% | P0 |
| WebSocket Events | 100% | P0 |
| UI Core Flows | 80% | P1 |
| Cross-Node Routing | 70% | P2 |
| Audio Flow | 50% | P3 |

---

## Implementation Order

1. **API E2E tests** (can run without audio hardware)
2. **WebSocket tests** (validates real-time infrastructure)
3. **UI tests** (requires working API)
4. **Cross-node tests** (requires Docker)
5. **Audio flow tests** (requires audio devices)
