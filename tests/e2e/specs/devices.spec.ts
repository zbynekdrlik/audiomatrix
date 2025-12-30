import { test, expect, waitForConsoleMessage } from '../fixtures/audiomatrix';

test.describe('Device Management', () => {
  test.beforeEach(async ({ navigateTo, waitForDevices, waitForWebSocket }) => {
    await navigateTo('/devices');
    await waitForDevices();
    await waitForWebSocket();
  });

  test('displays available input devices', async ({ page }) => {
    // Check for Available Input Devices section
    const inputSection = page.locator('h2:has-text("Available Input")').locator('..');

    // Should have at least one device card or "no devices" message
    // Device cards have Attach/Detach buttons, "ch" indicator, and Hz sample rate
    const hasDevices = await inputSection.locator('button:has-text("Attach"), button:has-text("Detach")').count() > 0;
    const hasMessage = await inputSection.locator('text=/No available input/i').count() > 0;

    expect(hasDevices || hasMessage).toBeTruthy();
  });

  test('displays available output devices', async ({ page }) => {
    // Check for Available Output Devices section
    const outputSection = page.locator('h2:has-text("Available Output")').locator('..');

    // Should have at least one device card or "no devices" message
    const hasDevices = await outputSection.locator('button:has-text("Attach"), button:has-text("Detach")').count() > 0;
    const hasMessage = await outputSection.locator('text=/No available output/i').count() > 0;

    expect(hasDevices || hasMessage).toBeTruthy();
  });

  test('device card shows device info', async ({ page }) => {
    // Find any device card
    const card = page.locator('.device-card').first();

    // Skip if no devices
    if (await card.count() === 0) {
      test.skip();
      return;
    }

    // Check device card has name (look for any text that's likely a device name)
    const nameElement = card.locator('.device-name, h3, h4').first();
    await expect(nameElement).toBeVisible();
    const nameText = await nameElement.textContent();
    expect(nameText?.length).toBeGreaterThan(0);

    // Check device card has status (use first() to avoid strict mode)
    const status = card.locator('.device-status, .status').first();
    await expect(status).toBeVisible();

    // Check device card has channel info (look for specific class or "X ch" pattern)
    const channelInfo = card.locator('.device-channels, :text-matches("\\\\d+ ch")').first();
    await expect(channelInfo).toBeVisible();
  });

  test('attach device button works', async ({ page, attachDevice, getDeviceCards }) => {
    // Find an available input device
    const availableSection = page.locator('h2:has-text("Available Input")').locator('..');
    const availableCards = availableSection.locator('.device-card');

    if (await availableCards.count() === 0) {
      test.skip();
      return;
    }

    // Get first device name
    const firstCard = availableCards.first();
    const deviceName = await firstCard.locator('h3, h4, .device-name').first().textContent();

    if (!deviceName) {
      test.skip();
      return;
    }

    // Listen for device_attached event
    const attachedPromise = waitForConsoleMessage(page, /Device attached:/);

    // Attach the device
    await attachDevice(deviceName.trim());

    // Verify event was received
    const attachedMessage = await attachedPromise;
    expect(attachedMessage).toContain(deviceName.trim().substring(0, 20));

    // Verify device moved to Attached section
    const attachedSection = page.locator('h2:has-text("Attached Input")').locator('..');
    await expect(attachedSection.locator(`.device-card:has-text("${deviceName.trim()}")`)).toBeVisible();
  });

  test('detach device button works', async ({ page, attachDevice, detachDevice }) => {
    // First, attach a device
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

    // Attach then detach
    await attachDevice(deviceName.trim());
    await page.waitForTimeout(500);
    await detachDevice(deviceName.trim());

    // Verify device is back in Available section
    await page.waitForTimeout(500);
    await expect(availableSection.locator(`.device-card:has-text("${deviceName.trim()}")`)).toBeVisible();
  });

  test('device meters show channel numbers', async ({ page }) => {
    // Find any device card with meters
    const card = page.locator('.device-card').first();

    if (await card.count() === 0) {
      test.skip();
      return;
    }

    // Check for channel numbers (meter labels)
    const meters = card.locator('.meter, .channel-meter, [class*="meter"]');
    if (await meters.count() > 0) {
      // At least some channel indicators should be present
      const channelLabels = card.locator(':text("1"), :text("2")');
      expect(await channelLabels.count()).toBeGreaterThan(0);
    }
  });

  test('create virtual device button is visible', async ({ page }) => {
    const createButton = page.locator('button:has-text("Create Virtual Device")');
    await expect(createButton).toBeVisible();
  });

  test('virtual device dialog opens', async ({ page }) => {
    const createButton = page.getByRole('button', { name: /Create Virtual Device/i });
    await createButton.click();

    // Wait for dialog content to appear (look for the dialog heading)
    await page.waitForTimeout(500);
    const dialogHeading = page.getByRole('heading', { name: /Create Virtual Device/i });
    await expect(dialogHeading).toBeVisible();

    // Check dialog has name input field
    const nameInput = page.getByLabel(/name/i).first();
    await expect(nameInput).toBeVisible();
  });

  test('virtual device dialog can be closed', async ({ page }) => {
    const createButton = page.locator('button:has-text("Create Virtual Device")');
    await createButton.click();

    // Wait for dialog
    await page.waitForTimeout(500);

    // Find close/cancel button
    const closeButton = page.locator('button:has-text("Cancel"), button:has-text("Close"), button[aria-label="Close"]');

    if (await closeButton.count() > 0) {
      await closeButton.click();
      await expect(page.locator('[role="dialog"]:visible, .dialog:visible')).toHaveCount(0);
    } else {
      // Press Escape to close
      await page.keyboard.press('Escape');
      await page.waitForTimeout(300);
    }
  });
});
