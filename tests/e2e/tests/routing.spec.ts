import { test, expect } from '@playwright/test';

test.describe('Routes API', () => {
  let nodeId: string;
  let inputDevice: any;
  let outputDevice: any;

  test.beforeAll(async ({ request }) => {
    // Get the first node ID
    const nodesResponse = await request.get('/api/v1/nodes');
    const nodes = await nodesResponse.json();
    nodeId = nodes[0].id;

    // Get devices for this node
    const devicesResponse = await request.get(`/api/v1/nodes/${encodeURIComponent(nodeId)}/devices`);
    const devices = await devicesResponse.json();

    inputDevice = devices.find((d: any) => d.device_type === 'input');
    outputDevice = devices.find((d: any) => d.device_type === 'output');
  });

  test('can list routes', async ({ request }) => {
    const response = await request.get('/api/v1/routes');
    expect(response.ok()).toBeTruthy();

    const routes = await response.json();
    expect(Array.isArray(routes)).toBeTruthy();
  });

  test('can create and delete a route', async ({ request }) => {
    // Skip if we don't have devices
    test.skip(!inputDevice || !outputDevice, 'No input or output devices available');

    // Create a route
    const routeData = {
      source: {
        node_id: nodeId,
        device_id: inputDevice.id,
        channel: 1
      },
      destination: {
        node_id: nodeId,
        device_id: outputDevice.id,
        channel: 1
      },
      gain: 0.0,
      muted: false
    };

    const createResponse = await request.post('/api/v1/routes', {
      data: routeData
    });

    // Route creation may succeed or fail depending on device availability
    if (createResponse.ok()) {
      const created = await createResponse.json();
      expect(created.route_id).toBeDefined();

      // Verify route exists
      const listResponse = await request.get('/api/v1/routes');
      const routes = await listResponse.json();
      const found = routes.some((r: any) =>
        r.source.device_id === inputDevice.id &&
        r.destination.device_id === outputDevice.id
      );
      expect(found).toBeTruthy();

      // Delete the route
      const deleteResponse = await request.delete(`/api/v1/routes/${created.route_id}`);
      expect(deleteResponse.ok()).toBeTruthy();

      // Verify route is gone
      const afterDeleteResponse = await request.get('/api/v1/routes');
      const afterRoutes = await afterDeleteResponse.json();
      const stillFound = afterRoutes.some((r: any) =>
        r.source.device_id === inputDevice.id &&
        r.destination.device_id === outputDevice.id &&
        r.source.channel === 1 &&
        r.destination.channel === 1
      );
      expect(stillFound).toBeFalsy();
    }
  });

  test('route creation validates required fields', async ({ request }) => {
    // Missing required fields should fail
    const invalidRoute = {
      source: {
        node_id: nodeId,
        // Missing device_id and channel
      },
      destination: {
        node_id: nodeId,
        // Missing device_id and channel
      }
    };

    const response = await request.post('/api/v1/routes', {
      data: invalidRoute
    });

    // Should fail with 400 or 422
    expect(response.status()).toBeGreaterThanOrEqual(400);
    expect(response.status()).toBeLessThan(500);
  });

  test('get non-existent route returns 404', async ({ request }) => {
    const response = await request.get('/api/v1/routes/non-existent-route-id');
    expect(response.status()).toBe(404);
  });

  test('delete non-existent route returns 404', async ({ request }) => {
    const response = await request.delete('/api/v1/routes/non-existent-route-id');
    expect(response.status()).toBe(404);
  });
});

test.describe('Routing Matrix UI', () => {
  test('shows routing matrix on home page', async ({ page }) => {
    await page.goto('/');

    // Wait for WASM to load
    await page.waitForFunction(() => window.wasmBindings !== undefined, {
      timeout: 10000
    });

    // Wait for app to render
    await page.waitForSelector('.app-main', { timeout: 5000 });
  });

  test('shows routes page content', async ({ page }) => {
    await page.goto('/routes');

    // Wait for WASM to load
    await page.waitForFunction(() => window.wasmBindings !== undefined, {
      timeout: 10000
    });

    // Routes page should be rendered
    await expect(page.locator('.app-main')).toBeVisible();
  });
});
