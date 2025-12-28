import { test, expect } from '@playwright/test';

test.describe('Devices API', () => {
  let nodeId: string;

  test.beforeAll(async ({ request }) => {
    // Get the first node ID
    const response = await request.get('/api/v1/nodes');
    const nodes = await response.json();
    nodeId = encodeURIComponent(nodes[0].id);
  });

  test('lists devices for a node', async ({ request }) => {
    const response = await request.get(`/api/v1/nodes/${nodeId}/devices`);
    expect(response.ok()).toBeTruthy();

    const devices = await response.json();
    expect(Array.isArray(devices)).toBeTruthy();
    expect(devices.length).toBeGreaterThan(0);

    // Verify device structure
    const device = devices[0];
    expect(device.id).toBeDefined();
    expect(device.name).toBeDefined();
    expect(device.device_type).toMatch(/^(input|output)$/);
    expect(device.channels).toBeGreaterThan(0);
    expect(device.sample_rate).toBeGreaterThan(0);
    expect(typeof device.is_virtual).toBe('boolean');
  });

  test('has input devices', async ({ request }) => {
    const response = await request.get(`/api/v1/nodes/${nodeId}/devices`);
    const devices = await response.json();

    const inputDevices = devices.filter((d: any) => d.device_type === 'input');
    expect(inputDevices.length).toBeGreaterThan(0);
  });

  test('has output devices', async ({ request }) => {
    const response = await request.get(`/api/v1/nodes/${nodeId}/devices`);
    const devices = await response.json();

    const outputDevices = devices.filter((d: any) => d.device_type === 'output');
    expect(outputDevices.length).toBeGreaterThan(0);
  });

  test('device IDs are unique', async ({ request }) => {
    const response = await request.get(`/api/v1/nodes/${nodeId}/devices`);
    const devices = await response.json();

    const ids = devices.map((d: any) => d.id);
    const uniqueIds = new Set(ids);
    expect(uniqueIds.size).toBe(ids.length);
  });
});

test.describe('Devices Page UI', () => {
  test('shows devices page content', async ({ page }) => {
    await page.goto('/devices');

    // Wait for WASM to load
    await page.waitForFunction(() => window.wasmBindings !== undefined, {
      timeout: 10000
    });

    // The devices page should be rendered
    await expect(page.locator('.app-main')).toBeVisible();
  });
});
