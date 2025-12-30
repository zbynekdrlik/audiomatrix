import { test, expect, collectConsoleLogs, waitForConsoleMessage } from '../fixtures/audiomatrix';

test.describe('WebSocket Events', () => {
  test('connects to WebSocket on page load', async ({ page, navigateTo }) => {
    // Listen for connection message
    const connectPromise = waitForConsoleMessage(page, /WebSocket connected/, 10000);

    await navigateTo('/devices');

    const message = await connectPromise;
    expect(message).not.toBeNull();
    expect(message).toContain('WebSocket connected');
  });

  test('receives node_status event on connect', async ({ page, navigateTo }) => {
    // Listen for node status
    const nodeStatusPromise = waitForConsoleMessage(page, /Node .+ online: true/, 10000);

    await navigateTo('/devices');

    const message = await nodeStatusPromise;
    expect(message).not.toBeNull();
    expect(message).toMatch(/Node .+@.+ online: true/);
  });

  test('no WebSocket parse errors', async ({ page, navigateTo, waitForWebSocket }) => {
    await navigateTo('/devices');
    await waitForWebSocket();

    // Collect logs for a few seconds
    const logs = await collectConsoleLogs(page, 3000);

    // Check for parse errors
    const parseErrors = logs.filter(log =>
      log.includes('Failed to parse') ||
      log.includes('missing field') ||
      log.includes('unknown variant')
    );

    expect(parseErrors).toHaveLength(0);
  });

  test('receives device_attached event when attaching device', async ({ page, navigateTo, waitForWebSocket, attachDevice }) => {
    await navigateTo('/devices');
    await waitForWebSocket();

    // Find an available device
    const availableSection = page.locator('h2:has-text("Available Input")').locator('..');
    const availableCards = availableSection.locator('.device-card');

    if (await availableCards.count() === 0) {
      test.skip();
      return;
    }

    const firstCard = availableCards.first();
    const deviceName = await firstCard.locator('h3, h4, .device-name').first().textContent();

    if (!deviceName) {
      test.skip();
      return;
    }

    // Set up listener for device_attached event
    const attachedPromise = waitForConsoleMessage(page, /Device attached:/, 5000);

    // Attach device
    await attachDevice(deviceName.trim());

    // Verify event received
    const message = await attachedPromise;
    expect(message).not.toBeNull();
    expect(message).toContain('Device attached');
  });

  test('metering subscription sent when device attached', async ({ page, navigateTo, waitForWebSocket, attachDevice }) => {
    await navigateTo('/devices');
    await waitForWebSocket();

    // Find an available device
    const availableSection = page.locator('h2:has-text("Available Input")').locator('..');
    const availableCards = availableSection.locator('.device-card');

    if (await availableCards.count() === 0) {
      test.skip();
      return;
    }

    const firstCard = availableCards.first();
    const deviceName = await firstCard.locator('h3, h4, .device-name').first().textContent();

    if (!deviceName) {
      test.skip();
      return;
    }

    // Set up listener for subscription message
    const subscribePromise = waitForConsoleMessage(page, /Subscribing to metering/, 5000);

    // Attach device
    await attachDevice(deviceName.trim());

    // Verify subscription was sent
    const message = await subscribePromise;
    expect(message).not.toBeNull();
    expect(message).toContain('Subscribing to metering');
  });

  test('WebSocket status indicator updates', async ({ page, navigateTo }) => {
    await navigateTo('/devices');

    // Wait for connection
    await page.waitForTimeout(2000);

    // Check for connected status in UI
    const statusIndicator = page.locator('[title*="WebSocket"], :text("WS"), .ws-status');
    await expect(statusIndicator).toBeVisible();

    // Should show connected state
    const connectedIndicator = page.locator('[title*="Connected"], :text("Connected"), .ws-connected');
    await expect(connectedIndicator).toBeVisible();
  });
});

test.describe('WebSocket Message Format', () => {
  test('node_status event has correct format', async ({ page, navigateTo }) => {
    // Capture the raw message
    let nodeStatusMessage: string | null = null;

    page.on('console', (msg) => {
      const text = msg.text();
      if (text.includes('Node ') && text.includes(' online:')) {
        nodeStatusMessage = text;
      }
    });

    await navigateTo('/devices');
    await page.waitForTimeout(3000);

    expect(nodeStatusMessage).not.toBeNull();
    // Format should be: "Node {name}@{hostname} online: {true/false}"
    expect(nodeStatusMessage).toMatch(/Node .+@.+ online: (true|false)/);
  });

  test('WebSocket reconnects after disconnect', async ({ page, navigateTo, waitForWebSocket }) => {
    await navigateTo('/devices');
    await waitForWebSocket();

    // Simulate network interruption by navigating away and back
    await page.goto('about:blank');
    await page.waitForTimeout(500);
    await navigateTo('/devices');

    // Should reconnect - verify WebSocket indicator shows connected state
    await waitForWebSocket();

    // Verify connection via UI indicator (more reliable than console logs)
    const wsIndicator = page.locator('[title*="Connected"]');
    await expect(wsIndicator).toBeVisible({ timeout: 5000 });
  });
});

test.describe('Metering WebSocket', () => {
  test.skip('metering data received for attached device', async ({ page, navigateTo, waitForWebSocket, attachDevice }) => {
    // This test is skipped by default as it requires actual audio activity
    // Enable for manual testing with real devices

    await navigateTo('/devices');
    await waitForWebSocket();

    // Attach a device
    const availableSection = page.locator('h2:has-text("Available Input")').locator('..');
    const availableCards = availableSection.locator('.device-card');

    if (await availableCards.count() === 0) {
      test.skip();
      return;
    }

    const firstCard = availableCards.first();
    const deviceName = await firstCard.locator('h3, h4, .device-name').first().textContent();

    if (!deviceName) {
      test.skip();
      return;
    }

    await attachDevice(deviceName.trim());

    // Wait for potential metering data
    // Note: Metering only comes when device is actively streaming via a route
    await page.waitForTimeout(5000);

    // This test verifies the subscription was set up correctly
    // Actual metering verification requires creating a route
  });
});
