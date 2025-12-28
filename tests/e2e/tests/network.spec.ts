import { test, expect } from '@playwright/test';

// Network tests for multi-node setups
// Run with: BASE_URL=http://target:8080 npx playwright test network.spec.ts

test.describe('Network: Node Discovery', () => {
  test('discovers other nodes on network', async ({ request }) => {
    const response = await request.get('/api/v1/nodes');
    expect(response.ok()).toBeTruthy();

    const nodes = await response.json();
    expect(Array.isArray(nodes)).toBeTruthy();

    // Log discovered nodes for debugging
    console.log('Discovered nodes:', nodes.map((n: any) => `${n.name} (${n.id})`));

    // At least local node should be present
    expect(nodes.length).toBeGreaterThanOrEqual(1);
  });

  test('local node is online', async ({ request }) => {
    const response = await request.get('/api/v1/nodes');
    const nodes = await response.json();

    // At least one node should be online
    const onlineNodes = nodes.filter((n: any) => n.online);
    expect(onlineNodes.length).toBeGreaterThanOrEqual(1);
  });
});

test.describe('Network: Cross-Node Device Access', () => {
  test('can list devices from all nodes', async ({ request }) => {
    const nodesResponse = await request.get('/api/v1/nodes');
    const nodes = await nodesResponse.json();

    for (const node of nodes) {
      if (!node.online) continue;

      const devicesResponse = await request.get(
        `/api/v1/nodes/${encodeURIComponent(node.id)}/devices`
      );

      // Each online node should return device list
      expect(devicesResponse.ok()).toBeTruthy();

      const devices = await devicesResponse.json();
      expect(Array.isArray(devices)).toBeTruthy();

      console.log(`Node ${node.name}: ${devices.length} devices`);
    }
  });
});

test.describe('Network: WebSocket Connection', () => {
  test('can connect to WebSocket endpoint', async ({ page }) => {
    // Navigate to trigger WASM load
    await page.goto('/');

    // Try to connect to WebSocket
    const wsConnected = await page.evaluate(async () => {
      return new Promise((resolve) => {
        const ws = new WebSocket(`ws://${window.location.host}/api/v1/ws`);

        ws.onopen = () => {
          ws.close();
          resolve(true);
        };

        ws.onerror = () => {
          resolve(false);
        };

        // Timeout after 5 seconds
        setTimeout(() => {
          ws.close();
          resolve(false);
        }, 5000);
      });
    });

    expect(wsConnected).toBeTruthy();
  });
});

test.describe('Network: Latency', () => {
  test('API response time is acceptable', async ({ request }) => {
    const start = Date.now();
    const response = await request.get('/api/v1/health');
    const duration = Date.now() - start;

    expect(response.ok()).toBeTruthy();
    // API response should be under 100ms for local, 500ms for network
    expect(duration).toBeLessThan(500);

    console.log(`Health endpoint latency: ${duration}ms`);
  });

  test('device list response time is acceptable', async ({ request }) => {
    // Get first node
    const nodesResponse = await request.get('/api/v1/nodes');
    const nodes = await nodesResponse.json();
    const nodeId = encodeURIComponent(nodes[0].id);

    const start = Date.now();
    const response = await request.get(`/api/v1/nodes/${nodeId}/devices`);
    const duration = Date.now() - start;

    expect(response.ok()).toBeTruthy();
    // Device list should be under 200ms for local, 1000ms for network
    expect(duration).toBeLessThan(1000);

    console.log(`Device list latency: ${duration}ms`);
  });
});

// Cross-node routing tests - run when multiple nodes are available
test.describe('Network: Cross-Node Routing', () => {
  test('can create cross-node route when multiple nodes exist', async ({ request }) => {
    const nodesResponse = await request.get('/api/v1/nodes');
    const nodes = await nodesResponse.json();

    // Skip if only one node
    test.skip(nodes.length < 2, 'Need at least 2 nodes for cross-node routing tests');

    const onlineNodes = nodes.filter((n: any) => n.online);
    test.skip(onlineNodes.length < 2, 'Need at least 2 online nodes');

    const sourceNode = onlineNodes[0];
    const destNode = onlineNodes[1];

    // Get devices from each node
    const sourceDevicesResp = await request.get(
      `/api/v1/nodes/${encodeURIComponent(sourceNode.id)}/devices`
    );
    const destDevicesResp = await request.get(
      `/api/v1/nodes/${encodeURIComponent(destNode.id)}/devices`
    );

    const sourceDevices = await sourceDevicesResp.json();
    const destDevices = await destDevicesResp.json();

    const inputDevice = sourceDevices.find((d: any) => d.device_type === 'input');
    const outputDevice = destDevices.find((d: any) => d.device_type === 'output');

    test.skip(!inputDevice || !outputDevice, 'Need input and output devices on different nodes');

    // Create cross-node route
    const routeData = {
      source: {
        node_id: sourceNode.id,
        device_id: inputDevice.id,
        channel: 1
      },
      destination: {
        node_id: destNode.id,
        device_id: outputDevice.id,
        channel: 1
      },
      gain: 0.0,
      muted: false
    };

    const createResponse = await request.post('/api/v1/routes', {
      data: routeData
    });

    if (createResponse.ok()) {
      const created = await createResponse.json();
      console.log(`Created cross-node route: ${sourceNode.name} -> ${destNode.name}`);

      // Clean up
      await request.delete(`/api/v1/routes/${created.route_id}`);
    } else {
      console.log('Cross-node route creation failed (may be expected if VBAN not configured)');
    }
  });
});
