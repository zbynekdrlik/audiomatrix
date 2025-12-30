import { test as base, expect, Page } from '@playwright/test';

/**
 * AudioMatrix test fixtures and utilities
 */

export interface AudioMatrixFixtures {
  /** Wait for WebSocket to connect */
  waitForWebSocket: () => Promise<void>;
  /** Wait for devices to load */
  waitForDevices: () => Promise<void>;
  /** Get all device cards */
  getDeviceCards: () => Promise<string[]>;
  /** Attach a device by name */
  attachDevice: (deviceName: string) => Promise<void>;
  /** Detach a device by name */
  detachDevice: (deviceName: string) => Promise<void>;
  /** Navigate to a page and wait for it to load */
  navigateTo: (path: string) => Promise<void>;
  /** Get API health status */
  getHealth: () => Promise<{ status: string; version: string; hostname: string }>;
}

export const test = base.extend<AudioMatrixFixtures>({
  waitForWebSocket: async ({ page }, use) => {
    const fn = async () => {
      // Wait for WebSocket status indicator to show "Connected"
      // The status shows as a generic element with title "WebSocket: Connected" and text "WS"
      await page.waitForFunction(() => {
        // Look for WebSocket connected indicator in the page
        const wsIndicator = document.querySelector('[title*="Connected"]');
        if (wsIndicator) return true;
        // Also check for the WS text that appears when connected
        const elements = document.querySelectorAll('*');
        for (const el of elements) {
          if (el.textContent === 'WS' && el.getAttribute('title')?.includes('Connected')) {
            return true;
          }
        }
        return false;
      }, { timeout: 10000 });
    };
    await use(fn);
  },

  waitForDevices: async ({ page }, use) => {
    const fn = async () => {
      // Wait for device list to load - either device cards appear or empty messages
      // First ensure the page structure exists
      await page.waitForSelector('.device-section, .device-list', { timeout: 5000 });

      // Then wait for actual content (cards or empty messages)
      await page.waitForFunction(() => {
        const cards = document.querySelectorAll('.device-card');
        const emptyMessages = document.querySelectorAll('.empty-message');
        return cards.length > 0 || emptyMessages.length > 0;
      }, { timeout: 10000 });
    };
    await use(fn);
  },

  getDeviceCards: async ({ page }, use) => {
    const fn = async () => {
      const cards = await page.locator('.device-card').all();
      const names: string[] = [];
      for (const card of cards) {
        const nameEl = await card.locator('.device-name, h3, h4').first();
        const name = await nameEl.textContent();
        if (name) names.push(name.trim());
      }
      return names;
    };
    await use(fn);
  },

  attachDevice: async ({ page }, use) => {
    const fn = async (deviceName: string) => {
      // Find device card containing the device name
      const card = page.locator('.device-card', { hasText: deviceName }).first();
      await expect(card).toBeVisible();

      // Click attach button
      const attachBtn = card.locator('button:has-text("Attach")');
      await expect(attachBtn).toBeVisible();
      await attachBtn.click();

      // Wait for status to change to "Attached"
      await expect(card.locator(':text("Attached")')).toBeVisible({ timeout: 5000 });
    };
    await use(fn);
  },

  detachDevice: async ({ page }, use) => {
    const fn = async (deviceName: string) => {
      // Find device card containing the device name
      const card = page.locator('.device-card', { hasText: deviceName }).first();
      await expect(card).toBeVisible();

      // Click detach button
      const detachBtn = card.locator('button:has-text("Detach")');
      await expect(detachBtn).toBeVisible();
      await detachBtn.click();

      // Wait for status to change (card may move to Available section)
      await page.waitForTimeout(1000);
    };
    await use(fn);
  },

  navigateTo: async ({ page, baseURL }, use) => {
    const fn = async (path: string) => {
      await page.goto(path);
      // Wait for Leptos app to mount
      await page.waitForSelector('main, .app, [data-leptos]', { timeout: 10000 });
    };
    await use(fn);
  },

  getHealth: async ({ request, baseURL }, use) => {
    const fn = async () => {
      const response = await request.get(`${baseURL}/api/v1/health`);
      expect(response.ok()).toBeTruthy();
      return await response.json();
    };
    await use(fn);
  },
});

export { expect };

/**
 * Console message collector for WebSocket debugging
 */
export async function collectConsoleLogs(page: Page, duration: number = 3000): Promise<string[]> {
  const logs: string[] = [];
  const handler = (msg: any) => logs.push(`[${msg.type()}] ${msg.text()}`);
  page.on('console', handler);
  await page.waitForTimeout(duration);
  page.off('console', handler);
  return logs;
}

/**
 * Wait for a specific console message pattern
 */
export async function waitForConsoleMessage(
  page: Page,
  pattern: RegExp,
  timeout: number = 5000
): Promise<string | null> {
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      page.off('console', handler);
      resolve(null);
    }, timeout);

    const handler = (msg: any) => {
      const text = msg.text();
      if (pattern.test(text)) {
        clearTimeout(timer);
        page.off('console', handler);
        resolve(text);
      }
    };

    page.on('console', handler);
  });
}
