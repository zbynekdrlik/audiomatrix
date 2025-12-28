import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  timeout: 30000,
  retries: 1,
  reporter: [
    ['html', { open: 'never' }],
    ['list']
  ],
  use: {
    // Base URL for local testing
    baseURL: process.env.BASE_URL || 'http://localhost:8080',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'on-first-retry',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
    {
      name: 'mobile',
      use: { ...devices['Pixel 5'] },
    },
  ],
  // Web server to start before tests
  webServer: process.env.NO_SERVER ? undefined : {
    command: 'cd ../.. && ./target/release/audiomatrix 2>&1',
    url: 'http://localhost:8080/api/v1/health',
    reuseExistingServer: true,
    timeout: 30000,
  },
});
