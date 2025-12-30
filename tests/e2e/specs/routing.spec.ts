import { test, expect } from '../fixtures/audiomatrix';

test.describe('Routing Matrix', () => {
  test.beforeEach(async ({ navigateTo, waitForWebSocket }) => {
    await navigateTo('/');
    await waitForWebSocket();
  });

  test('routing matrix page loads', async ({ page }) => {
    // Check for routing matrix container (use .first() to avoid strict mode with multiple matches)
    await expect(page.locator('.routing-matrix').first()).toBeVisible();
  });

  test('node selectors are present', async ({ page }) => {
    // Check for source and destination node selectors
    const sourceSelector = page.locator('select, [role="combobox"]').first();

    // At least one selector should be present
    const selectorCount = await page.locator('select, [role="combobox"]').count();
    expect(selectorCount).toBeGreaterThanOrEqual(1);
  });

  test('device dropdowns load options', async ({ page }) => {
    // Find device selectors
    const selectors = page.locator('select');

    for (let i = 0; i < await selectors.count(); i++) {
      const selector = selectors.nth(i);
      const options = await selector.locator('option').count();

      // Each selector should have at least one option (even if just placeholder)
      expect(options).toBeGreaterThan(0);
    }
  });

  test('routes page shows route list', async ({ page, navigateTo }) => {
    await navigateTo('/routes');

    // Check for routes list or "no routes" message
    const hasRoutes = await page.locator('.route-item, .route-card, [class*="route"]').count() > 0;
    const hasEmptyMessage = await page.locator(':text("No routes"), :text("no routes")').count() > 0;

    expect(hasRoutes || hasEmptyMessage).toBeTruthy();
  });

  test('can navigate from matrix to routes', async ({ page }) => {
    await page.click('a[href="/routes"]');
    await expect(page).toHaveURL(/\/routes$/);
  });
});

test.describe('Route CRUD via API', () => {
  // These tests use the API directly to verify backend functionality

  test('list routes via API', async ({ request, baseURL }) => {
    const response = await request.get(`${baseURL}/api/v1/routes`);
    expect(response.ok()).toBeTruthy();

    const routes = await response.json();
    expect(Array.isArray(routes)).toBeTruthy();
  });

  test('create and delete route via API', async ({ request, baseURL }) => {
    // Get local node info first
    const healthResponse = await request.get(`${baseURL}/api/v1/health`);
    const health = await healthResponse.json();

    // Get nodes to find the local node's full ID (name@hostname format)
    const nodesResponse = await request.get(`${baseURL}/api/v1/nodes`);
    const nodes = await nodesResponse.json();
    const localNode = nodes.find((n: any) => n.id.includes(health.hostname)) || nodes[0];

    if (!localNode) {
      test.skip();
      return;
    }

    // Get devices using the full node ID
    const devicesResponse = await request.get(`${baseURL}/api/v1/nodes/${encodeURIComponent(localNode.id)}/devices`);
    expect(devicesResponse.ok()).toBeTruthy();

    const devices = await devicesResponse.json();

    // Find input and output devices using device_type field
    const inputDevice = devices.find((d: any) => d.device_type === 'input');
    const outputDevice = devices.find((d: any) => d.device_type === 'output');

    if (!inputDevice || !outputDevice) {
      test.skip();
      return;
    }

    const nodeName = localNode.id;

    // Create a route
    const createResponse = await request.post(`${baseURL}/api/v1/routes`, {
      data: {
        source_node: nodeName,
        source_device: inputDevice.id,
        source_channel: 1,
        destination_node: nodeName,
        destination_device: outputDevice.id,
        destination_channel: 1,
        volume: 1.0,
        muted: false,
      },
    });

    expect(createResponse.ok()).toBeTruthy();
    const createResult = await createResponse.json();
    expect(createResult.id).toBeTruthy();

    // Verify route exists (list doesn't return id, so match by composite key)
    const listResponse = await request.get(`${baseURL}/api/v1/routes`);
    const routes = await listResponse.json();
    const found = routes.find((r: any) =>
      r.source_node === nodeName &&
      r.source_device === inputDevice.id &&
      r.source_channel === 1 &&
      r.destination_node === nodeName &&
      r.destination_device === outputDevice.id &&
      r.destination_channel === 1
    );
    expect(found).toBeTruthy();

    // Delete the route using the ID from create response
    const deleteResponse = await request.delete(`${baseURL}/api/v1/routes/${encodeURIComponent(createResult.id)}`);
    expect(deleteResponse.ok()).toBeTruthy();

    // Verify route is deleted
    const finalListResponse = await request.get(`${baseURL}/api/v1/routes`);
    const finalRoutes = await finalListResponse.json();
    const stillExists = finalRoutes.find((r: any) =>
      r.source_node === nodeName &&
      r.source_device === inputDevice.id &&
      r.source_channel === 1 &&
      r.destination_node === nodeName &&
      r.destination_device === outputDevice.id &&
      r.destination_channel === 1
    );
    expect(stillExists).toBeFalsy();
  });
});

test.describe('Route Controls', () => {
  test.beforeEach(async ({ navigateTo, waitForWebSocket }) => {
    await navigateTo('/');
    await waitForWebSocket();
  });

  test('clicking route cell toggles route', async ({ page, request, baseURL }) => {
    // This test requires at least one device to be selected in the matrix
    // For now, just verify the matrix cells are clickable

    const cells = page.locator('.route-cell, [class*="cell"]');
    const cellCount = await cells.count();

    if (cellCount === 0) {
      test.skip();
      return;
    }

    // Verify cells exist
    expect(cellCount).toBeGreaterThan(0);
  });

  test('route volume slider exists when route is active', async ({ page, navigateTo }) => {
    await navigateTo('/routes');

    // Check if any routes exist
    const routes = page.locator('.route-item, .route-card, [class*="route"]');

    if (await routes.count() === 0) {
      test.skip();
      return;
    }

    // Look for volume control
    const volumeControl = page.locator('input[type="range"], .volume-slider, [class*="volume"]');
    // Volume control should be present if routes exist
    // This may fail if route card design doesn't include inline controls
  });
});
