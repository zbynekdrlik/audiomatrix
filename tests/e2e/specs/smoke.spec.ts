import { test, expect, collectConsoleLogs, waitForConsoleMessage } from '../fixtures/audiomatrix';

test.describe('Smoke Tests', () => {
  test('API health check returns ok', async ({ getHealth }) => {
    const health = await getHealth();
    expect(health.status).toBe('ok');
    expect(health.version).toMatch(/^\d+\.\d+\.\d+/);
    expect(health.hostname).toBeTruthy();
  });

  test('home page loads and displays routing matrix', async ({ page, navigateTo }) => {
    await navigateTo('/');

    // Check header is present
    await expect(page.getByRole('heading', { name: 'AudioMatrix', level: 1 })).toBeVisible();

    // Check navigation links
    await expect(page.getByRole('link', { name: 'Matrix' })).toBeVisible();
    await expect(page.getByRole('link', { name: 'Devices' })).toBeVisible();
    await expect(page.getByRole('link', { name: 'Routes' })).toBeVisible();
    await expect(page.getByRole('link', { name: 'Settings' })).toBeVisible();
  });

  test('devices page loads and shows device list', async ({ page, navigateTo, waitForDevices }) => {
    await navigateTo('/devices');

    // Wait for devices to load
    await waitForDevices();

    // Check page title
    await expect(page.getByRole('heading', { name: 'Audio Devices', level: 1 })).toBeVisible();

    // Check section headers exist (at least one attached or available section)
    const hasAttached = await page.getByRole('heading', { name: /Attached/i }).count() > 0;
    const hasAvailable = await page.getByRole('heading', { name: /Available/i }).count() > 0;
    expect(hasAttached || hasAvailable).toBeTruthy();
  });

  test('routes page loads', async ({ page, navigateTo }) => {
    await navigateTo('/routes');

    // Check page has route-related content (main element should be present)
    await expect(page.getByRole('main')).toBeVisible();
  });

  test('settings page loads', async ({ page, navigateTo }) => {
    await navigateTo('/settings');

    // Check settings heading is present
    await expect(page.getByRole('heading', { name: 'Settings', level: 1 })).toBeVisible();
  });

  test('WebSocket connects successfully', async ({ page, navigateTo, waitForWebSocket }) => {
    await navigateTo('/devices');

    // Wait for WebSocket to connect
    await waitForWebSocket();

    // Verify no WebSocket errors in console
    const logs = await collectConsoleLogs(page, 2000);
    const errors = logs.filter(log => log.includes('WebSocket error') || log.includes('Failed to parse'));
    expect(errors).toHaveLength(0);
  });

  test('WebSocket receives node status on connect', async ({ page, navigateTo }) => {
    // Set up console listener before navigation
    const nodeStatusPromise = waitForConsoleMessage(page, /Node .+ online: true/, 10000);

    await navigateTo('/devices');

    // Wait for node status message
    const message = await nodeStatusPromise;
    expect(message).not.toBeNull();
    expect(message).toMatch(/Node .+@.+ online: true/);
  });

  test('navigation between pages works', async ({ page, navigateTo }) => {
    // Start at home
    await navigateTo('/');
    await expect(page).toHaveURL(/\/$/);

    // Navigate to devices
    await page.click('a[href="/devices"]');
    await expect(page).toHaveURL(/\/devices$/);
    await expect(page.locator('h1:has-text("Audio Devices")')).toBeVisible();

    // Navigate to routes
    await page.click('a[href="/routes"]');
    await expect(page).toHaveURL(/\/routes$/);

    // Navigate back to matrix
    await page.click('a[href="/"]');
    await expect(page).toHaveURL(/\/$/);
  });

  test('version is displayed in header', async ({ page, navigateTo, getHealth }) => {
    await navigateTo('/');

    // Get version from API
    const health = await getHealth();

    // Check version is displayed (partial match, UI shows "v0.1.0-dev" not full version)
    await expect(page.locator(':text("v0.1.0"), :text("0.1.0")')).toBeVisible();
  });
});
