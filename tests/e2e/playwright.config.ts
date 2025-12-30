import { defineConfig, devices } from '@playwright/test';

/**
 * AudioMatrix E2E Test Configuration
 *
 * Tests run against a live AudioMatrix instance.
 * Set BASE_URL env var for remote testing (default: http://localhost:8080)
 */
export default defineConfig({
  testDir: './specs',

  // Run tests in parallel
  fullyParallel: true,

  // Fail the build on CI if test.only left in source
  forbidOnly: !!process.env.CI,

  // Retry on CI only
  retries: process.env.CI ? 2 : 0,

  // Limit workers on CI
  workers: process.env.CI ? 1 : undefined,

  // Reporter configuration
  reporter: [
    ['html', { open: 'never' }],
    ['list']
  ],

  // Shared settings for all projects
  use: {
    // Base URL for tests - default to stagebox1.lan where real devices exist
    baseURL: process.env.BASE_URL || 'http://stagebox1.lan:8080',

    // Collect trace on first retry
    trace: 'on-first-retry',

    // Screenshot on failure
    screenshot: 'only-on-failure',

    // Video on failure
    video: 'on-first-retry',

    // Default timeout for actions
    actionTimeout: 10000,

    // Default navigation timeout
    navigationTimeout: 30000,
  },

  // Test timeout
  timeout: 60000,

  // Expect timeout
  expect: {
    timeout: 10000,
  },

  // Projects for different browsers
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
    // Optionally add Firefox and WebKit
    // {
    //   name: 'firefox',
    //   use: { ...devices['Desktop Firefox'] },
    // },
    // {
    //   name: 'webkit',
    //   use: { ...devices['Desktop Safari'] },
    // },
  ],

  // No web server - tests run against external AudioMatrix instance
  // To run against stagebox1.lan: BASE_URL=http://stagebox1.lan:8080 npm test
});
